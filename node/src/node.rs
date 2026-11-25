//! The node: shared state, the HTTP peer interface, the sync loop that pulls
//! objects from peers (BitTorrent-style: nobody pushes work at you), and the
//! check loop that attests to submissions this node has not judged yet.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::{DefaultBodyLimit, Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::check::{self, statement_key};
use crate::config::Config;
use crate::identity::Identity;
use crate::model::{
    is_id, now, signing_bytes, Attestation, Envelope, Kind, Submission, Target, Verdict,
    MAX_OBJECT_BYTES, PROTOCOL_VERSION,
};
use crate::store::Store;

pub struct Node {
    pub datadir: PathBuf,
    pub cfg: Config,
    pub id: Identity,
    pub store: Store,
}

impl Node {
    pub fn open(datadir: PathBuf) -> Result<Node> {
        std::fs::create_dir_all(&datadir)?;
        let cfg = Config::load_or_create(&datadir)?;
        let id = Identity::load_or_create(&datadir.join("node.key"))?;
        let store = Store::open(&datadir)?;
        Ok(Node {
            datadir,
            cfg,
            id,
            store,
        })
    }

    pub fn roles(&self) -> Vec<&'static str> {
        let mut r = vec!["keep", "relay"];
        if check::checking_supported(&self.cfg.checker).is_ok() {
            r.push("check");
        }
        r
    }

    pub fn sign(&self, kind: Kind, body: &impl serde::Serialize) -> Result<Envelope> {
        let body = serde_json::to_value(body)?;
        let sig = self.id.sign(&signing_bytes(kind, &body)?);
        Ok(Envelope {
            kind,
            body,
            signer: self.id.public_hex(),
            sig,
        })
    }

    pub fn target(&self, id: &str) -> Result<Target> {
        let env = self.store.get(id)?;
        anyhow::ensure!(env.kind == Kind::Target, "{id} is not a target");
        Ok(serde_json::from_value(env.body)?)
    }

    pub fn submission(&self, id: &str) -> Result<Submission> {
        let env = self.store.get(id)?;
        anyhow::ensure!(env.kind == Kind::Submission, "{id} is not a submission");
        Ok(serde_json::from_value(env.body)?)
    }

    /// This node's own attestations, in ledger order.
    fn my_attestations(&self) -> Result<Vec<Attestation>> {
        let me = self.id.public_hex();
        Ok(self
            .store
            .of_kind(Kind::Attestation)?
            .into_iter()
            .filter(|(_, e)| e.signer == me)
            .filter_map(|(_, e)| serde_json::from_value(e.body).ok())
            .collect())
    }

    /// What this node had accepted before judging `sub_id`: the duplicate and compression context.
    fn context_for(&self, sub_id: &str, sub: &Submission) -> Result<check::Context_> {
        let mut ctx = check::Context_ {
            accepted_statements: HashSet::new(),
            best_loc: None,
        };
        for a in self.my_attestations()? {
            if a.verdict != Verdict::Accepted || a.submission == sub_id {
                continue;
            }
            if let Ok(prev) = self.submission(&a.submission) {
                for l in &prev.lemmas {
                    ctx.accepted_statements.insert(statement_key(&l.statement));
                }
                if a.target == sub.target {
                    if let Some(s) = &a.score {
                        ctx.best_loc = Some(
                            ctx.best_loc
                                .map_or(s.solution_loc, |b| b.min(s.solution_loc)),
                        );
                    }
                }
            }
        }
        Ok(ctx)
    }

    fn judge(&self, sub_id: &str) -> Result<(Submission, check::Outcome)> {
        let sub = self.submission(sub_id)?;
        let target = self
            .target(&sub.target)
            .context("target not in store yet")?;
        let ctx = self.context_for(sub_id, &sub)?;
        let log = self.datadir.join("logs").join(format!("{sub_id}.log"));
        let out = check::run(
            &target,
            &sub,
            &self.cfg.checker,
            &self.datadir.join("work"),
            &log,
            &ctx,
        )?;
        Ok((sub, out))
    }

    /// Check one submission now, sign the attestation and store it.
    pub fn check_and_attest(&self, sub_id: &str) -> Result<(String, Attestation, bool)> {
        let (sub, out) = self.judge(sub_id)?;
        let att = Attestation {
            v: PROTOCOL_VERSION,
            submission: sub_id.to_string(),
            target: sub.target.clone(),
            verdict: out.verdict,
            gates: out.gates,
            score: out.score,
            params: out.params,
            created: now(),
        };
        let marker = self.datadir.join("unjudged").join(sub_id);
        if !out.complete {
            // Remember locally so the background loop does not retry it forever.
            // `synterra check <id>` always retries. Nothing is signed or shared.
            let why = att
                .gates
                .iter()
                .find(|g| g.status == crate::model::GateStatus::Skipped);
            std::fs::create_dir_all(marker.parent().unwrap())?;
            std::fs::write(
                &marker,
                why.map(|g| g.detail.as_str()).unwrap_or("incomplete"),
            )?;
            return Ok((String::new(), att, false));
        }
        let _ = std::fs::remove_file(&marker);
        let env = self.sign(Kind::Attestation, &att)?;
        let (id, _) = self.store.put(&env)?;
        Ok((id, att, true))
    }

    /// Re-judge the submission behind someone's attestation with this node's own checker and
    /// compare. The attestation's recorded command is never executed: it came from a stranger.
    pub fn reproduce(&self, att_id: &str) -> Result<Reproduction> {
        let env = self.store.get(att_id)?;
        anyhow::ensure!(
            env.kind == Kind::Attestation,
            "{att_id} is not an attestation"
        );
        let theirs: Attestation = serde_json::from_value(env.body)?;
        let (_, ours) = self.judge(&theirs.submission)?;
        let mut rows = Vec::new();
        let mut binding_match = ours.complete;
        for g in &theirs.gates {
            let mine = ours.gates.iter().find(|m| m.name == g.name);
            // not_duplicate depends on each node's history, so it is reported, not compared.
            let history = g.name == "not_duplicate";
            let same = mine.map(|m| m.status == g.status).unwrap_or(false);
            if !same && !history {
                binding_match = false;
            }
            rows.push((g.name.clone(), g.status, mine.map(|m| m.status), history));
        }
        for m in &ours.gates {
            if !theirs.gates.iter().any(|g| g.name == m.name) {
                rows.push((
                    m.name.clone(),
                    crate::model::GateStatus::Skipped,
                    Some(m.status),
                    false,
                ));
                binding_match = false;
            }
        }
        let verdict_match = ours.complete && ours.verdict == theirs.verdict;
        let params_differ = theirs.params != ours.params;
        Ok(Reproduction {
            signer: env.signer,
            theirs: theirs.verdict,
            ours: ours.complete.then_some(ours.verdict),
            rows,
            verdict_match,
            binding_match,
            params_differ,
            their_params: theirs.params,
            our_params: ours.params,
        })
    }

    /// Write a submission and its target as a plain Lean project with the exact
    /// comparator config, so anyone can judge it with their own Lean and no trust in this binary.
    pub fn materialize(&self, sub_id: &str, dir: &std::path::Path) -> Result<()> {
        let sub = self.submission(sub_id)?;
        let target = self.target(&sub.target)?;
        sub.validate_against(&target)?;
        anyhow::ensure!(
            !dir.exists() || std::fs::read_dir(dir)?.next().is_none(),
            "{} is not empty",
            dir.display()
        );
        std::fs::create_dir_all(dir)?;
        target.project.write(dir)?;
        let config = serde_json::json!({
            "challenge_module": target.challenge_module,
            "solution_module": sub.solution_module,
            "theorem_names": target.theorem_names,
            "permitted_axioms": target.permitted_axioms,
        });
        std::fs::write(
            dir.join("comparator.json"),
            serde_json::to_vec_pretty(&config)?,
        )?;
        sub.solution.write(dir)?;
        let claims: Vec<_> = sub
            .lemmas
            .iter()
            .map(|l| json!({"name": l.name, "statement": l.statement}))
            .collect();
        std::fs::write(
            dir.join("SYNTERRA.json"),
            serde_json::to_vec_pretty(&json!({
                "submission": sub_id,
                "target": sub.target,
                "target_name": target.name,
                "lemma_claims": claims,
                "judge_with": "lake comparator --config=comparator.json --paranoid",
            }))?,
        )?;
        Ok(())
    }

    /// Submissions with a known target that this node has not attested to.
    pub fn pending(&self) -> Result<Vec<String>> {
        let done: HashSet<String> = self
            .my_attestations()?
            .into_iter()
            .map(|a| a.submission)
            .collect();
        let mut out = Vec::new();
        for (id, env) in self.store.of_kind(Kind::Submission)? {
            if done.contains(&id) || self.datadir.join("unjudged").join(&id).exists() {
                continue;
            }
            if let Ok(s) = serde_json::from_value::<Submission>(env.body) {
                if self.store.has(&s.target) {
                    out.push(id);
                }
            }
        }
        Ok(out)
    }
}

pub struct Reproduction {
    pub signer: String,
    pub theirs: Verdict,
    /// None when a gate could not run here.
    pub ours: Option<Verdict>,
    /// (gate, their status, our status, history-dependent)
    pub rows: Vec<(
        String,
        crate::model::GateStatus,
        Option<crate::model::GateStatus>,
        bool,
    )>,
    pub verdict_match: bool,
    pub binding_match: bool,
    pub params_differ: bool,
    pub their_params: crate::model::CheckParams,
    pub our_params: crate::model::CheckParams,
}

type Shared = Arc<Node>;

async fn info(State(n): State<Shared>) -> Json<serde_json::Value> {
    let height = n.store.ledger().map(|l| l.len()).unwrap_or(0);
    Json(json!({
        "protocol": PROTOCOL_VERSION,
        "software": concat!("synterra-core/", env!("CARGO_PKG_VERSION")),
        "node": n.id.public_hex(),
        "roles": n.roles(),
        "height": height,
    }))
}

/// Ids in this node's ledger order, so peers that pull inherit the order objects were
/// first seen in. Priority between duplicate lemmas follows it until anchoring on Solana.
async fn inventory(State(n): State<Shared>) -> Result<Json<Vec<String>>, StatusCode> {
    n.store
        .ledger()
        .map(|l| Json(l.into_iter().map(|e| e.object).collect()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn object(
    State(n): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Envelope>, StatusCode> {
    if !is_id(&id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    n.store
        .get(&id)
        .map(Json)
        .map_err(|_| StatusCode::NOT_FOUND)
}

async fn accept(
    State(n): State<Shared>,
    Json(env): Json<Envelope>,
) -> (StatusCode, Json<serde_json::Value>) {
    let n2 = n.clone();
    match tokio::task::spawn_blocking(move || n2.store.put(&env)).await {
        Ok(Ok((id, added))) => (StatusCode::OK, Json(json!({ "id": id, "added": added }))),
        Ok(Err(e)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": e.to_string() })),
        ),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal" })),
        ),
    }
}

pub fn router(n: Shared) -> Router {
    Router::new()
        .route("/v1/info", get(info))
        .route("/v1/inventory", get(inventory))
        .route("/v1/objects/:id", get(object))
        .route("/v1/objects", post(accept))
        .layer(DefaultBodyLimit::max(MAX_OBJECT_BYTES + 64 * 1024))
        .with_state(n)
}

/// Pull everything we lack from one peer. Objects are re-validated and re-addressed locally.
pub async fn sync_peer(n: &Shared, client: &reqwest::Client, peer: &str) -> Result<usize> {
    let base = peer.trim_end_matches('/');
    let ids: Vec<String> = client
        .get(format!("{base}/v1/inventory"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mut added = 0;
    for id in ids.into_iter().filter(|i| is_id(i)).take(10_000) {
        if n.store.has(&id) {
            continue;
        }
        let resp = client
            .get(format!("{base}/v1/objects/{id}"))
            .send()
            .await?
            .error_for_status()?;
        if resp.content_length().unwrap_or(0) as usize > MAX_OBJECT_BYTES + 64 * 1024 {
            continue;
        }
        let env: Envelope = match resp.json().await {
            Ok(e) => e,
            Err(_) => continue,
        };
        if env.id().ok().as_deref() != Some(id.as_str()) {
            eprintln!("peer {base} served {id} with the wrong content; ignoring");
            continue;
        }
        let n2 = n.clone();
        match tokio::task::spawn_blocking(move || n2.store.put(&env)).await? {
            Ok((_, true)) => added += 1,
            Ok(_) => {}
            Err(e) => eprintln!("rejected {id} from {base}: {e}"),
        }
    }
    Ok(added)
}

pub async fn run(n: Node) -> Result<()> {
    let n: Shared = Arc::new(n);
    let addr: SocketAddr = n.cfg.listen.parse().context("bad listen address")?;
    let roles = n.roles();
    eprintln!("synterra node {} listening on {addr}", n.id.public_hex());
    eprintln!("roles: {}", roles.join(", "));
    if let Err(why) = check::checking_supported(&n.cfg.checker) {
        eprintln!("not checking: {why}");
    }
    if n.cfg.checker.sandbox.is_empty() {
        eprintln!("WARNING: checker.sandbox is empty; untrusted Lean would run unsandboxed");
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let syncer = n.clone();
    tokio::spawn(async move {
        loop {
            for peer in &syncer.cfg.peers {
                match sync_peer(&syncer, &client, peer).await {
                    Ok(k) if k > 0 => eprintln!("pulled {k} object(s) from {peer}"),
                    Ok(_) => {}
                    Err(e) => eprintln!("sync {peer}: {e}"),
                }
            }
            if syncer.roles().contains(&"check") {
                if let Ok(pending) = syncer.pending() {
                    for sid in pending {
                        let n2 = syncer.clone();
                        let s2 = sid.clone();
                        match tokio::task::spawn_blocking(move || n2.check_and_attest(&s2)).await {
                            Ok(Ok((aid, att, true))) => {
                                eprintln!("attested {} as {:?} ({aid})", &sid[..12], att.verdict)
                            }
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => eprintln!("check {}: {e}", &sid[..12]),
                            Err(e) => eprintln!("check task: {e}"),
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(syncer.cfg.sync_secs.max(5))).await;
        }
    });

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router(n)).await?;
    Ok(())
}

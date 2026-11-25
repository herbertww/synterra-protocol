//! Synterra Core (`synterra`): the reference node for Synterra proof mining.
//!
//! Like a Bitcoin full node, it keeps a copy of everything, relays it to
//! peers and verifies it itself. What it verifies is not hashes but proofs:
//! it kernel-checks proof-mining submissions with Lean's own comparator and
//! signs what it finds. SYN is not issued here; see README.

mod bundle;
mod check;
mod color;
mod config;
mod identity;
mod model;
mod node;
mod store;
mod vectors;

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use crate::bundle::Bundle;
use crate::model::{now, Kind, LemmaClaim, Submission, Target, PROTOCOL_VERSION};
use crate::node::Node;

#[derive(Parser)]
#[command(
    name = "synterra",
    version,
    about = "Synterra Core: the reference proof-mining node. Keep, relay and kernel-check proofs."
)]
struct Cli {
    /// Data directory. Defaults to $SYNTERRA_DATADIR or ~/.synterra.
    #[arg(long, global = true)]
    datadir: Option<PathBuf>,
    /// Colour output: auto (honours NO_COLOR and pipes), always or never.
    #[arg(long, global = true, default_value = "auto", value_parser = ["auto", "always", "never"])]
    color: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create the data directory, node key and config.
    Init,
    /// Show this node's key, roles and ledger height.
    Info,
    /// Nominate a target from a Lean project directory.
    Nominate {
        /// Project directory (lakefile, lean-toolchain, challenge module).
        dir: PathBuf,
        #[arg(long)]
        name: String,
        /// Module that states the target theorems, e.g. `Challenge`.
        #[arg(long, default_value = "Challenge")]
        challenge: String,
        /// A theorem every submission must prove. Repeat for several.
        #[arg(long = "theorem", required = true)]
        theorems: Vec<String>,
        /// Permitted axioms. Defaults to Lean's standard three.
        #[arg(long = "axiom")]
        axioms: Vec<String>,
        /// Lines of the best known solution, for the compression measure.
        #[arg(long)]
        baseline_loc: Option<u64>,
    },
    /// Package and sign a submission against a target.
    Submit {
        target: String,
        /// The solution module file or directory of .lean files.
        path: PathBuf,
        #[arg(long, default_value = "Solution")]
        module: String,
        /// A lemma you claim, as `name::statement`. Repeat for several.
        #[arg(long = "lemma")]
        lemmas: Vec<String>,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Run every gate on a submission now and sign the result.
    Check { submission: String },
    /// List stored objects of one kind.
    List {
        #[arg(value_parser = ["targets", "submissions", "attestations"])]
        kind: String,
    },
    /// Print one object as JSON.
    Show { id: String },
    /// Import a signed object (JSON) from a file, e.g. one received out of band.
    Import { file: PathBuf },
    /// Re-check every ledger link and every stored object.
    Verify,
    /// Write a submission and its target as a plain Lean project, with the exact
    /// comparator config, so you can judge it with your own Lean and no trust in this binary.
    Materialize { submission: String, dir: PathBuf },
    /// Re-judge the submission behind someone's attestation with your own checker and compare.
    /// Exit 0: match. 1: mismatch. 2: a gate could not run here.
    Reproduce { attestation: String },
    /// Print the wire-format test vectors (compare with docs/test-vectors.json).
    Vectors,
    /// Run the node: serve peers, pull from peers, check what is pending.
    Run {
        #[arg(long)]
        listen: Option<String>,
        /// Peer base URL. Repeat for several. Added to those in the config.
        #[arg(long = "peer")]
        peers: Vec<String>,
    },
}

fn datadir(arg: Option<PathBuf>) -> PathBuf {
    if let Some(d) = arg {
        return d;
    }
    if let Some(d) = std::env::var_os("SYNTERRA_DATADIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .unwrap_or_else(|| ".".into());
    PathBuf::from(home).join(".synterra")
}

fn short(id: &str) -> &str {
    &id[..12.min(id.len())]
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    color::init(&cli.color);
    if let Cmd::Vectors = cli.cmd {
        println!("{}", serde_json::to_string_pretty(&vectors::generate()?)?);
        return Ok(());
    }
    let dir = datadir(cli.datadir);
    let n = Node::open(dir.clone())?;

    match cli.cmd {
        Cmd::Init | Cmd::Info => {
            println!("datadir  {}", dir.display());
            println!("node     {}", n.id.public_hex());
            println!("roles    {}", n.roles().join(", "));
            println!("height   {}", n.store.ledger()?.len());
            if let Err(why) = check::checking_supported(&n.cfg.checker) {
                println!("{} {why}", color::paint(color::Role::Warn, "checking off:"));
            }
        }
        Cmd::Nominate {
            dir: proj,
            name,
            challenge,
            theorems,
            axioms,
            baseline_loc,
        } => {
            let axioms = if axioms.is_empty() {
                vec![
                    "propext".into(),
                    "Quot.sound".into(),
                    "Classical.choice".into(),
                ]
            } else {
                axioms
            };
            let t = Target {
                v: PROTOCOL_VERSION,
                name,
                challenge_module: challenge,
                theorem_names: theorems,
                permitted_axioms: axioms,
                project: Bundle::read(&proj, false).context("reading project")?,
                baseline_loc,
                created: now(),
            };
            t.validate()?;
            let (id, _) = n.store.put(&n.sign(Kind::Target, &t)?)?;
            println!("{id}");
        }
        Cmd::Submit {
            target,
            path,
            module,
            lemmas,
            note,
        } => {
            let t = n.target(&target)?;
            let mut solution = Bundle::read(&path, true).context("reading solution")?;
            // A single file is the module root, whatever it is called on disk.
            if path.is_file() {
                solution.files[0].path = bundle::module_path(&module);
            } else {
                let prefix = module.replace('.', "/");
                for f in &mut solution.files {
                    if f.path != format!("{prefix}.lean")
                        && !f.path.starts_with(&format!("{prefix}/"))
                    {
                        f.path = format!("{prefix}/{}", f.path);
                    }
                }
                solution.files.sort_by(|a, b| a.path.cmp(&b.path));
            }
            let lemmas = lemmas
                .iter()
                .map(|s| match s.split_once("::") {
                    Some((name, st)) => Ok(LemmaClaim {
                        name: name.trim().into(),
                        statement: st.trim().into(),
                    }),
                    None => bail!("--lemma must be name::statement"),
                })
                .collect::<Result<Vec<_>>>()?;
            let s = Submission {
                v: PROTOCOL_VERSION,
                target: target.clone(),
                solution_module: module,
                solution,
                lemmas,
                note,
                created: now(),
            };
            s.validate_against(&t)?;
            let (id, _) = n.store.put(&n.sign(Kind::Submission, &s)?)?;
            println!("{id}");
        }
        Cmd::Check { submission } => {
            let (aid, att, signed) = n.check_and_attest(&submission)?;
            use color::{paint, Role};
            for g in &att.gates {
                let adv = if g.advisory {
                    paint(Role::Advisory, " (advisory)")
                } else {
                    String::new()
                };
                println!(
                    "{:<14} {} {}{adv}",
                    g.name,
                    color::status(g.status, 8),
                    g.detail.lines().next().unwrap_or("")
                );
            }
            if let Some(s) = &att.score {
                println!(
                    "score          {} proof lines, {} saved {}",
                    s.solution_loc,
                    s.loc_saved,
                    paint(Role::Dim, &format!("[{}]", s.label))
                );
            }
            let accepted = att.verdict == model::Verdict::Accepted;
            println!(
                "verdict        {}",
                color::chip(accepted, &format!("{:?}", att.verdict))
            );
            if signed {
                println!("attestation    {}", paint(Role::Dim, &aid));
            } else {
                println!(
                    "{}",
                    paint(
                        Role::Warn,
                        "not signed: a gate could not run on this node (see above)"
                    )
                );
                std::process::exit(2);
            }
        }
        Cmd::List { kind } => {
            let k = match kind.as_str() {
                "targets" => Kind::Target,
                "submissions" => Kind::Submission,
                _ => Kind::Attestation,
            };
            for (id, env) in n.store.of_kind(k)? {
                let b = &env.body;
                let what = match k {
                    Kind::Target => b["name"].as_str().unwrap_or("").to_string(),
                    Kind::Submission => format!(
                        "target {} {}",
                        short(b["target"].as_str().unwrap_or("")),
                        b["note"].as_str().unwrap_or("")
                    ),
                    Kind::Attestation => format!(
                        "{} submission {}",
                        b["verdict"].as_str().unwrap_or(""),
                        short(b["submission"].as_str().unwrap_or(""))
                    ),
                };
                println!("{id}  by {}  {what}", short(&env.signer));
            }
        }
        Cmd::Show { id } => println!("{}", serde_json::to_string_pretty(&n.store.get(&id)?)?),
        Cmd::Import { file } => {
            let env: model::Envelope = serde_json::from_slice(&std::fs::read(file)?)?;
            let (id, added) = n.store.put(&env)?;
            println!("{id} {}", if added { "added" } else { "already stored" });
        }
        Cmd::Verify => println!("ledger ok: {} entries verified", n.store.verify_ledger()?),
        Cmd::Materialize {
            submission,
            dir: out,
        } => {
            n.materialize(&submission, &out)?;
            println!("wrote {}", out.display());
            println!(
                "judge it yourself:  cd {} && lake comparator --config=comparator.json --paranoid",
                out.display()
            );
        }
        Cmd::Reproduce { attestation } => {
            use color::{paint, Role};
            let r = n.reproduce(&attestation)?;
            println!(
                "attestation by {}: theirs {:?}, ours {}",
                paint(Role::Dim, short(&r.signer)),
                r.theirs,
                r.ours
                    .as_ref()
                    .map(|v| format!("{v:?}"))
                    .unwrap_or_else(|| "incomplete".into())
            );
            println!("{:<14} {:<8} {}", "gate", "theirs", "ours");
            for (name, theirs, ours, history) in &r.rows {
                let note = if *history {
                    paint(
                        Role::Dim,
                        "  (depends on each node's history; not compared)",
                    )
                } else {
                    String::new()
                };
                let ours = ours
                    .map(|s| color::status(s, 8))
                    .unwrap_or_else(|| "-".into());
                println!("{:<14} {} {ours}{note}", name, color::status(*theirs, 8));
            }
            if r.params_differ {
                println!(
                    "{}\n  theirs {}\n  ours   {}",
                    paint(Role::Warn, "params differ:"),
                    serde_json::to_string(&r.their_params)?,
                    serde_json::to_string(&r.our_params)?
                );
            } else {
                println!("params identical");
            }
            if r.ours.is_none() {
                println!(
                    "RESULT  {}",
                    paint(Role::Warn, "INCOMPLETE: a gate could not run on this node")
                );
                std::process::exit(2);
            } else if r.verdict_match && r.binding_match {
                println!("RESULT  {}", color::chip(true, "match"));
            } else {
                println!("RESULT  {}", color::chip(false, "mismatch"));
                std::process::exit(1);
            }
        }
        Cmd::Vectors => unreachable!(),
        Cmd::Run { listen, peers } => {
            let mut n = n;
            if let Some(l) = listen {
                n.cfg.listen = l;
            }
            n.cfg.peers.extend(peers);
            tokio::runtime::Runtime::new()?.block_on(node::run(n))?;
        }
    }
    Ok(())
}

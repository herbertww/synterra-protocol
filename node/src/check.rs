//! The gate pipeline. A submission is accepted only if every gate passes.
//!
//! Binding gates: shape, no_sorry, kernel (Lean's `lake comparator`).
//! The kernel gate is the one that makes correctness mechanical: same target
//! statements, no axioms beyond the permitted ones, accepted by the kernel.
//!
//! Advisory gates: not_trivial and not_duplicate read the lemma statements a
//! submitter *claims*. A failure there is trustworthy (nobody profits from
//! being called trivial), a pass is not yet: see README, threat model.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::config::CheckerConfig;
use crate::model::{sha256_hex, CheckParams, Gate, GateStatus, Score, Submission, Target, Verdict};

pub struct Outcome {
    pub verdict: Verdict,
    pub gates: Vec<Gate>,
    pub score: Option<Score>,
    pub params: CheckParams,
    /// False when the kernel gate could not run. Incomplete outcomes are never signed.
    pub complete: bool,
}

/// What this node already knows when it checks: statements accepted earlier.
pub struct Context_ {
    pub accepted_statements: HashSet<String>,
    pub best_loc: Option<u64>,
}

/// Claims are written in signature form, as they follow a lemma's name:
/// `(a b : Nat) : a + b = b + a`. A bare proposition gets the colon added.
pub fn signature(statement: &str) -> String {
    let s = statement.trim();
    if s.starts_with(['(', '{', '[', '⦃', ':']) {
        s.to_string()
    } else {
        format!(": {s}")
    }
}

pub fn normalize_statement(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn statement_key(s: &str) -> String {
    sha256_hex(normalize_statement(s).as_bytes())
}

fn gate(name: &str, status: GateStatus, advisory: bool, detail: impl Into<String>) -> Gate {
    Gate {
        name: name.into(),
        status,
        advisory,
        detail: detail.into(),
    }
}

/// Is `bin` runnable: a path that exists, or a name found on PATH.
pub fn which(bin: &str) -> Option<PathBuf> {
    let p = Path::new(bin);
    if p.components().count() > 1 {
        return p.exists().then(|| p.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for cand in [dir.join(bin), dir.join(format!("{bin}.exe"))] {
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// Checking untrusted Lean needs Linux namespaces (bubblewrap), so it is a Linux-only role.
pub fn checking_supported(cfg: &CheckerConfig) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("checking needs Linux (bubblewrap sandbox): run the node in WSL2, a Linux VM or a container".into());
    }
    match cfg.command.first() {
        None => return Err("checker.command is empty".into()),
        // A templated path ("{toolchain}/bin/lake") is resolved per target, in toolchain_ready.
        Some(bin) if !bin.contains('{') && which(bin).is_none() => {
            return Err(format!("checker binary {bin:?} not found on PATH"))
        }
        _ => {}
    }
    // lake comparator sandboxes with bubblewrap, and so does the default triviality sandbox.
    let needs_bwrap = cfg
        .command
        .first()
        .map(|s| s.ends_with("lake"))
        .unwrap_or(false)
        || cfg.sandbox.first().map(|s| s == "bwrap").unwrap_or(false);
    if needs_bwrap && which("bwrap").is_none() {
        return Err("bubblewrap (bwrap) not found on PATH: install it before checking".into());
    }
    Ok(())
}

fn elan_home() -> String {
    std::env::var("ELAN_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
        format!("{home}/.elan")
    })
}

/// The elan directory of a pinned toolchain: `leanprover/lean4:v4.36.0` lives in
/// `<elan>/toolchains/leanprover--lean4---v4.36.0`. Resolved on the host from the
/// trusted target, so the sandbox can mount that one toolchain and nothing else of elan.
pub fn toolchain_dir(pin: &str) -> PathBuf {
    let name = pin.trim().replace('/', "--").replace(':', "---");
    PathBuf::from(elan_home()).join("toolchains").join(name)
}

/// A target names the toolchain that judges it, and the judge runs outside the sandbox.
/// So: only toolchains this node's policy allows (official Lean releases by default),
/// already installed by the operator. Nothing is ever downloaded during a check.
pub fn toolchain_ready(pin: &str, dir: &Path, cfg: &CheckerConfig) -> Result<(), String> {
    if pin.is_empty()
        || !pin
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-+".contains(&b))
        || pin.contains("..")
    {
        return Err(format!("toolchain pin {pin:?} is malformed"));
    }
    if !cfg
        .allowed_toolchains
        .iter()
        .any(|p| pin.starts_with(p.as_str()))
    {
        return Err(format!(
            "toolchain {pin} is outside this node's policy (allowed: {}); not judged",
            cfg.allowed_toolchains.join(", ")
        ));
    }
    for bin in ["lake", "lean"] {
        if !dir.join("bin").join(bin).is_file() {
            return Err(format!(
                "toolchain {pin} is not installed here (elan toolchain install {pin}); not judged"
            ));
        }
    }
    Ok(())
}

fn substitute(args: &[String], work: &Path, config: &Path, toolchain: &Path) -> Vec<String> {
    args.iter()
        .map(|a| {
            a.replace("{work}", &work.to_string_lossy())
                .replace("{config}", &config.to_string_lossy())
                .replace("{toolchain}", &toolchain.to_string_lossy())
                .replace("{elan}", &elan_home())
        })
        .collect()
}

pub struct RunResult {
    pub ok: bool,
    pub timed_out: bool,
    /// Exit code; None if killed by a signal or the timeout.
    pub code: Option<i32>,
    pub tail: String,
}

pub fn run_with_timeout(argv: &[String], cwd: &Path, timeout: Duration) -> Result<RunResult> {
    let (bin, rest) = argv.split_first().context("empty command")?;
    let mut child = Command::new(bin)
        .args(rest)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning {bin}"))?;
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let t_out = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
        s
    });
    let t_err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break Some(st);
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            timed_out = true;
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let text = format!(
        "{}{}",
        t_out.join().unwrap_or_default(),
        t_err.join().unwrap_or_default()
    );
    Ok(RunResult {
        ok: status.map(|s| s.success()).unwrap_or(false),
        timed_out,
        code: status.and_then(|s| s.code()),
        tail: tail(&text),
    })
}

/// Last lines of tool output, printable ASCII only, capped. For the local log only.
fn tail(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(15);
    let t: String = lines[start..]
        .join("\n")
        .chars()
        .map(|c| {
            if c == '\n' || (c.is_ascii() && !c.is_ascii_control()) {
                c
            } else {
                '?'
            }
        })
        .collect();
    t.chars()
        .rev()
        .take(1500)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

struct Work {
    dir: PathBuf,
}

impl Work {
    fn new(root: &Path, tag: &str) -> Result<Work> {
        let dir = root.join(format!("{tag}-{}", rand::random::<u64>()));
        fs::create_dir_all(&dir)?;
        Ok(Work { dir })
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        if std::env::var_os("SYNTERRA_KEEP_WORK").is_none() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// Tokens that mean "not actually proved" or "new assumptions".
fn cheat_tokens(sub: &Submission) -> Vec<String> {
    let mut found = Vec::new();
    for f in &sub.solution.files {
        for (i, line) in f.content.lines().enumerate() {
            let code = line.split("--").next().unwrap_or("");
            for tok in code.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')) {
                if matches!(
                    tok,
                    "sorry" | "admit" | "sorryAx" | "axiom" | "implemented_by" | "extern"
                ) {
                    found.push(format!("{}:{}: {tok}", f.path, i + 1));
                }
            }
        }
    }
    found
}

/// Untrusted tool output never goes into a signed attestation: a hostile proof could print
/// anything it can read. It is kept in a local log the operator can inspect instead.
fn keep_log(log: &Path, stage: &str, text: &str) {
    if let Some(dir) = log.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(log) {
        use std::io::Write;
        let _ = writeln!(f, "=== {stage}\n{text}");
    }
}

pub fn run(
    target: &Target,
    sub: &Submission,
    cfg: &CheckerConfig,
    workroot: &Path,
    log: &Path,
    ctx: &Context_,
) -> Result<Outcome> {
    fs::create_dir_all(workroot)?;
    let toolchain = target
        .project
        .get("lean-toolchain")
        .unwrap_or("?")
        .trim()
        .to_string();
    let tc_dir = toolchain_dir(&toolchain);
    let params = CheckParams {
        software: concat!("synterra-core/", env!("CARGO_PKG_VERSION")).to_string(),
        command: cfg.command.clone(),
        toolchain: toolchain.clone(),
        trivial_tactics: cfg.trivial_tactics.clone(),
        timeout_secs: cfg.timeout_secs,
        sandboxed: !cfg.sandbox.is_empty(),
    };
    let mut gates = Vec::new();
    let timeout = Duration::from_secs(cfg.timeout_secs.max(10));

    // 1. shape
    if let Err(e) = sub.validate_against(target) {
        gates.push(gate("shape", GateStatus::Fail, false, e.to_string()));
        return Ok(Outcome {
            verdict: Verdict::Rejected,
            gates,
            score: None,
            params,
            complete: true,
        });
    }
    gates.push(gate(
        "shape",
        GateStatus::Pass,
        false,
        "paths, sizes and module layout are clean",
    ));

    // 2. no_sorry: cheap, and the kernel gate would catch these anyway.
    let cheats = cheat_tokens(sub);
    if !cheats.is_empty() {
        gates.push(gate("no_sorry", GateStatus::Fail, false, cheats.join("; ")));
        return Ok(Outcome {
            verdict: Verdict::Rejected,
            gates,
            score: None,
            params,
            complete: true,
        });
    }
    gates.push(gate(
        "no_sorry",
        GateStatus::Pass,
        false,
        "no sorry, admit, axiom or FFI escape",
    ));

    // 3. kernel: lake comparator in a fresh workspace.
    // Anything that stops the judge from running is "not judged here", never a rejection:
    // a node must not sign a false "rejected" because its own setup is broken.
    if let Err(why) =
        checking_supported(cfg).and_then(|_| toolchain_ready(&toolchain, &tc_dir, cfg))
    {
        gates.push(gate("kernel", GateStatus::Skipped, false, why));
        return Ok(Outcome {
            verdict: Verdict::Rejected,
            gates,
            score: None,
            params,
            complete: false,
        });
    }
    let work = Work::new(workroot, "kernel")?;
    target.project.write(&work.dir)?;
    let config_path = work.dir.join("comparator.json");
    let config = serde_json::json!({
        "challenge_module": target.challenge_module,
        "solution_module": sub.solution_module,
        "theorem_names": target.theorem_names,
        "permitted_axioms": target.permitted_axioms,
    });
    fs::write(&config_path, serde_json::to_vec_pretty(&config)?)?;
    // The challenge is written first and the solution is never compiled outside the checker.
    sub.solution.write(&work.dir)?;
    let r = run_with_timeout(
        &substitute(&cfg.command, &work.dir, &config_path, &tc_dir),
        &work.dir,
        timeout,
    )?;
    drop(work);
    keep_log(log, "kernel", &r.tail);
    // lake comparator exit codes: 0 accepted, 1 rejected, 2 could not start.
    match (r.timed_out, r.code) {
        (false, Some(0)) => {}
        (false, Some(1)) => {
            gates.push(gate(
                "kernel",
                GateStatus::Fail,
                false,
                "checker rejected the solution (output kept in the attesting node's log)",
            ));
            return Ok(Outcome {
                verdict: Verdict::Rejected,
                gates,
                score: None,
                params,
                complete: true,
            });
        }
        (true, _) => {
            gates.push(gate(
                "kernel",
                GateStatus::Skipped,
                false,
                format!(
                    "did not finish within {}s on this node; not judged",
                    timeout.as_secs()
                ),
            ));
            return Ok(Outcome {
                verdict: Verdict::Rejected,
                gates,
                score: None,
                params,
                complete: false,
            });
        }
        (false, code) => {
            gates.push(gate(
                "kernel",
                GateStatus::Skipped,
                false,
                format!("checker could not run (exit {code:?}); not judged"),
            ));
            return Ok(Outcome {
                verdict: Verdict::Rejected,
                gates,
                score: None,
                params,
                complete: false,
            });
        }
    }
    gates.push(gate(
        "kernel",
        GateStatus::Pass,
        false,
        format!(
            "checker accepted all {} target theorem(s)",
            target.theorem_names.len()
        ),
    ));

    // 4. not_trivial: does a stock tactic close each claimed lemma unaided?
    // If the probe cannot run, the outcome is incomplete and is not signed: a node that
    // cannot apply a gate must not vouch for a lemma, or every trivial claim would pass.
    let mut trivial = Vec::new();
    let unsandboxed = cfg.sandbox.is_empty();
    let mut probed = 0usize;
    let mut malformed = Vec::new();
    for l in &sub.lemmas {
        let work = Work::new(workroot, "trivial")?;
        target.project.write(&work.dir)?;
        let sig = signature(&l.statement);
        let tactics = cfg
            .trivial_tactics
            .iter()
            .map(|t| format!("  | {t}"))
            .collect::<Vec<_>>()
            .join("\n");
        // Two probes. The first proves the claim is a real statement (it must elaborate),
        // so a malformed claim cannot dodge the gate by making the tactic probe error out.
        let header = format!("import {}\n\n", target.challenge_module);
        fs::write(
            work.dir.join("SynterraWellFormed.lean"),
            format!("{header}example {sig} := by\n  sorry\n"),
        )?;
        fs::write(
            work.dir.join("SynterraTrivial.lean"),
            format!("{header}example {sig} := by\n  first\n{tactics}\n"),
        )?;
        let cfgp = work.dir.join("none.json");
        let mut build = substitute(&cfg.sandbox, &work.dir, &cfgp, &tc_dir);
        build.extend([
            "lake".into(),
            "build".into(),
            target.challenge_module.clone(),
        ]);
        let mut wellformed = substitute(&cfg.sandbox, &work.dir, &cfgp, &tc_dir);
        wellformed.extend([
            "lake".into(),
            "env".into(),
            "lean".into(),
            "SynterraWellFormed.lean".into(),
        ]);
        let mut probe = substitute(&cfg.sandbox, &work.dir, &cfgp, &tc_dir);
        probe.extend([
            "lake".into(),
            "env".into(),
            "lean".into(),
            "SynterraTrivial.lean".into(),
        ]);
        let short = Duration::from_secs(cfg.timeout_secs.clamp(10, 300));
        let built = match run_with_timeout(&build, &work.dir, timeout) {
            Ok(r) if r.ok => r,
            Ok(r) => {
                keep_log(log, &format!("not_trivial build for {}", l.name), &r.tail);
                gates.push(gate(
                    "not_trivial",
                    GateStatus::Skipped,
                    true,
                    format!("challenge did not build in the sandbox for {}", l.name),
                ));
                return Ok(Outcome {
                    verdict: Verdict::Rejected,
                    gates,
                    score: None,
                    params,
                    complete: false,
                });
            }
            Err(e) => {
                gates.push(gate(
                    "not_trivial",
                    GateStatus::Skipped,
                    true,
                    format!("could not run the probe for {}: {e}", l.name),
                ));
                return Ok(Outcome {
                    verdict: Verdict::Rejected,
                    gates,
                    score: None,
                    params,
                    complete: false,
                });
            }
        };
        drop(built);
        match run_with_timeout(&wellformed, &work.dir, short) {
            Ok(r) if r.ok => {}
            Ok(r) => {
                keep_log(
                    log,
                    &format!("not_trivial well-formed probe for {}", l.name),
                    &r.tail,
                );
                malformed.push(l.name.clone());
                continue;
            }
            Err(e) => {
                gates.push(gate(
                    "not_trivial",
                    GateStatus::Skipped,
                    true,
                    format!("could not run the probe for {}: {e}", l.name),
                ));
                return Ok(Outcome {
                    verdict: Verdict::Rejected,
                    gates,
                    score: None,
                    params,
                    complete: false,
                });
            }
        }
        probed += 1;
        if run_with_timeout(&probe, &work.dir, short)
            .map(|r| r.ok)
            .unwrap_or(false)
        {
            trivial.push(l.name.clone());
        }
    }
    if !malformed.is_empty() {
        gates.push(gate(
            "not_trivial",
            GateStatus::Fail,
            false,
            format!(
                "claimed statement does not elaborate against the challenge: {}",
                malformed.join(", ")
            ),
        ));
    } else if probed > 0 {
        if trivial.is_empty() {
            let note = if unsandboxed {
                " (WARNING: sandbox prefix is empty)"
            } else {
                ""
            };
            gates.push(gate(
                "not_trivial",
                GateStatus::Pass,
                true,
                format!(
                    "no claimed lemma closes with {}{note}",
                    cfg.trivial_tactics.join(", ")
                ),
            ));
        } else {
            gates.push(gate(
                "not_trivial",
                GateStatus::Fail,
                false,
                format!("closed by a stock tactic: {}", trivial.join(", ")),
            ));
        }
    }

    // 5. not_duplicate: first accepted wins, by this node's ledger order.
    let mut seen = HashSet::new();
    let mut dups = Vec::new();
    for l in &sub.lemmas {
        let k = statement_key(&l.statement);
        if ctx.accepted_statements.contains(&k) || !seen.insert(k) {
            dups.push(l.name.clone());
        }
    }
    if !sub.lemmas.is_empty() {
        if dups.is_empty() {
            gates.push(gate(
                "not_duplicate",
                GateStatus::Pass,
                true,
                "no statement matches an accepted lemma (text match; Mathlib not yet searched)",
            ));
        } else {
            gates.push(gate(
                "not_duplicate",
                GateStatus::Fail,
                false,
                format!("already accepted: {}", dups.join(", ")),
            ));
        }
    }

    let accepted = gates.iter().all(|g| g.status != GateStatus::Fail);
    let solution_loc = sub.solution.loc();
    let baseline = target.baseline_loc.or(ctx.best_loc);
    let score = accepted.then(|| Score {
        solution_loc,
        baseline_loc: baseline,
        loc_saved: baseline
            .map(|b| b.saturating_sub(solution_loc))
            .unwrap_or(0),
        label: "demo: compression proxy only, no weights or band decided".into(),
    });
    Ok(Outcome {
        verdict: if accepted {
            Verdict::Accepted
        } else {
            Verdict::Rejected
        },
        gates,
        score,
        params,
        complete: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{Bundle, FileEntry};

    fn sub_with(src: &str) -> Submission {
        Submission {
            v: 1,
            target: "a".repeat(64),
            solution_module: "Solution".into(),
            solution: Bundle {
                files: vec![FileEntry {
                    path: "Solution.lean".into(),
                    content: src.into(),
                }],
            },
            lemmas: vec![],
            note: String::new(),
            created: 0,
        }
    }

    #[test]
    fn catches_cheats_but_not_comments() {
        assert!(!cheat_tokens(&sub_with("theorem t : 1 = 1 := by sorry")).is_empty());
        assert!(!cheat_tokens(&sub_with("axiom magic : False")).is_empty());
        assert!(cheat_tokens(&sub_with("theorem t : 1 = 1 := rfl -- no sorry here")).is_empty());
        assert!(cheat_tokens(&sub_with("theorem sorryless : 1 = 1 := rfl")).is_empty());
    }

    #[test]
    fn statements_normalize() {
        assert_eq!(
            statement_key("a  +\n b = b + a"),
            statement_key("a + b = b + a")
        );
    }
}

//! Node configuration, read from `<datadir>/synterra.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    /// Where the HTTP peer interface listens. Loopback by default: opening it is a choice.
    pub listen: String,
    /// Peer base URLs, e.g. "http://203.0.113.7:8590".
    pub peers: Vec<String>,
    pub sync_secs: u64,
    pub checker: CheckerConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CheckerConfig {
    /// Judge a submission against its target. `{config}`, `{work}` and `{elan}` are substituted.
    /// The default is Lean's own `lake comparator` (Lean >= 4.35), which builds the solution
    /// inside a bubblewrap sandbox. `--paranoid` also replays the export through every
    /// external kernel bundled with Lean: leanchecker-paranoid, lean4lean, nanoda,
    /// con-leche and con-ron. A solution must satisfy all of them.
    pub command: Vec<String>,
    /// Prefix for every other run of untrusted Lean (the triviality gate). The default is
    /// bubblewrap: read-only /usr and toolchain, writable work dir, everything else
    /// (home, the node key, the network) absent.
    pub sandbox: Vec<String>,
    pub timeout_secs: u64,
    /// Tactics that make a lemma "trivial" if any of them closes it unaided.
    pub trivial_tactics: Vec<String>,
    /// Toolchain pins this node will judge with. The judge runs outside the sandbox, so a
    /// target must not be able to name an arbitrary build. Default: official Lean releases.
    #[serde(default = "default_toolchains")]
    pub allowed_toolchains: Vec<String>,
}

fn default_toolchains() -> Vec<String> {
    vec!["leanprover/lean4:v".into()]
}

impl Default for Config {
    fn default() -> Self {
        Config {
            listen: "127.0.0.1:8590".into(),
            peers: vec![],
            sync_secs: 30,
            checker: CheckerConfig {
                // The pinned toolchain's own lake, never elan's proxy: no download can happen mid-check.
                command: vec![
                    "{toolchain}/bin/lake".into(),
                    "comparator".into(),
                    "--config={config}".into(),
                    "--paranoid".into(),
                ],
                // Mount order matters: /tmp is replaced before the work dir is bound, so a
                // work dir under /tmp stays visible. Only the target's pinned toolchain is
                // mounted, never the rest of elan or the home directory.
                sandbox: [
                    "bwrap",
                    "--ro-bind",
                    "/usr",
                    "/usr",
                    "--symlink",
                    "usr/bin",
                    "/bin",
                    "--symlink",
                    "usr/lib",
                    "/lib",
                    "--symlink",
                    "usr/lib64",
                    "/lib64",
                    "--proc",
                    "/proc",
                    "--dev",
                    "/dev",
                    "--tmpfs",
                    "/tmp",
                    "--ro-bind",
                    "{toolchain}",
                    "{toolchain}",
                    "--bind",
                    "{work}",
                    "{work}",
                    "--unshare-all",
                    "--die-with-parent",
                    "--new-session",
                    "--clearenv",
                    "--setenv",
                    "HOME",
                    "{work}",
                    "--setenv",
                    "PATH",
                    "{toolchain}/bin:/usr/bin",
                    "--chdir",
                    "{work}",
                    "--",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect(),
                timeout_secs: 900,
                trivial_tactics: vec![
                    "omega".into(),
                    "decide".into(),
                    "(simp; done)".into(),
                    "grind".into(),
                ],
                allowed_toolchains: default_toolchains(),
            },
        }
    }
}

impl Config {
    pub fn path(datadir: &Path) -> PathBuf {
        datadir.join("synterra.toml")
    }

    pub fn load_or_create(datadir: &Path) -> Result<Config> {
        let p = Config::path(datadir);
        if p.exists() {
            return Ok(toml::from_str(&fs::read_to_string(p)?)?);
        }
        let c = Config::default();
        fs::write(p, toml::to_string_pretty(&c)?)?;
        Ok(c)
    }
}

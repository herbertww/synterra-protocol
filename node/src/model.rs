//! Wire objects. Every object on the network is an `Envelope`: a kind, a JSON
//! body, and the ed25519 signature of whoever made it. The object id is the
//! SHA-256 of the envelope's canonical JSON, so ids are content addresses.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::bundle::Bundle;

pub const PROTOCOL_VERSION: u32 = 1;
/// Hard cap on any single object, in bytes of canonical JSON.
pub const MAX_OBJECT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Target,
    Submission,
    Attestation,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Target => "target",
            Kind::Submission => "submission",
            Kind::Attestation => "attestation",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Envelope {
    pub kind: Kind,
    pub body: Value,
    /// Hex ed25519 public key of the signer.
    pub signer: String,
    /// Hex ed25519 signature over `signing_bytes(kind, body)`.
    pub sig: String,
}

/// serde_json without `preserve_order` keeps object keys sorted, so this is canonical.
pub fn canonical(v: &impl Serialize) -> Result<Vec<u8>> {
    let value = serde_json::to_value(v)?;
    Ok(serde_json::to_vec(&value)?)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn signing_bytes(kind: Kind, body: &Value) -> Result<Vec<u8>> {
    let mut out = b"synterra/v1/".to_vec();
    out.extend_from_slice(kind.as_str().as_bytes());
    out.push(b'/');
    out.extend_from_slice(&canonical(body)?);
    Ok(out)
}

impl Envelope {
    pub fn id(&self) -> Result<String> {
        Ok(sha256_hex(&canonical(self)?))
    }

    /// Full validation of an object received from anywhere: size, signature, schema.
    pub fn validate(&self) -> Result<()> {
        let bytes = canonical(self)?;
        if bytes.len() > MAX_OBJECT_BYTES {
            bail!(
                "object is {} bytes, over the {} byte cap",
                bytes.len(),
                MAX_OBJECT_BYTES
            );
        }
        crate::identity::verify(
            &self.signer,
            &signing_bytes(self.kind, &self.body)?,
            &self.sig,
        )
        .context("bad signature")?;
        match self.kind {
            Kind::Target => {
                let t: Target =
                    serde_json::from_value(self.body.clone()).context("target schema")?;
                t.validate()?;
            }
            Kind::Submission => {
                let s: Submission =
                    serde_json::from_value(self.body.clone()).context("submission schema")?;
                s.validate_shape()?;
            }
            Kind::Attestation => {
                let a: Attestation =
                    serde_json::from_value(self.body.clone()).context("attestation schema")?;
                if !is_id(&a.submission) || !is_id(&a.target) {
                    bail!("attestation references a malformed id");
                }
            }
        }
        Ok(())
    }
}

pub fn is_id(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// A Lean identifier, conservatively: dotted segments of [A-Za-z_][A-Za-z0-9_']*.
pub fn is_lean_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 200
        && s.split('.').all(|seg| {
            let mut c = seg.chars();
            matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
                && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '\'')
        })
}

/// A nominated target: a trusted Lean project whose challenge module states
/// theorems (with `sorry` or a proof) that every submission must still prove.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Target {
    pub v: u32,
    pub name: String,
    pub challenge_module: String,
    pub theorem_names: Vec<String>,
    pub permitted_axioms: Vec<String>,
    /// The project: lakefile, lean-toolchain, challenge module and its local imports.
    pub project: Bundle,
    /// Lines of the best known solution, for the compression measure. Optional.
    pub baseline_loc: Option<u64>,
    pub created: u64,
}

impl Target {
    pub fn validate(&self) -> Result<()> {
        if self.v != PROTOCOL_VERSION {
            bail!("unsupported target version {}", self.v);
        }
        if self.name.is_empty() || self.name.len() > 120 {
            bail!("target name must be 1 to 120 characters");
        }
        if !is_lean_name(&self.challenge_module) {
            bail!("challenge module is not a Lean module name");
        }
        if self.theorem_names.is_empty() || !self.theorem_names.iter().all(|n| is_lean_name(n)) {
            bail!("target needs at least one valid theorem name");
        }
        if !self.permitted_axioms.iter().all(|n| is_lean_name(n)) {
            bail!("permitted axioms must be Lean names");
        }
        self.project.validate(false)?;
        if !self
            .project
            .has(&crate::bundle::module_path(&self.challenge_module))
        {
            bail!("project does not contain the challenge module");
        }
        if !self.project.has("lean-toolchain") {
            bail!("project must pin its toolchain in lean-toolchain");
        }
        if !self.project.has("lake-manifest.json") {
            bail!("project must carry lake-manifest.json (build it once before nominating): lake comparator resolves dependencies inside its sandbox");
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LemmaClaim {
    pub name: String,
    /// The statement as Lean source, used by the triviality and duplicate gates.
    pub statement: String,
}

/// A proof-mining contribution against one target: a solution module that
/// still proves every target theorem, plus the lemmas it claims as its own.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Submission {
    pub v: u32,
    pub target: String,
    pub solution_module: String,
    /// Only `.lean` files under the solution module. Never lakefiles or toolchains.
    pub solution: Bundle,
    pub lemmas: Vec<LemmaClaim>,
    pub note: String,
    pub created: u64,
}

impl Submission {
    pub fn validate_shape(&self) -> Result<()> {
        if self.v != PROTOCOL_VERSION {
            bail!("unsupported submission version {}", self.v);
        }
        if !is_id(&self.target) {
            bail!("submission target is not an object id");
        }
        if !is_lean_name(&self.solution_module) {
            bail!("solution module is not a Lean module name");
        }
        self.solution.validate(true)?;
        let root = crate::bundle::module_path(&self.solution_module);
        let dir = root.trim_end_matches(".lean").to_string() + "/";
        for f in &self.solution.files {
            if f.path != root && !f.path.starts_with(&dir) {
                bail!("solution file {} is outside the solution module", f.path);
            }
        }
        if !self.solution.has(&root) {
            bail!("solution bundle is missing {root}");
        }
        if self.lemmas.len() > 64 {
            bail!("at most 64 lemma claims per submission");
        }
        for l in &self.lemmas {
            if !is_lean_name(&l.name) {
                bail!("lemma name {:?} is not a Lean name", l.name);
            }
            if l.statement.trim().is_empty() || l.statement.len() > 4000 {
                bail!("lemma statement must be 1 to 4000 characters");
            }
        }
        if self.note.len() > 2000 {
            bail!("note is over 2000 characters");
        }
        Ok(())
    }

    /// Shape checks plus the ones that need the target.
    pub fn validate_against(&self, target: &Target) -> Result<()> {
        self.validate_shape()?;
        for f in &self.solution.files {
            if target.project.has(&f.path) {
                bail!("solution file {} would overwrite a target file", f.path);
            }
        }
        if self.solution_module == target.challenge_module {
            bail!("solution module cannot be the challenge module");
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GateStatus {
    Pass,
    Fail,
    Skipped,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Gate {
    pub name: String,
    pub status: GateStatus,
    /// True when a pass cannot be fully trusted yet (see README, threat model).
    pub advisory: bool,
    pub detail: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Score {
    pub solution_loc: u64,
    pub baseline_loc: Option<u64>,
    pub loc_saved: u64,
    /// Always "demo": no scoring weight or band is decided (see CLAUDE.md, open questions).
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Accepted,
    Rejected,
}

/// Everything that can change a verdict, recorded so any verifier can reproduce it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CheckParams {
    /// e.g. "synterra-core/0.1.0".
    pub software: String,
    /// The judge command as configured, before path substitution.
    pub command: Vec<String>,
    /// The Lean toolchain the target pins.
    pub toolchain: String,
    pub trivial_tactics: Vec<String>,
    pub timeout_secs: u64,
    /// False only if the operator ran untrusted Lean without a sandbox prefix.
    pub sandboxed: bool,
}

/// One node's signed account of checking one submission.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Attestation {
    pub v: u32,
    pub submission: String,
    pub target: String,
    pub verdict: Verdict,
    pub gates: Vec<Gate>,
    pub score: Option<Score>,
    pub params: CheckParams,
    pub created: u64,
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lean_names() {
        assert!(is_lean_name("Nat.add_comm'"));
        assert!(is_lean_name("_aux_88213"));
        assert!(!is_lean_name("1bad"));
        assert!(!is_lean_name("a..b"));
        assert!(!is_lean_name("a b"));
        assert!(!is_lean_name("a;#eval"));
    }

    #[test]
    fn ids() {
        assert!(is_id(&"a".repeat(64)));
        assert!(!is_id(&"A".repeat(64)));
        assert!(!is_id("../etc/passwd"));
    }
}

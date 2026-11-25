//! Protocol test vectors. An independent implementation that reproduces these
//! bytes, signature and id exactly speaks the same wire format as this node.
//! `synterra vectors` prints them; docs/test-vectors.json is the committed copy,
//! and a unit test keeps the two identical.

use anyhow::Result;
use serde_json::{json, Value};

use crate::bundle::{Bundle, FileEntry};
use crate::identity::Identity;
use crate::model::{canonical, signing_bytes, Envelope, Kind, Target, PROTOCOL_VERSION};

pub const SEED: [u8; 32] = [7u8; 32];

pub fn generate() -> Result<Value> {
    let id = Identity::from_seed(SEED);
    let file = |p: &str, c: &str| FileEntry {
        path: p.into(),
        content: c.into(),
    };
    let target = Target {
        v: PROTOCOL_VERSION,
        name: "test vector".into(),
        challenge_module: "Challenge".into(),
        theorem_names: vec!["t".into()],
        permitted_axioms: vec![
            "propext".into(),
            "Quot.sound".into(),
            "Classical.choice".into(),
        ],
        project: Bundle {
            files: vec![
                file("Challenge.lean", "theorem t : True := sorry\n"),
                file("lake-manifest.json", "{}\n"),
                file("lakefile.toml", "name = \"vector\"\n"),
                file("lean-toolchain", "leanprover/lean4:v4.36.0\n"),
            ],
        },
        baseline_loc: None,
        created: 0,
    };
    let body = serde_json::to_value(&target)?;
    let sb = signing_bytes(Kind::Target, &body)?;
    let sig = id.sign(&sb);
    let env = Envelope {
        kind: Kind::Target,
        body,
        signer: id.public_hex(),
        sig: sig.clone(),
    };
    env.validate()?;
    Ok(json!({
        "protocol": PROTOCOL_VERSION,
        "about": "Synterra node wire-format test vectors. The ed25519 secret key is the RFC 8032 seed of 32 bytes of 0x07. Hex is lowercase.",
        "secret_key_seed": hex::encode(SEED),
        "public_key": id.public_hex(),
        "vectors": [{
            "kind": "target",
            "canonical_body": String::from_utf8(canonical(&env.body)?)?,
            "signing_bytes": String::from_utf8(sb)?,
            "signature": sig,
            "canonical_envelope": String::from_utf8(canonical(&env)?)?,
            "id": env.id()?,
        }],
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn committed_vectors_match_the_binary() {
        let committed: serde_json::Value =
            serde_json::from_str(include_str!("../docs/test-vectors.json")).unwrap();
        assert_eq!(
            super::generate().unwrap(),
            committed,
            "regenerate with `synterra vectors > docs/test-vectors.json`"
        );
    }
}

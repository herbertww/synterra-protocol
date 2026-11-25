//! Node identity: one ed25519 key per node, kept in the data directory.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

pub struct Identity {
    key: SigningKey,
}

impl Identity {
    pub fn load_or_create(path: &Path) -> Result<Identity> {
        if path.exists() {
            let hexkey = fs::read_to_string(path)?;
            let bytes: [u8; 32] = hex::decode(hexkey.trim())?
                .try_into()
                .map_err(|_| anyhow::anyhow!("node key must be 32 bytes"))?;
            return Ok(Identity {
                key: SigningKey::from_bytes(&bytes),
            });
        }
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        fs::write(path, hex::encode(key.to_bytes()))?;
        restrict(path)?;
        Ok(Identity { key })
    }

    /// A fixed key, for test vectors only.
    pub fn from_seed(seed: [u8; 32]) -> Identity {
        Identity {
            key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn public_hex(&self) -> String {
        hex::encode(self.key.verifying_key().to_bytes())
    }

    pub fn sign(&self, msg: &[u8]) -> String {
        hex::encode(self.key.sign(msg).to_bytes())
    }
}

pub fn verify(pubkey_hex: &str, msg: &[u8], sig_hex: &str) -> Result<()> {
    let pk: [u8; 32] = hex::decode(pubkey_hex)
        .context("signer is not hex")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("signer must be 32 bytes"))?;
    let sig: [u8; 64] = hex::decode(sig_hex)
        .context("sig is not hex")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("sig must be 64 bytes"))?;
    let vk = VerifyingKey::from_bytes(&pk)?;
    if vk.verify(msg, &Signature::from_bytes(&sig)).is_err() {
        bail!("signature does not verify");
    }
    Ok(())
}

#[cfg(unix)]
fn restrict(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let dir = tempfile::tempdir().unwrap();
        let id = Identity::load_or_create(&dir.path().join("k")).unwrap();
        let sig = id.sign(b"hello");
        assert!(verify(&id.public_hex(), b"hello", &sig).is_ok());
        assert!(verify(&id.public_hex(), b"hellO", &sig).is_err());
        let again = Identity::load_or_create(&dir.path().join("k")).unwrap();
        assert_eq!(again.public_hex(), id.public_hex());
    }
}

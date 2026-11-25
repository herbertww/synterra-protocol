//! Content-addressed object store plus a local hash-chained ledger.
//!
//! The ledger is this node's own ordered record of what it has seen and when.
//! It is not consensus: anchoring to Solana is the next step (see README).

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{canonical, is_id, sha256_hex, Envelope, Kind};

pub struct Store {
    dir: PathBuf,
    lock: Mutex<()>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LedgerEntry {
    pub height: u64,
    pub prev: String,
    pub object: String,
    pub kind: Kind,
    pub ts: u64,
    pub hash: String,
}

fn entry_hash(height: u64, prev: &str, object: &str, kind: Kind, ts: u64) -> String {
    sha256_hex(format!("{height}|{prev}|{object}|{}|{ts}", kind.as_str()).as_bytes())
}

impl Store {
    pub fn open(datadir: &Path) -> Result<Store> {
        fs::create_dir_all(datadir.join("objects"))?;
        Ok(Store {
            dir: datadir.to_path_buf(),
            lock: Mutex::new(()),
        })
    }

    fn obj_path(&self, id: &str) -> Result<PathBuf> {
        if !is_id(id) {
            bail!("malformed object id");
        }
        Ok(self.dir.join("objects").join(format!("{id}.json")))
    }

    pub fn has(&self, id: &str) -> bool {
        self.obj_path(id).map(|p| p.exists()).unwrap_or(false)
    }

    pub fn get(&self, id: &str) -> Result<Envelope> {
        let bytes = fs::read(self.obj_path(id)?).with_context(|| format!("no object {id}"))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Validate, then store and append to the ledger. Returns (id, newly_added).
    pub fn put(&self, env: &Envelope) -> Result<(String, bool)> {
        env.validate()?;
        let id = env.id()?;
        let _g = self.lock.lock().unwrap();
        let path = self.obj_path(&id)?;
        if path.exists() {
            return Ok((id, false));
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, canonical(env)?)?;
        fs::rename(&tmp, &path)?;
        self.append(&id, env.kind)?;
        Ok((id, true))
    }

    pub fn of_kind(&self, kind: Kind) -> Result<Vec<(String, Envelope)>> {
        let mut out = Vec::new();
        for e in self.ledger()? {
            if e.kind == kind {
                if let Ok(env) = self.get(&e.object) {
                    out.push((e.object, env));
                }
            }
        }
        Ok(out)
    }

    fn ledger_path(&self) -> PathBuf {
        self.dir.join("ledger.jsonl")
    }

    pub fn ledger(&self) -> Result<Vec<LedgerEntry>> {
        let p = self.ledger_path();
        if !p.exists() {
            return Ok(vec![]);
        }
        let mut out = Vec::new();
        for line in BufReader::new(fs::File::open(p)?).lines() {
            let line = line?;
            if !line.trim().is_empty() {
                out.push(serde_json::from_str(&line)?);
            }
        }
        Ok(out)
    }

    fn append(&self, object: &str, kind: Kind) -> Result<()> {
        let entries = self.ledger()?;
        let (height, prev) = match entries.last() {
            Some(e) => (e.height + 1, e.hash.clone()),
            None => (0, "0".repeat(64)),
        };
        let ts = crate::model::now();
        let hash = entry_hash(height, &prev, object, kind, ts);
        let e = LedgerEntry {
            height,
            prev,
            object: object.to_string(),
            kind,
            ts,
            hash,
        };
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.ledger_path())?;
        writeln!(f, "{}", serde_json::to_string(&e)?)?;
        Ok(())
    }

    /// Recompute every link and re-validate every object. Returns the height checked.
    pub fn verify_ledger(&self) -> Result<u64> {
        let mut prev = "0".repeat(64);
        let entries = self.ledger()?;
        for (i, e) in entries.iter().enumerate() {
            if e.height != i as u64 || e.prev != prev {
                bail!("ledger broken at height {i}");
            }
            if entry_hash(e.height, &e.prev, &e.object, e.kind, e.ts) != e.hash {
                bail!("ledger hash mismatch at height {i}");
            }
            let env = self.get(&e.object)?;
            env.validate()
                .with_context(|| format!("object at height {i}"))?;
            if env.id()? != e.object || env.kind != e.kind {
                bail!("object at height {i} does not match its address");
            }
            prev = e.hash.clone();
        }
        Ok(entries.len() as u64)
    }
}

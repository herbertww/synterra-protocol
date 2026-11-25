//! File bundles: the text files of a Lean project or solution, carried inside
//! objects. Every path is checked before anything touches the disk, because
//! bundles arrive from strangers.

use std::fs;
use std::path::{Component, Path};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_FILES: usize = 512;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FileEntry {
    pub path: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Bundle {
    pub files: Vec<FileEntry>,
}

/// `Foo.Bar` → `Foo/Bar.lean`.
pub fn module_path(module: &str) -> String {
    format!("{}.lean", module.replace('.', "/"))
}

/// Files a target project may carry. Build output, git data and binaries never travel.
fn project_file_allowed(path: &str) -> bool {
    path.ends_with(".lean")
        || matches!(
            path,
            "lakefile.toml" | "lakefile.lean" | "lean-toolchain" | "lake-manifest.json"
        )
}

pub fn safe_rel_path(path: &str) -> Result<()> {
    if path.is_empty() || path.len() > 255 {
        bail!("bad path length: {path:?}");
    }
    if !path
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"_-./".contains(&b))
    {
        bail!("path has characters outside [A-Za-z0-9_-./]: {path:?}");
    }
    let p = Path::new(path);
    if p.is_absolute() || path.starts_with('/') {
        bail!("absolute path: {path:?}");
    }
    for c in p.components() {
        match c {
            Component::Normal(seg) if !seg.is_empty() && seg != "." && seg != ".." => {}
            _ => bail!("path escapes or is unclean: {path:?}"),
        }
    }
    if path.split('/').any(|s| s.is_empty() || s.starts_with('.')) {
        bail!("hidden or empty path segment: {path:?}");
    }
    Ok(())
}

impl Bundle {
    pub fn has(&self, path: &str) -> bool {
        self.files.iter().any(|f| f.path == path)
    }

    pub fn get(&self, path: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|f| f.path == path)
            .map(|f| f.content.as_str())
    }

    /// `lean_only` is for submissions: nothing but `.lean` sources.
    pub fn validate(&self, lean_only: bool) -> Result<()> {
        if self.files.is_empty() || self.files.len() > MAX_FILES {
            bail!("bundle must hold 1 to {MAX_FILES} files");
        }
        let mut prev: Option<&str> = None;
        for f in &self.files {
            safe_rel_path(&f.path)?;
            let base = f.path.rsplit('/').next().unwrap_or("");
            if lean_only && (!f.path.ends_with(".lean") || base == "lakefile.lean") {
                bail!("only Lean source modules are allowed here, got {}", f.path);
            }
            if !lean_only && !project_file_allowed(&f.path) {
                bail!("file type not allowed in a project bundle: {}", f.path);
            }
            if f.content.len() > MAX_FILE_BYTES {
                bail!("{} is over {MAX_FILE_BYTES} bytes", f.path);
            }
            if let Some(p) = prev {
                if p >= f.path.as_str() {
                    bail!("bundle files must be sorted and unique");
                }
            }
            prev = Some(&f.path);
        }
        Ok(())
    }

    /// Read a directory (or one file) into a sorted bundle.
    pub fn read(root: &Path, lean_only: bool) -> Result<Bundle> {
        let mut files = Vec::new();
        if root.is_file() {
            let name = root
                .file_name()
                .and_then(|s| s.to_str())
                .context("bad file name")?;
            files.push(FileEntry {
                path: name.to_string(),
                content: fs::read_to_string(root)?,
            });
        } else {
            walk(root, root, lean_only, &mut files)?;
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let b = Bundle { files };
        b.validate(lean_only)?;
        Ok(b)
    }

    /// Write into `dir`. Paths were validated, but check again: this is the line that touches disk.
    pub fn write(&self, dir: &Path) -> Result<()> {
        for f in &self.files {
            safe_rel_path(&f.path)?;
            let dest = dir.join(&f.path);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dest, &f.content).with_context(|| format!("writing {}", dest.display()))?;
        }
        Ok(())
    }

    /// Lines that carry proof: not blank, not a `--` comment. The compression measure.
    pub fn loc(&self) -> u64 {
        let mut n = 0u64;
        for f in &self.files {
            let mut in_block = false;
            for line in f.content.lines() {
                let t = line.trim();
                if in_block {
                    if t.contains("-/") {
                        in_block = false;
                    }
                    continue;
                }
                if t.starts_with("/-") {
                    in_block = !t.contains("-/");
                    continue;
                }
                if t.is_empty() || t.starts_with("--") {
                    continue;
                }
                n += 1;
            }
        }
        n
    }
}

fn walk(root: &Path, dir: &Path, lean_only: bool, out: &mut Vec<FileEntry>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "build" {
            continue; // .lake, .git, build output
        }
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            walk(root, &path, lean_only, out)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        let keep = if lean_only {
            rel.ends_with(".lean") && name != "lakefile.lean"
        } else {
            project_file_allowed(&rel)
        };
        if keep {
            out.push(FileEntry {
                path: rel,
                content: fs::read_to_string(&path)?,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_escapes() {
        for bad in [
            "../x.lean",
            "/etc/passwd",
            "a/../../b.lean",
            ".lake/x.lean",
            "a//b.lean",
            "a\\b.lean",
            "a b.lean",
            "",
        ] {
            assert!(safe_rel_path(bad).is_err(), "{bad:?} should be rejected");
        }
        for good in ["Solution.lean", "Solution/Part_1.lean", "lean-toolchain"] {
            assert!(safe_rel_path(good).is_ok(), "{good:?} should pass");
        }
    }

    #[test]
    fn submission_bundles_are_lean_only() {
        let b = Bundle {
            files: vec![FileEntry {
                path: "lakefile.lean".into(),
                content: "x".into(),
            }],
        };
        assert!(b.validate(true).is_err());
    }

    #[test]
    fn counts_proof_lines() {
        let b = Bundle {
            files: vec![FileEntry {
                path: "S.lean".into(),
                content: "-- comment\n/- block\nstill -/\ntheorem t : True := by\n\n  trivial\n"
                    .into(),
            }],
        };
        assert_eq!(b.loc(), 2);
    }
}

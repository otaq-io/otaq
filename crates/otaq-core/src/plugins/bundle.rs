//! A plugin's files as one release asset (the otaq-marketplace README, "Layout"). A protocol 1 plugin
//! is only text, so a release is a JSON file listing each file's path and text: `jira-1.2.0.json`. The marketplace
//! CI writes it from the tagged folder, and Otaq checks its SHA256 against the index before reading it.
//!
//! The same shape holds a Local folder's files, read from disk.

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::manifest::{self, Manifest};

pub const FORMAT: u32 = 1;
/// The most files a plugin may hold, and their total size.
const MAX_FILES: usize = 200;
const MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;
pub const MANIFEST_FILE: &str = "plugin.yaml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bundle {
    pub format: u32,
    pub id: String,
    pub version: String,
    pub files: Vec<BundleFile>,
    /// A Local folder's files that aren't text, kept only for the open-source check to name.
    #[serde(skip)]
    pub not_text: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleFile {
    /// Relative, `/`-separated: `plugin.yaml`, `tests/fixtures/search.json`.
    pub path: String,
    pub text: String,
}

/// The lowercase hex SHA256 of `bytes`.
pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Bundle {
    /// Reads a release asset whose hash the index gave, checking the hash first.
    pub fn read(bytes: &[u8], expected_sha256: &str) -> Result<Self, String> {
        let actual = sha256(bytes);
        if !actual.eq_ignore_ascii_case(expected_sha256.trim()) {
            return Err(format!(
                "the download's SHA256 is {actual}, but the marketplace says {expected_sha256}"
            ));
        }
        let bundle: Self = serde_json::from_slice(bytes)
            .map_err(|e| format!("the download isn't a plugin: {e}"))?;
        if bundle.format != FORMAT {
            return Err(format!(
                "plugin bundle format {} isn't known",
                bundle.format
            ));
        }
        bundle.check_paths()?;
        Ok(bundle)
    }

    /// A folder's files (a Local plugin). Symbolic links are refused; hidden files and folders (`.git`) are
    /// skipped.
    pub fn from_dir(dir: &Path) -> Result<Self, String> {
        let mut files = Vec::new();
        let mut not_text = Vec::new();
        walk(dir, dir, &mut files, &mut not_text)?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let manifest = files
            .iter()
            .find(|f| f.path == MANIFEST_FILE)
            .ok_or_else(|| format!("{} has no {MANIFEST_FILE}", dir.display()))?;
        let read = manifest::read(&manifest.text).map_err(|e| e.0)?;
        let bundle = Self {
            format: FORMAT,
            id: read.id,
            version: read.version,
            files,
            not_text,
        };
        bundle.check_paths()?;
        Ok(bundle)
    }

    fn check_paths(&self) -> Result<(), String> {
        if self.files.len() > MAX_FILES {
            return Err(format!("a plugin holds at most {MAX_FILES} files"));
        }
        let total: usize = self.files.iter().map(|f| f.text.len()).sum();
        if total > MAX_TOTAL_BYTES {
            return Err("a plugin's files are over 8 MB".to_owned());
        }
        for file in &self.files {
            if !safe_path(&file.path) {
                return Err(format!("{} isn't a path inside the plugin", file.path));
            }
        }
        Ok(())
    }

    /// The manifest, which must name the bundle's own id and version.
    pub fn manifest(&self) -> Result<Manifest, String> {
        let file = self
            .files
            .iter()
            .find(|f| f.path == MANIFEST_FILE)
            .ok_or("the plugin has no plugin.yaml")?;
        let manifest = manifest::read(&file.text).map_err(|e| e.0)?;
        if manifest.id != self.id || manifest.version != self.version {
            return Err(format!(
                "plugin.yaml says {} {}, but the release is {} {}",
                manifest.id, manifest.version, self.id, self.version
            ));
        }
        Ok(manifest)
    }

    /// The open-source rules' problems ([`super::open_source`]).
    pub fn open_source_problems(&self, manifest: &Manifest) -> Vec<String> {
        super::open_source::problems(
            &manifest.license,
            self.files
                .iter()
                .map(|f| (f.path.as_str(), f.text.as_bytes()))
                .chain(
                    self.not_text
                        .iter()
                        .map(|(p, b)| (p.as_str(), b.as_slice())),
                ),
        )
    }

    /// A fingerprint of every path and text, in path order: a Local folder's is compared to see it changed.
    pub fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        let mut files: Vec<&BundleFile> = self.files.iter().collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        for file in files {
            hasher.update(file.path.as_bytes());
            hasher.update([0]);
            hasher.update(file.text.as_bytes());
            hasher.update([0]);
        }
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Writes the files under `dir`, which must not exist yet.
    pub fn write_to(&self, dir: &Path) -> std::io::Result<()> {
        crate::data_dir::DataDir::ensure_dir(dir)?;
        for file in &self.files {
            let path = dir.join(file.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = path.parent() {
                crate::data_dir::DataDir::ensure_dir(parent)?;
            }
            std::fs::write(path, &file.text)?;
        }
        Ok(())
    }

    /// The bundle as written by the marketplace's CI, and the hash an index lists for it.
    pub fn to_release(&self) -> (Vec<u8>, String) {
        let bytes = serde_json::to_vec_pretty(self).expect("a bundle always serializes");
        let hash = sha256(&bytes);
        (bytes, hash)
    }
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.contains('\0')
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

fn walk(
    root: &Path,
    dir: &Path,
    files: &mut Vec<BundleFile>,
    not_text: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("can't read {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err(format!(
                "{} is a symbolic link: plugins hold plain files",
                path.display()
            ));
        }
        if kind.is_dir() {
            walk(root, &path, files, not_text)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let bytes =
            std::fs::read(&path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
        if files.len() + not_text.len() >= MAX_FILES {
            return Err(format!("a plugin holds at most {MAX_FILES} files"));
        }
        match String::from_utf8(bytes) {
            Ok(text) => files.push(BundleFile {
                path: relative,
                text,
            }),
            Err(e) => not_text.push((relative, e.into_bytes())),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::tests::github;

    fn bundle() -> Bundle {
        Bundle {
            format: FORMAT,
            id: "github-issues".into(),
            version: "1.0.0".into(),
            files: vec![
                BundleFile {
                    path: MANIFEST_FILE.into(),
                    text: github(),
                },
                BundleFile {
                    path: "LICENSE".into(),
                    text: "MIT".into(),
                },
            ],
            not_text: Vec::new(),
        }
    }

    #[test]
    fn a_release_reads_only_with_its_hash() {
        let (bytes, hash) = bundle().to_release();
        let read = Bundle::read(&bytes, &hash).unwrap();
        assert_eq!(read.manifest().unwrap().id, "github-issues");
        assert!(
            read.open_source_problems(&read.manifest().unwrap())
                .is_empty()
        );
        assert!(
            Bundle::read(&bytes, &"0".repeat(64))
                .unwrap_err()
                .contains("SHA256")
        );
    }

    #[test]
    fn paths_stay_inside_the_plugin() {
        for path in ["../x", "/etc/passwd", "a/../../b", "a\\b", ""] {
            let mut b = bundle();
            b.files.push(BundleFile {
                path: path.into(),
                text: String::new(),
            });
            let (bytes, hash) = b.to_release();
            assert!(Bundle::read(&bytes, &hash).is_err(), "{path}");
        }
    }

    #[test]
    fn the_manifest_names_the_release() {
        let mut b = bundle();
        b.version = "1.0.1".into();
        assert!(b.manifest().unwrap_err().contains("release is"));
    }

    #[test]
    fn a_folder_reads_and_writes() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        bundle().write_to(&out).unwrap();
        std::fs::create_dir(out.join(".git")).unwrap();
        std::fs::write(out.join(".git").join("HEAD"), "x").unwrap();
        let read = Bundle::from_dir(&out).unwrap();
        assert_eq!(read.files.len(), 2, "hidden folders skipped");
        assert_eq!(read.fingerprint(), bundle().fingerprint());
        std::fs::write(out.join("tool"), [0xcf, 0xfa, 0xed, 0xfe, 0xff]).unwrap();
        let read = Bundle::from_dir(&out).unwrap();
        let problems = read.open_source_problems(&read.manifest().unwrap());
        assert!(
            problems.iter().any(|p| p.starts_with("tool")),
            "{problems:?}"
        );
    }
}

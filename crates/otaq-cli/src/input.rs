//! Where a command's long text comes from: stdin (`--stdin`), or the file `--stdin-file <PATH>` names, which every
//! command with `--stdin` also takes and reads the same way (fixes-before-0-3, piece 1).
//!
//! A Codex agent's command that feeds `otaq` through a heredoc, a pipe or `<` runs in Codex's sandbox, where `otaq`
//! can't reach the service; a plain `otaq … --stdin-file <PATH>` matches Codex's `otaq` rule and runs outside it.
//! [`crate::cli::Command::take_stdin_file`] turns `--stdin-file` into `--stdin` once the arguments are parsed, so each
//! command checks and trims its text exactly as before; only where the bytes come from changes, here.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use otaq_core::protocol::{ErrorBody, ErrorKind};

use crate::service::internal;

/// The `--stdin-file` of this run, set once from `main` before the command runs.
static STDIN_FILE: OnceLock<PathBuf> = OnceLock::new();

/// Reads the command's text from `path` instead of stdin.
pub fn use_file(path: PathBuf) {
    // One command per process: set at most once.
    let _ = STDIN_FILE.set(path);
}

/// The command's whole text, untrimmed: the `--stdin-file` if one was given, else stdin.
pub fn read() -> Result<String, ErrorBody> {
    read_from(STDIN_FILE.get().map(PathBuf::as_path))
}

/// [`read`] with the file given: `None` reads stdin.
fn read_from(file: Option<&Path>) -> Result<String, ErrorBody> {
    let mut text = String::new();
    match file {
        Some(path) => {
            std::fs::File::open(path)
                .and_then(|mut f| f.read_to_string(&mut text))
                .map_err(|e| {
                    ErrorBody::new(
                        ErrorKind::Usage,
                        format!("can't read {}: {e}", path.display()),
                    )
                })?;
        }
        None => {
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| internal(format!("can't read stdin: {e}")))?;
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_read_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("summary.md");
        let text = "  Added the price table.\n\n`npm test`: 48 passed — \"quotes\", 'single', $HOME, `ticks`\n\n";
        std::fs::write(&path, text).unwrap();
        assert_eq!(read_from(Some(&path)).unwrap(), text);
    }

    #[test]
    fn a_missing_file_is_a_usage_error_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.md");
        let error = read_from(Some(&path)).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Usage);
        assert!(
            error
                .message
                .starts_with(&format!("can't read {}: ", path.display())),
            "{}",
            error.message
        );
    }
}

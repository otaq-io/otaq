//! `file_links.*`: file paths in a terminal's output as links (d0117 point 8). The page sends the lines it shows, a
//! batch at a time; each candidate path (`otaq-view`'s rules) is a link only when it names an existing file inside the
//! session's folder, checked here once per batch: no folders, nothing missing, nothing outside, a symlink out
//! included. A `file:` link a program sent (OSC 8, point 7) is checked the same way.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use otaq_core::board::files::clean_path;
use otaq_core::protocol::{KeyParams, Reply, Request, SessionListParams};
use otaq_core::session::SessionKind;
use otaq_view::file_paths::{self, FileLink};
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

/// The most lines one batch looks at (a tall terminal's screen, wrapped lines joined).
const MAX_LINES: usize = 400;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "file_links.find" => reply(&find(app, parse(params)?).await?),
        "file_links.check" => reply(&check(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

/// The lines a terminal shows (`file_links.find`), answered with each line's links.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct FileLinksFind {
    pub session: String,
    pub lines: Vec<String>,
}

/// A `file:` link's path (`file_links.check`), answered with the file inside the session's folder, or `null`.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct FileLinkCheck {
    pub session: String,
    pub path: String,
}

async fn find(app: &App, params: FileLinksFind) -> Result<Vec<Vec<FileLink>>, RequestError> {
    let Some(folder) = folder(app, &params.session).await? else {
        return Ok(vec![Vec::new(); params.lines.len()]);
    };
    // Lines past the most get no links (one answer per line all the same).
    let count = params.lines.len();
    let lines: Vec<String> = params.lines.into_iter().take(MAX_LINES).collect();
    let mut out = tokio::task::spawn_blocking(move || links_in(&folder, &lines))
        .await
        .map_err(|e| RequestError::plain(e.to_string()))?;
    out.resize(count, Vec::new());
    Ok(out)
}

async fn check(app: &App, params: FileLinkCheck) -> Result<Option<String>, RequestError> {
    let Some(folder) = folder(app, &params.session).await? else {
        return Ok(None);
    };
    tokio::task::spawn_blocking(move || Folder::new(&folder)?.file(&params.path))
        .await
        .map_err(|e| RequestError::plain(e.to_string()))
}

/// The folder `session` works in, as the service started it: a ticket's sessions (its agent, validator, Shell, Run)
/// in the ticket's worktree; Noyan and Project terminals in the project's folder. `None`
/// for a session of neither (an agent tool's Sign in…) or a ticket without a worktree.
async fn folder(app: &App, session: &str) -> Result<Option<PathBuf>, RequestError> {
    let info = match app
        .service
        .send(Request::SessionList(SessionListParams { key: None }))
        .await?
    {
        Reply::Sessions(list) => list.into_iter().find(|s| s.id == session),
        other => return Err(unexpected("session.list", &other)),
    };
    let Some(info) = info else { return Ok(None) };
    match info.kind {
        SessionKind::Project | SessionKind::ProjectAgent => {
            match app.service.send(Request::ProjectList).await? {
                Reply::Projects(list) => Ok(list
                    .into_iter()
                    .find(|p| p.key == info.project)
                    .map(|p| PathBuf::from(p.repo_path))),
                other => Err(unexpected("project.list", &other)),
            }
        }
        _ => {
            let Some(key) = info.ticket else { return Ok(None) };
            match app
                .service
                .send(Request::TicketGet(KeyParams {
                    key: key.to_string(),
                }))
                .await?
            {
                Reply::Ticket(t) => Ok(t.work.and_then(|w| w.worktree).map(PathBuf::from)),
                other => Err(unexpected("ticket.get", &other)),
            }
        }
    }
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {what}: {other:?}"))
}

/// Each line's links: every candidate looked up once per batch, however often it appears.
fn links_in(folder: &Path, lines: &[String]) -> Vec<Vec<FileLink>> {
    let Some(folder) = Folder::new(folder) else {
        return vec![Vec::new(); lines.len()];
    };
    let mut known: HashMap<String, Option<String>> = HashMap::new();
    lines
        .iter()
        .map(|line| {
            let found = file_paths::candidates(line)
                .into_iter()
                .filter_map(|c| {
                    let file = known
                        .entry(c.path.clone())
                        .or_insert_with(|| folder.file(&c.path))
                        .clone()?;
                    Some((c, file))
                })
                .collect();
            file_paths::links(found)
        })
        .collect()
}

/// A session's folder, as written and as it resolves.
struct Folder {
    written: PathBuf,
    real: PathBuf,
}

impl Folder {
    fn new(folder: &Path) -> Option<Self> {
        Some(Self {
            written: folder.to_path_buf(),
            real: std::fs::canonicalize(folder).ok()?,
        })
    }

    /// The existing file `path` names inside the folder (relative to it, or absolute under it), as a clean relative
    /// path; `None` for a folder, a missing file, or anything that is or resolves outside (a symlink out).
    fn file(&self, path: &str) -> Option<String> {
        let relative = if Path::new(path).is_absolute() {
            let p = Path::new(path);
            let rest = p
                .strip_prefix(&self.written)
                .or_else(|_| p.strip_prefix(&self.real))
                .ok()?;
            rest.components()
                .map(|c| match c {
                    Component::Normal(part) => part.to_str().map(ToOwned::to_owned),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?
                .join("/")
        } else {
            path.to_owned()
        };
        let clean = clean_path(&relative, false).ok()?;
        let full = std::fs::canonicalize(self.real.join(&clean)).ok()?;
        let inside = full
            .strip_prefix(&self.real)
            .is_ok_and(|rest| rest.components().all(|c| matches!(c, Component::Normal(_))));
        (inside && full.is_file()).then_some(clean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("shop");
        std::fs::create_dir_all(root.join("src/lib")).unwrap();
        std::fs::write(root.join("src/pricing.ts"), "export {}\n").unwrap();
        std::fs::write(tmp.path().join("outside.ts"), "secret\n").unwrap();
        (tmp, root)
    }

    #[test]
    fn an_existing_file_inside_the_folder_is_a_link() {
        let (_tmp, root) = folder();
        let f = Folder::new(&root).unwrap();
        assert_eq!(f.file("src/pricing.ts").as_deref(), Some("src/pricing.ts"));
        let absolute = root.join("src/pricing.ts");
        assert_eq!(f.file(absolute.to_str().unwrap()).as_deref(), Some("src/pricing.ts"));
        let real = std::fs::canonicalize(&root).unwrap().join("src/pricing.ts");
        assert_eq!(f.file(real.to_str().unwrap()).as_deref(), Some("src/pricing.ts"));
    }

    #[test]
    fn folders_missing_files_and_paths_outside_are_refused() {
        let (tmp, root) = folder();
        let f = Folder::new(&root).unwrap();
        assert_eq!(f.file("src/lib"), None, "a folder");
        assert_eq!(f.file("src/gone.ts"), None, "a missing file");
        assert_eq!(f.file("../outside.ts"), None, "a relative path out");
        let outside = tmp.path().join("outside.ts");
        assert_eq!(f.file(outside.to_str().unwrap()), None, "an absolute path out");
        assert_eq!(f.file("src/../../outside.ts"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_folder_is_refused() {
        let (tmp, root) = folder();
        std::os::unix::fs::symlink(tmp.path().join("outside.ts"), root.join("src/link.ts")).unwrap();
        std::os::unix::fs::symlink(tmp.path(), root.join("up")).unwrap();
        std::os::unix::fs::symlink(root.join("src/pricing.ts"), root.join("src/inside.ts")).unwrap();
        let f = Folder::new(&root).unwrap();
        assert_eq!(f.file("src/link.ts"), None, "a file symlinked out");
        assert_eq!(f.file("up/outside.ts"), None, "through a folder symlinked out");
        assert_eq!(f.file("src/inside.ts").as_deref(), Some("src/inside.ts"), "a symlink inside");
    }

    #[test]
    fn a_batch_finds_each_lines_links() {
        let (_tmp, root) = folder();
        let lines = vec![
            "Edited src/pricing.ts:42 and src/gone.ts.".to_owned(),
            "nothing here".to_owned(),
            "../outside.ts src/lib src/pricing.ts(3,1)".to_owned(),
        ];
        let links = links_in(&root, &lines);
        assert_eq!(links.len(), 3);
        assert_eq!(
            links[0],
            vec![FileLink {
                start: 7,
                end: 24,
                path: "src/pricing.ts".into(),
                line: Some(42),
            }]
        );
        assert!(links[1].is_empty());
        assert_eq!(links[2].len(), 1);
        assert_eq!((links[2][0].path.as_str(), links[2][0].line), ("src/pricing.ts", Some(3)));
    }
}

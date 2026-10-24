//! `report.*`: the report view over the sheet (CMT-12; d0109; `s05-41` / `s05-42`), for a comment's report (by its
//! number, also a deleted comment's from History, CMT-14) or the text pasted into a comment box. The page never
//! holds a report whole: `report.open` indexes the file's lines here (`otaq-view`'s `report_view.rs`) and answers its
//! words and size; `report.page` sends a page of lines, `report.search` / `report.step` search the file here and
//! answer "2 of 6" and the match's line. **Copy** pipes the file to the system clipboard tool and **Download** copies
//! it where the save dialog said, both from the file, so 20 MB never travels to the page. `report.close` forgets it.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use otaq_core::protocol::{KeyParams, Reply, Request, TicketCommentParams};
use otaq_view::view_models::report_view::{self as vm, Body, OpenReport};
use otaq_view::view_models::ticket_comments as comments;
use otaq_view::view_models::tool_avatar;
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

/// The open reports, by the handle `report.open` answered.
#[derive(Default)]
pub struct OpenReports {
    open: Mutex<HashMap<String, Arc<Mutex<OpenReport>>>>,
}

impl OpenReports {
    fn get(&self, handle: &str) -> Result<Arc<Mutex<OpenReport>>, RequestError> {
        self.open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(handle)
            .cloned()
            .ok_or_else(|| RequestError::plain("the report is closed"))
    }
}

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "report.open" => reply(&open(app, parse(params)?).await?),
        "report.page" => reply(&page(app, parse(params)?).await?),
        "report.search" => reply(&search(app, parse(params)?).await?),
        "report.step" => reply(&step(app, parse(params)?)?),
        "report.copy" => reply(&copy(app, parse(params)?).await?),
        "report.download" => reply(&download(app, parse(params)?).await?),
        "report.close" => reply(&close(app, parse(params)?)),
        _ => Err(unknown(method)),
    }
}

/// The page's types (`generated.rs`' one line for them).
#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    vm::View::export_all(cfg)?;
    vm::Page::export_all(cfg)?;
    vm::Found::export_all(cfg)?;
    OpenParams::export_all(cfg)?;
    PageParams::export_all(cfg)?;
    SearchParams::export_all(cfg)?;
    StepParams::export_all(cfg)?;
    HandleParams::export_all(cfg)?;
    DownloadParams::export_all(cfg)?;
    Ok(())
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(format!("bad parameters: {e}")))
}

fn failed(e: impl std::fmt::Display) -> RequestError {
    RequestError::plain(format!("the report can\u{2019}t be read: {e}"))
}

/// What Open shows: comment `n`'s report, or the text pasted into a box on `ticket` (the chip's Open).
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "ReportOpenParams")]
pub enum OpenParams {
    Comment { ticket: String, n: u32 },
    Pasted { ticket: String, text: String },
}

/// `{"handle": "…", "start": 1180, "count": 200}`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ReportPageParams")]
pub struct PageParams {
    pub handle: String,
    pub start: u64,
    pub count: u32,
}

/// `{"handle": "…", "query": "10000"}`; an empty query clears the search.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ReportSearchParams")]
pub struct SearchParams {
    pub handle: String,
    pub query: String,
}

/// Next (`forward`) or previous match.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ReportStepParams")]
pub struct StepParams {
    pub handle: String,
    pub forward: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ReportHandleParams")]
pub struct HandleParams {
    pub handle: String,
}

/// Download: where the save dialog said to write it.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ReportDownloadParams")]
pub struct DownloadParams {
    pub handle: String,
    pub to: String,
}

/// The comment's row in the thread (its poster line); none once deleted.
async fn comment_row(app: &App, key: &str, n: u32) -> Option<comments::CommentRow> {
    let Ok(Reply::TicketComments(list)) = app
        .service
        .send(Request::TicketComments(KeyParams {
            key: key.to_owned(),
        }))
        .await
    else {
        return None;
    };
    let thread = comments::thread(
        &comments::Facts {
            key,
            comments: &list.comments,
            working: None,
            avatars: tool_avatar::shown(),
        },
        &comments::local_clock,
    );
    thread.comments.into_iter().find(|c| c.n == n)
}

/// Opens it in a background thread (the index is one streaming read of the file).
async fn index(body: Body) -> Result<OpenReport, RequestError> {
    tokio::task::spawn_blocking(move || OpenReport::open(body))
        .await
        .map_err(failed)?
        .map_err(failed)
}

pub async fn open(app: &App, params: OpenParams) -> Result<vm::View, RequestError> {
    let (report, ticket, n, row) = match params {
        OpenParams::Comment { ticket, n } => {
            let file = match app
                .service
                .send(Request::TicketReport(TicketCommentParams {
                    key: ticket.clone(),
                    n,
                }))
                .await?
            {
                Reply::TicketReport(file) => file,
                other => {
                    return Err(RequestError::plain(format!(
                        "unexpected reply to ticket.report: {other:?}"
                    )));
                }
            };
            let row = match file.deleted {
                true => None,
                false => comment_row(app, &ticket, n).await,
            };
            let report = index(Body::File(PathBuf::from(&file.path))).await?;
            (report, ticket, Some(n), row)
        }
        OpenParams::Pasted { ticket, text } => {
            let report = index(Body::Text(Arc::from(text.into_bytes()))).await?;
            (report, ticket, None, None)
        }
    };
    let handle = uuid::Uuid::new_v4().to_string();
    let of = match n {
        Some(n) => vm::Of::Comment {
            key: &ticket,
            n,
            row: row.as_ref(),
        },
        None => vm::Of::Pasted { key: &ticket },
    };
    let view = vm::view(handle.clone(), &report, &of);
    app.reports
        .open
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(handle, Arc::new(Mutex::new(report)));
    Ok(view)
}

/// Runs `work` on the report off the async threads (it reads the file).
async fn with_report<T: Send + 'static>(
    app: &App,
    handle: &str,
    work: impl FnOnce(&mut OpenReport) -> std::io::Result<T> + Send + 'static,
) -> Result<T, RequestError> {
    let report = app.reports.get(handle)?;
    tokio::task::spawn_blocking(move || {
        let mut report = report
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        work(&mut report)
    })
    .await
    .map_err(failed)?
    .map_err(failed)
}

pub async fn page(app: &App, params: PageParams) -> Result<vm::Page, RequestError> {
    with_report(app, &params.handle, move |r| r.page(params.start, params.count)).await
}

pub async fn search(app: &App, params: SearchParams) -> Result<vm::Found, RequestError> {
    with_report(app, &params.handle, move |r| r.search(&params.query)).await
}

pub fn step(app: &App, params: StepParams) -> Result<vm::Found, RequestError> {
    let report = app.reports.get(&params.handle)?;
    let mut report = report
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Ok(report.step(params.forward))
}

/// The system's clipboard tool and its arguments; Windows' takes UTF-16 (see [`copy_text`]).
fn clipboard_tools() -> &'static [(&'static str, &'static [&'static str])] {
    if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(windows) {
        &[("clip", &[])]
    } else {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    }
}

/// Puts the report's text on the clipboard through the system's tool, streamed from its file (Rust side only).
fn copy_text(body: &Body) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, "no clipboard tool");
    for (tool, args) in clipboard_tools() {
        let child = Command::new(tool)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(e) => {
                last = e;
                continue;
            }
        };
        {
            let mut stdin = child.stdin.take().expect("piped");
            if cfg!(windows) {
                // clip.exe reads UTF-16 with its byte-order mark.
                let text = match body {
                    Body::File(path) => String::from_utf8_lossy(&std::fs::read(path)?).into_owned(),
                    Body::Text(text) => String::from_utf8_lossy(text).into_owned(),
                };
                let mut wide = vec![0xFF, 0xFE];
                wide.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
                stdin.write_all(&wide)?;
            } else {
                match body {
                    Body::File(path) => {
                        std::io::copy(&mut std::fs::File::open(path)?, &mut stdin)?;
                    }
                    Body::Text(text) => stdin.write_all(text)?,
                }
            }
        }
        let status = child.wait()?;
        if status.success() {
            return Ok(());
        }
        last = std::io::Error::other(format!("{tool} failed"));
    }
    Err(last)
}

/// Copy: answers the button's "Copied".
pub async fn copy(app: &App, params: HandleParams) -> Result<String, RequestError> {
    with_report(app, &params.handle, |r| {
        copy_text(r.body())?;
        Ok("Copied".to_owned())
    })
    .await
}

/// Download: writes the report where the save dialog said; answers the button's "Saved".
pub async fn download(app: &App, params: DownloadParams) -> Result<String, RequestError> {
    let to = PathBuf::from(&params.to);
    with_report(app, &params.handle, move |r| {
        match r.body() {
            Body::File(path) => std::fs::copy(path, &to).map(|_| ())?,
            Body::Text(text) => std::fs::write(&to, text)?,
        }
        Ok("Saved".to_owned())
    })
    .await
}

pub fn close(app: &App, params: HandleParams) -> bool {
    app.reports
        .open
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&params.handle)
        .is_some()
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::board::tickets::NewTicketInput;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    use super::*;

    /// A comment's report through the page's requests: the head, a page, a search with next / previous, Download
    /// (from the file), the deleted comment's report still opening, and a paste not posted yet.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_comments_report_opens_pages_searches_and_downloads() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("shop-web", 1);
        let Reply::ProjectAdded { project, .. } = h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }))
            .await
            .unwrap()
        else {
            panic!("no project");
        };
        let Reply::Ticket(created) = h
            .app
            .service
            .send(Request::TicketCreate(NewTicketInput {
                project: project.key.clone(),
                title: "Export orders as CSV".into(),
                ..Default::default()
            }))
            .await
            .unwrap()
        else {
            panic!("no ticket");
        };
        let key = created.key.to_string();
        let mut run: String = (1..=2_310)
            .map(|i| format!("test export::case_{i} ... ok\n"))
            .collect();
        run.push_str("assertion failed: expected at most 10000 rows\n");
        table(
            &h.app,
            "ticket_comments.post",
            json!({ "key": key, "body": "Fail: the 10,000 row limit isn't enforced.", "pasted": run }),
        )
        .await
        .unwrap();

        let view = table(
            &h.app,
            "report.open",
            json!({ "kind": "comment", "ticket": key, "n": 1 }),
        )
        .await
        .unwrap();
        assert_eq!(view["title"], "Report");
        assert_eq!(view["lines"], 2_311);
        assert!(
            view["meta"]
                .as_str()
                .unwrap()
                .ends_with("\u{b7} 2,311 lines \u{b7} read-only"),
            "{}",
            view["meta"]
        );
        assert_eq!(view["poster"]["on"], format!("\u{b7} on {key}\u{2019}s comment"));
        assert!(view["poster"]["heading"].as_str().unwrap().starts_with("You \u{b7} "));
        let handle = view["handle"].as_str().unwrap().to_owned();

        let page = table(
            &h.app,
            "report.page",
            json!({ "handle": handle, "start": 1_179, "count": 20 }),
        )
        .await
        .unwrap();
        assert_eq!(page["lines"][0]["number"], 1_180);
        assert_eq!(page["lines"].as_array().unwrap().len(), 20);

        let found = table(
            &h.app,
            "report.search",
            json!({ "handle": handle, "query": "case_23" }),
        )
        .await
        .unwrap();
        assert_eq!(found["label"], "1 of 22");
        assert_eq!(found["line"], 22);
        let found = table(
            &h.app,
            "report.step",
            json!({ "handle": handle, "forward": false }),
        )
        .await
        .unwrap();
        assert_eq!(found["label"], "22 of 22");
        assert_eq!(found["line"], 2_309);

        let to = repos.root().join("SHOP-report.txt");
        let saved = table(
            &h.app,
            "report.download",
            json!({ "handle": handle, "to": to.display().to_string() }),
        )
        .await
        .unwrap();
        assert_eq!(saved, "Saved");
        assert_eq!(std::fs::read_to_string(&to).unwrap(), run);
        assert_eq!(
            table(&h.app, "report.close", json!({ "handle": handle }))
                .await
                .unwrap(),
            true
        );
        assert!(
            table(
                &h.app,
                "report.page",
                json!({ "handle": handle, "start": 0, "count": 1 })
            )
            .await
            .is_err()
        );

        // The user's description edit, and the comment deleted: History draws them (CMT-14).
        h.app
            .service
            .send(Request::TicketUpdate(otaq_core::protocol::TicketUpdateParams {
                key: key.clone(),
                update: otaq_core::board::tickets::TicketUpdate {
                    body: Some("Admins export orders.\n## Agreed\n- CSV only".into()),
                    ..Default::default()
                },
            }))
            .await
            .unwrap();
        // A deleted comment's report still opens (History), without a poster line.
        table(&h.app, "ticket_comments.delete", json!({ "key": key, "n": 1 }))
            .await
            .unwrap();
        let history = table(&h.app, "sections.history", json!({ "key": key }))
            .await
            .unwrap();
        let titles: Vec<&str> = history["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["title"].as_str().unwrap())
            .collect();
        // Activity lists them oldest first (SHT-05).
        assert_eq!(
            titles[titles.len() - 3..],
            [
                "You commented \u{b7} with a report (67 KB)",
                "You edited the description \u{b7} added Admins export orders. (3 lines)",
                "You deleted a comment",
            ],
            "{titles:?}"
        );
        let last = titles.len() - 1;
        let deleted = &history["rows"][last];
        assert_eq!(deleted["mark"]["kind"], "you");
        assert_eq!(deleted["opens"]["report"]["n"], 1);
        assert_eq!(
            history["rows"][last - 1]["opens"]["lines"][1]["text"],
            "## Agreed"
        );
        let view = table(
            &h.app,
            "report.open",
            json!({ "kind": "comment", "ticket": key, "n": 1 }),
        )
        .await
        .unwrap();
        assert_eq!(view["lines"], 2_311);
        assert_eq!(view["poster"], Value::Null);

        // A paste not posted yet.
        let view = table(
            &h.app,
            "report.open",
            json!({ "kind": "pasted", "ticket": key, "text": "one\ntwo\n" }),
        )
        .await
        .unwrap();
        assert_eq!(view["title"], "Pasted text");
        assert_eq!(view["meta"], "\u{b7} 8 bytes \u{b7} 2 lines \u{b7} read-only");
        h.service.stop_all().await;
    }
}

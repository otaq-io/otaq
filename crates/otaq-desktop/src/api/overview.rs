//! `overview.*`: the sheet's Overview (SHT-03, RPT-02; `s25-03`…`s25-05`). Its words, the description's editing words
//! and the Attachments column come from `otaq-view`'s `overview.rs`; the reports are the ticket's comments' (the
//! service's `ticket.comments`), so the column follows `ticket_changed` like the thread. `overview.file` reads a file
//! picked with Add or dropped on Overview for the comment box's chip: its name and size, or why it can't be a report
//! (missing, not a file, over 20 MB: core's own refusals, so the box says what posting would say).
//! `overview.usage_limit` is USG-07's "Mark as usage limit" on an Idle ticket (the Inbox's row action before T-0459).
//! `overview.manual_check_tick` ticks an item of "Manual checks for you" (d0125; the service's
//! `ticket.manual_check_tick`).

use std::path::PathBuf;

use otaq_core::board::reports::{open_report, size_label};
use otaq_core::protocol::{KeyParams, ManualCheckTickParams, Reply, Request};
use otaq_view::keymap::Platform;
use otaq_view::view_models::overview::{self as vm, OverviewView};
use otaq_view::view_models::tool_avatar;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "overview.view" => reply(&view(app, parse(params)?).await?),
        "overview.file" => reply(&file(parse(params)?).await?),
        "overview.usage_limit" => {
            mark_usage_limit(app, parse(params)?).await?;
            reply(&Value::Null)
        }
        "overview.manual_check_tick" => {
            tick(app, parse(params)?).await?;
            reply(&Value::Null)
        }
        _ => Err(unknown(method)),
    }
}

/// The page's types: Overview's view and the requests' params (`generated.rs`' one line for them).
#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    OverviewView::export_all(cfg)?;
    vm::DescriptionState::export_all(cfg)?;
    ViewParams::export_all(cfg)?;
    FileParams::export_all(cfg)?;
    FileChip::export_all(cfg)?;
    vm::UsageLimitButton::export_all(cfg)?;
    TickParams::export_all(cfg)?;
    Ok(())
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(format!("bad parameters: {e}")))
}

/// `{"key": "SHOP-22"}`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "OverviewParams")]
pub struct ViewParams {
    pub key: String,
}

/// `{"path": "/Users/me/perf-notes.md"}`: a file picked with Add or dropped on Overview.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "OverviewFileParams")]
pub struct FileParams {
    pub path: String,
}

/// `{"key": "DEMO-3", "n": 2, "ticked": true}`: the user ticks (or unticks) manual check 2 (d0125).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ManualCheckTickParams")]
pub struct TickParams {
    pub key: String,
    pub n: u32,
    pub ticked: bool,
}

/// The comment box's chip for a file: "perf-notes.md", "2 KB"; posted as the comment's report by its path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OverviewFileChip")]
pub struct FileChip {
    pub path: String,
    pub name: String,
    pub size: String,
}

pub async fn view(app: &App, params: ViewParams) -> Result<OverviewView, RequestError> {
    let k = || KeyParams {
        key: params.key.clone(),
    };
    let (ticket, checks) = tokio::join!(
        app.service.send(Request::TicketGet(k())),
        app.service.send(Request::TicketManualChecks(k())),
    );
    let status = match ticket? {
        Reply::Ticket(t) => t.status,
        other => {
            return Err(RequestError::plain(format!(
                "unexpected reply to ticket.get: {other:?}"
            )));
        }
    };
    let comments = match app
        .service
        .send(Request::TicketComments(KeyParams {
            key: params.key.clone(),
        }))
        .await?
    {
        Reply::TicketComments(list) => list.comments,
        other => {
            return Err(RequestError::plain(format!(
                "unexpected reply to ticket.comments: {other:?}"
            )));
        }
    };
    // Unread, the checklist doesn't show; the review goes on without it (d0125: it never blocks).
    let checks = match checks {
        Ok(Reply::ManualChecks(c)) => Some(c),
        _ => None,
    };
    Ok(vm::view(
        &params.key,
        status,
        &comments,
        checks.as_ref(),
        tool_avatar::shown(),
        Platform::current(),
    ))
}

/// "Mark as usage limit" (USG-07): the Idle agents fold into the usage-limit banner (`usage.mark_limit`).
pub async fn mark_usage_limit(app: &App, params: ViewParams) -> Result<(), RequestError> {
    app.service
        .send(Request::UsageMarkLimit(KeyParams { key: params.key }))
        .await?;
    Ok(())
}

/// A tick in "Manual checks for you" (d0125): recorded in History by the service, which publishes `ticket_changed`,
/// so the Overview follows.
pub async fn tick(app: &App, params: TickParams) -> Result<(), RequestError> {
    app.service
        .send(Request::TicketManualCheckTick(ManualCheckTickParams {
            key: params.key,
            n: params.n,
            ticked: params.ticked,
        }))
        .await?;
    Ok(())
}

/// The chip for the file at `path`, or core's refusal as the error ("The report is 212 MB; the limit is 20 MB…").
pub async fn file(params: FileParams) -> Result<FileChip, RequestError> {
    let path = PathBuf::from(&params.path);
    let opened = path.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        let file = open_report(&opened).map_err(|e| e.to_string())?;
        file.metadata().map(|m| m.len()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| RequestError::plain(e.to_string()))?
    .map_err(RequestError::plain)?;
    Ok(FileChip {
        name: path
            .file_name()
            .map_or_else(|| params.path.clone(), |n| n.to_string_lossy().into_owned()),
        size: size_label(bytes),
        path: params.path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file for the chip: its name and size; a missing one and one over 20 MB refused with core's words.
    #[tokio::test]
    async fn a_file_becomes_a_chip_or_says_why_not() {
        let dir = tempfile::tempdir().unwrap();
        let notes = dir.path().join("perf-notes.md");
        std::fs::write(&notes, "x".repeat(2 * 1024)).unwrap();
        let chip = file(FileParams {
            path: notes.display().to_string(),
        })
        .await
        .unwrap();
        assert_eq!((chip.name.as_str(), chip.size.as_str()), ("perf-notes.md", "2 KB"));

        let missing = file(FileParams {
            path: dir.path().join("gone.md").display().to_string(),
        })
        .await
        .unwrap_err();
        assert!(missing.message.ends_with("it doesn't exist"), "{}", missing.message);

        let big = dir.path().join("run.log");
        let f = std::fs::File::create(&big).unwrap();
        f.set_len(21 * 1024 * 1024).unwrap();
        let refused = file(FileParams {
            path: big.display().to_string(),
        })
        .await
        .unwrap_err();
        assert!(refused.message.starts_with("The report is 21 MB; the limit is 20 MB."), "{}", refused.message);
    }
}

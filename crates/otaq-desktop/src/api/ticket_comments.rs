//! `ticket_comments.*`: the comments in the Ticket tab and their box (CMT-10, CMT-11; d0109; `s05-41`…`s05-44`). The
//! thread's words come from `otaq-view`'s `ticket_comments.rs`; posting, editing and deleting go to the service
//! (`ticket.comment_post`, `…_edit`, `…_delete`), which checks the limit and the rights again and publishes
//! `ticket_changed`, so the thread follows. A paste over the limit travels as the comment's report (`{"text": …}`).

use otaq_core::board::reports::ReportSource;
use otaq_core::protocol::{
    KeyParams, Reply, Request, SessionListParams, TicketCommentEditParams, TicketCommentParams,
    TicketCommentPostParams,
};
use otaq_core::session::{SessionInfo, SessionKind};
use otaq_view::view_models::ticket_comments::{self as vm, ThreadView};
use otaq_view::view_models::tool_avatar;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "ticket_comments.thread" => reply(&thread(app, parse(params)?).await?),
        "ticket_comments.post" => reply(&post(app, parse(params)?).await?),
        "ticket_comments.edit" => reply(&edit(app, parse(params)?).await?),
        "ticket_comments.delete" => reply(&delete(app, parse(params)?).await?),
        "ticket_comments.pasted_size" => reply(&pasted_size(parse(params)?)),
        _ => Err(unknown(method)),
    }
}

/// The page's types: the thread's view and the requests' params (`generated.rs`' one line for them).
#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    ThreadView::export_all(cfg)?;
    ThreadParams::export_all(cfg)?;
    PostParams::export_all(cfg)?;
    EditParams::export_all(cfg)?;
    DeleteParams::export_all(cfg)?;
    SizeParams::export_all(cfg)?;
    Done::export_all(cfg)?;
    Ok(())
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(format!("bad parameters: {e}")))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected reply to {what}: {other:?}"))
}

/// `{"key": "SHOP-34"}`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "CommentThreadParams")]
pub struct ThreadParams {
    pub key: String,
}

/// `{"key": "SHOP-34", "body": "…", "pasted": "…48 KB of log…"}`: a new comment, with the pasted text as its report;
/// or with `"file": "/Users/me/perf-notes.md"`, a file added in Overview (Add, a drop; RPT-02), copied as its report.
/// `"tell_agent": true` also tells the running agent to read it (d0123).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "CommentPostParams")]
pub struct PostParams {
    pub key: String,
    pub body: String,
    #[serde(default)]
    pub pasted: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    /// "Also tell the agent now" ticked (d0123).
    #[serde(default)]
    pub tell_agent: bool,
}

/// `{"key": "SHOP-34", "n": 3, "body": "…"}`: comment 3's new text.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "CommentEditParams")]
pub struct EditParams {
    pub key: String,
    pub n: u32,
    pub body: String,
}

/// `{"key": "SHOP-34", "n": 4}`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "CommentDeleteParams")]
pub struct DeleteParams {
    pub key: String,
    pub n: u32,
}

/// `{"bytes": 49152}`: a pasted text's size in UTF-8 bytes.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "CommentSizeParams")]
pub struct SizeParams {
    pub bytes: u64,
}

/// A change the service took: the comment's number. The thread reloads on `ticket_changed`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "CommentDone")]
pub struct Done {
    pub n: u32,
}

async fn comments(
    app: &App,
    key: &str,
) -> Result<Vec<otaq_core::board::ticket_comments::TicketComment>, RequestError> {
    match app
        .service
        .send(Request::TicketComments(KeyParams {
            key: key.to_owned(),
        }))
        .await?
    {
        Reply::TicketComments(list) => Ok(list.comments),
        other => Err(unexpected("ticket.comments", &other)),
    }
}

/// The agent working on the ticket now: its running agent or check session's agent.
fn working(sessions: &[SessionInfo]) -> Option<&str> {
    sessions
        .iter()
        .filter(|s| matches!(s.kind, SessionKind::Agent | SessionKind::Validator) && s.is_running())
        .find_map(|s| s.agent.as_ref().map(|a| a.name.as_str()))
}

pub async fn thread(app: &App, params: ThreadParams) -> Result<ThreadView, RequestError> {
    let (list, sessions) = tokio::join!(
        comments(app, &params.key),
        app.service.send(Request::SessionList(SessionListParams {
            key: Some(params.key.clone()),
        })),
    );
    let sessions = match sessions {
        Ok(Reply::Sessions(list)) => list,
        _ => Vec::new(),
    };
    Ok(vm::thread(
        &vm::Facts {
            key: &params.key,
            comments: &list?,
            working: working(&sessions),
            avatars: tool_avatar::shown(),
        },
        &vm::local_clock,
    ))
}

fn posted(what: &str, answer: Reply) -> Result<Done, RequestError> {
    match answer {
        Reply::TicketComment(c) => Ok(Done { n: c.n }),
        other => Err(unexpected(what, &other)),
    }
}

pub async fn post(app: &App, params: PostParams) -> Result<Done, RequestError> {
    let answer = app
        .service
        .send(Request::TicketCommentPost(TicketCommentPostParams {
            key: params.key,
            body: params.body,
            report: match (params.file, params.pasted) {
                (Some(path), _) => Some(ReportSource::Path(path.into())),
                (None, pasted) => pasted.map(ReportSource::Text),
            },
            tell_agent: params.tell_agent,
        }))
        .await?;
    posted("ticket.comment_post", answer)
}

pub async fn edit(app: &App, params: EditParams) -> Result<Done, RequestError> {
    let answer = app
        .service
        .send(Request::TicketCommentEdit(TicketCommentEditParams {
            key: params.key,
            n: params.n,
            body: params.body,
        }))
        .await?;
    posted("ticket.comment_edit", answer)
}

pub async fn delete(app: &App, params: DeleteParams) -> Result<Done, RequestError> {
    let answer = app
        .service
        .send(Request::TicketCommentDelete(TicketCommentParams {
            key: params.key,
            n: params.n,
        }))
        .await?;
    posted("ticket.comment_delete", answer)
}

/// The pasted chip's size, "· 48 KB", in the reports' own words.
fn pasted_size(params: SizeParams) -> String {
    vm::pasted_size(params.bytes)
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

    /// The thread through the page's requests: post (with a pasted report), edit, delete, the limit refused by the
    /// service, and the Ticket tab's count.
    #[tokio::test(flavor = "multi_thread")]
    async fn post_edit_and_delete_through_the_thread() {
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

        let empty = table(&h.app, "ticket_comments.thread", json!({ "key": key }))
            .await
            .unwrap();
        assert_eq!(empty["heading"], "Comments");
        assert_eq!(empty["box"]["limits"]["chars"], 2000);
        assert_eq!(empty["box"]["footer"], Value::Null, "no agent works");

        let pasted = "test export::limit_rows ... FAILED\n".repeat(1500);
        let done = table(
            &h.app,
            "ticket_comments.post",
            json!({ "key": key, "body": "Look at limit_rows first.", "pasted": pasted }),
        )
        .await
        .unwrap();
        assert_eq!(done["n"], 1);
        table(
            &h.app,
            "ticket_comments.post",
            json!({ "key": key, "body": "Also keep the column order the same as the table on screen." }),
        )
        .await
        .unwrap();
        // Typed text over the limit is refused by the service too, never turned into a report.
        let refused = table(
            &h.app,
            "ticket_comments.post",
            json!({ "key": key, "body": "x".repeat(2140) }),
        )
        .await
        .unwrap_err();
        assert_eq!(refused.refusal.as_deref(), Some("too_long"));
        assert!(refused.message.starts_with("Comment is 2,140 characters; the limit is 2,000."));

        let view = table(&h.app, "ticket_comments.thread", json!({ "key": key }))
            .await
            .unwrap();
        assert_eq!(view["heading"], "Comments \u{b7} 2");
        assert_eq!(view["comments"][0]["author"], "You");
        assert_eq!(view["comments"][0]["mine"], true);
        assert_eq!(view["comments"][0]["report"]["size"], "\u{b7} 51 KB");
        assert_eq!(view["comments"][1]["report"], Value::Null);

        table(
            &h.app,
            "ticket_comments.edit",
            json!({ "key": key, "n": 2, "body": "Also keep the column order, and the date in the shop's time zone." }),
        )
        .await
        .unwrap();
        table(&h.app, "ticket_comments.delete", json!({ "key": key, "n": 1 }))
            .await
            .unwrap();
        let view = table(&h.app, "ticket_comments.thread", json!({ "key": key }))
            .await
            .unwrap();
        assert_eq!(view["count"], 1);
        assert_eq!(view["comments"][0]["n"], 2);
        assert_eq!(view["comments"][0]["edited"], "\u{b7} edited");

        let size = table(&h.app, "ticket_comments.pasted_size", json!({ "bytes": 48 * 1024 }))
            .await
            .unwrap();
        assert_eq!(size, "\u{b7} 48 KB");
        h.service.stop_all().await;
    }
}

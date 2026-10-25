//! `files.*`: the ticket sheet's Files tab (FTB-01…FTB-05, SHT-06; d0017, d0043, d0087, d0109; `s25-07`…`s25-09`).
//! The tree folder by folder, the search by name, a file as it is and its diff, All changes (uncommitted edits
//! included), the line comments with the comment box, Unified / Split (kept in [`App`] while the app runs), the
//! review notes above All changes (how it reached Review, the final validation report, the agent's summary; VAL-12)
//! and the review bar under every tab while the check is yours. Everything is read from the ticket's worktree or
//! git through the service (`board/files.rs`), never from the user's checkout.

use otaq_core::board::comments::NewComment;
use otaq_core::board::files::FilesFrom;
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::domain::{Actor, Status};
use otaq_core::protocol::{
    CommentIdParams, CommentTextParams, CommentsParams, DiffFileParams, FilesPathParams,
    FilesSearchParams, KeyParams, Reply, Request,
};
use otaq_view::keymap::KeyAction;
use otaq_view::view_models::diff_page::{self as page, DiffSide};
use otaq_view::view_models::files_tab as vm;
use otaq_view::view_models::validation_report::{self, Review};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "files.view" => reply(&view(app, parse(params)?).await?),
        "files.tree" => reply(&tree(app, parse(params)?).await?),
        "files.search" => reply(&search(app, parse(params)?).await?),
        "files.changes" => reply(&changes(app, parse(params)?).await?),
        "files.read" => reply(&read(app, parse(params)?).await?),
        "files.diff" => reply(&diff(app, parse(params)?).await?),
        "files.review" => reply(&review(app, parse(params)?).await?),
        "files.comments" => reply(&comments(app, parse(params)?).await?),
        "files.bar" => reply(&bar(app, parse(params)?).await?),
        "files.mode" => reply(&mode(app, parse(params)?)),
        "files.comment_add" => reply(&comment_add(app, parse(params)?).await?),
        "files.comment_reply" => reply(&comment_text(app, parse(params)?, true).await?),
        "files.comment_edit" => reply(&comment_text(app, parse(params)?, false).await?),
        "files.comment_delete" => reply(&comment_delete(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {what}: {other:?}"))
}

async fn ticket(app: &App, key: &str) -> Result<Ticket, RequestError> {
    match app
        .service
        .send(Request::TicketGet(KeyParams {
            key: key.to_owned(),
        }))
        .await?
    {
        Reply::Ticket(t) => Ok(*t),
        other => Err(unexpected("ticket.get", &other)),
    }
}

/// The stage Send back returns `ticket` to ("Product spec"), from its project's flow and its skips (d0131), and
/// whether that flow isn't the default's shape (Approve is then the human check's Pass, FLW-14).
async fn back_to(app: &App, ticket: &Ticket) -> (String, bool) {
    let project = ticket.key.project.to_string();
    let (flow, agents) = tokio::join!(
        super::board::flow(app, &project),
        super::editor::agent_facts(app, super::editor::of_ticket(&ticket.key.to_string())),
    );
    let to = super::review::send_back_stage(ticket, &flow, agents.as_ref()).unwrap_or_else(|| "Dev".to_owned());
    (to, !otaq_view::view_models::flow_columns::keeps_default_look(&flow))
}

/// The open ticket.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesParams")]
pub struct Params {
    pub key: String,
}

/// What `files.view` answers: the tab, or why there are no files to show.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FilesLoad")]
pub struct Load {
    pub view: Option<vm::TabView>,
    pub problem: Option<String>,
}

/// The tab's frame with the top of the tree.
pub async fn view(app: &App, params: Params) -> Result<Load, RequestError> {
    let (ticket, top, keys) = tokio::join!(
        ticket(app, &params.key),
        app.service.send(Request::FilesTree(FilesPathParams {
            key: params.key.clone(),
            path: String::new(),
        })),
        super::keymap::labels(app),
    );
    let ticket = ticket?;
    Ok(match top {
        Ok(Reply::FilesTree(tree)) => {
            let view = vm::tab_view(
                &ticket,
                &tree.source,
                &tree.entries,
                tree.changed,
                keys.label(KeyAction::GoToFile),
            );
            Load {
                view: Some(view),
                problem: None,
            }
        }
        Ok(other) => return Err(unexpected("files.tree", &other)),
        Err(e) => Load {
            view: None,
            problem: Some(e.message),
        },
    })
}

/// A folder of the tree, opened at `depth`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesTreeParams")]
pub struct TreeParams {
    pub key: String,
    pub path: String,
    pub depth: u32,
}

pub async fn tree(app: &App, params: TreeParams) -> Result<Vec<vm::TreeRow>, RequestError> {
    match app
        .service
        .send(Request::FilesTree(FilesPathParams {
            key: params.key,
            path: params.path,
        }))
        .await?
    {
        Reply::FilesTree(tree) => Ok(vm::tree_rows(&tree.entries, params.depth)),
        other => Err(unexpected("files.tree", &other)),
    }
}

/// What is typed in the search box.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesSearchParams")]
pub struct SearchParams {
    pub key: String,
    pub query: String,
}

pub async fn search(app: &App, params: SearchParams) -> Result<vm::SearchView, RequestError> {
    match app
        .service
        .send(Request::FilesSearch(FilesSearchParams {
            key: params.key,
            query: params.query.clone(),
        }))
        .await?
    {
        Reply::FilesSearch(found) => Ok(vm::search_view(
            &params.query,
            &found.matches,
            found.total,
        )),
        other => Err(unexpected("files.search", &other)),
    }
}

/// All changes: the changed files without their lines (each comes from `files.diff` as it scrolls into view).
pub async fn changes(app: &App, params: Params) -> Result<vm::ChangesView, RequestError> {
    let (ticket, changes) = tokio::join!(
        ticket(app, &params.key),
        app.service.send(Request::FilesChanges(KeyParams {
            key: params.key.clone(),
        })),
    );
    let ticket = ticket?;
    match changes? {
        Reply::FilesChanges(changes) => {
            let memory = app.review.lock().unwrap_or_else(|e| e.into_inner());
            Ok(vm::changes_view(&ticket, &changes, &memory))
        }
        other => Err(unexpected("files.changes", &other)),
    }
}

/// A file of the tree.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesFileParams")]
pub struct FileParams {
    pub key: String,
    pub path: String,
    /// `files.diff` only: the commit the review saw, to show only what changed after it (REV-06).
    #[ts(optional)]
    pub since: Option<String>,
}

/// A file as it is, highlighted off the runtime's threads (a long file takes a while); a deleted one says so.
pub async fn read(app: &App, params: FileParams) -> Result<vm::FilePage, RequestError> {
    let request = Request::FilesRead(FilesPathParams {
        key: params.key.clone(),
        path: params.path.clone(),
    });
    match app.service.send(request).await {
        Ok(Reply::FileContent(content)) => {
            let memory = app.review.lock().unwrap_or_else(|e| e.into_inner()).clone();
            tokio::task::spawn_blocking(move || vm::file_page(&content, &memory))
                .await
                .map_err(|e| RequestError::plain(e.to_string()))
        }
        Ok(other) => Err(unexpected("files.read", &other)),
        // Deleted: not on disk, but its diff shows what it was.
        Err(e) if e.not_found => {
            let source = match app
                .service
                .send(Request::FilesTree(FilesPathParams {
                    key: params.key,
                    path: String::new(),
                        }))
                .await?
            {
                Reply::FilesTree(tree) => tree.source,
                other => return Err(unexpected("files.tree", &other)),
            };
            if source.from == FilesFrom::Base {
                return Err(e);
            }
            let memory = app.review.lock().unwrap_or_else(|e| e.into_inner()).clone();
            Ok(vm::deleted_page(&source, &params.path, None, &memory))
        }
        Err(e) => Err(e),
    }
}

/// A file's hunks against the base (uncommitted edits included), or since the reviewed commit, highlighted.
pub async fn diff(app: &App, params: FileParams) -> Result<page::FileView, RequestError> {
    let request = match params.since {
        Some(since) => Request::DiffFile(DiffFileParams {
            key: params.key,
            path: params.path,
            since: Some(since),
        }),
        None => Request::FilesDiff(FilesPathParams {
            key: params.key,
            path: params.path,
        }),
    };
    let diff = match app.service.send(request).await? {
        Reply::FileDiff(diff) => diff,
        other => return Err(unexpected("files.diff", &other)),
    };
    tokio::task::spawn_blocking(move || page::file_view(&diff))
        .await
        .map_err(|e| RequestError::plain(e.to_string()))
}

/// What sits above All changes when the ticket reached Review: the Review entry ("Validated · round 2") with the
/// final report or the last findings (VAL-12), and the agent's summary.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FilesReviewNotes")]
pub struct ReviewNotes {
    pub review: Option<Review>,
    pub summary: Option<vm::AgentSummary>,
}

pub async fn review(app: &App, params: Params) -> Result<ReviewNotes, RequestError> {
    let ticket = match ticket(app, &params.key).await {
        Ok(t) => t,
        Err(e) if e.not_found => {
            return Ok(ReviewNotes {
                review: None,
                summary: None,
            });
        }
        Err(e) => return Err(e),
    };
    let events = match app
        .service
        .send(Request::TicketHistory(KeyParams {
            key: params.key.clone(),
        }))
        .await
    {
        Ok(Reply::History(events)) => events,
        _ => Vec::new(),
    };
    // The agent's last word when its stage finished (`prime`'s review summary).
    let summary = events
        .iter()
        .rev()
        .find(|e| matches!(e.kind, EventKind::ReviewSubmitted | EventKind::StageFinished))
        .and_then(|e| e.payload.get("summary")?.as_str().map(str::to_owned));
    let review = match ticket.flags.review_entry {
        Some(flag) => {
            let records = super::sections::validations(app, &params.key).await;
            let rounds = super::sheet::check_rounds(app, &ticket).await;
            // When the user moved it to Review (for "Moved by you at 11:12").
            let moved_at = events
                .iter()
                .filter(|e| e.kind == EventKind::Moved && matches!(e.actor, Actor::User(_)))
                .filter(|e| {
                    e.payload
                        .get("to")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<Status>().ok())
                        == Some(Status::WaitingForYou)
                })
                .map(|e| e.at)
                .max();
            validation_report::review(Some(flag), &records, rounds, moved_at)
        }
        None => None,
    };
    Ok(ReviewNotes {
        review,
        summary: vm::agent_summary(summary.as_deref()),
    })
}

/// The ticket's line comments; they can be added only while it waits in your check.
pub async fn comments(app: &App, params: Params) -> Result<vm::CommentsView, RequestError> {
    let ticket = ticket(app, &params.key).await?;
    let (list, (to, _)) = tokio::join!(
        app.service.send(Request::ReviewComments(CommentsParams {
            key: params.key.clone(),
            round: None,
        })),
        back_to(app, &ticket),
    );
    let list = match list? {
        Reply::ReviewComments(list) => list,
        other => return Err(unexpected("review.comments", &other)),
    };
    Ok(vm::comments_view(
        &params.key,
        &list.comments,
        vm::can_comment(&ticket),
        &to,
    ))
}

/// The review bar's question: is a new comment being written in the tab now.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesBarParams")]
pub struct BarParams {
    pub key: String,
    #[serde(default)]
    pub writing: bool,
}

/// The review bar (SHT-06), or `null` when the ticket doesn't wait in your check.
pub async fn bar(app: &App, params: BarParams) -> Result<Option<vm::ReviewBar>, RequestError> {
    let ticket = match ticket(app, &params.key).await {
        Ok(t) => t,
        Err(e) if e.not_found => return Ok(None),
        Err(e) => return Err(e),
    };
    if !vm::waits_for_you(&ticket) {
        return Ok(None);
    }
    let (list, (to, custom), keys) = tokio::join!(
        app.service.send(Request::ReviewComments(CommentsParams {
            key: params.key.clone(),
            round: None,
        })),
        back_to(app, &ticket),
        super::keymap::labels(app),
    );
    let pending = match list? {
        Reply::ReviewComments(list) => list.pending,
        other => return Err(unexpected("review.comments", &other)),
    };
    Ok(vm::review_bar(
        &ticket,
        pending,
        params.writing && vm::can_comment(&ticket),
        &to,
        keys.label(KeyAction::Approve),
        custom,
    ))
}

/// Unified or Split, for every ticket.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesModeParams")]
pub struct ModeParams {
    pub side_by_side: bool,
}

pub fn mode(app: &App, params: ModeParams) {
    app.review
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .side_by_side = params.side_by_side;
}

/// A new comment on a line.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesCommentAdd")]
pub struct AddParams {
    pub key: String,
    pub file: String,
    pub line: u32,
    pub side: DiffSide,
    pub body: String,
}

fn body_of(text: &str) -> Result<String, RequestError> {
    let body = text.trim();
    if body.is_empty() {
        return Err(RequestError::plain("Write the comment first."));
    }
    Ok(body.to_owned())
}

/// Adds a comment; the list comes back through the ticket's change event.
pub async fn comment_add(app: &App, params: AddParams) -> Result<(), RequestError> {
    let body = body_of(&params.body)?;
    match app
        .service
        .send(Request::ReviewCommentAdd(NewComment {
            key: params.key,
            file: params.file,
            line: params.line,
            side: params.side.into(),
            body,
        }))
        .await?
    {
        Reply::ReviewComment(_) => Ok(()),
        other => Err(unexpected("review.comment_add", &other)),
    }
}

/// A reply to, or the new text of, a comment.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesCommentText")]
pub struct TextParams {
    pub id: String,
    pub body: String,
}

pub async fn comment_text(app: &App, params: TextParams, reply: bool) -> Result<(), RequestError> {
    let p = CommentTextParams {
        id: params.id,
        body: body_of(&params.body)?,
    };
    let request = if reply {
        Request::ReviewCommentReply(p)
    } else {
        Request::ReviewCommentEdit(p)
    };
    match app.service.send(request).await? {
        Reply::ReviewComment(_) => Ok(()),
        other => Err(unexpected("review.comment", &other)),
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesCommentId")]
pub struct IdParams {
    pub id: String,
}

/// Deletes a pending comment.
pub async fn comment_delete(app: &App, params: IdParams) -> Result<(), RequestError> {
    app.service
        .send(Request::ReviewCommentDelete(CommentIdParams { id: params.id }))
        .await
        .map(|_| ())
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

    /// A Backlog ticket shows its base branch, labelled so; it takes no comments and has no bar; Split is kept.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_ticket_that_never_started_shows_its_base_branch() {
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
                title: "Checkout API".into(),
                body: String::new(),
                plan: String::new(),
                priority: None,
                base_branch: None,
                external_id: None,
                first_comment: None,
            }))
            .await
            .unwrap()
        else {
            panic!("no ticket");
        };
        let key = created.key.to_string();

        let load = table(&h.app, "files.view", json!({ "key": key })).await.unwrap();
        assert_eq!(load["problem"], Value::Null, "{load}");
        let view = &load["view"];
        assert_eq!(view["source"]["kind"], "base");
        assert_eq!(view["filters"][1], "Changed \u{b7} 0");
        assert_eq!(view["opens_on"], "all");
        assert!(view["rows"].as_array().is_some_and(|r| !r.is_empty()), "{view}");

        let changes = table(&h.app, "files.changes", json!({ "key": key }))
            .await
            .unwrap();
        assert_eq!(changes["files"], json!([]));
        assert!(changes["empty"]["text"].as_str().unwrap().contains("hasn\u{2019}t started"));

        let bar = table(&h.app, "files.bar", json!({ "key": key })).await.unwrap();
        assert_eq!(bar, Value::Null, "not in your check");
        let notes = table(&h.app, "files.review", json!({ "key": key })).await.unwrap();
        assert_eq!(notes, json!({ "review": null, "summary": null }));

        let comments = table(&h.app, "files.comments", json!({ "key": key }))
            .await
            .unwrap();
        assert_eq!(comments["can_comment"], false, "a Backlog ticket takes no comments");
        let refused = table(
            &h.app,
            "files.comment_add",
            json!({ "key": key, "file": "a.ts", "line": 1, "side": "new", "body": "x" }),
        )
        .await
        .unwrap_err();
        assert!(!refused.message.is_empty());
        let empty = table(
            &h.app,
            "files.comment_add",
            json!({ "key": key, "file": "a.ts", "line": 1, "side": "new", "body": "  " }),
        )
        .await
        .unwrap_err();
        assert_eq!(empty.message, "Write the comment first.");

        table(&h.app, "files.mode", json!({ "side_by_side": true }))
            .await
            .unwrap();
        assert!(h.app.review.lock().unwrap().side_by_side);
        let changes = table(&h.app, "files.changes", json!({ "key": key }))
            .await
            .unwrap();
        assert_eq!(changes["side_by_side"], true);
        h.service.stop_all().await;
    }
}

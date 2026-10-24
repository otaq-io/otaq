//! `pr.*`: Open PR, Push to PR and the PR open box (REV-10, REV-11, REV-12, REV-13, REV-14, EPC-08, CLS-02, UI-07;
//! d0041, d0050, d0051, d0059; *07 → Open PR · …, Approve · Push to PR #481, Push to PR · confirmation, Open PR · epic
//! SHOP-40*; *05 → SHOP-26 · Approved, PR open*). The dialog's state is a `pr_dialog::PrDialog` per ticket, kept in
//! the app between requests (the proposal fills the fields once; the branch name's check is debounced by edit
//! number); the page sends each edit and draws `pr_page`'s view. Nothing is pushed before `pr.confirm`.

use std::time::{SystemTime, UNIX_EPOCH};

use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::pr::flow::PrFlowView;
use otaq_core::pr::provider::PrProvider;
use otaq_core::pr::service::PrOutcome;
use otaq_core::protocol::{
    BranchRenameParams, KeyParams, OpenPrParams, PrConfirmParams, ProjectParams, Reply, Request,
};
use otaq_core::review::LocalMergeOutcome;
use otaq_view::view_models::pr_dialog::{PrDialog, Stage};
use otaq_view::view_models::pr_page::{self as page, DialogView, OpenBox, PushIntro};
use otaq_view::view_models::review_page::{self, ChangedView, MergeResult};
use otaq_view::view_models::terminal_strip::{Click, UnsentPrompt};
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::sheet::Unsent;
use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "pr.open" => reply(&open(app, parse(params)?).await?),
        "pr.start" => reply(&start(app, parse(params)?).await?),
        "pr.view" => reply(&view(app, parse(params)?).await?),
        "pr.edit" => reply(&edit(app, parse(params)?)?),
        "pr.check" => reply(&check(app, parse(params)?).await?),
        "pr.confirm" => reply(&confirm(app, parse(params)?).await?),
        "pr.retry" => reply(&retry(app, parse(params)?).await?),
        "pr.cancel" => reply(&cancel(app, parse(params)?).await?),
        "pr.open_box" => reply(&open_box(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {what}: {other:?}"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
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

async fn provider(app: &App, ticket: &Ticket) -> Option<PrProvider> {
    match app
        .service
        .send(Request::ProjectPrProvider(ProjectParams {
            project: ticket.key.project.to_string(),
        }))
        .await
    {
        Ok(Reply::PrProvider(p)) => Some(p),
        _ => None,
    }
}

async fn flow(app: &App, key: &str) -> Result<PrFlowView, RequestError> {
    match app
        .service
        .send(Request::TicketPrFlow(KeyParams {
            key: key.to_owned(),
        }))
        .await?
    {
        Reply::PrFlow(view) => Ok(*view),
        other => Err(unexpected("ticket.pr_flow", &other)),
    }
}

/// Runs `f` on the ticket's dialog, if it is open.
fn with<T>(app: &App, key: &str, f: impl FnOnce(&mut PrDialog) -> T) -> Option<T> {
    let mut dialogs = app.pr_dialogs.lock().unwrap_or_else(|e| e.into_inner());
    dialogs.get_mut(key).map(f)
}

fn current(app: &App, key: &str) -> Option<DialogView> {
    with(app, key, |d| page::dialog_view(d))
}

fn forget(app: &App, key: &str) -> Option<PrDialog> {
    app.pr_dialogs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(key)
}

/// What the dialog shows next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(rename = "PrAnswer")]
pub enum Answer {
    /// The dialog at its stage; `intro` first on Push to PR from the review flow (*Approve · Push to PR #481*).
    Dialog {
        view: Box<DialogView>,
        intro: Option<PushIntro>,
    },
    /// The ticket's terminal has unsent text: Clear it and send, or keep it (d0048). Nothing was changed.
    Ask { unsent: Unsent },
    /// *Uncommitted files · refused* (REV-08), drawn by the review dialogs.
    Uncommitted { result: MergeResult },
    /// *Approve · changed since review* (REV-06): Approve anyway asks again with `approve_anyway`.
    Changed { changed: ChangedView },
    /// The dialog is gone: the PR was opened or pushed (its toast shows), or it was cancelled.
    Closed,
}

fn dialog(app: &App, key: &str) -> Answer {
    match current(app, key) {
        Some(view) => Answer::Dialog {
            view: Box::new(view),
            intro: None,
        },
        None => Answer::Closed,
    }
}

/// Approve ▾ → Open PR / Push to PR #N. `approve_anyway`: the user already chose Approve anyway after "changed since
/// you reviewed it" (REV-06), so the service doesn't ask again. `intro`: Push to PR shows what goes to the PR before
/// its skill runs (the review flow; the board's choice already was that click).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "PrOpenParams")]
pub struct OpenParams {
    pub key: String,
    #[serde(default)]
    pub approve_anyway: bool,
    #[serde(default)]
    pub intro: bool,
}

pub async fn open(app: &App, params: OpenParams) -> Result<Answer, RequestError> {
    let ticket = ticket(app, &params.key).await?;
    let provider = provider(app, &ticket).await;
    let skill = provider
        .as_ref()
        .map(|p| p.open_pr_skill.clone())
        .unwrap_or_default();
    let model = PrDialog::opening(&ticket, &skill, provider.as_ref());
    let not_logged_in = matches!(model.stage, Stage::NotLoggedIn { .. });
    app.pr_dialogs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(params.key.clone(), model);
    if not_logged_in {
        return Ok(dialog(app, &params.key));
    }
    if params.intro && ticket.flags.pr_number.is_some() {
        let flow = flow(app, &params.key).await?;
        if let Some(intro) = page::push_intro(&ticket, &flow) {
            with(app, &params.key, |d| d.update(&flow));
            let view = current(app, &params.key).ok_or_else(|| RequestError::plain("closed"))?;
            return Ok(Answer::Dialog {
                view: Box::new(view),
                intro: Some(intro),
            });
        }
    }
    start(
        app,
        StartParams {
            key: params.key,
            approve_anyway: params.approve_anyway,
            clear: false,
        },
    )
    .await
}

/// Runs the Open PR or Push skill (the intro's Push to PR, or "Clear it and send").
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "PrStartParams")]
pub struct StartParams {
    pub key: String,
    #[serde(default)]
    pub approve_anyway: bool,
    #[serde(default)]
    pub clear: bool,
}

pub async fn start(app: &App, params: StartParams) -> Result<Answer, RequestError> {
    let sent = app
        .service
        .send(Request::TicketOpenPr(OpenPrParams {
            key: params.key.clone(),
            approve_anyway: params.approve_anyway,
            clear: params.clear,
        }))
        .await;
    answered(app, &params.key, sent, Click::OpenPr).await
}

/// An answer to Open PR, confirm or Retry, as the dialog goes on.
async fn answered(
    app: &App,
    key: &str,
    sent: Result<Reply, RequestError>,
    click: Click,
) -> Result<Answer, RequestError> {
    match sent {
        Ok(Reply::PrOutcome(PrOutcome::Started { .. })) => {
            if click == Click::PrConfirm {
                with(app, key, |d| d.stage = Stage::Pushing);
            }
            view(app, Params { key: key.to_owned() }).await
        }
        Ok(Reply::PrOutcome(PrOutcome::NeedsConfirmation {
            ticket,
            unclearable_by,
        })) => {
            let sending = with(app, key, |d| d.skill.clone()).unwrap_or_default();
            Ok(Answer::Ask {
                unsent: Unsent::of(
                    &UnsentPrompt::for_click(ticket, None, sending, click)
                        .unclearable_by(unclearable_by),
                ),
            })
        }
        Ok(Reply::PrOutcome(PrOutcome::RefusedUncommitted { files, message })) => {
            forget(app, key);
            let t = ticket(app, key).await?;
            let outcome = LocalMergeOutcome::RefusedUncommitted { files, message };
            Ok(Answer::Uncommitted {
                result: review_page::merge_result(&t, "", &outcome, &[]),
            })
        }
        Ok(Reply::PrOutcome(PrOutcome::Changed { check })) => {
            let t = ticket(app, key).await?;
            match review_page::changed_view(&t.key, &check) {
                Some(changed) => Ok(Answer::Changed { changed }),
                None => Ok(dialog(app, key)),
            }
        }
        Ok(other) => Err(unexpected("ticket.open_pr", &other)),
        Err(e) => {
            app.notices.toast(NewToast::new(
                ToastLevel::Error,
                format!("Open PR for {key} didn\u{2019}t go through"),
                Some(e.message.clone()),
            ));
            if click == Click::OpenPr {
                forget(app, key);
            }
            Err(e)
        }
    }
}

/// The open dialog's ticket.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "PrParams")]
pub struct Params {
    pub key: String,
}

/// The flow as the service has it now (the page asks every second while the skill works). Once the push brought the
/// ticket to Approved · PR open, the dialog closes with the success toast.
pub async fn view(app: &App, params: Params) -> Result<Answer, RequestError> {
    let key = params.key;
    let Some(pushing) = with(app, &key, |d| d.stage == Stage::Pushing) else {
        return Ok(Answer::Closed);
    };
    if pushing {
        let t = ticket(app, &key).await?;
        if t.status == Status::PrOpen && t.flags.pr_number.is_some() {
            if let Some(d) = forget(app, &key)
                && let Some(toast) = page::success_toast(&t, d.kind, &d.target)
            {
                app.notices
                    .toast(NewToast::new(ToastLevel::Success, toast.title, toast.text));
            }
            return Ok(Answer::Closed);
        }
    }
    let flow = flow(app, &key).await?;
    with(app, &key, |d| {
        if !flow.skill.is_empty() {
            d.skill = flow.skill.clone();
        }
        let was = d.stage.clone();
        d.update(&flow);
        // The proposal arrived: its branch name is checked like an edit.
        if was != Stage::Proposed && d.stage == Stage::Proposed && !d.branch_locked {
            d.debounce.edit();
        }
    });
    Ok(dialog(app, &key))
}

/// One field as the user typed it: `branch`, `title`, `description`, or `commit` with its `index`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "PrEditParams")]
pub struct EditParams {
    pub key: String,
    pub field: String,
    #[serde(default)]
    pub index: u32,
    pub value: String,
}

pub fn edit(app: &App, params: EditParams) -> Result<Answer, RequestError> {
    let known = with(app, &params.key, |d| match params.field.as_str() {
        "branch" => {
            if d.branch != params.value {
                d.edit_branch(&params.value);
            }
            true
        }
        "title" => {
            d.title = params.value.clone();
            true
        }
        "description" => {
            d.description = params.value.clone();
            true
        }
        "commit" => match d.commits.get_mut(params.index as usize) {
            Some(c) => {
                c.message = params.value.clone();
                true
            }
            None => false,
        },
        _ => false,
    });
    match known {
        Some(false) => Err(RequestError::plain(format!(
            "no such field: {}",
            params.field
        ))),
        _ => Ok(dialog(app, &params.key)),
    }
}

/// The live branch-name check, asked once the name stayed the same for `check_after_ms`; a newer edit wins.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "PrCheckParams")]
pub struct CheckParams {
    pub key: String,
    pub check: u64,
}

pub async fn check(app: &App, params: CheckParams) -> Result<Answer, RequestError> {
    let Some(name) = with(app, &params.key, |d| {
        d.debounce.is_current(params.check).then(|| d.branch.clone())
    }) else {
        return Ok(Answer::Closed);
    };
    if let Some(name) = name
        && let Ok(Reply::BranchCheck(answer)) = app
            .service
            .send(Request::TicketPrBranchCheck(BranchRenameParams {
                key: params.key.clone(),
                name,
            }))
            .await
    {
        with(app, &params.key, |d| d.checked(params.check, &answer));
    }
    Ok(dialog(app, &params.key))
}

/// **Push & open PR** / **Push to PR #N** with the user's edits; `clear` is "Clear it and send".
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "PrConfirmParams")]
pub struct ConfirmParams {
    pub key: String,
    #[serde(default)]
    pub clear: bool,
}

pub async fn confirm(app: &App, params: ConfirmParams) -> Result<Answer, RequestError> {
    let proposal = with(app, &params.key, |d| d.can_confirm().then(|| d.proposal())).flatten();
    let Some(proposal) = proposal else {
        return Ok(dialog(app, &params.key));
    };
    let sent = app
        .service
        .send(Request::TicketPrConfirm(PrConfirmParams {
            key: params.key.clone(),
            proposal,
            clear: params.clear,
        }))
        .await;
    answered(app, &params.key, sent, Click::PrConfirm).await
}

/// **Retry**: after "not logged in" the login is read again (and the skill starts once it is); after the skill
/// reported a problem, the skill runs again.
pub async fn retry(app: &App, params: StartParams) -> Result<Answer, RequestError> {
    let Some(not_logged_in) = with(app, &params.key, |d| {
        matches!(d.stage, Stage::NotLoggedIn { .. })
    }) else {
        return Ok(Answer::Closed);
    };
    if not_logged_in {
        return open(
            app,
            OpenParams {
                key: params.key,
                approve_anyway: params.approve_anyway,
                intro: false,
            },
        )
        .await;
    }
    let sent = app
        .service
        .send(Request::TicketPrRetry(OpenPrParams {
            key: params.key.clone(),
            approve_anyway: params.approve_anyway,
            clear: params.clear,
        }))
        .await;
    answered(app, &params.key, sent, Click::PrRetry).await
}

/// **Cancel** before pushing: the proposal is discarded; nothing was pushed.
pub async fn cancel(app: &App, params: Params) -> Result<Answer, RequestError> {
    if let Some(d) = forget(app, &params.key)
        && !matches!(d.stage, Stage::NotLoggedIn { .. })
    {
        let _ = app
            .service
            .send(Request::TicketPrCancel(KeyParams { key: params.key }))
            .await;
    }
    Ok(Answer::Closed)
}

/// The sheet's PR box while the PR is open (*05 → SHOP-26 · Approved, PR open*).
pub async fn open_box(app: &App, params: Params) -> Result<Option<OpenBox>, RequestError> {
    let t = ticket(app, &params.key).await?;
    Ok(page::open_box(&t, now_ms()))
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

    #[tokio::test(flavor = "multi_thread")]
    async fn open_pr_refuses_a_ticket_not_in_review_and_leaves_no_dialog() {
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
                kind: Default::default(),
                title: "Checkout API".into(),
                body: String::new(),
                priority: None,
                epic: None,
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

        let refused = table(&h.app, "pr.open", json!({ "key": key })).await;
        assert!(refused.is_err(), "{refused:?}");
        assert!(h.app.pr_dialogs.lock().unwrap().is_empty());
        // Every other request answers "closed" with no dialog open; nothing reaches the service.
        for method in ["pr.view", "pr.cancel"] {
            assert_eq!(
                table(&h.app, method, json!({ "key": key })).await.unwrap(),
                json!({ "outcome": "closed" }),
                "{method}"
            );
        }
        assert_eq!(
            table(&h.app, "pr.check", json!({ "key": key, "check": 1 }))
                .await
                .unwrap(),
            json!({ "outcome": "closed" })
        );
        assert_eq!(
            table(&h.app, "pr.open_box", json!({ "key": key }))
                .await
                .unwrap(),
            Value::Null,
            "no PR box outside Approved · PR open"
        );
        h.service.stop_all().await;
    }
}

//! `overview.*`: the sheet's Overview (SHT-03, RPT-02; `s25-03`…`s25-05`). Its words, the description's editing words
//! and the Attachments column come from `otaq-view`'s `overview.rs`; the reports are the ticket's comments' (the
//! service's `ticket.comments`), so the column follows `ticket_changed` like the thread. `overview.file` reads a file
//! picked with Add or dropped on Overview for the comment box's chip: its name and size, or why it can't be a report
//! (missing, not a file, over 20 MB: core's own refusals, so the box says what posting would say).
//! `overview.usage_limit` is USG-07's "Mark as usage limit" on an Idle ticket (the Inbox's row action before T-0459).
//! `overview.manual_check_tick` ticks an item of "Manual checks for you" (d0125; the service's
//! `ticket.manual_check_tick`).

use std::path::PathBuf;

use otaq_core::board::reports::{AttachmentKind, open_report, size_label};
use otaq_core::domain::Status;
use otaq_core::protocol::{
    KeyParams, ManualCheckTickParams, ProjectParams, Reply, Request, SessionListParams,
};
use otaq_view::keymap::Platform;
use otaq_view::view_models::overview::{self as vm, OverviewView};
use otaq_view::view_models::plan;
use otaq_view::view_models::planning;
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
    plan::PlanSection::export_all(cfg)?;
    planning::Did::export_all(cfg)?;
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

/// The comment box's chip for a file: "perf-notes.md", "2 KB"; posted as one of the comment's attachments by its
/// path. An image's chip shows a thumbnail (tuq 002), read with the door's `box_image`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OverviewFileChip")]
pub struct FileChip {
    pub path: String,
    pub name: String,
    pub size: String,
    /// An image by its name ([`AttachmentKind::is_image_name`]), as the posted attachment's kind will be.
    pub image: bool,
}

impl FileChip {
    pub fn new(path: String, name: String, bytes: u64) -> Self {
        Self {
            image: AttachmentKind::is_image_name(&name),
            size: size_label(bytes),
            path,
            name,
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

pub async fn view(app: &App, params: ViewParams) -> Result<OverviewView, RequestError> {
    let k = || KeyParams {
        key: params.key.clone(),
    };
    let (ticket, checks) = tokio::join!(
        app.service.send(Request::TicketGet(k())),
        app.service.send(Request::TicketManualChecks(k())),
    );
    let ticket = match ticket? {
        Reply::Ticket(t) => *t,
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
    let k = || KeyParams {
        key: params.key.clone(),
    };
    let (history, flow) = tokio::join!(
        app.service.send(Request::TicketHistory(k())),
        app.service.send(Request::FlowCurrent(ProjectParams {
            project: ticket.key.project.to_string(),
        })),
    );
    let flow = match flow {
        Ok(Reply::FlowVersion(version)) => version.file().ok().map(|f| f.flow),
        _ => None,
    };
    // The PLAN section (d0133): who wrote it comes from History; unread, the header names the last writer only.
    let history = match history {
        Ok(Reply::History(events)) => events,
        _ => Vec::new(),
    };
    let mut plan = plan::section(
        &ticket,
        &plan::Facts {
            stage: flow.as_ref().and_then(|f| plan::stage_in(&ticket, f)),
            events: &history,
            now_ms: now_ms(),
        },
        Platform::current(),
    );
    // Each piece's status beside the Plan once the ticket split its work (d0133, `s30-03`).
    if ticket.started_at.is_some() {
        let helpers = super::board::helpers(app, ticket.key.project.as_str()).await;
        plan.statuses = otaq_view::view_models::pieces::plan_statuses(
            &otaq_view::view_models::pieces::Facts::new(&ticket, &helpers),
        );
    }
    let mut view = vm::with_plan(
        vm::view(
            &params.key,
            ticket.status,
            &comments,
            checks.as_ref(),
            tool_avatar::shown(),
            Platform::current(),
        ),
        plan,
    );
    // The ticket's own agent for a stage, as its card says it, from the moment it is set; unread, Overview goes on
    // without the line.
    if let Ok(Reply::BoardAgents(agents)) = app
        .service
        .send(Request::BoardAgents(ProjectParams {
            project: ticket.key.project.to_string(),
        }))
        .await
    {
        let own = agents
            .cards
            .iter()
            .find(|c| c.key == ticket.key)
            .and_then(|c| c.own_agent.as_ref());
        view = vm::with_own_agent(view, &ticket, own);
    }
    // The question you answered here stays, quietly, while the agent works on it in this stage's run.
    if matches!(
        ticket.status,
        Status::Working | Status::WorkingInBackground | Status::Idle
    ) {
        let (answers, agents) = tokio::join!(
            app.service.send(Request::TicketHumanAnswers(k())),
            super::editor::agent_facts(app, super::editor::of_ticket(&params.key)),
        );
        if let Ok(Reply::HumanAnswers(answers)) = answers {
            let stage = ticket.place.stage();
            let run_started = agents.and_then(|a| {
                a.runs
                    .iter()
                    .rev()
                    .find(|r| r.ended_at.is_none() && Some(r.stage.as_str()) == stage)
                    .map(|r| r.started_at)
            });
            view = vm::with_answered(view, vm::answered(ticket.status, &answers, run_started));
        }
    }
    // A check of planning: what each planning stage did, from its comment and its commits (d0129, `s30-02`).
    if let Some(p) = super::files::planning_facts(app, &ticket).await
        && let Some(did) = planning::did(
            &ticket,
            &p.flow,
            &p.check,
            &p.runs,
            &p.files,
            plan::stage_writer(&ticket, &history).as_deref(),
        )
    {
        view = vm::with_planning(view, did);
    }
    // A human check right after a Check stage: each check agent's verdict (d0130); unread, the review goes on without.
    let Some(check) = vm::passed_check(&ticket) else {
        return Ok(view);
    };
    let (records, sessions) = tokio::join!(
        app.service.send(Request::TicketValidations(k())),
        app.service.send(Request::SessionList(SessionListParams {
            key: Some(params.key.clone()),
        })),
    );
    let (Ok(Reply::Validations(records)), Ok(Reply::Sessions(sessions))) = (records, sessions) else {
        return Ok(view);
    };
    let rounds = flow
        .as_ref()
        .and_then(|f| {
            let stage = f.stages.iter().find(|s| s.name == check)?;
            Some(otaq_core::flow::engine::rounds_of(stage, 0))
        })
        .unwrap_or(otaq_core::flow::check::DEFAULT_VALIDATION_ROUNDS);
    Ok(vm::with_before_you(
        view,
        check,
        &records,
        &sessions,
        rounds,
        tool_avatar::shown(),
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

/// The chip for the file at `path`, or core's refusal as the error ("perf.mov is 212 MB; the limit is 20 MB…").
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
    let name = path
        .file_name()
        .map_or_else(|| params.path.clone(), |n| n.to_string_lossy().into_owned());
    Ok(FileChip::new(params.path, name, bytes))
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::board::tickets::NewTicketInput;
    use otaq_core::git::testing::TempRepos;
    use otaq_core::protocol::SetChoiceParams;
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    /// A ticket's own agent shows from the moment it is set: a Backlog ticket with Dev set to Checker says
    /// "Dev: Checker instead of Developer" on its card and on its Overview, before Dev starts.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_backlog_ticket_shows_its_own_agent_on_its_card_and_overview() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let send = |r| h.app.service.send(r);
        let Ok(Reply::ProjectAdded { project, .. }) = send(Request::ProjectAdd(AddProject {
            path: repos.repo("shop-web", 1).display().to_string(),
            key: None,
            base_branch: None,
        }))
        .await
        else {
            panic!("no project");
        };
        let Ok(Reply::Ticket(t)) = send(Request::TicketCreate(NewTicketInput {
            project: project.key.clone(),
            title: "Fix the word tiny in README".into(),
            ..Default::default()
        }))
        .await
        else {
            panic!("no ticket");
        };
        let key = t.key.to_string();
        let line = "Dev: Checker instead of Developer";
        let card_note = || async {
            let view = request(
                &h.app,
                "board.view",
                json!({ "project": project.key, "width": 1376.0, "height": 856.0 }),
            )
            .await
            .unwrap();
            assert_eq!(view["columns"][0]["id"], "backlog");
            view["columns"][0]["cards"][0]["card"]["note"]["text"].clone()
        };
        let overview_line = || async {
            request(&h.app, "overview.view", json!({ "key": key }))
                .await
                .unwrap()["own_agent"]
                .clone()
        };
        assert_eq!(card_note().await, Value::Null, "nothing chosen yet");
        assert_eq!(overview_line().await, Value::Null);

        send(Request::TicketSetChoice(SetChoiceParams {
            key: key.clone(),
            stage: "Dev".into(),
            choice: "Checker".into(),
        }))
        .await
        .unwrap();
        assert_eq!(card_note().await, line, "on the Backlog card");
        assert_eq!(overview_line().await, line, "on Overview");
        h.service.stop_all().await;
    }

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
        assert!(!chip.image);
        let shot = dir.path().join("Shot.PNG");
        std::fs::write(&shot, [0x89, b'P', b'N', b'G']).unwrap();
        let chip = file(FileParams {
            path: shot.display().to_string(),
        })
        .await
        .unwrap();
        assert!(chip.image, "an image shows its thumbnail in the box");

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
        assert!(refused.message.starts_with("run.log is 21 MB; the limit is 20 MB."), "{}", refused.message);
    }
}

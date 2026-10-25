//! `board.*`: the board (UI-02, TKT-06, CLS-03, CLS-06, REV-14; d0004, d0037, d0041, d0047, d0052, d0066;
//! *03 · Board — shop-web*, *03 · Board & columns*). `board.view` builds the columns of the project's flow for the
//! page from its snapshot (SHL-06, STG-08, d0087, d0108: on the default flow five, Review holding Ready for Review
//! and Approved as two sections); a drag is the UIT-04 way: `board.drag` answers once, at drag start, with every
//! drop target's verdict (the Review column and its Approved section separately) from the core's `check_move_in` on
//! the project's flow, so the page draws the whole drag itself;
//! `board.drop` says what the drop does (and carries out a plain move, `ticket.drag`); `board.park` is the Park
//! confirmation's button.
//!
//! T-0421 (FLW-18; d0108): every stage is a drop target. A drop back to an earlier Agent stage or agent check answers
//! with the drag-back dialog ("Send SHOP-19 back to Dev?", "Run Security again on SHOP-18?"), whose confirm is
//! `board.drag_back` (`ticket.drag` confirmed, with the instructions or the note; unsent text in the agent's terminal
//! asks first, d0048). A parked card's drag answer names the stage it returns to.
//!
//! T-0324 (d0087, Q-G): `board.view` takes an optional Project agent id (`agent`) and then shows only the tickets in
//! that agent's "Created and changed" (`project_agent.created`), with the "Created or changed by <agent> ✕" chip.
//!
//! T-0221 (UI-02, EPC-01, TKT-06, TKT-08, TKT-09, CLS-01; d0047, d0060, d0065, d0033): `board.search` is the header's
//! search and filter field (the board takes the same filter with `board.view`); `board.menu` and `board.menu_choose`
//! are the card's right-click menu; `board.delete_dialog` and `board.delete` are the one delete confirmation of a
//! never-started ticket, which the card menu, the sheet's "…" menu and the Project agent's summary all open.
//!
//! T-0270 (EPC-12, EPC-14, EPC-15; d0080): an epic's drag answer counts what it queues ("Queue 2 children"), and a
//! drop of the epic card, one of its rows or a "not queued" chip (or ⌘D on the epic) sends `epic.queue`, then
//! shows the queue toast with its outcome.
//!
//! T-0271 (EPC-14; d0080, d0052, d0066): an epic's track dragged by its name onto Backlog takes back: `board.drop`
//! sends `epic.take_back` when it unqueues waiting children, or answers with the Park-all confirmation when every
//! child started, whose button is `board.park_all` (`epic.take_back` confirmed).
//!
//! T-0274 (EPC-18, EPC-17; d0080, Q55): `board.focus` puts the epic of a card or lane in the filter (its
//! `epic:` token, which dims the other cards) or takes it out (Esc); the menu of an epic, its card or its lane head,
//! has the epic's own items.
//!
//! T-0289 (LEAD-11, LEAD-02, LEAD-12; d0083): `board.view` reads the project's epic leads (`lead.list`) for the
//! Lead row on epic cards and the lead on lane heads (EPL-01, T-0462); `board.lead_open` is the card's Open lead
//! (`lead.open`) and `board.lead_resume` the Resume of an ended lead (`lead.resume`).
//!
//! T-0460 (SHB-05, BRD-01…BRD-04; d0113): `board.view` reads the project's view preferences from the service
//! (`ProjectSettings.view`: Board or List, lanes, folds) and the page changes one at a time with `board.set_view`
//! (`project_view.set`); nothing is kept in the page.
//!
//! T-0362 (AGT-08, SHL-08; d0086, d0087): `board.agents` also brings the connected tools (avatars once two are
//! connected, here and on every screen through `tool_avatar::set_connected`), each card's holds and row moves.

use otaq_core::board::snapshot::BoardSnapshot;
use otaq_core::board::stage_choices::StageOptions;
use otaq_core::board::tickets::{Base, EventKind, Ticket};
use otaq_core::domain::Status;
use otaq_core::flow::Flow;
use otaq_core::board::settings::ViewPrefs;
use otaq_core::protocol::{
    CloseParams, DragParams, EpicQueueParams, IdParams, KeyParams, LeadParams, ProjectParams,
    ProjectViewParams, Reply, Request, TicketChoicesParams,
};
use otaq_core::review::ReturnOutcome;
use otaq_view::view_models::board_page::{
    self, AgentFilter, BoardPage, DragAnswer, DropOutcome, DropStep, DropTargetId, Params, ProjectFacts,
};
use otaq_view::view_models::card_menu::{self, Choice};
use otaq_view::view_models::delete::{self as delete_text, DeleteText};
use otaq_view::view_models::epic_actions;
use otaq_view::view_models::menu::MenuItem;
use otaq_view::view_models::search_page::{self, FilterState, SearchView};
use otaq_view::view_models::send_back as sb;
use otaq_view::view_models::terminal_strip::{Click, UnsentPrompt};
use otaq_view::view_models::view_prefs::{self, Change};
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::sheet::{Sent, Unsent};
use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "board.view" => reply(&view(app, parse(params)?).await?),
        "board.set_view" => reply(&set_view(app, parse(params)?).await?),
        "board.drag" => reply(&drag(app, parse(params)?).await?),
        "board.drop" => reply(&drop(app, parse(params)?).await?),
        "board.start" => reply(&start(app, parse(params)?).await?),
        "board.park" => reply(&park(app, parse(params)?).await?),
        "board.drag_back" => reply(&drag_back(app, parse(params)?).await?),
        "board.park_all" => reply(&park_all(app, parse(params)?).await?),
        "board.search" => reply(&search(app, parse(params)?).await?),
        "board.focus" => reply(&focus(app, parse(params)?).await?),
        "board.menu" => reply(&menu(app, parse(params)?).await?),
        "board.menu_choose" => reply(&menu_choose(app, parse(params)?).await?),
        "board.delete_dialog" => reply(&delete_dialog(app, parse(params)?).await),
        "board.delete" => reply(&delete(app, parse(params)?).await?),
        "board.lead_open" => reply(&lead_open(app, parse(params)?).await?),
        "board.lead_resume" => reply(&lead_resume(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(method: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {method}: {other:?}"))
}

/// What `board.drag`, `board.park` and `board.park_all` take: the ticket (the epic for `board.park_all`).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "BoardTicketParams")]
pub struct TicketParams {
    pub key: String,
}

/// What `board.drop` takes: the ticket and where it was dropped (a column, or Review's Approved section).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "BoardDropParams")]
pub struct DropParams {
    pub key: String,
    pub target: DropTargetId,
}

/// What `board.drag_back` takes: the drag-back dialog's ticket and target, the text, and "Clear it and send".
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "BoardDragBackParams")]
pub struct DragBackParams {
    pub key: String,
    pub target: DropTargetId,
    #[serde(default)]
    pub note: String,
    /// Text pasted into the note as its report (CMT-13).
    #[serde(default)]
    #[ts(optional)]
    pub report: Option<String>,
    #[serde(default)]
    pub clear: bool,
}

/// What `board.search` takes: the project, the filter as the page keeps it, and the open ticket.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SearchParams")]
pub struct SearchParams {
    pub project: String,
    pub filter: FilterState,
    pub selected: Option<String>,
}

/// What `board.focus` takes: the project, its filter as the page keeps it, and the card (or track) to focus the
/// epic of; `None` leaves the focus (Esc).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "BoardFocusParams")]
pub struct FocusParams {
    pub project: String,
    pub filter: FilterState,
    pub key: Option<String>,
}

/// What `board.set_view` takes: the project and one change of its view preferences (SHB-05).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "BoardSetViewParams")]
pub struct SetViewParams {
    pub project: String,
    pub change: Change,
}

/// What `board.menu_choose` takes: the card and the chosen item's id.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "CardMenuParams")]
pub struct MenuParams {
    pub key: String,
    pub id: String,
}

pub(super) async fn snapshot(app: &App, project: &str) -> Result<BoardSnapshot, RequestError> {
    match app
        .service
        .send(Request::BoardSnapshot(ProjectParams {
            project: project.to_owned(),
        }))
        .await?
    {
        Reply::Board(board) => Ok(board),
        other => Err(unexpected("board.snapshot", &other)),
    }
}

/// The project's flow (FLW-01, STG-08): its current stored version, which the board's columns and drags follow; the
/// default flow when it can't be read.
pub(crate) async fn flow(app: &App, project: &str) -> Flow {
    match app
        .service
        .send(Request::FlowCurrent(ProjectParams {
            project: project.to_owned(),
        }))
        .await
    {
        Ok(Reply::FlowVersion(version)) => version
            .file()
            .map_or_else(|_| otaq_core::flow::default_flow(), |f| f.flow),
        _ => otaq_core::flow::default_flow(),
    }
}

async fn ticket(app: &App, key: &str) -> Result<Ticket, RequestError> {
    match app
        .service
        .send(Request::TicketGet(KeyParams {
            key: key.to_owned(),
        }))
        .await?
    {
        Reply::Ticket(ticket) => Ok(*ticket),
        other => Err(unexpected("ticket.get", &other)),
    }
}

/// The project's view preferences (SHB-05), from the service; the defaults when they can't be read.
pub(crate) async fn view_prefs(app: &App, project: &str) -> ViewPrefs {
    match app
        .service
        .send(Request::ProjectSettingsGet(ProjectParams {
            project: project.to_owned(),
        }))
        .await
    {
        Ok(Reply::ProjectSettings(reply)) => reply.settings.view,
        _ => ViewPrefs::default(),
    }
}

/// One change of the view preferences, stored by the service (`project_view.set`); the page then asks for the
/// board again.
pub async fn set_view(app: &App, params: SetViewParams) -> Result<(), RequestError> {
    let mut view = view_prefs(app, &params.project).await;
    view_prefs::apply(&mut view, &params.change);
    match app
        .service
        .send(Request::ProjectViewSet(ProjectViewParams {
            project: params.project,
            view,
        }))
        .await?
    {
        Reply::Ok => Ok(()),
        other => Err(unexpected("project_view.set", &other)),
    }
}

pub async fn view(app: &App, params: Params) -> Result<BoardPage, RequestError> {
    let board = snapshot(app, &params.project).await?;
    let tickets: Vec<Ticket> = board.tickets().cloned().collect();
    let project = &board.project;
    // Cards a Project agent created name it by its current name (T-0243, d0068).
    let agent_names: Vec<(String, String)> = match app
        .service
        .send(Request::ProjectAgentList(ProjectParams {
            project: params.project.clone(),
        }))
        .await
    {
        Ok(Reply::ProjectAgents(list)) => list
            .open
            .into_iter()
            .chain(list.recent)
            .map(|a| (a.id, a.name))
            .collect(),
        _ => Vec::new(),
    };
    // The epic leads: the Lead row and the track's lead line (T-0289). An older service has none.
    let leads = match app
        .service
        .send(Request::LeadList(ProjectParams {
            project: params.project.clone(),
        }))
        .await
    {
        Ok(Reply::Leads(leads)) => leads,
        _ => Vec::new(),
    };
    // The agent filter (d0087): the agent's created and changed tickets; a gone agent filters nothing.
    let agent_filter = match &params.agent {
        Some(id) => match app
            .service
            .send(Request::ProjectAgentCreated(IdParams { id: id.clone() }))
            .await
        {
            Ok(Reply::CreatedAndChanged(created)) => {
                let name = agent_names
                    .iter()
                    .find(|(agent, _)| agent == id)
                    .map_or("Noyan", |(_, name)| name.as_str());
                Some(AgentFilter::of(id, name, &created))
            }
            _ => None,
        },
        None => None,
    };
    // The cards' named agents: the footer's "set by you" / "picked by", who has the turn (T-0345).
    let agents = match app
        .service
        .send(Request::BoardAgents(ProjectParams {
            project: params.project.clone(),
        }))
        .await
    {
        Ok(Reply::BoardAgents(agents)) => Some(*agents),
        _ => None,
    };
    // The connected tools come with them (T-0362): every screen's avatars follow (SHL-08).
    if let Some(agents) = &agents {
        otaq_view::view_models::tool_avatar::set_connected(&agents.connected);
    }
    // A ticket interrupted while validating says Resume starts a fresh validation (T-0244).
    let was_validating = match app.service.send(Request::StartupReport).await {
        Ok(Reply::StartupReport(report)) => report.was_validating,
        _ => Vec::new(),
    };
    // Every agent: the column headers name each stage's tool and model (FLW-12).
    let agent_list = match app.service.send(Request::AgentsList).await {
        Ok(Reply::Agents(list)) => Some(*list),
        _ => None,
    };
    // No ticket enters what a waiting flow change waits for (FLW-06, FED-08): the cards it holds say why.
    let starts_held = match app
        .service
        .send(Request::FlowWaiting(ProjectParams {
            project: params.project.clone(),
        }))
        .await
    {
        Ok(Reply::FlowWaiting(Some(waiting))) => Some(waiting.hold),
        _ => None,
    };
    let flow = flow(app, &params.project).await;
    let view = view_prefs(app, &params.project).await;
    let page = board_page::page(
        &tickets,
        &ProjectFacts {
            flow: &flow,
            name: &project.name,
            repo_path: &project.repo_path,
            base_branch: &project.base_branch,
            agent_names: &agent_names,
            was_validating: &was_validating,
            leads: &leads,
            agent_filter: agent_filter.as_ref(),
            agents: agents.as_ref(),
            agent_list: agent_list.as_ref(),
            starts_held: starts_held.as_ref(),
            view: &view,
            paused: project.paused_since.is_some(),
        },
        &params,
        otaq_core::store::now_ms(),
        &super::keymap::labels(app).await,
    );
    Ok(page)
}

/// Asked once when a drag starts; the page asks nothing more until the drop. An epic or a child also reads its
/// project's tickets, for what the drop queues.
pub async fn drag(app: &App, params: TicketParams) -> Result<DragAnswer, RequestError> {
    let ticket = ticket(app, &params.key).await?;
    let project = ticket.key.project.to_string();
    let tickets: Vec<Ticket> = if ticket.is_epic() || ticket.epic.is_some() {
        let board = snapshot(app, &project).await?;
        board.tickets().cloned().collect()
    } else {
        Vec::new()
    };
    let flow = flow(app, &project).await;
    let left = match ticket.status {
        Status::Parked => parked_from(app, &ticket, &flow).await,
        _ => None,
    };
    Ok(board_page::drag_answer_in(
        &flow,
        &ticket,
        &tickets,
        left.as_deref(),
    ))
}

/// The Agent stage a parked ticket left, by name (its Parked event's stage; a check's Agent stage before it).
async fn parked_from(app: &App, ticket: &Ticket, flow: &Flow) -> Option<String> {
    let history = app
        .service
        .send(Request::TicketHistory(KeyParams {
            key: ticket.key.to_string(),
        }))
        .await;
    let Ok(Reply::History(events)) = history else {
        return None;
    };
    let id = events
        .iter()
        .rev()
        .find(|e| e.kind == EventKind::Parked)?
        .payload
        .get("stage_id")?
        .as_str()?
        .to_owned();
    let stage = flow.stage(&id)?;
    let stage = match stage.is_agent_stage() {
        true => stage,
        false => otaq_core::flow::engine::back_to(flow, &id, &[])?,
    };
    Some(stage.name.clone())
}

/// The ticket's stage choices (the agent it picked for a stage, the stages it skips), for the drag-back dialog.
async fn choices_of(app: &App, key: &str) -> Vec<StageOptions> {
    match app
        .service
        .send(Request::TicketChoices(TicketChoicesParams {
            key: Some(key.to_owned()),
            project: None,
        }))
        .await
    {
        Ok(Reply::TicketChoices(c)) => c.stages,
        _ => Vec::new(),
    }
}

/// The branch an approve merges into: the service's own proposal, else the ticket's base.
async fn merge_target(app: &App, ticket: &Ticket) -> String {
    let proposal = app
        .service
        .send(Request::TicketMergeProposal(KeyParams {
            key: ticket.key.to_string(),
        }))
        .await;
    match (proposal, &ticket.base) {
        (Ok(Reply::MergeProposal(p)), _) => p.target,
        (_, Base::Branch(branch)) => branch.clone(),
        (_, Base::Epic(epic)) => otaq_core::naming::branch(&epic.to_string()),
    }
}

pub async fn drop(app: &App, params: DropParams) -> Result<DropOutcome, RequestError> {
    let ticket = ticket(app, &params.key).await?;
    let flow = flow(app, &ticket.key.project.to_string()).await;
    // An epic's drop depends on its place (its card or its track) and its children; a linked ticket's on its parents
    // (DPV-03).
    let tickets: Vec<Ticket> = if ticket.is_epic() || !ticket.depends_on.is_empty() {
        let board = snapshot(app, &ticket.key.project.to_string()).await?;
        board.tickets().cloned().collect()
    } else {
        Vec::new()
    };
    let step = board_page::drop_step_in(&flow, &ticket, &tickets, &params.target);
    match step {
        // A blocked ticket goes to (or stays in) Ready for Dev, then the dialog says why it can't start (DPV-03).
        DropStep::Blocked { dialog, queue } => {
            if queue {
                let ready = DropTargetId::new(otaq_view::view_models::flow_columns::READY_FOR_DEV);
                let step = board_page::drop_step_in(&flow, &ticket, &tickets, &ready);
                carry_out(app, &params.key, &ticket, &flow, ready, step).await?;
            }
            Ok(DropOutcome::Blocked { dialog })
        }
        step => carry_out(app, &params.key, &ticket, &flow, params.target, step).await,
    }
}

/// What `board.start` takes: the ticket the sheet shows.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "BoardStartParams")]
pub struct StartParams {
    pub key: String,
}

/// The sheet's Start: a drop on the first agent column, as ⌘D (DPV-03: a blocked ticket waits in Ready for Dev and
/// the page shows why).
pub async fn start(app: &App, params: StartParams) -> Result<DropOutcome, RequestError> {
    let ticket = ticket(app, &params.key).await?;
    let flow = flow(app, &ticket.key.project.to_string()).await;
    let Some(column) = otaq_view::view_models::flow_columns::Columns::of(&flow)
        .first(otaq_view::view_models::flow_columns::ColumnKind::Agent)
        .map(|c| c.id.clone())
    else {
        return Ok(DropOutcome::Nothing);
    };
    drop(
        app,
        DropParams {
            key: params.key,
            target: DropTargetId::new(column),
        },
    )
    .await
}

/// Carries out a drop's step: sends the move, or answers with the dialog the page shows first.
async fn carry_out(
    app: &App,
    key: &str,
    ticket: &Ticket,
    flow: &Flow,
    target: DropTargetId,
    step: DropStep,
) -> Result<DropOutcome, RequestError> {
    Ok(match step {
        DropStep::Nothing => DropOutcome::Nothing,
        DropStep::NotYet(message) => DropOutcome::NotYet { message },
        DropStep::Blocked { dialog, .. } => DropOutcome::Blocked { dialog },
        DropStep::Park => DropOutcome::Park {
            dialog: board_page::park_dialog(ticket),
        },
        DropStep::Move => {
            // A move only follows a target the flow has (drop_step_in says Nothing otherwise).
            let Some(target) = board_page::core_target(flow, &target) else {
                return Ok(DropOutcome::Nothing);
            };
            app.service
                .send(Request::TicketDrag(DragParams {
                    key: key.to_owned(),
                    target,
                    confirmed: false,
                    note: String::new(),
                    clear_first: false,
                    report: None,
                }))
                .await?;
            DropOutcome::Moved
        }
        DropStep::DragBack => {
            let choices = choices_of(app, key).await;
            match board_page::drag_back_dialog(flow, ticket, &target, &choices) {
                Some(dialog) => DropOutcome::DragBack {
                    dialog: Box::new(dialog),
                },
                None => DropOutcome::Nothing,
            }
        }
        DropStep::TakeBack { epic } => {
            take_back(app, epic.to_string(), false).await?;
            DropOutcome::Moved
        }
        DropStep::ParkAll(dialog) => DropOutcome::ParkAll { dialog },
        DropStep::QueueEpic { epic, child } => {
            let request = Request::EpicQueue(EpicQueueParams {
                key: epic.to_string(),
                child: child.map(|c| c.to_string()),
            });
            match app.service.send(request).await? {
                Reply::EpicQueued(outcome) => {
                    crate::api::notices::show_epic_queued(app, &outcome).await;
                    DropOutcome::Moved
                }
                other => return Err(unexpected("epic.queue", &other)),
            }
        }
        DropStep::Approve => {
            let project = ticket.key.project.to_string();
            let (board, target, provider) = tokio::join!(
                snapshot(app, &project),
                merge_target(app, ticket),
                app.service.send(Request::ProjectPrProvider(ProjectParams {
                    project: project.clone(),
                })),
            );
            let tickets: Vec<Ticket> = board?.tickets().cloned().collect();
            let provider = match provider {
                Ok(Reply::PrProvider(p)) => Some(p),
                _ => None,
            };
            DropOutcome::Approve {
                menu: board_page::approve_menu(ticket, &target, &tickets, provider.as_ref()),
            }
        }
    })
}

/// The drag-back dialog's confirm (FLW-18): `ticket.drag` confirmed with the instructions or the note. Unsent text in
/// the agent's terminal asks first ("Clear it and send" sends again with `clear`). A refusal stays in the dialog.
pub async fn drag_back(app: &App, params: DragBackParams) -> Result<Sent, RequestError> {
    let ticket = ticket(app, &params.key).await?;
    let flow = flow(app, &ticket.key.project.to_string()).await;
    let Some(target) = board_page::core_target(&flow, &params.target) else {
        return Err(RequestError::plain(format!(
            "{} has no column {}",
            ticket.key.project,
            params.target.as_str()
        )));
    };
    let sent = app
        .service
        .send(Request::TicketDrag(DragParams {
            key: params.key.clone(),
            target,
            confirmed: true,
            note: params.note.clone(),
            clear_first: params.clear,
            report: sb::pasted_report(params.report.as_deref()),
        }))
        .await;
    match sent? {
        Reply::Returned(ReturnOutcome::NeedsConfirmation { unclearable_by, .. }) => {
            Ok(Sent::Ask(Unsent::of(
                &UnsentPrompt::for_click(
                    ticket.key.clone(),
                    None,
                    sb::sending(0),
                    Click::SendBack { note: params.note },
                )
                .unclearable_by(unclearable_by),
            )))
        }
        // A refusal comes back as the error: the dialog shows it and stays open.
        _ => Ok(Sent::Done),
    }
}

/// The Park confirmation's button: the agent stops; branch, worktree and conversation are kept (CLS-06).
pub async fn park(app: &App, params: TicketParams) -> Result<(), RequestError> {
    app.service
        .send(Request::TicketPark(CloseParams {
            key: params.key,
            confirm: true,
        }))
        .await?;
    Ok(())
}

/// `epic.take_back`: unqueues the epic's waiting children, or, `confirm`ed, parks its running ones (EPC-14).
async fn take_back(app: &App, epic: String, confirm: bool) -> Result<(), RequestError> {
    match app
        .service
        .send(Request::EpicTakeBack(CloseParams {
            key: epic,
            confirm,
        }))
        .await?
    {
        Reply::EpicTakenBack(_) => Ok(()),
        other => Err(unexpected("epic.take_back", &other)),
    }
}

/// Open lead on a Backlog epic card (LEAD-02): the click is the go-ahead for the lead's tokens. The page then
/// opens the epic sheet (its Lead tab from T-0290).
pub async fn lead_open(app: &App, params: TicketParams) -> Result<(), RequestError> {
    match app
        .service
        .send(Request::LeadOpen(LeadParams { epic: params.key }))
        .await?
    {
        Reply::Lead(_) => Ok(()),
        other => Err(unexpected("lead.open", &other)),
    }
}

/// Resume on an ended lead (LEAD-12): the same conversation.
pub async fn lead_resume(app: &App, params: TicketParams) -> Result<(), RequestError> {
    match app
        .service
        .send(Request::LeadResume(LeadParams { epic: params.key }))
        .await?
    {
        Reply::Lead(_) => Ok(()),
        other => Err(unexpected("lead.resume", &other)),
    }
}

/// The Park-all confirmation's button: every running child of the epic is parked, its work kept (EPC-14, d0052).
pub async fn park_all(app: &App, params: TicketParams) -> Result<(), RequestError> {
    take_back(app, params.key, true).await
}

/// The header's field: chips, the count, and the suggestions with theirs.
pub async fn search(app: &App, params: SearchParams) -> Result<SearchView, RequestError> {
    let board = snapshot(app, &params.project).await?;
    let tickets: Vec<Ticket> = board.tickets().cloned().collect();
    let selected = params.selected.and_then(|k| k.parse().ok());
    Ok(search_page::view(
        &params.filter,
        &tickets,
        selected.as_ref(),
        &super::keymap::labels(app).await,
    ))
}

/// Focusing an epic (EPC-18): the filter with the epic's token in place of another (`null` for a standalone ticket),
/// or, with no key, without it (`null` when nothing was focused).
pub async fn focus(app: &App, params: FocusParams) -> Result<Option<FilterState>, RequestError> {
    let board = snapshot(app, &params.project).await?;
    let tickets: Vec<Ticket> = board.tickets().cloned().collect();
    let key = match params.key {
        Some(k) => Some(
            k.parse()
                .map_err(|_| RequestError::plain(format!("not a ticket key: {k}")))?,
        ),
        None => None,
    };
    Ok(board_page::focus_filter(
        &params.filter,
        key.as_ref(),
        &tickets,
    ))
}

/// The card's right-click menu: only the items its ticket allows (Delete… disabled once started).
pub async fn menu(app: &App, params: TicketParams) -> Result<Vec<MenuItem>, RequestError> {
    let ticket = ticket(app, &params.key).await?;
    let board = snapshot(app, &ticket.key.project.to_string()).await?;
    let tickets: Vec<Ticket> = board.tickets().cloned().collect();
    let has_epics = !epic_actions::epic_choices(&tickets).is_empty();
    Ok(card_menu::items(
        &ticket,
        &tickets,
        has_epics,
        &super::keymap::labels(app).await,
    ))
}

/// A chosen item: Pin, Unpin and the priorities are sent here; the dialogs are the page's to open.
pub async fn menu_choose(app: &App, params: MenuParams) -> Result<Choice, RequestError> {
    let ticket = ticket(app, &params.key).await?;
    let choice = card_menu::choice(&params.id, &ticket);
    if choice == Choice::Sent
        && let Some(request) = card_menu::request(&params.id, &ticket)
    {
        app.service.send(request).await?;
    }
    Ok(choice)
}

/// The confirmation's texts from the service's preview; a refusal (the ticket started) comes back in the dialog.
pub async fn delete_dialog(app: &App, params: TicketParams) -> DeleteText {
    match app
        .service
        .send(Request::TicketDeletePreview(KeyParams {
            key: params.key.clone(),
        }))
        .await
    {
        Ok(Reply::DeletePreview(preview)) => {
            let text = delete_text::text(&preview);
            match agent_creator(app, &params.key).await {
                Some(name) => delete_text::created_by_agent(text, &name),
                None => text,
            }
        }
        Ok(other) => delete_text::refused(
            &params.key,
            unexpected("ticket.delete_preview", &other).message,
        ),
        Err(e) => delete_text::refused(&params.key, e.message),
    }
}

/// The current name of the Project agent that created the ticket, if one did (T-0243).
async fn agent_creator(app: &App, key: &str) -> Option<String> {
    let ticket = ticket(app, key).await.ok()?;
    let otaq_core::domain::Actor::ProjectAgent(id) = &ticket.created_by else {
        return None;
    };
    match app
        .service
        .send(Request::ProjectAgentList(ProjectParams {
            project: ticket.key.project.to_string(),
        }))
        .await
    {
        Ok(Reply::ProjectAgents(list)) => list
            .open
            .into_iter()
            .chain(list.recent)
            .find(|a| &a.id == id)
            .map(|a| a.name),
        _ => None,
    }
}

/// The confirm click: the service deletes (refusing a started ticket) and a toast says what went.
pub async fn delete(app: &App, params: TicketParams) -> Result<(), RequestError> {
    match app
        .service
        .send(Request::TicketDelete(KeyParams { key: params.key }))
        .await?
    {
        Reply::DeletePreview(preview) => {
            app.notices.toast(NewToast::new(
                ToastLevel::Success,
                delete_text::deleted(&preview),
                None,
            ));
            Ok(())
        }
        other => Err(unexpected("ticket.delete", &other)),
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::board::tickets::NewTicketInput;
    use otaq_core::git::testing::TempRepos;
    use otaq_core::board::projects::AddProject;
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    async fn send(h: &Harness, r: Request) -> Reply {
        h.app.service.send(r).await.unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_board_and_a_drag_come_from_the_real_service() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let Reply::ProjectAdded { project, .. } = send(
            &h,
            Request::ProjectAdd(AddProject {
                path: repos.repo("shop-web", 1).display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }),
        )
        .await
        else {
            panic!("no project");
        };
        let params = json!({ "project": project.key, "width": 1376.0, "height": 856.0 });

        // No tickets yet: the empty board over the six columns.
        let view = request(&h.app, "board.view", params.clone()).await.unwrap();
        assert_eq!(view["empty"]["title"], "shop-web is ready");
        // Validate has its own column (FLW-01): Backlog · Ready for Dev · Dev · Validate · Review · Done.
        assert_eq!(view["columns"].as_array().map(Vec::len), Some(6));
        // The view preferences are the service's (SHB-05): Done folded at first, a fold from Dev's ⋯ comes back.
        assert_eq!(view["prefs"]["folded_columns"], json!(["done"]));
        assert_eq!(view["done"]["text"], "Done 0 \u{2192}");
        let fold = json!({ "project": project.key, "change": { "kind": "fold_column", "column": "dev", "folded": true } });
        request(&h.app, "board.set_view", fold).await.unwrap();
        let list = json!({ "project": project.key, "change": { "kind": "view", "view": "list" } });
        request(&h.app, "board.set_view", list).await.unwrap();
        let view = request(&h.app, "board.view", params.clone()).await.unwrap();
        assert_eq!(view["columns"][2]["collapsed"], true);
        assert_eq!(view["prefs"]["view"], "list");
        assert_eq!(view["toolbar"]["views"]["options"][1]["selected"], true);
        let back = json!({ "project": project.key, "change": { "kind": "view", "view": "board" } });
        request(&h.app, "board.set_view", back).await.unwrap();

        let mut keys = Vec::new();
        for title in ["Update footer links", "Old idea"] {
            let Reply::Ticket(t) = send(
                &h,
                Request::TicketCreate(NewTicketInput {
                    project: project.key.clone(),
                    kind: Default::default(),
                    title: title.into(),
                    body: String::new(),
                    priority: None,
                    epic: None,
                    base_branch: None,
                    external_id: None,
                    first_comment: None,
                }),
            )
            .await
            else {
                panic!("no ticket");
            };
            keys.push(t.key.to_string());
        }
        let view = request(&h.app, "board.view", params.clone()).await.unwrap();
        assert_eq!(view["empty"], Value::Null);
        assert_eq!(view["columns"][0]["count"], "2");
        assert_eq!(view["columns"][0]["cards"][0]["kind"], "ticket");
        assert_eq!(view["columns"][4]["sections"][1]["empty"], "Nothing approved");
        assert_eq!(view["min_column_width"], 180.0);

        // An agent filter for an agent the service doesn't know filters nothing and shows no chip.
        let mut gone = params.clone();
        gone["agent"] = json!("no-such-agent");
        let view = request(&h.app, "board.view", gone).await.unwrap();
        assert_eq!(view["agent_filter"], Value::Null);
        assert_eq!(view["columns"][0]["count"], "2");

        // One answer per drag, every column at once.
        let drag = request(&h.app, "board.drag", json!({ "key": keys[0] }))
            .await
            .unwrap();
        assert_eq!(drag["targets"][0]["verdict"]["kind"], "home");
        assert_eq!(drag["targets"][1]["verdict"]["kind"], "allowed");
        assert_eq!(drag["targets"][3]["target"], "validate");
        assert_eq!(
            drag["targets"][4]["verdict"]["title"],
            "Only agents move tickets to Ready for Review"
        );
        assert_eq!(drag["targets"][5]["target"], "approved");
        assert_eq!(drag["targets"][5]["verdict"]["kind"], "forbidden");

        // A refused drop does nothing.
        let outcome = request(
            &h.app,
            "board.drop",
            json!({ "key": keys[0], "target": "review" }),
        )
        .await
        .unwrap();
        assert_eq!(outcome["kind"], "nothing");

        // Reopen: a closed ticket dragged out of Done goes back to Backlog (CLS-03).
        send(
            &h,
            Request::TicketClose(CloseParams {
                key: keys[1].clone(),
                confirm: true,
            }),
        )
        .await;
        let outcome = request(
            &h.app,
            "board.drop",
            json!({ "key": keys[1], "target": "backlog" }),
        )
        .await
        .unwrap();
        assert_eq!(outcome["kind"], "moved");
        let view = request(&h.app, "board.view", params).await.unwrap();
        assert_eq!(view["columns"][0]["count"], "2");
        h.service.stop_all().await;
    }

    /// T-0345, AGM-09: `board.agents` reaches the cards, and a card names no agent before it starts, whatever its
    /// flow's stages run (four agents on a fresh install).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_card_names_no_agent_before_it_starts() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let Reply::ProjectAdded { project, .. } = send(
            &h,
            Request::ProjectAdd(AddProject {
                path: repos.repo("shop-web", 1).display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }),
        )
        .await
        else {
            panic!("no project");
        };
        for title in ["Add gift wrap option", "Update footer links"] {
            send(
                &h,
                Request::TicketCreate(NewTicketInput {
                    project: project.key.clone(),
                    kind: Default::default(),
                    title: title.into(),
                    body: String::new(),
                    priority: None,
                    epic: None,
                    base_branch: None,
                    external_id: None,
                    first_comment: None,
                }),
            )
            .await;
        }
        let params = json!({ "project": project.key, "width": 1376.0, "height": 856.0 });
        let view = request(&h.app, "board.view", params).await.unwrap();
        let cards = view["columns"][0]["cards"].as_array().unwrap();
        assert_eq!(cards.len(), 2);
        assert!(cards.iter().all(|c| c["card"]["footer"].is_null()));
        h.service.stop_all().await;
    }

    /// T-0221: the filter hides cards on the board and fills the header's field; the card menu pins and sets the
    /// priority; the delete confirmation words the service's preview, deletes, and is refused for a started ticket.
    #[tokio::test(flavor = "multi_thread")]
    async fn search_card_menu_and_delete_come_from_the_real_service() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let Reply::ProjectAdded { project, .. } = send(
            &h,
            Request::ProjectAdd(AddProject {
                path: repos.repo("shop-web", 1).display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }),
        )
        .await
        else {
            panic!("no project");
        };
        let mut keys = Vec::new();
        for title in ["Checkout API", "Footer links"] {
            let Reply::Ticket(t) = send(
                &h,
                Request::TicketCreate(NewTicketInput {
                    project: project.key.clone(),
                    kind: Default::default(),
                    title: title.into(),
                    body: String::new(),
                    priority: None,
                    epic: None,
                    base_branch: None,
                    external_id: None,
                    first_comment: None,
                }),
            )
            .await
            else {
                panic!("no ticket");
            };
            keys.push(t.key.to_string());
        }

        // Search: the board hides what doesn't match; the field turns a finished token into a chip.
        let view = request(
            &h.app,
            "board.view",
            json!({ "project": project.key, "width": 1376.0, "height": 856.0,
                    "filter": { "tokens": [], "text": "checkout" } }),
        )
        .await
        .unwrap();
        assert_eq!(view["columns"][0]["count"], "1/2");
        assert_eq!(view["columns"][0]["hidden"], "1 hidden by filter");
        let field = request(
            &h.app,
            "board.search",
            json!({ "project": project.key, "filter": { "tokens": [], "text": "is:backlog " },
                    "selected": null }),
        )
        .await
        .unwrap();
        assert_eq!(field["filter"]["tokens"], json!(["is:backlog"]));
        assert_eq!(field["results"], "2 results");

        // The card menu: Pin is sent here, Delete opens the dialog.
        let items = request(&h.app, "board.menu", json!({ "key": keys[1] }))
            .await
            .unwrap();
        assert_eq!(items[0]["label"], "Pin to top");
        let pin = request(
            &h.app,
            "board.menu_choose",
            json!({ "key": keys[1], "id": items[0]["id"] }),
        )
        .await
        .unwrap();
        assert_eq!(pin["kind"], "sent");
        let Reply::Ticket(pinned) = send(&h, Request::TicketGet(KeyParams { key: keys[1].clone() })).await
        else {
            panic!("no ticket");
        };
        assert!(pinned.flags.is_pinned());
        let delete_item = items
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["label"] == "Delete…")
            .unwrap();
        assert_eq!(delete_item["disabled"], false);
        let chosen = request(
            &h.app,
            "board.menu_choose",
            json!({ "key": keys[1], "id": delete_item["id"] }),
        )
        .await
        .unwrap();
        assert_eq!(chosen["kind"], "delete");

        // The delete confirmation, then the delete.
        let dialog = request(&h.app, "board.delete_dialog", json!({ "key": keys[1] }))
            .await
            .unwrap();
        assert_eq!(dialog["title"], format!("Delete {} Footer links?", keys[1]));
        assert_eq!(dialog["confirm"], "Delete ticket");
        assert_eq!(dialog["error"], Value::Null);
        request(&h.app, "board.delete", json!({ "key": keys[1] }))
            .await
            .unwrap();
        let gone = h
            .app
            .service
            .send(Request::TicketGet(KeyParams { key: keys[1].clone() }))
            .await;
        assert!(gone.is_err(), "the ticket is gone");
        h.service.stop_all().await;
    }

    /// T-0270 (EPC-12, EPC-15): an epic's drag counts what it queues; dropping one row queues only that child, then
    /// the epic card queues the rest, each with the queue toast.
    #[tokio::test(flavor = "multi_thread")]
    async fn dropping_an_epic_or_one_row_queues_and_shows_the_toast() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let Reply::ProjectAdded { project, .. } = send(
            &h,
            Request::ProjectAdd(AddProject {
                path: repos.repo("shop-web", 1).display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }),
        )
        .await
        else {
            panic!("no project");
        };
        let create = |kind, title: &str, epic: Option<String>| {
            Request::TicketCreate(NewTicketInput {
                project: project.key.clone(),
                kind,
                title: title.into(),
                body: String::new(),
                priority: None,
                epic,
                base_branch: None,
                external_id: None,
                first_comment: None,
            })
        };
        let Reply::Ticket(epic) = send(&h, create(otaq_core::domain::Kind::Epic, "Search", None)).await
        else {
            panic!("no epic");
        };
        let epic = epic.key.to_string();
        let mut children = Vec::new();
        for title in ["Search index API", "Search results page"] {
            let Reply::Ticket(t) = send(&h, create(Default::default(), title, Some(epic.clone()))).await
            else {
                panic!("no child");
            };
            children.push(t.key.to_string());
        }
        let toasts = || async {
            let view = request(&h.app, "notices.view", Value::Null).await.unwrap();
            view["toasts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["title"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };

        // T-0274: the epic's menu counts what it queues; Focus epic on a child focuses the epic, which dims the rest.
        let items = request(&h.app, "board.menu", json!({ "key": epic })).await.unwrap();
        let labels: Vec<&str> = items
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["label"].as_str().unwrap())
            .collect();
        assert_eq!(labels[..3], ["Open epic", "Queue never-started (2)", "Add ticket to epic"]);
        let none = json!({ "tokens": [], "text": "" });
        let focused = request(
            &h.app,
            "board.focus",
            json!({ "project": project.key, "filter": none, "key": children[1] }),
        )
        .await
        .unwrap();
        assert_eq!(focused["tokens"], json!(["epic:Search"]));
        let view = request(
            &h.app,
            "board.view",
            json!({ "project": project.key, "width": 1376.0, "height": 856.0,
                    "filter": focused }),
        )
        .await
        .unwrap();
        assert_eq!(view["focus"]["epics"], json!([epic]));
        let left = request(
            &h.app,
            "board.focus",
            json!({ "project": project.key, "filter": focused, "key": null }),
        )
        .await
        .unwrap();
        assert_eq!(left["tokens"], json!([]));

        let drag = request(&h.app, "board.drag", json!({ "key": epic })).await.unwrap();
        assert_eq!(drag["targets"][1]["verdict"]["hint"], "Queue 2 children");
        assert_eq!(drag["targets"][5]["verdict"]["kind"], "forbidden");
        let drag = request(&h.app, "board.drag", json!({ "key": children[0] }))
            .await
            .unwrap();
        assert_eq!(
            drag["targets"][1]["verdict"]["hint"],
            format!("Queue {}", children[0])
        );

        // One row: only that child, and the toast says so (a new project's Auto-merge is Off: its variant).
        let outcome = request(
            &h.app,
            "board.drop",
            json!({ "key": children[0], "target": "ready_for_dev" }),
        )
        .await
        .unwrap();
        assert_eq!(outcome["kind"], "moved");
        assert_eq!(
            toasts().await,
            [format!("Moved {} of Search to Ready for Dev", children[0])]
        );
        let Reply::Ticket(second) = send(&h, Request::TicketGet(KeyParams { key: children[1].clone() })).await
        else {
            panic!("no child");
        };
        assert_eq!(second.place, otaq_core::domain::Place::Backlog, "the other waits");

        // The epic (as ⌘D sends it, on Dev): the rest.
        let outcome = request(&h.app, "board.drop", json!({ "key": epic, "target": "dev" }))
            .await
            .unwrap();
        assert_eq!(outcome["kind"], "moved");
        let titles = toasts().await;
        assert_eq!(titles.len(), 2, "{titles:?}");
        let Reply::Ticket(second) = send(&h, Request::TicketGet(KeyParams { key: children[1].clone() })).await
        else {
            panic!("no child");
        };
        assert_eq!(second.place, otaq_core::domain::Place::Waiting(None));

        // Nothing left to queue: the drag says so, and a drop is refused.
        let drag = request(&h.app, "board.drag", json!({ "key": epic })).await.unwrap();
        assert_eq!(drag["targets"][1]["verdict"]["kind"], "forbidden");
        assert!(
            request(&h.app, "board.drop", json!({ "key": epic, "target": "ready_for_dev" }))
                .await
                .is_err()
        );
        h.service.stop_all().await;
    }

    /// T-0271 (EPC-14): the queued epic's track dragged onto Backlog says what it takes back, and the drop unqueues
    /// both never-started children at once (no dialog): the epic is its Backlog card again. With nothing queued or
    /// running, the drag over Backlog is not allowed and Park all is refused.
    #[tokio::test(flavor = "multi_thread")]
    async fn dropping_a_track_on_backlog_takes_it_back() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let Reply::ProjectAdded { project, .. } = send(
            &h,
            Request::ProjectAdd(AddProject {
                path: repos.repo("shop-web", 1).display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }),
        )
        .await
        else {
            panic!("no project");
        };
        let create = |kind, title: &str, epic: Option<String>| {
            Request::TicketCreate(NewTicketInput {
                project: project.key.clone(),
                kind,
                title: title.into(),
                body: String::new(),
                priority: None,
                epic,
                base_branch: None,
                external_id: None,
                first_comment: None,
            })
        };
        let Reply::Ticket(epic) = send(&h, create(otaq_core::domain::Kind::Epic, "Search", None)).await
        else {
            panic!("no epic");
        };
        let epic = epic.key.to_string();
        let mut children = Vec::new();
        for title in ["Search index API", "Search results page"] {
            let Reply::Ticket(t) = send(&h, create(Default::default(), title, Some(epic.clone()))).await
            else {
                panic!("no child");
            };
            children.push(t.key.to_string());
        }
        // Both children wait on a closed child, so the scheduler never starts them: a child whose start runs isn't
        // taken back, and the test is about never-started ones.
        let Reply::Ticket(closed) = send(
            &h,
            create(Default::default(), "Search schema", Some(epic.clone())),
        )
        .await
        else {
            panic!("no closed child");
        };
        let closed = closed.key.to_string();
        for child in &children {
            send(
                &h,
                Request::DepAdd(otaq_core::protocol::DepParams {
                    child: child.clone(),
                    parent: closed.clone(),
                }),
            )
            .await;
        }
        send(
            &h,
            Request::TicketClose(otaq_core::protocol::CloseParams {
                key: closed,
                confirm: true,
            }),
        )
        .await;
        let column = |key: String| {
            let h = &h;
            async move {
                let Reply::Ticket(t) = send(h, Request::TicketGet(KeyParams { key })).await else {
                    panic!("no ticket");
                };
                t.place
            }
        };

        // The card in Backlog: its drag over Backlog is home.
        let drag = request(&h.app, "board.drag", json!({ "key": epic })).await.unwrap();
        assert_eq!(drag["targets"][0]["verdict"]["kind"], "home");

        let outcome = request(&h.app, "board.drop", json!({ "key": epic, "target": "ready_for_dev" }))
            .await
            .unwrap();
        assert_eq!(outcome["kind"], "moved");
        assert_eq!(column(children[0].clone()).await, otaq_core::domain::Place::Waiting(None));

        // Now a track: Backlog takes both back.
        let n1 = children[0].rsplit('-').next().unwrap();
        let n2 = children[1].rsplit('-').next().unwrap();
        let drag = request(&h.app, "board.drag", json!({ "key": epic })).await.unwrap();
        assert_eq!(drag["targets"][0]["verdict"]["kind"], "allowed");
        assert_eq!(
            drag["targets"][0]["verdict"]["hint"],
            format!("Unqueue 2 never-started children ({n1}, {n2})")
        );
        assert_eq!(drag["targets"][0]["verdict"]["note"], Value::Null);
        assert_eq!(drag["targets"][5]["verdict"]["kind"], "forbidden");
        let outcome = request(&h.app, "board.drop", json!({ "key": epic, "target": "backlog" }))
            .await
            .unwrap();
        assert_eq!(outcome["kind"], "moved");
        for child in &children {
            assert_eq!(column(child.clone()).await, otaq_core::domain::Place::Backlog);
        }
        let board = request(
            &h.app,
            "board.view",
            json!({
                "project": project.key,
                "width": 1376.0,
                "height": 856.0,
                "filter": { "tokens": [], "text": "" },
            }),
        )
        .await
        .unwrap();
        let backlog: Vec<&str> = board["columns"][0]["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["card"]["key"].as_str().unwrap())
            .collect();
        assert_eq!(backlog, [epic.as_str()], "the card again");

        // Nothing to take back: Park all is refused.
        assert!(
            request(&h.app, "board.park_all", json!({ "key": epic }))
                .await
                .is_err()
        );
        h.service.stop_all().await;
    }
}

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
//! T-0460 (SHB-05, BRD-01…BRD-04; d0113): `board.view` reads the project's view preferences from the service
//! (`ProjectSettings.view`: Board or List, folds) and the page changes one at a time with `board.set_view`
//! (`project_view.set`); nothing is kept in the page.
//!
//! T-0362 (AGT-08, SHL-08; d0086, d0087): `board.agents` also brings the connected tools (avatars once two are
//! connected, here and on every screen through `tool_avatar::set_connected`), each card's holds and row moves.

use otaq_core::board::snapshot::BoardSnapshot;
use otaq_core::board::stage_choices::StageOptions;
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::domain::Status;
use otaq_core::flow::Flow;
use otaq_core::board::settings::ViewPrefs;
use otaq_core::protocol::{
    CloseParams, DragParams, IdParams, KeyParams, ProjectParams,
    ProjectViewParams, Reply, Request, TicketChoicesParams, TicketDeleteParams,
};
use otaq_core::review::ReturnOutcome;
use otaq_view::view_models::board_page::{
    self, AgentFilter, BoardPage, DragAnswer, DropOutcome, DropStep, DropTargetId, Params, ProjectFacts,
};
use otaq_view::view_models::card_menu::{self, Choice};
use otaq_view::view_models::delete::{self as delete_text, DeleteText};
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
        "board.search" => reply(&search(app, parse(params)?).await?),
        "board.menu" => reply(&menu(app, parse(params)?).await?),
        "board.menu_choose" => reply(&menu_choose(app, parse(params)?).await?),
        "board.delete_dialog" => reply(&delete_dialog(app, parse(params)?).await),
        "board.delete" => reply(&delete(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(method: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {method}: {other:?}"))
}

/// What `board.drag` and `board.park` take: the ticket.
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

/// The project's helpers (`board.helpers`, d0133): a build stage's piece rows on the card, the sheet's Session list.
/// None when they can't be read (the cards look as they would without).
pub(crate) async fn helpers(app: &App, project: &str) -> Vec<otaq_core::helpers::Helper> {
    match app
        .service
        .send(Request::BoardHelpers(ProjectParams {
            project: project.to_owned(),
        }))
        .await
    {
        Ok(Reply::Helpers(helpers)) => helpers,
        _ => Vec::new(),
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
    let helpers = helpers(app, &params.project).await;
    let page = board_page::page(
        &tickets,
        &ProjectFacts {
            flow: &flow,
            name: &project.name,
            repo_path: &project.repo_path,
            base_branch: &project.base_branch,
            agent_names: &agent_names,
            was_validating: &was_validating,
            agent_filter: agent_filter.as_ref(),
            agents: agents.as_ref(),
            agent_list: agent_list.as_ref(),
            starts_held: starts_held.as_ref(),
            view: &view,
            paused: project.paused_since.is_some(),
            helpers: &helpers,
        },
        &params,
        otaq_core::store::now_ms(),
        &super::keymap::labels(app).await,
    );
    Ok(page)
}

/// Asked once when a drag starts; the page asks nothing more until the drop.
pub async fn drag(app: &App, params: TicketParams) -> Result<DragAnswer, RequestError> {
    let ticket = ticket(app, &params.key).await?;
    let project = ticket.key.project.to_string();
    let tickets: Vec<Ticket> = Vec::new();
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
    match proposal {
        Ok(Reply::MergeProposal(p)) => p.target,
        _ => ticket.base.branch().to_owned(),
    }
}

pub async fn drop(app: &App, params: DropParams) -> Result<DropOutcome, RequestError> {
    let ticket = ticket(app, &params.key).await?;
    let flow = flow(app, &ticket.key.project.to_string()).await;
    // A linked ticket's drop depends on its parents (DPV-03).
    let tickets: Vec<Ticket> = if !ticket.depends_on.is_empty() {
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

/// The card's right-click menu: only the items its ticket allows (Delete… disabled once started).
pub async fn menu(app: &App, params: TicketParams) -> Result<Vec<MenuItem>, RequestError> {
    let ticket = ticket(app, &params.key).await?;
    Ok(card_menu::items(&ticket))
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
        .send(Request::TicketDelete(TicketDeleteParams {
            key: params.key,
            reason: None,
        }))
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
                    title: title.into(),
                    body: String::new(),
                    plan: String::new(),
                    priority: None,
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
                    title: title.into(),
                    body: String::new(),
                    plan: String::new(),
                    priority: None,
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
                    title: title.into(),
                    body: String::new(),
                    plan: String::new(),
                    priority: None,
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

}

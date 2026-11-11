//! `agent_panel.*`: the Project agent panel (SHL-04, PAG-01, PAG-04, PAG-05, PAG-06; d0087, d0068, d0018; *18 · Project
//! agents & terminals → Row · Pane: empty, hidden, tool problems, menu (0087)*). `agent_panel.view` answers everything the
//! agent panel draws for a project and the agent it shows (`agent_panel.rs`); the others are its clicks: Send (the first
//! message starts the agent, d0087), Rename, End agent (asks first mid-turn), Close ended agents and Resume. Which
//! agent each project shows, + and the empty agent panel are the page's own state: nothing here starts without a message
//! or a click. The empty agent panel names the agent Noyan uses (one app setting, "Noyan uses <agent>"), which
//! [`project_agent`] finds for Ask the agent too, and lets the new conversation pick another agent for itself.
//! `agent_panel.undo` is a CHANGED row's Undo (decision 013): it takes back one change Noyan made on a ticket.

use otaq_core::agents::Agent;
use otaq_core::board::stage_choices::TicketChoices;
use otaq_core::board::tickets::Ticket;
use otaq_core::project_agents::{CreatedAndChanged, ProjectAgent, ProjectAgentList, WhatChanged};
use otaq_core::protocol::{
    IdParams, KeyParams, ProjectAgentCloseParams, ProjectAgentOpenParams, ProjectAgentRenameParams,
    ProjectAgentUndoneParams,
    ProjectParams, Reply, Request, TicketChoicesParams,
};
use otaq_view::view_models::agent_panel::{self as vm, EditedTicket, AgentPanelView};
use otaq_view::view_models::project_agent_page::{self as agent_page, CloseDialog};
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "agent_panel.view" => reply(&view(app, parse(params)?).await?),
        "agent_panel.send" => reply(&send(app, parse(params)?).await?),
        "agent_panel.rename" => reply(&rename(app, parse(params)?).await?),
        "agent_panel.end" => reply(&end(app, parse(params)?).await?),
        "agent_panel.close_ended" => reply(&close_ended(app, parse(params)?).await?),
        "agent_panel.resume" => reply(&resume(app, parse(params)?).await?),
        "agent_panel.answer" => reply(&answer(app, parse(params)?).await?),
        "agent_panel.undo" => reply(&undo(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {what}: {other:?}"))
}

fn toast_error(app: &App, title: &str, error: &RequestError) {
    app.notices.toast(NewToast::new(
        ToastLevel::Error,
        title,
        Some(error.message.clone()),
    ));
}

pub(super) async fn agents(app: &App, project: &str) -> Result<ProjectAgentList, RequestError> {
    match app
        .service
        .send(Request::ProjectAgentList(ProjectParams {
            project: project.to_owned(),
        }))
        .await?
    {
        Reply::ProjectAgents(list) => Ok(*list),
        other => Err(unexpected("project_agent.list", &other)),
    }
}

async fn created_of(app: &App, id: &str) -> Result<CreatedAndChanged, RequestError> {
    match app
        .service
        .send(Request::ProjectAgentCreated(IdParams { id: id.to_owned() }))
        .await?
    {
        Reply::CreatedAndChanged(c) => Ok(*c),
        other => Err(unexpected("project_agent.created", &other)),
    }
}

async fn ticket_of(app: &App, key: &str) -> Option<Ticket> {
    match app
        .service
        .send(Request::TicketGet(KeyParams {
            key: key.to_owned(),
        }))
        .await
    {
        Ok(Reply::Ticket(t)) => Some(*t),
        _ => None,
    }
}

/// The ticket's stage choices, read only when one of `changes` is a stage's choice (Undo's check needs them).
async fn choices_for<'c>(
    app: &App,
    key: &str,
    mut changes: impl Iterator<Item = &'c WhatChanged>,
) -> Option<TicketChoices> {
    if !changes.any(|c| matches!(c, WhatChanged::StageChoice { .. })) {
        return None;
    }
    match app
        .service
        .send(Request::TicketChoices(TicketChoicesParams {
            key: Some(key.to_owned()),
            project: None,
        }))
        .await
    {
        Ok(Reply::TicketChoices(c)) => Some(*c),
        _ => None,
    }
}

/// The tickets it edited, with their titles and places as the board names them and the changes on each that still
/// read as Noyan left them (their rows have Undo); one that is gone is left out.
async fn edited_of(app: &App, created: &CreatedAndChanged) -> Vec<EditedTicket> {
    let mut edited = Vec::new();
    // Each project's flow and cards' agents: a held ticket is named at the stage it starts at, as on the board.
    type Board = (
        String,
        otaq_core::flow::Flow,
        Option<otaq_core::agents::facts::BoardAgents>,
    );
    let mut flows: Vec<Board> = Vec::new();
    for key in &created.edited {
        if let Some(t) = ticket_of(app, &key.to_string()).await {
            let on_it = || {
                created
                    .changes
                    .iter()
                    .filter(|c| c.ticket == t.key)
                    .map(|c| &c.change)
            };
            let choices = choices_for(app, &key.to_string(), on_it()).await;
            let undo = on_it()
                .filter(|c| vm::still_as_left(c, &t, choices.as_ref()))
                .cloned()
                .collect();
            let project = t.key.project.to_string();
            if !flows.iter().any(|(p, _, _)| *p == project) {
                let (flow, agents) = tokio::join!(
                    super::board::flow(app, &project),
                    super::board::agents(app, &project)
                );
                flows.push((project.clone(), flow, agents));
            }
            let place = flows
                .iter()
                .find(|(p, _, _)| *p == project)
                .map(|(_, flow, agents)| {
                    let columns = otaq_view::view_models::flow_columns::Columns::of(flow)
                        .knowing(agents.as_ref());
                    otaq_view::view_models::dependencies::place_name(&columns, &t)
                })
                .unwrap_or_default();
            edited.push(EditedTicket {
                key: t.key,
                title: t.title,
                place,
                undo,
            });
        }
    }
    edited
}

/// The agent Noyan uses (the app setting): the one every new Noyan conversation starts on unless it picks another.
/// `None` while none is set.
pub(super) async fn project_agent(app: &App) -> Result<Option<Agent>, RequestError> {
    Ok(project_agent_and_list(app).await?.0)
}

/// The agent tools as the agent panel reads them ([`super::editor::agent_tools`]).
pub(super) async fn tools(app: &App) -> otaq_view::view_models::tool_problems::Tools {
    super::editor::agent_tools(app).await
}

/// [`project_agent`] and every agent.
async fn project_agent_and_list(app: &App) -> Result<(Option<Agent>, Vec<Agent>), RequestError> {
    let list = match app.service.send(Request::AgentsList).await? {
        Reply::Agents(list) => *list,
        other => return Err(unexpected("agents.list", &other)),
    };
    let default = list.noyan().cloned();
    Ok((default, list.agents))
}

async fn project_name(app: &App, project: &str) -> Result<String, RequestError> {
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => Ok(list
            .into_iter()
            .find(|p| p.key == project)
            .map_or_else(|| project.to_owned(), |p| p.name)),
        other => Err(unexpected("project.list", &other)),
    }
}

/// What `agent_panel.view` takes: the project, the agent the page shows (none for the empty agent panel) and the window's width
/// (Created and changed folds to one line at the 1280 class).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AgentPanelParams")]
pub struct Params {
    pub project: String,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub window_width: Option<f32>,
    /// The agent picked for this conversation in the empty agent panel: its first message starts on it instead of the
    /// agent Noyan uses.
    #[serde(default)]
    #[ts(optional)]
    pub start_on: Option<String>,
}

pub async fn view(app: &App, params: Params) -> Result<AgentPanelView, RequestError> {
    let agents = agents(app, &params.project).await?;
    let name = project_name(app, &params.project).await?;
    let (new_agent, named) = project_agent_and_list(app).await?;
    let start_on = params
        .start_on
        .as_deref()
        .and_then(|id| named.iter().find(|a| a.id == id));
    let tools = super::editor::agent_tools(app).await;
    let shown = params
        .agent
        .as_deref()
        .filter(|id| agents.open.iter().any(|a| a.id == *id));
    let (created, edited) = match shown {
        Some(id) => {
            let created = created_of(app, id).await?;
            let edited = edited_of(app, &created).await;
            (Some(created), edited)
        }
        None => (None, Vec::new()),
    };
    // A created ticket the user started says where it is or why it waits, as its card does; the board is read only
    // while one has left Backlog.
    let started: Vec<_> = created
        .iter()
        .flat_map(|c| &c.created)
        .filter(|t| t.status.set() != otaq_core::domain::StatusSet::Backlog)
        .map(|t| t.key.clone())
        .collect();
    let whereabouts = if started.is_empty() {
        Vec::new()
    } else {
        super::board::whereabouts(app, &params.project, &started)
            .await
            .unwrap_or_default()
    };
    Ok(vm::view(&vm::Facts {
        agents: &agents,
        shown,
        created: created.as_ref(),
        edited: &edited,
        whereabouts: &whereabouts,
        project_name: &name,
        new_agent: new_agent.as_ref(),
        window_width: params.window_width,
        now_ms: otaq_core::store::now_ms(),
        keys: super::keymap::labels(app).await,
        tools: &tools,
        agent_list: &named,
        start_on,
        project: &params.project,
    }))
}

/// What `agent_panel.send` takes: the empty agent panel's first message.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AgentPanelSendParams")]
pub struct SendParams {
    pub project: String,
    pub message: String,
    /// The empty agent panel's `starts_on`: the agent picked for this conversation; `None` for the agent Noyan uses.
    #[serde(default)]
    #[ts(optional)]
    pub agent: Option<String>,
}

/// Send in the empty agent panel: the message is the go-ahead, and the new agent starts with it as its first prompt
/// (`project_agent.open`, d0087). The new agent's id, which the agent panel shows next.
pub async fn send(app: &App, params: SendParams) -> Result<String, RequestError> {
    match app
        .service
        .send(Request::ProjectAgentOpen(ProjectAgentOpenParams {
            project: params.project,
            message: params.message,
            agent: params.agent,
        }))
        .await
    {
        Ok(Reply::ProjectAgent(agent)) => Ok(agent.id),
        Ok(other) => Err(unexpected("project_agent.open", &other)),
        Err(e) => {
            toast_error(app, "Noyan didn\u{2019}t start", &e);
            Err(e)
        }
    }
}

/// What `agent_panel.rename` takes: the agent and the user's name for it (which always wins, PAG-04).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AgentPanelRenameParams")]
pub struct RenameParams {
    pub id: String,
    pub name: String,
}

/// An empty name changes nothing.
pub async fn rename(app: &App, params: RenameParams) -> Result<(), RequestError> {
    let name = params.name.trim().to_owned();
    if name.is_empty() {
        return Ok(());
    }
    if let Err(e) = app
        .service
        .send(Request::ProjectAgentRename(ProjectAgentRenameParams {
            id: params.id,
            name,
        }))
        .await
    {
        toast_error(app, "The conversation wasn\u{2019}t renamed", &e);
        return Err(e);
    }
    Ok(())
}

/// What `agent_panel.end` takes: the shown agent, and `confirm` after End agent's question.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AgentPanelEndParams")]
pub struct EndParams {
    pub project: String,
    pub id: String,
    #[serde(default)]
    pub confirm: bool,
}

/// How End agent went: done (it moves to Recently ended), or it is mid-turn and the user is asked first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(rename = "AgentPanelEnded")]
pub enum Ended {
    Done,
    /// *Close an agent mid-turn · confirm*: "Stop and close".
    Ask { dialog: CloseDialog },
}

async fn find_agent(app: &App, project: &str, id: &str) -> Result<ProjectAgent, RequestError> {
    let list = agents(app, project).await?;
    list.open
        .into_iter()
        .chain(list.recent)
        .find(|a| a.id == id)
        .ok_or_else(|| RequestError::plain(format!("no such conversation with Noyan: {id}")))
}

pub async fn end(app: &App, params: EndParams) -> Result<Ended, RequestError> {
    match app
        .service
        .send(Request::ProjectAgentClose(ProjectAgentCloseParams {
            id: params.id.clone(),
            confirm: params.confirm,
        }))
        .await
    {
        Ok(_) => Ok(Ended::Done),
        Err(e) if e.refusal.as_deref() == Some("needs_confirmation") => {
            let agent = find_agent(app, &params.project, &params.id).await?;
            let created = created_of(app, &params.id).await?;
            let keys: Vec<_> = created.created.iter().map(|t| t.key.clone()).collect();
            let places = super::board::whereabouts(app, &params.project, &keys)
                .await
                .unwrap_or_default();
            Ok(Ended::Ask {
                dialog: agent_page::close_dialog(&agent, &created, &places),
            })
        }
        Err(e) => {
            toast_error(app, "Noyan didn\u{2019}t end", &e);
            Err(e)
        }
    }
}

/// What `agent_panel.close_ended` takes: the shown project.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AgentPanelProjectParams")]
pub struct ProjectOnly {
    pub project: String,
}

/// … → Close ended agents: they leave the switcher and stay in Recently ended.
pub async fn close_ended(app: &App, params: ProjectOnly) -> Result<(), RequestError> {
    match app
        .service
        .send(Request::ProjectAgentCloseEnded(ProjectParams {
            project: params.project,
        }))
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            toast_error(app, "The ended agents didn\u{2019}t close", &e);
            Err(e)
        }
    }
}

/// What `agent_panel.resume` takes: an ended Project agent (its footer, or Recently ended).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AgentPanelResumeParams")]
pub struct ResumeParams {
    pub id: String,
}

/// Resume (the click is the go-ahead, d0018): its conversation continues. The agent's id, which the agent panel shows.
pub async fn resume(app: &App, params: ResumeParams) -> Result<String, RequestError> {
    match app
        .service
        .send(Request::ProjectAgentResume(IdParams { id: params.id }))
        .await
    {
        Ok(Reply::ProjectAgent(agent)) => Ok(agent.id),
        Ok(other) => Err(unexpected("project_agent.resume", &other)),
        Err(e) => {
            toast_error(app, "Noyan didn\u{2019}t resume", &e);
            Err(e)
        }
    }
}

/// What `agent_panel.answer` takes (OTQ-9): the conversation, its open question, the picked number (Other… is the
/// last; none for an open question), the typed text, and whether the terminal's typed line is cleared first.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AgentAnswerParams")]
pub struct AnswerParams {
    pub project: String,
    pub agent: String,
    pub question: String,
    #[serde(default)]
    #[ts(optional)]
    pub pick: Option<u32>,
    #[serde(default)]
    pub other: String,
    #[serde(default)]
    pub clear: bool,
}

/// Send answer on a Project agent's question, from the strip's popover or the panel (OTQ-9, d0119): typed into its
/// session as the options word it (`project_agent.answer`). Unsent text in its terminal: the user is asked first
/// (d0048), nothing is typed.
pub async fn answer(app: &App, params: AnswerParams) -> Result<super::sheet::Sent, RequestError> {
    use otaq_core::protocol::ProjectAgentAnswerParams;
    use otaq_core::session::deliveries::Delivery;
    use otaq_view::view_models::{agent_question, answer as answer_vm, answer_options};
    let list = agents(app, &params.project).await?;
    let changed = || RequestError::plain("Noyan\u{2019}s question has changed");
    let agent = list
        .open
        .iter()
        .find(|a| a.id == params.agent)
        .ok_or_else(changed)?;
    let question = agent
        .question
        .as_ref()
        .filter(|q| q.id == params.question)
        .ok_or_else(changed)?;
    let choice = answer_options::choice_for(question, params.pick, &params.other)
        .ok_or_else(|| RequestError::plain("Pick an answer or type one"))?;
    let request = Request::ProjectAgentAnswer(ProjectAgentAnswerParams {
        id: agent.id.clone(),
        choice: choice.clone(),
        clear: params.clear,
    });
    match app.service.send(request).await {
        Ok(Reply::Delivery(Delivery::Sent { .. })) => Ok(super::sheet::Sent::Done),
        Ok(Reply::Delivery(Delivery::NeedsConfirmation { unclearable_by, .. })) => {
            Ok(super::sheet::Sent::Ask(super::sheet::Unsent::asking(
                agent_question::unsent_title(&agent.name),
                format!(
                    "{} {}.",
                    otaq_view::view_models::terminal_strip::UnsentPrompt::EXPLANATION,
                    answer_vm::sending(&choice)
                ),
                unclearable_by.as_deref(),
            )))
        }
        Ok(other) => Err(unexpected("project_agent.answer", &other)),
        Err(e) => {
            toast_error(app, "The answer to Noyan wasn\u{2019}t sent", &e);
            Err(e)
        }
    }
}

/// What `agent_panel.undo` takes: a CHANGED row of the shown conversation, by its ticket and what it says changed
/// ("now waits for SHOP-6"), which names one change: each part of a ticket has one row.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AgentPanelUndoParams")]
pub struct UndoParams {
    pub agent: String,
    pub ticket: String,
    pub change: String,
}

/// Undo on a CHANGED row (decision 013): takes Noyan's change back on the ticket through the board's own edit
/// ([`vm::undo_request`]), so History records it as the user's. Only while that part still reads as Noyan left it
/// ([`vm::still_as_left`]), checked again here: Undo never overwrites a later edit. Then the service records the Undo
/// (`project_agent.undone`), so the row says it was undone ("Undone · SHOP-7 no longer waits for SHOP-6") with no Undo;
/// a row whose part someone else changed keeps what it says and only loses Undo.
pub async fn undo(app: &App, params: UndoParams) -> Result<(), RequestError> {
    const TITLE: &str = "Noyan\u{2019}s change wasn\u{2019}t undone";
    let result = async {
        let created = created_of(app, &params.agent).await?;
        let change = created
            .changes
            .iter()
            .find(|c| c.ticket.to_string() == params.ticket && vm::what_changed(&c.change) == params.change)
            .ok_or_else(|| RequestError::plain(format!("Noyan no longer lists this change on {}", params.ticket)))?;
        let ticket = ticket_of(app, &params.ticket)
            .await
            .ok_or_else(|| RequestError::plain(format!("{} is gone", params.ticket)))?;
        let choices = choices_for(app, &params.ticket, std::iter::once(&change.change)).await;
        if !vm::still_as_left(&change.change, &ticket, choices.as_ref()) {
            return Err(RequestError::plain(format!(
                "{} has changed since Noyan changed it; Undo would overwrite that",
                params.ticket
            )));
        }
        let request = vm::undo_request(&ticket.key, &change.change, choices.as_ref())
            .ok_or_else(|| RequestError::plain("There is nothing to put back"))?;
        app.service.send(request).await?;
        app.service
            .send(Request::ProjectAgentUndone(ProjectAgentUndoneParams {
                id: params.agent.clone(),
                ticket: params.ticket.clone(),
                change: change.change.clone(),
            }))
            .await
            .map(|_| ())
    }
    .await;
    if let Err(e) = &result {
        toast_error(app, TITLE, e);
    }
    result
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    #[tokio::test(flavor = "multi_thread")]
    async fn the_empty_agent_panel_starts_nothing_and_send_needs_words() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("pocket-notes", 1);
        let project = match h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
            }))
            .await
            .unwrap()
        {
            Reply::ProjectAdded { project: p, .. } => p.key,
            other => panic!("{other:?}"),
        };

        let view = table(
            &h.app,
            "agent_panel.view",
            json!({ "project": project, "window_width": 1280 }),
        )
        .await
        .unwrap();
        assert_eq!(view["agent"], Value::Null);
        assert_eq!(view["opens_on"], Value::Null);
        assert_eq!(view["empty"]["title"], "Ask Noyan");
        assert_eq!(view["empty"]["hint"], "Enter starts Claude Code");
        assert_eq!(view["empty"]["line"]["agent"], "Claude Code");
        assert!(
            view["empty"]["text"]
                .as_str()
                .unwrap()
                .contains("in pocket-notes but never edits it")
        );
        assert_eq!(view["switcher"]["rows"], json!([]));
        // An agent the page remembers but the project doesn't have: the empty agent panel.
        let gone = table(
            &h.app,
            "agent_panel.view",
            json!({ "project": project, "agent": "gone" }),
        )
        .await
        .unwrap();
        assert_eq!(gone["agent"], Value::Null);

        // Noyan uses Strong now: the next conversation's empty agent panel starts on it.
        let Reply::Agent(strong) = h
            .app
            .service
            .send(Request::AgentsAdd(otaq_core::agents::AgentFields {
                name: "Strong".into(),
                instructions: String::new(),
                model: otaq_core::agents::AgentModel::Tool {
                    tool: otaq_core::agent_tools::Tool::ALL[0],
                    model: None,
                },
                effort: None,
            }))
            .await
            .unwrap()
        else {
            panic!("no agent");
        };
        h.app
            .service
            .send(Request::AgentsNoyan(otaq_core::protocol::NoyanAgentParams {
                agent: strong.id.clone(),
            }))
            .await
            .unwrap();
        let view = table(&h.app, "agent_panel.view", json!({ "project": project }))
            .await
            .unwrap();
        assert_eq!(view["empty"]["line"]["agent"], "Strong");
        assert_eq!(view["empty"]["line"]["note"], "\u{b7} starts when you send");
        assert_eq!(view["empty"]["hint"], "Enter starts Strong");
        let picked: Vec<(&str, bool)> = view["empty"]["pick"]["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| (o["label"].as_str().unwrap(), o["selected"].as_bool().unwrap()))
            .collect();
        assert!(picked.contains(&("Strong", true)) && picked.contains(&("Noyan", false)));
        // A new conversation can pick another agent: the empty agent panel starts on it, for this conversation only.
        let Reply::Agents(list) = h.app.service.send(Request::AgentsList).await.unwrap() else {
            panic!("agents.list");
        };
        let noyan = list.agents.iter().find(|a| a.name == "Noyan").unwrap();
        let view = table(
            &h.app,
            "agent_panel.view",
            json!({ "project": project, "start_on": noyan.id }),
        )
        .await
        .unwrap();
        assert_eq!(view["empty"]["starts_on"], json!(noyan.id));
        let selected: Vec<&str> = view["empty"]["pick"]["options"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["selected"] == true)
            .map(|o| o["label"].as_str().unwrap())
            .collect();
        assert_eq!(selected, ["Noyan"]);
        // Picking the agent Noyan uses is the setting again: nothing to pass on.
        let view = table(
            &h.app,
            "agent_panel.view",
            json!({ "project": project, "start_on": strong.id }),
        )
        .await
        .unwrap();
        assert_eq!(view["empty"]["starts_on"], Value::Null);

        // Send without words is refused; nothing was started.
        assert!(
            table(
                &h.app,
                "agent_panel.send",
                json!({ "project": project, "message": "  " })
            )
            .await
            .is_err()
        );
        let list = agents(&h.app, &project).await.unwrap();
        assert!(list.open.is_empty() && list.recent.is_empty());

        // Close ended agents with none is a no-op; an empty rename changes nothing.
        table(&h.app, "agent_panel.close_ended", json!({ "project": project }))
            .await
            .unwrap();
        table(&h.app, "agent_panel.rename", json!({ "id": "x", "name": " " }))
            .await
            .unwrap();
        // OTQ-9: an answer to a conversation that isn't there (or no longer asks) is refused; nothing is typed.
        let refused = table(
            &h.app,
            "agent_panel.answer",
            json!({ "project": project, "agent": "gone", "question": "q1", "pick": 1, "other": "" }),
        )
        .await
        .unwrap_err();
        assert_eq!(refused.message, "Noyan\u{2019}s question has changed");
        h.service.stop_all().await;
    }

    /// Undo on a CHANGED row (decision 013): "now waits for …" has Undo while the wait is there, and Undo removes it as
    /// the user's edit; the row keeps what it says and loses Undo. A wait the user removed themselves has no Undo, and
    /// Undo sent anyway is refused, so it never puts back what the user did.
    #[tokio::test(flavor = "multi_thread")]
    async fn undo_takes_back_noyans_wait_and_never_a_later_edit() {
        use otaq_core::board::Board;
        use otaq_core::board::tickets::{EventKind, NewTicketInput};
        use otaq_core::domain::Actor;

        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("shop", 1);
        let project = match h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
            }))
            .await
            .unwrap()
        {
            Reply::ProjectAdded { project: p, .. } => p,
            other => panic!("{other:?}"),
        };
        let mut keys = Vec::new();
        for title in ["Checkout API", "Check total", "Coupons"] {
            let Reply::Ticket(t) = h
                .app
                .service
                .send(Request::TicketCreate(NewTicketInput {
                    project: project.key.clone(),
                    title: title.into(),
                    body: String::new(),
                    plan: String::new(),
                    priority: None,
                    base_branch: None,
                    first_comment: None,
                }))
                .await
                .unwrap()
            else {
                panic!("no ticket");
            };
            keys.push(t.key.to_string());
        }
        let (parent, waits, also) = (&keys[0], &keys[1], &keys[2]);

        // Noyan (a conversation's row, its edits made as it) makes two tickets wait for the first.
        let data = h.service.data.clone();
        let store = otaq_core::store::Store::open(&data.database_path())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO project_agents (id, project_id, tool, state, created_at) SELECT 'n1', id, 'claude', 'running', 1 FROM projects WHERE key = ?",
        )
        .bind(&project.key)
        .execute(store.pool())
        .await
        .unwrap();
        let board = Board::new(store, None, data);
        let noyan = Actor::ProjectAgent("n1".into());
        for child in [waits, also] {
            board.add_dependency(child, parent, &noyan).await.unwrap();
        }
        let row_says = format!("now waits for {parent}");
        let undoable = |edited: &[EditedTicket], key: &str| {
            edited
                .iter()
                .find(|t| t.key.to_string() == key)
                .is_some_and(|t| !t.undo.is_empty())
        };
        let created = created_of(&h.app, "n1").await.unwrap();
        let edited = edited_of(&h.app, &created).await;
        assert!(undoable(&edited, waits) && undoable(&edited, also));

        // Undo removes the wait, recorded in History as the user's.
        table(
            &h.app,
            "agent_panel.undo",
            json!({ "agent": "n1", "ticket": waits, "change": row_says }),
        )
        .await
        .unwrap();
        let after = ticket_of(&h.app, waits).await.unwrap();
        assert!(after.depends_on.is_empty());
        let removed = board
            .history(waits)
            .await
            .unwrap()
            .into_iter()
            .find(|e| e.kind == EventKind::DependencyRemoved)
            .unwrap();
        assert!(matches!(removed.actor, Actor::User(_)), "{:?}", removed.actor);
        // The row stays, says it was undone, and has no Undo.
        let created = created_of(&h.app, "n1").await.unwrap();
        let undone = |created: &CreatedAndChanged, key: &str| {
            created
                .changes
                .iter()
                .find(|c| c.ticket.to_string() == key && vm::what_changed(&c.change) == row_says)
                .map(|c| c.undone)
        };
        assert_eq!(undone(&created, waits), Some(true));
        assert_eq!(undone(&created, also), Some(false));
        let edited = edited_of(&h.app, &created).await;
        assert!(!undoable(&edited, waits) && undoable(&edited, also));

        // The user removes the other wait themselves: its Undo is gone, and Undo sent anyway changes nothing.
        h.app
            .service
            .send(Request::DepRemove(otaq_core::protocol::DepParams {
                child: also.clone(),
                parent: parent.clone(),
            }))
            .await
            .unwrap();
        let edited = edited_of(&h.app, &created).await;
        assert!(!undoable(&edited, also));
        // Not by Undo: the row keeps what it says.
        let created = created_of(&h.app, "n1").await.unwrap();
        assert_eq!(undone(&created, also), Some(false));
        let refused = table(
            &h.app,
            "agent_panel.undo",
            json!({ "agent": "n1", "ticket": also, "change": row_says }),
        )
        .await
        .unwrap_err();
        assert!(refused.message.contains("has changed since Noyan changed it"));
        assert!(ticket_of(&h.app, also).await.unwrap().depends_on.is_empty());
        h.service.stop_all().await;
    }
}

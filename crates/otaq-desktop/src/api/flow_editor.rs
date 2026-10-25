//! `flow_editor.*`: the flow editor (T-0471; FED-01, FED-03, FED-04, FED-07; d0115, d0087; `s27-02`…`s27-07`).
//! `flow_editor.view` answers everything the editor draws for a project (`otaq-view`'s `flow_editor.rs`) from the
//! project's draft (`flow.draft`, T-0470), its tools and proposals (`onboarding.flow_setup`), the agents and the panel's
//! Project agent. The others are its clicks, and **every one of the user's edits goes through `flow.draft_update`**:
//! `flow_editor.edit` (a step's name, kind, agent, options, a move, an added or removed stage: the whole document),
//! `flow_editor.pick` (Start from), `flow_editor.undo` (one stage the agent changed, or all) and `flow_editor.import`
//! (a file). `flow_editor.apply` and `flow_editor.cancel` are the footer's buttons (Apply · Cancel in the project, Use
//! this flow · Skip, use the default in onboarding). `flow_editor.send` and `flow_editor.ask` are the agent panel's:
//! **nothing starts before the user's words, a chip or an Ask** (d0087); the first message starts the project's
//! Project agent on the `flow` skill (`/flow …`, through `agent_panel.send`).
//!
//! The Skill field (T-0472; FED-05, FED-06): a template and Pick a skill… are edits (`flow_editor.edit`). Ask the
//! agent to write it (and the Write the skill chip) **sends nothing** (Q441): the page puts "Write a skill for the
//! Plan stage: " in the idle panel's box, or `flow_editor.prefill` types it into the running agent's input line
//! without Return; `flow_editor.write_skill` is the user's send from the box (the Project agent starts with their
//! words, and only then the field says it is being written; a running agent's field says so once it asks for its
//! folder). `flow_editor.stop_writing` is Stop (it interrupts the agent's turn), `flow_editor.undo_skill` the written
//! skill's Undo (Otaq's copy goes) and `flow_editor.open_skill` its Open (Otaq's copy in the file manager).
//!
//! Problems that need a pick (Q426): `flow_editor.view` takes the page's picks (`choices`) and `flow_editor.apply`
//! sends them with the draft (`flow.apply`'s `ApplyChoices`, as FLW-04's preview did).

use std::collections::BTreeMap;

use otaq_core::agent_tools::Tool;
use otaq_core::agents::{Agent, SkillRef};
use otaq_core::flow::apply::{ApplyParams, ProposeFrom};
use otaq_core::flow::draft::{Draft, DraftEdit, DraftUpdateParams};
use otaq_core::flow::model::FlowFile;
use otaq_core::flow::yaml;
use otaq_core::project_agents::first_flow::FlowSetup;
use otaq_core::project_agents::{AgentState, FLOW_COMMAND};
use otaq_core::protocol::{
    InputParams, OnboardingAskParams, ProjectParams, Reply, Request,
    encode_bytes,
};
use otaq_view::view_models::flow_editor::{self as vm, Choices, Opened, Pick, StageEdit};
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "flow_editor.view" => reply(&view(app, parse(params)?).await?),
        "flow_editor.edit" => reply(&edit(app, parse(params)?).await?),
        "flow_editor.pick" => reply(&pick(app, parse(params)?).await?),
        "flow_editor.undo" => reply(&undo(app, parse(params)?).await?),
        "flow_editor.import" => reply(&import(app, parse(params)?).await?),
        "flow_editor.apply" => reply(&apply(app, parse(params)?).await?),
        "flow_editor.cancel" => reply(&cancel(app, parse(params)?).await?),
        "flow_editor.send" => reply(&send(app, parse(params)?).await?),
        "flow_editor.ask" => reply(&ask(app, parse(params)?).await?),
        "flow_editor.write_skill" => reply(&write_skill(app, parse(params)?).await?),
        "flow_editor.prefill" => reply(&prefill(app, parse(params)?).await?),
        "flow_editor.stop_writing" => reply(&stop_writing(app, parse(params)?).await?),
        "flow_editor.undo_skill" => reply(&undo_skill(app, parse(params)?).await?),
        "flow_editor.open_skill" => reply(&open_skill(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(format!("bad parameters: {e}")))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected reply to {what}: {other:?}"))
}

fn toast_error(app: &App, title: &str, error: &RequestError) {
    app.notices.toast(NewToast::new(
        ToastLevel::Error,
        title,
        Some(error.message.clone()),
    ));
}

#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    ViewParams::export_all(cfg)?;
    EditParams::export_all(cfg)?;
    PickParams::export_all(cfg)?;
    UndoParams::export_all(cfg)?;
    ImportParams::export_all(cfg)?;
    ProjectFrom::export_all(cfg)?;
    SendParams::export_all(cfg)?;
    AskParams::export_all(cfg)?;
    SkillParams::export_all(cfg)?;
    WriteParams::export_all(cfg)?;
    Edited::export_all(cfg)?;
    Started::export_all(cfg)?;
    vm::View::export_all(cfg)?;
    vm::Gallery::export_all(cfg)?;
    Ok(())
}

/// What `flow_editor.view` takes: the project, where it opened, the selected step and the panel's agent (both the
/// page's own state).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorParams")]
pub struct ViewParams {
    pub project: String,
    pub from: Opened,
    #[serde(default)]
    #[ts(optional)]
    pub selected: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub agent: Option<String>,
    /// The user's picks for the draft's problems so far (Q426).
    #[serde(default)]
    #[ts(optional)]
    pub choices: Option<Choices>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorEditParams")]
pub struct EditParams {
    pub project: String,
    pub edit: StageEdit,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorPickParams")]
pub struct PickParams {
    pub project: String,
    pub pick: Pick,
}

/// Undo on a tinted step (`stage`), or Undo all (none).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorUndoParams")]
pub struct UndoParams {
    pub project: String,
    #[serde(default)]
    #[ts(optional)]
    pub stage: Option<String>,
}

/// Import a flow… → A file…: the picked file's path.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorImportParams")]
pub struct ImportParams {
    pub project: String,
    pub path: String,
}

/// Apply and Cancel: the project, where the editor opened, the draft Apply applies and the picks it sends (the view's
/// `choices`).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorProjectFrom")]
pub struct ProjectFrom {
    pub project: String,
    pub from: Opened,
    #[serde(default)]
    #[ts(optional)]
    pub proposal: Option<u32>,
    #[serde(default)]
    #[ts(optional)]
    pub choices: Option<Choices>,
}

/// The panel's first message: typed, or a chip's words.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorSendParams")]
pub struct SendParams {
    pub project: String,
    pub text: String,
}

/// An Ask under Made for <project>, the chooser's tool, or "Ask Claude Code".
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorAskParams")]
pub struct AskParams {
    pub project: String,
    pub from: Opened,
    #[ts(type = "string")]
    pub tool: Tool,
}

/// The Skill field's buttons on a stage: Ask the agent to write it on a running agent (`flow_editor.prefill`, the
/// panel's agent), Stop, Undo, Open.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorSkillParams")]
pub struct SkillParams {
    pub project: String,
    pub stage: String,
    #[serde(default)]
    #[ts(optional)]
    pub agent: Option<String>,
}

/// The user's send from the idle panel's box that Ask the agent to write it prefilled: their words, for `stage`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorWriteParams")]
pub struct WriteParams {
    pub project: String,
    pub stage: String,
    pub text: String,
}

/// After an edit: the step to select (a stage just added), if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowEditorEdited")]
pub struct Edited {
    pub selected: Option<String>,
}

/// The agent the panel now shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowEditorStarted")]
pub struct Started {
    pub agent: String,
}

async fn draft(app: &App, project: &str) -> Result<Draft, RequestError> {
    match app
        .service
        .send(Request::FlowDraft(ProjectParams {
            project: project.to_owned(),
        }))
        .await?
    {
        Reply::FlowDraft(draft) => Ok(*draft),
        other => Err(unexpected("flow.draft", &other)),
    }
}

async fn update(app: &App, project: &str, edit: DraftEdit) -> Result<Draft, RequestError> {
    let request = Request::FlowDraftUpdate(DraftUpdateParams {
        project: project.to_owned(),
        edit,
    });
    match app.service.send(request).await {
        Ok(Reply::FlowDraft(draft)) => Ok(*draft),
        Ok(other) => Err(unexpected("flow.draft_update", &other)),
        Err(e) => {
            toast_error(app, "The flow didn\u{2019}t change", &e);
            Err(e)
        }
    }
}

async fn setup(app: &App, project: &str) -> Result<FlowSetup, RequestError> {
    match app
        .service
        .send(Request::OnboardingFlowSetup(ProjectParams {
            project: project.to_owned(),
        }))
        .await?
    {
        Reply::FlowSetup(setup) => Ok(*setup),
        other => Err(unexpected("onboarding.flow_setup", &other)),
    }
}

/// The flow in use now (the In use entry); the default flow before one was applied.
async fn in_use(app: &App, project: &str) -> FlowFile {
    let current = app
        .service
        .send(Request::FlowCurrent(ProjectParams {
            project: project.to_owned(),
        }))
        .await;
    match current {
        Ok(Reply::FlowVersion(v)) => v.file().ok(),
        _ => None,
    }
    .unwrap_or_else(|| otaq_core::flow::presets::presets()[0].read())
}

/// Every project's key and name.
async fn projects(app: &App) -> Result<Vec<(String, String)>, RequestError> {
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => Ok(list.into_iter().map(|p| (p.key, p.name)).collect()),
        other => Err(unexpected("project.list", &other)),
    }
}

/// A SKILL.md's `description:` (its front matter's, folded onto one line), if it has one.
fn description(text: &str) -> Option<String> {
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return None;
    }
    let mut found: Option<String> = None;
    for line in lines {
        if line.trim_end() == "---" {
            break;
        }
        match (&mut found, line.starts_with([' ', '\t'])) {
            (Some(text), true) => {
                text.push(' ');
                text.push_str(line.trim());
            }
            (Some(_), false) => break,
            (None, _) => {
                if let Some(value) = line.strip_prefix("description:") {
                    found = Some(value.trim().trim_matches(['"', '\'']).to_owned());
                }
            }
        }
    }
    found.filter(|d| !d.is_empty() && d != ">" && d != "|")
}

/// The captions of the user's imported skills (d0115): each one's SKILL.md description, by the skill's name, read
/// from Otaq's own copy.
async fn captions(app: &App, agents: &[Agent]) -> BTreeMap<String, String> {
    let Some(data) = app.service.data() else {
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    for agent in agents {
        for skill in &agent.skills {
            let SkillRef::Imported(id) = skill else {
                continue;
            };
            let Some(record) = agent.imported_skill(id) else {
                continue;
            };
            let Ok(folder) = data.skill_path(id) else {
                continue;
            };
            if let Ok(text) = tokio::fs::read_to_string(folder.join("SKILL.md")).await
                && let Some(text) = description(&text)
            {
                out.insert(record.name.clone(), text);
            }
        }
    }
    out
}

pub async fn view(app: &App, params: ViewParams) -> Result<vm::View, RequestError> {
    let draft = draft(app, &params.project).await?;
    let file = yaml::read(&draft.proposal.yaml)
        .map_err(|e| RequestError::plain(format!("The draft doesn\u{2019}t read: {e}")))?;
    let in_use = in_use(app, &params.project).await;
    let setup = setup(app, &params.project).await?;
    let stored = super::settings::agent_list(app).await?.agents;
    let tools = super::agent_panel::tools(app).await;
    let others: Vec<(String, String)> = projects(app)
        .await?
        .into_iter()
        .filter(|(key, _)| *key != params.project)
        .collect();
    let captions = captions(app, &stored).await;
    let agents = super::agent_panel::agents(app, &params.project).await?;
    let agent = params
        .agent
        .as_deref()
        .and_then(|id| agents.open.iter().find(|a| a.id == id));
    let new_agent = super::agent_panel::project_agent(app).await?;
    let keys = super::keymap::labels(app).await;
    Ok(vm::view(&vm::Facts {
        project: &params.project,
        project_name: &setup.project_name,
        opened: params.from,
        proposal: &draft.proposal,
        marks: &draft.marks,
        can_undo_all: draft.can_undo_all,
        swap: draft.swap,
        applies: &draft.applies,
        file: &file,
        in_use: &in_use,
        stored: &stored,
        tools: &tools,
        setup: &setup,
        projects: &others,
        captions: &captions,
        writing: &draft.writing,
        written: &draft.written,
        selected: params.selected.as_deref(),
        agent,
        new_agent: new_agent.as_ref(),
        keys: &keys,
        choices: &params.choices.unwrap_or_default(),
    }))
}

/// A step's edit: the draft's document with it, sent whole (`flow.draft_update`, as the CLI sends one).
async fn edit(app: &App, params: EditParams) -> Result<Edited, RequestError> {
    let draft = draft(app, &params.project).await?;
    let file = yaml::read(&draft.proposal.yaml)
        .map_err(|e| RequestError::plain(format!("The draft doesn\u{2019}t read: {e}")))?;
    let stored = super::settings::agent_list(app).await?.agents;
    let (file, selected) = vm::edited(&file, &params.edit, &stored).map_err(|why| {
        let e = RequestError::plain(why);
        toast_error(app, "The flow didn\u{2019}t change", &e);
        e
    })?;
    update(
        app,
        &params.project,
        DraftEdit::Document {
            yaml: yaml::write(&file),
        },
    )
    .await?;
    Ok(Edited { selected })
}

/// Start from: a preset, a tool's proposal, another project's flow, or back to the flow in use.
async fn pick(app: &App, params: PickParams) -> Result<Edited, RequestError> {
    let edit = match params.pick {
        Pick::FlowInUse => DraftEdit::FlowInUse,
        Pick::Preset { file } => DraftEdit::StartFrom {
            source: ProposeFrom::Preset { file },
        },
        Pick::Entry { proposal } => DraftEdit::Entry { proposal },
        Pick::Project { project } => DraftEdit::StartFrom {
            source: ProposeFrom::Project { project },
        },
    };
    update(app, &params.project, edit).await?;
    Ok(Edited { selected: None })
}

async fn undo(app: &App, params: UndoParams) -> Result<Edited, RequestError> {
    let edit = match params.stage {
        Some(stage) => DraftEdit::UndoStage { stage },
        None => DraftEdit::UndoAll,
    };
    update(app, &params.project, edit).await?;
    Ok(Edited { selected: None })
}

/// Import a flow… → A file…: the file's flow copied into the draft.
async fn import(app: &App, params: ImportParams) -> Result<Edited, RequestError> {
    let source = super::settings::read_flow_file(&params.path)
        .await
        .inspect_err(|e| toast_error(app, "The flow wasn\u{2019}t imported", e))?;
    update(app, &params.project, DraftEdit::StartFrom { source }).await?;
    Ok(Edited { selected: None })
}

/// Apply (the project) or Use this flow (onboarding): the draft applies; in onboarding an untouched draft keeps the
/// default, as Skip does.
async fn apply(app: &App, params: ProjectFrom) -> Result<(), RequestError> {
    let proposal = params.proposal.unwrap_or(0);
    if proposal == 0 {
        return match params.from {
            Opened::Onboarding => use_default(app, &params.project).await,
            Opened::Project => Err(RequestError::plain("Nothing changed")),
        };
    }
    let request = Request::FlowApply(ApplyParams {
        project: params.project,
        proposal,
        choices: params.choices.unwrap_or_default().apply_choices(),
    });
    match app.service.send(request).await {
        Ok(Reply::FlowApplied(_)) => Ok(()),
        Ok(other) => Err(unexpected("flow.apply", &other)),
        Err(e) => {
            toast_error(app, "The flow wasn\u{2019}t applied", &e);
            Err(e)
        }
    }
}

async fn use_default(app: &App, project: &str) -> Result<(), RequestError> {
    app.service
        .send(Request::OnboardingUseDefault(ProjectParams {
            project: project.to_owned(),
        }))
        .await?;
    Ok(())
}

/// Cancel drops the draft (the project); Skip, use the default keeps the default flow (onboarding).
async fn cancel(app: &App, params: ProjectFrom) -> Result<(), RequestError> {
    match params.from {
        Opened::Onboarding => use_default(app, &params.project).await,
        Opened::Project if params.proposal.unwrap_or(0) == 0 => Ok(()),
        Opened::Project => {
            match app
                .service
                .send(Request::FlowCancelProposal(ProjectParams {
                    project: params.project,
                }))
                .await
            {
                Ok(_) => Ok(()),
                // Gone meanwhile (applied or cancelled elsewhere): nothing left to drop.
                Err(e) if e.not_found => Ok(()),
                Err(e) => Err(e),
            }
        }
    }
}

/// The first message, on the `flow` skill: `/flow <the user's words>`.
pub fn flow_message(text: &str) -> String {
    format!("{FLOW_COMMAND} {}", text.trim())
}

/// The panel's first message (typed, or a chip): the project's Project agent starts on the `flow` skill with it.
async fn send(app: &App, params: SendParams) -> Result<Started, RequestError> {
    if params.text.trim().is_empty() {
        return Err(RequestError::plain(
            otaq_core::project_agents::EMPTY_MESSAGE,
        ));
    }
    let agent = super::agent_panel::send(
        app,
        super::agent_panel::SendParams {
            project: params.project,
            message: flow_message(&params.text),
            agent: None,
        },
    )
    .await?;
    Ok(Started { agent })
}

/// What a Made for Ask asks in the project (Q-A): the same look as Flow setup's, landing in the draft.
pub fn make_one_message(project_name: &str, tool: Tool) -> String {
    flow_message(&format!(
        "Make a flow for {project_name}. Look at how I work with {tool}: your own history on this Mac (find it \
         yourself; read only what I typed, never login or credential data) and this repo. Then propose a whole flow.",
        tool = tool.display_name(),
    ))
}

/// Ask <Tool>: in onboarding the tool's flow agent (`onboarding.ask`, its proposal its own Made for entry); in the
/// project a Project agent on an agent that runs on that tool (the Project agent role's when it does), whose
/// proposal lands in the draft.
async fn ask(app: &App, params: AskParams) -> Result<Started, RequestError> {
    if params.from == Opened::Onboarding {
        let request = Request::OnboardingAsk(OnboardingAskParams {
            project: params.project,
            tool: params.tool,
        });
        return match app.service.send(request).await {
            Ok(Reply::ProjectAgent(agent)) => Ok(Started { agent: agent.id }),
            Ok(other) => Err(unexpected("onboarding.ask", &other)),
            Err(e) => {
                toast_error(app, "The tool wasn\u{2019}t asked", &e);
                Err(e)
            }
        };
    }
    let tools = super::agent_panel::tools(app).await;
    let default = tools.default_model.as_ref();
    let runs_on = |a: &Agent| a.model.on(default).is_some_and(|on| on.tool == params.tool);
    let role = super::agent_panel::project_agent(app).await?;
    let agents = super::settings::agent_list(app).await?.agents;
    let on = match role.filter(|a| runs_on(a)) {
        Some(_) => None,
        None => Some(agents.iter().find(|a| runs_on(a)).ok_or_else(|| {
            let e = RequestError::plain(format!(
                "No agent runs on {}: add one in Settings \u{2192} Agents",
                params.tool.display_name()
            ));
            toast_error(app, "The tool wasn\u{2019}t asked", &e);
            e
        })?),
    };
    let name = setup(app, &params.project).await?.project_name;
    let agent = super::agent_panel::send(
        app,
        super::agent_panel::SendParams {
            project: params.project,
            message: make_one_message(&name, params.tool),
            agent: on.map(|a| a.id.clone()),
        },
    )
    .await?;
    Ok(Started { agent })
}

/// The user's send from the box Ask the agent to write it (or the Write the skill chip) prefilled (FED-06, Q441): the
/// project's Project agent starts on the `flow` skill with the user's words, and the field says it is being written by
/// that agent. Nothing reaches an agent before this.
async fn write_skill(app: &App, params: WriteParams) -> Result<Started, RequestError> {
    if params.text.trim().is_empty() {
        return Err(RequestError::plain(
            otaq_core::project_agents::EMPTY_MESSAGE,
        ));
    }
    let agent = super::agent_panel::send(
        app,
        super::agent_panel::SendParams {
            project: params.project.clone(),
            message: flow_message(&params.text),
            agent: None,
        },
    )
    .await?;
    update(
        app,
        &params.project,
        DraftEdit::SkillWriting {
            stage: params.stage,
            agent: Some(agent.clone()),
        },
    )
    .await?;
    Ok(Started { agent })
}

/// Ask the agent to write it while the panel's agent runs (FED-06, Q441): "Write a skill for the Plan stage: " is
/// typed into its input line, without Return, only when that line is empty (refused with a toast over unsent text);
/// the user finishes it and sends in its terminal. The field says it is
/// being written once the agent asks for its folder (`otaq flow write-skill`).
async fn prefill(app: &App, params: SkillParams) -> Result<(), RequestError> {
    let draft = draft(app, &params.project).await?;
    let file = yaml::read(&draft.proposal.yaml)
        .map_err(|e| RequestError::plain(format!("The draft doesn\u{2019}t read: {e}")))?;
    let open = super::agent_panel::agents(app, &params.project).await?.open;
    let session = params
        .agent
        .and_then(|id| open.into_iter().find(|a| a.id == id))
        .filter(|a| a.state == AgentState::Running)
        .and_then(|a| a.session.map(|s| s.id))
        .ok_or_else(|| RequestError::plain("Noyan isn\u{2019}t running"))?;
    type_if_empty(app, session, &vm::write_skill_prefill(&file, &params.stage)).await
}

/// `text` typed into `session`'s input line without Return, never over the user's unsent text (SDD §2, d0048): the
/// service then types nothing and refuses, which a toast says.
async fn type_if_empty(app: &App, session: String, text: &str) -> Result<(), RequestError> {
    app.service
        .send(Request::SessionInput(InputParams {
            session,
            data: encode_bytes(text.as_bytes()),
            reply: false,
            if_empty: true,
        }))
        .await
        .inspect_err(|e| toast_error(app, "Not typed", e))?;
    Ok(())
}

/// Stop on a skill being written: the field is empty again, the agent's folder goes, and the agent writing it (the
/// draft's) is interrupted (Esc) when it is mid-turn.
async fn stop_writing(app: &App, params: SkillParams) -> Result<(), RequestError> {
    let before = draft(app, &params.project).await?;
    let file = yaml::read(&before.proposal.yaml)
        .map_err(|e| RequestError::plain(format!("The draft doesn\u{2019}t read: {e}")))?;
    let writer = file.flow.stage(&params.stage).and_then(|stage| {
        before
            .writing
            .iter()
            .find(|w| w.stage_id == stage.id)
            .and_then(|w| w.agent.clone())
    });
    update(
        app,
        &params.project,
        DraftEdit::StopWriting {
            stage: params.stage.clone(),
        },
    )
    .await?;
    let open = super::agent_panel::agents(app, &params.project).await?.open;
    let working = writer
        .and_then(|id| open.into_iter().find(|a| a.id == id))
        .filter(|a| a.state == AgentState::Running && a.mid_turn)
        .and_then(|a| a.session.map(|s| s.id));
    if let Some(session) = working {
        app.service
            .send(Request::SessionInput(InputParams {
                session,
                data: encode_bytes(b"\x1b"),
                reply: false,
                if_empty: false,
            }))
            .await?;
    }
    Ok(())
}

/// Undo on the skill the agent wrote: the stage's skill as before, and Otaq's copy deleted.
async fn undo_skill(app: &App, params: SkillParams) -> Result<Edited, RequestError> {
    update(
        app,
        &params.project,
        DraftEdit::UndoSkill {
            stage: params.stage,
        },
    )
    .await?;
    Ok(Edited { selected: None })
}

/// Open on the skill the agent wrote: Otaq's copy in the file manager (only a copy the draft names).
async fn open_skill(app: &App, params: SkillParams) -> Result<(), RequestError> {
    let draft = draft(app, &params.project).await?;
    let file = yaml::read(&draft.proposal.yaml)
        .map_err(|e| RequestError::plain(format!("The draft doesn\u{2019}t read: {e}")))?;
    let stage = file
        .flow
        .stage(&params.stage)
        .ok_or_else(|| RequestError::plain(format!("No stage {}", params.stage)))?;
    let written = draft
        .written
        .iter()
        .find(|w| w.stage_id == stage.id)
        .ok_or_else(|| RequestError::plain(format!("The agent wrote no skill for {}", stage.name)))?;
    let data = app
        .service
        .data()
        .ok_or_else(|| RequestError::plain("no data folder"))?;
    let folder = data
        .skill_path(&written.copy)
        .map_err(|e| RequestError::plain(e.to_string()))?;
    open::that_detached(&folder).map_err(|e| RequestError::plain(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    #[test]
    fn a_skills_description_is_its_front_matter_line() {
        let text = "---\nname: security-review\ndescription: A security agent reviews the change\n  for risky code.\n---\n# Body";
        assert_eq!(
            description(text).as_deref(),
            Some("A security agent reviews the change for risky code.")
        );
        assert_eq!(description("---\nname: x\n---\n"), None);
        assert_eq!(description("no front matter"), None);
    }

    #[test]
    fn the_first_message_runs_the_flow_skill() {
        assert_eq!(flow_message("  What\u{2019}s Validate? "), "/flow What\u{2019}s Validate?");
        assert_eq!(
            otaq_core::project_agents::first_skill(&make_one_message("shop-web", Tool::ALL[0])).0,
            "flow"
        );
    }

    async fn shop_web(h: &Harness, repos: &TempRepos) {
        h.app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: repos.repo("shop-web", 1).display().to_string(),
                ..Default::default()
            }))
            .await
            .unwrap();
    }

    async fn load(h: &Harness, selected: Option<&str>) -> vm::View {
        view(
            &h.app,
            ViewParams {
                project: "SHOP".into(),
                from: Opened::Project,
                selected: selected.map(str::to_owned),
                agent: None,
                choices: None,
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn edits_go_through_the_draft_and_cancel_drops_it() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;

        // Opening the editor stores nothing and starts nothing.
        let v = request(&h.app, "flow_editor.view", json!({ "project": "SHOP", "from": "project" }))
            .await
            .unwrap();
        assert_eq!(v["proposal"], 0);
        assert_eq!(v["agent"]["pill"]["label"], "Not started");
        assert_eq!(v["footer"]["apply"], "No changes");
        let agents = super::super::agent_panel::agents(&h.app, "SHOP").await.unwrap();
        assert!(agents.open.is_empty(), "no agent starts when the editor opens");

        // Add a stage after Dev: the draft holds it, selected.
        let edited = request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "add", "after": "dev" } }),
        )
        .await
        .unwrap();
        assert_eq!(edited["selected"], "new-stage");
        let v = request(
            &h.app,
            "flow_editor.view",
            json!({ "project": "SHOP", "from": "project", "selected": "new-stage" }),
        )
        .await
        .unwrap();
        assert_ne!(v["proposal"], 0);
        assert_eq!(v["flow"]["steps"][2]["name"], "New stage");
        assert_eq!(v["flow"]["details"]["title"], "New stage");
        assert_eq!(v["footer"]["apply"], "Apply to shop-web");
        assert_eq!(v["flow"]["summary"], "6 steps \u{b7} 1 change, not applied");

        // Rename it, then Cancel: the draft is gone.
        request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "rename", "stage": "new-stage", "name": "Plan" } }),
        )
        .await
        .unwrap();
        let v = load(&h, None).await;
        assert_eq!(v.flow.steps[2].name, "Plan");
        request(
            &h.app,
            "flow_editor.cancel",
            json!({ "project": "SHOP", "from": "project", "proposal": v.proposal }),
        )
        .await
        .unwrap();
        let v = load(&h, None).await;
        assert_eq!(v.proposal, 0);
        assert_eq!(v.flow.steps.len(), 5);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_preset_picked_then_applied_becomes_the_flow() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        request(
            &h.app,
            "flow_editor.pick",
            json!({ "project": "SHOP", "pick": { "pick": "preset", "file": "grill-first.yaml" } }),
        )
        .await
        .unwrap();
        let v = load(&h, None).await;
        assert!(v.start_from.entries.iter().any(|e| e.name == "Grill first" && e.chosen));
        assert_eq!(v.flow.steps[1].name, "Grill");
        request(
            &h.app,
            "flow_editor.apply",
            json!({ "project": "SHOP", "from": "project", "proposal": v.proposal }),
        )
        .await
        .unwrap();
        let v = load(&h, None).await;
        assert_eq!(v.proposal, 0);
        assert_eq!(v.flow.summary, "6 steps \u{b7} in use");
    }

    /// FED-05, FED-06: Plan picked in a new stage keeps Apply off ("Plan needs a skill"); one of the user's imported
    /// skills picked for it turns Apply on, and applying gives it to the stage's agent.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_new_stage_waits_for_a_skill_and_takes_a_picked_one() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let added = request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "add", "after": "dev" } }),
        )
        .await
        .unwrap();
        let picked = request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "template", "stage": added["selected"], "template": "plan" } }),
        )
        .await
        .unwrap();
        assert_eq!(picked["selected"], "plan");
        let v = load(&h, Some("plan")).await;
        let details = v.flow.details.as_ref().unwrap();
        assert_eq!(
            details.templates.as_ref().map(|t| t.title.as_str()),
            Some("New stage after Dev")
        );
        assert!(matches!(
            details.skill.as_ref().map(|s| &s.state),
            Some(vm::SkillState::Empty { .. })
        ));
        assert!(!v.footer.apply_enabled);
        assert_eq!(v.footer.reason.as_deref(), Some("Plan needs a skill"));

        // The user's own skill, imported on the Validator.
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join("SKILL.md"),
            "---\nname: plan-steps\ndescription: Lists the steps first.\n---\n# Plan\n",
        )
        .unwrap();
        let agents = super::super::settings::agent_list(&h.app).await.unwrap().agents;
        let validator = agents.iter().find(|a| a.name == "Validator").unwrap();
        h.app
            .service
            .send(Request::AgentsImportSkill(
                otaq_core::protocol::ImportSkillParams {
                    id: validator.id.clone(),
                    folder: folder.path().to_owned(),
                },
            ))
            .await
            .unwrap();
        let v = load(&h, Some("plan")).await;
        let picker = &v.flow.details.as_ref().unwrap().skill.as_ref().unwrap().picker;
        let names: Vec<&str> = picker.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(names, ["plan-steps"]);
        assert_eq!(picker.items[0].hint.as_deref(), Some("Lists the steps first."));

        request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "skill", "stage": "plan", "skill": "plan-steps" } }),
        )
        .await
        .unwrap();
        let v = load(&h, Some("plan")).await;
        assert_eq!(
            v.flow.details.as_ref().unwrap().skill.as_ref().map(|s| &s.state),
            Some(&vm::SkillState::Set {
                skill: "plan-steps".into()
            })
        );
        assert!(v.footer.apply_enabled, "{:?}", v.footer.reason);
        request(
            &h.app,
            "flow_editor.apply",
            json!({ "project": "SHOP", "from": "project", "proposal": v.proposal }),
        )
        .await
        .unwrap();
        let agents = super::super::settings::agent_list(&h.app).await.unwrap().agents;
        let developer = agents.iter().find(|a| a.name == "Developer").unwrap();
        assert!(
            developer.imported.iter().any(|s| s.name == "plan-steps"),
            "the stage's agent runs it: {developer:?}"
        );
    }

    /// Q426 (the ticket's hand check, automated): a flow file naming a skill that isn't on this machine marks Security
    /// with "Import its folder…" and keeps Apply off; the folder picked (the page's choices) clears it, and Apply
    /// imports it onto the file's agent.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_missing_skills_folder_picked_in_the_step_then_applied() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("team.yaml");
        std::fs::write(
            &file,
            "agents:\n  - name: Security reviewer\n    skills: [security-review]\n    model: default\nflow:\n  - \
             name: Dev\n    kind: agent\n    agent: Developer\n    skill: work-ticket\n  - name: Security\n    \
             kind: check\n    by: agent\n    agent: Security reviewer\n    skill: security-review\n    rounds: 2\n  \
             - name: Review\n    kind: check\n    by: you\n  - name: Done\n    kind: delivery\n    column: \
             with-previous\n    agent: Developer\n    mode: click\n    delivers: either\n",
        )
        .unwrap();
        request(
            &h.app,
            "flow_editor.import",
            json!({ "project": "SHOP", "path": file.display().to_string() }),
        )
        .await
        .unwrap();
        let view_with = |choices: Value| {
            let app = &h.app;
            async move {
                let v = request(
                    app,
                    "flow_editor.view",
                    // Security takes the place (and the id) of Validate, whose name the file drops.
                    json!({ "project": "SHOP", "from": "project", "selected": "validate", "choices": choices }),
                )
                .await
                .unwrap();
                serde_json::from_value::<Value>(v).unwrap()
            }
        };
        let v = view_with(json!({})).await;
        assert_eq!(v["flow"]["steps"][2]["name"], "Security");
        assert_eq!(
            v["flow"]["steps"][2]["problem"],
            "security-review isn\u{2019}t on this machine"
        );
        let fix = &v["flow"]["details"]["fixes"][0];
        assert_eq!(fix["control"]["kind"], "folder");
        assert_eq!(fix["control"]["label"], "Import its folder\u{2026}");
        assert_eq!(v["footer"]["apply_enabled"], false);

        let folder = otaq_core::skills::testing::skill_folder(dir.path(), "security-review");
        let picks = json!({ "skill_folders": { "security-review": folder.display().to_string() } });
        let v = view_with(picks.clone()).await;
        assert_eq!(v["flow"]["steps"][2]["problem"], Value::Null);
        assert_eq!(v["flow"]["details"]["fixes"][0]["fixed"], true);
        assert_eq!(v["footer"]["apply_enabled"], true, "{}", v["footer"]);
        request(
            &h.app,
            "flow_editor.apply",
            json!({ "project": "SHOP", "from": "project", "proposal": v["proposal"], "choices": v["choices"] }),
        )
        .await
        .unwrap();
        let agents = super::super::settings::agent_list(&h.app).await.unwrap().agents;
        let reviewer = agents.iter().find(|a| a.name == "Security reviewer").unwrap();
        assert!(
            reviewer.imported.iter().any(|s| s.name == "security-review"),
            "{reviewer:?}"
        );
    }

    /// Q441: Ask the agent to write it sends nothing by itself. On a running agent it only types into the input line
    /// (refused with none running); the user's send from the box is what starts one, and an empty send starts none.
    #[tokio::test(flavor = "multi_thread")]
    async fn writing_a_skill_waits_for_the_users_send() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let refused = request(
            &h.app,
            "flow_editor.prefill",
            json!({ "project": "SHOP", "stage": "dev", "agent": "nobody" }),
        )
        .await;
        assert!(refused.is_err());
        let refused = request(
            &h.app,
            "flow_editor.write_skill",
            json!({ "project": "SHOP", "stage": "dev", "text": "  " }),
        )
        .await;
        assert!(refused.is_err());
        let agents = super::super::agent_panel::agents(&h.app, "SHOP").await.unwrap();
        assert!(agents.open.is_empty(), "nothing started");
        let v = load(&h, None).await;
        assert_eq!(v.proposal, 0, "the field isn't marked as being written");
    }

    /// Q441, SDD §2 (d0048): the prefill types into an empty input line, and over the user's unsent text it types
    /// nothing and says so in a toast.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_prefill_never_types_over_unsent_text() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let Reply::ProjectTerminal(terminal) = h
            .app
            .service
            .send(Request::ProjectTerminalOpen(
                otaq_core::protocol::ProjectTerminalParams {
                    project: "SHOP".into(),
                    new: true,
                },
            ))
            .await
            .unwrap()
        else {
            panic!("no terminal")
        };
        let session = terminal.session.id;
        let unsent = || h.service.sessions().has_unsent_input(&session).unwrap();
        type_if_empty(&h.app, session.clone(), "Write a skill for the Plan stage: ")
            .await
            .unwrap();
        assert!(unsent(), "typed, not sent");
        let refused = type_if_empty(&h.app, session.clone(), "again").await.unwrap_err();
        assert_eq!(
            refused.message,
            "Its input line has unsent text: send or clear it in its terminal."
        );
        assert!(
            h.app
                .notices
                .view()
                .toasts
                .iter()
                .any(|t| t.title == "Not typed" && t.text.as_deref() == Some(refused.message.as_str())),
            "a toast says so"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sending_needs_words() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let refused = request(
            &h.app,
            "flow_editor.send",
            json!({ "project": "SHOP", "text": "  " }),
        )
        .await;
        assert!(refused.is_err());
        let agents = super::super::agent_panel::agents(&h.app, "SHOP").await.unwrap();
        assert!(agents.open.is_empty());
    }
}

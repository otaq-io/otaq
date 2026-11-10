//! `flow_editor.*`: the requests Edit flow on the board (`flow_board.*`, `otaq-view`'s `flow_board.rs`) sends for the
//! project's draft (`flow.draft`, T-0470). **Every one of the user's edits goes through `flow.draft_update`**:
//! `flow_editor.edit` (a stage's name, kind, agent, options, a move, a ready stage added or a stage removed: the whole
//! document), `flow_editor.pick` (Start from) and `flow_editor.import` (a file). `flow_editor.apply` and
//! `flow_editor.cancel` are Save flow and Cancel.
//!
//! The Skill field (T-0472; FED-05, FED-06): Pick a skill… is an edit (`flow_editor.edit`); the picker lists what
//! `skills.list` finds for the project (the user's own skills, then Otaq's), and takes a typed name. Ask the agent to
//! write it **sends nothing** (Q441): the page asks for the user's words, and `flow_editor.write_skill` is their send
//! (Noyan's agent starts on the `flow` skill with them, and only then the field says it is being written).
//! `flow_editor.stop_writing` is Stop (it interrupts the agent's turn), `flow_editor.undo_skill` the written skill's
//! Undo (its folder goes only while it is uncommitted and unchanged, else the field just clears) and
//! `flow_editor.open_skill` its Open (its folder in the project's checkout, in the file manager).
//!
//! Problems that need a pick (Q426): `flow_board.view` takes the page's picks (`choices`) and `flow_editor.apply`
//! sends them with the draft (`flow.apply`'s `ApplyChoices`, as FLW-04's preview did).

use otaq_core::agents::Agent;
use otaq_core::flow::apply::{ApplyParams, ProposeFrom};
use otaq_core::flow::draft::{Draft, DraftEdit, DraftUpdateParams};
use otaq_core::flow::model::FlowFile;
use otaq_core::flow::yaml;
use otaq_core::project_agents::{AgentState, FLOW_COMMAND};
use otaq_core::protocol::{InputParams, ProjectParams, Reply, Request, encode_bytes};
use otaq_core::skills::found::{self, FoundSkill, Homes};
use otaq_view::view_models::flow_editor::{self as vm, Choices, Pick, StageEdit};
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "flow_editor.edit" => reply(&edit(app, parse(params)?).await?),
        "flow_editor.pick" => reply(&pick(app, parse(params)?).await?),
        "flow_editor.import" => reply(&import(app, parse(params)?).await?),
        "flow_editor.apply" => reply(&apply(app, parse(params)?).await?),
        "flow_editor.cancel" => reply(&cancel(app, parse(params)?).await?),
        "flow_editor.write_skill" => reply(&write_skill(app, parse(params)?).await?),
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

pub(super) fn toast_error(app: &App, title: &str, error: &RequestError) {
    app.notices.toast(NewToast::new(
        ToastLevel::Error,
        title,
        Some(error.message.clone()),
    ));
}

#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    EditParams::export_all(cfg)?;
    PickParams::export_all(cfg)?;
    ImportParams::export_all(cfg)?;
    ProjectFrom::export_all(cfg)?;
    SkillParams::export_all(cfg)?;
    WriteParams::export_all(cfg)?;
    Edited::export_all(cfg)?;
    Started::export_all(cfg)?;
    Ok(())
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

/// Import…: the picked file's path.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorImportParams")]
pub struct ImportParams {
    pub project: String,
    pub path: String,
}

/// Save flow and Cancel: the project, the draft Save applies and the picks it sends (the view's `choices`).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorProjectFrom")]
pub struct ProjectFrom {
    pub project: String,
    #[serde(default)]
    #[ts(optional)]
    pub proposal: Option<u32>,
    #[serde(default)]
    #[ts(optional)]
    pub choices: Option<Choices>,
}

/// The Skill field's buttons on a stage: Stop, Undo, Open.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorSkillParams")]
pub struct SkillParams {
    pub project: String,
    pub stage: String,
}

/// The user's send from the box Ask the agent to write it opened: their words, for `stage`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowEditorWriteParams")]
pub struct WriteParams {
    pub project: String,
    pub stage: String,
    pub text: String,
}

/// After an edit: the stage to select (a stage just added), if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowEditorEdited")]
pub struct Edited {
    pub selected: Option<String>,
}

/// The agent that now drafts or writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowEditorStarted")]
pub struct Started {
    pub agent: String,
}

pub(super) async fn draft(app: &App, project: &str) -> Result<Draft, RequestError> {
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

pub(super) async fn update(
    app: &App,
    project: &str,
    edit: DraftEdit,
) -> Result<Draft, RequestError> {
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

/// The flow in use now; the default flow before one was applied.
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

/// The project's name, or its key when the service can't say.
async fn project_name(app: &App, project: &str) -> String {
    match app.service.send(Request::ProjectList).await {
        Ok(Reply::Projects(list)) => list.into_iter().find(|p| p.key == project).map(|p| p.name),
        _ => None,
    }
    .unwrap_or_else(|| project.to_owned())
}

/// The skills the user already has for `project` and Otaq's own (`skills.list`): Pick a skill… and the captions
/// (d0115). Only read, never copied. When the service can't answer, Otaq's own still show, so the picker is never
/// empty.
async fn skills(app: &App, project: &str) -> Vec<FoundSkill> {
    let listed = app
        .service
        .send(Request::SkillsList(ProjectParams {
            project: project.to_owned(),
        }))
        .await;
    match listed {
        Ok(Reply::Skills(skills)) => skills,
        _ => found::list(&Homes::default(), None),
    }
}

/// Everything the board's editor reads for a project, owned.
pub(super) struct Gathered {
    pub draft: Draft,
    pub file: FlowFile,
    pub in_use: FlowFile,
    pub project_name: String,
    pub stored: Vec<Agent>,
    pub tools: otaq_view::view_models::tool_problems::Tools,
    pub skills: Vec<FoundSkill>,
    pub open: Vec<otaq_core::project_agents::ProjectAgent>,
    pub new_agent: Option<Agent>,
}

pub(super) async fn gather(app: &App, project: &str) -> Result<Gathered, RequestError> {
    let draft = draft(app, project).await?;
    let file = yaml::read(&draft.proposal.yaml)
        .map_err(|e| RequestError::plain(format!("The draft doesn\u{2019}t read: {e}")))?;
    Ok(Gathered {
        draft,
        file,
        in_use: in_use(app, project).await,
        project_name: project_name(app, project).await,
        stored: super::settings::agent_list(app).await?.agents,
        tools: super::agent_panel::tools(app).await,
        skills: skills(app, project).await,
        open: super::agent_panel::agents(app, project).await?.open,
        new_agent: super::agent_panel::project_agent(app).await?,
    })
}

impl Gathered {
    pub(super) fn facts<'a>(
        &'a self,
        project: &'a str,
        selected: Option<&'a str>,
        agent: Option<&str>,
        choices: &'a Choices,
    ) -> vm::Facts<'a> {
        vm::Facts {
            project,
            project_name: &self.project_name,
            proposal: &self.draft.proposal,
            marks: &self.draft.marks,
            swap: self.draft.swap,
            applies: &self.draft.applies,
            file: &self.file,
            in_use: &self.in_use,
            stored: &self.stored,
            tools: &self.tools,
            skills: &self.skills,
            writing: &self.draft.writing,
            written: &self.draft.written,
            selected,
            agent: agent.and_then(|id| self.open.iter().find(|a| a.id == id)),
            new_agent: self.new_agent.as_ref(),
            choices,
        }
    }
}

/// A stage's edit: the draft's document with it, sent whole (`flow.draft_update`, as the CLI sends one).
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

/// Import…: the file's flow copied into the draft.
async fn import(app: &App, params: ImportParams) -> Result<Edited, RequestError> {
    let source = super::settings::read_flow_file(&params.path)
        .await
        .inspect_err(|e| toast_error(app, "The flow wasn\u{2019}t imported", e))?;
    update(app, &params.project, DraftEdit::StartFrom { source }).await?;
    Ok(Edited { selected: None })
}

/// Save flow: the draft applies, with the picks its problems got.
async fn apply(app: &App, params: ProjectFrom) -> Result<(), RequestError> {
    let proposal = params.proposal.unwrap_or(0);
    if proposal == 0 {
        return Err(RequestError::plain("Nothing changed"));
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

/// Cancel drops the draft.
/// Cancel drops the draft the page showed (`proposal`; `0`: none was stored, so nothing to drop). With no
/// `proposal` the page had no view to say which (the draft doesn't draw): whatever draft is open goes.
async fn cancel(app: &App, params: ProjectFrom) -> Result<(), RequestError> {
    if params.proposal == Some(0) {
        return Ok(());
    }
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

/// The first message, on the `flow` skill: `/flow <the user's words>`.
pub fn flow_message(text: &str) -> String {
    format!("{FLOW_COMMAND} {}", text.trim())
}

/// The user's send from the box Ask the agent to write it opened (FED-06, Q441): Noyan's agent starts on the `flow`
/// skill with the user's words, and the field says it is being written by that agent. Nothing reaches an agent before
/// this.
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

/// Undo on the skill the agent wrote: the stage's skill as before; its folder is deleted only while it is uncommitted
/// and holds only what was written.
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

/// Open on the skill the agent wrote: its folder in the project's checkout, in the file manager (only one the draft
/// names).
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
        .ok_or_else(|| {
            RequestError::plain(format!("The agent wrote no skill for {}", stage.name))
        })?;
    let checkout = match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => list
            .into_iter()
            .find(|p| p.key == draft.proposal.project)
            .map(|p| std::path::PathBuf::from(p.repo_path))
            .ok_or_else(|| RequestError::plain(format!("No project {}", draft.proposal.project)))?,
        _ => return Err(RequestError::plain("project.list answered something else")),
    };
    let folder = written
        .folder
        .split('/')
        .fold(checkout, |p, part| p.join(part));
    open::that_detached(&folder).map_err(|e| RequestError::plain(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use otaq_view::view_models::flow_board::View as BoardEditor;
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    #[test]
    fn the_first_message_runs_the_flow_skill() {
        assert_eq!(
            flow_message("  What\u{2019}s Validate? "),
            "/flow What\u{2019}s Validate?"
        );
        assert_eq!(
            otaq_core::project_agents::first_skill(&flow_message("Suggest a flow for shop-web.")).0,
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

    /// The board's editor over the draft, the stage `selected` open.
    async fn load(h: &Harness, selected: Option<&str>) -> BoardEditor {
        super::super::flow_board::view(
            &h.app,
            super::super::flow_board::ViewParams {
                project: "SHOP".into(),
                selected: selected.map(str::to_owned),
                agent: None,
                choices: None,
            },
        )
        .await
        .unwrap()
    }

    fn names(v: &BoardEditor) -> Vec<&str> {
        v.columns.iter().map(|c| c.name.as_str()).collect()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn edits_go_through_the_draft_and_cancel_drops_it() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;

        // Opening the editor stores nothing and starts nothing.
        let v = load(&h, None).await;
        assert_eq!(v.proposal, 0);
        assert!(!v.bar.save_enabled);
        let agents = super::super::agent_panel::agents(&h.app, "SHOP")
            .await
            .unwrap();
        assert!(
            agents.open.is_empty(),
            "no agent starts when the editor opens"
        );

        // A Blank stage after Dev: the draft holds it, selected.
        let edited = request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "add_template", "after": "dev", "template": "new-stage" } }),
        )
        .await
        .unwrap();
        assert_eq!(edited["selected"], "new-stage");
        let v = load(&h, Some("new-stage")).await;
        assert_ne!(v.proposal, 0);
        assert_eq!(names(&v)[2], "New stage");

        // Rename it, then Cancel: the draft is gone.
        request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "rename", "stage": "new-stage", "name": "Plan" } }),
        )
        .await
        .unwrap();
        let v = load(&h, None).await;
        assert_eq!(names(&v)[2], "Plan");
        request(
            &h.app,
            "flow_editor.cancel",
            json!({ "project": "SHOP", "proposal": v.proposal }),
        )
        .await
        .unwrap();
        let v = load(&h, None).await;
        assert_eq!(v.proposal, 0);
        assert_eq!(names(&v), ["Backlog", "Dev", "Validate", "Review", "Done"]);
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
            json!({ "project": "SHOP", "pick": { "pick": "preset", "file": "spec-first.yaml" } }),
        )
        .await
        .unwrap();
        let v = load(&h, None).await;
        assert_eq!(names(&v)[1], "Product spec");
        request(
            &h.app,
            "flow_editor.apply",
            json!({ "project": "SHOP", "proposal": v.proposal }),
        )
        .await
        .unwrap();
        let v = load(&h, None).await;
        assert_eq!(v.proposal, 0);
        assert_eq!(names(&v)[1], "Product spec", "in use");
    }

    /// T-0529 (d0131): another agent for Validate (a check has one) and Code review's fails sent to Product spec go
    /// through the draft, apply, and read back from the flow in use.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_check_agent_and_fails_go_to_apply() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let apply = |proposal: u32| {
            request(
                &h.app,
                "flow_editor.apply",
                json!({ "project": "SHOP", "proposal": proposal }),
            )
        };
        request(
            &h.app,
            "flow_editor.pick",
            json!({ "project": "SHOP", "pick": { "pick": "preset", "file": "spec-first.yaml" } }),
        )
        .await
        .unwrap();
        apply(load(&h, None).await.proposal).await.unwrap();

        for edit in [
            json!({ "edit": "agent", "stage": "validate", "agent": "Developer" }),
            json!({ "edit": "fail_to", "stage": "code-review", "to": "product-spec" }),
        ] {
            request(
                &h.app,
                "flow_editor.edit",
                json!({ "project": "SHOP", "edit": edit }),
            )
            .await
            .unwrap();
        }
        let v = load(&h, Some("validate")).await;
        assert!(v.bar.save_enabled, "{:?}", v.bar.reason);
        apply(v.proposal).await.unwrap();

        let v = load(&h, Some("validate")).await;
        assert_eq!(v.proposal, 0, "applied");
        let column = |name: &str| v.columns.iter().find(|c| c.name == name).unwrap();
        assert_eq!(column("Validate").who.as_deref(), Some("Developer"));
        assert!(
            column("Code review")
                .check_line
                .as_deref()
                .is_some_and(|l| l.ends_with("go to Product spec")),
            "{:?}",
            column("Code review").check_line
        );
        let agent = v.details.unwrap().agent.unwrap();
        assert_eq!(agent.name, "Developer");
    }

    /// FED-05, FED-06: a Blank stage keeps Save off ("New stage needs a skill"); a skill typed for it turns Save on,
    /// and it applies.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_new_stage_waits_for_a_skill_and_takes_a_picked_one() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "add_template", "after": "dev", "template": "new-stage" } }),
        )
        .await
        .unwrap();
        let v = load(&h, Some("new-stage")).await;
        let details = v.details.as_ref().unwrap();
        assert!(matches!(
            details.skill.as_ref().map(|s| &s.state),
            Some(vm::SkillState::Empty { .. })
        ));
        assert!(!v.bar.save_enabled);
        assert_eq!(v.bar.reason.as_deref(), Some("New stage needs a skill"));
        // Pick a skill… lists what `skills.list` finds: here only Otaq's own, so it is never empty.
        let picker = &details.skill.as_ref().unwrap().picker;
        assert!(picker.items.iter().any(|i| i.id == "validate"));
        let otaqs = format!("{}\u{2019}s", otaq_core::naming::PRODUCT_NAME);
        assert!(
            picker
                .items
                .iter()
                .all(|i| i.hint.as_deref().is_some_and(|h| h.starts_with(&otaqs)))
        );

        // A name typed by hand: nothing is imported or copied; nobody has it, so it warns, and Save still works.
        request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "skill", "stage": "new-stage", "skill": "plan-steps" } }),
        )
        .await
        .unwrap();
        let v = load(&h, Some("new-stage")).await;
        assert_eq!(
            v.details.as_ref().unwrap().skill.as_ref().map(|s| &s.state),
            Some(&vm::SkillState::Set {
                skill: "plan-steps".into(),
                warning: Some("You don't have the skill `plan-steps`".into()),
            })
        );
        assert!(v.bar.save_enabled, "{:?}", v.bar.reason);
        request(
            &h.app,
            "flow_editor.apply",
            json!({ "project": "SHOP", "proposal": v.proposal }),
        )
        .await
        .unwrap();
    }

    /// A flow file naming a skill the user doesn't have still applies: Security carries no problem, shows "You don't
    /// have the skill `team-security-review`" on its column and in its Skill field, and Save stays on; the file's agent
    /// `skills:` is ignored.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_skill_the_user_doesnt_have_still_applies() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("team.yaml");
        std::fs::write(
            &file,
            "agents:\n  - name: Security reviewer\n    skills: [team-security-review]\n    model: default\nflow:\n  - \
             name: Dev\n    kind: agent\n    agent: Developer\n    skill: work-ticket\n  - name: Security\n    \
             kind: check\n    by: agent\n    agents: [Security reviewer]\n    skill: team-security-review\n    rounds: 2\n  \
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
        // Security takes the place (and the id) of Validate, whose name the file drops.
        let v = load(&h, Some("validate")).await;
        let security = &v.columns[2];
        assert_eq!(security.name, "Security");
        assert_eq!(security.problem, None);
        assert_eq!(
            security.warning.as_deref(),
            Some("You don't have the skill `team-security-review`")
        );
        assert_eq!(
            v.details.as_ref().unwrap().skill.as_ref().map(|s| &s.state),
            Some(&vm::SkillState::Set {
                skill: "team-security-review".into(),
                warning: Some("You don't have the skill `team-security-review`".into()),
            })
        );
        assert!(v.bar.save_enabled, "{:?}", v.bar.reason);
        request(
            &h.app,
            "flow_editor.apply",
            json!({ "project": "SHOP", "proposal": v.proposal, "choices": v.choices }),
        )
        .await
        .unwrap();
        let agents = super::super::settings::agent_list(&h.app)
            .await
            .unwrap()
            .agents;
        assert!(agents.iter().any(|a| a.name == "Security reviewer"));
    }

    /// A flow file naming a tool's built-in command as a stage's skill opens with the problem on that stage (not a
    /// warning), and Save stays off.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_built_in_command_is_a_problem_on_its_stage() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("team.yaml");
        std::fs::write(
            &file,
            "flow:\n  - name: Dev\n    kind: agent\n    agent: Developer\n    skill: work-ticket\n  - name: \
             Security\n    kind: check\n    by: agent\n    agent: Developer\n    skill: security-review\n    \
             rounds: 2\n  - name: Review\n    kind: check\n    by: you\n  - name: Done\n    kind: delivery\n    \
             column: with-previous\n    agent: Developer\n    mode: click\n    delivers: either\n",
        )
        .unwrap();
        request(
            &h.app,
            "flow_editor.import",
            json!({ "project": "SHOP", "path": file.display().to_string() }),
        )
        .await
        .unwrap();
        let v = load(&h, Some("validate")).await;
        let said = "Security: `/security-review` is Claude Code's built-in command, not a skill. Write a small \
                    skill that runs it, and name that skill here";
        let security = &v.columns[2];
        assert_eq!(security.name, "Security");
        assert_eq!(security.problem.as_deref(), Some(said));
        assert_eq!(security.warning, None);
        assert!(!v.bar.save_enabled);
    }

    /// Q441: Ask the agent to write it sends nothing by itself: the user's send is what starts an agent, and an empty
    /// send starts none.
    #[tokio::test(flavor = "multi_thread")]
    async fn writing_a_skill_waits_for_the_users_send() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let refused = request(
            &h.app,
            "flow_editor.write_skill",
            json!({ "project": "SHOP", "stage": "dev", "text": "  " }),
        )
        .await;
        assert!(refused.is_err());
        let agents = super::super::agent_panel::agents(&h.app, "SHOP")
            .await
            .unwrap();
        assert!(agents.open.is_empty(), "nothing started");
        let v = load(&h, None).await;
        assert_eq!(v.proposal, 0, "the field isn't marked as being written");
    }
}

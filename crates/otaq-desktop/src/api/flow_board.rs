//! `flow_board.*`: Edit flow on the board (`otaq-view`'s `flow_board.rs`). `flow_board.view` answers everything the
//! board's editor draws from the project's draft, as `flow_editor.view` does for the overlay's facts, plus the base
//! branch, the other projects' flows (Same as <project>) and whether there is a flow before this one. Edits, Start
//! from's picks, Import, Save and Cancel are the draft's own requests (`flow_editor.edit` / `pick` / `import` /
//! `apply` / `cancel`); this area adds what only the board's editor does:
//!
//! - `flow_board.restore`: Undo, the draft's document as it was before the user's last edit.
//! - `flow_board.suggest`: Suggest a flow for <project>: Noyan's agent starts on the `flow` skill and drafts one into
//!   the draft, its changes marked (today's flow-proposal path).
//! - `flow_board.ask_changes`: Ask for changes… on a suggestion: the user's words go back to the agent that made it
//!   (`flow.ask_for_changes`).
//! - `flow_board.export`: Export flow file…: the draft's document where the save dialog said; with no draft, the
//!   flow in use as `otaq flow show` prints it.

use otaq_core::flow::draft::DraftEdit;
use otaq_core::flow::propose::AskForChangesParams;
use otaq_core::protocol::{ProjectParams, Reply, Request};
use otaq_view::view_models::flow_board as vm;
use otaq_view::view_models::flow_editor::{Choices, Opened};
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::flow_editor::{Started, gather, update};
use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "flow_board.view" => reply(&view(app, parse(params)?).await?),
        "flow_board.restore" => reply(&restore(app, parse(params)?).await?),
        "flow_board.suggest" => reply(&suggest(app, parse(params)?).await?),
        "flow_board.ask_changes" => reply(&ask_changes(app, parse(params)?).await?),
        "flow_board.export" => reply(&export(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(format!("bad parameters: {e}")))
}

#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    ViewParams::export_all(cfg)?;
    RestoreParams::export_all(cfg)?;
    SuggestParams::export_all(cfg)?;
    AskChangesParams::export_all(cfg)?;
    ExportParams::export_all(cfg)?;
    vm::View::export_all(cfg)?;
    Ok(())
}

/// What `flow_board.view` takes: the project, the selected column (a stage's id, or `done`), the agent drafting a
/// suggestion (the page's own state) and the picks the draft's problems got so far.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowBoardParams")]
pub struct ViewParams {
    pub project: String,
    #[serde(default)]
    #[ts(optional)]
    pub selected: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub agent: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub choices: Option<Choices>,
}

/// Undo: the draft's document before the last edit.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowBoardRestoreParams")]
pub struct RestoreParams {
    pub project: String,
    pub yaml: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowBoardSuggestParams")]
pub struct SuggestParams {
    pub project: String,
}

/// Ask for changes… on the suggestion `proposal`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowBoardAskChangesParams")]
pub struct AskChangesParams {
    pub project: String,
    pub proposal: u32,
    pub text: String,
}

/// Export flow file…: where the save dialog said.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowBoardExportParams")]
pub struct ExportParams {
    pub project: String,
    pub path: String,
}

async fn projects(app: &App) -> Result<Vec<otaq_core::board::snapshot::ProjectInfo>, RequestError> {
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => Ok(list),
        other => Err(RequestError::plain(format!(
            "unexpected reply to project.list: {other:?}"
        ))),
    }
}

/// A project's flow in use, when it reads.
async fn flow_of(app: &App, project: &str) -> Option<otaq_core::flow::FlowFile> {
    match app
        .service
        .send(Request::FlowCurrent(ProjectParams {
            project: project.to_owned(),
        }))
        .await
    {
        Ok(Reply::FlowVersion(v)) => v.file().ok(),
        _ => None,
    }
}

/// Whether the project has a flow before the one in use.
async fn has_previous(app: &App, project: &str) -> bool {
    matches!(
        app.service
            .send(Request::FlowVersions(ProjectParams {
                project: project.to_owned(),
            }))
            .await,
        Ok(Reply::FlowVersions(versions)) if versions.len() > 1
    )
}

pub async fn view(app: &App, params: ViewParams) -> Result<vm::View, RequestError> {
    let gathered = gather(app, &params.project).await?;
    let all = projects(app).await?;
    let base = all
        .iter()
        .find(|p| p.key == params.project)
        .map_or_else(|| "main".to_owned(), |p| p.base_branch.clone());
    let mut others = Vec::new();
    for p in all.iter().filter(|p| p.key != params.project) {
        others.push((p.key.clone(), p.name.clone(), flow_of(app, &p.key).await));
    }
    let previous = has_previous(app, &params.project).await;
    let choices = params.choices.unwrap_or_default();
    let facts = gathered.facts(
        &params.project,
        Opened::Project,
        params.selected.as_deref(),
        params.agent.as_deref(),
        &choices,
    );
    Ok(vm::view(
        &facts,
        &vm::BoardFacts {
            base_branch: &base,
            others: &others,
            previous,
        },
    ))
}

async fn restore(app: &App, params: RestoreParams) -> Result<(), RequestError> {
    update(
        app,
        &params.project,
        DraftEdit::Document { yaml: params.yaml },
    )
    .await?;
    Ok(())
}

/// Suggest a flow for <project>…: Noyan's agent drafts one on the `flow` skill; its proposal lands in the draft,
/// marked.
async fn suggest(app: &App, params: SuggestParams) -> Result<Started, RequestError> {
    let name = projects(app)
        .await?
        .into_iter()
        .find(|p| p.key == params.project)
        .map_or_else(|| params.project.clone(), |p| p.name);
    let agent = super::agent_panel::send(
        app,
        super::agent_panel::SendParams {
            project: params.project,
            message: super::flow_editor::flow_message(&vm::suggest_message(&name)),
            agent: None,
        },
    )
    .await?;
    Ok(Started { agent })
}

async fn ask_changes(app: &App, params: AskChangesParams) -> Result<(), RequestError> {
    app.service
        .send(Request::FlowAskForChanges(AskForChangesParams {
            project: params.project,
            proposal: params.proposal,
            text: params.text,
        }))
        .await?;
    Ok(())
}

/// The draft's document as it stands (the flow in use when nothing changed), written where the save dialog said.
pub async fn export(app: &App, params: ExportParams) -> Result<(), RequestError> {
    let draft = super::flow_editor::draft(app, &params.project).await?;
    let yaml = if draft.proposal.id == 0 {
        match app
            .service
            .send(Request::FlowShow(ProjectParams {
                project: params.project.clone(),
            }))
            .await?
        {
            Reply::FlowDocument(document) => document.yaml,
            other => {
                return Err(RequestError::plain(format!(
                    "unexpected reply to flow.show: {other:?}"
                )));
            }
        }
    } else {
        draft.proposal.yaml
    };
    tokio::fs::write(&params.path, yaml)
        .await
        .map_err(|e| RequestError::plain(format!("Couldn\u{2019}t save {}: {e}", params.path)))
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

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
                selected: selected.map(str::to_owned),
                agent: None,
                choices: None,
            },
        )
        .await
        .unwrap()
    }

    /// + after Dev → Security review: the ready stage arrives with its description, agent and skill, marked New,
    /// the bar counts the change, and Save flow applies it at once, so the board's columns have it.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_ready_stage_added_between_two_columns_saves() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;

        let v = load(&h, None).await;
        let names: Vec<&str> = v.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Backlog", "Dev", "Validate", "Review", "Done"]);
        assert_eq!(v.bar.changes, None);
        assert!(!v.bar.save_enabled);
        assert_eq!(
            v.columns[1].add.as_ref().unwrap().title,
            "Add a stage between Dev and Validate"
        );

        let edited = request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "add_template", "after": "dev", "template": "security-review" } }),
        )
        .await
        .unwrap();
        assert_eq!(edited["selected"], "security-review");
        let v = load(&h, Some("security-review")).await;
        let column = &v.columns[2];
        assert_eq!(column.name, "Security review");
        assert_eq!(column.what, "An agent checks it");
        assert_eq!(column.who.as_deref(), Some("Checker"));
        assert_eq!(column.check_line.as_deref(), Some("2 rounds \u{b7} fails go to Dev"));
        assert_eq!(column.mark.as_ref().map(|m| m.label), Some("New"));
        // Nobody has its skill here: the usual warning, and it still saves.
        assert_eq!(
            column.warning.as_deref(),
            Some("You don't have the skill `security-check`")
        );
        assert_eq!(v.bar.changes.as_deref(), Some("1 change"));
        assert_eq!(v.bar.change_words.as_deref(), Some("added Security review"));
        let details = v.details.as_ref().unwrap();
        assert_eq!(details.title, "New stage");
        assert!(details.description.starts_with("Settles whether the change is safe to ship"));
        assert_eq!(details.rounds.as_ref().map(|r| r.value), Some(2));
        assert!(v.bar.save_enabled, "{:?}", v.bar.reason);
        assert_eq!(v.saved.title, "Flow saved");

        request(
            &h.app,
            "flow_editor.apply",
            json!({ "project": "SHOP", "from": "project", "proposal": v.proposal }),
        )
        .await
        .unwrap();
        let board = request(
            &h.app,
            "board.view",
            json!({ "project": "SHOP", "width": 1600.0, "height": 900.0 }),
        )
        .await
        .unwrap();
        let ids: Vec<&str> = board["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["backlog", "dev", "security-review", "validate", "review", "done"]);
    }

    /// What it's for and How work leaves the flow are the draft's edits; the delivery follows the Repository card's
    /// rules (Either has no Automatically).
    #[tokio::test(flavor = "multi_thread")]
    async fn the_description_and_delivery_are_edited_in_the_draft() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        for edit in [
            json!({ "edit": "description", "stage": "dev", "description": "  Builds it.  " }),
            json!({ "edit": "delivery", "field": "delivery.delivers", "value": "pr" }),
            json!({ "edit": "delivery", "field": "delivery.mode", "value": "automatic" }),
        ] {
            request(&h.app, "flow_editor.edit", json!({ "project": "SHOP", "edit": edit }))
                .await
                .unwrap();
        }
        let v = load(&h, Some("done")).await;
        assert_eq!(v.columns[1].description.as_deref(), Some("Builds it."));
        assert_eq!(
            v.columns.last().unwrap().delivery.as_deref(),
            Some("Pull request \u{b7} automatically")
        );
        let panel = v.delivery.unwrap();
        assert!(panel.delivers.iter().any(|c| c.id == "pr" && c.selected));
        assert!(panel.modes.iter().any(|c| c.id == "automatic" && c.selected));
        // Either is chosen at your click: picking it sets On Approve, and Automatically isn't offered with it.
        request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "delivery", "field": "delivery.delivers", "value": "either" } }),
        )
        .await
        .unwrap();
        let v = load(&h, Some("done")).await;
        assert!(v.delivery.unwrap().modes.is_empty());
    }

    /// Import a flow file…, then Export flow file…: the same file comes back.
    #[tokio::test(flavor = "multi_thread")]
    async fn import_then_export_gives_back_the_same_file() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("team.flow.yaml");
        let text = otaq_core::flow::yaml::write(
            &otaq_core::flow::presets::preset("spec-first.yaml")
                .unwrap()
                .read(),
        );
        std::fs::write(&shared, &text).unwrap();
        request(
            &h.app,
            "flow_editor.import",
            json!({ "project": "SHOP", "path": shared.display().to_string() }),
        )
        .await
        .unwrap();
        let out = dir.path().join("shop-web.flow.yaml");
        request(
            &h.app,
            "flow_board.export",
            json!({ "project": "SHOP", "path": out.display().to_string() }),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), text);

        // Saved, the flow in use exports the same stages and delivery.
        let v = load(&h, None).await;
        request(
            &h.app,
            "flow_editor.apply",
            json!({ "project": "SHOP", "from": "project", "proposal": v.proposal }),
        )
        .await
        .unwrap();
        request(
            &h.app,
            "flow_board.export",
            json!({ "project": "SHOP", "path": out.display().to_string() }),
        )
        .await
        .unwrap();
        let again = otaq_core::flow::yaml::read(&std::fs::read_to_string(&out).unwrap()).unwrap();
        let sent = otaq_core::flow::yaml::read(&text).unwrap();
        assert_eq!(again.flow, sent.flow);
    }

    /// Undo puts the draft's document back as it was before the last edit.
    #[tokio::test(flavor = "multi_thread")]
    async fn undo_restores_the_document_before_the_edit() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let before = load(&h, None).await.yaml;
        request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "remove", "stage": "validate" } }),
        )
        .await
        .unwrap();
        assert_eq!(load(&h, None).await.columns.len(), 4);
        request(
            &h.app,
            "flow_board.restore",
            json!({ "project": "SHOP", "yaml": before }),
        )
        .await
        .unwrap();
        let v = load(&h, None).await;
        assert_eq!(v.columns.len(), 5);
        assert_eq!(v.bar.changes, None);
    }
}

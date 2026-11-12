//! `flow_board.*`: Edit flow on the board (`otaq-view`'s `flow_board.rs`). `flow_board.view` answers everything the
//! board's editor draws from the project's draft, as `flow_editor.view` does for the overlay's facts, plus the base
//! branch, the other projects' flows (Same as <project>) and whether there is a flow before this one. Edits, Start
//! from's picks, Import, Save and Cancel are the draft's own requests (`flow_editor.edit` / `pick` / `import` /
//! `apply` / `cancel`); this area adds what only the board's editor does:
//!
//! - `flow_board.restore`: Undo, the draft's document as it was before the user's last edit.
//! - `flow_board.suggest`: the box at the editor's foot: Noyan's agent starts on the `flow` skill with the user's
//!   words, or (Suggest a whole flow, no words) asked for a whole flow, and drafts into the draft, its changes marked
//!   (today's flow-proposal path).
//! - `flow_board.ask_changes`: the box's send while a suggestion shows: the user's words go back to the agent that
//!   made it (`flow.ask_for_changes`).
//! - `flow_board.export`: Export flow file…: the draft's document where the save dialog said; with no draft, the
//!   flow in use as `otaq flow show` prints it.

use otaq_core::flow::draft::DraftEdit;
use otaq_core::flow::propose::AskForChangesParams;
use otaq_core::protocol::{ProjectParams, Reply, Request};
use otaq_view::view_models::flow_board as vm;
use otaq_view::view_models::flow_editor::Choices;
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

/// The box's send: the user's words, or none for Suggest a whole flow.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowBoardSuggestParams")]
pub struct SuggestParams {
    pub project: String,
    #[serde(default)]
    #[ts(optional)]
    pub text: Option<String>,
}

/// The box's send while the suggestion `proposal` shows.
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

/// The stages of the flow in use with tickets in them, by id: a ticket held before a stage counts as in it, as the
/// flow change that removes the stage waits for it.
fn occupied(
    tickets: &[otaq_core::board::tickets::Ticket],
    in_use: &otaq_core::flow::model::Flow,
) -> Vec<String> {
    let mut ids: Vec<String> = tickets
        .iter()
        .filter_map(|t| {
            otaq_core::flow::apply::standing(t, in_use, in_use)
                .stage_id()
                .map(str::to_owned)
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

pub async fn view(app: &App, params: ViewParams) -> Result<vm::View, RequestError> {
    let gathered = gather(app, &params.project).await?;
    let tickets: Vec<_> = super::board::snapshot(app, &params.project)
        .await
        .map(|b| b.tickets().cloned().collect())
        .unwrap_or_default();
    let occupied = occupied(&tickets, &gathered.in_use.flow);
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
            occupied: &occupied,
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

/// What the box asks Noyan's agent, on the `flow` skill: the user's words, or without them the whole-flow request
/// for the project `name`. Words that are only spaces are refused: nothing would be asked.
fn suggest_request(text: Option<&str>, name: &str) -> Result<String, RequestError> {
    match text {
        Some(words) if words.trim().is_empty() => Err(RequestError::plain(
            otaq_core::project_agents::EMPTY_MESSAGE,
        )),
        Some(words) => Ok(super::flow_editor::flow_message(words)),
        None => Ok(super::flow_editor::flow_message(&vm::suggest_message(name))),
    }
}

/// The box's send (↵, or Suggest a whole flow): Noyan's agent starts on the `flow` skill; its proposal lands in the
/// draft, marked. The page doesn't open the agent panel for it.
async fn suggest(app: &App, params: SuggestParams) -> Result<Started, RequestError> {
    let name = projects(app)
        .await?
        .into_iter()
        .find(|p| p.key == params.project)
        .map_or_else(|| params.project.clone(), |p| p.name);
    let message = suggest_request(params.text.as_deref(), &name)?;
    let agent = super::agent_panel::send(
        app,
        super::agent_panel::SendParams {
            project: params.project,
            message,
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

    /// The note beside Remove stage reads from the board: none while Dev is empty, a ticket in Backlog included.
    #[tokio::test(flavor = "multi_thread")]
    async fn no_remove_note_on_an_empty_stage() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let note = |v: vm::View| v.details.expect("the stage's panel").remove_note;
        assert_eq!(note(load(&h, Some("dev")).await), None, "Dev is empty");

        let Reply::Ticket(_) = h
            .app
            .service
            .send(Request::TicketCreate(otaq_core::board::tickets::NewTicketInput {
                project: "SHOP".into(),
                title: "Footer links go to old pages".into(),
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
        assert_eq!(note(load(&h, Some("dev")).await), None, "in Backlog");
    }

    /// The stages with tickets in them, as a change removing them waits for: a ticket in a stage, one held before a
    /// stage (waiting to begin the first one included); not one in Backlog, queued to begin, or done.
    #[test]
    fn occupied_stages_are_those_a_removal_waits_for() {
        use otaq_core::domain::{Place, Status};
        use otaq_view::view_models::card_fixtures::ticket;

        let flow = otaq_core::flow::default_flow();
        let at = |status: Status, place: Place| {
            let mut t = ticket("SHOP-1", "Footer links", status);
            t.place = place;
            t
        };
        let in_stages = |tickets: &[otaq_core::board::tickets::Ticket]| occupied(tickets, &flow);
        assert!(
            in_stages(&[
                at(Status::Backlog, Place::Backlog),
                at(Status::Queued, Place::Waiting(None)),
                at(Status::Merged, Place::Done),
            ])
            .is_empty()
        );
        assert_eq!(
            in_stages(&[
                at(Status::Working, Place::InStage("dev".into())),
                at(Status::Working, Place::InStage("dev".into())),
                at(Status::Held, Place::Waiting(Some("validate".into()))),
            ]),
            ["dev", "validate"]
        );
        assert_eq!(
            in_stages(&[at(Status::Held, Place::Waiting(None))]),
            ["dev"]
        );
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
        // The palette's ready stages whose skill nobody here has carry the usual missing-skill warning; Otaq ships no
        // skill for them.
        let warning = |id: &str| {
            v.palette
                .templates
                .iter()
                .find(|t| t.id == id)
                .unwrap_or_else(|| panic!("no ready stage {id}"))
                .warning
                .clone()
        };
        assert_eq!(
            warning("security-review").as_deref(),
            Some("You don't have the skill `security-check`")
        );
        assert_eq!(
            warning("test-in-browser").as_deref(),
            Some("You don't have the skill `test-in-browser`")
        );
        assert_eq!(warning("product-spec"), None, "a shipped skill");
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
            json!({ "project": "SHOP", "proposal": v.proposal }),
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
        // Either is chosen at your click: picking it sets On Approve, and the modes stay, switched off, as on the
        // project's Repository card.
        request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "delivery", "field": "delivery.delivers", "value": "either" } }),
        )
        .await
        .unwrap();
        let v = load(&h, Some("done")).await;
        let panel = v.delivery.unwrap();
        assert!(panel.modes_off);
        let modes: Vec<(&str, bool)> = panel.modes.iter().map(|c| (c.id.as_str(), c.selected)).collect();
        assert_eq!(modes, [("click", true), ("automatic", false)]);
        assert_eq!(
            panel.delivers_note.as_deref(),
            Some(otaq_view::view_models::project_settings::EITHER_NOTE)
        );
    }

    /// Done's panel names the delivery's agent and, for a PR, its PR skill: the user's own Open PR skill goes there
    /// (empty: Otaq's own `open-pr`); a local merge has no PR skill and may name no agent.
    #[tokio::test(flavor = "multi_thread")]
    async fn dones_panel_sets_the_deliverys_agent_and_pr_skill() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let edit = async |field: &str, value: &str| {
            request(
                &h.app,
                "flow_editor.edit",
                json!({ "project": "SHOP", "edit": { "edit": "delivery", "field": field, "value": value } }),
            )
            .await
        };
        edit("delivery.delivers", "pr").await.unwrap();
        let panel = load(&h, Some("done")).await.delivery.unwrap();
        let skill = panel.skill.expect("a PR has a PR skill");
        // The default flow names Otaq's own, which never warns.
        assert_eq!((skill.value.as_str(), skill.placeholder), ("open-pr", "open-pr"));
        assert_eq!(skill.warning, None);
        assert!(panel.agent.items.iter().all(|i| !i.id.is_empty()), "a PR needs an agent");

        edit("delivery.skill", " /my-pr ").await.unwrap();
        edit("delivery.agent", "Checker").await.unwrap();
        let panel = load(&h, Some("done")).await.delivery.unwrap();
        assert_eq!(panel.agent.value, "Checker");
        let skill = panel.skill.unwrap();
        assert_eq!(skill.value, "my-pr");
        assert_eq!(skill.warning.as_deref(), Some("You don't have the skill `my-pr`"));
        assert!(edit("delivery.agent", "").await.is_err(), "a PR keeps an agent");
        // Emptied, Otaq's own runs.
        edit("delivery.skill", "").await.unwrap();
        assert_eq!(load(&h, Some("done")).await.delivery.unwrap().skill.unwrap().value, "");

        edit("delivery.delivers", "local").await.unwrap();
        let panel = load(&h, Some("done")).await.delivery.unwrap();
        assert_eq!(panel.skill, None);
        assert_eq!(panel.agent.items[0].id, "", "None, for the plain message");
        edit("delivery.agent", "").await.unwrap();
        assert_eq!(load(&h, Some("done")).await.delivery.unwrap().agent.value, "None");
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
            json!({ "project": "SHOP", "proposal": v.proposal }),
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

    /// The box asks with the user's words on the `flow` skill; without words it asks for a whole flow for the project.
    #[test]
    fn the_box_asks_with_the_users_words_or_for_a_whole_flow() {
        let words = suggest_request(Some("  Add a security review after Validate "), "shop-web").unwrap();
        assert_eq!(words, "/flow Add a security review after Validate");
        assert_eq!(otaq_core::project_agents::first_skill(&words).0, "flow");
        let whole = suggest_request(None, "shop-web").unwrap();
        assert_eq!(
            whole,
            super::super::flow_editor::flow_message(&vm::suggest_message("shop-web"))
        );
        assert!(whole.starts_with("/flow Suggest a flow for shop-web."), "{whole}");
        assert!(suggest_request(Some(" \n "), "shop-web").is_err());
    }

    /// Edit flow's box, as the editor first shows it: who answers and what it does; Noyan isn't drafting, so it can be
    /// asked. Words that are only spaces start nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_box_asks_noyan_and_empty_words_start_nothing() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        let v = load(&h, None).await;
        assert_eq!(v.ask.placeholder, "Ask Noyan to change this flow\u{2026}");
        assert_eq!(v.ask.whole, Some("Suggest a whole flow"));
        assert_eq!(
            v.ask.line,
            "Proposes a draft \u{b7} nothing changes until you save"
        );
        assert!(v.ask.agent.starts_with("Noyan \u{b7} "), "{}", v.ask.agent);
        assert_eq!(v.ask.send, "Ask Noyan");
        assert_eq!(v.ask.off, None);

        let refused = request(
            &h.app,
            "flow_board.suggest",
            json!({ "project": "SHOP", "text": "   " }),
        )
        .await;
        assert!(refused.is_err());
        let agents = super::super::agent_panel::agents(&h.app, "SHOP")
            .await
            .unwrap();
        assert!(agents.open.is_empty(), "nothing started");
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

    /// The release test's refused reorder on shop: Validate's fails were set to go to Dev while another Agent stage
    /// sat between them, that stage went again, then Validate was dragged in front of Dev. The drag is refused with
    /// why, and the editor's view still builds after it; a draft stored that way before (Validate first, its fails to
    /// Dev) draws too, its problem on Validate, instead of leaving the editor blank.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_editor_still_draws_after_a_check_is_dragged_before_dev() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        shop_web(&h, &repos).await;
        for edit in [
            json!({ "edit": "add_template", "after": "dev", "template": "new-stage" }),
            json!({ "edit": "fail_to", "stage": "validate", "to": "dev" }),
            json!({ "edit": "remove", "stage": "new-stage" }),
        ] {
            request(&h.app, "flow_editor.edit", json!({ "project": "SHOP", "edit": edit }))
                .await
                .unwrap();
        }
        let refused = request(
            &h.app,
            "flow_editor.edit",
            json!({ "project": "SHOP", "edit": { "edit": "move", "stage": "validate", "to": 0 } }),
        )
        .await
        .unwrap_err();
        assert_eq!(
            refused.message,
            "Validate can\u{2019}t go before Dev: a check sends its fails back to a stage where an agent works, so \
             one has to come before it"
        );
        // The page's drag shows it as the toast the refusal raised.
        let toast = h.app.notices.view().toasts.last().cloned().unwrap();
        assert_eq!(toast.title, "The flow didn\u{2019}t change");
        assert_eq!(toast.text.as_deref(), Some(refused.message.as_str()));
        let v = load(&h, Some("validate")).await;
        let ids: Vec<&str> = v.columns.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["backlog", "dev", "validate", "review", "done"]);
        assert!(v.details.is_some());

        // The draft the release test was left with, stored as it was: it draws.
        let mut file = otaq_core::flow::yaml::read(&v.yaml).unwrap();
        let validate = file.flow.position("validate").unwrap();
        assert_eq!(file.flow.stages[validate].fail_to.as_deref(), Some("dev"));
        let moved = file.flow.stages.remove(validate);
        file.flow.stages.insert(0, moved);
        request(
            &h.app,
            "flow_board.restore",
            json!({ "project": "SHOP", "yaml": otaq_core::flow::yaml::write(&file) }),
        )
        .await
        .unwrap();
        let v = load(&h, Some("validate")).await;
        let ids: Vec<&str> = v.columns.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["backlog", "validate", "dev", "review", "done"]);
        assert!(v.details.is_some());
        assert!(!v.bar.save_enabled, "a check first can't be saved");
        assert!(v.bar.reason.is_some());
        assert!(!v.watch.fails.is_empty());
        // The page's test draws this view (flow_board.test.ts); `OTAQ_WRITE_FIXTURES=1` writes it again.
        if std::env::var_os(otaq_core::naming::env_var("WRITE_FIXTURES")).is_some() {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("ui/src/screens/flow_board/check_first.view.json");
            std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap() + "\n").unwrap();
        }

        // The page's Discard the draft, when it has no view to name the draft: whatever is open goes, and again with
        // nothing open is fine.
        for _ in 0..2 {
            request(&h.app, "flow_editor.cancel", json!({ "project": "SHOP" }))
                .await
                .unwrap();
        }
        let v = load(&h, None).await;
        assert_eq!(v.proposal, 0);
        let ids: Vec<&str> = v.columns.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["backlog", "dev", "validate", "review", "done"]);
    }
}

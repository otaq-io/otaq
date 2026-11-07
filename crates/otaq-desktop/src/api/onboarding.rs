//! `onboarding.*`: first run and Add project (UI-08, UI-11, SET-02, PRJ-01, PRJ-02, PRJ-03; d0021, d0023; *02 → No
//! projects yet*). A new user walks Welcome → Tools → Project → the board. The steps' words come once
//! (`onboarding.texts`); the Tools step runs the service's tools check and answers the agent tools and git; the Add
//! project form sends what the user picked and typed and gets the form back as Rust words it (the folder's check, the
//! detected values, the key's live check, the tools that will run their hooks in the folder, and, for a project added
//! after the first, its Flow choice); `onboarding.add` adds the project or says why not, and which tools' folder trust
//! wasn't recorded and why (PRJ-07, d0085, d0086). The Tools step's agent tool rows act through `agent_tools.*` (Sign
//! in…, Show, Hide). A folder dropped on the window reaches the page as a `files_dropped` event ([`crate::drops`]).
//!
//! Every project starts on its flow in use at once (Otaq default), with no Flow setup: a later project's Flow choice
//! copies another project's flow when added ("Same as <project>"), or the page opens Edit flow on Noyan's agent's
//! suggestion ("Suggest one").

use otaq_core::agent_tools::SetupOutcome as Outcome;
use otaq_core::agent_tools::Tool;
use otaq_core::board::projects::AddProject;
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::flow::draft::DraftEdit;
use otaq_core::flow::apply::{ApplyParams, ProposeFrom};
use otaq_core::protocol::{OnboardingState, PathParams, Reply, Request};
use otaq_view::view_models::add_project::AddProjectModel;
use otaq_view::view_models::onboarding::OnboardingModel;
use otaq_view::view_models::onboarding_page::{self, AddProjectPage, FlowPick, OtherFlow, Texts, Tools};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "onboarding.start" => reply(&start(app).await?),
        "onboarding.texts" => reply(&texts()),
        "onboarding.tools" => reply(&tools(app).await?),
        "onboarding.form" => reply(&form(app, parse(params)?).await?.1),
        "onboarding.add" => reply(&add(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(method: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {method}: {other:?}"))
}

/// Whether onboarding opens at start: first run means no project is stored yet (UI-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingStart")]
pub struct Start {
    pub show: bool,
}

async fn state(app: &App) -> Result<OnboardingState, RequestError> {
    match app.service.send(Request::OnboardingState).await? {
        Reply::OnboardingState(state) => Ok(state),
        other => Err(unexpected("onboarding.state", &other)),
    }
}

/// A ticket was approved and reached Done: the box for new work, the usage-data banner and the hints that close a new
/// user's first run wait for it. `false` when the service can't say.
pub(super) async fn first_approve(app: &App) -> bool {
    state(app).await.is_ok_and(|s| s.first_approve)
}

/// The one-time hints the user dismissed; none when the service can't say.
pub(super) async fn hints_seen(app: &App) -> Vec<otaq_core::store::settings::Hint> {
    match app.service.send(Request::HintsSeen).await {
        Ok(Reply::HintsSeen(seen)) => seen,
        _ => Vec::new(),
    }
}

async fn start(app: &App) -> Result<Start, RequestError> {
    Ok(Start {
        show: projects(app).await?.is_empty(),
    })
}

fn texts() -> Texts {
    onboarding_page::texts()
}

/// Runs the service's tools check and answers the Tools step (Check again asks again).
async fn tools(app: &App) -> Result<Tools, RequestError> {
    let state = state(app).await?;
    let mut model = OnboardingModel::new();
    model.start_check();
    let result = match app.service.send(Request::ToolsCheck).await {
        Ok(Reply::ToolsCheck(report)) => Ok(report),
        Ok(other) => Err(format!("unexpected answer: {other:?}")),
        Err(e) => Err(e.message),
    };
    model.finish_check(result);
    Ok(onboarding_page::tools(&model, state.has_projects))
}

/// What the Add project form holds: the folder picked (or dropped), the key and base branch as typed, and the Flow
/// choice of a project added after the first.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(rename = "AddProjectParams")]
pub struct FormParams {
    pub folder: Option<String>,
    pub key: String,
    /// The user typed in the key field: the suggestion no longer replaces it.
    pub key_edited: bool,
    pub base_branch: String,
    /// The user typed in the base-branch field (else the detected one is used).
    pub base_edited: bool,
    /// The onboarding step (else the dialog).
    pub first_run: bool,
    /// The dialog's Flow choice; Otaq default when left out.
    #[serde(default)]
    #[ts(optional)]
    pub flow: Option<FlowPick>,
}

async fn projects(app: &App) -> Result<Vec<ProjectInfo>, RequestError> {
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(projects) => Ok(projects),
        other => Err(unexpected("project.list", &other)),
    }
}

/// The name of the agent that drafts a suggested flow: Noyan's.
async fn suggester(app: &App) -> String {
    match app.service.send(Request::AgentsList).await {
        Ok(Reply::Agents(list)) => list
            .noyan()
            .map_or_else(|| "Noyan".to_owned(), |a| a.name.clone()),
        _ => "Noyan".to_owned(),
    }
}

/// The form for what the page holds: the folder checked by the service, the typed values on top of the detected
/// ones. Also returns the model and the projects, for Add.
async fn form(
    app: &App,
    params: FormParams,
) -> Result<((AddProjectModel, Vec<ProjectInfo>), AddProjectPage), RequestError> {
    let projects = projects(app).await?;
    let mut model = AddProjectModel {
        trust_tools: super::agent_tools::trust_tools(app).await,
        ..AddProjectModel::default()
    };
    if let Some(folder) = params.folder.clone() {
        model.pick(folder.clone());
        let result = match app
            .service
            .send(Request::ProjectProbe(PathParams { path: folder }))
            .await
        {
            Ok(Reply::FolderProbe(probe)) => Ok(probe),
            Ok(other) => Err((None, format!("unexpected answer: {other:?}"))),
            Err(e) => Err((e.refusal, e.message)),
        };
        model.probed(result, &projects, &super::keymap::labels(app).await);
    }
    if params.key_edited {
        model.set_key(&params.key);
    }
    if params.base_edited {
        model.set_base(&params.base_branch);
    }
    let mut page = onboarding_page::add_project(&model, &projects, params.first_run);
    // A project added after the first: which flow it starts on.
    if !params.first_run && !projects.is_empty() {
        let other = match projects.last() {
            Some(p) => Some(OtherFlow {
                key: &p.key,
                name: &p.name,
                line: otaq_view::view_models::flow_board::chain(&super::board::flow(app, &p.key).await),
            }),
            None => None,
        };
        let name = model.probe.as_ref().map(|p| p.name.as_str());
        page.flow = Some(onboarding_page::flow_choice(other, name, &suggester(app).await));
    }
    Ok(((model, projects), page))
}

/// The answer to `onboarding.add`: the new project's key, or the form with the reason under the key or the form.
/// When the project was added but a tool's trust wasn't recorded, the form says which and why, and the page waits
/// for its Continue before going on as after `Added` (PRJ-07).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "AddProjectOutcome")]
pub enum AddOutcome {
    Added {
        key: String,
    },
    AddedUntrusted {
        key: String,
        form: Box<AddProjectPage>,
    },
    Refused {
        form: Box<AddProjectPage>,
    },
}

async fn add(app: &App, params: FormParams) -> Result<AddOutcome, RequestError> {
    let first_run = params.first_run;
    let pick = params.flow.clone().unwrap_or_default();
    let ((mut model, projects), page) = form(app, params).await?;
    let Some(probe) = model.probe.clone().filter(|_| page.can_add) else {
        return Ok(AddOutcome::Refused {
            form: Box::new(page),
        });
    };
    let added = app
        .service
        .send(Request::ProjectAdd(AddProject {
            path: probe.repo_path,
            key: Some(model.key.clone()),
            base_branch: Some(model.base_branch.clone()),
            // Every project starts on its flow in use (Otaq default); Edit flow changes it.
            flow_setup: false,
        }))
        .await;
    if let Ok(Reply::ProjectAdded { project, .. }) = &added
        && let FlowPick::SameAs { project: from } = &pick
    {
        same_flow_as(app, &project.key, from).await;
    }
    match added {
        // A service whose tools keep no folder trust answers none: nothing to say.
        Ok(Reply::ProjectAdded {
            project,
            trust: Some(Outcome::Failed { reason }),
        }) => {
            model.added_untrusted(failed_tools(app, &project.key, reason).await);
            let mut form = onboarding_page::add_project(&model, &projects, first_run);
            form.flow = page.flow;
            Ok(AddOutcome::AddedUntrusted {
                key: project.key,
                form: Box::new(form),
            })
        }
        Ok(Reply::ProjectAdded { project, .. }) => Ok(AddOutcome::Added { key: project.key }),
        Ok(other) => Err(unexpected("project.add", &other)),
        Err(e) => {
            model.add_failed(e.refusal.as_deref(), e.message);
            let mut form = onboarding_page::add_project(&model, &projects, first_run);
            form.flow = page.flow;
            Ok(AddOutcome::Refused {
                form: Box::new(form),
            })
        }
    }
}

/// "Same as <project>": the new project's flow becomes a copy of `from`'s, saved at once. When that fails the project
/// stays on Otaq default and a toast says why; Edit flow can copy it later.
async fn same_flow_as(app: &App, project: &str, from: &str) {
    let copied = async {
        let draft = super::flow_editor::update(
            app,
            project,
            DraftEdit::StartFrom {
                source: ProposeFrom::Project {
                    project: from.to_owned(),
                },
            },
        )
        .await?;
        match app
            .service
            .send(Request::FlowApply(ApplyParams {
                project: project.to_owned(),
                proposal: draft.proposal.id,
                choices: Default::default(),
            }))
            .await?
        {
            Reply::FlowApplied(_) => Ok(()),
            other => Err(unexpected("flow.apply", &other)),
        }
    };
    if let Err(e) = copied.await {
        super::flow_editor::toast_error(app, "The flow wasn\u{2019}t copied", &e);
    }
}
/// Which tools' trust failed for the project just added, from the service's notices (each tool's reason); the
/// combined reason under the first tool that keeps folder trust when they can't be read.
async fn failed_tools(app: &App, key: &str, reason: String) -> Vec<(Tool, String)> {
    let failed: Vec<(Tool, String)> = state(app)
        .await
        .map(|s| s.trust_failed)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| f.project_key == key)
        .map(|f| (f.tool, f.reason))
        .collect();
    if !failed.is_empty() {
        return failed;
    }
    let tool = super::agent_tools::trust_tools(app)
        .await
        .first()
        .copied()
        .unwrap_or(Tool::ALL[0]);
    vec![(tool, reason)]
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    #[tokio::test(flavor = "multi_thread")]
    async fn first_run_adds_a_project_from_the_real_service() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();

        let start = request(&h.app, "onboarding.start", Value::Null)
            .await
            .unwrap();
        assert_eq!(start["show"], true, "no project yet: onboarding opens");
        let texts = request(&h.app, "onboarding.texts", Value::Null)
            .await
            .unwrap();
        assert_eq!(texts["welcome"]["button"], "Get started");
        // `onboarding.tools` isn't asked here: the tools check runs `claude --version` (the view is tested in
        // otaq-view with made-up reports).

        // A folder that isn't a repository: the frame's text, Add stays off.
        let plain = tempfile::tempdir().unwrap();
        let form = |folder: &Path| {
            json!({ "folder": folder.display().to_string(), "key": "", "key_edited": false,
                    "base_branch": "", "base_edited": false, "first_run": true })
        };
        let refused = request(&h.app, "onboarding.form", form(plain.path()))
            .await
            .unwrap();
        assert_eq!(refused["error"]["title"], "Not a git repository");
        assert_eq!(refused["can_add"], false);

        // A repository: the suggested key and the detected base branch; Add lands it.
        let repo = repos.repo("shop-web", 1);
        let valid = request(&h.app, "onboarding.form", form(&repo))
            .await
            .unwrap();
        assert_eq!(valid["folder_look"], "valid");
        assert_eq!(valid["key"]["value"], "SHOP");
        assert_eq!(valid["can_add"], true);
        let added = request(&h.app, "onboarding.add", form(&repo))
            .await
            .unwrap();
        assert_eq!(added, json!({ "kind": "added", "key": "SHOP" }));

        // The same folder again: already added.
        let again = request(&h.app, "onboarding.form", form(&repo))
            .await
            .unwrap();
        assert_eq!(
            again["error"]["title"],
            format!("shop-web is already in {}", otaq_core::naming::PRODUCT_NAME)
        );
        // The project is on its board at once: no Flow step, and onboarding doesn't open again.
        let start = request(&h.app, "onboarding.start", Value::Null)
            .await
            .unwrap();
        assert_eq!(start, json!({ "show": false }));

        // A second project: the dialog's Flow choice, Otaq default first; Same as shop-web copies its flow.
        let billing = repos.repo("billing-api", 1);
        let dialog = |flow: Value| {
            json!({ "folder": billing.display().to_string(), "key": "", "key_edited": false,
                    "base_branch": "", "base_edited": false, "first_run": false, "flow": flow })
        };
        let later = request(&h.app, "onboarding.form", dialog(Value::Null))
            .await
            .unwrap();
        let names: Vec<&str> = later["flow"]["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                format!("{} default", otaq_core::naming::PRODUCT_NAME).as_str(),
                "Same as shop-web",
                "Suggest one for billing-api"
            ]
        );
        let added = request(
            &h.app,
            "onboarding.add",
            dialog(json!({ "kind": "same_as", "project": "SHOP" })),
        )
        .await
        .unwrap();
        assert_eq!(added, json!({ "kind": "added", "key": "BILL" }));
        let projects = projects(&h.app).await.unwrap();
        assert!(projects.iter().all(|p| p.flow_setup_since.is_none()));
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_trust_still_adds_the_project_and_says_why() {
        // Claude Code's config file isn't JSON: it is never overwritten, and the trust fails.
        let claude = tempfile::tempdir().unwrap();
        let config =
            otaq_core::agent_tools::claude::trust::ClaudeConfig::locate(Some(claude.path().into()), None)
                .unwrap();
        std::fs::write(&config.file, "not json").unwrap();
        let h = Harness::with_claude(None, Some(config.clone()));
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let repo = repos.repo("pocket-notes", 1);
        let form = json!({ "folder": repo.display().to_string(), "key": "NOTE", "key_edited": true,
                           "base_branch": "", "base_edited": false, "first_run": false });
        let valid = request(&h.app, "onboarding.form", form.clone())
            .await
            .unwrap();
        // No agent tool is connected in the test service (an empty tools PATH), so the line names none; the trust
        // is still recorded, and its failure named per tool.
        assert_eq!(valid["trust_line"], Value::Null);
        assert_eq!(valid["trust_error"], Value::Null);

        let added = request(&h.app, "onboarding.add", form).await.unwrap();
        assert_eq!(added["kind"], "added_untrusted");
        assert_eq!(added["key"], "NOTE");
        assert_eq!(
            added["form"]["trust_error"],
            "Claude Code’s trust couldn’t be recorded: Claude Code's config file couldn't be read. Its agents will \
             wait on Claude Code’s trust screen."
        );
        assert_eq!(added["form"]["continue_label"], "Continue");
        assert_eq!(added["form"]["can_add"], false);
        assert_eq!(
            std::fs::read_to_string(&config.file).unwrap(),
            "not json",
            "left as it was"
        );
        // The board's warning, after the service's trust_changed.
        h.wait_for_event("the failed warning", |e| {
            e["kind"] == "notices" && e["notices"]["banners"][0]["kind"] == "trust_failed"
        })
        .await;
        let view = request(&h.app, "notices.view", Value::Null).await.unwrap();
        assert_eq!(
            view["banners"][0]["title"],
            "Claude Code doesn’t trust NOTE’s folder:"
        );
        assert_eq!(view["banners"][0]["dismissable"], false);
        h.service.stop_all().await;
    }
}

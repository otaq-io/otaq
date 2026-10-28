//! `onboarding.*`: first run and Add project (UI-08, UI-11, SET-02, PRJ-01, PRJ-02, PRJ-03; d0011, d0021, d0023,
//! d0069, d0070; *14 · First run & onboarding*, *02 → No projects yet*). The steps' words come once
//! (`onboarding.texts`); the tools step runs the service's tools check and answers the checklist; the Add project
//! form sends what the user picked and typed and gets the form back as Rust words it (the folder's check, the
//! detected values, the key's live check, the connected tools that keep folder trust); `onboarding.add` adds the
//! project or says why not, and which tools' folder trust wasn't recorded and why (PRJ-07, d0085, d0086). The Tools
//! step's agent tool rows act through `agent_tools.*` (Sign in…, Show, Hide). A folder dropped on the window reaches
//! the page as a `files_dropped` event ([`crate::drops`]).
//!
//! The Flow step and a later project's Flow setup page are the flow editor (`flow_editor.*` with `from:
//! "onboarding"`; FED-09, d0115): its Made for <project> asks each tool, Use this flow and Skip, use the default settle
//! the flow. `onboarding.flow_setup_page` says whether the shown project's main area is that page. The project stays
//! in Flow setup until its flow is settled, so onboarding reopens on the Flow step while no project's flow is
//! (`onboarding.start`).

use otaq_core::board::projects::AddProject;
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::agent_tools::SetupOutcome as Outcome;
use otaq_core::agent_tools::Tool;
use otaq_core::protocol::{OnboardingState, PathParams, Reply, Request};
use otaq_view::view_models::add_project::AddProjectModel;
use otaq_view::view_models::flow_setup_page;
use otaq_view::view_models::onboarding::OnboardingModel;
use otaq_view::view_models::onboarding_page::{self, AddProjectPage, Texts, Tools};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "onboarding.start" => reply(&start(app).await?),
        "onboarding.texts" => reply(&texts(app).await),
        "onboarding.tools" => reply(&tools(app).await?),
        "onboarding.dismiss_warning" => {
            app.service.send(Request::DismissPermissionWarning).await?;
            Ok(Value::Null)
        }
        "onboarding.form" => reply(&form(app, parse(params)?).await?.1),
        "onboarding.add" => reply(&add(app, parse(params)?).await?),
        "onboarding.flow_setup_page" => reply(&flow_setup_page(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(method: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {method}: {other:?}"))
}

/// Whether onboarding opens at start: first run means no project is stored yet (UI-08), or every project is still in
/// Flow setup (first run was left on the Flow step, ASK-04): then it opens on that step for the first of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingStart")]
pub struct Start {
    pub show: bool,
    /// The step it opens on when not the first.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub step: Option<onboarding_page::StepId>,
    /// The project whose Flow step it opens on.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project: Option<String>,
}

async fn state(app: &App) -> Result<OnboardingState, RequestError> {
    match app.service.send(Request::OnboardingState).await? {
        Reply::OnboardingState(state) => Ok(state),
        other => Err(unexpected("onboarding.state", &other)),
    }
}

async fn start(app: &App) -> Result<Start, RequestError> {
    let projects = projects(app).await?;
    let unsettled = projects.iter().all(|p| p.flow_setup_since.is_some());
    let flow = projects.first().filter(|_| unsettled);
    Ok(Start {
        show: projects.is_empty() || flow.is_some(),
        step: flow.map(|_| onboarding_page::StepId::Flow),
        project: flow.map(|p| p.key.clone()),
    })
}

async fn texts(app: &App) -> Texts {
    onboarding_page::texts(&super::keymap::labels(app).await)
}

/// Runs the service's tools check and answers the checklist (Re-check asks again).
async fn tools(app: &App) -> Result<Tools, RequestError> {
    let state = state(app).await?;
    let mut model = OnboardingModel::new(state.permission_warning_dismissed);
    model.start_check();
    let result = match app.service.send(Request::ToolsCheck).await {
        Ok(Reply::ToolsCheck(report)) => Ok(report),
        Ok(other) => Err(format!("unexpected answer: {other:?}")),
        Err(e) => Err(e.message),
    };
    model.finish_check(result);
    Ok(onboarding_page::tools(&model, state.has_projects))
}

/// What the Add project form holds: the folder picked (or dropped), and the key and base branch as typed.
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
    /// The onboarding card (else the dialog).
    pub first_run: bool,
}

async fn projects(app: &App) -> Result<Vec<ProjectInfo>, RequestError> {
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(projects) => Ok(projects),
        other => Err(unexpected("project.list", &other)),
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
    let page = onboarding_page::add_project(&model, &projects, params.first_run);
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
    let ((mut model, projects), page) = form(app, params).await?;
    let Some(probe) = model.probe.clone().filter(|_| page.can_add) else {
        return Ok(AddOutcome::Refused {
            form: Box::new(page),
        });
    };
    match app
        .service
        .send(Request::ProjectAdd(AddProject {
            path: probe.repo_path,
            key: Some(model.key.clone()),
            base_branch: Some(model.base_branch.clone()),
            // The user's Add project: the project stays in Flow setup until its flow is settled (ASK-04).
            flow_setup: true,
        }))
        .await
    {
        // A service whose tools keep no folder trust answers none: nothing to say.
        Ok(Reply::ProjectAdded {
            project,
            trust: Some(Outcome::Failed { reason }),
        }) => {
            model.added_untrusted(failed_tools(app, &project.key, reason).await);
            Ok(AddOutcome::AddedUntrusted {
                key: project.key,
                form: Box::new(onboarding_page::add_project(&model, &projects, first_run)),
            })
        }
        Ok(Reply::ProjectAdded { project, .. }) => Ok(AddOutcome::Added { key: project.key }),
        Ok(other) => Err(unexpected("project.add", &other)),
        Err(e) => {
            model.add_failed(e.refusal.as_deref(), e.message);
            Ok(AddOutcome::Refused {
                form: Box::new(onboarding_page::add_project(&model, &projects, first_run)),
            })
        }
    }
}

/// `{"project": "SHOP"}`: the project in Flow setup.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "OnboardingProjectParams")]
pub struct ProjectKey {
    pub project: String,
}

/// Whether the shown project's main area is its Flow setup page (`{"project": "BILL"}`; ASK-04): a project added
/// later shows it instead of its board until its flow is settled. Its body is the flow editor (FED-09).
async fn flow_setup_page(app: &App, params: ProjectKey) -> Result<flow_setup_page::Page, RequestError> {
    Ok(flow_setup_page::page(&projects(app).await?, &params.project))
}

/// The Flow setup page's type (T-0436).
#[cfg(test)]
pub fn export_flow_setup_page_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    ProjectKey::export_all(cfg)?;
    flow_setup_page::Page::export_all(cfg)
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
    // The Flow step's tests are Unix only: Claude Code's stand-in is an `sh` script found by name, which Windows can't
    // run (Q598). On Windows their helpers go unused.
    #![cfg_attr(not(unix), allow(dead_code, unused_imports))]

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
        assert_eq!(texts["theme"]["title"], "Pick a look");
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
        // Its flow isn't settled: onboarding reopens on its Flow step (ASK-04).
        let start = request(&h.app, "onboarding.start", Value::Null)
            .await
            .unwrap();
        assert_eq!(
            start,
            json!({ "show": true, "step": "flow", "project": "SHOP" })
        );
        h.service.stop_all().await;
    }

    /// A service whose agents start as a stand-in (`sh` running `cat`; no agent tool runs) and whose tools PATH holds
    /// only a `claude` that prints a version (Claude Code connected), with shop-web in Flow setup. The last part keeps
    /// the temporary folders.
    #[cfg(unix)]
    async fn flow_world() -> (Harness, TempRepos, tempfile::TempDir) {
        use otaq_core::agent_tools::LaunchContext;
        use otaq_core::agent_tools::claude::ClaudeLaunch;

        let bin = tempfile::tempdir().unwrap();
        let claude = bin.path().join("claude");
        std::fs::write(&claude, "#!/bin/sh\necho '2.1.4 (Claude Code)'\n").unwrap();
        std::fs::set_permissions(&claude, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let path = bin.path().as_os_str().to_owned();
        let h = Harness::configured(move |c| {
            c.tools.path = path.clone();
            c.launch = Some(LaunchContext {
                bin_dir: "/nonexistent".into(),
                plugin_dir: "/nonexistent/plugin".into(),
                path: std::env::var_os("PATH").unwrap_or_default(),
                claude: ClaudeLaunch {
                    program: vec!["sh".into(), "-c".into(), "exec cat".into(), "sh".into()],
                    config_dir: None,
                },
            });
        });
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let repo = repos.repo("shop-web", 1);
        h.app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: repo.display().to_string(),
                key: Some("SHOP".into()),
                base_branch: None,
                flow_setup: true,
            }))
            .await
            .unwrap();
        // The service's check at start finds the stand-in `claude`: its row is in the Flow setup.
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                if flow_setup(&h.app, "SHOP")
                    .await
                    .is_ok_and(|s| !s.tools.is_empty())
                {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("Claude Code connected");
        (h, repos, bin)
    }

    /// The project's Flow setup now: each tool's row and the open proposals (ASK-04).
    async fn flow_setup(
        app: &App,
        project: &str,
    ) -> Result<otaq_core::project_agents::first_flow::FlowSetup, RequestError> {
        match app
            .service
            .send(Request::OnboardingFlowSetup(otaq_core::protocol::ProjectParams {
                project: project.to_owned(),
            }))
            .await?
        {
            Reply::FlowSetup(setup) => Ok(*setup),
            other => Err(unexpected("onboarding.flow_setup", &other)),
        }
    }

    /// The Flow step: the flow editor opened from onboarding.
    async fn editor(h: &Harness) -> Value {
        request(
            &h.app,
            "flow_editor.view",
            json!({ "project": "SHOP", "from": "onboarding" }),
        )
        .await
        .unwrap()
    }

    /// Claude Code's row under Made for shop-web.
    async fn claude_row(h: &Harness) -> Value {
        editor(h).await["start_from"]["made_for"]["tools"][0].clone()
    }

    /// Asks Claude Code for SHOP's flow from the editor's Made for: the flow agent's id and session.
    async fn ask_claude(h: &Harness) -> (String, String) {
        let asked = request(
            &h.app,
            "flow_editor.ask",
            json!({ "project": "SHOP", "from": "onboarding", "tool": "claude" }),
        )
        .await
        .unwrap();
        let id = asked["agent"].as_str().unwrap().to_owned();
        let session = agent_of(h, &id).await.session.unwrap().id;
        (id, session)
    }

    async fn agent_of(h: &Harness, id: &str) -> otaq_core::project_agents::ProjectAgent {
        h.service.agents().0.get(id).await.unwrap()
    }

    /// The agent's hook, as the service handles it: the signals map it, the Project agents apply it.
    async fn hook(h: &Harness, id: &str, event: &str, payload: Value) {
        let (agents, signals, _) = h.service.agents();
        let agent = agents.get(id).await.unwrap();
        let caller = otaq_core::caller::Caller::ProjectAgent {
            project: agent.project.clone(),
            agent: agent.id.clone(),
            session: agent.session.as_ref().unwrap().id.clone(),
        };
        for signal in signals
            .hook(&caller, agent.tool, event, &payload)
            .await
            .unwrap()
        {
            agents.hook(id, &signal).await.unwrap();
        }
    }

    /// Waits until Claude Code's row under Made for says `status`, and answers the row.
    async fn row_says(h: &Harness, status: &str) -> Value {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let row = claude_row(h).await;
                if row["status"] == status {
                    return row;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Claude Code's row: {status}"))
    }

    /// FED-09 (T-0473; was T-0434's ASK-05, ASK-06, d0088): the Flow step is the editor from Otaq default with Use
    /// this flow · Skip, use the default; Made for's Ask starts the tool's flow agent, which its row shows working;
    /// the plan limit stops it and the tool's Resume continues the same session; Stop ends it into Recently ended and
    /// its row offers Ask again; Skip, use the default settles the flow.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn asking_a_tool_its_limit_stop_and_skip() {
        use otaq_core::usage::limit::{LimitHit, LimitKind};

        let (h, _repos, _bin) = flow_world().await;
        let step = editor(&h).await;
        assert_eq!(step["from"], "onboarding");
        assert_eq!(step["footer"]["cancel"], "Skip, use the default");
        assert_eq!(step["footer"]["apply"], "Use this flow");
        let default = format!("{} default", otaq_core::naming::PRODUCT_NAME);
        assert_eq!(
            step["flow"]["summary"],
            format!("5 steps \u{b7} starting from {default}")
        );
        let row = &step["start_from"]["made_for"]["tools"][0];
        assert_eq!(row["tool"], "claude");
        assert_eq!(row["status"], "Connected");
        assert_eq!(row["ask"], "Ask");
        assert!(
            h.service.agents().0.list("SHOP").await.unwrap().open.is_empty(),
            "opening the step starts nothing"
        );

        let (id, session) = ask_claude(&h).await;
        let row = row_says(&h, "Working").await;
        assert_eq!(row["ask"], Value::Null, "no Ask while it works");
        assert_eq!(agent_of(&h, &id).await.name, "Flow setup \u{b7} Claude Code");
        // Asking again: the same agent.
        assert_eq!(ask_claude(&h).await.0, id);

        // The plan limit stops it.
        hook(&h, &id, "UserPromptSubmit", json!({ "prompt": "/flow" })).await;
        let hit = LimitHit {
            kind: LimitKind::Session,
            text: "You've hit your session limit".into(),
        };
        hook(
            &h,
            &id,
            "StopFailure",
            json!({ "error": "rate_limit", "last_assistant_message": hit.text }),
        )
        .await;
        let tool = agent_of(&h, &id).await.tool;
        let now = otaq_core::store::now_ms();
        h.service
            .agents()
            .2
            .record_hit_reset(tool, &hit, Some(now + 72 * 60_000), now)
            .await
            .unwrap();
        row_says(&h, "Plan limit").await;

        // The tool's Resume (once the limit resets): "continue" goes to the same session, which keeps running.
        request(&h.app, "limits.resume_tool", json!({ "tool": "claude" }))
            .await
            .unwrap();
        let typed = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let seen = h
                    .service
                    .sessions()
                    .attach(&session)
                    .map(|s| String::from_utf8_lossy(&s.scrollback).contains("continue"))
                    .unwrap_or(false);
                if seen {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;
        assert!(typed.is_ok(), "continue typed into the same session");
        hook(&h, &id, "UserPromptSubmit", json!({ "prompt": "continue" })).await;
        let agent = agent_of(&h, &id).await;
        assert_eq!(agent.session.as_ref().unwrap().id, session, "the same session");
        row_says(&h, "Working").await;

        // Stop: the agent ends into Recently ended; its row offers Ask again.
        let ended = request(
            &h.app,
            "agent_panel.end",
            json!({ "project": "SHOP", "id": id, "confirm": true }),
        )
        .await
        .unwrap();
        assert_eq!(ended["outcome"], "done");
        let list = h.service.agents().0.list("SHOP").await.unwrap();
        assert!(list.open.iter().all(|a| a.id != id));
        assert_eq!(list.recent.first().map(|a| a.id.as_str()), Some(id.as_str()));
        let row = row_says(&h, "Ended without a proposal").await;
        assert_eq!(row["ask"], "Ask");

        // Skip, use the default: the flow is settled, onboarding no longer reopens on the step.
        let start = request(&h.app, "onboarding.start", Value::Null)
            .await
            .unwrap();
        assert_eq!(
            start,
            json!({ "show": true, "step": "flow", "project": "SHOP" })
        );
        let step = editor(&h).await;
        request(
            &h.app,
            "flow_editor.cancel",
            json!({ "project": "SHOP", "from": "onboarding", "proposal": step["proposal"] }),
        )
        .await
        .unwrap();
        let start = request(&h.app, "onboarding.start", Value::Null)
            .await
            .unwrap();
        assert_eq!(start, json!({ "show": false }));
        h.service.stop_all().await;
    }

    /// T-0434 (ASK-06, d0088): its session ends before a proposal (a crash, or `/exit`): the ended row, and the
    /// agent's Resume continues the same conversation.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn ending_without_a_proposal_shows_its_row_and_resume_continues_the_conversation() {
        let (h, _repos, _bin) = flow_world().await;
        let (id, session) = ask_claude(&h).await;
        let conversation = agent_of(&h, &id).await.tool_session_id;
        assert!(conversation.is_some());

        h.service.sessions().stop(&session).await.unwrap();
        let row = row_says(&h, "Ended without a proposal").await;
        assert_eq!(row["ask"], "Ask");

        let resumed = request(&h.app, "agent_panel.resume", json!({ "id": id }))
            .await
            .unwrap();
        assert_eq!(resumed, json!(id));
        let agent = agent_of(&h, &id).await;
        assert_eq!(agent.tool_session_id, conversation, "the same conversation");
        row_says(&h, "Working").await;
        h.service.stop_all().await;
    }

    /// A tool that isn't connected can't be asked: the refusal comes back, and nothing starts.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn asking_a_tool_that_isnt_connected_is_refused() {
        let (h, _repos, _bin) = flow_world().await;
        let refused = request(
            &h.app,
            "flow_editor.ask",
            json!({ "project": "SHOP", "from": "onboarding", "tool": "opencode" }),
        )
        .await
        .unwrap_err();
        assert!(refused.message.contains("OpenCode"), "{}", refused.message);
        assert!(h.service.agents().0.list("SHOP").await.unwrap().open.is_empty());
        h.service.stop_all().await;
    }

    /// FED-09: Use this flow on an untouched step keeps Otaq default; a preset picked in the step and used becomes
    /// the project's flow. Either settles Flow setup, so onboarding doesn't reopen.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn use_this_flow_settles_the_flow_step() {
        let (h, _repos, _bin) = flow_world().await;
        request(
            &h.app,
            "flow_editor.pick",
            json!({ "project": "SHOP", "pick": { "pick": "preset", "file": "spec-first.yaml" } }),
        )
        .await
        .unwrap();
        let step = editor(&h).await;
        assert_eq!(step["flow"]["steps"][1]["name"], "Product spec");
        assert_ne!(step["proposal"], 0);
        request(
            &h.app,
            "flow_editor.apply",
            json!({ "project": "SHOP", "from": "onboarding", "proposal": step["proposal"] }),
        )
        .await
        .unwrap();
        let start = request(&h.app, "onboarding.start", Value::Null)
            .await
            .unwrap();
        assert_eq!(start, json!({ "show": false }));
        let page = request(&h.app, "onboarding.flow_setup_page", json!({ "project": "SHOP" }))
            .await
            .unwrap();
        assert_eq!(page["shows"], false, "the board shows");
        let current = h
            .app
            .service
            .send(Request::FlowCurrent(otaq_core::protocol::ProjectParams {
                project: "SHOP".into(),
            }))
            .await
            .unwrap();
        let Reply::FlowVersion(version) = current else {
            panic!("{current:?}")
        };
        assert!(version.file().unwrap().flow.stage("product-spec").is_some());
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

//! `shell.*`: the header, Activity and the project rail (SHL-02, SHL-03, SCH-03, RUN-04, USG-03, ATT-10, PRJ-01; d0087,
//! d0086, d0024, d0016, d0020, d0058; *02 · App shell & project switcher → Row · Title bars, Row · Header states, Row ·
//! Header parts: Activity, rail, Row · Activity popover states*). The header has four items: the shown project with
//! its base branch, search, Activity (agents, processes and the usage warning across all projects) and Needs you. The
//! Activity popover (`shell.activity`) lists what Activity sums up. The rail's view is the switcher model's, kept here
//! between calls so a project that newly needs you rings for 10 s; Terminals and Settings sit at its bottom. The page
//! refetches on the events that change their counts, so they update live with no spinner.

use std::time::Instant;

use otaq_core::board::settings::{AgentLimits, GlobalSettings};
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::protocol::{ProjectParams, Reply, Request};
use otaq_core::scheduler::SchedulerStatus;
use otaq_core::terminals::run::RunInfo;
use otaq_core::tools::Os;
use otaq_core::usage::activity::Activity;
use otaq_view::key_labels::KeyLabels;
use otaq_view::keymap::KeyAction;
use otaq_view::shell::tilde;
use otaq_view::view_models::activity::{self, Button as ActivityButton, Inputs};
use otaq_view::view_models::flow_setup_page::{self, Header as HeaderFlowSetup};
use otaq_view::view_models::limits::{self, Field, Popover};
use otaq_view::view_models::menu::MenuItem;
use otaq_view::view_models::project_menu::{self, ProjectMenu};
use otaq_view::view_models::search_page;
use otaq_view::view_models::switcher::{self, QuickRow, RAIL_WIDTH, RailBottom, RailItem};
use otaq_view::view_models::tool_avatar::{self, AvatarTool};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "shell.header" => reply(&header(app, parse(params)?).await?),
        "shell.activity" => reply(&activity_popover(app).await?),
        "shell.rail" => reply(&rail(app, parse(params)?).await?),
        "shell.quick" => reply(&quick(app, parse(params)?)),
        "shell.project_for_digit" => reply(&project_for_digit(app, parse(params)?)),
        "shell.step_limit" => reply(&step_limit(app, parse(params)?).await?),
        "shell.cleanup" => reply(&cleanup(app).await?),
        "shell.open_folder" => {
            open_folder(app, parse(params)?).await?;
            Ok(Value::Null)
        }
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(method: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {method}: {other:?}"))
}

/// What `shell.header` and `shell.rail` take: the project on screen (the page's selection), if any.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(rename = "ShellParams")]
pub struct Params {
    pub project: Option<String>,
}

/// The shown project at the header's start: "shop-web" and its base branch; the folder on the name's hover, and the
/// project menu on a click (FED-09).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "HeaderProject")]
pub struct Project {
    pub key: String,
    pub name: String,
    pub base_branch: String,
    /// "~/code/shop-web": the name's hover.
    pub path: String,
    /// The full folder, which Open in Finder opens.
    pub repo_path: String,
    /// The project menu (FED-09, `s26-08`): Change flow… with the flow in use, Export flow…, Project settings…, Open
    /// in Finder, Open a terminal here.
    pub menu: ProjectMenu,
    /// While the project is in Flow setup (ASK-04): "Flow setup" and "2 agents at work" or "2 proposals" after the
    /// name, in place of the base branch.
    pub flow_setup: Option<HeaderFlowSetup>,
}

/// A button's tooltip: its name and the user's key for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "HeaderHint")]
pub struct Hint {
    pub text: &'static str,
    pub keys: Option<String>,
}

impl Hint {
    fn new(text: &'static str, action: KeyAction, keys: &KeyLabels) -> Self {
        Self {
            text,
            keys: keys.label(action),
        }
    }
}

/// The header, left to right (*02 → Row · Header states (0086 · V)*): exactly four items. New ticket, Terminals,
/// Settings, the tokens, the plan's windows, the agents and the processes are not in it (SHL-02).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "HeaderView")]
pub struct Header {
    /// `None` before the first project.
    pub project: Option<Project>,
    /// The search entry's placeholder and the user's key for Filter the board (`/`): it stays the board's filter
    /// (T-0221, KEY-09).
    pub search: Hint,
    pub activity: ActivityButton,
}

async fn projects(app: &App) -> Result<Vec<ProjectInfo>, RequestError> {
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(projects) => Ok(projects),
        other => Err(unexpected("project.list", &other)),
    }
}

async fn settings(app: &App) -> Result<GlobalSettings, RequestError> {
    match app.service.send(Request::SettingsGet).await? {
        Reply::GlobalSettings(reply) => Ok(reply.settings),
        other => Err(unexpected("settings.get", &other)),
    }
}

/// The parts the header can draw without: a failure leaves that part out instead of failing the whole header.
async fn optional<T>(app: &App, request: Request, pick: impl Fn(Reply) -> Option<T>) -> Option<T> {
    app.service.send(request).await.ok().and_then(pick)
}

/// What Activity needs from the service: the settings (agent limits), the scheduler's slots, the activity summary
/// and the Runs.
struct ActivityData {
    limits: AgentLimits,
    scheduler: Option<SchedulerStatus>,
    activity: Option<Activity>,
    runs: Vec<RunInfo>,
}

impl ActivityData {
    async fn fetch(app: &App) -> Result<Self, RequestError> {
        let (settings, scheduler, activity, runs) = tokio::join!(
            settings(app),
            optional(app, Request::SchedulerStatus, |r| match r {
                Reply::SchedulerStatus(s) => Some(s),
                _ => None,
            }),
            optional(app, Request::UsageActivity, |r| match r {
                Reply::Activity(a) => Some(*a),
                _ => None,
            }),
            optional(app, Request::RunList, |r| match r {
                Reply::Runs(runs) => Some(runs),
                _ => None,
            }),
        );
        Ok(Self {
            limits: settings?.agent_limits,
            scheduler,
            activity,
            runs: runs.unwrap_or_default(),
        })
    }

    /// The inputs with `connected` as the connected tools (the app passes `tool_avatar::connected()`).
    fn inputs(&self, now: i64, connected: &[AvatarTool]) -> Inputs<'_> {
        Inputs {
            tools: self
                .activity
                .as_ref()
                .map(|a| activity::connected_among(a, connected))
                .unwrap_or_default(),
            tokens_today: self.activity.as_ref().map_or(0, |a| a.tokens_today),
            limits: &self.limits,
            scheduler: self.scheduler.as_ref(),
            runs: &self.runs,
            now,
        }
    }
}

pub async fn header(app: &App, params: Params) -> Result<Header, RequestError> {
    let (projects, data, keys, paused) = tokio::join!(
        projects(app),
        ActivityData::fetch(app),
        super::keymap::labels(app),
        super::pause::counts(app),
    );
    let (projects, data) = (projects?, data?);
    let shown = params.project.as_deref();
    // The shown project's Flow setup, for its title bar part; without an answer the name shows alone.
    let setup = match shown.filter(|p| flow_setup_page::in_flow_setup(&projects, p)) {
        Some(project) => {
            optional(
                app,
                Request::OnboardingFlowSetup(ProjectParams {
                    project: project.to_owned(),
                }),
                |r| match r {
                    Reply::FlowSetup(setup) => Some(*setup),
                    _ => None,
                },
            )
            .await
        }
        None => None,
    };
    let mut view = header_view(
        &projects,
        shown,
        &data.inputs(otaq_core::store::now_ms(), &tool_avatar::connected()),
        &keys,
    );
    if let (Some(project), Some(setup)) = (view.project.as_mut(), setup.as_ref()) {
        project.flow_setup = flow_setup_page::header(setup);
    }
    if let Some(project) = view.project.as_mut() {
        project.menu = flow_menu(app, project, &keys).await;
    }
    // "5 paused" while the shown project is (PAU-08); without the counts Activity reads as before.
    if let Some(chip) = paused
        .ok()
        .and_then(|counts| otaq_view::view_models::pause::activity(shown, &counts))
    {
        view.activity.agents = chip;
    }
    Ok(view)
}

fn header_view(
    projects: &[ProjectInfo],
    shown: Option<&str>,
    activity: &Inputs<'_>,
    keys: &KeyLabels,
) -> Header {
    Header {
        project: projects
            .iter()
            .find(|p| Some(p.key.as_str()) == shown)
            .map(|p| {
                let path = tilde(&p.repo_path);
                Project {
                    key: p.key.clone(),
                    name: p.name.clone(),
                    base_branch: p.base_branch.clone(),
                    menu: project_menu::menu(&project_menu::Facts {
                        name: &p.name,
                        path: &path,
                        flow: None,
                        waiting: None,
                        os: Os::current(),
                        keys,
                    }),
                    path,
                    repo_path: p.repo_path.clone(),
                    flow_setup: None,
                }
            }),
        search: Hint::new(search_page::PLACEHOLDER, KeyAction::FilterBoard, keys),
        activity: activity::button(activity, keys),
    }
}

/// The project menu with the project's flow in use and its waiting change, if the service answers them.
async fn flow_menu(app: &App, project: &Project, keys: &KeyLabels) -> ProjectMenu {
    let params = || ProjectParams {
        project: project.key.clone(),
    };
    let (current, waiting) = tokio::join!(
        optional(app, Request::FlowCurrent(params()), |r| match r {
            Reply::FlowVersion(v) => v.file().ok(),
            _ => None,
        }),
        optional(app, Request::FlowWaiting(params()), |r| match r {
            Reply::FlowWaiting(w) => w.map(|w| *w),
            _ => None,
        }),
    );
    project_menu::menu(&project_menu::Facts {
        name: &project.name,
        path: &project.path,
        flow: current.as_ref(),
        waiting: waiting.as_ref(),
        os: Os::current(),
        keys,
    })
}

/// `shell.open_folder {project}`: Open in Finder, the project's folder in the file manager.
async fn open_folder(app: &App, params: ProjectParams) -> Result<(), RequestError> {
    let folder = projects(app)
        .await?
        .into_iter()
        .find(|p| p.key == params.project)
        .map(|p| p.repo_path)
        .ok_or_else(|| RequestError::plain(format!("no project {}", params.project)))?;
    open::that_detached(&folder).map_err(|e| RequestError::plain(e.to_string()))
}

/// The Activity popover (⇧⌘U or a click on Activity).
pub async fn activity_popover(app: &App) -> Result<activity::Popover, RequestError> {
    popover_with(app, &tool_avatar::connected()).await
}

/// The Activity popover with `connected` as the connected tools.
async fn popover_with(app: &App, connected: &[AvatarTool]) -> Result<activity::Popover, RequestError> {
    let (data, keys) = tokio::join!(ActivityData::fetch(app), super::keymap::labels(app));
    Ok(activity::popover(
        &data?.inputs(otaq_core::store::now_ms(), connected),
        &keys,
    ))
}

/// The project rail, the 64 px column of tiles at every width, in the user's order.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "RailView")]
pub struct Rail {
    /// The project to show: the page's selection while it exists, else the first one (`None`: no projects).
    pub selected: Option<String>,
    pub items: Vec<RailItem>,
    /// The rail's width in px.
    pub width: u32,
    /// When a ringing tile stops ringing, so the page asks again then.
    pub ring_ends_in_ms: Option<u64>,
    /// The user's key for the quick switcher (⌘P).
    pub switch_keys: Option<String>,
    /// Terminals and Settings, under Cleanup.
    pub bottom: RailBottom,
    /// A right-click on a tile: Project settings…, Rename…, Remove project… (#11).
    pub project_menu: Vec<MenuItem>,
}

pub async fn rail(app: &App, params: Params) -> Result<Rail, RequestError> {
    let mut projects = projects(app).await?;
    // Waiting Project agents count in their project's badge too (PAG-07); without the queue, tickets only.
    if let Some(agents) = optional(app, Request::InboxList, |r| match r {
        Reply::Inbox(list) => Some(list.project_agents),
        _ => None,
    })
    .await
    {
        switcher::add_waiting_agents(&mut projects, &agents);
    }
    let keys = super::keymap::labels(app).await;
    // The paused projects' tiles count their paused agents (PAU-08); without the counts they say "paused".
    let paused: std::collections::BTreeMap<String, u32> = super::pause::counts(app)
        .await
        .map(|c| c.projects.into_iter().map(|p| (p.project, p.paused)).collect())
        .unwrap_or_default();
    let now = Instant::now();
    let mut model = app.switcher.lock().expect("switcher");
    model.keys = keys;
    model.paused = paused;
    if let Some(shown) = params.project
        && !model.select(&shown)
    {
        // Not in the last list (just added): the new list decides whether it exists.
        model.selected = Some(shown);
    }
    model.set_projects(projects, now);
    model.expire(now);
    Ok(Rail {
        selected: model.selected.clone(),
        items: model.rail(now),
        width: RAIL_WIDTH,
        ring_ends_in_ms: model
            .next_ring_end(now)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
        switch_keys: model.keys.label(KeyAction::SwitchProject),
        bottom: switcher::bottom(&model.keys),
        project_menu: project_menu::rail_menu(&model.keys),
    })
}

/// What `shell.quick` takes: the typed filter.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(rename = "QuickSwitcherParams")]
pub struct QuickParams {
    pub filter: String,
}

/// A footer hint of the quick switcher: "↑↓ move", "↵ switch", "⌘N add project".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "QuickSwitcherHint")]
pub struct QuickHint {
    pub keys: String,
    pub label: &'static str,
}

/// The quick switcher (⌘P): needs-you first, then working, then idle, filtered by name or key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "QuickSwitcherView")]
pub struct Quick {
    pub placeholder: &'static str,
    pub rows: Vec<QuickRow>,
    pub hints: Vec<QuickHint>,
}

/// From the rail's last list and keys, so typing never waits on the service. "add project" is New ticket's key (the
/// switcher runs that action); without one the hint is left out.
pub fn quick(app: &App, params: QuickParams) -> Quick {
    let model = app.switcher.lock().expect("switcher");
    Quick {
        placeholder: "Switch project…",
        rows: model
            .quick(&params.filter, Instant::now())
            .into_iter()
            .map(QuickRow::from)
            .collect(),
        hints: quick_hints(&model.keys),
    }
}

fn quick_hints(keys: &KeyLabels) -> Vec<QuickHint> {
    let add = keys.label(KeyAction::NewTicket).map(|keys| QuickHint {
        keys,
        label: "add project",
    });
    [
        Some(QuickHint {
            keys: "↑↓".to_owned(),
            label: "move",
        }),
        Some(QuickHint {
            keys: "↵".to_owned(),
            label: "switch",
        }),
        add,
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// What `shell.project_for_digit` takes: the digit of ⌘1–9.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[ts(rename = "ProjectDigitParams")]
pub struct DigitParams {
    pub digit: u8,
}

/// ⌘1–9: the key of the project at that place in the rail, if there is one.
pub fn project_for_digit(app: &App, params: DigitParams) -> Option<String> {
    let model = app.switcher.lock().expect("switcher");
    model.project_for_digit(params.digit).map(str::to_owned)
}

/// What `shell.step_limit` takes: which stepper value, one up or down.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[ts(rename = "LimitStep")]
pub struct StepParams {
    pub field: Field,
    pub up: bool,
}

/// A stepper press in Activity's popover: saved at once; it reaches the scheduler without restarting anything, and
/// never stops a running agent (SCH-03). Answers the stepper with the new values.
pub async fn step_limit(app: &App, params: StepParams) -> Result<Popover, RequestError> {
    let mut settings = settings(app).await?;
    let limits: AgentLimits = limits::stepped(&settings.agent_limits, params.field, params.up);
    settings.agent_limits = limits;
    match app.service.send(Request::SettingsSet(settings)).await? {
        Reply::GlobalSettings(reply) => Ok(limits::popover(&reply.settings.agent_limits)),
        other => Err(unexpected("settings.set", &other)),
    }
}

/// The broom at the bottom of the rail (*16 → Where Cleanup lives*): a dot only while leftovers wait.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupBroom")]
pub struct Broom {
    pub count: u32,
    pub tooltip: String,
    pub detail: &'static str,
}

fn broom(count: usize) -> Broom {
    let tooltip = match count {
        0 => "Cleanup".to_owned(),
        1 => "Cleanup · 1 leftover".to_owned(),
        n => format!("Cleanup · {n} leftovers"),
    };
    Broom {
        count: u32::try_from(count).unwrap_or(u32::MAX),
        tooltip,
        detail: "Nothing is deleted without your click",
    }
}

/// A read-only scan of the leftovers; the page asks at start and after a resync, not on every event.
pub async fn cleanup(app: &App) -> Result<Broom, RequestError> {
    match app.service.send(Request::CleanupScan).await? {
        Reply::CleanupScan(items) => Ok(broom(items.len())),
        other => Err(unexpected("cleanup.scan", &other)),
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use otaq_view::keymap::Platform;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    fn project(key: &str, name: &str, working: u32, needs: u32) -> ProjectInfo {
        ProjectInfo {
            key: key.into(),
            name: name.into(),
            repo_path: format!("/code/{name}"),
            base_branch: "main".into(),
            agents_working: working,
            needs_you: needs,
            flow_setup_since: None,
            paused_since: None,
        }
    }

    #[test]
    fn the_header_has_exactly_three_items() {
        let projects = [project("NOTE", "pocket-notes", 2, 6), project("API", "notes-api", 1, 1)];
        let fixtures = activity::fixtures::Fixtures::new();
        let mac = KeyLabels::base(Platform::Mac);
        let view = header_view(&projects, Some("API"), &fixtures.low_one.inputs(), &mac);
        let json = serde_json::to_value(&view).unwrap();
        let keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
        // Project and branch, search, Activity (Pause comes in its slot; Needs you is the board's strip, d0114).
        assert_eq!(keys, ["activity", "project", "search"]);

        let shown = view.project.unwrap();
        assert_eq!((shown.name.as_str(), shown.base_branch.as_str()), ("notes-api", "main"));
        assert_eq!(shown.path, "/code/notes-api");
        assert_eq!(shown.menu.title, "notes-api");
        let menu: Vec<&str> = shown.menu.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(menu[0], "Change flow\u{2026}");
        assert_eq!(menu.len(), 5);
        assert_eq!(view.activity.agents.count, "3");
        assert_eq!(view.activity.processes.as_deref(), Some("2"));
        assert_eq!(view.activity.warning.unwrap().text, "5h 18% left");

        // No project shown, nothing running, nothing waiting.
        let view = header_view(&projects, None, &fixtures.idle.inputs(), &mac);
        assert_eq!(view.project, None);
        assert_eq!(view.activity.processes, None);
        assert_eq!(view.activity.warning, None);
    }

    /// KEY-08, KEY-09: the search field shows `/` (the board filter), the tooltips the user's keys; Activity → ⌘J
    /// shows ⌘J; a removed key and letters off show none.
    #[test]
    fn the_header_shows_the_users_keys() {
        use otaq_view::key_labels::fixtures as keys;
        let projects = [project("NOTE", "pocket-notes", 0, 0)];
        let idle = activity::fixtures::Fixtures::new().idle;
        let view = |labels: &KeyLabels| header_view(&projects, Some("NOTE"), &idle.inputs(), labels);
        let base = view(&keys::base());
        assert_eq!(base.search.keys.as_deref(), Some("/"));
        assert_eq!(base.activity.keys.as_deref(), Some("⇧⌘U"));
        let j = view(&keys::mac(&keys::activity_on_cmd_j()));
        assert_eq!(j.activity.keys.as_deref(), Some("⌘J"));
        let removed = view(&keys::with(true, &[("open_activity", None, 0, None)]));
        assert_eq!(removed.activity.keys.as_deref(), Some("g a"));
        let off = view(&keys::with(false, &[("open_activity", None, 0, None)]));
        assert_eq!((off.search.keys, off.activity.keys), (None, None));
    }

    /// The quick switcher's "add project" is New ticket's key, left out without one.
    #[test]
    fn the_quick_switchers_hints_show_the_users_keys() {
        use otaq_view::key_labels::fixtures as keys;
        let labels = |hints: Vec<QuickHint>| hints.into_iter().map(|h| (h.keys, h.label)).collect::<Vec<_>>();
        assert_eq!(labels(quick_hints(&keys::base()))[2], ("⌘N".to_owned(), "add project"));
        assert_eq!(
            labels(quick_hints(&keys::removed("new_ticket", None, 0)))[2],
            ("c".to_owned(), "add project")
        );
        let none = keys::with(false, &[("new_ticket", None, 0, None)]);
        assert_eq!(quick_hints(&none).len(), 2);
    }

    /// KEY-08 end to end: a key recorded in Settings shows in the header, the Activity popover and the rail once the
    /// page asks again (on `settings_changed`).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_recorded_key_shows_in_the_header_and_the_rail() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let mac = Platform::current() == Platform::Mac;
        let (inbox, terminals) = if mac { ("⌘J", "⌘T") } else { ("Ctrl+J", "Ctrl+T") };
        let record = |action: &str, key: &str| {
            json!({ "edit": "record", "action": { "action": action }, "slot": 0, "key": key, "confirmed": true })
        };
        for edit in [record("open_activity", inbox), record("terminals", terminals)] {
            let done = request(&h.app, "keyboard.edit", edit).await.unwrap();
            assert_eq!(done["outcome"], "done");
        }
        h.wait_for_event("settings_changed", |e| {
            e["kind"] == "service" && e["name"] == "settings_changed"
        })
        .await;
        let header = request(&h.app, "shell.header", json!({})).await.unwrap();
        assert_eq!(header["activity"]["keys"], inbox);
        let popover = request(&h.app, "shell.activity", Value::Null).await.unwrap();
        assert_eq!(popover["open_terminals"]["keys"], terminals);
        let rail = request(&h.app, "shell.rail", json!({})).await.unwrap();
        assert_eq!(rail["bottom"]["terminals"]["keys"], terminals);
        h.service.stop_all().await;
    }

    #[test]
    fn the_broom_names_its_leftovers() {
        assert_eq!(broom(0).tooltip, "Cleanup");
        assert_eq!(broom(1).tooltip, "Cleanup · 1 leftover");
        assert_eq!(broom(3).count, 3);
    }

    async fn add(h: &Harness, repos: &TempRepos, name: &str) -> ProjectInfo {
        let path = repos.repo(name, 1);
        match h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }))
            .await
            .unwrap()
        {
            Reply::ProjectAdded { project: p, .. } => p,
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_project_in_flow_setup_is_marked_in_the_rail_and_the_title_bar() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let shop = add(&h, &repos, "shop-web").await;
        let path = repos.repo("billing-api", 1);
        let bill = match h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: Some("BILL".into()),
                base_branch: None,
                flow_setup: true,
            }))
            .await
            .unwrap()
        {
            Reply::ProjectAdded { project: p, .. } => p,
            other => panic!("{other:?}"),
        };

        let rail = request(&h.app, "shell.rail", json!({ "project": bill.key }))
            .await
            .unwrap();
        assert_eq!(rail["items"][0]["flow_setup"], Value::Null);
        assert_eq!(rail["items"][1]["flow_setup"], "Flow setup");
        let header = request(&h.app, "shell.header", json!({ "project": bill.key }))
            .await
            .unwrap();
        assert_eq!(header["project"]["flow_setup"]["label"], "Flow setup");
        // Nothing asked yet: no pill.
        assert_eq!(header["project"]["flow_setup"]["status"], Value::Null);
        let page = request(&h.app, "onboarding.flow_setup_page", json!({ "project": bill.key }))
            .await
            .unwrap();
        assert_eq!(page["shows"], true);
        let settled = request(&h.app, "onboarding.flow_setup_page", json!({ "project": shop.key }))
            .await
            .unwrap();
        assert_eq!(settled["shows"], false);

        // Skip, use the default: the board, and the marks go.
        request(
            &h.app,
            "flow_editor.cancel",
            json!({ "project": bill.key, "from": "onboarding" }),
        )
        .await
        .unwrap();
        let header = request(&h.app, "shell.header", json!({ "project": bill.key }))
            .await
            .unwrap();
        assert_eq!(header["project"]["flow_setup"], Value::Null);
        let page = request(&h.app, "onboarding.flow_setup_page", json!({ "project": bill.key }))
            .await
            .unwrap();
        assert_eq!(page["shows"], false);
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_rail_header_and_switcher_come_from_the_real_service() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();

        let empty = request(&h.app, "shell.rail", json!({ "project": null }))
            .await
            .unwrap();
        assert_eq!(empty["selected"], Value::Null);
        assert_eq!(empty["items"], json!([]));

        let shop = add(&h, &repos, "shop-web").await;
        let docs = add(&h, &repos, "docs-site").await;

        // No selection yet: the first project is shown.
        let rail = request(&h.app, "shell.rail", json!({ "project": null }))
            .await
            .unwrap();
        assert_eq!(rail["selected"], shop.key);
        assert_eq!(rail["items"][1]["title"], format!("docs-site · {}", docs.key));
        assert_eq!(rail["items"][1]["idle"], true);
        assert_eq!(rail["width"], 64);
        assert_eq!(rail["bottom"]["terminals"]["label"], "Terminals");
        assert_eq!(rail["bottom"]["settings"]["label"], "Settings");

        // The page's selection is kept; a project that is gone falls back to the first.
        let rail = request(&h.app, "shell.rail", json!({ "project": docs.key }))
            .await
            .unwrap();
        assert_eq!(rail["selected"], docs.key);
        assert_eq!(rail["items"][1]["selected"], true);
        let rail = request(&h.app, "shell.rail", json!({ "project": "GONE" }))
            .await
            .unwrap();
        assert_eq!(rail["selected"], shop.key);

        assert_eq!(
            request(&h.app, "shell.project_for_digit", json!({ "digit": 2 }))
                .await
                .unwrap(),
            json!(docs.key)
        );
        assert_eq!(
            request(&h.app, "shell.project_for_digit", json!({ "digit": 3 }))
                .await
                .unwrap(),
            Value::Null
        );

        let quick = request(&h.app, "shell.quick", json!({ "filter": "docs" }))
            .await
            .unwrap();
        assert_eq!(quick["rows"].as_array().unwrap().len(), 1);
        assert_eq!(quick["rows"][0]["project"]["key"], docs.key);
        assert_eq!(quick["hints"][2]["label"], "add project");

        let header = request(&h.app, "shell.header", json!({ "project": shop.key }))
            .await
            .unwrap();
        assert_eq!(header["project"]["name"], "shop-web");
        assert_eq!(header["activity"]["agents"]["label"], "running");
        assert_eq!(header["activity"]["processes"], Value::Null);
        assert_eq!(header["activity"]["warning"], Value::Null);
        assert_eq!(header.get("inbox"), None);

        // The popover: Claude Code's block with no windows yet, nothing running, no processes. The connected tools are
        // passed in: the app's list is one global that the other tests in this binary set from their own services.
        let popover =
            serde_json::to_value(popover_with(&h.app, &[AvatarTool::ClaudeCode]).await.unwrap()).unwrap();
        assert_eq!(popover["limits"], Value::Null);
        assert_eq!(popover["agents"]["count"], "0");
        assert_eq!(
            popover["agents"]["empty"],
            "Nothing running. Tickets in Ready for Dev start when a slot and their parents allow."
        );
        assert_eq!(popover["usage"]["tools"][0]["name"], "Claude Code");
        assert_eq!(popover["usage"]["tools"][0]["avatar"], Value::Null);
        assert_eq!(popover["usage"]["tools"][0]["notes"][0], "No plan limit · tokens only");
        assert_eq!(popover["processes"], Value::Null);
        assert_eq!(popover["open_terminals"]["label"], "Open Terminals");

        let broom = request(&h.app, "shell.cleanup", Value::Null).await.unwrap();
        assert_eq!(broom["count"], 0);
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_stepper_press_saves_the_limits_at_once() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let mut settings = settings(&h.app).await.unwrap();
        settings.agent_limits.enabled = true;
        h.app
            .service
            .send(Request::SettingsSet(settings))
            .await
            .unwrap();

        let popover = request(
            &h.app,
            "shell.step_limit",
            json!({ "field": "max_agents", "up": true }),
        )
        .await
        .unwrap();
        assert_eq!(popover["max_agents"]["value"], 6);
        let popover = request(
            &h.app,
            "shell.step_limit",
            json!({ "field": "back_pressure", "up": false }),
        )
        .await
        .unwrap();
        assert_eq!(popover["back_pressure"]["value"], 2);
        let stored = super::settings(&h.app).await.unwrap().agent_limits;
        assert_eq!(
            stored,
            AgentLimits {
                enabled: true,
                max_concurrency: 6,
                back_pressure: 2
            }
        );
        let header = request(&h.app, "shell.header", json!({ "project": null }))
            .await
            .unwrap();
        assert_eq!(header["activity"]["agents"]["count"], "0/6");
        let popover = request(&h.app, "shell.activity", Value::Null).await.unwrap();
        assert_eq!(popover["limits"]["max_agents"]["value"], 6);
        h.service.stop_all().await;
    }
}

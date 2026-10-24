//! `drawer.*`: the Terminals drawer (SHL-04, RUN-06, UI-09; d0068, d0058, d0019, d0087; *17 · Project terminals*). The
//! drawer holds the shown project's Project terminals only, one tab each, and + for a new one. The last-used tab of
//! each project is kept here, so the rail's Terminals and its key (⌃`) find the last-used terminal (or open one). Project agents
//! live in the agent panel since d0087 (`agent_panel.*`): nothing here answers about them.

use otaq_core::protocol::{ProjectParams, ProjectTerminalParams, Reply, Request, SessionIdParams};
use otaq_core::session::{SessionInfo, SessionKind, SessionState};
use otaq_core::terminals::shell::ProjectTerminal;
use otaq_view::key_labels::KeyLabels;
use otaq_view::keymap::KeyAction;
use otaq_view::shell::tilde;
use otaq_view::view_models::project_terminal::{
    self as vm, HintFacts, LastUsed, NEW_TERMINAL, Tab, terminal_tab_id,
};
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "drawer.view" => reply(&view(app, parse(params)?).await?),
        "drawer.select" => reply(&select(app, parse(params)?)),
        "drawer.last_terminal" => reply(&last_terminal(app, parse(params)?).await?),
        "drawer.new" => reply(&new(app, parse(params)?).await?),
        "drawer.close" => reply(&close(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {what}: {other:?}"))
}

/// What most `drawer.*` requests take: the shown project.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "DrawerParams")]
pub struct Params {
    pub project: String,
}

async fn terminals(app: &App, project: &str) -> Result<Vec<ProjectTerminal>, RequestError> {
    match app
        .service
        .send(Request::ProjectTerminalList(ProjectParams {
            project: project.to_owned(),
        }))
        .await?
    {
        Reply::ProjectTerminals(list) => Ok(list),
        other => Err(unexpected("project_terminal.list", &other)),
    }
}

/// The drawer: its tabs, which one shows, + and the texts around a terminal.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "DrawerView")]
pub struct View {
    pub title: &'static str,
    pub tabs: Vec<Tab>,
    /// The tab shown: the project's last-used one while it is open, else the first.
    pub selected: Option<String>,
    /// + opens a new terminal straight away.
    pub new_label: &'static str,
    pub close_tab: &'static str,
    pub hide: &'static str,
    pub maximize: &'static str,
    pub restore: &'static str,
    /// What the drawer says with no tab open.
    pub empty: Vec<String>,
    pub height: f32,
}

pub struct Facts<'a> {
    pub terminals: &'a [ProjectTerminal],
    pub remembered: Option<&'a str>,
    pub project_name: &'a str,
    pub short_repo: &'a str,
    /// The user's keys: the hint names the agent panel's focus key as it works inside a terminal (KEY-08).
    pub keys: KeyLabels,
}

pub fn view_of(f: &Facts) -> View {
    let focus_keys = f.keys.in_terminal(KeyAction::FocusAgentPanel);
    let tabs = vm::tabs(
        f.terminals,
        &HintFacts {
            project_name: f.project_name,
            short_repo: f.short_repo,
            focus_keys: focus_keys.as_deref(),
        },
    );
    let selected = f
        .remembered
        .filter(|r| tabs.iter().any(|t| t.id == *r))
        .map(str::to_owned)
        .or_else(|| tabs.first().map(|t| t.id.clone()));
    View {
        title: "Terminals",
        tabs,
        selected,
        new_label: NEW_TERMINAL,
        close_tab: "Close tab",
        hide: "Hide \u{b7} terminals keep running",
        maximize: "Maximize",
        restore: "Restore",
        empty: vm::empty_lines(f.short_repo),
        height: vm::DRAWER_HEIGHT,
    }
}

fn remembered(app: &App, project: &str) -> Option<String> {
    app.drawer_tabs
        .lock()
        .ok()
        .and_then(|m| m.get(project).cloned())
}

fn remember(app: &App, project: &str, tab: &str) {
    if let Ok(mut m) = app.drawer_tabs.lock() {
        m.insert(project.to_owned(), tab.to_owned());
    }
}

pub async fn view(app: &App, params: Params) -> Result<View, RequestError> {
    let terminals = terminals(app, &params.project).await?;
    let (name, repo) = match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => list
            .into_iter()
            .find(|p| p.key == params.project)
            .map(|p| (p.name, tilde(&p.repo_path)))
            .unwrap_or_else(|| (params.project.clone(), String::new())),
        other => return Err(unexpected("project.list", &other)),
    };
    let remembered = remembered(app, &params.project);
    Ok(view_of(&Facts {
        terminals: &terminals,
        remembered: remembered.as_deref(),
        project_name: &name,
        short_repo: &repo,
        keys: super::keymap::labels(app).await,
    }))
}

/// What `drawer.select` and `drawer.close` take: a tab of the project's drawer.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "DrawerTabParams")]
pub struct TabParams {
    pub project: String,
    pub tab: String,
}

/// The user picked a tab: it is the last used one now.
pub fn select(app: &App, params: TabParams) {
    remember(app, &params.project, &params.tab);
}

async fn open_terminal(app: &App, project: &str, new: bool) -> Result<String, RequestError> {
    match app
        .service
        .send(Request::ProjectTerminalOpen(ProjectTerminalParams {
            project: project.to_owned(),
            new,
        }))
        .await
    {
        Ok(Reply::ProjectTerminal(t)) => Ok(terminal_tab_id(&t.session.id)),
        Ok(other) => Err(unexpected("project_terminal.open", &other)),
        Err(e) => {
            toast_error(app, "The terminal didn\u{2019}t open", &e);
            Err(e)
        }
    }
}

fn toast_error(app: &App, title: &str, error: &RequestError) {
    app.notices.toast(NewToast::new(
        ToastLevel::Error,
        title,
        Some(error.message.clone()),
    ));
}

/// Terminals (⌃`): the tab to show, the last-used terminal or a new one (`project_terminal.open {new}`).
pub async fn last_terminal(app: &App, params: Params) -> Result<String, RequestError> {
    let open = terminals(app, &params.project).await?;
    let tab = match vm::last_used(&open, remembered(app, &params.project).as_deref()) {
        LastUsed::Show(tab) => tab,
        LastUsed::OpenNew => open_terminal(app, &params.project, true).await?,
    };
    remember(app, &params.project, &tab);
    Ok(tab)
}

/// +: a new terminal ("Terminal 3"), the tab to show.
pub async fn new(app: &App, params: Params) -> Result<String, RequestError> {
    let tab = open_terminal(app, &params.project, true).await?;
    remember(app, &params.project, &tab);
    Ok(tab)
}

/// Closing a terminal's tab ends its shell.
pub async fn close(app: &App, params: TabParams) -> Result<(), RequestError> {
    let Some(session) = params.tab.strip_prefix("terminal:") else {
        return Err(RequestError::plain(format!("not a tab: {}", params.tab)));
    };
    if let Err(e) = app
        .service
        .send(Request::ProjectTerminalClose(SessionIdParams {
            session: session.to_owned(),
        }))
        .await
    {
        toast_error(app, "The tab didn\u{2019}t close", &e);
        return Err(e);
    }
    Ok(())
}

/// One state of the drawer in the gallery.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "DrawerGalleryState")]
pub struct GalleryState {
    pub name: &'static str,
    pub view: View,
}

/// The drawer in the states of *17 · Project terminals* (T-0331): a fresh Terminal 1 with its hint, two terminals
/// with Terminal 2 exited, and the empty drawer after a restart.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "DrawerGallery")]
pub struct Gallery {
    pub states: Vec<GalleryState>,
}

/// A Project terminal of the gallery: number `number`, its shell running or exited.
fn gallery_terminal(number: u32, running: bool) -> ProjectTerminal {
    ProjectTerminal {
        number,
        name: format!("Terminal {number}"),
        session: SessionInfo {
            id: format!("gallery-t{number}"),
            kind: SessionKind::Project,
            project: "NOTE".into(),
            ticket: None,
            state: if running {
                SessionState::Running
            } else {
                SessionState::Ended
            },
            started_at: 0,
            ended_at: (!running).then_some(1_791_600_000_000),
            end_reason: (!running).then(|| "exit 0".to_owned()),
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            model_ran: None,
            size: None,
        },
    }
}

pub fn gallery() -> Gallery {
    let terminal = gallery_terminal;
    let view = |terminals: &[ProjectTerminal], remembered: Option<&str>| {
        view_of(&Facts {
            terminals,
            remembered,
            project_name: "pocket-notes",
            short_repo: "~/code/pocket-notes",
            keys: KeyLabels::default(),
        })
    };
    let two = [terminal(1, true), terminal(2, false)];
    Gallery {
        states: vec![
            GalleryState {
                name: "Empty \u{b7} shell prompt (Terminal 1)",
                view: view(&two[..1], None),
            },
            GalleryState {
                name: "Ended \u{b7} the shell exited (Terminal 2)",
                view: view(&two, Some("terminal:gallery-t2")),
            },
            GalleryState {
                name: "After a restart \u{b7} terminals are gone",
                view: view(&[], None),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    #[test]
    fn the_shown_tab_is_the_last_used_while_open() {
        let facts = |remembered| Facts {
            terminals: &[],
            remembered,
            project_name: "shop-web",
            short_repo: "~/code/shop-web",
            keys: KeyLabels::base(otaq_view::keymap::Platform::Mac),
        };
        let empty = view_of(&facts(Some("terminal:gone")));
        assert_eq!(empty.selected, None);
        assert!(empty.empty[0].contains("~/code/shop-web"));
        assert_eq!(
            (empty.title, empty.new_label, empty.hide),
            ("Terminals", "New terminal", "Hide \u{b7} terminals keep running")
        );
        assert_eq!(empty.height, 380.);
    }

    /// KEY-08: the hint names the user's key for focusing the agent panel as it works inside a terminal (⌘L; Ctrl+Shift+L in
    /// a Windows/Linux terminal), a recorded key that works there, and no key once removed.
    #[test]
    fn the_hint_names_the_users_agent_panel_key() {
        use otaq_view::key_labels::fixtures as keys;
        use otaq_view::keymap::Platform;
        let fresh = [gallery_terminal(1, true)];
        let last = |labels: KeyLabels| {
            let view = view_of(&Facts {
                terminals: &fresh,
                remembered: None,
                project_name: "pocket-notes",
                short_repo: "~/code/pocket-notes",
                keys: labels,
            });
            view.tabs[0].hint.last().cloned().unwrap()
        };
        let ask = "Want an agent\u{2019}s help? Ask the Project agent in the Ask bar";
        assert_eq!(last(keys::base()), format!("{ask} (\u{2318}L)."));
        assert_eq!(
            last(KeyLabels::base(Platform::WinLinux)),
            format!("{ask} (Ctrl+Shift+L).")
        );
        assert_eq!(
            last(keys::replaced("focus_agent_panel", None, 0, "⌥⌘L")),
            format!("{ask} (⌥⌘L).")
        );
        // A recorded ⌥ key stays the terminal's there (d0097), so the hint names none.
        assert_eq!(
            last(keys::replaced("focus_agent_panel", None, 0, "⌥L")),
            format!("{ask}.")
        );
        assert_eq!(last(keys::removed("focus_agent_panel", None, 0)), format!("{ask}."));
    }

    #[test]
    fn the_gallery_shows_the_three_frames_with_terminals_only() {
        let g = gallery();
        let names: Vec<&str> = g.states.iter().map(|s| s.name).collect();
        assert_eq!(names.len(), 3);
        let fresh = &g.states[0].view;
        assert_eq!(fresh.tabs[0].label, "Terminal 1");
        assert_eq!(fresh.tabs[0].hint.len(), 4);
        let ended = &g.states[1].view;
        assert_eq!(ended.selected.as_deref(), Some("terminal:gallery-t2"));
        assert!(ended.tabs[1].ended_line.is_some());
        assert!(g.states[2].view.tabs.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn terminals_open_show_and_close_through_the_drawer() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("shop-web", 1);
        let project = match h
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
            Reply::ProjectAdded { project: p, .. } => p.key,
            other => panic!("{other:?}"),
        };
        let p = json!({ "project": project });

        let view = table(&h.app, "drawer.view", p.clone()).await.unwrap();
        assert_eq!(view["tabs"], json!([]));
        assert_eq!(view["selected"], Value::Null);
        assert!(
            table(&h.app, "drawer.button", p.clone()).await.is_err(),
            "no header button any more: the rail's Terminals is the shell's"
        );

        // Terminals with nothing open: a new terminal, shown.
        let first = table(&h.app, "drawer.last_terminal", p.clone())
            .await
            .unwrap();
        let first = first.as_str().unwrap().to_owned();
        assert!(first.starts_with("terminal:"));
        // + makes a terminal straight away.
        let second = table(&h.app, "drawer.new", p.clone()).await.unwrap();
        let view = table(&h.app, "drawer.view", p.clone()).await.unwrap();
        let labels: Vec<&str> = view["tabs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["label"].as_str().unwrap())
            .collect();
        assert_eq!(labels, ["Terminal 1", "Terminal 2"]);
        assert!(
            view["tabs"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| t["id"].as_str().unwrap().starts_with("terminal:")),
            "terminals only"
        );
        let hint = view["tabs"][1]["hint"].as_array().unwrap();
        assert_eq!(hint[1], "Run project commands, or make tickets yourself:");
        assert_eq!(view["selected"], second, "the newest pick");

        // Picking Terminal 1 makes it the last used: Terminals shows it.
        table(
            &h.app,
            "drawer.select",
            json!({ "project": project, "tab": first }),
        )
        .await
        .unwrap();
        let again = table(&h.app, "drawer.last_terminal", p.clone())
            .await
            .unwrap();
        assert_eq!(again, json!(first));

        let closed = table(
            &h.app,
            "drawer.close",
            json!({ "project": project, "tab": first }),
        )
        .await
        .unwrap();
        assert_eq!(closed, Value::Null);
        let view = table(&h.app, "drawer.view", p.clone()).await.unwrap();
        assert_eq!(view["tabs"].as_array().unwrap().len(), 1);
        assert_eq!(view["selected"], second, "the closed one is gone");
        h.service.stop_all().await;
    }
}

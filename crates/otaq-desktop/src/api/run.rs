//! `run.*`: the Run tab, Open URL, the run-command dialog, the running processes and the Shell tab (RUN-01…RUN-05;
//! d0026, d0035, d0046; *13 · Run & Shell*). The views come from `otaq-view`'s `run_page.rs`; the run-command
//! dialog's state is a `run_command::RunCommandModel` per project, kept in the app between requests, with Ask
//! the agent's progress handed back by the page from the `run_suggestion` events. Nothing starts without a click:
//! Run, Ask the agent (P1) and the Shell's first open are each the user's.

use std::time::{SystemTime, UNIX_EPOCH};

use otaq_core::board::tickets::Ticket;
use otaq_core::protocol::{
    KeyParams, ProjectParams, ProjectSettingsParams, Reply, Request, ShellOpenParams,
};
use otaq_core::terminals::run::RunInfo;
use otaq_core::terminals::suggest::SuggestUpdate;
use otaq_view::view_models::clock;
use otaq_view::view_models::agent_panel;
use otaq_view::view_models::run_command::RunCommandModel;
use otaq_view::view_models::run_page::{
    self as page, CommandView, FillView, ProcessesView, ShellFacts, ShellView, TabView,
};
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "run.tab" => reply(&tab(app, parse(params)?).await?),
        "run.start" => reply(&start(app, parse(params)?).await?),
        "run.stop" => reply(&stop(app, parse(params)?).await?),
        "run.open" => reply(&open(parse(params)?)?),
        "run.processes" => reply(&processes(app).await?),
        "run.stop_all" => reply(&stop_all(app).await?),
        "run.command_open" => reply(&command_open(app, parse(params)?).await?),
        "run.command_ask" => reply(&command_ask(app, parse(params)?).await?),
        "run.command_suggestion" => reply(&command_suggestion(app, parse(params)?)?),
        "run.command_edit" => reply(&command_edit(app, parse(params)?)?),
        "run.command_save" => reply(&command_save(app, parse(params)?).await?),
        "run.command_cancel" => reply(&command_cancel(app, parse(params)?).await?),
        "run.shell" => reply(&shell(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {what}: {other:?}"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn toast_error(app: &App, title: String, e: &RequestError) {
    app.notices.toast(NewToast::new(
        ToastLevel::Error,
        title,
        Some(e.message.clone()),
    ));
}

async fn status(app: &App, key: &str) -> Result<RunInfo, RequestError> {
    match app
        .service
        .send(Request::RunStatus(KeyParams {
            key: key.to_owned(),
        }))
        .await?
    {
        Reply::Run(run) => Ok(*run),
        other => Err(unexpected("run.status", &other)),
    }
}

/// The open ticket.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "RunParams")]
pub struct Params {
    pub key: String,
}

pub async fn tab(app: &App, params: Params) -> Result<TabView, RequestError> {
    Ok(page::tab_view(
        &status(app, &params.key).await?,
        clock::local_hhmm,
    ))
}

/// How a Run click went: the tab as it is now, or the project has no run command yet and the dialog asks for one.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(rename = "RunStarted")]
pub enum Started {
    Tab { view: Box<TabView> },
    NeedsCommand { project: String },
}

/// Run (and Retry): refused without a run command, which opens the dialog (RUN-02).
pub async fn start(app: &App, params: Params) -> Result<Started, RequestError> {
    match app
        .service
        .send(Request::RunStart(KeyParams {
            key: params.key.clone(),
        }))
        .await
    {
        Ok(Reply::Run(run)) => Ok(Started::Tab {
            view: Box::new(page::tab_view(&run, clock::local_hhmm)),
        }),
        Ok(other) => Err(unexpected("run.start", &other)),
        Err(e) if e.refusal.as_deref() == Some("no_run_command") => Ok(Started::NeedsCommand {
            project: params
                .key
                .parse::<otaq_core::domain::TicketKey>()
                .map_or(params.key.clone(), |k| k.project.to_string()),
        }),
        Err(e) => {
            toast_error(app, format!("{} can\u{2019}t run", params.key), &e);
            Err(e)
        }
    }
}

pub async fn stop(app: &App, params: Params) -> Result<TabView, RequestError> {
    match app
        .service
        .send(Request::RunStop(KeyParams {
            key: params.key.clone(),
        }))
        .await
    {
        Ok(_) => tab(app, params).await,
        Err(e) => {
            toast_error(app, format!("{}\u{2019}s Run didn\u{2019}t stop", params.key), &e);
            Err(e)
        }
    }
}

/// Open: the substituted Open URL in the system browser.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "RunOpenParams")]
pub struct OpenParams {
    pub url: String,
}

pub fn open(params: OpenParams) -> Result<(), RequestError> {
    if !(params.url.starts_with("http://") || params.url.starts_with("https://")) {
        return Err(RequestError::plain(format!(
            "not a web address: {}",
            params.url
        )));
    }
    open::that_detached(&params.url)
        .map_err(|e| RequestError::plain(format!("couldn't open {}: {e}", params.url)))
}

async fn runs(app: &App) -> Result<Vec<RunInfo>, RequestError> {
    match app.service.send(Request::RunList).await? {
        Reply::Runs(list) => Ok(list),
        other => Err(unexpected("run.list", &other)),
    }
}

/// The header's "▶ 3 processes" menu (RUN-04).
pub async fn processes(app: &App) -> Result<ProcessesView, RequestError> {
    Ok(page::processes_view(&runs(app).await?, now_ms()))
}

/// Stop all: every running Run, across projects.
pub async fn stop_all(app: &App) -> Result<ProcessesView, RequestError> {
    for run in runs(app).await?.into_iter().filter(|r| r.state.is_live()) {
        let key = run.key.to_string();
        if let Err(e) = app.service.send(Request::RunStop(KeyParams { key: key.clone() })).await {
            toast_error(app, format!("{key}\u{2019}s Run didn\u{2019}t stop"), &e);
        }
    }
    processes(app).await
}

/// The run-command dialog for a project, from a ticket's Run (its Run starts after Save) or from Settings.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "RunCommandOpenParams")]
pub struct CommandOpenParams {
    pub project: String,
    #[serde(default)]
    pub ticket: Option<String>,
}

/// The dialog as it opens, with the fields' first values (the project's settings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "RunCommandOpened")]
pub struct CommandOpened {
    pub view: CommandView,
    pub command: String,
    pub open_url: String,
}

async fn settings(
    app: &App,
    project: &str,
) -> Result<otaq_core::board::settings::ProjectSettings, RequestError> {
    match app
        .service
        .send(Request::ProjectSettingsGet(ProjectParams {
            project: project.to_owned(),
        }))
        .await?
    {
        Reply::ProjectSettings(reply) => Ok(reply.settings),
        other => Err(unexpected("project.settings_get", &other)),
    }
}

pub async fn command_open(
    app: &App,
    params: CommandOpenParams,
) -> Result<CommandOpened, RequestError> {
    let settings = settings(app, &params.project).await?;
    let run = match &params.ticket {
        Some(key) => status(app, key).await.ok(),
        None => None,
    };
    let name = match &run {
        Some(r) => r.project_name.clone(),
        None => match app.service.send(Request::ProjectList).await {
            Ok(Reply::Projects(list)) => list
                .into_iter()
                .find(|p| p.key == params.project)
                .map_or(params.project.clone(), |p| p.name),
            _ => params.project.clone(),
        },
    };
    // Ask the agent asks the project's Project agent (d0086).
    let agent = super::agent_panel::project_agent(app).await?;
    // Hidden only when no agent tool is connected (RUN-02, d0086).
    let any_tool = super::editor::tool_states(app).await.any();
    let model = RunCommandModel::new(
        &params.project,
        &name,
        &agent_panel::new_agent_name(agent.as_ref(), &super::agent_panel::tools(app).await),
        run.as_ref(),
        &settings.run_command,
    )
    .with_tool_connected(any_tool);
    let view = page::command_view(&model, &settings.run_command, None);
    app.run_commands
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(params.project, model);
    Ok(CommandOpened {
        view,
        command: settings.run_command,
        open_url: settings.open_url,
    })
}

/// Runs `f` on the project's dialog and answers its view with the command as typed now.
fn with_model(
    app: &App,
    project: &str,
    command: &str,
    f: impl FnOnce(&mut RunCommandModel) -> Option<FillView>,
) -> Result<CommandView, RequestError> {
    let mut models = app.run_commands.lock().unwrap_or_else(|e| e.into_inner());
    let model = models
        .get_mut(project)
        .ok_or_else(|| RequestError::plain(format!("no run-command dialog for {project}")))?;
    let fill = f(model);
    let command = fill.as_ref().map_or(command, |f| f.command.as_str());
    Ok(page::command_view(model, command, fill.clone()))
}

/// A field changed, or Ask the agent / Cancel was clicked: the project and the command as typed.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "RunCommandParams")]
pub struct CommandParams {
    pub project: String,
    #[serde(default)]
    pub command: String,
}

/// **Ask the agent** (RUN-02, P1: the user's click): progress and the answer arrive as `run_suggestion` events.
pub async fn command_ask(app: &App, params: CommandParams) -> Result<CommandView, RequestError> {
    let asked = with_model(app, &params.project, &params.command, |m| {
        m.ask();
        None
    })?;
    if asked.thinking.is_none() {
        return Ok(asked);
    }
    match app
        .service
        .send(Request::RunSuggest(ProjectParams {
            project: params.project.clone(),
        }))
        .await
    {
        Ok(_) => Ok(asked),
        Err(e) => with_model(app, &params.project, &params.command, |m| {
            m.failed(&e.message, &params.command);
            None
        }),
    }
}

/// A `run_suggestion` event, handed back by the page: the agent looked at something, answered, or gave up.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "RunCommandSuggestionParams")]
pub struct SuggestionParams {
    pub project: String,
    #[serde(default)]
    pub command: String,
    #[ts(type = "unknown")]
    pub update: SuggestUpdate,
}

pub fn command_suggestion(
    app: &App,
    params: SuggestionParams,
) -> Result<CommandView, RequestError> {
    with_model(app, &params.project, &params.command, |m| match &params.update {
        SuggestUpdate::Progress { line } => {
            m.progress(line);
            None
        }
        SuggestUpdate::Done { suggestion } => m
            .suggested(suggestion)
            .map(|(command, open_url)| FillView { command, open_url }),
        SuggestUpdate::Failed { message } => {
            m.failed(message, &params.command);
            None
        }
    })
}

pub fn command_edit(app: &App, params: CommandParams) -> Result<CommandView, RequestError> {
    with_model(app, &params.project, &params.command, |m| {
        m.edited();
        None
    })
}

/// **Save (& start)**: both fields go into the project's settings (never the repo, d0023), then the ticket's Run
/// starts.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "RunCommandSaveParams")]
pub struct SaveParams {
    pub project: String,
    pub command: String,
    #[serde(default)]
    pub open_url: String,
}

pub async fn command_save(app: &App, params: SaveParams) -> Result<CommandView, RequestError> {
    let (can, ticket) = {
        let models = app.run_commands.lock().unwrap_or_else(|e| e.into_inner());
        let model = models.get(&params.project).ok_or_else(|| {
            RequestError::plain(format!("no run-command dialog for {}", params.project))
        })?;
        (
            model.can_save(&params.command),
            model.ticket.as_ref().map(|(k, _)| k.to_string()),
        )
    };
    if !can {
        return with_model(app, &params.project, &params.command, |_| None);
    }
    let saved = match settings(app, &params.project).await {
        Ok(mut s) => {
            s.run_command = params.command.trim().to_owned();
            s.open_url = params.open_url.trim().to_owned();
            app.service
                .send(Request::ProjectSettingsSet(ProjectSettingsParams {
                    project: params.project.clone(),
                    settings: Box::new(s),
                }))
                .await
                .map(|_| ())
        }
        Err(e) => Err(e),
    };
    if let Err(e) = saved {
        return with_model(app, &params.project, &params.command, |m| {
            m.error = Some(e.message.clone());
            None
        });
    }
    let view = with_model(app, &params.project, &params.command, |m| {
        m.saved();
        None
    })?;
    if let Some(key) = ticket
        && let Err(e) = app.service.send(Request::RunStart(KeyParams { key: key.clone() })).await
    {
        toast_error(app, format!("{key} can\u{2019}t run"), &e);
    }
    Ok(view)
}

/// Cancel, Esc or Done: a running Ask the agent stops (its process is killed); nothing is saved.
pub async fn command_cancel(app: &App, params: CommandParams) -> Result<(), RequestError> {
    let model = app
        .run_commands
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&params.project);
    if model.is_some_and(|m| m.thinking()) {
        let _ = app
            .service
            .send(Request::RunSuggestCancel(ProjectParams {
                project: params.project,
            }))
            .await;
    }
    Ok(())
}

/// The Shell tab shows: the shell opens the first time (the service keeps it); `reopen` is the user's Reopen.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ShellTabParams")]
pub struct ShellTabParams {
    pub key: String,
    #[serde(default)]
    pub reopen: bool,
}

async fn ticket(app: &App, key: &str) -> Result<Ticket, RequestError> {
    match app
        .service
        .send(Request::TicketGet(KeyParams {
            key: key.to_owned(),
        }))
        .await?
    {
        Reply::Ticket(t) => Ok(*t),
        other => Err(unexpected("ticket.get", &other)),
    }
}

pub async fn shell(app: &App, params: ShellTabParams) -> Result<ShellView, RequestError> {
    let t = ticket(app, &params.key).await?;
    let opened = app
        .service
        .send(Request::ShellOpen(ShellOpenParams {
            key: params.key.clone(),
            reopen: params.reopen,
        }))
        .await;
    let (session, problem) = match opened {
        Ok(Reply::Session(s)) => (Some(*s), None),
        Ok(other) => return Err(unexpected("shell.open", &other)),
        Err(e) => (None, Some(e.message)),
    };
    let work = t.work.clone().unwrap_or_default();
    Ok(page::shell_view(
        ShellFacts {
            key: &params.key,
            worktree: work.worktree.as_deref(),
            port: work.run_port,
            session: session.as_ref(),
            problem,
            keys: &super::keymap::labels(app).await,
        },
        clock::local_hhmm,
    ))
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::board::tickets::NewTicketInput;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn run_asks_for_a_command_first_and_the_dialog_saves_it_in_settings() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("shop-web", 1);
        let Reply::ProjectAdded { project, .. } = h
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
        else {
            panic!("no project");
        };
        let Reply::Ticket(created) = h
            .app
            .service
            .send(Request::TicketCreate(NewTicketInput {
                project: project.key.clone(),
                title: "Price table".into(),
                body: String::new(),
                plan: String::new(),
                priority: None,
                base_branch: None,
                external_id: None,
                first_comment: None,
            }))
            .await
            .unwrap()
        else {
            panic!("no ticket");
        };
        let key = created.key.to_string();

        let view = table(&h.app, "run.tab", json!({ "key": key })).await.unwrap();
        assert_eq!(view["state"]["text"], "Not running");
        assert_eq!(view["session"], Value::Null);

        // Not started: no worktree to run in (the refusal is a toast); "no_run_command" opens the dialog instead.
        let refused = table(&h.app, "run.start", json!({ "key": key }))
            .await
            .unwrap_err();
        assert_eq!(refused.refusal.as_deref(), Some("no_worktree"));

        let opened = table(
            &h.app,
            "run.command_open",
            json!({ "project": project.key }),
        )
        .await
        .unwrap();
        assert_eq!(opened["view"]["title"], "What command runs shop-web?");
        assert_eq!(opened["command"], "");
        // The test service has no agent tool on its PATH: Ask the agent is hidden (RUN-02, d0086); the dialog
        // still saves a typed command.
        assert_eq!(opened["view"]["ask"], Value::Null, "no agent tool connected");
        assert_eq!(opened["view"]["save"], "Save", "no ticket: Save only");

        let edited = table(
            &h.app,
            "run.command_edit",
            json!({ "project": project.key, "command": "sh -c 'sleep 30'" }),
        )
        .await
        .unwrap();
        assert_eq!(edited["can_save"], true);
        let saved = table(
            &h.app,
            "run.command_save",
            json!({ "project": project.key, "command": " sh -c 'sleep 30' ", "open_url": "" }),
        )
        .await
        .unwrap();
        assert!(
            saved["saved"].as_str().unwrap().starts_with("Saved for shop-web."),
            "{saved}"
        );
        let Reply::ProjectSettings(reply) = h
            .app
            .service
            .send(Request::ProjectSettingsGet(ProjectParams {
                project: project.key.clone(),
            }))
            .await
            .unwrap()
        else {
            panic!("no settings");
        };
        assert_eq!(reply.settings.run_command, "sh -c 'sleep 30'");
        table(&h.app, "run.command_cancel", json!({ "project": project.key }))
            .await
            .unwrap();
        assert!(h.app.run_commands.lock().unwrap().is_empty());

        let processes = table(&h.app, "run.processes", Value::Null).await.unwrap();
        assert_eq!(processes["rows"], json!([]));
        assert!(
            table(&h.app, "run.open", json!({ "url": "file:///etc/passwd" }))
                .await
                .is_err(),
            "only web addresses open"
        );
        // A never-started ticket has no worktree: the Shell says why instead of opening.
        let shell = table(&h.app, "run.shell", json!({ "key": key })).await.unwrap();
        assert_eq!(shell["session"], Value::Null);
        assert!(shell["problem"].is_string(), "{shell}");
        h.service.stop_all().await;
    }
}

//! `palette.*`: the command palette (KEY-06, d0097; *02 · Command palette*). `palette.view` answers the list for what
//! is typed, from every project's tickets, the user's keys and the recent items the service keeps; which actions can
//! run is decided from the ticket open in the sheet. `palette.record` keeps a use for the recent items: the page sends
//! every action run (palette or keys), ticket opened and project switched to, from anywhere.

use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::board::tickets::Ticket;
use otaq_core::protocol::{ProjectParams, RecentItemsParams, Reply, Request};
use otaq_core::store::recents::{RecentItem, RecentUse};
use otaq_view::keymap::{self, KeyAction, Platform};
use otaq_view::keymap_edit;
use otaq_view::view_models::palette::{self, Context, Inputs, OpenTicket, View};
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "palette.view" => reply(&view(app, parse(params)?).await?),
        "palette.record" => {
            record(app, parse(params)?).await?;
            reply(&Value::Null)
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

/// What `palette.view` takes: the field's text and where the palette was opened.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(rename = "PaletteParams")]
pub struct ViewParams {
    pub query: String,
    /// The project on screen.
    pub project: Option<String>,
    /// The ticket open in the sheet.
    pub ticket: Option<String>,
    /// A terminal had the keyboard when the palette opened.
    #[serde(default)]
    pub terminal: bool,
}

/// What `palette.record` takes: one use. `PaletteUse` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "PaletteUse")]
pub enum Use {
    Action { action: KeyAction },
    Ticket { key: String },
    Project { project: String },
}

async fn projects(app: &App) -> Result<Vec<ProjectInfo>, RequestError> {
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(projects) => Ok(projects),
        other => Err(unexpected("project.list", &other)),
    }
}

async fn tickets(app: &App, project: &str) -> Result<Vec<Ticket>, RequestError> {
    match app
        .service
        .send(Request::TicketList(ProjectParams {
            project: project.to_owned(),
        }))
        .await?
    {
        Reply::Tickets(list) => Ok(list),
        other => Err(unexpected("ticket.list", &other)),
    }
}

async fn recent(app: &App, record: Option<RecentUse>) -> Result<Vec<RecentItem>, RequestError> {
    match app
        .service
        .send(Request::RecentItems(RecentItemsParams { record }))
        .await?
    {
        Reply::RecentItems(items) => Ok(items),
        other => Err(unexpected("recent.items", &other)),
    }
}

/// The palette for `params.query`. A project whose tickets don't load is searched by its name only; the recent items
/// and the keys fall back to none and the base map.
pub async fn view(app: &App, params: ViewParams) -> Result<View, RequestError> {
    let projects = projects(app).await?;
    let mut all = Vec::new();
    for p in &projects {
        all.extend(tickets(app, &p.key).await.unwrap_or_default());
    }
    let recent = recent(app, None).await.unwrap_or_default();
    let keyboard = super::keyboard::global_settings(app)
        .await
        .map(|s| s.keyboard)
        .unwrap_or_default();
    let bindings = keymap::user_keys(Platform::current(), &keymap_edit::custom_keys(&keyboard));
    let open = params
        .ticket
        .as_deref()
        .and_then(|key| all.iter().find(|t| t.key.to_string() == key))
        .map(OpenTicket::of);
    // Go to file searches the Files view when it is the shown project's view (FIL-11).
    let files_view = match params.project.as_deref() {
        Some(project) => {
            super::board::view_prefs(app, project).await.view == otaq_core::board::settings::BoardView::Files
        }
        None => false,
    };
    let inputs = Inputs {
        bindings: &bindings,
        letters: keyboard.letters,
        project: params.project.as_deref(),
        projects: &projects,
        tickets: &all,
        recent: &recent,
        context: Context {
            open,
            terminal: params.terminal,
            files_view,
        },
    };
    Ok(palette::view(&params.query, &inputs))
}

/// Keeps a use for the recent items; an action that doesn't go there ([`palette::recorded`]) is let go.
pub async fn record(app: &App, used: Use) -> Result<(), RequestError> {
    let used = match used {
        Use::Action { action } if palette::recorded(action) => RecentUse::Action {
            action: serde_json::to_value(action).map_err(|e| RequestError::plain(e.to_string()))?,
        },
        Use::Action { .. } => return Ok(()),
        Use::Ticket { key } => RecentUse::Ticket { key },
        Use::Project { project } => RecentUse::Project { project },
    };
    recent(app, Some(used)).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::board::tickets::NewTicketInput;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    async fn add(h: &Harness, repos: &TempRepos, name: &str, key: &str) {
        let path = repos.repo(name, 1);
        h.app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: Some(key.into()),
                base_branch: None,
                flow_setup: false,
            }))
            .await
            .unwrap();
    }

    async fn ticket(h: &Harness, project: &str, title: &str) {
        h.app
            .service
            .send(Request::TicketCreate(NewTicketInput {
                project: project.into(),
                title: title.into(),
                body: String::new(),
                plan: String::new(),
                priority: None,
                base_branch: None,
                external_id: None,
                first_comment: None,
            }))
            .await
            .unwrap();
    }

    fn words(view: &Value) -> Vec<String> {
        view["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let i = &r["item"];
                match i["kind"].as_str().unwrap() {
                    "ticket" => format!("{} {}", i["key"].as_str().unwrap(), i["other_project"]),
                    "divider" => i["label"].as_str().unwrap().to_owned(),
                    _ => i["title"].as_str().unwrap().to_owned(),
                }
            })
            .collect()
    }

    /// The example's NOTE and API: "export" finds NOTE-2, then API-1 under Other projects; uses are kept by the
    /// service, newest first, across projects, and only actions that go there.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_palette_searches_every_project_and_keeps_recent_items() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        add(&h, &repos, "pocket-notes", "NOTE").await;
        add(&h, &repos, "pocket-api", "API").await;
        ticket(&h, "NOTE", "Sort notes by date").await;
        ticket(&h, "NOTE", "Export as CSV").await;
        ticket(&h, "API", "Rate limit the export").await;

        let params = |query: &str| json!({ "query": query, "project": "NOTE", "ticket": null });
        let view = request(&h.app, "palette.view", params("export")).await.unwrap();
        assert_eq!(
            words(&view),
            ["NOTE-2 null", "Other projects", "API-1 \"· pocket-api\""]
        );
        let view = request(&h.app, "palette.view", params("app")).await.unwrap();
        assert_eq!(view["rows"][0]["item"]["title"], "Approve ▾");
        assert_eq!(view["rows"][0]["item"]["reason"], "· no ticket in review");
        assert_eq!(view["rows"][0]["pickable"], false);

        for used in [
            json!({ "kind": "ticket", "key": "API-1" }),
            json!({ "kind": "action", "action": { "action": "next_ticket" } }),
            json!({ "kind": "action", "action": { "action": "park" } }),
            json!({ "kind": "ticket", "key": "NOTE-2" }),
            json!({ "kind": "project", "project": "API" }),
        ] {
            request(&h.app, "palette.record", used).await.unwrap();
        }
        let view = request(&h.app, "palette.view", params("")).await.unwrap();
        assert_eq!(view["section"], "Recent");
        assert_eq!(
            words(&view),
            ["Switch to pocket-api", "NOTE-2 null", "Park", "API-1 \"· pocket-api\""],
            "next ticket isn't kept"
        );
        assert!(request(&h.app, "palette.nothing", Value::Null).await.is_err());
        h.service.stop_all().await;
    }
}

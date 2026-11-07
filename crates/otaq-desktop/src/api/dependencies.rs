//! `dependencies.*`: the sheet's Dependencies section (DPV-04; d0006, d0049; *24 → Adding and removing* `s24-11`),
//! in Overview under the description: Waits for and Waiting for it with where each ticket is now
//! (`dependencies.view`). A link is added only from the waiting ticket: Add searches by key or title for a ticket it
//! will wait for (`dependencies.search`, which the New ticket form's Waits for uses too, before the ticket exists); a
//! pick or a Waits for row's ✕ sends `dependencies.link`, which the service checks (loops, bases, who may) and answers
//! with DPV-04's words for a loop. The words are `otaq_view::view_models::dependencies`'s.

use otaq_core::board::tickets::Ticket;
use otaq_core::protocol::{DepParams, KeyParams, ProjectParams, Reply, Request};
use otaq_core::domain::TicketKey;
use otaq_view::view_models::dependencies::{self, SearchRow, Section};
use otaq_view::view_models::flow_columns::Columns;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

/// How many tickets the picker lists at most.
const SEARCH_LIMIT: usize = 8;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "dependencies.view" => reply(&view(app, parse(params)?).await?),
        "dependencies.search" => reply(&search(app, parse(params)?).await?),
        "dependencies.link" => reply(&link(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

/// The page's types (`generated.rs`).
#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    // The sheet's Start (`board.start`), which opens DPV-03's dialog for a blocked ticket.
    super::board::StartParams::export_all(cfg)?;
    ViewParams::export_all(cfg)?;
    Section::export_all(cfg)?;
    SearchParams::export_all(cfg)?;
    SearchFor::export_all(cfg)?;
    SearchRow::export_all(cfg)?;
    LinkParams::export_all(cfg)?;
    Linked::export_all(cfg)?;
    otaq_view::view_models::dependencies::Dialog::export_all(cfg)?;
    otaq_view::view_models::dependencies::Blocks::export_all(cfg)?;
    otaq_view::view_models::dependencies::Gallery::export_all(cfg)
}

/// The open ticket and its project's tickets; `None` when it is gone.
async fn with_project(app: &App, key: &str) -> Result<Option<(Ticket, Vec<Ticket>)>, RequestError> {
    let ticket = match app
        .service
        .send(Request::TicketGet(KeyParams { key: key.to_owned() }))
        .await
    {
        Ok(Reply::Ticket(t)) => *t,
        Ok(_) => return Ok(None),
        Err(e) if e.not_found => return Ok(None),
        Err(e) => return Err(e),
    };
    let all = tickets(app, &ticket.key.project.to_string()).await?;
    Ok(Some((ticket, all)))
}

/// The tickets of `project`.
async fn tickets(app: &App, project: &str) -> Result<Vec<Ticket>, RequestError> {
    match app
        .service
        .send(Request::TicketList(ProjectParams {
            project: project.to_owned(),
        }))
        .await?
    {
        Reply::Tickets(all) => Ok(all),
        other => Err(RequestError::plain(format!(
            "unexpected answer to ticket.list: {other:?}"
        ))),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// What `dependencies.view` takes: the ticket the sheet shows.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "DependenciesParams")]
pub struct ViewParams {
    pub key: String,
}

/// The section, or `null` for a ticket that is gone.
pub async fn view(app: &App, params: ViewParams) -> Result<Option<Section>, RequestError> {
    let Some((ticket, all)) = with_project(app, &params.key).await? else {
        return Ok(None);
    };
    let flow = super::board::flow(app, &ticket.key.project.to_string()).await;
    Ok(dependencies::section(&Columns::of(&flow), &ticket, &all, now_ms()))
}

/// Who searches for a ticket to wait for: an existing ticket, or a new one in the New ticket form.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[ts(rename = "DependencySearchFor")]
pub enum SearchFor {
    /// The open ticket: it and what it already waits for are left out.
    Ticket { key: String },
    /// A ticket not created yet: its project's tickets, leaving out the keys the form already picked.
    New {
        project: String,
        #[serde(default)]
        picked: Vec<String>,
    },
}

/// What `dependencies.search` takes: who waits, and what is typed.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "DependencySearchParams")]
pub struct SearchParams {
    pub waiting: SearchFor,
    pub query: String,
}

pub async fn search(app: &App, params: SearchParams) -> Result<Vec<SearchRow>, RequestError> {
    let (project, all, taken) = match params.waiting {
        SearchFor::Ticket { key } => {
            let Some((ticket, all)) = with_project(app, &key).await? else {
                return Ok(Vec::new());
            };
            (
                ticket.key.project.to_string(),
                all,
                dependencies::taken(&ticket),
            )
        }
        SearchFor::New { project, picked } => {
            let all = tickets(app, &project).await?;
            let taken: Vec<TicketKey> = picked.iter().filter_map(|k| k.parse().ok()).collect();
            (project, all, taken)
        }
    };
    let flow = super::board::flow(app, &project).await;
    Ok(dependencies::search(
        &Columns::of(&flow),
        &all,
        &taken,
        &params.query,
        SEARCH_LIMIT,
    ))
}

/// What `dependencies.link` takes: the waiting ticket, the one it waits for, and whether to add or remove the link.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "DependencyLinkParams")]
pub struct LinkParams {
    pub key: String,
    pub other: String,
    pub add: bool,
}

/// Done (the section follows on the ticket's change event), or why the service refused it, in the section's words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(rename = "DependencyLinked")]
pub enum Linked {
    Linked,
    Refused { message: String },
}

pub async fn link(app: &App, params: LinkParams) -> Result<Linked, RequestError> {
    let dep = DepParams {
        child: params.key,
        parent: params.other,
    };
    let request = if params.add {
        Request::DepAdd(dep)
    } else {
        Request::DepRemove(dep)
    };
    Ok(match app.service.send(request).await {
        Ok(_) => Linked::Linked,
        Err(e) => Linked::Refused {
            message: dependencies::refusal_text(e.refusal.as_deref(), &e.message),
        },
    })
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::board::tickets::NewTicketInput;
    use otaq_core::git::testing::TempRepos;
    use otaq_core::protocol::{Reply, Request};
    use serde_json::{Value, json};

    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    async fn send(h: &Harness, r: Request) -> Reply {
        h.app.service.send(r).await.unwrap()
    }

    /// The section through the real service: SHOP-2 waits for SHOP-1; SHOP-1 lists it as Waiting for it, read-only;
    /// adding the reverse link is refused with DPV-04's words; ✕ on the waiting ticket removes it.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_section_adds_lists_refuses_loops_and_removes() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let Reply::ProjectAdded { project, .. } = send(
            &h,
            Request::ProjectAdd(AddProject {
                path: repos.repo("shop-web", 1).display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }),
        )
        .await
        else {
            panic!("no project");
        };
        let mut keys = Vec::new();
        for title in ["Checkout API", "Checkout page"] {
            let Reply::Ticket(t) = send(
                &h,
                Request::TicketCreate(NewTicketInput {
                    project: project.key.clone(),
                    title: title.into(),
                    body: String::new(),
                    plan: String::new(),
                    priority: None,
                    base_branch: None,
                    external_id: None,
                    first_comment: None,
                }),
            )
            .await
            else {
                panic!("no ticket");
            };
            keys.push(t.key.to_string());
        }
        let (api, page) = (&keys[0], &keys[1]);
        let found = table(
            &h.app,
            "dependencies.search",
            json!({ "waiting": { "mode": "ticket", "key": page }, "query": "api" }),
        )
        .await
        .unwrap();
        assert_eq!(found[0]["key"], json!(api));
        let linked = table(
            &h.app,
            "dependencies.link",
            json!({ "key": page, "other": api, "add": true }),
        )
        .await
        .unwrap();
        assert_eq!(linked["outcome"], "linked");
        let view = table(&h.app, "dependencies.view", json!({ "key": page }))
            .await
            .unwrap();
        assert_eq!(view["waits_for"]["rows"][0]["key"], json!(api));
        assert_eq!(view["waits_for"]["rows"][0]["place"], "Backlog \u{b7} ready to start");
        assert_eq!(view["waits_for"]["note"], "starts when it merges");
        // DPV-03: dropped on Dev, the blocked page goes to Ready for Dev and the dialog says why it waits.
        let dropped = table(
            &h.app,
            "board.drop",
            json!({ "key": page, "target": "dev" }),
        )
        .await
        .unwrap();
        assert_eq!(dropped["kind"], "blocked");
        assert_eq!(dropped["dialog"]["title"], format!("{page} is blocked"));
        assert_eq!(dropped["dialog"]["keep"], "Keep it waiting");
        let Reply::Ticket(queued) = send(
            &h,
            Request::TicketGet(otaq_core::protocol::KeyParams { key: page.clone() }),
        )
        .await
        else {
            panic!("no ticket");
        };
        assert_eq!(queued.status, otaq_core::domain::Status::Queued);
        assert_eq!(queued.place, otaq_core::domain::Place::Waiting(None));
        let view = table(&h.app, "dependencies.view", json!({ "key": page }))
            .await
            .unwrap();
        assert_eq!(view["waits_for"]["rows"][0]["key"], json!(api));
        let other = table(&h.app, "dependencies.view", json!({ "key": api }))
            .await
            .unwrap();
        assert_eq!(other["waiting_for_it"]["rows"][0]["key"], json!(page));
        assert_eq!(other["waiting_for_it"]["removable"], false);
        // A new ticket's form searches the project, leaving out what it already picked.
        let found = table(
            &h.app,
            "dependencies.search",
            json!({ "waiting": { "mode": "new", "project": project.key, "picked": [api] }, "query": "checkout" }),
        )
        .await
        .unwrap();
        assert_eq!(found.as_array().map(|r| r.len()), Some(1));
        assert_eq!(found[0]["key"], json!(page));
        // The API ticket can't also wait for the page: a loop.
        let refused = table(
            &h.app,
            "dependencies.link",
            json!({ "key": api, "other": page, "add": true }),
        )
        .await
        .unwrap();
        assert_eq!(
            refused,
            json!({
                "outcome": "refused",
                "message": otaq_view::view_models::dependencies::loop_refused()
            })
        );
        // ✕ on the page's Waits for row removes the link.
        let removed = table(
            &h.app,
            "dependencies.link",
            json!({ "key": page, "other": api, "add": false }),
        )
        .await
        .unwrap();
        assert_eq!(removed["outcome"], "linked");
        let view = table(&h.app, "dependencies.view", json!({ "key": page }))
            .await
            .unwrap();
        assert_eq!(view["waits_for"]["rows"], json!([]));
        assert_eq!(
            table(&h.app, "dependencies.view", json!({ "key": "SHOP-99" }))
                .await
                .unwrap(),
            Value::Null
        );
    }
}

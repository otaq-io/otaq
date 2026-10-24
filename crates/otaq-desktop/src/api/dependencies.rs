//! `dependencies.*`: the sheet's Dependencies section (DPV-04; d0006, d0049; *24 → Adding and removing* `s24-11`),
//! in Overview under the description: Blocked by and Blocks with where each ticket is now (`dependencies.view`); Add
//! picks the direction, then searches by key or title (`dependencies.search`); a pick or a row's ✕ sends
//! `dependencies.link`, which the service checks (loops, bases, who may) and answers with DPV-04's words for a loop.
//! The words are `otaq_view::view_models::dependencies`'s.

use otaq_core::board::tickets::Ticket;
use otaq_core::protocol::{DepParams, KeyParams, ProjectParams, Reply, Request};
use otaq_view::view_models::dependencies::{self, Direction, SearchRow, Section};
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
    match app
        .service
        .send(Request::TicketList(ProjectParams {
            project: ticket.key.project.to_string(),
        }))
        .await?
    {
        Reply::Tickets(all) => Ok(Some((ticket, all))),
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

/// The section, or `null` for a ticket that is gone or an epic.
pub async fn view(app: &App, params: ViewParams) -> Result<Option<Section>, RequestError> {
    let Some((ticket, all)) = with_project(app, &params.key).await? else {
        return Ok(None);
    };
    let flow = super::board::flow(app, &ticket.key.project.to_string()).await;
    Ok(dependencies::section(&Columns::of(&flow), &ticket, &all, now_ms()))
}

/// What `dependencies.search` takes: the open ticket, the direction picked and what is typed.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "DependencySearchParams")]
pub struct SearchParams {
    pub key: String,
    pub direction: Direction,
    pub query: String,
}

pub async fn search(app: &App, params: SearchParams) -> Result<Vec<SearchRow>, RequestError> {
    let Some((ticket, all)) = with_project(app, &params.key).await? else {
        return Ok(Vec::new());
    };
    let flow = super::board::flow(app, &ticket.key.project.to_string()).await;
    Ok(dependencies::search(
        &Columns::of(&flow),
        &ticket,
        &all,
        params.direction,
        &params.query,
        SEARCH_LIMIT,
    ))
}

/// What `dependencies.link` takes: the open ticket, the other one, which way, and whether to add or remove it.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "DependencyLinkParams")]
pub struct LinkParams {
    pub key: String,
    pub other: String,
    pub direction: Direction,
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
    let (child, parent) = params.direction.link(&params.key, &params.other);
    let dep = DepParams {
        child: child.to_owned(),
        parent: parent.to_owned(),
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

    /// The section through the real service: SHOP-2 waits for SHOP-1; Blocks from the other side; adding the
    /// reverse link is refused with DPV-04's words; ✕ removes it.
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
                    kind: Default::default(),
                    title: title.into(),
                    body: String::new(),
                    priority: None,
                    epic: None,
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
            json!({ "key": page, "direction": "blocked_by", "query": "api" }),
        )
        .await
        .unwrap();
        assert_eq!(found[0]["key"], json!(api));
        let linked = table(
            &h.app,
            "dependencies.link",
            json!({ "key": page, "other": api, "direction": "blocked_by", "add": true }),
        )
        .await
        .unwrap();
        assert_eq!(linked["outcome"], "linked");
        let view = table(&h.app, "dependencies.view", json!({ "key": page }))
            .await
            .unwrap();
        assert_eq!(view["blocked_by"]["rows"][0]["key"], json!(api));
        assert_eq!(view["blocked_by"]["rows"][0]["place"], "Backlog \u{b7} ready to start");
        assert_eq!(view["blocked_by"]["note"], "starts when it merges");
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
        assert_eq!(view["blocked_by"]["rows"][0]["key"], json!(api));
        let other = table(&h.app, "dependencies.view", json!({ "key": api }))
            .await
            .unwrap();
        assert_eq!(other["blocks"]["rows"][0]["key"], json!(page));
        // The API ticket can't also wait for the page: a loop.
        let refused = table(
            &h.app,
            "dependencies.link",
            json!({ "key": api, "other": page, "direction": "blocked_by", "add": true }),
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
        // ✕ on the API ticket's Blocks row removes the same link.
        let removed = table(
            &h.app,
            "dependencies.link",
            json!({ "key": api, "other": page, "direction": "blocks", "add": false }),
        )
        .await
        .unwrap();
        assert_eq!(removed["outcome"], "linked");
        let view = table(&h.app, "dependencies.view", json!({ "key": page }))
            .await
            .unwrap();
        assert_eq!(view["blocked_by"]["rows"], json!([]));
        assert_eq!(
            table(&h.app, "dependencies.view", json!({ "key": "SHOP-99" }))
                .await
                .unwrap(),
            Value::Null
        );
    }
}

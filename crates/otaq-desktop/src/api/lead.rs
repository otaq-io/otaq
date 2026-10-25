//! `lead.*`: the epic sheet's Lead tab (LEAD-11, LEAD-12; d0083, d0045; *05 → Row · Epic sheet · Lead tab (0083)*,
//! *Row · Lead tab · ended*, *16 → Before the restart*). `lead.view` gathers the epic, its project's tickets, its
//! lead (`lead.list`), the epic's History (the lead's changes) and, after a restart, the lead's saved conversation
//! (`session.before_restart` on the epic's key reads its lead's), and `lead_tab.rs` says what the tab shows. Open
//! lead and Resume are the board's `board.lead_open` / `board.lead_resume` (T-0289). With several tools (M23) it
//! also reads the epic's holds and the agent tools: the lead's waiting states.

use otaq_core::board::tickets::{HistoryEvent, Ticket};
use otaq_core::project_agents::lead::Lead;
use otaq_core::protocol::{BeforeRestartParams, KeyParams, ProjectParams, Reply, Request};
use otaq_core::recovery::BeforeRestart;
use otaq_view::view_models::lead_tab::{self, LeadTabView};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "lead.view" => reply(&view(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(format!("bad parameters: {e}")))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected reply to {what}: {other:?}"))
}

/// `{"key": "SHOP-40"}`: the epic.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "LeadTabParams")]
pub struct Params {
    pub key: String,
}

/// What `lead.view` answers: the tab, or "SHOP-40 is gone." once the epic was deleted.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "LeadTabLoad")]
pub struct Load {
    pub view: Option<LeadTabView>,
    pub gone: String,
}

async fn epic(app: &App, key: &str) -> Result<Option<Ticket>, RequestError> {
    match app
        .service
        .send(Request::TicketGet(KeyParams { key: key.to_owned() }))
        .await
    {
        Ok(Reply::Ticket(t)) => Ok(Some(*t)),
        Ok(other) => Err(unexpected("ticket.get", &other)),
        Err(e) if e.not_found => Ok(None),
        Err(e) => Err(e),
    }
}

/// The epic's lead, if it has one (an older service knows no leads).
pub(super) async fn lead_of(app: &App, epic: &Ticket) -> Option<Lead> {
    match app
        .service
        .send(Request::LeadList(ProjectParams {
            project: epic.key.project.to_string(),
        }))
        .await
    {
        Ok(Reply::Leads(leads)) => leads.into_iter().find(|l| l.epic == epic.key),
        _ => None,
    }
}

pub async fn view(app: &App, params: Params) -> Result<Load, RequestError> {
    let gone = format!("{} is gone.", params.key);
    let Some(epic) = epic(app, &params.key).await? else {
        return Ok(Load { view: None, gone });
    };
    let tickets = match app
        .service
        .send(Request::TicketList(ProjectParams {
            project: epic.key.project.to_string(),
        }))
        .await?
    {
        Reply::Tickets(list) => list,
        other => return Err(unexpected("ticket.list", &other)),
    };
    let lead = lead_of(app, &epic).await;
    let events: Vec<HistoryEvent> = if lead.is_some() {
        match app
            .service
            .send(Request::TicketHistory(KeyParams {
                key: params.key.clone(),
            }))
            .await?
        {
            Reply::History(events) => events,
            other => return Err(unexpected("ticket.history", &other)),
        }
    } else {
        Vec::new()
    };
    let before: Option<BeforeRestart> = if lead_tab::needs_before_restart(lead.as_ref()) {
        match app
            .service
            .send(Request::SessionBeforeRestart(BeforeRestartParams {
                key: params.key.clone(),
                limit: None,
            }))
            .await?
        {
            Reply::BeforeRestart(before) => Some(*before),
            other => return Err(unexpected("session.before_restart", &other)),
        }
    } else {
        None
    };
    // The epic's holds and named agents (its lead held before its start, M23), and the tools it would run on.
    let agents = super::editor::agent_facts(app, super::editor::of_ticket(&params.key)).await;
    let tools = super::editor::agent_tools(app).await;
    Ok(Load {
        view: Some(lead_tab::view(&lead_tab::Facts {
            epic: &epic,
            tickets: &tickets,
            lead: lead.as_ref(),
            events: &events,
            before: before.as_ref(),
            holds: agents.as_ref().map_or(&[], |a| &a.holds),
            agent_list: agents.as_ref().map_or(&[], |a| &a.agents),
            tools: &tools,
        })),
        gone,
    })
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::board::tickets::NewTicketInput;
    use otaq_core::domain::Kind;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::service::testing::{Harness, connected};

    #[tokio::test(flavor = "multi_thread")]
    async fn the_lead_tab_of_a_real_epic_without_a_lead_offers_split_then_open_lead() {
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
        let epic = match h
            .app
            .service
            .send(Request::TicketCreate(NewTicketInput {
                project: project.clone(),
                kind: Kind::Epic,
                title: "Wishlist".into(),
                body: String::new(),
                priority: None,
                epic: None,
                base_branch: None,
                external_id: None,
                first_comment: None,
            }))
            .await
            .unwrap()
        {
            Reply::Ticket(t) => t.key.to_string(),
            other => panic!("{other:?}"),
        };
        // No children (LEAD-13, d0091): Split with the Project agent, with the agent panel's first message.
        let load = request(&h.app, "lead.view", json!({ "key": epic }))
            .await
            .unwrap();
        assert_eq!(
            load["view"]["status"]["full"],
            "Split with Noyan"
        );
        assert_eq!(
            load["view"]["body"]["button"]["split"],
            json!({ "project": project, "message": format!("Split {epic} into tickets") })
        );
        // A child: Open lead.
        let child = NewTicketInput {
            project: project.clone(),
            kind: Kind::Ticket,
            title: "Wishlist API".into(),
            body: String::new(),
            priority: None,
            epic: Some(epic.clone()),
            base_branch: None,
            external_id: None,
            first_comment: None,
        };
        h.app
            .service
            .send(Request::TicketCreate(child))
            .await
            .unwrap();
        let load = request(&h.app, "lead.view", json!({ "key": epic }))
            .await
            .unwrap();
        assert_eq!(load["view"]["status"]["full"], "Open lead");
        assert_eq!(load["view"]["body"]["kind"], "open_lead");
        assert_eq!(load["view"]["changes"], Value::Null);
        let missing = format!("{project}-99");
        let gone = request(&h.app, "lead.view", json!({ "key": missing }))
            .await
            .unwrap();
        assert_eq!(gone["view"], Value::Null);
        assert_eq!(gone["gone"], format!("{missing} is gone."));
        h.service.stop_all().await;
    }
}

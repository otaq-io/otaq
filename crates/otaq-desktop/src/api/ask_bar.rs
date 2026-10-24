//! `ask_bar.*`: the Ask bar and the rail's Project agent button (T-0458; SHB-02, SHB-03; d0113, d0068, d0087;
//! `ask_bar.rs` in `otaq-view`). `ask_bar.view` answers what the bar and the button draw for a project; `ask_bar.send`
//! is Return (a new conversation through `agent_panel.send`, or Continue into a running one); `ask_bar.looked` says the
//! panel shows a conversation, which clears its "New reply". Whether the panel is open and what it shows are the
//! page's own state, passed in; nothing here starts an agent without the user's words.

use otaq_core::protocol::{KeyParams, ProjectAgentSendParams, Reply, Request};
use otaq_core::session::deliveries::Delivery;
use otaq_view::view_models::ask_bar::{self as vm, About, AskBarView};
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "ask_bar.view" => reply(&view(app, parse(params)?).await?),
        "ask_bar.send" => reply(&send(app, parse(params)?).await?),
        "ask_bar.looked" => {
            let params: LookedParams = parse(params)?;
            app.looked
                .lock()
                .expect("looked")
                .look(&params.agent, otaq_core::store::now_ms());
            Ok(Value::Null)
        }
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

/// What `ask_bar.view` takes: the project, the selected ticket, and the panel's state.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AskBarParams")]
pub struct ViewParams {
    pub project: String,
    #[serde(default)]
    #[ts(optional)]
    pub ticket: Option<String>,
    /// The panel is open.
    #[serde(default)]
    pub open: bool,
    /// The conversation the panel shows.
    #[serde(default)]
    #[ts(optional)]
    pub shown: Option<String>,
}

async fn ticket_title(app: &App, key: &str) -> Option<String> {
    match app
        .service
        .send(Request::TicketGet(KeyParams {
            key: key.to_owned(),
        }))
        .await
    {
        Ok(Reply::Ticket(t)) => Some(t.title),
        _ => None,
    }
}

pub async fn view(app: &App, params: ViewParams) -> Result<AskBarView, RequestError> {
    let agents = super::agent_panel::agents(app, &params.project).await?;
    let title = match &params.ticket {
        Some(key) => ticket_title(app, key).await,
        None => None,
    };
    let keys = super::keymap::labels(app).await;
    let looked = app.looked.lock().expect("looked").clone();
    Ok(vm::view(&vm::Facts {
        agents: &agents.open,
        looked: &looked,
        open: params.open,
        shown: params.shown.as_deref(),
        selected: params
            .ticket
            .as_deref()
            .zip(title.as_deref())
            .map(|(key, title)| vm::Selected { key, title }),
        keys: &keys,
    }))
}

/// What `ask_bar.send` takes: the typed words and the suggestion chosen.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AskSendParams")]
pub struct SendParams {
    pub project: String,
    pub text: String,
    pub about: About,
    /// The ticket of an [`About::Ticket`] row.
    #[serde(default)]
    #[ts(optional)]
    pub ticket: Option<String>,
    /// The conversation of an [`About::Continue`] row.
    #[serde(default)]
    #[ts(optional)]
    pub agent: Option<String>,
}

/// How Return went: the conversation the panel opens on, and whether the words went (the bar keeps them if not).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AskSent")]
pub struct Sent {
    pub agent: String,
    pub sent: bool,
}

/// Return: a new conversation starts with the words (about the ticket, named first), or Continue types them into
/// the running one. Its input line holding unsent text: nothing is typed, a toast says why, and the panel opens on it.
pub async fn send(app: &App, params: SendParams) -> Result<Sent, RequestError> {
    let new = |message: String| super::agent_panel::SendParams {
        project: params.project.clone(),
        message,
        agent: None,
    };
    match (params.about, params.ticket.as_deref(), params.agent) {
        (About::Continue, _, Some(agent)) => {
            let request = Request::ProjectAgentSend(ProjectAgentSendParams {
                id: agent.clone(),
                message: params.text,
            });
            match app.service.send(request).await {
                Ok(Reply::Delivery(Delivery::Sent { .. })) => Ok(Sent { agent, sent: true }),
                Ok(_) => {
                    app.notices.toast(NewToast::new(
                        ToastLevel::Error,
                        "Not sent",
                        Some("Its input line has unsent text: send or clear it in its terminal.".to_owned()),
                    ));
                    Ok(Sent { agent, sent: false })
                }
                Err(e) => {
                    app.notices.toast(NewToast::new(
                        ToastLevel::Error,
                        "The message wasn\u{2019}t sent",
                        Some(e.message.clone()),
                    ));
                    Err(e)
                }
            }
        }
        (About::Ticket, Some(key), _) => {
            let message = vm::about_ticket(key, &params.text);
            Ok(Sent {
                agent: super::agent_panel::send(app, new(message)).await?,
                sent: true,
            })
        }
        _ => Ok(Sent {
            agent: super::agent_panel::send(app, new(params.text.clone())).await?,
            sent: true,
        }),
    }
}

/// What `ask_bar.looked` takes: the conversation the open panel shows.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AskLookedParams")]
pub struct LookedParams {
    pub agent: String,
}

#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    ViewParams::export_all(cfg)?;
    SendParams::export_all(cfg)?;
    Sent::export_all(cfg)?;
    LookedParams::export_all(cfg)?;
    AskBarView::export_all(cfg)?;
    vm::Gallery::export_all(cfg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    /// d0068, d0087: the bar answers for a project with no conversation; Return without words starts nothing;
    /// `looked` is taken.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_bar_answers_and_starts_nothing_without_words() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("pocket-notes", 1);
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
        let view = table(&h.app, "ask_bar.view", json!({ "project": project }))
            .await
            .unwrap();
        assert_eq!(view["suggestions"][0]["label"], "New conversation");
        assert_eq!(view["reply"], Value::Null);
        assert_eq!(view["button"]["tinted"], false);
        let open = table(
            &h.app,
            "ask_bar.view",
            json!({ "project": project, "open": true, "ticket": "NOPE-1" }),
        )
        .await
        .unwrap();
        assert_eq!(open["button"]["tinted"], true);
        // A ticket that isn't there isn't suggested.
        assert_eq!(open["suggestions"].as_array().unwrap().len(), 1);

        assert!(
            table(
                &h.app,
                "ask_bar.send",
                json!({ "project": project, "text": "  ", "about": "new" })
            )
            .await
            .is_err()
        );
        let list = super::super::agent_panel::agents(&h.app, &project).await.unwrap();
        assert!(list.open.is_empty());
        table(&h.app, "ask_bar.looked", json!({ "agent": "a1" }))
            .await
            .unwrap();
        assert!(h.app.looked.lock().unwrap().last("a1") > 0);
        h.service.stop_all().await;
    }
}

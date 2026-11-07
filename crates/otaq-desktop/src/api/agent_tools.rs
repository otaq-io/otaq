//! `agent_tools.*`: Settings → Agent tools and the agent tools of onboarding's Tools step (§20 Agent tools, SET-02,
//! ADP-10; d0086, d0085, d0011; *15* `bGHVl`, *14* `y2FCwP`). The page opens with `agent_tools.view {recheck: true}`
//! (opening the section rechecks every tool, d0086), Re-check asks it again, and the service's `agent_tools_changed`
//! event (a recheck, a setup, a Sign in… terminal that ended) asks `agent_tools.view {}`. Hide / Show, Re-check this
//! tool and Disconnect… (after its dialog) answer with the fresh page; Sign in… answers the terminal to show. The
//! words are `otaq_view::view_models::agent_tools_page`'s.

use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::connected::{AgentToolsList, ToolStatus};
use otaq_core::protocol::{RecheckParams, Reply, Request, SetHiddenParams, ToolParams};
use otaq_view::view_models::agent_tools_page::{self as page, DisconnectDialog, Page};
use otaq_view::view_models::project_terminal::terminal_tab_id;
use otaq_view::view_models::tool_avatar;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "agent_tools.view" => {
            let p: ViewParams = parse(params)?;
            let request = if p.recheck {
                Request::AgentToolsRecheck(RecheckParams { tool: None })
            } else {
                Request::AgentToolsList
            };
            reply(&match list(app, request).await {
                Ok(list) => page_of(&list),
                Err(e) => page::failed(e.message),
            })
        }
        "agent_tools.recheck" => {
            let p: MaybeToolParams = parse(params)?;
            let tool = p.tool.as_deref().map(tool).transpose()?;
            reply(&page_of(
                &list(app, Request::AgentToolsRecheck(RecheckParams { tool })).await?,
            ))
        }
        "agent_tools.set_hidden" => {
            let p: HiddenParams = parse(params)?;
            let request = Request::AgentToolsSetHidden(SetHiddenParams {
                tool: tool(&p.tool)?,
                hidden: p.hidden,
            });
            reply(&page_of(&list(app, request).await?))
        }
        "agent_tools.sign_in" => {
            let p: OneTool = parse(params)?;
            let request = Request::AgentToolsSignIn(ToolParams {
                tool: tool(&p.tool)?,
            });
            match app.service.send(request).await? {
                Reply::ProjectTerminal(t) => reply(&SignIn {
                    tab: terminal_tab_id(&t.session.id),
                    session: t.session.id.clone(),
                    title: t.name.clone(),
                }),
                other => Err(unexpected("agent_tools.sign_in", &other)),
            }
        }
        "agent_tools.install" => {
            let p: OneTool = parse(params)?;
            let request = Request::AgentToolsInstall(ToolParams {
                tool: tool(&p.tool)?,
            });
            match app.service.send(request).await? {
                Reply::ProjectTerminal(t) => reply(&SignIn {
                    tab: terminal_tab_id(&t.session.id),
                    session: t.session.id.clone(),
                    title: t.name.clone(),
                }),
                other => Err(unexpected("agent_tools.install", &other)),
            }
        }
        "agent_tools.disconnect_dialog" => {
            let p: OneTool = parse(params)?;
            reply(&page::disconnect_dialog(&status(app, tool(&p.tool)?).await?, None))
        }
        "agent_tools.disconnect" => {
            let p: OneTool = parse(params)?;
            reply(&disconnect(app, tool(&p.tool)?).await?)
        }
        "agent_tools.open_docs" => {
            let p: OneTool = parse(params)?;
            let url = page::docs_url(tool(&p.tool)?)
                .ok_or_else(|| RequestError::plain(format!("no docs for {}", p.tool)))?;
            open::that_detached(url)
                .map_err(|e| RequestError::plain(format!("couldn't open {url}: {e}")))?;
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

fn tool(name: &str) -> Result<Tool, RequestError> {
    name.parse()
        .map_err(|_| RequestError::plain(format!("not an agent tool: {name}")))
}

/// `agent_tools.view`: `recheck` detects every tool again first (the section opening, d0086).
#[derive(Debug, Clone, Copy, Default, Deserialize, TS)]
#[ts(rename = "AgentToolsViewParams")]
pub struct ViewParams {
    #[serde(default)]
    pub recheck: bool,
}

/// `agent_tools.recheck`: one tool (its … menu), or every tool (Re-check).
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(rename = "AgentToolsRecheckParams")]
pub struct MaybeToolParams {
    #[serde(default)]
    pub tool: Option<String>,
}

/// A request about one tool, named as the rows name it (`codex`).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AgentToolParams")]
pub struct OneTool {
    pub tool: String,
}

/// `agent_tools.set_hidden`: Hide (… menu) or Show.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "AgentToolHiddenParams")]
pub struct HiddenParams {
    pub tool: String,
    pub hidden: bool,
}

/// Sign in…'s terminal: the tab to open in the Terminals drawer, focused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolSignIn")]
pub struct SignIn {
    pub session: String,
    pub tab: String,
    /// "Codex sign in", "Installing Codex".
    pub title: String,
}

/// Disconnect's answer: the fresh page, or the dialog again with the service's refusal ("Codex has 2 agents
/// running. End them first.").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "AgentToolDisconnectOutcome")]
pub enum DisconnectOutcome {
    Done { page: Box<Page> },
    Refused { dialog: Box<DisconnectDialog> },
}

/// The service's rows; the avatars on every other screen follow its connected tools.
pub async fn list(app: &App, request: Request) -> Result<AgentToolsList, RequestError> {
    match app.service.send(request).await? {
        Reply::AgentTools(list) => {
            let connected: Vec<Tool> = list
                .tools
                .iter()
                .filter(|t| t.state.is_connected())
                .map(|t| t.tool)
                .collect();
            tool_avatar::set_connected(&connected);
            Ok(*list)
        }
        other => Err(unexpected("agent_tools.list", &other)),
    }
}

fn page_of(list: &AgentToolsList) -> Page {
    page::page(&list.tools, otaq_core::store::now_ms())
}

async fn status(app: &App, tool: Tool) -> Result<ToolStatus, RequestError> {
    list(app, Request::AgentToolsList)
        .await?
        .tools
        .into_iter()
        .find(|t| t.tool == tool)
        .ok_or_else(|| RequestError::plain(format!("no row for {tool}")))
}

async fn disconnect(app: &App, tool: Tool) -> Result<DisconnectOutcome, RequestError> {
    match app
        .service
        .send(Request::AgentToolsDisconnect(ToolParams { tool }))
        .await
    {
        Ok(Reply::AgentTools(list)) => Ok(DisconnectOutcome::Done {
            page: Box::new(page_of(&list)),
        }),
        Ok(other) => Err(unexpected("agent_tools.disconnect", &other)),
        Err(e) if e.refusal.is_none() => Err(e),
        Err(e) => {
            Ok(DisconnectOutcome::Refused {
                dialog: Box::new(page::disconnect_dialog(
                    &status(app, tool).await?,
                    Some(e.message),
                )),
            })
        }
    }
}

/// The connected tools that keep folder trust, for the Add project line (PRJ-07, d0086): those whose setup lists
/// an entry Disconnect leaves (folder trust stays on every tool). None when the list can't be read.
pub async fn trust_tools(app: &App) -> Vec<Tool> {
    match list(app, Request::AgentToolsList).await {
        Ok(list) => list
            .tools
            .iter()
            .filter(|t| t.state.is_connected())
            .filter(|t| t.setup_entries.iter().any(|e| !e.removed_on_disconnect))
            .map(|t| t.tool)
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    #[tokio::test(flavor = "multi_thread")]
    async fn the_page_from_the_real_service() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;

        // The test service's PATH has no agent tool: four rows, each Not installed with its install command.
        let view = request(&h.app, "agent_tools.view", json!({ "recheck": true }))
            .await
            .unwrap();
        assert_eq!(view["title"], "Tools");
        let rows = view["rows"].as_array().unwrap();
        let tools: Vec<&str> = rows.iter().map(|r| r["tool"].as_str().unwrap()).collect();
        assert_eq!(tools, ["claude", "codex", "copilot", "opencode"]);
        assert!(rows.iter().all(|r| r["pill"]["look"] == "not_installed"));
        assert!(rows.iter().all(|r| r["install"]["command"].is_string()));
        assert!(view["checked"].as_str().unwrap().starts_with("Checked "));

        // The dialog for a tool, and Disconnect refused for a tool that isn't connected.
        let dialog = request(
            &h.app,
            "agent_tools.disconnect_dialog",
            json!({ "tool": "codex" }),
        )
        .await
        .unwrap();
        assert_eq!(dialog["title"], "Disconnect Codex?");
        assert_eq!(dialog["can_confirm"], false, "not connected");

        // Nothing to sign in to: refused, as the service says.
        assert!(
            request(&h.app, "agent_tools.sign_in", json!({ "tool": "claude" }))
                .await
                .is_err()
        );
        assert!(
            request(&h.app, "agent_tools.set_hidden", json!({ "tool": "nope", "hidden": true }))
                .await
                .is_err()
        );
        assert!(trust_tools(&h.app).await.is_empty());
        h.service.stop_all().await;
    }
}

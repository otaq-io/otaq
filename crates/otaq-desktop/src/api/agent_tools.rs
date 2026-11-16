//! `agent_tools.*`: Settings → Tools and the agent tools of onboarding's Tools step (§20 Agent tools, SET-02,
//! ADP-10; d0086, d0085, d0011; UX-65, UX-70; *15* `bGHVl`, *14* `y2FCwP`, canvas A-8). Every page answer carries,
//! under the tools, Noyan starts on (the `noyan.starts_on` setting) and Where they're used (every project's flow in
//! use); `agent_tools.noyan` changes Noyan starts on and answers the fresh page. The page opens with
//! `agent_tools.view {recheck: true}` (opening the section rechecks every tool, d0086), Re-check asks it again, and
//! the service's `agent_tools_changed` event (a recheck, a setup, a Sign in… terminal that ended), `agents_changed`
//! (Noyan starts on) and `flow_changed` ask `agent_tools.view {}`. Hide / Show, Re-check this
//! tool and Disconnect… (after its dialog) answer with the fresh page; Sign in… answers the terminal to show. The
//! words are `otaq_view::view_models::agent_tools_page`'s.

use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::connected::{AgentToolsList, ToolStatus};
use otaq_core::agents::ModelRef;
use otaq_core::protocol::{ProjectParams, RecheckParams, Reply, Request, SetHiddenParams, ToolParams};
use otaq_view::view_models::agent_tools_page::{self as page, DisconnectDialog, NoyanEdit, Page};
use otaq_view::view_models::model_effort;
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
                Ok(list) => page_of(app, &list).await,
                Err(e) => page::failed(e.message),
            })
        }
        "agent_tools.recheck" => {
            let p: MaybeToolParams = parse(params)?;
            let tool = p.tool.as_deref().map(tool).transpose()?;
            let list = list(app, Request::AgentToolsRecheck(RecheckParams { tool })).await?;
            reply(&page_of(app, &list).await)
        }
        "agent_tools.set_hidden" => {
            let p: HiddenParams = parse(params)?;
            let request = Request::AgentToolsSetHidden(SetHiddenParams {
                tool: tool(&p.tool)?,
                hidden: p.hidden,
            });
            let list = list(app, request).await?;
            reply(&page_of(app, &list).await)
        }
        // Noyan starts on (UX-70): one edit of the field, stored as `noyan.set_starts_on`; the next new conversation
        // starts on it. Answers the fresh page.
        "agent_tools.noyan" => {
            let edit: NoyanEdit = parse(params)?;
            let on = super::settings::noyan_starts_on(app).await;
            let next = page::noyan_edit(&on, &edit).map_err(RequestError::plain)?;
            match app.service.send(Request::NoyanSetStartsOn(next)).await? {
                Reply::NoyanStartsOn(_) => {}
                other => return Err(unexpected("noyan.set_starts_on", &other)),
            }
            let list = list(app, Request::AgentToolsList).await?;
            reply(&page_of(app, &list).await)
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

/// Sign in…'s terminal: its session, shown in the Sign in dialog where Sign in… was clicked (`tab` names the
/// session's tab and is not used to open it).
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

/// The page: the tools' rows, then Noyan starts on and Where they're used, from the Noyan setting and every
/// project's flow in use (a flow that doesn't read is left out).
async fn page_of(app: &App, list: &AgentToolsList) -> Page {
    let rows = page::page(&list.tools, otaq_core::store::now_ms());
    let mut tools = super::editor::agent_tools(app).await;
    // `default` is the account's default model, which the service sends with the tools. Until a tool first connects
    // there is none; the service then sets it to that tool, so the first connected one is what `default` reads as.
    if tools.default_model.is_none() {
        tools.default_model = Some(ModelRef {
            tool: tools
                .connected
                .first()
                .copied()
                .unwrap_or(otaq_view::view_models::agents::new_entry_tool()),
            model: None,
        });
    }
    let noyan = super::settings::noyan_starts_on(app).await;
    let uses = uses(app).await;
    page::with_flows(
        rows,
        &page::Flows {
            noyan: &noyan,
            uses: &uses,
            tools: &tools,
        },
    )
}

/// Every project's stages and Done with a model ([`model_effort::uses_in`]), the projects in the list's order.
async fn uses(app: &App) -> Vec<model_effort::Use> {
    let projects = match app.service.send(Request::ProjectList).await {
        Ok(Reply::Projects(list)) => list,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for p in projects {
        let current = app
            .service
            .send(Request::FlowCurrent(ProjectParams {
                project: p.key.clone(),
            }))
            .await;
        if let Ok(Reply::FlowVersion(v)) = current
            && let Ok(file) = v.file()
        {
            out.extend(model_effort::uses_in(&p.name, &file.flow));
        }
    }
    out
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
            page: Box::new(page_of(app, &list).await),
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

    /// Settings → Tools (UX-70, A-8): Noyan starts on and where each tool is used, from the projects' flows; changing
    /// Noyan starts on changes what the next Noyan conversation starts on.
    #[tokio::test(flavor = "multi_thread")]
    async fn noyan_starts_on_and_where_they_are_used() {
        use otaq_core::board::projects::AddProject;
        use otaq_core::git::testing::TempRepos;

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
            }))
            .await
            .unwrap()
        {
            Reply::ProjectAdded { project: p, .. } => p.key,
            other => panic!("{other:?}"),
        };

        let view = request(&h.app, "agent_tools.view", json!({})).await.unwrap();
        assert_eq!(view["rows_heading"], "TOOLS");
        assert_eq!(view["noyan"]["label"], "Noyan starts on");
        assert_eq!(
            view["noyan"]["picker"]["hint"],
            "For new conversations. Switch for one conversation in the Noyan panel."
        );
        // No tool is set up in the test service: the project's stages and Noyan are on `default`, marked.
        let used = view["used"]["rows"].as_array().unwrap();
        assert_eq!(used.len(), 1, "{used:?}");
        let places = used[0]["places"].as_str().unwrap();
        assert!(places.starts_with("pocket-notes: "), "{places}");
        assert!(places.ends_with(" · Noyan"), "{places}");
        assert!(used[0]["off"].is_string());
        assert_eq!(
            view["used"]["note"],
            "To change one, open its project\u{2019}s flow. Signing out of a tool makes its stages wait until you \
             pick another model."
        );

        // Noyan starts on Codex · medium now: the page and the setting say so.
        let view = request(
            &h.app,
            "agent_tools.noyan",
            json!({ "edit": "pair", "model": "codex", "effort": "medium" }),
        )
        .await
        .unwrap();
        assert_eq!(view["noyan"]["picker"]["value"]["text"], "Codex \u{b7} default");
        let rows = view["used"]["rows"].as_array().unwrap();
        let codex = rows.iter().find(|r| r["tool"] == "codex").unwrap();
        assert_eq!(codex["places"], "Noyan");
        match h.app.service.send(Request::NoyanStartsOn).await.unwrap() {
            Reply::NoyanStartsOn(on) => {
                assert_eq!(on.model.as_deref(), Some("codex"));
                assert_eq!(on.effort.as_deref(), Some("medium"));
            }
            other => panic!("{other:?}"),
        }
        // The next Noyan conversation starts on it.
        let panel = request(&h.app, "agent_panel.view", json!({ "project": project }))
            .await
            .unwrap();
        assert_eq!(panel["empty"]["line"]["agent"], "Codex");
        // An effort alone keeps the model; an effort the tool doesn't have is refused by the service.
        let view = request(&h.app, "agent_tools.noyan", json!({ "edit": "effort" }))
            .await
            .unwrap();
        assert_eq!(view["noyan"]["picker"]["value"]["text"], "Codex \u{b7} default");
        assert!(
            request(
                &h.app,
                "agent_tools.noyan",
                json!({ "edit": "effort", "effort": "turbo" })
            )
            .await
            .is_err()
        );
        h.service.stop_all().await;
    }
}

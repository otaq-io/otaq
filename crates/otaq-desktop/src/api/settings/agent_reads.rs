//! `settings.agent_reads*`: "What this agent reads" (T-0440; AGR-01…AGR-03, d0108 rule 12): the view for one agent
//! and a project (`settings.agent_reads`: the picked project, else the one on screen, else the first; the text is the
//! service's `agents.reads`), and Open skill folder → / an Also has: name (`settings.agent_reads_reveal`), which reveals
//! only a folder that agent's reads name.

use otaq_core::agent_reads::{AgentReads, AgentReadsParams};
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::protocol::{Reply, Request};
use otaq_view::view_models::agent_reads::{self as vm, Project};
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, agent_list, parse, reply, unexpected, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "settings.agent_reads" => {
            let p: AgentReadsOpen = parse(params)?;
            reply(&view(app, &p.id, p.project.as_deref()).await?)
        }
        "settings.agent_reads_reveal" => {
            let p: AgentReadsReveal = parse(params)?;
            let reads = reads(app, &p.id, &p.project).await?;
            let folder = reads
                .skills
                .iter()
                .filter(|s| s.skill == p.skill)
                .find_map(|s| s.folder.clone())
                .or_else(|| {
                    reads
                        .others
                        .iter()
                        .filter(|o| o.skill == p.skill)
                        .find_map(|o| o.folder.clone())
                })
                .ok_or_else(|| RequestError::plain(format!("{} has no folder to show", p.skill)))?;
            open::that_detached(&folder).map_err(|e| RequestError::plain(e.to_string()))?;
            reply(&Value::Null)
        }
        _ => Err(unknown(method)),
    }
}

/// `settings.agent_reads`: the agent, and the project picked or on screen.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct AgentReadsOpen {
    pub id: String,
    #[serde(default)]
    pub project: Option<String>,
}

/// `settings.agent_reads_reveal`: the skill whose folder to show, as the view names it.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct AgentReadsReveal {
    pub id: String,
    pub project: String,
    pub skill: String,
}

#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    vm::View::export_all(cfg)?;
    AgentReadsOpen::export_all(cfg)?;
    AgentReadsReveal::export_all(cfg)
}

async fn reads(app: &App, agent: &str, project: &str) -> Result<AgentReads, RequestError> {
    let request = Request::AgentsReads(AgentReadsParams {
        agent: agent.to_owned(),
        project: project.to_owned(),
    });
    match app.service.send(request).await? {
        Reply::AgentReads(reads) => Ok(*reads),
        other => Err(unexpected("agents.reads", &other)),
    }
}

async fn view(app: &App, id: &str, wanted: Option<&str>) -> Result<vm::View, RequestError> {
    let projects: Vec<Project> = match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => list
            .into_iter()
            .map(|p: ProjectInfo| Project {
                key: p.key,
                name: p.name,
            })
            .collect(),
        other => return Err(unexpected("project.list", &other)),
    };
    let name = agent_list(app)
        .await?
        .agents
        .into_iter()
        .find(|a| a.id == id)
        .map(|a| a.name)
        .ok_or_else(|| RequestError::plain(format!("no agent {id}")))?;
    let read = match vm::opening_project(&projects, wanted) {
        Some(project) => Some(reads(app, id, &project).await?),
        None => None,
    };
    Ok(vm::view(id, &name, projects, read.as_ref()))
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    /// With no project the view says to add one; with two it opens on the one asked for, else the first, and Open
    /// skill folder → reveals only a skill the agent's reads name.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_view_opens_on_a_project_and_reveals_only_its_skills() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let developer = agent_list(&h.app)
            .await
            .unwrap()
            .agents
            .into_iter()
            .find(|a| a.name == "Developer")
            .unwrap()
            .id;
        let ask = |method: &'static str, params: Value| request(&h.app, method, params);
        let view = ask("settings.agent_reads", json!({ "id": developer })).await.unwrap();
        assert_eq!(view["subtitle"], "Developer \u{b7} read-only");
        assert_eq!(view["empty"], "Add a project to see what this agent reads.");
        assert_eq!(view["parts"], json!([]));

        let repos = TempRepos::new();
        for name in ["shop-web", "billing-api"] {
            let path = repos.repo(name, 1).display().to_string();
            h.app
                .service
                .send(Request::ProjectAdd(AddProject {
                    path,
                    ..Default::default()
                }))
                .await
                .unwrap();
        }
        let first = ask("settings.agent_reads", json!({ "id": developer, "project": "GONE" }))
            .await
            .unwrap();
        let keys: Vec<&str> = first["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_str().unwrap())
            .collect();
        assert_eq!(first["project"].as_str(), Some(keys[0]));
        let other = ask("settings.agent_reads", json!({ "id": developer, "project": keys[1] }))
            .await
            .unwrap();
        assert_eq!(other["project"].as_str(), Some(keys[1]));
        let headings: Vec<&str> = other["parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["heading"].as_str().unwrap())
            .collect();
        assert_eq!(headings[..2], ["Its instructions", "Notes for billing-api"]);
        assert_eq!(
            headings[2],
            format!(
                "Skill: {} \u{b7} Dev",
                otaq_core::agent_tools::WORK_TICKET_SKILL
            )
        );

        let refused = ask(
            "settings.agent_reads_reveal",
            json!({ "id": developer, "project": keys[1], "skill": "../../etc" }),
        )
        .await
        .unwrap_err();
        assert_eq!(refused.message, "../../etc has no folder to show");
        h.service.stop_all().await;
    }
}

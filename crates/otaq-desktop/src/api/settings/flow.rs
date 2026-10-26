//! `settings.flow*`: a project's Flow section (T-0422; FLW-06…FLW-10, d0108). The section (`settings.flow`), Use a
//! preset… (`settings.flow_presets`), Copy from another project…'s list (`settings.flow_copy`), and the actions: every
//! one but Export puts its flow in the project's draft (`flow.propose_from`), and the page opens the flow editor on it
//! (FED-09); Export flow… writes the flow document (`flow.show`) where the save dialog said; Import flow… reads the file
//! the open dialog picked; Cancel change drops the waiting change (`flow.cancel_waiting`, no question).

use std::path::PathBuf;

use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::flow::apply::{ProposeFrom, ProposeFromParams, WaitingChange};
use otaq_core::flow::store::FlowVersion;
use otaq_core::protocol::{ProjectParams, Reply, Request};
use otaq_view::view_models::flow_settings::{self as vm, CopyMenu, PresetDialog};
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, agent_list, agent_tools, parse, reply, unexpected, unknown};
use crate::App;

/// The largest file Import flow… reads: a flow document is a few kilobytes.
const IMPORT_LIMIT: u64 = 1024 * 1024;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "settings.flow" => {
            let p: FlowProject = parse(params)?;
            reply(&section(app, &p.project).await?)
        }
        "settings.flow_presets" => {
            let p: FlowProject = parse(params)?;
            reply(&presets(app, &p.project).await?)
        }
        "settings.flow_copy" => {
            let p: FlowProject = parse(params)?;
            reply(&copy(app, &p.project).await?)
        }
        "settings.flow_propose" => {
            let p: FlowPropose = parse(params)?;
            propose(app, &p.project, p.source.into()).await?;
            reply(&Value::Null)
        }
        "settings.flow_import" => {
            let p: FlowFile = parse(params)?;
            let source = import(&p.path).await?;
            propose(app, &p.project, source).await?;
            reply(&Value::Null)
        }
        "settings.flow_export" => {
            let p: FlowFile = parse(params)?;
            export(app, &p.project, &p.path).await?;
            reply(&Value::Null)
        }
        "settings.flow_cancel" => {
            let p: FlowProject = parse(params)?;
            app.service
                .send(Request::FlowCancelWaiting(ProjectParams {
                    project: p.project.clone(),
                }))
                .await?;
            reply(&section(app, &p.project).await?)
        }
        _ => Err(unknown(method)),
    }
}

/// `{"project": "SHOP"}`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowSettingsParams")]
pub struct FlowProject {
    pub project: String,
}

/// `{"project": "SHOP", "path": "/Users/me/shop-web-flow.yaml"}`: the file Import flow… picked, or where Export
/// flow… saves.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowFileParams")]
pub struct FlowFile {
    pub project: String,
    pub path: String,
}

/// What a Flow action proposes (`flow.propose_from`, FLW-08…FLW-10).
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "from", rename_all = "snake_case")]
#[ts(rename = "FlowSource")]
pub enum Source {
    /// Back to the previous flow: its stages only.
    Previous,
    /// Copy from another project…: its stages.
    Project { project: String },
    /// Use a preset…: the preset's file ("spec-first.yaml").
    Preset { file: String },
    /// Reset to the default flow.
    Default,
    /// Apply the shipped agents file again (Settings → Agents).
    AgentsFile,
}

impl From<Source> for ProposeFrom {
    fn from(source: Source) -> Self {
        match source {
            Source::Previous => Self::Previous { version: None },
            Source::Project { project } => Self::Project { project },
            Source::Preset { file } => Self::Preset { file },
            Source::Default => Self::Preset {
                file: vm::RESET_FILE.to_owned(),
            },
            Source::AgentsFile => Self::AgentsFile,
        }
    }
}

/// `{"project": "SHOP", "source": {"from": "preset", "file": "spec-first.yaml"}}`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FlowProposeParams")]
pub struct FlowPropose {
    pub project: String,
    pub source: Source,
}

/// The types Settings → Flow reads and sends, for `generated.rs` (T-0422).
#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    vm::Section::export_all(cfg)?;
    PresetDialog::export_all(cfg)?;
    CopyMenu::export_all(cfg)?;
    FlowProject::export_all(cfg)?;
    FlowFile::export_all(cfg)?;
    FlowPropose::export_all(cfg)?;
    crate::api::NoticesProject::export_all(cfg)
}

async fn projects(app: &App) -> Result<Vec<ProjectInfo>, RequestError> {
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(projects) => Ok(projects),
        other => Err(unexpected("project.list", &other)),
    }
}

fn find(projects: &[ProjectInfo], key: &str) -> Result<ProjectInfo, RequestError> {
    projects
        .iter()
        .find(|p| p.key == key)
        .cloned()
        .ok_or_else(|| RequestError::plain(format!("unknown project {key}")))
}

/// The section from `flow.versions`, `flow.waiting`, the agents and the tools.
async fn section(app: &App, key: &str) -> Result<vm::Section, RequestError> {
    let all = projects(app).await?;
    let project = find(&all, key)?;
    let params = || ProjectParams {
        project: key.to_owned(),
    };
    let versions: Vec<FlowVersion> =
        match app.service.send(Request::FlowVersions(params())).await? {
            Reply::FlowVersions(versions) => versions,
            other => return Err(unexpected("flow.versions", &other)),
        };
    let current = versions
        .iter()
        .find(|v| v.current)
        .or(versions.last())
        .cloned()
        .ok_or_else(|| RequestError::plain(format!("{key} has no flow")))?;
    let waiting: Option<WaitingChange> =
        match app.service.send(Request::FlowWaiting(params())).await? {
            Reply::FlowWaiting(waiting) => waiting.map(|w| *w),
            other => return Err(unexpected("flow.waiting", &other)),
        };
    let agents = agent_list(app).await?;
    let tools = agent_tools(app, false).await?;
    Ok(vm::section(&vm::Inputs {
        project: &project,
        current: &current,
        versions: versions.len(),
        waiting: waiting.as_ref(),
        agents: &agents,
        tools: &tools,
        other_projects: all.len().saturating_sub(1),
        now_ms: otaq_core::store::now_ms(),
    }))
}

async fn presets(app: &App, key: &str) -> Result<PresetDialog, RequestError> {
    Ok(vm::preset_dialog(&find(&projects(app).await?, key)?))
}

async fn copy(app: &App, key: &str) -> Result<CopyMenu, RequestError> {
    Ok(vm::copy_menu(key, &projects(app).await?))
}

/// The draft for `project` from `source`, which the flow editor then shows. A refusal (a file that doesn't read, no
/// previous flow) is the request's error, shown on the page.
async fn propose(app: &App, project: &str, source: ProposeFrom) -> Result<(), RequestError> {
    let request = Request::FlowProposeFrom(ProposeFromParams {
        project: project.to_owned(),
        source,
    });
    match app.service.send(request).await? {
        Reply::FlowProposal(_) => Ok(()),
        other => Err(unexpected("flow.propose_from", &other)),
    }
}

/// Import flow…: the picked file's name and text (FLW-09). The editor marks what it lacks on its steps.
pub(in crate::api) async fn import(path: &str) -> Result<ProposeFrom, RequestError> {
    let path = PathBuf::from(path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let size = tokio::fs::metadata(&path)
        .await
        .map_err(|e| RequestError::plain(format!("Couldn\u{2019}t read {name}: {e}")))?
        .len();
    if size > IMPORT_LIMIT {
        return Err(RequestError::plain(format!(
            "{name} is too big for a flow file"
        )));
    }
    let yaml = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| RequestError::plain(format!("Couldn\u{2019}t read {name}: {e}")))?;
    Ok(ProposeFrom::File { name, yaml })
}

/// Export flow…: the applied flow with the agents it names, as `otaq flow show` prints it (FLW-02, FLW-09).
async fn export(app: &App, project: &str, path: &str) -> Result<(), RequestError> {
    let yaml = match app
        .service
        .send(Request::FlowShow(ProjectParams {
            project: project.to_owned(),
        }))
        .await?
    {
        Reply::FlowDocument(document) => document.yaml,
        other => return Err(unexpected("flow.show", &other)),
    };
    tokio::fs::write(path, yaml)
        .await
        .map_err(|e| RequestError::plain(format!("Couldn\u{2019}t save {path}: {e}")))
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    async fn add(h: &Harness, repos: &TempRepos, name: &str) -> String {
        let path = repos.repo(name, 1);
        match h
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
            Reply::ProjectAdded { project, .. } => project.key,
            other => panic!("{other:?}"),
        }
    }

    async fn proposal(h: &Harness, project: &str) -> Value {
        let request = Request::FlowProposal(ProjectParams {
            project: project.to_owned(),
        });
        match h.app.service.send(request).await.unwrap() {
            Reply::FlowProposal(Some(p)) => serde_json::to_value(*p).unwrap(),
            other => panic!("{other:?}"),
        }
    }

    /// A new project's section (the default flow, nothing to go back to); Export → Import round-trips into a "same
    /// as your current flow" preview (FLW-09); a preset and another project's flow open proposals; Back is refused
    /// with the service's words.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_section_and_its_actions() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let shop = add(&h, &repos, "shop-web").await;
        let section = request(&h.app, "settings.flow", json!({ "project": shop }))
            .await
            .unwrap();
        assert_eq!(section["title"], "Flow");
        assert!(
            section["changed"]
                .as_str()
                .unwrap()
                .starts_with(&format!(
                    "{}\u{2019}s default flow",
                    otaq_core::naming::PRODUCT_NAME
                )),
            "{section}"
        );
        let stages: Vec<&str> = section["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["name"].as_str().unwrap())
            .collect();
        assert_eq!(stages, ["Dev", "Validate", "Review", "Done"]);
        assert_eq!(section["waiting"], Value::Null);
        assert_eq!(section["copy"]["disabled"], true, "no other project yet");
        assert_eq!(section["menu"][2]["disabled"], true, "no previous flow");

        let file = h._tmp.path().join("shop-web-flow.yaml");
        let path = file.display().to_string();
        request(
            &h.app,
            "settings.flow_export",
            json!({ "project": shop, "path": path }),
        )
        .await
        .unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("name: Developer"), "the agents too: {text}");
        request(
            &h.app,
            "settings.flow_import",
            json!({ "project": shop, "path": path }),
        )
        .await
        .unwrap();
        let imported = proposal(&h, &shop).await;
        assert_eq!(imported["diff"]["same"], true, "{imported}");
        assert_eq!(imported["source"]["file"], "shop-web-flow.yaml");

        request(
            &h.app,
            "settings.flow_propose",
            json!({ "project": shop, "source": { "from": "preset", "file": "spec-first.yaml" } }),
        )
        .await
        .unwrap();
        let product_spec = proposal(&h, &shop).await;
        assert_eq!(product_spec["diff"]["same"], false);
        assert!(product_spec["yaml"].as_str().unwrap().contains("name: Product spec"));

        let back = request(
            &h.app,
            "settings.flow_propose",
            json!({ "project": shop, "source": { "from": "previous" } }),
        )
        .await
        .unwrap_err();
        assert!(back.message.contains("no previous flow"), "{back:?}");

        let bill = add(&h, &repos, "billing-api").await;
        let copy = request(&h.app, "settings.flow_copy", json!({ "project": shop }))
            .await
            .unwrap();
        assert_eq!(copy["projects"][0]["key"], bill.as_str());
        request(
            &h.app,
            "settings.flow_propose",
            json!({ "project": shop, "source": { "from": "project", "project": bill } }),
        )
        .await
        .unwrap();
        assert_eq!(proposal(&h, &shop).await["diff"]["same"], true);

        let presets = request(&h.app, "settings.flow_presets", json!({ "project": shop }))
            .await
            .unwrap();
        assert_eq!(presets["title"], "Use a preset for shop-web");
        assert_eq!(presets["presets"].as_array().unwrap().len(), 4);
        h.service.stop_all().await;
    }
}

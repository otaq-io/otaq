//! `files_view.*`: the project Files view (FIL-01…FIL-10; d0087; `s28-03`…`s28-10`). The project's base branch from
//! git with every ticket's changes on top (`files.project_*`, `board/project_files.rs`): the left column with its
//! filters and counts, a folder as it opens, the search by name, the picker's sources, a picked ticket or epic as its
//! own Files tab, read-only (its tree, files and All changes come from `files.*`), a picked file with Changed by and
//! one ticket's Diff, and All changes grouped by ticket. The tickets' titles, lines and tools are their board cards'
//! (`board::ticket_card`). What it says is `otaq-view`'s `files_view.rs`.

use std::collections::BTreeMap;

use otaq_core::board::tickets::Ticket;
use otaq_core::protocol::{
    ProjectFilesDiffParams, ProjectFilesPathParams, ProjectFilesSearchParams, ProjectParams, Reply,
    Request,
};
use otaq_view::keymap::KeyAction;
use otaq_view::view_models::board::{BoardInput, ticket_card};
use otaq_view::view_models::diff_page;
use otaq_view::view_models::files_tab::{self, SearchView, TreeRow};
use otaq_view::view_models::files_view::{self as vm, Filter};
use otaq_view::view_models::flow_columns::Columns;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "files_view.view" => reply(&view(app, parse(params)?).await?),
        "files_view.tree" => reply(&tree(app, parse(params)?).await?),
        "files_view.search" => reply(&search(app, parse(params)?).await?),
        "files_view.sources" => reply(&sources(app, parse(params)?).await?),
        "files_view.picked" => reply(&picked(app, parse(params)?).await?),
        "files_view.file" => reply(&file(app, parse(params)?).await?),
        "files_view.diff" => reply(&diff(app, parse(params)?).await?),
        "files_view.changes" => reply(&changes(app, parse(params)?).await?),
        "files_view.new_ticket_body" => {
            let params: PathParams = parse(params)?;
            reply(&files_tab::new_ticket_body(&params.path))
        }
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {what}: {other:?}"))
}

/// The shown project's view under a filter; `base_only` lists the bare base at once (the Loading state), the page
/// then asks again without it.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesViewParams")]
pub struct ViewParams {
    pub project: String,
    pub filter: Filter,
    #[serde(default)]
    #[ts(optional)]
    pub base_only: Option<bool>,    /// One of your own worktrees instead of the base (its folder, from the picker; #43).
    #[serde(default)]
    #[ts(optional)]
    pub worktree: Option<String>,
}

/// What `files_view.view` answers: the view, or why the base can't be read (no repository, no base branch).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FilesViewLoad")]
pub struct Load {
    pub view: Option<vm::Page>,
    pub problem: Option<String>,
}

/// Whether one of the project's agents works now: the page then asks every 5 s (FIL-10).
async fn working(app: &App, project: &str) -> bool {
    super::panel::tickets(app, project)
        .await
        .unwrap_or_default()
        .iter()
        .any(|t: &Ticket| vm::agent_works(t.status))
}

pub async fn view(app: &App, params: ViewParams) -> Result<Load, RequestError> {
    let base_only = params.base_only.unwrap_or(false);
    let tree = |base_only| {
        app.service
            .send(Request::FilesProjectTree(ProjectFilesPathParams {
                project: params.project.clone(),
                path: String::new(),
                base_only,
                worktree: params.worktree.clone(),
            }))
    };
    let (keys, working) = tokio::join!(super::keymap::labels(app), working(app, &params.project));
    let keys = vm::ViewKeys {
        go_to_file: keys.label(KeyAction::GoToFile),
        new_ticket: keys.label(KeyAction::NewTicket),
    };
    // One of your worktrees has only its tree: no ticket's changes to filter.
    let filter = match params.worktree {
        Some(_) => Filter::All,
        None => params.filter,
    };
    let answer = match filter {
        Filter::All => tree(base_only).await.and_then(|r| match r {
            Reply::ProjectTree(t) => Ok(vm::page(
                &t.source,
                t.counts.as_ref(),
                Filter::All,
                vm::entry_rows(&t.entries, 0),
                keys,
                working,
            )),
            other => Err(unexpected("files.project_tree", &other)),
        }),
        filter => app
            .service
            .send(Request::FilesProjectChanges(ProjectParams {
                project: params.project.clone(),
            }))
            .await
            .and_then(|r| match r {
                Reply::ProjectChanges(c) => Ok(vm::page(
                    &c.source,
                    Some(&c.counts),
                    filter,
                    vm::changed_rows(&c, filter),
                    keys,
                    working,
                )),
                other => Err(unexpected("files.project_changes", &other)),
            }),
    };
    Ok(match answer {
        Ok(page) => Load {
            view: Some(page),
            problem: None,
        },
        Err(e) => Load {
            view: None,
            problem: Some(e.message),
        },
    })
}

/// A folder of the base, opened at `depth`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesViewTreeParams")]
pub struct TreeParams {
    pub project: String,
    pub path: String,
    pub depth: u32,
    /// One of your own worktrees instead of the base (its folder, from the picker; #43).
    #[serde(default)]
    #[ts(optional)]
    pub worktree: Option<String>,
}

pub async fn tree(app: &App, params: TreeParams) -> Result<Vec<TreeRow>, RequestError> {
    match app
        .service
        .send(Request::FilesProjectTree(ProjectFilesPathParams {
            project: params.project,
            path: params.path,
            base_only: false,
            worktree: params.worktree,
        }))
        .await?
    {
        Reply::ProjectTree(tree) => Ok(vm::entry_rows(&tree.entries, params.depth)),
        other => Err(unexpected("files.project_tree", &other)),
    }
}

/// What is typed in the search box.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesViewSearchParams")]
pub struct SearchParams {
    pub project: String,
    pub query: String,
    /// One of your own worktrees instead of the base (its folder, from the picker; #43).
    #[serde(default)]
    #[ts(optional)]
    pub worktree: Option<String>,
}

pub async fn search(app: &App, params: SearchParams) -> Result<SearchView, RequestError> {
    match app
        .service
        .send(Request::FilesProjectSearch(ProjectFilesSearchParams {
            project: params.project,
            query: params.query.clone(),
            worktree: params.worktree,
        }))
        .await?
    {
        Reply::ProjectSearch(found) => Ok(vm::search_view(&params.query, &found)),
        other => Err(unexpected("files.project_search", &other)),
    }
}

/// The shown project.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesViewProjectParams")]
pub struct ProjectOnly {
    pub project: String,
}

/// The picker's rows; "when it reaches Dev" names the flow's first agent stage.
pub async fn sources(app: &App, params: ProjectOnly) -> Result<vm::Picker, RequestError> {
    let (answer, flow) = tokio::join!(
        app.service
            .send(Request::FilesProjectSources(ProjectParams {
                project: params.project.clone(),
            })),
        super::board::flow(app, &params.project),
    );
    let first = flow
        .first_agent_stage()
        .map_or_else(|| "Dev".to_owned(), |s| s.name.clone());
    match answer? {
        Reply::ProjectSources(sources) => Ok(vm::picker(&sources, &first)),
        other => Err(unexpected("files.project_sources", &other)),
    }
}

/// A ticket or an epic picked in the picker, in the shown project.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesViewPickedParams")]
pub struct PickedParams {
    pub project: String,
    pub key: String,
}

/// What `files_view.picked` answers: the picked source, or why it has no files to show.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FilesViewPickedLoad")]
pub struct PickedLoad {
    pub picked: Option<vm::Picked>,
    pub problem: Option<String>,
}

/// Its own Files tab's frame (`files.view`), read-only, with the back link to the base.
pub async fn picked(app: &App, params: PickedParams) -> Result<PickedLoad, RequestError> {
    let (tab, base) = tokio::join!(
        super::files::view(
            app,
            super::files::Params {
                key: params.key.clone(),
            },
        ),
        app.service
            .send(Request::FilesProjectTree(ProjectFilesPathParams {
                project: params.project.clone(),
                path: String::new(),
                base_only: true,
                worktree: None,
            })),
    );
    let tab = tab?;
    let base_branch = match base {
        Ok(Reply::ProjectTree(tree)) => tree.source.base_branch,
        Ok(other) => return Err(unexpected("files.project_tree", &other)),
        Err(e) => {
            return Ok(PickedLoad {
                picked: None,
                problem: Some(e.message),
            });
        }
    };
    Ok(match tab.view {
        Some(view) => PickedLoad {
            picked: Some(vm::picked(&base_branch, view)),
            problem: None,
        },
        None => PickedLoad {
            picked: None,
            problem: tab.problem,
        },
    })
}

/// What the cards of the project's tickets say of `keys` (title, place, line, tool), as the board builds them; none
/// when the board can't be read.
async fn owner_facts(
    app: &App,
    project: &str,
    keys: &[String],
) -> BTreeMap<String, vm::OwnerFacts> {
    let Ok(board) = super::board::snapshot(app, project).await else {
        return BTreeMap::new();
    };
    let tickets: Vec<Ticket> = board.tickets().cloned().collect();
    let (flow, agents) = tokio::join!(
        super::board::flow(app, project),
        app.service.send(Request::BoardAgents(ProjectParams {
            project: project.to_owned(),
        })),
    );
    let agents = match agents {
        Ok(Reply::BoardAgents(agents)) => Some(*agents),
        _ => None,
    };
    let input = BoardInput {
        flow: &flow,
        tickets: &tickets,
        visible: None,
        strong: None,
        project_base: None,
        now_ms: otaq_core::store::now_ms(),
        dev_budget: None,
        folded: &[],
        fold_empty: false,
        agent_names: &[],
        was_validating: &[],
        agents: agents.as_ref(),
        agent_list: None,
        starts_held: None,
        paused: board.project.paused_since.is_some(),
    };
    let columns = Columns::of(&flow);
    tickets
        .iter()
        .filter(|t| keys.contains(&t.key.to_string()))
        .map(|t| {
            (
                t.key.to_string(),
                vm::owner_facts(&columns, t, ticket_card(&input, t)),
            )
        })
        .collect()
}

/// A file of the shown project.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesViewFileParams")]
pub struct FileParams {
    pub project: String,
    pub path: String,
    /// One of your own worktrees instead of the base (its folder, from the picker; #43).
    #[serde(default)]
    #[ts(optional)]
    pub worktree: Option<String>,
}

/// A picked file (`files.project_read`): the base's lines with the bars, highlighted off the runtime's threads (a
/// long file takes a while), and Changed by.
pub async fn file(app: &App, params: FileParams) -> Result<vm::FilePane, RequestError> {
    let read = match app
        .service
        .send(Request::FilesProjectRead(ProjectFilesPathParams {
            project: params.project.clone(),
            path: params.path,
            base_only: false,
            worktree: params.worktree,
        }))
        .await?
    {
        Reply::ProjectFile(file) => file,
        other => return Err(unexpected("files.project_read", &other)),
    };
    let keys: Vec<String> = read.owners.iter().map(|o| o.key.to_string()).collect();
    let facts = owner_facts(app, &params.project, &keys).await;
    tokio::task::spawn_blocking(move || vm::file_pane(&read, &facts))
        .await
        .map_err(|e| RequestError::plain(e.to_string()))
}

/// A file's path: New ticket from this file's description (FIL-09) for a right-clicked file or ⌘N.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesViewPathParams")]
pub struct PathParams {
    pub path: String,
}

/// One ticket's change of a file of the shown project (Diff's "of SHOP-29 ▾").
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "FilesViewDiffParams")]
pub struct DiffParams {
    pub project: String,
    pub path: String,
    pub ticket: String,
}

/// Its hunks against the ticket's base (uncommitted edits included), highlighted; no line comments (d0017).
pub async fn diff(app: &App, params: DiffParams) -> Result<diff_page::FileView, RequestError> {
    let diff = match app
        .service
        .send(Request::FilesProjectDiff(ProjectFilesDiffParams {
            project: params.project,
            path: params.path,
            ticket: params.ticket,
        }))
        .await?
    {
        Reply::FileDiff(diff) => diff,
        other => return Err(unexpected("files.project_diff", &other)),
    };
    tokio::task::spawn_blocking(move || diff_page::file_view(&diff))
        .await
        .map_err(|e| RequestError::plain(e.to_string()))
}

/// All changes (FIL-08): every ticket's changed files, grouped, without their lines (each file's come from
/// `files.diff` of its ticket as it scrolls into view); Unified / Split as the Files tab keeps it.
pub async fn changes(app: &App, params: ProjectOnly) -> Result<vm::ChangesPane, RequestError> {
    let changes = match app
        .service
        .send(Request::FilesProjectChanges(ProjectParams {
            project: params.project.clone(),
        }))
        .await?
    {
        Reply::ProjectChanges(c) => c,
        other => return Err(unexpected("files.project_changes", &other)),
    };
    let keys: Vec<String> = changes
        .groups
        .iter()
        .map(|g| g.ticket.key.to_string())
        .collect();
    let facts = owner_facts(app, &params.project, &keys).await;
    let memory = app.review.lock().unwrap_or_else(|e| e.into_inner()).clone();
    Ok(vm::changes_pane(&changes, &facts, &memory))
}

#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    ViewParams::export_all(cfg)?;
    Load::export_all(cfg)?;
    TreeParams::export_all(cfg)?;
    SearchParams::export_all(cfg)?;
    ProjectOnly::export_all(cfg)?;
    PickedParams::export_all(cfg)?;
    PickedLoad::export_all(cfg)?;
    FileParams::export_all(cfg)?;
    DiffParams::export_all(cfg)?;
    PathParams::export_all(cfg)?;
    vm::FilePane::export_all(cfg)?;
    vm::ChangesPane::export_all(cfg)?;
    vm::Picker::export_all(cfg)?;
    vm::Gallery::export_all(cfg)
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    use super::*;

    /// A project nobody is changing: a file reads as on main, read-only, with no Changed by; All changes says nothing
    /// is changing; a file's menu and New ticket's description; Diff of a ticket that isn't changing files is refused;
    /// your own worktree is in the picker and opens read-only (#43).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_file_and_all_changes_of_a_quiet_project() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("shop-web", 2);
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
        let key = project.key.clone();

        let file = table(&h.app, "files_view.file", json!({ "project": key, "path": "file2.txt" }))
            .await
            .unwrap();
        assert_eq!(file["subtitle"], "on main \u{b7} read-only", "{file}");
        assert_eq!(file["owners"], json!([]));
        assert_eq!(file["can_diff"], false);
        assert_eq!(file["rows"][0]["kind"], "line");
        assert_eq!(file["new_ticket_body"], "`file2.txt`\n\n");

        let changes = table(&h.app, "files_view.changes", json!({ "project": key }))
            .await
            .unwrap();
        assert_eq!(changes["groups"], json!([]));
        assert_eq!(
            changes["empty"]["text"],
            "Start one from the board, or ask the Project agent."
        );

        let body = table(&h.app, "files_view.new_ticket_body", json!({ "path": "src/a.ts" }))
            .await
            .unwrap();
        assert_eq!(body, "`src/a.ts`\n\n");

        let view = table(&h.app, "files_view.view", json!({ "project": key, "filter": "all" }))
            .await
            .unwrap();
        assert_eq!(view["view"]["menu"][0]["label"], "New ticket from this file");
        assert_eq!(view["view"]["menu"][1]["label"], "Copy path");
        assert_eq!(
            view["view"]["checking"],
            "on main \u{b7} checking which tickets change it\u{2026}"
        );

        let refused = table(
            &h.app,
            "files_view.diff",
            json!({ "project": key, "path": "file2.txt", "ticket": format!("{key}-1") }),
        )
        .await;
        assert!(refused.is_err());

        // #43: your own worktree is in the picker and opens read-only, from git at its HEAD.
        let experiment = repos.root().join("habitat-experiment");
        otaq_core::git::testing::git(
            &path,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "experiment/accent",
                &experiment.display().to_string(),
            ],
        );
        let picker = table(&h.app, "files_view.sources", json!({ "project": key }))
            .await
            .unwrap();
        let yours = &picker["sections"][1];
        assert_eq!(yours["title"], "YOUR WORKTREES", "{picker}");
        assert_eq!(yours["rows"][0]["title"], "experiment/accent");
        let folder = yours["rows"][0]["pick"]["path"].clone();
        assert_eq!(yours["rows"][0]["pick"]["kind"], "worktree");
        let view = table(
            &h.app,
            "files_view.view",
            json!({ "project": key, "filter": "changed", "worktree": folder }),
        )
        .await
        .unwrap();
        assert_eq!(
            view["view"]["source"]["title"],
            "experiment/accent \u{b7} your worktree",
            "{view}"
        );
        assert_eq!((view["view"]["filter"].clone(), view["view"]["filters"].clone()), (json!("all"), json!([])));
        let file = table(
            &h.app,
            "files_view.file",
            json!({ "project": key, "path": "file2.txt", "worktree": folder }),
        )
        .await
        .unwrap();
        assert_eq!(file["subtitle"], "on experiment/accent \u{b7} read-only", "{file}");
        h.service.stop_all().await;
    }
}

//! `otaq dep add | remove | tree | cycles` (SDD §11.2; DEP-01, DEP-02, CLI-02; d0032). The service checks
//! every link: loops ("SHOP-21 → SHOP-20 → SHOP-21"), bases ("Linked tickets must use the same base branch.") and,
//! for agents, the permission scope.
//!
//! # `--json` shapes
//!
//! - `dep add` / `dep remove`: the dependent `Ticket` afterwards.
//! - `dep tree`: `{"key", "title", "kind", "status", "parents": [node], "children": [node]}`, each node
//!   `{"key", "title", "kind", "status", "nodes": [node]}`: what it waits for (up) and what waits for it (down), each
//!   ticket once (#9).
//! - `dep cycles`: `[[key, …, key]]`, each loop starting and ending on the same ticket; `[]` when healthy.

use otaq_core::board::deps::{DepNode, DepTree};
use otaq_core::board::stack::empty_text;
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Status, TicketKey};
use otaq_core::protocol::{DepParams, ErrorBody, KeyParams, ProjectParams, Reply, Request};

use crate::cli::{DepCommand, GlobalArgs};
use crate::output::Output;
use crate::read::unexpected;
use crate::service::{self, client_error, internal};
use crate::target;

pub fn run(command: DepCommand, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = async |k: Option<String>| target::resolve_key(&client, &data, k, global).await;
        let adding = matches!(command, DepCommand::Add { .. });
        match command {
            DepCommand::Add {
                key: child,
                depends_on,
            }
            | DepCommand::Remove {
                key: child,
                depends_on,
            } => {
                let child = key(Some(child)).await?.to_string();
                let parent = key(Some(depends_on)).await?.to_string();
                let params = DepParams { child, parent };
                let request = if adding {
                    Request::DepAdd(params.clone())
                } else {
                    Request::DepRemove(params.clone())
                };
                let ticket = match client.request(request).await.map_err(client_error)? {
                    Reply::Ticket(t) => *t,
                    other => return Err(unexpected(&other)),
                };
                let human = if adding {
                    format!("{} now depends on {}", ticket.key, params.parent)
                } else {
                    format!("{} no longer depends on {}", ticket.key, params.parent)
                };
                out.print(&human, &ticket, Some(&ticket.key.to_string()));
            }
            DepCommand::Tree { key: k } => {
                let key = key(k).await?.to_string();
                let tree = match client
                    .request(Request::DepTree(KeyParams { key }))
                    .await
                    .map_err(client_error)?
                {
                    Reply::DepTree(t) => t,
                    other => return Err(unexpected(&other)),
                };
                out.print(&tree_text(&tree), &tree, Some(&tree.key.to_string()));
            }
            DepCommand::Cycles => {
                let project = target::require_project(&client, &data, global).await?;
                let cycles = match client
                    .request(Request::DepCycles(ProjectParams {
                        project: project.to_string(),
                    }))
                    .await
                    .map_err(client_error)?
                {
                    Reply::Cycles(c) => c,
                    other => return Err(unexpected(&other)),
                };
                let human = if cycles.is_empty() {
                    format!("No dependency loops in {project}")
                } else {
                    cycles
                        .iter()
                        .map(|c| {
                            c.iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(" → ")
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                out.print(&human, &cycles, None);
            }
        }
        Ok(())
    })
}

/// The ticket on top, then "Waits for:" and "Waited on by:" as indented trees, each ticket once (#9): a diamond's
/// shared ticket shows under its nearest path only. A ticket without links says so.
fn tree_text(tree: &DepTree) -> String {
    if tree.parents.is_empty() && tree.children.is_empty() {
        return empty_text(&tree.key);
    }
    let mut lines = vec![line(0, &tree.key, tree.status, &tree.title)];
    for (heading, nodes) in [
        ("Waits for:", &tree.parents),
        ("Waited on by:", &tree.children),
    ] {
        if !nodes.is_empty() {
            lines.push(heading.to_owned());
            nodes_text(nodes, 1, &mut lines);
        }
    }
    lines.join("\n")
}

fn nodes_text(nodes: &[DepNode], depth: usize, lines: &mut Vec<String>) {
    for node in nodes {
        lines.push(line(depth, &node.key, node.status, &node.title));
        nodes_text(&node.nodes, depth + 1, lines);
    }
}

fn line(depth: usize, key: &TicketKey, status: Status, title: &str) -> String {
    format!(
        "{}{:<10} {:<22} {title}",
        "  ".repeat(depth),
        key.to_string(),
        status.label()
    )
}

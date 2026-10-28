//! `otaq plugins list|search|import` (the otaq-marketplace README, "Agents and ticket sources"): the project's tracker plugins from
//! the command line, for the user and for the project's Project agent (Noyan), which may search and import into the
//! Backlog. Ticket agents are refused by the service.
//!
//! `--json`: the plugins, the search result or the import as the service answers them.

use otaq_core::data_dir::DataDir;
use otaq_core::plugins::params::{ImportParams, SearchParams};
use otaq_core::protocol::{ErrorBody, ProjectParams, Reply, Request};

use crate::cli::{GlobalArgs, PluginsCommand};
use crate::output::Output;
use crate::read::unexpected;
use crate::service::{self, client_error, internal};
use crate::target;

pub fn run(command: PluginsCommand, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let project = target::require_project(&client, &data, global)
            .await?
            .to_string();
        match command {
            PluginsCommand::List => {
                let reply = client
                    .request(Request::ProjectPluginsGet(ProjectParams {
                        project: project.clone(),
                    }))
                    .await
                    .map_err(client_error)?;
                let Reply::ProjectPlugins(plugins) = reply else {
                    return Err(unexpected(&reply));
                };
                let on: Vec<_> = plugins.iter().filter(|p| p.enabled).collect();
                let text = if on.is_empty() {
                    format!(
                        "No tracker plugin is turned on for {project} (Project settings → Plugins)"
                    )
                } else {
                    on.iter()
                        .map(|p| format!("{}  {}", p.plugin, p.name))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                out.print(&text, &on, None);
            }
            PluginsCommand::Search { plugin, query } => {
                let reply = client
                    .request(Request::PluginsSearch(SearchParams {
                        project,
                        plugin,
                        query,
                    }))
                    .await
                    .map_err(client_error)?;
                let Reply::PluginSearch(result) = reply else {
                    return Err(unexpected(&reply));
                };
                let mut lines: Vec<String> = result
                    .hits
                    .iter()
                    .map(|h| {
                        let note = match (&h.ticket, h.dismissed) {
                            (Some(key), _) => format!("  (imported as {key})"),
                            (None, true) => "  (dismissed)".to_owned(),
                            (None, false) => String::new(),
                        };
                        format!(
                            "{}  {}  [id {}]{note}",
                            h.issue.key, h.issue.title, h.issue.id
                        )
                    })
                    .collect();
                if lines.is_empty() {
                    lines.push("No open work items match".to_owned());
                }
                if result.truncated {
                    lines.push("Showing the first 100: refine the search for more".to_owned());
                }
                out.print(&lines.join("\n"), &*result, None);
            }
            PluginsCommand::Import { plugin, ids } => {
                let reply = client
                    .request(Request::PluginsImport(ImportParams {
                        project,
                        plugin,
                        ids,
                    }))
                    .await
                    .map_err(client_error)?;
                let Reply::PluginImported(imported) = reply else {
                    return Err(unexpected(&reply));
                };
                let mut lines: Vec<String> = imported
                    .tickets
                    .iter()
                    .map(|t| {
                        let from = t.external_id.as_deref().unwrap_or("");
                        format!("Created {} {} (from {from})", t.key, t.title)
                    })
                    .collect();
                lines.extend(imported.skipped.iter().map(|s| format!("Skipped: {s}")));
                out.print(&lines.join("\n"), &*imported, None);
            }
        }
        Ok(())
    })
}

//! Plugin requests (the otaq-marketplace README): Settings → Plugins, a project's plugin settings, the board's Import dialog
//! and a ticket's Refresh. [`super::permissions`] lets only the user change plugins; the project's Project agent may
//! search and import. Every plugin change publishes `plugins_changed`; imported and refreshed tickets publish their
//! usual ticket events.

use std::path::Path;

use super::Shared;
use super::board_requests::{created_by_agent, error_body, mutate};
use crate::board::BoardError;
use crate::caller::Caller;
use crate::plugins::index::PAGE_MAX_AGE;
use crate::protocol::{ErrorBody, ErrorKind, Event, Reply, Request};

pub(super) async fn handle(
    request: Request,
    caller: &Caller,
    shared: &Shared,
) -> Result<Reply, ErrorBody> {
    let plugins = &shared.plugins;
    let board = &shared.board;
    let actor = caller.actor(None);
    let list = || async { plugins.view().await.map(|v| Reply::Plugins(Box::new(v))) };
    let changed = |reply: Reply| {
        shared.events.publish(Event::PluginsChanged);
        reply
    };
    let answer: Result<Reply, BoardError> = match request {
        Request::PluginsList => {
            if let Err(e) = plugins.refresh_indexes(PAGE_MAX_AGE).await {
                tracing::warn!(error = %e, "can't read the plugin marketplaces");
            }
            list().await
        }
        Request::PluginsRefreshIndex => {
            match plugins.refresh_indexes(std::time::Duration::ZERO).await {
                Ok(()) => list().await.map(changed),
                Err(e) => Err(e),
            }
        }
        Request::PluginsMarketplaceAdd(p) => match plugins.add_marketplace(&p.url).await {
            Ok(_) => list().await.map(changed),
            Err(e) => Err(e),
        },
        Request::PluginsMarketplaceRemove(p) => match plugins.remove_marketplace(&p.id).await {
            Ok(()) => list().await.map(changed),
            Err(e) => Err(e),
        },
        Request::PluginsConsent(p) => plugins
            .consent(&p.marketplace, &p.id, p.version.as_deref())
            .await
            .map(|c| Reply::PluginConsent(Box::new(c))),
        Request::PluginsInstall(p) => match plugins
            .install(&p.marketplace, &p.id, &p.version, &p.digest)
            .await
        {
            Ok(_) => list().await.map(changed),
            Err(e) => Err(e),
        },
        Request::PluginsLocal(p) => match &p.digest {
            None => plugins
                .consent_local(Path::new(&p.folder))
                .await
                .map(|c| Reply::PluginConsent(Box::new(c))),
            Some(digest) => match plugins.add_local(Path::new(&p.folder), digest).await {
                Ok(_) => list().await.map(changed),
                Err(e) => Err(e),
            },
        },
        Request::PluginsRemove(p) => match plugins.remove(&p.id).await {
            Ok(()) => list().await.map(changed),
            Err(e) => Err(e),
        },
        Request::PluginsDefaultsSet(p) => match plugins.set_defaults(&p.id, &p.values).await {
            Ok(()) => list().await.map(changed),
            Err(e) => Err(e),
        },
        Request::PluginsCheck(p) => plugins.check(&p.id).await.map(Reply::PluginChecks),
        Request::PluginsTerminal(p) => match plugins
            .terminal_command(&p.id, &p.requirement, p.extension)
            .await
        {
            Ok(argv) => shared
                .tool_terminals
                .open_command(&p.id, &argv)
                .await
                .map(|s| Reply::Session(Box::new(s))),
            Err(e) => Err(e),
        },
        Request::ProjectPluginsGet(p) => {
            plugins.project(&p.project).await.map(Reply::ProjectPlugins)
        }
        Request::ProjectPluginsSet(p) => {
            match plugins
                .set_project(
                    &p.project,
                    &p.plugin,
                    p.enabled,
                    &p.values,
                    p.auto_pull,
                    p.pull_minutes,
                )
                .await
            {
                Ok(()) => plugins
                    .project(&p.project)
                    .await
                    .map(|l| changed(Reply::ProjectPlugins(l))),
                Err(e) => Err(e),
            }
        }
        Request::PluginsPullNow(p) => {
            let pulled = mutate(
                shared,
                &p.project,
                plugins.pull(board, &p.project, &p.plugin),
            )
            .await;
            pulled.map(|tickets| {
                Reply::PluginImported(Box::new(crate::plugins::registry::Imported {
                    tickets,
                    skipped: Vec::new(),
                }))
            })
        }
        Request::PluginsSearch(p) => plugins
            .search(board, &p.project, &p.plugin, p.query.as_deref())
            .await
            .map(|r| Reply::PluginSearch(Box::new(r))),
        Request::PluginsImport(p) => {
            let imported = mutate(
                shared,
                &p.project,
                plugins.import(board, &p.project, &p.plugin, &p.ids, &actor),
            )
            .await;
            match imported {
                Ok(imported) => {
                    created_by_agent(shared, caller, &imported.tickets, &actor).await;
                    Ok(Reply::PluginImported(Box::new(imported)))
                }
                Err(e) => Err(e),
            }
        }
        Request::PluginsLinks(p) => plugins
            .links(board, &p.project)
            .await
            .map(Reply::TrackerLinks),
        Request::TicketTrackerRefresh(p) => {
            let project = project_of(&p.key);
            mutate(shared, &project, plugins.refresh(board, &p.key, &actor))
                .await
                .map(|o| Reply::TrackerRefresh(Box::new(o)))
        }
        Request::TicketTrackerResolve(p) => {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                plugins.resolve(board, &p.key, &p.picks, &actor),
            )
            .await
            .map(|()| Reply::Ok)
        }
        other => {
            return Err(ErrorBody::new(
                ErrorKind::BadRequest,
                format!("{} isn't a plugin request", other.method()),
            ));
        }
    };
    answer.map_err(|e| error_body(&e))
}

/// The project part of a ticket key (`SHOP-18` → `SHOP`).
fn project_of(key: &str) -> String {
    key.split_once('-').map_or(key, |(p, _)| p).to_owned()
}

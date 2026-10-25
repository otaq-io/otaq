//! **Ticket resources** (M19-d, d0087): what a ticket needs besides its worktree, such as its own database, made by
//! the project's commands.
//!
//! - **Create** runs at Prepare, after the env files and before setup, in the ticket's worktree. Its name is recorded
//!   before it runs, so a crash still leaves the name for Remove; a ticket whose resource was created is never
//!   created again (Retry start, Bring back).
//! - Where a project has ticket resources, Otaq sets **`OTAQ_DB_NAME`** and **`OTAQ_DB_TEMPLATE`** in the ticket's
//!   commands and sessions ([`ResourceVars`]). With empty settings, and no resource recorded, it sets neither.
//! - **`OTAQ_RESOURCE_DIR`**, Otaq's own folder for the ticket outside the repository, is set in every ticket's
//!   commands and sessions, resources or not (d0109: long output saved there is attached as a report).
//! - **Remove** runs at Release (Done, Close) for the recorded name only, in the project's main checkout; a failure
//!   is listed in Cleanup ("Ticket resources"), whose Delete runs it again. Otaq's own resource folder goes after a
//!   successful Remove, or at Release when no Remove command is set or no resource is recorded.
//! - A **Template refresh** runs at Done after a merge (never at Close). It holds the project's template lock for
//!   writing; every Create holds it for reading, so a Create waits while the template is being refreshed.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::board::Board;
use crate::board::settings::ProjectSettings;
use serde_json::json;

use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::data_dir::DataDir;
use crate::domain::{Actor, ResourceState, TicketKey};
use crate::naming;
use crate::session::env::applied;
use crate::start::StartError;
use crate::start::setup::run_command;
use crate::store::projects::Project;
use crate::tools::Os;
use crate::warm::store::{WarmBuilds, repo_name};

/// The longest name Postgres keeps (63 bytes); MySQL's limit (64) is above it.
pub const MAX_DB_NAME: usize = 63;

/// The ticket's database name: `<prefix>_<project>_<ticket>` such as `otaq_shop_shop_18` for SHOP-18 in SHOP
/// ([`naming::db_prefix`]). Lowercase; every character outside `a-z`, `0-9` and `_` becomes `_`, and runs of `_`
/// become one. At most [`MAX_DB_NAME`] bytes: a longer name keeps the ticket part whole, cuts what is before it
/// and ends with `_` and 8 hex digits of a hash of the full name, so two long names stay apart.
pub fn db_name(project_key: &str, ticket: &TicketKey) -> String {
    capped(
        &format!("{}_{project_key}", naming::db_prefix()),
        &ticket.to_string(),
    )
}

/// The project's template database name: `<prefix>_<project>_template`, by the same rules as [`db_name`].
pub fn db_template(project_key: &str) -> String {
    capped(
        &format!("{}_{project_key}", naming::db_prefix()),
        "template",
    )
}

/// `<head>_<tail>`, made safe, cut in `head` when too long.
fn capped(head: &str, tail: &str) -> String {
    let full = safe(&format!("{head}_{tail}"));
    if full.len() <= MAX_DB_NAME {
        return full;
    }
    let end = format!("_{}_{:08x}", safe(tail), fnv1a(&full));
    let room = MAX_DB_NAME.saturating_sub(end.len());
    let head = safe(head);
    // `safe` leaves ASCII only, so any byte index is a char boundary.
    let head = head[..room.min(head.len())].trim_end_matches('_');
    let name = safe(&format!("{head}{end}"));
    name[..name.len().min(MAX_DB_NAME)].to_owned()
}

/// Lowercase ASCII letters, digits and single `_`s, with no `_` at either end.
fn safe(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars().flat_map(char::to_lowercase) {
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() {
            c
        } else {
            '_'
        };
        if !(c == '_' && (out.is_empty() || out.ends_with('_'))) {
            out.push(c);
        }
    }
    out.trim_end_matches('_').to_owned()
}

/// FNV-1a (32 bits): stable across builds and machines, which `DefaultHasher` doesn't promise.
fn fnv1a(text: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in text.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Whether the project has ticket resources for this ticket: a Create command, or a resource already recorded.
pub fn has_resources(settings: &ProjectSettings, resource_name: Option<&str>) -> bool {
    !settings.resource_create.trim().is_empty() || resource_name.is_some()
}

/// The ticket resource variables (d0087).
pub struct ResourceVars;

impl ResourceVars {
    /// `OTAQ_DB_NAME` (the recorded name when there is one, else [`db_name`]) and `OTAQ_DB_TEMPLATE` when the
    /// project has ticket resources ([`has_resources`]), and always `OTAQ_RESOURCE_DIR` (made when missing).
    pub fn for_ticket(
        data: &DataDir,
        project: &Project,
        key: &TicketKey,
        resource_name: Option<&str>,
        settings: &ProjectSettings,
    ) -> Vec<(String, String)> {
        if !has_resources(settings, resource_name) {
            return resource_dir_var(data, project, key, true)
                .into_iter()
                .collect();
        }
        let name = resource_name.map_or_else(|| db_name(&project.key, key), str::to_owned);
        resource_vars(data, project, key, &name, true)
    }
}

/// `OTAQ_DB_NAME` = `name`, `OTAQ_DB_TEMPLATE` and `OTAQ_RESOURCE_DIR`, the folder made when `make_dir`.
fn resource_vars(
    data: &DataDir,
    project: &Project,
    key: &TicketKey,
    name: &str,
    make_dir: bool,
) -> Vec<(String, String)> {
    let mut vars = vec![
        (naming::db_name_env_var(), name.to_owned()),
        (naming::db_template_env_var(), db_template(&project.key)),
    ];
    vars.extend(resource_dir_var(data, project, key, make_dir));
    vars
}

/// `OTAQ_RESOURCE_DIR` = the ticket's `res/<repo>/<KEY>`, the folder made when `make_dir`.
fn resource_dir_var(
    data: &DataDir,
    project: &Project,
    key: &TicketKey,
    make_dir: bool,
) -> Option<(String, String)> {
    match data.resource_path(&repo_name(project), key) {
        Ok(dir) => {
            if make_dir && let Err(e) = DataDir::ensure_dir(&dir) {
                tracing::warn!(ticket = %key, error = %e, "can't create the ticket's resource folder");
            }
            Some((
                naming::resource_dir_env_var(),
                dir.to_string_lossy().into_owned(),
            ))
        }
        Err(e) => {
            tracing::warn!(ticket = %key, error = %e, "no resource folder for the ticket");
            None
        }
    }
}

/// Otaq's own variables in a ticket's setup and resource commands: `OTAQ_TICKET`, then the resource variables.
pub(crate) fn command_vars(
    data: &DataDir,
    project: &Project,
    key: &TicketKey,
    resource_name: Option<&str>,
    settings: &ProjectSettings,
) -> Vec<(String, String)> {
    std::iter::once((naming::ticket_env_var(), key.to_string()))
        .chain(ResourceVars::for_ticket(
            data,
            project,
            key,
            resource_name,
            settings,
        ))
        .collect()
}

/// The resource variables of the ticket with this id, for its sessions (the agent, validator, answering, Run and
/// Shell): read at each spawn, so a change reaches what starts after it. An unknown ticket gets none.
pub async fn session_vars(board: &Board, data: &DataDir, ticket_id: &str) -> Vec<(String, String)> {
    let read = async {
        let ticket = board.ticket_by_id(ticket_id).await?;
        let project = board.project(ticket.key.project.as_str()).await?;
        let settings = board.project_settings(&project.key).await?;
        let recorded = ticket.work.as_ref().and_then(|w| w.resource_name.clone());
        Ok::<_, crate::board::BoardError>(ResourceVars::for_ticket(
            data,
            &project,
            &ticket.key,
            recorded.as_deref(),
            &settings,
        ))
    };
    read.await.unwrap_or_else(|e| {
        tracing::warn!(ticket = %ticket_id, error = %e, "can't read the ticket's resource variables");
        Vec::new()
    })
}

/// Runs the project's ticket resource commands. Cheap to clone; one per service, so a project's Template refresh
/// and its Creates share one lock.
#[derive(Clone, Default)]
pub struct Resources {
    /// Per project id: Create reads, Template refresh writes.
    templates: Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::RwLock<()>>>>>,
}

impl std::fmt::Debug for Resources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resources").finish_non_exhaustive()
    }
}

impl Resources {
    /// The project's template lock.
    pub(crate) fn template_lock(&self, project_id: &str) -> Arc<tokio::sync::RwLock<()>> {
        self.templates
            .lock()
            .expect("template locks")
            .entry(project_id.to_owned())
            .or_default()
            .clone()
    }

    /// **Create**: runs the project's Create command for `ticket` in `worktree`, its output appended to `log` (the
    /// setup log). Skipped with no Create command, or when the ticket's resource was already created. The name is
    /// recorded before the command runs, and a name recorded earlier is kept (first write wins). Waits while the
    /// project's template is being refreshed. A failure records `failed` with the message and returns it.
    pub(crate) async fn create(
        &self,
        board: &Board,
        data: &DataDir,
        project: &Project,
        ticket: &Ticket,
        worktree: &Path,
        log: &Path,
    ) -> Result<(), StartError> {
        let settings = board.project_settings(&project.key).await?;
        let command = settings.resource_create.trim();
        if command.is_empty() {
            return Ok(());
        }
        let row = board
            .store()
            .ticket(&ticket.id)
            .await?
            .ok_or_else(|| StartError::Other(format!("{} is gone", ticket.key)))?;
        if row.resource_state.parse::<ResourceState>().ok() == Some(ResourceState::Created) {
            return Ok(());
        }
        let name = row
            .resource_name
            .clone()
            .unwrap_or_else(|| db_name(&project.key, &ticket.key));
        let store = board.store();
        store
            .set_resource(&ticket.id, Some(&name), ResourceState::None, None)
            .await?;
        let lock = self.template_lock(&project.id);
        let _reading = lock.read().await;
        let own = command_vars(data, project, &ticket.key, Some(&name), &settings);
        let project_env = applied(&settings.env_vars, Os::current());
        match run_command("create command", command, worktree, log, &project_env, &own).await {
            Ok(()) => {
                store
                    .set_resource(&ticket.id, Some(&name), ResourceState::Created, None)
                    .await?;
                tracing::info!(ticket = %ticket.key, resource = %name, "ticket resources created");
                Ok(())
            }
            Err(e) => {
                let message = e.to_string();
                store
                    .set_resource(
                        &ticket.id,
                        Some(&name),
                        ResourceState::Failed,
                        Some(&message),
                    )
                    .await?;
                tracing::warn!(ticket = %ticket.key, resource = %name, error = %message, "creating ticket resources failed");
                Err(e)
            }
        }
    }

    /// **Remove** at Release (Done, Close) or as Cleanup's retry: runs the project's Remove command for the
    /// ticket's **recorded** resource name only, never a computed one (no name recorded: nothing runs), in the
    /// project's main checkout, with `OTAQ_DB_NAME` = the recorded name, `OTAQ_DB_TEMPLATE` and `OTAQ_RESOURCE_DIR`;
    /// its output is appended to the project's resource log. On success the state is `removed` and Otaq's resource
    /// folder goes to the trash. With no Remove command only the folder goes, the state stays. A failure records
    /// `failed` with the message (shown in Cleanup, never retried on its own, d0018) and returns it.
    pub(crate) async fn remove(
        &self,
        board: &Board,
        data: &DataDir,
        warm: &WarmBuilds,
        project: &Project,
        ticket: &Ticket,
    ) -> Result<(), String> {
        let store = board.store();
        let row = store
            .ticket(&ticket.id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("{} is gone", ticket.key))?;
        let Some(name) = row.resource_name else {
            return Ok(());
        };
        // Removed already (a second click on a stale Cleanup item): Remove never runs twice.
        if row.resource_state.parse::<ResourceState>().ok() == Some(ResourceState::Removed) {
            return Ok(());
        }
        let settings = board
            .project_settings(&project.key)
            .await
            .map_err(|e| e.to_string())?;
        let command = settings.resource_remove.trim();
        if command.is_empty() {
            trash_resource_dir(data, warm, project, &ticket.key);
            return Ok(());
        }
        let log = data
            .resource_log_path(&repo_name(project))
            .map_err(|e| e.to_string())?;
        let own: Vec<(String, String)> =
            std::iter::once((naming::ticket_env_var(), ticket.key.to_string()))
                .chain(resource_vars(data, project, &ticket.key, &name, false))
                .collect();
        let project_env = applied(&settings.env_vars, Os::current());
        let ran = run_command(
            "remove command",
            command,
            Path::new(&project.repo_path),
            &log,
            &project_env,
            &own,
        )
        .await;
        match ran {
            Ok(()) => {
                store
                    .set_resource(&ticket.id, Some(&name), ResourceState::Removed, None)
                    .await
                    .map_err(|e| e.to_string())?;
                tracing::info!(ticket = %ticket.key, resource = %name, "ticket resources removed");
                trash_resource_dir(data, warm, project, &ticket.key);
                Ok(())
            }
            Err(e) => {
                let message = e.to_string();
                store
                    .set_resource(
                        &ticket.id,
                        Some(&name),
                        ResourceState::Failed,
                        Some(&message),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                tracing::warn!(ticket = %ticket.key, resource = %name, error = %message, "removing ticket resources failed");
                Err(message)
            }
        }
    }

    /// **Template refresh** at Done (a merge, never a Close): runs the project's Template refresh command in its
    /// main checkout with `OTAQ_TICKET` (the merged ticket) and `OTAQ_DB_TEMPLATE`, its output appended to the
    /// project's resource log. It holds the project's template lock for writing, so the project's Creates wait
    /// meanwhile, and two refreshes of one project run one after the other. A failure is logged and recorded on
    /// the ticket ([`EventKind::TemplateRefreshFailed`]); it never fails Done.
    pub(crate) async fn refresh_template(
        &self,
        board: &Board,
        data: &DataDir,
        project: &Project,
        ticket: &Ticket,
    ) {
        let settings = match board.project_settings(&project.key).await {
            Ok(settings) => settings,
            Err(e) => {
                tracing::warn!(ticket = %ticket.key, error = %e, "can't read the settings to refresh the template");
                return;
            }
        };
        let command = settings.resource_template_refresh.trim();
        if command.is_empty() {
            return;
        }
        let lock = self.template_lock(&project.id);
        let _writing = lock.write().await;
        let ran = async {
            let log = data
                .resource_log_path(&repo_name(project))
                .map_err(|e| StartError::Other(e.to_string()))?;
            let own = [
                (naming::ticket_env_var(), ticket.key.to_string()),
                (naming::db_template_env_var(), db_template(&project.key)),
            ];
            let project_env = applied(&settings.env_vars, Os::current());
            run_command(
                "template refresh command",
                command,
                Path::new(&project.repo_path),
                &log,
                &project_env,
                &own,
            )
            .await
        }
        .await;
        let Err(e) = ran else {
            tracing::info!(ticket = %ticket.key, project = %project.key, "template refreshed");
            return;
        };
        let message = e.to_string();
        tracing::warn!(ticket = %ticket.key, project = %project.key, error = %message, "the template refresh failed");
        let noted = async {
            let mut tx = board.store().begin_write().await?;
            record_event(
                &mut tx,
                &ticket.id,
                EventKind::TemplateRefreshFailed,
                &Actor::current_user(),
                json!({ "message": message }),
            )
            .await?;
            tx.commit().await?;
            Ok::<_, crate::board::BoardError>(())
        }
        .await;
        if let Err(e) = noted {
            tracing::warn!(ticket = %ticket.key, error = %e, "can't record the failed template refresh");
        }
    }
}

/// Moves Otaq's resource folder of `key` (`res/<repo>/<KEY>`) to the trash, which is emptied in the background. A
/// folder that can't move (a file held open) is left and logged.
pub(crate) fn trash_resource_dir(
    data: &DataDir,
    warm: &WarmBuilds,
    project: &Project,
    key: &TicketKey,
) {
    let repo = repo_name(project);
    let Ok(dir) = data.resource_path(&repo, key) else {
        return;
    };
    if !dir.exists() {
        return;
    }
    if let Err(e) = warm.to_trash(&repo, &dir) {
        tracing::warn!(ticket = %key, error = %e, "can't move the ticket's resource folder to the trash");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> TicketKey {
        text.parse().unwrap()
    }

    fn project() -> Project {
        Project {
            id: "p1".into(),
            key: "SHOP".into(),
            name: "shop-web".into(),
            repo_path: "/code/shop-web".into(),
            base_branch: "main".into(),
            next_ticket_number: 1,
            settings_json: "{}".into(),
            created_at: 0,
            flow_setup_since: None,
            paused_since: None,
        }
    }

    #[test]
    fn db_names_are_lowercase_safe_and_short() {
        let prefix = naming::db_prefix();
        assert_eq!(
            db_name("SHOP", &key("SHOP-18")),
            format!("{prefix}_shop_shop_18")
        );
        assert_eq!(db_template("SHOP"), format!("{prefix}_shop_template"));
        // Anything else becomes one `_`, never two in a row; non-ASCII too.
        assert_eq!(
            db_name("Ünï--cödé", &key("SHOP-2")),
            format!("{prefix}_n_c_d_shop_2")
        );
        // Too long: cut before the ticket part, which stays whole, and a hash keeps two names apart.
        let long = "A".repeat(80);
        let other = "B".repeat(80);
        let name = db_name(&long, &key("SHOP-1234"));
        assert_eq!(name.len(), MAX_DB_NAME, "{name}");
        assert!(name.starts_with(&format!("{prefix}_aaa")), "{name}");
        let (before_hash, hash) = name.rsplit_once('_').unwrap();
        assert!(before_hash.ends_with("_shop_1234"), "{name}");
        assert_eq!(hash.len(), 8);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(
            db_name(&other, &key("SHOP-1234"))
                .rsplit_once('_')
                .unwrap()
                .1,
            hash
        );
        assert_eq!(name, db_name(&long, &key("SHOP-1234")), "stable");
        for name in [name, db_template(&long)] {
            assert!(name.len() <= MAX_DB_NAME);
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            );
            assert!(!name.contains("__"), "{name}");
        }
    }

    #[test]
    fn without_resource_settings_only_the_folder_is_set() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path());
        let settings = ProjectSettings::default();
        let vars = ResourceVars::for_ticket(&data, &project(), &key("SHOP-1"), None, &settings);
        let dir = data.resource_path("shop-web", &key("SHOP-1")).unwrap();
        assert_eq!(
            vars,
            [(
                naming::resource_dir_env_var(),
                dir.to_string_lossy().into_owned()
            )]
        );
        assert!(dir.is_dir(), "made on demand");

        let settings = ProjectSettings {
            resource_create: "createdb x".into(),
            ..ProjectSettings::default()
        };
        let vars = ResourceVars::for_ticket(&data, &project(), &key("SHOP-1"), None, &settings);
        let names: Vec<&str> = vars.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                naming::db_name_env_var(),
                naming::db_template_env_var(),
                naming::resource_dir_env_var()
            ]
        );
        let dir = data.resource_path("shop-web", &key("SHOP-1")).unwrap();
        assert_eq!(vars[2].1, dir.to_string_lossy());
        assert!(dir.is_dir(), "made on demand");
    }

    #[test]
    fn a_recorded_name_wins_over_a_computed_one() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path());
        // Recorded with no Create command any more: the variables stay, with the recorded name.
        let vars = ResourceVars::for_ticket(
            &data,
            &project(),
            &key("SHOP-1"),
            Some("old_name_1"),
            &ProjectSettings::default(),
        );
        assert_eq!(
            vars[0],
            (naming::db_name_env_var(), "old_name_1".to_owned())
        );
        let own = command_vars(
            &data,
            &project(),
            &key("SHOP-1"),
            None,
            &ProjectSettings::default(),
        );
        let dir = data.resource_path("shop-web", &key("SHOP-1")).unwrap();
        assert_eq!(
            own,
            [
                (naming::ticket_env_var(), "SHOP-1".to_owned()),
                (
                    naming::resource_dir_env_var(),
                    dir.to_string_lossy().into_owned()
                )
            ]
        );
    }
}

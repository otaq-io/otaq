//! Project environment variables (T-0294; ENV-01…03, d0082): stored as written, flagged by row, and set in
//! everything Otaq starts in the project: every session kind, the setup command and the git commands whose hooks
//! run, with Otaq's own variables winning and its bin folder kept first on a `PATH` row. Sessions are ordinary
//! `sh` commands that write their environment to a file; no real `claude` starts.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::common::{Fixture, me};
use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::settings::{
    ENV_VAR_EMPTY_NAME, EnvVar, ProjectSettings, env_var_duplicate_message, env_var_equals_message,
    env_var_field,
};
use otaq_core::domain::{ResourceState, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::naming;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, path_without_build_folders};
use otaq_core::session::{SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize};
use otaq_core::ticket_env::resources;

fn rows(pairs: &[(&str, &str)]) -> Vec<EnvVar> {
    pairs.iter().map(|(n, v)| EnvVar::new(n, v)).collect()
}

async fn set_rows(board: &otaq_core::board::Board, env_vars: Vec<EnvVar>) {
    let mut settings = board.project_settings("SHOP").await.unwrap();
    settings.env_vars = env_vars;
    board.set_project_settings("SHOP", &settings).await.unwrap();
}

/// A file's `NAME=value` lines, once it exists (30 s at most).
async fn env_file(path: &Path) -> Vec<String> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            // `env` may still be writing: wait for the end marker.
            if let Ok(text) = std::fs::read_to_string(path)
                && text.contains("END_OF_ENV")
            {
                return text.lines().map(str::to_owned).collect();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{} was never written", path.display()))
}

/// A shell line writing the environment to `file`.
fn write_env(file: &Path) -> String {
    format!(
        "env > '{0}.tmp'; echo END_OF_ENV >> '{0}.tmp'; mv '{0}.tmp' '{0}'",
        file.display()
    )
}

/// The setup command's line writing the environment to `file`: `cmd` syntax on Windows, where setup commands run
/// with `cmd` (Q33).
fn setup_writing_env(file: &Path) -> String {
    if cfg!(windows) {
        format!(
            "set > \"{0}.tmp\"& echo END_OF_ENV>> \"{0}.tmp\"& move /y \"{0}.tmp\" \"{0}\" >nul",
            file.display()
        )
    } else {
        write_env(file)
    }
}

fn has(env: &[String], line: &str) -> bool {
    env.iter().any(|l| l == line)
}

fn value<'a>(env: &'a [String], name: &str) -> Option<&'a str> {
    env.iter().find_map(|l| l.strip_prefix(&format!("{name}=")))
}

#[tokio::test]
async fn rows_default_to_none_and_old_settings_read_without_them() {
    assert!(ProjectSettings::default().env_vars.is_empty());
    let f = Fixture::shop().await;
    assert!(
        f.board
            .project_settings("SHOP")
            .await
            .unwrap()
            .env_vars
            .is_empty()
    );
    // Settings stored before the field existed.
    sqlx::query("UPDATE projects SET settings_json = ? WHERE key = 'SHOP'")
        .bind(r#"{"setup_command":"echo ready","env_files":[]}"#)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    let settings = f.board.project_settings("SHOP").await.unwrap();
    assert!(settings.env_vars.is_empty());
    assert_eq!(settings.setup_command, "echo ready");
    assert!(
        f.board
            .project_setting_problems("SHOP")
            .await
            .unwrap()
            .is_empty()
    );

    // Stored as written, in order, values in plain text.
    let written = rows(&[("CI", "true"), ("TOKEN", "s3cret"), ("CI", "false")]);
    set_rows(&f.board, written.clone()).await;
    assert_eq!(
        f.board.project_settings("SHOP").await.unwrap().env_vars,
        written
    );
    let json: String = sqlx::query_scalar("SELECT settings_json FROM projects WHERE key = 'SHOP'")
        .fetch_one(f.board.store().pool())
        .await
        .unwrap();
    assert!(
        json.contains(r#"{"name":"TOKEN","value":"s3cret"}"#),
        "{json}"
    );
}

#[tokio::test]
async fn problems_name_their_row_and_saving_never_refuses() {
    let f = Fixture::shop().await;
    set_rows(
        &f.board,
        rows(&[
            ("CI", "true"),
            ("", "orphan"),
            ("MAX=JOBS", "2"),
            ("A\0B", "1"),
            ("CI", "again"),
            ("MY VAR", ""),
            ("1ST", "a=b"),
        ]),
    )
    .await;
    assert_eq!(
        f.board
            .project_settings("SHOP")
            .await
            .unwrap()
            .env_vars
            .len(),
        7,
        "kept as written"
    );
    let problems = f.board.project_setting_problems("SHOP").await.unwrap();
    let of = |row: usize| {
        problems
            .iter()
            .find(|p| p.field == env_var_field(row))
            .map(|p| p.message.clone())
    };
    assert_eq!(of(0), None);
    assert_eq!(of(1).as_deref(), Some(ENV_VAR_EMPTY_NAME));
    assert_eq!(of(2), Some(env_var_equals_message("MAX=JOBS")));
    assert!(of(3).unwrap().contains("null character"));
    assert_eq!(of(4), Some(env_var_duplicate_message("CI")));
    assert_eq!(of(5), None, "a space is allowed");
    assert_eq!(of(6), None, "a leading digit is allowed");
    assert_eq!(env_var_field(4), "env_vars.4");
}

/// Every session kind gets the applied rows; Otaq's own variables win over a row of the same name.
#[tokio::test]
async fn every_session_kind_gets_the_rows() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Env").await;
    set_rows(
        &w.board,
        rows(&[
            ("CI", "true"),
            ("DATABASE_URL", "postgres://localhost:5432/shop_dev"),
            ("DATABASE_URL", "second"),
            ("BAD=NAME", "x"),
            ("PORT", "1"),
            (&naming::ticket_env_var(), "NOPE-1"),
        ]),
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    for kind in SessionKind::ALL {
        let file = dir.path().join(format!("{kind}.env"));
        w.sessions
            .spawn(SpawnSpec {
                kind: *kind,
                start: Start::User,
                project_id: project.id.clone(),
                project_key: project.key.clone(),
                ticket: Some(SessionTicket {
                    id: ticket.id.clone(),
                    key: ticket.key.clone(),
                }),
                program: "sh".into(),
                args: vec!["-c".into(), write_env(&file)],
                cwd: PathBuf::from(&project.repo_path),
                env: vec![
                    (naming::ticket_env_var(), ticket.key.to_string()),
                    ("PORT".into(), "4019".into()),
                    ("PATH".into(), path_without_build_folders()),
                ],
                size: TerminalSize::default(),
                tool: None,
                tool_session_id: None,
                model: None,
                agent: None,
                effort: None,
                theme: None,
            })
            .await
            .unwrap();
        let env = env_file(&file).await;
        assert!(has(&env, "CI=true"), "{kind}: {env:?}");
        assert!(
            has(&env, "DATABASE_URL=postgres://localhost:5432/shop_dev"),
            "{kind}: the first row applies"
        );
        assert!(!env.iter().any(|l| l.starts_with("BAD=NAME")), "{kind}");
        assert_eq!(value(&env, "PORT"), Some("4019"), "{kind}: Otaq's own win");
        assert_eq!(
            value(&env, &naming::ticket_env_var()),
            Some(ticket.key.to_string().as_str()),
            "{kind}"
        );
    }
    w.stop().await;
}

/// The ticket agent's real start: the setup command and the agent both get the rows, the agent's `PORT` and
/// ticket stay Otaq's, and a `PATH` row keeps Otaq's bin folder first.
#[tokio::test]
async fn the_setup_command_and_the_started_agent_get_the_rows() {
    let dir = tempfile::tempdir().unwrap();
    let agent_file = dir.path().join("agent.env");
    let setup_file = dir.path().join("setup.env");
    let script = format!("{}; exec cat", write_env(&agent_file));
    let launch = LaunchContext {
        bin_dir: "/opt/cb/bin".into(),
        plugin_dir: "/opt/cb/plugin".into(),
        path: path_without_build_folders().into(),
        claude: ClaudeLaunch {
            program: vec!["sh".into(), "-c".into(), script, "sh".into()],
            config_dir: None,
        },
    };
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(launch),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Env").await;
    let row_path = std::env::join_paths(
        [PathBuf::from("/custom/bin")]
            .into_iter()
            .chain(std::env::split_paths(&path_without_build_folders())),
    )
    .unwrap()
    .to_string_lossy()
    .into_owned();
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.setup_command = setup_writing_env(&setup_file);
    settings.env_vars = rows(&[("CI", "true"), ("PORT", "1"), ("PATH", &row_path)]);
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();

    w.force_status(&ticket, Status::Queued).await;
    w.starter
        .start(
            &ticket.key.to_string(),
            &me(),
            otaq_core::session::Start::Auto,
        )
        .await
        .unwrap();

    let setup = env_file(&setup_file).await;
    assert!(has(&setup, "CI=true"), "{setup:?}");
    let agent = env_file(&agent_file).await;
    assert!(has(&agent, "CI=true"), "{agent:?}");
    assert_ne!(value(&agent, "PORT"), Some("1"), "Otaq's own PORT wins");
    assert_eq!(
        value(&agent, &naming::ticket_env_var()),
        Some(ticket.key.to_string().as_str())
    );
    // Git for Windows' `sh` rewrites PATH in its own form; the order rule is tested on every platform in
    // `session::env`.
    if cfg!(unix) {
        let path = value(&agent, "PATH").unwrap();
        assert!(path.starts_with("/opt/cb/bin:/custom/bin:"), "{path}");
        assert_eq!(value(&setup, "PATH"), Some(row_path.as_str()));
    }
    w.stop().await;
}

/// The checkpoint commit and `worktree add` run the user's hooks with the rows set (GIT-02).
#[tokio::test]
async fn the_user_s_git_hooks_see_the_rows() {
    let f = Fixture::shop().await;
    let repo = f.repo();
    let log = f.home.path().join("hooks.log");
    for hook in ["pre-commit", "post-checkout"] {
        let path = repo.join(".git/hooks").join(hook);
        std::fs::write(
            &path,
            format!("#!/bin/sh\necho \"{hook} CI=$CI\" >> '{}'\n", log.display()),
        )
        .unwrap();
        // Git for Windows runs hooks with its own sh; only macOS and Linux need the executable bit.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    set_rows(&f.board, rows(&[("CI", "hooked")])).await;
    let ticket = f.create("Hooks").await;
    let worktree = f.start_worktree(&ticket).await;
    let logged = std::fs::read_to_string(&log).unwrap();
    assert!(logged.contains("post-checkout CI=hooked"), "{logged}");

    commit(&repo, "main.txt", "moved on\n");
    std::fs::write(worktree.join("dirty.txt"), "uncommitted\n").unwrap();
    let git_with_rows = f.board.project_git("SHOP").await.unwrap();
    git_with_rows
        .merge_with_checkpoint(&worktree, "main")
        .await
        .unwrap();
    let logged = std::fs::read_to_string(&log).unwrap();
    assert!(logged.contains("pre-commit CI=hooked"), "{logged}");
    assert_eq!(
        std::fs::read_to_string(worktree.join("dirty.txt")).unwrap(),
        "uncommitted\n"
    );
    assert_eq!(
        git(&worktree, &["rev-parse", "HEAD"]),
        git(&repo, &["rev-parse", "main"]),
        "up to date with main (a fast-forward: the branch had no commits of its own)"
    );
}

/// A change reaches the next session; a running one keeps the values it started with.
#[tokio::test]
async fn a_change_reaches_the_next_spawn_and_not_a_running_one() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Env").await;
    set_rows(&w.board, rows(&[("T0294_VALUE", "first")])).await;
    let running = w.shell(&ticket, SessionKind::Shell).await;
    assert_eq!(
        running.run("echo $T0294_VALUE").await.stdout.trim(),
        "first"
    );

    set_rows(&w.board, rows(&[("T0294_VALUE", "second")])).await;
    assert_eq!(
        running.run("echo $T0294_VALUE").await.stdout.trim(),
        "first"
    );
    let next = w.shell(&ticket, SessionKind::Shell).await;
    assert_eq!(next.run("echo $T0294_VALUE").await.stdout.trim(), "second");

    set_rows(&w.board, Vec::new()).await;
    let none = w.shell(&ticket, SessionKind::Shell).await;
    assert_eq!(
        none.run("echo \"[${T0294_VALUE-unset}]\"")
            .await
            .stdout
            .trim(),
        "[unset]"
    );
    w.stop().await;
}

/// Turns on ticket resources for SHOP (a Create command; the sessions below never run it).
async fn with_resources(board: &otaq_core::board::Board, env_vars: Vec<EnvVar>) {
    let mut settings = board.project_settings("SHOP").await.unwrap();
    settings.resource_create = "createdb x".into();
    settings.env_vars = env_vars;
    board.set_project_settings("SHOP", &settings).await.unwrap();
}

/// A session spawned with `sh -c` writing its environment to `file`, for `ticket` (or none: a project session).
async fn spawn_writing_env(
    w: &TestService,
    project: &otaq_core::store::projects::Project,
    kind: SessionKind,
    ticket: Option<&otaq_core::board::tickets::Ticket>,
    file: &Path,
) -> Vec<String> {
    w.sessions
        .spawn(SpawnSpec {
            kind,
            start: Start::User,
            project_id: project.id.clone(),
            project_key: project.key.clone(),
            ticket: ticket.map(|t| SessionTicket {
                id: t.id.clone(),
                key: t.key.clone(),
            }),
            program: "sh".into(),
            args: vec!["-c".into(), write_env(file)],
            cwd: PathBuf::from(&project.repo_path),
            env: vec![("PATH".into(), path_without_build_folders())],
            size: TerminalSize::default(),
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            theme: None,
        })
        .await
        .unwrap();
    env_file(file).await
}

/// Ticket resources (M19-d, d0087): every session of a ticket (the agent, validator, answering, Run and Shell) gets
/// `OTAQ_DB_NAME`, `OTAQ_DB_TEMPLATE` and `OTAQ_RESOURCE_DIR`; a project's own sessions get none.
#[tokio::test]
async fn ticket_sessions_run_and_shell_get_the_resource_vars() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Env").await;
    with_resources(&w.board, rows(&[("CI", "true")])).await;
    let name = resources::db_name("SHOP", &ticket.key);
    let dir = w.data.resource_path("shop-web", &ticket.key).unwrap();
    let dir = dir.to_string_lossy().into_owned();
    let tmp = tempfile::tempdir().unwrap();
    for kind in SessionKind::ALL {
        let file = tmp.path().join(format!("{kind}.env"));
        let env = spawn_writing_env(&w, &project, *kind, Some(&ticket), &file).await;
        assert_eq!(
            value(&env, &naming::db_name_env_var()),
            Some(name.as_str()),
            "{kind}"
        );
        assert_eq!(
            value(&env, &naming::db_template_env_var()),
            Some(resources::db_template("SHOP").as_str()),
            "{kind}"
        );
        // Git for Windows' `sh` may print the folder in its own form; it is the same folder.
        let shown = value(&env, &naming::resource_dir_env_var()).unwrap_or_default();
        assert!(
            shown == dir || cfg!(windows) && shown.ends_with("SHOP-1"),
            "{kind}: {shown}"
        );
        assert!(has(&env, "CI=true"), "{kind}");
    }
    assert!(Path::new(&dir).is_dir(), "the folder is made on demand");

    // The ticket's Shell, opened the real way.
    let shell = w.shell(&ticket, SessionKind::Shell).await;
    let db = format!("echo ${}", naming::db_name_env_var());
    assert_eq!(shell.run(&db).await.stdout.trim(), name);

    // A project's own session has no ticket, so no resource variables.
    let file = tmp.path().join("no-ticket.env");
    let env = spawn_writing_env(&w, &project, SessionKind::Project, None, &file).await;
    for var in [
        naming::db_name_env_var(),
        naming::db_template_env_var(),
        naming::resource_dir_env_var(),
    ] {
        assert_eq!(value(&env, &var), None, "{var}");
    }
    w.stop().await;
}

/// T-0445 (CMT-09, d0109): with no ticket resources, every session of a ticket still gets `OTAQ_RESOURCE_DIR` (the
/// folder long output is saved to, made on demand) and no `OTAQ_DB_*`.
#[tokio::test]
async fn ticket_sessions_get_the_resource_folder_without_resources() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Env").await;
    let dir = w.data.resource_path("shop-web", &ticket.key).unwrap();
    let dir = dir.to_string_lossy().into_owned();
    let tmp = tempfile::tempdir().unwrap();
    for kind in SessionKind::ALL {
        let file = tmp.path().join(format!("{kind}.env"));
        let env = spawn_writing_env(&w, &project, *kind, Some(&ticket), &file).await;
        for var in [naming::db_name_env_var(), naming::db_template_env_var()] {
            assert_eq!(value(&env, &var), None, "{kind}: {var}");
        }
        let shown = value(&env, &naming::resource_dir_env_var()).unwrap_or_default();
        assert!(
            shown == dir || cfg!(windows) && shown.ends_with("SHOP-1"),
            "{kind}: {shown}"
        );
    }
    assert!(Path::new(&dir).is_dir(), "the folder is made on demand");
    w.stop().await;
}

/// Otaq's resource variables win over a project row of the same name, and the recorded name wins over a computed
/// one.
#[tokio::test]
async fn a_project_row_cannot_override_the_resource_vars() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Env").await;
    with_resources(
        &w.board,
        rows(&[
            (&naming::db_name_env_var(), "mine"),
            (&naming::db_template_env_var(), "my_template"),
        ]),
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let env = spawn_writing_env(
        &w,
        &project,
        SessionKind::Agent,
        Some(&ticket),
        &tmp.path().join("a.env"),
    )
    .await;
    assert_eq!(
        value(&env, &naming::db_name_env_var()),
        Some(resources::db_name("SHOP", &ticket.key).as_str())
    );
    assert_eq!(
        value(&env, &naming::db_template_env_var()),
        Some(resources::db_template("SHOP").as_str())
    );

    w.board
        .store()
        .set_resource(
            &ticket.id,
            Some("recorded_db"),
            ResourceState::Created,
            None,
        )
        .await
        .unwrap();
    let env = spawn_writing_env(
        &w,
        &project,
        SessionKind::Validator,
        Some(&ticket),
        &tmp.path().join("b.env"),
    )
    .await;
    assert_eq!(value(&env, &naming::db_name_env_var()), Some("recorded_db"));
    w.stop().await;
}

/// `${OTAQ_DB_NAME}` in a project row (plan §6 item 4, d0087): a ticket's sessions get the ticket's database name
/// in the value; a project's own session, where Otaq sets no resource variables, gets it empty. Other `${…}` stay
/// as written.
#[tokio::test]
async fn database_url_gets_the_tickets_db_name() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Env").await;
    let url = format!("postgres://localhost/${{{}}}", naming::db_name_env_var());
    with_resources(
        &w.board,
        rows(&[("DATABASE_URL", &url), ("KEEP", "${HOME_NOT_OURS}")]),
    )
    .await;
    let name = resources::db_name("SHOP", &ticket.key);
    let tmp = tempfile::tempdir().unwrap();
    for kind in [
        SessionKind::Agent,
        SessionKind::Validator,
        SessionKind::Shell,
    ] {
        let env = spawn_writing_env(
            &w,
            &project,
            kind,
            Some(&ticket),
            &tmp.path().join(format!("{kind}.env")),
        )
        .await;
        assert_eq!(
            value(&env, "DATABASE_URL"),
            Some(format!("postgres://localhost/{name}").as_str()),
            "{kind}"
        );
        assert_eq!(value(&env, "KEEP"), Some("${HOME_NOT_OURS}"), "{kind}");
    }
    let env = spawn_writing_env(
        &w,
        &project,
        SessionKind::Project,
        None,
        &tmp.path().join("project.env"),
    )
    .await;
    assert_eq!(value(&env, "DATABASE_URL"), Some("postgres://localhost/"));
    w.stop().await;
}

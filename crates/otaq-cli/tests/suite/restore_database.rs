//! A board from a newer build (UPD-08, d0140 gap review G5): the service refuses it with the plain message and the
//! typed reason, and `otaq restore-database --yes` puts the copy back and starts the service. A temporary data
//! folder and home; the service is stopped at the end even if the test fails.

use std::path::Path;
use std::process::{Command, Output};

use otaq_core::client::{Client, ClientConfig, CommandLauncher};
use otaq_core::data_dir::DataDir;
use otaq_core::naming;
use otaq_core::store::{Store, versions};
use serde_json::Value;

struct Home {
    tmp: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().unwrap(),
        }
    }

    fn path(&self) -> &Path {
        self.tmp.path()
    }

    fn data(&self) -> DataDir {
        DataDir::new(self.path())
    }

    fn env(&self) -> Vec<(String, std::path::PathBuf)> {
        let p = self.path();
        vec![
            (DataDir::env_var(), p.to_owned()),
            // The real service locates Claude Code's config file: never the user's (PRJ-06).
            ("CLAUDE_CONFIG_DIR".into(), p.to_owned()),
            // Nor any agent tool's: the service detects the tools on an empty test PATH, with a test home.
            ("PATH".into(), p.join("no-tools")),
            ("HOME".into(), p.to_owned()),
        ]
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_otaq"))
            .args(args)
            .envs(self.env())
            .env_remove(naming::ticket_env_var())
            .output()
            .unwrap()
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".db"))
            .collect();
        names.sort();
        names
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = self.run(&["daemon", "stop"]);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_newer_board_is_refused_plainly_and_restored_from_its_copy() {
    let home = Home::new();
    let database = home.data().database_path();
    versions::testing_change_as_newer(&database, "9.0.0")
        .await
        .unwrap();
    let copy = versions::copy_path(&database, "9.0.0");
    assert!(copy.is_file());

    // The start refuses it: page 32's words, no raw migration error, and the typed reason with --json.
    let human = home.run(&["daemon", "start"]);
    assert!(!human.status.success());
    let stderr = text(&human.stderr);
    assert!(
        stderr.contains(&format!(
            "This board was last opened by {} 9.0.0. Run `{} restore-database` to go back to the copy from \
             before it, or reinstall 9.0.0.",
            naming::PRODUCT_NAME,
            naming::COMMAND_NAME
        )),
        "{stderr}"
    );
    assert!(!stderr.contains("migration"), "{stderr}");
    let json = home.run(&["daemon", "start", "--json"]);
    let body: Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(body["error"]["kind"], "refused");
    assert_eq!(body["error"]["data"]["refusal"], "newer_database");
    assert_eq!(
        body["error"]["data"]["newer_database"]["last_opened_by"],
        "9.0.0"
    );
    assert_eq!(
        body["error"]["data"]["newer_database"]["copy"]["path"],
        copy.to_str().unwrap()
    );

    // The app's start gets the typed reason too.
    let mut launcher = CommandLauncher::new(env!("CARGO_BIN_EXE_otaq"));
    for (key, value) in home.env() {
        launcher = launcher.with_env(key, value);
    }
    let Err(failed) = Client::connect_or_start(ClientConfig::new(&home.data()), &launcher).await
    else {
        panic!("the start is refused");
    };
    let newer = failed.newer_database().expect("the typed reason");
    assert_eq!(newer.last_opened_by, "9.0.0");
    assert_eq!(newer.copy.as_ref().unwrap().path, copy);

    // Without a terminal and without --yes: refused, nothing changed.
    let before = home.names();
    let asked = home.run(&["restore-database"]);
    assert!(!asked.status.success(), "{asked:?}");
    assert!(text(&asked.stderr).contains("--yes"), "{asked:?}");
    assert_eq!(home.names(), before);

    // --yes: the steps, the copy back, the current one kept aside, the service started.
    let restored = home.run(&["restore-database", "--yes"]);
    assert!(restored.status.success(), "{restored:?}");
    let stdout = text(&restored.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "Background service stopped", "{stdout}");
    assert!(
        lines[1].starts_with("Putting back the copy from "),
        "{stdout}"
    );
    assert_eq!(lines[2], "Starting the background service", "{stdout}");
    assert!(
        lines[3].starts_with(&format!(
            "The board is back as it was before {} 9.0.0.",
            naming::PRODUCT_NAME
        )),
        "{stdout}"
    );
    let names = home.names();
    assert!(
        names.iter().any(|n| n.contains(".db.replaced-")),
        "{names:?}"
    );
    assert!(copy.is_file(), "the copy stays");
    let status: Value =
        serde_json::from_slice(&home.run(&["daemon", "status", "--json"]).stdout).unwrap();
    assert_eq!(status["running"], true);
    assert!(Store::newer_database(&database).await.unwrap().is_none());
}

#[test]
fn without_a_copy_nothing_changes() {
    let home = Home::new();
    std::fs::write(home.data().database_path(), "the board").unwrap();
    let out = home.run(&["restore-database", "--yes"]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        text(&out.stderr).contains("There's no copy of the board"),
        "{out:?}"
    );
    assert_eq!(
        std::fs::read_to_string(home.data().database_path()).unwrap(),
        "the board"
    );
    assert_eq!(home.names().len(), 1);
}

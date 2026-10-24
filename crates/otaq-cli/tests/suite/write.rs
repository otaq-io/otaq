//! `otaq create` and `otaq update` (T-0069, PLG-05, CLI-02, CLI-04) against a service with the sample
//! board.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use otaq_core::board::tickets::NewTicketInput;
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Actor, Kind};
use otaq_core::naming;
use otaq_core::service::testing::TestService;
use serde_json::Value;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

fn cb_in(w: &TestService, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(CB)
        .args(args)
        .env(DataDir::env_var(), w.data.root())
        .env_remove(naming::ticket_env_var())
        .current_dir(w.home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn cb(w: &TestService, args: &[&str]) -> Output {
    cb_in(w, args, "")
}

fn ok(out: &Output) -> String {
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn error(out: &Output) -> Value {
    assert!(!out.status.success());
    serde_json::from_slice::<Value>(&out.stdout).unwrap()["error"].clone()
}

async fn count(w: &TestService) -> usize {
    w.board.tickets("SHOP").await.unwrap().len()
}

#[tokio::test(flavor = "multi_thread")]
async fn create_with_every_flag() {
    let w = TestService::start().await;
    w.shop_sample().await;

    let out = cb(
        &w,
        &[
            "-P",
            "SHOP",
            "create",
            "Gift wrap option",
            "-d",
            "Let buyers add gift wrap.",
            "--deps",
            "SHOP-18,19",
            "--json",
        ],
    );
    let ticket: Value = serde_json::from_str(&ok(&out)).unwrap();
    assert_eq!(ticket["key"], "SHOP-41");
    assert_eq!(ticket["place"], "backlog");
    assert_eq!(ticket["body"], "Let buyers add gift wrap.");
    assert!(
        ticket.get("done_when").is_none(),
        "no Done when field (d0067)"
    );
    assert_eq!(
        ticket["depends_on"],
        serde_json::json!(["SHOP-18", "SHOP-19"])
    );

    assert_eq!(
        ok(&cb(&w, &["-P", "SHOP", "new", "Quiet one", "--silent"])),
        "SHOP-42\n"
    );
    assert_eq!(
        ok(&cb(&w, &["-P", "SHOP", "create", "Human one"])),
        "Created SHOP-43 · Human one\n"
    );

    // Long text through stdin, quotes and backticks untouched.
    let body = "Rename `fullName` to \"first\" and 'last'.\n\n```ts\nconst a = `x`;\n```\n";
    let out = cb_in(
        &w,
        &["-P", "SHOP", "create", "Split names", "--stdin", "--json"],
        body,
    );
    let ticket: Value = serde_json::from_str(&ok(&out)).unwrap();
    assert_eq!(ticket["body"], body.trim_end());

    // --parent makes an epic child; --type epic an epic.
    let child: Value = serde_json::from_str(&ok(&cb(
        &w,
        &[
            "create",
            "Wishlist sharing",
            "--parent",
            "SHOP-40",
            "--json",
        ],
    )))
    .unwrap();
    assert_eq!(child["key"], "SHOP-40.4");
    let epic: Value = serde_json::from_str(&ok(&cb(
        &w,
        &[
            "-P",
            "SHOP",
            "create",
            "Gift cards",
            "--type",
            "epic",
            "--json",
        ],
    )))
    .unwrap();
    assert_eq!(epic["kind"], Kind::Epic.as_str());
    assert_eq!(epic["created_by"], Actor::current_user().to_string());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn dependency_checks_apply_and_nothing_is_left_behind() {
    let w = TestService::start().await;
    w.shop_sample().await;
    w.board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Release hotfix".into(),
                base_branch: Some("release/2.3".into()),
                ..Default::default()
            },
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    let before = count(&w).await;
    let e = error(&cb(
        &w,
        &[
            "-P", "SHOP", "create", "Cross", "--deps", "SHOP-41", "--json",
        ],
    ));
    assert_eq!(e["data"]["refusal"], "cross_base", "{e}");
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains("Linked tickets must use the same base branch.")
    );
    let e = error(&cb(
        &w,
        &[
            "-P", "SHOP", "create", "Ghost", "--deps", "SHOP-99", "--json",
        ],
    ));
    assert_eq!(e["kind"], "not_found");
    assert_eq!(count(&w).await, before, "nothing created");
    w.stop().await;
}

const PLAN: &str = "# Gift cards plan

### P1 · Gift card model
**What and why**
Store cards with a balance.

**Done when**
- [ ] cards have a code and a balance

### P2 · Redeem at checkout
- **Depends on:** P1, SHOP-18

Apply the balance to the order.

**Done when**
- [ ] the balance is taken off the total
- [ ] a used-up card is refused

### P3 · Gift card emails
- **Depends on:** P2
";

#[tokio::test(flavor = "multi_thread")]
async fn batch_import_all_or_nothing() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let dir = tempfile::tempdir().unwrap();
    let plan = dir.path().join("plan.md");
    std::fs::write(&plan, PLAN).unwrap();

    let out = cb(
        &w,
        &[
            "-P",
            "SHOP",
            "create",
            "-f",
            plan.to_str().unwrap(),
            "--json",
        ],
    );
    let created: Value = serde_json::from_str(&ok(&out)).unwrap();
    let created = created.as_array().unwrap();
    assert_eq!(created.len(), 3);
    assert_eq!(created[1]["title"], "Redeem at checkout");
    assert_eq!(
        created[1]["body"],
        "Apply the balance to the order.\n\n## Done when\n\n- the balance is taken off the total\n\
         - a used-up card is refused"
    );
    assert_eq!(
        created[1]["depends_on"],
        serde_json::json!(["SHOP-18", "SHOP-41"])
    );
    assert_eq!(created[2]["depends_on"], serde_json::json!(["SHOP-42"]));
    assert_eq!(
        created[0]["body"],
        "Store cards with a balance.\n\n## Done when\n\n- cards have a code and a balance"
    );
    assert_eq!(created[2]["body"], "");

    // A reference to nothing, or a link the board refuses: nothing is created.
    let before = count(&w).await;
    std::fs::write(&plan, PLAN.replace("P1, SHOP-18", "P1, T-9999")).unwrap();
    let e = error(&cb(
        &w,
        &[
            "-P",
            "SHOP",
            "create",
            "-f",
            plan.to_str().unwrap(),
            "--json",
        ],
    ));
    assert_eq!(e["kind"], "not_found");
    w.board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Release hotfix".into(),
                base_branch: Some("release/2.3".into()),
                ..Default::default()
            },
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    let before = before + 1;
    std::fs::write(
        &plan,
        PLAN.replace("**Depends on:** P2", "**Depends on:** P2, SHOP-44"),
    )
    .unwrap();
    let e = error(&cb(
        &w,
        &[
            "-P",
            "SHOP",
            "create",
            "-f",
            plan.to_str().unwrap(),
            "--json",
        ],
    ));
    assert_eq!(e["data"]["refusal"], "cross_base", "{e}");
    assert_eq!(count(&w).await, before, "the batch was undone");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn update_changes_only_the_given_fields() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let before = w.board.ticket("SHOP-19").await.unwrap();
    let out = cb(
        &w,
        &["update", "SHOP-19", "-d", "Sorted by price.", "--json"],
    );
    let after: Value = serde_json::from_str(&ok(&out)).unwrap();
    assert_eq!(after["title"], before.title);
    assert_eq!(after["body"], "Sorted by price.");
    let out = cb_in(
        &w,
        &["update", "SHOP-19", "--stdin", "--title", "Price table v2"],
        "New body\n",
    );
    assert_eq!(ok(&out), "Updated SHOP-19 · Price table v2\n");
    let t = w.board.ticket("SHOP-19").await.unwrap();
    assert_eq!(
        (t.title.as_str(), t.body.as_str()),
        ("Price table v2", "New body")
    );
    let e = error(&cb(&w, &["update", "SHOP-19", "--json"]));
    assert_eq!(e["kind"], "usage");
    assert_eq!(
        e["message"],
        "nothing to change: give --title, -d, -p or --stdin"
    );
    w.stop().await;
}

/// From inside SHOP-19's agent session: its tickets are "created by agent", its edits are tracked, and other
/// tickets are refused.
#[tokio::test(flavor = "multi_thread")]
async fn agent_created_tickets_and_agent_edits() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    let agent = w.agent_shell(&shop19).await;

    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' create 'Price table: currency column' --stdin --json"),
            "Show `EUR` and \"USD\".",
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let created: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(created["created_by"], "agent:SHOP-19");
    assert_eq!(created["place"], "backlog");
    assert_eq!(created["body"], "Show `EUR` and \"USD\".");

    let ran = agent
        .run(&format!("'{CB}' update -d 'Prices align right.'"))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let edits = w.board.agent_edits("SHOP-19").await.unwrap();
    assert!(!edits.changes.is_empty(), "{edits:?}");

    let ran = agent
        .run(&format!("'{CB}' update SHOP-20 --title nope --json"))
        .await;
    assert_ne!(ran.status, 0);
    let e: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        e["error"]["message"],
        "SHOP-19's agent can't change SHOP-20"
    );
    // An agent's --actor can't make it the user.
    let ran = agent
        .run(&format!(
            "'{CB}' create 'Claims to be me' --actor me --json"
        ))
        .await;
    let created: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(created["created_by"], "agent:SHOP-19");
    w.stop().await;
}

/// Q17 (deferred, built as option 1): an agent's new ticket can't depend on a ticket it didn't create, even its
/// own; nothing is created.
#[tokio::test(flavor = "multi_thread")]
async fn agent_links_follow_the_literal_rule() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    let agent = w.agent_shell(&shop19).await;
    let before = count(&w).await;
    let ran = agent
        .run(&format!(
            "'{CB}' create 'Currency column' --deps SHOP-19 --json"
        ))
        .await;
    assert_ne!(ran.status, 0);
    let e: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(e["error"]["data"]["refusal"], "not_allowed", "{e}");
    assert_eq!(count(&w).await, before);
    w.stop().await;
}

/// "Done when" is gone (TKT-04, d0067): its flags are unknown, like any other made-up flag.
#[tokio::test(flavor = "multi_thread")]
async fn done_when_flags_are_unknown() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let before = count(&w).await;
    for args in [
        &["create", "Gift wrap", "--done-when", "x", "--json"][..],
        &["update", "SHOP-19", "--add-done-when", "x", "--json"][..],
    ] {
        let e = error(&cb(&w, args));
        assert_eq!(e["kind"], "usage");
        let message = e["message"].as_str().unwrap();
        assert!(message.contains("unexpected argument '--"), "{message}");
    }
    assert_eq!(count(&w).await, before, "nothing was created");
    w.stop().await;
}

/// Priorities (T-0177, TKT-09, d0065): `-p` on create and update, `list --priority`, `show`; anything outside 1–4
/// is refused with the allowed values; a `-f` file reads "- **Priority:**".
#[tokio::test(flavor = "multi_thread")]
async fn priorities_on_the_command_line() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let out = cb(
        &w,
        &["-P", "SHOP", "create", "Payments down", "-p", "1", "--json"],
    );
    let created: Value = serde_json::from_str(&ok(&out)).unwrap();
    assert_eq!(created["priority"], 1);
    let key = created["key"].as_str().unwrap().to_owned();
    let plain = cb(&w, &["-P", "SHOP", "create", "Footer links", "--json"]);
    assert_eq!(
        serde_json::from_str::<Value>(&ok(&plain)).unwrap()["priority"],
        3
    );

    let out = cb(&w, &["update", &key, "-p", "2", "--json"]);
    assert_eq!(
        serde_json::from_str::<Value>(&ok(&out)).unwrap()["priority"],
        2
    );
    let listed = ok(&cb(&w, &["-P", "SHOP", "list", "--priority", "2"]));
    assert_eq!(listed.lines().count(), 1, "{listed}");
    assert!(listed.contains(&key) && listed.contains(" P2 "), "{listed}");
    let shown = ok(&cb(&w, &["show", &key]));
    assert!(shown.contains("Priority:    P2 · High\n"), "{shown}");

    for args in [
        &["-P", "SHOP", "create", "Nope", "-p", "0", "--json"][..],
        &["update", &key, "-p", "5", "--json"][..],
        &["-P", "SHOP", "list", "--priority", "9", "--json"][..],
    ] {
        let e = error(&cb(&w, args));
        assert_eq!(e["kind"], "usage");
        let message = e["message"].as_str().unwrap();
        assert!(
            message.contains("Priority is 1 (Urgent) to 4 (Low)"),
            "{message}"
        );
    }

    let dir = tempfile::tempdir().unwrap();
    let plan = dir.path().join("plan.md");
    std::fs::write(
        &plan,
        "### Gift cards\n- **Priority:** 4\n\n### Gift card emails\n",
    )
    .unwrap();
    let out = cb(
        &w,
        &[
            "-P",
            "SHOP",
            "create",
            "-f",
            plan.to_str().unwrap(),
            "--json",
        ],
    );
    let created: Value = serde_json::from_str(&ok(&out)).unwrap();
    assert_eq!(created[0]["priority"], 4);
    assert_eq!(created[1]["priority"], 3, "no line: Normal");
    w.stop().await;
}

/// #43: `otaq create` inside the user's own git worktree of a project's repository (`git worktree add
/// ../experiment`, outside the repository's folder) finds the project through git's common dir; a folder that is no
/// project's worktree still asks for `-P`.
#[tokio::test(flavor = "multi_thread")]
async fn create_in_your_own_worktree_finds_the_project() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let repo = w.board.project("SHOP").await.unwrap().repo_path;
    let experiment = w.home.path().join("habitat-experiment");
    otaq_core::git::testing::git(
        std::path::Path::new(&repo),
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "experiment/accent",
            &experiment.display().to_string(),
        ],
    );
    let before = count(&w).await;
    let run = |cwd: &std::path::Path| {
        Command::new(CB)
            .args(["create", "Violet accent", "--json"])
            .env(DataDir::env_var(), w.data.root())
            .env_remove(naming::ticket_env_var())
            .current_dir(cwd)
            .output()
            .unwrap()
    };
    let made: Value = serde_json::from_str(&ok(&run(&experiment))).unwrap();
    assert!(made["key"].as_str().unwrap().starts_with("SHOP-"), "{made}");
    assert_eq!(count(&w).await, before + 1);

    let elsewhere = w.home.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    otaq_core::git::testing::git(&elsewhere, &["init", "--quiet"]);
    let e = error(&run(&elsewhere));
    assert_eq!(
        e["message"], "name a project with -P SHOP, or run it inside a project's folder",
        "{e}"
    );
    assert_eq!(count(&w).await, before + 1);
    w.stop().await;
}

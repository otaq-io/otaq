//! `otaq dep add | remove | tree | cycles` (T-0070, DEP-01, DEP-02, CLI-02) on the sample board.

use std::process::{Command, Output};

use otaq_core::board::tickets::NewTicketInput;
use otaq_core::data_dir::DataDir;
use otaq_core::domain::Actor;
use otaq_core::naming;
use otaq_core::service::testing::TestService;
use serde_json::Value;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

fn cb(w: &TestService, args: &[&str]) -> Output {
    Command::new(CB)
        .args(args)
        .env(DataDir::env_var(), w.data.root())
        .env_remove(naming::ticket_env_var())
        .current_dir(w.home.path())
        .output()
        .unwrap()
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

/// The human error line on stderr, after a non-zero exit.
fn refused(out: &Output) -> String {
    assert!(!out.status.success());
    String::from_utf8(out.stderr.clone()).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn add_remove_and_refusals() {
    let w = TestService::start().await;
    w.shop_sample().await;

    assert_eq!(
        ok(&cb(&w, &["dep", "add", "SHOP-24", "SHOP-17"])),
        "SHOP-24 now depends on SHOP-17\n"
    );
    let json: Value = serde_json::from_str(&ok(&cb(
        &w,
        &["dep", "remove", "SHOP-24", "SHOP-17", "--json"],
    )))
    .unwrap();
    assert_eq!(json["depends_on"], serde_json::json!([]));
    assert_eq!(
        ok(&cb(&w, &["-P", "SHOP", "dep", "add", "24", "17"])),
        "SHOP-24 now depends on SHOP-17\n",
        "bare numbers resolve in the current project"
    );

    // A loop names its path (DEP-01).
    let error = refused(&cb(&w, &["dep", "add", "SHOP-20", "SHOP-21"]));
    assert!(error.contains("SHOP-21 → SHOP-20 → SHOP-21"), "{error}");

    // Linked tickets must share a base (DEP-02).
    w.board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Release fix".into(),
                base_branch: Some("release/2.3".into()),
                ..Default::default()
            },
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    let error = refused(&cb(&w, &["dep", "add", "SHOP-41", "SHOP-20"]));
    assert_eq!(
        error.trim(),
        "error: SHOP-41 merges into `release/2.3`, SHOP-20 into `main`. Linked tickets must use the same base branch."
    );
    let out = cb(&w, &["dep", "add", "SHOP-41", "SHOP-20", "--json"]);
    assert!(!out.status.success());
    let e: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(e["error"]["data"]["refusal"], "cross_base");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn tree_and_cycles() {
    let w = TestService::start().await;
    w.shop_sample().await;

    let tree = ok(&cb(&w, &["dep", "tree", "SHOP-20"]));
    assert_eq!(
        outline(&tree),
        [
            "0:SHOP-20",
            "0:Waits",
            "1:SHOP-18",
            "1:SHOP-19",
            "0:Waited",
            "1:SHOP-21"
        ],
        "{tree}"
    );
    assert!(
        tree.lines().next().unwrap().contains("Ready for Dev"),
        "{tree}"
    );
    assert!(
        tree.lines().next().unwrap().ends_with("Checkout page"),
        "{tree}"
    );
    assert!(
        tree.lines().nth(2).unwrap().contains("Ready for Review"),
        "{tree}"
    );
    let json: Value =
        serde_json::from_str(&ok(&cb(&w, &["dep", "tree", "SHOP-20", "--json"]))).unwrap();
    assert_eq!(json["key"], "SHOP-20");
    assert_eq!(json["parents"][0]["key"], "SHOP-18");
    assert_eq!(json["parents"][1]["key"], "SHOP-19");
    assert_eq!(json["children"][0]["key"], "SHOP-21");
    assert_eq!(
        ok(&cb(&w, &["dep", "tree", "SHOP-12"])),
        "No dependencies · nothing waits on SHOP-12\n"
    );

    assert_eq!(
        ok(&cb(&w, &["-P", "SHOP", "dep", "cycles"])),
        "No dependency loops in SHOP\n"
    );
    assert_eq!(
        ok(&cb(&w, &["-P", "SHOP", "dep", "cycles", "--json"])).trim(),
        "[]"
    );
    w.stop().await;
}

/// "depth:first word" of each line: two spaces per level.
fn outline(tree: &str) -> Vec<String> {
    tree.lines()
        .map(|l| {
            let depth = (l.len() - l.trim_start().len()) / 2;
            format!("{depth}:{}", l.split_whitespace().next().unwrap())
        })
        .collect()
}

/// #9: a diamond (SHOP-20 waits for SHOP-18 and SHOP-19, both made to wait for SHOP-17) prints each ticket once,
/// from either end: SHOP-17 under SHOP-18 only, SHOP-20 under SHOP-18 only.
#[tokio::test(flavor = "multi_thread")]
async fn a_diamond_prints_each_ticket_once() {
    let w = TestService::start().await;
    w.shop_sample().await;
    ok(&cb(&w, &["dep", "add", "SHOP-18", "SHOP-17"]));
    ok(&cb(&w, &["dep", "add", "SHOP-19", "SHOP-17"]));

    let up = ok(&cb(&w, &["dep", "tree", "SHOP-20"]));
    assert_eq!(
        outline(&up),
        [
            "0:SHOP-20",
            "0:Waits",
            "1:SHOP-18",
            "2:SHOP-17",
            "1:SHOP-19",
            "0:Waited",
            "1:SHOP-21"
        ],
        "{up}"
    );
    let down = ok(&cb(&w, &["dep", "tree", "SHOP-17"]));
    assert_eq!(
        outline(&down),
        [
            "0:SHOP-17",
            "0:Waited",
            "1:SHOP-18",
            "2:SHOP-20",
            "3:SHOP-21",
            "1:SHOP-19"
        ],
        "{down}"
    );
    for tree in [&up, &down] {
        let keys: Vec<&str> = tree
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .filter(|k| k.starts_with("SHOP-"))
            .collect();
        let once: std::collections::HashSet<&&str> = keys.iter().collect();
        assert_eq!(once.len(), keys.len(), "{tree}");
    }
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_links_only_from_its_own_ticket() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    let agent = w.agent_shell(&shop19).await;

    let own = agent.run(&format!("'{CB}' dep add SHOP-19 SHOP-17")).await;
    assert_eq!(
        (own.status, own.stdout.as_str()),
        (0, "SHOP-19 now depends on SHOP-17\n"),
        "{own:?}"
    );
    let other = agent.run(&format!("'{CB}' dep add SHOP-24 SHOP-17")).await;
    assert_ne!(other.status, 0);
    assert!(
        other
            .stderr
            .contains("SHOP-19's agent can't change SHOP-24"),
        "{other:?}"
    );
    assert!(
        w.board
            .ticket("SHOP-24")
            .await
            .unwrap()
            .depends_on
            .is_empty()
    );
    w.stop().await;
}

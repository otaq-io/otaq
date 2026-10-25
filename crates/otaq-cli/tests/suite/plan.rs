//! The Plan on the command line (T-0536, d0133): `create --plan` / `--plan-file`, `update --plan` / `--plan-file` /
//! `--plan-stdin`, `show` printing it after the description, and `show --piece N` finding one piece of its ticket.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use otaq_core::data_dir::DataDir;
use otaq_core::naming;
use otaq_core::service::testing::TestService;
use serde_json::Value;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

const PLAN: &str = "Why: the API lands first.\n\n## Pieces\n\n1. Wishlist API — waits for nothing\n\
                    2. Wishlist button — waits for 1\n3. Wishlist page — waits for 1\n   One list per shopper.\n\
                    4. Share link — waits for 2, 3";

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

#[tokio::test(flavor = "multi_thread")]
async fn create_and_update_through_each_flag() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let file = w.home.path().join("plan.md");
    std::fs::write(&file, format!("{PLAN}\n")).unwrap();

    let out = cb(
        &w,
        &[
            "-P",
            "SHOP",
            "create",
            "Wishlist v2",
            "-d",
            "Shoppers keep a list.",
            "--plan-file",
            file.to_str().unwrap(),
            "--json",
        ],
    );
    let created: Value = serde_json::from_str(&ok(&out)).unwrap();
    assert_eq!(created["key"], "SHOP-44");
    assert_eq!(created["body"], "Shoppers keep a list.");
    assert_eq!(created["plan"], PLAN);
    assert!(created["plan_by"].is_string() && created["plan_at"].is_i64());

    let out = cb(
        &w,
        &[
            "-P",
            "SHOP",
            "create",
            "Gift wrap",
            "--plan",
            "Wrap it.",
            "--json",
        ],
    );
    let created: Value = serde_json::from_str(&ok(&out)).unwrap();
    assert_eq!(created["plan"], "Wrap it.");

    // `--plan-stdin` reads the Plan; the description stays.
    let out = cb_in(&w, &["update", "SHOP-44", "--plan-stdin"], "1. One piece\n");
    assert_eq!(ok(&out), "Updated SHOP-44 · Wishlist v2\n");
    let t = w.board.ticket("SHOP-44").await.unwrap();
    assert_eq!(
        (t.body.as_str(), t.plan.as_str()),
        ("Shoppers keep a list.", "1. One piece")
    );
    // `--stdin` keeps its meaning: the description.
    ok(&cb_in(
        &w,
        &[
            "update",
            "SHOP-44",
            "--stdin",
            "--plan-file",
            file.to_str().unwrap(),
        ],
        "New body\n",
    ));
    let t = w.board.ticket("SHOP-44").await.unwrap();
    assert_eq!((t.body.as_str(), t.plan.as_str()), ("New body", PLAN));
    ok(&cb(&w, &["update", "SHOP-45", "--plan", "Wrap it twice."]));
    assert_eq!(
        w.board.ticket("SHOP-45").await.unwrap().plan,
        "Wrap it twice."
    );

    // Two sources of one Plan are a usage error.
    let both = cb_in(&w, &["update", "SHOP-44", "--stdin", "--plan-stdin"], "x");
    assert!(!both.status.success());
    let both = cb(
        &w,
        &["update", "SHOP-44", "--plan", "a", "--plan-file", "b"],
    );
    assert!(!both.status.success());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn show_prints_the_plan_and_one_piece() {
    let w = TestService::start().await;
    w.shop_sample().await;
    ok(&cb_in(&w, &["update", "SHOP-40", "--plan-stdin"], PLAN));

    let text = ok(&cb(&w, &["show", "SHOP-40"]));
    assert!(
        text.ends_with(&format!("Plan (written by you)\n{PLAN}\n")),
        "{text}"
    );
    let value: Value = serde_json::from_str(&ok(&cb(&w, &["show", "SHOP-40", "--json"]))).unwrap();
    assert_eq!(value["ticket"]["plan"], PLAN);
    assert!(value["ticket"]["plan_by"].is_string());
    assert!(value["ticket"]["plan_at"].is_i64());

    let piece = ok(&cb(&w, &["show", "SHOP-40", "--piece", "3"]));
    assert_eq!(
        piece,
        "40.3 · Wishlist page\nWaits for: 40.1\n\nOne list per shopper.\n"
    );
    let value: Value = serde_json::from_str(&ok(&cb(
        &w,
        &["show", "40", "-P", "SHOP", "--piece", "4", "--json"],
    )))
    .unwrap();
    assert_eq!(value["name"], "40.4");
    assert_eq!(value["title"], "Share link");
    assert_eq!(value["waits_for"], serde_json::json!([2, 3]));

    let missing = cb(&w, &["show", "SHOP-40", "--piece", "9", "--json"]);
    assert!(!missing.status.success());
    let error = serde_json::from_slice::<Value>(&missing.stdout).unwrap()["error"].clone();
    assert_eq!(error["kind"], "not_found");
    assert_eq!(error["message"], "SHOP-40's plan has no piece 9");
    w.stop().await;
}

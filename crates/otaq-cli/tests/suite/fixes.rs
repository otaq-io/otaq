//! T-0542 (d0129): `otaq delete` and a ticket's agent fixing the never-started tickets it created, from inside
//! SHOP-40's agent session: what `prime` lists, what passes, and the refusal texts.

use std::process::{Command, Output};

use otaq_core::board::tickets::EventKind;
use otaq_core::data_dir::DataDir;
use otaq_core::domain::Status;
use otaq_core::naming;
use otaq_core::service::testing::TestService;

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

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_fixes_and_deletes_what_it_filed() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let shop40 = w.board.ticket("SHOP-40").await.unwrap();
    let agent = w.agent_shell(&shop40).await;
    for title in ["Wishlist share link", "Wishlist share email"] {
        let ran = agent.run(&format!("'{CB}' create '{title}'")).await;
        assert_eq!(ran.status, 0, "{ran:?}");
    }

    let ran = agent.run(&format!("'{CB}' prime SHOP-40")).await;
    assert!(
        ran.stdout
            .contains("Tickets you created, not started: SHOP-44, SHOP-45."),
        "{ran:?}"
    );

    let ran = agent
        .run(&format!(
            "'{CB}' update SHOP-44 --title 'Share a wishlist' -p 2"
        ))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let ran = agent.run(&format!("'{CB}' dep add SHOP-44 SHOP-40")).await;
    assert_eq!(
        (ran.status, ran.stdout.as_str()),
        (0, "SHOP-44 now depends on SHOP-40\n"),
        "{ran:?}"
    );
    let ran = agent
        .run(&format!(
            "'{CB}' delete SHOP-45 --reason 'merged into SHOP-44'"
        ))
        .await;
    assert_eq!(
        (ran.status, ran.stdout.as_str()),
        (0, "Deleted SHOP-45 Wishlist share email\n"),
        "{ran:?}"
    );
    assert!(w.board.ticket("SHOP-45").await.is_err());
    let deleted = w
        .board
        .history("SHOP-40")
        .await
        .unwrap()
        .into_iter()
        .find(|e| e.kind == EventKind::FixedCreatedTicket)
        .unwrap();
    assert_eq!(deleted.payload["change"], "deleted");
    assert_eq!(deleted.payload["reason"], "merged into SHOP-44");

    // The refusal texts.
    let started = w.board.ticket("SHOP-44").await.unwrap();
    w.force_status(&started, Status::Working).await;
    for (command, expected) in [
        (
            "update SHOP-44 --title x",
            "SHOP-44 has started; SHOP-40's agent can only change tickets it created that haven't started",
        ),
        (
            "delete SHOP-43",
            "SHOP-43 was created by you; SHOP-40's agent can only change tickets it created",
        ),
        (
            "update SHOP-43 --title x",
            "SHOP-43 was created by you; SHOP-40's agent can only change tickets it created",
        ),
        (
            "dep add SHOP-43 SHOP-40",
            "SHOP-43 was created by you; SHOP-40's agent can only change tickets it created",
        ),
        (
            "delete SHOP-40",
            "SHOP-40's agent can't delete its own ticket; ask the user",
        ),
    ] {
        let ran = agent.run(&format!("'{CB}' {command}")).await;
        assert_ne!(ran.status, 0, "{command}");
        assert!(ran.stderr.contains(expected), "{command}: {ran:?}");
    }
    assert_eq!(
        w.board.ticket("SHOP-43").await.unwrap().title,
        "Wishlist page"
    );

    // The user deletes any never-started ticket.
    let out = cb(&w, &["delete", "SHOP-43"]);
    assert!(out.status.success(), "{out:?}");
    assert!(w.board.ticket("SHOP-43").await.is_err());
    w.stop().await;
}

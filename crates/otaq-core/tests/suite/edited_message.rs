//! The user's edit of a ticket's merge or PR message is kept on the ticket until it lands: after a hook refuses the
//! merge, a closed dialog and a restart, Approve opens on the edit; Use the proposed message (an empty edit) puts back
//! the proposal; a Send back keeps it; the merge that lands drops it. Against temporary repositories with real hooks,
//! never this repository's.

use std::path::{Path, PathBuf};

use otaq_core::board::tickets::Ticket;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::pr::message::{EditedMessage, MessageEdit};
use otaq_core::protocol::{KeepMessageParams, KeyParams, Reply, Request};
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::testing::{TestService, said};

fn me() -> Actor {
    Actor::User("me".into())
}

const EDIT: &str = "feat: add subtraction (SHOP-1)\n\nAdds subtract().";

/// A ticket in Ready for Review with one commit in its worktree.
async fn reviewed(w: &TestService) -> (Ticket, PathBuf) {
    let t = w.create("SHOP", "Add subtraction").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "calc.js", "subtract\n");
    w.force_status(&t, Status::Working).await;
    w.board
        .submit_for_review(
            "SHOP-1",
            said(&w.board, "SHOP-1", "Adds subtract()").await.as_ref(),
            &Actor::Agent(t.key.clone()),
        )
        .await
        .unwrap();
    (w.board.ticket("SHOP-1").await.unwrap(), wt)
}

fn hook(repo: &Path, name: &str, body: &str) {
    let path = repo.join(".git/hooks").join(name);
    std::fs::write(&path, body).unwrap();
    // Git for Windows runs hooks with its own sh; only macOS and Linux need the executable bit.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// `ticket.keep_message` over the socket, as the app sends it.
async fn keep(w: &TestService, edit: MessageEdit) {
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let reply = client
        .request(Request::TicketKeepMessage(KeepMessageParams {
            key: "SHOP-1".into(),
            edit,
        }))
        .await
        .unwrap();
    assert!(matches!(reply, Reply::Ok), "{reply:?}");
}

/// `ticket.merge_proposal`, as Approve & merge asks it when it opens.
async fn proposal(w: &TestService) -> otaq_core::board::merge::MergeProposal {
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client
        .request(Request::TicketMergeProposal(KeyParams {
            key: "SHOP-1".into(),
        }))
        .await
        .unwrap()
    {
        Reply::MergeProposal(p) => p,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn an_edited_merge_message_outlives_a_refusal_and_a_restart_until_it_lands() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (t, _wt) = reviewed(&w).await;
    let plain = proposal(&w).await;
    assert_eq!(plain.edited, None, "nothing edited yet");

    // The user edits the message; a commit-msg hook refuses the merge.
    keep(
        &w,
        MessageEdit::Merge {
            message: Some(EDIT.into()),
        },
    )
    .await;
    hook(
        &repo,
        "commit-msg",
        "#!/bin/sh\necho 'commit-msg: subject must start with fix:' >&2\nexit 1\n",
    );
    let main = git(&repo, &["rev-parse", "main"]);
    let refused = w.review.local_merge("SHOP-1", EDIT, false, &me()).await;
    assert!(
        matches!(refused, Ok(LocalMergeOutcome::HookRefused { .. })),
        "the hook refused it: {refused:?}"
    );
    let output = w.board.ticket("SHOP-1").await.unwrap().flags.hook_refusal;
    assert!(
        output.is_some_and(|r| r.output.contains("must start with fix:")),
        "its output is on the ticket"
    );
    assert_eq!(git(&repo, &["rev-parse", "main"]), main, "main unchanged");
    let again = proposal(&w).await;
    assert_eq!(again.edited.as_deref(), Some(EDIT), "reopened on the edit");
    assert_eq!(
        again.message, plain.message,
        "the proposal is still at hand"
    );

    // A restart keeps it.
    let w = w.restart().await;
    assert_eq!(proposal(&w).await.edited.as_deref(), Some(EDIT));

    // Use the proposed message: the proposal comes back, nothing is kept.
    keep(&w, MessageEdit::Merge { message: None }).await;
    assert_eq!(proposal(&w).await.edited, None);
    assert_eq!(
        w.board.edited_message(&t.id).await.unwrap(),
        EditedMessage::default()
    );

    // Edited again, the merge that lands takes it and drops it.
    keep(
        &w,
        MessageEdit::Merge {
            message: Some(EDIT.into()),
        },
    )
    .await;
    hook(&repo, "commit-msg", "#!/bin/sh\nexit 0\n");
    let merged = w
        .review
        .local_merge("SHOP-1", EDIT, false, &me())
        .await
        .unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Merged { .. }),
        "{merged:?}"
    );
    assert_eq!(git(&repo, &["log", "-1", "--format=%B", "main"]), EDIT);
    assert_eq!(
        w.board.edited_message(&t.id).await.unwrap(),
        EditedMessage::default(),
        "landed: the edit is gone"
    );
}

/// The PR dialog's part and the merge dialog's part are kept side by side; a Send back keeps both.
#[tokio::test]
async fn a_send_back_keeps_the_edit_and_each_dialog_keeps_its_own_part() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, _wt) = reviewed(&w).await;
    keep(
        &w,
        MessageEdit::Merge {
            message: Some(EDIT.into()),
        },
    )
    .await;
    keep(
        &w,
        MessageEdit::Pr {
            title: Some("feat: add subtraction".into()),
            description: None,
            commits: [("abc123".to_owned(), "feat: subtract".to_owned())].into(),
        },
    )
    .await;
    let both = w.board.edited_message(&t.id).await.unwrap();
    assert_eq!(both.merge.as_deref(), Some(EDIT));
    assert_eq!(both.title.as_deref(), Some("feat: add subtraction"));
    assert_eq!(both.commits["abc123"], "feat: subtract");

    w.review
        .fail("SHOP-1", "Handle floats too.", Vec::new(), false, &me())
        .await
        .unwrap();
    assert_ne!(
        w.board.ticket("SHOP-1").await.unwrap().status,
        Status::WaitingForYou,
        "sent back"
    );
    assert_eq!(
        w.board.edited_message(&t.id).await.unwrap(),
        both,
        "a Send back keeps it"
    );

    // The PR dialog's Use the proposed message leaves the merge dialog's edit as it is.
    keep(
        &w,
        MessageEdit::Pr {
            title: None,
            description: None,
            commits: Default::default(),
        },
    )
    .await;
    let merge_only = w.board.edited_message(&t.id).await.unwrap();
    assert_eq!(merge_only.merge.as_deref(), Some(EDIT));
    assert!(merge_only.title.is_none() && merge_only.commits.is_empty());
}

/// The PR opened with the edit: it landed, and the kept edit goes.
#[tokio::test(flavor = "multi_thread")]
async fn an_opened_pr_drops_the_kept_edit() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, _wt) = reviewed(&w).await;
    keep(
        &w,
        MessageEdit::Pr {
            title: Some("feat: add subtraction".into()),
            description: Some("Adds subtract().".into()),
            commits: Default::default(),
        },
    )
    .await;
    assert!(!w.board.edited_message(&t.id).await.unwrap().is_empty());
    let open = w
        .prs
        .opened(
            "SHOP-1",
            "https://github.com/acme/shop-web/pull/481",
            &Actor::Agent(t.key.clone()),
        )
        .await
        .unwrap();
    assert_eq!(open.status, Status::PrOpen);
    assert!(w.board.edited_message(&t.id).await.unwrap().is_empty());
    w.stop().await;
}

//! An answer session can be seen and reached (re-test 6, NEW-4): a merged ticket started again to answer another
//! ticket's question stops at a permission prompt. The answering ticket keeps its status (the card stays in Done and
//! doesn't float) while the prompt gets its own Needs you item naming the command; the asking ticket gets none.
//! `otaq attach SHOP-1` reaches the answer session while no agent session runs, focus in it holds back only the item's
//! alert, a key in its terminal answers the prompt, and Cleanup treats the ticket as in use (its session, its re-made
//! worktree) until the answer session ends.
//!
//! The answer session is the real resume path of a merged ticket (its worktree re-made, its conversation resumed
//! answer-only); an ordinary command stands in for `claude`, as in `answer_resume.rs`. The prompt is Claude Code's
//! `NeedsYou` event, as the hook maps it, and Enter answers it as on Claude Code (Codex's "y" alone at an answer
//! session's prompt is `quiet_turn_ends.rs`'s `y_at_a_codex_answer_sessions_prompt_answers_it`).

use std::path::PathBuf;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::claude::transcript::project_folder;
use otaq_core::agent_tools::{LaunchContext, Tool, ToolEvent};
use otaq_core::board::needs_you::NeedsYouGroup;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, PromptTurn, Status};
use otaq_core::git::testing::commit;
use otaq_core::messages::Route;
use otaq_core::protocol::{
    AttachParams, DeliverNowParams, InputParams, Reply, Request, SessionTarget, encode_bytes,
};
use otaq_core::recovery::cleanup::CleanupItem;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use tempfile::TempDir;

const CONVERSATION: &str = "0199aaaa-bbbb-7ccc-8ddd-eeeeffff0001";
const PROMPT: &str = "Run command: git show --stat HEAD";

fn agent(key: &str) -> Actor {
    Actor::Agent(key.parse().unwrap())
}

/// SHOP-1 merged and cleaned up (its conversation on disk), SHOP-2 depending on it and working.
async fn world() -> (TestService, TempDir, Ticket, Ticket, PathBuf) {
    let claude = TempDir::new().unwrap();
    let claude_dir = claude.path().to_path_buf();
    let w = TestService::start_with(move |mut c| {
        c.launch = Some(LaunchContext {
            bin_dir: PathBuf::from("/nonexistent"),
            plugin_dir: PathBuf::from("/nonexistent/plugin"),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "echo \"answer-session: $*\"; exec cat".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        });
        c.tools.claude_config_dir = Some(claude_dir);
        c
    })
    .await;
    w.add_project("shop-web").await;
    let parent = w.create("SHOP", "Retest6 restack").await;
    let child = w.create("SHOP", "Retest6 checker").await;
    w.board
        .add_dependency("SHOP-2", "SHOP-1", &Actor::User("me".into()))
        .await
        .unwrap();
    let worktree = w.start_worktree(&parent).await;
    commit(&worktree, "README.md", "retest\n");
    w.record_ended_agent(&parent, CONVERSATION).await;
    let folder = claude
        .path()
        .join("projects")
        .join(project_folder(&worktree));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(format!("{CONVERSATION}.jsonl")), "{}\n").unwrap();
    w.force_status(&parent, Status::Merged).await;
    let parent = w.board.ticket("SHOP-1").await.unwrap();
    w.review.finish(&parent).await.unwrap();
    assert!(!worktree.exists(), "Done cleanup removed it");
    w.force_status(&child, Status::Working).await;
    (w, claude, parent, child, worktree)
}

async fn client(w: &TestService) -> Client {
    Client::connect(ClientConfig::new(&w.data)).await.unwrap()
}

/// The ticket's Needs you items: (group, status, reason).
async fn items(w: &TestService, t: &Ticket) -> Vec<(NeedsYouGroup, Status, String)> {
    w.board
        .needs_you_items(0)
        .await
        .unwrap()
        .into_iter()
        .filter(|i| i.key == t.key)
        .map(|i| (i.group, i.status, i.reason))
        .collect()
}

/// Cleanup's items about `t`.
async fn listed(w: &TestService, t: &Ticket, worktree: &std::path::Path) -> Vec<CleanupItem> {
    let worktree = worktree.display().to_string();
    w.cleanup
        .scan()
        .await
        .unwrap()
        .into_iter()
        .filter(|i| i.ticket.as_ref() == Some(&t.key) || i.path.as_deref() == Some(&worktree))
        .collect()
}

/// What `otaq attach SHOP-1` attaches to: the key alone.
async fn attached_to(w: &TestService, key: &str) -> Result<String, String> {
    match client(w)
        .await
        .request(Request::SessionAttach(AttachParams {
            target: SessionTarget {
                key: Some(key.to_owned()),
                ..SessionTarget::default()
            },
            quiet: true,
        }))
        .await
    {
        Ok(Reply::SessionAttached(a)) => Ok(a.session.id),
        Ok(other) => panic!("{other:?}"),
        Err(e) => Err(e.to_string()),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_sessions_prompt_reads_needs_you_on_the_answering_ticket_and_can_be_reached() {
    let (w, _claude, parent, child, worktree) = world().await;
    let before = attached_to(&w, "SHOP-1").await;
    assert!(before.is_err(), "nothing runs yet: {before:?}");
    let asked = w
        .messages
        .ask(
            "SHOP-2",
            "SHOP-1",
            "Which line did you add to README.md?",
            &agent("SHOP-2"),
        )
        .await
        .unwrap();
    assert_eq!(asked.route, Some(Route::Resumed));
    let session = w
        .sessions
        .running_of(&parent.id, SessionKind::Answer)
        .expect("an answer session")
        .id;
    w.wait_for_output(&session, "answer-session:").await;

    // Running: "answering SHOP-2", reached by `otaq attach SHOP-1`, and in use for Cleanup.
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.flags.answering, Some(child.key.clone()));
    assert_eq!(t.session_prompt(), None);
    assert_eq!(
        attached_to(&w, "SHOP-1").await.as_deref(),
        Ok(session.as_str())
    );
    assert!(worktree.is_dir(), "re-made");
    assert_eq!(listed(&w, &parent, &worktree).await, [], "in use");
    // Notes and hand-offs go to the agent session only: Deliver now by key finds none.
    let deliver = client(&w)
        .await
        .request(Request::SessionDeliverNow(DeliverNowParams {
            target: SessionTarget {
                key: Some("SHOP-1".into()),
                ..SessionTarget::default()
            },
            clear_first: false,
        }))
        .await;
    assert!(deliver.is_err(), "{deliver:?}");

    // At a prompt: Needs you on the answering ticket only, which keeps its status and doesn't float.
    w.signals
        .apply_events(
            &Caller::Agent {
                ticket: parent.key.clone(),
                ticket_id: parent.id.clone(),
                session: session.clone(),
            },
            Tool::Claude,
            &[ToolEvent::NeedsYou {
                prompt: PROMPT.into(),
                helper_agent: false,
            }],
        )
        .await
        .unwrap();
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.status, Status::Merged, "it keeps its status");
    assert_eq!(t.place, parent.place, "and its column");
    let waiting = t.session_prompt().expect("the mark");
    assert_eq!(waiting.prompt, PROMPT);
    assert_eq!(waiting.session, session);
    assert_eq!(
        waiting.turn,
        PromptTurn::Answering {
            asking: child.key.clone()
        }
    );
    assert!(!t.needs_user() && !t.shows_needs_you(), "it doesn't float");
    assert_eq!(t.flags.prompts_drawn_in, std::slice::from_ref(&session));
    assert_eq!(
        items(&w, &parent).await,
        [(
            NeedsYouGroup::Family,
            Status::NeedsYou,
            format!("permission: {PROMPT}")
        )]
    );
    assert_eq!(items(&w, &child).await, [], "the asker gets none");
    assert_eq!(listed(&w, &parent, &worktree).await, [], "still in use");

    // Being in its terminal holds back only the alert: the item goes, the mark stays.
    w.sessions.set_focused(&session, 7, true).unwrap();
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert!(t.flags.your_turn && t.session_prompt().is_some());
    assert_eq!(items(&w, &parent).await, []);
    w.sessions.set_focused(&session, 7, false).unwrap();
    assert_eq!(items(&w, &parent).await.len(), 1);

    // A key in its terminal answers it.
    client(&w)
        .await
        .request(Request::SessionInput(InputParams {
            session: session.clone(),
            data: encode_bytes(b"\r"),
            reply: false,
            if_empty: false,
        }))
        .await
        .unwrap();
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.session_prompt(), None);
    assert!(t.flags.prompts_drawn_in.is_empty());
    assert_eq!(items(&w, &parent).await, []);

    // The answer reaches the asker and the answer session ends: nothing answers for SHOP-2 any more.
    w.messages
        .answer(&asked.id, "The retest line.", &agent("SHOP-1"))
        .await
        .unwrap();
    assert!(
        w.sessions
            .running_of(&parent.id, SessionKind::Answer)
            .is_none()
    );
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (t.flags.answering.as_ref(), t.session_prompt()),
        (None, None)
    );
    w.stop().await;
}

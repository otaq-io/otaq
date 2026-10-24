//! A stuck child goes to its epic's lead once (T-0285; LEAD-07, LEAD-04; d0083, d0063, d0064, d0042, d0018). The
//! child's agent, its validator and the lead are ordinary `sh` sessions; the lead's note is sent over the socket from
//! inside the lead's session, so it is judged as the lead. Hooks are fed as the service gets them, and Auto-merge runs
//! against temporary repositories. No test starts `claude`.

use otaq_core::service::testing::said;
use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::needs_you::NeedsYouItem;
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::caller::Caller;
use otaq_core::domain::{Actor, AutoMergeStop, LeadNoteSent, Status, StuckStop, ValidationTrigger};
use otaq_core::epic_lead::notes::{self, ReadHow};
use otaq_core::epic_lead::stuck::{self, FirstStop, StuckState};
use otaq_core::git::testing::{TempRepos, commit, git};
use otaq_core::naming;
use otaq_core::project_agents::lead::{Lead, LeadStatus};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, path_without_build_folders};
use otaq_core::session::SessionKind;
use otaq_core::session::deliveries::handoff_sentence;
use otaq_core::store::now_ms;
use serde_json::{Value, json};

const REPORT: &str = "orders.test.ts expects `list`\n- it gets `items` since the API change";
const NOTE: &str = "The API returns `items[]`; fix the test, keep the API.";
const VIEW: &str = "The agent keeps changing the API instead of the test.";

/// The sample board (SHOP-40 Wishlist: 40.1 Merged, 40.2 Working, 40.3 Backlog); `claude` (the lead, the validator)
/// is an interactive `sh`. Validation runs one round, so the first failure is the last.
async fn world() -> TestService {
    let home = tempfile::TempDir::new().unwrap();
    let data = home.path().join("h");
    let w = TestService::start_in(home, TempRepos::new(), |config| ServiceConfig {
        launch: Some(launch(&data)),
        ..config
    })
    .await;
    w.shop_sample().await;
    settings(&w, false).await;
    w
}

fn launch(data: &Path) -> LaunchContext {
    let script = format!(
        "export {}='{}'; export PS1=''; exec sh",
        otaq_core::data_dir::DataDir::env_var(),
        data.display()
    );
    LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: path_without_build_folders().into(),
        claude: ClaudeLaunch {
            program: vec!["sh".into(), "-c".into(), script, "sh".into()],
            config_dir: None,
        },
    }
}

/// SHOP's flow: Validate with 1 round, and with `auto_merge` an automatic local Delivery.
async fn settings(w: &TestService, auto_merge: bool) {
    use otaq_core::flow::default::testing::{automatic_local_delivery, with_rounds};
    w.board.edit_flow_for_tests("SHOP", with_rounds(1)).await;
    if auto_merge {
        w.board
            .edit_flow_for_tests("SHOP", automatic_local_delivery)
            .await;
    }
}

fn me() -> Actor {
    Actor::User("me".into())
}

async fn ticket(w: &TestService, key: &str) -> Ticket {
    w.board.ticket(key).await.unwrap()
}

/// SHOP-40.2 with a committed worktree and its agent's `sh` session.
async fn button(w: &TestService) -> (Ticket, AgentShell) {
    let t = ticket(w, "SHOP-40.2").await;
    w.give_worktree(&t).await;
    let shell = w.agent_shell(&t).await;
    (ticket(w, "SHOP-40.2").await, shell)
}

fn agent_caller(t: &Ticket, shell: &AgentShell) -> Caller {
    Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: shell.session.clone(),
    }
}

/// The agent submits and the round fails (the last round: "Validation failed 1 time").
async fn submit_and_fail(w: &TestService, key: &str) -> Ticket {
    let t = ticket(w, key).await;
    w.validation
        .submit(
            key,
            said(&w.board, key, "done").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    let validator = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .unwrap()
        .id;
    w.validation
        .verdict(
            key,
            false,
            said(&w.board, key, REPORT).await.as_ref(),
            &validator,
        )
        .await
        .unwrap()
}

async fn open_lead(w: &TestService, epic: &str) -> (Lead, AgentShell) {
    let lead = w.project_agents.leads().open(epic).await.unwrap();
    let shell = w.shell_in(&session(&lead));
    (lead, shell)
}

fn session(lead: &Lead) -> String {
    lead.agent.session.as_ref().expect("a session").id.clone()
}

fn lead_caller(lead: &Lead) -> Caller {
    Caller::EpicLead {
        project: lead.agent.project.clone(),
        agent: lead.agent.id.clone(),
        epic: lead.epic.clone(),
        epic_id: lead.epic_id.clone(),
        session: session(lead),
    }
}

async fn lead_hook(w: &TestService, lead: &Lead, event: &str, payload: Value) {
    let signals = w
        .signals
        .hook(&lead_caller(lead), Tool::Claude, event, &payload)
        .await
        .unwrap();
    for signal in &signals {
        w.project_agents.hook(&lead.agent.id, signal).await.unwrap();
        if signal.ends_turn() {
            w.deliveries.deliver_if_ready(&session(lead)).await.unwrap();
        }
    }
}

async fn lead_prompt(w: &TestService, lead: &Lead, text: &str) {
    lead_hook(w, lead, "UserPromptSubmit", json!({ "prompt": text })).await;
}

async fn lead_stop(w: &TestService, lead: &Lead) {
    lead_hook(
        w,
        lead,
        "Stop",
        json!({ "last_assistant_message": "Done." }),
    )
    .await;
}

async fn pending(w: &TestService, session: &str) -> Vec<String> {
    w.deliveries
        .pending(session)
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.text)
        .collect()
}

/// The lead is busy with a turn the user typed, so its wake-ups wait.
async fn busy_lead(w: &TestService) -> (Lead, AgentShell) {
    let (lead, shell) = open_lead(w, "SHOP-40").await;
    lead_prompt(w, &lead, "Plan it").await;
    (lead, shell)
}

/// The lead's turn that its one wake-up starts.
async fn lead_takes_the_wake_up(w: &TestService, lead: &Lead) -> String {
    let waiting = pending(w, &session(lead)).await;
    assert_eq!(waiting.len(), 1, "{waiting:?}");
    lead_stop(w, lead).await;
    assert!(pending(w, &session(lead)).await.is_empty(), "typed");
    lead_prompt(w, lead, &waiting[0]).await;
    waiting[0].clone()
}

/// `otaq note <KEY> [--view …] --stdin` from inside the lead's session.
async fn note(w: &TestService, lead: &AgentShell, key: &str, view: Option<&str>) -> Value {
    let mut params = json!({"key": key, "text": NOTE});
    if let Some(view) = view {
        params["view"] = json!(view);
    }
    let request = json!({"protocol_version": 1, "id": 1, "request": {"method": "lead.note", "params": params}});
    let ran = lead
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    let answer: Value = serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"));
    assert!(answer.get("error").is_none(), "{answer}");
    answer["reply"].clone()
}

async fn item(w: &TestService, key: &str) -> Option<NeedsYouItem> {
    w.board
        .needs_you_items(now_ms())
        .await
        .unwrap()
        .into_iter()
        .find(|i| i.key.to_string() == key)
}

async fn steps(w: &TestService, key: &str) -> Vec<String> {
    let mut steps: Vec<String> = w
        .board
        .history(key)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EventKind::LeadStuck)
        .map(|e| e.payload["step"].as_str().unwrap_or_default().to_owned())
        .collect();
    steps.reverse();
    steps
}

async fn state(w: &TestService, key: &str) -> Option<StuckState> {
    let t = ticket(w, key).await;
    stuck::find(&w.board, &t.id).await.unwrap().map(|h| h.state)
}

async fn wait_until<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    tokio::time::timeout(Duration::from_secs(10), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting: {what}"));
}

/// Validation failed: the first stop wakes the lead with no inbox item; its note restarts the counter and goes to the
/// agent with the findings; the second stop goes to the inbox with the note and the view, and the lead isn't woken.
#[tokio::test(flavor = "multi_thread")]
async fn validation_failed_goes_to_the_lead_once_and_its_note_counts_as_answer_the_agent() {
    let w = world().await;
    let (lead, lead_shell) = busy_lead(&w).await;
    let (_, agent) = button(&w).await;

    let held = submit_and_fail(&w, "SHOP-40.2").await;
    assert_eq!(held.status, Status::Working, "held at work, not turquoise");
    assert_eq!(held.validation_round, 1, "the round counted");
    assert!(item(&w, "SHOP-40.2").await.is_none(), "no inbox item");
    assert_eq!(state(&w, "SHOP-40.2").await, Some(StuckState::Held));
    let wake = lead_takes_the_wake_up(&w, &lead).await;
    assert_eq!(
        wake,
        stuck::wake_text(&held.key, &FirstStop::validation_failed(REPORT, 1))
    );
    assert!(wake.starts_with(
        "SHOP-40.2 stopped: Validation failed 1 time. Last report: orders.test.ts expects `list` \u{2026}. Send it \
         one note with"
    ));
    let leads = w.project_agents.leads();
    assert_eq!(
        leads.status("SHOP-40").await.unwrap(),
        LeadStatus::Replanning
    );
    assert_eq!(leads.handovers_of("SHOP-40").await.unwrap().len(), 1);

    let reply = note(&w, &lead_shell, "SHOP-40.2", Some(VIEW)).await;
    assert_eq!(reply["lead_note"]["read_how"], "with_stop");
    assert_eq!(reply["lead_note"]["view"], VIEW);
    assert!(
        leads.handovers_of("SHOP-40").await.unwrap().is_empty(),
        "settled"
    );
    let after = ticket(&w, "SHOP-40.2").await;
    assert_eq!(after.status, Status::Working);
    assert_eq!(after.validation_round, 0, "the counter restarts (VAL-06)");
    assert_eq!(after.flags.unread_notes, None, "it went with the stop");
    let sent = after
        .flags
        .lead_note_sent
        .clone()
        .expect("Lead's note sent");
    assert_eq!(
        (
            sent.lead.as_str(),
            sent.text.as_str(),
            sent.view.as_deref(),
            sent.stop,
            sent.after_rounds
        ),
        (
            "Wishlist lead",
            NOTE,
            Some(VIEW),
            StuckStop::ValidationFailed,
            Some(1)
        )
    );
    let delivered = pending(&w, &agent.session).await;
    assert_eq!(
        delivered.last().unwrap(),
        &format!(
            "Note from Wishlist lead: {NOTE}\n\nThe validation's findings for SHOP-40.2. Fix these, commit, and \
             submit again:\n\n{}",
            REPORT
        )
    );
    // The turn ends: nothing is handed back.
    lead_stop(&w, &lead).await;
    assert_eq!(w.status("SHOP-40.2").await, Status::Working);
    assert_eq!(state(&w, "SHOP-40.2").await, Some(StuckState::Noted));

    // The second stop: the inbox, with the lead's note and view; the lead isn't woken.
    let second = submit_and_fail(&w, "SHOP-40.2").await;
    assert_eq!(second.status, Status::CheckFailed);
    let inbox = item(&w, "SHOP-40.2").await.expect("an inbox item");
    assert_eq!(inbox.lead_note, Some(sent));
    assert!(
        pending(&w, &session(&lead)).await.is_empty(),
        "not woken again"
    );
    for key in ["SHOP-40.2", "SHOP-40"] {
        assert_eq!(steps(&w, key).await, ["held", "noted"], "{key}");
    }
    w.stop().await;
}

/// Hand-back: the lead's turn ends without a note to the child; the held stop goes to the inbox as today, with no
/// lead's note. A later note to the child is an ordinary unread note.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_without_a_note_hands_the_stop_back() {
    let w = world().await;
    let (lead, lead_shell) = busy_lead(&w).await;
    button(&w).await;
    submit_and_fail(&w, "SHOP-40.2").await;
    lead_takes_the_wake_up(&w, &lead).await;
    lead_stop(&w, &lead).await;

    let back = ticket(&w, "SHOP-40.2").await;
    assert_eq!(back.status, Status::CheckFailed);
    let inbox = item(&w, "SHOP-40.2").await.expect("an inbox item");
    assert_eq!(inbox.lead_note, None);
    assert_eq!(back.flags.lead_note_sent, None);
    assert_eq!(state(&w, "SHOP-40.2").await, Some(StuckState::HandedBack));
    assert_eq!(steps(&w, "SHOP-40").await, ["held", "handed_back"]);
    assert!(
        w.project_agents
            .leads()
            .handovers_of("SHOP-40")
            .await
            .unwrap()
            .is_empty()
    );

    // The attempt is used: the next failure goes straight to the inbox.
    w.validation
        .user_action(
            "SHOP-40.2",
            otaq_core::domain::ValidationAction::AnotherRound,
            "",
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(
        submit_and_fail(&w, "SHOP-40.2").await.status,
        Status::CheckFailed
    );
    assert!(pending(&w, &session(&lead)).await.is_empty());
    let _ = lead_shell;
    w.stop().await;
}

/// No live lead: the stop goes to the inbox at once and the attempt isn't used.
#[tokio::test(flavor = "multi_thread")]
async fn without_a_live_lead_the_stop_goes_to_the_inbox_at_once() {
    let w = world().await;
    button(&w).await;
    let failed = submit_and_fail(&w, "SHOP-40.2").await;
    assert_eq!(failed.status, Status::CheckFailed);
    assert!(item(&w, "SHOP-40.2").await.is_some());
    assert_eq!(state(&w, "SHOP-40.2").await, None, "the attempt isn't used");
    w.stop().await;
}

/// Idle: the first Stop that would make the child Idle wakes the lead instead and the child stays at work; the note
/// is typed in. The next Idle is an ordinary Idle stop with the lead's note on its item.
#[tokio::test(flavor = "multi_thread")]
async fn idle_goes_to_the_lead_once_and_its_note_is_typed_in() {
    let w = world().await;
    let (lead, lead_shell) = busy_lead(&w).await;
    let (t, agent) = button(&w).await;
    let stop = async |line: &str| {
        w.signals
            .hook(
                &agent_caller(&t, &agent),
                Tool::Claude,
                "Stop",
                &json!({ "last_assistant_message": line }),
            )
            .await
            .unwrap();
    };

    stop("Should I also update the tests?").await;
    assert_eq!(w.status("SHOP-40.2").await, Status::Working, "held at work");
    assert!(item(&w, "SHOP-40.2").await.is_none());
    let wake = lead_takes_the_wake_up(&w, &lead).await;
    assert!(
        wake.starts_with(
            "SHOP-40.2 stopped: Idle. Its last message: Should I also update the tests?. Send it"
        ),
        "{wake}"
    );

    note(&w, &lead_shell, "SHOP-40.2", None).await;
    w.wait_for_output(&agent.session, &format!("Note from Wishlist lead: {NOTE}"))
        .await;
    let after = ticket(&w, "SHOP-40.2").await;
    assert_eq!(after.status, Status::Working);
    let sent = after.flags.lead_note_sent.expect("Lead's note sent");
    assert_eq!(
        (sent.stop, sent.view, sent.after_rounds),
        (StuckStop::Idle, None, None)
    );
    let read = notes::list_for_epic(&w.board, "SHOP-40").await.unwrap();
    assert_eq!(read[0].read_how, Some(ReadHow::WithStop));

    // Its next Idle goes to the inbox, with the note; the lead isn't woken.
    w.signals
        .hook(
            &agent_caller(&t, &agent),
            Tool::Claude,
            "UserPromptSubmit",
            &json!({"prompt": NOTE}),
        )
        .await
        .unwrap();
    stop("Done; anything else?").await;
    assert_eq!(w.status("SHOP-40.2").await, Status::Idle);
    let inbox = item(&w, "SHOP-40.2").await.expect("an Idle item");
    assert_eq!(inbox.lead_note.map(|n| n.text), Some(NOTE.to_owned()));
    assert!(pending(&w, &session(&lead)).await.is_empty());
    w.stop().await;
}

/// The Idle stop handed back shows Idle with the agent's last line.
#[tokio::test(flavor = "multi_thread")]
async fn an_idle_stop_handed_back_is_idle() {
    let w = world().await;
    let (lead, _lead_shell) = busy_lead(&w).await;
    let (t, agent) = button(&w).await;
    w.signals
        .hook(
            &agent_caller(&t, &agent),
            Tool::Claude,
            "Stop",
            &json!({ "last_assistant_message": "Should I also update the tests?" }),
        )
        .await
        .unwrap();
    lead_takes_the_wake_up(&w, &lead).await;
    lead_stop(&w, &lead).await;
    let back = ticket(&w, "SHOP-40.2").await;
    assert_eq!(back.status, Status::Idle);
    assert_eq!(
        back.signal.and_then(|s| s.text).as_deref(),
        Some("Should I also update the tests?")
    );
    assert!(item(&w, "SHOP-40.2").await.is_some());
    w.stop().await;
}

/// A restart ends the lead: the held stop goes to the user before the reconcile.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_hands_the_held_stop_back() {
    let w = world().await;
    let (_lead, _shell) = busy_lead(&w).await;
    button(&w).await;
    assert_eq!(
        submit_and_fail(&w, "SHOP-40.2").await.status,
        Status::Working
    );
    let w = w.restart().await;
    assert_eq!(w.status("SHOP-40.2").await, Status::CheckFailed);
    assert_eq!(state(&w, "SHOP-40.2").await, Some(StuckState::HandedBack));
    assert!(item(&w, "SHOP-40.2").await.is_some());
    w.stop().await;
}

/// The child's epic branch gets a conflicting commit on `file`.
fn conflict_on_epic_branch(repo: &Path, file: &str) {
    let branch = naming::branch("SHOP-40");
    git(repo, &["checkout", "-q", &branch]);
    commit(repo, file, "the epic's own line\n");
    git(repo, &["checkout", "-q", "main"]);
}

/// Passes validation from the running validator; Auto-merge runs.
async fn pass(w: &TestService, key: &str) -> Ticket {
    let t = ticket(w, key).await;
    w.validation
        .submit(
            key,
            said(&w.board, key, "done").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    let validator = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .unwrap()
        .id;
    w.validation
        .verdict(
            key,
            true,
            said(&w.board, key, "ok").await.as_ref(),
            &validator,
        )
        .await
        .unwrap();
    ticket(w, key).await
}

/// Keeps conflicting: the third conflict wakes the lead with no inbox item; the note restarts the conflict counter,
/// the merge is retried, and the conflict goes back to the agent with the note before restack.
#[tokio::test(flavor = "multi_thread")]
async fn keeps_conflicting_goes_to_the_lead_once_and_its_note_retries_the_merge() {
    let w = world().await;
    settings(&w, true).await;
    let project = w.board.project("SHOP").await.unwrap();
    let repo = PathBuf::from(&project.repo_path);
    let (lead, lead_shell) = busy_lead(&w).await;
    let t = ticket(&w, "SHOP-40.2").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "wishlist.ts", "the child's line\n");
    let agent = w.agent_shell(&t).await;
    conflict_on_epic_branch(&repo, "wishlist.ts");

    for n in 1..=2 {
        let after = pass(&w, "SHOP-40.2").await;
        assert_eq!(after.status, Status::MergeConflictFixing, "conflict {n}");
    }
    let held = pass(&w, "SHOP-40.2").await;
    assert_eq!(held.status, Status::Working, "held at work, not turquoise");
    assert_eq!(held.auto_merge_conflicts, 3);
    assert!(item(&w, "SHOP-40.2").await.is_none());
    let wake = lead_takes_the_wake_up(&w, &lead).await;
    assert!(
        wake.starts_with(&format!(
            "SHOP-40.2 stopped: Keeps conflicting with `{}`. Send it one note",
            naming::branch("SHOP-40")
        )),
        "{wake}"
    );

    let before = pending(&w, &agent.session).await.len();
    note(&w, &lead_shell, "SHOP-40.2", Some(VIEW)).await;
    let after = ticket(&w, "SHOP-40.2").await;
    assert_eq!(
        after.status,
        Status::MergeConflictFixing,
        "retried: back to the agent"
    );
    assert_eq!(after.auto_merge_conflicts, 1, "the counter restarted");
    let delivered = pending(&w, &agent.session).await;
    assert_eq!(
        delivered[before..],
        [
            format!("Note from Wishlist lead: {NOTE}"),
            handoff_sentence(w.adapters.of_session(None).skill_root(), "restack", None)
        ]
    );
    assert_eq!(
        after.flags.lead_note_sent.map(|n| n.stop),
        Some(StuckStop::KeepsConflicting)
    );

    // Two more conflicts and the third stops for the user, with the note.
    for _ in 0..2 {
        pass(&w, "SHOP-40.2").await;
    }
    let stopped = ticket(&w, "SHOP-40.2").await;
    assert_eq!(stopped.status, Status::DeliveryStopped);
    assert_eq!(
        stopped.auto_merge_stop,
        Some(AutoMergeStop::KeepsConflicting)
    );
    let inbox = item(&w, "SHOP-40.2").await.expect("an inbox item");
    assert_eq!(inbox.lead_note.and_then(|n| n.view), Some(VIEW.to_owned()));
    lead_stop(&w, &lead).await;
    assert!(pending(&w, &session(&lead)).await.is_empty());
    w.stop().await;
}

/// A held Keeps conflicting handed back stops as today.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_conflict_handed_back_stops() {
    let w = world().await;
    settings(&w, true).await;
    let project = w.board.project("SHOP").await.unwrap();
    let repo = PathBuf::from(&project.repo_path);
    let (lead, _shell) = busy_lead(&w).await;
    let t = ticket(&w, "SHOP-40.2").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "wishlist.ts", "the child's line\n");
    w.agent_shell(&t).await;
    conflict_on_epic_branch(&repo, "wishlist.ts");
    for _ in 0..3 {
        pass(&w, "SHOP-40.2").await;
    }
    assert_eq!(w.status("SHOP-40.2").await, Status::Working);
    lead_takes_the_wake_up(&w, &lead).await;
    lead_stop(&w, &lead).await;
    wait_until("handed back", async || {
        w.status("SHOP-40.2").await == Status::DeliveryStopped
    })
    .await;
    let stopped = ticket(&w, "SHOP-40.2").await;
    assert_eq!(stopped.auto_merge_conflicts, 3);
    assert_eq!(
        stopped.signal.and_then(|s| s.text).as_deref(),
        Some(format!("Keeps conflicting with `{}`", naming::branch("SHOP-40")).as_str())
    );
    w.stop().await;
}

/// The flag's wire form.
#[test]
fn the_flag_round_trips() {
    let sent = LeadNoteSent {
        lead: "Wishlist lead".into(),
        text: NOTE.into(),
        view: None,
        stop: StuckStop::Idle,
        after_rounds: None,
        at: 1,
    };
    let json = serde_json::to_value(&sent).unwrap();
    assert_eq!(
        json,
        json!({"lead": "Wishlist lead", "text": NOTE, "stop": "idle", "at": 1})
    );
}

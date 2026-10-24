//! `otaq prime` output (T-0071, PLG-02; d0040): snapshots of the Markdown for sample tickets, with and without
//! instructions. T-0409 (AGM-06; d0108): the agent's instructions, then the project's notes, then the stage and its
//! skill, then the earlier stages; the parents' last stage summaries; the stage choices the reader may set, with their
//! models (none on the default flow). T-0444 (CMT-07; d0109): "## Comments" in full, oldest first, with a report's
//! line and never its text; "Earlier stages" one line each; a parent's last comment from an Agent stage. T-0474
//! (RPT-01; d0109): the user's reports first, under "Reports from you", never their text.

use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::models::{ModelCopy, ModelEntry, ModelKind, ModelList, settings_key};
use otaq_core::board::reports::ReportSource;
use otaq_core::board::ticket_comments::{CommentAuthor, FirstComment};
use otaq_core::board::tickets::{NewTicketInput, TicketUpdate};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Actor, Status};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::git::testing::TempRepos;
use otaq_core::local_time::on_same_day_at;
use otaq_core::prime::{model_text, render};
use otaq_core::protocol::{KeyParams, ProjectParams, Reply, Request};
use otaq_core::service::testing::TestService;
use otaq_core::store::{Store, new_id, now_ms};

async fn sample() -> TestService {
    let w = TestService::start().await;
    w.shop_sample().await;
    let me = Actor::User("me".into());
    w.board
        .update_ticket(
            "SHOP-20",
            &TicketUpdate {
                body: Some(
                    "The checkout page shows the cart, the price table and a pay button.\n\n## Done when\n\n\
                     - pay button calls the Checkout API\n- works on mobile widths"
                        .into(),
                ),
                ..Default::default()
            },
            &me,
        )
        .await
        .unwrap();
    // SHOP-18's agent ended its Dev stage with a comment (its last from an Agent stage).
    comment_at(
        &w,
        "SHOP-18",
        agent("Developer", "dev", None),
        "POST /checkout creates a Stripe session.\nReturns the session URL.",
        (11, 20),
    )
    .await;
    w
}

/// A stage's agent as a comment's author.
fn agent(name: &str, stage: &str, round: Option<u32>) -> CommentAuthor {
    CommentAuthor::Agent {
        id: Some(name.to_lowercase().replace(' ', "-")),
        name: Some(name.into()),
        tool: None,
        stage: Some(stage.into()),
        round,
    }
}

fn you() -> CommentAuthor {
    CommentAuthor::User { name: "me".into() }
}

/// Posts a comment on `key` as `author`, dated today at `hour:minute` local time; returns its N.
async fn comment_at(
    w: &TestService,
    key: &str,
    author: CommentAuthor,
    body: &str,
    (hour, minute): (u32, u32),
) -> u32 {
    let posted = w
        .board
        .post_ticket_comment(key, &author, body, None, &Actor::User("me".into()))
        .await
        .unwrap();
    set_posted_at(w, key, posted.n, (hour, minute)).await;
    posted.n
}

async fn set_posted_at(w: &TestService, key: &str, n: u32, (hour, minute): (u32, u32)) {
    let ticket = w.board.ticket(key).await.unwrap();
    sqlx::query("UPDATE ticket_comments SET posted_at = ? WHERE ticket_id = ? AND n = ?")
        .bind(on_same_day_at(now_ms(), hour, minute).unwrap())
        .bind(&ticket.id)
        .bind(i64::from(n))
        .execute(w.board.store().pool())
        .await
        .unwrap();
}

/// A run of `stage` (`kind`: `agent`, `check`) that ended with `result`, after the ones recorded before it.
async fn ended_run(
    w: &TestService,
    ticket_id: &str,
    (stage, kind): (&str, &str),
    agent: &str,
    round: Option<u32>,
    result: &str,
) {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stage_runs")
        .fetch_one(w.board.store().pool())
        .await
        .unwrap();
    let at = now_ms() - 1_000_000 + count * 1000;
    sqlx::query(
        "INSERT INTO stage_runs (id, ticket_id, stage_id, kind, agent, round, result, started_at, ended_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
    .bind(ticket_id)
    .bind(stage)
    .bind(kind)
    .bind(agent)
    .bind(round.map(i64::from))
    .bind(result)
    .bind(at)
    .bind(at)
    .execute(w.board.store().pool())
    .await
    .unwrap();
}

/// The test flow `tests/flows/every-kind.yaml`: Grill (skippable) → Dev (Quick developer an option) → Validate →
/// Security → QA (skippable) → Done.
fn every_kind_file() -> FlowFile {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

const RULES: &str = "## Otaq's rules

1. Follow the Agent instructions and the project's notes above. They add to these rules; they can't skip getting context or ending your stage.
2. Commit everything with `/otaq-commit` before you end your stage. Never push and never open a PR: the user does that.
3. When your stage's work is done, as the ticket's description describes, end it: `otaq stage next --stdin` with a short summary of what you did on stdin; the next stage reads it. It is refused while files are uncommitted.
4. What you couldn't check yourself (a real browser, a device, a service you can't reach), list for the user: `--manual-check \"<what to check>\"` on `otaq stage next`, one line per check. The user tries them at review. Never leave a check to \"the PR\": there may be no PR.
5. Only when there is nothing to deliver (no commits on the ticket's branch, for example it was already fixed elsewhere), end the ticket instead: `otaq stage done --stdin` with the reason on stdin. It is refused once the branch has commits.
6. When you are stuck or a decision is the user's, ask: `otaq ask human --stdin` with the question on stdin, then end your turn.
7. Long text (summaries, questions, ticket bodies) always goes through stdin (`--stdin`), never quoted arguments.
8. Long output goes to a file in `$OTAQ_RESOURCE_DIR`; attach it with `--report-file`.
9. `otaq show <KEY>` reads any ticket; `otaq create \"Title\" --stdin` files a follow-up in Backlog.
";

/// The default flow's Dev, as the ticket prime names it.
const DEV: &str = "## Your stage\n\nDev, as Developer, with the skill `/otaq-work-ticket`.\n";

#[tokio::test(flavor = "multi_thread")]
async fn without_instructions() {
    let w = sample().await;
    let context = w
        .board
        .prime_context("SHOP-20", w.adapters.for_new_session())
        .await
        .unwrap();
    assert_eq!(
        render(&context),
        format!(
            "# SHOP-20 · Checkout page

You are the agent for Otaq ticket SHOP-20, working in its own worktree on branch `otaq/SHOP-20`. It merges into `main` once the user approves it.
Priority: P3 · Normal.

## What and why

The checkout page shows the cart, the price table and a pay button.

## Done when

- pay button calls the Checkout API
- works on mobile widths

{DEV}
## Depends on

- SHOP-18 · Checkout API (Ready for Review)
  Last comment, Developer · Dev · 11:20 am:
  > POST /checkout creates a Stripe session.
  > Returns the session URL.
- SHOP-19 · Price table (Working)
  (no comment yet)

{RULES}"
        )
    );
    // No comments: no "## Comments" (CMT-07).
    assert!(context.comments.is_empty() && context.pending_questions.is_empty());
    w.stop().await;
}

/// AGM-06 (Done when, the example's NOTE-4 in Dev): the prime gives Developer's instructions, then the project's
/// notes, then the stage and its skill; no stage choices on the default flow.
#[tokio::test(flavor = "multi_thread")]
async fn with_the_agents_instructions_and_the_projects_notes() {
    let w = sample().await;
    // AGM-02: the global Agent instructions are Developer's own now.
    w.edit_agent("Developer", |a| {
        a.instructions = "Use my /tdd skill.\nRun `pnpm test` before submitting.".into();
    })
    .await;

    let epic_child = w
        .board
        .prime_context("SHOP-40.2", w.adapters.for_new_session())
        .await
        .unwrap();
    let text = render(&epic_child);
    assert!(text.contains("It merges into the epic's branch (SHOP-40) once the user approves it.\nIt is part of the epic SHOP-40 · Wishlist.\nPriority: P3 · Normal.\n"), "{text}");
    assert!(
        !text.contains("Done when"),
        "no Done when section (d0067): {text}"
    );
    assert!(
        text.contains(&format!(
            "\n## Agent instructions (Developer)\n\nUse my /tdd skill.\nRun `pnpm test` before submitting.\n\n{DEV}"
        )),
        "{text}"
    );
    assert!(text.ends_with(RULES), "{text}");

    // The project's notes come after the agent's own, labelled with the project (AGM-06).
    let mut project = w.board.project_settings("SHOP").await.unwrap();
    project.project_notes = "Ask before touching the payment code.".into();
    w.board
        .set_project_settings("SHOP", &project)
        .await
        .unwrap();
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let text = match client
        .request(Request::TicketPrime(KeyParams {
            key: "SHOP-20".into(),
        }))
        .await
        .unwrap()
    {
        Reply::Prime(c) => render(&c),
        other => panic!("{other:?}"),
    };
    let own = text
        .find("\n## Agent instructions (Developer)\n\nUse my /tdd skill.\nRun `pnpm test` before submitting.\n")
        .expect(&text);
    let notes = text
        .find("\n## Notes every agent in shop-web gets\n\nAsk before touching the payment code.\n")
        .expect(&text);
    let stage = text.find(DEV).expect(&text);
    let depends = text.find("\n## Depends on\n").expect(&text);
    assert!(own < notes && notes < stage && stage < depends, "{text}");
    assert!(!text.contains("## Stage choices"), "{text}");
    assert!(!text.contains("## Agents"), "{text}");

    // Pending questions render once they exist (M10); the never-filled "Review feedback" is gone (CMT-07).
    let mut context = w
        .board
        .prime_context("SHOP-20", w.adapters.for_new_session())
        .await
        .unwrap();
    context.pending_questions = vec!["SHOP-21 asks: which email template?".into()];
    let text = render(&context);
    assert!(!text.contains("## Review feedback"), "{text}");
    assert!(text.contains("\n## Pending questions\n\n- SHOP-21 asks: which email template?\n"));
    w.stop().await;
}

/// AGM-06 on the test flow: Dev's prime names Rust developer's instructions and its stage, lists Grill's latest run
/// under "Earlier stages" (one line, CMT-07) and Griller's comment under "Comments", and the later stages whose
/// choice its agent may still set (QA's skip); it names no commit skill, since Rust developer has none
/// (`commit-compact` isn't `commit`).
#[tokio::test(flavor = "multi_thread")]
async fn on_the_test_flow_dev_reads_grills_comment() {
    let w = with_agents().await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let t = w.create("SHOP", "Sort notes by date").await;
    // Grill ran twice; only its latest run is listed.
    ended_run(&w, &t.id, ("grill", "agent"), "Griller", None, "cancelled").await;
    ended_run(&w, &t.id, ("grill", "agent"), "Griller", None, "done").await;
    let key = t.key.to_string();
    comment_at(
        &w,
        &key,
        agent("Griller", "grill", None),
        "Agreed: newest first.\nDates in the user's time zone.",
        (9, 5),
    )
    .await;
    w.force_status(&t, Status::Working).await; // in Dev
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let context = match client
        .request(Request::TicketPrime(KeyParams { key: key.clone() }))
        .await
        .unwrap()
    {
        Reply::Prime(c) => c,
        other => panic!("{other:?}"),
    };
    let text = render(&context);
    for part in [
        "\n## Agent instructions (Rust developer)\n\nFollow our error-handling rules: no unwrap in library code.\n",
        "\n## Your stage\n\nDev, as Rust developer, with the skill `/otaq-work-ticket`.\n",
        "\n## Earlier stages\n\n- Grill · Griller · finished\n\n## Comments\n\n### Griller · Grill · 9:05 am\n\nAgreed: newest first.\nDates in the user's time zone.\n",
        "\n## Stage choices\n\nYou may change these later stages' choices for this ticket",
        &format!("`otaq update {key} --stage \"QA=skip\" --reason \"…\"`"),
        "\n- QA (now: required)\n  - skip: the ticket passes this stage\n",
        "2. Commit everything with git (`git add -A`, `git commit`) before you end your stage.",
    ] {
        assert!(text.contains(part), "missing {part:?} in\n{text}");
    }
    assert!(!text.contains("stopped"), "only the latest run: {text}");
    assert!(!text.contains("- Dev ("), "Dev has started: {text}");
    assert_eq!(context.commit_skill, "");
    w.stop().await;
}

/// AGM-06, AGM-08 on the test flow: the Project agent's prime lists every stage with options, each agent with its
/// model and its instructions' first line, and how to set a choice; the epic lead's the same, for its children.
#[tokio::test(flavor = "multi_thread")]
async fn on_the_test_flow_the_project_agent_reads_the_stage_options() {
    let w = with_agents().await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let (_, project, epic) = primes(&w).await;
    let default = match w.board.store().default_model().await.unwrap() {
        Some(m) => format!(
            "default · {} · {}",
            m.tool.display_name(),
            model_text(m.model.as_deref(), Some(&claude_list()))
        ),
        None => "default".to_owned(),
    };
    let options = format!(
        "- Grill\n  - Griller (the stage's own) · {default}: Ask one question at a time.\n  - skip: the ticket passes \
         this stage\n- Dev\n  - Rust developer (the stage's own) · {default}: Follow our error-handling rules: no \
         unwrap in library code.\n  - Quick developer · {default}\n- QA\n  - skip: the ticket passes this stage\n"
    );
    assert!(
        project.contains(&format!(
            "\n## Stage choices\n\nA new ticket runs each stage's own agent. Pick another option for a ticket you \
             create only when how hard the work is says so, with a reason: `otaq create \"Title\" --stage \
             \"Grill=skip\" --reason \"…\" --stdin`. When unsure, leave the stage's own agent.\n\n{options}"
        )),
        "{project}"
    );
    assert!(
        epic.contains(&format!(
            "\n## Stage choices\n\nSet them on never-started children only, when how hard the work is says so, \
             always with a reason: `otaq update <KEY> --stage \"Grill=skip\" --reason \"…\"`, or on `otaq create \
             --parent`. Never change a choice the user set.\n\n{options}"
        )),
        "{epic}"
    );
    w.stop().await;
}

/// T-0184 (VAL-10): `prime --validate` for a plain ticket: the ticket, the summary, the parents with theirs, the
/// Agent instructions, the diff and how to report. T-0445 (CMT-09): a short verdict, the full run as a report.
#[tokio::test(flavor = "multi_thread")]
async fn validate_prime_plain() {
    let w = sample().await;
    // The check's own agent's instructions (AGM-06), not Developer's.
    w.edit_agent("Validator", |a| {
        a.instructions = "Run `pnpm test` before submitting.".into();
    })
    .await;
    let ctx = w.board.validate_prime("SHOP-20").await.unwrap();
    assert_eq!((ctx.round, ctx.rounds), (1, 3));
    let text = otaq_core::prime::render_validate(&ctx);
    for part in [
        "# Validate SHOP-20 · Checkout page\n",
        "a read-only judge of its agent's work (round 1 of 3). Never edit files, never commit and never fix anything.",
        "## What and why\n\nThe checkout page shows the cart",
        "## Agent instructions (Validator)\n\nRun `pnpm test` before submitting.",
        "## Linked tickets",
        "- depends on SHOP-18 · Checkout API (Ready for Review)\n  Last comment, Developer · Dev · 11:20 am:\n  > POST /checkout creates a Stripe session.",
        "- depends on SHOP-19 · Price table (Working)",
        "## The diff\n\n```diff\n",
        "\n## Your verdict\n\nKeep it short: one or two lines, one line per finding with its file and line; notes \
         start with \"Note:\". Finish with `otaq check pass --stdin` or `otaq check fail --stdin` and the verdict on \
         stdin. Save the full check run to a file in `$OTAQ_RESOURCE_DIR` (never in the worktree) and attach it with \
         `--report-file`: `otaq check fail --stdin --report-file \"$OTAQ_RESOURCE_DIR/test-run.txt\"`. What you couldn't \
         check yourself (a real browser, a device, a service you can't reach), list for the user: `--manual-check \
         \"<what to check>\"` on `otaq check pass|fail`, one line per check. The user tries them at review. Never \
         leave a check to \"the PR\": there may be no PR.\n",
    ] {
        assert!(text.contains(part), "missing {part:?} in\n{text}");
    }
    assert!(!text.contains("## The user's review"), "never sent back");
    assert!(
        !text.contains("## Earlier stages"),
        "no stage ran yet: {text}"
    );
    w.stop().await;
}

/// An epic child: the epic's description and its siblings.
#[tokio::test(flavor = "multi_thread")]
async fn validate_prime_epic_child_with_siblings() {
    let w = sample().await;
    w.board
        .update_ticket(
            "SHOP-40",
            &TicketUpdate {
                body: Some("Customers keep a wishlist.".into()),
                ..Default::default()
            },
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    let ctx = w.board.validate_prime("SHOP-40.2").await.unwrap();
    let epic = ctx.epic.as_ref().unwrap();
    assert_eq!(epic.key.to_string(), "SHOP-40");
    let siblings: Vec<String> = epic.siblings.iter().map(|s| s.key.to_string()).collect();
    assert_eq!(siblings, ["SHOP-40.1", "SHOP-40.3"]);
    let text = otaq_core::prime::render_validate(&ctx);
    assert!(
        text.contains("Epic SHOP-40 · Wishlist:\nCustomers keep a wishlist.\n- sibling SHOP-40.1"),
        "{text}"
    );
    w.stop().await;
}

/// Claude Code's list as its `initialize` request gives it: the tool default and four follow choices.
fn claude_list() -> ModelList {
    let entry = |id: &str, name: &str, kind| ModelEntry {
        id: id.into(),
        display_name: name.into(),
        description: None,
        resolves_to: None,
        kind,
        family: None,
        released_at: None,
    };
    let mut entries = vec![entry(
        "default",
        "Default (recommended)",
        ModelKind::ToolDefault,
    )];
    for (id, name) in [
        ("opus", "Opus"),
        ("sonnet", "Sonnet"),
        ("haiku", "Haiku"),
        ("fable", "Fable"),
    ] {
        entries.push(entry(id, name, ModelKind::Follow));
    }
    ModelList { entries }
}

/// The sample board on a service whose copy of Claude Code's list was read earlier.
async fn with_agents() -> TestService {
    let home = tempfile::TempDir::new().unwrap();
    let store = Store::open_in(&DataDir::new(home.path().join("h")))
        .await
        .unwrap();
    let tool = Tool::ALL[0];
    let copy = ModelCopy {
        tool,
        list: claude_list(),
        read_at: 1,
    };
    store
        .set_setting(&settings_key(tool), &serde_json::to_string(&copy).unwrap())
        .await
        .unwrap();
    store.close().await;
    let w = TestService::start_in(home, TempRepos::new(), |config| config).await;
    w.shop_sample().await;
    w
}

/// The three primes as the user's terminal gets them, rendered.
async fn primes(w: &TestService) -> (String, String, String) {
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let ticket = match client
        .request(Request::TicketPrime(KeyParams {
            key: "SHOP-20".into(),
        }))
        .await
        .unwrap()
    {
        Reply::Prime(c) => render(&c),
        other => panic!("{other:?}"),
    };
    let project = match client
        .request(Request::ProjectAgentPrime(ProjectParams {
            project: "SHOP".into(),
        }))
        .await
        .unwrap()
    {
        Reply::ProjectPrime(c) => otaq_core::project_agents::prime::render(&c),
        other => panic!("{other:?}"),
    };
    let epic = match client
        .request(Request::LeadPrime(KeyParams {
            key: "SHOP-40".into(),
        }))
        .await
        .unwrap()
    {
        Reply::EpicPrime(c) => otaq_core::epic_lead::prime::render(&c),
        other => panic!("{other:?}"),
    };
    (ticket, project, epic)
}

/// AGM-06: the default flow has no stage options, so no prime lists choices to set.
#[tokio::test(flavor = "multi_thread")]
async fn no_prime_lists_choices_on_the_default_flow() {
    let w = with_agents().await;
    let (ticket, project, epic) = primes(&w).await;
    for text in [&ticket, &project, &epic] {
        assert!(!text.contains("\n## Stage choices\n"), "{text}");
        assert!(!text.contains("\n## Agents\n"), "{text}");
    }
    w.stop().await;
}

/// The model as Settings shows it, from the tool's list.
#[test]
fn models_read_as_settings_shows_them() {
    let list = claude_list();
    assert_eq!(model_text(None, Some(&list)), "tool default");
    assert_eq!(model_text(Some("default"), Some(&list)), "tool default");
    assert_eq!(model_text(Some("default"), None), "tool default");
    assert_eq!(
        model_text(Some("opus"), Some(&list)),
        "Opus · always the newest"
    );
    assert_eq!(model_text(Some("opus"), None), "opus");
    assert_eq!(
        model_text(Some("claude-opus-5-5"), Some(&list)),
        "claude-opus-5-5"
    );
}

/// The example's SHOP-34 on the test flow, in Dev after Validate failed round 2: Grill and Validate ran, and four
/// comments (Griller's, the user's, Rust developer's and Validator's with its 48 KB report), plus a fifth the user
/// deleted. Returns the ticket's key.
async fn the_example(w: &TestService) -> String {
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let t = w.create("SHOP", "Export orders as CSV").await;
    let key = t.key.to_string();
    ended_run(w, &t.id, ("grill", "agent"), "Griller", None, "done").await;
    ended_run(w, &t.id, ("dev", "agent"), "Rust developer", None, "done").await;
    ended_run(
        w,
        &t.id,
        ("validate", "check"),
        "Validator",
        Some(1),
        "failed",
    )
    .await;
    ended_run(w, &t.id, ("dev", "agent"), "Rust developer", None, "done").await;
    ended_run(
        w,
        &t.id,
        ("validate", "check"),
        "Validator",
        Some(2),
        "failed",
    )
    .await;
    comment_at(
        w,
        &key,
        agent("Griller", "grill", None),
        "Agreed with you: CSV only, admins only. Reuse the CSV writer in `export.rs` from SHOP-12.",
        (13, 40),
    )
    .await;
    comment_at(
        w,
        &key,
        you(),
        "Also keep the column order the same as the table on screen.",
        (14, 14),
    )
    .await;
    comment_at(
        w,
        &key,
        agent("Rust developer", "dev", None),
        "Done. Excel not added; left a TODO where it would go.",
        (14, 50),
    )
    .await;
    let failed = w
        .board
        .post_ticket_comment(
            &key,
            &agent("Validator", "validate", Some(2)),
            "Fail: the 10,000 row limit isn't enforced; 3 tests fail in `export_tests.rs`.",
            Some(ReportSource::Text(
                "test export_tests::row_limit ... FAILED\n".into(),
            )),
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    set_posted_at(w, &key, failed.n, (15, 5)).await;
    // The report as the example's: 48 KB, 2,310 lines.
    sqlx::query(
        "UPDATE ticket_comments SET report_bytes = 49152, report_lines = 2310 WHERE ticket_id = ? AND n = ?",
    )
    .bind(&t.id)
    .bind(i64::from(failed.n))
    .execute(w.board.store().pool())
    .await
    .unwrap();
    let gone = comment_at(w, &key, you(), "Ignore the Excel part.", (15, 10)).await;
    w.board
        .delete_ticket_comment(&key, gone, &Actor::User("me".into()))
        .await
        .unwrap();
    w.force_status(&t, Status::Working).await; // back in Dev
    key
}

/// The example's four comments as `prime` and `prime --validate` print them (CMT-07).
fn example_comments(key: &str) -> String {
    format!(
        "\n## Comments\n\n\
         ### Griller · Grill · 1:40 pm\n\n\
         Agreed with you: CSV only, admins only. Reuse the CSV writer in `export.rs` from SHOP-12.\n\n\
         ### You · 2:14 pm\n\n\
         Also keep the column order the same as the table on screen.\n\n\
         ### Rust developer · Dev · 2:50 pm\n\n\
         Done. Excel not added; left a TODO where it would go.\n\n\
         ### Validator · Validate · round 2 · 3:05 pm\n\n\
         Fail: the 10,000 row limit isn't enforced; 3 tests fail in `export_tests.rs`.\n\n\
         Report: 48 KB, 2,310 lines · `otaq report {key} 4`\n"
    )
}

/// CMT-07 (Done when, the example): Dev's prime prints the four comments in full, oldest first, after "Your stage"
/// and "Earlier stages" (one line per stage, no summaries); the deleted one and the report's text never appear.
#[tokio::test(flavor = "multi_thread")]
async fn prime_prints_the_examples_four_comments() {
    let w = with_agents().await;
    let key = the_example(&w).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let context = match client
        .request(Request::TicketPrime(KeyParams { key: key.clone() }))
        .await
        .unwrap()
    {
        Reply::Prime(c) => c,
        other => panic!("{other:?}"),
    };
    let text = render(&context);
    let expected = format!(
        "\n## Your stage\n\nDev, as Rust developer, with the skill `/otaq-work-ticket`.\n\
         \n## Earlier stages\n\n- Grill · Griller · finished\n- Validate · Validator · round 2 · failed\n{}\
         \n## Stage choices\n",
        example_comments(&key)
    );
    assert!(text.contains(&expected), "missing {expected:?} in\n{text}");
    for gone in [
        "Ignore the Excel part.",
        "FAILED",
        "## Review feedback",
        "Earlier rounds' findings",
    ] {
        assert!(!text.contains(gone), "{gone:?} in\n{text}");
    }
    assert!(text.contains(
        "\n8. Long output goes to a file in `$OTAQ_RESOURCE_DIR`; attach it with `--report-file`.\n"
    ));
    w.stop().await;
}

/// RPT-01 (T-0474): `prime` lists the user's report first, under "Reports from you · read these before you start",
/// with its name, size, `otaq report KEY N` and its comment's line; the agent's report stays a line under its comment;
/// neither report's text is printed. `prime --validate` lists it the same way.
#[tokio::test(flavor = "multi_thread")]
async fn prime_lists_the_users_reports_first() {
    let w = with_agents().await;
    let resources = tempfile::TempDir::new().unwrap();
    let mine = resources.path().join("test-run-login.md");
    std::fs::write(&mine, "SECRET-USER-RUN\n2 failed\n18 passed\n").unwrap();
    let theirs = resources.path().join("test-run.txt");
    std::fs::write(&theirs, "SECRET-AGENT-RUN\n").unwrap();
    let t = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Fix flaky login redirect".into(),
                first_comment: Some(FirstComment {
                    body: "Fails 2 in 20 on Safari.\nThe full run is attached.".into(),
                    report: Some(ReportSource::Path(mine)),
                }),
                ..Default::default()
            },
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    let key = t.key.to_string();
    set_posted_at(&w, &key, 1, (14, 20)).await;
    let n = w
        .board
        .post_ticket_comment(
            &key,
            &agent("Rust developer", "dev", None),
            "The middleware reads the cookie too early.",
            Some(ReportSource::Path(theirs)),
            &Actor::User("me".into()),
        )
        .await
        .unwrap()
        .n;
    set_posted_at(&w, &key, n, (14, 38)).await;
    w.force_status(&t, Status::Working).await;

    let context = w
        .board
        .prime_context(&key, w.adapters.for_new_session())
        .await
        .unwrap();
    let text = render(&context);
    let expected = format!(
        "\n## Reports from you \u{b7} read these before you start\n\n\
         - test-run-login.md \u{b7} 35 bytes, 3 lines \u{b7} `otaq report {key} 1`\n  \
         With comment 1 (You \u{b7} at creation \u{b7} 2:20 pm): Fails 2 in 20 on Safari.\n\
         \n## Comments\n\n\
         ### You \u{b7} at creation \u{b7} 2:20 pm\n\n\
         Fails 2 in 20 on Safari.\nThe full run is attached.\n\n\
         Report: 35 bytes, 3 lines \u{b7} `otaq report {key} 1`\n\
         \n### Rust developer \u{b7} Dev \u{b7} 2:38 pm\n\n\
         The middleware reads the cookie too early.\n\n\
         Report: 17 bytes, 1 line \u{b7} `otaq report {key} 2`\n"
    );
    assert!(text.contains(&expected), "missing {expected:?} in\n{text}");
    assert_eq!(text.matches("Reports from you").count(), 1, "{text}");
    for gone in ["SECRET-USER-RUN", "SECRET-AGENT-RUN", "test-run.txt"] {
        assert!(!text.contains(gone), "{gone:?} in\n{text}");
    }

    let validate = otaq_core::prime::render_validate(&w.board.validate_prime(&key).await.unwrap());
    assert!(validate.contains(&expected), "{validate}");

    // Without a report of yours, no section.
    let plain = w.create("SHOP", "Order list paging").await;
    post_agent_report(&w, &plain.key.to_string()).await;
    let ctx = w
        .board
        .prime_context(&plain.key.to_string(), w.adapters.for_new_session())
        .await
        .unwrap();
    assert!(!render(&ctx).contains("Reports from you"));
    w.stop().await;
}

async fn post_agent_report(w: &TestService, key: &str) {
    w.board
        .post_ticket_comment(
            key,
            &agent("Validator", "validate", Some(1)),
            "Fail: 3 tests fail.",
            Some(ReportSource::Text("FAILED\n".into())),
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
}

/// CMT-07: `prime --validate` prints the comments too (earlier rounds' findings are among them); "Earlier rounds'
/// findings" is gone.
#[tokio::test(flavor = "multi_thread")]
async fn prime_validate_prints_the_comments() {
    let w = with_agents().await;
    let key = the_example(&w).await;
    let ctx = w.board.validate_prime(&key).await.unwrap();
    let text = otaq_core::prime::render_validate(&ctx);
    let comments = example_comments(&key);
    assert!(text.contains(&comments), "missing {comments:?} in\n{text}");
    let at = text.find("\n## Comments\n").unwrap();
    assert!(
        text.find("\n## Earlier stages\n").unwrap() < at
            && at < text.find("\n## The diff\n").unwrap(),
        "{text}"
    );
    for gone in [
        "Ignore the Excel part.",
        "FAILED",
        "Earlier rounds' findings",
        "## The user's review",
    ] {
        assert!(!text.contains(gone), "{gone:?} in\n{text}");
    }

    // No comments: no section.
    let plain = w.create("SHOP", "Order list paging").await;
    let text = otaq_core::prime::render_validate(
        &w.board
            .validate_prime(&plain.key.to_string())
            .await
            .unwrap(),
    );
    assert!(!text.contains("## Comments"), "{text}");
    w.stop().await;
}

/// CMT-07: "Depends on" shows a parent's last comment from an Agent stage (not a check's, not the user's), with its
/// report's line; the epic lead reads its children's the same way.
#[tokio::test(flavor = "multi_thread")]
async fn a_parents_last_comment_from_an_agent_stage() {
    let w = with_agents().await;
    let parent = the_example(&w).await;
    let child = w.create("SHOP", "Export invoices as CSV").await;
    let me = Actor::User("me".into());
    w.board
        .add_dependency(&child.key.to_string(), &parent, &me)
        .await
        .unwrap();
    let context = w
        .board
        .prime_context(&child.key.to_string(), w.adapters.for_new_session())
        .await
        .unwrap();
    let text = render(&context);
    let expected = format!(
        "\n## Depends on\n\n- {parent} · Export orders as CSV (Working)\n  Last comment, Rust developer · Dev · 2:50 \
         pm:\n  > Done. Excel not added; left a TODO where it would go.\n"
    );
    assert!(text.contains(&expected), "missing {expected:?} in\n{text}");

    // A later Dev comment with a report: it is the one shown, with the report's line.
    let posted = w
        .board
        .post_ticket_comment(
            &parent,
            &agent("Rust developer", "dev", None),
            "Fixed the row limit.",
            Some(ReportSource::Text("ok\n".into())),
            &me,
        )
        .await
        .unwrap();
    set_posted_at(&w, &parent, posted.n, (16, 0)).await;
    let ctx = w
        .board
        .validate_prime(&child.key.to_string())
        .await
        .unwrap();
    let text = otaq_core::prime::render_validate(&ctx);
    let expected = format!(
        "- depends on {parent} · Export orders as CSV (Working)\n  Last comment, Rust developer · Dev · 4:00 pm:\n  \
         > Fixed the row limit.\n  Report: 3 bytes, 1 line · `otaq report {parent} {}`\n",
        posted.n
    );
    assert!(text.contains(&expected), "missing {expected:?} in\n{text}");
    w.stop().await;
}

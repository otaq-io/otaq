//! Approve & open PR's commit message fields, end to end through the requests the page sends: shop-web (SHOP) whose
//! `origin` is a local bare repository and whose commit-msg hook allows only `feat:` and `fix:` messages, SHOP-1 in
//! Review with two commits. A commit message typed in Open PR's field is kept on the ticket and goes through the hook
//! on confirm; the refusal rewrites nothing and closes the dialog; Edit messages and retry opens Open PR again with
//! the kept text in that field, marked; fixed in the field, the confirm goes on and the commit is reworded.
//!
//! The app talks to a core `TestService` (temporary `OTAQ_HOME` and repositories); the hook is a plain `sh` script in
//! the temporary repository's `.git/hooks`; the ticket's agent session is a plain `sh`. No agent tool runs.

use std::path::Path;
use std::time::Duration;

use otaq_core::board::projects::AddProject;
use otaq_core::board::settings::WorkflowStep;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::naming;
use otaq_core::service::testing::{TestService, said};
use otaq_core::skills::testing::skill_folder;
use serde_json::{Value, json};

use crate::App;
use crate::api::request;
use crate::service::testing::{app_on, connected};

async fn app(w: &TestService) -> App {
    let app = app_on(w.data.clone());
    tokio::time::timeout(Duration::from_secs(10), async {
        while !connected(&app.service.connection()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the app connects");
    app
}

/// The subjects of `range`, oldest first.
fn subjects(repo: &Path, range: &str) -> Vec<String> {
    git(repo, &["log", "--reverse", "--format=%s", range])
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The dialog's view in an answer.
fn view(answer: &Value) -> &Value {
    assert_eq!(answer["outcome"], "dialog", "{answer}");
    &answer["view"]
}

/// The commit fields as (message, refused).
fn fields(view: &Value) -> Vec<(String, bool)> {
    view["commits"]
        .as_array()
        .expect("commit fields")
        .iter()
        .map(|c| {
            (
                c["message"].as_str().unwrap().to_owned(),
                c["refused"].as_bool().unwrap(),
            )
        })
        .collect()
}

/// Types `value` into commit field `index`, as the page does on each keystroke.
async fn type_commit(app: &App, index: u32, value: &str) -> Value {
    request(
        app,
        "pr.edit",
        json!({ "key": "SHOP-1", "field": "commit", "index": index, "value": value }),
    )
    .await
    .unwrap()
}

/// Names the branch and checks it, as the page does after the debounce: Push & open PR is on.
async fn name_branch(app: &App, branch: &str) {
    let edited = request(
        app,
        "pr.edit",
        json!({ "key": "SHOP-1", "field": "branch", "value": branch }),
    )
    .await
    .unwrap();
    let check = view(&edited)["check"].clone();
    let checked = request(app, "pr.check", json!({ "key": "SHOP-1", "check": check }))
        .await
        .unwrap();
    assert_eq!(view(&checked)["can_confirm"], true, "{checked}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_commit_message_is_fixed_in_open_prs_field() {
    let w = TestService::start().await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    let origin = w.repos.root().join("shop-web-origin.git");
    w.board
        .add_project(&AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    // origin is a local folder: the project uses its own Open PR skill.
    let folder = skill_folder(w.data.root(), "my-pr");
    w.use_step_skill(WorkflowStep::OpenPr, &folder).await;
    let ticket = w.create("SHOP", "Login redirect").await;
    let worktree = w.start_worktree(&ticket).await;
    commit(&worktree, "login.ts", "redirect\n");
    commit(&worktree, "login.test.ts", "test\n");
    w.force_status(&ticket, Status::Working).await;
    let me = Actor::User("me".into());
    w.board
        .submit_for_review(
            "SHOP-1",
            said(&w.board, "SHOP-1", "Redirects after login.")
                .await
                .as_ref(),
            &me,
        )
        .await
        .unwrap();
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    let agent = w.agent_shell(&ticket).await;
    let hook = repo.join(".git/hooks/commit-msg");
    std::fs::write(
        &hook,
        "#!/bin/sh\n\
         grep -qE '^(feat|fix):' \"$1\" || { echo 'commitlint: type must be one of [feat, fix]' >&2; exit 1; }\n",
    )
    .unwrap();
    // Git for Windows runs hooks with its own sh; only macOS and Linux need the executable bit.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let branch = naming::branch("SHOP-1");
    let before = git(&repo, &["rev-parse", &branch]);
    let app = app(&w).await;

    // Approve & open PR: the dialog shows both commits' messages, editable.
    let opened = request(&app, "pr.open", json!({ "key": "SHOP-1" }))
        .await
        .unwrap();
    let shown = view(&opened);
    assert_eq!(shown["push"], false);
    assert_eq!(shown["stage"]["kind"], "proposed", "{shown}");
    assert_eq!(shown["commits_label"], "Commit messages (2 commits)");
    let proposed: Vec<String> = fields(shown).into_iter().map(|(m, _)| m).collect();
    assert_eq!(proposed.len(), 2);
    assert!(fields(shown).iter().all(|(_, refused)| !refused));

    // The user types a message the team's rule rejects into the first commit's field and a good one into the second,
    // and confirms.
    let typed = type_commit(&app, 0, "update stuff").await;
    assert_eq!(fields(view(&typed))[0].0, "update stuff");
    assert_eq!(view(&typed)["use_proposed"], "Use the proposed message");
    type_commit(&app, 1, "fix: cover deep links").await;
    name_branch(&app, "feature/SHOP-1-login").await;
    let refused = request(&app, "pr.confirm", json!({ "key": "SHOP-1" }))
        .await
        .unwrap();
    assert_eq!(
        refused,
        json!({ "outcome": "closed" }),
        "the refusal is on the ticket"
    );
    assert_eq!(
        git(&repo, &["rev-parse", &branch]),
        before,
        "nothing rewritten"
    );
    assert!(
        git(&origin, &["branch", "--list", "feature/*"])
            .trim()
            .is_empty(),
        "nothing pushed"
    );
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    let kept = ticket.flags.hook_refusal.clone().expect("on the ticket");
    assert_eq!(kept.message.as_deref(), Some("update stuff"));

    // Edit messages and retry: Open PR again, the kept texts in the commit fields, the refused one marked.
    let reopened = request(&app, "pr.open", json!({ "key": "SHOP-1" }))
        .await
        .unwrap();
    assert_eq!(
        fields(view(&reopened)),
        [
            ("update stuff".to_owned(), true),
            ("fix: cover deep links".to_owned(), false)
        ]
    );
    // Use the proposed message puts the commit messages back and keeps nothing; the user types them again.
    let back = request(&app, "pr.use_proposed", json!({ "key": "SHOP-1" }))
        .await
        .unwrap();
    assert_eq!(
        fields(view(&back))
            .into_iter()
            .map(|(m, _)| m)
            .collect::<Vec<_>>(),
        proposed
    );
    assert_eq!(view(&back)["use_proposed"], Value::Null);
    let edited = w.board.edited_message(&ticket.id).await.unwrap();
    assert!(edited.commits.is_empty(), "{edited:?}");
    type_commit(&app, 1, "fix: cover deep links").await;

    // Fixed in the commit's field, the confirm goes on and the commit is reworded.
    let fixed = type_commit(&app, 0, "fix: redirect after login").await;
    assert_eq!(
        fields(view(&fixed))[0],
        ("fix: redirect after login".to_owned(), false)
    );
    name_branch(&app, "feature/SHOP-1-login").await;
    let confirmed = request(&app, "pr.confirm", json!({ "key": "SHOP-1" }))
        .await
        .unwrap();
    assert_eq!(view(&confirmed)["stage"]["kind"], "pushing", "{confirmed}");
    assert_eq!(
        subjects(&repo, "main..feature/SHOP-1-login"),
        ["fix: redirect after login", "fix: cover deep links"]
    );
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.flags.hook_refusal, None);
    w.wait_for_output(&agent.session, "The user confirmed the pull request")
        .await;
    w.stop().await;
}

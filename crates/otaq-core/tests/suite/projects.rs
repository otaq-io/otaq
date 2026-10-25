//! Projects against temporary git repositories (PRJ-01…PRJ-04): validation, key rules, base-branch prefill,
//! and that neither adding nor removing ever touches the repository.

use std::path::Path;

use otaq_core::board::projects::{AddProject, ProjectUpdate};
use otaq_core::board::{Board, Refusal};
use otaq_core::data_dir::DataDir;
use otaq_core::git::testing::{TempRepos, git, snapshot};
use otaq_core::store::tickets::NewTicket;
use otaq_core::store::{Store, new_id};

struct Fixture {
    _home: tempfile::TempDir,
    repos: TempRepos,
    board: Board,
}

async fn fixture() -> Fixture {
    let home = tempfile::tempdir().unwrap();
    let data = DataDir::new(home.path().join("home"));
    let store = Store::open_in(&data).await.unwrap();
    Fixture {
        _home: home,
        repos: TempRepos::new(),
        board: Board::with_path_git(store, data),
    }
}

fn add(path: &Path) -> AddProject {
    AddProject {
        path: path.display().to_string(),
        ..Default::default()
    }
}

fn repo_state(repo: &Path) -> (Vec<String>, String) {
    (
        snapshot(repo),
        git(repo, &["status", "--porcelain", "--ignored"]),
    )
}

#[tokio::test]
async fn adds_a_repo_with_suggested_key_and_remote_default_base() {
    let f = fixture().await;
    let repo = f.repos.clone_with_origin("shop-web", "develop", 2);
    let before = repo_state(&repo);

    let nested = repo.join("src");
    std::fs::create_dir(&nested).unwrap();
    let probe = f
        .board
        .probe_folder(&nested.display().to_string())
        .await
        .unwrap();
    std::fs::remove_dir(&nested).unwrap();
    assert_eq!(
        probe.repo_path,
        repo.display().to_string(),
        "the repo root is stored"
    );
    assert_eq!(probe.name, "shop-web");
    assert_eq!(probe.suggested_key, "SHOP");
    assert_eq!(probe.base_branch, "develop");
    assert_eq!(probe.branches.local, ["develop"], "local branches");
    assert_eq!(
        probe.branches.remote,
        ["origin/develop"],
        "remote-tracking branches, without origin/HEAD"
    );

    let project = f.board.add_project(&add(&repo)).await.unwrap();
    assert_eq!(project.key, "SHOP");
    assert_eq!(project.name, "shop-web");
    assert_eq!(project.base_branch, "develop");
    assert_eq!(project.repo_path, repo.display().to_string());
    assert_eq!(
        repo_state(&repo),
        before,
        "nothing is written into the repo"
    );
}

#[tokio::test]
async fn without_a_remote_the_current_branch_is_the_base() {
    let f = fixture().await;
    let repo = f.repos.repo("billing-api", 1);
    git(&repo, &["checkout", "--quiet", "-b", "trunk"]);
    let project = f.board.add_project(&add(&repo)).await.unwrap();
    assert_eq!(project.key, "BILL");
    assert_eq!(project.base_branch, "trunk");
}

#[tokio::test]
async fn validation() {
    let f = fixture().await;
    let plain = f.repos.plain_folder("notes");
    let err = f.board.add_project(&add(&plain)).await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NotAGitRepo), "{err}");
    assert!(err.to_string().contains("isn't a git repository"), "{err}");

    let missing = f.repos.root().join("nowhere");
    let err = f.board.add_project(&add(&missing)).await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NotAGitRepo), "{err}");

    let shop = f.repos.repo("shop-web", 1);
    f.board.add_project(&add(&shop)).await.unwrap();
    let err = f
        .board
        .add_project(&add(&shop.join(".")))
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::AlreadyAdded), "{err}");
    assert_eq!(err.to_string(), "shop-web is already added as SHOP");

    let other = f.repos.repo("shopify", 1);
    for (key, refusal) in [
        ("shop", Refusal::BadKey),
        ("1SHOP", Refusal::BadKey),
        ("SH-OP", Refusal::BadKey),
        ("SHOP", Refusal::KeyTaken),
    ] {
        let err = f
            .board
            .add_project(&AddProject {
                key: Some(key.into()),
                ..add(&other)
            })
            .await
            .unwrap_err();
        assert_eq!(err.refusal(), Some(refusal), "{key}: {err}");
    }
    let probe = f
        .board
        .probe_folder(&other.display().to_string())
        .await
        .unwrap();
    assert_eq!(
        probe.suggested_key, "SHOP2",
        "a taken key gets a distinct suggestion"
    );
    let project = f
        .board
        .add_project(&AddProject {
            key: Some("SHPY".into()),
            base_branch: Some("release/2.3".into()),
            ..add(&other)
        })
        .await
        .unwrap();
    assert_eq!(
        (project.key.as_str(), project.base_branch.as_str()),
        ("SHPY", "release/2.3")
    );
}

#[tokio::test]
async fn update_changes_name_and_base_but_never_the_key() {
    let f = fixture().await;
    let repo = f.repos.repo("shop-web", 1);
    f.board.add_project(&add(&repo)).await.unwrap();

    let updated = f
        .board
        .update_project(
            "SHOP",
            &ProjectUpdate {
                name: Some("Shop web".into()),
                base_branch: Some("develop".into()),
                key: Some("SHOP".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (updated.name.as_str(), updated.base_branch.as_str()),
        ("Shop web", "develop")
    );

    let err = f
        .board
        .update_project(
            "SHOP",
            &ProjectUpdate {
                key: Some("SHP".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::KeyLocked), "{err}");
    assert_eq!(f.board.project("SHOP").await.unwrap().key, "SHOP");
}

#[tokio::test]
async fn remove_deletes_everything_in_the_database_and_leaves_the_repo() {
    let f = fixture().await;
    let repo = f.repos.clone_with_origin("shop-web", "develop", 2);
    let project = f.board.add_project(&add(&repo)).await.unwrap();
    let other = f
        .board
        .add_project(&add(&f.repos.repo("billing-api", 1)))
        .await
        .unwrap();
    let store = f.board.store();
    let pool = store.pool();

    // A ticket with a dependency, a comment, a session row and an event.
    let mut ids = Vec::new();
    for number in [18, 19] {
        let t = store
            .insert_ticket(&NewTicket {
                project_id: project.id.clone(),
                number,
                title: format!("SHOP-{number}"),
                body: String::new(),
                stage_id: None,
                state: "backlog".into(),
                created_by: "me".into(),
            })
            .await
            .unwrap();
        ids.push(t.id);
    }
    for (sql, binds) in [
        (
            "INSERT INTO ticket_deps (ticket_id, depends_on_id) VALUES (?, ?)",
            vec![ids[1].clone(), ids[0].clone()],
        ),
        (
            "INSERT INTO review_comments (id, ticket_id, round, file, line, side, body, created_at) VALUES (?, ?, 1, 'a', 1, 'new', 'b', 0)",
            vec![new_id(), ids[0].clone()],
        ),
        (
            "INSERT INTO sessions (id, project_id, ticket_id, kind, state, started_at) VALUES (?, ?, ?, 'agent', 'ended', 0)",
            vec![new_id(), project.id.clone(), ids[0].clone()],
        ),
        (
            "INSERT INTO sessions (id, project_id, kind, state, started_at) VALUES (?, ?, 'project', 'ended', 0)",
            vec![new_id(), project.id.clone()],
        ),
        (
            "INSERT INTO events (id, ticket_id, kind, actor, at) VALUES (?, ?, 'created', 'me', 0)",
            vec![new_id(), ids[0].clone()],
        ),
    ] {
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for b in binds {
            q = q.bind(b);
        }
        q.execute(pool).await.unwrap();
    }
    sqlx::query("UPDATE projects SET settings_json = '{\"setup_command\":\"npm i\"}' WHERE id = ?")
        .bind(&project.id)
        .execute(pool)
        .await
        .unwrap();
    let before = repo_state(&repo);

    f.board.remove_project("SHOP").await.unwrap();

    for table in [
        "tickets",
        "ticket_deps",
        "review_comments",
        "sessions",
        "events",
    ] {
        let n: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(n, 0, "{table}");
    }
    let keys: Vec<String> = f
        .board
        .projects()
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.key)
        .collect();
    assert_eq!(keys, [other.key]);
    assert_eq!(repo_state(&repo), before, "the repo folder is untouched");
    assert!(repo.join(".git").is_dir());
}

#[tokio::test]
async fn branches_for_the_base_dropdown() {
    let f = fixture().await;
    let repo = f.repos.clone_with_origin("shop-web", "develop", 1);
    git(&repo, &["branch", "release/2.3"]);
    f.board.add_project(&add(&repo)).await.unwrap();
    let branches = f.board.branches("SHOP").await.unwrap();
    assert_eq!(branches.local, ["develop", "release/2.3"]);
    assert_eq!(branches.remote, ["origin/develop"]);
}

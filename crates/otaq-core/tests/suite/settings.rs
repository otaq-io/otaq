//! Settings (SET-01): defaults, per-project fallback, validation against a temporary repository, and that no
//! settings code writes into the repository.

use crate::common::Fixture;
use otaq_core::board::settings::{
    GlobalSettings, KeyboardChange, KeyboardSettings, ProjectSettings, Theme, ThemeSetting,
    WorkflowStep, resource_without_create,
};
use otaq_core::git::testing::{git, snapshot};

#[tokio::test]
async fn defaults() {
    let f = Fixture::shop().await;
    let global = f.board.global_settings().await.unwrap();
    assert!(!global.agent_limits.enabled);
    assert_eq!(
        (
            global.agent_limits.max_concurrency,
            global.agent_limits.back_pressure
        ),
        (5, 3)
    );
    assert!(global.notifications);
    assert!(!global.description_source_mode);
    assert_eq!(global.agent_panel_width, None);
    assert_eq!(global.keyboard, KeyboardSettings::default());
    assert_eq!(global.theme, ThemeSetting::default());
    let effective = f.board.effective_settings("SHOP").await.unwrap();
    assert_eq!(effective.project_notes, "");
    // The PR path runs Otaq's own skills, by their own names (AGM-07).
    for step in [WorkflowStep::OpenPr, WorkflowStep::Push] {
        assert_eq!(
            f.board.delivery_skill("SHOP", step).await.unwrap(),
            step.own_skill()
        );
    }

    let project = f.board.project_settings("SHOP").await.unwrap();
    assert!(project.let_finished_tickets_answer);
    assert_eq!(project, ProjectSettings::default());
    assert!(f.board.global_setting_problems().await.unwrap().is_empty());
    assert!(
        f.board
            .project_setting_problems("SHOP")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn global_values_round_trip_and_keep_other_rows() {
    let f = Fixture::shop().await;
    f.board
        .store()
        .set_setting("onboarding.permission_warning_dismissed", "true")
        .await
        .unwrap();
    let mut global = GlobalSettings::default();
    global.agent_limits.enabled = true;
    global.agent_limits.max_concurrency = 8;
    global.theme = ThemeSetting::Picked(Theme::TokyoNight);
    global.terminal_font_size = 15;
    global.notifications = false;
    global.description_source_mode = true;
    global.agent_panel_width = Some(560);
    global.keyboard.letters = false;
    global.keyboard.changes.push(KeyboardChange {
        action: "park".into(),
        number: None,
        slot: 0,
        key: Some("g p".into()),
        works_in_terminals: false,
    });
    f.board.set_global_settings(&global).await.unwrap();
    assert_eq!(f.board.global_settings().await.unwrap(), global);
    assert_eq!(
        f.board
            .store()
            .setting("onboarding.permission_warning_dismissed")
            .await
            .unwrap()
            .as_deref(),
        Some("true")
    );
}

#[tokio::test]
async fn a_0069_mode_and_two_themes_become_one_theme() {
    let f = Fixture::shop().await;
    let store = f.board.store();
    store.set_setting("appearance", "\"light\"").await.unwrap();
    store
        .set_setting("dark_theme", "\"tokyo-night\"")
        .await
        .unwrap();
    store.set_setting("light_theme", "\"nord\"").await.unwrap();
    store.set_setting("notifications", "false").await.unwrap();
    let global = f.board.global_settings().await.unwrap();
    // Light mode showed the light theme; `nord` is no light theme, so it reads as Otaq Light.
    assert_eq!(global.theme, ThemeSetting::Picked(Theme::OtaqLight));
    assert!(!global.notifications);

    // Saving stores `theme` and drops the old rows.
    f.board.set_global_settings(&global).await.unwrap();
    for key in ["appearance", "dark_theme", "light_theme"] {
        assert_eq!(store.setting(key).await.unwrap(), None, "{key}");
    }
    assert_eq!(
        store.setting("theme").await.unwrap().as_deref(),
        Some("\"otaq-light\"")
    );
    assert_eq!(f.board.global_settings().await.unwrap(), global);
}

#[tokio::test]
async fn a_stored_dollar_estimate_is_ignored() {
    // The setting was removed (d0086); a database from before still has its row.
    let f = Fixture::shop().await;
    let store = f.board.store();
    store.set_setting("dollar_estimate", "true").await.unwrap();
    store.set_setting("notifications", "false").await.unwrap();
    let global = f.board.global_settings().await.unwrap();
    assert!(!global.notifications);
    assert_eq!(
        global,
        GlobalSettings {
            notifications: false,
            ..Default::default()
        }
    );
}

#[tokio::test]
async fn a_0069_system_mode_waits_for_the_os() {
    let f = Fixture::shop().await;
    let store = f.board.store();
    store.set_setting("appearance", "\"system\"").await.unwrap();
    store
        .set_setting("dark_theme", "\"dracula\"")
        .await
        .unwrap();
    let global = f.board.global_settings().await.unwrap();
    assert_eq!(
        global.theme,
        ThemeSetting::ByOs {
            dark: Theme::Dracula,
            light: Theme::OtaqLight
        }
    );
}

#[tokio::test]
async fn project_values_round_trip_with_the_projects_notes() {
    let f = Fixture::shop().await;
    let effective = f.board.effective_settings("SHOP").await.unwrap();
    assert_eq!(effective.project_notes, "");

    let own = ProjectSettings {
        project_notes: "Use pnpm, not npm.".into(),
        let_finished_tickets_answer: false,
        ..Default::default()
    };
    f.board.set_project_settings("SHOP", &own).await.unwrap();
    assert_eq!(f.board.project_settings("SHOP").await.unwrap(), own);
    let effective = f.board.effective_settings("SHOP").await.unwrap();
    assert_eq!(effective.project_notes, "Use pnpm, not npm.");
}

#[tokio::test]
async fn invalid_values_are_stored_and_flagged_and_the_repo_is_untouched() {
    let f = Fixture::shop().await;
    let repo = std::path::PathBuf::from(f.board.project("SHOP").await.unwrap().repo_path);
    std::fs::write(repo.join(".env.local"), "A=1\n").unwrap();
    let before = (
        snapshot(&repo),
        git(&repo, &["status", "--porcelain", "--ignored"]),
    );

    let settings = ProjectSettings {
        env_files: vec![
            ".env.local".into(),
            ".env.missing".into(),
            "../secrets".into(),
        ],
        open_url: "localhost:3000".into(),
        setup_command: "npm install".into(),
        ..Default::default()
    };
    f.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    assert_eq!(
        f.board.project_settings("SHOP").await.unwrap(),
        settings,
        "stored as given"
    );
    let problems = f.board.project_setting_problems("SHOP").await.unwrap();
    let missing = &problems[0].message;
    assert!(
        missing.starts_with("Can\u{2019}t find .env.missing in ")
            && missing.ends_with(". Add it to the repo root, or remove it from this list."),
        "{missing}"
    );
    let fields: Vec<_> = problems
        .iter()
        .skip(1)
        .map(|p| (p.field.as_str(), p.message.as_str()))
        .collect();
    assert_eq!(
        fields,
        [
            (
                "env_files",
                "../secrets must be a path inside the repository, like .env.local"
            ),
            (
                "open_url",
                "localhost:3000 isn't a URL; use something like http://localhost:$PORT"
            ),
        ]
    );

    let mut global = GlobalSettings::default();
    global.agent_limits.max_concurrency = 0;
    global.terminal_font_size = 99;
    f.board.set_global_settings(&global).await.unwrap();
    let problems = f.board.global_setting_problems().await.unwrap();
    assert_eq!(problems.len(), 2);

    assert_eq!(
        (
            snapshot(&repo),
            git(&repo, &["status", "--porcelain", "--ignored"])
        ),
        before
    );
}

/// FLW-01 (d0108, no migration): Validation, Validation rounds and Auto-merge aren't settings any more. A value
/// stored before is ignored, and the settings written back don't carry it.
#[tokio::test]
async fn validation_and_auto_merge_are_no_settings() {
    let f = Fixture::shop().await;
    let pool = f.board.store().pool();
    for (key, value) in [
        ("validation", "false"),
        ("validation_rounds", "5"),
        ("auto_merge", "\"all\""),
    ] {
        sqlx::query("INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, 0)")
            .bind(key)
            .bind(value)
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query(
        "UPDATE projects SET settings_json = '{\"validation\": false, \"auto_merge\": \"all\"}' WHERE key = 'SHOP'",
    )
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(
        f.board.global_settings().await.unwrap(),
        GlobalSettings::default()
    );
    assert_eq!(
        f.board.project_settings("SHOP").await.unwrap(),
        ProjectSettings::default()
    );
    let global = serde_json::to_value(GlobalSettings::default()).unwrap();
    let project = serde_json::to_value(ProjectSettings::default()).unwrap();
    for field in ["validation", "validation_rounds", "auto_merge"] {
        assert!(global.get(field).is_none(), "{field}");
        assert!(project.get(field).is_none(), "{field}");
    }
    assert!(f.board.global_setting_problems().await.unwrap().is_empty());
}

/// Schema paths (M19-e, d0087): each must be a relative glob inside the repository; a Remove or Template refresh
/// command without a Create command is flagged too, and nothing is refused.
#[tokio::test]
async fn schema_path_problems() {
    let f = Fixture::shop().await;
    let repo = f.repos.root().join("shop-web");
    let before = snapshot(&repo);
    let settings = ProjectSettings {
        schema_paths: vec![
            "db/migrations/**".into(),
            "".into(),
            "/etc/schema.sql".into(),
            "../other/db/**".into(),
            "db/[0-9.sql".into(),
            "prisma/schema.prisma".into(),
        ],
        resource_remove: "dropdb x".into(),
        resource_template_refresh: "make template".into(),
        ..ProjectSettings::default()
    };
    f.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    assert_eq!(
        f.board.project_settings("SHOP").await.unwrap(),
        settings,
        "stored as written"
    );
    let problems = f.board.project_setting_problems("SHOP").await.unwrap();
    let of = |field: &str| -> Vec<String> {
        problems
            .iter()
            .filter(|p| p.field == field)
            .map(|p| p.message.clone())
            .collect()
    };
    let schema = of("schema_paths");
    assert_eq!(schema.len(), 4, "{schema:?}");
    assert_eq!(schema[0], "A schema path entry is empty");
    assert!(schema[1].starts_with("/etc/schema.sql must be a path inside"));
    assert!(schema[2].starts_with("../other/db/** must be a path inside"));
    assert!(schema[3].contains("[ has no matching ]"), "{}", schema[3]);
    assert_eq!(of("resource_remove"), [resource_without_create()]);
    assert_eq!(of("resource_template_refresh").len(), 1);

    // With a Create command, Remove and Refresh are fine.
    let settings = ProjectSettings {
        resource_create: "createdb x".into(),
        schema_paths: vec!["db/migrations/**".into()],
        ..settings
    };
    f.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    assert!(
        f.board
            .project_setting_problems("SHOP")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(snapshot(&repo), before, "the repository is untouched");
}

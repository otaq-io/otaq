//! A project's view preferences (SHB-05, T-0460): stored by the service in `ProjectSettings.view`, written with
//! `project_view.set`, read back after a restart; the Settings page's save never puts an older view back.

use otaq_core::board::settings::{
    BoardView, DONE_COLUMN, Dependencies, GroupBy, ProjectSettings, ViewPrefs,
};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::protocol::{
    ProjectParams, ProjectSettingsParams, ProjectViewParams, Reply, Request,
};
use otaq_core::service::testing::TestService;

async fn settings(client: &Client, project: &str) -> ProjectSettings {
    let Reply::ProjectSettings(reply) = client
        .request(Request::ProjectSettingsGet(ProjectParams {
            project: project.into(),
        }))
        .await
        .unwrap()
    else {
        panic!("project settings")
    };
    reply.settings
}

#[tokio::test]
async fn view_preferences_survive_a_service_restart() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.add_project("billing-api").await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    // A new project opens on the Board, by stage, Done folded.
    let fresh = settings(&client, "SHOP").await.view;
    assert_eq!(fresh, ViewPrefs::default());
    assert_eq!(fresh.view, BoardView::Board);
    assert_eq!(fresh.folded_columns, [DONE_COLUMN]);

    let view = ViewPrefs {
        view: BoardView::List,
        group_by: GroupBy::Status,
        fold_empty_columns: true,
        hide_empty_stages: true,
        dependencies: Dependencies::Always,
        folded_columns: vec!["grill".into()],
        folded_sections: vec!["delivery".into()],
        dev_unfolded: true,
    };
    let reply = client
        .request(Request::ProjectViewSet(ProjectViewParams {
            project: "SHOP".into(),
            view: view.clone(),
        }))
        .await
        .unwrap();
    assert_eq!(reply, Reply::Ok);
    // The Settings page saves what it read before the change: the view stays the board's.
    let mut page = fresh_settings_page();
    page.project_notes = "Use pnpm.".into();
    client
        .request(Request::ProjectSettingsSet(ProjectSettingsParams {
            project: "SHOP".into(),
            settings: Box::new(page),
        }))
        .await
        .unwrap();
    drop(client);

    let w = w.restart().await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let after = settings(&client, "SHOP").await;
    assert_eq!(after.view, view);
    assert_eq!(after.project_notes, "Use pnpm.");
    // Each project keeps its own.
    assert_eq!(settings(&client, "BILL").await.view, ViewPrefs::default());
    w.stop().await;
}

/// The Settings page's copy of the settings, read before the board's change.
fn fresh_settings_page() -> ProjectSettings {
    ProjectSettings::default()
}

#[tokio::test]
async fn an_unreadable_view_value_falls_back_alone() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    // A stored view with a value this version doesn't know: the rest stays.
    sqlx::query("UPDATE projects SET settings_json = ? WHERE id = ?")
        .bind(r#"{"view": {"view": "list", "group_by": "swimlanes", "fold_empty_columns": true}}"#)
        .bind(&project.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let view = w.board.project_settings("SHOP").await.unwrap().view;
    assert_eq!(view.view, BoardView::List);
    assert_eq!(view.group_by, GroupBy::Stage);
    assert!(view.fold_empty_columns);
    w.stop().await;
}

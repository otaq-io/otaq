//! `noyan_settings.*`: Settings → Noyan (UX-70, refined 2026-11-16; canvas A-8), app settings' own section right
//! after Tools. `noyan_settings.view` answers its page, "Noyan starts on" (the `noyan.starts_on` setting, drawn with
//! the flows' model-and-effort pairs on top of its menu); `noyan_settings.edit` changes it (one edit of the field,
//! stored as `noyan.set_starts_on`; the next new Noyan conversation starts on it) and answers the fresh page. The
//! service's `agents_changed` (Noyan starts on), `agent_tools_changed` (a tool set up), `flow_changed` and
//! `projects_changed` (the pairs) ask `noyan_settings.view` again. The words are
//! `otaq_view::view_models::noyan_settings`'s.

use otaq_core::protocol::{Reply, Request};
use otaq_view::view_models::agent_tools_page::Flows;
use otaq_view::view_models::noyan_settings::{self as vm, Edit, Page};
use serde_json::Value;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "noyan_settings.view" => reply(&page(app).await),
        "noyan_settings.edit" => {
            let edit: Edit = serde_json::from_value(params)
                .map_err(|e| RequestError::plain(e.to_string()))?;
            let on = super::settings::noyan_starts_on(app).await;
            let next = vm::edit(&on, &edit).map_err(RequestError::plain)?;
            match app.service.send(Request::NoyanSetStartsOn(next)).await? {
                Reply::NoyanStartsOn(_) => {}
                other => {
                    return Err(RequestError::plain(format!(
                        "unexpected answer to noyan.set_starts_on: {other:?}"
                    )));
                }
            }
            reply(&page(app).await)
        }
        _ => Err(unknown(method)),
    }
}

/// The page, from the Noyan setting, the tools and every project's flow in use.
async fn page(app: &App) -> Page {
    let (tools, noyan, uses) = super::agent_tools::flows(app).await;
    vm::page(&Flows {
        noyan: &noyan,
        uses: &uses,
        tools: &tools,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    /// Settings → Noyan (UX-70): Noyan starts on, and changing it changes what the next Noyan conversation starts on.
    #[tokio::test(flavor = "multi_thread")]
    async fn noyan_starts_on_changes_the_next_conversation() {
        use otaq_core::board::projects::AddProject;
        use otaq_core::git::testing::TempRepos;

        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("pocket-notes", 1);
        let project = match h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
            }))
            .await
            .unwrap()
        {
            Reply::ProjectAdded { project: p, .. } => p.key,
            other => panic!("{other:?}"),
        };

        let view = request(&h.app, "noyan_settings.view", json!({}))
            .await
            .unwrap();
        assert_eq!(view["title"], "Noyan");
        assert_eq!(view["label"], "Noyan starts on");
        assert_eq!(
            view["picker"]["hint"],
            "For new conversations. Switch for one conversation in the Noyan panel."
        );

        // Noyan starts on Codex · medium now: the page and the setting say so.
        let view = request(
            &h.app,
            "noyan_settings.edit",
            json!({ "edit": "pair", "model": "codex", "effort": "medium" }),
        )
        .await
        .unwrap();
        assert_eq!(view["picker"]["value"]["text"], "Codex \u{b7} default");
        match h.app.service.send(Request::NoyanStartsOn).await.unwrap() {
            Reply::NoyanStartsOn(on) => {
                assert_eq!(on.model.as_deref(), Some("codex"));
                assert_eq!(on.effort.as_deref(), Some("medium"));
            }
            other => panic!("{other:?}"),
        }
        // The next Noyan conversation starts on it.
        let panel = request(&h.app, "agent_panel.view", json!({ "project": project }))
            .await
            .unwrap();
        assert_eq!(panel["empty"]["line"]["agent"], "Codex");
        // An effort alone keeps the model; an effort the tool doesn't have is refused by the service.
        let view = request(&h.app, "noyan_settings.edit", json!({ "edit": "effort" }))
            .await
            .unwrap();
        assert_eq!(view["picker"]["value"]["text"], "Codex \u{b7} default");
        assert!(
            request(
                &h.app,
                "noyan_settings.edit",
                json!({ "edit": "effort", "effort": "turbo" })
            )
            .await
            .is_err()
        );
        h.service.stop_all().await;
    }
}

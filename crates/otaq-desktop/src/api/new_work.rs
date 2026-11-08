//! `new_work.*`: the box for new work at the bottom of the board and the List, where a ticket or words for Noyan
//! start (`new_work.rs` in `otaq-view`).
//! `new_work.view` answers what the box says and whether it shows. What it sends are other areas' requests: ↵ is
//! `editor.save` on a new ticket, ⌘↵ `editor.create_and_start`, ⇧↵ `agent_panel.send` (a conversation on Noyan's
//! agent, which only writes tickets to Backlog).

use otaq_view::view_models::new_work::{self as vm, NewWorkView};
use serde_json::Value;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, _params: Value) -> Result<Value, RequestError> {
    match method {
        "new_work.view" => reply(&view(app).await),
        _ => Err(unknown(method)),
    }
}

pub async fn view(app: &App) -> NewWorkView {
    let keys = super::keymap::labels(app).await;
    // Not on a new user's board: the box appears once the first ticket was approved.
    vm::view(&vm::Facts {
        keys: &keys,
        shown: super::onboarding::first_approve(app).await,
    })
}

#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    use ts_rs::TS;
    NewWorkView::export_all(cfg)?;
    vm::Gallery::export_all(cfg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    /// The box is hidden on a new user's board and shows once a ticket was approved, with the user's New ticket key.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_box_shows_after_the_first_approve() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let view = table(&h.app, "new_work.view", json!({})).await.unwrap();
        assert_eq!(view["shown"], false, "a new user");
        h.set_setting(otaq_core::store::settings::FIRST_APPROVE, "true")
            .await;
        let view = table(&h.app, "new_work.view", json!({})).await.unwrap();
        assert_eq!(view["shown"], true);
        assert_eq!(view["placeholder"], "New ticket, or ask Noyan\u{2026}");
        assert_eq!(view["create"]["label"], "Create");
        h.service.stop_all().await;
    }
}

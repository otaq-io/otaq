//! `layout.*`: the window's layout (SHB-01, T-0327, T-0458, T-0502): the size classes, the rail's width and the
//! Project agent panel's width with its drag bounds. The page asks at start; `layout.agent_panel_width` keeps a
//! dragged width in the global settings (d0023), so it survives a restart, and answers the new view.

use otaq_core::board::settings::GlobalSettings;
use otaq_core::protocol::{Reply, Request};
use otaq_view::view_models::layout;
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

/// What `layout.agent_panel_width` takes: the width where the drag of the panel's edge ended, or `null` for the
/// double-click (back to the default).
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[ts(rename = "LayoutAgentPanelWidthParams")]
pub struct AgentPanelWidthParams {
    pub width: Option<f32>,
}

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "layout.view" => {
            let settings = global_settings(app).await?;
            reply(&layout::view(settings.agent_panel_width))
        }
        "layout.agent_panel_width" => {
            let params: AgentPanelWidthParams =
                serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))?;
            let mut settings = global_settings(app).await?;
            let stored = layout::stored_agent_panel_width(params.width);
            if settings.agent_panel_width != stored {
                settings.agent_panel_width = stored;
                app.service.send(Request::SettingsSet(settings)).await?;
            }
            reply(&layout::view(stored))
        }
        _ => Err(unknown(method)),
    }
}

async fn global_settings(app: &App) -> Result<GlobalSettings, RequestError> {
    match app.service.send(Request::SettingsGet).await? {
        Reply::GlobalSettings(r) => Ok(r.settings),
        other => Err(RequestError::plain(format!(
            "unexpected answer to settings.get: {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    #[tokio::test(flavor = "multi_thread")]
    async fn the_table_answers_layout_view_for_this_platform() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let view = request(&h.app, "layout.view", Value::Null).await.unwrap();
        assert_eq!(view, serde_json::to_value(layout::view(None)).unwrap());
        assert_eq!(view["agent_panel"], 420.0);
        assert_eq!(view["agent_panel_min"], 320.0);
        assert_eq!(view["classes"][0]["class"], "w1280");
        assert!(request(&h.app, "layout.nothing", Value::Null).await.is_err());
        h.service.stop_all().await;
    }

    /// T-0502: a dragged width is kept in the global settings (so a restart, which asks `layout.view` again, gets it
    /// back), and the double-click forgets it.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_dragged_panel_width_is_kept_until_the_double_click() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let set = |width: Value| request(&h.app, "layout.agent_panel_width", json!({ "width": width }));
        let view = set(json!(612.6)).await.unwrap();
        assert_eq!(view["agent_panel"], 613.0);
        assert_eq!(global_settings(&h.app).await.unwrap().agent_panel_width, Some(613));
        let view = request(&h.app, "layout.view", Value::Null).await.unwrap();
        assert_eq!(view["agent_panel"], 613.0);
        // Narrower than the minimum keeps the minimum.
        assert_eq!(set(json!(100)).await.unwrap()["agent_panel"], 320.0);
        let view = set(Value::Null).await.unwrap();
        assert_eq!(view["agent_panel"], 420.0);
        assert_eq!(global_settings(&h.app).await.unwrap().agent_panel_width, None);
        assert!(request(&h.app, "layout.agent_panel_width", json!({ "width": "wide" })).await.is_err());
        h.service.stop_all().await;
    }
}

//! `layout.*`: the window's layout (SHB-01, T-0327, T-0458, T-0502): the size classes, the rail's width and the
//! dragged widths with their bounds: the Project agent panel's, the ticket sheet's and the Files tree's. The page asks
//! at start; `layout.agent_panel_width`, `layout.sheet_width` and `layout.files_tree_width` keep a dragged width in
//! the global settings (d0023), so it survives a restart, and answer the new view.

use otaq_core::board::settings::GlobalSettings;
use otaq_core::protocol::{Reply, Request};
use otaq_view::view_models::layout;
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

/// What `layout.agent_panel_width`, `layout.sheet_width` and `layout.files_tree_width` take: the width where the
/// drag of the edge ended, or `null` for the double-click (back to the default).
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[ts(rename = "LayoutWidthParams")]
pub struct WidthParams {
    pub width: Option<f32>,
}

/// A dragged width's rule (`layout::stored_*_width`) and its field in the global settings.
type KeptWidth = (
    fn(Option<f32>) -> Option<u32>,
    fn(&mut GlobalSettings) -> &mut Option<u32>,
);

/// The dragged widths in `settings`.
fn kept(settings: &GlobalSettings) -> layout::KeptWidths {
    layout::KeptWidths {
        agent_panel: settings.agent_panel_width,
        sheet: settings.sheet_width,
        files_tree: settings.files_tree_width,
    }
}

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    let (stored, field): KeptWidth =
        match method {
            "layout.view" => {
                let settings = global_settings(app).await?;
                return reply(&layout::view(kept(&settings)));
            }
            "layout.agent_panel_width" => (layout::stored_agent_panel_width, |s| &mut s.agent_panel_width),
            "layout.sheet_width" => (layout::stored_sheet_width, |s| &mut s.sheet_width),
            "layout.files_tree_width" => (layout::stored_files_tree_width, |s| &mut s.files_tree_width),
            _ => return Err(unknown(method)),
        };
    let params: WidthParams = serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))?;
    let mut settings = global_settings(app).await?;
    let width = stored(params.width);
    if *field(&mut settings) != width {
        *field(&mut settings) = width;
        app.service.send(Request::SettingsSet(settings.clone())).await?;
    }
    reply(&layout::view(kept(&settings)))
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
        assert_eq!(view, serde_json::to_value(layout::view(layout::KeptWidths::default())).unwrap());
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

    /// The sheet's and the Files tree's dragged widths are kept the same way, each in its own setting.
    #[tokio::test(flavor = "multi_thread")]
    async fn dragged_sheet_and_files_tree_widths_are_kept_until_the_double_click() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let set = |method: &'static str, width: Value| request(&h.app, method, json!({ "width": width }));
        assert_eq!(set("layout.sheet_width", json!(780.2)).await.unwrap()["sheet"], 780.0);
        assert_eq!(set("layout.files_tree_width", json!(360)).await.unwrap()["files_tree"], 360.0);
        let settings = global_settings(&h.app).await.unwrap();
        assert_eq!((settings.sheet_width, settings.files_tree_width), (Some(780), Some(360)));
        let view = request(&h.app, "layout.view", Value::Null).await.unwrap();
        assert_eq!((&view["sheet"], &view["files_tree"]), (&json!(780.0), &json!(360.0)));
        assert_eq!(set("layout.sheet_width", Value::Null).await.unwrap()["sheet"], Value::Null);
        let view = set("layout.files_tree_width", Value::Null).await.unwrap();
        assert_eq!(view["files_tree"], Value::Null);
        assert_eq!(view["sheet"], Value::Null);
        h.service.stop_all().await;
    }
}

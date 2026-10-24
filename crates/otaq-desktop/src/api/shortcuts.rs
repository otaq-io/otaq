//! `shortcuts.*`: the `?` shortcut list (KEY-07, d0097; `otaq_view::view_models::shortcut_list`). The page asks when
//! it opens and again on every `settings_changed`, so a key changed in Settings shows at once.

use otaq_view::keymap::{self, Platform};
use otaq_view::keymap_edit;
use otaq_view::view_models::shortcut_list::{self, ShortcutList};
use serde_json::Value;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, _params: Value) -> Result<Value, RequestError> {
    match method {
        "shortcuts.list" => {
            // Without the service the base map, letters on, as `keymap.map` answers.
            let keyboard = super::keyboard::global_settings(app)
                .await
                .map(|s| s.keyboard)
                .unwrap_or_default();
            reply(&list(
                Platform::current(),
                &keymap_edit::custom_keys(&keyboard),
                keyboard.letters,
            ))
        }
        _ => Err(unknown(method)),
    }
}

/// The list for these keys and the letters switch.
pub fn list(platform: Platform, custom: &keymap::CustomKeys, letters: bool) -> ShortcutList {
    shortcut_list::shortcut_list(&keymap::user_keys(platform, custom), letters)
}

#[cfg(test)]
mod tests {
    use otaq_core::board::settings::KeyboardChange;
    use otaq_core::protocol::{Reply, Request};

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    /// The stored keys and switch: Activity on ⌘J shows "changed", Next item that needs you (which lost ⌘J) leaves the list,
    /// and with the letters off a letter-only row says "off".
    #[tokio::test(flavor = "multi_thread")]
    async fn the_list_follows_the_stored_keys() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let before = request(&h.app, "shortcuts.list", Value::Null).await.unwrap();
        assert_eq!(before["title"], "Keyboard shortcuts");
        assert_eq!(
            before,
            serde_json::to_value(list(Platform::current(), &Default::default(), true)).unwrap()
        );
        let Reply::GlobalSettings(reply) = h.app.service.send(Request::SettingsGet).await.unwrap()
        else {
            panic!()
        };
        let mut settings = reply.settings;
        settings.keyboard.letters = false;
        settings.keyboard.changes = vec![
            KeyboardChange {
                action: "open_activity".into(),
                number: None,
                slot: 0,
                key: Some(if cfg!(target_os = "macos") { "⌘J" } else { "Ctrl+J" }.into()),
                works_in_terminals: false,
            },
            KeyboardChange {
                action: "next_needs_you_item".into(),
                number: None,
                slot: 0,
                key: None,
                works_in_terminals: false,
            },
        ];
        h.app
            .service
            .send(Request::SettingsSet(settings))
            .await
            .unwrap();
        let after = request(&h.app, "shortcuts.list", Value::Null).await.unwrap();
        let rows: Vec<&Value> = after["columns"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|c| c.as_array().unwrap())
            .flat_map(|g| g["rows"].as_array().unwrap())
            .collect();
        let row = |title: &str| rows.iter().find(|r| r["title"] == title).copied();
        assert_eq!(row("Activity").unwrap()["mark"], "changed");
        assert!(row("Next item that needs you").is_none());
        assert_eq!(row("Filter the board").unwrap()["mark"], "off");
        assert!(after["note"].as_str().unwrap().contains("are off"));
        assert!(request(&h.app, "shortcuts.nothing", Value::Null).await.is_err());
        h.service.stop_all().await;
    }
}

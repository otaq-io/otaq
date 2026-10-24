//! `keymap.*`: the user's keys (d0097, SDD §21.3) for the page's key dispatcher (T-0215). The page asks at start and
//! again on every `settings_changed` event (as the theme is), so a key changed in Settings applies at once.

use otaq_core::naming;
use otaq_core::board::settings::KeyboardSettings;
use otaq_view::key_labels::KeyLabels;
use otaq_view::keymap::{
    self, ActionInfo, Binding, CustomKeys, Modifiers, Platform, SequenceHint, TerminalBinding,
};
use otaq_view::keymap_edit;
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, _params: Value) -> Result<Value, RequestError> {
    match method {
        "keymap.map" => reply(&Keymap::stored(
            Platform::current(),
            &stored_keyboard(app).await,
            key_log(),
        )),
        _ => Err(unknown(method)),
    }
}

/// The stored custom keys and letters switch (T-0390). Without the service the base map, letters on, until the page
/// asks again on the reconnect's resync.
async fn stored_keyboard(app: &App) -> KeyboardSettings {
    super::keyboard::global_settings(app)
        .await
        .map(|s| s.keyboard)
        .unwrap_or_default()
}

/// The user's keys for a view's labels (KEY-08): every view that names a key builds them here, and the page asks for
/// it again on `settings_changed`, as for the map.
pub async fn labels(app: &App) -> KeyLabels {
    KeyLabels::stored(Platform::current(), &stored_keyboard(app).await)
}

/// The map on this platform: the user's keys with their actions and where each works, every action's title and
/// group, and which modifiers take any key from a focused terminal to the app ([`keymap::route`]'s rule, as data
/// the page applies to each key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Keymap {
    pub platform: Platform,
    /// The user's keys ([`keymap::user_keys`]): the base map with their changes.
    pub bindings: Vec<Binding>,
    /// The single-letter switch (KEY-02): off, no single letter or `g` sequence runs, built-in or custom.
    pub letters: bool,
    /// Every action in the order Settings and the `?` list show them, with its title and group.
    pub actions: Vec<ActionInfo>,
    /// The `g` hint's second keys ([`keymap::sequence_hint`]), from the user's keys.
    pub sequences: Vec<SequenceHint>,
    /// The hint while a fast burst of letters runs nothing ([`keymap::TYPING_HINT`], d0126).
    pub typing_hint: &'static str,
    pub terminal_app_modifiers: Vec<Modifiers>,
    /// A focused terminal's own keys (copy, paste, font size), which the terminal handles itself (T-0216).
    pub terminal_keys: Vec<TerminalBinding>,
    /// Whether the page logs every key it gets and where it went, for checking the map by hand. Set by starting the
    /// app with `OTAQ_KEY_LOG=1`.
    pub log: bool,
}

impl Keymap {
    pub fn new(platform: Platform, custom: &CustomKeys, letters: bool, log: bool) -> Self {
        let bindings = keymap::user_keys(platform, custom);
        Self {
            platform,
            sequences: keymap::sequence_hint(&bindings),
            bindings,
            typing_hint: keymap::TYPING_HINT,
            letters,
            actions: keymap::action_infos(),
            terminal_app_modifiers: keymap::terminal_app_modifiers(platform).to_vec(),
            terminal_keys: keymap::terminal_bindings(platform),
            log,
        }
    }

    /// The map for the stored keyboard settings.
    pub fn stored(platform: Platform, keyboard: &KeyboardSettings, log: bool) -> Self {
        Self::new(
            platform,
            &keymap_edit::custom_keys(keyboard),
            keyboard.letters,
            log,
        )
    }
}

fn key_log() -> bool {
    std::env::var(naming::env_var("KEY_LOG")).is_ok_and(|v| !v.is_empty() && v != "0")
}

#[cfg(test)]
mod tests {
    use otaq_core::protocol::{Reply, Request};
    use otaq_view::keymap::{CustomKey, Key, KeyAction, KeyChange};
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    #[tokio::test(flavor = "multi_thread")]
    async fn the_users_keys_answer_keymap_map_for_this_platform() {
        let h = Harness::new(None);
        let map = request(&h.app, "keymap.map", Value::Null).await.unwrap();
        assert_eq!(
            map,
            serde_json::to_value(Keymap::new(
                Platform::current(),
                &CustomKeys::default(),
                true,
                key_log()
            ))
            .unwrap()
        );
        assert!(request(&h.app, "keymap.nothing", Value::Null).await.is_err());
        h.service.stop_all().await;
    }

    /// The page asks again on `settings_changed` (`ui/src/app/keyboard.ts`): a settings change announces it, and the
    /// map answers after it as before (T-0390 makes the answer follow the stored keys).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_settings_change_is_announced_and_the_map_answers_after_it() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let before = request(&h.app, "keymap.map", Value::Null).await.unwrap();
        let Reply::GlobalSettings(reply) = h.app.service.send(Request::SettingsGet).await.unwrap()
        else {
            panic!()
        };
        h.app
            .service
            .send(Request::SettingsSet(reply.settings))
            .await
            .unwrap();
        h.wait_for_event("settings_changed", |e| {
            e["kind"] == "service" && e["name"] == "settings_changed"
        })
        .await;
        let after = request(&h.app, "keymap.map", Value::Null).await.unwrap();
        assert_eq!(after, before);
        h.service.stop_all().await;
    }

    #[test]
    fn the_page_gets_the_bindings_the_letters_switch_and_the_titles() {
        let map = serde_json::to_value(Keymap::new(
            Platform::WinLinux,
            &CustomKeys::default(),
            true,
            false,
        ))
        .unwrap();
        assert_eq!(map["platform"], "win_linux");
        assert_eq!(map["log"], false);
        assert_eq!(map["letters"], true);
        assert_eq!(
            map["terminal_app_modifiers"][0],
            json!({ "control": true, "alt": false, "shift": true, "platform": false, "function": false })
        );
        let bindings = map["bindings"].as_array().unwrap();
        let of = |action: Value| -> Vec<&Value> {
            bindings.iter().filter(|b| b["action"] == action).collect()
        };
        let palette = of(json!({ "action": "command_palette" }));
        assert_eq!(palette[0]["text"], "Ctrl+K");
        assert_eq!(palette[0]["place"], "everywhere");
        assert_eq!(palette[0]["key"]["stroke"]["code"], "KeyK");
        let new_ticket = of(json!({ "action": "new_ticket" }));
        let shown: Vec<(&Value, &Value, &Value)> = new_ticket
            .iter()
            .map(|b| (&b["slot"], &b["text"], &b["place"]))
            .collect();
        assert_eq!(
            shown,
            [
                (&json!(0), &json!("Ctrl+N"), &json!("outside_terminals")),
                (&json!(0), &json!("Ctrl+Shift+N"), &json!("inside_terminals")),
                (&json!(1), &json!("c"), &json!("outside_terminals")),
            ]
        );
        assert_eq!(new_ticket[2]["key"], json!({ "kind": "letter", "letter": "c" }));
        // FIL-11: Files is Ctrl+Shift+O (in terminals too) and g f; Go to file Ctrl+Alt+P here, Ctrl+Shift+Alt+P in
        // a terminal.
        let files: Vec<Value> = of(json!({ "action": "show_files" }))
            .iter()
            .map(|b| b["text"].clone())
            .collect();
        assert_eq!(files, [json!("Ctrl+Shift+O"), json!("g f")]);
        let search: Vec<Value> = of(json!({ "action": "go_to_file" }))
            .iter()
            .map(|b| b["text"].clone())
            .collect();
        assert_eq!(search, [json!("Ctrl+Alt+P"), json!("Ctrl+Shift+Alt+P")]);
        assert_eq!(map["terminal_keys"][0]["key"], "copy");
        assert_eq!(map["terminal_keys"][0]["keystroke"]["code"], "KeyC");
        assert_eq!(
            map["actions"][0],
            json!({ "action": { "action": "command_palette" }, "title": "Command palette", "group": "moving_around" })
        );

        let mac = Keymap::new(Platform::Mac, &CustomKeys::default(), false, true);
        assert!(mac.log);
        assert!(!mac.letters);
        assert_eq!(
            mac.bindings
                .iter()
                .find(|b| b.action == KeyAction::Project(3))
                .map(|b| b.text.as_str()),
            Some("⌘3")
        );
        // The user's keys, not the table's.
        let custom = CustomKeys(vec![CustomKey {
            action: KeyAction::Park,
            slot: 0,
            change: KeyChange::Replaced {
                key: Key::parse("g p").unwrap(),
            },
            works_in_terminals: false,
        }]);
        let mine = Keymap::new(Platform::Mac, &custom, true, false);
        let park: Vec<&str> = mine
            .bindings
            .iter()
            .filter(|b| b.action == KeyAction::Park)
            .map(|b| b.text.as_str())
            .collect();
        assert_eq!(park, ["g p"]);
    }
}

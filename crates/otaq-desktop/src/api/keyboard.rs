//! `keyboard.*`: custom shortcuts and the single-letter switch (d0097, KEY-04, KEY-02). `keyboard.edit` takes one
//! edit, checks it (`otaq_view::keymap_edit`: taken, refused, the warning) and stores it only when it is done; the
//! service then announces `settings_changed`, on which the page asks for `keymap.map` again. The answer says what
//! the Settings page shows in place: done (with how many keys are changed), the question, the reason or the warning.
//! Settings → Keyboard shortcuts (T-0395) also asks for its page (`keyboard.view`), records a pressed key as the
//! map's text (`keyboard.record`, so the page never formats one) and resets a row's keys at once
//! (`keyboard.reset_row`); each stores only when done, as `keyboard.edit` does.

use otaq_core::board::settings::GlobalSettings;
use otaq_core::protocol::{Reply, Request};
use otaq_view::keymap::Platform;
use otaq_view::keymap_edit::{self, KeyEdit, KeyEditAnswer};
use otaq_view::view_models::keyboard_page::{self, KeySlot, Press, RecordAnswer};
use serde::Deserialize;
use serde_json::Value;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "keyboard.edit" => {
            let edit: KeyEdit =
                serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))?;
            reply(&edit_keys(app, Platform::current(), &edit).await?)
        }
        "keyboard.view" => reply(&keyboard_page::page(
            Platform::current(),
            &global_settings(app).await?.keyboard,
        )),
        "keyboard.record" => {
            let p: RecordParams = parse(params)?;
            reply(&record(app, Platform::current(), &p).await?)
        }
        "keyboard.reset_row" => {
            let p: ResetRowParams = parse(params)?;
            reply(&reset_row(app, Platform::current(), &p).await?)
        }
        _ => Err(unknown(method)),
    }
}

/// The stored settings. A keyboard edit changes only `keyboard`, read just before it is written, as every other
/// page's edit does with its own field.
pub async fn global_settings(app: &App) -> Result<GlobalSettings, RequestError> {
    match app.service.send(Request::SettingsGet).await? {
        Reply::GlobalSettings(reply) => Ok(reply.settings),
        other => Err(RequestError::plain(format!(
            "unexpected answer to settings.get: {other:?}"
        ))),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

/// `keyboard.record`: a key pressed while a cell records, `after_g` when it follows a lone `g`.
#[derive(Deserialize)]
struct RecordParams {
    #[serde(flatten)]
    slot: KeySlot,
    #[serde(flatten)]
    press: Press,
    #[serde(default)]
    after_g: bool,
}

/// `keyboard.reset_row`: the row's changed keys; `confirmed` once the user said "Use here".
#[derive(Deserialize)]
struct ResetRowParams {
    keys: Vec<KeySlot>,
    #[serde(default)]
    confirmed: bool,
}

async fn store(app: &App, keyboard: Option<otaq_core::board::settings::KeyboardSettings>) -> Result<(), RequestError> {
    if let Some(keyboard) = keyboard {
        let mut settings = global_settings(app).await?;
        settings.keyboard = keyboard;
        app.service.send(Request::SettingsSet(settings)).await?;
    }
    Ok(())
}

async fn record(app: &App, platform: Platform, p: &RecordParams) -> Result<RecordAnswer, RequestError> {
    let stored = global_settings(app).await?.keyboard;
    let (answer, keyboard) = keyboard_page::record(platform, &stored, p.slot, &p.press, p.after_g)
        .map_err(RequestError::plain)?;
    store(app, keyboard).await?;
    Ok(answer)
}

async fn reset_row(app: &App, platform: Platform, p: &ResetRowParams) -> Result<KeyEditAnswer, RequestError> {
    let stored = global_settings(app).await?.keyboard;
    let edited = keyboard_page::reset_row(platform, &stored, &p.keys, p.confirmed).map_err(RequestError::plain)?;
    store(app, edited.store).await?;
    Ok(edited.answer)
}

async fn edit_keys(
    app: &App,
    platform: Platform,
    edit: &KeyEdit,
) -> Result<KeyEditAnswer, RequestError> {
    let mut settings = global_settings(app).await?;
    let edited =
        keymap_edit::edit(platform, &settings.keyboard, edit).map_err(RequestError::plain)?;
    if let Some(keyboard) = edited.store {
        settings.keyboard = keyboard;
        app.service.send(Request::SettingsSet(settings)).await?;
    }
    Ok(edited.answer)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    fn settings_changed(e: &Value) -> bool {
        e["kind"] == "service" && e["name"] == "settings_changed"
    }

    /// The keys a `keymap.map` answer binds for `action`, outside terminals.
    fn texts(map: &Value, action: Value) -> Vec<String> {
        map["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["action"] == action && b["place"] != "inside_terminals")
            .map(|b| b["text"].as_str().unwrap().to_owned())
            .collect()
    }

    /// KEY-04 end to end: an edit is checked, stored only when done, announced, and `keymap.map` binds the new keys;
    /// the letters switch reaches the map; other settings are kept.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_edit_is_stored_announced_and_the_map_shows_the_new_keys() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let mut before = global_settings(&h.app).await.unwrap();
        before.terminal_font_size = 17;
        h.app
            .service
            .send(Request::SettingsSet(before.clone()))
            .await
            .unwrap();
        let platform = Platform::current();
        let needs_you_key = if platform == Platform::Mac {
            "⌘J"
        } else {
            "Ctrl+J"
        };

        // Taken: the question, nothing stored.
        let edit = json!({
            "edit": "record", "action": { "action": "open_activity" }, "slot": 0, "key": needs_you_key,
        });
        let asked = request(&h.app, "keyboard.edit", edit.clone()).await.unwrap();
        assert_eq!(asked["outcome"], "taken");
        assert_eq!(
            asked["question"],
            format!("{needs_you_key} is used by Next item that needs you. Use it here instead?")
        );
        assert_eq!(global_settings(&h.app).await.unwrap(), before);

        // Use here: stored and announced; the map follows.
        let mut confirmed = edit;
        confirmed["confirmed"] = json!(true);
        let done = request(&h.app, "keyboard.edit", confirmed).await.unwrap();
        assert_eq!(done, json!({ "outcome": "done", "changed": 2 }));
        h.wait_for_event("settings_changed", settings_changed).await;
        let map = request(&h.app, "keymap.map", Value::Null).await.unwrap();
        assert_eq!(
            texts(&map, json!({ "action": "open_activity" })),
            [needs_you_key, "g a"]
        );
        assert!(texts(&map, json!({ "action": "next_needs_you_item" })).is_empty());

        // Refused: the reason, nothing stored.
        let quit = if platform == Platform::Mac {
            "⌘Q"
        } else {
            "Alt+F4"
        };
        let refused = request(
            &h.app,
            "keyboard.edit",
            json!({ "edit": "record", "action": { "action": "park" }, "slot": 0, "key": quit }),
        )
        .await
        .unwrap();
        assert_eq!(refused["outcome"], "refused");

        // The letters switch, then Reset all.
        let off = request(&h.app, "keyboard.edit", json!({ "edit": "letters", "on": false }))
            .await
            .unwrap();
        assert_eq!(off, json!({ "outcome": "done", "changed": 2 }));
        let map = request(&h.app, "keymap.map", Value::Null).await.unwrap();
        assert_eq!(map["letters"], false);
        request(&h.app, "keyboard.edit", json!({ "edit": "reset_all" }))
            .await
            .unwrap();
        let after = global_settings(&h.app).await.unwrap();
        assert_eq!(after.keyboard.changes, []);
        assert!(!after.keyboard.letters);
        assert_eq!(after.terminal_font_size, 17, "other settings are kept");
        let map = request(&h.app, "keymap.map", Value::Null).await.unwrap();
        assert_eq!(
            texts(&map, json!({ "action": "next_needs_you_item" })),
            [needs_you_key]
        );

        // An edit the page can't send is an error.
        assert!(
            request(
                &h.app,
                "keyboard.edit",
                json!({ "edit": "remove", "action": { "action": "park" }, "slot": 4 }),
            )
            .await
            .is_err()
        );
        assert!(request(&h.app, "keyboard.nothing", Value::Null).await.is_err());
        h.service.stop_all().await;
    }

    /// The Settings page's own requests (T-0395): the page, a pressed key recorded as the map's text (a lone `g`
    /// waits, `g` then `p` is stored), a taken key asks, a row's reset gives the key back.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_page_records_a_pressed_key_and_resets_a_row() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let mac = Platform::current() == Platform::Mac;
        let page = request(&h.app, "keyboard.view", Value::Null).await.unwrap();
        assert_eq!(page["reset_all"]["enabled"], false);
        assert_eq!(page["letters"]["on"], true);

        let park = json!({ "action": { "action": "park" }, "slot": 0 });
        let press = |key: &str, code: &str, platform: bool, after_g: bool| {
            let mut p = park.clone();
            p["key"] = json!(key);
            p["code"] = json!(code);
            p["modifiers"] = json!({
                "control": false, "alt": false, "shift": false, "platform": platform, "function": false,
            });
            p["after_g"] = json!(after_g);
            p
        };
        let g = request(&h.app, "keyboard.record", press("g", "KeyG", false, false))
            .await
            .unwrap();
        assert_eq!(g, json!({ "outcome": "then_letter" }));
        let gp = request(&h.app, "keyboard.record", press("p", "KeyP", false, true))
            .await
            .unwrap();
        assert_eq!(
            gp,
            json!({ "outcome": "edited", "key": "g p", "answer": { "outcome": "done", "changed": 1 } })
        );
        h.wait_for_event("settings_changed", settings_changed).await;
        let page = request(&h.app, "keyboard.view", Value::Null).await.unwrap();
        assert_eq!(page["reset_all"]["enabled"], true);

        // Activity ⌘J / Ctrl+J: the question; Use here through `keyboard.edit`; Next item that needs you's reset asks back.
        let mut activity = press("j", "KeyJ", mac, false);
        activity["action"] = json!({ "action": "open_activity" });
        if !mac {
            activity["modifiers"]["control"] = json!(true);
        }
        let asked = request(&h.app, "keyboard.record", activity).await.unwrap();
        assert_eq!(asked["outcome"], "edited");
        assert_eq!(asked["answer"]["outcome"], "taken");
        let key = asked["key"].clone();
        request(
            &h.app,
            "keyboard.edit",
            json!({ "edit": "record", "action": { "action": "open_activity" }, "slot": 0, "key": key, "confirmed": true }),
        )
        .await
        .unwrap();
        let back = json!({ "keys": [{ "action": { "action": "next_needs_you_item" }, "slot": 0 }] });
        let taken = request(&h.app, "keyboard.reset_row", back.clone()).await.unwrap();
        assert_eq!(taken["outcome"], "taken");
        let mut confirmed = back;
        confirmed["confirmed"] = json!(true);
        let done = request(&h.app, "keyboard.reset_row", confirmed).await.unwrap();
        assert_eq!(done["outcome"], "done");
        let map = request(&h.app, "keymap.map", Value::Null).await.unwrap();
        assert_eq!(
            texts(&map, json!({ "action": "next_needs_you_item" })),
            [key.as_str().unwrap()]
        );
        assert!(
            request(&h.app, "keyboard.reset_row", json!({ "keys": [] }))
                .await
                .is_err()
        );
        h.service.stop_all().await;
    }
}

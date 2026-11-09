//! `theme.*`: the theme the page draws with (UIT-07), and the theme picker's mode, cards and picks (UI-11). The page
//! asks again on every `settings_changed` event, so a pick in Settings or in another window recolors this one at once,
//! and on every change of the OS appearance, so Match system follows the OS live.

use otaq_core::board::settings::{GlobalSettings, Theme, ThemeMode};
use otaq_core::protocol::{OsDarkParams, Reply, Request};
use otaq_view::theme::{Appearance, Mode, ThemeId};
use otaq_view::view_models::theme_picker;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "theme.active" => active(app, params).await,
        "theme.picker" => picker(app, params).await,
        "theme.pick" => pick(app, params).await,
        "theme.mode" => mode(app, params).await,
        _ => Err(unknown(method)),
    }
}

/// What `theme.active` and `theme.picker` take: whether the OS appearance is dark now, which Match system shows.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
pub struct ThemeParams {
    pub os_dark: bool,
}

/// What `theme.pick` takes: the picked theme's key (`tokyo-night`), which becomes the light or the dark theme by its
/// kind. The answer is the [`ActiveTheme`].
#[derive(Debug, Clone, Deserialize, TS)]
pub struct ThemePick {
    pub key: String,
}

/// What `theme.mode` takes: the mode chosen. The answer is the [`ActiveTheme`].
#[derive(Debug, Clone, Copy, Deserialize, TS)]
pub struct ThemeModePick {
    pub mode: Mode,
}

/// The theme in use: its key (the page's `data-theme`, e.g. `tokyo-night`), its name, whether it is dark, and the
/// mode, which the window's own parts follow (the title bar and the menus: as the OS on Match system, else light or
/// dark).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ActiveTheme {
    pub key: String,
    pub name: String,
    pub dark: bool,
    pub mode: Mode,
}

impl ActiveTheme {
    fn of(settings: &GlobalSettings) -> Self {
        let id = ThemeId::from_stored(settings.theme_in_use());
        Self {
            key: id.stored().as_str().to_owned(),
            name: id.name(),
            dark: id.appearance() == Appearance::Dark,
            mode: settings.theme.mode.into(),
        }
    }
}

fn params<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

async fn settings(app: &App) -> Result<GlobalSettings, RequestError> {
    match app.service.send(Request::SettingsGet).await? {
        Reply::GlobalSettings(reply) => Ok(reply.settings),
        other => Err(RequestError::plain(format!(
            "unexpected answer to settings.get: {other:?}"
        ))),
    }
}

/// The settings, with the OS appearance the page sees now. On Match system a changed appearance is stored, so the
/// service tells sessions starting from now the theme the window shows; the service then announces
/// `settings_changed`, on which every window asks again. In Light or Dark mode the window's own appearance is fixed
/// (the page then reads that, not the OS's), so nothing is stored.
async fn seen(app: &App, params: ThemeParams) -> Result<GlobalSettings, RequestError> {
    let settings = settings(app).await?;
    if settings.theme.mode == ThemeMode::System && settings.os_dark != params.os_dark {
        let dark = OsDarkParams {
            dark: params.os_dark,
        };
        return match app.service.send(Request::SettingsOsDark(dark)).await? {
            Reply::GlobalSettings(reply) => Ok(reply.settings),
            other => Err(RequestError::plain(format!(
                "unexpected answer to settings.os_dark: {other:?}"
            ))),
        };
    }
    Ok(settings)
}

pub async fn active(app: &App, params: Value) -> Result<Value, RequestError> {
    reply(&ActiveTheme::of(&seen(app, self::params(params)?).await?))
}

/// The picker: the mode row, and the Dark and Light cards with the dark and the light pick checked
/// (`ThemePickerView`).
pub async fn picker(app: &App, params: Value) -> Result<Value, RequestError> {
    let settings = seen(app, self::params(params)?).await?;
    reply(&theme_picker::picker(settings.theme))
}

/// A click on a theme card: it becomes the light or the dark theme, by its kind, stored at once (UI-11). The service
/// then announces `settings_changed`, on which every window recolors.
pub async fn pick(app: &App, params: Value) -> Result<Value, RequestError> {
    let ThemePick { key } = self::params(params)?;
    let theme: Theme = key
        .parse()
        .map_err(|_| RequestError::plain(format!("unknown theme: {key}")))?;
    let mut settings = settings(app).await?;
    settings.theme.pick(theme);
    app.service
        .send(Request::SettingsSet(settings.clone()))
        .await?;
    reply(&ActiveTheme::of(&settings))
}

/// A click on Match system, Light or Dark: stored at once, and announced like a pick.
pub async fn mode(app: &App, params: Value) -> Result<Value, RequestError> {
    let ThemeModePick { mode } = self::params(params)?;
    let mut settings = settings(app).await?;
    settings.theme.mode = mode.into();
    app.service
        .send(Request::SettingsSet(settings.clone()))
        .await?;
    reply(&ActiveTheme::of(&settings))
}

#[cfg(test)]
mod tests {
    use otaq_core::board::settings::ThemeSetting;
    use otaq_core::naming;
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    async fn stored(h: &Harness) -> GlobalSettings {
        match h.app.service.send(Request::SettingsGet).await.unwrap() {
            Reply::GlobalSettings(reply) => reply.settings,
            other => panic!("{other:?}"),
        }
    }

    async fn settings_changed(h: &Harness) {
        h.wait_for_event("settings_changed", |e| {
            e["kind"] == "service" && e["name"] == "settings_changed"
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_fresh_data_folder_matches_the_system_and_follows_it_live() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        assert_eq!(stored(&h).await.theme, ThemeSetting::default());

        let dark = request(&h.app, "theme.active", json!({ "os_dark": true }))
            .await
            .unwrap();
        assert_eq!(
            dark,
            json!({ "key": naming::THEME_DARK, "name": format!("{} Dark", naming::PRODUCT_NAME), "dark": true, "mode": "system" })
        );
        // The OS turned light: the light theme shows, and the service hears of it for the sessions it starts.
        let light = request(&h.app, "theme.active", json!({ "os_dark": false }))
            .await
            .unwrap();
        assert_eq!(light["key"], naming::THEME_LIGHT);
        assert_eq!(light["dark"], false);
        let settings = stored(&h).await;
        assert!(!settings.os_dark);
        assert_eq!(settings.theme, ThemeSetting::default());
        settings_changed(&h).await;
        h.service.stop_all().await;
    }

    /// A whole-settings save from a page that read the settings before the OS turned dark never puts the old
    /// appearance back: only the app's own look at the OS writes it.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_whole_settings_save_keeps_the_os_appearance() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        request(&h.app, "theme.active", json!({ "os_dark": false }))
            .await
            .unwrap();
        let mut older = stored(&h).await;
        assert!(!older.os_dark);
        request(&h.app, "theme.active", json!({ "os_dark": true }))
            .await
            .unwrap();
        assert!(stored(&h).await.os_dark);
        older.terminal_font_size = 15;
        h.app
            .service
            .send(Request::SettingsSet(older))
            .await
            .unwrap();
        let after = stored(&h).await;
        assert_eq!(after.terminal_font_size, 15, "the save is taken");
        assert!(after.os_dark, "the OS appearance stays as the app last saw it");
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_fixed_mode_ignores_the_os_and_a_change_is_announced() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let mut settings = stored(&h).await;
        settings.theme.mode = ThemeMode::Dark;
        settings.theme.dark = Theme::TokyoNight;
        h.app
            .service
            .send(Request::SettingsSet(settings))
            .await
            .unwrap();
        // The page refetches on this event (UIT-07).
        settings_changed(&h).await;

        for os_dark in [false, true] {
            let theme = request(&h.app, "theme.active", json!({ "os_dark": os_dark }))
                .await
                .unwrap();
            assert_eq!(
                theme,
                json!({ "key": "tokyo-night", "name": "Tokyo Night", "dark": true, "mode": "dark" })
            );
        }
        assert!(stored(&h).await.os_dark, "a fixed mode stores no OS appearance");
        let wrong = request(&h.app, "theme.active", Value::Null).await;
        assert!(wrong.is_err());
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_card_click_picks_its_kinds_theme_and_a_mode_click_chooses_the_mode() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;

        let before = request(&h.app, "theme.picker", json!({ "os_dark": false }))
            .await
            .unwrap();
        assert_eq!(before["modes"][0]["mode"], "system");
        assert_eq!(before["modes"][0]["selected"], true);
        assert_eq!(before["sections"][0]["rows"][0][0]["id"], naming::THEME_DARK);
        assert_eq!(before["sections"][0]["rows"][0][0]["selected"], true);
        assert_eq!(before["sections"][1]["rows"][0][0]["id"], naming::THEME_LIGHT);
        assert_eq!(before["sections"][1]["rows"][0][0]["selected"], true);

        // A dark card on a light OS: it becomes the dark theme; the light one still shows.
        let picked = request(&h.app, "theme.pick", json!({ "key": "dracula" }))
            .await
            .unwrap();
        assert_eq!(picked["key"], naming::THEME_LIGHT);
        assert_eq!(stored(&h).await.theme.dark, Theme::Dracula);
        settings_changed(&h).await;
        let after = request(&h.app, "theme.picker", json!({ "os_dark": false }))
            .await
            .unwrap();
        assert_eq!(after["sections"][0]["rows"][1][0]["id"], "dracula");
        assert_eq!(after["sections"][0]["rows"][1][0]["selected"], true);
        assert_eq!(after["sections"][0]["rows"][0][0]["selected"], false);
        assert_eq!(after["sections"][1]["rows"][0][0]["selected"], true);

        let dark = request(&h.app, "theme.mode", json!({ "mode": "dark" }))
            .await
            .unwrap();
        assert_eq!(
            dark,
            json!({ "key": "dracula", "name": "Dracula", "dark": true, "mode": "dark" })
        );
        assert_eq!(stored(&h).await.theme.mode, ThemeMode::Dark);
        // A light card while the mode is Dark: the mode turns Light, so the click shows.
        let daylight = request(&h.app, "theme.pick", json!({ "key": "daylight" }))
            .await
            .unwrap();
        assert_eq!(
            daylight,
            json!({ "key": "daylight", "name": "Daylight", "dark": false, "mode": "light" })
        );

        let unknown = request(&h.app, "theme.pick", json!({ "key": "neon" })).await;
        assert_eq!(
            unknown,
            Err(RequestError::plain("unknown theme: neon"))
        );
        let bad_mode = request(&h.app, "theme.mode", json!({ "mode": "sepia" })).await;
        assert!(bad_mode.is_err());
        assert_eq!(stored(&h).await.theme.light, Theme::Daylight);
        h.service.stop_all().await;
    }
}

//! The theme picker shared by onboarding and Settings → Appearance (UI-11, d0069; *14 · First run & onboarding → 2 ·
//! Theme*, *15 · Settings → Global · Usage, notifications, data, appearance*): the mode (Match system, Light, Dark),
//! then the 15 theme cards under a Dark and a Light heading. One card is checked in each: the dark and the light
//! pick. A click on a card picks it for its kind, a click on a mode chooses it; both apply and save at once.

use otaq_core::board::settings::ThemeSetting;
use serde::Serialize;
use ts_rs::TS;

use crate::theme::{Appearance, Mode, ThemeId};

/// One theme card: the preview is drawn from `id`'s own colours. On the page (`ThemePickerCard`) `id` is the
/// theme's key, the value of `data-theme`, so the preview draws itself in that theme's variables.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ThemePickerCard")]
pub struct ThemeCard {
    #[ts(as = "String")]
    pub id: ThemeId,
    pub name: String,
    pub tagline: &'static str,
    pub selected: bool,
}

/// The Dark or the Light group of cards. `ThemePickerSection` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ThemePickerSection")]
pub struct PickerSection {
    #[serde(skip)]
    pub kind: Appearance,
    pub label: &'static str,
    /// Rows of five cards.
    pub rows: Vec<Vec<ThemeCard>>,
}

/// One choice of the mode row. `ThemeModeOption` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ThemeModeOption")]
pub struct ModeOption {
    pub mode: Mode,
    pub label: &'static str,
    pub selected: bool,
}

/// The whole picker: the mode row, then the Dark and the Light cards. `ThemePickerView` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ThemePickerView")]
pub struct Picker {
    pub modes: Vec<ModeOption>,
    pub sections: Vec<PickerSection>,
}

/// The picker for `setting`: its mode chosen, its dark and its light theme checked.
pub fn picker(setting: ThemeSetting) -> Picker {
    let mode = Mode::from(setting.mode);
    Picker {
        modes: Mode::ALL
            .into_iter()
            .map(|m| ModeOption {
                mode: m,
                label: m.label(),
                selected: m == mode,
            })
            .collect(),
        sections: sections(
            ThemeId::from_stored(setting.dark),
            ThemeId::from_stored(setting.light),
        )
        .into(),
    }
}

/// Cards per row in both frames.
pub const PER_ROW: usize = 5;

/// The one-line tagline under a theme's name, as in the frames.
pub fn tagline(id: ThemeId) -> &'static str {
    match id {
        ThemeId::OtaqDark => "Default · graphite",
        ThemeId::CatppuccinFrappe => "Muted, soft pastels",
        ThemeId::CatppuccinMacchiato => "Medium-contrast pastels",
        ThemeId::CatppuccinMocha => "Deep, cozy pastels",
        ThemeId::TokyoNight => "Neon city after dark",
        ThemeId::Dracula => "Bold, high contrast",
        ThemeId::Nord => "Arctic, calm blues",
        ThemeId::GruvboxDark => "Retro, warm earth",
        ThemeId::RosePine => "Soho vibes, muted",
        ThemeId::OneDark => "Atom’s classic",
        ThemeId::OtaqLight => "Default · clean paper",
        ThemeId::CatppuccinLatte => "Pastels in daylight",
        ThemeId::Daylight => "Familiar and crisp",
        ThemeId::SolarizedLight => "Low-glare cream",
        ThemeId::RosePineDawn => "Warm morning blush",
    }
}

/// Both groups, with the dark pick checked in Dark and the light pick in Light.
pub fn sections(dark: ThemeId, light: ThemeId) -> [PickerSection; 2] {
    let section = |kind: Appearance, label, themes: &[ThemeId], selected: ThemeId| {
        let cards: Vec<ThemeCard> = themes
            .iter()
            .map(|&id| ThemeCard {
                id,
                name: id.name(),
                tagline: tagline(id),
                selected: id == selected,
            })
            .collect();
        PickerSection {
            kind,
            label,
            rows: cards.chunks(PER_ROW).map(<[ThemeCard]>::to_vec).collect(),
        }
    };
    [
        section(Appearance::Dark, "Dark", &ThemeId::DARK, dark),
        section(Appearance::Light, "Light", &ThemeId::LIGHT, light),
    ]
}

#[cfg(test)]
mod tests {
    use otaq_core::naming;

    use super::*;

    fn selected(section: &PickerSection) -> Vec<ThemeId> {
        section
            .rows
            .iter()
            .flatten()
            .filter(|c| c.selected)
            .map(|c| c.id)
            .collect()
    }

    #[test]
    fn two_groups_as_in_the_frames() {
        let [dark, light] = sections(ThemeId::OtaqDark, ThemeId::OtaqLight);
        assert_eq!((dark.label, light.label), ("Dark", "Light"));
        assert_eq!(
            dark.rows.iter().map(Vec::len).collect::<Vec<_>>(),
            [5, 5],
            "two rows of five"
        );
        assert_eq!(light.rows.iter().map(Vec::len).collect::<Vec<_>>(), [5]);
        assert_eq!(dark.rows[0][4].name, "Tokyo Night");
        assert_eq!(dark.rows[0][4].tagline, "Neon city after dark");
        assert_eq!(light.rows[0][3].name, "Solarized Light");
        assert_eq!(selected(&dark), [ThemeId::OtaqDark]);
        assert_eq!(selected(&light), [ThemeId::OtaqLight]);
    }

    #[test]
    fn the_page_gets_each_card_by_its_key() {
        let json = serde_json::to_value(sections(ThemeId::Dracula, ThemeId::Daylight)).unwrap();
        assert_eq!(json[0]["label"], "Dark");
        assert_eq!(
            json[0]["rows"][1][0],
            serde_json::json!({
                "id": "dracula",
                "name": "Dracula",
                "tagline": "Bold, high contrast",
                "selected": true,
            })
        );
        assert_eq!(json[1]["rows"][0][0]["id"], naming::THEME_LIGHT);
    }

    #[test]
    fn one_card_is_checked_in_each_group() {
        for dark_id in ThemeId::DARK {
            for light_id in ThemeId::LIGHT {
                let [dark, light] = sections(dark_id, light_id);
                assert_eq!(selected(&dark), [dark_id]);
                assert_eq!(selected(&light), [light_id]);
            }
        }
    }

    #[test]
    fn the_mode_row_offers_match_system_light_and_dark_with_the_stored_one_chosen() {
        use otaq_core::board::settings::{Theme, ThemeMode};
        let setting = ThemeSetting {
            mode: ThemeMode::Light,
            light: Theme::SolarizedLight,
            dark: Theme::Nord,
        };
        let json = serde_json::to_value(picker(setting)).unwrap();
        assert_eq!(
            json["modes"],
            serde_json::json!([
                {"mode": "system", "label": "Match system", "selected": false},
                {"mode": "light", "label": "Light", "selected": true},
                {"mode": "dark", "label": "Dark", "selected": false},
            ])
        );
        let sections = picker(setting).sections;
        let checked: Vec<ThemeId> = sections.iter().flat_map(selected).collect();
        assert_eq!(checked, [ThemeId::Nord, ThemeId::SolarizedLight]);
        assert_eq!(
            picker(ThemeSetting::default()).modes[0],
            ModeOption {
                mode: Mode::System,
                label: "Match system",
                selected: true,
            },
            "a fresh install matches the system"
        );
    }
}

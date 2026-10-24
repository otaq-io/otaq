//! The design gallery (T-0217): a developer-only page that draws the foundations and every component next to each
//! other, to compare with *crewboard.pen → 01 · Foundations* and *01 · Components* by hand. This is what it shows
//! that comes from the Rust side: the Status Legend (with the live parts of the frame's samples), the Priority
//! tier, the type scale, spacing, radius and density, the icons, the keyboard map and sample menus.

use otaq_core::domain::Priority;
use otaq_core::naming;
use serde::Serialize;
use ts_rs::TS;

use super::keyboard_map::{self, KeyMap};
use super::menu::MenuItem;
use super::priority::{self, PriorityView};
use super::shortcut_list::{self, ShortcutList};
use super::status::PillView;
use crate::icons::IconName;
use crate::keymap::{
    self, CustomKey, CustomKeys, Key, KeyAction, KeyChange, Platform, SequenceHint,
};
use crate::theme::{Status, TextStyle, Tier, density, radius, space, text};

/// One tier of the *Status Legend*: its pills, then its *Pills · M12 (0063–0066)* row. `LegendTier` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LegendTier")]
pub struct LegendTier {
    pub title: String,
    pub pills: Vec<PillView>,
    pub m12: Vec<PillView>,
}

/// *Status Legend → 5 · Priority (0065)*. `LegendPriority` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LegendPriority")]
pub struct LegendPriority {
    pub title: String,
    pub note: String,
    pub chips: Vec<PriorityView>,
    pub order: String,
}

/// One step of the type scale: its name and spec, the CSS variable it uses (`--text-<token>`) and a sample.
/// `TypeScaleRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TypeScaleRow")]
pub struct TypeRow {
    pub name: String,
    pub spec: String,
    pub token: String,
    pub sample: String,
}

/// A named size in px (a spacing step or a radius). `GalleryMeasure` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "GalleryMeasure")]
pub struct Measure {
    pub name: String,
    pub px: f32,
}

/// An icon and its Lucide name. `GalleryIcon` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "GalleryIcon")]
pub struct IconEntry {
    pub icon: IconName,
    pub name: String,
}

/// Everything the gallery draws from the Rust side. `GalleryView` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "GalleryView")]
pub struct GalleryView {
    pub legend: Vec<LegendTier>,
    pub priority: LegendPriority,
    pub type_scale: Vec<TypeRow>,
    pub spacing: Vec<Measure>,
    pub radii: Vec<Measure>,
    pub density: String,
    pub icons: Vec<IconEntry>,
    pub keyboard_map: KeyMap,
    /// The `?` list (design `s02-24`…`s02-27`): the defaults, with Pause / Resume agents on ⌘I and New ticket's letter
    /// n, and with the letters off.
    pub shortcut_lists: [ShortcutList; 3],
    /// The `g` hint (design `s02-30`): with the base map's sequences, and with a recorded `g` `p` Park.
    pub g_hints: [Vec<SequenceHint>; 2],
    /// The command palette's states (design `s02-20` … `s02-23`, T-0393).
    pub palette: Vec<crate::view_models::palette::GalleryFrame>,
    /// A card's context menu as in *Overlays & feedback → Context menu*.
    pub context_menu: Vec<MenuItem>,
    /// A dropdown's list: the priority menu with P2 checked.
    pub priority_menu: Vec<MenuItem>,
}

/// The legend's samples with their live parts, as in the frame.
fn live_label(status: Status) -> Option<&'static str> {
    match status {
        Status::Conflicts => Some("Conflicts with main"),
        Status::Running => Some("Serving :4020"),
        Status::Asked => Some("Asked SHOP-40.1"),
        Status::Epic => Some("Epic · 1 of 3 merged"),
        Status::ValidationFailed => Some("Validation failed 3 times"),
        Status::AutoMergeStopped => Some("Keeps conflicting"),
        Status::Validated => Some("Validated · round 2"),
        Status::FailedMoved => Some("Failed 3 times · moved by you"),
        Status::Validating => Some("Validating… · round 1 of 3"),
        Status::ValidationRound => Some("Validation failed · round 2 of 3"),
        Status::ProjectAgent => Some("by Checkout slowness"),
        _ => None,
    }
}

fn legend() -> Vec<LegendTier> {
    let tiers = [
        (Tier::NeedsYou, "1 · Needs you"),
        (Tier::Review, "2 · Waiting on your review"),
        (Tier::Working, "3 · Working normally"),
        (Tier::Done, "4 · Fades back"),
    ];
    let pills = |list: &[Status], tier: Tier| -> Vec<PillView> {
        list.iter()
            .filter(|s| s.spec().0 == tier)
            .map(|s| PillView::legend(*s, live_label(*s)))
            .collect()
    };
    tiers
        .into_iter()
        .map(|(tier, title)| {
            let mut m12 = pills(&Status::M12, tier);
            if tier == Tier::Working {
                // After the merge-conflict marker: the Queued entry with the slot count (d0066).
                m12.push(PillView::legend(
                    Status::Queued,
                    Some("Waiting for a slot · 5/5"),
                ));
            }
            LegendTier {
                title: title.to_owned(),
                pills: pills(&Status::ALL, tier),
                m12,
            }
        })
        .collect()
}

fn priority_tier() -> LegendPriority {
    LegendPriority {
        title: "5 · Priority (0065)".into(),
        note: "Not a status: an ink-only scale, never turquoise or red. Icon + number on cards; number + word in the \
               sheet and forms. P3 Normal is hidden on cards."
            .into(),
        chips: Priority::ALL.into_iter().map(priority::view).collect(),
        order: "Start order in Ready for Dev: pinned → P1 … P4 → ready the longest".into(),
    }
}

/// "20 / 600 / -0.3", "13 / 400 / 1.5", "12 mono", "13 mono / 1.45".
pub fn spec(style: TextStyle) -> String {
    let mut parts = vec![if style.mono {
        format!("{} mono", style.size)
    } else {
        format!("{} / {}", style.size, style.weight)
    }];
    if let Some(line_height) = style.line_height {
        parts.push(line_height.to_string());
    }
    if style.letter_spacing != 0. {
        parts.push(style.letter_spacing.to_string());
    }
    parts.join(" / ")
}

fn type_scale() -> Vec<TypeRow> {
    let code = format!("{}  a1b2c3d", naming::branch("SHOP-18"));
    let samples: [(&str, &str); 8] = [
        ("Display / dialog title", "Approve SHOP-18 Checkout API"),
        ("Header · project", "shop-web"),
        ("Column title", "Working  3"),
        ("Card title", "Fix flaky login redirect"),
        ("Body", "Colors are hard-coded in 14 files."),
        ("Metadata", "active 40s ago · 182k tokens"),
        ("Code / IDs", &code),
        ("Terminal", "● Read(src/app/checkout/page.tsx)"),
    ];
    text::ALL
        .into_iter()
        .zip(samples)
        .map(|((token, style), (name, sample))| TypeRow {
            name: name.to_owned(),
            spec: spec(style),
            token: token.to_owned(),
            sample: sample.to_owned(),
        })
        .collect()
}

fn measures(list: &[(&str, f32)]) -> Vec<Measure> {
    list.iter()
        .map(|(name, px)| Measure {
            name: (*name).to_owned(),
            px: *px,
        })
        .collect()
}

fn context_menu() -> Vec<MenuItem> {
    vec![
        MenuItem::new("open", "Open panel")
            .icon(IconName::PanelRight)
            .shortcut("↵"),
        MenuItem::new("terminal", "Open terminal").icon(IconName::SquareTerminal),
        MenuItem::new("diff", "Open diff").icon(IconName::FileDiff),
        MenuItem::new("pin", "Pin to top")
            .icon(IconName::Pin)
            .separated(),
        MenuItem::new("copy", "Copy branch name").icon(IconName::Copy),
        MenuItem::new("chain", "Highlight chain")
            .icon(IconName::Link2)
            .shortcut("H"),
        MenuItem::new("send-back", "Send back…")
            .icon(IconName::Undo2)
            .separated(),
        MenuItem::new("close", "Close ticket…")
            .icon(IconName::CircleSlash)
            .danger(),
        MenuItem::new("delete", "Delete…")
            .icon(IconName::Trash2)
            .danger()
            .disabled(true)
            .hint("Only for tickets that never started"),
    ]
}

pub fn view() -> GalleryView {
    GalleryView {
        legend: legend(),
        priority: priority_tier(),
        type_scale: type_scale(),
        spacing: measures(&[
            ("s-1", space::S1),
            ("s-2", space::S2),
            ("s-3", space::S3),
            ("s-4", space::S4),
            ("s-5", space::S5),
            ("s-6", space::S6),
        ]),
        radii: measures(&[
            ("radius sm", radius::SM),
            ("radius md", radius::MD),
            ("radius lg", radius::LG),
            ("dialog", radius::DIALOG),
        ]),
        density: format!(
            "Column {} px (comfortable) · {} px (compact) · Card padding {} × {} · Header {} px · Row {} px",
            density::COLUMN,
            density::COLUMN_COMPACT,
            density::CARD_PADDING.0,
            density::CARD_PADDING.1,
            density::HEADER,
            density::ROW
        ),
        icons: IconName::ALL
            .iter()
            .map(|icon| IconEntry {
                icon: *icon,
                name: icon.lucide_name().to_owned(),
            })
            .collect(),
        keyboard_map: keyboard_map::map(),
        shortcut_lists: shortcut_lists(),
        g_hints: g_hints(),
        palette: crate::view_models::palette::gallery(),
        context_menu: context_menu(),
        priority_menu: priority::menu_items(Priority::High),
    }
}

fn shortcut_lists() -> [ShortcutList; 3] {
    let replaced = |action, slot, key: &str| CustomKey {
        action,
        slot,
        change: KeyChange::Replaced {
            key: Key::parse(key).expect("a key"),
        },
        works_in_terminals: false,
    };
    let changed = CustomKeys(vec![
        // `s02-24`'s second list: Pause / Resume agents on ⌘I.
        replaced(KeyAction::Pause, 0, "⌘I"),
        replaced(KeyAction::NewTicket, 1, "n"),
    ]);
    let list = |custom: &CustomKeys, letters| {
        shortcut_list::shortcut_list(&keymap::user_keys(Platform::Mac, custom), letters)
    };
    [
        list(&CustomKeys::default(), true),
        list(&changed, true),
        list(&CustomKeys::default(), false),
    ]
}

fn g_hints() -> [Vec<SequenceHint>; 2] {
    let park = CustomKeys(vec![CustomKey {
        action: KeyAction::Park,
        slot: 0,
        change: KeyChange::Replaced {
            key: Key::Sequence { then: "p".into() },
        },
        works_in_terminals: false,
    }]);
    [CustomKeys::default(), park]
        .map(|custom| keymap::sequence_hint(&keymap::user_keys(Platform::Mac, &custom)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_legend_shows_every_pill_with_icon_and_label() {
        let view = view();
        let shown: usize = view
            .legend
            .iter()
            .map(|t| t.pills.len() + t.m12.len())
            .sum();
        // Every entry, every M12 entry, and the Queued entry again as "Waiting for a slot".
        assert_eq!(shown, Status::ALL.len() + Status::M12.len() + 1);
        for pill in view
            .legend
            .iter()
            .flat_map(|t| t.pills.iter().chain(&t.m12))
        {
            assert!(!pill.label.is_empty());
        }
        let titles: Vec<&str> = view.legend.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "1 · Needs you",
                "2 · Waiting on your review",
                "3 · Working normally",
                "4 · Fades back"
            ]
        );
        assert!(
            view.legend[2]
                .pills
                .iter()
                .any(|p| p.label == "Serving :4020")
        );
        assert_eq!(
            view.legend[2].m12.last().unwrap().label,
            "Waiting for a slot · 5/5"
        );
        assert_eq!(view.priority.chips.len(), 4);
    }

    #[test]
    fn type_scale_specs_read_as_in_the_frame() {
        let specs: Vec<(String, String)> = type_scale()
            .into_iter()
            .map(|r| (r.token, r.spec))
            .collect();
        let expected = [
            ("display", "20 / 600 / -0.3"),
            ("header", "15 / 600"),
            ("column-title", "12 / 600"),
            ("card-title", "13 / 500 / 1.35"),
            ("body", "13 / 400 / 1.5"),
            ("meta", "11 / 400"),
            ("code", "12 mono"),
            ("terminal", "13 mono / 1.45"),
        ];
        assert_eq!(
            specs,
            expected.map(|(t, s)| (t.to_owned(), s.to_owned())).to_vec()
        );
    }

    #[test]
    fn menus_and_icons() {
        let view = view();
        assert_eq!(view.icons.len(), IconName::ALL.len());
        assert!(
            view.context_menu
                .iter()
                .any(|i| i.disabled && i.hint.is_some())
        );
        assert_eq!(view.priority_menu.iter().filter(|i| i.checked).count(), 1);
    }
}

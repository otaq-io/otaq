//! One-time hints: a concept is explained when it first matters, never behind a setting. A hint is one sentence in a
//! dark popover next to the element it explains, with one button; the button dismisses it for good (kept by the
//! service per data folder, so a restart doesn't bring it back and a fresh data folder shows it again).
//!
//! The ids are a closed set ([`Hint`], kept with the store); each hint's wording lives here.

use otaq_core::store::settings::Hint;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A hint's id as the page names it: the same names as the hints the service stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum HintId {
    /// The design gallery's sample, the one hint with no moment in the app.
    GallerySample,
    /// The first time Edit flow turns the board into the editor.
    FlowColumns,
}

impl From<HintId> for Hint {
    fn from(id: HintId) -> Self {
        match id {
            HintId::GallerySample => Hint::GallerySample,
            HintId::FlowColumns => Hint::FlowColumns,
        }
    }
}

impl From<Hint> for HintId {
    fn from(hint: Hint) -> Self {
        match hint {
            Hint::GallerySample => HintId::GallerySample,
            Hint::FlowColumns => HintId::FlowColumns,
        }
    }
}

/// What the popover shows: the one sentence and its one button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "HintView")]
pub struct View {
    pub id: HintId,
    pub text: String,
    pub button: String,
}

/// The button every hint has: it dismisses the hint for good.
pub const DISMISS: &str = "Got it";

/// The first time Edit flow turns the board into the editor.
pub const FLOW_COLUMNS: &str = "This is your flow, as columns. Click a stage to see what it\u{2019}s for, + adds one \
     between two stages, and drag to reorder. Watch a ticket to see what your draft would do. Nothing changes until \
     you save.";

/// The hint's popover.
pub fn view(id: HintId) -> View {
    let text = match id {
        HintId::GallerySample => {
            "A hint like this shows once, when something first matters, and never again."
        }
        HintId::FlowColumns => FLOW_COLUMNS,
    };
    View {
        id,
        text: text.to_owned(),
        button: DISMISS.to_owned(),
    }
}

/// The hint's popover, or nothing once the user dismissed it (`seen`).
pub fn shown(id: HintId, seen: &[Hint]) -> Option<View> {
    (!seen.contains(&id.into())).then(|| view(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stored_hint_has_a_page_id_with_the_same_name() {
        for &hint in Hint::ALL {
            let id = HintId::from(hint);
            assert_eq!(Hint::from(id), hint);
            assert_eq!(
                serde_json::to_value(id).unwrap(),
                serde_json::to_value(hint).unwrap()
            );
        }
    }

    #[test]
    fn a_hint_is_one_sentence_with_one_button_until_dismissed() {
        let shown = shown(HintId::GallerySample, &[]).unwrap();
        assert_eq!(shown.button, "Got it");
        assert_eq!(shown.text.matches(". ").count(), 0, "one sentence");
        assert!(shown.text.ends_with('.'));
        assert_eq!(
            super::shown(HintId::GallerySample, &[Hint::GallerySample]),
            None
        );
    }
}

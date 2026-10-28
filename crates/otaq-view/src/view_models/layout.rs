//! The window's layout (SHB-01, d0113, *24 → Row · The Project agent*): project rail | the **Project agent panel**,
//! only while it is open, pushing the board | the main area. There is no always-visible agent panel, no strip and no Board |
//! Files tab row: the project Files view is the toolbar's third view (FIL-01). The page asks at start (`layout.view`);
//! whether the panel is open or expanded stays on the page, per window.
//!
//! The panel's width (T-0502, dogfood #7): 420 px until its right edge is dragged, at least [`AGENT_PANEL_MIN`] and
//! at most what leaves the board [`BOARD_MIN`]; the dragged width is kept in the global settings, so it survives a
//! restart (Otaq has one window), and a double-click on the edge goes back to 420. **Expand** in its header lays the
//! panel over the board at the window's full width after the rail; the board keeps its place under it.
//!
//! The ticket sheet's sizes are `sheet.rs`'s; it opens over the main area (SHL-05) and uses [`SizeClass`] too. Its
//! left edge drags it like the panel's: at least [`SHEET_MIN`], at most the main area less its margins. The Files
//! tree (the project Files view and the sheet's Files tab) drags at its right edge between [`FILES_TREE_MIN`] and
//! [`FILES_TREE_MAX`]. Both dragged widths are kept in the global settings too; a double-click forgets them.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::view_models::board::{BOARD_PADDING, COLUMN_GAP, COLUMN_MIN_WIDTH};
use crate::view_models::switcher::RAIL_WIDTH;

/// Which of the design's three windows a window is closest to (Q-C): the ticket sheet's remembered width is kept per
/// class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SizeClass {
    W1280,
    W1440,
    W1920,
}

/// A class and the window width it ends below (`None`: the last class), narrowest first. The page picks a class
/// from this table, so [`size_class`] is the only rule.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(rename = "SizeClassBound")]
pub struct ClassBound {
    pub class: SizeClass,
    pub below: Option<f32>,
}

/// The classes split at the midpoints between the design's widths: below 1360 px → 1280, below 1680 → 1440, else
/// 1920.
pub const CLASSES: [ClassBound; 3] = [
    ClassBound {
        class: SizeClass::W1280,
        below: Some(1360.),
    },
    ClassBound {
        class: SizeClass::W1440,
        below: Some(1680.),
    },
    ClassBound {
        class: SizeClass::W1920,
        below: None,
    },
];

/// The class of a window `window_width` px wide.
pub fn size_class(window_width: f32) -> SizeClass {
    CLASSES
        .iter()
        .find(|c| c.below.is_none_or(|below| window_width < below))
        .map_or(SizeClass::W1920, |c| c.class)
}

/// One width per size class.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
pub struct ClassWidths {
    pub w1280: f32,
    pub w1440: f32,
    pub w1920: f32,
}

impl ClassWidths {
    pub fn get(self, class: SizeClass) -> f32 {
        match class {
            SizeClass::W1280 => self.w1280,
            SizeClass::W1440 => self.w1440,
            SizeClass::W1920 => self.w1920,
        }
    }
}

/// The Project agent panel's width while it is open and not dragged (SHB-01, `s24-05`): the same at every window size.
pub const AGENT_PANEL: f32 = 420.;

/// The narrowest the panel's edge drags it (T-0502).
pub const AGENT_PANEL_MIN: f32 = 320.;

/// What the board keeps beside the panel, the most the edge takes from it (T-0502): two columns at their minimum
/// (BRD-02) with the board's padding, so a card can still be dragged from one column to the next.
pub const BOARD_MIN: f32 = 2. * BOARD_PADDING + 2. * COLUMN_MIN_WIDTH + COLUMN_GAP;

/// How the panel is shown now (page state, per window).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentPanel {
    /// Folded into the Ask bar (d0113).
    Closed,
    /// Between the rail and the main area, pushing the board.
    Open,
    /// Over the board, as wide as the window after the rail (Expand).
    Expanded,
}

/// The widest the edge drags the panel in a window `window_width` px wide: the board keeps [`BOARD_MIN`]. Never
/// under [`AGENT_PANEL_MIN`]: in a window too narrow for both, the board gives way.
pub fn agent_panel_widest(window_width: f32) -> f32 {
    (window_width - RAIL_WIDTH as f32 - BOARD_MIN).max(AGENT_PANEL_MIN)
}

/// The open panel's width in a window `window_width` px wide: the dragged width (`stored`, else [`AGENT_PANEL`]) kept
/// between [`AGENT_PANEL_MIN`] and [`agent_panel_widest`]. A window made narrower narrows it; widened again, it goes
/// back to the dragged width.
pub fn agent_panel_width(window_width: f32, stored: Option<u32>) -> f32 {
    stored
        .map_or(AGENT_PANEL, |w| w as f32)
        .clamp(AGENT_PANEL_MIN, agent_panel_widest(window_width))
}

/// What a drag of an edge ending at `width` px keeps (`None`: the double-click): whole pixels between `min` and
/// `max`; the `default` width itself keeps nothing.
fn stored_width(width: Option<f32>, min: f32, max: f32, default: Option<f32>) -> Option<u32> {
    let width = width.filter(|w| w.is_finite())?;
    let kept = width.clamp(min, max).round() as u32;
    (Some(kept) != default.map(|d| d as u32)).then_some(kept)
}

/// What a drag of the edge ending at `width` px keeps (`None`: the double-click, back to [`AGENT_PANEL`]): whole
/// pixels, at least [`AGENT_PANEL_MIN`]. The window's own bound is applied when drawn ([`agent_panel_width`]), so a
/// width dragged in a wide window comes back there.
pub fn stored_agent_panel_width(width: Option<f32>) -> Option<u32> {
    stored_width(width, AGENT_PANEL_MIN, f32::MAX, Some(AGENT_PANEL))
}

/// The narrowest the ticket sheet's left edge drags it. The widest is the main area less its margins
/// (`sheet::width`), applied when drawn.
pub const SHEET_MIN: f32 = 560.;

/// What a drag of the sheet's edge keeps: whole pixels, at least [`SHEET_MIN`]; it replaces both default widths
/// (1040 px, 920 beside the panel), so the double-click is the only way back to them.
pub fn stored_sheet_width(width: Option<f32>) -> Option<u32> {
    stored_width(width, SHEET_MIN, f32::MAX, None)
}

/// The Files tree's default width (`files.css`; 270 px in a narrow project Files view) and its drag's bounds.
pub const FILES_TREE: f32 = 300.;
pub const FILES_TREE_MIN: f32 = 200.;
pub const FILES_TREE_MAX: f32 = 640.;

/// What a drag of the Files tree's edge keeps: whole pixels between [`FILES_TREE_MIN`] and [`FILES_TREE_MAX`].
pub fn stored_files_tree_width(width: Option<f32>) -> Option<u32> {
    stored_width(width, FILES_TREE_MIN, FILES_TREE_MAX, Some(FILES_TREE))
}

/// The panel's width on screen: nothing while closed, its width while open, the window after the rail while
/// expanded.
pub fn agent_panel_shown(window_width: f32, stored: Option<u32>, panel: AgentPanel) -> f32 {
    match panel {
        AgentPanel::Closed => 0.,
        AgentPanel::Open => agent_panel_width(window_width, stored),
        AgentPanel::Expanded => (window_width - RAIL_WIDTH as f32).max(0.),
    }
}

/// The main area's width in a window `window_width` px wide: all of it after the rail, less the panel while it is
/// open (it pushes the board, d0113). Expanded, the panel lies over the main area, which keeps its open width.
pub fn main_width(window_width: f32, stored: Option<u32>, panel: AgentPanel) -> f32 {
    let taken = match panel {
        AgentPanel::Closed => 0.,
        AgentPanel::Open | AgentPanel::Expanded => agent_panel_width(window_width, stored),
    };
    (window_width - RAIL_WIDTH as f32 - taken).max(0.)
}

/// What the frame draws and its sizes. `LayoutView` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "LayoutView")]
pub struct View {
    pub classes: Vec<ClassBound>,
    pub rail: f32,
    /// The open panel's width before the window's bound: the dragged width, else [`AGENT_PANEL`].
    pub agent_panel: f32,
    /// The drag's bounds: the panel's narrowest, and what the board keeps ([`agent_panel_widest`]).
    pub agent_panel_min: f32,
    pub board_min: f32,
    /// The ticket sheet's dragged width (`None`: its defaults) and its narrowest.
    pub sheet: Option<f32>,
    pub sheet_min: f32,
    /// The Files tree's dragged width (`None`: its default) and its drag's bounds.
    pub files_tree: Option<f32>,
    pub files_tree_min: f32,
    pub files_tree_max: f32,
    /// Every dragged edge's tooltip.
    pub edge: &'static str,
}

/// A dragged edge's tooltip (T-0502; d0113 sets no wording, listed for the designer).
pub const EDGE: &str = "Drag to resize. Double-click for the default width.";

/// The dragged widths kept in the global settings: the panel's, the sheet's and the Files tree's.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct KeptWidths {
    pub agent_panel: Option<u32>,
    pub sheet: Option<u32>,
    pub files_tree: Option<u32>,
}

/// The layout with the dragged widths `kept`. A width stored by hand outside its bounds is shown inside them.
pub fn view(kept: KeptWidths) -> View {
    View {
        classes: CLASSES.to_vec(),
        rail: RAIL_WIDTH as f32,
        agent_panel: kept
            .agent_panel
            .map_or(AGENT_PANEL, |w| (w as f32).max(AGENT_PANEL_MIN)),
        agent_panel_min: AGENT_PANEL_MIN,
        board_min: BOARD_MIN,
        sheet: kept.sheet.map(|w| (w as f32).max(SHEET_MIN)),
        sheet_min: SHEET_MIN,
        files_tree: kept
            .files_tree
            .map(|w| (w as f32).clamp(FILES_TREE_MIN, FILES_TREE_MAX)),
        files_tree_min: FILES_TREE_MIN,
        files_tree_max: FILES_TREE_MAX,
        edge: EDGE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_class_splits_at_the_midpoints() {
        for (width, class) in [
            (800., SizeClass::W1280),
            (1280., SizeClass::W1280),
            (1359., SizeClass::W1280),
            (1360., SizeClass::W1440),
            (1440., SizeClass::W1440),
            (1679., SizeClass::W1440),
            (1680., SizeClass::W1920),
            (1920., SizeClass::W1920),
            (2560., SizeClass::W1920),
        ] {
            assert_eq!(size_class(width), class, "{width} px");
        }
    }

    /// SHB-01 (d0113): without an agent panel the main area takes the width after the 64 px rail; the open panel takes its
    /// 420 px from it at every window size until it is dragged.
    #[test]
    fn the_main_area_takes_the_width_the_agent_panel_had() {
        for (window, closed, open) in [
            (1280., 1216., 796.),
            (1440., 1376., 956.),
            (1920., 1856., 1436.),
        ] {
            assert_eq!(
                main_width(window, None, AgentPanel::Closed),
                closed,
                "{window} px"
            );
            assert_eq!(
                main_width(window, None, AgentPanel::Open),
                open,
                "{window} px, panel open"
            );
        }
        assert_eq!(main_width(300., None, AgentPanel::Open), 0.);
        let view = view(KeptWidths::default());
        assert_eq!((view.rail, view.agent_panel), (64., 420.));
        assert_eq!((view.agent_panel_min, view.board_min), (320., 524.));
        assert_eq!(view.classes.len(), 3);
    }

    /// T-0502: the edge drags the panel between 320 px and what leaves the board two columns (524 px); a narrower
    /// window narrows a dragged panel, which comes back once the window is wide again.
    #[test]
    fn the_dragged_width_stays_between_its_bounds() {
        assert_eq!(agent_panel_widest(1280.), 692.);
        assert_eq!(agent_panel_widest(1920.), 1332.);
        // Too narrow for both: the panel keeps its narrowest, the board gives way.
        assert_eq!(agent_panel_widest(700.), 320.);
        for (window, stored, width) in [
            (1440., None, 420.),
            (1440., Some(640), 640.),
            (1440., Some(200), 320.),
            (1440., Some(1400), 852.),
            (1280., Some(900), 692.),
            (1920., Some(900), 900.),
            (700., None, 320.),
        ] {
            assert_eq!(
                agent_panel_width(window, stored),
                width,
                "{window} px, {stored:?}"
            );
        }
        assert_eq!(main_width(1440., Some(640), AgentPanel::Open), 736.);
        // The board keeps its minimum at the widest drag.
        assert_eq!(main_width(1280., Some(5000), AgentPanel::Open), BOARD_MIN);
    }

    /// T-0502: what a drag keeps in the settings: whole pixels, at least 320; the default width and the double-click
    /// keep nothing. The view hands it back.
    #[test]
    fn the_stored_width_is_whole_pixels_and_the_double_click_forgets_it() {
        assert_eq!(stored_agent_panel_width(Some(612.4)), Some(612));
        assert_eq!(stored_agent_panel_width(Some(120.)), Some(320));
        assert_eq!(stored_agent_panel_width(Some(420.2)), None);
        assert_eq!(stored_agent_panel_width(None), None);
        assert_eq!(stored_agent_panel_width(Some(f32::NAN)), None);
        // A width stored by hand under the minimum is shown at the minimum.
        assert_eq!(view(panel(612)).agent_panel, 612.);
        assert_eq!(view(panel(10)).agent_panel, 320.);
    }

    fn panel(width: u32) -> KeptWidths {
        KeptWidths {
            agent_panel: Some(width),
            ..KeptWidths::default()
        }
    }

    /// The sheet's and the Files tree's edges keep whole pixels inside their bounds; the double-click forgets them.
    /// The sheet has no single default, so any drag is kept; the tree's default keeps nothing.
    #[test]
    fn the_sheet_and_files_tree_widths_stay_between_their_bounds() {
        assert_eq!(stored_sheet_width(Some(812.6)), Some(813));
        assert_eq!(stored_sheet_width(Some(300.)), Some(560));
        assert_eq!(stored_sheet_width(Some(1040.)), Some(1040));
        assert_eq!(stored_sheet_width(None), None);
        assert_eq!(stored_files_tree_width(Some(360.4)), Some(360));
        assert_eq!(stored_files_tree_width(Some(50.)), Some(200));
        assert_eq!(stored_files_tree_width(Some(2000.)), Some(640));
        assert_eq!(stored_files_tree_width(Some(300.)), None);
        assert_eq!(stored_files_tree_width(Some(f32::NAN)), None);
        let kept = view(KeptWidths {
            agent_panel: None,
            sheet: Some(100),
            files_tree: Some(900),
        });
        assert_eq!((kept.sheet, kept.files_tree), (Some(560.), Some(640.)));
        let default = view(KeptWidths::default());
        assert_eq!((default.sheet, default.files_tree), (None, None));
        assert_eq!(
            (
                default.sheet_min,
                default.files_tree_min,
                default.files_tree_max
            ),
            (560., 200., 640.)
        );
    }

    /// T-0502: Expand lays the panel over the board at the window's width after the rail; the board under it keeps
    /// its open width, so nothing moves when it comes back.
    #[test]
    fn expanded_the_panel_covers_the_main_area() {
        assert_eq!(
            agent_panel_shown(1440., Some(640), AgentPanel::Expanded),
            1376.
        );
        assert_eq!(agent_panel_shown(1440., Some(640), AgentPanel::Open), 640.);
        assert_eq!(agent_panel_shown(1440., Some(640), AgentPanel::Closed), 0.);
        assert_eq!(
            main_width(1440., Some(640), AgentPanel::Expanded),
            main_width(1440., Some(640), AgentPanel::Open)
        );
    }
}

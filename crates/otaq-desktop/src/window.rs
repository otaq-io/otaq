//! The window's own chrome (UI-01, d0074; *02 · App shell → Title bar · macOS*, *Title bar · Windows & Linux*): the
//! header row is the title bar on every OS. macOS keeps its traffic lights, inset in the header row; Windows and Linux
//! get the app's own caption buttons at its right end. The window keeps its size and position between launches, in
//! the data folder, and is never smaller than 1280 × 800. The door makes the Tauri calls; the choices are made here.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use otaq_core::data_dir::DataDir;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The smallest window (*1280 × 800 · minimum*), in logical pixels.
pub const MIN_SIZE: (f64, f64) = (1280.0, 800.0);
/// The first launch's window, centered.
pub const FIRST_SIZE: (f64, f64) = (1440.0, 900.0);
/// Where the traffic lights sit: 14 px from the left edge, centered on the 44 px header's middle line (22 px).
///
/// The y isn't the buttons' top edge: tao makes the title bar `button height (16) + y` tall and leaves the buttons
/// 10 px above its bottom, so their center lands at `y − 2`. Measured in the running app (OT-1, tao 0.37.1, macOS
/// 27): y = 16 put the centers at 14 px; y = 24 puts the 16 px button frames at 14–30 and the drawn circles' center
/// on 22, through leaving full screen, resizing and focus changes (tao re-applies the inset itself on those).
#[cfg(target_os = "macos")]
pub const TRAFFIC_LIGHTS: (f64, f64) = (14.0, 24.0);
/// The header's height (`density::HEADER`): the part of the window that must stay on a screen.
const HEADER_HEIGHT: f64 = 44.0;

/// Which window controls the title bar carries on this OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "WindowChrome")]
pub enum Chrome {
    /// macOS: the system's traffic lights, inset at the start of the header row.
    TrafficLights,
    /// Windows and Linux: the app's own minimize, maximize/restore and close at the end of the header row.
    CaptionButtons,
}

impl Chrome {
    pub fn current() -> Self {
        Self::of(std::env::consts::OS)
    }

    fn of(os: &str) -> Self {
        if os == "macos" {
            Self::TrafficLights
        } else {
            Self::CaptionButtons
        }
    }
}

/// What a caption button does, or bringing the window forward (a notification click, T-0231).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "WindowAction")]
pub enum Action {
    Minimize,
    /// Maximizes, or restores a maximized window.
    ToggleMaximize,
    Close,
    /// Shows the window (restored if minimized) and gives it the keyboard.
    Focus,
    /// Quits the app: the Move window's Quit, and after its move (T-0579), when nothing else runs.
    Quit,
}

/// A rectangle in logical pixels: a window's place, or a screen's.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Where the window was when the app last quit: its normal (not maximized) place and whether it was maximized.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    #[serde(flatten)]
    pub bounds: Rect,
    pub maximized: bool,
}

/// How the window opens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opening {
    pub width: f64,
    pub height: f64,
    /// `None`: centered on the main screen.
    pub position: Option<(f64, f64)>,
    pub maximized: bool,
}

/// How the window opens, from where it was and the screens there are now: the same place, at least the minimum size,
/// unless its header would no longer be on any screen (a screen was unplugged); then centered.
pub fn opening(saved: Option<Placement>, screens: &[Rect]) -> Opening {
    let Some(saved) = saved else {
        return Opening {
            width: FIRST_SIZE.0,
            height: FIRST_SIZE.1,
            position: None,
            maximized: false,
        };
    };
    let b = saved.bounds;
    let header = Rect {
        height: HEADER_HEIGHT,
        ..b
    };
    // Enough of the header to grab it and drag the window back.
    let reachable = screens.iter().any(|s| {
        let w = (header.x + header.width).min(s.x + s.width) - header.x.max(s.x);
        let h = (header.y + header.height).min(s.y + s.height) - header.y.max(s.y);
        w >= 100.0 && h >= 20.0
    });
    Opening {
        width: b.width.max(MIN_SIZE.0),
        height: b.height.max(MIN_SIZE.1),
        position: reachable.then_some((b.x, b.y)),
        maximized: saved.maximized,
    }
}

/// The file that keeps the window's place, in the data folder.
pub fn placement_file(data: &DataDir) -> PathBuf {
    data.root().join("window.json")
}

/// The saved place; `None` when there is none or it can't be read (the window then opens as on the first launch).
pub fn load(file: &Path) -> Option<Placement> {
    serde_json::from_slice(&std::fs::read(file).ok()?).ok()
}

pub fn save(file: &Path, placement: &Placement) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        DataDir::ensure_dir(dir)?;
    }
    std::fs::write(file, serde_json::to_vec_pretty(placement)?)
}

/// The WebKitGTK switch the app sets before its webview exists (Q39, option 1): on Linux the window can freeze after
/// its first frame while WebKitGTK draws through DMABUF (some GPU drivers, VMs among them), so it is turned off unless
/// the user's environment already says (`WEBKIT_DISABLE_DMABUF_RENDERER=0` keeps it on). The variable to set, if any.
pub fn webkit_env(os: &str, existing: Option<&OsStr>) -> Option<(&'static str, &'static str)> {
    (os == "linux" && existing.is_none()).then_some((WEBKIT_DMABUF, "1"))
}

pub const WEBKIT_DMABUF: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };

    fn placed(x: f64, y: f64, width: f64, height: f64) -> Placement {
        Placement {
            bounds: Rect {
                x,
                y,
                width,
                height,
            },
            maximized: false,
        }
    }

    #[test]
    fn dmabuf_is_switched_off_on_linux_unless_the_environment_says() {
        assert_eq!(webkit_env("linux", None), Some((WEBKIT_DMABUF, "1")));
        assert_eq!(webkit_env("linux", Some(OsStr::new("0"))), None);
        assert_eq!(webkit_env("linux", Some(OsStr::new("1"))), None);
        assert_eq!(webkit_env("macos", None), None);
        assert_eq!(webkit_env("windows", None), None);
    }

    #[test]
    fn macos_keeps_its_traffic_lights_the_others_get_caption_buttons() {
        assert_eq!(Chrome::of("macos"), Chrome::TrafficLights);
        assert_eq!(Chrome::of("windows"), Chrome::CaptionButtons);
        assert_eq!(Chrome::of("linux"), Chrome::CaptionButtons);
    }

    #[test]
    fn the_first_launch_opens_centered_at_1440_by_900() {
        assert_eq!(
            opening(None, &[SCREEN]),
            Opening {
                width: 1440.0,
                height: 900.0,
                position: None,
                maximized: false
            }
        );
    }

    #[test]
    fn the_window_returns_where_it_was_never_below_the_minimum() {
        let o = opening(Some(placed(200.0, 120.0, 1500.0, 950.0)), &[SCREEN]);
        assert_eq!(
            (o.width, o.height, o.position),
            (1500.0, 950.0, Some((200.0, 120.0)))
        );

        let small = opening(Some(placed(10.0, 10.0, 900.0, 600.0)), &[SCREEN]);
        assert_eq!((small.width, small.height), (1280.0, 800.0));

        let maximized = Placement {
            maximized: true,
            ..placed(10.0, 10.0, 1300.0, 820.0)
        };
        assert!(opening(Some(maximized), &[SCREEN]).maximized);
    }

    #[test]
    fn a_window_whose_header_is_off_every_screen_opens_centered() {
        // A second screen to the right, since unplugged.
        let gone = placed(2100.0, 100.0, 1400.0, 900.0);
        assert_eq!(opening(Some(gone), &[SCREEN]).position, None);
        let second = Rect {
            x: 1920.0,
            ..SCREEN
        };
        assert_eq!(
            opening(Some(gone), &[SCREEN, second]).position,
            Some((2100.0, 100.0))
        );
        // Header above the screen's top, or only a sliver of it left at the edge.
        assert_eq!(
            opening(Some(placed(100.0, -500.0, 1400.0, 900.0)), &[SCREEN]).position,
            None
        );
        assert_eq!(
            opening(Some(placed(1850.0, 100.0, 1400.0, 900.0)), &[SCREEN]).position,
            None
        );
        // Partly off the edge but still easy to grab.
        assert_eq!(
            opening(Some(placed(-600.0, 50.0, 1400.0, 900.0)), &[SCREEN]).position,
            Some((-600.0, 50.0))
        );
    }

    #[test]
    fn the_place_is_saved_and_read_back_and_a_bad_file_counts_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let file = placement_file(&DataDir::new(dir.path().join("data")));
        assert_eq!(load(&file), None);
        let placement = Placement {
            maximized: true,
            ..placed(-30.5, 40.0, 1300.0, 820.0)
        };
        save(&file, &placement).unwrap();
        assert_eq!(load(&file), Some(placement));
        std::fs::write(&file, "not json").unwrap();
        assert_eq!(load(&file), None);
    }
}

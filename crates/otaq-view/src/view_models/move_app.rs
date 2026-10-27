//! **Move Otaq to Applications** (T-0579; UPD-06; d0140 gap review G3; page 32 item 6, artboard `s32-03`): on macOS a
//! release runs only from an Applications folder. Opened from the disk image, or from a copy macOS runs in a hidden
//! temporary folder (an app opened from Downloads), the window shows only this: no board, no service, no agents, no
//! update check. **Move to Applications** copies it to `/Applications` (or `~/Applications`) and reopens it from
//! there; **Quit** is the other choice. A move that fails says why and points to dragging it in Finder: **Try
//! again**, **Show in Finder**.

use otaq_core::naming::PRODUCT_NAME;
use serde::Serialize;
use ts_rs::TS;

pub const QUIT: &str = "Quit";
pub const MOVE: &str = "Move to Applications";
pub const TRY_AGAIN: &str = "Try again";
pub const SHOW_IN_FINDER: &str = "Show in Finder";

/// Where the app runs from, as the window says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A mounted disk image (`/Volumes/…`).
    DiskImage,
    /// Anywhere else outside an Applications folder (a copy macOS translocated, a folder of the user's).
    Elsewhere,
}

/// Why the move failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Neither `/Applications` nor `~/Applications` could be written.
    NoPermission,
    /// Anything else (a full disk, say), with the system's message.
    Other(String),
}

/// The window (page 32 item 6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "MoveAppView")]
pub struct View {
    /// The title bar's "Otaq".
    pub window_title: String,
    /// "Move Otaq to Applications", or "Otaq couldn't be moved".
    pub title: String,
    /// The red line of a failed move.
    pub error: Option<String>,
    pub text: String,
    pub quit: String,
    /// "Move to Applications" (the main button), before a failed move.
    pub move_to: Option<String>,
    /// After a failed move: "Try again" and "Show in Finder" (the main button).
    pub try_again: Option<String>,
    pub show_in_finder: Option<String>,
}

/// The window for an app running from `source`, after `failure` if the last move failed.
pub fn view(source: Source, failure: Option<&Failure>) -> View {
    let Some(failure) = failure else {
        return View {
            window_title: PRODUCT_NAME.to_owned(),
            title: format!("Move {PRODUCT_NAME} to Applications"),
            error: None,
            text: format!(
                "{PRODUCT_NAME} is running from {}. It runs only from an Applications folder, so the board, agents \
                 and updates start once it\u{2019}s there. An older {PRODUCT_NAME} in Applications is replaced.",
                match source {
                    Source::DiskImage => "the disk image",
                    // Stand-in: page 32 draws only the disk image.
                    Source::Elsewhere => "outside an Applications folder",
                }
            ),
            quit: QUIT.to_owned(),
            move_to: Some(MOVE.to_owned()),
            try_again: None,
            show_in_finder: None,
        };
    };
    View {
        window_title: PRODUCT_NAME.to_owned(),
        title: format!("{PRODUCT_NAME} couldn\u{2019}t be moved"),
        error: Some(match failure {
            Failure::NoPermission => {
                "You don\u{2019}t have permission to change /Applications or ~/Applications."
                    .to_owned()
            }
            // Stand-in: page 32 draws only the permission failure.
            Failure::Other(reason) => {
                format!("{PRODUCT_NAME} couldn\u{2019}t be copied to Applications: {reason}")
            }
        }),
        text: match source {
            Source::DiskImage => format!(
                "Drag {PRODUCT_NAME} from the disk image\u{2019}s window onto its Applications shortcut, then open \
                 it from Applications."
            ),
            // Stand-in: page 32 draws only the disk image.
            Source::Elsewhere => format!(
                "Drag {PRODUCT_NAME} into the Applications folder in Finder, then open it from Applications."
            ),
        },
        quit: QUIT.to_owned(),
        move_to: None,
        try_again: Some(TRY_AGAIN.to_owned()),
        show_in_finder: Some(SHOW_IN_FINDER.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Page 32 item 6, word for word, from the disk image.
    #[test]
    fn the_window_reads_as_page_32() {
        let v = view(Source::DiskImage, None);
        assert_eq!(v.title, format!("Move {PRODUCT_NAME} to Applications"));
        assert_eq!(
            v.text,
            format!(
                "{PRODUCT_NAME} is running from the disk image. It runs only from an Applications folder, so the \
                 board, agents and updates start once it’s there. An older {PRODUCT_NAME} in Applications is \
                 replaced."
            )
        );
        assert_eq!(v.quit, "Quit");
        assert_eq!(v.move_to.as_deref(), Some("Move to Applications"));
        assert_eq!((v.try_again, v.show_in_finder, v.error), (None, None, None));

        let failed = view(Source::DiskImage, Some(&Failure::NoPermission));
        assert_eq!(failed.title, format!("{PRODUCT_NAME} couldn’t be moved"));
        assert_eq!(
            failed.error.as_deref(),
            Some("You don’t have permission to change /Applications or ~/Applications.")
        );
        assert_eq!(
            failed.text,
            format!(
                "Drag {PRODUCT_NAME} from the disk image’s window onto its Applications shortcut, then open it \
                 from Applications."
            )
        );
        assert_eq!(failed.move_to, None);
        assert_eq!(failed.try_again.as_deref(), Some("Try again"));
        assert_eq!(failed.show_in_finder.as_deref(), Some("Show in Finder"));
    }

    #[test]
    fn elsewhere_and_other_failures() {
        let v = view(Source::Elsewhere, None);
        assert!(v.text.starts_with(&format!(
            "{PRODUCT_NAME} is running from outside an Applications folder. "
        )));
        let failed = view(
            Source::Elsewhere,
            Some(&Failure::Other("No space left on device".into())),
        );
        assert_eq!(
            failed.error.unwrap(),
            format!("{PRODUCT_NAME} couldn’t be copied to Applications: No space left on device")
        );
        assert!(failed.text.contains("in Finder"));
    }
}

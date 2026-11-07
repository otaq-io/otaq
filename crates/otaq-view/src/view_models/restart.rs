//! The service restart confirmation (*11 · Banners → Service restart · confirm*): it names exactly who gets
//! interrupted, and only the confirm button restarts (d0022: the user picks the moment).

use serde::Serialize;
use ts_rs::TS;

/// The dialog's texts as the page draws them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "RestartDialog")]
pub struct RestartDialogView {
    pub title: String,
    pub message: String,
    pub detail: String,
    pub confirm: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestartDialog {
    pub working: u32,
    pub waiting: u32,
}

/// The success toast after a restart (or a start) the user asked for, when no summary banner says more.
pub const RESTARTED: &str = "Background service restarted";
pub const RESTARTED_TEXT: &str = "Connected to the new service";
/// The error toast when it failed; its text is the reason.
pub const RESTART_FAILED: &str = "Couldn’t restart the background service";

impl RestartDialog {
    pub const TITLE: &str = "Restart the background service?";
    pub const DETAIL: &str = "Waiting tickets keep their questions. Review and Approved tickets aren’t touched. Run and Terminal processes stop.";
    pub const CONFIRM: &str = "Restart service";

    /// "3 working agents and 2 waiting ones will be interrupted. You can Resume all afterwards."
    pub fn message(&self) -> String {
        match (self.working, self.waiting) {
            (0, 0) => {
                "No agents are working or waiting, so nothing will be interrupted.".to_owned()
            }
            (working, waiting) => {
                let working = match working {
                    1 => "1 working agent".to_owned(),
                    n => format!("{n} working agents"),
                };
                let waiting = match waiting {
                    1 => "1 waiting one".to_owned(),
                    n => format!("{n} waiting ones"),
                };
                format!(
                    "{working} and {waiting} will be interrupted. You can Resume all afterwards."
                )
            }
        }
    }

    pub fn view(&self) -> RestartDialogView {
        RestartDialogView {
            title: Self::TITLE.to_owned(),
            message: self.message(),
            detail: Self::DETAIL.to_owned(),
            confirm: Self::CONFIRM.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_who_is_interrupted() {
        assert_eq!(
            RestartDialog {
                working: 3,
                waiting: 2
            }
            .message(),
            "3 working agents and 2 waiting ones will be interrupted. You can Resume all afterwards."
        );
        assert_eq!(
            RestartDialog {
                working: 1,
                waiting: 1
            }
            .message(),
            "1 working agent and 1 waiting one will be interrupted. You can Resume all afterwards."
        );
        assert!(
            RestartDialog {
                working: 0,
                waiting: 0
            }
            .message()
            .starts_with("No agents")
        );
    }

    #[test]
    fn the_page_gets_the_frames_texts() {
        let view = RestartDialog {
            working: 3,
            waiting: 2,
        }
        .view();
        assert_eq!(view.title, "Restart the background service?");
        assert_eq!(
            view.detail,
            "Waiting tickets keep their questions. Review and Approved tickets aren’t touched. Run and Terminal processes stop."
        );
        assert_eq!(view.confirm, "Restart service");
        assert!(
            view.message
                .starts_with("3 working agents and 2 waiting ones")
        );
    }
}

//! **Quitting** (T-0569; TRY-03, TRY-07; d0139 R2, R5, G2, C4; page 31 items 4 and 5, the notes `s31-04` "NEW
//! WORDING", audit AS finding 2): the ⌘Q question while agents work, and what the window says when Stop agents and
//! quit didn't quit. The steps themselves (the service's `service.stop_agents`, waiting for it to exit) are the app's.
//!
//! - **⌘Q / Dock → Quit with agents working** asks "2 agents are working": **Keep them working and quit** (first: what
//!   ⌘Q means elsewhere on macOS), **Stop agents and quit**, **Cancel**. With nothing working, or no service to ask,
//!   it quits at once.
//! - **A failed stop** brings the window back with "Otaq didn't quit": the service couldn't be reached (nothing was
//!   stopped), or it hadn't exited within [`STOP_WAIT_SECS`] (the pause is stored; the ticket whose agent was still
//!   running is named, and "may still be working" is said only then, since the service couldn't confirm the end).
//!   **Try again**, **Open SHOP-18** (when a ticket is named), **Not now**.

use otaq_core::naming::PRODUCT_NAME;

/// How long the app waits for the service to exit after `service.stop_agents` answered. The service ends an agent
/// that outlives its grace period hard, so a wait this long means the service itself didn't finish.
pub const STOP_WAIT_SECS: u64 = 20;

pub const KEEP_WORKING_AND_QUIT: &str = "Keep them working and quit";
pub const STOP_AGENTS_AND_QUIT: &str = "Stop agents and quit";
pub const CANCEL: &str = "Cancel";
pub const TRY_AGAIN: &str = "Try again";
pub const NOT_NOW: &str = "Not now";

/// The ⌘Q question (page 31 item 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuitQuestion {
    /// "2 agents are working".
    pub title: String,
    pub text: String,
    /// In order: Keep them working and quit, Stop agents and quit, Cancel.
    pub keep: &'static str,
    pub stop: &'static str,
    pub cancel: &'static str,
}

/// What the user picked in [`QuitQuestion`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitAnswer {
    /// Quit the app only; the agents keep working in the service (d0022).
    KeepWorking,
    StopAgents,
    Cancel,
}

/// The question when `working` agents work; `None` quits at once (nothing working, or the service can't say).
pub fn quit_question(working: Option<u32>) -> Option<QuitQuestion> {
    let n = working.filter(|n| *n > 0)?;
    Some(QuitQuestion {
        title: if n == 1 {
            "1 agent is working".to_owned()
        } else {
            format!("{n} agents are working")
        },
        text: format!(
            "{} keep working in the background with no window and no icon to show it, or stop {} and pause every \
             project so nothing starts until you resume.",
            if n == 1 { "It" } else { "They" },
            if n == 1 { "it" } else { "them" }
        ),
        keep: KEEP_WORKING_AND_QUIT,
        stop: STOP_AGENTS_AND_QUIT,
        cancel: CANCEL,
    })
}

/// Why Stop agents and quit didn't quit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopFailure {
    /// The service couldn't be reached or refused: no pause stored, nothing stopped.
    Unreachable,
    /// The pause is stored, but the service hadn't exited within [`STOP_WAIT_SECS`]. `still` names the ticket whose
    /// agent was still running then, with its tool ("SHOP-18", "Codex"), when the service could say.
    StillRunning { still: Option<(String, String)> },
}

/// "Otaq didn't quit" (page 31 item 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedStop {
    pub title: String,
    pub text: String,
    /// In order: Try again, Open SHOP-18 (when a ticket is named), Not now.
    pub try_again: &'static str,
    /// "Open SHOP-18" and the ticket's key.
    pub open: Option<(String, String)>,
    pub not_now: &'static str,
}

pub fn failed_stop(failure: &StopFailure) -> FailedStop {
    let log = "Try again; if it keeps failing, Settings \u{2192} About \u{2192} Show log says what went wrong.";
    let (text, open) = match failure {
        StopFailure::Unreachable => (
            format!(
                "{PRODUCT_NAME} can\u{2019}t reach its background service, so it couldn\u{2019}t store the pause or \
                 stop any agent. Nothing was stopped and agents may still be working. {log}"
            ),
            None,
        ),
        StopFailure::StillRunning {
            still: Some((key, tool)),
        } => (
            format!(
                "All projects are paused, but {tool} in {key} didn\u{2019}t stop within {STOP_WAIT_SECS} seconds and \
                 may still be working. Stop it from {key}\u{2019}s terminal, then try again."
            ),
            Some((format!("Open {key}"), key.clone())),
        ),
        StopFailure::StillRunning { still: None } => (
            format!(
                "All projects are paused, but the background service didn\u{2019}t stop within {STOP_WAIT_SECS} \
                 seconds, so agents may still be working. {log}"
            ),
            None,
        ),
    };
    FailedStop {
        title: format!("{PRODUCT_NAME} didn\u{2019}t quit"),
        text,
        try_again: TRY_AGAIN,
        open,
        not_now: NOT_NOW,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(text: &str) -> String {
        text.replace('@', PRODUCT_NAME)
    }

    #[test]
    fn quit_asks_only_while_agents_work() {
        assert_eq!(quit_question(None), None, "no service to ask: quit");
        assert_eq!(quit_question(Some(0)), None);
        let q = quit_question(Some(2)).unwrap();
        assert_eq!(q.title, "2 agents are working");
        assert_eq!(
            q.text,
            "They keep working in the background with no window and no icon to show it, or stop them and pause \
             every project so nothing starts until you resume."
        );
        assert_eq!(
            [q.keep, q.stop, q.cancel],
            [
                "Keep them working and quit",
                "Stop agents and quit",
                "Cancel"
            ]
        );
        let one = quit_question(Some(1)).unwrap();
        assert_eq!(one.title, "1 agent is working");
        assert!(one.text.starts_with("It keep"), "{}", one.text);
    }

    #[test]
    fn a_failed_stop_says_why() {
        let lost = failed_stop(&StopFailure::Unreachable);
        assert_eq!(lost.title, o("@ didn\u{2019}t quit"));
        assert!(
            lost.text
                .starts_with(&o("@ can\u{2019}t reach its background service"))
        );
        assert!(lost.text.contains("Nothing was stopped"));
        assert_eq!(lost.open, None);
        assert_eq!((lost.try_again, lost.not_now), ("Try again", "Not now"));

        let codex = failed_stop(&StopFailure::StillRunning {
            still: Some(("SHOP-18".into(), "Codex".into())),
        });
        assert_eq!(
            codex.text,
            "All projects are paused, but Codex in SHOP-18 didn\u{2019}t stop within 20 seconds and may still be \
             working. Stop it from SHOP-18\u{2019}s terminal, then try again."
        );
        assert_eq!(
            codex.open,
            Some(("Open SHOP-18".to_owned(), "SHOP-18".to_owned()))
        );

        let service = failed_stop(&StopFailure::StillRunning { still: None });
        assert!(
            service
                .text
                .contains("didn\u{2019}t stop within 20 seconds")
        );
        assert_eq!(service.open, None);
    }
}

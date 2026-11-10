//! **Quitting** (T-0569; TRY-03, TRY-07; d0139 R2, R5, G2, C4): the question while agents work, and what the window
//! says when Quit and stop agents didn't quit. The steps themselves (the service's `service.stop_agents`, waiting for
//! it to exit) are the app's.
//!
//! - **⌘Q, the menu bar's or the tray's Quit Otaq, with agents working** (as Running counts them: a checker at work
//!   too) asks "Quit Otaq?", on the window, brought up first when it was closed to the tray: **Keep them running**
//!   (the default: the agents keep working in the background), **Quit and stop agents** (the clear second choice),
//!   **Cancel**. With nothing working, or no service to ask, it quits at once.
//! - **A failed stop** brings the window back with "Otaq didn't quit": the service couldn't be reached (nothing was
//!   stopped), or it hadn't exited within [`STOP_WAIT_SECS`] (the pause is stored; the ticket whose agent was still
//!   running is named, and "may still be working" is said only then, since the service couldn't confirm the end).
//!   **Try again**, **Open SHOP-18** (when a ticket is named), **Not now**.

use otaq_core::naming::PRODUCT_NAME;

/// How long the app waits for the service to exit after `service.stop_agents` answered. The service ends an agent
/// that outlives its grace period hard, so a wait this long means the service itself didn't finish.
pub const STOP_WAIT_SECS: u64 = 20;

pub const KEEP_WORKING_AND_QUIT: &str = "Keep them running";
pub const STOP_AGENTS_AND_QUIT: &str = "Quit and stop agents";
pub const CANCEL: &str = "Cancel";
pub const TRY_AGAIN: &str = "Try again";
pub const NOT_NOW: &str = "Not now";

/// The ⌘Q question (page 31 item 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuitQuestion {
    /// "Quit Otaq?".
    pub title: String,
    /// "2 agents are working. Quitting keeps them running in the background; …".
    pub text: String,
    /// In order: Keep them running (the default), Quit and stop agents, Cancel.
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
    let (agents, them, they_are) = if n == 1 {
        ("1 agent is working".to_owned(), "it", "it is")
    } else {
        (format!("{n} agents are working"), "them", "they are")
    };
    Some(QuitQuestion {
        title: format!("Quit {PRODUCT_NAME}?"),
        text: format!(
            "{agents}. Quitting keeps {them} running in the background; {STOP_AGENTS_AND_QUIT} stops {them} where \
             {they_are} (Resume later)."
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
        let q = quit_question(Some(4)).unwrap();
        assert_eq!(q.title, o("Quit @?"));
        assert_eq!(
            q.text,
            "4 agents are working. Quitting keeps them running in the background; Quit and stop agents stops them \
             where they are (Resume later)."
        );
        assert_eq!(
            [q.keep, q.stop, q.cancel],
            ["Keep them running", "Quit and stop agents", "Cancel"]
        );
        let one = quit_question(Some(1)).unwrap();
        assert!(
            one.text
                .starts_with("1 agent is working. Quitting keeps it running"),
            "{}",
            one.text
        );
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

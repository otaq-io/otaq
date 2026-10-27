//! Closing the window when there is no tray to keep Otaq in (d0139 gap review G8, TRY-08; page 31 item 9): stock
//! GNOME shows no tray icons, so a hidden window couldn't be found again. There, closing acts as Stop agents and quit
//! and always stores the All projects pause; with agents working it asks first. With a tray (macOS and Windows
//! always; Linux when a tray host runs and the icon was made), closing hides the window. The tray is checked at every
//! close, so one that appears later counts from the next close.

use otaq_core::naming::PRODUCT_NAME;

/// What closing the window does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnClose {
    /// The window hides; Otaq keeps running in the tray.
    Hide,
    /// Stop agents and quit, at once (nothing is working; the pause is still stored).
    StopAndQuit,
    /// Ask first ([`NoTrayQuestion`]); its Stop and quit then stops agents and quits.
    Ask(NoTrayQuestion),
}

/// The close decision: `tray` is whether a tray shows the icon now, `agents` the agents working.
pub fn on_close(tray: bool, agents: u32) -> OnClose {
    match (tray, agents) {
        (true, _) => OnClose::Hide,
        (false, 0) => OnClose::StopAndQuit,
        (false, n) => OnClose::Ask(NoTrayQuestion::new(n)),
    }
}

/// The question on close without a tray (page 31 item 9): "Stop 2 agents and quit?".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoTrayQuestion {
    pub title: String,
    pub text: String,
    pub stop: String,
    pub cancel: String,
}

impl NoTrayQuestion {
    pub fn new(agents: u32) -> Self {
        let agents = match agents {
            1 => "1 agent".to_owned(),
            n => format!("{n} agents"),
        };
        Self {
            title: format!("Stop {agents} and quit?"),
            text: format!(
                "This desktop has no tray, so {PRODUCT_NAME} can\u{2019}t keep running out of sight. Closing stops \
                 every agent and pauses all projects; Resume continues them next time."
            ),
            stop: "Stop and quit".into(),
            cancel: "Cancel".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-0570 (TRY-08): tray / no tray × agents working / not.
    #[test]
    fn the_close_decision() {
        for (tray, agents, expected) in [
            (true, 0, OnClose::Hide),
            (true, 2, OnClose::Hide),
            (false, 0, OnClose::StopAndQuit),
            (false, 2, OnClose::Ask(NoTrayQuestion::new(2))),
            (false, 1, OnClose::Ask(NoTrayQuestion::new(1))),
        ] {
            assert_eq!(
                on_close(tray, agents),
                expected,
                "tray {tray}, {agents} agents"
            );
        }
    }

    #[test]
    fn the_question_reads_as_page_31() {
        let q = NoTrayQuestion::new(2);
        assert_eq!(q.title, "Stop 2 agents and quit?");
        assert_eq!(
            q.text,
            format!(
                "This desktop has no tray, so {PRODUCT_NAME} can\u{2019}t keep running out of sight. Closing stops \
                 every agent and pauses all projects; Resume continues them next time."
            )
        );
        assert_eq!(
            (q.stop.as_str(), q.cancel.as_str()),
            ("Stop and quit", "Cancel")
        );
        assert_eq!(NoTrayQuestion::new(1).title, "Stop 1 agent and quit?");
    }
}

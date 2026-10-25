//! The Terminals drawer (RUN-06, SHL-04, UI-09; d0068, d0019, d0087; *17 · Project terminals*): one tab per Project
//! terminal of the project, by number, a terminal's header state, the path line, the hint in a new terminal's empty
//! scrollback, the line under an ended one and which tab Terminals (⌃`) shows. Each terminal is a plain shell in the project's
//! folder, owned by the service. Project agents live in the agent panel since d0087 (`agent_panel.rs`), never here.

use otaq_core::session::{END_STOPPED_BY_RESTART, SessionInfo};
use otaq_core::terminals::shell::ProjectTerminal;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;

/// The drawer's height when it opens (*Empty · shell prompt*: 380 px).
pub const DRAWER_HEIGHT: f32 = 380.;
/// The header's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Opening,
    /// The shell runs: "Shell".
    Live,
    /// The user exited the shell: "Ended", with Reopen.
    Ended,
    /// A restart ended it: "Stopped by restart", with Reopen.
    StoppedByRestart,
}

impl State {
    pub fn of(session: Option<&SessionInfo>) -> Self {
        match session {
            None => Self::Opening,
            Some(s) if s.is_running() => Self::Live,
            Some(s) if s.end_reason.as_deref() == Some(END_STOPPED_BY_RESTART) => {
                Self::StoppedByRestart
            }
            Some(_) => Self::Ended,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Opening => "Opening…",
            Self::Live => "Shell",
            Self::Ended => "Ended",
            Self::StoppedByRestart => "Stopped by restart",
        }
    }

    /// Reopen is a click (nothing reopens on its own, d0045).
    pub fn offers_reopen(self) -> bool {
        matches!(self, Self::Ended | Self::StoppedByRestart)
    }
}

/// "~/code/shop-web · plain shell · full otaq access".
pub fn path_line(short_repo: &str) -> String {
    format!(
        "{short_repo} · plain shell · full {} access",
        otaq_core::naming::COMMAND_NAME
    )
}

/// The hint in a new terminal's empty scrollback, until the first keystroke (*17 → Empty · shell prompt*):
/// "Terminal 1 · shop-web · a shell in ~/code/shop-web", how to make tickets yourself, and where the agent is.
/// `focus_keys` is the agent panel's ⌘L on this platform.
pub fn hint(
    terminal: &str,
    project_name: &str,
    short_repo: &str,
    focus_keys: Option<&str>,
) -> Vec<String> {
    let cb = otaq_core::naming::COMMAND_NAME;
    let keys = focus_keys.map(|k| format!(" ({k})")).unwrap_or_default();
    vec![
        format!("{terminal} \u{b7} {project_name} \u{b7} a shell in {short_repo}"),
        "Run project commands, or make tickets yourself:".to_owned(),
        format!("  \u{203a} {cb} create \"Wishlist share link\""),
        format!("Want an agent\u{2019}s help? Ask Noyan in the Ask bar{keys}."),
    ]
}

/// Whether the hint shows: an open shell nobody typed into yet.
pub fn shows_hint(state: State, typed: bool) -> bool {
    state == State::Live && !typed
}

/// One tab of the drawer's strip: a Project terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DrawerTab")]
pub struct Tab {
    /// `terminal:<session>`; what the page selects and closes by.
    pub id: String,
    /// "Terminal 1".
    pub label: String,
    /// " · exited".
    pub note: Option<&'static str>,
    pub icon: IconName,
    /// The session its terminal attaches to.
    pub session: String,
    /// Its shell ended: the scrollback stays, read-only.
    pub ended: bool,
    /// The hint in its empty scrollback while it is open (`hint`); empty once it ended.
    pub hint: Vec<String>,
    /// The line under an ended terminal's scrollback.
    pub ended_line: Option<String>,
}

pub fn terminal_tab_id(session: &str) -> String {
    format!("terminal:{session}")
}

/// "── shell exited 11:20 · scrollback kept · close the tab, or + → New terminal ──".
pub fn ended_line(ended_at: Option<i64>) -> String {
    let at = ended_at
        .map(|t| format!(" {}", crate::view_models::clock::local_hhmm(t)))
        .unwrap_or_default();
    format!(
        "\u{2500}\u{2500} shell exited{at} \u{b7} scrollback kept \u{b7} close the tab, or + \u{2192} New terminal \u{2500}\u{2500}"
    )
}

/// What the hint names besides the terminal: the project, its folder and the agent panel's focus keys.
pub struct HintFacts<'a> {
    pub project_name: &'a str,
    pub short_repo: &'a str,
    pub focus_keys: Option<&'a str>,
}

fn terminal_tab(terminal: &ProjectTerminal, facts: &HintFacts) -> Tab {
    let ended = !terminal.session.is_running();
    Tab {
        id: terminal_tab_id(&terminal.session.id),
        label: terminal.name.clone(),
        note: ended.then_some(" \u{b7} exited"),
        icon: IconName::SquareTerminal,
        session: terminal.session.id.clone(),
        ended,
        hint: if ended {
            Vec::new()
        } else {
            hint(
                &terminal.name,
                facts.project_name,
                facts.short_repo,
                facts.focus_keys,
            )
        },
        ended_line: ended.then(|| ended_line(terminal.session.ended_at)),
    }
}

/// The strip: the project's terminals by number.
pub fn tabs(terminals: &[ProjectTerminal], facts: &HintFacts) -> Vec<Tab> {
    let mut terminals: Vec<&ProjectTerminal> = terminals.iter().collect();
    terminals.sort_by_key(|t| t.number);
    terminals
        .into_iter()
        .map(|t| terminal_tab(t, facts))
        .collect()
}

/// What Terminals (⌃`) shows: the last-used terminal while it is open, else the newest open one, else a new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LastUsed {
    Show(String),
    OpenNew,
}

pub fn last_used(terminals: &[ProjectTerminal], remembered: Option<&str>) -> LastUsed {
    if let Some(tab) = remembered.filter(|r| {
        terminals
            .iter()
            .any(|t| terminal_tab_id(&t.session.id) == *r)
    }) {
        return LastUsed::Show(tab.to_owned());
    }
    terminals
        .iter()
        .max_by_key(|t| t.number)
        .map_or(LastUsed::OpenNew, |t| {
            LastUsed::Show(terminal_tab_id(&t.session.id))
        })
}

/// + in the strip: a new terminal straight away (d0087: no New Project agent here; that is the agent panel's +).
pub const NEW_TERMINAL: &str = "New terminal";

/// The empty drawer (no tab open, for example after a restart closed every terminal, *17 → After a restart*).
pub fn empty_lines(short_repo: &str) -> Vec<String> {
    vec![format!(
        "Nothing open. + \u{2192} New terminal opens a fresh shell in {short_repo}."
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::session::{SessionKind, SessionState};

    fn session(state: SessionState, reason: Option<&str>) -> SessionInfo {
        SessionInfo {
            id: "p".into(),
            kind: SessionKind::Project,
            project: "SHOP".into(),
            ticket: None,
            state,
            started_at: 0,
            ended_at: None,
            end_reason: reason.map(str::to_owned),
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            model_ran: None,
            size: None,
        }
    }

    #[test]
    fn states() {
        assert_eq!(State::of(None), State::Opening);
        let live = State::of(Some(&session(SessionState::Running, None)));
        assert_eq!((live, live.label()), (State::Live, "Shell"));
        assert!(!live.offers_reopen());
        let ended = State::of(Some(&session(SessionState::Ended, Some("exit 0"))));
        assert_eq!((ended, ended.label()), (State::Ended, "Ended"));
        assert!(ended.offers_reopen());
        let restart = State::of(Some(&session(
            SessionState::Ended,
            Some(END_STOPPED_BY_RESTART),
        )));
        assert_eq!(restart, State::StoppedByRestart);
        assert!(restart.offers_reopen());
    }

    fn terminal(number: u32, id: &str, state: SessionState) -> ProjectTerminal {
        ProjectTerminal {
            number,
            name: format!("Terminal {number}"),
            session: SessionInfo {
                id: id.into(),
                ..session(state, None)
            },
        }
    }

    const FACTS: HintFacts = HintFacts {
        project_name: "shop-web",
        short_repo: "~/code/shop-web",
        focus_keys: Some("\u{2318}L"),
    };

    #[test]
    fn the_tabs_hold_terminals_only_by_number() {
        let terminals = vec![
            terminal(2, "t2", SessionState::Ended),
            terminal(1, "t1", SessionState::Running),
        ];
        let strip = tabs(&terminals, &FACTS);
        let labels: Vec<&str> = strip.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, ["Terminal 1", "Terminal 2"]);
        assert!(
            strip
                .iter()
                .all(|t| t.id.starts_with("terminal:") && t.icon == IconName::SquareTerminal)
        );
        assert_eq!(
            (strip[0].id.as_str(), strip[0].session.as_str()),
            ("terminal:t1", "t1")
        );
        assert!(!strip[0].ended && strip[0].ended_line.is_none());
        assert_eq!(
            strip[0].hint[0],
            "Terminal 1 \u{b7} shop-web \u{b7} a shell in ~/code/shop-web"
        );
        assert_eq!(strip[1].note, Some(" \u{b7} exited"));
        assert!(strip[1].hint.is_empty(), "an exited shell has no hint");
        assert!(
            strip[1]
                .ended_line
                .as_ref()
                .unwrap()
                .contains("scrollback kept")
        );
    }

    #[test]
    fn shift_cmd_t_shows_the_last_used_terminal_or_a_new_one() {
        assert_eq!(last_used(&[], None), LastUsed::OpenNew);
        assert_eq!(last_used(&[], Some("terminal:gone")), LastUsed::OpenNew);
        let open = vec![
            terminal(1, "t1", SessionState::Running),
            terminal(2, "t2", SessionState::Running),
        ];
        assert_eq!(
            last_used(&open, Some("terminal:t1")),
            LastUsed::Show("terminal:t1".into())
        );
        assert_eq!(
            last_used(&open, Some("terminal:gone")),
            LastUsed::Show("terminal:t2".into()),
            "a closed one: the newest terminal"
        );
        assert_eq!(last_used(&open, None), LastUsed::Show("terminal:t2".into()));
    }

    #[test]
    fn the_hint_makes_tickets_yourself_and_points_at_the_agent_panel() {
        assert!(shows_hint(State::Live, false));
        assert!(!shows_hint(State::Live, true));
        assert!(!shows_hint(State::Ended, false));
        let cb = otaq_core::naming::COMMAND_NAME;
        assert_eq!(
            hint(
                "Terminal 1",
                "shop-web",
                "~/code/shop-web",
                Some("\u{2318}L")
            ),
            [
                "Terminal 1 \u{b7} shop-web \u{b7} a shell in ~/code/shop-web".to_owned(),
                "Run project commands, or make tickets yourself:".to_owned(),
                format!("  \u{203a} {cb} create \"Wishlist share link\""),
                "Want an agent\u{2019}s help? Ask Noyan in the Ask bar (\u{2318}L).".to_owned(),
            ]
        );
        let lines = hint("Terminal 2", "shop-web", "~/code/shop-web", None);
        assert!(lines[3].ends_with("in the Ask bar."));
        assert!(
            lines.iter().all(|l| !l.contains("claude")),
            "no agent is started from here"
        );
        assert_eq!(
            path_line("~/code/shop-web"),
            format!("~/code/shop-web · plain shell · full {cb} access")
        );
        assert_eq!(
            empty_lines("~/code/shop-web"),
            ["Nothing open. + \u{2192} New terminal opens a fresh shell in ~/code/shop-web."]
        );
    }
}

//! The Shell tab's strip and states (RUN-05; *13 · Run & Shell → Shell tab · BILL-4*): "Terminal · not the agent", the
//! worktree, "Run port · $PORT=4004", and the stopped-by-restart state with its Reopen click (d0045).

use otaq_core::session::{END_STOPPED_BY_RESTART, SessionInfo};

/// The strip's label: the Shell is never the agent.
pub const SHELL_LABEL: &str = "Terminal · not the agent";

/// "Run port · $PORT=4004".
pub fn port_label(port: u16) -> String {
    format!("Run port · $PORT={port}")
}

/// The Shell's ports (agents run the app): one app's "Run port · $PORT=4004"; several, "Run ports · OTAQ_PORT_WEB=4020 ·
/// OTAQ_PORT_API=4021" (the Shell has no `$PORT` then); none before the ticket has one.
pub fn ports_label(ports: &[(String, u16)]) -> Option<String> {
    match ports {
        [] => None,
        [(_, port)] => Some(port_label(*port)),
        several => Some(format!(
            "Run ports · {}",
            several
                .iter()
                .map(|(app, port)| format!("{}={port}", otaq_core::naming::app_port_env_var(app)))
                .collect::<Vec<_>>()
                .join(" · ")
        )),
    }
}

/// A terminal a restart stopped: its title and note, for the state shown instead of an empty terminal.
pub fn stopped_note(
    session: &SessionInfo,
    hhmm: impl Fn(i64) -> String,
) -> Option<(String, String)> {
    if session.is_running() || session.end_reason.as_deref() != Some(END_STOPPED_BY_RESTART) {
        return None;
    }
    Some((
        format!(
            "Stopped by restart · was open at {}",
            hhmm(session.started_at)
        ),
        "A restart ended this shell. It never reopens on its own.".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Agents run the app (T4): one app's Shell has `$PORT`; with several, only each app's variable.
    #[test]
    fn the_ports_label_follows_the_apps() {
        assert_eq!(ports_label(&[]), None);
        assert_eq!(
            ports_label(&[("app".into(), 4004)]).as_deref(),
            Some("Run port · $PORT=4004")
        );
        assert_eq!(
            ports_label(&[("web".into(), 4020), ("api".into(), 4021)]).as_deref(),
            Some(
                format!(
                    "Run ports · {}=4020 · {}=4021",
                    otaq_core::naming::app_port_env_var("web"),
                    otaq_core::naming::app_port_env_var("api")
                )
                .as_str()
            )
        );
    }
    use otaq_core::session::{SessionKind, SessionState};

    fn shell(state: SessionState, reason: Option<&str>) -> SessionInfo {
        SessionInfo {
            id: "s".into(),
            kind: SessionKind::Shell,
            project: "BILL".into(),
            ticket: Some("BILL-4".parse().unwrap()),
            state,
            started_at: 0,
            ended_at: None,
            end_reason: reason.map(str::to_owned),
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            model_ran: None,
            size: None,
        }
    }

    #[test]
    fn strip_and_stopped_state() {
        assert_eq!(port_label(4004), "Run port · $PORT=4004");
        let hhmm = |_| "08:24".to_owned();
        assert_eq!(
            stopped_note(
                &shell(SessionState::Ended, Some(END_STOPPED_BY_RESTART)),
                hhmm
            )
            .unwrap()
            .0,
            "Stopped by restart · was open at 08:24"
        );
        assert_eq!(
            stopped_note(&shell(SessionState::Running, None), hhmm),
            None
        );
        assert_eq!(
            stopped_note(&shell(SessionState::Ended, Some("exit 0")), hhmm),
            None
        );
    }
}

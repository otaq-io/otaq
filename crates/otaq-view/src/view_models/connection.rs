//! What the window shows about its link to the background service (ARC-03, SVC-04).

use otaq_core::client::{Compatibility, ConnectionState, ServiceInfo};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;

/// The app's view of the link, including the phases before a client exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkState {
    /// Connecting, starting the service first if it isn't running.
    Connecting,
    Connected(ServiceInfo),
    /// The connection dropped; the client is retrying. Agents keep running.
    Reconnecting {
        attempt: u32,
    },
    /// The service process has ended (reboot, crash or `otaq daemon stop`). The client keeps retrying, but
    /// nothing starts the service until the user clicks (d0018).
    Stopped,
    /// The service couldn't be reached or started.
    Failed(String),
}

impl From<ConnectionState> for LinkState {
    fn from(state: ConnectionState) -> Self {
        match state {
            ConnectionState::Connected(info) => Self::Connected(info),
            ConnectionState::Reconnecting { attempt } => Self::Reconnecting { attempt },
        }
    }
}

/// What a terminal says over its scrollback while the link is down (#58): agents keep running only while the
/// client is reconnecting to a live service; once the service has stopped they stopped with it.
/// `TerminalLinkNote` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TerminalLinkNote")]
pub struct TerminalNote {
    pub text: &'static str,
    pub icon: IconName,
}

pub fn terminal_note(state: &LinkState) -> Option<TerminalNote> {
    match state {
        LinkState::Reconnecting { .. } => Some(TerminalNote {
            text: "Reconnecting\u{2026} agents keep running",
            icon: IconName::RefreshCw,
        }),
        LinkState::Stopped | LinkState::Failed(_) => Some(TerminalNote {
            text: "Service stopped",
            icon: IconName::CircleStop,
        }),
        LinkState::Connecting | LinkState::Connected(_) => None,
    }
}

/// Whether what the window last heard from the service may be stale (#58): the header's running count dims.
pub fn service_down(state: &LinkState) -> bool {
    matches!(
        state,
        LinkState::Reconnecting { .. } | LinkState::Stopped | LinkState::Failed(_)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Calm,
    Attention,
    Error,
}

/// The connection line the skeleton window shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionViewModel {
    pub label: String,
    pub detail: Option<String>,
    pub tone: Tone,
    /// Whether the window accepts input that needs the service (disabled while reconnecting, SVC-04).
    pub input_enabled: bool,
}

impl ConnectionViewModel {
    pub fn new(state: &LinkState) -> Self {
        match state {
            LinkState::Connecting => Self {
                label: "Connecting…".into(),
                detail: None,
                tone: Tone::Calm,
                input_enabled: false,
            },
            LinkState::Connected(info) => Self {
                label: "Connected".into(),
                detail: Some(format!(
                    "Background service · PID {} · build {}{}",
                    info.status.pid,
                    info.status.build_id,
                    match info.compatibility {
                        Compatibility::Same => "",
                        Compatibility::ServiceOlder => " · update ready",
                        Compatibility::Incompatible => " · restart required",
                    }
                )),
                tone: match info.compatibility {
                    Compatibility::Incompatible => Tone::Attention,
                    _ => Tone::Calm,
                },
                input_enabled: info.compatibility != Compatibility::Incompatible,
            },
            LinkState::Reconnecting { .. } => Self {
                label: "Reconnecting…".into(),
                detail: Some("agents keep running".into()),
                tone: Tone::Attention,
                input_enabled: false,
            },
            LinkState::Stopped => Self {
                label: "Background service stopped".into(),
                detail: Some("Nothing restarts on its own".into()),
                tone: Tone::Error,
                input_enabled: false,
            },
            LinkState::Failed(reason) => Self {
                label: "Not connected".into(),
                detail: Some(reason.clone()),
                tone: Tone::Error,
                input_enabled: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::protocol::{PROTOCOL_VERSION, ServiceStatus};

    /// #58: a stopped service doesn't say its agents keep running, and the header's count dims while the link is
    /// down.
    #[test]
    fn a_stopped_service_says_so_in_terminals() {
        use super::{LinkState, service_down, terminal_note};
        let text = |state| terminal_note(&state).map(|n| n.text);
        assert_eq!(
            text(LinkState::Reconnecting { attempt: 2 }),
            Some("Reconnecting\u{2026} agents keep running")
        );
        assert_eq!(text(LinkState::Stopped), Some("Service stopped"));
        assert_eq!(
            text(LinkState::Failed("no socket".into())),
            Some("Service stopped")
        );
        assert_eq!(text(LinkState::Connecting), None);
        assert!(service_down(&LinkState::Stopped));
        assert!(service_down(&LinkState::Reconnecting { attempt: 1 }));
        assert!(!service_down(&LinkState::Connecting));
    }

    use super::*;

    fn info(compatibility: Compatibility) -> ServiceInfo {
        ServiceInfo {
            status: ServiceStatus {
                pid: 4120,
                build_id: "abc.1".into(),
                protocol_version: PROTOCOL_VERSION,
                started_at: 0,
                uptime_secs: 0,
                agents_working: 0,
                agents_waiting: 0,
                ..Default::default()
            },
            compatibility,
        }
    }

    #[test]
    fn connected_shows_the_service() {
        let vm = ConnectionViewModel::new(&LinkState::Connected(info(Compatibility::Same)));
        assert_eq!(vm.label, "Connected");
        assert_eq!(
            vm.detail.as_deref(),
            Some("Background service · PID 4120 · build abc.1")
        );
        assert!(vm.input_enabled);
        assert_eq!(vm.tone, Tone::Calm);
    }

    #[test]
    fn reconnecting_disables_input() {
        let vm = ConnectionViewModel::new(&LinkState::from(ConnectionState::Reconnecting {
            attempt: 2,
        }));
        assert_eq!(vm.label, "Reconnecting…");
        assert_eq!(vm.detail.as_deref(), Some("agents keep running"));
        assert!(!vm.input_enabled);
    }

    #[test]
    fn incompatible_service_disables_input() {
        let vm = ConnectionViewModel::new(&LinkState::Connected(info(Compatibility::Incompatible)));
        assert!(!vm.input_enabled);
        assert_eq!(vm.tone, Tone::Attention);
        let vm = ConnectionViewModel::new(&LinkState::Connected(info(Compatibility::ServiceOlder)));
        assert!(
            vm.input_enabled,
            "an older service keeps working until the user restarts"
        );
    }

    #[test]
    fn failure_and_connecting() {
        let vm = ConnectionViewModel::new(&LinkState::Failed("no such file".into()));
        assert_eq!(vm.tone, Tone::Error);
        assert_eq!(vm.detail.as_deref(), Some("no such file"));
        assert!(!ConnectionViewModel::new(&LinkState::Connecting).input_enabled);
        let stopped = ConnectionViewModel::new(&LinkState::Stopped);
        assert_eq!(stopped.tone, Tone::Error);
        assert!(!stopped.input_enabled);
    }
}

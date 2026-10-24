//! The terminal's state as the sheet shows it; the emulator itself stays in the app.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Where the terminal stands. `TerminalPhase` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "TerminalPhase")]
pub enum Phase {
    /// The ticket has no session (yet).
    NoSession,
    /// Waiting for the stored output.
    Attaching,
    /// Following the live session; input goes to it.
    Live,
    /// The connection to the service is lost: the scrollback stays, input is paused (SVC-04).
    Reconnecting,
    /// The session ended: read-only scrollback with an end marker.
    Ended,
}

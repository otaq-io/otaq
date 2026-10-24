//! Attention (SDD §12; d0016, d0042): what needs the user and when to tell them. The needs-you family itself is
//! [`crate::signals`]; the needs-you items are [`crate::board::needs_you`].

pub mod bundle;
pub mod focus;
pub mod limit_stops;
pub mod needs_you;
#[cfg(target_os = "macos")]
pub mod notification_center;
pub mod notify;
pub mod notify_check;
pub mod os_permission;

pub use focus::{ClientId, Focus, FocusChange, Looking, Released, Side};
pub use limit_stops::{LimitStops, Suppression};
pub use needs_you::NeedsYouQueue;

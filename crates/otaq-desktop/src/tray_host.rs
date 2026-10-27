//! Whether a tray shows Otaq's icon on Linux (d0139 gap review C2, G8; TRY-08): a StatusNotifier host on the session
//! D-Bus, that is `org.kde.StatusNotifierWatcher` has an owner (KDE, most desktops, GNOME with the AppIndicator
//! extension; stock GNOME has none). Asked with `gdbus` (GLib's, there with every GTK desktop), else `dbus-send`; when
//! neither answers, there is no tray as far as Otaq knows, so closing asks rather than hiding the window into nothing.
//! macOS and Windows always have one; nothing is asked there.

use std::process::{Command, Stdio};

/// The StatusNotifier host's well-known name.
pub const WATCHER: &str = "org.kde.StatusNotifierWatcher";

/// Whether a StatusNotifier host is on the session bus now. Runs a short command: call off the UI thread.
pub fn watcher_present() -> bool {
    let gdbus = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.NameHasOwner",
            WATCHER,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    if let Some(present) = gdbus
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| gdbus_answer(&String::from_utf8_lossy(&o.stdout)))
    {
        return present;
    }
    let dbus_send = Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply",
            "--dest=org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus.NameHasOwner",
            &format!("string:{WATCHER}"),
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    dbus_send
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| dbus_send_answer(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or(false)
}

/// `gdbus call`'s answer to NameHasOwner: `(true,)` or `(false,)`.
fn gdbus_answer(out: &str) -> Option<bool> {
    match out.trim() {
        "(true,)" => Some(true),
        "(false,)" => Some(false),
        _ => None,
    }
}

/// `dbus-send --print-reply`'s answer: a `method return …` line, then `   boolean true`.
fn dbus_send_answer(out: &str) -> Option<bool> {
    out.lines()
        .find_map(|line| line.trim().strip_prefix("boolean "))
        .and_then(|value| match value.trim() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
}

/// Whether a tray keeps Otaq now. macOS and Windows always have one (d0139); on Linux the icon was made (`icon_up`:
/// the AppIndicator library loaded) and a host shows it.
pub fn tray_shows(os: &str, icon_up: bool, watcher_present: impl FnOnce() -> bool) -> bool {
    os != "linux" || (icon_up && watcher_present())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answers_are_read() {
        assert_eq!(gdbus_answer("(true,)\n"), Some(true));
        assert_eq!(gdbus_answer("(false,)\n"), Some(false));
        assert_eq!(gdbus_answer("Error: no bus"), None);
        let reply = "method return time=1730000000.1 sender=org.freedesktop.DBus -> destination=:1.42 serial=3 \
                     reply_serial=2\n   boolean true\n";
        assert_eq!(dbus_send_answer(reply), Some(true));
        assert_eq!(
            dbus_send_answer(&reply.replace("boolean true", "boolean false")),
            Some(false)
        );
        assert_eq!(dbus_send_answer(""), None);
    }

    /// T-0570 (TRY-08): macOS and Windows never ask the bus; Linux needs the icon and a host.
    #[test]
    fn where_a_tray_shows_the_icon() {
        let never = || panic!("not asked");
        assert!(tray_shows("macos", true, never));
        assert!(tray_shows("windows", true, never));
        assert!(tray_shows("macos", false, never), "always a tray there");
        assert!(
            !tray_shows("linux", false, never),
            "no icon: the bus isn't asked"
        );
        assert!(tray_shows("linux", true, || true));
        assert!(!tray_shows("linux", true, || false));
    }
}

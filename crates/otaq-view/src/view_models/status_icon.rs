//! The status icon in the menu bar / tray (d0139, TRY-01, TRY-04): its look, its accessible name and its native
//! menu, from the app's link to the service. T-0566 has the plain and not connected looks with Open Otaq and a
//! stand-in Quit Otaq; the needs-you count, the lists and Pause / Resume all come with T-0567 / T-0568.
//!
//! **Not connected** follows the link with a grace period ([`NOT_CONNECTED_AFTER`]): a link down for less than that
//! (a service restart, a dropped connection the client retries at once) keeps the icon as it was, so it doesn't
//! flicker; a link still down after it shows the not-connected look until it is connected again.

use std::time::{Duration, Instant};

use otaq_core::naming::PRODUCT_NAME;

use crate::view_models::connection::LinkState;

/// How long the link may be down (reconnecting, stopped, failed) before the icon says not connected. The client
/// retries from 100 ms, doubling to 2 s, so a service that comes back reconnects well within it.
pub const NOT_CONNECTED_AFTER: Duration = Duration::from_secs(3);

/// The icon's look (page 31 item 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconLook {
    Plain,
    NotConnected,
}

/// What a menu item does when picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconAction {
    /// Shows and focuses the window.
    OpenOtaq,
    /// Quits the app only, agents keep working (stand-in until T-0569's Stop agents and quit; T-0568 keeps it as the
    /// not-connected menu's Quit Otaq, audit AS q2).
    Quit,
}

impl IconAction {
    pub const ALL: [IconAction; 2] = [IconAction::OpenOtaq, IconAction::Quit];

    /// The native menu item's id.
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenOtaq => "open",
            Self::Quit => "quit",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }
}

/// One row of the native menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuRow {
    /// A disabled line that says something ("Otaq lost its connection").
    Line(String),
    Action {
        action: IconAction,
        label: String,
    },
    Separator,
}

/// Everything the door shows for the icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusIconView {
    pub look: IconLook,
    /// The tooltip and accessible name (page 31 "Icon names").
    pub name: String,
    pub menu: Vec<MenuRow>,
}

/// "Open Otaq" (page 31).
pub fn open_label() -> String {
    format!("Open {PRODUCT_NAME}")
}

/// "Quit Otaq" (page 31's not-connected menu, audit AS q2).
pub fn quit_label() -> String {
    format!("Quit {PRODUCT_NAME}")
}

/// The not-connected menu's line, "Otaq lost its connection".
pub fn lost_connection() -> String {
    format!("{PRODUCT_NAME} lost its connection")
}

/// The icon's name per look (page 31 "Icon names"): "Otaq", "Otaq · not connected to its background service".
pub fn icon_name(look: IconLook) -> String {
    match look {
        IconLook::Plain => PRODUCT_NAME.to_owned(),
        IconLook::NotConnected => {
            format!("{PRODUCT_NAME} \u{b7} not connected to its background service")
        }
    }
}

impl StatusIconView {
    pub fn of(look: IconLook) -> Self {
        let open = MenuRow::Action {
            action: IconAction::OpenOtaq,
            label: open_label(),
        };
        let quit = MenuRow::Action {
            action: IconAction::Quit,
            label: quit_label(),
        };
        let menu = match look {
            IconLook::Plain => vec![open, MenuRow::Separator, quit],
            IconLook::NotConnected => vec![
                MenuRow::Line(lost_connection()),
                open,
                MenuRow::Separator,
                quit,
            ],
        };
        Self {
            look,
            name: icon_name(look),
            menu,
        }
    }
}

/// The link part of the icon's state (its [`IconLook::NotConnected`] with the grace period), fed the link's states as
/// they come.
#[derive(Debug, Default)]
pub struct IconLink {
    /// Since when the link has been down; `None` while connected or connecting.
    down_since: Option<Instant>,
}

impl IconLink {
    /// The link is now `state`.
    pub fn on_link(&mut self, state: &LinkState, now: Instant) {
        match state {
            LinkState::Reconnecting { .. } | LinkState::Stopped | LinkState::Failed(_) => {
                self.down_since.get_or_insert(now);
            }
            // Connecting is the app's own start: nothing is known yet, so the icon stays plain.
            LinkState::Connected(_) | LinkState::Connecting => self.down_since = None,
        }
    }

    pub fn connection_look(&self, now: Instant) -> IconLook {
        match self.down_since {
            Some(since) if now.duration_since(since) >= NOT_CONNECTED_AFTER => {
                IconLook::NotConnected
            }
            _ => IconLook::Plain,
        }
    }

    pub fn view(&self, now: Instant) -> StatusIconView {
        StatusIconView::of(self.connection_look(now))
    }

    /// When the look changes by itself (the grace period ends), if the link stays as it is.
    pub fn changes_at(&self, now: Instant) -> Option<Instant> {
        let at = self.down_since? + NOT_CONNECTED_AFTER;
        (at > now).then_some(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected() -> LinkState {
        LinkState::Connected(otaq_core::client::ServiceInfo {
            status: Default::default(),
            compatibility: otaq_core::client::Compatibility::Same,
        })
    }

    #[test]
    fn a_short_drop_keeps_the_icon_plain() {
        let start = Instant::now();
        let mut m = IconLink::default();
        m.on_link(&LinkState::Connecting, start);
        assert_eq!(m.connection_look(start), IconLook::Plain);
        m.on_link(&connected(), start);
        m.on_link(&LinkState::Reconnecting { attempt: 1 }, start);
        assert_eq!(m.changes_at(start), Some(start + NOT_CONNECTED_AFTER));
        // Later attempts don't restart the grace period.
        let soon = start + Duration::from_secs(1);
        m.on_link(&LinkState::Reconnecting { attempt: 4 }, soon);
        assert_eq!(m.connection_look(soon), IconLook::Plain);
        m.on_link(&connected(), soon);
        assert_eq!(
            m.connection_look(start + Duration::from_secs(10)),
            IconLook::Plain
        );
        assert_eq!(m.changes_at(soon), None);
    }

    #[test]
    fn a_link_down_past_the_grace_period_is_not_connected() {
        let start = Instant::now();
        for down in [
            LinkState::Reconnecting { attempt: 2 },
            LinkState::Stopped,
            LinkState::Failed("can't start the service".into()),
        ] {
            let mut m = IconLink::default();
            m.on_link(&connected(), start);
            m.on_link(&down, start);
            let past = start + NOT_CONNECTED_AFTER;
            assert_eq!(m.connection_look(past), IconLook::NotConnected, "{down:?}");
            assert_eq!(m.changes_at(past), None);
            m.on_link(&connected(), past);
            assert_eq!(
                m.connection_look(past),
                IconLook::Plain,
                "back when it returns"
            );
        }
    }

    #[test]
    fn the_menus_and_names() {
        let plain = StatusIconView::of(IconLook::Plain);
        assert_eq!(plain.name, PRODUCT_NAME);
        assert_eq!(
            plain.menu,
            [
                MenuRow::Action {
                    action: IconAction::OpenOtaq,
                    label: format!("Open {PRODUCT_NAME}")
                },
                MenuRow::Separator,
                MenuRow::Action {
                    action: IconAction::Quit,
                    label: format!("Quit {PRODUCT_NAME}")
                },
            ]
        );
        let lost = StatusIconView::of(IconLook::NotConnected);
        assert_eq!(
            lost.name,
            format!("{PRODUCT_NAME} · not connected to its background service")
        );
        assert_eq!(
            lost.menu[0],
            MenuRow::Line(format!("{PRODUCT_NAME} lost its connection"))
        );
        assert_eq!(lost.menu[1], plain.menu[0]);
        for action in IconAction::ALL {
            assert_eq!(IconAction::from_id(action.id()), Some(action));
        }
        assert_eq!(IconAction::from_id("nope"), None);
    }
}

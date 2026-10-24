//! The toast host's rules (*01 · Components → Overlays & feedback*): toasts stack bottom-right, at most three,
//! success toasts hide after 6 s (hover pauses), error toasts stay until dismissed. A toast about a ticket opens it
//! when clicked (*11 · Banners → Success toasts*).

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::sheet::Content;

/// How long a success toast stays.
pub const AUTO_HIDE: Duration = Duration::from_secs(6);
/// Toasts shown at once; older ones make room.
pub const MAX_TOASTS: usize = 3;

/// The quiet toast after the app reconnects to a service that kept running (REC-06).
pub const RECONNECTED: &str = "Reconnected";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ToastLevel {
    Success,
    Error,
}

/// The link after a toast's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToastLink {
    /// "Copy command": copies `text` (`gh auth login`).
    Copy { label: String, text: String },
    /// "Send back to resolve →", "Retry start →": opens the ticket, on `tab` if given.
    Open {
        label: String,
        ticket: String,
        tab: Option<Content>,
    },
    /// One or two buttons after the title: "Undo", or "Turn on for epic children" and "Undo" (EPC-15). A press
    /// is sent back with `notices.toast_action` and replaces the toast.
    Actions { actions: Vec<ToastButton> },
}

/// A toast's button and what pressing it does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ToastButton {
    pub label: String,
    pub action: ToastAction,
}

/// What a toast's button asks the app to do, on the user's click.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToastAction {
    /// The queue toast's Undo: `epic.undo_queue` with what the queue moved (T-0264).
    UndoEpicQueue { epic: String, keys: Vec<String> },
}

/// A toast to show, as a screen or the app asks for it. Its wording comes from the view models or the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NewToast {
    pub level: ToastLevel,
    pub title: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub link: Option<ToastLink>,
    /// The ticket a click on the toast opens (`SHOP-18`).
    #[serde(default)]
    pub ticket: Option<String>,
}

impl NewToast {
    pub fn new(level: ToastLevel, title: impl Into<String>, text: Option<String>) -> Self {
        Self {
            level,
            title: title.into(),
            text,
            link: None,
            ticket: None,
        }
    }
}

/// The Auto-merge toast's title, "SHOP-40.1 merged automatically into the Wishlist branch" (d0064): the merge icon.
const MERGED_AUTOMATICALLY: &str = " merged automatically into ";
/// The Local merge's warning toast, "Couldn't update main from origin · merged into your local main" (d0121): the
/// warning triangle.
const REMOTE_UNREACHABLE: &str = "Couldn\u{2019}t update ";
/// The cancel toast's title, "Validation cancelled: SHOP-29’s code changed" (d0063): the ban icon, secondary.
const VALIDATION_CANCELLED: &str = "Validation cancelled: ";

/// The icon's colour when it isn't the level's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ToastIconTone {
    Review,
    Investigate,
    Working,
    Secondary,
}

/// One toast as the page draws it, oldest first (the newest at the bottom).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "Toast")]
pub struct ToastView {
    pub id: u64,
    pub level: ToastLevel,
    pub title: String,
    pub text: Option<String>,
    /// Another icon than the level's: "Reconnected" the refresh arrows, a ticket created by an agent the sparkles
    /// in the agent colour, "started · all dependencies merged" the play icon in the working colour.
    pub icon: Option<IconName>,
    pub icon_tone: Option<ToastIconTone>,
    pub link: Option<ToastLink>,
    pub ticket: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastItem {
    pub id: u64,
    pub level: ToastLevel,
    pub title: String,
    pub text: Option<String>,
    /// The action link's label (e.g. "Copy command") and the text it copies.
    pub copy_action: Option<(String, String)>,
    pub link: Option<ToastLink>,
    pub ticket: Option<String>,
    pub shown_at: Instant,
    /// Since when the pointer rests on it: a held toast doesn't time out.
    pub held_since: Option<Instant>,
}

impl ToastItem {
    pub fn view(&self) -> ToastView {
        use crate::view_models::{
            agent_activity::CREATED_BY_AGENT, cleanup::CLEANED_UP, queue::DEPENDENCIES_MERGED,
        };
        let (icon, icon_tone) = if self.title == RECONNECTED {
            (Some(IconName::RefreshCw), None)
        } else if self.title.ends_with(CREATED_BY_AGENT) {
            (Some(IconName::Sparkles), Some(ToastIconTone::Investigate))
        } else if self.title.ends_with(DEPENDENCIES_MERGED) {
            (Some(IconName::Play), Some(ToastIconTone::Working))
        } else if self.title.contains(MERGED_AUTOMATICALLY) {
            (Some(IconName::GitMerge), None)
        } else if self.title.starts_with(REMOTE_UNREACHABLE) {
            // d0121: merged into the local base branch, the remote couldn't be reached.
            (Some(IconName::TriangleAlert), None)
        } else if self.title.starts_with(VALIDATION_CANCELLED) {
            (Some(IconName::Ban), Some(ToastIconTone::Secondary))
        } else if self.title == crate::view_models::flow_settings::FLOW_CHANGE_CANCELLED {
            // *11 → Applied, or cancelled*: the undo arrow, secondary.
            (Some(IconName::Undo2), Some(ToastIconTone::Secondary))
        } else if self.title.starts_with(CLEANED_UP)
            || crate::view_models::warm_build::is_cleared_toast(&self.title)
        {
            // *16 → Success*, and *Cleanup · right after Clear (0082)*: the broom, in the success colour.
            (Some(IconName::BrushCleaning), None)
        } else {
            (None, None)
        };
        let link = self.link.clone().or_else(|| {
            self.copy_action
                .clone()
                .map(|(label, text)| ToastLink::Copy { label, text })
        });
        ToastView {
            id: self.id,
            level: self.level,
            title: self.title.clone(),
            text: self.text.clone(),
            icon,
            icon_tone,
            link,
            ticket: self.ticket.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ToastModel {
    items: Vec<ToastItem>,
    next_id: u64,
}

impl ToastModel {
    pub fn push(
        &mut self,
        level: ToastLevel,
        title: impl Into<String>,
        text: Option<String>,
        now: Instant,
    ) -> u64 {
        self.push_item(level, title.into(), text, None, now)
    }

    /// Any toast, with its link and the ticket it opens.
    pub fn push_toast(&mut self, toast: NewToast, now: Instant) -> u64 {
        let id = self.push_item(toast.level, toast.title, toast.text, None, now);
        if let Some(item) = self.items.iter_mut().find(|t| t.id == id) {
            item.link = toast.link;
            item.ticket = toast.ticket;
        }
        id
    }

    /// The pointer came onto (`held`) or left the toast: its time-out pauses meanwhile.
    pub fn hold(&mut self, id: u64, held: bool, now: Instant) {
        let Some(item) = self.items.iter_mut().find(|t| t.id == id) else {
            return;
        };
        match (held, item.held_since) {
            (true, None) => item.held_since = Some(now),
            (false, Some(since)) => {
                item.shown_at += now.saturating_duration_since(since);
                item.held_since = None;
            }
            _ => {}
        }
    }

    /// An error toast with an action link that copies `command` to the clipboard.
    pub fn push_with_copy(
        &mut self,
        title: impl Into<String>,
        text: Option<String>,
        label: impl Into<String>,
        command: impl Into<String>,
        now: Instant,
    ) -> u64 {
        self.push_item(
            ToastLevel::Error,
            title.into(),
            text,
            Some((label.into(), command.into())),
            now,
        )
    }

    fn push_item(
        &mut self,
        level: ToastLevel,
        title: String,
        text: Option<String>,
        copy_action: Option<(String, String)>,
        now: Instant,
    ) -> u64 {
        self.next_id += 1;
        self.items.push(ToastItem {
            id: self.next_id,
            level,
            title,
            text,
            copy_action,
            link: None,
            ticket: None,
            shown_at: now,
            held_since: None,
        });
        if self.items.len() > MAX_TOASTS {
            let overflow = self.items.len() - MAX_TOASTS;
            self.items.drain(..overflow);
        }
        self.next_id
    }

    pub fn dismiss(&mut self, id: u64) {
        self.items.retain(|t| t.id != id);
    }

    /// Removes success toasts older than [`AUTO_HIDE`]. Returns whether anything changed.
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.items.len();
        self.items.retain(|t| {
            t.level == ToastLevel::Error
                || t.held_since.is_some()
                || now.saturating_duration_since(t.shown_at) < AUTO_HIDE
        });
        before != self.items.len()
    }

    /// Oldest first; drawn bottom-up so the newest sits at the bottom.
    pub fn items(&self) -> &[ToastItem] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_toasts_time_out_errors_stay() {
        let t0 = Instant::now();
        let mut m = ToastModel::default();
        m.push(ToastLevel::Success, "Reconnected", None, t0);
        let err = m.push_with_copy(
            "gh is not signed in",
            None,
            "Copy command",
            "gh auth login",
            t0,
        );
        assert!(!m.expire(t0 + Duration::from_secs(5)));
        assert_eq!(m.items().len(), 2);
        assert!(m.expire(t0 + AUTO_HIDE));
        assert_eq!(m.items().len(), 1);
        assert_eq!(m.items()[0].id, err);
        assert!(!m.expire(t0 + Duration::from_secs(600)), "errors stay");
        m.dismiss(err);
        assert!(m.items().is_empty());
    }

    #[test]
    fn at_most_three_newest_kept() {
        let t0 = Instant::now();
        let mut m = ToastModel::default();
        for i in 0..5 {
            m.push(ToastLevel::Success, format!("t{i}"), None, t0);
        }
        let titles: Vec<&str> = m.items().iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["t2", "t3", "t4"]);
    }

    #[test]
    fn error_toasts_carry_their_command() {
        let mut m = ToastModel::default();
        m.push_with_copy(
            "gh is not signed in",
            Some("Run gh auth login, then retry Open PR.".into()),
            "Copy command",
            "gh auth login",
            Instant::now(),
        );
        let item = &m.items()[0];
        assert_eq!(item.level, ToastLevel::Error);
        assert_eq!(
            item.copy_action,
            Some(("Copy command".into(), "gh auth login".into()))
        );
    }

    #[test]
    fn hover_pauses_the_time_out() {
        let t0 = Instant::now();
        let mut m = ToastModel::default();
        let id = m.push(ToastLevel::Success, "Local merge done · SHOP-18", None, t0);
        m.hold(id, true, t0 + Duration::from_secs(2));
        assert!(!m.expire(t0 + Duration::from_secs(60)), "held");
        m.hold(id, false, t0 + Duration::from_secs(60));
        assert!(
            !m.expire(t0 + Duration::from_secs(63)),
            "2 s shown before the hover, 3 s after"
        );
        assert!(m.expire(t0 + Duration::from_secs(64)));
        m.hold(id, true, t0);
    }

    #[test]
    fn the_page_gets_links_tickets_and_icons() {
        let now = Instant::now();
        let mut m = ToastModel::default();
        m.push_toast(
            NewToast {
                link: Some(ToastLink::Open {
                    label: "Send back to resolve →".into(),
                    ticket: "SHOP-18".into(),
                    tab: None,
                }),
                ticket: Some("SHOP-18".into()),
                ..NewToast::new(
                    ToastLevel::Error,
                    "Merge conflict · SHOP-18",
                    Some("Local merge was aborted; main is unchanged.".into()),
                )
            },
            now,
        );
        m.push_with_copy(
            "gh is not signed in",
            None,
            "Copy command",
            "gh auth login",
            now,
        );
        m.push(ToastLevel::Success, RECONNECTED, None, now);
        let views: Vec<ToastView> = m.items().iter().map(ToastItem::view).collect();
        assert_eq!(views[0].ticket.as_deref(), Some("SHOP-18"));
        assert!(
            matches!(&views[0].link, Some(ToastLink::Open { ticket, .. }) if ticket == "SHOP-18")
        );
        assert_eq!(
            views[1].link,
            Some(ToastLink::Copy {
                label: "Copy command".into(),
                text: "gh auth login".into()
            })
        );
        assert_eq!(
            (views[2].icon, views[2].icon_tone),
            (Some(IconName::RefreshCw), None)
        );
        let agent = ToastItem {
            title: "Ticket created by agent".into(),
            ..m.items()[0].clone()
        };
        assert_eq!(
            (agent.view().icon, agent.view().icon_tone),
            (Some(IconName::Sparkles), Some(ToastIconTone::Investigate))
        );
        let cleaned = ToastItem {
            title: "Cleaned up 2 items".into(),
            ..m.items()[0].clone()
        };
        assert_eq!(cleaned.view().icon, Some(IconName::BrushCleaning));
        let remote = ToastItem {
            title: "Couldn\u{2019}t update main from origin \u{b7} merged into your local main"
                .into(),
            ..m.items()[0].clone()
        };
        assert_eq!(remote.view().icon, Some(IconName::TriangleAlert));
        assert_eq!(views[0].icon, None);
    }
}

//! What macOS allows Otaq's notifications, as the app shows it (ATT-13, ATT-14; d0090): the permission read through
//! the notifier, Otaq's own "Allow notifications?" line (the ask) and the banner when macOS blocks a notification
//! that mattered. `notifications.state` answers [`NotificationsState`]; every change publishes
//! `notifications_changed` with the new state.
//!
//! - **The ask** is set pending when the first agent session of any kind starts (a ticket's agent, a Project agent) while macOS hasn't been asked, once: after `notifications.ask_answered` (Allow, Not now, or Settings'
//!   Allow notifications) it is never set again. The request to macOS itself happens in the app's process; the
//!   service only reads the answer. Outside the bundle the permission is never "not asked", so nothing is pending.
//! - **The blocked banner** shows while Otaq's switch is on, macOS has notifications turned off and Needs you left out
//!   a notification for it, unless the user closed it for this permission. The permission becoming allowed forgets
//!   both the closing and the blocked notification, so a later turn-off shows it once more, at the next one.
//! - The permission is read again at start, when an app window gains focus (the user may have changed System
//!   Settings), before posting ([`super::NeedsYouQueue`]) and after the ask was answered; a read never waits.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, broadcast, watch};

use super::NeedsYouQueue;
use super::notify::{Notifier, OnClick, PermissionState};
use crate::board::{Board, BoardError};
use crate::protocol::Event;
use crate::service::EventBus;
use crate::session::SessionKind;
use crate::store::Store;
use crate::store::settings::{
    FIRST_HIDE_NOTE_SHOWN, NOTIFICATIONS_ASK_ANSWERED, NOTIFICATIONS_ASK_PENDING,
    NOTIFICATIONS_BLOCKED_DISMISSED,
};

/// The answer to `notifications.state` and the payload of `notifications_changed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationsState {
    /// What macOS allows, as last read.
    pub permission: PermissionState,
    /// Otaq's "Allow notifications?" banner is up.
    pub ask_pending: bool,
    /// The "macOS is blocking Otaq's notifications" banner is up.
    pub blocked_banner: bool,
}

/// The answer to `app.first_hide` (TRY-02, d0139): where the first-hide note went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FirstHide {
    /// It went out before, for this data folder: nothing now.
    Shown,
    /// Posted as an OS notification.
    Posted,
    /// Otaq's notifications are off or the OS blocks them: the window shows it the next time it opens.
    InWindow { title: String, body: String },
}

/// Whether the first-hide note can go out as an OS notification: Otaq's switch on and the OS not known to drop it
/// (turned off, or never asked: macOS drops a notification it was never asked about).
pub fn first_hide_posts(switch_on: bool, permission: PermissionState) -> bool {
    switch_on
        && !matches!(
            permission,
            PermissionState::TurnedOff | PermissionState::NotAsked
        )
}

/// The permission, the ask and the blocked banner. Cheap to clone.
#[derive(Clone)]
pub struct OsPermission {
    inner: Arc<Inner>,
}

struct Inner {
    store: Store,
    board: Board,
    needs_you: NeedsYouQueue,
    notifier: Arc<dyn Notifier>,
    events: EventBus,
    /// Woken by a permission read that found a change and by Needs you's first blocked notification.
    wake: Arc<Notify>,
    /// The state last published.
    last: Mutex<Option<NotificationsState>>,
    /// The permission last seen here: a change to allowed forgets the banner's closing.
    seen: Mutex<PermissionState>,
}

impl OsPermission {
    pub fn new(
        store: Store,
        board: Board,
        needs_you: NeedsYouQueue,
        notifier: Arc<dyn Notifier>,
        events: EventBus,
    ) -> Self {
        let wake = Arc::new(Notify::new());
        needs_you.on_notification_blocked(wake.clone());
        {
            let wake = wake.clone();
            notifier.on_permission(Arc::new(move || wake.notify_one()));
        }
        Self {
            inner: Arc::new(Inner {
                store,
                board,
                needs_you,
                notifier,
                events,
                wake,
                last: Mutex::new(None),
                seen: Mutex::new(PermissionState::Unknown),
            }),
        }
    }

    /// Reads macOS's permission again, off this thread; a change publishes the new state.
    pub fn refresh(&self) {
        self.inner.notifier.refresh_permission();
    }

    /// The state now (`notifications.state`).
    pub async fn state(&self) -> Result<NotificationsState, BoardError> {
        let permission = self.follow_permission().await?;
        let store = &self.inner.store;
        let ask_pending = permission == PermissionState::NotAsked
            && flag(store, NOTIFICATIONS_ASK_PENDING).await?
            && !flag(store, NOTIFICATIONS_ASK_ANSWERED).await?;
        let dismissed_for = store
            .setting(NOTIFICATIONS_BLOCKED_DISMISSED)
            .await?
            .and_then(|v| serde_json::from_str::<PermissionState>(&v).ok());
        let blocked_banner = permission == PermissionState::TurnedOff
            && self.inner.needs_you.notification_blocked()
            && dismissed_for != Some(permission)
            && self.inner.board.global_settings().await?.notifications;
        Ok(NotificationsState {
            permission,
            ask_pending,
            blocked_banner,
        })
    }

    /// The permission as last read; when it became allowed, the banner's closing and the blocked notification are
    /// forgotten.
    async fn follow_permission(&self) -> Result<PermissionState, BoardError> {
        let now = self.inner.notifier.permission();
        let before = std::mem::replace(&mut *self.inner.seen.lock().expect("permission seen"), now);
        if now == PermissionState::Allowed && before != PermissionState::Allowed {
            // Before any wait, so a state read meanwhile already sees it.
            self.inner.needs_you.clear_notification_blocked();
            self.inner
                .store
                .remove_setting(NOTIFICATIONS_BLOCKED_DISMISSED)
                .await?;
        }
        Ok(now)
    }

    /// Publishes `notifications_changed` when the state differs from the one last published.
    pub async fn sync(&self) -> Result<NotificationsState, BoardError> {
        let state = self.state().await?;
        let changed = {
            let mut last = self.inner.last.lock().expect("notifications state");
            let changed = last.as_ref() != Some(&state);
            *last = Some(state);
            changed
        };
        if changed {
            self.inner
                .events
                .publish(Event::NotificationsChanged { state });
        }
        Ok(state)
    }

    /// An agent session started: the ask becomes pending if macOS hasn't been asked and the ask was never answered.
    pub async fn agent_started(&self) -> Result<(), BoardError> {
        let store = &self.inner.store;
        if self.inner.notifier.permission() != PermissionState::NotAsked
            || flag(store, NOTIFICATIONS_ASK_ANSWERED).await?
            || flag(store, NOTIFICATIONS_ASK_PENDING).await?
        {
            return Ok(());
        }
        store.set_setting(NOTIFICATIONS_ASK_PENDING, "true").await?;
        self.sync().await?;
        Ok(())
    }

    /// `notifications.ask_answered`: the ask is answered for good (Allow, Not now, Settings' Allow notifications).
    /// The app sends it again once macOS answered its question: the permission is read again either way.
    pub async fn answered(&self) -> Result<(), BoardError> {
        let store = &self.inner.store;
        store
            .set_setting(NOTIFICATIONS_ASK_ANSWERED, "true")
            .await?;
        store.remove_setting(NOTIFICATIONS_ASK_PENDING).await?;
        self.refresh();
        self.sync().await?;
        Ok(())
    }

    /// `app.first_hide`: the window hid; the first time for this data folder, the note says Otaq keeps running in the
    /// menu bar / tray (`note`), as an OS notification ([`first_hide_posts`], `on_click` on a click) or, when it can't
    /// go out, kept for the window.
    pub async fn first_hide(
        &self,
        note: super::notify::OsNotification,
        on_click: OnClick,
    ) -> Result<FirstHide, BoardError> {
        let store = &self.inner.store;
        if flag(store, FIRST_HIDE_NOTE_SHOWN).await? {
            return Ok(FirstHide::Shown);
        }
        store.set_setting(FIRST_HIDE_NOTE_SHOWN, "true").await?;
        let switch_on = self.inner.board.global_settings().await?.notifications;
        if first_hide_posts(switch_on, self.inner.notifier.permission()) {
            self.inner.notifier.post(&note, on_click);
            Ok(FirstHide::Posted)
        } else {
            Ok(FirstHide::InWindow {
                title: note.title,
                body: note.body,
            })
        }
    }

    /// `notifications.dismiss_blocked`: the banner stays closed for the permission it was closed for.
    pub async fn dismiss_blocked(&self) -> Result<(), BoardError> {
        let permission = self.inner.notifier.permission();
        let value = serde_json::to_string(&permission).expect("a permission state serializes");
        self.inner
            .store
            .set_setting(NOTIFICATIONS_BLOCKED_DISMISSED, &value)
            .await?;
        self.sync().await?;
        Ok(())
    }

    /// Follows the permission reads, the blocked notifications, agent starts and settings changes until the service
    /// stops.
    pub async fn run(self, mut stop: watch::Receiver<bool>) {
        let mut listener = self.inner.events.listen();
        // The state at start is only kept: clients ask for it when they connect.
        match self.state().await {
            Ok(state) => *self.inner.last.lock().expect("notifications state") = Some(state),
            Err(e) => tracing::warn!(error = %e, "can't read the notifications state"),
        }
        loop {
            let next = tokio::select! {
                biased;
                _ = stop.wait_for(|s| *s) => return,
                () = self.inner.wake.notified() => Next::Sync,
                event = listener.receiver.recv() => match event {
                    Ok(Event::SessionState { session })
                        if session.is_running()
                            && matches!(session.kind, SessionKind::Agent | SessionKind::ProjectAgent) =>
                    {
                        Next::AgentStarted
                    }
                    Ok(Event::SettingsChanged) | Err(broadcast::error::RecvError::Lagged(_)) => Next::Sync,
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                },
            };
            let outcome = match next {
                Next::Sync => self.sync().await.map(|_| ()),
                Next::AgentStarted => self.agent_started().await,
            };
            if let Err(e) = outcome {
                tracing::warn!(error = %e, "can't follow the notifications state");
            }
        }
    }
}

/// What [`OsPermission::run`] does next.
enum Next {
    Sync,
    AgentStarted,
}

async fn flag(store: &Store, key: &str) -> Result<bool, BoardError> {
    Ok(store.setting(key).await?.is_some_and(|v| v == "true"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-0566 (d0139, audit AS q4): the first-hide note is an OS notification unless Otaq's switch is off or the OS
    /// would drop it.
    #[test]
    fn where_the_first_hide_note_goes() {
        use PermissionState::*;
        for (switch_on, permission, posts) in [
            (true, Allowed, true),
            (true, OutsideBundle, true),
            (true, Unknown, true),
            (true, TurnedOff, false),
            (true, NotAsked, false),
            (false, Allowed, false),
            (false, Unknown, false),
        ] {
            assert_eq!(
                first_hide_posts(switch_on, permission),
                posts,
                "{switch_on} {permission:?}"
            );
        }
    }
}

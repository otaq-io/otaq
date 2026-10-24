//! The native notifier (d0090): the service posts through macOS's notification API under `Otaq.app`'s bundle ID, so
//! notifications show as "Otaq" with its icon. Made only with an [`InBundle`] (outside the bundle the API kills the
//! process). The bundle check (T-0370, 2026-10-18) found that the service, the bundle's second program, may post under
//! the bundle's ID, and that a click arrives in the posting process's delegate: so the service is the delegate, and a
//! click opens the target the notification carries in its `userInfo` ([`Notifier::on_clicks`]).
//!
//! macOS's replies can take seconds: posts go through one thread of their own, and the permission state is read off
//! the caller's thread and kept ([`Notifier::permission`] never waits).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use super::{
    Notifier, OnClick, OpenTarget, OsNotification, PermissionChanged, PermissionState, Target,
};
use crate::attention::notification_center::{
    self as center, Clicked, Content, InBundle, Permission,
};
use crate::naming;

/// Posts through `UNUserNotificationCenter` from inside the bundle.
pub struct NativeNotifier {
    in_bundle: InBundle,
    /// The permission as last read; [`PermissionState::Unknown`] until the first read answers.
    permission: Arc<Mutex<PermissionState>>,
    /// A read is under way: another one waits for it.
    reading: Arc<AtomicBool>,
    posts: Mutex<Option<Sender<OsNotification>>>,
    /// What a click opens, once the service set it.
    open: Arc<Mutex<Option<OpenTarget>>>,
    /// Told when a read finds the permission changed, once the service set it.
    changed: Arc<Mutex<Option<PermissionChanged>>>,
}

impl NativeNotifier {
    /// The notifier, the service as the notification center's delegate, and a first read of the permission.
    pub fn new(in_bundle: InBundle) -> Self {
        let open: Arc<Mutex<Option<OpenTarget>>> = Arc::default();
        let on_click = open.clone();
        center::on_clicks(&in_bundle, move |clicked: Clicked| {
            let target = clicked
                .target
                .as_deref()
                .and_then(Target::parse_open_value)
                .unwrap_or(Target::NeedsYou);
            tracing::info!(id = clicked.id, ?target, "notification clicked");
            let open = on_click.lock().expect("click handler").clone();
            match open {
                Some(open) => open(target),
                None => tracing::warn!("a notification click came before the service was ready"),
            }
        });
        let notifier = Self {
            posts: Mutex::new(poster(in_bundle.clone())),
            in_bundle,
            permission: Arc::new(Mutex::new(PermissionState::Unknown)),
            reading: Arc::default(),
            open,
            changed: Arc::default(),
        };
        notifier.refresh_permission();
        notifier
    }
}

/// The thread every post goes through, in order; `None` when it can't start.
fn poster(in_bundle: InBundle) -> Option<Sender<OsNotification>> {
    let (sender, posts) = channel::<OsNotification>();
    let spawned = std::thread::Builder::new()
        .name("notifications".into())
        .spawn(move || {
            let mut hinted = false;
            for (n, sequence) in posts.into_iter().zip(1u64..) {
                let id = format!(
                    "{}-{}-{sequence}",
                    naming::COMMAND_NAME,
                    std::process::id()
                );
                let target = n.target.to_open_value();
                let content = Content {
                    id: &id,
                    title: &n.title,
                    subtitle: &n.subtitle,
                    body: &n.body,
                    thread: n.thread.as_deref(),
                    sound: n.sound,
                    target: Some(&target),
                };
                if let Err(e) = center::post_content(&in_bundle, &content) {
                    tracing::warn!(error = %e, title = n.title, "can't post a notification");
                    let root = &in_bundle.bundle().root;
                    if !hinted && crate::attention::notify_check::in_temp_folder_now(root) {
                        hinted = true;
                        tracing::warn!(
                            bundle = %root.display(),
                            "this bundle is in a temporary folder; macOS refuses notifications for apps there"
                        );
                    }
                }
            }
        });
    match spawned {
        Ok(_) => Some(sender),
        Err(e) => {
            tracing::warn!(error = %e, "can't post notifications");
            None
        }
    }
}

impl Notifier for NativeNotifier {
    fn post(&self, notification: &OsNotification, _on_click: OnClick) {
        let posts = self.posts.lock().expect("notification thread");
        if let Some(posts) = posts.as_ref()
            && posts.send(notification.clone()).is_err()
        {
            tracing::warn!("the notification thread has ended");
        }
    }

    fn permission(&self) -> PermissionState {
        *self.permission.lock().expect("permission")
    }

    fn refresh_permission(&self) {
        if self.reading.swap(true, Ordering::SeqCst) {
            return;
        }
        let (in_bundle, permission, reading, changed) = (
            self.in_bundle.clone(),
            self.permission.clone(),
            self.reading.clone(),
            self.changed.clone(),
        );
        let spawned = std::thread::Builder::new()
            .name("notification permission".into())
            .spawn(move || {
                if let Some(read) = center::permission(&in_bundle) {
                    let read = match read {
                        Permission::NotAsked => PermissionState::NotAsked,
                        Permission::Allowed => PermissionState::Allowed,
                        Permission::TurnedOff => PermissionState::TurnedOff,
                    };
                    let before =
                        std::mem::replace(&mut *permission.lock().expect("permission"), read);
                    let tell = changed.lock().expect("permission listener").clone();
                    if before != read
                        && let Some(tell) = tell
                    {
                        tell();
                    }
                }
                reading.store(false, Ordering::SeqCst);
            });
        if let Err(e) = spawned {
            self.reading.store(false, Ordering::SeqCst);
            tracing::warn!(error = %e, "can't read the notification permission");
        }
    }

    fn on_permission(&self, changed: PermissionChanged) {
        *self.changed.lock().expect("permission listener") = Some(changed);
    }

    fn on_clicks(&self, open: OpenTarget) {
        *self.open.lock().expect("click handler") = Some(open);
    }
}

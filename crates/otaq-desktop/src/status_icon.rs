//! The status icon's look and menu as the link to the service and its lists change (d0139, TRY-04; the model is
//! [`otaq_view::view_models::status_icon`]). The door draws each [`StatusIcon::changes`] on the native icon.
//!
//! **The summary, kept current** (T-0567; TRY-04, TRY-05): `StatusIcon` follows the link (with [`IconLink`]'s grace
//! period) and the service's events, and after each event that can change what the icon shows it asks the service
//! again once a burst has settled: `needs_you.list` (every project's needs-you items, waiting Noyan conversations and
//! running tickets in one answer) and `agents.pause_counts` for All projects. No polling: nothing is asked without an
//! event, a resync or a new connection.

use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use otaq_core::agents::pause::{PauseCounts, PauseScope};
use otaq_core::attention::needs_you::NeedsYouList;
use otaq_core::client::Client;
use otaq_core::protocol::{Reply, Request};
use otaq_view::view_models::connection::LinkState;
use otaq_view::view_models::status_icon::{IconLink, IconLook, Summary, summary};
use serde_json::Value;
use tokio::sync::{mpsc, watch};

use crate::events::EventHub;
use crate::service::ServiceLink;

pub use summary_feed::StatusIcon;

mod summary_feed {
    use super::*;

    /// How long a burst of events may go on before the icon asks again (a turn ending sends several at once).
    const SETTLE: Duration = Duration::from_millis(150);
    /// A question to the service that takes longer is given up; the next event asks again.
    const TIMEOUT: Duration = Duration::from_secs(5);
    /// The service events after which the icon asks again: what needs you changed, a card's status (a ticket starts or
    /// stops working), a ticket or project came or went, a project was paused or resumed, a session started or ended.
    pub const ICON_EVENTS: &[&str] = &[
        "needs_you_changed",
        "card_status_changed",
        "ticket_changed",
        "ticket_deleted",
        "projects_changed",
        "pause_changed",
        "session_state",
    ];

    /// Whether the page event `event` can change the icon: one of [`ICON_EVENTS`], or a resync (events may have been
    /// missed).
    pub fn asks_again(event: &Value) -> bool {
        match event["kind"].as_str() {
            Some("resync") => true,
            Some("service") => ICON_EVENTS.contains(&event["name"].as_str().unwrap_or_default()),
            _ => false,
        }
    }

    /// What the summary is built from, as last known.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Inputs {
        /// The link's look: plain, or not connected past the grace period.
        pub connection: IconLook,
        pub list: Option<NeedsYouList>,
        pub pause: Option<PauseCounts>,
    }

    impl Default for Inputs {
        fn default() -> Self {
            Self {
                connection: IconLook::Plain,
                list: None,
                pause: None,
            }
        }
    }

    impl Inputs {
        pub fn summary(&self) -> Summary {
            summary(self.connection, self.list.as_ref(), self.pause.as_ref())
        }
    }

    /// The icon's summary, following the service from [`StatusIcon::start`] on.
    pub struct StatusIcon {
        summary: watch::Sender<Summary>,
    }

    impl StatusIcon {
        /// Follows `service` and the app's `events` until the returned value is dropped. Call inside the app's tokio
        /// runtime.
        pub fn start(service: &ServiceLink, events: Arc<EventHub>) -> Arc<Self> {
            let icon = Arc::new(Self {
                summary: watch::Sender::new(Inputs::default().summary()),
            });
            let (sender, receiver) = mpsc::unbounded_channel();
            events.subscribe(Box::new(move |event| sender.send(event).is_ok()));
            tokio::spawn(follow_summary(
                Arc::downgrade(&icon),
                service.states(),
                service.clients(),
                receiver,
            ));
            icon
        }

        /// The summary now (the door follows [`Self::changes`]).
        #[cfg(test)]
        pub fn summary(&self) -> Summary {
            self.summary.borrow().clone()
        }

        /// Every change of the summary (the tray redraws on each).
        pub fn changes(&self) -> watch::Receiver<Summary> {
            self.summary.subscribe()
        }

        fn set(&self, inputs: &Inputs) {
            let next = inputs.summary();
            self.summary.send_if_modified(|now| {
                let changed = *now != next;
                *now = next;
                changed
            });
        }
    }

    async fn follow_summary(
        weak: Weak<StatusIcon>,
        mut states: watch::Receiver<LinkState>,
        clients: watch::Receiver<Option<Client>>,
        mut events: mpsc::UnboundedReceiver<Value>,
    ) {
        let mut link = IconLink::default();
        let mut inputs = Inputs::default();
        let on_link = |link: &mut IconLink, state: &LinkState, inputs: &mut Inputs| {
            let now = Instant::now();
            link.on_link(state, now);
            inputs.connection = link.connection_look(now);
            matches!(state, LinkState::Connected(_))
        };
        let first = states.borrow_and_update().clone();
        // Ask once a burst has settled: the first event sets the time, later ones in the burst don't move it.
        let mut ask_at = on_link(&mut link, &first, &mut inputs).then(|| Instant::now() + SETTLE);
        loop {
            let grace_ends = link.changes_at(Instant::now());
            tokio::select! {
                changed = states.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    let state = states.borrow_and_update().clone();
                    if on_link(&mut link, &state, &mut inputs) {
                        ask_at.get_or_insert_with(|| Instant::now() + SETTLE);
                    }
                    let Some(icon) = weak.upgrade() else { return };
                    icon.set(&inputs);
                }
                event = events.recv() => {
                    let Some(event) = event else { return };
                    if asks_again(&event) {
                        ask_at.get_or_insert_with(|| Instant::now() + SETTLE);
                    }
                }
                // The link stayed down past the grace period: not connected.
                () = tokio::time::sleep_until(grace_ends.unwrap_or_else(Instant::now).into()), if grace_ends.is_some() => {
                    inputs.connection = link.connection_look(Instant::now());
                    let Some(icon) = weak.upgrade() else { return };
                    icon.set(&inputs);
                }
                () = tokio::time::sleep_until(ask_at.unwrap_or_else(Instant::now).into()), if ask_at.is_some() => {
                    ask_at = None;
                    let client = clients.borrow().clone();
                    if let Some(client) = client {
                        ask(&client, &mut inputs).await;
                    }
                    let Some(icon) = weak.upgrade() else { return };
                    icon.set(&inputs);
                }
            }
        }
    }

    /// Both lists from the service; one it doesn't answer in time stays as last known.
    async fn ask(client: &Client, inputs: &mut Inputs) {
        let request = |r| tokio::time::timeout(TIMEOUT, client.request(r));
        if let Ok(Ok(Reply::NeedsYou(list))) = request(Request::NeedsYouList).await {
            inputs.list = Some(*list);
        }
        if let Ok(Ok(Reply::PauseCounts(counts))) =
            request(Request::AgentsPauseCounts(PauseScope::All)).await
        {
            inputs.pause = Some(*counts);
        }
    }
}

#[cfg(test)]
mod summary_tests {
    use std::sync::Mutex;

    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use otaq_view::view_models::status_icon::{IconAction, IconLook};
    use serde_json::json;

    use super::summary_feed::*;
    use super::*;
    use crate::service::testing::{Harness, connected};

    #[test]
    fn it_asks_again_after_the_events_that_change_it() {
        let service =
            |name: &str| json!({"kind": "service", "name": name, "event": {"kind": name}});
        for name in ICON_EVENTS {
            assert!(asks_again(&service(name)), "{name}");
        }
        assert!(asks_again(&json!({"kind": "resync"})));
        assert!(!asks_again(&service("usage_changed")));
        assert!(!asks_again(&service("output")));
        assert!(!asks_again(&json!({"kind": "notices", "notices": {}})));
    }

    /// Waits until the summary matches, without asking the service itself: only events move it.
    async fn until(icon: &StatusIcon, what: &str, want: impl Fn(&Summary) -> bool) {
        let mut changes = icon.changes();
        let reached = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if want(&changes.borrow_and_update()) {
                    return;
                }
                changes.changed().await.unwrap();
            }
        })
        .await;
        assert!(reached.is_ok(), "never: {what} (last {:?})", icon.summary());
    }

    /// T-0566 (TRY-04): the icon is plain while connected and turns not connected once the stopped service stays
    /// away past the grace period (its menu: the line, Open Otaq and Quit Otaq); back to plain when it returns, with no
    /// flicker in between.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_icon_follows_the_link() {
        let h = Harness::new(None);
        let icon = StatusIcon::start(&h.app.service, h.app.events.clone());
        h.wait_for("connected", connected).await;
        let looks = Arc::new(Mutex::new(Vec::new()));
        let mut changes = icon.changes();
        let seen = looks.clone();
        let record = tokio::spawn(async move {
            loop {
                let look = changes.borrow_and_update().look;
                {
                    let mut looks = seen.lock().unwrap();
                    if looks.last() != Some(&look) {
                        looks.push(look);
                    }
                }
                if changes.changed().await.is_err() {
                    return;
                }
            }
        });
        until(&icon, "plain", |s| s.look == IconLook::Plain).await;
        h.service.stop_all().await;
        until(&icon, "not connected", |s| s.look == IconLook::NotConnected).await;
        assert!(
            icon.summary()
                .actions
                .iter()
                .any(|a| a.action == IconAction::Quit)
        );
        h.service.bind().await.unwrap();
        until(&icon, "plain again", |s| s.look == IconLook::Plain).await;
        // The recorder sees each change on its own task: wait until it has recorded the last one too.
        tokio::time::timeout(Duration::from_secs(10), async {
            while looks.lock().unwrap().last() != Some(&IconLook::Plain) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("never recorded plain again: {:?}", looks.lock().unwrap()));
        record.abort();
        assert_eq!(
            *looks.lock().unwrap(),
            [IconLook::Plain, IconLook::NotConnected, IconLook::Plain],
            "no flicker"
        );
        h.service.stop_all().await;
    }

    /// Against the real service: connecting reads the lists; a pause, a ticket that needs you and a resume each
    /// reach the summary through their events.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_summary_follows_the_services_events() {
        let h = Harness::new(None);
        let icon = StatusIcon::start(&h.app.service, h.app.events.clone());
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("shop-web", 1);
        let Reply::ProjectAdded { project, .. } = h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }))
            .await
            .unwrap()
        else {
            panic!("project added")
        };
        until(&icon, "Pause all offered", |s| {
            s.actions.iter().any(|a| a.action == IconAction::PauseAll)
        })
        .await;
        assert_eq!(icon.summary().look, IconLook::Plain);

        h.app
            .service
            .send(Request::AgentsPause(PauseScope::All))
            .await
            .unwrap();
        until(&icon, "all projects paused", |s| {
            s.look == IconLook::AllPaused
        })
        .await;
        assert!(
            icon.summary()
                .actions
                .iter()
                .any(|a| a.action == IconAction::ResumeAll)
        );

        h.app
            .service
            .send(Request::AgentsResume(PauseScope::All))
            .await
            .unwrap();
        until(&icon, "running again", |s| s.look == IconLook::Plain).await;
        assert!(
            icon.summary()
                .lines
                .iter()
                .all(|l| !format!("{l:?}").contains(&project.name)),
            "nothing paused on its own"
        );
        h.service.stop_all().await;
    }
}

#[cfg(test)]
mod image_tests {
    use otaq_view::view_models::status_icon::{LOOKS, Taskbar, badge_image, template_image};

    /// T-0568 (page 31 item 1): every look has its macOS template, and for Windows and Linux an image per taskbar
    /// (light, dark), size (16, 32 px) and count (1–9, 9+); each a PNG of its size.
    #[test]
    fn every_look_and_count_has_its_image() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/tray");
        let size = |name: &str| {
            let bytes = std::fs::read(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(&bytes[1..4], b"PNG", "{name}");
            // The IHDR chunk's width and height.
            let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
            let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
            (width, height)
        };
        let mut names = std::collections::BTreeSet::new();
        for look in LOOKS {
            assert_eq!(size(&template_image(look)), (36, 36), "{look:?}");
            names.insert(template_image(look));
            for taskbar in [Taskbar::Light, Taskbar::Dark] {
                for px in [16, 32] {
                    for count in 1..=12 {
                        let name = badge_image(look, count, taskbar, px);
                        assert_eq!(size(&name), (px, px), "{name}");
                        names.insert(name);
                    }
                }
            }
        }
        // 5 templates; per taskbar and size, 3 looks without a count and 2 with 10 counts each.
        assert_eq!(names.len(), 5 + 2 * 2 * (3 + 2 * 10));
    }
}

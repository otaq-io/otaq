//! The page's event stream: the door subscribes once per page load and every published event goes to each
//! subscriber. The service link ([`crate::service`]) publishes the service's events, a resync when events may have
//! been missed, and every change of the connection state.

use std::sync::Mutex;

use otaq_core::protocol::Event;
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use crate::notices::NoticesView;
use crate::service::Connection;

/// Where one subscriber's messages go. Returns false once the page behind it is gone, and is then dropped.
pub type Sink = Box<dyn Fn(Value) -> bool + Send + Sync>;

/// One message of the page's event stream.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "PageEvent")]
pub enum PageEvent {
    /// An event of the service as it sends it (SDD §5.4); `name` is its `kind`, e.g. `ticket_changed`.
    Service {
        name: String,
        #[ts(type = "Record<string, unknown>")]
        event: Value,
    },
    /// Events may have been missed (the connection was renewed, or the page fell too far behind): refetch what is
    /// shown.
    Resync,
    /// The link to the service changed.
    Connection { connection: Connection },
    /// The banner rows or the toasts changed (T-0231).
    Notices { notices: NoticesView },
    /// Settings → About's update parts and the rail's dot changed (UPD-04).
    Updates {
        updates: Box<otaq_view::view_models::updates::View>,
    },
    /// Files were dropped on the window at `x`, `y` (CSS pixels, from the page's top left; T-0233, T-0388, d0099).
    /// Over a terminal that takes typing it gets `paste` (the paths quoted as Terminal.app does); over one that
    /// doesn't nothing happens; anywhere else Add project opens with the first path.
    FilesDropped {
        paths: Vec<String>,
        paste: String,
        x: f64,
        y: f64,
    },
}

impl PageEvent {
    pub fn service(event: &Event) -> Self {
        let event = serde_json::to_value(event).expect("events serialize");
        Self::Service {
            name: event["kind"].as_str().unwrap_or_default().to_owned(),
            event,
        }
    }
}

#[derive(Default)]
pub struct EventHub {
    sinks: Mutex<Vec<Sink>>,
}

impl EventHub {
    pub fn subscribe(&self, sink: Sink) {
        self.sinks.lock().expect("event sinks").push(sink);
    }

    pub fn publish(&self, event: &PageEvent) {
        let event = serde_json::to_value(event).expect("page events serialize");
        self.sinks
            .lock()
            .expect("event sinks")
            .retain(|sink| sink(event.clone()));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use serde_json::json;

    use super::*;

    #[test]
    fn every_subscriber_gets_each_event_and_gone_pages_are_dropped() {
        let hub = EventHub::default();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let alive = Arc::new(AtomicBool::new(true));
        for name in ["first", "second"] {
            let (seen, alive) = (seen.clone(), alive.clone());
            hub.subscribe(Box::new(move |event| {
                seen.lock()
                    .unwrap()
                    .push(format!("{name} {}", event["kind"]));
                name == "first" || alive.load(Ordering::SeqCst)
            }));
        }
        hub.publish(&PageEvent::Resync);
        alive.store(false, Ordering::SeqCst);
        hub.publish(&PageEvent::service(&Event::ProjectsChanged));
        hub.publish(&PageEvent::Connection {
            connection: Connection::Stopped,
        });
        assert_eq!(
            *seen.lock().unwrap(),
            [
                "first \"resync\"",
                "second \"resync\"",
                "first \"service\"",
                "second \"service\"",
                "first \"connection\""
            ]
        );
    }

    #[test]
    fn dropped_files_carry_their_paths_the_paste_and_the_point() {
        let event = PageEvent::FilesDropped {
            paths: vec!["/tmp/a b.png".into(), "/tmp/c".into()],
            paste: r"/tmp/a\ b.png /tmp/c ".into(),
            x: 120.5,
            y: 48.0,
        };
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            json!({"kind": "files_dropped", "paths": ["/tmp/a b.png", "/tmp/c"],
                   "paste": r"/tmp/a\ b.png /tmp/c ", "x": 120.5, "y": 48.0})
        );
    }

    #[test]
    fn a_service_event_is_named_by_its_kind() {
        let event = Event::RunChanged {
            project: "SHOP".into(),
            key: "SHOP-4".parse().unwrap(),
        };
        assert_eq!(
            serde_json::to_value(PageEvent::service(&event)).unwrap(),
            json!({"kind": "service", "name": "run_changed",
                   "event": {"kind": "run_changed", "project": "SHOP", "key": "SHOP-4"}})
        );
    }
}

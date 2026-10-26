//! The live event stream (d0055, SDD §5.4). Later tickets publish ticket, session, Needs you, banner and usage
//! events here; every subscribed connection forwards them to its client in order.
//!
//! Publishing never waits: a subscriber that falls too far behind (its connection's queue is full, or it
//! missed events) is disconnected. The client reconnects and fetches fresh state.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{broadcast, mpsc, watch};

use crate::protocol::{Event, Message};

/// Publishes events to all subscribers. Cheap to clone.
#[derive(Debug, Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Event>,
    /// Listeners inside the service (the scheduler), which aren't connections.
    internal: Arc<AtomicUsize>,
    /// Connections subscribed as the desktop app (Q565): the app is open while one is.
    apps: Arc<AtomicUsize>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        Self {
            sender: broadcast::channel(capacity).0,
            internal: Arc::new(AtomicUsize::new(0)),
            apps: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Sends an event to every subscriber. Never blocks; with no subscribers the event is dropped.
    pub fn publish(&self, event: Event) {
        let _ = self.sender.send(event);
    }

    /// Connections currently subscribed.
    pub fn subscriber_count(&self) -> usize {
        self.sender
            .receiver_count()
            .saturating_sub(self.internal.load(Ordering::SeqCst))
    }

    /// Connections subscribed as the desktop app ([`Self::app_subscribed`]): the CLI's (`otaq attach`, say) never
    /// count.
    pub fn app_count(&self) -> usize {
        self.apps.load(Ordering::SeqCst)
    }

    /// A connection subscribed as the desktop app; it counts until the guard is dropped.
    pub fn app_subscribed(&self) -> AppSubscription {
        self.apps.fetch_add(1, Ordering::SeqCst);
        AppSubscription(self.apps.clone())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// A listener inside the service; it isn't counted as a subscribed connection.
    pub fn listen(&self) -> Listener {
        self.internal.fetch_add(1, Ordering::SeqCst);
        Listener {
            receiver: self.sender.subscribe(),
            internal: self.internal.clone(),
        }
    }
}

/// The desktop app's subscription ([`EventBus::app_subscribed`]).
pub struct AppSubscription(Arc<AtomicUsize>);

impl Drop for AppSubscription {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// An in-service listener of the bus ([`EventBus::listen`]).
pub struct Listener {
    pub receiver: broadcast::Receiver<Event>,
    internal: Arc<AtomicUsize>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.internal.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Moves events from the bus into one connection's outgoing queue until the connection ends. If the client
/// can't keep up, sets `kick` so the connection closes.
pub(super) async fn forward(
    mut events: broadcast::Receiver<Event>,
    out: mpsc::Sender<Message>,
    kick: Arc<watch::Sender<bool>>,
) {
    loop {
        match events.recv().await {
            Ok(event) => match out.try_send(Message::event(event)) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    tracing::warn!("subscriber fell behind (queue full); disconnecting it");
                    kick.send_replace(true);
                    return;
                }
                Err(TrySendError::Closed(_)) => return,
            },
            Err(RecvError::Lagged(missed)) => {
                tracing::warn!(
                    missed,
                    "subscriber fell behind (missed events); disconnecting it"
                );
                kick.send_replace(true);
                return;
            }
            Err(RecvError::Closed) => return,
        }
    }
}

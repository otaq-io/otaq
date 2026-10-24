//! At most one pop-up every 2 minutes (d0090): the first notification goes out at once; new ones in the next 2
//! minutes (all projects) are held. At the end of the period one held item is posted as itself, two or more as one
//! "N more agents need you" summary; items answered meanwhile are dropped, and when none remain nothing is posted.
//! A new period starts only when something was posted. The 3-at-once rule ([`super::group`]) applies to what is
//! posted at once. Fixed, no setting.
//!
//! Time comes from a [`Clock`]: the service's is the system's, tests move a [`FakeClock`] by hand.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use tokio::sync::watch;

use super::{OsNotification, Planned, group, summary};

/// How long new notifications are held after a pop-up.
pub const QUIET: Duration = Duration::from_secs(2 * 60);

/// Where the quiet period's time comes from.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    /// Resolves once [`Self::now`] reaches `at`.
    fn sleep_until(&self, at: Instant) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

/// The system's time.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep_until(&self, at: Instant) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(tokio::time::sleep_until(at.into()))
    }
}

/// A clock that moves only when told ([`Self::advance`]), for tests: nothing sleeps for 2 minutes.
pub struct FakeClock {
    start: Instant,
    offset: watch::Sender<Duration>,
}

impl Default for FakeClock {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            offset: watch::Sender::new(Duration::ZERO),
        }
    }
}

impl FakeClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves the clock on by `by`; sleeps that end by then wake.
    pub fn advance(&self, by: Duration) {
        self.offset.send_modify(|offset| *offset += by);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.start + *self.offset.borrow()
    }

    fn sleep_until(&self, at: Instant) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let (start, mut offset) = (self.start, self.offset.subscribe());
        Box::pin(async move {
            if offset.wait_for(|o| start + *o >= at).await.is_err() {
                std::future::pending::<()>().await;
            }
        })
    }
}

/// The quiet period and what it holds.
#[derive(Debug, Default)]
pub struct Pace {
    /// The end of the current period; `None` when none runs.
    quiet_until: Option<Instant>,
    held: Vec<Planned>,
}

impl Pace {
    /// What the end of the period posts, if it has ended: the held items that still wait (`waiting` holds their
    /// [`super::Held::presence`]), as themselves when one is left, as a summary when several are. Call before
    /// [`Self::offer`] at every derivation.
    pub fn due(&mut self, now: Instant, waiting: &HashSet<String>) -> Vec<OsNotification> {
        if self.quiet_until.is_none_or(|until| now < until) {
            return Vec::new();
        }
        self.quiet_until = None;
        let mut held = std::mem::take(&mut self.held);
        held.retain(|p| waiting.contains(&p.item.presence));
        let out = match held.len() {
            0 => Vec::new(),
            1 => vec![held.remove(0).notification],
            _ => {
                let items: Vec<_> = held.iter().map(|p| p.item.clone()).collect();
                vec![summary(&items, held[0].notification.sound)]
            }
        };
        if !out.is_empty() {
            self.quiet_until = Some(now + QUIET);
        }
        out
    }

    /// New items: posted at once (grouped by the 3-at-once rule) when no period runs, which starts one; held
    /// otherwise.
    pub fn offer(&mut self, now: Instant, fresh: Vec<Planned>) -> Vec<OsNotification> {
        if fresh.is_empty() {
            return Vec::new();
        }
        if self.quiet_until.is_some_and(|until| now < until) {
            self.held.extend(fresh);
            return Vec::new();
        }
        self.quiet_until = Some(now + QUIET);
        group(fresh)
    }

    /// Forgets what is held: the user is looking at Otaq, or notifications are off.
    pub fn drop_held(&mut self) {
        self.held.clear();
    }

    /// Whether anything is held.
    pub fn holds(&self) -> bool {
        !self.held.is_empty()
    }

    /// When Needs you must look again: the end of the period, while something is held.
    pub fn wake_at(&self) -> Option<Instant> {
        self.quiet_until.filter(|_| self.holds())
    }
}

//! The port pool: two ports per started ticket (SES-01 step 6, d0026).
//!
//! The **agent port** is `$PORT` in the ticket's agent session (4019 for SHOP-19); the **Run port** is `$PORT` in
//! its Run and Shell (4020). Two ports, so a dev server the agent left running never blocks the user's Run, and
//! parallel agents never collide.
//!
//! Implementation choices (SDD §26):
//! - **Range:** 4000–4999 ([`DEFAULT_RANGE`]), shared by every project.
//! - **Strategy:** lowest free port first. A port is free when no ticket holds it (read from the `tickets`
//!   table, so allocations survive a service restart) **and** nothing on the machine uses it: binding it on
//!   `127.0.0.1` and on `0.0.0.0` must both succeed. The bind check is a snapshot; a program may take the port
//!   a moment later, which nothing can prevent.
//! - Allocation is serialized by a lock, so two starts never pick the same ports.
//!
//! Ports are freed at Done, Close and Park (their tickets call [`PortPool::free`]); leaked ones are listed in
//! Cleanup later (T-0116).

use std::collections::HashSet;
use std::fmt;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::ops::RangeInclusive;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::store::{Store, StoreError, now_ms};

/// The ports handed out when no other range is configured.
pub const DEFAULT_RANGE: RangeInclusive<u16> = 4000..=4999;

/// A ticket's two ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TicketPorts {
    /// `$PORT` in the agent's session.
    pub agent: u16,
    /// `$PORT` in Run and Shell.
    pub run: u16,
}

/// Hands out and frees ticket ports. Cheap to clone; clones share the allocation lock.
#[derive(Debug, Clone)]
pub struct PortPool {
    store: Store,
    range: RangeInclusive<u16>,
    lock: Arc<Mutex<()>>,
}

impl PortPool {
    pub fn new(store: Store) -> Self {
        Self::with_range(store, DEFAULT_RANGE)
    }

    /// A pool over another range (tests use small ones).
    pub fn with_range(store: Store, range: RangeInclusive<u16>) -> Self {
        Self {
            store,
            range,
            lock: Arc::new(Mutex::new(())),
        }
    }

    /// The ticket's two ports. A ticket that holds ports already gets the same two again (idempotent, SES-02);
    /// otherwise the two lowest free ports are stored on it.
    pub async fn allocate(&self, ticket_id: &str) -> Result<TicketPorts, PortError> {
        let _guard = self.lock.lock().await;
        let (agent, run): (Option<i64>, Option<i64>) =
            sqlx::query_as("SELECT agent_port, run_port FROM tickets WHERE id = ?")
                .bind(ticket_id)
                .fetch_optional(self.store.pool())
                .await
                .map_err(StoreError::from)?
                .ok_or_else(|| PortError::NoSuchTicket(ticket_id.to_owned()))?;
        if let (Some(agent), Some(run)) = (to_port(agent), to_port(run)) {
            return Ok(TicketPorts { agent, run });
        }

        let held = self.held_ports().await?;
        let mut free = self
            .range
            .clone()
            .filter(|port| !held.contains(port))
            .filter(|port| is_free_on_machine(*port));
        let (Some(agent), Some(run)) = (free.next(), free.next()) else {
            return Err(PortError::Exhausted {
                range: self.range.clone(),
            });
        };
        sqlx::query("UPDATE tickets SET agent_port = ?, run_port = ?, updated_at = ? WHERE id = ?")
            .bind(i64::from(agent))
            .bind(i64::from(run))
            .bind(now_ms())
            .bind(ticket_id)
            .execute(self.store.pool())
            .await
            .map_err(StoreError::from)?;
        Ok(TicketPorts { agent, run })
    }

    /// The port of the ticket's helper for `piece` (d0133: a helper's dev server never takes the ticket's): the one it
    /// holds, else the lowest free port, stored on its row (which must exist).
    pub async fn allocate_helper(&self, ticket_id: &str, piece: u32) -> Result<u16, PortError> {
        let _guard = self.lock.lock().await;
        let held: Option<Option<i64>> =
            sqlx::query_scalar("SELECT port FROM helpers WHERE ticket_id = ? AND piece = ?")
                .bind(ticket_id)
                .bind(i64::from(piece))
                .fetch_optional(self.store.pool())
                .await
                .map_err(StoreError::from)?;
        let held = held.ok_or_else(|| PortError::NoSuchTicket(format!("{ticket_id}.{piece}")))?;
        if let Some(port) = to_port(held) {
            return Ok(port);
        }
        let taken = self.held_ports().await?;
        let port = self
            .range
            .clone()
            .filter(|port| !taken.contains(port))
            .find(|port| is_free_on_machine(*port))
            .ok_or_else(|| PortError::Exhausted {
                range: self.range.clone(),
            })?;
        sqlx::query("UPDATE helpers SET port = ? WHERE ticket_id = ? AND piece = ?")
            .bind(i64::from(port))
            .bind(ticket_id)
            .bind(i64::from(piece))
            .execute(self.store.pool())
            .await
            .map_err(StoreError::from)?;
        Ok(port)
    }

    /// Releases both of the ticket's ports; other tickets may get them from now on.
    pub async fn free(&self, ticket_id: &str) -> Result<(), PortError> {
        let _guard = self.lock.lock().await;
        sqlx::query(
            "UPDATE tickets SET agent_port = NULL, run_port = NULL, updated_at = ? WHERE id = ?",
        )
        .bind(now_ms())
        .bind(ticket_id)
        .execute(self.store.pool())
        .await
        .map_err(StoreError::from)?;
        Ok(())
    }

    /// Every port some ticket or helper holds.
    async fn held_ports(&self) -> Result<HashSet<u16>, PortError> {
        let rows: Vec<(Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT agent_port, run_port FROM tickets
             WHERE agent_port IS NOT NULL OR run_port IS NOT NULL
             UNION ALL SELECT port, NULL FROM helpers WHERE port IS NOT NULL",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(StoreError::from)?;
        Ok(rows
            .into_iter()
            .flat_map(|(a, r)| [to_port(a), to_port(r)])
            .flatten()
            .collect())
    }
}

fn to_port(value: Option<i64>) -> Option<u16> {
    value.and_then(|v| u16::try_from(v).ok())
}

/// [`is_free_on_machine`], asked again a few times over 100 ms before saying no. A port can look taken for a
/// moment without anyone using it: on Linux a process forked at the instant another check holds its test socket
/// keeps that socket until it starts its program. Run's "port in use" waits for this (seen in the Linux VM, M11).
pub async fn is_free_on_machine_settled(port: u16) -> bool {
    for _ in 0..4 {
        if is_free_on_machine(port) {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    is_free_on_machine(port)
}

/// Whether nothing on this machine listens on `port`: it can be bound on the loopback and on all interfaces.
/// Dev servers bind either, so both are checked.
pub fn is_free_on_machine(port: u16) -> bool {
    [Ipv4Addr::LOCALHOST, Ipv4Addr::UNSPECIFIED]
        .into_iter()
        .all(|ip| TcpListener::bind(SocketAddrV4::new(ip, port)).is_ok())
}

#[derive(Debug)]
pub enum PortError {
    /// Every port of the range is held or in use. The start sequence reports this as its failing step.
    Exhausted {
        range: RangeInclusive<u16>,
    },
    NoSuchTicket(String),
    Store(StoreError),
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhausted { range } => write!(
                f,
                "no two free ports left in {}–{}; finish or park some tickets, or stop programs using these ports",
                range.start(),
                range.end()
            ),
            Self::NoSuchTicket(id) => write!(f, "unknown ticket {id}"),
            Self::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PortError {}

impl From<StoreError> for PortError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

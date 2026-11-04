//! The port pool: the ports of a started ticket (SES-01 step 6, d0026; d{RUN} for apps).
//!
//! The **agent port** is `$PORT` in the ticket's agent session (4019 for SHOP-19); each of the project's apps has its
//! own **Run port** (4020 for `web`, 4021 for `api`): `$PORT` in that app's Run, `OTAQ_PORT_<NAME>` in every app's Run
//! and in the Shell. Separate ports, so a dev server the agent left running never blocks a Run, and parallel tickets
//! never collide. A project with one app (or none yet) has one Run port, as before apps had names.
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

/// A ticket's ports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketPorts {
    /// `$PORT` in the agent's session.
    pub agent: u16,
    /// Each app's Run port, in the order asked for.
    pub apps: Vec<(String, u16)>,
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

    /// The ticket's agent port and a Run port for each of `apps`. Ports it holds already come back the same
    /// (idempotent, SES-02); the missing ones are the lowest free ports, stored on it. All or nothing: when the range
    /// can't give every missing port, nothing is stored.
    pub async fn allocate(
        &self,
        ticket_id: &str,
        apps: &[String],
    ) -> Result<TicketPorts, PortError> {
        let _guard = self.lock.lock().await;
        let agent: Option<i64> = sqlx::query_scalar("SELECT agent_port FROM tickets WHERE id = ?")
            .bind(ticket_id)
            .fetch_optional(self.store.pool())
            .await
            .map_err(StoreError::from)?
            .ok_or_else(|| PortError::NoSuchTicket(ticket_id.to_owned()))?;
        let held_agent = to_port(agent);
        let (mut apps, missing) = self.split_apps(ticket_id, apps).await?;
        let mut fresh = self
            .lowest_free(missing.len() + usize::from(held_agent.is_none()))
            .await?
            .into_iter();
        let agent = match held_agent {
            Some(agent) => agent,
            None => {
                let agent = fresh.next().expect("counted");
                sqlx::query("UPDATE tickets SET agent_port = ?, updated_at = ? WHERE id = ?")
                    .bind(i64::from(agent))
                    .bind(now_ms())
                    .bind(ticket_id)
                    .execute(self.store.pool())
                    .await
                    .map_err(StoreError::from)?;
                agent
            }
        };
        self.store_apps(ticket_id, &mut apps, missing, fresh)
            .await?;
        Ok(TicketPorts { agent, apps })
    }

    /// A Run port for each of `apps` of the ticket (d{RUN}): the ones it holds, the missing ones the lowest free. An
    /// app added to the project while the ticket works gets its port here, at its first Run or Shell.
    pub async fn app_ports(
        &self,
        ticket_id: &str,
        apps: &[String],
    ) -> Result<Vec<(String, u16)>, PortError> {
        let _guard = self.lock.lock().await;
        let (mut out, missing) = self.split_apps(ticket_id, apps).await?;
        let fresh = self.lowest_free(missing.len()).await?.into_iter();
        self.store_apps(ticket_id, &mut out, missing, fresh).await?;
        Ok(out)
    }

    /// The Run ports the ticket holds, by app, lowest port first (no allocation).
    pub async fn held_app_ports(&self, ticket_id: &str) -> Result<Vec<(String, u16)>, PortError> {
        let rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT app, port FROM app_ports WHERE ticket_id = ? ORDER BY port")
                .bind(ticket_id)
                .fetch_all(self.store.pool())
                .await
                .map_err(StoreError::from)?;
        Ok(rows
            .into_iter()
            .filter_map(|(app, port)| Some((app, to_port(Some(port))?)))
            .collect())
    }

    /// `apps` with the ports the ticket holds (0 where it holds none yet), and the indexes still missing one.
    async fn split_apps(
        &self,
        ticket_id: &str,
        apps: &[String],
    ) -> Result<(Vec<(String, u16)>, Vec<usize>), PortError> {
        let held = self.held_app_ports(ticket_id).await?;
        let mut out = Vec::with_capacity(apps.len());
        let mut missing = Vec::new();
        for (index, app) in apps.iter().enumerate() {
            match held.iter().find(|(name, _)| name == app) {
                Some((_, port)) => out.push((app.clone(), *port)),
                None => {
                    missing.push(index);
                    out.push((app.clone(), 0));
                }
            }
        }
        Ok((out, missing))
    }

    /// Stores the `fresh` ports on the `missing` apps of `out`, in order.
    async fn store_apps(
        &self,
        ticket_id: &str,
        out: &mut [(String, u16)],
        missing: Vec<usize>,
        fresh: impl Iterator<Item = u16>,
    ) -> Result<(), PortError> {
        for (index, port) in missing.into_iter().zip(fresh) {
            sqlx::query("INSERT INTO app_ports (ticket_id, app, port) VALUES (?, ?, ?)")
                .bind(ticket_id)
                .bind(&out[index].0)
                .bind(i64::from(port))
                .execute(self.store.pool())
                .await
                .map_err(StoreError::from)?;
            out[index].1 = port;
        }
        Ok(())
    }

    /// The `count` lowest ports of the range no ticket or helper holds that nothing on the machine uses; exhausted
    /// when there are fewer.
    async fn lowest_free(&self, count: usize) -> Result<Vec<u16>, PortError> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let held = self.held_ports().await?;
        let found: Vec<u16> = self
            .range
            .clone()
            .filter(|port| !held.contains(port))
            .filter(|port| is_free_on_machine(*port))
            .take(count)
            .collect();
        if found.len() < count {
            return Err(PortError::Exhausted {
                range: self.range.clone(),
            });
        }
        Ok(found)
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
        let port = self.lowest_free(1).await?[0];
        sqlx::query("UPDATE helpers SET port = ? WHERE ticket_id = ? AND piece = ?")
            .bind(i64::from(port))
            .bind(ticket_id)
            .bind(i64::from(piece))
            .execute(self.store.pool())
            .await
            .map_err(StoreError::from)?;
        Ok(port)
    }

    /// Releases all of the ticket's ports (its agent's and every app's); other tickets may get them from now on.
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
        sqlx::query("DELETE FROM app_ports WHERE ticket_id = ?")
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
             UNION ALL SELECT port, NULL FROM helpers WHERE port IS NOT NULL
             UNION ALL SELECT port, NULL FROM app_ports",
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
                "no free ports left in {}–{}; finish or park some tickets, or stop programs using these ports",
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

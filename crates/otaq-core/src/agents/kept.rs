//! **The kept starts** (PAU-04, PAU-06; d0112): while a project is paused nothing starts an agent there on its own,
//! and each automatic start is **kept** where it already waits rather than failing, so the project's Resume runs it
//! once. One marker, one backstop: every session says who asked ([`Start`]), and [`crate::session::Sessions::spawn`]
//! refuses an automatic one in a paused project. Where each kept start waits:
//!
//! - **a ticket's agent** (the scheduler from Ready for Dev and between two Agent stages, a drag, Send back, a
//!   conflict's `restack`, d0064): Queued, as it was ([`crate::start::StartOutcome::Kept`]); the scheduler holds a
//!   paused project's tickets and starts them once it wakes;
//! - **a check's checker** (after a submit or a pass, a drag into a check, d0063): the ticket is interrupted from its
//!   check the pause's way, and Resume starts a fresh check ([`crate::recovery::Recovery::kept_paused`]);
//! - **an answer session** (d0030): the question waits, and Resume routes it again;
//! - **an automatic PR Delivery**: the card waits in its Delivery, and Resume runs it;
//! - **a held start** (the launch gate's, d0086): its hold stays, and Resume lifts the holds once more.
//!
//! [`register`] runs these on every project's Resume, after its stopped agents were resumed and before the scheduler
//! wakes. Each one runs once: a second Resume finds nothing waiting.

use std::sync::Arc;

use super::gate::LaunchGate;
use super::pause::Pause;
use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError};
use crate::domain::{Status, TicketKey};
use crate::messages::Messages;
use crate::project_agents::ProjectAgents;
use crate::start::sequence::WAITING_TO_START;

#[cfg(doc)]
use crate::session::Start;

/// What [`register`] needs.
pub struct KeptParts {
    pub board: Board,
    pub gate: LaunchGate,
    pub messages: Messages,
    pub project_agents: ProjectAgents,
}

/// Runs the project's kept starts on each Resume ([`Pause::on_resume`]).
pub fn register(pause: &Pause, parts: KeptParts) {
    let parts = Arc::new(parts);
    pause.on_resume(Arc::new(move |project| {
        let parts = parts.clone();
        Box::pin(async move {
            if let Err(e) = run(&parts, &project).await {
                tracing::warn!(%project, error = %e, "the kept starts didn't all run");
            }
        })
    }));
}

/// The kept starts of `project`, now that it runs again.
async fn run(parts: &KeptParts, project: &str) -> Result<(), BoardError> {
    // Held starts: the holds a paused project kept are checked again (a Developer goes back to Queued).
    let lifted = parts.gate.lift().await?;
    // Questions waiting for a paused target.
    parts.messages.route_waiting().await?;
    // Automatic Deliveries that waited.
    let mut deliveries = Vec::new();
    for ticket in parts.board.tickets(project).await? {
        if ticket.status == Status::ToDeliver && parts.board.pr_flow(&ticket.id).await?.is_none() {
            parts.board.delivery().arrived(&ticket.id).await;
            deliveries.push(ticket.key);
        }
    }
    tracing::info!(project, lifted, deliveries = ?deliveries, "kept starts run");
    Ok(())
}

/// Whether `ticket` waits with a kept start (the card's "Paused · starts when you resume"): its project is paused
/// (`paused`) and it waits to start with nothing else in the way, so the scheduler would start it now. `tickets` are
/// its project's, for its dependencies. A ticket the pause stopped is Interrupted ("Paused") instead.
pub fn starts_on_resume(ticket: &Ticket, tickets: &[Ticket], paused: bool) -> bool {
    paused
        && WAITING_TO_START.contains(&ticket.status)
        && crate::board::readiness::readiness(ticket, tickets).is_ready()
}

/// The tickets of `project` that wait with a kept start ([`starts_on_resume`]), in key order.
pub async fn kept_tickets(board: &Board, project: &str) -> Result<Vec<TicketKey>, BoardError> {
    let paused = super::pause::is_paused(board.store(), project).await?;
    let tickets = board.tickets(project).await?;
    let mut keys: Vec<TicketKey> = tickets
        .iter()
        .filter(|t| starts_on_resume(t, &tickets, paused))
        .map(|t| t.key.clone())
        .collect();
    keys.sort();
    Ok(keys)
}

//! A model the tool refused (AGT-05, SDD §7.2 `[M22]` rows, §11.4 `StopFailure`; d0088, d0086, d0018): the tool
//! started, but its first API call failed on the model ([`ToolEvent::ModelRefused`](crate::agent_tools::ToolEvent)).
//!
//! - **A Developer's first turn** (no turn of the session got through, and no earlier session of the ticket
//!   started): the ticket counts as not started, back in Ready for Dev as Start failed
//!   ([`crate::start::Starter::model_refused`]).
//! - **A Validator round**: No result, not counted as a round ([`crate::validation::Validation::model_refused`]).
//! - **A Project agent's first turn** (Q-C): the session ends, its terminal keeps the tool's message.
//! - **A resumed session or a later turn**: today's failed turn ([`Signal::Failed`]).
//!
//! In every case the session's agent is marked ([`crate::agents::mark`]). Nothing here starts a session
//! (d0018): Retry start, Retry validation or a new row are the user's.
//!
//! The mark also clears here: when a session of a marked entry gets through its first turn on the entry's model.

use super::{Signal, Signals};
use crate::agent_tools::Tool;
use crate::agents::mark;
use crate::board::BoardError;
use crate::caller::Caller;
use crate::session::{END_MODEL_REFUSED, Sessions};
use crate::start::Starter;
use crate::validation::Validation;

/// The error kind a refused model's failed turn is logged with (a resumed session or a later turn).
pub const MODEL_REFUSED: &str = "model_refused";

/// What applies a refusal beyond the mark (set once at service start; unset, every refusal is a failed turn).
#[derive(Clone)]
pub struct Refusals {
    pub starter: Starter,
    pub validation: Validation,
    pub sessions: Sessions,
}

/// The refused session as stored.
#[derive(Debug, sqlx::FromRow)]
struct Stored {
    kind: String,
    ticket_id: Option<String>,
    project_agent_id: Option<String>,
    agent_id: Option<String>,
    model: Option<String>,
    model_ran: Option<String>,
    started_at: i64,
    project_name: String,
}

impl Stored {
    async fn of(signals: &Signals, session: &str) -> Result<Option<Self>, BoardError> {
        Ok(sqlx::query_as(
            "SELECT s.kind, s.ticket_id, s.project_agent_id, COALESCE(s.agent_id, a.agent_id) AS
                    agent_id, s.model, COALESCE(s.model_ran, a.model_ran) AS model_ran, s.started_at,
                    p.name AS project_name
             FROM sessions s JOIN projects p ON p.id = s.project_id
             LEFT JOIN project_agents a ON a.id = s.project_agent_id
             WHERE s.id = ?",
        )
        .bind(session)
        .fetch_optional(signals.inner.board.store().pool())
        .await?)
    }
}

impl Signals {
    /// Gives the signals what applies a refused model (once, at service start).
    pub fn set_refusals(&self, refusals: Refusals) {
        let _ = self.inner.refusals.set(refusals);
    }

    /// Whether a turn of `session` got through (ended normally) since it started.
    pub(super) fn got_through_a_turn(&self, session: &str) -> bool {
        self.inner.turned.lock().expect("turned").contains(session)
    }

    /// A turn of `session` ended normally: on its first, the session's agent loses its mark when the session
    /// started after the mark and ran on the model the entry has now (AGT-05).
    pub(super) async fn turn_got_through(&self, session: &str) -> Result<(), BoardError> {
        if !self
            .inner
            .turned
            .lock()
            .expect("turned")
            .insert(session.to_owned())
        {
            return Ok(());
        }
        let Some(stored) = Stored::of(self, session).await? else {
            return Ok(());
        };
        if let Some(agent) = &stored.agent_id
            && mark::clear_after_first_turn(
                self.inner.board.store(),
                agent,
                stored.model.as_deref(),
                stored.started_at,
            )
            .await?
        {
            tracing::info!(
                session,
                agent,
                "the agent's mark cleared: a first turn got through"
            );
            self.inner
                .events
                .publish(crate::protocol::Event::AgentsChanged);
        }
        Ok(())
    }

    /// `tool` refused `session`'s model (`model`: the one its message names). Marks the session's agent and
    /// applies the refusal; returns the failed turn's signal for a resumed session or a later turn, `None` when the
    /// refusal was applied as not started or No result.
    pub(super) async fn model_refused(
        &self,
        caller: &Caller,
        tool: Tool,
        session: &str,
        model: Option<&str>,
        message: &str,
    ) -> Result<Option<Signal>, BoardError> {
        let failed = Some(Signal::Failed {
            error: MODEL_REFUSED.to_owned(),
            message: Some(message.to_owned()),
        });
        let Some(stored) = Stored::of(self, session).await? else {
            return Ok(failed);
        };
        let named = mark::model_named(stored.model_ran.as_deref(), stored.model.as_deref(), model);
        let on = match caller {
            Caller::Agent { ticket, .. }
            | Caller::Validator { ticket, .. }
            | Caller::Helper { ticket, .. } => ticket.to_string(),
            Caller::ProjectAgent { .. } | Caller::User => stored.project_name.clone(),
        };
        let tool = self.inner.adapters.get(tool).display_name();
        if let Some(agent) = &stored.agent_id
            && mark::set(
                self.inner.board.store(),
                agent,
                &mark::mark(tool, named, &on, message),
            )
            .await?
        {
            self.inner
                .events
                .publish(crate::protocol::Event::AgentsChanged);
        }
        tracing::warn!(
            session,
            model = named,
            message,
            "the tool refused the model"
        );
        let Some(refusals) = self.inner.refusals.get() else {
            return Ok(failed);
        };
        let first_turn = !self.got_through_a_turn(session);
        let applied = match caller {
            Caller::Validator { ticket_id, .. } => {
                refusals
                    .validation
                    .model_refused(ticket_id, session, named, message)
                    .await?;
                true
            }
            Caller::Agent { ticket_id, .. }
                if first_turn
                    && stored.kind == crate::session::SessionKind::Agent.as_str()
                    && !self.started_before(&stored, session).await? =>
            {
                refusals
                    .starter
                    .model_refused(ticket_id, session, message)
                    .await?
            }
            Caller::ProjectAgent { .. }
                if first_turn && !self.started_before(&stored, session).await? =>
            {
                if let Err(e) = refusals.sessions.stop_as(session, END_MODEL_REFUSED).await {
                    tracing::warn!(session, error = %e, "can't end the refused session");
                }
                true
            }
            _ => false,
        };
        Ok(if applied { None } else { failed })
    }

    /// Whether another session of the same ticket's agent, or of the same Project agent, started before
    /// (it didn't end "not started"): `session` then resumes or follows work that ran.
    async fn started_before(&self, stored: &Stored, session: &str) -> Result<bool, BoardError> {
        let (query, owner) = match (&stored.project_agent_id, &stored.ticket_id) {
            (Some(agent), _) => (
                "SELECT EXISTS (SELECT 1 FROM sessions WHERE project_agent_id = ? AND kind = ? AND id <> ?
                                                         AND COALESCE(end_reason, '') <> ?)",
                agent,
            ),
            (None, Some(ticket)) => (
                "SELECT EXISTS (SELECT 1 FROM sessions WHERE ticket_id = ? AND kind = ? AND id <> ?
                                                         AND COALESCE(end_reason, '') <> ?)",
                ticket,
            ),
            (None, None) => return Ok(false),
        };
        Ok(sqlx::query_scalar(query)
            .bind(owner)
            .bind(&stored.kind)
            .bind(session)
            .bind(END_MODEL_REFUSED)
            .fetch_one(self.inner.board.store().pool())
            .await?)
    }
}

//! Who is calling the service: one of its agent sessions, or the user (CLI-01, CLI-04; d0031, d0032).
//!
//! The service decides by **where the process runs**: it walks the calling process's ancestry up to the root
//! process of one of its agent sessions. `$OTAQ_TICKET` alone decides nothing, so unsetting it inside an
//! agent session doesn't grant the user's full access (d0031). The calling PID is the socket's peer PID, which
//! the kernel reports; a PID claimed in the message must match it.
//!
//! Only sessions that run an agent tool count: a ticket's (the agent, the answer-only and delivery sessions of M10,
//! the validator of M12) and a Project agent's (d0068). Shell, Project terminal and Run sessions are the user's own
//! terminals: their callers are the user (CLI-03).
//!
//! Limit (implementation choice): a process that leaves the session's process tree on purpose (double fork plus
//! `setsid`, reparented to `launchd`) is no longer recognised. Claude Code runs its tools, hooks and background
//! tasks as descendants of the session, so its commands always are.
//!
//! **The role comes from the stage** (STG-10, d0108): a ticket's session has the rights of the kind of the stage its
//! stage run is in ([`by_stage`]): a session in an Agent stage is the ticket's agent (never a verdict), a session in
//! an agent check is its checker (the verdict only), whatever kind of session runs it. A session with no stage run
//! (an answer-only session, a delivery session) keeps the role of its kind.
//!
//! Windows: Git Bash (which Claude Code runs commands with) replaces a process by starting a new one and ending
//! the old, so the Windows parent chain can break inside a session. There a session's processes are also found
//! by its job object, which holds everything started inside it ([`crate::process::Group::contains`]); the
//! ancestry walk stays the first check.

use serde::{Deserialize, Serialize};

use crate::domain::{Actor, TicketKey};
use crate::process;
use crate::session::Sessions;

/// The resolved caller of one request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Caller {
    /// The user: the app, a terminal of their own, the Project terminal, Shell or Run (CLI-03).
    User,
    /// A process inside an agent session.
    Agent {
        ticket: TicketKey,
        /// The ticket's id.
        ticket_id: String,
        /// The session the caller runs in.
        session: String,
    },
    /// A process inside a ticket's validator session (d0063): it reads, and reports its verdict.
    Validator {
        ticket: TicketKey,
        ticket_id: String,
        session: String,
    },
    /// A process inside a helper's session (d0133): it works on one piece of its ticket and hands it back to the
    /// agent in charge; it never moves the ticket ([`crate::helpers`]).
    Helper {
        ticket: TicketKey,
        ticket_id: String,
        session: String,
    },
    /// A process inside a Project agent's session (d0068): it investigates and files Backlog tickets.
    ProjectAgent {
        /// The project's key.
        project: String,
        /// The Project agent's id.
        agent: String,
        session: String,
    },
}

impl Caller {
    pub fn is_agent(&self) -> bool {
        matches!(self, Self::Agent { .. })
    }

    /// The ticket of an agent, validator or helper caller.
    pub fn ticket(&self) -> Option<&TicketKey> {
        match self {
            Self::Agent { ticket, .. }
            | Self::Validator { ticket, .. }
            | Self::Helper { ticket, .. } => Some(ticket),
            Self::User | Self::ProjectAgent { .. } => None,
        }
    }

    /// The caller a running ticket session's processes are: its validator for a validator session, a helper for a
    /// helper's, else the ticket's agent (an Agent stage's, an answer or delivery session; [`by_stage`] refines it).
    pub fn of_root(root: crate::session::AgentRoot) -> Self {
        use crate::session::SessionKind;
        let (ticket, ticket_id, session) = (root.ticket, root.ticket_id, root.session);
        match root.kind {
            SessionKind::Validator => Self::Validator {
                ticket,
                ticket_id,
                session,
            },
            SessionKind::Helper => Self::Helper {
                ticket,
                ticket_id,
                session,
            },
            _ => Self::Agent {
                ticket,
                ticket_id,
                session,
            },
        }
    }

    /// The Project agent id and session of a Project agent.
    pub fn project_agent(&self) -> Option<(&str, &str)> {
        match self {
            Self::ProjectAgent { agent, session, .. } => Some((agent, session)),
            Self::User | Self::Agent { .. } | Self::Validator { .. } | Self::Helper { .. } => None,
        }
    }

    /// Who the change is recorded as (`--actor`, CLI-04). An agent is always `agent:<its KEY>`: an explicit actor
    /// can't make it the user or another agent. The user's commands use the requested actor, else the OS user;
    /// the CLI requests `agent:<KEY>` itself when `$OTAQ_TICKET` is set (d0032), so the plugin also works by
    /// hand.
    pub fn actor(&self, requested: Option<&str>) -> Actor {
        match self {
            Self::Agent { ticket, .. }
            | Self::Validator { ticket, .. }
            | Self::Helper { ticket, .. } => {
                let own = Actor::Agent(ticket.clone());
                if let Some(requested) = requested
                    && requested.parse::<Actor>().ok().as_ref() != Some(&own)
                {
                    tracing::warn!(%own, requested, "an agent's --actor was replaced by its own");
                }
                own
            }
            // A Project agent is always itself too (PAG-05): cards name it.
            Self::ProjectAgent { agent, .. } => Actor::ProjectAgent(agent.clone()),
            Self::User => requested
                .filter(|r| !r.trim().is_empty())
                .map(|r| r.parse().expect("infallible"))
                .unwrap_or_else(Actor::current_user),
        }
    }
}

/// The caller for a process: an agent (or a validator, or a Project agent) when the process or any of its
/// ancestors is the root process of a running agent (or validator, or Project agent) session of `sessions`, else
/// the user.
pub fn resolve(pid: Option<u32>, sessions: &Sessions) -> Caller {
    let Some(pid) = pid else {
        return Caller::User;
    };
    let roots = sessions.agent_roots();
    let project_agents = sessions.project_agent_roots();
    if roots.is_empty() && project_agents.is_empty() {
        return Caller::User;
    }
    let ancestry = process::ancestry(pid);
    let project_agent = ancestry
        .iter()
        .find_map(|ancestor| project_agents.iter().find(|r| r.pid == *ancestor).cloned())
        .or_else(|| {
            (!project_agents.is_empty())
                .then(|| sessions.project_agent_holding(pid))
                .flatten()
        });
    if let Some(root) = project_agent {
        return Caller::ProjectAgent {
            project: root.project,
            agent: root.agent,
            session: root.session,
        };
    }
    let by_ancestry = ancestry
        .into_iter()
        .find_map(|ancestor| roots.iter().find(|r| r.pid == ancestor).cloned());
    by_ancestry
        .or_else(|| sessions.agent_root_holding(pid))
        .map_or(Caller::User, Caller::of_root)
}

/// The stage a ticket's session ran in, from its stage run (STG-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStage {
    /// The stage's id.
    pub stage: String,
    /// The run's kind: `agent`, `check` (an agent check), `you` or `delivery`.
    pub kind: String,
}

impl SessionStage {
    /// The latest stage run `session` ran in, if any.
    pub async fn of(pool: &sqlx::SqlitePool, session: &str) -> Result<Option<Self>, sqlx::Error> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT stage_id, kind FROM stage_runs WHERE session_id = ?
             ORDER BY started_at DESC, rowid DESC LIMIT 1",
        )
        .bind(session)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|(stage, kind)| Self { stage, kind }))
    }

    /// It ran an agent check: its session is the checker.
    pub fn is_check(&self) -> bool {
        self.kind
            == crate::flow::engine::run_kind(crate::flow::StageKind::Check {
                by: crate::flow::CheckBy::Agent,
            })
    }

    /// It ran an Agent stage: its session is the ticket's agent.
    pub fn is_agent_stage(&self) -> bool {
        self.kind == crate::flow::engine::run_kind(crate::flow::StageKind::Agent)
    }
}

/// `caller`'s role from the kind of its session's stage run (STG-10): a ticket session that ran an agent check is
/// that ticket's checker ([`Caller::Validator`]), one that ran an Agent stage its agent ([`Caller::Agent`]). Other
/// callers, and sessions with no stage run, are left as [`resolve`] found them.
pub async fn by_stage(caller: Caller, pool: &sqlx::SqlitePool) -> Caller {
    let (Caller::Agent {
        ticket,
        ticket_id,
        session,
    }
    | Caller::Validator {
        ticket,
        ticket_id,
        session,
    }) = &caller
    else {
        return caller;
    };
    let stage = match SessionStage::of(pool, session).await {
        Ok(stage) => stage,
        Err(e) => {
            tracing::warn!(session, error = %e, "can't read a session's stage run");
            None
        }
    };
    let (ticket, ticket_id, session) = (ticket.clone(), ticket_id.clone(), session.clone());
    match stage {
        Some(stage) if stage.is_check() => Caller::Validator {
            ticket,
            ticket_id,
            session,
        },
        Some(stage) if stage.is_agent_stage() => Caller::Agent {
            ticket,
            ticket_id,
            session,
        },
        _ => caller,
    }
}

/// The Project terminal a user's process runs in, if any: the process or one of its ancestors is the root process
/// of a running Project terminal (an agent the user started there counts too). Tickets created from it are
/// listed in its summary (T-0167, TKT-08).
pub fn project_terminal_of(pid: Option<u32>, sessions: &Sessions) -> Option<String> {
    let pid = pid?;
    let roots = sessions.project_terminal_roots();
    if roots.is_empty() {
        return None;
    }
    process::ancestry(pid)
        .into_iter()
        .find_map(|ancestor| {
            roots
                .iter()
                .find(|(root, _)| *root == ancestor)
                .map(|(_, session)| session.clone())
        })
        .or_else(|| sessions.project_terminal_holding(pid))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> Caller {
        Caller::Agent {
            ticket: "SHOP-18".parse().unwrap(),
            ticket_id: "t18".into(),
            session: "s1".into(),
        }
    }

    #[test]
    fn agents_are_always_their_own_actor() {
        let own = Actor::Agent("SHOP-18".parse().unwrap());
        assert_eq!(agent().actor(None), own);
        assert_eq!(agent().actor(Some("agent:SHOP-18")), own);
        assert_eq!(agent().actor(Some("nevzat")), own, "can't claim the user");
        assert_eq!(agent().actor(Some("agent:SHOP-12")), own);
    }

    #[test]
    fn users_act_as_requested_or_as_the_os_user() {
        assert_eq!(Caller::User.actor(None), Actor::current_user());
        assert_eq!(Caller::User.actor(Some("")), Actor::current_user());
        assert_eq!(
            Caller::User.actor(Some("agent:SHOP-12")),
            Actor::Agent("SHOP-12".parse().unwrap())
        );
        assert_eq!(Caller::User.actor(Some("ci")), Actor::User("ci".into()));
    }
}

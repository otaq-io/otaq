//! Helpers (d0133, T-0537): a build stage's **agent in charge** splits the ticket's work among helpers, one per piece
//! of the ticket's Plan, each an agent session in its own worktree, and merges their work into the ticket's branch.
//! Helpers don't walk the flow: the checks after the build stage judge the whole ticket.
//!
//! **The agent in charge** (the ticket's live Agent-stage session; anyone else is refused, the user too) runs:
//! - `otaq helper start <n> --title "<piece>" [--stdin]` ([`Helpers::start`]): the branch `otaq/<KEY>.<n>` from the
//!   ticket's branch's head and its own worktree `wt/<repo>/<KEY>.<n>` (named after the ticket's, the piece number
//!   after a dot: "40.3" is how the piece is shown), seeded as a start seeds (d0082: the ticket's own worktree build
//!   first, else the warm build), the env files and the setup command as a start runs them, its own `$PORT`, the
//!   agent's skills and the hand-off skills copied (d0118), then a fresh session of the stage's agent on the tool
//!   and model the agent in charge runs on (Q513 may add a pick), told "You are the helper for piece 3 of SHOP-40,
//!   Wishlist page. …" with `OTAQ_PIECE=3`. The note on stdin is posted as a comment "To 40.3: …". A tool at its limit
//!   (or not available) holds the helper ("Waiting for limit reset"); it starts once the hold lifts
//!   ([`Helpers::start_held`]);
//! - `otaq helper list`, `otaq helper diff <n>` (the helper's branch against the ticket's);
//! - `otaq helper back <n> --stdin`: the note is typed into the helper's session when it is idle (d0048), status
//!   `sent back`;
//! - `otaq helper merge <n>` ([`merge`]): refused while either worktree has uncommitted files; a free rebase of the
//!   helper's branch onto the ticket's (d0038, no tokens); a conflict merges nothing and the reply says to send the
//!   helper back to rebase; clean, a squash commit on the ticket's branch in its worktree, the helper's session
//!   ended, its worktree removed (its branch stays until the ticket is Done, d0018) and the files recorded for the
//!   Files tab's piece tags.
//!
//! **A helper** (`OTAQ_PIECE` set; its session is of kind `helper`) runs `otaq helper done --stdin` (status `being
//! reviewed`, its text a comment; refused with uncommitted files) and `otaq prime` ([`prime`]: this piece first);
//! everything that moves the ticket is refused ([`crate::policy`]).
//!
//! **One slot per ticket** (d0133 audit 4, d0016, d0108): the scheduler counts tickets, so helpers never take a slot.
//! Their tokens count on the ticket and in today's total ([`crate::usage::TicketUsage::helpers`]). A helper's
//! hooks never move the ticket (questions, prompts and idle are T-0538's); its worktree left behind shows in Cleanup
//! under its ticket, and the ticket's Done removes what is left ([`Helpers::finished`]).
//!
//! Nothing here starts without the agent in charge's command, inside a stage the user started: the stage's start is
//! the go-ahead, as a check's start is.

pub mod merge;
pub mod prime;
pub mod start;
pub mod store;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::agent_tools::Adapters;
use crate::agents::gate::LaunchGate;
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{Board, BoardError, Refusal};
use crate::caller::Caller;
use crate::data_dir::DataDir;
use crate::domain::{Actor, TicketKey, stored_enum};
use crate::naming;
use crate::ports::PortPool;
use crate::protocol::Event;
use crate::service::EventBus;
use crate::session::deliveries::{Deliveries, DeliveryKind};
use crate::session::{SessionKind, Sessions};
use crate::warm::seed::Seeder;

stored_enum! {
    /// Where a helper is (the `helpers.status` column).
    pub enum HelperStatus ("helper status") {
        /// Its session works on the piece.
        Working = "working",
        /// Its tool is at its limit (or not available): it starts once the hold lifts.
        Held = "held",
        /// The agent in charge sent it back with a note.
        SentBack = "sent_back",
        /// It handed its piece back (`otaq helper done`): the agent in charge reviews it.
        BeingReviewed = "being_reviewed",
        /// Its work is on the ticket's branch.
        Merged = "merged",
    }
}

impl HelperStatus {
    /// "working", "waiting for limit reset", "sent back", "being reviewed", "merged".
    pub fn label(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Held => "waiting for limit reset",
            Self::SentBack => "sent back",
            Self::BeingReviewed => "being reviewed",
            Self::Merged => "merged",
        }
    }
}

/// One helper of a ticket, as `otaq helper list` and the sheet read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Helper {
    pub id: String,
    pub ticket: TicketKey,
    /// The piece of the ticket's Plan: 3 for "40.3".
    pub piece: u32,
    /// The piece's title: "Wishlist page".
    pub title: String,
    /// `otaq/SHOP-40.3`.
    pub branch: String,
    /// `<data>/wt/shop-web/SHOP-40.3`; gone once merged.
    pub worktree: String,
    /// Its latest session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    pub status: HelperStatus,
    /// The latest note from the agent in charge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Held: why ("Claude Code limit reached").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_reason: Option<String>,
    /// `$PORT` in its session; freed when it merges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub started_at: i64,
    /// Its last change: the list's "last activity".
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_at: Option<i64>,
    /// Merged: the paths its squash touched (the Files tab's piece tags).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

impl Helper {
    /// How the piece is shown: "40.3".
    pub fn label(&self) -> String {
        piece_label(&self.ticket, self.piece)
    }
}

/// What a helper command answers: the helper after it, and what the agent is told.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperReply {
    pub helper: Helper,
    pub text: String,
}

/// `otaq helper diff <n>`: the helper's branch against the ticket's, since they parted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperDiff {
    pub helper: Helper,
    /// The ticket's branch: `otaq/SHOP-40`.
    pub against: String,
    pub files: Vec<crate::git::diff::ChangedFile>,
    /// The patch, as `git diff` prints it.
    pub patch: String,
}

/// How a piece is shown: "40.3" (d0133 audit 2: a display name, not a ticket key).
pub use crate::domain::plan::piece_name as piece_label;

/// The name a helper's branch and worktree folder are made from: the ticket's key and the piece, `SHOP-40.3`.
pub fn folder_name(ticket: &TicketKey, piece: u32) -> String {
    format!("{ticket}.{piece}")
}

/// A helper's branch: `otaq/SHOP-40.3` (the ticket's `otaq/SHOP-40` can't also be a folder of refs).
pub fn branch_name(ticket: &TicketKey, piece: u32) -> String {
    naming::branch(&folder_name(ticket, piece))
}

/// The variable that tells a helper's session its piece: `OTAQ_PIECE`.
pub fn piece_env_var() -> String {
    naming::env_var("PIECE")
}

/// A helper's first prompt: "You are the helper for piece 3 of SHOP-40, Wishlist page. Work only on this piece in this
/// worktree. Run `otaq prime`."
pub fn first_prompt(ticket: &TicketKey, piece: u32, title: &str) -> String {
    format!(
        "You are the helper for piece {piece} of {ticket}, {title}. Work only on this piece in this worktree. Run `{} prime`.",
        naming::COMMAND_NAME
    )
}

/// What the agent in charge is told when a helper hands its piece back: "40.3 handed its piece back: Page done, with
/// tests. Read it with `otaq helper diff 3`, then merge it (`otaq helper merge 3`) or send it back (`otaq helper back 3
/// --stdin`)."
pub fn handed_back_text(label: &str, piece: u32, summary: &str) -> String {
    let cmd = naming::COMMAND_NAME;
    format!(
        "{label} handed its piece back: {}. Read it with `{cmd} helper diff {piece}`, then merge it (`{cmd} helper merge \
         {piece}`) or send it back (`{cmd} helper back {piece} --stdin`).",
        summary
            .lines()
            .next()
            .unwrap_or_default()
            .trim_end_matches('.')
    )
}

/// The refusal of a helper command from anyone but the agent in charge: "Only the agent in charge of SHOP-40 starts,
/// sends back and merges its helpers".
pub fn only_in_charge(ticket: &TicketKey) -> String {
    format!("Only the agent in charge of {ticket} starts, sends back and merges its helpers")
}

fn no_helper(ticket: &TicketKey, piece: u32) -> BoardError {
    BoardError::NotFound(format!("{ticket} has no helper for piece {piece}"))
}

/// Starts, tracks and merges helpers. Cheap to clone.
#[derive(Clone)]
pub struct Helpers {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    data: DataDir,
    sessions: Sessions,
    deliveries: Deliveries,
    adapters: Adapters,
    events: EventBus,
    ports: PortPool,
    /// Seeds a new helper's worktree (set by the service; unset, nothing is seeded).
    seeder: OnceLock<Seeder>,
    /// The tools' limits and states before a helper's launch (set by the service; unset, nothing is checked).
    gate: OnceLock<LaunchGate>,
    /// One helper start or merge at a time per ticket: git's worktree and branch steps share the repository.
    working: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl Helpers {
    pub fn new(
        board: Board,
        data: DataDir,
        sessions: Sessions,
        deliveries: Deliveries,
        adapters: Adapters,
        events: EventBus,
        ports: PortPool,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                board,
                data,
                sessions,
                deliveries,
                adapters,
                events,
                ports,
                seeder: OnceLock::new(),
                gate: OnceLock::new(),
                working: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Gives the helpers the service's seeder (WRM-04).
    pub fn set_seeder(&self, seeder: Seeder) {
        let _ = self.inner.seeder.set(seeder);
    }

    /// Gives the helpers the service's check before a launch (the plan limit, d0020).
    pub fn set_gate(&self, gate: LaunchGate) {
        let _ = self.inner.gate.set(gate);
    }

    pub fn board(&self) -> &Board {
        &self.inner.board
    }

    fn lock_of(&self, ticket_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.inner
            .working
            .lock()
            .expect("helper locks")
            .entry(ticket_id.to_owned())
            .or_default()
            .clone()
    }

    /// The ticket `key` when `caller` is its agent in charge (its live Agent-stage session); refused for anyone else,
    /// the user too. Returns the ticket and the agent in charge's session.
    pub async fn in_charge(
        &self,
        caller: &Caller,
        key: &str,
    ) -> Result<(Ticket, crate::session::SessionInfo), BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        let refuse = || BoardError::refused(Refusal::NotAllowed, only_in_charge(&ticket.key));
        let Caller::Agent {
            ticket_id, session, ..
        } = caller
        else {
            return Err(refuse());
        };
        let live = self
            .inner
            .sessions
            .running_of(&ticket.id, SessionKind::Agent)
            .filter(|live| *ticket_id == ticket.id && live.id == *session)
            .ok_or_else(refuse)?;
        Ok((ticket, live))
    }

    /// The helper `caller` is, with its ticket; refused for anyone but a helper.
    pub async fn helper_of(&self, caller: &Caller) -> Result<(Ticket, Helper), BoardError> {
        let refuse =
            || BoardError::refused(Refusal::NotAllowed, crate::policy::ONLY_A_HELPER_HANDS_BACK);
        let Caller::Helper {
            ticket_id, session, ..
        } = caller
        else {
            return Err(refuse());
        };
        let helper = store::of_session(self.inner.board.store().pool(), session)
            .await?
            .ok_or_else(refuse)?;
        let ticket = self.inner.board.ticket_by_id(ticket_id).await?;
        Ok((ticket, helper))
    }

    /// The ticket's helpers, by piece (T-0538 and the sheet build on it).
    pub async fn of_ticket(&self, ticket_id: &str) -> Result<Vec<Helper>, BoardError> {
        Ok(store::of_ticket(self.inner.board.store().pool(), ticket_id).await?)
    }

    /// The ticket's running helper sessions (T-0538: Pause, Park and drag back stop them all).
    pub fn running_sessions(&self, ticket_id: &str) -> Vec<crate::session::SessionInfo> {
        self.inner
            .sessions
            .running_all_of(ticket_id, SessionKind::Helper)
    }

    /// Stops every running helper session of the ticket; their rows and worktrees stay.
    pub async fn stop_all(&self, ticket_id: &str) -> Result<(), BoardError> {
        for session in self.running_sessions(ticket_id) {
            self.inner
                .sessions
                .stop(&session.id)
                .await
                .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        }
        Ok(())
    }

    async fn piece(&self, ticket: &Ticket, piece: u32) -> Result<Helper, BoardError> {
        store::of_piece(self.inner.board.store().pool(), &ticket.id, piece)
            .await?
            .ok_or_else(|| no_helper(&ticket.key, piece))
    }

    /// `otaq helper list`: the ticket's helpers, by piece.
    pub async fn list(&self, caller: &Caller, key: &str) -> Result<Vec<Helper>, BoardError> {
        let (ticket, _) = self.in_charge(caller, key).await?;
        self.of_ticket(&ticket.id).await
    }

    /// `otaq helper diff <n>`: the helper's branch against the ticket's, since the helper's branch left it.
    pub async fn diff(
        &self,
        caller: &Caller,
        key: &str,
        piece: u32,
    ) -> Result<HelperDiff, BoardError> {
        let (ticket, _) = self.in_charge(caller, key).await?;
        let helper = self.piece(&ticket, piece).await?;
        let against = ticket_branch(&ticket)?;
        let project = self
            .inner
            .board
            .project(ticket.key.project.as_str())
            .await?;
        let repo = PathBuf::from(&project.repo_path);
        let git = self.inner.board.git()?;
        let spec = crate::git::diff::DiffSpec::since(&against, &helper.branch);
        let files = git.changed_files(&repo, &spec).await?;
        let range = format!("{against}...{}", helper.branch);
        let patch = git
            .run(&repo, &["diff", "--no-color", "--no-ext-diff", &range])
            .await?;
        Ok(HelperDiff {
            helper,
            against,
            files,
            patch,
        })
    }

    /// `otaq helper back <n> --stdin`: the note is typed into the helper's session when it is idle (d0048), never into
    /// a running turn; status `sent back`. Also posted as a comment "To 40.3: …".
    pub async fn back(
        &self,
        caller: &Caller,
        key: &str,
        piece: u32,
        note: &str,
    ) -> Result<HelperReply, BoardError> {
        let note = crate::board::ticket_comments::check_body(note)?;
        let (ticket, in_charge) = self.in_charge(caller, key).await?;
        let helper = self.piece(&ticket, piece).await?;
        let label = helper.label();
        let live = helper
            .session
            .as_deref()
            .and_then(|s| self.inner.sessions.info(s).ok())
            .filter(|info| info.is_running());
        let Some(live) = live else {
            return Err(BoardError::refused(
                Refusal::SessionEnded,
                format!(
                    "{label}'s helper isn't running: start it again with `{} helper start {piece}`",
                    naming::COMMAND_NAME
                ),
            ));
        };
        if helper.status == HelperStatus::Merged {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!("{label} is merged already"),
            ));
        }
        self.note_comment(&ticket, &in_charge.id, &label, &note)
            .await?;
        self.inner
            .deliveries
            .queue_delivery(&live.id, DeliveryKind::AgentMessage, &note)
            .await
            .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        let pool = self.inner.board.store().pool();
        store::set_status(pool, &helper.id, HelperStatus::SentBack, Some(&note), None).await?;
        self.record(&ticket, &helper, "sent_back", Some(&note))
            .await?;
        let helper = self.piece(&ticket, piece).await?;
        Ok(HelperReply {
            text: format!(
                "Sent {label} back: your note is typed into its session as soon as it is idle."
            ),
            helper,
        })
    }

    /// `otaq helper done --stdin` from a helper: its piece goes back to the agent in charge (status `being reviewed`),
    /// its text posted as its comment; refused while its worktree has uncommitted files, as `stage next` is.
    pub async fn done(&self, caller: &Caller, summary: &str) -> Result<HelperReply, BoardError> {
        let summary = crate::board::ticket_comments::check_body(summary)?;
        let (ticket, helper) = self.helper_of(caller).await?;
        let git = self.inner.board.git()?;
        let worktree = Path::new(&helper.worktree);
        if worktree.is_dir() {
            let uncommitted = git.uncommitted_files(worktree).await?;
            if !uncommitted.is_empty() {
                return Err(BoardError::refused(
                    Refusal::Uncommitted,
                    crate::board::review::uncommitted_message(uncommitted.len()),
                ));
            }
        }
        let session = helper.session.clone().unwrap_or_default();
        let author = self.helper_author(&session, &helper).await?;
        self.inner
            .board
            .post_ticket_comment(
                &ticket.key.to_string(),
                &author,
                &summary,
                None,
                &Actor::Agent(ticket.key.clone()),
            )
            .await?;
        let pool = self.inner.board.store().pool();
        store::set_status(pool, &helper.id, HelperStatus::BeingReviewed, None, None).await?;
        self.record(&ticket, &helper, "handed_back", Some(&summary))
            .await?;
        let label = helper.label();
        // The agent in charge hears it as it hears any note: typed when it is idle (d0048).
        if let Some(in_charge) = self
            .inner
            .sessions
            .running_of(&ticket.id, SessionKind::Agent)
            && let Err(e) = self
                .inner
                .deliveries
                .queue_delivery(
                    &in_charge.id,
                    DeliveryKind::AgentMessage,
                    &handed_back_text(&label, helper.piece, &summary),
                )
                .await
        {
            tracing::warn!(helper = helper.id, error = %e, "can't tell the agent in charge");
        }
        let helper = self.piece(&ticket, helper.piece).await?;
        Ok(HelperReply {
            text: format!(
                "Handed {label} back to the agent in charge. End your turn: it reviews your work, and may send it back with a note."
            ),
            helper,
        })
    }

    /// A helper's comment author: its session's agent, named with its piece ("Developer · 40.3").
    async fn helper_author(
        &self,
        session: &str,
        helper: &Helper,
    ) -> Result<crate::board::ticket_comments::CommentAuthor, BoardError> {
        use crate::board::ticket_comments::CommentAuthor;
        let author = self.inner.board.session_author(session).await;
        let label = helper.label();
        Ok(match author {
            Ok(CommentAuthor::Agent {
                id,
                name,
                tool,
                stage,
                round,
            }) => CommentAuthor::Agent {
                id,
                name: Some(match name {
                    Some(name) => format!("{name} \u{b7} {label}"),
                    None => label,
                }),
                tool,
                stage,
                round,
            },
            _ => CommentAuthor::Agent {
                id: None,
                name: Some(label),
                tool: None,
                stage: None,
                round: None,
            },
        })
    }

    /// The agent in charge's note to a helper, as its comment: "To 40.3: …".
    async fn note_comment(
        &self,
        ticket: &Ticket,
        in_charge: &str,
        label: &str,
        note: &str,
    ) -> Result<(), BoardError> {
        let author = self.inner.board.session_author(in_charge).await?;
        self.inner
            .board
            .post_ticket_comment(
                &ticket.key.to_string(),
                &author,
                &format!("To {label}: {note}"),
                None,
                &Actor::Agent(ticket.key.clone()),
            )
            .await?;
        Ok(())
    }

    /// Records a helper's change in the ticket's History (`what`: started, held, sent_back, handed_back, merged) and
    /// tells clients.
    async fn record(
        &self,
        ticket: &Ticket,
        helper: &Helper,
        what: &str,
        note: Option<&str>,
    ) -> Result<(), BoardError> {
        let mut tx = self.inner.board.store().begin_write().await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::Helper,
            &Actor::Agent(ticket.key.clone()),
            json!({
                "piece": helper.piece,
                "label": helper.label(),
                "title": helper.title,
                "what": what,
                "note": note.map(first_line),
            }),
        )
        .await?;
        tx.commit().await?;
        self.changed(ticket);
        Ok(())
    }

    fn changed(&self, ticket: &Ticket) {
        self.inner.events.publish(Event::TicketChanged {
            project: ticket.key.project.to_string(),
            id: ticket.id.clone(),
            key: ticket.key.clone(),
        });
    }

    /// The ticket reached Done (Merged or Closed, d0018): every helper session ends, each helper worktree left is
    /// removed when clean (one with uncommitted files stays, listed in Cleanup under its ticket), the helpers' branches
    /// go and their ports return to the pool.
    pub async fn finished(&self, ticket_id: &str) -> Result<(), BoardError> {
        self.stop_all(ticket_id).await?;
        let helpers = self.of_ticket(ticket_id).await?;
        if helpers.is_empty() {
            return Ok(());
        }
        let ticket = self.inner.board.ticket_by_id(ticket_id).await?;
        let project = self
            .inner
            .board
            .project(ticket.key.project.as_str())
            .await?;
        let repo = PathBuf::from(&project.repo_path);
        let git = self.inner.board.project_git(&project.key).await?;
        for helper in &helpers {
            let worktree = Path::new(&helper.worktree);
            let kept = if worktree.is_dir() {
                match git.uncommitted_files(worktree).await {
                    Ok(dirty) if dirty.is_empty() => {
                        if let Err(e) =
                            crate::review::finish::remove_worktree(&git, &repo, worktree).await
                        {
                            tracing::warn!(helper = helper.id, error = %e, "can't remove a helper's worktree");
                        }
                        worktree.is_dir()
                    }
                    _ => true,
                }
            } else {
                false
            };
            // A branch whose worktree stays stays with it: Cleanup lists both.
            if !kept
                && let Err(e) = git
                    .run(&repo, &["branch", "--quiet", "-D", &helper.branch])
                    .await
            {
                tracing::debug!(branch = helper.branch, error = %e, "a helper's branch wasn't deleted");
            }
        }
        store::free_ports(self.inner.board.store().pool(), ticket_id).await?;
        Ok(())
    }
}

/// A note's first line, for History.
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// The ticket's branch, where its helpers start from and merge into.
fn ticket_branch(ticket: &Ticket) -> Result<String, BoardError> {
    ticket
        .work
        .as_ref()
        .and_then(|w| w.branch.clone())
        .ok_or_else(|| {
            BoardError::refused(
                Refusal::NoBranch,
                format!("{} has no branch yet", ticket.key),
            )
        })
}

/// The ticket's worktree, where helpers merge.
fn ticket_worktree(ticket: &Ticket) -> Result<PathBuf, BoardError> {
    ticket
        .work
        .as_ref()
        .and_then(|w| w.worktree.clone())
        .map(PathBuf::from)
        .filter(|w| w.is_dir())
        .ok_or_else(|| {
            BoardError::refused(
                Refusal::NoWorktree,
                format!("{} has no worktree", ticket.key),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let key: TicketKey = "SHOP-40".parse().unwrap();
        assert_eq!(piece_label(&key, 3), "40.3");
        assert_eq!(branch_name(&key, 3), naming::branch("SHOP-40.3"));
        assert_eq!(piece_env_var(), naming::env_var("PIECE"));
        assert_eq!(
            first_prompt(&key, 3, "Wishlist page"),
            format!(
                "You are the helper for piece 3 of SHOP-40, Wishlist page. Work only on this piece in this worktree. \
                 Run `{} prime`.",
                naming::COMMAND_NAME
            )
        );
        assert_eq!(
            only_in_charge(&key),
            "Only the agent in charge of SHOP-40 starts, sends back and merges its helpers"
        );
        assert_eq!(HelperStatus::Held.label(), "waiting for limit reset");
    }
}

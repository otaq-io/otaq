//! How a waiting ticket came to wait (tuq decisions 119, 120; tech spec slot-between-stages, architecture decisions 1
//! and 2): `tickets.wait_kind`, written with `queued_at` by every queue write through [`set_queued!`], and picked by
//! one rule, [`WaitKind::after`], from the status the ticket leaves.
//!
//! [`set_queued!`]: crate::store::tickets::set_queued

use sqlx::Sqlite;
use sqlx::query::Query;
use sqlx::sqlite::SqliteArguments;

use crate::domain::{Status, StatusSet, stored_enum};
use crate::scheduler::HOLDS_A_SLOT;

stored_enum! {
    /// How a ticket waiting before an Agent stage came to wait. Stored on the waiting ticket only (NULL elsewhere,
    /// and a NULL reads as [`WaitKind::Begin`]: how every ticket waited before migration 0129).
    #[derive(Default)]
    pub enum WaitKind ("wait kind") {
        /// An ordinary start: waiting to begin, or after something that held no slot (Backlog, Parked, Interrupted,
        /// Start interrupted…).
        #[default]
        Begin = "begin",
        /// It left an Agent stage or an agent check, which held a slot: it keeps that slot through the hand-over (119).
        Kept = "kept",
        /// It left one of the user's checks (Spec review, Review), or the user sent it back from an agent's ask: it
        /// goes ahead of every ticket waiting to begin (120).
        AfterCheck = "after_check",
    }
}

/// What put the ticket in the wait, besides the status it leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// The usual case: the status the ticket leaves says it all.
    Flow,
    /// The user answered an agent's ask with Send back: it leaves Asks you (a slot holder) yet follows 120, and
    /// nothing in the status tells it apart.
    UsersSendBack,
}

impl WaitKind {
    /// The one rule (architecture decision 2): the user's Send back from an agent's ask and a status of one of the
    /// user's checks are [`Self::AfterCheck`], a status that held a slot ([`HOLDS_A_SLOT`]) is [`Self::Kept`], and
    /// anything else is [`Self::Begin`].
    pub fn after(left: Status, cause: Cause) -> Self {
        if cause == Cause::UsersSendBack {
            return Self::AfterCheck;
        }
        if HOLDS_A_SLOT.contains(&left) {
            return Self::Kept;
        }
        if left.set() == StatusSet::HumanCheck {
            return Self::AfterCheck;
        }
        Self::Begin
    }

    /// What a stored `tickets.wait_kind` reads as: NULL (and anything unknown) is [`Self::Begin`].
    pub fn stored(value: Option<&str>) -> Self {
        value.and_then(|v| v.parse().ok()).unwrap_or_default()
    }

    pub fn is_begin(&self) -> bool {
        *self == Self::Begin
    }
}

/// Binds [`set_queued!`]'s four values in its place in the chain: whether the write queues the ticket, now, and how it
/// came to wait. Not queued, `queued_at` stays and `wait_kind` is cleared (it waits no more).
///
/// [`set_queued!`]: crate::store::tickets::set_queued
pub(crate) trait BindQueued {
    fn bind_queued(self, queues: bool, now: i64, kind: WaitKind) -> Self;
}

impl<'q> BindQueued for Query<'q, Sqlite, SqliteArguments> {
    fn bind_queued(self, queues: bool, now: i64, kind: WaitKind) -> Self {
        self.bind(queues).bind(now).bind(queues).bind(kind.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One row per status set (and the statuses within a set the rule tells apart) and cause.
    #[test]
    fn the_kind_follows_from_the_status_it_leaves() {
        use Cause::{Flow, UsersSendBack};
        use Status as S;
        use WaitKind::{AfterCheck, Begin, Kept};
        let rows: &[(Status, Cause, WaitKind)] = &[
            // Backlog
            (S::Backlog, Flow, Begin),
            (S::Parked, Flow, Begin),
            // Waiting before an Agent stage: a Retry start, a parent closed and reopened
            (S::Queued, Flow, Begin),
            (S::WaitingForSlot, Flow, Begin),
            (S::Held, Flow, Begin),
            (S::ParentClosed, Flow, Begin),
            (S::StartInterrupted, Flow, Begin),
            // An Agent stage: stage next, the user's drag of a working ticket
            (S::Working, Flow, Kept),
            (S::WorkingInBackground, Flow, Kept),
            (S::AsksYou, Flow, Kept),
            (S::NeedsYou, Flow, Kept),
            (S::Idle, Flow, Kept),
            (S::WaitingForLimitReset, Flow, Kept),
            (S::MergeConflictFixing, Flow, Kept),
            (S::DeliveryStopped, Flow, Kept),
            // …but its session gone: Resume or Retry is an ordinary start
            (S::Interrupted, Flow, Begin),
            (S::ResumeFailed, Flow, Begin),
            // An agent check: Pass, Fail, Move on anyway, Another round, Answer the agent
            (S::Checking, Flow, Kept),
            (S::CheckerNeedsYou, Flow, Kept),
            (S::CheckFailed, Flow, Kept),
            (S::CheckNoResult, Flow, Kept),
            // A check by you: Pass, Send back
            (S::WaitingForYou, Flow, AfterCheck),
            // A Delivery: a conflict sent back
            (S::ToDeliver, Flow, Begin),
            (S::PrOpen, Flow, Begin),
            (S::MergedInto, Flow, Begin),
            // Done: reopened
            (S::Merged, Flow, Begin),
            (S::ClosedNotMerged, Flow, Begin),
            // The user's Send back answer to an agent's ask (120), whatever it leaves
            (S::AsksYou, UsersSendBack, AfterCheck),
            (S::Working, UsersSendBack, AfterCheck),
            (S::Interrupted, UsersSendBack, AfterCheck),
        ];
        for &(left, cause, kind) in rows {
            assert_eq!(WaitKind::after(left, cause), kind, "{left:?}, {cause:?}");
        }
        // Every status has its row.
        for status in Status::ALL {
            assert!(
                rows.iter().any(|(s, c, _)| s == status && *c == Flow),
                "{status:?} has no row"
            );
        }
    }

    #[test]
    fn null_reads_as_begin() {
        assert_eq!(WaitKind::stored(None), WaitKind::Begin);
        assert_eq!(WaitKind::stored(Some("kept")), WaitKind::Kept);
        assert_eq!(WaitKind::stored(Some("after_check")), WaitKind::AfterCheck);
    }

    /// Every `sqlx::query…` statement in `src/`, from `sqlx::query` to its `.execute(`/`.fetch…(`, with its file.
    fn statements() -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&src, &mut files);
        let mut found = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            // Comments say `queued_at = …` too: only code counts.
            let code: String = text
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            let name = file.strip_prefix(&src).unwrap().display().to_string();
            let mut rest = code.as_str();
            while let Some(at) = rest.find("sqlx::query") {
                let tail = &rest[at..];
                let end = ["\n.execute(", ".execute(", ".fetch"]
                    .iter()
                    .filter_map(|m| tail.find(m))
                    .min()
                    .unwrap_or(tail.len());
                found.push((name.clone(), tail[..end].to_owned()));
                rest = &tail[end.max(1)..];
            }
        }
        found
    }

    /// Risk "a missed queue write" (tech spec slot-between-stages): every UPDATE that queues a ticket (sets
    /// `queued_at`, or writes Queued as its state) goes through `set_queued!()`, so it writes `wait_kind` with it;
    /// the one that takes a ticket out of the wait (`queued_at = NULL`) clears `wait_kind` too.
    #[test]
    fn every_queue_write_uses_set_queued() {
        let mut uses = 0;
        let mut bad = Vec::new();
        for (file, stmt) in statements() {
            if !stmt.contains("UPDATE tickets") {
                continue;
            }
            let macro_used = stmt.contains("set_queued!()");
            uses += usize::from(macro_used);
            let sets_queued_at = stmt.match_indices("queued_at =").any(|(i, _)| {
                !stmt[i + "queued_at =".len()..]
                    .trim_start()
                    .starts_with("NULL")
            });
            let clears = stmt.contains("queued_at = NULL");
            // Queued as the new state: bound (with no `state` placeholder after WHERE to bind it for) or written.
            let (set, filter) = stmt.split_once("WHERE").unwrap_or((&stmt, ""));
            let writes_queued = set.contains("state = 'queued'")
                || (stmt.contains(".bind(Status::Queued.as_str())") && !filter.contains("state"));
            if (sets_queued_at || writes_queued) && !macro_used {
                bad.push(format!("{file}: queues without set_queued!():\n{stmt}"));
            }
            if clears && !stmt.contains("wait_kind = NULL") {
                bad.push(format!(
                    "{file}: clears queued_at but not wait_kind:\n{stmt}"
                ));
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n\n"));
        // The scan finds the queue writes the tech spec names: moves (2), human_check, review, send_back,
        // validation (2) and review/resume.
        assert!(
            uses >= 8,
            "only {uses} statements use set_queued!(): the scan missed some"
        );
    }
}

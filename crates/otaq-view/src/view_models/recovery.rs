//! Interrupted, "Before the restart" and Resume failed in the sheet (REC-02, REC-03; d0045, d0018; *06 → Interrupted ·
//! Before the restart; 16 → Resume failed · Start from scratch*).
//!
//! An Interrupted ticket's Terminal tab can't replay its grid (the session is gone), so it shows the agent's last
//! messages and actions read from the agent tool's saved conversation, with a Resume call to action; when the
//! conversation is gone it says so and offers Retry and Start from scratch. Start from scratch asks first and says
//! what is kept and what is lost.

use otaq_core::agent_tools::Tool;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::recovery::{BeforeRestart, TranscriptEntry};
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::clock::local_hhmm;

/// The first 8 characters of a tool session id ("3be94c10"), as the frames show it.
pub fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

/// One line of the view: time, who or what, text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryLine {
    pub time: String,
    /// "You", the agent tool's short name ("Claude") or the action's tool ("Bash", "Edit").
    pub who: String,
    pub text: String,
    /// A tool action (drawn in the accent colour).
    pub action: bool,
}

/// What the Terminal tab of an Interrupted (or Resume failed) ticket shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryView {
    /// The last messages and actions, the "Session ended…" line and Resume.
    BeforeRestart {
        session: String,
        lines: Vec<EntryLine>,
        ended: String,
    },
    /// "Conversation no longer available", with Retry and Start from scratch.
    NotAvailable { text: String },
}

/// Whether the ticket's Terminal tab shows the recovery view instead of a live grid.
pub fn shows_recovery(ticket: &Ticket, session_running: bool) -> bool {
    matches!(ticket.status, Status::Interrupted | Status::ResumeFailed) && !session_running
}

pub fn view(ticket: &Ticket, before: &BeforeRestart) -> RecoveryView {
    match before {
        BeforeRestart::Available {
            tool,
            tool_session_id,
            entries,
        } => {
            let short = short_id(tool_session_id).to_owned();
            let hint = tool.resume_hint(&short);
            let at = ticket.status_since.map(local_hhmm);
            let ended = match at {
                Some(at) => format!(
                    "Session ended by a restart at {at}. Resume reopens this conversation ({hint})."
                ),
                None => format!("The session ended. Resume reopens this conversation ({hint})."),
            };
            RecoveryView::BeforeRestart {
                session: format!("session {short}"),
                lines: entries.iter().map(|e| entry_line(e, *tool)).collect(),
                ended,
            }
        }
        BeforeRestart::NotAvailable {
            tool,
            tool_session_id,
        } => RecoveryView::NotAvailable {
            text: match tool.zip(tool_session_id.as_deref()) {
                Some((tool, id)) => format!(
                    "{}’s saved conversation for session {} can’t be found, so there is nothing to show or \
                     resume. The worktree and branch are untouched.",
                    tool.short_name(),
                    short_id(id)
                ),
                None => "There is no saved conversation for this ticket, so there is nothing to show or resume. \
                         The worktree and branch are untouched."
                    .to_owned(),
            },
        },
    }
}

/// One entry of a conversation that ran on `tool`.
pub fn entry_line(entry: &TranscriptEntry, tool: Tool) -> EntryLine {
    let time = |at: &Option<i64>| at.map(local_hhmm).unwrap_or_default();
    match entry {
        TranscriptEntry::User { text, at } => EntryLine {
            time: time(at),
            who: "You".into(),
            text: text.clone(),
            action: false,
        },
        TranscriptEntry::Assistant { text, at } => EntryLine {
            time: time(at),
            who: tool.short_name().into(),
            text: text.clone(),
            action: false,
        },
        TranscriptEntry::Action { tool, summary, at } => EntryLine {
            time: time(at),
            who: tool.clone(),
            text: summary.clone(),
            action: true,
        },
    }
}

/// The Start from scratch confirmation (*16 → Resume failed · Start from scratch*). `ScratchConfirm` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ScratchConfirm {
    pub title: String,
    pub text: &'static str,
    pub kept_title: &'static str,
    pub kept: Vec<String>,
    pub lost_title: &'static str,
    pub lost: &'static str,
    pub note: &'static str,
    pub confirm: &'static str,
}

pub fn scratch_confirm(ticket: &Ticket) -> ScratchConfirm {
    let branch = ticket
        .work
        .as_ref()
        .and_then(|w| w.branch.clone())
        .map_or_else(
            || "The branch and its commits".to_owned(),
            |b| format!("Branch {b} and its commits"),
        );
    ScratchConfirm {
        title: format!("Start {} {} from scratch?", ticket.key, ticket.title),
        text: "The previous conversation can’t be restored. A new agent session starts in the same worktree and \
               reads the ticket again.",
        kept_title: "Kept",
        kept: vec![
            branch,
            "Uncommitted files in the worktree".into(),
            "The ticket and its dependencies".into(),
        ],
        lost_title: "Lost",
        lost: "The previous conversation: what the agent had read, planned and decided",
        note: "Activity records this as “Started from scratch (resume failed)”.",
        confirm: "Start from scratch",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures;

    fn ticket(status: Status) -> Ticket {
        let mut t = card_fixtures::ticket("BILL-4", "Invoice PDF export", status);
        t.work = Some(otaq_core::board::tickets::Work {
            branch: Some(otaq_core::naming::branch("BILL-4")),
            ..Default::default()
        });
        t
    }

    #[test]
    fn before_the_restart_lists_messages_and_actions() {
        let mut t = ticket(Status::Interrupted);
        t.status_since = Some(1_791_582_000_000);
        let before = BeforeRestart::Available {
            tool: Tool::Claude,
            tool_session_id: "3be94c10-aaaa-7bbb-8ccc-ddddeeeeffff".into(),
            entries: vec![
                TranscriptEntry::User {
                    text: "Upgrade to Next.js 15".into(),
                    at: Some(1_791_581_000_000),
                },
                TranscriptEntry::Action {
                    tool: "Bash".into(),
                    summary: "pnpm build".into(),
                    at: None,
                },
                TranscriptEntry::Assistant {
                    text: "Two pages still read params synchronously.".into(),
                    at: None,
                },
            ],
        };
        let RecoveryView::BeforeRestart {
            session,
            lines,
            ended,
        } = view(&t, &before)
        else {
            panic!("available")
        };
        assert_eq!(session, "session 3be94c10");
        let who: Vec<(&str, bool)> = lines.iter().map(|l| (l.who.as_str(), l.action)).collect();
        assert_eq!(who, [("You", false), ("Bash", true), ("Claude", false)]);
        assert_eq!(lines[0].time.len(), 5);
        assert!(ended.starts_with("Session ended by a restart at "));
        assert!(ended.ends_with("Resume reopens this conversation (claude --resume 3be94c10)."));
    }

    #[test]
    fn a_gone_conversation_says_so() {
        let t = ticket(Status::Interrupted);
        let view = view(
            &t,
            &BeforeRestart::NotAvailable {
                tool: Some(Tool::Claude),
                tool_session_id: Some("3be94c10-aaaa".into()),
            },
        );
        assert_eq!(
            view,
            RecoveryView::NotAvailable {
                text: "Claude’s saved conversation for session 3be94c10 can’t be found, so there is nothing to \
                       show or resume. The worktree and branch are untouched."
                    .into()
            }
        );
    }

    #[test]
    fn the_recovery_view_replaces_an_ended_grid() {
        assert!(shows_recovery(&ticket(Status::Interrupted), false));
        assert!(shows_recovery(&ticket(Status::ResumeFailed), false));
        assert!(!shows_recovery(&ticket(Status::Interrupted), true));
        assert!(!shows_recovery(&ticket(Status::Working), false));
    }

    #[test]
    fn start_from_scratch_says_what_is_kept() {
        let c = scratch_confirm(&ticket(Status::ResumeFailed));
        assert_eq!(c.title, "Start BILL-4 Invoice PDF export from scratch?");
        assert_eq!(
            c.kept[0],
            format!(
                "Branch {} and its commits",
                otaq_core::naming::branch("BILL-4")
            )
        );
        assert!(c.note.contains("Started from scratch (resume failed)"));
    }

    /// "Before the restart" and Resume failed per tool (REC-02, REC-03, d0086): the tool's own name and resume
    /// command; each tool's action names as its adapter gives them (OpenCode's "shell", Claude Code's "Bash").
    #[test]
    fn recovery_texts_per_tool() {
        let t = ticket(Status::Interrupted);
        for &tool in &Tool::ALL[1..] {
            let before = BeforeRestart::Available {
                tool,
                tool_session_id: "01a147a1-0000-7000-8000-000000000000".into(),
                entries: vec![
                    TranscriptEntry::Assistant {
                        text: "Done.".into(),
                        at: None,
                    },
                    TranscriptEntry::Action {
                        tool: "shell".into(),
                        summary: "npm test".into(),
                        at: None,
                    },
                ],
            };
            let RecoveryView::BeforeRestart { lines, ended, .. } = view(&t, &before) else {
                panic!("available")
            };
            assert_eq!(lines[0].who, tool.short_name());
            assert_eq!((lines[1].who.as_str(), lines[1].action), ("shell", true));
            assert!(
                ended.ends_with(&format!("({}).", tool.resume_hint("01a147a1"))),
                "{ended}"
            );
            let gone = view(
                &t,
                &BeforeRestart::NotAvailable {
                    tool: Some(tool),
                    tool_session_id: Some("01a147a1-0000".into()),
                },
            );
            let RecoveryView::NotAvailable { text } = gone else {
                panic!("not available")
            };
            assert!(
                text.starts_with(&format!(
                    "{}\u{2019}s saved conversation for session 01a147a1",
                    tool.short_name()
                )),
                "{text}"
            );
        }
    }
}

//! Line comments in the Files tab (REV-03; d0017): where a comment anchors, which comments make a thread, what a
//! comment's header says, and the pending counter shown on the Files tab and next to Send back. There is no
//! partial approval: nothing here approves a file or a line.

use otaq_core::board::comments::{ReviewComment, Side};
use otaq_core::git::diff::{DiffLine, LineKind};

/// Where a comment sits: a file, a line number and the side it is numbered on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub file: String,
    pub line: u32,
    pub side: Side,
}

impl Anchor {
    /// A diff line's anchor: a removed line by its old number, any other line by its new one.
    pub fn of(file: &str, line: &DiffLine) -> Option<Self> {
        let (number, side) = match line.kind {
            LineKind::Removed => (line.old?, Side::Old),
            LineKind::Added | LineKind::Context => (line.new?, Side::New),
        };
        Some(Self {
            file: file.to_owned(),
            line: number,
            side,
        })
    }

    pub fn holds(&self, comment: &ReviewComment) -> bool {
        comment.file == self.file && comment.line == self.line && comment.side == self.side
    }
}

/// The open comment box: a new comment (or a reply) at an anchor, or the edit of a pending one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composer {
    pub anchor: Anchor,
    /// The pending comment being edited.
    pub editing: Option<String>,
}

/// The comments on one anchor, oldest first.
pub fn thread<'a>(comments: &'a [ReviewComment], anchor: &Anchor) -> Vec<&'a ReviewComment> {
    comments.iter().filter(|c| anchor.holds(c)).collect()
}

/// How many comments Send back would send.
pub fn pending(comments: &[ReviewComment]) -> usize {
    comments.iter().filter(|c| c.is_pending()).count()
}

/// "2 pending comments".
pub fn pending_label(count: usize) -> String {
    match count {
        1 => "1 pending comment".to_owned(),
        n => format!("{n} pending comments"),
    }
}

/// The Send back button where its stage isn't known: "Send back (2)…" with pending comments; a human check's names
/// its stage ([`crate::view_models::send_back::button_label`], d0093).
pub fn send_back_label(count: usize) -> String {
    match count {
        0 => crate::view_models::sheet_actions::SEND_BACK.to_owned(),
        n => format!("Send back ({n})\u{2026}"),
    }
}

/// The badge after the author: "pending" (editable), or the round it was sent in (read-only).
pub fn badge(comment: &ReviewComment) -> String {
    if comment.is_pending() {
        "pending".to_owned()
    } else {
        format!("sent · round {}", comment.round)
    }
}

/// Only pending comments can be edited or deleted.
pub fn editable(comment: &ReviewComment) -> bool {
    comment.is_pending()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(id: &str, line: u32, side: Side, sent: bool) -> ReviewComment {
        ReviewComment {
            id: id.into(),
            round: 1,
            file: "lib/a.ts".into(),
            line,
            side,
            body: "b".into(),
            created_at: 0,
            sent_at: sent.then_some(5),
        }
    }

    fn diff_line(kind: LineKind, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine {
            kind,
            old,
            new,
            text: String::new(),
            no_newline: false,
        }
    }

    #[test]
    fn removed_lines_anchor_on_the_old_side() {
        let a = Anchor::of("f", &diff_line(LineKind::Removed, Some(13), None)).unwrap();
        assert_eq!((a.line, a.side), (13, Side::Old));
        let a = Anchor::of("f", &diff_line(LineKind::Added, None, Some(14))).unwrap();
        assert_eq!((a.line, a.side), (14, Side::New));
        let a = Anchor::of("f", &diff_line(LineKind::Context, Some(10), Some(12))).unwrap();
        assert_eq!((a.line, a.side), (12, Side::New));
    }

    #[test]
    fn threads_counters_and_badges() {
        let comments = vec![
            comment("1", 12, Side::New, true),
            comment("2", 12, Side::New, false),
            comment("3", 12, Side::Old, false),
            comment("4", 9, Side::New, false),
        ];
        let anchor = Anchor {
            file: "lib/a.ts".into(),
            line: 12,
            side: Side::New,
        };
        let ids: Vec<_> = thread(&comments, &anchor)
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(ids, ["1", "2"]);
        assert_eq!(pending(&comments), 3);
        assert_eq!(pending_label(3), "3 pending comments");
        assert_eq!(pending_label(1), "1 pending comment");
        assert_eq!(send_back_label(2), "Send back (2)\u{2026}");
        assert_eq!(send_back_label(0), "Send back\u{2026}");
        assert_eq!(badge(&comments[0]), "sent · round 1");
        assert_eq!(badge(&comments[1]), "pending");
        assert!(!editable(&comments[0]));
        assert!(editable(&comments[1]));
    }
}

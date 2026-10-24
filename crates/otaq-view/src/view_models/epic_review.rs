//! An epic as a delivery unit in the sheet (EPC-03…EPC-05; d0059; *05 → Epic panel · SHOP-40 Wishlist, Epic in
//! Review · Diff by child, Approve ▾ on an epic*): External ID, progress, its local branch, the child list, the
//! note about Send back, and the Files tab's grouping by child.

use otaq_core::board::diff::{ChildDiff, DiffSummary};
use otaq_core::board::epics::progress_of;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Status, TicketKey};

use crate::view_models::diff::counts;

/// One child in the epic sheet's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildRow {
    pub key: TicketKey,
    pub title: String,
    /// "⛓ SHOP-40.1" when it depends on siblings.
    pub deps: Option<String>,
    pub status: Status,
}

/// What the sheet shows above an epic's tabs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpicHeader {
    pub external_id: Option<String>,
    /// "1 / 3 merged".
    pub progress: String,
    /// "otaq/SHOP-40 · local".
    pub branch: String,
    pub children: Vec<ChildRow>,
    /// "· new tickets get the next key, SHOP-40.4".
    pub next_key: Option<String>,
    pub note: String,
}

pub fn header(epic: &Ticket, tickets: &[Ticket]) -> EpicHeader {
    let branch = epic
        .work
        .as_ref()
        .and_then(|w| w.branch.clone())
        .unwrap_or_else(|| otaq_core::naming::branch(&epic.key.to_string()));
    let mut children: Vec<ChildRow> = tickets
        .iter()
        .filter(|t| t.epic.as_ref() == Some(&epic.key))
        .map(|t| ChildRow {
            key: t.key.clone(),
            title: t.title.clone(),
            deps: (!t.depends_on.is_empty()).then(|| {
                format!(
                    "\u{26d3} {}",
                    t.depends_on
                        .iter()
                        .map(|k| k.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            }),
            status: t.status,
        })
        .collect();
    children.sort_by(|a, b| a.key.cmp(&b.key));
    let note = if epic.status == Status::WaitingForYou {
        "No Send back on an epic. Add or reopen a child ticket to change it.".to_owned()
    } else {
        format!(
            "When every child is merged into {branch} (or closed, with at least one merged), the epic moves to \
             Ready for Review: you review the whole feature and approve it once."
        )
    };
    EpicHeader {
        external_id: epic.external_id.clone(),
        progress: progress_of(&epic.key, tickets).label(),
        branch: format!("{branch} \u{b7} local"),
        next_key: epic
            .next_child_number
            .map(|n| format!("\u{b7} new tickets get the next key, {}.{n}", epic.key)),
        children,
        note,
    }
}

/// The epic's diff line: "otaq/SHOP-40 vs main at 7e4f9a0 · 3 children · 21 files".
pub fn diff_label(summary: &DiffSummary) -> String {
    let children = match summary.children.len() {
        1 => "1 child".to_owned(),
        n => format!("{n} children"),
    };
    let files = match summary.files.len() {
        1 => "1 file".to_owned(),
        n => format!("{n} files"),
    };
    format!(
        "{} vs {} at {} \u{b7} {children} \u{b7} {files}",
        summary.branch, summary.base_branch, summary.base_short
    )
}

/// A child group's counts: "+210 −4 · 6 files".
pub fn child_label(child: &ChildDiff) -> String {
    let files = match child.files.len() {
        1 => "1 file".to_owned(),
        n => format!("{n} files"),
    };
    format!("{} \u{b7} {files}", counts(child.added, child.removed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::board::tickets::Work;
    use otaq_core::domain::Kind;

    fn sample() -> (Ticket, Vec<Ticket>) {
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = Kind::Epic;
        epic.external_id = Some("PAY-123".into());
        epic.next_child_number = Some(4);
        epic.work = Some(Work {
            branch: Some(otaq_core::naming::branch("SHOP-40")),
            ..Default::default()
        });
        let mut api = ticket("SHOP-40.1", "Wishlist API", Status::Merged);
        api.epic = Some(epic.key.clone());
        let mut button = ticket("SHOP-40.2", "Wishlist button", Status::Working);
        button.epic = Some(epic.key.clone());
        button.depends_on = vec![api.key.clone()];
        let mut page = ticket("SHOP-40.3", "Wishlist page", Status::Backlog);
        page.epic = Some(epic.key.clone());
        page.depends_on = vec![button.key.clone()];
        (epic.clone(), vec![epic, page, api, button])
    }

    #[test]
    fn the_epic_panel_reads_like_the_frame() {
        let (epic, tickets) = sample();
        let h = header(&epic, &tickets);
        assert_eq!(h.external_id.as_deref(), Some("PAY-123"));
        assert_eq!(h.progress, "1 of 3 merged");
        assert_eq!(
            h.branch,
            format!("{} \u{b7} local", otaq_core::naming::branch("SHOP-40"))
        );
        let keys: Vec<_> = h.children.iter().map(|c| c.key.to_string()).collect();
        assert_eq!(keys, ["SHOP-40.1", "SHOP-40.2", "SHOP-40.3"]);
        assert_eq!(h.children[1].deps.as_deref(), Some("\u{26d3} SHOP-40.1"));
        assert_eq!(
            h.next_key.as_deref(),
            Some("\u{b7} new tickets get the next key, SHOP-40.4")
        );
        let mut reviewing = epic.clone();
        reviewing.status = Status::WaitingForYou;
        assert_eq!(
            header(&reviewing, &tickets).note,
            "No Send back on an epic. Add or reopen a child ticket to change it."
        );
    }

    #[test]
    fn the_diff_groups_by_child() {
        use otaq_core::git::diff::{ChangedFile, FileStatus};
        let file = |p: &str| ChangedFile {
            status: FileStatus::Added,
            path: p.into(),
            old_path: None,
            added: 1,
            removed: 0,
            binary: false,
            size: None,
            old_size: None,
            similarity: None,
        };
        let child = ChildDiff {
            key: "SHOP-40.1".parse().unwrap(),
            title: "Wishlist API".into(),
            commit: "c".into(),
            files: vec![file("a"), file("b")],
            added: 210,
            removed: 4,
        };
        assert_eq!(child_label(&child), "+210 \u{2212}4 \u{b7} 2 files");
        let summary = DiffSummary {
            key: "SHOP-40".parse().unwrap(),
            base_branch: "main".into(),
            base_sha: "7e4f9a0aaa".into(),
            base_short: "7e4f9a0".into(),
            branch: "epic-branch".into(),
            files: vec![file("a"), file("b")],
            added: 1,
            removed: 0,
            round: 1,
            review_summary: None,
            children: vec![child],
        };
        assert_eq!(
            diff_label(&summary),
            "epic-branch vs main at 7e4f9a0 \u{b7} 1 child \u{b7} 2 files"
        );
    }
}

//! "Manual checks for you" (d0125): what the agents and checks couldn't check themselves (`--manual-check`, T-0498),
//! shown on the review's Overview above Approve as a checklist the user ticks. Ticking is the user's and is recorded
//! in History; it never blocks: Approve works with items unticked, and the Local merge dialog says "2 manual checks
//! not ticked".

use otaq_core::board::manual_checks::{ManualCheck, ManualChecks};
use otaq_core::domain::Status;
use serde::Serialize;
use ts_rs::TS;

pub const HEADING: &str = "Manual checks for you";

/// One item: "Offline: reload a never-visited detail page with the server stopped", from "Validator · Validate ·
/// round 2"; its tick sends `n`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ManualCheckRow")]
pub struct Row {
    pub n: u32,
    pub text: String,
    pub source: String,
    pub ticked: bool,
}

/// The checklist above Approve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ManualChecksView")]
pub struct ChecklistView {
    /// "Manual checks for you" (the page sets it in capitals, as every section label).
    pub heading: String,
    pub items: Vec<Row>,
}

/// Where the checklist shows: the statuses whose bar has Approve (or Pass), a human check or a click-mode Delivery.
pub fn shown_in(status: Status) -> bool {
    matches!(status, Status::WaitingForYou | Status::ToDeliver)
}

fn row(item: &ManualCheck) -> Row {
    Row {
        n: item.n,
        text: item.text.clone(),
        source: item.source(),
        ticked: item.ticked(),
    }
}

/// The checklist for a ticket with this status, oldest first; none outside the review or with no items.
pub fn checklist(status: Status, checks: &ManualChecks) -> Option<ChecklistView> {
    (shown_in(status) && !checks.items.is_empty()).then(|| ChecklistView {
        heading: HEADING.to_owned(),
        items: checks.items.iter().map(row).collect(),
    })
}

/// The Local merge dialog's line: "2 manual checks not ticked", "1 manual check not ticked"; none when all are.
pub fn not_ticked(unticked: u32) -> Option<String> {
    match unticked {
        0 => None,
        1 => Some("1 manual check not ticked".to_owned()),
        n => Some(format!("{n} manual checks not ticked")),
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use otaq_core::board::manual_checks::{ManualCheck, ManualChecks};
    use otaq_core::board::ticket_comments::CommentAuthor;

    /// DEMO-3's two checks (finding 34): the developer's and the validator's, the second ticked.
    pub fn demo_3() -> ManualChecks {
        let item =
            |n: u32, text: &str, name: &str, stage: &str, round: Option<u32>, ticked: bool| {
                ManualCheck {
                    n,
                    text: text.into(),
                    author: CommentAuthor::Agent {
                        id: Some(name.to_lowercase()),
                        name: Some(name.into()),
                        tool: None,
                        stage: Some(stage.to_lowercase()),
                        round,
                    },
                    stage_name: Some(stage.into()),
                    comment: Some(n),
                    added_at: 0,
                    ticked_at: ticked.then_some(1),
                    ticked_by: ticked.then(|| "me".into()),
                }
            };
        ManualChecks {
            key: "DEMO-3".parse().unwrap(),
            items: vec![
                item(
                    1,
                    "Offline: with the server stopped, reload a page you visited",
                    "Developer",
                    "Dev",
                    None,
                    false,
                ),
                item(
                    2,
                    "Offline: open a habit's detail page you never visited",
                    "Validator",
                    "Validate",
                    Some(2),
                    true,
                ),
            ],
            unticked: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// d0125: the review shows the items above Approve, each with where it came from and whether it is ticked.
    #[test]
    fn the_checklist_at_review() {
        let checks = fixtures::demo_3();
        let view = checklist(Status::WaitingForYou, &checks).expect("at Review");
        assert_eq!(view.heading, "Manual checks for you");
        let rows: Vec<(u32, &str, bool)> = view
            .items
            .iter()
            .map(|r| (r.n, r.source.as_str(), r.ticked))
            .collect();
        assert_eq!(
            rows,
            [
                (1, "Developer \u{b7} Dev", false),
                (2, "Validator \u{b7} Validate \u{b7} round 2", true)
            ]
        );
        assert_eq!(
            view.items[0].text,
            "Offline: with the server stopped, reload a page you visited"
        );
        assert!(
            checklist(Status::ToDeliver, &checks).is_some(),
            "a click-mode Delivery"
        );
        for status in [
            Status::Working,
            Status::Checking,
            Status::PrOpen,
            Status::Merged,
        ] {
            assert_eq!(checklist(status, &checks), None, "{status}");
        }
        let none = ManualChecks {
            items: Vec::new(),
            unticked: 0,
            ..checks
        };
        assert_eq!(checklist(Status::WaitingForYou, &none), None);
    }

    /// The Local merge dialog's count; it never disables Approve, so with all ticked it says nothing.
    #[test]
    fn the_dialogs_count() {
        assert_eq!(not_ticked(0), None);
        assert_eq!(not_ticked(1).as_deref(), Some("1 manual check not ticked"));
        assert_eq!(not_ticked(2).as_deref(), Some("2 manual checks not ticked"));
    }
}

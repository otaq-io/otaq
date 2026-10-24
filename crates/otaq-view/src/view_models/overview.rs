//! The sheet's Overview (SHT-03, RPT-02; d0109, d0078, d0061, d0099; canvas `s25-03` Overview, `s25-04` editing in
//! place, `s25-05` attachments). Top to bottom on the left: what the ticket needs from you (the panel's answer box and
//! action bar, T-0464's), the **description** shown as it reads, the sections not yet moved elsewhere (Epic, Stack,
//! Messages), and the comments; on the right the **Attachments** column.
//!
//! - **The description in place:** hovering it says "Click to edit"; a click turns it into the editor (d0078) with
//!   "Editing · Esc to discard, ⌘S to save" on its label row. Once the text differs from what is saved
//!   ([`DescriptionState::Changed`]) the tab row's right end shows "Description changed · Discard · Save ⌘S"
//!   ([`SaveBar`]); switching tabs, moving to another ticket or closing the sheet then asks first ([`leaving`],
//!   [`LeaveDialog`]). Saving sends only the description ([`crate::view_models::editor::EditorModel::description`]),
//!   and an untouched description is never saved.
//! - **Attachments** ([`attachments`]): every report of the ticket, newest first, each with its size, who added it and
//!   in which stage, and its comment's first line; Add, a drop or a paste opens the comment box with the file as a
//!   chip, since a comment always has its own line (d0109).

use otaq_core::board::manual_checks::ManualChecks;
use otaq_core::board::reports::size_label;
use otaq_core::board::ticket_comments::{AT_CREATION_LABEL, CommentAuthor, TicketComment};
use serde::Serialize;
use ts_rs::TS;

use otaq_core::domain::Status;

use crate::icons::IconName;
use crate::keymap::Platform;
use crate::view_models::manual_checks::{self, ChecklistView};
use crate::view_models::ticket_comments::{Mark, agent_mark};

pub const DESCRIPTION: &str = "Description";
pub const CLICK_TO_EDIT: &str = "Click to edit";
/// The editor's placeholder while the description is empty (`s25-04`).
pub const BLOCKS_HINT: &str = "Type / for blocks";
pub const CHANGED: &str = "Description changed";
pub const DISCARD: &str = "Discard";
pub const SAVE: &str = "Save";
pub const ATTACHMENTS: &str = "Attachments";
pub const ADD: &str = "Add";
pub const DROP: &str = "Drop files here or paste";
/// The column's note (the audit's item 8 wording, `s25-03`).
pub const NOTE: &str = "Markdown reports, yours and the agents\u{2019} (a test run summary, findings, notes), each \
                        with the comment it came with. Agents open reports when they need them; yours are marked from \
                        you and read first.";
/// The system file picker's title for Add.
pub const PICK_TITLE: &str = "Attach a report";

/// The save key: ⌘S on a Mac, Ctrl+S elsewhere.
pub fn save_keys(platform: Platform) -> &'static str {
    match platform {
        Platform::Mac => "\u{2318}S",
        Platform::WinLinux | Platform::WinLinuxTerminal => "Ctrl+S",
    }
}

/// Where the description stands: read, being edited and still as saved, or edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "DescriptionState")]
pub enum DescriptionState {
    Reading,
    /// The editor is open; the text is what is saved.
    Editing,
    /// The text differs from what is saved: the tab row's bar shows, and leaving asks first.
    Changed,
}

/// The state for the saved description `saved` and the editor's text (`None` while it isn't open). A text brought
/// back to what is saved is unchanged again.
pub fn state(saved: &str, draft: Option<&str>) -> DescriptionState {
    match draft {
        None => DescriptionState::Reading,
        Some(text) if text == saved => DescriptionState::Editing,
        Some(_) => DescriptionState::Changed,
    }
}

/// The tab row's "Description changed · Discard · Save ⌘S" (`s25-04`): only while the text differs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DescriptionSaveBar")]
pub struct SaveBar {
    pub changed: String,
    pub discard: String,
    pub save: String,
    pub keys: String,
}

pub fn save_bar(state: DescriptionState, platform: Platform) -> Option<SaveBar> {
    (state == DescriptionState::Changed).then(|| SaveBar {
        changed: CHANGED.to_owned(),
        discard: DISCARD.to_owned(),
        save: SAVE.to_owned(),
        keys: save_keys(platform).to_owned(),
    })
}

/// The label row's words on the right: "Click to edit" on hover while reading, the keys while editing.
pub fn label_hint(state: DescriptionState, platform: Platform) -> String {
    match state {
        DescriptionState::Reading => CLICK_TO_EDIT.to_owned(),
        DescriptionState::Editing | DescriptionState::Changed => format!(
            "Editing \u{b7} Esc to discard, {} to save",
            save_keys(platform)
        ),
    }
}

/// What a move away from Overview does: go, or ask first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leave {
    /// Go; an open editor with nothing changed just closes.
    Go,
    /// The description changed: ask first ([`LeaveDialog`]).
    Ask,
}

/// Leaving Overview (another tab, another ticket, closing the sheet; `stays` when the move keeps it on this ticket's
/// Overview, which never asks).
pub fn leaving(state: DescriptionState, stays: bool) -> Leave {
    match state {
        DescriptionState::Changed if !stays => Leave::Ask,
        _ => Leave::Go,
    }
}

/// The question before leaving a changed description (Q-B: the frames don't draw it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DescriptionLeaveDialog")]
pub struct LeaveDialog {
    pub title: String,
    pub body: String,
    /// Leaves, the changes dropped.
    pub discard: String,
    /// Stays in the editor.
    pub keep: String,
}

pub fn leave_dialog(key: &str) -> LeaveDialog {
    LeaveDialog {
        title: "Discard your description changes?".to_owned(),
        body: format!("{key}\u{2019}s description has changes you haven\u{2019}t saved."),
        discard: "Discard changes".to_owned(),
        keep: "Keep editing".to_owned(),
    }
}

/// One report in the Attachments column (RPT-02): "Report · 3 KB", "CX Rust developer · Dev", its comment's line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AttachmentRow")]
pub struct AttachmentRow {
    /// Its comment's number: what Open sends (the report view, CMT-12).
    pub n: u32,
    pub name: String,
    /// "3 KB".
    pub size: String,
    pub mark: Mark,
    /// "Rust developer · Dev", "You", "You · at creation" (RPT-03), "Wishlist lead".
    pub who: String,
    /// The user's own (RPT-01: marked from you).
    pub mine: bool,
    /// The first line of the comment it came with.
    pub line: String,
}

/// The first non-empty line of a comment, trimmed.
fn first_line(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_owned()
}

fn who(comment: &TicketComment) -> String {
    let author = comment.author_label();
    match (&comment.author, &comment.stage_name) {
        (CommentAuthor::Agent { .. }, Some(stage)) => format!("{author} \u{b7} {stage}"),
        (CommentAuthor::User { .. }, _) if comment.at_creation => {
            format!("{author} \u{b7} {AT_CREATION_LABEL}")
        }
        _ => author,
    }
}

/// Every report of the ticket, newest first (`comments` oldest first, as `ticket.comments` answers).
pub fn attachments(comments: &[TicketComment], avatars: bool) -> Vec<AttachmentRow> {
    comments
        .iter()
        .rev()
        .filter_map(|c| {
            let report = c.report?;
            let mine = matches!(c.author, CommentAuthor::User { .. });
            let mark = match &c.author {
                CommentAuthor::User { .. } => Mark::You,
                CommentAuthor::Agent { tool, .. } | CommentAuthor::Lead { tool, .. } => {
                    agent_mark(*tool, avatars)
                }
            };
            Some(AttachmentRow {
                n: c.n,
                name: c.report_label().unwrap_or_default(),
                size: size_label(report.bytes),
                mark,
                who: who(c),
                mine,
                line: first_line(&c.body),
            })
        })
        .collect()
}

/// "Mark as usage limit" (USG-07, the safety net when limit detection missed; the Inbox's row action until T-0459
/// removed it): one click folds the Idle agents into the usage-limit banner (`usage.mark_limit`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OverviewUsageLimit")]
pub struct UsageLimitButton {
    pub label: String,
    pub icon: IconName,
}

pub const MARK_USAGE_LIMIT: &str = "Mark as usage limit";

/// Only a live Idle agent offers it, as the Inbox did: Asks you, Needs you and an agent interrupted by a restart
/// (status Interrupted) don't.
pub fn usage_limit(status: Status) -> Option<UsageLimitButton> {
    (status == Status::Idle).then(|| UsageLimitButton {
        label: MARK_USAGE_LIMIT.to_owned(),
        icon: IconName::Gauge,
    })
}

/// Overview's words and its attachments: `OverviewView` on the page (`overview.view`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OverviewView")]
pub struct OverviewView {
    pub key: String,
    /// "Description" (the page sets it in capitals, as every section label).
    pub description: String,
    /// The label row's right end while reading (on hover) and while editing.
    pub click_to_edit: String,
    pub editing: String,
    pub placeholder: String,
    /// What the tab row shows once the text differs.
    pub bar: SaveBar,
    pub leave: LeaveDialog,
    /// "Mark as usage limit" in what it needs from you, while its agent is Idle (USG-07).
    pub usage_limit: Option<UsageLimitButton>,
    /// "Manual checks for you" above Approve, while it waits in a human check or a click-mode Delivery (d0125).
    pub manual_checks: Option<ChecklistView>,
    /// "Attachments · 3"; "Attachments" with none.
    pub attachments_heading: String,
    pub attachments: Vec<AttachmentRow>,
    pub add: String,
    pub pick_title: String,
    pub drop: String,
    pub note: String,
}

pub fn view(
    key: &str,
    status: Status,
    comments: &[TicketComment],
    checks: Option<&ManualChecks>,
    avatars: bool,
    platform: Platform,
) -> OverviewView {
    let attachments = attachments(comments, avatars);
    OverviewView {
        key: key.to_owned(),
        description: DESCRIPTION.to_owned(),
        click_to_edit: label_hint(DescriptionState::Reading, platform),
        editing: label_hint(DescriptionState::Editing, platform),
        placeholder: BLOCKS_HINT.to_owned(),
        bar: save_bar(DescriptionState::Changed, platform)
            .expect("a changed description has the bar"),
        leave: leave_dialog(key),
        usage_limit: usage_limit(status),
        manual_checks: checks.and_then(|c| manual_checks::checklist(status, c)),
        attachments_heading: match attachments.len() {
            0 => ATTACHMENTS.to_owned(),
            n => format!("{ATTACHMENTS} \u{b7} {n}"),
        },
        attachments,
        add: ADD.to_owned(),
        pick_title: PICK_TITLE.to_owned(),
        drop: DROP.to_owned(),
        note: NOTE.to_owned(),
    }
}

/// The gallery's Overview: SHOP-22 as in `s25-03` (tool avatars shown).
pub fn gallery() -> OverviewView {
    view(
        "SHOP-22",
        Status::NeedsYou,
        &fixtures::shop_22(),
        None,
        true,
        Platform::Mac,
    )
}

/// SHOP-22's comments in the frames (`s25-03`, `s25-05`): the user's first comment with safari-notes, Griller's
/// repro with its findings, the user's line, and Rust developer's test run. Shared by the tests and the gallery.
pub mod fixtures {
    use otaq_core::agent_tools::Tool;
    use otaq_core::board::reports::ReportInfo;
    use otaq_core::board::ticket_comments::{CommentAuthor, TicketComment};

    const MIN: i64 = 60_000;

    fn agent(name: &str, tool: Tool, stage: &str) -> CommentAuthor {
        CommentAuthor::Agent {
            id: Some(name.to_lowercase()),
            name: Some(name.into()),
            tool: Some(tool),
            stage: Some(stage.to_lowercase()),
            round: None,
        }
    }

    fn comment(
        n: u32,
        author: CommentAuthor,
        stage: Option<&str>,
        body: &str,
        report_kb: Option<u64>,
        minute: i64,
    ) -> TicketComment {
        TicketComment {
            n,
            author,
            stage_name: stage.map(str::to_owned),
            body: body.into(),
            report: report_kb.map(|kb| ReportInfo {
                bytes: kb * 1024,
                lines: kb * 30,
            }),
            posted_at: (14 * 60 + minute) * MIN,
            edited_at: None,
            report_name: None,
            at_creation: false,
        }
    }

    /// `s25-05`'s three reports, each under its file's name; yours posted with the ticket ("at creation").
    pub fn shop_22() -> Vec<TicketComment> {
        let mut comments = shop_22_unnamed();
        for (c, name) in comments.iter_mut().zip([
            Some("safari-notes.md"),
            Some("repro-findings.md"),
            None,
            Some("test-run-login.md"),
        ]) {
            c.report_name = name.map(str::to_owned);
        }
        comments[0].at_creation = true;
        comments
    }

    fn shop_22_unnamed() -> Vec<TicketComment> {
        let you = || CommentAuthor::User { name: "me".into() };
        vec![
            comment(1, you(), None, "Steps I used to reproduce it", Some(1), 0),
            comment(
                2,
                agent("Griller", Tool::Claude, "Grill"),
                Some("Grill"),
                "Repro: 18 of 20 pass on Safari; the cookie is set after the first navigation.",
                Some(2),
                14,
            ),
            comment(
                3,
                you(),
                None,
                "Don\u{2019}t touch the auth middleware\u{2019}s public API.",
                None,
                20,
            ),
            comment(
                4,
                agent("Rust developer", Tool::Codex, "Dev"),
                Some("Dev"),
                "Fails 2 in 20 on Safari: the middleware reads the cookie too early.\nFull run attached.",
                Some(3),
                38,
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `s25-03` / `s25-05`: SHOP-22's Overview, its three reports newest first.
    #[test]
    fn overview_for_shop_22() {
        let v = view(
            "SHOP-22",
            Status::NeedsYou,
            &fixtures::shop_22(),
            None,
            true,
            Platform::Mac,
        );
        assert_eq!(v.description, "Description");
        assert_eq!(v.click_to_edit, "Click to edit");
        assert_eq!(
            v.editing,
            "Editing \u{b7} Esc to discard, \u{2318}S to save"
        );
        assert_eq!(v.placeholder, "Type / for blocks");
        assert_eq!(v.attachments_heading, "Attachments \u{b7} 3");
        assert_eq!(
            (v.add.as_str(), v.drop.as_str()),
            ("Add", "Drop files here or paste")
        );
        assert!(
            v.note
                .ends_with("yours are marked from you and read first.")
        );
        let rows: Vec<(u32, &str, &str, &str, bool)> = v
            .attachments
            .iter()
            .map(|r| {
                (
                    r.n,
                    r.size.as_str(),
                    r.who.as_str(),
                    r.line.as_str(),
                    r.mine,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    4,
                    "3 KB",
                    "Rust developer \u{b7} Dev",
                    "Fails 2 in 20 on Safari: the middleware reads the cookie too early.",
                    false
                ),
                (
                    2,
                    "2 KB",
                    "Griller \u{b7} Grill",
                    "Repro: 18 of 20 pass on Safari; the cookie is set after the first navigation.",
                    false
                ),
                (
                    1,
                    "1 KB",
                    "You \u{b7} at creation",
                    "Steps I used to reproduce it",
                    true
                ),
            ]
        );
        // Agents carry their tool's avatar; yours the person mark.
        let marks: Vec<String> = v
            .attachments
            .iter()
            .map(|r| match &r.mark {
                Mark::Tool { avatar } => avatar.letters.clone(),
                Mark::You => "you".into(),
                Mark::Agent => "agent".into(),
            })
            .collect();
        assert_eq!(marks, ["CX", "CC", "you"]);
        // Each under its file's own name (Q414).
        let names: Vec<&str> = v.attachments.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            ["test-run-login.md", "repro-findings.md", "safari-notes.md"]
        );
        // With one tool connected, agents get the plain mark.
        let plain = attachments(&fixtures::shop_22(), false);
        assert_eq!(plain[0].mark, Mark::Agent);
    }

    /// Pasted text has no name: "Pasted text"; a comment you posted later is plain "You".
    #[test]
    fn a_pasted_report_and_a_later_one_of_yours() {
        let mut comments = fixtures::shop_22();
        comments[0].report_name = None;
        comments[0].at_creation = false;
        let rows = attachments(&comments, true);
        let mine = rows.iter().find(|r| r.n == 1).unwrap();
        assert_eq!(
            (mine.name.as_str(), mine.who.as_str()),
            ("Pasted text", "You")
        );
    }

    #[test]
    fn no_reports_no_count() {
        let mut comments = fixtures::shop_22();
        for c in &mut comments {
            c.report = None;
        }
        let v = view(
            "SHOP-22",
            Status::Idle,
            &comments,
            None,
            true,
            Platform::WinLinux,
        );
        assert_eq!(v.attachments_heading, "Attachments");
        assert!(v.attachments.is_empty());
        assert_eq!(v.bar.keys, "Ctrl+S");
        assert_eq!(v.editing, "Editing \u{b7} Esc to discard, Ctrl+S to save");
    }

    /// USG-07: only a live Idle agent offers "Mark as usage limit", as the Inbox's row did.
    #[test]
    fn mark_as_usage_limit_only_while_idle() {
        let button = usage_limit(Status::Idle).unwrap();
        assert_eq!(
            (button.label.as_str(), button.icon),
            ("Mark as usage limit", IconName::Gauge)
        );
        for status in [
            Status::NeedsYou,
            Status::AsksYou,
            Status::Interrupted,
            Status::Working,
        ] {
            assert_eq!(usage_limit(status), None, "{status:?}");
        }
        let idle = view("SHOP-22", Status::Idle, &[], None, true, Platform::Mac);
        assert_eq!(idle.usage_limit, Some(button));
        assert_eq!(gallery().usage_limit, None);
    }

    /// d0125: "Manual checks for you" on the review's Overview (above Approve), not while the agent works.
    #[test]
    fn manual_checks_on_the_review_overview() {
        let checks = crate::view_models::manual_checks::fixtures::demo_3();
        let review = view(
            "DEMO-3",
            Status::WaitingForYou,
            &[],
            Some(&checks),
            true,
            Platform::Mac,
        );
        let list = review.manual_checks.expect("at Review");
        assert_eq!(list.heading, "Manual checks for you");
        assert_eq!(list.items.len(), 2);
        let working = view(
            "DEMO-3",
            Status::Working,
            &[],
            Some(&checks),
            true,
            Platform::Mac,
        );
        assert_eq!(working.manual_checks, None);
        assert_eq!(gallery().manual_checks, None);
    }

    /// `s25-04`: reading, editing unchanged (no bar), changed (the bar), and back to the saved text (no bar again).
    #[test]
    fn the_editing_states() {
        let saved = "Users land on `/login` again.";
        assert_eq!(state(saved, None), DescriptionState::Reading);
        assert_eq!(state(saved, Some(saved)), DescriptionState::Editing);
        let edited = "Users land on `/login` again.\n\n- [ ] no redirect loop";
        assert_eq!(state(saved, Some(edited)), DescriptionState::Changed);
        assert_eq!(
            state(saved, Some(saved)),
            DescriptionState::Editing,
            "undone"
        );

        assert_eq!(save_bar(DescriptionState::Reading, Platform::Mac), None);
        assert_eq!(save_bar(DescriptionState::Editing, Platform::Mac), None);
        let bar = save_bar(DescriptionState::Changed, Platform::Mac).unwrap();
        assert_eq!(
            (
                bar.changed.as_str(),
                bar.discard.as_str(),
                bar.save.as_str(),
                bar.keys.as_str()
            ),
            ("Description changed", "Discard", "Save", "\u{2318}S")
        );
        assert_eq!(
            label_hint(DescriptionState::Reading, Platform::Mac),
            "Click to edit"
        );
        assert_eq!(
            label_hint(DescriptionState::Changed, Platform::Mac),
            "Editing \u{b7} Esc to discard, \u{2318}S to save"
        );
    }

    /// Switching tabs, moving to another ticket or closing asks first only with changes; staying never asks.
    #[test]
    fn closing_with_changes_asks_first() {
        assert_eq!(leaving(DescriptionState::Reading, false), Leave::Go);
        assert_eq!(leaving(DescriptionState::Editing, false), Leave::Go);
        assert_eq!(leaving(DescriptionState::Changed, false), Leave::Ask);
        assert_eq!(leaving(DescriptionState::Changed, true), Leave::Go);
        let d = leave_dialog("SHOP-22");
        assert_eq!(d.title, "Discard your description changes?");
        assert_eq!(
            d.body,
            "SHOP-22\u{2019}s description has changes you haven\u{2019}t saved."
        );
        assert_eq!(
            (d.discard.as_str(), d.keep.as_str()),
            ("Discard changes", "Keep editing")
        );
    }
}

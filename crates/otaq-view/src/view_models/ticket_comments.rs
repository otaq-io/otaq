//! Ticket comments in the Ticket tab (CMT-10, CMT-11; d0109; *05 → Row · Ticket comments and a report* `s05-41` /
//! `s05-42`, *Row · Writing, editing and deleting* `s05-43` / `s05-44`): the thread under "Description · the plan"
//! and the box at its end. Not the line comments on a diff ([`super::comments`]).
//!
//! - **The thread:** "Comments · 4" with "Every stage's agent reads them before it starts", oldest first. Each comment
//!   says who wrote it, in which stage and round, and when ("Validate · round 2 · 3:05 pm"); yours are tinted with the
//!   accent, an agent's carries its tool's avatar (a plain agent mark while avatars are hidden, SHL-08); "· edited"
//!   once changed; a report is a chip "Report · 48 KB" with Open.
//! - **The box** ([`CommentBoxView`]): "Add a comment for the next stage…", **Comment** (⌘↵). A paste over the limit
//!   becomes a "Pasted text · 48 KB" chip, never text; typing past it shows "2,140 / 2,000" and turns Comment off. The
//!   page applies these rules while typing (it keeps no limit of its own: [`CommentLimits`] comes with the view); the
//!   service checks again. While an agent works, a quiet line says the comment is for the next stage. The dialogs
//!   (Send back, Fail, drag back; T-0447) reuse it with their own label and line ([`CommentBoxView::dialog`]).
//! - **The ⋯ menu:** Edit (in place, Cancel · Save), Copy text, Delete… with its dialog. The user may edit and delete
//!   every comment (CMT-04), so every comment has all three.
//! - **A failed check's card** shows its verdict ([`verdict_line`]).

use otaq_core::board::reports::size_label;
use otaq_core::board::ticket_comments::{CommentAuthor, TicketComment};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::clock::clock_12h;
use crate::view_models::comment_limits::CommentLimits;
use crate::view_models::menu::MenuItem;
use crate::view_models::tool_avatar::ToolAvatarView;

pub const HEADING: &str = "Comments";
pub const HINT: &str = "Every stage\u{2019}s agent reads them before it starts";
pub const PLACEHOLDER: &str = "Add a comment for the next stage\u{2026}";
pub const SUBMIT: &str = "Comment";
/// With only a pasted report, the comment still needs its own line (d0109).
pub const NEEDS_LINE: &str = "Say what the next agent should look for";
pub const PASTED: &str = "Pasted text";
/// The box's line while a file added in Overview waits in it (RPT-02, `s25-05`).
pub const FILE_LINE: &str = "A comment always has its own line; the file goes with it.";
pub const REPORT: &str = "Report";
pub const OPEN: &str = "Open";
pub const EDITED: &str = "\u{b7} edited";
pub const EDITING: &str = "editing";

/// How a comment's author is marked before its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "CommentMark")]
pub enum Mark {
    /// The user: a person icon on the accent tint.
    You,
    /// An agent or a lead whose tool is known, once avatars show (SHL-08).
    Tool { avatar: ToolAvatarView },
    /// An agent while avatars are hidden (one tool connected) or its tool isn't known: a plain agent mark.
    Agent,
}

/// A comment's report: "Report · 48 KB" and Open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CommentReportChip")]
pub struct ReportChip {
    /// "Report".
    pub label: String,
    /// "· 48 KB".
    pub size: String,
    /// "Open".
    pub open: String,
}

/// One comment in the thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CommentRow")]
pub struct CommentRow {
    /// Its number on the ticket: what Edit, Delete and Open send.
    pub n: u32,
    /// "Griller", "You", "Wishlist lead".
    pub author: String,
    /// The user's own: tinted with the accent.
    pub mine: bool,
    pub mark: Mark,
    /// After the name: "Grill · 2:14 pm", "2:20 pm", "Validate · round 2 · 3:05 pm".
    pub meta: String,
    /// The same while it is being edited: "2:20 pm · editing".
    pub editing_meta: String,
    /// "· edited" once its text changed.
    pub edited: Option<String>,
    /// Who and when in one line, for the delete dialog: "Validator · Validate · round 2 · 3:05 pm".
    pub heading: String,
    pub body: String,
    pub report: Option<ReportChip>,
}

/// The box's words and its limits; the dialogs give it their own label and line (T-0447).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CommentBoxView")]
pub struct CommentBoxView {
    /// A dialog's label over the box ("Comment for Dev (optional)"); none in the thread.
    pub label: Option<String>,
    pub placeholder: String,
    /// The box's own button ("Comment", ⌘↵); none in a dialog, whose button sends it.
    pub submit: Option<String>,
    /// The quiet line under it: "Read by the next stage's agent · to talk to Rust developer now, use its terminal"
    /// while an agent works, a dialog's "Posted as a comment for Dev · every later stage reads it".
    pub footer: Option<String>,
    /// Before the line: an arrow in the thread, a speech bubble in a dialog (`s03-50`, `s07-20`).
    pub footer_icon: IconName,
    /// The hint while only a pasted report is there.
    pub needs_line: String,
    /// The pasted chip's label: "Pasted text".
    pub pasted: String,
    /// The line while a file waits in the box (Overview's Add, a drop): it still needs its own line.
    pub file_line: String,
    /// The file chip's ✕, for screen readers.
    pub remove_file: String,
    pub open: String,
    /// The chip's ✕, for screen readers.
    pub remove: String,
    /// An empty box is fine (a dialog's optional comment); the thread's box needs text.
    pub optional: bool,
    pub limits: CommentLimits,
    /// The tick box's label, "Also tell the agent now", only in the thread's box while the ticket's agent or check
    /// runs (d0123); ticked by default. Ticked, the post sends `tell_agent`.
    pub tell_agent: Option<String>,
}

impl CommentBoxView {
    fn new(
        label: Option<String>,
        submit: Option<String>,
        footer: Option<String>,
        optional: bool,
    ) -> Self {
        Self {
            footer_icon: match label {
                Some(_) => IconName::MessageSquare,
                None => IconName::ArrowRight,
            },
            label,
            placeholder: PLACEHOLDER.to_owned(),
            submit,
            footer,
            needs_line: NEEDS_LINE.to_owned(),
            pasted: PASTED.to_owned(),
            file_line: FILE_LINE.to_owned(),
            remove_file: "Remove the file".to_owned(),
            open: OPEN.to_owned(),
            remove: "Remove the pasted text".to_owned(),
            optional,
            limits: CommentLimits::current(),
            tell_agent: None,
        }
    }

    /// The thread's box: its own Comment button, and the quiet line and "Also tell the agent now" while `working`
    /// (the running agent's name).
    pub fn thread(working: Option<&str>) -> Self {
        Self {
            tell_agent: working.map(|_| TELL_AGENT.to_owned()),
            ..Self::new(
                None,
                Some(SUBMIT.to_owned()),
                working.map(working_line),
                false,
            )
        }
    }

    /// A dialog's box (Send back, Fail, drag back; T-0447): its label and line, the dialog's own button.
    pub fn dialog(label: impl Into<String>, footer: impl Into<String>, optional: bool) -> Self {
        Self::new(Some(label.into()), None, Some(footer.into()), optional)
    }

    /// No label over the box (the New ticket form's report has its own field label).
    pub fn without_label(mut self) -> Self {
        self.label = None;
        self
    }

    /// A different placeholder (a dialog's own).
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }
}

/// The thread box's tick box while the ticket's agent or check runs (d0123).
pub const TELL_AGENT: &str = "Also tell the agent now";

/// "Read by the next stage's agent · to talk to Rust developer now, use its terminal" (CMT-10).
pub fn working_line(agent: &str) -> String {
    format!("Read by the next stage\u{2019}s agent \u{b7} to talk to {agent} now, use its terminal")
}

/// Edit in place: Cancel · Save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CommentEditTexts")]
pub struct EditTexts {
    pub cancel: String,
    pub save: String,
}

/// Delete…'s dialog (`s05-43`): it shows the comment and what deleting means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CommentDeleteTexts")]
pub struct DeleteTexts {
    pub title: String,
    pub note: String,
    pub cancel: String,
    pub confirm: String,
}

/// The menu's choices, as [`MenuItem`] ids.
pub const EDIT: &str = "edit";
pub const COPY: &str = "copy";
pub const DELETE: &str = "delete";

/// The Ticket tab's comments: `CommentThreadView` on the page (`ticket_comments.thread`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "CommentThreadView")]
pub struct ThreadView {
    pub key: String,
    /// "Comments · 4"; "Comments" with none.
    pub heading: String,
    pub hint: String,
    pub count: u32,
    pub comments: Vec<CommentRow>,
    /// The ⋯ menu, the same on every comment (the user may edit and delete any, CMT-04).
    pub menu: Vec<MenuItem>,
    /// The ⋯ button's name for screen readers.
    pub menu_label: String,
    pub edit: EditTexts,
    pub delete: DeleteTexts,
    #[serde(rename = "box")]
    #[ts(rename = "box")]
    pub composer: CommentBoxView,
}

/// What the thread is made of.
pub struct Facts<'a> {
    pub key: &'a str,
    /// Oldest first, as `ticket.comments` answers.
    pub comments: &'a [TicketComment],
    /// The name of the agent working on the ticket now (its running session's), if any.
    pub working: Option<&'a str>,
    /// Tool avatars show (two or more tools connected, [`super::tool_avatar::shown`]).
    pub avatars: bool,
}

/// "2:14 pm" in local time (0093).
pub fn local_clock(at: i64) -> String {
    otaq_core::local_time::local(at).map_or_else(String::new, |t| clock_12h(t.hour, t.minute))
}

pub fn thread(facts: &Facts, clock: &dyn Fn(i64) -> String) -> ThreadView {
    let count = u32::try_from(facts.comments.len()).unwrap_or(u32::MAX);
    ThreadView {
        key: facts.key.to_owned(),
        heading: match count {
            0 => HEADING.to_owned(),
            n => format!("{HEADING} \u{b7} {n}"),
        },
        hint: HINT.to_owned(),
        count,
        comments: facts
            .comments
            .iter()
            .map(|c| row(c, facts.avatars, clock))
            .collect(),
        menu: menu(),
        menu_label: "Comment actions".to_owned(),
        edit: EditTexts {
            cancel: "Cancel".to_owned(),
            save: "Save".to_owned(),
        },
        delete: DeleteTexts {
            title: "Delete this comment?".to_owned(),
            note: "Agents that start after this won\u{2019}t read it. History keeps what it said."
                .to_owned(),
            cancel: "Cancel".to_owned(),
            confirm: "Delete".to_owned(),
        },
        composer: CommentBoxView::thread(facts.working),
    }
}

/// Edit, Copy text, Delete… (`s05-43`).
pub fn menu() -> Vec<MenuItem> {
    vec![
        MenuItem::new(EDIT, "Edit").icon(IconName::Pencil),
        MenuItem::new(COPY, "Copy text").icon(IconName::Copy),
        MenuItem::new(DELETE, "Delete\u{2026}")
            .icon(IconName::Trash2)
            .separated(),
    ]
}

fn row(comment: &TicketComment, avatars: bool, clock: &dyn Fn(i64) -> String) -> CommentRow {
    let time = clock(comment.posted_at);
    let (mine, mark, mut meta) = match &comment.author {
        // The user's comment posted with the ticket (RPT-03): "You · at creation · 2:20 pm".
        CommentAuthor::User { .. } => (
            true,
            Mark::You,
            comment
                .at_creation
                .then(|| otaq_core::board::ticket_comments::AT_CREATION_LABEL.to_owned())
                .into_iter()
                .collect(),
        ),
        CommentAuthor::Agent { tool, round, .. } => {
            let mut meta: Vec<String> = comment.stage_name.iter().cloned().collect();
            meta.extend(round.map(|r| format!("round {r}")));
            (false, agent_mark(*tool, avatars), meta)
        }
        CommentAuthor::Lead { tool, .. } => (false, agent_mark(*tool, avatars), Vec::new()),
    };
    meta.extend((!time.is_empty()).then_some(time));
    let meta = meta.join(" \u{b7} ");
    let author = comment.author_label();
    CommentRow {
        n: comment.n,
        heading: [author.as_str(), meta.as_str()]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" \u{b7} "),
        editing_meta: [meta.as_str(), EDITING]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" \u{b7} "),
        author,
        mine,
        mark,
        meta,
        edited: comment.edited_at.map(|_| EDITED.to_owned()),
        body: comment.body.clone(),
        report: comment.report.map(|r| ReportChip {
            label: REPORT.to_owned(),
            size: format!("\u{b7} {}", size_label(r.bytes)),
            open: OPEN.to_owned(),
        }),
    }
}

/// An agent's or a lead's mark: its tool's avatar once avatars show, else the plain agent mark (also History's, CMT-14).
pub fn agent_mark(tool: Option<otaq_core::agent_tools::Tool>, avatars: bool) -> Mark {
    match tool.filter(|_| avatars) {
        Some(tool) => Mark::Tool {
            avatar: ToolAvatarView::of(tool, None),
        },
        None => Mark::Agent,
    }
}

/// The pasted chip's size: "· 48 KB" (the same words as a report's).
pub fn pasted_size(bytes: u64) -> String {
    format!("\u{b7} {}", size_label(bytes))
}

/// How much of a verdict a card shows before "· see comment", so the pointer stays on its two lines.
const VERDICT_CHARS: usize = 40;

/// A failed check's card line (CMT-10, `s05-41`): the verdict comment's first line, without the "Fail:" its card
/// already says and up to its first ";", cut at a word to fit, and "· see comment" ("Fail: the 10,000 row limit isn't
/// enforced; 3 tests fail in export_tests.rs." → "the 10,000 row limit isn't enforced · see comment").
pub fn verdict_line(verdict: &str) -> String {
    let line = verdict
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let line = match line.split_once(':') {
        Some((word, rest)) if word.trim().eq_ignore_ascii_case("fail") => rest.trim(),
        _ => line,
    };
    let clause = line.split(';').next().unwrap_or(line).trim();
    format!("{} \u{b7} see comment", cut(clause, VERDICT_CHARS))
}

/// `text` within `max` characters, cut at the last space before it with "…".
pub(crate) fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    // One more character: a space there ends the last word whole.
    let head: String = text.chars().take(max + 1).collect();
    let head = match head.rfind(' ') {
        Some(at) if at > 0 => head[..at].to_owned(),
        _ => text.chars().take(max).collect(),
    };
    format!(
        "{}\u{2026}",
        head.trim_end_matches([' ', ',', ';', ':', '.'])
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::agent_tools::Tool;
    use otaq_core::board::reports::ReportInfo;

    const MIN: i64 = 60_000;

    /// The frames' times: minutes after midnight, as "2:14 pm".
    fn clock(at: i64) -> String {
        let minutes = at / MIN;
        clock_12h(
            u32::try_from(minutes / 60).unwrap(),
            u32::try_from(minutes % 60).unwrap(),
        )
    }

    fn at(hour: i64, minute: i64) -> i64 {
        (hour * 60 + minute) * MIN
    }

    fn agent(name: &str, tool: Tool, stage: &str, round: Option<u32>) -> CommentAuthor {
        CommentAuthor::Agent {
            id: Some(name.to_lowercase()),
            name: Some(name.into()),
            tool: Some(tool),
            stage: Some(stage.to_lowercase()),
            round,
        }
    }

    fn comment(
        n: u32,
        author: CommentAuthor,
        stage: Option<&str>,
        body: &str,
        posted: i64,
    ) -> TicketComment {
        TicketComment {
            n,
            author,
            stage_name: stage.map(str::to_owned),
            body: body.into(),
            report: None,
            posted_at: posted,
            edited_at: None,
            report_name: None,
            at_creation: false,
        }
    }

    /// SHOP-34's four comments (`s05-41`): Grill's hand-over, the user's, Dev's, and Validate's failed round 2 with
    /// its 48 KB report.
    fn shop_34() -> Vec<TicketComment> {
        let mut validate = comment(
            4,
            agent("Validator", Tool::Codex, "Validate", Some(2)),
            Some("Validate"),
            "Fail: the 10,000 row limit isn\u{2019}t enforced; 3 tests fail in export_tests.rs.",
            at(15, 5),
        );
        validate.report = Some(ReportInfo {
            bytes: 48 * 1024,
            lines: 2310,
        });
        vec![
            comment(
                1,
                agent("Griller", Tool::Claude, "Grill", None),
                Some("Grill"),
                "Agreed with you: CSV only, admins only. Reuse the CSV writer in export.rs from SHOP-12.",
                at(14, 14),
            ),
            comment(
                2,
                CommentAuthor::User { name: "me".into() },
                None,
                "Also keep the column order the same as the table on screen.",
                at(14, 20),
            ),
            comment(
                3,
                agent("Rust developer", Tool::Claude, "Dev", None),
                Some("Dev"),
                "Done. Excel not added; left a TODO where it would go.",
                at(14, 58),
            ),
            validate,
        ]
    }

    fn view(comments: &[TicketComment], working: Option<&str>) -> ThreadView {
        thread(
            &Facts {
                key: "SHOP-34",
                comments,
                working,
                avatars: true,
            },
            &clock,
        )
    }

    /// `s05-41`: the thread while Rust developer works.
    #[test]
    fn the_four_comment_thread_reads_as_the_frame() {
        let v = view(&shop_34(), Some("Rust developer"));
        assert_eq!(v.heading, "Comments \u{b7} 4");
        // d0123: the tick box while the agent (or its check) runs.
        assert_eq!(
            v.composer.tell_agent.as_deref(),
            Some("Also tell the agent now")
        );
        assert_eq!(
            v.hint,
            "Every stage\u{2019}s agent reads them before it starts"
        );
        assert_eq!(v.count, 4);
        let lines: Vec<(&str, &str, bool)> = v
            .comments
            .iter()
            .map(|c| (c.author.as_str(), c.meta.as_str(), c.mine))
            .collect();
        assert_eq!(
            lines,
            [
                ("Griller", "Grill \u{b7} 2:14 pm", false),
                ("You", "2:20 pm", true),
                ("Rust developer", "Dev \u{b7} 2:58 pm", false),
                ("Validator", "Validate \u{b7} round 2 \u{b7} 3:05 pm", false),
            ]
        );
        // Yours carry the person mark, agents their tool's avatar.
        assert_eq!(v.comments[1].mark, Mark::You);
        let letters: Vec<String> = v
            .comments
            .iter()
            .filter_map(|c| match &c.mark {
                Mark::Tool { avatar } => Some(avatar.letters.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(letters, ["CC", "CC", "CX"]);
        // Only Validate's has a report.
        let reports: Vec<Option<&ReportChip>> =
            v.comments.iter().map(|c| c.report.as_ref()).collect();
        assert_eq!(reports[..3], [None, None, None]);
        assert_eq!(
            reports[3],
            Some(&ReportChip {
                label: "Report".into(),
                size: "\u{b7} 48 KB".into(),
                open: "Open".into(),
            })
        );
        assert!(v.comments.iter().all(|c| c.edited.is_none()));
        // The box, and the quiet line while an agent works.
        let b = &v.composer;
        assert_eq!(b.placeholder, "Add a comment for the next stage\u{2026}");
        assert_eq!(b.submit.as_deref(), Some("Comment"));
        assert_eq!(
            b.footer.as_deref(),
            Some(
                "Read by the next stage\u{2019}s agent \u{b7} to talk to Rust developer now, use its terminal"
            )
        );
        assert_eq!(b.label, None);
        assert!(!b.optional);
        assert_eq!(b.limits, CommentLimits::current());
    }

    /// RPT-03: your comment posted with the ticket reads "You · at creation · 2:20 pm".
    #[test]
    fn your_first_comment_says_at_creation() {
        let mut comments = shop_34();
        comments[1].at_creation = true;
        let v = view(&comments, None);
        assert_eq!(v.comments[1].meta, "at creation \u{b7} 2:20 pm");
        assert_eq!(
            v.comments[1].heading,
            "You \u{b7} at creation \u{b7} 2:20 pm"
        );
    }

    #[test]
    fn the_quiet_line_shows_only_while_an_agent_works() {
        assert_eq!(view(&shop_34(), None).composer.footer, None);
    }

    #[test]
    fn no_comments_yet() {
        let v = view(&[], None);
        assert_eq!(v.heading, "Comments");
        assert_eq!(v.count, 0);
        assert!(v.comments.is_empty());
    }

    /// One tool connected: avatars are hidden everywhere (SHL-08), so agents get the plain agent mark; a lead names
    /// itself with only its time.
    #[test]
    fn hidden_avatars_and_the_lead() {
        let lead = comment(
            5,
            CommentAuthor::Lead {
                epic_id: "e".into(),
                name: "Wishlist lead".into(),
                tool: Some(Tool::Claude),
            },
            None,
            "Use the same API as SHOP-40.1.",
            at(16, 0),
        );
        let comments = [shop_34().remove(0), lead];
        let v = thread(
            &Facts {
                key: "SHOP-40.2",
                comments: &comments,
                working: None,
                avatars: false,
            },
            &clock,
        );
        assert_eq!(v.comments[0].mark, Mark::Agent);
        assert_eq!(v.comments[1].mark, Mark::Agent);
        assert_eq!(v.comments[1].author, "Wishlist lead");
        assert_eq!(v.comments[1].meta, "4:00 pm");
        assert!(!v.comments[1].mine);
    }

    /// `s05-43`: an edited comment says so; the one being edited says "editing"; the menu and the delete dialog.
    #[test]
    fn editing_the_menu_and_the_delete_dialog() {
        let mut comments = shop_34();
        comments[2].body =
            "Done. Excel not added; left a TODO in export.rs where it would go.".into();
        comments[2].edited_at = Some(at(15, 30));
        let v = view(&comments, Some("Rust developer"));
        assert_eq!(v.comments[2].edited.as_deref(), Some("\u{b7} edited"));
        assert_eq!(v.comments[2].body, comments[2].body);
        assert_eq!(v.comments[1].editing_meta, "2:20 pm \u{b7} editing");
        assert_eq!(
            (v.edit.cancel.as_str(), v.edit.save.as_str()),
            ("Cancel", "Save")
        );
        let menu: Vec<(&str, &str, bool)> = v
            .menu
            .iter()
            .map(|i| (i.id.as_str(), i.label.as_str(), i.separated))
            .collect();
        assert_eq!(
            menu,
            [
                ("edit", "Edit", false),
                ("copy", "Copy text", false),
                ("delete", "Delete\u{2026}", true),
            ]
        );
        assert!(v.menu.iter().all(|i| !i.disabled));
        // The dialog shows who and when, the text, the report, and what deleting means.
        assert_eq!(
            v.comments[3].heading,
            "Validator \u{b7} Validate \u{b7} round 2 \u{b7} 3:05 pm"
        );
        assert_eq!(v.comments[1].heading, "You \u{b7} 2:20 pm");
        assert_eq!(
            v.delete,
            DeleteTexts {
                title: "Delete this comment?".into(),
                note:
                    "Agents that start after this won\u{2019}t read it. History keeps what it said."
                        .into(),
                cancel: "Cancel".into(),
                confirm: "Delete".into(),
            }
        );
    }

    /// `s05-43`'s three writing states rest on the box's words and limits: the paste chip ("Pasted text · 48 KB"),
    /// the hint with only a chip, and the counter against the limit (the page counts; the view gives the limit).
    #[test]
    fn the_writing_states_words() {
        let b = CommentBoxView::thread(None);
        assert_eq!(b.pasted, "Pasted text");
        assert_eq!(b.tell_agent, None, "no tick box without a running session");
        assert_eq!(pasted_size(48 * 1024), "\u{b7} 48 KB");
        assert_eq!(b.needs_line, "Say what the next agent should look for");
        assert_eq!(b.limits.chars, 2000);
        assert_eq!(b.open, "Open");
        // A dialog's box (T-0447): its label and line, no button of its own.
        let d = CommentBoxView::dialog(
            "Comment for Dev (optional)",
            "Posted as a comment for Dev \u{b7} every later stage reads it",
            true,
        );
        assert_eq!(d.label.as_deref(), Some("Comment for Dev (optional)"));
        assert_eq!(d.submit, None);
        assert_eq!(
            d.tell_agent, None,
            "a dialog's comment is for the next stage"
        );
        assert!(d.optional);
        assert_eq!(d.limits, b.limits);
    }

    /// `s05-41`'s card: "10,000 row limit isn't enforced · see comment".
    #[test]
    fn a_failed_checks_verdict_on_its_card() {
        assert_eq!(
            verdict_line(
                "Fail: the 10,000 row limit isn\u{2019}t enforced; 3 tests fail in export_tests.rs."
            ),
            "the 10,000 row limit isn\u{2019}t enforced \u{b7} see comment"
        );
        assert_eq!(
            verdict_line(
                "Fail: the export holds every row of the orders table in memory before writing"
            ),
            "the export holds every row of the orders\u{2026} \u{b7} see comment"
        );
        assert_eq!(
            verdict_line("\n  10,000 row limit isn\u{2019}t enforced\nmore lines"),
            "10,000 row limit isn\u{2019}t enforced \u{b7} see comment"
        );
        assert_eq!(verdict_line("fail : short"), "short \u{b7} see comment");
        assert_eq!(
            verdict_line("Failing: a word that only starts like it"),
            "Failing: a word that only starts like it \u{b7} see comment"
        );
    }
}

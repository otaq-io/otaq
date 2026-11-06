//! Ticket comments in the Ticket tab (CMT-10, CMT-11; d0109; *05 → Row · Ticket comments and a report* `s05-41` /
//! `s05-42`, *Row · Writing, editing and deleting* `s05-43` / `s05-44`): the thread under "Description · the plan"
//! and the box at its top. Not the line comments on a diff ([`super::comments`]).
//!
//! - **The thread:** "Comments · 4" with "Every stage's agent reads them before it starts", newest first under the
//!   box (agents still read them oldest first in `otaq prime`, d0109). Each comment says who wrote it, in which stage
//!   and round, and when ("Validate · round 2 · 3:05 pm"); yours are tinted with the accent, an agent's carries its tool's avatar (a plain agent mark while avatars are hidden, SHL-08); "· edited"
//!   once changed; its images are a row of thumbnails under its text that open full size (tuq 004, 006), each other
//!   attachment a chip, "test-run.txt · 48 KB", with Open for text (tuq 001, 002).
//! - **The box** ([`CommentBoxView`]): "Add a comment for the next stage…", **Comment** (⌘↵). A paste over the limit
//!   becomes a "Pasted text · 48 KB" chip, never text; typing past it shows "2,140 / 2,000" and turns Comment off. The
//!   page applies these rules while typing (it keeps no limit of its own: [`CommentLimits`] comes with the view); the
//!   service checks again. While an agent works, a quiet line says the comment is for the next stage. The dialogs
//!   (Send back, Fail, drag back; T-0447) reuse it with their own label and line ([`CommentBoxView::dialog`]).
//! - **The ⋯ menu:** Edit (in place, Cancel · Save), Copy text, Delete… with its dialog. The user may edit and delete
//!   every comment (CMT-04), so every comment has all three.
//! - **A failed check's card** shows its verdict ([`verdict_line`]).

use otaq_core::board::reports::{Attachment, size_label};
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
pub const PASTED: &str = "Pasted text";
/// The New ticket form's line while its report waits in the box (RPT-03): that form's first comment still needs its own
/// line; every other box sends attachments alone (tuq 007).
pub const FILE_LINE: &str = "A comment always has its own line; the file goes with it.";
pub const OPEN: &str = "Open";
/// The button by the box that opens the system picker (tuq 003), and the picker's title.
pub const ATTACH: &str = "Attach";
pub const ATTACH_TITLE: &str = "Attach files to the comment";
pub const EDITED: &str = "\u{b7} edited";
pub const EDITING: &str = "editing";

/// How a comment's author is marked before its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "CommentMark")]
pub enum Mark {
    /// The user: a person icon on the accent tint.
    You,
    /// An agent whose tool is known, once avatars show (SHL-08).
    Tool { avatar: ToolAvatarView },
    /// An agent while avatars are hidden (one tool connected) or its tool isn't known: a plain agent mark.
    Agent,
    /// Otaq itself, in Activity (Q432): what happened on its own (a restart, an automatic merge, a start that failed).
    Otaq,
}

/// What an attachment is (tuq 002): an image, text (opens in the report view) or any other file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "AttachmentKind")]
pub enum AttachmentKind {
    Image,
    Text,
    Other,
}

impl From<otaq_core::board::reports::AttachmentKind> for AttachmentKind {
    fn from(kind: otaq_core::board::reports::AttachmentKind) -> Self {
        use otaq_core::board::reports::AttachmentKind as Core;
        match kind {
            Core::Image => Self::Image,
            Core::Text => Self::Text,
            Core::Other => Self::Other,
        }
    }
}

/// One attachment of a comment (tuq 001, 002): "test-run.txt · 48 KB" and, for text, Open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CommentAttachmentChip")]
pub struct AttachmentChip {
    /// Its number in its comment (1, 2, 3…): what Open sends with the comment's number.
    pub attachment: u32,
    /// "test-run.txt", "Pasted text", "shot.png".
    pub name: String,
    pub kind: AttachmentKind,
    /// "· 48 KB".
    pub size: String,
    /// "Open" for text, which opens in the report view; none for images and other files.
    pub open: Option<String>,
}

impl AttachmentChip {
    pub fn of(a: &Attachment) -> Self {
        let kind = AttachmentKind::from(a.kind);
        Self {
            attachment: a.n,
            name: a.name.clone(),
            kind,
            size: format!("\u{b7} {}", size_label(a.bytes)),
            open: (kind == AttachmentKind::Text).then(|| OPEN.to_owned()),
        }
    }
}

/// The thumbnail's label: "Open shot.png full size" (tuq 004).
pub fn open_image_label(name: &str) -> String {
    format!("Open {name} full size")
}

/// The full-size view's close button (tuq 004).
pub const CLOSE_IMAGE: &str = "Close";

/// One image of a comment as a thumbnail (tuq 004; 006: the user's and agents' alike): a click opens it full size over
/// the sheet. The page loads its bytes by the ticket, the comment's number and [`Self::attachment`], never by a path
/// (the desktop's `otaq-attachment` images, which serve only images inside the data folder's reports).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CommentThumbnail")]
pub struct Thumbnail {
    /// Its number in its comment (1, 2, 3…).
    pub attachment: u32,
    /// "shot.png": the image's alt text and the full-size view's title.
    pub name: String,
    /// "120 KB", after the title in the full-size view.
    pub size: String,
    /// "Open shot.png full size": the thumbnail's label for screen readers and its tooltip.
    pub open: String,
    /// The full-size view's close button: "Close".
    pub close: String,
}

impl Thumbnail {
    /// An image's thumbnail; none for text and other files, which stay chips (tuq 002).
    pub fn of(a: &Attachment) -> Option<Self> {
        (AttachmentKind::from(a.kind) == AttachmentKind::Image).then(|| Self {
            attachment: a.n,
            name: a.name.clone(),
            size: size_label(a.bytes),
            open: open_image_label(&a.name),
            close: CLOSE_IMAGE.to_owned(),
        })
    }
}

/// A comment's attachments as the page shows them (tuq 002, 004): its images as a row of thumbnails, the rest (text,
/// other files) as chips, each in order. One rule for the thread and History; the Attachments column gives an
/// image's row its [`Thumbnail`] the same way.
pub fn thumbnails_and_chips(attachments: &[Attachment]) -> (Vec<Thumbnail>, Vec<AttachmentChip>) {
    let thumbnails = attachments.iter().filter_map(Thumbnail::of).collect();
    let chips = attachments
        .iter()
        .filter(|a| AttachmentKind::from(a.kind) != AttachmentKind::Image)
        .map(AttachmentChip::of)
        .collect();
    (thumbnails, chips)
}

/// One comment in the thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CommentRow")]
pub struct CommentRow {
    /// Its number on the ticket: what Edit, Delete and Open send.
    pub n: u32,
    /// "Spec writer", "You".
    pub author: String,
    /// The user's own: tinted with the accent.
    pub mine: bool,
    pub mark: Mark,
    /// After the name: "Product spec · 2:14 pm", "2:20 pm", "Validate · round 2 · 3:05 pm".
    pub meta: String,
    /// The same while it is being edited: "2:20 pm · editing".
    pub editing_meta: String,
    /// "· edited" once its text changed.
    pub edited: Option<String>,
    /// Who and when in one line, for the delete dialog: "Validator · Validate · round 2 · 3:05 pm".
    pub heading: String,
    pub body: String,
    /// Its images as a row of thumbnails after its text, in order (tuq 004, 006).
    pub thumbnails: Vec<Thumbnail>,
    /// One chip per other attachment (text, other files), in order (tuq 001, 002).
    pub attachments: Vec<AttachmentChip>,
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
    /// The quiet line under it: "Read by the next stage's agent" while an agent works (the tick box beside it says
    /// whether the running one is told too, d0123), a dialog's "Posted as a comment for Dev · every later stage reads it".
    pub footer: Option<String>,
    /// Before the line: an arrow in the thread, a speech bubble in a dialog (`s03-50`, `s07-20`).
    pub footer_icon: IconName,
    /// The pasted chip's label: "Pasted text".
    pub pasted: String,
    /// The line while a file waits in the box: only the New ticket form's ([`Self::one_report`]), whose report still
    /// needs its own line (RPT-03). Every other box sends attachments with no text (tuq 007).
    pub file_line: Option<String>,
    /// The file chip's ✕, for screen readers.
    pub remove_file: String,
    pub open: String,
    /// The chip's ✕, for screen readers.
    pub remove: String,
    /// The Attach button by the box (tuq 003): the system picker, several files at once. `None` for a box that holds
    /// one report, where a new file replaces the last (the New ticket form's, RPT-03, with its own Add).
    pub attach: Option<String>,
    /// The picker's title.
    pub attach_title: String,
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
            pasted: PASTED.to_owned(),
            file_line: None,
            remove_file: "Remove the file".to_owned(),
            open: OPEN.to_owned(),
            remove: "Remove the pasted text".to_owned(),
            attach: Some(ATTACH.to_owned()),
            attach_title: ATTACH_TITLE.to_owned(),
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
                working.map(|_| WORKING_LINE.to_owned()),
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

    /// A box that holds one report (the New ticket form's, RPT-03): no Attach button, a file added replaces the last,
    /// and the report waits for its line ([`FILE_LINE`]).
    pub fn one_report(mut self) -> Self {
        self.attach = None;
        self.file_line = Some(FILE_LINE.to_owned());
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

/// The box's line while an agent works (CMT-10): the comment is a hand-over for the next stage; "Also tell the agent
/// now" beside it is what reaches the running one (d0123), so the line no longer sends the user to its terminal.
pub const WORKING_LINE: &str = "Read by the next stage\u{2019}s agent";

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
    /// Newest first, under the box.
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
            .rev()
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
    };
    meta.extend((!time.is_empty()).then_some(time));
    let meta = meta.join(" \u{b7} ");
    let author = comment.author_label();
    let (thumbnails, attachments) = thumbnails_and_chips(&comment.attachments);
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
        thumbnails,
        attachments,
    }
}

/// An agent's mark: its tool's avatar once avatars show, else the plain agent mark (also History's, CMT-14).
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

/// What a failed check's card line ends with.
const SEE_COMMENT: &str = "see comment";

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
    // A verdict of only attachments (tuq 007): nothing to quote.
    if clause.is_empty() {
        return SEE_COMMENT.to_owned();
    }
    format!("{} \u{b7} {SEE_COMMENT}", cut(clause, VERDICT_CHARS))
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
    use otaq_core::board::reports::AttachmentKind as Kind;

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
            attachments: Vec::new(),
            posted_at: posted,
            edited_at: None,
            at_creation: false,
        }
    }

    fn attachment(n: u32, name: &str, kind: Kind, bytes: u64) -> Attachment {
        Attachment {
            n,
            name: name.into(),
            kind,
            bytes,
            lines: (kind == Kind::Text).then_some(bytes / 20),
            path: format!("/data/reports/shop-web/SHOP-34/c/{n}/{name}"),
        }
    }

    /// tuq 004: a comment with two images and a .txt shows two thumbnails, in order, and one chip that opens.
    #[test]
    fn a_comments_images_are_thumbnails_and_its_text_a_chip() {
        let mut c = comment(
            6,
            agent("Validator", Tool::Codex, "Validate", Some(1)),
            Some("Validate"),
            "The page before and after.",
            at(15, 40),
        );
        c.attachments = vec![
            attachment(1, "before.png", Kind::Image, 120 * 1024),
            attachment(2, "run.txt", Kind::Text, 2 * 1024),
            attachment(3, "after.jpg", Kind::Image, 80 * 1024),
        ];
        let v = view(&[c], None);
        let row = &v.comments[0];
        assert_eq!(
            row.thumbnails,
            [
                Thumbnail {
                    attachment: 1,
                    name: "before.png".into(),
                    size: "120 KB".into(),
                    open: "Open before.png full size".into(),
                    close: "Close".into(),
                },
                Thumbnail {
                    attachment: 3,
                    name: "after.jpg".into(),
                    size: "80 KB".into(),
                    open: "Open after.jpg full size".into(),
                    close: "Close".into(),
                },
            ]
        );
        let chips: Vec<(u32, &str, Option<&str>)> = row
            .attachments
            .iter()
            .map(|a| (a.attachment, a.name.as_str(), a.open.as_deref()))
            .collect();
        assert_eq!(chips, [(2, "run.txt", Some("Open"))]);
    }

    /// tuq 001, 002: a comment's text file and zip are one chip each, in order, and only text opens; its image is a
    /// thumbnail, not a chip (tuq 004).
    #[test]
    fn a_comment_has_one_chip_per_attachment() {
        let mut c = comment(
            5,
            CommentAuthor::User { name: "me".into() },
            None,
            "Before and after, with the log.",
            at(15, 30),
        );
        c.attachments = vec![
            attachment(1, "shot.png", Kind::Image, 120 * 1024),
            attachment(2, "run.txt", Kind::Text, 2 * 1024),
            attachment(3, "repro.zip", Kind::Other, 3 * 1024 * 1024),
        ];
        let v = view(&[c], None);
        let chips: Vec<(u32, &str, AttachmentKind, &str, Option<&str>)> = v.comments[0]
            .attachments
            .iter()
            .map(|a| {
                (
                    a.attachment,
                    a.name.as_str(),
                    a.kind,
                    a.size.as_str(),
                    a.open.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            chips,
            [
                (
                    2,
                    "run.txt",
                    AttachmentKind::Text,
                    "\u{b7} 2 KB",
                    Some("Open")
                ),
                (3, "repro.zip", AttachmentKind::Other, "\u{b7} 3 MB", None),
            ]
        );
        let images: Vec<&str> = v.comments[0]
            .thumbnails
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(images, ["shot.png"]);
    }

    /// SHOP-34's four comments (`s05-41`): Product spec's hand-over, the user's, Dev's, and Validate's failed round 2
    /// with its 48 KB report.
    fn shop_34() -> Vec<TicketComment> {
        let mut validate = comment(
            4,
            agent("Validator", Tool::Codex, "Validate", Some(2)),
            Some("Validate"),
            "Fail: the 10,000 row limit isn\u{2019}t enforced; 3 tests fail in export_tests.rs.",
            at(15, 5),
        );
        validate.attachments = vec![attachment(1, "test-run.txt", Kind::Text, 48 * 1024)];
        vec![
            comment(
                1,
                agent("Spec writer", Tool::Claude, "Product spec", None),
                Some("Product spec"),
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
                ("Validator", "Validate \u{b7} round 2 \u{b7} 3:05 pm", false),
                ("Rust developer", "Dev \u{b7} 2:58 pm", false),
                ("You", "2:20 pm", true),
                ("Spec writer", "Product spec \u{b7} 2:14 pm", false),
            ],
            "newest first"
        );
        // Yours carry the person mark, agents their tool's avatar.
        assert_eq!(v.comments[2].mark, Mark::You);
        let letters: Vec<String> = v
            .comments
            .iter()
            .filter_map(|c| match &c.mark {
                Mark::Tool { avatar } => Some(avatar.letters.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(letters, ["CX", "CC", "CC"]);
        // Only Validate's has a report.
        let chips: Vec<&[AttachmentChip]> = v
            .comments
            .iter()
            .map(|c| c.attachments.as_slice())
            .collect();
        assert!(chips[1..].iter().all(|c| c.is_empty()));
        assert_eq!(
            chips[0],
            [AttachmentChip {
                attachment: 1,
                name: "test-run.txt".into(),
                kind: AttachmentKind::Text,
                size: "\u{b7} 48 KB".into(),
                open: Some("Open".into()),
            }]
        );
        assert!(v.comments.iter().all(|c| c.edited.is_none()));
        // The box, and the quiet line while an agent works.
        let b = &v.composer;
        assert_eq!(b.placeholder, "Add a comment for the next stage\u{2026}");
        assert_eq!(b.submit.as_deref(), Some("Comment"));
        assert_eq!(
            b.footer.as_deref(),
            Some("Read by the next stage\u{2019}s agent")
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
        assert_eq!(v.comments[2].meta, "at creation \u{b7} 2:20 pm");
        assert_eq!(
            v.comments[2].heading,
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

    /// `s05-43`: an edited comment says so; the one being edited says "editing"; the menu and the delete dialog.
    #[test]
    fn editing_the_menu_and_the_delete_dialog() {
        let mut comments = shop_34();
        comments[2].body =
            "Done. Excel not added; left a TODO in export.rs where it would go.".into();
        comments[2].edited_at = Some(at(15, 30));
        let v = view(&comments, Some("Rust developer"));
        assert_eq!(v.comments[1].edited.as_deref(), Some("\u{b7} edited"));
        assert_eq!(v.comments[1].body, comments[2].body);
        assert_eq!(v.comments[2].editing_meta, "2:20 pm \u{b7} editing");
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
            v.comments[0].heading,
            "Validator \u{b7} Validate \u{b7} round 2 \u{b7} 3:05 pm"
        );
        assert_eq!(v.comments[2].heading, "You \u{b7} 2:20 pm");
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

    /// `s05-43`'s writing states rest on the box's words and limits: the paste chip ("Pasted text · 48 KB") and the
    /// counter against the limit (the page counts; the view gives the limit). A chip alone needs no line (tuq 007):
    /// only the New ticket form's box asks for one.
    #[test]
    fn the_writing_states_words() {
        let b = CommentBoxView::thread(None);
        assert_eq!(b.pasted, "Pasted text");
        assert_eq!(b.tell_agent, None, "no tick box without a running session");
        assert_eq!(pasted_size(48 * 1024), "\u{b7} 48 KB");
        assert_eq!(b.file_line, None, "attachments alone are sent (tuq 007)");
        assert_eq!(b.limits.chars, 2000);
        assert_eq!(b.open, "Open");
        assert_eq!(
            b.attach.as_deref(),
            Some("Attach"),
            "every comment box takes files (tuq 003)"
        );
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
        assert_eq!(d.attach, b.attach);
        assert_eq!(d.file_line, None);
        let one = d.one_report();
        assert_eq!(one.attach, None);
        assert_eq!(one.file_line.as_deref(), Some(FILE_LINE));
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
        // A verdict of only attachments (tuq 007).
        assert_eq!(verdict_line(""), "see comment");
        assert_eq!(verdict_line("Fail:"), "see comment");
        assert_eq!(
            verdict_line("Failing: a word that only starts like it"),
            "Failing: a word that only starts like it \u{b7} see comment"
        );
    }
}

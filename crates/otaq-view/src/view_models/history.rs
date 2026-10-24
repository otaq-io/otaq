//! The History tab (UI-04, CMT-14; d0045, d0109): every session the ticket had, with its time, duration, end reason
//! and full tool session id (copyable, so the tool's resume finds the conversation), and the ticket's events
//! (created, moved, start interrupted, held, retried, edited…), newest first; one entry per validation round
//! (VAL-12, `validation_report.rs`).
//!
//! **Description and comments** (*05 → Row · History* `s05-45` / `s05-46`): every description change by anyone
//! ("Griller · Grill edited the description · added Agreed (4 lines)", "You edited the description · 1 line changed")
//! and every comment posted ("Validator · Validate · round 2 commented · with a report (48 KB)"), edited ("You edited
//! Rust developer's comment") or deleted ("You deleted a comment"), each with its author's mark, opening a before /
//! after or what the comment said, with its report (a deleted comment's still opens). Agents never see History.
//!
//! **Activity** (SHT-05, T-0466; canvas `s25-07`): the sheet's tab shows these rows oldest first ([`oldest_first`]),
//! each tagged with its [`Kind`], under the filters **All · Comments · Stages · Description** ([`Filter`]).

use std::collections::BTreeMap;

use otaq_core::board::epic_flight::EpicQueueMode;
use otaq_core::board::reports::size_label;
use otaq_core::board::ticket_comments::CommentAuthor;
use otaq_core::board::tickets::{EventKind, HistoryEvent};
use otaq_core::board::validation::ValidationRecord;
use otaq_core::domain::{Actor, StartStep, Status};
use otaq_core::session::{SessionInfo, SessionKind};
use otaq_core::usage::SessionUsage;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::theme::Palette;
use crate::view_models::clock;
use crate::view_models::stage_choices::{self, HistoryLine};
use crate::view_models::ticket_comments::{self as comments, Mark, ReportChip};
use crate::view_models::validation_report;

/// What History knows beyond the events: the flow's stage names (a comment's author names its stage by id) and
/// whether tool avatars show (SHL-08).
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// Stage id → its name in the ticket's flow now.
    pub stages: BTreeMap<String, String>,
    pub avatars: bool,
}

/// How a line of a before / after reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "HistoryLineKind")]
pub enum LineKind {
    Same,
    Added,
    Removed,
    /// Lines left out between two changes: "…".
    Gap,
}

/// A line of a before / after: "+", "-" or nothing before it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "HistoryDiffLine")]
pub struct DiffLine {
    pub kind: LineKind,
    pub sign: String,
    pub text: String,
}

/// A comment's report in History: "Report · 48 KB" and Open (comment `n`'s, also once deleted).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "HistoryReport")]
pub struct OpenReport {
    pub n: u32,
    pub chip: ReportChip,
}

/// What a row opens (`s05-45`): "DESCRIPTION" or "COMMENT", "Before / after" on an edit, the lines, a report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "HistoryOpens")]
pub struct Opens {
    pub label: String,
    pub caption: Option<String>,
    pub lines: Vec<DiffLine>,
    pub report: Option<OpenReport>,
}

/// What a row is about, for Activity's filters (SHT-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ActivityKind")]
pub enum Kind {
    /// A comment posted, edited or deleted.
    Comment,
    /// Who worked on it and where it went: sessions, check rounds, moves, passes, send-backs, merges.
    Stage,
    /// A change of the description (with its before / after).
    Description,
    /// The rest (created, pinned, a dependency, a question…): only under All.
    Other,
}

/// Activity's filters (SHT-05), in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ActivityFilter")]
pub enum Filter {
    All,
    Comments,
    Stages,
    Description,
}

impl Filter {
    pub const ALL: [Filter; 4] = [
        Filter::All,
        Filter::Comments,
        Filter::Stages,
        Filter::Description,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Comments => "Comments",
            Self::Stages => "Stages",
            Self::Description => "Description",
        }
    }

    /// The row kind it shows (`None`: every kind).
    pub fn kind(self) -> Option<Kind> {
        match self {
            Self::All => None,
            Self::Comments => Some(Kind::Comment),
            Self::Stages => Some(Kind::Stage),
            Self::Description => Some(Kind::Description),
        }
    }

    pub fn shows(self, kind: Kind) -> bool {
        self.kind().is_none_or(|k| k == kind)
    }
}

/// Activity's order note beside the filters.
pub const OLDEST_FIRST: &str = "oldest first";

/// The rows as Activity lists them: oldest first (History builds them newest first).
pub fn oldest_first(mut rows: Vec<HistoryRow>) -> Vec<HistoryRow> {
    rows.reverse();
    rows
}

/// The rows `filter` shows, in their order.
pub fn filtered(rows: &[HistoryRow], filter: Filter) -> Vec<&HistoryRow> {
    rows.iter().filter(|r| filter.shows(r.kind)).collect()
}

/// The kind of an event's row (descriptions and comments are told apart in [`comment_row`]).
fn kind_of(kind: EventKind) -> Kind {
    match kind {
        EventKind::Moved
        | EventKind::Dragged
        | EventKind::StageFinished
        | EventKind::Passed
        | EventKind::ReviewSubmitted
        | EventKind::SentToFix
        | EventKind::SentBack
        | EventKind::SentBackWithPrComments
        | EventKind::Merged
        | EventKind::MergedAutomatically
        | EventKind::Reopened
        | EventKind::Parked
        | EventKind::Closed
        | EventKind::StartInterrupted
        | EventKind::StartRetried
        | EventKind::Interrupted
        | EventKind::SessionEnded
        | EventKind::Resumed
        | EventKind::ResumeFailed
        | EventKind::StartedFromScratch
        | EventKind::ValidationEnded
        | EventKind::PrProposed
        | EventKind::PrConfirmed
        | EventKind::PrCancelled
        | EventKind::PrOpened
        | EventKind::AutoMergeStopped => Kind::Stage,
        _ => Kind::Other,
    }
}

/// One line of the History table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    pub title: String,
    pub detail: Option<String>,
    /// Unix milliseconds; shown as HH:MM.
    pub at: i64,
    /// "72 min", "running", or "—".
    pub duration: String,
    /// The session's tokens ("226k"); "—" for events, other sessions, and unknown counts (USG-01).
    pub tokens: String,
    /// The full tool session id, for the copy button.
    pub tool_session_id: Option<String>,
    pub is_session: bool,
    /// A validation round's outcome icon and colours; sessions and events have their own icons.
    pub outcome: Option<(IconName, Palette)>,
    /// A session row's session id (its title names the agent, [`with_agents`]).
    pub session: Option<String>,
    /// A description or comment row's author mark, in place of the icon.
    pub mark: Option<Mark>,
    /// What the row opens: a before / after, or what a comment said.
    pub opens: Option<Opens>,
    /// Activity's filter it shows under.
    pub kind: Kind,
}

/// "Built: only in the profile form · Other option: everywhere"; several other options read "Other options: a · b".
fn decision_detail(p: &serde_json::Value) -> String {
    let others: Vec<&str> = p
        .get("other_options")
        .and_then(|o| o.as_array())
        .map(|o| o.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let built = format!("Built: {}", text(p, "built").unwrap_or_default());
    match others.as_slice() {
        [] => built,
        [one] => format!("{built} \u{b7} Other option: {one}"),
        many => format!("{built} \u{b7} Other options: {}", many.join(" \u{b7} ")),
    }
}

/// Sessions and events merged, newest first, with each session's tokens from `usage`.
pub fn rows(
    events: &[HistoryEvent],
    sessions: &[SessionInfo],
    usage: &[SessionUsage],
) -> Vec<HistoryRow> {
    rows_in(events, sessions, usage, &Facts::default())
}

/// [`rows`] knowing the flow's stage names and whether avatars show.
pub fn rows_in(
    events: &[HistoryEvent],
    sessions: &[SessionInfo],
    usage: &[SessionUsage],
    facts: &Facts,
) -> Vec<HistoryRow> {
    let mut rows: Vec<HistoryRow> = sessions.iter().map(|s| session_row(s, usage)).collect();
    rows.extend(events.iter().filter_map(|e| event_row_in(e, facts)));
    // Newest first; a session row comes before an event at the same moment.
    rows.sort_by(|a, b| b.at.cmp(&a.at).then(b.is_session.cmp(&a.is_session)));
    rows
}

/// [`rows`] with one entry per validation round from `records` (in place of the rounds' bare events), `rounds`
/// being the project's Validation rounds.
pub fn rows_with_rounds(
    events: &[HistoryEvent],
    sessions: &[SessionInfo],
    usage: &[SessionUsage],
    records: &[ValidationRecord],
    rounds: u32,
    facts: &Facts,
) -> Vec<HistoryRow> {
    let events: Vec<HistoryEvent> = events
        .iter()
        .filter(|e| e.kind != EventKind::ValidationEnded)
        .cloned()
        .collect();
    let mut rows = rows_in(&events, sessions, usage, facts);
    rows.extend(records.iter().map(|r| {
        let round = validation_report::round_row(r, rounds);
        HistoryRow {
            title: round.title,
            detail: round.detail,
            at: round.at,
            duration: round.duration,
            tokens: round.tokens,
            tool_session_id: None,
            is_session: false,
            outcome: Some((round.icon, round.palette)),
            session: None,
            mark: None,
            opens: None,
            kind: Kind::Stage,
        }
    }));
    rows.sort_by(|a, b| b.at.cmp(&a.at).then(b.is_session.cmp(&a.is_session)));
    rows
}

fn session_row(s: &SessionInfo, usage: &[SessionUsage]) -> HistoryRow {
    let title = match s.kind {
        SessionKind::Agent => "Agent session",
        SessionKind::Answer => "Answer session",
        SessionKind::Delivery => "PR session",
        SessionKind::Shell => "Shell",
        SessionKind::Project => "Project terminal",
        SessionKind::Run => "Run",
        SessionKind::Validator => "Validator session",
        SessionKind::ProjectAgent => "Project agent",
    };
    let duration = match s.ended_at {
        Some(end) => clock::duration(end - s.started_at),
        None => "running".to_owned(),
    };
    // "claude 3be94c10… · service stopped": the tool's command and its id.
    let conversation = s
        .tool
        .zip(s.tool_session_id.as_deref())
        .map(|(tool, id)| format!("{} {id}", tool.command_name()));
    let detail = match (conversation, s.end_reason.as_deref()) {
        (Some(conversation), Some(reason)) => Some(format!("{conversation} · {reason}")),
        (Some(conversation), None) => Some(conversation),
        (None, Some(reason)) => Some(reason.to_owned()),
        (None, None) => None,
    };
    HistoryRow {
        title: title.to_owned(),
        detail,
        at: s.started_at,
        duration,
        tokens: crate::view_models::usage::session_label(usage, &s.id),
        tool_session_id: s.tool_session_id.clone(),
        is_session: true,
        outcome: None,
        session: Some(s.id.clone()),
        mark: None,
        opens: None,
        // The Run's and the Shell's sessions aren't stage work.
        kind: match s.kind {
            SessionKind::Shell | SessionKind::Run | SessionKind::Project => Kind::Other,
            _ => Kind::Stage,
        },
    }
}

fn who(actor: &Actor) -> String {
    match actor {
        Actor::User(_) => "by you".to_owned(),
        Actor::Agent(key) => format!("by {key}'s agent"),
        Actor::ProjectAgent(_) => "by a Project agent".to_owned(),
    }
}

/// A decision's question cut short for a "Kept by you" line: "Split fullName …"; a short one stays whole.
fn short_question(question: &str) -> String {
    let question = question.trim();
    if question.chars().count() <= 24 {
        return question.to_owned();
    }
    let mut out = String::new();
    for word in question.split_whitespace() {
        let word = word.trim_end_matches([',', ';', ':']);
        if !out.is_empty() && out.chars().count() + 1 + word.chars().count() > 20 {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    format!("{out} \u{2026}")
}

/// "by Wishlist lead" when an epic lead made the change (its name is in the payload, CLI-07), else [`who`].
fn by(e: &HistoryEvent) -> String {
    match text(&e.payload, "lead") {
        Some(lead) => format!("by {lead}"),
        None => who(&e.actor),
    }
}

/// "description", "title · priority": the fields a lead's update changed, as History names them.
fn fields(names: impl Iterator<Item = String>) -> String {
    names
        .map(|f| match f.as_str() {
            "body" => "description".to_owned(),
            "base_branch" => "base branch".to_owned(),
            "external_id" => "External ID".to_owned(),
            _ => f,
        })
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}

/// An epic lead's change of a child, on the epic (CLI-07, d0083).
fn lead_changed(p: &serde_json::Value) -> (String, Option<String>) {
    let lead = text(p, "lead").unwrap_or_else(|| "the lead".to_owned());
    let child = text(p, "child").unwrap_or_default();
    let parent = text(p, "parent").unwrap_or_default();
    let by = format!("by {lead}");
    match text(p, "change").as_deref() {
        Some("updated") => {
            let names = p
                .get("fields")
                .and_then(|f| f.as_array())
                .into_iter()
                .flatten()
                .filter_map(|f| f.as_str().map(str::to_owned));
            let changed = fields(names);
            (
                format!("Updated {child}"),
                Some(if changed.is_empty() {
                    by
                } else {
                    format!("{by} \u{b7} {changed}")
                }),
            )
        }
        Some("dep_added") => (format!("{child} waits for {parent}"), Some(by)),
        Some("dep_removed") => (format!("{child} no longer waits for {parent}"), Some(by)),
        Some("created") => (
            format!("Added {child} {}", text(p, "title").unwrap_or_default())
                .trim_end()
                .to_owned(),
            Some(by),
        ),
        _ => (format!("Changed {child}"), Some(by)),
    }
}

/// "SHOP-40.2 asked Wishlist lead", "Wishlist lead answered SHOP-40.2", "Wishlist lead passed SHOP-40.2's question
/// on to you", "SHOP-40.2's question handed back to you" (LEAD-06).
fn lead_question(p: &serde_json::Value) -> (String, Option<String>) {
    let child = text(p, "child").unwrap_or_default();
    let lead = text(p, "lead").unwrap_or_else(|| "the lead".to_owned());
    let quoted = |field: &str| {
        text(p, field).map(|t| format!("\u{201c}{}\u{201d}", t.lines().next().unwrap_or_default()))
    };
    match text(p, "step").as_deref() {
        Some("answered") => (format!("{lead} answered {child}"), quoted("answer")),
        Some("passed_on") => (
            format!("{lead} passed {child}'s question on to you"),
            Some(match (text(p, "recommended"), text(p, "reason")) {
                (Some(option), Some(reason)) => {
                    format!("recommended \u{201c}{option}\u{201d}: {reason}")
                }
                (None, Some(reason)) => reason,
                (_, None) => String::new(),
            })
            .filter(|d| !d.is_empty()),
        ),
        Some("handed_back") => (
            format!("{child}'s question handed back to you"),
            Some(format!("no answer from {lead}")),
        ),
        _ => (format!("{child} asked {lead}"), quoted("question")),
    }
}

/// A stuck child and its epic's lead (LEAD-07): "SHOP-40.3 stopped · sent to Wishlist lead" · "Validation failed 3
/// times", "Wishlist lead sent its note to SHOP-40.3", "SHOP-40.3's stop handed back to you" · "no note from
/// Wishlist lead".
fn lead_stuck(p: &serde_json::Value) -> (String, Option<String>) {
    let child = text(p, "child").unwrap_or_default();
    let lead = text(p, "lead").unwrap_or_else(|| "the lead".to_owned());
    match text(p, "step").as_deref() {
        Some("noted") => (
            format!("{lead} sent its note to {child}"),
            text(p, "reason"),
        ),
        Some("handed_back") => (
            format!("{child}'s stop handed back to you"),
            Some(format!("no note from {lead}")),
        ),
        _ => (
            format!("{child} stopped \u{b7} sent to {lead}"),
            text(p, "reason"),
        ),
    }
}

/// A stage choice (AGM-08): "Dev: Quick developer (set by the Project agent)" · "typo fix"; on an epic for a child,
/// "Wishlist lead set SHOP-40.3's Dev: Quick developer".
fn choice_line(p: &serde_json::Value) -> (String, Option<String>) {
    let stage = text(p, "stage").unwrap_or_default();
    let choice = text(p, "choice").unwrap_or_default();
    let by = text(p, "by").unwrap_or_else(|| "you".to_owned());
    let reason = text(p, "reason");
    match text(p, "child") {
        Some(child) => (format!("{by} set {child}'s {stage}: {choice}"), reason),
        None => (format!("{stage}: {choice} (set by {by})"), reason),
    }
}

/// `detail` with the stage Apply now's move came from, when it did: "by you · Security was removed" (APN-04).
fn removed(payload: &serde_json::Value, detail: String) -> String {
    match text(payload, "removed") {
        Some(stage) => format!(
            "{detail} \u{b7} {}",
            otaq_core::flow::apply_now::removed_note(&stage)
        ),
        None => detail,
    }
}

fn text(payload: &serde_json::Value, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

/// How much of a comment a row quotes: "“Agreed with you: CSV only, admins only…”".
const QUOTE_CHARS: usize = 40;

/// The start of `text` in quotes, cut at a word to [`QUOTE_CHARS`] with "…".
fn quote(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out = String::new();
    let mut cut = false;
    for word in &words {
        let len = out.chars().count() + usize::from(!out.is_empty()) + word.chars().count();
        if len > QUOTE_CHARS && !out.is_empty() {
            cut = true;
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if cut {
        out = out.trim_end_matches(['.', ',', ';', ':']).to_owned();
        out.push('\u{2026}');
    }
    format!("\u{201c}{out}\u{201d}")
}

/// "Rust developer · Dev", "Validator · Validate · round 2", "You", "Wishlist lead".
fn author_label(author: &CommentAuthor, facts: &Facts) -> String {
    match author {
        CommentAuthor::User { .. } => "You".to_owned(),
        CommentAuthor::Agent {
            name, stage, round, ..
        } => {
            let mut parts = vec![name.clone().unwrap_or_else(|| "Agent".to_owned())];
            parts.extend(
                stage
                    .as_ref()
                    .map(|id| facts.stages.get(id).cloned().unwrap_or_else(|| id.clone())),
            );
            parts.extend(round.map(|r| format!("round {r}")));
            parts.join(" \u{b7} ")
        }
        CommentAuthor::Lead { name, .. } => name.clone(),
    }
}

fn author_mark(author: &CommentAuthor, facts: &Facts) -> Mark {
    match author {
        CommentAuthor::User { .. } => Mark::You,
        CommentAuthor::Agent { tool, .. } | CommentAuthor::Lead { tool, .. } => {
            comments::agent_mark(*tool, facts.avatars)
        }
    }
}

/// The payload's comment author.
fn author_of(p: &serde_json::Value) -> Option<CommentAuthor> {
    serde_json::from_value(p.get("author")?.clone()).ok()
}

/// "Rust developer's", "Wishlist lead's".
fn whose(author: &CommentAuthor) -> String {
    match author {
        CommentAuthor::User { .. } => "your".to_owned(),
        CommentAuthor::Agent { name, .. } => {
            format!("{}\u{2019}s", name.as_deref().unwrap_or("an agent"))
        }
        CommentAuthor::Lead { name, .. } => format!("{name}\u{2019}s"),
    }
}

fn line(kind: LineKind, text: &str) -> DiffLine {
    let sign = match kind {
        LineKind::Added => "+",
        LineKind::Removed => "-",
        LineKind::Same | LineKind::Gap => "",
    };
    DiffLine {
        kind,
        sign: sign.to_owned(),
        text: text.to_owned(),
    }
}

/// Above this many line pairs the before / after is shown as all removed, then all added.
const DIFF_CELLS: usize = 1_000_000;

/// `before` and `after` line by line: the lines both keep (a longest common subsequence) and what went and came.
fn line_diff(before: &str, after: &str) -> Vec<DiffLine> {
    let (a, b): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    let (n, m) = (a.len(), b.len());
    if n.saturating_mul(m) > DIFF_CELLS {
        return a
            .iter()
            .map(|l| line(LineKind::Removed, l))
            .chain(b.iter().map(|l| line(LineKind::Added, l)))
            .collect();
    }
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push(line(LineKind::Same, a[i]));
            i += 1;
            j += 1;
        } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
            // In a change the removed lines come first, then the added ones.
            out.push(line(LineKind::Removed, a[i]));
            i += 1;
        } else {
            out.push(line(LineKind::Added, b[j]));
            j += 1;
        }
    }
    out
}

/// Only the changes and `context` unchanged lines around each; a "…" line where more were left out.
fn around_changes(lines: Vec<DiffLine>, context: usize) -> Vec<DiffLine> {
    let changed: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind != LineKind::Same)
        .map(|(i, _)| i)
        .collect();
    let near = |i: usize| changed.iter().any(|&c| c.abs_diff(i) <= context);
    let mut out = Vec::new();
    let mut skipped = false;
    for (i, l) in lines.into_iter().enumerate() {
        if l.kind != LineKind::Same || near(i) {
            if skipped && !out.is_empty() {
                out.push(line(LineKind::Gap, "\u{2026}"));
            }
            skipped = false;
            out.push(l);
        } else {
            skipped = true;
        }
    }
    out
}

/// A Markdown line as words: "## Agreed" → "Agreed", "- admins only" → "admins only".
fn plain(text: &str) -> String {
    let t = text.trim().trim_start_matches('#').trim_start();
    let t = t
        .strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .unwrap_or(t);
    quote(t.trim())
        .trim_matches(['\u{201c}', '\u{201d}'])
        .to_owned()
}

fn lines_count(n: usize) -> String {
    match n {
        1 => "1 line".to_owned(),
        n => format!("{n} lines"),
    }
}

/// What a description change did: "added Agreed (4 lines)", "removed Out of scope (2 lines)", "1 line changed".
fn change_summary(diff: &[DiffLine]) -> String {
    let added: Vec<&DiffLine> = diff.iter().filter(|l| l.kind == LineKind::Added).collect();
    let removed: Vec<&DiffLine> = diff
        .iter()
        .filter(|l| l.kind == LineKind::Removed)
        .collect();
    let first = |lines: &[&DiffLine]| {
        lines
            .iter()
            .map(|l| plain(&l.text))
            .find(|t| !t.is_empty())
            .unwrap_or_default()
    };
    match (added.len(), removed.len()) {
        (0, 0) => "no line changed".to_owned(),
        (a, 0) => format!("added {} ({})", first(&added), lines_count(a)),
        (0, r) => format!("removed {} ({})", first(&removed), lines_count(r)),
        (a, r) => format!("{} changed", lines_count(a.max(r))),
    }
}

/// Who made a change the event doesn't name a comment author for: "You", the lead, "SHOP-34's agent".
fn changed_by(e: &HistoryEvent) -> String {
    if let Some(lead) = text(&e.payload, "lead") {
        return lead;
    }
    match &e.actor {
        Actor::User(_) => "You".to_owned(),
        Actor::Agent(key) => format!("{key}\u{2019}s agent"),
        Actor::ProjectAgent(_) => "A Project agent".to_owned(),
    }
}

fn report_of(p: &serde_json::Value, n: u32, field: &str) -> Option<OpenReport> {
    let bytes = p.get(field)?.get("bytes")?.as_u64()?;
    Some(OpenReport {
        n,
        chip: ReportChip {
            label: comments::REPORT.to_owned(),
            size: format!("\u{b7} {}", size_label(bytes)),
            open: comments::OPEN.to_owned(),
        },
    })
}

fn told(
    e: &HistoryEvent,
    title: String,
    detail: Option<String>,
    mark: Mark,
    opens: Opens,
    kind: Kind,
) -> HistoryRow {
    HistoryRow {
        title,
        detail,
        at: e.at,
        duration: "\u{2014}".to_owned(),
        tokens: "\u{2014}".to_owned(),
        tool_session_id: None,
        is_session: false,
        outcome: None,
        session: None,
        mark: Some(mark),
        opens: Some(opens),
        kind,
    }
}

const BEFORE_AFTER: &str = "Before / after";

/// The rows of CMT-14: a description change, and a comment posted, edited or deleted. `None` for anything else.
fn comment_row(e: &HistoryEvent, facts: &Facts) -> Option<HistoryRow> {
    let p = &e.payload;
    let n = p
        .get("n")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0);
    let by_user = matches!(e.actor, Actor::User(_));
    match e.kind {
        EventKind::Updated => {
            let changes = p.get("changes")?.as_array()?;
            let body = changes
                .iter()
                .find(|c| text(c, "field").as_deref() == Some("body"))?;
            let diff = line_diff(
                &text(body, "before").unwrap_or_default(),
                &text(body, "after").unwrap_or_default(),
            );
            let author = author_of(p);
            let (who, mark) = match &author {
                Some(a) => (author_label(a, facts), author_mark(a, facts)),
                None if by_user && text(p, "lead").is_none() => ("You".to_owned(), Mark::You),
                None => (changed_by(e), Mark::Agent),
            };
            let others = fields(
                changes
                    .iter()
                    .filter_map(|c| text(c, "field"))
                    .filter(|f| f != "body"),
            );
            Some(told(
                e,
                format!(
                    "{who} edited the description \u{b7} {}",
                    change_summary(&diff)
                ),
                (!others.is_empty()).then_some(others),
                mark,
                Opens {
                    label: "DESCRIPTION".to_owned(),
                    caption: Some(BEFORE_AFTER.to_owned()),
                    lines: around_changes(diff, 1),
                    report: None,
                },
                Kind::Description,
            ))
        }
        EventKind::CommentPosted => {
            let author = author_of(p)?;
            let excerpt = text(p, "excerpt").unwrap_or_default();
            let report = report_of(p, n, "report");
            let with = p
                .get("report")
                .and_then(|r| r.get("bytes"))
                .and_then(serde_json::Value::as_u64)
                .map(|bytes| format!(" \u{b7} with a report ({})", size_label(bytes)))
                .unwrap_or_default();
            Some(told(
                e,
                format!("{} commented{with}", author_label(&author, facts)),
                Some(quote(&excerpt)),
                author_mark(&author, facts),
                Opens {
                    label: "COMMENT".to_owned(),
                    caption: None,
                    lines: excerpt.lines().map(|l| line(LineKind::Same, l)).collect(),
                    report,
                },
                Kind::Comment,
            ))
        }
        EventKind::CommentEdited => {
            let author = author_of(p)?;
            let (title, mark) = match by_user {
                true => (format!("You edited {} comment", whose(&author)), Mark::You),
                false => (
                    format!("{} edited its comment", author_label(&author, facts)),
                    author_mark(&author, facts),
                ),
            };
            Some(told(
                e,
                title,
                None,
                mark,
                Opens {
                    label: "COMMENT".to_owned(),
                    caption: Some(BEFORE_AFTER.to_owned()),
                    lines: line_diff(
                        &text(p, "before").unwrap_or_default(),
                        &text(p, "after").unwrap_or_default(),
                    ),
                    report: None,
                },
                Kind::Comment,
            ))
        }
        EventKind::CommentDeleted => {
            let author = author_of(p)?;
            let body = text(p, "body").unwrap_or_default();
            let (title, mark) = match by_user {
                true => ("You deleted a comment".to_owned(), Mark::You),
                false => (
                    format!("{} deleted its comment", author_label(&author, facts)),
                    author_mark(&author, facts),
                ),
            };
            Some(told(
                e,
                title,
                Some(quote(&body)),
                mark,
                Opens {
                    label: "COMMENT".to_owned(),
                    caption: None,
                    lines: body.lines().map(|l| line(LineKind::Same, l)).collect(),
                    report: report_of(p, n, "report"),
                },
                Kind::Comment,
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
fn event_row(e: &HistoryEvent) -> Option<HistoryRow> {
    event_row_in(e, &Facts::default())
}

fn event_row_in(e: &HistoryEvent, facts: &Facts) -> Option<HistoryRow> {
    if let Some(row) = comment_row(e, facts) {
        return Some(row);
    }
    let p = &e.payload;
    let first_line = |s: String| s.lines().next().unwrap_or_default().to_owned();
    let (title, detail) = match e.kind {
        // The session row stands for a successful start.
        EventKind::Started | EventKind::AgentEditsSeen => return None,
        // Each drawn by `comment_row`.
        EventKind::CommentPosted | EventKind::CommentEdited | EventKind::CommentDeleted => {
            return None;
        }
        EventKind::Created => ("Created".to_owned(), Some(by(e))),
        // The agent filed another ticket (#47): "Created DEMO-12 Fix the build" · "by DEMO-7's agent · in Backlog".
        EventKind::CreatedTicket => (
            format!(
                "Created {} {}",
                text(p, "child").unwrap_or_default(),
                text(p, "title").unwrap_or_default()
            )
            .trim_end()
            .to_owned(),
            Some(match text(p, "to").and_then(|s| s.parse::<Status>().ok()) {
                Some(to) => format!("{} \u{b7} in {}", who(&e.actor), to.label()),
                None => who(&e.actor),
            }),
        ),
        EventKind::Moved => {
            let to = text(p, "to").and_then(|s| s.parse::<Status>().ok());
            // A child queued by its epic's drag (d0080): "Queued with the epic · by you".
            let title = match to {
                Some(Status::Queued) if p.get("with_epic").is_some() => {
                    "Ready for Dev with the epic".to_owned()
                }
                Some(s) => s.label().to_owned(),
                None => "moved".to_owned(),
            };
            (title, Some(who(&e.actor)))
        }
        EventKind::StartInterrupted => {
            let step = text(p, "step")
                .and_then(|s| s.parse::<StartStep>().ok())
                .map_or("start stopped", StartStep::failure_label);
            (
                "Start failed".to_owned(),
                Some(match text(p, "message") {
                    Some(m) => format!("{step}: {}", first_line(m)),
                    None => step.to_owned(),
                }),
            )
        }
        EventKind::StartRetried => ("Retry start".to_owned(), Some(who(&e.actor))),
        EventKind::Held => ("Held".to_owned(), text(p, "message").map(first_line)),
        EventKind::StartNote => ("Note".to_owned(), text(p, "message").map(first_line)),
        // d0121: "Couldn't update main from origin · started from your local main", git's reason under it.
        EventKind::RemoteUnreachable => (
            serde_json::from_value(p.clone()).map_or_else(
                |_| "Couldn\u{2019}t update the base from its remote".to_owned(),
                |w| super::card::remote_warning_text(&w),
            ),
            text(p, "reason").map(first_line),
        ),
        EventKind::TemplateRefreshFailed => (
            "Template refresh failed".to_owned(),
            text(p, "message").map(first_line),
        ),
        EventKind::AgentRowSet => stage_choices::row_change_line(p),
        EventKind::StageChoiceSet => choice_line(p),
        // FLW-05: "Quick developer is no longer an option for Dev · runs Rust developer".
        EventKind::ChoiceDropped => (
            otaq_core::flow::apply::dropped_choice_line(
                &text(p, "chose").unwrap_or_default(),
                &text(p, "stage").unwrap_or_default(),
                &text(p, "runs").unwrap_or_default(),
            ),
            None,
        ),
        EventKind::FallbackUsed => (stage_choices::fallback_line(p), None),
        EventKind::SchemaOverlap => (
            match text(p, "other") {
                Some(other) => format!("Schema path shared with {other}"),
                None => "Schema path shared".to_owned(),
            },
            text(p, "path"),
        ),
        // An epic lead's edit names it and the fields: "Updated by Wishlist lead · description" (CLI-07).
        EventKind::Updated => match text(p, "lead") {
            Some(lead) => {
                let names = p
                    .get("changes")
                    .and_then(|c| c.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|c| text(c, "field"));
                let changed = fields(names);
                (
                    format!("Updated by {lead}"),
                    (!changed.is_empty()).then_some(changed),
                )
            }
            None => ("Edited".to_owned(), Some(who(&e.actor))),
        },
        EventKind::Pinned => ("Pinned".to_owned(), None),
        EventKind::Unpinned => ("Unpinned".to_owned(), None),
        EventKind::DependencyAdded => (
            "Dependency added".to_owned(),
            text(p, "parent").map(|k| match text(p, "lead") {
                Some(lead) => format!("depends on {k} \u{b7} by {lead}"),
                None => format!("depends on {k}"),
            }),
        ),
        EventKind::DependencyRemoved => (
            "Dependency removed".to_owned(),
            text(p, "parent").map(|k| match text(p, "lead") {
                Some(lead) => format!("no longer depends on {k} \u{b7} by {lead}"),
                None => format!("no longer depends on {k}"),
            }),
        ),
        EventKind::MovedToEpic => ("Moved to epic".to_owned(), text(p, "epic")),
        // `otaq stage done` (AGM-10): the agent's reason.
        EventKind::Closed if p.get("nothing_to_merge").is_some() => (
            "Closed \u{b7} nothing to merge".to_owned(),
            text(p, "reason").map(first_line),
        ),
        EventKind::Closed => ("Closed".to_owned(), Some(who(&e.actor))),
        EventKind::ReviewSubmitted => (
            "Submitted for review".to_owned(),
            text(p, "summary").map(first_line),
        ),
        // An Agent stage's `stage next` into anything but a human check (Q263): "Finished Dev".
        EventKind::StageFinished => (
            format!("Finished {}", text(p, "stage").unwrap_or_default()),
            text(p, "summary").map(first_line),
        ),
        EventKind::Merged => (
            "Merged".to_owned(),
            text(p, "target").map(|t| format!("into {t}")),
        ),
        EventKind::Reopened => ("Reopened".to_owned(), Some(who(&e.actor))),
        // A human check's Pass (FLW-14): "Passed QA" · "by you".
        EventKind::Passed => (
            format!("Passed {}", text(p, "stage").unwrap_or_default()),
            Some(who(&e.actor)),
        ),
        EventKind::Parked => ("Parked".to_owned(), Some(removed(p, who(&e.actor)))),
        // A manual check the user ticked at review (d0125): "Ticked a manual check" · "“Reload a page offline” ·
        // by you".
        EventKind::ManualCheckTicked => (
            match p.get("ticked").and_then(serde_json::Value::as_bool) {
                Some(false) => "Unticked a manual check".to_owned(),
                _ => "Ticked a manual check".to_owned(),
            },
            Some(format!(
                "{} \u{b7} {}",
                quote(&text(p, "text").unwrap_or_default()),
                who(&e.actor)
            )),
        ),
        // A drag to another stage (FLW-18): "Moved back to Dev" · "Grill · run by you", the note, or "by you";
        // "Moved to Review" · "Skipped Validate · by you". Apply now's moves add the stage that went (APN-04):
        // "Moved to Review" · "by you · Security was removed".
        EventKind::Dragged => {
            let stage = text(p, "stage").unwrap_or_default();
            let skipped: Vec<&str> = p
                .get("skipped")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let flag = |field: &str| p.get(field).and_then(|v| v.as_bool()) == Some(true);
            let title = match flag("back") {
                true => format!("Moved back to {stage}"),
                false => format!("Moved to {stage}"),
            };
            let detail = if flag("run_by_you") {
                format!("{stage} \u{b7} run by you")
            } else if !skipped.is_empty() {
                format!("Skipped {} \u{b7} by you", skipped.join(", "))
            } else {
                text(p, "note")
                    .map(first_line)
                    .unwrap_or_else(|| who(&e.actor))
            };
            (title, Some(removed(p, detail)))
        }
        EventKind::Rebased => (
            "Rebased".to_owned(),
            text(p, "onto").map(|b| match p.get("conflict") {
                Some(_) => format!("onto {b}: conflicts, the agent resolves them first"),
                None => format!("onto {b}"),
            }),
        ),
        // A Delivery's conflict went to the agent with restack, no click (FLW-17).
        EventKind::SentToFix => (
            "Merge conflict \u{b7} sent to the agent".to_owned(),
            text(p, "target").map(|t| format!("conflicts with {t}")),
        ),
        EventKind::SentBackWithPrComments => (
            "Sent back with PR comments".to_owned(),
            text(p, "note").filter(|n| !n.is_empty()).map(first_line),
        ),
        // *16 → History · Claude session IDs*: "Interrupted by restart · conversation saved".
        EventKind::Interrupted => match text(p, "reason").as_deref() {
            Some("restart") | None => (
                "Interrupted by restart".to_owned(),
                Some("conversation saved".to_owned()),
            ),
            // The user's Pause (PAU-03, d0112).
            Some(otaq_core::recovery::REASON_PAUSED) => {
                ("Paused".to_owned(), Some("conversation saved".to_owned()))
            }
            // "codex exited (crash) · conversation saved": the session's tool, by its command (Q77).
            Some(reason) => {
                let program = text(p, "tool")
                    .and_then(|t| t.parse::<otaq_core::agent_tools::Tool>().ok())
                    .map_or("the agent", |t| t.command_name());
                (
                    "Interrupted".to_owned(),
                    Some(format!("{program} exited ({reason}) · conversation saved")),
                )
            }
        },
        EventKind::SessionEnded => match text(p, "reason").as_deref() {
            Some(otaq_core::recovery::REASON_PAUSED) => ("Session ended (paused)".to_owned(), None),
            _ => ("Session ended (restart)".to_owned(), None),
        },
        EventKind::Resumed => (
            if p.get("retry").is_some() {
                "Retry resume"
            } else {
                "Resumed"
            }
            .to_owned(),
            Some(format!("same conversation · {}", who(&e.actor))),
        ),
        EventKind::ResumeFailed => (
            "Resume failed".to_owned(),
            Some(match text(p, "tool_session_id") {
                Some(id) => format!("conversation {id} not found"),
                None => "conversation not found".to_owned(),
            }),
        ),
        EventKind::LimitMarked => (
            "Marked as usage limit".to_owned(),
            Some(format!("waiting for limit reset · {}", who(&e.actor))),
        ),
        EventKind::StartedFromScratch => (
            "Started from scratch (resume failed)".to_owned(),
            Some(match text(p, "old_tool_session_id") {
                Some(id) => format!("same worktree · old conversation {id}"),
                None => "same worktree".to_owned(),
            }),
        ),
        EventKind::BranchRenamed => (
            "Branch renamed".to_owned(),
            match (text(p, "from"), text(p, "to")) {
                (Some(from), Some(to)) => Some(format!("{from} \u{2192} {to}")),
                _ => None,
            },
        ),
        EventKind::PrProposed => ("PR proposed".to_owned(), text(p, "branch")),
        EventKind::PrConfirmed => (
            match text(p, "kind").as_deref() {
                Some("push") => "Push confirmed",
                _ => "PR confirmed",
            }
            .to_owned(),
            Some(format!(
                "{} \u{b7} {}",
                text(p, "branch").unwrap_or_default(),
                who(&e.actor)
            )),
        ),
        EventKind::PrCancelled => ("PR cancelled".to_owned(), Some(who(&e.actor))),
        EventKind::QuestionAsked => match (text(p, "to"), text(p, "from")) {
            (Some(to), _) => (format!("Asked {to}"), text(p, "question").map(first_line)),
            (_, Some(from)) => (format!("{from} asked"), text(p, "question").map(first_line)),
            _ => ("Question".to_owned(), text(p, "question").map(first_line)),
        },
        EventKind::QuestionAnswered => (
            match text(p, "with") {
                Some(with) => format!("Question answered \u{b7} {with}"),
                None => "Question answered".to_owned(),
            },
            text(p, "answer").map(first_line),
        ),
        EventKind::MergedAutomatically => (
            otaq_core::domain::validation::MERGED_AUTOMATICALLY.to_owned(),
            text(p, "target").map(
                |t| match p.get("without_checks").and_then(|w| w.as_bool()) {
                    Some(true) => format!(
                        "into {t} \u{b7} {}",
                        otaq_core::domain::validation::DELIVERED_WITHOUT_CHECKS
                    ),
                    _ => format!("into {t}"),
                },
            ),
        ),
        EventKind::AutoMergeStopped => ("Delivery stopped".to_owned(), text(p, "message")),
        // An epic's queue mode (d0080).
        EventKind::EpicQueueModeSet => (
            match text(p, "to")
                .and_then(|m| m.parse::<EpicQueueMode>().ok())
                .unwrap_or_default()
            {
                EpicQueueMode::Whole => "Queued the whole epic",
                EpicQueueMode::Single => "Queued one child",
                EpicQueueMode::None => "Taken back",
            }
            .to_owned(),
            Some(who(&e.actor)),
        ),
        // A new child of a whole-queued epic (EPC-13).
        EventKind::JoinedQueue => (
            "Joined the queue".to_owned(),
            text(p, "epic").map(|epic| format!("{epic} is queued as a whole")),
        ),
        // *05 · Ticket side panel → Row · Recorded decisions (0084)*.
        EventKind::DecisionRecorded => (
            format!(
                "Recorded a decision: {}",
                text(p, "question").unwrap_or_default()
            ),
            Some(decision_detail(p)),
        ),
        // Approve kept them (DEC-05; *05 → Row · Recorded decisions (0084)*).
        EventKind::DecisionsKept => {
            let count = p.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
            let kept: Vec<String> = p
                .get("decisions")
                .and_then(|d| d.as_array())
                .map(|d| {
                    d.iter()
                        .map(|d| {
                            format!(
                                "{} \u{2192} {}",
                                short_question(&text(d, "question").unwrap_or_default()),
                                text(d, "built").unwrap_or_default()
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            (
                format!(
                    "Kept {} \u{b7} {count} decision{}",
                    who(&e.actor),
                    if count == 1 { "" } else { "s" }
                ),
                (!kept.is_empty()).then(|| kept.join(" \u{b7} ")),
            )
        }
        // Change… (DEC-06): sent back, or a fix child in the epic's review.
        EventKind::DecisionChanged => {
            let changed = format!("Changed {}", who(&e.actor));
            match text(p, "child") {
                Some(child) => (
                    format!("{changed} \u{2192} {child}"),
                    Some(format!(
                        "\u{201c}{}\u{201d} \u{b7} added in {}'s review",
                        text(p, "title").unwrap_or_default(),
                        text(p, "epic").unwrap_or_default()
                    )),
                ),
                None => (
                    format!("{changed} \u{2192} sent back"),
                    Some(format!(
                        "\u{201c}{}\u{201d} \u{b7} round {} starts",
                        text(p, "text").unwrap_or_default(),
                        p.get("round").and_then(|r| r.as_u64()).unwrap_or(0)
                    )),
                ),
            }
        }
        // The epic's lead ended at Ready for Review (d0083).
        EventKind::LeadEnded => (
            "Lead ended \u{b7} epic in review".to_owned(),
            text(p, "lead"),
        ),
        EventKind::LeadChanged => lead_changed(p),
        // "Suggested closing SHOP-40.6 · closed by you" (LEAD-08).
        EventKind::LeadSuggestion => {
            let verb = match text(p, "kind").as_deref() {
                Some("delete") => "deleting",
                _ => "closing",
            };
            (
                format!("Suggested {verb} {}", text(p, "child").unwrap_or_default()),
                Some(format!(
                    "{} {}",
                    text(p, "outcome").unwrap_or_default(),
                    who(&e.actor)
                )),
            )
        }
        // A child's question and its epic's lead (LEAD-06), on the child and the epic.
        EventKind::LeadQuestion => lead_question(p),
        // A stuck child and its epic's lead (LEAD-07), on the child and the epic.
        EventKind::LeadStuck => lead_stuck(p),
        // "Marked by Wishlist lead · contradicts PAY-123: “…”" (DEC-04).
        EventKind::DecisionMarked => (
            format!(
                "Marked by {}",
                text(p, "lead").unwrap_or_else(|| "the lead".to_owned())
            ),
            Some(match text(p, "source") {
                Some(source) => format!(
                    "contradicts {source}: \u{201c}{}\u{201d}",
                    text(p, "text").unwrap_or_default()
                ),
                None => format!("\u{201c}{}\u{201d}", text(p, "text").unwrap_or_default()),
            }),
        ),
        // "Note from Wishlist lead" · "to SHOP-40.4: “…”" (LEAD-10), on the child and the epic.
        EventKind::LeadNote => (
            format!(
                "Note from {}",
                text(p, "lead").unwrap_or_else(|| "the lead".to_owned())
            ),
            Some(format!(
                "to {}: \u{201c}{}\u{201d}",
                text(p, "child").unwrap_or_default(),
                text(p, "text").unwrap_or_default()
            )),
        ),
        // One row per validation round; T-0195 opens its report.
        EventKind::ValidationEnded => (
            format!(
                "Validation round {} \u{b7} {}",
                p.get("round").and_then(|r| r.as_u64()).unwrap_or(0),
                text(p, "outcome").unwrap_or_default()
            ),
            None,
        ),
        EventKind::PrOpened => (
            match p.get("number").and_then(|n| n.as_u64()) {
                Some(n) => format!("PR #{n} opened"),
                None => "PR opened".to_owned(),
            },
            text(p, "url"),
        ),
        EventKind::SentBack => {
            let n = p.get("comments").and_then(|v| v.as_u64()).unwrap_or(0);
            let round = p.get("round").and_then(|v| v.as_u64()).unwrap_or(0);
            let plural = if n == 1 { "" } else { "s" };
            (
                "Sent back".to_owned(),
                Some(format!("{n} comment{plural} · round {round}")),
            )
        }
    };
    Some(HistoryRow {
        title,
        detail,
        at: e.at,
        duration: "—".to_owned(),
        tokens: "—".to_owned(),
        tool_session_id: None,
        is_session: false,
        outcome: None,
        session: None,
        mark: None,
        opens: None,
        kind: kind_of(e.kind),
    })
}

/// The named agents in History (AGT-05, d0086): each session's row titled "Session started on Sonnet ·
/// claude-sonnet-5-6" (with "(Sonnet → claude-sonnet-5-6)" when it moved on; with several tools "Session started on
/// Verifier · Copilot · auto", the swap note before its session id), and its end and a Done ticket's agents as lines
/// of their own, newest first like the rest.
pub fn with_agents(mut rows: Vec<HistoryRow>, lines: &[HistoryLine]) -> Vec<HistoryRow> {
    for line in lines {
        match line {
            HistoryLine::Start {
                session,
                title,
                note,
            } => {
                if let Some(row) = rows
                    .iter_mut()
                    .find(|r| r.session.as_ref() == Some(session))
                {
                    row.title = title.clone();
                    // "asked for gpt-5.5, ran on auto · copilot c7f1d0…" (AGT-05).
                    if let Some(note) = note {
                        row.detail = Some(match row.detail.take() {
                            Some(detail) => format!("{note} \u{b7} {detail}"),
                            None => note.clone(),
                        });
                    }
                }
            }
            HistoryLine::Line { at, title, tokens } => rows.push(HistoryRow {
                title: title.clone(),
                detail: None,
                at: *at,
                duration: "\u{2014}".to_owned(),
                tokens: tokens.clone(),
                tool_session_id: None,
                is_session: false,
                outcome: None,
                session: None,
                mark: None,
                opens: None,
                kind: Kind::Stage,
            }),
        }
    }
    rows.sort_by(|a, b| b.at.cmp(&a.at).then(b.is_session.cmp(&a.is_session)));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::agent_tools::Tool;
    use otaq_core::session::SessionState;
    use serde_json::json;

    fn session(id: &str, start: i64, end: Option<i64>, reason: Option<&str>) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            kind: SessionKind::Agent,
            project: "SHOP".into(),
            ticket: Some("SHOP-28".parse().unwrap()),
            state: if end.is_some() {
                SessionState::Ended
            } else {
                SessionState::Running
            },
            started_at: start,
            ended_at: end,
            end_reason: reason.map(str::to_owned),
            tool: Some(Tool::Claude),
            tool_session_id: Some(format!("3be94c10-{id}")),
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            model_ran: None,
            size: None,
        }
    }

    fn event(kind: EventKind, at: i64, payload: serde_json::Value) -> HistoryEvent {
        HistoryEvent {
            kind,
            actor: Actor::User("me".into()),
            at,
            payload,
        }
    }

    const MIN: i64 = 60_000;

    /// #47: the ticket whose agent filed another one says so, under All.
    #[test]
    fn a_ticket_s_agent_created_another() {
        let row = event_row(&HistoryEvent {
            actor: Actor::Agent("DEMO-7".parse().unwrap()),
            ..event(
                EventKind::CreatedTicket,
                MIN,
                json!({"child": "DEMO-12", "title": "Fix the build", "to": "backlog"}),
            )
        })
        .unwrap();
        assert_eq!(row.title, "Created DEMO-12 Fix the build");
        assert_eq!(
            row.detail.as_deref(),
            Some("by DEMO-7's agent \u{b7} in Backlog")
        );
        assert_eq!(row.kind, Kind::Other);
    }

    /// d0121: the warning's own words, git's reason under it.
    #[test]
    fn an_unreachable_remote() {
        let row = event_row(&event(
            EventKind::RemoteUnreachable,
            MIN,
            json!({ "went_on": "started", "branch": "main", "remote": "origin",
                    "reason": "git fetch --quiet origin failed (exit 128): Host key verification failed." }),
        ))
        .unwrap();
        assert_eq!(
            row.title,
            "Couldn\u{2019}t update main from origin \u{b7} started from your local main"
        );
        assert_eq!(
            row.detail.as_deref(),
            Some("git fetch --quiet origin failed (exit 128): Host key verification failed.")
        );
    }

    /// One entry per validation round in place of its bare event, sorted with the rest (VAL-12).
    #[test]
    fn validation_rounds_replace_their_events() {
        use otaq_core::domain::ValidationOutcome;
        use otaq_core::domain::validation::ValidationTrigger;
        let record = ValidationRecord {
            round: 1,
            trigger: ValidationTrigger::Agent,
            outcome: Some(ValidationOutcome::Fail),
            started_at: 20 * MIN,
            ended_at: Some(24 * MIN),
            duration_ms: Some(4 * MIN),
            session: None,
            report: Some("a.ts:1: wrong".into()),
            notes: Vec::new(),
            tokens: None,
        };
        let rows = rows_with_rounds(
            &[
                event(EventKind::Created, 0, json!({})),
                event(
                    EventKind::ValidationEnded,
                    24 * MIN,
                    json!({ "round": 1, "outcome": "fail" }),
                ),
            ],
            &[],
            &[],
            &[record],
            3,
            &Facts::default(),
        );
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["Round 1 · failed", "Created"]);
        assert_eq!(rows[0].outcome, Some((IconName::ShieldX, Palette::Attn)));
        // The verdict is the checker's comment (its own row, with its report): the round opens nothing (CMT-14).
        assert_eq!(rows[0].opens, None);
    }

    fn agent(name: &str, stage: &str, round: Option<u32>, tool: Tool) -> serde_json::Value {
        let mut a = json!({"kind": "agent", "id": name.to_lowercase(), "name": name, "tool": tool, "stage": stage});
        if let Some(r) = round {
            a["round"] = json!(r);
        }
        a
    }

    fn by_agent(kind: EventKind, at: i64, payload: serde_json::Value) -> HistoryEvent {
        HistoryEvent {
            actor: Actor::Agent("SHOP-34".parse().unwrap()),
            ..event(kind, at, payload)
        }
    }

    fn sign(lines: &[DiffLine]) -> Vec<String> {
        lines
            .iter()
            .map(|l| format!("{}{}", l.sign, l.text))
            .collect()
    }

    /// *05 → Row · History* (`s05-45` / `s05-46`), SHOP-34 word for word (the "Done when" sample built as "You edited
    /// the description · 1 line changed", 0067): description edits with their before / after, comments posted (one
    /// with a report), an edit with the original, a deleted comment with what it said and its report.
    #[test]
    fn description_and_comment_lines_read_like_the_frame() {
        let plan = "Admins can export the orders table as CSV, with the filters on screen.";
        let agreed =
            format!("{plan}\n## Agreed\n- CSV only, no Excel\n- admins only\n- up to 10,000 rows");
        let facts = Facts {
            stages: [("grill", "Grill"), ("dev", "Dev"), ("validate", "Validate")]
                .into_iter()
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
                .collect(),
            avatars: true,
        };
        let events = [
            by_agent(
                EventKind::Updated,
                MIN,
                json!({"changes": [{"field": "body", "before": plan, "after": agreed}],
                       "author": agent("Griller", "grill", None, Tool::Claude)}),
            ),
            by_agent(
                EventKind::CommentPosted,
                2 * MIN,
                json!({"n": 1, "excerpt": "Agreed with you: CSV only, admins only. Reuse the CSV writer in export.rs from SHOP-12.",
                       "report": null, "author": agent("Griller", "grill", None, Tool::Claude)}),
            ),
            event(
                EventKind::CommentPosted,
                3 * MIN,
                json!({"n": 2, "excerpt": "Also keep the column order the same as the table on screen.",
                       "report": null, "author": {"kind": "user", "name": "me"}}),
            ),
            by_agent(
                EventKind::CommentPosted,
                4 * MIN,
                json!({"n": 3, "excerpt": "Done. Excel not added; left a TODO where it would go.",
                       "report": null, "author": agent("Rust developer", "dev", None, Tool::Claude)}),
            ),
            by_agent(
                EventKind::CommentPosted,
                5 * MIN,
                json!({"n": 4, "excerpt": "Fail: the 10,000 row limit isn't enforced; 3 tests fail in export_tests.rs.",
                       "report": {"bytes": 48 * 1024, "lines": 2310},
                       "author": agent("Validator", "validate", Some(2), Tool::Codex)}),
            ),
            event(
                EventKind::CommentEdited,
                6 * MIN,
                json!({"n": 3, "before": "Done. Excel not added; left a TODO where it would go.",
                       "after": "Done. Excel not added; left a TODO in export.rs where it would go.",
                       "author": agent("Rust developer", "dev", None, Tool::Claude)}),
            ),
            event(
                EventKind::Updated,
                7 * MIN,
                json!({"changes": [{"field": "body", "before": agreed,
                                    "after": agreed.replace("up to 10,000 rows", "up to 10,000 rows, then a message")}]}),
            ),
            event(
                EventKind::CommentDeleted,
                8 * MIN,
                json!({"n": 5, "body": "Retry with the old writer if this fails.",
                       "report": {"bytes": 6 * 1024, "lines": 120},
                       "author": agent("Rust developer", "dev", None, Tool::Claude)}),
            ),
        ];
        let rows = rows_in(&events, &[], &[], &facts);
        let lines: Vec<_> = rows
            .iter()
            .rev()
            .map(|r| (r.title.as_str(), r.detail.as_deref()))
            .collect();
        assert_eq!(
            lines,
            [
                (
                    "Griller \u{b7} Grill edited the description \u{b7} added Agreed (4 lines)",
                    None
                ),
                (
                    "Griller \u{b7} Grill commented",
                    Some("\u{201c}Agreed with you: CSV only, admins only\u{2026}\u{201d}")
                ),
                (
                    "You commented",
                    Some("\u{201c}Also keep the column order the same as\u{2026}\u{201d}")
                ),
                (
                    "Rust developer \u{b7} Dev commented",
                    Some("\u{201c}Done. Excel not added; left a TODO where\u{2026}\u{201d}")
                ),
                (
                    "Validator \u{b7} Validate \u{b7} round 2 commented \u{b7} with a report (48 KB)",
                    Some("\u{201c}Fail: the 10,000 row limit isn't\u{2026}\u{201d}")
                ),
                ("You edited Rust developer\u{2019}s comment", None),
                ("You edited the description \u{b7} 1 line changed", None),
                (
                    "You deleted a comment",
                    Some("\u{201c}Retry with the old writer if this fails.\u{201d}")
                ),
            ]
        );
        let at = |title: &str| rows.iter().find(|r| r.title.starts_with(title)).unwrap();
        // The description edit opens its before / after: the line before and the four added.
        let grill = at("Griller \u{b7} Grill edited").opens.clone().unwrap();
        assert_eq!(
            (grill.label.as_str(), grill.caption.as_deref()),
            ("DESCRIPTION", Some("Before / after"))
        );
        assert_eq!(
            sign(&grill.lines),
            [
                plan.to_owned(),
                "+## Agreed".into(),
                "+- CSV only, no Excel".into(),
                "+- admins only".into(),
                "+- up to 10,000 rows".into()
            ]
        );
        assert_eq!(
            at("Griller \u{b7} Grill edited").mark,
            Some(Mark::Tool {
                avatar: crate::view_models::tool_avatar::ToolAvatarView::of(Tool::Claude, None)
            })
        );
        // Only the changed line and one around it, with "…" where lines were left out.
        let yours = at("You edited the description").opens.clone().unwrap();
        assert_eq!(
            sign(&yours.lines),
            [
                "- admins only".to_owned(),
                "-- up to 10,000 rows".into(),
                "+- up to 10,000 rows, then a message".into()
            ]
        );
        assert_eq!(at("You edited the description").mark, Some(Mark::You));
        // The edited comment: before / after.
        let edited = at("You edited Rust").opens.clone().unwrap();
        assert_eq!(
            (edited.label.as_str(), edited.caption.as_deref()),
            ("COMMENT", Some("Before / after"))
        );
        assert_eq!(
            sign(&edited.lines),
            [
                "-Done. Excel not added; left a TODO where it would go.",
                "+Done. Excel not added; left a TODO in export.rs where it would go."
            ]
        );
        // Validate's comment and the deleted one open their reports (the deleted one's still opens).
        let validate = at("Validator").opens.clone().unwrap();
        let report = validate.report.unwrap();
        assert_eq!((report.n, report.chip.size.as_str()), (4, "\u{b7} 48 KB"));
        let deleted = at("You deleted").opens.clone().unwrap();
        assert_eq!(
            sign(&deleted.lines),
            ["Retry with the old writer if this fails."]
        );
        assert_eq!(deleted.report.unwrap().n, 5);
        // Never "Done when" (0067).
        assert!(rows.iter().all(|r| !r.title.contains("Done when")));
    }

    /// Activity (SHT-05, `s25-07`): SHOP-22's story oldest first, under All · Comments · Stages · Description.
    #[test]
    fn activity_lists_oldest_first_under_its_filters() {
        let before =
            "Users land on /login again right after signing in, about 1 in 20 times on Safari.";
        let after = format!(
            "{before}\n- the redirect waits for the session cookie\n- e2e/login.spec passes 20 times in a row"
        );
        let facts = Facts {
            stages: [("grill", "Grill"), ("dev", "Dev")]
                .into_iter()
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
                .collect(),
            avatars: false,
        };
        let events = [
            event(EventKind::Created, MIN, json!({})),
            by_agent(
                EventKind::Updated,
                3 * MIN,
                json!({"changes": [{"field": "body", "before": before, "after": after}],
                       "author": agent("Griller", "grill", None, Tool::Claude)}),
            ),
            by_agent(
                EventKind::CommentPosted,
                4 * MIN,
                json!({"n": 1, "excerpt": "Repro: 18 of 20 pass on Safari.", "report": null,
                       "author": agent("Griller", "grill", None, Tool::Claude)}),
            ),
            event(
                EventKind::StageFinished,
                5 * MIN,
                json!({"stage": "grill", "to": "dev"}),
            ),
            event(
                EventKind::CommentPosted,
                6 * MIN,
                json!({"n": 2, "excerpt": "Don't touch the auth middleware's public API.", "report": null,
                       "author": {"kind": "user", "name": "me"}}),
            ),
            event(EventKind::Pinned, 7 * MIN, json!({})),
        ];
        let sessions = [session("dev", 5 * MIN, None, None)];
        let rows = oldest_first(rows_in(&events, &sessions, &[], &facts));
        let kinds: Vec<Kind> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            [
                Kind::Other,
                Kind::Description,
                Kind::Comment,
                Kind::Stage,
                Kind::Stage,
                Kind::Comment,
                Kind::Other
            ]
        );
        assert_eq!(rows[0].title, "Created", "oldest first");
        let titles = |filter| -> Vec<String> {
            filtered(&rows, filter)
                .iter()
                .map(|r| r.title.clone())
                .collect()
        };
        assert_eq!(titles(Filter::All).len(), rows.len());
        assert_eq!(
            titles(Filter::Comments),
            ["Griller \u{b7} Grill commented", "You commented"]
        );
        assert_eq!(
            titles(Filter::Description),
            [
                "Griller \u{b7} Grill edited the description \u{b7} added the redirect waits for the session\u{2026} (2 lines)"
            ]
        );
        assert_eq!(filtered(&rows, Filter::Stages).len(), 2);
        // The description edit opens its before / after.
        let opens = filtered(&rows, Filter::Description)[0]
            .opens
            .clone()
            .unwrap();
        assert_eq!(opens.caption.as_deref(), Some("Before / after"));
        assert_eq!(
            Filter::ALL.map(Filter::label),
            ["All", "Comments", "Stages", "Description"]
        );
    }

    #[test]
    fn other_description_changes_and_who_made_them() {
        let facts = Facts::default();
        let row = |e: HistoryEvent| event_row_in(&e, &facts).unwrap();
        // Lines removed; a lead's edit; another field changed with it; an agent with no author recorded.
        let removed = row(event(
            EventKind::Updated,
            MIN,
            json!({"changes": [{"field": "body", "before": "Plan\n## Out of scope\n- Excel", "after": "Plan"},
                               {"field": "title", "before": "a", "after": "b"}]}),
        ));
        assert_eq!(
            removed.title,
            "You edited the description \u{b7} removed Out of scope (2 lines)"
        );
        assert_eq!(removed.detail.as_deref(), Some("title"));
        let lead = row(by_agent(
            EventKind::Updated,
            MIN,
            json!({"lead": "Wishlist lead", "changes": [{"field": "body", "before": "a\nb", "after": "a\nc\nd"}]}),
        ));
        assert_eq!(
            lead.title,
            "Wishlist lead edited the description \u{b7} 2 lines changed"
        );
        assert_eq!(lead.mark, Some(Mark::Agent));
        // An agent that edits or deletes its own comment.
        let own = row(by_agent(
            EventKind::CommentEdited,
            MIN,
            json!({"n": 3, "before": "a", "after": "b", "author": agent("Rust developer", "dev", None, Tool::Claude)}),
        ));
        assert_eq!(own.title, "Rust developer \u{b7} dev edited its comment");
        assert_eq!(own.mark, Some(Mark::Agent), "avatars hidden");
        let gone = row(by_agent(
            EventKind::CommentDeleted,
            MIN,
            json!({"n": 3, "body": "x", "report": null, "author": agent("Rust developer", "dev", None, Tool::Claude)}),
        ));
        assert_eq!(gone.title, "Rust developer \u{b7} dev deleted its comment");
        assert_eq!(gone.opens.unwrap().report, None);
        // The user's own comment edited.
        let mine = row(event(
            EventKind::CommentEdited,
            MIN,
            json!({"n": 2, "before": "a", "after": "b", "author": {"kind": "user", "name": "me"}}),
        ));
        assert_eq!(mine.title, "You edited your comment");
        // An edit of other fields only stays "Edited · by you".
        let title = row(event(
            EventKind::Updated,
            MIN,
            json!({"changes": [{"field": "title", "before": "a", "after": "b"}]}),
        ));
        assert_eq!((title.title.as_str(), title.opens), ("Edited", None));
    }

    /// An epic's queue mode reads as what the user did (d0080).
    #[test]
    fn epic_queue_modes_read_as_queued_or_taken_back() {
        let rows = rows(
            &[
                event(
                    EventKind::EpicQueueModeSet,
                    MIN,
                    json!({"from": "none", "to": "whole"}),
                ),
                event(
                    EventKind::EpicQueueModeSet,
                    2 * MIN,
                    json!({"from": "whole", "to": "none"}),
                ),
                event(
                    EventKind::EpicQueueModeSet,
                    3 * MIN,
                    json!({"from": "none", "to": "single"}),
                ),
            ],
            &[],
            &[],
        );
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Queued one child", "Taken back", "Queued the whole epic"]
        );
    }

    /// A recorded decision (DEC-02; *05 · Ticket side panel → Row · Recorded decisions (0084)*).
    #[test]
    fn a_recorded_decision_reads_as_the_frame() {
        let rows = rows(
            &[
                event(
                    EventKind::DecisionRecorded,
                    MIN,
                    json!({
                        "ordinal": 1,
                        "question": "Split fullName everywhere, or only in the profile form?",
                        "built": "only in the profile form",
                        "other_options": ["everywhere"],
                        "why": "the description only mentions the profile form",
                    }),
                ),
                event(
                    EventKind::DecisionRecorded,
                    2 * MIN,
                    json!({
                        "ordinal": 2,
                        "question": "Order of saved items?",
                        "built": "newest first",
                        "other_options": ["oldest first", "by price"],
                        "why": "w",
                    }),
                ),
            ],
            &[],
            &[],
        );
        assert_eq!(
            rows[1].title,
            "Recorded a decision: Split fullName everywhere, or only in the profile form?"
        );
        assert_eq!(
            rows[1].detail.as_deref(),
            Some("Built: only in the profile form \u{b7} Other option: everywhere")
        );
        assert_eq!(
            rows[0].detail.as_deref(),
            Some("Built: newest first \u{b7} Other options: oldest first \u{b7} by price")
        );
    }

    /// Approve kept them, and Change… sent back or added a fix child (DEC-05, DEC-06; *05 → Row · Recorded
    /// decisions (0084)*).
    #[test]
    fn kept_and_changed_decisions_read_as_the_frame() {
        let rows = rows(
            &[
                event(
                    EventKind::DecisionsKept,
                    MIN,
                    json!({
                        "count": 1,
                        "decisions": [{
                            "ordinal": 1,
                            "question": "Split fullName everywhere, or only in the profile form?",
                            "built": "only in the profile form",
                        }],
                    }),
                ),
                event(
                    EventKind::DecisionChanged,
                    2 * MIN,
                    json!({
                        "ordinal": 1,
                        "question": "Split fullName everywhere, or only in the profile form?",
                        "to": "sent_back",
                        "text": "Split fullName everywhere",
                        "round": 2,
                    }),
                ),
                event(
                    EventKind::DecisionChanged,
                    3 * MIN,
                    json!({
                        "ordinal": 1,
                        "question": "Order of saved items?",
                        "to": "fix_child",
                        "child": "SHOP-40.7",
                        "title": "Change: list wishlist items in the order they were added",
                        "epic": "SHOP-40",
                    }),
                ),
            ],
            &[],
            &[],
        );
        assert_eq!(rows[2].title, "Kept by you \u{b7} 1 decision");
        assert_eq!(
            rows[2].detail.as_deref(),
            Some("Split fullName \u{2026} \u{2192} only in the profile form")
        );
        assert_eq!(rows[1].title, "Changed by you \u{2192} sent back");
        assert_eq!(
            rows[1].detail.as_deref(),
            Some("\u{201c}Split fullName everywhere\u{201d} \u{b7} round 2 starts")
        );
        assert_eq!(rows[0].title, "Changed by you \u{2192} SHOP-40.7");
        assert_eq!(
            rows[0].detail.as_deref(),
            Some(
                "\u{201c}Change: list wishlist items in the order they were added\u{201d} \u{b7} added in SHOP-40's review"
            )
        );
        assert_eq!(
            short_question("Order of saved items?"),
            "Order of saved items?"
        );
    }

    /// T-0280 (CLI-07, LEAD-08, DEC-04; d0083): the lead's changes on the child and the epic, a suggestion's outcome
    /// and the lead's mark.
    #[test]
    fn the_leads_rows_name_it() {
        let lead_event = |kind: EventKind, payload: serde_json::Value| HistoryEvent {
            actor: "project-agent:l1".parse().unwrap(),
            ..event(kind, MIN, payload)
        };
        let row = |e: HistoryEvent| {
            let rows = rows(&[e], &[], &[]);
            (rows[0].title.clone(), rows[0].detail.clone())
        };
        let cases = [
            (
                lead_event(
                    EventKind::Updated,
                    json!({"lead": "Wishlist lead", "changes": [{"field": "title", "before": "a", "after": "b"}]}),
                ),
                ("Updated by Wishlist lead", Some("title")),
            ),
            // A description change is one of CMT-14's lines, by anyone.
            (
                lead_event(
                    EventKind::Updated,
                    json!({"lead": "Wishlist lead", "changes": [{"field": "body", "before": "a", "after": "b"}]}),
                ),
                (
                    "Wishlist lead edited the description \u{b7} 1 line changed",
                    None,
                ),
            ),
            (
                lead_event(EventKind::Updated, json!({"changes": []})),
                ("Edited", Some("by a Project agent")),
            ),
            (
                lead_event(
                    EventKind::DependencyAdded,
                    json!({"lead": "Wishlist lead", "parent": "SHOP-40.4"}),
                ),
                (
                    "Dependency added",
                    Some("depends on SHOP-40.4 \u{b7} by Wishlist lead"),
                ),
            ),
            (
                lead_event(EventKind::Created, json!({"lead": "Wishlist lead"})),
                ("Created", Some("by Wishlist lead")),
            ),
            (
                lead_event(
                    EventKind::LeadChanged,
                    json!({"lead": "Wishlist lead", "child": "SHOP-40.3", "change": "updated",
                           "fields": ["description", "priority"]}),
                ),
                (
                    "Updated SHOP-40.3",
                    Some("by Wishlist lead \u{b7} description \u{b7} priority"),
                ),
            ),
            (
                lead_event(
                    EventKind::LeadChanged,
                    json!({"lead": "Wishlist lead", "child": "SHOP-40.3", "change": "dep_added",
                           "parent": "SHOP-40.4"}),
                ),
                ("SHOP-40.3 waits for SHOP-40.4", Some("by Wishlist lead")),
            ),
            (
                lead_event(
                    EventKind::LeadChanged,
                    json!({"lead": "Wishlist lead", "child": "SHOP-40.5", "change": "created",
                           "title": "Wishlist empty state"}),
                ),
                (
                    "Added SHOP-40.5 Wishlist empty state",
                    Some("by Wishlist lead"),
                ),
            ),
            (
                event(
                    EventKind::LeadSuggestion,
                    MIN,
                    json!({"lead": "Wishlist lead", "child": "SHOP-40.6", "kind": "close",
                           "outcome": "closed", "reason": "r"}),
                ),
                ("Suggested closing SHOP-40.6", Some("closed by you")),
            ),
            (
                event(
                    EventKind::LeadSuggestion,
                    MIN,
                    json!({"child": "SHOP-40.6", "kind": "delete", "outcome": "dismissed"}),
                ),
                ("Suggested deleting SHOP-40.6", Some("dismissed by you")),
            ),
            (
                lead_event(
                    EventKind::DecisionMarked,
                    json!({"ordinal": 1, "lead": "Wishlist lead", "source": "PAY-123",
                           "text": "items are listed in the order they were added"}),
                ),
                (
                    "Marked by Wishlist lead",
                    Some(
                        "contradicts PAY-123: \u{201c}items are listed in the order they were added\u{201d}",
                    ),
                ),
            ),
            (
                lead_event(
                    EventKind::LeadNote,
                    json!({"lead": "Wishlist lead", "child": "SHOP-40.4",
                           "text": "Use the same order on the shared list."}),
                ),
                (
                    "Note from Wishlist lead",
                    Some("to SHOP-40.4: \u{201c}Use the same order on the shared list.\u{201d}"),
                ),
            ),
            (
                lead_event(
                    EventKind::DecisionMarked,
                    json!({"ordinal": 1, "lead": "Wishlist lead", "text": "the page sorts the other way"}),
                ),
                (
                    "Marked by Wishlist lead",
                    Some("\u{201c}the page sorts the other way\u{201d}"),
                ),
            ),
        ];
        for (e, (title, detail)) in cases {
            assert_eq!(row(e), (title.to_owned(), detail.map(str::to_owned)));
        }
    }

    /// A new child of a whole-queued epic (EPC-13).
    /// T-0406 (Q263, AGM-10): "Finished Grill" and "Finished Dev" for stages that end on their way to anything but
    /// a human check, "Submitted for review" before one, and `stage done`'s "Closed · nothing to merge" with the
    /// reason.
    #[test]
    fn how_stages_ended() {
        let rows = rows(
            &[
                event(
                    EventKind::StageFinished,
                    MIN,
                    json!({"stage": "Grill", "stage_id": "grill", "summary": "Agreed: CSV.\nWith a header."}),
                ),
                event(
                    EventKind::StageFinished,
                    2 * MIN,
                    json!({"stage": "Dev", "stage_id": "dev", "summary": "CSV export."}),
                ),
                event(
                    EventKind::ReviewSubmitted,
                    3 * MIN,
                    json!({"summary": "CSV export, sorted."}),
                ),
                event(
                    EventKind::Closed,
                    4 * MIN,
                    json!({"from": "working", "nothing_to_merge": true, "reason": "Already fixed in SHOP-31."}),
                ),
            ],
            &[],
            &[],
        );
        let lines: Vec<_> = rows
            .iter()
            .map(|r| (r.title.as_str(), r.detail.as_deref()))
            .collect();
        assert_eq!(
            lines,
            [
                (
                    "Closed \u{b7} nothing to merge",
                    Some("Already fixed in SHOP-31.")
                ),
                ("Submitted for review", Some("CSV export, sorted.")),
                ("Finished Dev", Some("CSV export.")),
                ("Finished Grill", Some("Agreed: CSV.")),
            ]
        );
    }

    #[test]
    fn a_joined_child_says_why() {
        let rows = rows(
            &[event(
                EventKind::JoinedQueue,
                MIN,
                json!({"epic": "SHOP-40", "from": "backlog", "to": "queued"}),
            )],
            &[],
            &[],
        );
        assert_eq!(rows[0].title, "Joined the queue");
        assert_eq!(
            rows[0].detail.as_deref(),
            Some("SHOP-40 is queued as a whole")
        );
    }

    /// T-0285 (LEAD-07): a stuck child and its lead, the same rows on the child and the epic.
    #[test]
    fn a_stuck_child_and_its_lead() {
        let row = |step: &str| {
            let payload = json!({"child": "SHOP-40.3", "lead": "Wishlist lead", "stop": "validation_failed",
                                 "reason": "Validation failed 3 times", "step": step});
            let rows = rows(&[event(EventKind::LeadStuck, MIN, payload)], &[], &[]);
            (rows[0].title.clone(), rows[0].detail.clone())
        };
        assert_eq!(
            row("held"),
            (
                "SHOP-40.3 stopped \u{b7} sent to Wishlist lead".to_owned(),
                Some("Validation failed 3 times".to_owned())
            )
        );
        assert_eq!(
            row("noted"),
            (
                "Wishlist lead sent its note to SHOP-40.3".to_owned(),
                Some("Validation failed 3 times".to_owned())
            )
        );
        assert_eq!(
            row("handed_back"),
            (
                "SHOP-40.3's stop handed back to you".to_owned(),
                Some("no note from Wishlist lead".to_owned())
            )
        );
    }

    /// T-0284 (LEAD-06): a child's question and its lead, the same rows on the child and the epic.
    #[test]
    fn a_question_to_the_lead_names_both() {
        let row = |payload: serde_json::Value| {
            let rows = rows(&[event(EventKind::LeadQuestion, MIN, payload)], &[], &[]);
            (rows[0].title.clone(), rows[0].detail.clone())
        };
        let base = json!({"id": "7", "child": "SHOP-40.2", "lead": "Wishlist lead",
                          "question": "Can guests save items without logging in?"});
        let with = |extra: serde_json::Value| {
            let mut p = base.clone();
            p.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            p
        };
        let cases = [
            (
                with(json!({"step": "asked"})),
                (
                    "SHOP-40.2 asked Wishlist lead",
                    Some("\u{201c}Can guests save items without logging in?\u{201d}"),
                ),
            ),
            (
                with(
                    json!({"step": "answered", "answer": "Yes: filled heart means saved.\nMore."}),
                ),
                (
                    "Wishlist lead answered SHOP-40.2",
                    Some("\u{201c}Yes: filled heart means saved.\u{201d}"),
                ),
            ),
            (
                with(
                    json!({"step": "passed_on", "recommended": "Logged-in users only",
                            "reason": "PAY-123 only mentions signed-in users."}),
                ),
                (
                    "Wishlist lead passed SHOP-40.2's question on to you",
                    Some(
                        "recommended \u{201c}Logged-in users only\u{201d}: PAY-123 only mentions signed-in users.",
                    ),
                ),
            ),
            (
                with(
                    json!({"step": "passed_on", "recommended": null, "reason": "Not in the plan."}),
                ),
                (
                    "Wishlist lead passed SHOP-40.2's question on to you",
                    Some("Not in the plan."),
                ),
            ),
            (
                with(json!({"step": "handed_back"})),
                (
                    "SHOP-40.2's question handed back to you",
                    Some("no answer from Wishlist lead"),
                ),
            ),
        ];
        for (payload, (title, detail)) in cases {
            assert_eq!(row(payload), (title.to_owned(), detail.map(str::to_owned)));
        }
    }

    /// The epic's lead ended at Ready for Review (d0083).
    #[test]
    fn the_lead_ending_names_the_review() {
        let rows = rows(
            &[event(
                EventKind::LeadEnded,
                MIN,
                json!({"lead": "Wishlist lead"}),
            )],
            &[],
            &[],
        );
        assert_eq!(rows[0].title, "Lead ended \u{b7} epic in review");
        assert_eq!(rows[0].detail.as_deref(), Some("Wishlist lead"));
    }

    /// A child the epic's drag queued (d0080).
    #[test]
    fn a_child_queued_with_its_epic_says_so() {
        let rows = rows(
            &[event(
                EventKind::Moved,
                MIN,
                json!({"from": "backlog", "to": "queued", "with_epic": "SHOP-45"}),
            )],
            &[],
            &[],
        );
        assert_eq!(rows[0].title, "Ready for Dev with the epic");
        assert_eq!(rows[0].detail.as_deref(), Some("by you"));
    }

    /// Q77: the exit row names the session's tool by its command.
    #[test]
    fn an_exit_names_the_tool() {
        let exit = |payload| {
            rows(&[event(EventKind::Interrupted, MIN, payload)], &[], &[])[0]
                .detail
                .clone()
        };
        assert_eq!(
            exit(json!({"reason": "crash", "session": "s1", "tool": "codex", "from": "working"})),
            Some("codex exited (crash) · conversation saved".to_owned())
        );
        assert_eq!(
            exit(json!({"reason": "exit 1", "session": "s1", "tool": "claude"})),
            Some("claude exited (exit 1) · conversation saved".to_owned())
        );
        assert_eq!(
            exit(json!({"reason": "crash", "session": "s1"})),
            Some("the agent exited (crash) · conversation saved".to_owned())
        );
    }

    #[test]
    fn recovery_events_read_like_the_frame() {
        let rows = rows(
            &[
                event(
                    EventKind::Interrupted,
                    3 * MIN,
                    json!({"reason": "restart", "from": "working"}),
                ),
                event(EventKind::Resumed, 4 * MIN, json!({"from": "interrupted"})),
                event(
                    EventKind::StartedFromScratch,
                    5 * MIN,
                    json!({"tool": "claude", "old_tool_session_id": "3be94c10-x"}),
                ),
                event(
                    EventKind::ResumeFailed,
                    6 * MIN,
                    json!({"tool": "claude", "tool_session_id": "3be94c10-x"}),
                ),
            ],
            &[],
            &[],
        );
        let shown: Vec<(&str, &str)> = rows
            .iter()
            .map(|r| (r.title.as_str(), r.detail.as_deref().unwrap_or_default()))
            .collect();
        assert_eq!(
            shown,
            [
                ("Resume failed", "conversation 3be94c10-x not found"),
                (
                    "Started from scratch (resume failed)",
                    "same worktree · old conversation 3be94c10-x"
                ),
                ("Resumed", "same conversation · by you"),
                ("Interrupted by restart", "conversation saved"),
            ]
        );
    }

    #[test]
    fn a_pause_reads_paused() {
        let rows = rows(
            &[
                event(
                    EventKind::Interrupted,
                    3 * MIN,
                    json!({"reason": "paused", "from": "working"}),
                ),
                event(
                    EventKind::SessionEnded,
                    4 * MIN,
                    json!({"reason": "paused", "session": "s2"}),
                ),
            ],
            &[],
            &[],
        );
        let shown: Vec<(&str, &str)> = rows
            .iter()
            .map(|r| (r.title.as_str(), r.detail.as_deref().unwrap_or_default()))
            .collect();
        assert_eq!(
            shown,
            [
                ("Session ended (paused)", ""),
                ("Paused", "conversation saved"),
            ]
        );
    }

    #[test]
    fn sessions_and_events_newest_first() {
        let sessions = vec![
            session("b", 100 * MIN, None, None),
            session("a", 10 * MIN, Some(82 * MIN), Some("service stopped")),
        ];
        let events = vec![
            event(EventKind::Started, 10 * MIN, json!({})),
            event(
                EventKind::StartInterrupted,
                5 * MIN,
                json!({"step": "setup", "message": "setup command failed (exit 1):\nERR"}),
            ),
            event(
                EventKind::Moved,
                2 * MIN,
                json!({"from": "backlog", "to": "queued"}),
            ),
            event(EventKind::Created, MIN, json!({})),
        ];
        let rows = rows(&events, &sessions, &[]);
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Agent session",
                "Agent session",
                "Start failed",
                "Ready for Dev",
                "Created"
            ]
        );
        assert_eq!(rows[0].duration, "running");
        assert_eq!(rows[1].duration, "72 min");
        assert_eq!(
            rows[1].detail.as_deref(),
            Some("claude 3be94c10-a · service stopped")
        );
        assert_eq!(rows[1].tool_session_id.as_deref(), Some("3be94c10-a"));
        assert_eq!(
            rows[2].detail.as_deref(),
            Some("setup command failed: setup command failed (exit 1):")
        );
        assert_eq!(rows[4].detail.as_deref(), Some("by you"));
        assert!(rows.iter().all(|r| r.tokens == "—"));
    }

    #[test]
    fn durations_round_to_minutes() {
        let r = session_row(
            &session("x", 0, Some(125 * MIN + 30_000), Some("exit 0")),
            &[],
        );
        assert_eq!(r.duration, "2 h 5 min");
        let r = session_row(&session("y", 0, Some(30_000), None), &[]);
        assert_eq!(r.duration, "0 min");
    }

    /// AGM-08: a stage choice's line, on the ticket and (for an epic lead's choice) on the epic.
    #[test]
    fn a_stage_choice_line() {
        let p = serde_json::json!({
            "stage": "Dev", "choice": "Quick developer", "set_by": "project_agent", "by": "the Project agent",
            "reason": "typo fix"
        });
        assert_eq!(
            choice_line(&p),
            (
                "Dev: Quick developer (set by the Project agent)".to_owned(),
                Some("typo fix".to_owned())
            )
        );
        let p = serde_json::json!({
            "stage": "Grill", "choice": "skip", "by": "Wishlist lead", "reason": "docs only", "child": "SHOP-40.3"
        });
        assert_eq!(
            choice_line(&p).0,
            "Wishlist lead set SHOP-40.3's Grill: skip"
        );
    }

    /// FLW-05: a choice the flow no longer offers, dropped when its stage started.
    #[test]
    fn a_dropped_choice_line() {
        let e = event(
            EventKind::ChoiceDropped,
            1,
            serde_json::json!({
                "stage": "Dev", "stage_id": "dev", "chose": "Quick developer", "runs": "Rust developer"
            }),
        );
        let row = event_row(&e).unwrap();
        assert_eq!(
            row.title,
            "Quick developer is no longer an option for Dev \u{b7} runs Rust developer"
        );
        assert_eq!(row.detail, None);
    }

    /// APN-04: Apply now's moves name the stage that went: "Moved to Review" · "by you · Security was removed".
    #[test]
    fn apply_now_moves_name_the_removed_stage() {
        let row = |kind, payload| event_row(&event(kind, 1, payload)).unwrap();
        let moved = row(
            EventKind::Dragged,
            serde_json::json!({ "stage": "Review", "back": false, "skipped": [], "removed": "Security" }),
        );
        assert_eq!(moved.title, "Moved to Review");
        assert_eq!(
            moved.detail.as_deref(),
            Some("by you \u{b7} Security was removed")
        );
        let skipped = row(
            EventKind::Dragged,
            serde_json::json!({ "stage": "QA", "back": false, "skipped": ["Dev"], "removed": "Grill" }),
        );
        assert_eq!(
            skipped.detail.as_deref(),
            Some("Skipped Dev \u{b7} by you \u{b7} Grill was removed")
        );
        let parked = row(EventKind::Parked, serde_json::json!({ "removed": "Grill" }));
        assert_eq!(
            parked.detail.as_deref(),
            Some("by you \u{b7} Grill was removed")
        );
        let dragged = row(
            EventKind::Dragged,
            serde_json::json!({ "stage": "Dev", "back": true }),
        );
        assert_eq!(
            dragged.detail.as_deref(),
            Some("by you"),
            "an ordinary drag says nothing more"
        );
    }

    /// T-0498 (d0125): ticking a manual check at review is a History line with the item.
    #[test]
    fn ticking_a_manual_check_is_a_history_line() {
        let row = |ticked| {
            event_row(&event(
                EventKind::ManualCheckTicked,
                1,
                serde_json::json!({ "n": 2, "text": "Reload a never-visited page offline", "ticked": ticked }),
            ))
            .unwrap()
        };
        let ticked = row(true);
        assert_eq!(ticked.title, "Ticked a manual check");
        assert_eq!(
            ticked.detail.as_deref(),
            Some("\u{201c}Reload a never-visited page offline\u{201d} \u{b7} by you")
        );
        assert_eq!(row(false).title, "Unticked a manual check");
    }
}

//! A ticket's **pieces** while its build stage's agent in charge splits the work among helpers (d0133; canvas `s30-02`,
//! `s30-03`, `s30-04`): one card in its stage with a row per piece, the sheet's Session list and the status beside each
//! piece of the Plan. Statuses are Otaq's own records (the `helpers` table, T-0537), never words in the Plan's text
//! (audit answer 1).
//!
//! - **Rows** ([`rows`]): one per piece, by number: the Plan's pieces and every helper (a fix after a check's Send back
//!   may be a piece the Plan doesn't list, "40.5 · fixing the report"). Each has one status: "merged", "working",
//!   "being reviewed", "sent back: no empty state", "waits for 40.2, 40.3", "waiting for limit reset", "not started",
//!   "was working" after a restart.
//! - **On the card** ([`card`]): only once a helper started (a ticket with no helpers looks as today). "PIECES · 4",
//!   the rows, and past four rows the merged ones fold into one ("3 merged"); "4 pieces merged · moving to Validate"
//!   once every piece is merged; the status line "Working · 2 helpers" (· "1 sent back"), "Working · fixing the
//!   Validate report" with "Back from Validate" after a check's Send back, "Interrupted · 2 helpers were working";
//!   who reads "Developer · in charge". After the build stage the quiet line says "4 pieces merged" and a check reads
//!   "Checking the whole ticket · round 1 of 3".
//! - **In the sheet**: the header "Dev · 2 helpers working", Details "In charge", "Pieces 1 of 4 merged" and the
//!   tokens split ([`tokens_split`]), and the Session list ([`session_list`]).

use otaq_core::board::tickets::Ticket;
use otaq_core::domain::plan::{self, Piece};
use otaq_core::domain::{Status, StatusSet};
use otaq_core::helpers::{Helper, HelperStatus};
use otaq_core::usage::TicketUsage;
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::card::{Tone, tokens_label};

/// The pieces block's label on the card: "PIECES · 4" (the page sets it as written).
pub const PIECES: &str = "PIECES";
/// More rows than this on the card fold the merged ones into one row (d0133, accepted: "3 merged").
pub const CARD_ROWS: usize = 4;
/// "in charge": after the agent's name on the card and in the sheet.
pub const IN_CHARGE: &str = "in charge";
/// Details' label for the agent in charge of a ticket with helpers.
pub const IN_CHARGE_LABEL: &str = "In charge";
/// The column beside the Plan's pieces and its hover (`s30-03`, third pass).
pub fn status_label() -> String {
    format!("Status \u{b7} from {}", otaq_core::naming::PRODUCT_NAME)
}

pub fn status_hover() -> String {
    format!(
        "{}\u{2019}s record of each piece, shown beside the plan. It isn\u{2019}t part of the plan\u{2019}s text, so \
         editing the plan never changes it.",
        otaq_core::naming::PRODUCT_NAME
    )
}

/// Where a piece stands, as Otaq records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "PieceState")]
pub enum State {
    Merged,
    Working,
    BeingReviewed,
    SentBack,
    /// Its tool is at its limit: it starts once the hold lifts.
    Held,
    /// No helper yet; the pieces it waits for aren't merged.
    Waits,
    /// No helper yet, nothing to wait for.
    NotStarted,
    /// A helper that was working when the service stopped (the ticket is Interrupted).
    WasWorking,
    /// The helper fixing what a later check sent back (a piece the Plan doesn't list).
    Fixing,
    /// Its question is with the agent in charge (d0133 audit 3): the card stays Working.
    AskedInCharge,
    /// The agent in charge passed its question on: the user's.
    AsksYou,
    /// Its tool's permission prompt is open: the user answers it in the helper's terminal (d0011, d0016).
    NeedsYou,
}

impl State {
    /// The words' colour: green while it works, amber sent back or held, quiet otherwise.
    pub fn tone(self) -> Tone {
        match self {
            Self::Working | Self::Fixing | Self::AskedInCharge => Tone::Working,
            Self::AsksYou | Self::NeedsYou => Tone::Attention,
            Self::BeingReviewed => Tone::Review,
            Self::SentBack | Self::Held | Self::WasWorking => Tone::Warning,
            Self::Merged => Tone::Passed,
            Self::Waits | Self::NotStarted => Tone::Muted,
        }
    }
}

/// One piece: its number, display name ("40.3"), title and where it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub number: u32,
    pub name: String,
    pub title: String,
    pub state: State,
    /// Its helper, once one started.
    pub helper: Option<Helper>,
    /// Waits: the display names of the pieces not merged yet ("40.2", "40.3").
    pub waits_for: Vec<String>,
}

impl Row {
    /// The status on the card: "merged", "sent back: no empty state", "waits for 40.2, 40.3".
    pub fn card_status(&self) -> String {
        match self.state {
            State::SentBack => match self.note() {
                Some(note) => format!("sent back: {note}"),
                None => "sent back".to_owned(),
            },
            _ => self.status(),
        }
    }

    /// The status in the sheet's Session list: "sent back · fixing", "waits for 40.2, 40.3".
    pub fn status(&self) -> String {
        match self.state {
            State::Merged => "merged".to_owned(),
            State::Working => "working".to_owned(),
            State::BeingReviewed => "being reviewed".to_owned(),
            State::SentBack => "sent back \u{b7} fixing".to_owned(),
            State::Held => HelperStatus::Held.label().to_owned(),
            State::Waits => format!("waits for {}", self.waits_for.join(", ")),
            State::NotStarted => "not started".to_owned(),
            State::WasWorking => "was working".to_owned(),
            State::Fixing => "fixing the report".to_owned(),
            State::AskedInCharge => "asked \u{b7} in charge is answering".to_owned(),
            State::AsksYou => "asks you".to_owned(),
            State::NeedsYou => "needs you \u{b7} a command".to_owned(),
        }
    }

    /// The chip beside the Plan's piece (`s30-03`): "sent back: the empty state is missing · fixing", "merged".
    pub fn plan_status(&self) -> String {
        match (self.state, self.note()) {
            (State::SentBack, Some(note)) => format!("sent back: {note} \u{b7} fixing"),
            _ => self.status(),
        }
    }

    /// The agent in charge's latest note to its helper, first line, without its full stop ("No empty state." reads "no
    /// empty state").
    fn note(&self) -> Option<String> {
        let note = self.helper.as_ref()?.note.as_deref()?;
        let line = note.lines().next()?.trim().trim_end_matches('.');
        (!line.is_empty()).then(|| crate::view_models::card::lowered(line))
    }
}

/// What the rows are made of besides the ticket.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    pub ticket: &'a Ticket,
    /// The ticket's helpers (others' are skipped).
    pub helpers: &'a [Helper],
}

impl<'a> Facts<'a> {
    pub fn new(ticket: &'a Ticket, helpers: &'a [Helper]) -> Self {
        Self { ticket, helpers }
    }

    fn own(&self) -> impl Iterator<Item = &'a Helper> + '_ {
        self.helpers.iter().filter(|h| h.ticket == self.ticket.key)
    }

    /// The ticket split its work: a helper started at least once.
    pub fn split(&self) -> bool {
        self.own().next().is_some()
    }

    fn count(&self, states: &[HelperStatus]) -> usize {
        self.own().filter(|h| states.contains(&h.status)).count()
    }

    /// How many pieces are merged.
    pub fn merged(&self) -> usize {
        self.count(&[HelperStatus::Merged])
    }
}

/// The ticket is back in its build stage from a later check's Send back: that check's name ("Validate").
pub fn back_from(ticket: &Ticket) -> Option<String> {
    let flags = &ticket.flags;
    flags.validation_round?;
    Some(
        flags
            .check
            .clone()
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| "Validate".to_owned()),
    )
}

/// Every piece, by number: the Plan's and every helper's.
pub fn rows(f: &Facts) -> Vec<Row> {
    let ticket = f.ticket;
    let planned: Vec<Piece> = plan::pieces(&ticket.plan);
    let helper = |n: u32| f.own().find(|h| h.piece == n);
    let merged = |n: u32| helper(n).is_some_and(|h| h.status == HelperStatus::Merged);
    let interrupted = ticket.status == Status::Interrupted;
    let back = back_from(ticket).is_some();
    let mut numbers: Vec<u32> = planned
        .iter()
        .map(|p| p.number)
        .chain(f.own().map(|h| h.piece))
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    numbers
        .into_iter()
        .map(|n| {
            let piece = planned.iter().find(|p| p.number == n);
            let h = helper(n);
            let waits_for: Vec<String> = piece
                .map(|p| p.waits_for.as_slice())
                .unwrap_or_default()
                .iter()
                .filter(|w| !merged(**w))
                .map(|w| plan::piece_name(&ticket.key, *w))
                .collect();
            use otaq_core::helpers::questions::HelperQuestionState as Q;
            let asked = h.and_then(|h| h.asked);
            let state = match h.map(|h| h.status) {
                Some(HelperStatus::Merged) => State::Merged,
                // After a restart, what Resume brings back (d0133 audit 5).
                Some(_) if interrupted && h.is_some_and(Helper::resumes) => State::WasWorking,
                // What waits (d0133 audit 3): its prompt, a question passed on, a question with the agent in charge.
                Some(_) if h.is_some_and(|h| h.prompt.is_some()) => State::NeedsYou,
                Some(_) if asked == Some(Q::PassedOn) => State::AsksYou,
                Some(_) if asked == Some(Q::WithInCharge) => State::AskedInCharge,
                Some(HelperStatus::Working) if back && piece.is_none() => State::Fixing,
                Some(HelperStatus::Working) => State::Working,
                Some(HelperStatus::BeingReviewed) => State::BeingReviewed,
                Some(HelperStatus::SentBack) => State::SentBack,
                Some(HelperStatus::Held) => State::Held,
                None if waits_for.is_empty() => State::NotStarted,
                None => State::Waits,
            };
            Row {
                number: n,
                name: plan::piece_name(&ticket.key, n),
                title: h
                    .map(|h| h.title.clone())
                    .or_else(|| piece.map(|p| p.title.clone()))
                    .unwrap_or_default(),
                state,
                helper: h.cloned(),
                waits_for,
            }
        })
        .collect()
}

/// One row on the card (`PieceRowView` on the page): "40.2 · Wishlist button · working".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PieceRowView")]
pub struct RowView {
    /// "40.2"; empty on the folded row.
    pub name: String,
    pub title: String,
    pub status: String,
    pub tone: Tone,
}

/// The card's pieces (`s30-02`, `s30-03`): "PIECES · 4" and a row per piece, or the one line once all are merged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardPiecesView")]
pub struct CardPieces {
    /// "Back from Validate", over the rows, after a check's Send back.
    pub back_from: Option<String>,
    /// "PIECES · 4"; `None` once every piece is merged (the line says it).
    pub label: Option<String>,
    pub rows: Vec<RowView>,
    /// "4 pieces merged · moving to Validate".
    pub done: Option<String>,
    /// A helper's permission prompt is open (d0133 audit 3): the card's action and ⌘J open that helper's terminal
    /// ("40.3"), where the user answers in the tool's own prompt.
    pub opens: Option<String>,
}

fn view(row: &Row) -> RowView {
    RowView {
        name: row.name.clone(),
        title: row.title.clone(),
        status: row.card_status(),
        tone: row.state.tone(),
    }
}

/// "1 piece", "4 pieces".
fn pieces_count(n: usize) -> String {
    match n {
        1 => "1 piece".to_owned(),
        n => format!("{n} pieces"),
    }
}

/// The card's pieces while its build stage runs; `None` before any helper started, or outside the build stage.
/// `next_stage`: the stage after this one ("moving to Validate").
pub fn card(f: &Facts, next_stage: Option<&str>) -> Option<CardPieces> {
    if !f.split() || f.ticket.status.set() != StatusSet::AgentStage {
        return None;
    }
    let rows = rows(f);
    let back = back_from(f.ticket);
    let all_merged = rows.iter().all(|r| r.state == State::Merged);
    if all_merged && back.is_none() {
        let merged = pieces_count(rows.len());
        return Some(CardPieces {
            back_from: None,
            label: None,
            rows: Vec::new(),
            opens: opens(f.ticket),
            done: Some(match next_stage {
                Some(stage) => format!("{merged} merged \u{b7} moving to {stage}"),
                None => format!("{merged} merged"),
            }),
        });
    }
    let merged = rows.iter().filter(|r| r.state == State::Merged).count();
    let fold = rows.len() > CARD_ROWS && merged > 1;
    let mut shown: Vec<RowView> = Vec::new();
    if fold {
        shown.push(RowView {
            name: String::new(),
            title: String::new(),
            status: format!("{merged} merged"),
            tone: Tone::Passed,
        });
    }
    shown.extend(
        rows.iter()
            .filter(|r| !fold || r.state != State::Merged)
            .map(view),
    );
    Some(CardPieces {
        back_from: back.map(|c| format!("Back from {c}")),
        label: Some(format!("{PIECES} \u{b7} {}", rows.len())),
        rows: shown,
        done: None,
        opens: opens(f.ticket),
    })
}

/// The helper waiting on its permission prompt: the terminal the card and the chip open ("40.3").
pub fn opens(ticket: &Ticket) -> Option<String> {
    let wait = ticket.flags.piece_wait.as_ref()?;
    (wait.kind == otaq_core::domain::PieceWaitKind::NeedsYou && ticket.status == Status::NeedsYou)
        .then(|| wait.label.clone())
}

/// The card's line while a helper waits for the user (d0133 audit 3, `s30-02`): "40.2 asks: keep the heart filled
/// after sign-out?", "40.3 needs you: allow pnpm install?"; `None` otherwise.
pub fn wait_line(ticket: &Ticket) -> Option<String> {
    let wait = ticket.flags.piece_wait.as_ref()?;
    matches!(ticket.status, Status::AsksYou | Status::NeedsYou).then(|| wait.line.clone())
}

/// "2 helpers", "1 helper".
fn helpers_count(n: usize) -> String {
    match n {
        1 => "1 helper".to_owned(),
        n => format!("{n} helpers"),
    }
}

/// The card's status line with helpers, in place of "Working" or "Interrupted": "Working · 2 helpers", "Working · 1
/// helper · 1 sent back", "Working · fixing the Validate report", "Interrupted · 2 helpers were working". `None`
/// keeps the card's own line.
pub fn card_line(f: &Facts) -> Option<String> {
    if !f.split() || f.ticket.status.set() != StatusSet::AgentStage {
        return None;
    }
    match f.ticket.status {
        Status::Working => {
            if let Some(check) = back_from(f.ticket) {
                return Some(format!("Working \u{b7} fixing the {check} report"));
            }
            // A question with the agent in charge keeps the card Working (d0133 audit 3).
            let busy = f.count(&[HelperStatus::Working, HelperStatus::BeingReviewed]);
            let sent = f.count(&[HelperStatus::SentBack]);
            let mut parts = vec!["Working".to_owned()];
            if busy > 0 {
                parts.push(helpers_count(busy));
            }
            if sent > 0 {
                parts.push(format!("{sent} sent back"));
            }
            (parts.len() > 1).then(|| parts.join(" \u{b7} "))
        }
        // "Interrupted · 2 helpers were working": the helpers Resume brings back (d0133 audit 5).
        Status::Interrupted => {
            let were = match f.ticket.flags.helpers_to_resume.len() {
                0 => f.own().filter(|h| h.resumes()).count(),
                n => n,
            };
            (were > 0).then(|| otaq_core::helpers::restart::interrupted_line(were))
        }
        _ => None,
    }
}

/// The quiet line after the build stage: "4 pieces merged" (in place of "Plan · 4 pieces"); `None` in the build
/// stage, before a helper merged, or once Done.
pub fn merged_line(f: &Facts) -> Option<String> {
    let set = f.ticket.status.set();
    if matches!(
        set,
        StatusSet::AgentStage | StatusSet::Backlog | StatusSet::Done
    ) {
        return None;
    }
    let merged = f.merged();
    (merged > 0).then(|| format!("{} merged", pieces_count(merged)))
}

/// A check of a ticket that split judges the whole ticket: "Validating · round 1 of 3" reads "Checking the whole
/// ticket · round 1 of 3" (`s30-04`).
pub fn whole_ticket(line: &str) -> String {
    for word in ["Validating", "Checking"] {
        if let Some(rest) = line.strip_prefix(word) {
            return format!("Checking the whole ticket{rest}");
        }
    }
    line.to_owned()
}

/// Who, on the card: "Developer · in charge".
pub fn in_charge(agent: &str) -> String {
    format!("{agent} \u{b7} {IN_CHARGE}")
}

/// Details' "Pieces 1 of 4 merged" (0093: "1 of 4 merged" in a sentence).
pub fn pieces_detail(f: &Facts) -> Option<String> {
    if !f.split() {
        return None;
    }
    let rows = rows(f);
    let merged = rows.iter().filter(|r| r.state == State::Merged).count();
    Some(if merged == rows.len() {
        format!("{merged} merged")
    } else {
        format!("{merged} of {} merged", rows.len())
    })
}

/// Details' tokens of a ticket that split (d0133 audit 4, d0020): "in charge 61k · helpers 120k · no checks yet",
/// "in charge 74k · helpers 134k · checks 23k"; `None` without helper tokens.
pub fn tokens_split(usage: &TicketUsage) -> Option<String> {
    let helpers = usage.helpers?;
    let in_charge = usage.in_charge().map_or(0, |t| t.shown());
    let checks = match usage.validation.map(|t| t.shown()).filter(|t| *t > 0) {
        Some(t) => format!("checks {}", tokens_label(t)),
        None => "no checks yet".to_owned(),
    };
    Some(format!(
        "{IN_CHARGE} {} \u{b7} helpers {} \u{b7} {checks}",
        tokens_label(in_charge),
        tokens_label(helpers.shown())
    ))
}

/// The sheet's header with helpers: "Dev · 2 helpers working" (their sessions run: working, or fixing what was sent
/// back); `None` while none works.
pub fn header(f: &Facts, stage: &str) -> Option<String> {
    if f.ticket.status.set() != StatusSet::AgentStage {
        return None;
    }
    let working = f.count(&[HelperStatus::Working, HelperStatus::SentBack]);
    (working > 0).then(|| format!("{stage} \u{b7} {} working", helpers_count(working)))
}

/// The agent in charge's status in the Session list: "reviewing 40.3", "waiting for 40.2 and 40.3", "was working".
pub fn in_charge_status(f: &Facts) -> String {
    let rows = rows(f);
    let names = |state: &[State]| -> Vec<String> {
        rows.iter()
            .filter(|r| state.contains(&r.state))
            .map(|r| r.name.clone())
            .collect()
    };
    let and = |names: Vec<String>| match names.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    if f.ticket.status == Status::Interrupted {
        return State::WasWorking.status_word().to_owned();
    }
    if f.ticket.status.is_needs_you_family() {
        return "needs you".to_owned();
    }
    let reviewing = names(&[State::BeingReviewed]);
    if !reviewing.is_empty() {
        return format!("reviewing {}", and(reviewing));
    }
    let waiting = names(&[State::Working, State::SentBack, State::Fixing, State::Held]);
    if !waiting.is_empty() {
        return format!("waiting for {}", and(waiting));
    }
    "working".to_owned()
}

impl State {
    fn status_word(self) -> &'static str {
        match self {
            Self::WasWorking => "was working",
            _ => "working",
        }
    }
}

/// One row of the Session list (`s30-03`): the agent in charge first, then a row per piece; each with a helper opens
/// its terminal. `SessionRowView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SessionRowView")]
pub struct SessionRow {
    /// "40.3" for a helper's row; `None` for the agent in charge.
    pub piece: Option<u32>,
    /// "Developer", "40.3".
    pub name: String,
    /// "Wishlist page"; the tool and model under the agent in charge ("Claude Code · sonnet").
    pub title: String,
    pub status: String,
    pub tone: Tone,
    /// Its terminal opens from the row: the agent in charge, and every piece with a helper.
    pub opens: bool,
}

/// The Session list: "SESSIONS · 1 IN CHARGE · 4 HELPERS", "IN CHARGE", "HELPERS" and the rows. `SessionListView` on
/// the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SessionListView")]
pub struct SessionList {
    pub label: String,
    pub in_charge_label: String,
    pub helpers_label: String,
    pub in_charge: SessionRow,
    pub helpers: Vec<SessionRow>,
}

/// The Session list of a ticket that split, from its agent session (the running one, else the newest); `None` without
/// helpers (Session's switch stays as today).
pub fn session_list_of(
    f: &Facts,
    session: Option<&otaq_core::session::SessionInfo>,
) -> Option<SessionList> {
    let agent = session
        .and_then(|s| s.agent.as_ref())
        .map_or("Agent", |a| a.name.as_str());
    let on = session.and_then(|s| {
        let model = s.model_ran.as_deref().or(s.model.as_deref());
        Some(crate::view_models::sheet::model_text(s.tool?, model))
    });
    session_list(f, agent, on.as_deref())
}

/// The Session list of a ticket that split; `None` without helpers (Session's switch stays as today). `agent`: the
/// agent in charge's name, `on`: its tool and model ("Claude Code · sonnet").
pub fn session_list(f: &Facts, agent: &str, on: Option<&str>) -> Option<SessionList> {
    if !f.split() {
        return None;
    }
    let rows = rows(f);
    let started = rows.iter().filter(|r| r.helper.is_some()).count();
    let tone = match f.ticket.status {
        Status::Interrupted => Tone::Warning,
        s if s.is_needs_you_family() => Tone::Attention,
        _ => Tone::Working,
    };
    Some(SessionList {
        label: format!(
            "SESSIONS \u{b7} 1 IN CHARGE \u{b7} {}",
            helpers_count(started).to_uppercase()
        ),
        in_charge_label: "IN CHARGE".to_owned(),
        helpers_label: "HELPERS".to_owned(),
        in_charge: SessionRow {
            piece: None,
            name: agent.to_owned(),
            title: on.unwrap_or_default().to_owned(),
            status: in_charge_status(f),
            tone,
            opens: true,
        },
        helpers: rows
            .iter()
            .map(|r| SessionRow {
                piece: Some(r.number),
                name: r.name.clone(),
                title: r.title.clone(),
                status: r.status(),
                tone: r.state.tone(),
                opens: r.helper.as_ref().is_some_and(|h| h.session.is_some()),
            })
            .collect(),
    })
}

/// The agent in charge's terminal line (`s30-03`): "Developer · in charge · ~/.otaq/wt/shop-web/SHOP-40 · the ticket's
/// branch".
pub fn in_charge_line(agent: &str, worktree: Option<&str>) -> String {
    [
        Some(in_charge(agent)),
        worktree.map(crate::shell::tilde),
        Some("the ticket\u{2019}s branch".to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" \u{b7} ")
}

/// A helper's terminal line (`s30-03`): "Helper for 40.3 · Claude Code · sonnet · its own worktree, otaq/SHOP-40.3".
pub fn helper_line(
    helper: &Helper,
    tool: Option<otaq_core::agent_tools::Tool>,
    model: Option<&str>,
) -> String {
    let model = model
        .filter(|m| !m.is_empty())
        .unwrap_or(crate::view_models::agents::TOOL_DEFAULT);
    [
        Some(format!("Helper for {}", helper.label())),
        tool.map(|t| format!("{} \u{b7} {model}", t.display_name())),
        Some(format!("its own worktree, {}", helper.branch)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" \u{b7} ")
}

/// Who typing reaches in a helper's terminal: "Typing goes to 40.3’s helper · SHOP-40".
pub fn helper_typing(helper: &Helper) -> crate::view_models::session_tab::Typing {
    crate::view_models::session_tab::typing_goes_to(
        &format!("{}\u{2019}s helper", helper.label()),
        &helper.ticket,
    )
}

/// "40.2", "40.2 and 40.3", "40.1, 40.2 and 40.3".
fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// One line of the Resume panel: who comes back and how. `ResumeRowView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ResumeRowView")]
pub struct ResumeRow {
    /// "Developer · in charge", "40.2".
    pub name: String,
    pub title: String,
    /// "told about the restart", "was working".
    pub status: String,
}

/// After a restart, the ticket's one Resume (d0133 audit 5, `s30-02` "After a restart: one item, one Resume"): the
/// agent in charge and each helper that was working, each in its own conversation. `ResumePanelView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ResumePanelView")]
pub struct ResumePanel {
    /// "Resume SHOP-40".
    pub title: String,
    /// "One item for the ticket. Resume continues, each in its own conversation:".
    pub line: String,
    pub rows: Vec<ResumeRow>,
    /// "40.1 is merged and 40.4 hasn't started: the agent in charge starts it when 40.2 and 40.3 are merged."
    pub note: Option<String>,
}

/// The Resume panel of an Interrupted ticket whose helpers were working; `None` otherwise (Resume as today). `agent`:
/// the agent in charge's name.
pub fn resume_panel(f: &Facts, agent: &str) -> Option<ResumePanel> {
    let ticket = f.ticket;
    let back = &ticket.flags.helpers_to_resume;
    if ticket.status != Status::Interrupted || back.is_empty() {
        return None;
    }
    let rows = rows(f);
    let mut out = vec![ResumeRow {
        name: in_charge(agent),
        title: String::new(),
        status: "told about the restart".to_owned(),
    }];
    out.extend(back.iter().map(|label| {
        ResumeRow {
            name: label.clone(),
            title: rows
                .iter()
                .find(|r| &r.name == label)
                .map(|r| r.title.clone())
                .unwrap_or_default(),
            status: State::WasWorking.status_word().to_owned(),
        }
    }));
    let merged: Vec<String> = rows
        .iter()
        .filter(|r| r.state == State::Merged)
        .map(|r| r.name.clone())
        .collect();
    let waiting: Vec<&Row> = rows
        .iter()
        .filter(|r| matches!(r.state, State::Waits | State::NotStarted))
        .collect();
    let mut parts = Vec::new();
    if !merged.is_empty() {
        let verb = if merged.len() == 1 { "is" } else { "are" };
        parts.push(format!("{} {verb} merged", and_list(&merged)));
    }
    if !waiting.is_empty() {
        let names: Vec<String> = waiting.iter().map(|r| r.name.clone()).collect();
        let verb = if names.len() == 1 {
            "hasn\u{2019}t"
        } else {
            "haven\u{2019}t"
        };
        parts.push(format!("{} {verb} started", and_list(&names)));
    }
    let mut note = and_list(&parts);
    let mut waits: Vec<String> = waiting.iter().flat_map(|r| r.waits_for.clone()).collect();
    waits.sort();
    waits.dedup();
    if !waits.is_empty() {
        let it = if waiting.len() == 1 { "it" } else { "them" };
        let verb = if waits.len() == 1 { "is" } else { "are" };
        note.push_str(&format!(
            ": the agent in charge starts {it} when {} {verb} merged",
            and_list(&waits)
        ));
    }
    Some(ResumePanel {
        title: format!("Resume {}", ticket.key),
        line: "One item for the ticket. Resume continues, each in its own conversation:".to_owned(),
        rows: out,
        note: (!note.is_empty()).then(|| format!("{note}.")),
    })
}

/// The status beside one piece of the Plan (`s30-03`): `PlanPieceStatus` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PlanPieceStatus")]
pub struct PlanStatus {
    pub text: String,
    pub tone: Tone,
}

/// The status of piece `number` beside the Plan, once the ticket split; `None` before.
pub fn plan_status(f: &Facts, number: u32) -> Option<PlanStatus> {
    if !f.split() {
        return None;
    }
    rows(f)
        .into_iter()
        .find(|r| r.number == number)
        .map(|r| PlanStatus {
            text: r.plan_status(),
            tone: r.state.tone(),
        })
}

/// One piece's row of the status column: "40.3" and its chip. `PlanStatusRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PlanStatusRow")]
pub struct PlanStatusRow {
    pub name: String,
    pub title: String,
    pub status: PlanStatus,
}

/// The column beside the Plan (`s30-03`, third pass): "Status · from Otaq", the sentence on hover, a chip per piece.
/// `PlanStatusesView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PlanStatusesView")]
pub struct PlanStatuses {
    pub label: String,
    pub hover: String,
    pub rows: Vec<PlanStatusRow>,
}

/// The status column beside the Plan once the ticket split its work; `None` before (d0133 audit answer 1).
pub fn plan_statuses(f: &Facts) -> Option<PlanStatuses> {
    if !f.split() {
        return None;
    }
    Some(PlanStatuses {
        label: status_label(),
        hover: status_hover(),
        rows: rows(f)
            .into_iter()
            .map(|r| PlanStatusRow {
                status: PlanStatus {
                    text: r.plan_status(),
                    tone: r.state.tone(),
                },
                name: r.name,
                title: r.title,
            })
            .collect(),
    })
}

/// Sample helpers for the gallery and the tests: SHOP-40's pieces as `s30-02` and `s30-03` draw them.
pub mod fixtures {
    use otaq_core::domain::TicketKey;
    use otaq_core::helpers::{Helper, HelperStatus};

    /// The helper of `piece` of `key` in `status`.
    pub fn helper(key: &str, piece: u32, title: &str, status: HelperStatus) -> Helper {
        let ticket: TicketKey = key.parse().expect("a key");
        Helper {
            id: format!("h-{key}-{piece}"),
            branch: otaq_core::helpers::branch_name(&ticket, piece),
            worktree: format!("/tmp/wt/shop-web/{key}.{piece}"),
            ticket,
            piece,
            title: title.to_owned(),
            session: Some(format!("s-{key}-{piece}")),
            status,
            note: None,
            held_reason: None,
            port: None,
            started_at: 0,
            updated_at: 0,
            merged_at: (status == HelperStatus::Merged).then_some(0),
            files: Vec::new(),
            prompt: None,
            resume_prompt: None,
            asked: None,
        }
    }

    /// SHOP-40 waiting on a helper (`s30-02`): 40.2's question passed on (`asks`) or 40.3's prompt open, with the
    /// ticket's wait as the service sets it.
    pub fn waiting(asks: bool) -> (otaq_core::domain::PieceWait, Vec<Helper>) {
        use otaq_core::domain::{PieceWait, PieceWaitKind};
        use otaq_core::helpers::questions::HelperQuestionState as Q;
        let mut helpers = mid_way();
        let wait = if asks {
            helpers[1].asked = Some(Q::PassedOn);
            helpers[2].status = HelperStatus::BeingReviewed;
            PieceWait {
                piece: 2,
                label: "40.2".into(),
                kind: PieceWaitKind::Asks,
                session: helpers[1].session.clone(),
                chip: "SHOP-40 \u{b7} 40.2 asks".into(),
                line: "40.2 asks: keep the heart filled after sign-out?".into(),
                note: Some("passed this to you: it\u{2019}s a product question the plan doesn\u{2019}t answer".into()),
            }
        } else {
            helpers[2].prompt = Some("allow pnpm install?".into());
            PieceWait {
                piece: 3,
                label: "40.3".into(),
                kind: PieceWaitKind::NeedsYou,
                session: helpers[2].session.clone(),
                chip: "SHOP-40 \u{b7} 40.3 needs you".into(),
                line: "40.3 needs you: allow pnpm install?".into(),
                note: None,
            }
        };
        (wait, helpers)
    }

    /// SHOP-40 mid-way (`s30-02`): 40.1 merged, 40.2 and 40.3 working; 40.4 has no helper yet.
    pub fn mid_way() -> Vec<Helper> {
        vec![
            helper("SHOP-40", 1, "Wishlist API", HelperStatus::Merged),
            helper("SHOP-40", 2, "Wishlist button", HelperStatus::Working),
            helper("SHOP-40", 3, "Wishlist page", HelperStatus::Working),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{helper, mid_way};
    use super::*;
    use crate::view_models::card_fixtures;
    use crate::view_models::plan::fixtures::WISHLIST;
    use otaq_core::domain::ValidationRound;
    use otaq_core::usage::Tokens;

    fn shop40(status: Status) -> Ticket {
        let mut t = card_fixtures::ticket("SHOP-40", "Wishlist", status);
        t.plan = WISHLIST.to_owned();
        t
    }

    fn statuses(rows: &[RowView]) -> Vec<String> {
        rows.iter()
            .map(|r| {
                format!("{} {} · {}", r.name, r.title, r.status)
                    .trim()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn mid_way_a_row_per_piece_with_one_status() {
        let t = shop40(Status::Working);
        let helpers = mid_way();
        let f = Facts::new(&t, &helpers);
        let card = card(&f, Some("Validate")).unwrap();
        assert_eq!(card.label.as_deref(), Some("PIECES · 4"));
        assert_eq!(
            statuses(&card.rows),
            [
                "40.1 Wishlist API · merged",
                "40.2 Wishlist button · working",
                "40.3 Wishlist page · working",
                "40.4 Share link · waits for 40.2, 40.3",
            ]
        );
        assert_eq!(card_line(&f).as_deref(), Some("Working · 2 helpers"));
        assert_eq!(in_charge("Developer"), "Developer · in charge");
        assert_eq!(card.back_from, None);
        assert_eq!(card.rows[0].tone, Tone::Passed);
        assert_eq!(card.rows[3].tone, Tone::Muted);
    }

    #[test]
    fn early_one_helper_and_the_waits() {
        let t = shop40(Status::Working);
        let helpers = vec![helper("SHOP-40", 1, "Wishlist API", HelperStatus::Working)];
        let f = Facts::new(&t, &helpers);
        let card = card(&f, None).unwrap();
        assert_eq!(
            statuses(&card.rows),
            [
                "40.1 Wishlist API · working",
                "40.2 Wishlist button · waits for 40.1",
                "40.3 Wishlist page · waits for 40.1",
                "40.4 Share link · waits for 40.2, 40.3",
            ]
        );
        assert_eq!(card_line(&f).as_deref(), Some("Working · 1 helper"));
    }

    #[test]
    fn one_reviewed_one_sent_back() {
        let t = shop40(Status::Working);
        let mut helpers = mid_way();
        helpers[1].status = HelperStatus::BeingReviewed;
        helpers[2].status = HelperStatus::SentBack;
        helpers[2].note = Some("No empty state.\nAdd one with a link.".into());
        let f = Facts::new(&t, &helpers);
        let card = card(&f, None).unwrap();
        assert_eq!(
            statuses(&card.rows)[1..3],
            [
                "40.2 Wishlist button · being reviewed",
                "40.3 Wishlist page · sent back: no empty state",
            ]
        );
        assert_eq!(card.rows[2].tone, Tone::Warning);
        assert_eq!(
            card_line(&f).as_deref(),
            Some("Working · 1 helper · 1 sent back")
        );
        // The Session list and the Plan's chip say it in their own words.
        let list = session_list(&f, "Developer", Some("Claude Code · sonnet")).unwrap();
        assert_eq!(list.helpers[2].status, "sent back · fixing");
        assert_eq!(list.in_charge.status, "reviewing 40.2");
        assert_eq!(
            plan_status(&f, 3).unwrap().text,
            "sent back: no empty state · fixing"
        );
    }

    #[test]
    fn past_four_pieces_the_merged_rows_fold() {
        let mut t = card_fixtures::ticket("SHOP-70", "Checkout redesign", Status::Working);
        t.plan = "## Pieces\n1. Cart\n2. Login step\n3. Shipping step\n4. Address step\n5. Payment step\n\
                  6. Order summary — waits for 4, 5\n"
            .to_owned();
        let helpers = vec![
            helper("SHOP-70", 1, "Cart", HelperStatus::Merged),
            helper("SHOP-70", 2, "Login step", HelperStatus::Merged),
            helper("SHOP-70", 3, "Shipping step", HelperStatus::Merged),
            helper("SHOP-70", 4, "Address step", HelperStatus::Working),
            helper("SHOP-70", 5, "Payment step", HelperStatus::Working),
        ];
        let f = Facts::new(&t, &helpers);
        let card = card(&f, None).unwrap();
        assert_eq!(card.label.as_deref(), Some("PIECES · 6"));
        assert_eq!(
            statuses(&card.rows),
            [
                "· 3 merged",
                "70.4 Address step · working",
                "70.5 Payment step · working",
                "70.6 Order summary · waits for 70.4, 70.5",
            ]
        );
        // The sheet lists every piece.
        let list = session_list(&f, "Developer", None).unwrap();
        assert_eq!(list.helpers.len(), 6);
        assert_eq!(list.label, "SESSIONS · 1 IN CHARGE · 5 HELPERS");
    }

    #[test]
    fn four_pieces_never_fold() {
        let t = shop40(Status::Working);
        let mut helpers = mid_way();
        helpers[1].status = HelperStatus::Merged;
        let f = Facts::new(&t, &helpers);
        assert_eq!(card(&f, None).unwrap().rows.len(), 4);
    }

    #[test]
    fn all_merged_moving_on() {
        let t = shop40(Status::Working);
        let helpers: Vec<Helper> = (1..=4)
            .map(|n| helper("SHOP-40", n, "x", HelperStatus::Merged))
            .collect();
        let f = Facts::new(&t, &helpers);
        let card = card(&f, Some("Validate")).unwrap();
        assert_eq!(card.label, None);
        assert!(card.rows.is_empty());
        assert_eq!(
            card.done.as_deref(),
            Some("4 pieces merged · moving to Validate")
        );
        assert_eq!(card_line(&f), None);
        assert_eq!(pieces_detail(&f).as_deref(), Some("4 merged"));
    }

    #[test]
    fn a_ticket_with_no_helpers_looks_as_today() {
        let t = shop40(Status::Working);
        let f = Facts::new(&t, &[]);
        assert_eq!(card(&f, None), None);
        assert_eq!(card_line(&f), None);
        assert_eq!(session_list(&f, "Developer", None), None);
        assert_eq!(plan_status(&f, 1), None);
        assert_eq!(pieces_detail(&f), None);
        // Another ticket's helpers don't count.
        let others = vec![helper("SHOP-41", 1, "x", HelperStatus::Working)];
        assert_eq!(card(&Facts::new(&t, &others), None), None);
    }

    #[test]
    fn back_from_validate_a_fix_is_a_new_piece() {
        let mut t = shop40(Status::Working);
        t.flags.validation_round = Some(ValidationRound { round: 1, of: 3 });
        t.flags.check = Some("Validate".into());
        let mut helpers: Vec<Helper> = (1..=4)
            .map(|n| helper("SHOP-40", n, "x", HelperStatus::Merged))
            .collect();
        helpers.push(helper(
            "SHOP-40",
            5,
            "Share link signed out",
            HelperStatus::Working,
        ));
        let f = Facts::new(&t, &helpers);
        let card = card(&f, Some("Validate")).unwrap();
        assert_eq!(card.back_from.as_deref(), Some("Back from Validate"));
        assert_eq!(card.label.as_deref(), Some("PIECES · 5"));
        assert_eq!(
            statuses(&card.rows),
            [
                "· 4 merged",
                "40.5 Share link signed out · fixing the report"
            ]
        );
        assert_eq!(
            card_line(&f).as_deref(),
            Some("Working · fixing the Validate report")
        );
    }

    #[test]
    fn after_a_restart_the_working_helpers_were_working() {
        let t = shop40(Status::Interrupted);
        let helpers = mid_way();
        let f = Facts::new(&t, &helpers);
        assert_eq!(
            card_line(&f).as_deref(),
            Some("Interrupted · 2 helpers were working")
        );
        let card = card(&f, None).unwrap();
        assert_eq!(card.rows[1].status, "was working");
        // The service's count of what Resume brings back wins.
        let mut counted = shop40(Status::Interrupted);
        counted.flags.helpers_to_resume = vec!["40.2".into(), "40.3".into(), "40.5".into()];
        assert_eq!(
            card_line(&Facts::new(&counted, &helpers)).as_deref(),
            Some("Interrupted · 3 helpers were working")
        );
        assert_eq!(card.rows[2].status, "was working");
        assert_eq!(in_charge_status(&f), "was working");
    }

    #[test]
    fn after_the_build_stage_the_pieces_merged_and_the_whole_ticket() {
        let t = shop40(Status::Checking);
        let helpers: Vec<Helper> = (1..=4)
            .map(|n| helper("SHOP-40", n, "x", HelperStatus::Merged))
            .collect();
        let f = Facts::new(&t, &helpers);
        assert_eq!(card(&f, None), None);
        assert_eq!(merged_line(&f).as_deref(), Some("4 pieces merged"));
        assert_eq!(
            whole_ticket("Validating · round 1 of 3"),
            "Checking the whole ticket · round 1 of 3"
        );
        assert_eq!(whole_ticket("Did not pass"), "Did not pass");
        let working = shop40(Status::Working);
        assert_eq!(merged_line(&Facts::new(&working, &helpers)), None);
    }

    #[test]
    fn the_sheet_header_details_and_tokens() {
        let t = shop40(Status::Working);
        let mut helpers = mid_way();
        helpers[2].status = HelperStatus::SentBack;
        let f = Facts::new(&t, &helpers);
        assert_eq!(
            header(&f, "Dev").as_deref(),
            Some("Dev · 2 helpers working")
        );
        assert_eq!(pieces_detail(&f).as_deref(), Some("1 of 4 merged"));
        let list = session_list(&f, "Developer", Some("Claude Code · sonnet")).unwrap();
        assert_eq!(list.label, "SESSIONS · 1 IN CHARGE · 3 HELPERS");
        assert_eq!(list.in_charge.name, "Developer");
        assert_eq!(list.in_charge.title, "Claude Code · sonnet");
        assert_eq!(list.in_charge.status, "waiting for 40.2 and 40.3");
        let rows: Vec<String> = list
            .helpers
            .iter()
            .map(|r| format!("{} {} · {} · {}", r.name, r.title, r.status, r.opens))
            .collect();
        assert_eq!(
            rows,
            [
                "40.1 Wishlist API · merged · true",
                "40.2 Wishlist button · working · true",
                "40.3 Wishlist page · sent back · fixing · true",
                "40.4 Share link · waits for 40.2, 40.3 · false",
            ]
        );

        let tokens = |input: u64| {
            Some(Tokens {
                input,
                cached: 0,
                output: 0,
                reasoning: None,
            })
        };
        let usage = TicketUsage {
            tokens: tokens(181_000),
            helpers: tokens(120_000),
            ..TicketUsage::default()
        };
        assert_eq!(
            tokens_split(&usage).as_deref(),
            Some("in charge 61k · helpers 120k · no checks yet")
        );
        let checked = TicketUsage {
            tokens: tokens(231_000),
            helpers: tokens(134_000),
            validation: tokens(23_000),
            ..TicketUsage::default()
        };
        assert_eq!(
            tokens_split(&checked).as_deref(),
            Some("in charge 74k · helpers 134k · checks 23k")
        );
        assert_eq!(tokens_split(&TicketUsage::default()), None);
    }

    #[test]
    fn a_helpers_question_and_prompt() {
        use super::fixtures::waiting;
        use otaq_core::helpers::questions::HelperQuestionState as Q;
        // With the agent in charge: the card stays Working, the row says so.
        let t = shop40(Status::Working);
        let mut helpers = mid_way();
        helpers[1].asked = Some(Q::WithInCharge);
        let f = Facts::new(&t, &helpers);
        assert_eq!(card_line(&f).as_deref(), Some("Working · 2 helpers"));
        assert_eq!(
            card(&f, None).unwrap().rows[1].status,
            "asked · in charge is answering"
        );
        assert_eq!(
            card_of(&t, helpers).status.unwrap().text,
            "Working · 2 helpers"
        );

        // Passed on: turquoise, the question on the line, the row "asks you".
        let (wait, helpers) = waiting(true);
        let mut t = shop40(Status::AsksYou);
        t.flags.piece_wait = Some(wait);
        let card = card_of(&t, helpers);
        let line = card.status.unwrap();
        assert_eq!(
            line.text,
            "40.2 asks: keep the heart filled after sign-out?"
        );
        assert_eq!(card.edge, Some(Tone::Attention));
        let pieces = card.pieces.unwrap();
        assert_eq!(pieces.rows[1].status, "asks you");
        assert_eq!(pieces.rows[1].tone, Tone::Attention);
        assert_eq!(pieces.rows[2].status, "being reviewed");
        assert_eq!(pieces.opens, None);

        // A permission prompt: the row "needs you · a command", the card opens 40.3's terminal.
        let (wait, helpers) = waiting(false);
        let mut t = shop40(Status::NeedsYou);
        t.flags.piece_wait = Some(wait);
        let card = card_of(&t, helpers);
        assert_eq!(
            card.status.unwrap().text,
            "40.3 needs you: allow pnpm install?"
        );
        assert_eq!(card.edge, Some(Tone::Attention));
        let pieces = card.pieces.unwrap();
        assert_eq!(pieces.rows[2].status, "needs you · a command");
        assert_eq!(pieces.opens.as_deref(), Some("40.3"));
    }

    #[test]
    fn after_a_restart_one_resume() {
        let mut t = shop40(Status::Interrupted);
        t.flags.helpers_to_resume = vec!["40.2".into(), "40.3".into()];
        let helpers = mid_way();
        let panel = resume_panel(&Facts::new(&t, &helpers), "Developer").unwrap();
        assert_eq!(panel.title, "Resume SHOP-40");
        assert_eq!(
            panel.line,
            "One item for the ticket. Resume continues, each in its own conversation:"
        );
        let rows: Vec<String> = panel
            .rows
            .iter()
            .map(|r| format!("{} {} · {}", r.name, r.title, r.status))
            .collect();
        assert_eq!(
            rows,
            [
                "Developer · in charge  · told about the restart",
                "40.2 Wishlist button · was working",
                "40.3 Wishlist page · was working",
            ]
        );
        assert_eq!(
            panel.note.as_deref(),
            Some(
                "40.1 is merged and 40.4 hasn’t started: the agent in charge starts it when 40.2 and 40.3 are merged."
            )
        );
        // Nothing to bring back but the agent: Resume as today.
        let mut alone = shop40(Status::Interrupted);
        alone.flags.helpers_to_resume.clear();
        assert_eq!(
            resume_panel(&Facts::new(&alone, &helpers), "Developer"),
            None
        );
    }

    #[test]
    fn the_terminals_lines() {
        let helpers = mid_way();
        assert_eq!(
            in_charge_line("Developer", Some("/tmp/wt/shop-web/SHOP-40")),
            "Developer · in charge · /tmp/wt/shop-web/SHOP-40 · the ticket’s branch"
        );
        assert_eq!(
            helper_line(
                &helpers[2],
                Some(otaq_core::agent_tools::Tool::Claude),
                Some("sonnet")
            ),
            format!(
                "Helper for 40.3 · Claude Code · sonnet · its own worktree, {}",
                otaq_core::naming::branch("SHOP-40.3")
            )
        );
        assert_eq!(
            helper_typing(&helpers[2]).focused,
            "Typing goes to 40.3’s helper · SHOP-40"
        );
    }

    #[test]
    fn the_status_beside_the_plan() {
        let t = shop40(Status::Working);
        let helpers = mid_way();
        let f = Facts::new(&t, &helpers);
        assert_eq!(plan_status(&f, 1).unwrap().text, "merged");
        assert_eq!(plan_status(&f, 4).unwrap().text, "waits for 40.2, 40.3");
        assert_eq!(plan_status(&f, 9), None);
        let column = plan_statuses(&f).unwrap();
        let product = otaq_core::naming::PRODUCT_NAME;
        assert_eq!(column.label, format!("Status · from {product}"));
        assert!(
            column
                .hover
                .starts_with(&format!("{product}’s record of each piece"))
        );
        assert_eq!(
            column
                .rows
                .iter()
                .map(|r| format!("{} {}", r.name, r.status.text))
                .collect::<Vec<_>>(),
            [
                "40.1 merged",
                "40.2 working",
                "40.3 working",
                "40.4 waits for 40.2, 40.3"
            ]
        );
        assert_eq!(plan_statuses(&Facts::new(&t, &[])), None);
    }

    fn card_of(t: &Ticket, helpers: Vec<Helper>) -> crate::view_models::card_page::TicketCardView {
        use crate::view_models::card::{AgentFacts, CardExtras};
        let extras = CardExtras {
            helpers,
            next_stage: Some("Validate".into()),
            agents: AgentFacts {
                count: 1,
                running_developer: Some("Developer".into()),
                ..AgentFacts::default()
            },
            ..CardExtras::default()
        };
        crate::view_models::card_page::TicketCardView::of(t, &extras, card_fixtures::NOW)
    }

    #[test]
    fn the_card_in_dev_with_helpers() {
        let t = shop40(Status::Working);
        let card = card_of(&t, mid_way());
        assert_eq!(card.status.unwrap().text, "Working · 2 helpers");
        assert_eq!(card.who.as_deref(), Some("Developer · in charge"));
        let pieces = card.pieces.unwrap();
        assert_eq!(pieces.label.as_deref(), Some("PIECES · 4"));
        assert_eq!(pieces.rows.len(), 4);
        assert_eq!(card.plan.as_deref(), Some("Plan · 4 pieces"));

        // Without helpers the card is today's: "Working", nobody named while there is one agent.
        let alone = card_of(&t, Vec::new());
        assert_eq!(alone.status.unwrap().text, "Working");
        assert_eq!(alone.who, None);
        assert_eq!(alone.pieces, None);
    }

    #[test]
    fn the_card_in_validate_checks_the_whole_ticket() {
        let mut t = shop40(Status::Checking);
        t.flags.validating_round = Some(ValidationRound { round: 1, of: 3 });
        let helpers: Vec<Helper> = (1..=4)
            .map(|n| helper("SHOP-40", n, "x", HelperStatus::Merged))
            .collect();
        let card = card_of(&t, helpers);
        assert_eq!(
            card.status.unwrap().text,
            "Checking the whole ticket · round 1 of 3"
        );
        assert_eq!(card.plan.as_deref(), Some("4 pieces merged"));
        assert_eq!(card.pieces, None);
    }
}

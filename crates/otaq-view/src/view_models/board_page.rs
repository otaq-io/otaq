//! The board as the Tauri page draws it (T-0220, T-0324, T-0404; UI-02, SHL-06, SHL-01, TKT-06, CLS-03, CLS-06,
//! REV-14, STG-08; d0087, d0108): [`board::build`]'s columns of the project's flow (on the default flow today's
//! five) turned into plain, serialized shapes with every text already worded (counts, hints, the Review column's two
//! sections and the Approved note, the Dev section and fold, the folded columns' rails and Done's line, the empty
//! board, the "Created or changed by" chip, the toolbar and the view preferences of SHB-05), each with its kind and
//! each section's own drop target, so the page draws any flow without naming a column. The drag is the UIT-04 way: [`drag_answer_in`] gives every drop target's verdict for one ticket at once
//! (every column, with the Review column split into its body, which means Ready for Review, and its Approved
//! section), from the core's `check_move_in` on the project's flow ([`drag::feedback_in`], [`drag::drop_hint_in`]),
//! so the page draws the whole drag without asking again; a drop then asks once more what happens
//! ([`drop_step_in`]): a move, an epic queue (EPC-12: the epic card, one of its rows or a "not queued" chip, which
//! the page follows with the queue toast), the Park confirmation ([`ParkDialog`]), the approve choice
//! ([`ApproveMenu`]) or, back to an earlier Agent stage or agent check, the drag-back dialog ([`DragBackDialog`],
//! FLW-18). Every stage is a drop target. An epic's track dragged onto Backlog takes back (EPC-14): its waiting
//! children at once, or, when every child started, Park all after its confirmation ([`ParkAllDialog`]).

use std::borrow::Cow;

use otaq_core::board::epic_flight::TakeBackPlan;
use otaq_core::board::moves::{DropOn, MoveAction, check_move_in};
use otaq_core::board::stage_choices::{Choice, StageOptions};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Status, TicketKey};
use otaq_core::flow::engine::Target;
use otaq_core::flow::{Flow, Stage};
use otaq_core::pr::provider::PrProvider;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::Platform;
use crate::view_models::approve::{self, ApproveChoice};
use crate::view_models::board::{
    self, BoardInput, CardEntry, ColumnView, DevSections, HeaderTooltip, SectionView,
};
use crate::view_models::card_page::{EpicCardView, TicketCardView};
use crate::view_models::chain;
use crate::view_models::check_dialogs;
use crate::view_models::close;
use crate::view_models::drag::{self, DropAction, DropFeedback};
use crate::view_models::empty_board;
use crate::view_models::filter::Token;
use crate::view_models::flow_columns::{self, ColumnKind, Columns};
use crate::view_models::list_view;
use crate::view_models::search_page::FilterState;
use crate::view_models::ticket_comments::CommentBoxView;
use crate::view_models::view_prefs::{self, Change, Prefs, Toolbar, ViewKind};
use otaq_core::board::settings::ViewPrefs;

/// A page column's id (SHL-06, STG-08): `"backlog"`, `"ready_for_dev"`, the id of a stage with its own column
/// (the default flow's `"dev"` and `"review"`), `"done"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(rename = "BoardColumnId")]
pub struct ColumnId(#[ts(type = "string")] pub Cow<'static, str>);

impl ColumnId {
    pub const BACKLOG: Self = Self::fixed(flow_columns::BACKLOG);
    pub const READY_FOR_DEV: Self = Self::fixed(flow_columns::READY_FOR_DEV);
    /// The default flow's Dev and Review columns.
    pub const DEV: Self = Self::fixed(otaq_core::flow::default::DEV);
    pub const REVIEW: Self = Self::fixed(otaq_core::flow::default::REVIEW);
    pub const DONE: Self = Self::fixed(flow_columns::DONE);

    const fn fixed(id: &'static str) -> Self {
        Self(Cow::Borrowed(id))
    }

    pub fn new(id: impl Into<String>) -> Self {
        Self(Cow::Owned(id.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for ColumnId {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

/// Where a card can be dropped (d0087, UIT-04, STG-08): a column, or a section of a human check's column after
/// its own. On the default flow `"review"` is the Review column outside its Approved section, which means Ready
/// for Review (only PR rework, from Approved, is allowed there); `"approved"` is the Approved section (from Ready
/// for Review it opens the Approve choice).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(rename = "BoardDropTarget")]
pub struct DropTargetId(#[ts(type = "string")] pub Cow<'static, str>);

impl DropTargetId {
    pub const BACKLOG: Self = Self::fixed(flow_columns::BACKLOG);
    pub const READY_FOR_DEV: Self = Self::fixed(flow_columns::READY_FOR_DEV);
    pub const DEV: Self = Self::fixed(otaq_core::flow::default::DEV);
    pub const REVIEW: Self = Self::fixed(otaq_core::flow::default::REVIEW);
    pub const APPROVED: Self = Self::fixed(flow_columns::APPROVED);
    pub const DONE: Self = Self::fixed(flow_columns::DONE);

    const fn fixed(id: &'static str) -> Self {
        Self(Cow::Borrowed(id))
    }

    pub fn new(id: impl Into<String>) -> Self {
        Self(Cow::Owned(id.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Every target of `flow`'s board, in board order: what a drag answers for.
    pub fn all(flow: &Flow) -> Vec<Self> {
        Columns::of(flow)
            .targets()
            .into_iter()
            .map(Self::new)
            .collect()
    }
}

/// A section of a human check's column (STG-08): on the default flow `"ready_for_review"` or `"approved"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(rename = "BoardReviewSectionId")]
pub struct ReviewSectionId(#[ts(type = "string")] pub Cow<'static, str>);

impl ReviewSectionId {
    pub const READY_FOR_REVIEW: Self = Self(Cow::Borrowed(flow_columns::READY_FOR_REVIEW));
    pub const APPROVED: Self = Self(Cow::Borrowed(flow_columns::APPROVED));
}

/// A card on the board. `needs_you` feeds the "7 above · 6 need you" chip of a scrolled column.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "BoardCard")]
pub enum BoardCard {
    Ticket {
        card: Box<TicketCardView>,
        needs_you: bool,
    },
    Epic {
        card: Box<EpicCardView>,
    },
}

impl BoardCard {
    pub fn key(&self) -> &str {
        match self {
            Self::Ticket { card, .. } => &card.key,
            Self::Epic { card } => &card.key,
        }
    }
}

/// A quiet line under a column's title with its icon (Approved: "Otaq doesn’t watch PRs. Mark it when it
/// lands.").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardNote")]
pub struct Note {
    pub icon: IconName,
    pub text: String,
}

/// Dev's parts: the floated cards (pins and needs-you), then the working cards and the fold.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "BoardDevSections")]
pub struct DevPage {
    pub top: Vec<BoardCard>,
    pub working_count: String,
    pub working: Vec<BoardCard>,
    /// "2 more · working" while folded; a click unfolds.
    pub fold: Option<String>,
    /// The keys behind the fold: the fold chip turns accent when one of them is in the highlighted chain (T-0229).
    pub folded: Vec<String>,
}

/// A section of the Review column (*Row · Review column (0086 · V)*): its header ("READY FOR REVIEW 2"), its cards
/// in that state's order, and a quiet line when empty. Whether it is folded is the page's own; the count stays.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "BoardReviewSection")]
pub struct SectionPage {
    pub id: ReviewSectionId,
    /// The drop target it is, apart from its column (Approved: `"approved"`); `null` for the column's own section.
    pub target: Option<DropTargetId>,
    /// "READY FOR REVIEW", "APPROVED".
    pub title: String,
    /// "2", "1/2" under a filter.
    pub count: String,
    pub cards: Vec<BoardCard>,
    /// "1 hidden by filter".
    pub hidden: Option<String>,
    /// Shown when the section has no card: "Nothing to review", "Nothing approved".
    pub empty: String,
    /// Approved, while a card in it has an open PR: "Otaq doesn’t watch PRs. Mark it when it lands."
    pub note: Option<Note>,
    /// Approved only: shown in place of `empty` while a Ready for Review card is dragged.
    pub drag_hint: Option<String>,
    /// The header's tooltip: "Fold Approved" / "Unfold Approved" (the page picks by its own fold state).
    pub fold: String,
    pub unfold: String,
}

/// A stage folded into an agent column, after its count in the header (FLW-12, Q270): "· Validating 1".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardFoldedStage")]
pub struct FoldedStage {
    /// "Validating".
    pub tag: String,
    pub count: String,
}

/// Who runs a column's stages, on hovering its title (FLW-12): a bold first line and detail lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardColumnTooltip")]
pub struct ColumnTooltip {
    /// "Grill · Griller · grill".
    pub title: String,
    /// "Claude Code · sonnet", "Options: skip".
    pub detail: Vec<String>,
}

impl From<HeaderTooltip> for ColumnTooltip {
    fn from(t: HeaderTooltip) -> Self {
        Self {
            title: t.title,
            detail: t.detail,
        }
    }
}

/// A folded column's rail (BRD-02, `s26-10`): its icon, count and name, a dot when something in it needs the user;
/// a click unfolds it. `BoardColumnRail` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardColumnRail")]
pub struct ColumnRail {
    /// "Grill · 2 tickets, 1 needs you".
    pub tooltip: String,
    /// "Click to unfold".
    pub hint: String,
    /// Something in it needs the user.
    pub dot: bool,
    pub unfold: Change,
}

/// An item of a column's ⋯ menu (BRD-04, FED-09; `s26-08`): Edit <stage>…, Add a stage after <stage>, Fold this
/// column, Change the whole flow…. `BoardColumnMenuItem` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "BoardColumnMenuItem")]
pub struct ColumnMenuItem {
    /// The row the menu draws; its `id` is what a choice gives back.
    pub item: crate::view_models::menu::MenuItem,
    /// The view change it makes; `null` for an item with its own request.
    pub change: Option<Change>,
    /// How the flow editor opens for it (the flow's items); `null` for the others.
    pub editor: Option<OpenEditor>,
}

/// How a column's flow item opens the flow editor over the board (FED-09, d0115), from the flow in use.
/// `BoardColumnEditor` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "open", rename_all = "snake_case")]
#[ts(rename = "BoardColumnEditor")]
pub enum OpenEditor {
    /// Edit <stage>…: that stage selected, its details open.
    Stage { stage: String },
    /// Add a stage after <stage>: a new stage right after `after` (`None`: first, after Ready for Dev), selected
    /// with its details open.
    AddAfter { after: Option<String> },
    /// Change the whole flow…: the editor as it opens from the project menu.
    Flow,
}

pub const FOLD_THIS_COLUMN: &str = "Fold this column";
/// Edit <stage>…'s second line.
pub const EDIT_STAGE_HINT: &str = "agent, skill, options";
pub const CHANGE_THE_WHOLE_FLOW: &str = "Change the whole flow\u{2026}";
/// Change the whole flow…'s second line.
pub const CHANGE_THE_WHOLE_FLOW_HINT: &str = "the same screen as onboarding";
pub const CLICK_TO_UNFOLD: &str = "Click to unfold";
pub const COLUMN_MENU: &str = "Column menu";

/// Done folded (BRD-02, `s24-03`): "Done 4 →" under the last column; a click opens Done as a column. `BoardDoneLine`
/// on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardDoneLine")]
pub struct DoneLine {
    pub text: String,
    pub unfold: Change,
}

/// The pill at the board's right edge while columns are cut off (BRD-02, `s24-04`): the page counts the columns past
/// the edge as it scrolls and words it with `one` or `many`; a click scrolls to the next ones. `BoardMoreStages` on
/// the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardMoreStages")]
pub struct MoreStages {
    pub one: String,
    /// "{n} more stages".
    pub many: String,
    /// At the board's left edge (not scrolled): "7 more stages"; `null` when the columns fit.
    pub at_start: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "BoardColumn")]
pub struct ColumnPage {
    pub id: ColumnId,
    /// What it holds: the look (the icon's ink, the strip's label over Backlog) and the drops (⌘D starts on the
    /// first agent column, Park drops on Backlog) follow it.
    pub kind: ColumnKind,
    pub title: String,
    pub icon: IconName,
    /// The icon in the muted colour (a flow that isn't the default's shape, FLW-12); else in its kind's status
    /// colour.
    pub quiet_icon: bool,
    /// Review: both sections together. An agent column with a folded stage: its own stage's cards.
    pub count: String,
    /// An agent column's folded stages, after its count: "· Validating 1".
    pub folded: Vec<FoldedStage>,
    /// Who runs it, on hovering the title; `None` for Backlog, Ready for Dev and Done.
    pub tooltip: Option<ColumnTooltip>,
    /// Under the title: Ready for Dev's "Starts when ready, in priority order".
    pub hint: Option<String>,
    /// Shown when the column has no card (Review's sections have their own lines).
    pub empty_hint: String,
    /// Folded (BRD-02): a stage's rail ([`ColumnRail`]); Done's is the board's "Done 4 →" line ([`DoneLine`]).
    pub collapsed: bool,
    /// The folded column's rail; `null` while it is open, and for Done.
    pub rail: Option<ColumnRail>,
    /// Its ⋯ menu on hover (BRD-04); Backlog has none.
    pub menu: Vec<ColumnMenuItem>,
    /// The ⋯ button's name ("Column menu").
    pub menu_label: String,
    /// "6 hidden by filter".
    pub hidden: Option<String>,
    /// Every column but Dev and Review.
    pub cards: Vec<BoardCard>,
    /// Review only: Ready for Review, then Approved.
    pub sections: Vec<SectionPage>,
    /// Done only: the epics' folded merged children (EPC-16); each fold's cards follow each other in `cards`.
    pub folds: Vec<DoneFold>,
    pub dev: Option<DevPage>,
    /// Backlog's "Add ticket": the + in its header (and its row); the same as ⌘N.
    pub add: Option<String>,
}

/// An epic's merged children as one line in Done (EPC-16): "Wishlist · 4 of 6 merged". The page draws the line
/// before the first of `keys` and hides those cards until the line is clicked (opened is the page's own, not
/// saved).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardDoneFold")]
pub struct DoneFold {
    pub epic: String,
    pub label: String,
    pub keys: Vec<String>,
}

/// The chip of a column scrolled down: `"{n} above"`, then `" · {n} need you"` when any of them need the user. The
/// page counts the cards above its view and fills in `{n}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardScrollChip")]
pub struct ScrollChip {
    pub above: String,
    pub need_you: String,
}

pub const SCROLL_CHIP_ABOVE: &str = "{n} above";
pub const SCROLL_CHIP_NEED_YOU: &str = "{n} need you";

/// The page's board.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "BoardView")]
pub struct BoardPage {
    pub columns: Vec<ColumnPage>,
    /// The centred card of a project with no tickets (the columns stay behind it).
    pub empty: Option<empty_board::View>,
    pub scroll_chip: ScrollChip,
    /// The chain of every linked card, for the highlight on hover or selection (DEP-05, T-0229).
    pub chains: Vec<chain::View>,
    /// Dependencies: Always (DPV-02): every link between open tickets, drawn dashed where both cards are in view;
    /// empty with On hover.
    pub links: Vec<chain::Edge>,
    /// Lanes: Epic (EPL-01): one lane per epic across `columns`, "No epic" last; `null` with Lanes: None (and in
    /// the List). The columns keep their headers and counts; their cards are the lanes'.
    pub lanes: Option<Vec<crate::view_models::lanes::Lane>>,
    /// The focused epic (an `epic:` filter, EPC-18, Q55): its cards stay, the other cards, tracks and Done lines
    /// dim, and ↑↓ walk only its cards.
    pub focus: Option<EpicFocus>,
    /// The "Created or changed by <agent> ✕" chip while the board is filtered by a Project agent (d0087).
    pub agent_filter: Option<AgentChip>,
    /// The one width rule (BRD-02): every open column at least this wide; they share the width when they fit,
    /// else the board scrolls sideways.
    pub min_column_width: f32,
    /// A folded column's rail.
    pub folded_width: f32,
    /// The "7 more stages" pill's words (BRD-02).
    pub more_stages: MoreStages,
    /// Done folded: "Done 4 →" under the last column.
    pub done: Option<DoneLine>,
    /// The project's view preferences (SHB-05): Board or List, lanes, grouping, folds, dependencies.
    pub prefs: Prefs,
    /// The toolbar of both views (BRD-01).
    pub toolbar: Toolbar,
    /// The List view (LST-01, T-0461); `null` on the Board.
    pub list: Option<list_view::ListView>,
}

/// The chip of the agent filter (d0087; Q-G): ✕ or Esc sets `Params::agent` back to `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardAgentChip")]
pub struct AgentChip {
    /// The Project agent's id, as in `Params::agent`.
    pub agent: String,
    /// "Created or changed by Checkout slowness".
    pub label: String,
    /// The ✕'s tooltip: "Clear filter · Esc".
    pub clear: String,
}

/// What the agent filter keeps: a Project agent's name and the tickets in its "Created and changed" (PAG-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFilter {
    pub agent: String,
    pub name: String,
    pub keys: Vec<TicketKey>,
}

impl AgentFilter {
    /// The filter of the agent `agent` named `name`, from its created and edited tickets.
    pub fn of(
        agent: &str,
        name: &str,
        created: &otaq_core::project_agents::CreatedAndChanged,
    ) -> Self {
        Self {
            agent: agent.to_owned(),
            name: name.to_owned(),
            keys: created
                .created
                .iter()
                .map(|c| c.key.clone())
                .chain(created.edited.iter().cloned())
                .collect(),
        }
    }

    fn chip(&self) -> AgentChip {
        AgentChip {
            agent: self.agent.clone(),
            label: format!("Created or changed by {}", self.name),
            clear: AGENT_FILTER_CLEAR.to_owned(),
        }
    }
}

pub const AGENT_FILTER_CLEAR: &str = "Clear filter · Esc";

/// What a focused epic keeps at full strength (*03 → epic flows 2 → 6 · Focus on Wishlist*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardEpicFocus")]
pub struct EpicFocus {
    /// The focused epics (one, unless a typed `epic:` names several by the same start): their tracks and Done
    /// lines stay.
    pub epics: Vec<String>,
    /// The cards that stay: the epic's own and its children's.
    pub keys: Vec<String>,
}

/// The filter after focusing (`key`: a selected epic, epic child or track, EPC-18) or leaving the focus (`None`,
/// Esc). `None` when nothing changes: a standalone ticket has no epic, and Esc without a focus does nothing.
pub fn focus_filter(
    state: &FilterState,
    key: Option<&TicketKey>,
    tickets: &[Ticket],
) -> Option<FilterState> {
    let mut filter = state.filter();
    match key {
        Some(key) => filter.focus(crate::view_models::filter::epic_of(key, tickets)?),
        None => {
            if !filter.unfocus() {
                return None;
            }
        }
    }
    Some(FilterState {
        tokens: filter.tokens.iter().map(Token::label).collect(),
        text: filter.text,
    })
}

/// What the page sends with `board.view`: the project and the board's size (for the "more stages" pill and Dev's
/// fold). The view preferences are the service's (SHB-05), read by `board.view` itself.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(rename = "BoardParams")]
pub struct Params {
    pub project: String,
    pub width: f32,
    pub height: f32,
    /// The header's search and filter (T-0221): cards that don't match hide, and each column says how many.
    #[serde(default)]
    pub filter: FilterState,
    /// A Project agent's id: only the tickets it created or changed show (d0087, "Show on board" in the pane). It
    /// combines with `filter`; ✕ on its chip or Esc sets it back to `None`.
    #[serde(default)]
    pub agent: Option<String>,
}

/// The project the board shows, for the empty board's wording.
pub struct ProjectFacts<'a> {
    /// The project's flow: the board's columns (STG-08).
    pub flow: &'a Flow,
    pub name: &'a str,
    pub repo_path: &'a str,
    pub base_branch: &'a str,
    /// Its Project agents by id and name (cards name the one that created them).
    pub agent_names: &'a [(String, String)],
    /// Interrupted tickets that were validating when the service restarted (T-0244).
    pub was_validating: &'a [TicketKey],
    /// Its epic leads (`lead.list`): the Lead row on epic cards, the lead's line on tracks (d0083, T-0289).
    pub leads: &'a [otaq_core::project_agents::lead::Lead],
    /// `Params::agent`'s filter, read from the service; `None` without one (or when the agent is gone).
    pub agent_filter: Option<&'a AgentFilter>,
    /// The cards' named agents (`board.agents`, T-0345): the footer's agent part.
    pub agents: Option<&'a otaq_core::agents::facts::BoardAgents>,
    /// Every agent (`agents.list`): the column headers' tooltips name each stage's tool and model (FLW-12).
    pub agent_list: Option<&'a otaq_core::agents::AgentList>,
    /// The starts a waiting flow change holds back (FLW-06, FED-08): its cards say
    /// [`board::WAITING_FOR_THE_FLOW_CHANGE`]; `None` while starts run.
    pub starts_held: Option<&'a otaq_core::flow::apply::Hold>,
    /// The project's view preferences (`ProjectSettings.view`, SHB-05).
    pub view: &'a ViewPrefs,
    /// The project is paused (PAU-03, d0112).
    pub paused: bool,
}

pub const APPROVED_NOTE: &str =
    "doesn\u{2019}t watch PRs. Mark it once it\u{2019}s merged on GitHub.";

fn card(entry: CardEntry, tickets: &[Ticket]) -> Option<BoardCard> {
    Some(match entry {
        CardEntry::Ticket(vm) => {
            let ticket = tickets.iter().find(|t| t.key == vm.key)?;
            BoardCard::Ticket {
                needs_you: ticket.needs_user(),
                card: Box::new(TicketCardView::from_vm(vm, ticket)),
            }
        }
        CardEntry::Epic(vm) => BoardCard::Epic {
            card: Box::new(EpicCardView::from_vm(vm)),
        },
    })
}

fn cards(entries: Vec<CardEntry>, tickets: &[Ticket]) -> Vec<BoardCard> {
    entries
        .into_iter()
        .filter_map(|e| card(e, tickets))
        .collect()
}

fn hidden_text(n: usize) -> Option<String> {
    (n > 0).then(|| format!("{n} hidden by filter"))
}

fn section_page(view: SectionView, tickets: &[Ticket]) -> SectionPage {
    let approved = view.delivery;
    let pr_open = view.cards.iter().any(|c| match c {
        CardEntry::Ticket(vm) => tickets
            .iter()
            .any(|t| t.key == vm.key && t.status == Status::PrOpen),
        CardEntry::Epic(_) => false,
    });
    let name = view.name;
    SectionPage {
        id: view.id,
        target: view.target,
        title: view.title.to_owned(),
        count: view.count,
        hidden: hidden_text(view.hidden),
        empty: view.empty.to_owned(),
        note: (approved && pr_open).then(|| Note {
            icon: IconName::Info,
            text: format!("{} {APPROVED_NOTE}", otaq_core::naming::PRODUCT_NAME),
        }),
        drag_hint: view.drag_hint.map(str::to_owned),
        fold: format!("Fold {name}"),
        unfold: format!("Unfold {name}"),
        cards: cards(view.cards, tickets),
    }
}

pub(crate) fn column_page(view: ColumnView, tickets: &[Ticket]) -> ColumnPage {
    let kind = view.kind;
    let dev = view.dev.map(|d: DevSections| DevPage {
        top: cards(d.top, tickets),
        working_count: d.working_count.to_string(),
        working: cards(d.working, tickets),
        fold: d.fold,
        folded: d.folded.iter().map(ToString::to_string).collect(),
    });
    let rail = (view.collapsed && kind != ColumnKind::Done).then(|| ColumnRail {
        tooltip: rail_tooltip(&view.title, view.total, view.needs_you),
        hint: CLICK_TO_UNFOLD.to_owned(),
        dot: view.needs_you > 0,
        unfold: Change::FoldColumn {
            column: view.id.as_str().to_owned(),
            folded: false,
        },
    });
    let menu = column_menu(&view.id, kind, &view.title);
    ColumnPage {
        id: view.id,
        kind,
        title: view.title,
        icon: view.icon,
        quiet_icon: view.quiet_icon,
        count: view.count,
        folded: match kind {
            ColumnKind::Agent => view
                .folded
                .into_iter()
                .map(|f| FoldedStage {
                    tag: f.tag,
                    count: f.count,
                })
                .collect(),
            _ => Vec::new(),
        },
        tooltip: view.tooltip.map(ColumnTooltip::from),
        hint: view.hint.map(str::to_owned),
        empty_hint: view.empty_hint.to_owned(),
        collapsed: view.collapsed,
        rail,
        menu,
        menu_label: COLUMN_MENU.to_owned(),
        hidden: hidden_text(view.hidden),
        cards: cards(view.cards, tickets),
        sections: view
            .sections
            .into_iter()
            .map(|s| section_page(s, tickets))
            .collect(),
        folds: view
            .folds
            .into_iter()
            .map(|f| DoneFold {
                epic: f.epic.to_string(),
                label: f.label,
                keys: f.keys.iter().map(ToString::to_string).collect(),
            })
            .collect(),
        dev,
        add: (kind == ColumnKind::Backlog).then(|| "Add ticket".to_owned()),
    }
}

/// "Grill · 2 tickets, 1 needs you" (`s26-10`).
pub fn rail_tooltip(title: &str, tickets: usize, needs_you: usize) -> String {
    let s = if tickets == 1 { "" } else { "s" };
    match needs_you {
        0 => format!("{title} \u{b7} {tickets} ticket{s}"),
        n => format!("{title} \u{b7} {tickets} ticket{s}, {n} needs you"),
    }
}

/// A column's ⋯ menu (BRD-04, FED-09; `s26-08`), on every column but Backlog: **Edit <stage>…** (a stage's own
/// column), **Add a stage after <stage>** (not after the Delivery, which nothing follows), **Fold this column**, then
/// **Change the whole flow…**. `title` is the column's ("Done · PR" names its stage "Done").
pub fn column_menu(id: &ColumnId, kind: ColumnKind, title: &str) -> Vec<ColumnMenuItem> {
    use crate::view_models::menu::MenuItem;
    if kind == ColumnKind::Backlog {
        return Vec::new();
    }
    let stage = match kind {
        ColumnKind::Delivery => title.split(" \u{b7} ").next().unwrap_or(title),
        _ => title,
    };
    let opens = |item: MenuItem, editor: OpenEditor| ColumnMenuItem {
        item,
        change: None,
        editor: Some(editor),
    };
    let mut menu = Vec::new();
    if matches!(
        kind,
        ColumnKind::Agent | ColumnKind::HumanCheck | ColumnKind::Delivery
    ) {
        menu.push(opens(
            MenuItem::new("edit_stage", format!("Edit {stage}\u{2026}"))
                .icon(IconName::Pencil)
                .hint(EDIT_STAGE_HINT),
            OpenEditor::Stage {
                stage: id.as_str().to_owned(),
            },
        ));
    }
    if matches!(
        kind,
        ColumnKind::ReadyForDev | ColumnKind::Agent | ColumnKind::HumanCheck
    ) {
        menu.push(opens(
            MenuItem::new("add_stage", format!("Add a stage after {stage}")).icon(IconName::Plus),
            OpenEditor::AddAfter {
                after: (kind != ColumnKind::ReadyForDev).then(|| id.as_str().to_owned()),
            },
        ));
    }
    menu.push(ColumnMenuItem {
        item: MenuItem::new("fold", FOLD_THIS_COLUMN).icon(IconName::FoldHorizontal),
        change: Some(Change::FoldColumn {
            column: id.as_str().to_owned(),
            folded: true,
        }),
        editor: None,
    });
    menu.push(opens(
        MenuItem {
            separated: true,
            ..MenuItem::new("change_flow", CHANGE_THE_WHOLE_FLOW)
                .icon(IconName::Workflow)
                .hint(CHANGE_THE_WHOLE_FLOW_HINT)
        },
        OpenEditor::Flow,
    ));
    menu
}

/// Puts each epic card's lead on it (d0083): the Lead row, Open lead, and "added by Wishlist lead" rows.
pub(crate) fn with_leads(
    columns: &mut [ColumnView],
    tickets: &[Ticket],
    leads: &[otaq_core::project_agents::lead::Lead],
) {
    for column in columns {
        let dev = column
            .dev
            .iter_mut()
            .flat_map(|d| d.top.iter_mut().chain(d.working.iter_mut()));
        let sections = column.sections.iter_mut().flat_map(|s| s.cards.iter_mut());
        for entry in column.cards.iter_mut().chain(dev).chain(sections) {
            let CardEntry::Epic(vm) = entry else { continue };
            let Some(epic) = tickets.iter().find(|t| t.key == vm.key) else {
                continue;
            };
            let lead = leads.iter().find(|l| l.epic == vm.key);
            vm.set_lead(epic, tickets, lead);
        }
    }
}

/// The board for `tickets` of one project, at `params`' size.
pub fn page(
    tickets: &[Ticket],
    project: &ProjectFacts,
    params: &Params,
    now_ms: i64,
    keys: &KeyLabels,
) -> BoardPage {
    let filter = params.filter.filter();
    // An `epic:` token dims instead of hiding (Q55).
    let mut visible = filter.hiding().visible(tickets);
    // The agent filter (d0087) narrows whatever the search shows.
    let agent_filter = project
        .agent_filter
        .filter(|f| params.agent.as_deref() == Some(f.agent.as_str()));
    if let Some(agent) = agent_filter {
        visible = Some(
            visible
                .unwrap_or_else(|| tickets.iter().map(|t| t.key.clone()).collect())
                .into_iter()
                .filter(|k| agent.keys.contains(k))
                .collect(),
        );
    }
    let strong = filter.strong(tickets);
    let prefs = project.view;
    let input = BoardInput {
        flow: project.flow,
        tickets,
        visible: visible.as_deref(),
        strong: strong.as_deref(),
        project_base: Some(project.base_branch),
        now_ms,
        dev_budget: (!prefs.dev_unfolded).then(|| board::dev_budget(params.height)),
        folded: &prefs.folded_columns,
        fold_empty: prefs.fold_empty_columns,
        agent_names: project.agent_names,
        was_validating: project.was_validating,
        agents: project.agents,
        agent_list: project.agent_list,
        starts_held: project.starts_held,
        paused: project.paused,
    };
    let view: ViewKind = prefs.view.into();
    let list = (view == ViewKind::List).then(|| list_view::view(&input, prefs, project.leads));
    let mut built = board::build(&input);
    with_leads(&mut built, tickets, project.leads);
    let view: ViewKind = prefs.view.into();
    let lanes = (view == ViewKind::Board && prefs.lanes == otaq_core::board::settings::Lanes::Epic)
        .then(|| crate::view_models::lanes::build(&input, prefs, project.leads));
    // Folded Done is a line under the last column, not a column of its own.
    let rails: Vec<bool> = built
        .iter()
        .filter(|c| !(c.kind == ColumnKind::Done && c.collapsed))
        .map(|c| c.collapsed)
        .collect();
    let done = built
        .iter()
        .find(|c| c.kind == ColumnKind::Done && c.collapsed)
        .map(|c| DoneLine {
            text: format!("{} {} \u{2192}", c.title, c.count),
            unfold: Change::FoldColumn {
                column: c.id.as_str().to_owned(),
                folded: false,
            },
        });
    BoardPage {
        lanes,
        columns: built.into_iter().map(|c| column_page(c, tickets)).collect(),
        empty: empty_board::shows(tickets.len(), false)
            .then(|| empty_board::view(project.name, keys)),
        scroll_chip: ScrollChip {
            above: SCROLL_CHIP_ABOVE.to_owned(),
            need_you: SCROLL_CHIP_NEED_YOU.to_owned(),
        },
        chains: chain::views(
            &crate::view_models::flow_columns::Columns::of(project.flow),
            tickets,
        ),
        // Always (DPV-02): every open link drawn, quiet and dashed.
        links: if prefs.dependencies == otaq_core::board::settings::Dependencies::Always {
            crate::view_models::dependencies::links(tickets)
        } else {
            Vec::new()
        },
        focus: strong.map(|keys| EpicFocus {
            epics: filter
                .focused_epics(tickets)
                .iter()
                .map(ToString::to_string)
                .collect(),
            keys: keys.iter().map(ToString::to_string).collect(),
        }),
        agent_filter: agent_filter.map(AgentFilter::chip),
        min_column_width: board::COLUMN_MIN_WIDTH,
        folded_width: board::FOLDED_COLUMN_WIDTH,
        more_stages: MoreStages {
            one: "1 more stage".to_owned(),
            many: "{n} more stages".to_owned(),
            at_start: board::more_stages(board::columns_after(params.width, 0., &rails)),
        },
        done,
        prefs: Prefs::from(prefs),
        toolbar: view_prefs::toolbar(prefs, keys),
        list,
    }
}

/// How a column looks while the card is over it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "DragVerdict")]
pub enum Verdict {
    /// The card's own column: no styling, dropping does nothing.
    Home,
    /// Accent outline and the hint under the pointer, with a second line under it when there is one ("SHOP-45.1
    /// keeps running").
    Allowed {
        icon: IconName,
        hint: String,
        note: Option<String>,
    },
    /// Red outline, a ban cursor and the reason; dropping does nothing.
    Forbidden {
        title: String,
        detail: Option<String>,
    },
}

/// One drop target's verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DropTargetVerdict")]
pub struct TargetVerdict {
    pub target: DropTargetId,
    pub verdict: Verdict,
}

/// The answer to `board.drag`, asked once when a drag starts: a verdict for every drop target (every column,
/// Review's Approved section on its own). `tickets` are the project's: an epic's answer counts the children it
/// queues ("Queue 2 children") or, for its track over Backlog, what it takes back; other tickets need none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DragAnswer")]
pub struct DragAnswer {
    pub key: String,
    pub targets: Vec<TargetVerdict>,
}

/// [`drag_answer_in`] on the default flow.
pub fn drag_answer(ticket: &Ticket, tickets: &[Ticket]) -> DragAnswer {
    drag_answer_in(&otaq_core::flow::default_file().flow, ticket, tickets, None)
}

/// The drag answer on `flow`'s board (the project's): one verdict per drop target, in board order. Every stage takes
/// a drop (FLW-18; "Moving a card here isn't available yet" went with Q271). `left` is the stage a parked ticket
/// left, by name, when known.
pub fn drag_answer_in(
    flow: &Flow,
    ticket: &Ticket,
    tickets: &[Ticket],
    left: Option<&str>,
) -> DragAnswer {
    let columns = Columns::of(flow);
    DragAnswer {
        key: ticket.key.to_string(),
        targets: DropTargetId::all(flow)
            .into_iter()
            .filter_map(|target| {
                let column = columns.drop_target(target.as_str())?;
                let verdict = verdict(flow, ticket, tickets, column, left);
                Some(TargetVerdict { target, verdict })
            })
            .collect(),
    }
}

fn verdict(
    flow: &Flow,
    ticket: &Ticket,
    tickets: &[Ticket],
    column: DropOn,
    left: Option<&str>,
) -> Verdict {
    match drag::feedback_in(flow, ticket, tickets, &column) {
        DropFeedback::Home => Verdict::Home,
        DropFeedback::Forbidden { title, detail } => Verdict::Forbidden {
            title,
            detail: detail.map(str::to_owned),
        },
        DropFeedback::Allowed => {
            let (icon, hint) = drag::drop_hint_in(flow, ticket, tickets, &column, left)
                .unwrap_or((IconName::Check, String::new()));
            Verdict::Allowed {
                icon,
                hint,
                note: drag::drop_note(ticket, tickets, column),
            }
        }
    }
}

/// One thing a confirmation keeps: "otaq/SHOP-25", "worktree · kept with its files".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ConfirmRow")]
pub struct Row {
    pub icon: IconName,
    pub text: String,
}

/// *03 → Park · Dev → Backlog (allowed)*: "Park SHOP-25?", what stops and what is kept (CLS-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ParkDialog")]
pub struct ParkDialog {
    pub key: String,
    pub title: String,
    pub message: String,
    pub kept: Vec<Row>,
    pub confirm: String,
    pub confirm_icon: IconName,
}

pub fn park_dialog(ticket: &Ticket) -> ParkDialog {
    let text = close::park_text(ticket);
    ParkDialog {
        key: ticket.key.to_string(),
        title: text.title,
        message: text.message,
        kept: text
            .kept
            .into_iter()
            .map(|(icon, text)| Row { icon, text })
            .collect(),
        confirm: "Park".to_owned(),
        confirm_icon: IconName::SquareParking,
    }
}

/// *03 → epic flows 4 → 11*: "Park 2 running tickets?", built like the Park confirmation (d0052); Cancel is the
/// default, the confirm button sends `board.park_all` for the epic (EPC-14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ParkAllDialog")]
pub struct ParkAllDialog {
    /// The epic.
    pub key: String,
    pub title: String,
    pub message: String,
    pub cancel: String,
    pub confirm: String,
    pub confirm_icon: IconName,
}

/// The Park-all confirmation of a take-back `plan`; `None` unless it parks.
pub fn park_all_dialog(plan: &TakeBackPlan) -> Option<ParkAllDialog> {
    let confirmation = plan.confirmation()?;
    Some(ParkAllDialog {
        key: plan.epic().to_string(),
        title: confirmation.title,
        message: confirmation.body,
        cancel: confirmation.cancel,
        confirm: confirmation.confirm,
        confirm_icon: IconName::SquareParking,
    })
}

/// One choice of the approve popover, with its number key ("1 Local merge").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ApproveItem")]
pub struct ApproveItem {
    pub choice: ApproveChoice,
    pub number: u32,
    pub icon: IconName,
    pub title: String,
    pub detail: String,
    pub enabled: bool,
}

/// *03 → Approve choice on drop*: a drop on Approved never approves by itself; it asks how (UI-02, T-0095).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ApproveMenu")]
pub struct ApproveMenu {
    pub key: String,
    pub title: String,
    pub items: Vec<ApproveItem>,
    pub note: String,
}

fn choice_icon(choice: ApproveChoice) -> IconName {
    match choice {
        ApproveChoice::LocalMerge => IconName::GitMerge,
        ApproveChoice::OpenPr => IconName::GitPullRequest,
        ApproveChoice::PushToPr => IconName::Upload,
    }
}

pub fn approve_menu(
    ticket: &Ticket,
    target: &str,
    tickets: &[Ticket],
    pr: Option<&PrProvider>,
) -> ApproveMenu {
    ApproveMenu {
        key: ticket.key.to_string(),
        title: approve::menu_title(ticket),
        items: approve::options(ticket, target, tickets, pr)
            .into_iter()
            .zip(1..)
            .map(|(o, number)| ApproveItem {
                choice: o.choice,
                number,
                icon: choice_icon(o.choice),
                title: o.title,
                detail: o.detail,
                enabled: o.enabled,
            })
            .collect(),
        note: approve::menu_note(ticket),
    }
}

/// What a drop on `column` asks of the app, before anything runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropStep {
    /// Its own column, or refused: the card flies back.
    Nothing,
    /// Send `ticket.move`.
    Move,
    /// Send `epic.queue` for the epic, or for one `child` of it (a row or a "not queued" chip), then show the
    /// queue toast with its outcome (EPC-12, EPC-15).
    QueueEpic {
        epic: TicketKey,
        child: Option<TicketKey>,
    },
    /// Dev → Backlog: the Park confirmation first.
    Park,
    /// An epic's track on Backlog: send `epic.take_back`, which unqueues its waiting children (EPC-14).
    TakeBack {
        epic: TicketKey,
    },
    /// An epic's track on Backlog with every child started: the Park-all confirmation first.
    ParkAll(ParkAllDialog),
    /// Ready for Review → the Approved section: the Approve choice (Local merge · Open PR); nothing moves until one
    /// is picked, and Esc or Cancel leaves the card where it was (d0087).
    Approve,
    /// Back to an earlier Agent stage or agent check (FLW-18): the drag-back dialog first ([`drag_back_dialog`]);
    /// Cancel leaves the card where it was.
    DragBack,
    NotYet(String),
    /// A blocked ticket dropped past Ready for Dev, or started (DPV-03): DPV-03's dialog. From Backlog it is queued in
    /// Ready for Dev first (`queue`), where it waits; there is no way to start it.
    Blocked {
        dialog: crate::view_models::dependencies::Dialog,
        queue: bool,
    },
}

/// The core's drop target a page target means on `flow`'s board; `None` for an id the board doesn't have.
pub fn core_target(flow: &Flow, target: &DropTargetId) -> Option<DropOn> {
    Columns::of(flow).drop_target(target.as_str())
}

/// [`drop_step_in`] on the default flow.
pub fn drop_step(ticket: &Ticket, tickets: &[Ticket], target: DropTargetId) -> DropStep {
    drop_step_in(
        &otaq_core::flow::default_file().flow,
        ticket,
        tickets,
        &target,
    )
}

/// `tickets` are the project's; only an epic's need them (its track's take back). `flow` is the project's.
pub fn drop_step_in(
    flow: &Flow,
    ticket: &Ticket,
    tickets: &[Ticket],
    target: &DropTargetId,
) -> DropStep {
    let Some(target) = core_target(flow, target) else {
        return DropStep::Nothing;
    };
    if otaq_core::board::moves::blocks_start(flow, ticket, tickets, &target) {
        return DropStep::Blocked {
            dialog: crate::view_models::dependencies::dialog(&Columns::of(flow), ticket, tickets),
            queue: ticket.place == otaq_core::domain::Place::Backlog,
        };
    }
    match drag::on_drop_in(flow, ticket, tickets, target) {
        DropAction::Nothing => DropStep::Nothing,
        DropAction::Move => DropStep::Move,
        DropAction::QueueEpic { epic, child } => DropStep::QueueEpic { epic, child },
        DropAction::AskPark => DropStep::Park,
        DropAction::TakeBack { epic } => DropStep::TakeBack { epic },
        DropAction::AskParkAll(plan) => match park_all_dialog(&plan) {
            Some(dialog) => DropStep::ParkAll(dialog),
            None => DropStep::Nothing,
        },
        DropAction::ChooseApprove => DropStep::Approve,
        DropAction::AskDragBack => DropStep::DragBack,
        DropAction::NotYet(message) => DropStep::NotYet(message),
    }
}

/// *03 → Dragging a card back to an earlier stage* (s03-50/s03-51, FLW-18, CMT-13): "Send SHOP-19 back to Dev?" with a
/// comment for that stage ("Comment for Dev (Rust developer)"), or "Run Security again on SHOP-18?" with a comment for
/// the check; either is posted as a comment every later stage reads, in the comment box (a long paste becomes its
/// report). When an agent
/// works where the card is now, the dialog says it stops ("Security reviewer is working on it now. It stops, and the
/// check runs again after Dev.") and the button reads "Stop and send to Dev"; otherwise it says where the card goes
/// after ("Then it goes on to Security, Review and Done as usual."). Cancel leaves the card where it was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DragBackDialog")]
pub struct DragBackDialog {
    pub key: String,
    /// Where it was dropped: what the confirm sends back (`board.drag_back`).
    pub target: DropTargetId,
    pub title: String,
    pub stops: Option<String>,
    /// "Comment for Dev (Rust developer)", "Comment for Security (Security reviewer)", with "Posted as a comment for
    /// Dev · every later stage reads it".
    pub comment: CommentBoxView,
    pub then: Option<String>,
    pub cancel: String,
    pub confirm: String,
    pub confirm_icon: IconName,
}

/// The drag-back dialog of `ticket` dropped on `target` of `flow`'s board; `choices` are its stage choices (the agent
/// it picked for a stage, the stages it skips). `None` unless the drop asks for one.
pub fn drag_back_dialog(
    flow: &Flow,
    ticket: &Ticket,
    target: &DropTargetId,
    choices: &[StageOptions],
) -> Option<DragBackDialog> {
    let on = core_target(flow, target)?;
    let action = check_move_in(flow, ticket, &on).action()?;
    let Some(Target::Stage(to)) = on.in_flow(flow) else {
        return None;
    };
    let here = ticket.place.stage().and_then(|id| flow.stage(id));
    let working = here.filter(|s| {
        (s.is_agent_stage() && agent_works(ticket.status))
            || (s.is_agent_check()
                && matches!(ticket.status, Status::Checking | Status::CheckerNeedsYou))
    });
    let agent = |s: &Stage| agent_of(s, choices);
    let named = |s: &Stage| match agent(s) {
        Some(agent) => format!("{} ({agent})", s.name),
        None => s.name.clone(),
    };
    let key = ticket.key.to_string();
    let stops = working.map(|here| {
        let who = agent(here).unwrap_or_else(|| here.name.clone());
        let after = match (here.is_agent_check(), action) {
            (true, MoveAction::SendBack) => format!("the check runs again after {}", to.name),
            (false, MoveAction::SendBack) => format!("{} runs again after {}", here.name, to.name),
            _ => format!("{} runs again with your note", to.name),
        };
        format!("{who} is working on it now. It stops, and {after}.")
    });
    Some(match action {
        MoveAction::SendBack => DragBackDialog {
            key: key.clone(),
            target: target.clone(),
            title: format!("Send {key} back to {}?", to.name),
            comment: CommentBoxView::dialog(
                format!("Comment for {}", named(to)),
                check_dialogs::posted_for(&to.name),
                true,
            )
            .placeholder(check_dialogs::WHAT_SHOULD_CHANGE),
            then: stops
                .is_none()
                .then(|| then_line(flow, to, choices))
                .flatten(),
            cancel: "Cancel".to_owned(),
            confirm: match stops {
                Some(_) => format!("Stop and send to {}", to.name),
                None => format!("Send to {}", to.name),
            },
            confirm_icon: IconName::Undo2,
            stops,
        },
        MoveAction::RunCheckAgain => DragBackDialog {
            key: key.clone(),
            target: target.clone(),
            title: format!("Run {} again on {key}?", to.name),
            // No placeholder drawn for a check (`s03-50`).
            comment: CommentBoxView::dialog(
                format!("Comment for {}", named(to)),
                check_dialogs::posted_for(&to.name),
                true,
            )
            .placeholder(""),
            then: None,
            cancel: "Cancel".to_owned(),
            confirm: match stops {
                Some(_) => format!("Stop and run {}", to.name),
                None => format!("Run {}", to.name),
            },
            confirm_icon: IconName::RotateCcw,
            stops,
        },
        _ => return None,
    })
}

/// An agent works in the card's Agent stage now (the drag back stops it).
fn agent_works(status: Status) -> bool {
    matches!(
        status,
        Status::Working
            | Status::WorkingInBackground
            | Status::AsksYou
            | Status::NeedsYou
            | Status::MergeConflictFixing
    )
}

/// The agent a stage runs for the ticket: the one it picked, else the stage's own.
fn agent_of(stage: &Stage, choices: &[StageOptions]) -> Option<String> {
    let picked = choices
        .iter()
        .find(|c| c.stage_id == stage.id)
        .and_then(|c| c.choice.as_ref())
        .and_then(|c| match &c.choice {
            Choice::Agent { name, .. } => Some(name.clone()),
            Choice::Usual | Choice::Skip => None,
        });
    picked.or_else(|| stage.agent.clone())
}

/// "Then it goes on to Security, Review and Done as usual.": the stages after `to` the ticket runs.
fn then_line(flow: &Flow, to: &Stage, choices: &[StageOptions]) -> Option<String> {
    let skipped = |s: &Stage| choices.iter().any(|c| c.stage_id == s.id && c.skipped());
    let index = flow.position(&to.id)?;
    let names: Vec<&str> = flow.stages[index + 1..]
        .iter()
        .filter(|s| !skipped(s))
        .map(|s| s.name.as_str())
        .collect();
    let list = match names.as_slice() {
        [] => return None,
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    Some(format!("Then it goes on to {list} as usual."))
}

/// The answer to `board.drop`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "DropOutcome")]
pub enum DropOutcome {
    /// Nothing happened: the card flies back.
    Nothing,
    /// The service carried out the move; the board refetches on its event.
    Moved,
    /// The page shows the confirmation, then sends `board.park`.
    Park { dialog: ParkDialog },
    /// The page shows the Park-all confirmation (Cancel by default), then sends `board.park_all`.
    ParkAll { dialog: ParkAllDialog },
    /// The page shows the popover; a choice opens its dialog.
    Approve { menu: ApproveMenu },
    /// The page shows the drag-back dialog; its confirm sends `board.drag_back` (FLW-18).
    DragBack { dialog: Box<DragBackDialog> },
    /// Allowed by the rules, but not built yet: shown as a toast.
    NotYet { message: String },
    /// A blocked ticket (DPV-03): the page shows the dialog; it already waits in Ready for Dev.
    Blocked {
        dialog: crate::view_models::dependencies::Dialog,
    },
}

/// Where a card is, as today's board names it: its section in a human check's column ("Ready for Review",
/// "Approved"), else its column ("Dev").
pub fn place_label(columns: &Columns, ticket: &Ticket) -> String {
    let spot = columns.spot(ticket);
    match (columns.get(spot.column), spot.part) {
        (Some(column), Some(part)) if column.has_sections() => match part {
            flow_columns::APPROVED => "Approved".to_owned(),
            flow_columns::READY_FOR_REVIEW => "Ready for Review".to_owned(),
            _ => column.title.clone(),
        },
        (Some(column), _) => column.title.clone(),
        (None, _) => spot.column.to_owned(),
    }
}

/// One Review column state in the gallery.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "BoardGalleryColumn")]
pub struct GalleryColumn {
    pub name: String,
    pub column: ColumnPage,
}

/// One dragged card's verdicts in the gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardGalleryDrag")]
pub struct GalleryDrag {
    /// "SHOP-25 · Dev".
    pub from: String,
    pub answer: DragAnswer,
}

/// The gallery's board section (T-0324): the Review column in its states (*Row · Review column (0086 · V)*), the
/// drop verdicts of a card from each column, the empty board and the agent filter's chip.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "BoardGallery")]
pub struct Gallery {
    pub review: Vec<GalleryColumn>,
    pub drags: Vec<GalleryDrag>,
    pub empty: empty_board::View,
    pub agent_chip: AgentChip,
    /// The toolbar on the Board (Fold empty columns on) and on the List (BRD-01, T-0460).
    pub toolbars: Vec<GalleryToolbar>,
    /// Folded columns (BRD-02, `s26-10`): Dev with a card that needs you (the dot), an empty Review.
    pub rails: Vec<GalleryColumn>,
    /// Done folded: "Done 4 →".
    pub done: DoneLine,
    /// The pill's words for 7 columns cut off.
    pub more: String,
}

/// One toolbar state in the gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardGalleryToolbar")]
pub struct GalleryToolbar {
    pub name: String,
    pub toolbar: Toolbar,
}

pub fn gallery() -> Gallery {
    use crate::view_models::card_fixtures::{NOW, ticket};
    let flow = otaq_core::flow::default_flow();
    let columns = Columns::of(&flow);
    let view = ViewPrefs::default();
    let facts = ProjectFacts {
        flow: &flow,
        name: "shop-web",
        repo_path: "/Users/me/code/shop-web",
        base_branch: "main",
        agent_names: &[],
        was_validating: &[],
        leads: &[],
        agent_filter: None,
        agents: None,
        agent_list: None,
        starts_held: None,
        view: &view,
        paused: false,
    };
    let params = Params {
        project: "SHOP".into(),
        width: 1376.,
        height: 856.,
        ..Params::default()
    };
    let mut pr = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
    pr.flags.pr_number = Some(481);
    let full = vec![
        ticket("SHOP-18", "Checkout API", Status::WaitingForYou),
        ticket("SHOP-23", "Split user full name", Status::WaitingForYou),
        pr.clone(),
    ];
    let review_of = |name: &str, tickets: &[Ticket]| GalleryColumn {
        name: name.to_owned(),
        column: page(
            tickets,
            &facts,
            &params,
            NOW,
            &KeyLabels::base(Platform::Mac),
        )
        .columns
        .into_iter()
        .find(|c| c.kind == ColumnKind::HumanCheck)
        .expect("the Review column"),
    };
    let drag_of = |t: &Ticket| GalleryDrag {
        from: format!("{} · {}", t.key, place_label(&columns, t)),
        answer: drag_answer_in(&flow, t, &[], None),
    };
    let one = |t: Ticket| vec![t, ticket("SHOP-24", "Update footer links", Status::Backlog)];
    Gallery {
        review: vec![
            review_of("both sections", &full),
            review_of(
                "an empty Approved section",
                &one(ticket("SHOP-18", "Checkout API", Status::WaitingForYou)),
            ),
            review_of("an empty Ready for Review section", &one(pr.clone())),
            review_of(
                "empty",
                &one(ticket("SHOP-17", "Add Stripe client", Status::Merged)),
            ),
        ],
        drags: [
            ticket("SHOP-24", "Update footer links", Status::Backlog),
            ticket("SHOP-25", "Upgrade to Next.js 15", Status::Working),
            ticket("SHOP-18", "Checkout API", Status::WaitingForYou),
            pr,
            ticket("SHOP-30", "Order history", Status::ClosedNotMerged),
        ]
        .iter()
        .map(drag_of)
        .collect(),
        empty: empty_board::view("shop-web", &KeyLabels::base(Platform::Mac)),
        agent_chip: AgentFilter {
            agent: "a1".into(),
            name: "Checkout slowness".into(),
            keys: Vec::new(),
        }
        .chip(),
        toolbars: gallery_toolbars(),
        rails: gallery_rails(&flow),
        done: DoneLine {
            text: "Done 4 \u{2192}".to_owned(),
            unfold: Change::FoldColumn {
                column: flow_columns::DONE.to_owned(),
                folded: false,
            },
        },
        more: board::more_stages(7).unwrap_or_default(),
    }
}

fn gallery_toolbars() -> Vec<GalleryToolbar> {
    let keys = KeyLabels::base(Platform::Mac);
    let board = ViewPrefs {
        fold_empty_columns: true,
        ..ViewPrefs::default()
    };
    let list = ViewPrefs {
        view: otaq_core::board::settings::BoardView::List,
        ..ViewPrefs::default()
    };
    vec![
        GalleryToolbar {
            name: "Board · Fold empty columns on".into(),
            toolbar: view_prefs::toolbar(&board, &keys),
        },
        GalleryToolbar {
            name: "List · by Stage".into(),
            toolbar: view_prefs::toolbar(&list, &keys),
        },
    ]
}

fn gallery_rails(flow: &Flow) -> Vec<GalleryColumn> {
    use crate::view_models::card_fixtures::{NOW, ticket};
    let view = ViewPrefs {
        folded_columns: vec!["dev".into(), "review".into()],
        ..ViewPrefs::default()
    };
    let facts = ProjectFacts {
        flow,
        name: "shop-web",
        repo_path: "/Users/me/code/shop-web",
        base_branch: "main",
        agent_names: &[],
        was_validating: &[],
        leads: &[],
        agent_filter: None,
        agents: None,
        agent_list: None,
        starts_held: None,
        view: &view,
        paused: false,
    };
    let tickets = vec![
        ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou),
        ticket("SHOP-33", "Shipping estimate", Status::Working),
    ];
    let board = page(
        &tickets,
        &facts,
        &Params {
            project: "SHOP".into(),
            width: 1376.,
            height: 856.,
            ..Params::default()
        },
        NOW,
        &KeyLabels::base(Platform::Mac),
    );
    board
        .columns
        .into_iter()
        .filter(|c| c.rail.is_some())
        .map(|column| GalleryColumn {
            name: column
                .rail
                .as_ref()
                .map(|r| r.tooltip.clone())
                .unwrap_or_default(),
            column,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::{NOW, ticket};
    use otaq_core::domain::Kind;

    /// A ticket of the test flow in `stage` with `status`.
    fn on_test_flow(key: &str, stage: &str, status: Status) -> Ticket {
        let mut t = ticket(key, "Price table", status);
        t.place = otaq_core::domain::Place::stage_of(stage);
        t
    }

    fn verdict_of(answer: &DragAnswer, target: &str) -> Verdict {
        answer
            .targets
            .iter()
            .find(|t| t.target.as_str() == target)
            .unwrap_or_else(|| panic!("no verdict for {target}"))
            .verdict
            .clone()
    }

    /// FLW-18, Q271: every stage of the test flow takes a drop; "Moving a card here isn't available yet" is gone.
    #[test]
    fn every_stage_is_a_drop_target() {
        let flow = flow_columns::every_kind();
        let qa = on_test_flow("SHOP-19", "qa", Status::WaitingForYou);
        let answer = drag_answer_in(&flow, &qa, &[], None);
        let targets: Vec<&str> = answer.targets.iter().map(|t| t.target.as_str()).collect();
        assert_eq!(
            targets,
            [
                "backlog",
                "ready_for_dev",
                "grill",
                "dev",
                "security",
                "qa",
                "ship",
                "done"
            ]
        );
        for t in &answer.targets {
            if let Verdict::Forbidden { title, .. } = &t.verdict {
                assert!(
                    !title.contains("available yet"),
                    "{}: {title}",
                    t.target.as_str()
                );
            }
        }
        let hint = |target: &str| match verdict_of(&answer, target) {
            Verdict::Allowed { hint, .. } => hint,
            other => panic!("{target}: {other:?}"),
        };
        assert_eq!(hint("dev"), "Drop to send back to Dev \u{b7} asks first");
        assert_eq!(
            hint("grill"),
            "Drop to send back to Grill \u{b7} asks first"
        );
        assert_eq!(
            hint("security"),
            "Drop to run Security again \u{b7} asks first"
        );
        // QA's next stage is the Delivery: the Approve drop, as on today's board.
        assert!(hint("ship").starts_with("Drop to approve SHOP-19"));

        // Forward from Dev between turns: Validate (folded into Dev) is the next stage; Security skips it.
        let dev = on_test_flow("SHOP-23", "dev", Status::Idle);
        let answer = drag_answer_in(&flow, &dev, &[], None);
        assert_eq!(
            verdict_of(&answer, "security"),
            Verdict::Allowed {
                icon: IconName::SkipForward,
                hint: "Drop to skip Validate".to_owned(),
                note: None,
            }
        );
        assert!(
            matches!(verdict_of(&answer, "qa"), Verdict::Allowed { hint, .. } if hint == "Drop to skip Validate, Security")
        );
        // Back onto a human check: no dialog.
        let ship = on_test_flow("SHOP-26", "ship", Status::ToDeliver);
        let answer = drag_answer_in(&flow, &ship, &[], None);
        assert!(matches!(verdict_of(&answer, "qa"), Verdict::Allowed { .. }));
        assert_eq!(
            drop_step_in(&flow, &ship, &[], &DropTargetId::new("grill")),
            DropStep::DragBack
        );
    }

    /// *03 → A parked card dropped anywhere*: it returns to the stage it left, wherever it is dropped.
    #[test]
    fn a_parked_card_returns_to_the_stage_it_left() {
        let flow = flow_columns::every_kind();
        let parked = ticket("SHOP-36", "Cache product prices", Status::Parked);
        let answer = drag_answer_in(&flow, &parked, &[], Some("Dev"));
        let hint = |target: &str| match verdict_of(&answer, target) {
            Verdict::Allowed { hint, .. } => hint,
            other => panic!("{target}: {other:?}"),
        };
        assert_eq!(hint("security"), "Returns to Dev, the stage it left");
        assert_eq!(hint("qa"), "Returns to Dev, the stage it left");
        assert_eq!(
            hint("ready_for_dev"),
            "Drop to resume \u{b7} same conversation"
        );
        assert_eq!(hint("grill"), "Drop to resume \u{b7} same conversation");
        assert_eq!(
            drop_step_in(&flow, &parked, &[], &DropTargetId::new("qa")),
            DropStep::Move
        );
    }

    /// *03 → An epic can't go back* (Q266): an epic waiting in its check is refused on any earlier stage.
    #[test]
    fn an_epic_cant_be_dragged_back() {
        let flow = flow_columns::every_kind();
        let mut epic = on_test_flow("PAY-123", "qa", Status::WaitingForYou);
        epic.kind = Kind::Epic;
        let answer = drag_answer_in(&flow, &epic, &[], None);
        for target in ["ready_for_dev", "grill", "dev", "security"] {
            assert_eq!(
                verdict_of(&answer, target),
                Verdict::Forbidden {
                    title: "An epic can\u{2019}t be dragged back".to_owned(),
                    detail: None,
                },
                "{target}"
            );
        }
        // The default flow too: Review → Dev.
        let mut epic = ticket("SHOP-40", "Wishlist", Status::WaitingForYou);
        epic.kind = Kind::Epic;
        assert_eq!(
            verdict_on(&epic, DropTargetId::DEV),
            Verdict::Forbidden {
                title: "An epic can\u{2019}t be dragged back".to_owned(),
                detail: None,
            }
        );
    }

    /// *03 → Dragging a card back to an earlier stage* (s03-50): the three dialogs, word for word.
    #[test]
    fn the_drag_back_dialogs() {
        let flow = flow_columns::every_kind();
        // SHOP-19 in Security, its check failed: nothing runs there.
        let failed = on_test_flow("SHOP-19", "security", Status::CheckFailed);
        let dialog = drag_back_dialog(&flow, &failed, &DropTargetId::DEV, &[]).unwrap();
        assert_eq!(dialog.title, "Send SHOP-19 back to Dev?");
        assert_eq!(
            dialog.comment.label.as_deref(),
            Some("Comment for Dev (Rust developer)")
        );
        assert_eq!(
            dialog.comment.placeholder,
            "What should change? Every later stage reads it too."
        );
        assert_eq!(
            dialog.comment.footer.as_deref(),
            Some("Posted as a comment for Dev \u{b7} every later stage reads it")
        );
        assert!(dialog.comment.optional);
        assert_eq!(dialog.stops, None);
        assert_eq!(
            dialog.then.as_deref(),
            Some("Then it goes on to Validate, Security, QA and Done as usual.")
        );
        assert_eq!(
            (dialog.cancel.as_str(), dialog.confirm.as_str()),
            ("Cancel", "Send to Dev")
        );
        assert_eq!(dialog.confirm_icon, IconName::Undo2);
        assert_eq!(dialog.target, DropTargetId::DEV);

        // SHOP-29 running in Security: the reviewer stops first.
        let running = on_test_flow("SHOP-29", "security", Status::Checking);
        let dialog = drag_back_dialog(&flow, &running, &DropTargetId::DEV, &[]).unwrap();
        assert_eq!(dialog.title, "Send SHOP-29 back to Dev?");
        assert_eq!(
            dialog.stops.as_deref(),
            Some(
                "Security reviewer is working on it now. It stops, and the check runs again after Dev."
            )
        );
        assert_eq!(dialog.then, None);
        assert_eq!(dialog.confirm, "Stop and send to Dev");

        // SHOP-18 in QA, dropped on Security: the check runs again with a note.
        let qa = on_test_flow("SHOP-18", "qa", Status::WaitingForYou);
        let dialog = drag_back_dialog(&flow, &qa, &DropTargetId::new("security"), &[]).unwrap();
        assert_eq!(dialog.title, "Run Security again on SHOP-18?");
        assert_eq!(
            dialog.comment.label.as_deref(),
            Some("Comment for Security (Security reviewer)")
        );
        assert_eq!(
            dialog.comment.footer.as_deref(),
            Some("Posted as a comment for Security \u{b7} every later stage reads it")
        );
        assert_eq!(dialog.comment.placeholder, "");
        assert_eq!((dialog.stops, dialog.then), (None, None));
        assert_eq!(dialog.confirm, "Run Security");
        assert_eq!(dialog.confirm_icon, IconName::RotateCcw);

        // The ticket's own choices: the agent it picked for Dev, and QA skipped.
        let choices: Vec<StageOptions> = serde_json::from_value(serde_json::json!([
            {"stage_id": "dev", "stage": "Dev", "usual": {"name": "Rust developer"},
             "agents": [{"name": "Quick developer"}], "can_skip": false,
             "choice": {"stage_id": "dev", "choice": {"kind": "agent", "id": "a2", "name": "Quick developer"},
                        "set_by": {"by": "user"}, "set_at": 0}, "locked": true},
            {"stage_id": "qa", "stage": "QA", "agents": [], "can_skip": true,
             "choice": {"stage_id": "qa", "choice": {"kind": "skip"}, "set_by": {"by": "user"}, "set_at": 0},
             "locked": false}
        ]))
        .unwrap();
        let dialog = drag_back_dialog(&flow, &failed, &DropTargetId::DEV, &choices).unwrap();
        assert_eq!(
            dialog.comment.label.as_deref(),
            Some("Comment for Dev (Quick developer)")
        );
        assert_eq!(
            dialog.then.as_deref(),
            Some("Then it goes on to Validate, Security and Done as usual.")
        );
        // On the default flow: Review → Dev.
        let review = ticket("NOTE-1", "Sort notes by date", Status::WaitingForYou);
        let flow = otaq_core::flow::default_flow();
        let dialog = drag_back_dialog(&flow, &review, &DropTargetId::DEV, &[]).unwrap();
        assert_eq!(dialog.title, "Send NOTE-1 back to Dev?");
        assert_eq!(
            dialog.comment.label.as_deref(),
            Some("Comment for Dev (Developer)")
        );
        assert_eq!(
            dialog.then.as_deref(),
            Some("Then it goes on to Validate, Review and Done as usual.")
        );
        // A drop that asks nothing has no dialog.
        assert_eq!(
            drag_back_dialog(&flow, &review, &DropTargetId::APPROVED, &[]),
            None
        );
    }

    fn facts() -> ProjectFacts<'static> {
        ProjectFacts {
            flow: &otaq_core::flow::default_file().flow,
            name: "shop-web",
            repo_path: "/tmp/shop-web",
            base_branch: "main",
            agent_names: &[],
            was_validating: &[],
            leads: &[],
            agent_filter: None,
            agents: None,
            agent_list: None,
            starts_held: None,
            view: view(ViewPrefs {
                folded_columns: Vec::new(),
                ..ViewPrefs::default()
            }),
            paused: false,
        }
    }

    /// View preferences that live as long as the test (`ProjectFacts` borrows them).
    fn view(prefs: ViewPrefs) -> &'static ViewPrefs {
        Box::leak(Box::new(prefs))
    }

    fn params(width: f32) -> Params {
        Params {
            project: "SHOP".into(),
            width,
            height: 856.,
            filter: FilterState::default(),
            agent: None,
        }
    }

    fn sample() -> Vec<Ticket> {
        let mut pinned = ticket("SHOP-19", "Price table", Status::Working);
        pinned.flags.pinned_at = Some(10);
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = Kind::Epic;
        let mut pr = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        vec![
            ticket("SHOP-24", "Update footer links", Status::Backlog),
            epic,
            pinned,
            ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou),
            ticket("SHOP-27", "Search results paging", Status::Working),
            ticket("SHOP-21", "Order confirmation email", Status::Queued),
            ticket("SHOP-18", "Checkout API", Status::WaitingForYou),
            pr,
            ticket("SHOP-17", "Add Stripe client", Status::Merged),
        ]
    }

    /// EPC-11, EPL-01 on the page: the epic card carries its rows (draggable before anything starts); once
    /// SHOP-45.1 is queued the epic is no Backlog card, SHOP-45.2 is a card of its own, and Backlog counts what it
    /// shows.
    #[test]
    fn epics_and_children_show_once() {
        let mut epic = ticket("SHOP-45", "Search", Status::Backlog);
        epic.kind = Kind::Epic;
        let child = |key: &str, status| {
            let mut t = ticket(key, "Child", status);
            t.epic = Some("SHOP-45".parse().unwrap());
            t
        };
        let mut tickets = vec![
            epic,
            child("SHOP-45.1", Status::Backlog),
            child("SHOP-45.2", Status::Backlog),
        ];
        let board = page(
            &tickets,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let backlog = &board.columns[0];
        assert_eq!(backlog.count, "1");
        let [BoardCard::Epic { card }] = backlog.cards.as_slice() else {
            panic!("one epic card: {:?}", backlog.cards)
        };
        let rows: Vec<(&str, bool)> = card
            .children
            .iter()
            .map(|c| (c.key.as_str(), c.draggable))
            .collect();
        assert_eq!(rows, [("SHOP-45.1", true), ("SHOP-45.2", true)]);

        tickets[1].status = Status::Queued;
        tickets[1].place = otaq_core::domain::Place::Waiting(None);
        let board = page(
            &tickets,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let backlog: Vec<&str> = board.columns[0].cards.iter().map(BoardCard::key).collect();
        assert_eq!(backlog, ["SHOP-45.2"]);
        assert_eq!(board.columns[0].count, "1");
        let ready: Vec<&str> = board.columns[1].cards.iter().map(BoardCard::key).collect();
        assert_eq!(ready, ["SHOP-45.1"]);
    }

    /// T-0289, EPL-01: the epic card's Lead row (Open lead without a lead) and the lane head's lead come from the
    /// leads.
    #[test]
    fn the_leads_reach_the_epic_card_and_the_lane() {
        use crate::view_models::lead_status::sample;
        use otaq_core::project_agents::lead::LeadStatus;

        let mut epic = ticket("SHOP-45", "Search", Status::Backlog);
        epic.kind = Kind::Epic;
        let mut child = ticket("SHOP-45.1", "Search index API", Status::Backlog);
        child.epic = Some("SHOP-45".parse().unwrap());
        let mut tickets = vec![epic, child];
        let board = page(
            &tickets,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let [BoardCard::Epic { card }] = board.columns[0].cards.as_slice() else {
            panic!("one epic card")
        };
        assert_eq!(card.lead.as_ref().unwrap().full, "Open lead");

        let leads = [sample(&tickets[0], LeadStatus::CheckingPlan, 0)];
        let with = ProjectFacts {
            leads: &leads,
            ..facts()
        };
        let board = page(
            &tickets,
            &with,
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let [BoardCard::Epic { card }] = board.columns[0].cards.as_slice() else {
            panic!("one epic card")
        };
        assert_eq!(
            card.lead.as_ref().unwrap().full,
            "Search lead · checking the plan"
        );

        tickets[1].status = Status::Queued;
        tickets[1].place = otaq_core::domain::Place::Waiting(None);
        tickets[1].queued_at = Some(1);
        let board = page(
            &tickets,
            &with,
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        assert!(board.lanes.is_none(), "Lanes: None");
        let lanes = ProjectFacts {
            view: view(ViewPrefs {
                lanes: otaq_core::board::settings::Lanes::Epic,
                ..ViewPrefs::default()
            }),
            ..with
        };
        let board = page(
            &tickets,
            &lanes,
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let lane = &board.lanes.expect("lanes")[0];
        assert_eq!(
            lane.head.lead.as_ref().unwrap().full,
            "Search lead · checking the plan"
        );
    }

    /// EPL-01: no strip above the columns. While SHOP-45 is a card its children are its rows; once
    /// SHOP-45.1 is queued, SHOP-45.2 left in Backlog is a card of its own that names its epic, and no epic card
    /// is left.
    #[test]
    fn in_flight_a_backlog_child_is_a_card_naming_its_epic() {
        let mut epic = ticket("SHOP-45", "Search", Status::Backlog);
        epic.kind = Kind::Epic;
        let child = |key: &str, status| {
            let mut t = ticket(key, "Child", status);
            t.epic = Some("SHOP-45".parse().unwrap());
            t
        };
        let mut tickets = vec![
            epic,
            child("SHOP-45.1", Status::Backlog),
            child("SHOP-45.2", Status::Backlog),
            ticket("SHOP-51", "Fix checkout rounding", Status::Working),
        ];
        let board = page(
            &tickets,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let backlog: Vec<&str> = board.columns[0].cards.iter().map(BoardCard::key).collect();
        assert_eq!(backlog, ["SHOP-45"]);

        tickets[1].status = Status::Queued;
        tickets[1].place = otaq_core::domain::Place::Waiting(None);
        tickets[1].queued_at = Some(1_000);
        let board = page(
            &tickets,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let backlog: Vec<&str> = board.columns[0].cards.iter().map(BoardCard::key).collect();
        assert_eq!(backlog, ["SHOP-45.2"]);
        let BoardCard::Ticket { card, .. } = &board.columns[0].cards[0] else {
            panic!("a ticket card")
        };
        assert_eq!(card.epic.as_deref(), Some("Search"));
        let json = serde_json::to_value(&board).unwrap();
        assert!(json.get("strip").is_none());
    }

    /// SHL-06, FLW-01: six columns in order (Validate its own); Review holds Ready for Review and Approved as two
    /// sections.
    #[test]
    fn six_columns_reach_the_page_worded() {
        let tickets = sample();
        let page = page(
            &tickets,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let ids: Vec<ColumnId> = page.columns.iter().map(|c| c.id.clone()).collect();
        assert_eq!(
            ids,
            [
                ColumnId::BACKLOG,
                ColumnId::READY_FOR_DEV,
                ColumnId::DEV,
                ColumnId::new("validate"),
                ColumnId::REVIEW,
                ColumnId::DONE
            ]
        );
        let titles: Vec<&str> = page.columns.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Backlog",
                "Ready for Dev",
                "Dev",
                "Validate",
                "Review",
                "Done"
            ]
        );
        assert_eq!(page.min_column_width, 180.);
        assert_eq!(page.folded_width, 52.);
        assert_eq!(page.empty, None);
        let backlog = &page.columns[0];
        assert_eq!(backlog.add.as_deref(), Some("Add ticket"));
        assert!(
            backlog
                .cards
                .iter()
                .any(|c| matches!(c, BoardCard::Epic { .. }))
        );
        assert_eq!(
            page.columns[1].hint.as_deref(),
            Some(board::READY_FOR_DEV_HINT)
        );
        let dev = page.columns[2].dev.as_ref().unwrap();
        let top: Vec<&str> = dev.top.iter().map(BoardCard::key).collect();
        assert_eq!(top, ["SHOP-19", "SHOP-22"], "pinned, then needs you");
        assert!(matches!(
            dev.top[1],
            BoardCard::Ticket {
                needs_you: true,
                ..
            }
        ));
        let review = &page.columns[4];
        assert_eq!(review.count, "2", "both sections");
        assert!(review.cards.is_empty(), "the cards are in the sections");
        let [ready, approved] = review.sections.as_slice() else {
            panic!("two sections: {:?}", review.sections)
        };
        assert_eq!(
            (ready.id.clone(), ready.title.as_str(), ready.count.as_str()),
            (ReviewSectionId::READY_FOR_REVIEW, "READY FOR REVIEW", "1")
        );
        assert_eq!(
            (
                approved.id.clone(),
                approved.title.as_str(),
                approved.count.as_str()
            ),
            (ReviewSectionId::APPROVED, "APPROVED", "1")
        );
        assert_eq!(
            ready.cards.iter().map(BoardCard::key).collect::<Vec<_>>(),
            ["SHOP-18"]
        );
        assert_eq!(
            approved
                .cards
                .iter()
                .map(BoardCard::key)
                .collect::<Vec<_>>(),
            ["SHOP-26"]
        );
        assert_eq!(
            approved.note.as_ref().unwrap().text,
            format!(
                "{} doesn’t watch PRs. Mark it once it’s merged on GitHub.",
                otaq_core::naming::PRODUCT_NAME
            )
        );
        assert_eq!(ready.note, None);
        assert_eq!(
            (ready.drag_hint.as_deref(), approved.drag_hint.is_some()),
            (None, true)
        );
        assert_eq!(approved.fold, "Fold Approved");
        assert_eq!(ready.unfold, "Unfold Ready for Review");
        let done = &page.columns[5];
        assert!(!done.collapsed, "Done opened");
        let labels = |c: &ColumnPage| -> Vec<String> {
            c.menu.iter().map(|m| m.item.label.clone()).collect()
        };
        assert_eq!(
            labels(done),
            ["Fold this column", "Change the whole flow\u{2026}"]
        );
        assert_eq!(page.done, None);
        let json = serde_json::to_value(&page).unwrap();
        assert_eq!(json["columns"][1]["id"], "ready_for_dev");
        assert_eq!(json["columns"][0]["cards"][0]["kind"], "ticket");
        assert_eq!(json["scroll_chip"]["above"], "{n} above");
    }

    /// EPC-16 on the page: SHOP-40.1 and 40.2 merged fold into one line before their cards, SHOP-40.5 closed stays
    /// a card; Done (and its folded line) counts the fold as one.
    #[test]
    fn merged_children_fold_on_the_page() {
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = Kind::Epic;
        let child = |key: &str, status| {
            let mut t = ticket(key, "Child", status);
            t.epic = Some("SHOP-40".parse().unwrap());
            t
        };
        let tickets = vec![
            epic,
            child("SHOP-40.1", Status::Merged),
            child("SHOP-40.2", Status::Merged),
            child("SHOP-40.3", Status::Working),
            child("SHOP-40.5", Status::ClosedNotMerged),
            ticket("SHOP-17", "Add Stripe client", Status::Merged),
        ];
        let board = page(
            &tickets,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let done = &board.columns[5];
        assert_eq!(
            done.folds,
            [DoneFold {
                epic: "SHOP-40".into(),
                // The closed child is counted apart (#31).
                label: "Wishlist · 2 of 3 merged · 1 closed".into(),
                keys: vec!["SHOP-40.1".into(), "SHOP-40.2".into()],
            }]
        );
        let cards: Vec<&str> = done.cards.iter().map(BoardCard::key).collect();
        assert_eq!(cards, ["SHOP-17", "SHOP-40.1", "SHOP-40.2", "SHOP-40.5"]);
        assert_eq!(done.count, "3");
        let json = serde_json::to_value(&board).unwrap();
        assert_eq!(
            json["columns"][5]["folds"][0]["label"],
            "Wishlist · 2 of 3 merged · 1 closed"
        );
        assert_eq!(json["columns"][0]["folds"], serde_json::json!([]));

        let folded = page(
            &tickets,
            &ProjectFacts {
                view: view(ViewPrefs::default()),
                ..facts()
            },
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        assert!(folded.columns[5].collapsed);
        assert_eq!(folded.done.unwrap().text, "Done 3 \u{2192}");
    }

    /// BRD-02: Done is folded at first, "Done 4 →" under the last column, at every width; the line unfolds it, and
    /// its ⋯ folds it back. A folded stage is a rail that says what needs the user.
    #[test]
    fn done_is_a_line_until_opened() {
        let tickets = sample();
        let first = page(
            &tickets,
            &ProjectFacts {
                view: view(ViewPrefs::default()),
                ..facts()
            },
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let done = &first.columns[5];
        assert!(done.collapsed);
        assert_eq!(done.rail, None, "Done folds to its line, not a rail");
        let line = first.done.as_ref().unwrap();
        assert_eq!(line.text, "Done 1 \u{2192}");
        assert_eq!(
            line.unfold,
            Change::FoldColumn {
                column: "done".into(),
                folded: false
            }
        );
        let dev_folded = page(
            &tickets,
            &ProjectFacts {
                view: view(ViewPrefs {
                    folded_columns: vec!["dev".into()],
                    ..ViewPrefs::default()
                }),
                ..facts()
            },
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let dev = &dev_folded.columns[2];
        let rail = dev.rail.as_ref().unwrap();
        assert!(rail.dot);
        assert!(rail.tooltip.starts_with("Dev \u{b7} "), "{}", rail.tooltip);
        assert!(rail.tooltip.ends_with("needs you"), "{}", rail.tooltip);
        assert_eq!(rail.hint, "Click to unfold");
        assert!(dev_folded.columns[0].menu.is_empty(), "Backlog has no ⋯");
    }

    /// BRD-04, FED-09 (`s26-08`): a stage's ⋯ edits it, adds one after it, folds it, or changes the whole flow; each
    /// flow item says how the editor opens. Ready for Dev adds first; the Delivery has nothing after it; Done only
    /// folds and changes the flow.
    #[test]
    fn a_columns_menu_opens_the_flow_editor_on_its_stage() {
        let grill = column_menu(&ColumnId::new("grill"), ColumnKind::Agent, "Grill");
        let rows: Vec<(&str, &str, Option<&str>, bool)> = grill
            .iter()
            .map(|m| {
                (
                    m.item.id.as_str(),
                    m.item.label.as_str(),
                    m.item.hint.as_deref(),
                    m.item.separated,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "edit_stage",
                    "Edit Grill\u{2026}",
                    Some("agent, skill, options"),
                    false
                ),
                ("add_stage", "Add a stage after Grill", None, false),
                ("fold", "Fold this column", None, false),
                (
                    "change_flow",
                    "Change the whole flow\u{2026}",
                    Some("the same screen as onboarding"),
                    true
                ),
            ]
        );
        let editors: Vec<Option<OpenEditor>> = grill.iter().map(|m| m.editor.clone()).collect();
        assert_eq!(
            editors,
            [
                Some(OpenEditor::Stage {
                    stage: "grill".into()
                }),
                Some(OpenEditor::AddAfter {
                    after: Some("grill".into())
                }),
                None,
                Some(OpenEditor::Flow),
            ]
        );
        assert!(grill[2].change.is_some() && grill[0].change.is_none());
        let json = serde_json::to_value(&grill[1].editor).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "open": "add_after", "after": "grill" })
        );

        let ready = column_menu(
            &ColumnId::READY_FOR_DEV,
            ColumnKind::ReadyForDev,
            "Ready for Dev",
        );
        assert_eq!(ready[0].item.label, "Add a stage after Ready for Dev");
        assert_eq!(ready[0].editor, Some(OpenEditor::AddAfter { after: None }));
        assert_eq!(ready.len(), 3, "no Edit: Ready for Dev isn't a stage");

        let pr = column_menu(&ColumnId::new("pr"), ColumnKind::Delivery, "Done \u{b7} PR");
        let ids: Vec<&str> = pr.iter().map(|m| m.item.id.as_str()).collect();
        assert_eq!(ids, ["edit_stage", "fold", "change_flow"]);
        assert_eq!(pr[0].item.label, "Edit Done\u{2026}");

        let done = column_menu(&ColumnId::DONE, ColumnKind::Done, "Done");
        let ids: Vec<&str> = done.iter().map(|m| m.item.id.as_str()).collect();
        assert_eq!(ids, ["fold", "change_flow"]);
    }

    #[test]
    fn dev_folds_by_height_until_unfolded() {
        let mut tickets = sample();
        for n in 50..60 {
            tickets.push(ticket(&format!("SHOP-{n}"), "Work", Status::Working));
        }
        let short = Params {
            height: 400.,
            ..params(1376.)
        };
        let folded = page(
            &tickets,
            &facts(),
            &short,
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let dev = folded.columns[2].dev.as_ref().unwrap();
        assert!(dev.fold.as_deref().unwrap().ends_with("more · working"));
        let open = page(
            &tickets,
            &ProjectFacts {
                view: view(ViewPrefs {
                    dev_unfolded: true,
                    ..ViewPrefs::default()
                }),
                ..facts()
            },
            &short,
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        assert_eq!(open.columns[2].dev.as_ref().unwrap().fold, None);
    }

    /// The header's filter (T-0221): non-matching cards hide, counts read "shown/total", and each column says how
    /// many it hides.
    #[test]
    fn the_filter_hides_cards_and_columns_say_how_many() {
        let tickets = sample();
        let filtered = page(
            &tickets,
            &facts(),
            &Params {
                filter: FilterState {
                    tokens: vec!["is:needs-you".into()],
                    text: String::new(),
                },
                ..params(1376.)
            },
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let dev = &filtered.columns[2];
        let shown: Vec<&str> = dev
            .dev
            .as_ref()
            .unwrap()
            .top
            .iter()
            .map(BoardCard::key)
            .collect();
        assert_eq!(shown, ["SHOP-22"]);
        assert_eq!(dev.count, "1/3");
        assert_eq!(dev.hidden.as_deref(), Some("2 hidden by filter"));
        assert!(filtered.columns[0].cards.is_empty());
        assert_eq!(filtered.columns[4].count, "1/2");
        assert_eq!(
            filtered.columns[4].sections[1].hidden.as_deref(),
            Some("1 hidden by filter")
        );
        let review: Vec<&str> = filtered.columns[4].sections[0]
            .cards
            .iter()
            .map(BoardCard::key)
            .collect();
        assert_eq!(review, ["SHOP-18"], "review needs you too");
        let unfiltered = page(
            &tickets,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        assert_eq!(unfiltered.columns[2].hidden, None);
    }

    /// EPC-18, Q55: Focus epic (`g` `e`) on SHOP-40.2 puts `epic:Wishlist` in the filter (another epic's token gives way, other
    /// tokens stay); the board then keeps every card, dims the others and says which epic and cards stay; Esc
    /// takes the token out. A standalone ticket focuses nothing.
    #[test]
    fn focusing_an_epic_dims_the_other_cards() {
        let mut tickets = sample();
        let mut child = ticket("SHOP-40.2", "Wishlist button", Status::Working);
        child.epic = Some("SHOP-40".parse().unwrap());
        tickets.push(child);
        let state = FilterState {
            tokens: vec!["epic:Search".into(), "is:working".into()],
            text: String::new(),
        };
        let key: TicketKey = "SHOP-40.2".parse().unwrap();
        let focused = focus_filter(&state, Some(&key), &tickets).unwrap();
        assert_eq!(focused.tokens, ["is:working", "epic:Wishlist"]);
        let standalone: TicketKey = "SHOP-22".parse().unwrap();
        assert_eq!(focus_filter(&state, Some(&standalone), &tickets), None);
        let left = focus_filter(&focused, None, &tickets).unwrap();
        assert_eq!(left.tokens, ["is:working"]);
        assert_eq!(
            focus_filter(&left, None, &tickets),
            None,
            "nothing to leave"
        );

        let only_epic = FilterState {
            tokens: vec!["epic:Wishlist".into()],
            text: String::new(),
        };
        let board = page(
            &tickets,
            &facts(),
            &Params {
                filter: only_epic,
                ..params(1376.)
            },
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let focus = board.focus.unwrap();
        assert_eq!(focus.epics, ["SHOP-40"]);
        assert_eq!(focus.keys, ["SHOP-40", "SHOP-40.2"]);
        let dev = &board.columns[2];
        assert_eq!(dev.hidden, None, "dimmed, not hidden");
        assert_eq!(dev.count, "1/4");
        let unfocused = page(
            &tickets,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        assert_eq!(unfocused.focus, None);
    }

    #[test]
    fn an_empty_project_shows_the_empty_board_over_its_columns() {
        let page = page(
            &[],
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let empty = page.empty.unwrap();
        assert_eq!(empty.title, "shop-web is ready");
        assert_eq!(
            empty.text,
            "Tell the Project agent on the left what you want: it reads the code and writes the tickets. Or write the first one yourself."
        );
        let labels: Vec<&str> = empty.buttons.iter().map(|b| b.label).collect();
        assert_eq!(
            labels,
            ["New ticket"],
            "no Project agent or terminal button"
        );
        assert_eq!(page.columns.len(), 6);
        let review = &page.columns[4];
        let empties: Vec<(&str, &str)> = review
            .sections
            .iter()
            .map(|s| (s.title.as_str(), s.empty.as_str()))
            .collect();
        assert_eq!(
            empties,
            [
                ("READY FOR REVIEW", "Nothing to review"),
                ("APPROVED", "Nothing approved")
            ]
        );
        assert!(
            review.sections[1]
                .drag_hint
                .as_deref()
                .unwrap()
                .starts_with("Drag a ticket here from Ready for Review")
        );
    }

    #[test]
    fn a_drag_answers_every_column_at_once() {
        let backlog = ticket("SHOP-24", "Update footer links", Status::Backlog);
        let answer = drag_answer(&backlog, &[]);
        assert_eq!(answer.targets.len(), 7);
        let verdict = |id: DropTargetId| {
            answer
                .targets
                .iter()
                .find(|c| c.target == id)
                .unwrap()
                .verdict
                .clone()
        };
        assert_eq!(verdict(DropTargetId::BACKLOG), Verdict::Home);
        assert_eq!(
            verdict(DropTargetId::READY_FOR_DEV),
            Verdict::Allowed {
                icon: IconName::Play,
                hint: "Drop to queue · starts in priority order".into(),
                note: None,
            }
        );
        assert_eq!(
            verdict(DropTargetId::REVIEW),
            Verdict::Forbidden {
                title: "Only agents move tickets to Ready for Review".into(),
                detail: Some("Drag to Ready for Dev to queue it.".into())
            },
            "*Drag · over a forbidden column*"
        );
        let json = serde_json::to_value(drag_answer(&backlog, &[])).unwrap();
        assert_eq!(json["targets"][0]["verdict"]["kind"], "home");
        assert_eq!(json["targets"][5]["target"], "approved");
    }

    /// The verdict of `ticket` over `target`.
    fn verdict_on(ticket: &Ticket, target: DropTargetId) -> Verdict {
        drag_answer(ticket, &[])
            .targets
            .into_iter()
            .find(|t| t.target == target)
            .unwrap()
            .verdict
    }

    /// d0087, §7.4: every source column over the Review column (outside Approved: Ready for Review) and over the
    /// Approved section, answered up front.
    #[test]
    fn review_and_approved_verdicts_for_every_source() {
        let mut pr = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        let sources = [
            ticket("SHOP-24", "Update footer links", Status::Backlog),
            ticket("SHOP-21", "Order confirmation email", Status::Queued),
            ticket("SHOP-25", "Upgrade to Next.js 15", Status::Working),
            ticket("SHOP-18", "Checkout API", Status::WaitingForYou),
            pr,
            ticket("SHOP-30", "Order history", Status::ClosedNotMerged),
            ticket("SHOP-17", "Add Stripe client", Status::Merged),
        ];
        let kind = |v: &Verdict| match v {
            Verdict::Home => "home".to_owned(),
            Verdict::Allowed { hint, .. } => format!("allowed: {hint}"),
            Verdict::Forbidden { title, .. } => format!("forbidden: {title}"),
        };
        let rows: Vec<(String, String, String)> = sources
            .iter()
            .map(|t| {
                (
                    t.key.to_string(),
                    kind(&verdict_on(t, DropTargetId::REVIEW)),
                    kind(&verdict_on(t, DropTargetId::APPROVED)),
                )
            })
            .collect();
        let only_agents = "forbidden: Only agents move tickets to Ready for Review";
        let approve_first = "forbidden: Approve a ticket from Ready for Review";
        let mid_turn = "forbidden: SHOP-25 can't move to the next stage now: its agent is in the middle of a turn";
        let row = |key: &str, review: &str, approved: &str| {
            (key.to_owned(), review.to_owned(), approved.to_owned())
        };
        assert_eq!(
            rows,
            [
                row("SHOP-24", only_agents, approve_first),
                row("SHOP-21", only_agents, approve_first),
                // A forward drag is Move to next stage's (FLW-18): not while the agent is mid-turn.
                row("SHOP-25", mid_turn, mid_turn),
                row(
                    "SHOP-18",
                    "home",
                    "allowed: Drop to approve SHOP-18 → choose Local merge or Open PR"
                ),
                row("SHOP-26", "allowed: Drop to rework · PR #481 stays", "home"),
                row(
                    "SHOP-30",
                    only_agents,
                    "forbidden: Drag it to Backlog to reopen it first"
                ),
                row(
                    "SHOP-17",
                    "forbidden: Merged tickets are locked",
                    "forbidden: Merged tickets are locked"
                ),
            ]
        );
        // Dev → Review between turns: past Validate, which is skipped by you (FLW-18).
        let idle = ticket("SHOP-25", "Upgrade to Next.js 15", Status::Idle);
        assert_eq!(
            kind(&verdict_on(&idle, DropTargetId::REVIEW)),
            "allowed: Drop to skip Validate"
        );
        assert_eq!(
            kind(&verdict_on(&idle, DropTargetId::new("validate"))),
            "allowed: Drop to move to Validate"
        );
    }

    /// d0087: Ready for Review → the Approved section opens the Approve choice and moves nothing by itself; Approved
    /// → the Review column is PR rework; a Ready for Review card dropped on its own column does nothing.
    #[test]
    fn approve_choice_and_pr_rework_on_drop() {
        let review = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        assert_eq!(
            drop_step(&review, &[], DropTargetId::APPROVED),
            DropStep::Approve,
            "the choice, never a Move"
        );
        assert_eq!(
            drop_step(&review, &[], DropTargetId::REVIEW),
            DropStep::Nothing,
            "home"
        );
        let mut pr = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        assert_eq!(
            drop_step(&pr, &[], DropTargetId::REVIEW),
            DropStep::Move,
            "PR rework"
        );
        assert_eq!(
            drop_step(&pr, &[], DropTargetId::APPROVED),
            DropStep::Nothing
        );
        let working = ticket("SHOP-25", "Upgrade to Next.js 15", Status::Working);
        for target in [DropTargetId::REVIEW, DropTargetId::APPROVED] {
            assert_eq!(drop_step(&working, &[], target), DropStep::Nothing);
        }
        let targets: Vec<DropTargetId> = drag_answer(&review, &[])
            .targets
            .iter()
            .map(|t| t.target.clone())
            .collect();
        assert_eq!(
            targets,
            [
                DropTargetId::BACKLOG,
                DropTargetId::READY_FOR_DEV,
                DropTargetId::DEV,
                DropTargetId::new("validate"),
                DropTargetId::REVIEW,
                DropTargetId::APPROVED,
                DropTargetId::DONE
            ],
            "both sections answered up front"
        );
        let flow = otaq_core::flow::default_flow();
        let columns = Columns::of(&flow);
        assert!(
            columns
                .get("review")
                .unwrap()
                .targets()
                .contains(&"approved".to_owned())
        );
        assert_eq!(
            columns.drop_target("review"),
            Some(otaq_core::board::moves::DropTarget::ReadyForReview.into())
        );
    }

    /// SHL-01, EPL-01 on the page: Done opened or folded; an epic's lane cells follow the columns, one per column,
    /// and a folded column's cell is empty.
    #[test]
    fn lane_cells_follow_dones_fold() {
        let mut epic = ticket("SHOP-45", "Search", Status::Backlog);
        epic.kind = Kind::Epic;
        let child = |key: &str, status| {
            let mut t = ticket(key, "Child", status);
            t.epic = Some("SHOP-45".parse().unwrap());
            t.queued_at = Some(1);
            t
        };
        let tickets = vec![
            epic,
            child("SHOP-45.1", Status::Merged),
            child("SHOP-45.2", Status::WaitingForYou),
            child("SHOP-45.3", Status::Working),
        ];
        let lanes = |folded_columns: Vec<String>| ViewPrefs {
            lanes: otaq_core::board::settings::Lanes::Epic,
            folded_columns,
            ..ViewPrefs::default()
        };
        let cells = |p: &BoardPage| -> Vec<(ColumnId, usize)> {
            p.lanes.as_ref().unwrap()[0]
                .cells
                .iter()
                .map(|c| (c.column.clone(), c.cards.len()))
                .collect()
        };
        let open = page(
            &tickets,
            &ProjectFacts {
                view: view(lanes(Vec::new())),
                ..facts()
            },
            &params(1360.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        assert!(!open.columns[5].collapsed);
        assert_eq!(
            cells(&open),
            [
                (ColumnId::BACKLOG, 0),
                (ColumnId::READY_FOR_DEV, 0),
                (ColumnId::DEV, 1),
                (ColumnId::new("validate"), 0),
                (ColumnId::REVIEW, 1),
                (ColumnId::DONE, 1),
            ],
            "one cell per column"
        );
        let folded = page(
            &tickets,
            &ProjectFacts {
                view: view(lanes(vec!["done".into()])),
                ..facts()
            },
            &params(1360.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        assert!(folded.columns[5].collapsed);
        assert_eq!(cells(&folded)[5], (ColumnId::DONE, 0));
    }

    #[test]
    fn the_gallery_has_every_review_state() {
        let g = gallery();
        let counts: Vec<(&str, Vec<&str>)> = g
            .review
            .iter()
            .map(|c| {
                (
                    c.column.count.as_str(),
                    c.column.sections.iter().map(|s| s.count.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            counts,
            [
                ("3", vec!["2", "1"]),
                ("1", vec!["1", "0"]),
                ("1", vec!["0", "1"]),
                ("0", vec!["0", "0"])
            ]
        );
        assert_eq!(g.drags.len(), 5);
        assert_eq!(g.drags[1].from, "SHOP-25 · Dev");
        assert!(g.empty.text.starts_with("Tell the Project agent"));
    }

    /// d0087 (Q-G): the agent filter shows only the tickets the agent created or changed, combines with search,
    /// shows its chip, and clears.
    #[test]
    fn the_agent_filter_shows_its_tickets_and_clears() {
        let tickets = sample();
        let created = otaq_core::project_agents::CreatedAndChanged {
            created: vec![otaq_core::project_agents::CreatedTicket {
                key: "SHOP-24".parse().unwrap(),
                title: "Update footer links".into(),
                status: Status::Backlog,
                deletable: true,
            }],
            edited: vec!["SHOP-22".parse().unwrap(), "SHOP-18".parse().unwrap()],
            dependencies: 0,
            summary: String::new(),
        };
        let agent = AgentFilter::of("a1", "Checkout slowness", &created);
        let with = ProjectFacts {
            agent_filter: Some(&agent),
            ..facts()
        };
        let filtered = |filter: FilterState, agent: Option<&str>| {
            page(
                &tickets,
                &with,
                &Params {
                    filter,
                    agent: agent.map(str::to_owned),
                    ..params(1376.)
                },
                NOW,
                &KeyLabels::base(Platform::Mac),
            )
        };
        let shown = |p: &BoardPage| -> Vec<String> {
            let mut keys: Vec<String> = p
                .columns
                .iter()
                .flat_map(|c| {
                    let dev = c.dev.iter().flat_map(|d| d.top.iter().chain(&d.working));
                    let sections = c.sections.iter().flat_map(|s| &s.cards);
                    c.cards.iter().chain(dev).chain(sections)
                })
                .map(|c| c.key().to_owned())
                .collect();
            keys.sort();
            keys
        };
        let by_agent = filtered(FilterState::default(), Some("a1"));
        assert_eq!(shown(&by_agent), ["SHOP-18", "SHOP-22", "SHOP-24"]);
        assert_eq!(
            by_agent.agent_filter,
            Some(AgentChip {
                agent: "a1".into(),
                label: "Created or changed by Checkout slowness".into(),
                clear: "Clear filter · Esc".into(),
            })
        );
        assert_eq!(by_agent.columns[0].count, "1/2");
        assert_eq!(by_agent.columns[4].count, "1/2");

        // With a search: both must match.
        let both = filtered(
            FilterState {
                tokens: vec![],
                text: "footer".into(),
            },
            Some("a1"),
        );
        assert_eq!(shown(&both), ["SHOP-24"]);
        let both = filtered(
            FilterState {
                tokens: vec!["is:needs-you".into()],
                text: String::new(),
            },
            Some("a1"),
        );
        assert_eq!(shown(&both), ["SHOP-18", "SHOP-22"]);

        // Cleared (✕ or Esc): every card, no chip.
        let cleared = filtered(FilterState::default(), None);
        assert_eq!(cleared.agent_filter, None);
        assert_eq!(shown(&cleared).len(), tickets.len(), "every card");
        assert_eq!(cleared.columns[0].count, "2");
        let json = serde_json::to_value(&by_agent).unwrap();
        assert_eq!(
            json["agent_filter"]["label"],
            "Created or changed by Checkout slowness"
        );
    }

    /// *Epic flows 1 → 1* and *epic flows 3 → 8* (EPC-12): the epic card and a row are allowed over Ready for Dev
    /// and Dev with what they queue, and not allowed over Review, Approved and Done; their drops queue through
    /// `epic.queue`.
    #[test]
    fn an_epic_and_a_row_answer_their_drag_and_queue_on_drop() {
        let mut epic = ticket("SHOP-45", "Search", Status::Backlog);
        epic.kind = Kind::Epic;
        let child = |key: &str, status: Status| {
            let mut t = ticket(key, "Search child", status);
            t.epic = Some(epic.key.clone());
            t
        };
        let tickets = vec![
            epic.clone(),
            child("SHOP-45.1", Status::Backlog),
            child("SHOP-45.2", Status::Backlog),
        ];
        let verdicts = |answer: DragAnswer| -> Vec<Verdict> {
            answer.targets.into_iter().map(|c| c.verdict).collect()
        };
        let card = verdicts(drag_answer(&epic, &tickets));
        assert_eq!(card[0], Verdict::Home);
        for v in &card[1..3] {
            assert_eq!(
                v,
                &Verdict::Allowed {
                    icon: IconName::Play,
                    hint: "Queue 2 children".into(),
                    note: None,
                },
                "an epic drop queues its children (d0080)"
            );
        }
        for v in &card[3..] {
            assert!(matches!(v, Verdict::Forbidden { .. }), "{v:?}");
        }
        let row = verdicts(drag_answer(&tickets[1], &tickets));
        assert_eq!(
            row[1],
            Verdict::Allowed {
                icon: IconName::Play,
                hint: "Queue SHOP-45.1".into(),
                note: None,
            }
        );
        assert!(
            row[3..]
                .iter()
                .all(|v| matches!(v, Verdict::Forbidden { .. }))
        );
        let mut parked = child("SHOP-45.2", Status::Parked);
        parked.place = otaq_core::domain::Place::Backlog;
        assert_eq!(
            verdicts(drag_answer(&parked, &tickets))[2],
            Verdict::Allowed {
                icon: IconName::Play,
                hint: "Resume SHOP-45.2".into(),
                note: None,
            }
        );

        assert_eq!(
            drop_step(&epic, &[], DropTargetId::READY_FOR_DEV),
            DropStep::QueueEpic {
                epic: epic.key.clone(),
                child: None
            }
        );
        assert_eq!(
            drop_step(&tickets[1], &[], DropTargetId::DEV),
            DropStep::QueueEpic {
                epic: epic.key.clone(),
                child: Some(tickets[1].key.clone())
            }
        );
        assert_eq!(drop_step(&epic, &[], DropTargetId::DONE), DropStep::Nothing);
        assert_eq!(
            drop_step(&tickets[1], &[], DropTargetId::APPROVED),
            DropStep::Nothing
        );
    }

    /// *Epic flows 1 → 3* and *epic flows 4 → 11* (EPC-14): the track's drag accepts Backlog with what it takes
    /// back (two lines when a child keeps running) and Ready for Dev / Dev with what it queues; a drop on Backlog
    /// unqueues at once, or asks first when every child started.
    #[test]
    fn a_track_answers_its_drag_and_takes_back_on_drop() {
        let mut epic = ticket("SHOP-45", "Search", Status::Backlog);
        epic.kind = Kind::Epic;
        let child = |key: &str, status: Status| {
            let mut t = ticket(key, "Search child", status);
            t.epic = Some(epic.key.clone());
            t
        };
        let mut tickets = vec![
            epic.clone(),
            child("SHOP-45.1", Status::Working),
            child("SHOP-45.2", Status::Queued),
            child("SHOP-45.3", Status::Backlog),
        ];
        let verdicts = |answer: DragAnswer| -> Vec<Verdict> {
            answer.targets.into_iter().map(|c| c.verdict).collect()
        };
        let track = verdicts(drag_answer(&epic, &tickets));
        assert_eq!(
            track[0],
            Verdict::Allowed {
                icon: IconName::ArrowDownToLine,
                hint: "Unqueue 1 never-started child (45.2)".into(),
                note: Some("SHOP-45.1 keeps running".into()),
            }
        );
        for v in &track[1..3] {
            assert_eq!(
                v,
                &Verdict::Allowed {
                    icon: IconName::Play,
                    hint: "Queue 1 child".into(),
                    note: None,
                },
                "the rest of the epic"
            );
        }
        for v in &track[3..] {
            assert!(matches!(v, Verdict::Forbidden { .. }), "{v:?}");
        }
        let json = serde_json::to_value(drag_answer(&epic, &tickets)).unwrap();
        assert_eq!(
            json["targets"][0]["verdict"]["note"],
            "SHOP-45.1 keeps running"
        );
        assert_eq!(
            drop_step(&epic, &tickets, DropTargetId::BACKLOG),
            DropStep::TakeBack {
                epic: epic.key.clone()
            }
        );
        assert_eq!(
            drop_step(&epic, &tickets, DropTargetId::READY_FOR_DEV),
            DropStep::QueueEpic {
                epic: epic.key.clone(),
                child: None
            }
        );
        assert_eq!(
            drop_step(&epic, &tickets, DropTargetId::DONE),
            DropStep::Nothing
        );

        // State 11: both started and run; the drop asks first.
        tickets.truncate(2);
        tickets.push(child("SHOP-45.2", Status::Working));
        assert_eq!(
            verdicts(drag_answer(&epic, &tickets))[0],
            Verdict::Allowed {
                icon: IconName::SquareParking,
                hint: "Park 2 running children".into(),
                note: None,
            }
        );
        let DropStep::ParkAll(dialog) = drop_step(&epic, &tickets, DropTargetId::BACKLOG) else {
            panic!("Park all asks first");
        };
        assert_eq!(
            dialog,
            ParkAllDialog {
                key: "SHOP-45".into(),
                title: "Park 2 running tickets?".into(),
                message: "Their work is kept. SHOP-45.1 and SHOP-45.2 stop and go back to Backlog with their \
                          conversations."
                    .into(),
                cancel: "Cancel".into(),
                confirm: "Park 2 tickets".into(),
                confirm_icon: IconName::SquareParking,
            }
        );
    }

    /// State 10 (d0066), EPL-01: a child card taken out of Ready for Dev goes back to Backlog without a dialog
    /// and is a Backlog card naming its epic while SHOP-45.1 runs.
    #[test]
    fn a_child_card_taken_out_of_the_queue_is_a_backlog_card() {
        let mut epic = ticket("SHOP-45", "Search", Status::Backlog);
        epic.kind = Kind::Epic;
        let child = |key: &str, status| {
            let mut t = ticket(key, "Child", status);
            t.epic = Some("SHOP-45".parse().unwrap());
            t
        };
        let mut tickets = vec![
            epic,
            child("SHOP-45.1", Status::Working),
            child("SHOP-45.2", Status::Queued),
        ];
        assert_eq!(
            drop_step(&tickets[2], &tickets, DropTargetId::BACKLOG),
            DropStep::Move
        );
        // What the service's unqueue leaves (d0066).
        tickets[2].status = Status::Backlog;
        tickets[2].place = otaq_core::domain::Place::Backlog;
        let board = page(
            &tickets,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let backlog: Vec<&str> = board.columns[0].cards.iter().map(BoardCard::key).collect();
        assert_eq!(backlog, ["SHOP-45.2"]);
    }

    #[test]
    fn drops_say_what_comes_next() {
        let working = ticket("SHOP-25", "Upgrade to Next.js 15", Status::Working);
        assert_eq!(
            drop_step(&working, &[], DropTargetId::BACKLOG),
            DropStep::Park
        );
        let park = park_dialog(&working);
        assert_eq!(park.title, "Park SHOP-25?");
        assert!(park.message.contains("will be stopped"), "{}", park.message);
        assert_eq!(park.kept.len(), 3);
        let review = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        assert_eq!(
            drop_step(&review, &[], DropTargetId::APPROVED),
            DropStep::Approve
        );
        let menu = approve_menu(&review, "main", std::slice::from_ref(&review), None);
        assert_eq!(menu.title, "Approve SHOP-18 Checkout API");
        assert_eq!(menu.items[0].choice, ApproveChoice::LocalMerge);
        assert_eq!((menu.items[0].number, menu.items[1].number), (1, 2));
        assert_eq!(menu.items[1].icon, IconName::GitPullRequest);
        assert_eq!(
            menu.note,
            "Both open a confirmation step before anything runs."
        );
        let mut pr = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        assert_eq!(
            drop_step(&pr, &[], DropTargetId::REVIEW),
            DropStep::Move,
            "PR rework"
        );
        let closed = ticket("SHOP-30", "Order history", Status::ClosedNotMerged);
        assert_eq!(
            drop_step(&closed, &[], DropTargetId::BACKLOG),
            DropStep::Move,
            "Reopen"
        );
        assert_eq!(
            drop_step(&closed, &[], DropTargetId::DONE),
            DropStep::Nothing
        );
        let json = serde_json::to_value(DropOutcome::Park { dialog: park }).unwrap();
        assert_eq!(json["kind"], "park");
        assert_eq!(json["dialog"]["confirm"], "Park");
    }
    /// Frame 12's chain at one storyboard step: SHOP-17 merged, SHOP-18 and SHOP-19 as given, SHOP-20 needs both,
    /// SHOP-21 needs SHOP-20.
    fn storyboard(s18: Status, s19: Status, s20: Status) -> Vec<Ticket> {
        let with = |key: &str, title: &str, status: Status, deps: &[&str]| {
            let mut t = ticket(key, title, status);
            t.depends_on = deps.iter().map(|d| d.parse().unwrap()).collect();
            t
        };
        let mut shop_20 = with("SHOP-20", "Checkout page", s20, &["SHOP-18", "SHOP-19"]);
        if s20 == Status::Working {
            shop_20.work = Some(otaq_core::board::tickets::Work {
                base_sha: Some("3b7d0e1c9a".into()),
                ..Default::default()
            });
        }
        vec![
            with("SHOP-17", "Add Stripe client", Status::Merged, &[]),
            with("SHOP-18", "Checkout API", s18, &["SHOP-17"]),
            with("SHOP-19", "Price table", s19, &[]),
            shop_20,
            with(
                "SHOP-21",
                "Order confirmation email",
                Status::Queued,
                &["SHOP-20"],
            ),
        ]
    }

    /// One card of the page as JSON text, to look for the words the storyboard draws on it.
    fn card_text(page: &BoardPage, key: &str) -> String {
        page.columns
            .iter()
            .flat_map(|c| {
                let dev = c.dev.iter().flat_map(|d| d.top.iter().chain(&d.working));
                let sections = c.sections.iter().flat_map(|s| &s.cards);
                c.cards.iter().chain(dev).chain(sections)
            })
            .find(|c| c.key() == key)
            .map(|c| serde_json::to_string(c).unwrap())
            .unwrap_or_else(|| panic!("{key} is on the board"))
    }

    /// DPV-03: SHOP-20 (Backlog, waiting for SHOP-18 and SHOP-19) dropped on Dev, or started with ⌘D or Start, opens
    /// the dialog and queues it in Ready for Dev first; dropped on Ready for Dev it queues as today. Once in Ready, a
    /// drag on to Dev opens the dialog with nothing to queue.
    #[test]
    fn a_blocked_ticket_past_ready_for_dev_opens_the_dialog() {
        let flow = &otaq_core::flow::default_file().flow;
        let tickets = storyboard(Status::WaitingForYou, Status::Working, Status::Backlog);
        let t20 = tickets
            .iter()
            .find(|t| t.key.to_string() == "SHOP-20")
            .unwrap();
        let DropStep::Blocked { dialog, queue } =
            drop_step_in(flow, t20, &tickets, &DropTargetId::DEV)
        else {
            panic!("the dialog");
        };
        assert!(queue, "from Backlog it goes to Ready for Dev");
        assert_eq!(dialog.title, "SHOP-20 is blocked");
        assert!(
            dialog
                .body
                .starts_with("It waits for SHOP-18 (in Review) and SHOP-19 (in Dev) to merge."),
            "{}",
            dialog.body
        );
        assert_eq!(
            drop_step_in(flow, t20, &tickets, &DropTargetId::READY_FOR_DEV),
            DropStep::Move
        );
        let queued = storyboard(Status::WaitingForYou, Status::Working, Status::Queued);
        let t20 = queued
            .iter()
            .find(|t| t.key.to_string() == "SHOP-20")
            .unwrap();
        assert!(matches!(
            drop_step_in(flow, t20, &queued, &DropTargetId::DEV),
            DropStep::Blocked { queue: false, .. }
        ));
        // A ticket whose parents merged is no longer blocked: Backlog → Dev queues it as before.
        let free = storyboard(Status::Merged, Status::Merged, Status::Backlog);
        let t20 = free
            .iter()
            .find(|t| t.key.to_string() == "SHOP-20")
            .unwrap();
        assert_eq!(
            drop_step_in(flow, t20, &free, &DropTargetId::DEV),
            DropStep::Move
        );
    }

    /// DPV-02: Always sends every open link (the page draws those in view, dashed); On hover sends none, and each
    /// chain carries its tooltip.
    #[test]
    fn always_sends_every_link_and_each_chain_its_tooltip() {
        let tickets = storyboard(Status::WaitingForYou, Status::Working, Status::Queued);
        let on_hover = page(
            &tickets,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        assert!(on_hover.links.is_empty());
        let tip = on_hover
            .chains
            .iter()
            .find(|c| c.key == "SHOP-20")
            .and_then(|c| c.tip.clone())
            .unwrap();
        assert_eq!(
            (tip.title.as_str(), tip.detail.as_str()),
            (
                "SHOP-20 is blocked",
                "by SHOP-18 (Review) and SHOP-19 (Dev)"
            )
        );
        let always = ProjectFacts {
            view: view(ViewPrefs {
                dependencies: otaq_core::board::settings::Dependencies::Always,
                ..ViewPrefs::default()
            }),
            ..facts()
        };
        let page = page(
            &tickets,
            &always,
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let links: Vec<(&str, &str)> = page
            .links
            .iter()
            .map(|e| (e.parent.as_str(), e.child.as_str()))
            .collect();
        // SHOP-17 is merged: its link to SHOP-18 is history.
        assert_eq!(
            links,
            [
                ("SHOP-18", "SHOP-20"),
                ("SHOP-19", "SHOP-20"),
                ("SHOP-20", "SHOP-21")
            ]
        );
        assert!(page.toolbar.dependencies.unwrap().options[1].selected);
    }

    /// *12 · Stack & dependencies → 2 · The chain runs … 6 · Closed parent*, as the page receives them (the hand
    /// check walks them on the real board).
    #[test]
    fn the_storyboards_reach_the_page() {
        let at = |s18, s19, s20| {
            let tickets = storyboard(s18, s19, s20);
            page(
                &tickets,
                &facts(),
                &params(1376.),
                NOW,
                &KeyLabels::base(Platform::Mac),
            )
        };
        // 1 · Planning: DPV-01's blocked line, in Backlog too.
        let planning = at(Status::Backlog, Status::Backlog, Status::Backlog);
        let blocked = "Blocked \u{b7} waits for SHOP-18 and SHOP-19 to merge";
        assert!(card_text(&planning, "SHOP-20").contains(blocked));
        // 2 · The chain runs, then SHOP-18 in Review: the same line until a parent merges.
        let runs = at(Status::Working, Status::Working, Status::Queued);
        assert!(card_text(&runs, "SHOP-20").contains(blocked));
        assert!(card_text(&runs, "SHOP-21").contains("Blocked \u{b7} waits for SHOP-20 to merge"));
        assert!(card_text(&runs, "SHOP-18").contains("Blocks SHOP-20"));
        let review = at(Status::WaitingForYou, Status::Working, Status::Queued);
        assert!(card_text(&review, "SHOP-20").contains(blocked));
        // 3 · One parent merged.
        let one = at(Status::Merged, Status::WaitingForYou, Status::Queued);
        assert!(card_text(&one, "SHOP-20").contains("Blocked \u{b7} waits for SHOP-19 to merge"));
        // 4 · All parents merged: SHOP-20 works from the new main; SHOP-21 still waits.
        let all = at(Status::Merged, Status::Merged, Status::Working);
        assert!(
            card_text(&all, "SHOP-20").contains("from main @ 3b7d0e1 · all dependencies merged")
        );
        assert!(card_text(&all, "SHOP-21").contains("Blocked \u{b7} waits for SHOP-20 to merge"));
        // 5 · PR variant.
        let mut tickets = storyboard(Status::PrOpen, Status::Merged, Status::Queued);
        tickets[1].flags.pr_number = Some(482);
        let pr = page(
            &tickets,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        assert!(card_text(&pr, "SHOP-20").contains("Blocked \u{b7} waits for SHOP-18 to merge"));
        // Mark merged is the sheet's now: a card that doesn't need the user has no button (BRD-03).
        assert!(!card_text(&pr, "SHOP-18").contains("Mark merged"));
        // 6 · Closed parent never counts as merged.
        let closed = at(
            Status::Merged,
            Status::ClosedNotMerged,
            Status::ParentClosed,
        );
        let shop_20 = card_text(&closed, "SHOP-20");
        assert!(shop_20.contains("Parent closed"), "{shop_20}");
        assert!(shop_20.contains("SHOP-19 was closed without merging"));
        // The highlight of SHOP-20 lights the whole chain.
        let chain = all.chains.iter().find(|c| c.key == "SHOP-20").unwrap();
        assert_eq!(
            chain.members,
            ["SHOP-17", "SHOP-18", "SHOP-19", "SHOP-20", "SHOP-21"]
        );
        assert!(planning.chains.iter().all(|c| c.key != "SHOP-22"));
    }

    /// The example's shop-web flow (*24 · The two views*): Grill, Dev with Validate shown in it, Security, Review
    /// and the PR; seven columns and Done's line.
    fn shop_web() -> Flow {
        let yaml = "\
flow:
  - {id: grill, name: Grill, kind: agent, agent: Griller, skill: grill}
  - {id: dev, name: Dev, kind: agent, agent: Rust developer, skill: work-ticket}
  - {id: validate, name: Validate, kind: check, by: agent, column: with-previous, agent: Validator, skill: validate, rounds: 3}
  - {id: security, name: Security, kind: check, by: agent, agent: Security reviewer, skill: security-review, rounds: 3}
  - {id: review, name: Review, kind: check, by: you}
  - {id: pr, name: PR open, kind: delivery, agent: Shipper, skill: open-pr, mode: click, delivers: pr}
";
        otaq_core::flow::yaml::read(yaml).unwrap().flow
    }

    /// A 12-stage flow (`s24-04`): Grill, Plan, Dev, Validate, Tests, Security, Perf, A11y, QA, Review, PR, Deploy.
    fn twelve_stages() -> Flow {
        let yaml = "\
flow:
  - {id: grill, name: Grill, kind: agent, agent: Griller, skill: grill}
  - {id: plan, name: Plan, kind: agent, agent: Planner, skill: plan}
  - {id: dev, name: Dev, kind: agent, agent: Rust developer, skill: work-ticket}
  - {id: validate, name: Validate, kind: check, by: agent, agent: Validator, skill: validate, rounds: 3}
  - {id: tests, name: Tests, kind: agent, agent: Test runner, skill: e2e}
  - {id: security, name: Security, kind: check, by: agent, agent: Security reviewer, skill: security-review, rounds: 3}
  - {id: perf, name: Perf, kind: check, by: agent, agent: Perf reviewer, skill: perf, rounds: 3}
  - {id: a11y, name: A11y, kind: check, by: agent, agent: A11y reviewer, skill: a11y, rounds: 3}
  - {id: qa, name: QA, kind: check, by: you}
  - {id: review, name: Review, kind: check, by: you}
  - {id: pr, name: PR, kind: delivery, agent: Shipper, skill: open-pr, mode: click, delivers: pr}
  - {id: deploy, name: Deploy, kind: agent, agent: Deployer, skill: deploy}
";
        otaq_core::flow::yaml::read(yaml).unwrap().flow
    }

    /// The example's tickets on shop-web (*24 · The two views*), each in its place.
    fn shop_web_tickets() -> Vec<Ticket> {
        use otaq_core::board::tickets::AgentSignal;
        use otaq_core::domain::{Place, Priority};
        let at = |key: &str, title: &str, status, place: Place| {
            let mut t = ticket(key, title, status);
            t.place = place;
            t
        };
        let asks = |mut t: Ticket, text: &str| {
            t.signal = Some(AgentSignal {
                text: Some(text.into()),
                since: Some(NOW - 60_000),
                wakeup_at: None,
            });
            t
        };
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = Kind::Epic;
        let mut child = at(
            "SHOP-40.3",
            "Wishlist page",
            Status::Backlog,
            Place::Backlog,
        );
        child.epic = Some("SHOP-40".parse().unwrap());
        let mut p1 = at(
            "SHOP-35",
            "Slow checkout query",
            Status::Queued,
            Place::Waiting(None),
        );
        p1.priority = Priority::Urgent;
        p1.queued_at = Some(1);
        let mut waits = at(
            "SHOP-20",
            "Checkout page",
            Status::Queued,
            Place::Waiting(None),
        );
        waits.depends_on = vec!["SHOP-18".parse().unwrap()];
        waits.queued_at = Some(2);
        let mut working = at(
            "SHOP-33",
            "Shipping estimate",
            Status::Working,
            Place::stage_of("dev"),
        );
        working.flags.last_signal_at = Some(NOW - 180_000);
        let mut failed = at(
            "SHOP-32",
            "Address autocomplete",
            Status::CheckFailed,
            Place::stage_of("security"),
        );
        failed.validation_round = 3;
        failed.flags.check = Some("Security".into());
        let checking = |key: &str, title: &str, stage: &str, check: &str| {
            let mut t = at(key, title, Status::Checking, Place::stage_of(stage));
            t.flags.check = Some(check.into());
            t.flags.validating_round = Some(otaq_core::domain::ValidationRound { round: 1, of: 3 });
            t
        };
        let mut pr = at(
            "SHOP-26",
            "Add sitemap",
            Status::PrOpen,
            Place::stage_of("pr"),
        );
        pr.flags.pr_number = Some(481);
        let mut merged = vec![];
        for key in ["SHOP-1", "SHOP-2", "SHOP-3", "SHOP-4"] {
            merged.push(at(key, "Shipped", Status::Merged, Place::Done));
        }
        let mut tickets = vec![
            epic,
            at(
                "SHOP-31",
                "Hotfix: checkout crash",
                Status::Backlog,
                Place::Backlog,
            ),
            at(
                "SHOP-41",
                "Fix README typo",
                Status::Backlog,
                Place::Backlog,
            ),
            child,
            p1,
            waits,
            asks(
                at(
                    "SHOP-12",
                    "Add dark mode toggle",
                    Status::AsksYou,
                    Place::stage_of("grill"),
                ),
                "Follow the system setting?",
            ),
            asks(
                at(
                    "SHOP-22",
                    "Fix flaky login redirect",
                    Status::NeedsYou,
                    Place::stage_of("dev"),
                ),
                "Allow rm -rf .next?",
            ),
            working,
            checking("SHOP-30", "Order history page", "validate", "Validate"),
            checking("SHOP-19", "Price table", "security", "Security"),
            failed,
            at(
                "SHOP-18",
                "Checkout API",
                Status::WaitingForYou,
                Place::stage_of("review"),
            ),
            pr,
        ];
        tickets.extend(merged);
        tickets
    }

    fn line_of(
        page: &BoardPage,
        key: &str,
    ) -> (
        Option<String>,
        Option<String>,
        Option<crate::view_models::card::Tone>,
    ) {
        let card = page
            .columns
            .iter()
            .flat_map(cards_in)
            .find_map(|c| match c {
                BoardCard::Ticket { card, .. } if card.key == key => Some(card),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no card {key}"));
        (
            card.status.as_ref().map(|l| l.text.clone()),
            card.action.as_ref().map(|a| a.label.to_owned()),
            card.edge,
        )
    }

    fn cards_in(c: &ColumnPage) -> Vec<&BoardCard> {
        let dev = c.dev.iter().flat_map(|d| d.top.iter().chain(&d.working));
        let sections = c.sections.iter().flat_map(|s| &s.cards);
        c.cards.iter().chain(dev).chain(sections).collect()
    }

    /// BRD-02, BRD-03 on the example (`s24-03`): at 1440 (the 64 px rail, no pane) the seven columns fit and share
    /// the width, Done is its "Done 4 →" line, Ready says "Starts in priority order", and each card reads as drawn.
    #[test]
    fn the_example_board_fits_at_1440_with_one_line_cards() {
        use crate::view_models::card::Tone;
        let flow = shop_web();
        let tickets = shop_web_tickets();
        let page = super::page(
            &tickets,
            &ProjectFacts {
                flow: &flow,
                view: view(ViewPrefs::default()),
                ..facts()
            },
            &params(1440. - 64.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let open: Vec<&str> = page
            .columns
            .iter()
            .filter(|c| !c.collapsed)
            .map(|c| c.title.as_str())
            .collect();
        assert_eq!(
            open,
            [
                "Backlog",
                "Ready for Dev",
                "Grill",
                "Dev",
                "Security",
                "Review",
                "PR open \u{b7} PR"
            ]
        );
        assert_eq!(page.more_stages.at_start, None, "seven columns fit at 1440");
        assert_eq!(page.done.as_ref().unwrap().text, "Done 4 \u{2192}");
        assert_eq!(
            page.columns[1].hint.as_deref(),
            Some("Starts in priority order")
        );
        assert_eq!(page.toolbar.views.options[0].label, "Board");
        assert!(page.toolbar.views.options[0].selected);
        assert_eq!(page.list, None);

        let s = |text: &str| Some(text.to_owned());
        assert_eq!(
            line_of(&page, "SHOP-31"),
            (None, None, None),
            "a calm Backlog card"
        );
        assert_eq!(
            line_of(&page, "SHOP-22"),
            (
                s("Needs you: allow rm -rf .next?"),
                s("Answer"),
                Some(Tone::Attention)
            )
        );
        assert_eq!(
            line_of(&page, "SHOP-12"),
            (
                s("Asks you: follow the system setting?"),
                s("Reply"),
                Some(Tone::Attention)
            )
        );
        assert_eq!(line_of(&page, "SHOP-33"), (s("Working"), None, None));
        assert_eq!(
            line_of(&page, "SHOP-19").0,
            s("Checking \u{b7} round 1 of 3")
        );
        assert_eq!(
            line_of(&page, "SHOP-30").0,
            s("Validating \u{b7} round 1 of 3")
        );
        assert_eq!(
            line_of(&page, "SHOP-32"),
            (
                s("Security failed 3 times"),
                s("Decide"),
                Some(Tone::Attention)
            )
        );
        let check = line_of(&page, "SHOP-18");
        assert_eq!(
            (check.0, check.1, check.2),
            (s("Your check"), s("Review"), Some(Tone::Review))
        );
        let pr = line_of(&page, "SHOP-26").0.unwrap();
        assert!(pr.starts_with("PR #481 open"), "{pr}");
        assert_eq!(line_of(&page, "SHOP-26").1, None);
        // #36: the priority is the card's chip after its key, in every column, not the line's lead.
        let p1 = line_of(&page, "SHOP-35").0.unwrap();
        assert!(!p1.starts_with("P1"), "{p1}");
        let chip = |key: &str| {
            page.columns
                .iter()
                .flat_map(cards_in)
                .find_map(|c| match c {
                    BoardCard::Ticket { card, .. } if card.key == key => {
                        Some(card.priority.as_ref().map(|p| p.short.clone()))
                    }
                    _ => None,
                })
                .flatten()
        };
        assert_eq!(chip("SHOP-35").as_deref(), Some("P1"));
        let waits = line_of(&page, "SHOP-20").0.unwrap();
        assert_eq!(waits, "Blocked \u{b7} waits for SHOP-18 to merge");
        let child = page.columns[0].cards.iter().find_map(|c| match c {
            BoardCard::Ticket { card, .. } if card.key == "SHOP-40.3" => Some(card),
            _ => None,
        });
        // An epic child in Backlog is a row of its epic's card (EPC-11), so it isn't a card of its own.
        assert!(child.is_none());
        let working = page.columns[3]
            .dev
            .iter()
            .flat_map(|d| &d.working)
            .find_map(|c| match c {
                BoardCard::Ticket { card, .. } if card.key == "SHOP-33" => Some(card),
                _ => None,
            })
            .unwrap();
        assert_eq!(working.time.as_ref().unwrap().text, "3m");
    }

    /// BRD-02 on a 12-stage flow (`s24-04`): every column keeps 180 px, nothing folds by itself, and at 1440 the
    /// board scrolls with "7 more stages"; Fold empty columns folds the empty ones to rails.
    #[test]
    fn a_twelve_stage_flow_scrolls_with_seven_more_stages() {
        let flow = twelve_stages();
        let tickets = shop_web_tickets()
            .into_iter()
            .filter(|t| !matches!(t.place, otaq_core::domain::Place::InStage(_)))
            .collect::<Vec<_>>();
        let facts_of = |prefs: ViewPrefs| ProjectFacts {
            flow: &flow,
            view: view(prefs),
            ..facts()
        };
        let page = super::page(
            &tickets,
            &facts_of(ViewPrefs::default()),
            &params(1440. - 64.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let open = page.columns.iter().filter(|c| !c.collapsed).count();
        assert_eq!(open, 14, "Backlog, Ready for Dev and twelve stages");
        assert_eq!(page.more_stages.at_start.as_deref(), Some("7 more stages"));
        assert_eq!(
            (
                page.more_stages.one.as_str(),
                page.more_stages.many.as_str()
            ),
            ("1 more stage", "{n} more stages")
        );
        let folded = super::page(
            &tickets,
            &facts_of(ViewPrefs {
                fold_empty_columns: true,
                ..ViewPrefs::default()
            }),
            &params(1440. - 64.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let open: Vec<&str> = folded
            .columns
            .iter()
            .filter(|c| !c.collapsed)
            .map(|c| c.title.as_str())
            .collect();
        assert_eq!(open, ["Backlog", "Ready for Dev"]);
        assert_eq!(folded.more_stages.at_start, None, "the rails fit");
        assert!(folded.toolbar.switch.as_ref().unwrap().selected);
        let grill = folded.columns[2].rail.as_ref().unwrap();
        assert_eq!(grill.tooltip, "Grill \u{b7} 0 tickets");
        assert!(!grill.dot);
    }

    /// `s26-10`: Grill folded from its ⋯ is a rail with SHOP-12's needs-you dot.
    #[test]
    fn a_folded_column_has_a_dot_when_something_needs_you() {
        let flow = shop_web();
        let tickets = shop_web_tickets();
        let page = super::page(
            &tickets,
            &ProjectFacts {
                flow: &flow,
                view: view(ViewPrefs {
                    folded_columns: vec!["done".into(), "grill".into()],
                    ..ViewPrefs::default()
                }),
                ..facts()
            },
            &params(1440. - 64.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let grill = &page.columns[2];
        assert!(grill.collapsed);
        let rail = grill.rail.as_ref().unwrap();
        assert_eq!(rail.tooltip, "Grill \u{b7} 1 ticket, 1 needs you");
        assert!(rail.dot);
        assert_eq!(
            rail.unfold,
            Change::FoldColumn {
                column: "grill".into(),
                folded: false
            }
        );
        let list = super::page(
            &tickets,
            &ProjectFacts {
                flow: &flow,
                view: view(ViewPrefs {
                    view: otaq_core::board::settings::BoardView::List,
                    ..ViewPrefs::default()
                }),
                ..facts()
            },
            &params(1440. - 64.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        assert!(list.list.is_some());
        assert_eq!(list.prefs.view, ViewKind::List);
    }
}

//! The board as the Tauri page draws it (T-0220, T-0324, T-0404; UI-02, SHL-06, SHL-01, TKT-06, CLS-03, CLS-06,
//! REV-14, STG-08; d0087, d0108): [`board::build`]'s columns of the project's flow (on the default flow today's
//! six) turned into plain, serialized shapes with every text already worded (counts, hints, the Dev section and
//! fold, the folded columns' rails and Done's line, the empty board, the "Created or changed by" chip, the toolbar
//! and the view preferences of SHB-05), each with its kind, so the page draws any flow without naming a column.
//! Every stage is its own column and its own drop target; the delivery is no column (a ticket being delivered is in
//! the last stage's). The drag is the UIT-04 way: [`drag_answer_in`] gives every drop target's verdict for one
//! ticket at once, from the core's `check_move_in` on the project's flow ([`drag::feedback_in`],
//! [`drag::drop_hint_in`]), so the page draws the whole drag without asking again; a drop then asks once more what
//! happens ([`drop_step_in`]): a move, the Park confirmation ([`ParkDialog`]) or, back to an earlier Agent stage or
//! agent check, the drag-back dialog ([`DragBackDialog`], FLW-18).

use std::borrow::Cow;

use otaq_core::board::moves::{DropOn, MoveAction, STARTS_BY_ITSELF, check_move_in};
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
    self, BoardInput, CardEntry, ColumnView, DevSections, HeaderTooltip,
};
use crate::view_models::card_page::TicketCardView;
use crate::view_models::chain;
use crate::view_models::check_dialogs;
use crate::view_models::close;
use crate::view_models::drag::{self, DropAction, DropFeedback};
use crate::view_models::empty_board;
use crate::view_models::filter::Scope;
use crate::view_models::flow_columns::{self, ColumnKind, Columns};
use crate::view_models::hint;
use crate::view_models::list_view;
use crate::view_models::search_page::FilterState;
use crate::view_models::ticket_comments::CommentBoxView;
use crate::view_models::view_prefs::{self, Change, Prefs, Toolbar, ViewKind};
use otaq_core::board::settings::ViewPrefs;

/// A page column's id (SHL-06, STG-08): `"backlog"`, the id of a stage with its own column
/// (the default flow's `"dev"` and `"review"`), `"done"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(rename = "BoardColumnId")]
pub struct ColumnId(#[ts(type = "string")] pub Cow<'static, str>);

impl ColumnId {
    pub const BACKLOG: Self = Self::fixed(flow_columns::BACKLOG);
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

/// Where a card can be dropped (d0087, UIT-04, STG-08): a column. On the default flow `"review"` is Review.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(rename = "BoardDropTarget")]
pub struct DropTargetId(#[ts(type = "string")] pub Cow<'static, str>);

impl DropTargetId {
    pub const BACKLOG: Self = Self::fixed(flow_columns::BACKLOG);
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

    /// Every target of `flow`'s board, in board order: what a drag answers for.
    pub fn all(flow: &Flow) -> Vec<Self> {
        Columns::of(flow)
            .targets()
            .into_iter()
            .map(Self::new)
            .collect()
    }
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
}

impl BoardCard {
    pub fn key(&self) -> &str {
        match self {
            Self::Ticket { card, .. } => &card.key,
        }
    }
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

/// Who runs a column's stages, on hovering its title (FLW-12): a bold first line and detail lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardColumnTooltip")]
pub struct ColumnTooltip {
    /// "Product spec · Spec writer · product-spec".
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
    /// "Product spec · 2 tickets, 1 needs you".
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
    /// Add a stage after <stage>: a new stage right after `after` (`None`: first, after Backlog), selected
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
    pub count: String,
    /// Who runs it, on hovering the title; `None` for Backlog and Done.
    pub tooltip: Option<ColumnTooltip>,
    /// Who acts there, in plain words under the title ("An agent does the work", "You review and approve").
    pub who_acts: String,
    /// Shown when the column has no card.
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
    /// Every column but the agent columns.
    pub cards: Vec<BoardCard>,
    pub dev: Option<DevPage>,
    /// Backlog's "Add ticket": the + in its header (and its row); the same as ⌘N.
    pub add: Option<String>,
    /// "changing": a saved flow change waits for the tickets in this stage ([`mark_changing`]).
    pub changing: Option<String>,
}

/// The mark on a column a saved flow change waits for.
pub const CHANGING: &str = "changing";

/// Marks the columns a waiting flow change waits for "changing": the stages of its span, or every stage when it
/// replaces the whole flow. Backlog and Done never change.
pub fn mark_changing(page: &mut BoardPage, wait: &otaq_core::flow::apply::Wait) {
    use otaq_core::flow::apply::Wait;
    for column in &mut page.columns {
        let stage = !matches!(column.kind, ColumnKind::Backlog | ColumnKind::Done);
        let covered = match wait {
            Wait::WholeBoard => stage,
            Wait::Span(span) => span.stages.iter().any(|id| column.id == *id.as_str()),
        };
        column.changing = covered.then(|| CHANGING.to_owned());
    }
}

/// A new user's first run on this board: "Your first ticket" in Backlog while the project has no tickets and the
/// card was never answered, and the one first-run hint whose moment is here now. Off while the board is filtered,
/// where the cards a hint points at may be hidden; on the List only the hints at Edit flow and the box for new work.
pub fn first_run(
    page: &mut BoardPage,
    facts: &hint::BoardFacts,
    tool: Option<otaq_core::agent_tools::Tool>,
) {
    page.first_ticket = (empty_board::shows(facts.tickets.len(), facts.seen)
        && page.list.is_none())
    .then(|| empty_board::first_ticket(tool));
    page.hint = (page.filter_strip.is_none() && page.agent_filter.is_none())
        .then(|| hint::board(facts))
        .flatten()
        .filter(|h| page.list.is_none() || !matches!(h.anchor, hint::Anchor::Card { .. }));
}

/// The chip of a column scrolled down:`"{n} above"`, then `" · {n} need you"` when any of them need the user. The
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
    /// "Your first ticket" in Backlog: a new user's first project with no tickets ([`empty_board`]); set by
    /// [`first_run`].
    pub first_ticket: Option<empty_board::FirstTicket>,
    /// The first-run hint whose moment is on this board now ([`hint::board`]); set by [`first_run`].
    pub hint: Option<hint::BoardHint>,
    pub scroll_chip: ScrollChip,
    /// What waits for each card others wait for, lit while it is hovered or selected.
    pub chains: Vec<chain::View>,
    /// The filter strip under the toolbar while the search filters the board; `null` otherwise.
    pub filter_strip: Option<FilterStrip>,
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
    /// The project's view preferences (SHB-05): Board or List, grouping, folds, dependencies.
    pub prefs: Prefs,
    /// The toolbar of both views (BRD-01).
    pub toolbar: Toolbar,
    /// The List view (LST-01, T-0461); `null` on the Board.
    pub list: Option<list_view::ListView>,
    /// "Edit flow", at the end of the board in either layout (after the columns, or after the List's rows): the board
    /// turns into the flow's editor in place, as columns. `null` on Files and Settings.
    pub edit_flow: Option<String>,
}

/// Edit flow's button at the end of the columns.
pub const EDIT_FLOW: &str = "Edit flow";

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
    /// A Project agent's id: only the tickets it created or changed show (d0087, "Show on board" in the agent panel). It
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
    /// The project's helpers (`board.helpers`, d0133): a build stage's piece rows.
    pub helpers: &'a [otaq_core::helpers::Helper],
}

fn card(entry: CardEntry, tickets: &[Ticket]) -> Option<BoardCard> {
    let CardEntry::Ticket(vm) = entry;
    let ticket = tickets.iter().find(|t| t.key == vm.key)?;
    Some(BoardCard::Ticket {
        needs_you: ticket.needs_user(),
        card: Box::new(TicketCardView::from_vm(vm, ticket)),
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
        tooltip: view.tooltip.map(ColumnTooltip::from),
        who_acts: view.who_acts,
        empty_hint: view.empty_hint.to_owned(),
        collapsed: view.collapsed,
        rail,
        menu,
        menu_label: COLUMN_MENU.to_owned(),
        hidden: hidden_text(view.hidden),
        cards: cards(view.cards, tickets),
        dev,
        add: (kind == ColumnKind::Backlog).then(|| "Add ticket".to_owned()),
        changing: None,
    }
}

/// "Product spec · 2 tickets, 1 needs you" (`s26-10`).
pub fn rail_tooltip(title: &str, tickets: usize, needs_you: usize) -> String {
    let s = if tickets == 1 { "" } else { "s" };
    match needs_you {
        0 => format!("{title} \u{b7} {tickets} ticket{s}"),
        n => format!("{title} \u{b7} {tickets} ticket{s}, {n} needs you"),
    }
}

/// A column's ⋯ menu (BRD-04, FED-09; `s26-08`): **Edit <stage>…** (a stage's column), **Add a stage after
/// <stage>** (after Backlog: the first stage, since Backlog and Done are the fixed ends of every flow), **Fold this
/// column** (not Backlog, which never folds), then **Change the whole flow…**. `title` is the column's, its stage's
/// name.
pub fn column_menu(id: &ColumnId, kind: ColumnKind, title: &str) -> Vec<ColumnMenuItem> {
    use crate::view_models::menu::MenuItem;
    let stage = title;
    let opens = |item: MenuItem, editor: OpenEditor| ColumnMenuItem {
        item,
        change: None,
        editor: Some(editor),
    };
    let mut menu = Vec::new();
    if matches!(kind, ColumnKind::Agent | ColumnKind::HumanCheck) {
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
        ColumnKind::Backlog | ColumnKind::Agent | ColumnKind::HumanCheck
    ) {
        menu.push(opens(
            MenuItem::new("add_stage", format!("Add a stage after {stage}")).icon(IconName::Plus),
            OpenEditor::AddAfter {
                after: (kind != ColumnKind::Backlog).then(|| id.as_str().to_owned()),
            },
        ));
    }
    if kind != ColumnKind::Backlog {
        menu.push(ColumnMenuItem {
            item: MenuItem::new("fold", FOLD_THIS_COLUMN).icon(IconName::FoldHorizontal),
            change: Some(Change::FoldColumn {
                column: id.as_str().to_owned(),
                folded: true,
            }),
            editor: None,
        });
    }
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

/// The board for `tickets` of one project, at `params`' size.
pub fn page(
    tickets: &[Ticket],
    project: &ProjectFacts,
    params: &Params,
    now_ms: i64,
    keys: &KeyLabels,
) -> BoardPage {
    let scope = Scope::of(project.flow, tickets, project.agents, project.agent_list);
    let filter = params.filter.filter(&scope);
    let mut visible = filter.visible(tickets, &scope);
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
    let prefs = project.view;
    let input = BoardInput {
        flow: project.flow,
        tickets,
        visible: visible.as_deref(),
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
        helpers: project.helpers,
    };
    let view: ViewKind = prefs.view.into();
    let list = (view == ViewKind::List).then(|| list_view::view(&input, prefs));
    let built = board::build(&input);
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
        columns: built.into_iter().map(|c| column_page(c, tickets)).collect(),
        first_ticket: None,
        hint: None,
        scroll_chip: ScrollChip {
            above: SCROLL_CHIP_ABOVE.to_owned(),
            need_you: SCROLL_CHIP_NEED_YOU.to_owned(),
        },
        chains: chain::views(
            &crate::view_models::flow_columns::Columns::of(project.flow),
            tickets,
        ),
        filter_strip: filter_strip(
            &params.filter,
            visible.as_ref().map_or(tickets.len(), Vec::len),
            tickets.len(),
        ),
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
        edit_flow: matches!(view, ViewKind::Board | ViewKind::List).then(|| EDIT_FLOW.to_owned()),
    }
}

/// How a column looks while the card is over it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "DragVerdict")]
pub enum Verdict {
    /// The card's own column: no styling, dropping does nothing.
    Home,
    /// Accent outline and the hint under the pointer, with a second line under it when there is one.
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

/// One chip of the filter strip: a token (`is:needs-you`) or the typed words (“checkout”). Its ✕ removes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilterStripChip")]
pub struct FilterStripChip {
    pub label: String,
    /// The token it stands for; `null` for the typed words.
    pub token: Option<String>,
}

/// The board's filter strip, under the toolbar while the search filters the board: "Showing 3 of 17", the active
/// filters as chips, one filter to add ("+ is:needs-you") while it isn't on, and Clear. The search field shows the same
/// filter: both read and change the one filter the board keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilterStrip")]
pub struct FilterStrip {
    pub showing: String,
    pub chips: Vec<FilterStripChip>,
    /// The filter offered to add, as a token; `null` once it is on.
    pub suggest: Option<String>,
    pub suggest_label: Option<String>,
    pub clear: String,
}

/// The filter the strip offers to add while it isn't on.
pub const SUGGESTED_FILTER: &str = "is:needs-you";

/// The strip for `filter`, showing `shown` of `total` tickets; `None` while nothing filters the board.
pub fn filter_strip(filter: &FilterState, shown: usize, total: usize) -> Option<FilterStrip> {
    let text = filter.text.trim();
    if filter.tokens.is_empty() && text.is_empty() {
        return None;
    }
    let mut chips: Vec<FilterStripChip> = filter
        .tokens
        .iter()
        .map(|t| FilterStripChip {
            label: t.clone(),
            token: Some(t.clone()),
        })
        .collect();
    if !text.is_empty() {
        chips.push(FilterStripChip {
            label: format!("\u{201c}{text}\u{201d}"),
            token: None,
        });
    }
    let suggest =
        (!filter.tokens.iter().any(|t| t == SUGGESTED_FILTER)).then(|| SUGGESTED_FILTER.to_owned());
    Some(FilterStrip {
        showing: format!("Showing {shown} of {total}"),
        chips,
        suggest_label: suggest.as_ref().map(|t| format!("+ {t}")),
        suggest,
        clear: "Clear".to_owned(),
    })
}

/// One drop target's verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DropTargetVerdict")]
pub struct TargetVerdict {
    pub target: DropTargetId,
    pub verdict: Verdict,
}

/// The answer to `board.drag`, asked once when a drag starts: a verdict for every drop target (every column,
/// Review's Approved section on its own). `tickets` are the project's.
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
        DropFeedback::Forbidden { title, .. }
            if title == STARTS_BY_ITSELF
                && let Some(waits) = waits_ahead(flow, ticket, tickets, &column) =>
        {
            waits
        }
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
                note: None,
            }
        }
    }
}

/// A started ticket still waiting to begin, dragged ahead to a stage: refused, and the toast says what it waits for
/// ("SHOP-30 can't go to Validate yet" · "It waits for SHOP-27 to merge, then starts by itself."; "It waits for a free
/// agent, then starts by itself."). Only for the core's "starts by itself" refusal: a Backlog ticket is never started
/// by itself, and Review is only ever reached through an agent, so their own refusals stand.
fn waits_ahead(
    flow: &Flow,
    ticket: &Ticket,
    tickets: &[Ticket],
    column: &DropOn,
) -> Option<Verdict> {
    let Some(Target::Stage(stage)) = column.in_flow(flow) else {
        return None;
    };
    let parents = crate::view_models::dependencies::waiting_for(ticket, tickets);
    let detail = if !parents.is_empty() {
        let names: Vec<String> = parents.iter().map(|p| p.key.to_string()).collect();
        format!(
            "It waits for {} to merge, then starts by itself.",
            crate::view_models::dependencies::and_list(&names)
        )
    } else if ticket.status == Status::WaitingForSlot {
        "It waits for a free agent, then starts by itself.".to_owned()
    } else {
        format!("{STARTS_BY_ITSELF}.")
    };
    Some(Verdict::Forbidden {
        title: format!("{} can\u{2019}t go to {} yet", ticket.key, stage.name),
        detail: Some(detail),
    })
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

/// The Approve menu of a ticket waiting for the user's Approve (UI-02, T-0095): what the flow's delivery allows,
/// named by its action, and the other ways to finish it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ApproveMenu")]
pub struct ApproveMenu {
    pub key: String,
    pub title: String,
    pub items: Vec<ApproveItem>,
    pub note: String,
    /// "Your flow delivers either way. Change it at the Done end of Edit flow.": under the delivery's choices.
    pub delivery_note: String,
    /// The one enabled choice when there is no other: the page opens its dialog straight away, without the choice
    /// (dogfood #18).
    pub direct: Option<ApproveChoice>,
}

fn choice_icon(choice: ApproveChoice) -> IconName {
    match choice {
        ApproveChoice::Done => IconName::CircleCheck,
        ApproveChoice::LocalMerge => IconName::GitMerge,
        ApproveChoice::OpenPr => IconName::GitPullRequest,
        ApproveChoice::PushToPr => IconName::Upload,
        ApproveChoice::MergeInto => IconName::GitBranch,
        ApproveChoice::DoneNotMerged => IconName::CircleMinus,
    }
}

/// `commits`: the commits on the branch beyond its base, `None` when unknown; `delivers`: what the flow's delivery
/// allows ([`approve::options`]).
pub fn approve_menu(
    ticket: &Ticket,
    target: &str,
    tickets: &[Ticket],
    pr: Option<&PrProvider>,
    commits: Option<u32>,
    delivers: otaq_core::flow::Delivers,
) -> ApproveMenu {
    let items: Vec<ApproveItem> = approve::options(ticket, target, tickets, pr, commits, delivers)
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
        .collect();
    let mut enabled = items.iter().filter(|i| i.enabled).map(|i| i.choice);
    let direct = match (enabled.next(), enabled.next()) {
        // Done has no confirmation of its own: the menu is where the user says it.
        (Some(ApproveChoice::Done), None) => None,
        (Some(only), None) => Some(only),
        _ => None,
    };
    let done_leads = items
        .first()
        .is_some_and(|i| i.choice == ApproveChoice::Done);
    ApproveMenu {
        key: ticket.key.to_string(),
        title: approve::menu_title(ticket),
        delivery_note: approve::delivery_note(ticket, target, delivers),
        items,
        note: if done_leads {
            approve::DONE_NOTE.to_owned()
        } else {
            approve::menu_note(ticket)
        },
        direct,
    }
}

/// What a drop on `column` asks of the app, before anything runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropStep {
    /// Its own column, or refused: the card flies back.
    Nothing,
    /// Send `ticket.move`.
    Move,
    /// Dev → Backlog: the Park confirmation first.
    Park,
    /// Back to an earlier Agent stage or agent check (FLW-18): the drag-back dialog first ([`drag_back_dialog`]);
    /// Cancel leaves the card where it was.
    DragBack,
    NotYet(String),
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

/// `tickets` are the project's. `flow` is the project's. A ticket waiting for a dependency starts like any other: it
/// waits at the top of the first stage and says what it waits for.
pub fn drop_step_in(
    flow: &Flow,
    ticket: &Ticket,
    tickets: &[Ticket],
    target: &DropTargetId,
) -> DropStep {
    let Some(target) = core_target(flow, target) else {
        return DropStep::Nothing;
    };
    match drag::on_drop_in(flow, ticket, tickets, target) {
        DropAction::Nothing => DropStep::Nothing,
        DropAction::Move => DropStep::Move,
        DropAction::AskPark => DropStep::Park,
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
    /// The page shows the drag-back dialog; its confirm sends `board.drag_back` (FLW-18).
    DragBack { dialog: Box<DragBackDialog> },
    /// Allowed by the rules, but not built yet: shown as a toast.
    NotYet { message: String },
    /// git has no name and email to sign the agent's commits with: nothing started. The page asks for them; its
    /// "Save and start" saves them (`tools.git_identity`) and starts again.
    Identity {
        form: crate::view_models::git_identity::Form,
        /// "Save and start".
        save: &'static str,
        cancel: &'static str,
    },
}

/// The question before a start when git has no name and email (an agent never commits under a borrowed name).
pub fn identity_outcome(has: &otaq_core::git::Identity) -> DropOutcome {
    DropOutcome::Identity {
        form: crate::view_models::git_identity::form(has),
        save: SAVE_AND_START,
        cancel: "Cancel",
    }
}

/// The identity question's button: saves the name and email, then starts.
pub const SAVE_AND_START: &str = "Save and start";

/// Where a card is, as the board names it: its column ("Dev", "Review").
pub fn place_label(columns: &Columns, ticket: &Ticket) -> String {
    let spot = columns.spot(ticket);
    columns
        .get(spot)
        .map_or_else(|| spot.to_owned(), |column| column.title.clone())
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
    pub first_ticket: empty_board::FirstTicket,
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
        agent_filter: None,
        agents: None,
        agent_list: None,
        starts_held: None,
        view: &view,
        paused: false,
        helpers: &[],
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
        first_ticket: empty_board::first_ticket(Some(
            super::agent_tools_page::fixtures::tools()[0],
        )),
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
            name: "List".into(),
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
        agent_filter: None,
        agents: None,
        agent_list: None,
        starts_held: None,
        view: &view,
        paused: false,
        helpers: &[],
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

    /// A started ticket waiting to begin, dragged ahead, is refused with what it waits for: the tickets it waits for
    /// to merge, else a free agent, else the plain "starts by itself".
    #[test]
    fn a_waiting_ticket_dragged_ahead_says_what_it_waits_for() {
        use otaq_core::domain::Place;
        let flow = &otaq_core::flow::default_file().flow;
        let mut parent = ticket("SHOP-27", "Price table", Status::WaitingForYou);
        parent.place = Place::stage_of("review");
        let mut waits = ticket("SHOP-30", "Checkout", Status::Queued);
        waits.place = Place::Waiting(None);
        waits.depends_on = vec!["SHOP-27".parse().unwrap()];
        let tickets = vec![parent, waits.clone()];
        let answer = drag_answer_in(flow, &waits, &tickets, None);
        assert_eq!(
            verdict_of(&answer, "validate"),
            Verdict::Forbidden {
                title: "SHOP-30 can\u{2019}t go to Validate yet".to_owned(),
                detail: Some("It waits for SHOP-27 to merge, then starts by itself.".to_owned()),
            }
        );

        let mut slot = ticket("SHOP-31", "Footer", Status::WaitingForSlot);
        slot.place = Place::Waiting(None);
        let answer = drag_answer_in(flow, &slot, std::slice::from_ref(&slot), None);
        assert_eq!(
            verdict_of(&answer, "validate"),
            Verdict::Forbidden {
                title: "SHOP-31 can\u{2019}t go to Validate yet".to_owned(),
                detail: Some("It waits for a free agent, then starts by itself.".to_owned()),
            }
        );

        let mut queued = ticket("SHOP-32", "Header", Status::Queued);
        queued.place = Place::Waiting(None);
        let answer = drag_answer_in(flow, &queued, std::slice::from_ref(&queued), None);
        assert!(matches!(
            verdict_of(&answer, "validate"),
            Verdict::Forbidden { title, detail: Some(detail) }
                if title == "SHOP-32 can\u{2019}t go to Validate yet" && *detail == format!("{STARTS_BY_ITSELF}.")
        ));

        // A Backlog ticket that waits keeps its own refusal: Backlog never starts by itself.
        let mut backlog = ticket("SHOP-33", "Banner", Status::Backlog);
        backlog.place = Place::Backlog;
        backlog.depends_on = vec!["SHOP-27".parse().unwrap()];
        let answer = drag_answer_in(flow, &backlog, &tickets, None);
        assert!(matches!(
            verdict_of(&answer, "validate"),
            Verdict::Forbidden { title, .. } if !title.contains("yet")
        ));
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
                "product-spec",
                "dev",
                "validate",
                "security",
                "qa",
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
            hint("product-spec"),
            "Drop to send back to Product spec \u{b7} asks first"
        );
        assert_eq!(
            hint("security"),
            "Drop to run Security again \u{b7} asks first"
        );
        // Forward from Dev between turns: Validate is the next stage; Security skips it.
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
        // A ticket being delivered stays in QA, the last stage: dropped there, nothing happens; back is a drag back.
        let ship = on_test_flow("SHOP-26", "qa", Status::ToDeliver);
        let answer = drag_answer_in(&flow, &ship, &[], None);
        assert_eq!(verdict_of(&answer, "qa"), Verdict::Home);
        assert_eq!(
            drop_step_in(&flow, &ship, &[], &DropTargetId::new("product-spec")),
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
            hint("product-spec"),
            "Drop to resume \u{b7} same conversation"
        );
        assert_eq!(
            drop_step_in(&flow, &parked, &[], &DropTargetId::new("qa")),
            DropStep::Move
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
            Some("Then it goes on to Validate, Security and QA as usual.")
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
            Some("Then it goes on to Validate and Security as usual.")
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
            Some("Then it goes on to Validate and Review as usual.")
        );
        // A drop that asks nothing has no dialog.
        assert_eq!(
            drag_back_dialog(&flow, &review, &DropTargetId::REVIEW, &[]),
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
            agent_filter: None,
            agents: None,
            agent_list: None,
            starts_held: None,
            view: view(ViewPrefs {
                folded_columns: Vec::new(),
                ..ViewPrefs::default()
            }),
            paused: false,
            helpers: &[],
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
        let mut pr = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        vec![
            ticket("SHOP-24", "Update footer links", Status::Backlog),
            pinned,
            ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou),
            ticket("SHOP-27", "Search results paging", Status::Working),
            ticket("SHOP-21", "Order confirmation email", Status::Queued),
            ticket("SHOP-18", "Checkout API", Status::WaitingForYou),
            pr,
            ticket("SHOP-17", "Add Stripe client", Status::Merged),
        ]
    }

    /// SHL-06, FLW-01: five columns in order, Backlog and Done at the ends (Validate its own); a started ticket waiting
    /// to begin is at the top of Dev; Review holds its cards with no sections.
    #[test]
    fn five_columns_reach_the_page_worded() {
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
                ColumnId::DEV,
                ColumnId::new("validate"),
                ColumnId::REVIEW,
                ColumnId::DONE
            ]
        );
        let titles: Vec<&str> = page.columns.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["Backlog", "Dev", "Validate", "Review", "Done"]);
        assert_eq!(page.min_column_width, 240.);
        assert_eq!(page.folded_width, 52.);
        assert_eq!(page.first_ticket, None);
        let backlog = &page.columns[0];
        assert_eq!(backlog.add.as_deref(), Some("Add ticket"));
        let dev = page.columns[1].dev.as_ref().unwrap();
        let top: Vec<&str> = dev.top.iter().map(BoardCard::key).collect();
        assert_eq!(
            top,
            ["SHOP-21", "SHOP-19", "SHOP-22"],
            "waiting to begin, then pinned, then needs you"
        );
        assert!(matches!(
            dev.top[2],
            BoardCard::Ticket {
                needs_you: true,
                ..
            }
        ));
        let review = &page.columns[3];
        assert_eq!(review.count, "2");
        // No sections: the PR open card is in Review with the one waiting for you, after it.
        let cards: Vec<&str> = review.cards.iter().map(BoardCard::key).collect();
        assert_eq!(cards, ["SHOP-18", "SHOP-26"]);
        let done = &page.columns[4];
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
        assert_eq!(json["columns"][1]["id"], "dev");
        assert_eq!(json["columns"][0]["cards"][0]["kind"], "ticket");
        assert_eq!(json["scroll_chip"]["above"], "{n} above");
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
        let done = &first.columns[4];
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
        let dev = &dev_folded.columns[1];
        let rail = dev.rail.as_ref().unwrap();
        assert!(rail.dot);
        assert!(rail.tooltip.starts_with("Dev \u{b7} "), "{}", rail.tooltip);
        assert!(rail.tooltip.ends_with("needs you"), "{}", rail.tooltip);
        assert_eq!(rail.hint, "Click to unfold");
        assert!(
            dev_folded.columns[0]
                .menu
                .iter()
                .all(|m| m.item.id != "fold"),
            "Backlog never folds"
        );
    }

    /// BRD-04, FED-09 (`s26-08`): a stage's ⋯ edits it, adds one after it, folds it, or changes the whole flow; each
    /// flow item says how the editor opens. Backlog adds first and doesn't fold; Done only folds and changes the
    /// flow.
    #[test]
    fn a_columns_menu_opens_the_flow_editor_on_its_stage() {
        let product_spec = column_menu(
            &ColumnId::new("product-spec"),
            ColumnKind::Agent,
            "Product spec",
        );
        let rows: Vec<(&str, &str, Option<&str>, bool)> = product_spec
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
                    "Edit Product spec\u{2026}",
                    Some("agent, skill, options"),
                    false
                ),
                ("add_stage", "Add a stage after Product spec", None, false),
                ("fold", "Fold this column", None, false),
                (
                    "change_flow",
                    "Change the whole flow\u{2026}",
                    Some("the same screen as onboarding"),
                    true
                ),
            ]
        );
        let editors: Vec<Option<OpenEditor>> =
            product_spec.iter().map(|m| m.editor.clone()).collect();
        assert_eq!(
            editors,
            [
                Some(OpenEditor::Stage {
                    stage: "product-spec".into()
                }),
                Some(OpenEditor::AddAfter {
                    after: Some("product-spec".into())
                }),
                None,
                Some(OpenEditor::Flow),
            ]
        );
        assert!(product_spec[2].change.is_some() && product_spec[0].change.is_none());
        let json = serde_json::to_value(&product_spec[1].editor).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "open": "add_after", "after": "product-spec" })
        );

        let backlog = column_menu(&ColumnId::BACKLOG, ColumnKind::Backlog, "Backlog");
        assert_eq!(backlog[0].item.label, "Add a stage after Backlog");
        assert_eq!(
            backlog[0].editor,
            Some(OpenEditor::AddAfter { after: None })
        );
        let ids: Vec<&str> = backlog.iter().map(|m| m.item.id.as_str()).collect();
        assert_eq!(
            ids,
            ["add_stage", "change_flow"],
            "no Edit and no Fold: Backlog isn't a stage and never folds"
        );

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
        let dev = folded.columns[1].dev.as_ref().unwrap();
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
        assert_eq!(open.columns[1].dev.as_ref().unwrap().fold, None);
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
        let dev = &filtered.columns[1];
        let shown: Vec<&str> = dev
            .dev
            .as_ref()
            .unwrap()
            .top
            .iter()
            .map(BoardCard::key)
            .collect();
        assert_eq!(shown, ["SHOP-22"]);
        assert_eq!(dev.count, "1/4");
        assert_eq!(dev.hidden.as_deref(), Some("3 hidden by filter"));
        assert!(filtered.columns[0].cards.is_empty());
        assert_eq!(filtered.columns[3].count, "1/2");
        assert_eq!(
            filtered.columns[3].hidden.as_deref(),
            Some("1 hidden by filter")
        );
        let review: Vec<&str> = filtered.columns[3]
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
        assert_eq!(unfiltered.columns[1].hidden, None);
    }

    fn first_run_on(page: &mut BoardPage, facts: &hint::BoardFacts) {
        first_run(page, facts, Some(otaq_core::agent_tools::Tool::Claude));
    }

    #[test]
    fn a_new_users_empty_project_shows_its_columns_with_the_first_ticket() {
        let keys = KeyLabels::base(Platform::Mac);
        let mut page = page(&[], &facts(), &params(1376.), NOW, &keys);
        let flow = otaq_core::flow::default::default_flow();
        let first_run = |seen: &'static [otaq_core::store::settings::Hint]| hint::BoardFacts {
            tickets: &[],
            flow: &flow,
            seen,
            first_approve: false,
            keys: &keys,
        };
        first_run_on(&mut page, &first_run(&[]));
        let card = page.first_ticket.clone().unwrap();
        assert_eq!(card.title, "Tidy the README");
        assert_eq!(page.hint, None);
        // Answered (Create & start or Not now): an empty Backlog with its plain Add ticket row.
        first_run_on(
            &mut page,
            &first_run(&[otaq_core::store::settings::Hint::FirstTicket]),
        );
        assert_eq!(page.first_ticket, None);
        assert_eq!(page.columns[0].add.as_deref(), Some("Add ticket"));
        assert_eq!(page.columns.len(), 5);
        let review = &page.columns[3];
        assert!(review.cards.is_empty());
        assert_eq!(
            review.empty_hint,
            "Tickets wait here for you when their agent is done."
        );
    }

    #[test]
    fn a_drag_answers_every_column_at_once() {
        let backlog = ticket("SHOP-24", "Update footer links", Status::Backlog);
        let answer = drag_answer(&backlog, &[]);
        assert_eq!(answer.targets.len(), 5);
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
            verdict(DropTargetId::DEV),
            Verdict::Allowed {
                icon: IconName::Play,
                hint: "Drop to start · waits here until it can begin".into(),
                note: None,
            }
        );
        assert_eq!(
            verdict(DropTargetId::REVIEW),
            Verdict::Forbidden {
                title: "Only agents move tickets to your review".into(),
                detail: Some("Drag it to the first stage to start it.".into())
            },
            "*Drag · over a forbidden column*"
        );
        let json = serde_json::to_value(drag_answer(&backlog, &[])).unwrap();
        assert_eq!(json["targets"][0]["verdict"]["kind"], "home");
        assert_eq!(json["targets"][4]["target"], "done");
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

    /// §7.4: every source column over the Review column, answered up front. A ticket being delivered (PR open) is in
    /// Review already: dropped there, nothing happens.
    #[test]
    fn review_verdicts_for_every_source() {
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
        let rows: Vec<(String, String)> = sources
            .iter()
            .map(|t| {
                (
                    t.key.to_string(),
                    kind(&verdict_on(t, DropTargetId::REVIEW)),
                )
            })
            .collect();
        let only_agents = "forbidden: Only agents move tickets to your review";
        let mid_turn = "forbidden: SHOP-25 can't move to the next stage now: its agent is in the middle of a turn";
        let row = |key: &str, review: &str| (key.to_owned(), review.to_owned());
        assert_eq!(
            rows,
            [
                row("SHOP-24", only_agents),
                row("SHOP-21", only_agents),
                // A forward drag is Move to next stage's (FLW-18): not while the agent is mid-turn.
                row("SHOP-25", mid_turn),
                row("SHOP-18", "home"),
                row("SHOP-26", "home"),
                row("SHOP-30", only_agents),
                row("SHOP-17", "forbidden: Merged tickets are locked"),
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

    /// Approving is the card's Approve, never a drop: the board has no Approved section, and a card dropped on the
    /// column it is in (Review, also for a PR open card) does nothing.
    #[test]
    fn no_approve_on_drop_and_no_approved_target() {
        let review = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        assert_eq!(
            drop_step(&review, &[], DropTargetId::REVIEW),
            DropStep::Nothing,
            "home"
        );
        let mut pr = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        assert_eq!(drop_step(&pr, &[], DropTargetId::REVIEW), DropStep::Nothing);
        let working = ticket("SHOP-25", "Upgrade to Next.js 15", Status::Working);
        assert_eq!(
            drop_step(&working, &[], DropTargetId::REVIEW),
            DropStep::Nothing
        );
        let targets: Vec<DropTargetId> = drag_answer(&review, &[])
            .targets
            .iter()
            .map(|t| t.target.clone())
            .collect();
        assert_eq!(
            targets,
            [
                DropTargetId::BACKLOG,
                DropTargetId::DEV,
                DropTargetId::new("validate"),
                DropTargetId::REVIEW,
                DropTargetId::DONE
            ],
            "one target per column, no Approved"
        );
        let flow = otaq_core::flow::default_flow();
        let columns = Columns::of(&flow);
        assert_eq!(
            columns.drop_target("review"),
            Some(otaq_core::board::moves::DropTarget::ReadyForReview.into())
        );
    }

    #[test]
    fn the_gallery_has_every_review_state() {
        let g = gallery();
        let counts: Vec<&str> = g.review.iter().map(|c| c.column.count.as_str()).collect();
        assert_eq!(counts, ["3", "1", "1", "0"]);
        assert_eq!(g.drags.len(), 5);
        assert_eq!(g.drags[1].from, "SHOP-25 · Dev");
        assert_eq!(g.first_ticket.title, "Tidy the README");
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
            changes: Vec::new(),
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
                    c.cards.iter().chain(dev)
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
        assert_eq!(by_agent.columns[0].count, "1/1");
        assert_eq!(by_agent.columns[3].count, "1/2");

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
        assert_eq!(cleared.columns[0].count, "1");
        let json = serde_json::to_value(&by_agent).unwrap();
        assert_eq!(
            json["agent_filter"]["label"],
            "Created or changed by Checkout slowness"
        );
    }

    /// Two enabled deliveries offer the choice; Open PR without a provider leaves one, whose dialog opens without the
    /// choice (dogfood #18) unless "Done, don't merge" is offered too.
    #[test]
    fn one_way_to_deliver_skips_the_choice() {
        let review = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        let no_provider = PrProvider {
            remote: Some("git.acme.internal/web/handbook".into()),
            provider: None,
            logged_in: None,
            cli_found: false,
            open_pr_skill: "open-pr".to_owned(),
        };
        assert!(no_provider.open_pr_blocked().is_some());
        let menu = approve_menu(
            &review,
            "main",
            std::slice::from_ref(&review),
            Some(&no_provider),
            None,
            otaq_core::flow::Delivers::Either,
        );
        assert_eq!(menu.items.len(), 3);
        assert!(!menu.items[1].enabled);
        assert!(!menu.items[2].enabled, "no other ticket to merge into");
        assert_eq!(menu.direct, Some(ApproveChoice::LocalMerge));
        // With commits, "Done, don't merge" is a second choice, so the menu shows rather than hiding it.
        let menu = approve_menu(
            &review,
            "main",
            std::slice::from_ref(&review),
            Some(&no_provider),
            Some(1),
            otaq_core::flow::Delivers::Either,
        );
        assert_eq!(menu.items.len(), 5);
        assert_eq!(menu.items[3].choice, ApproveChoice::Done);
        assert!(!menu.items[3].enabled, "the branch has a commit");
        assert_eq!(menu.items[4].choice, ApproveChoice::DoneNotMerged);
        assert_eq!(menu.items[4].number, 5);
        assert_eq!(menu.items[4].icon, IconName::CircleMinus);
        assert_eq!(
            menu.delivery_note,
            "Your flow delivers either way. Change it at the Done end of Edit flow."
        );
        assert_eq!(menu.direct, None);
        let mut pushed = ticket("SHOP-26", "Add sitemap", Status::WaitingForYou);
        pushed.flags.pr_number = Some(481);
        let menu = approve_menu(
            &pushed,
            "main",
            std::slice::from_ref(&pushed),
            None,
            Some(1),
            otaq_core::flow::Delivers::Either,
        );
        assert_eq!(menu.direct, Some(ApproveChoice::PushToPr));
    }

    /// Nothing to merge: Done is the only enabled choice, yet the menu shows (Done has no confirmation of its own),
    /// and its note says Done runs at once.
    #[test]
    fn nothing_to_merge_shows_the_menu_with_done_first() {
        let review = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        let menu = approve_menu(
            &review,
            "main",
            std::slice::from_ref(&review),
            None,
            Some(0),
            otaq_core::flow::Delivers::Either,
        );
        let items: Vec<_> = menu
            .items
            .iter()
            .map(|i| (i.number, i.choice, i.enabled))
            .collect();
        assert_eq!(
            items,
            [
                (1, ApproveChoice::Done, true),
                (2, ApproveChoice::LocalMerge, false),
                (3, ApproveChoice::OpenPr, false),
                (4, ApproveChoice::MergeInto, false),
                (5, ApproveChoice::DoneNotMerged, false),
            ]
        );
        assert_eq!(menu.items[0].icon, IconName::CircleCheck);
        assert_eq!(menu.direct, None, "the user picks Done in the menu");
        assert_eq!(menu.note, approve::DONE_NOTE);
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
        let menu = approve_menu(
            &review,
            "main",
            std::slice::from_ref(&review),
            None,
            Some(1),
            otaq_core::flow::Delivers::Either,
        );
        assert_eq!(menu.title, "Approve SHOP-18 Checkout API");
        assert_eq!(menu.items[0].choice, ApproveChoice::LocalMerge);
        assert_eq!((menu.items[0].number, menu.items[1].number), (1, 2));
        assert_eq!(menu.items[1].icon, IconName::GitPullRequest);
        assert_eq!(
            menu.note,
            "Each opens a confirmation step before anything runs."
        );
        assert_eq!(menu.direct, None, "two ways to deliver: the choice shows");
        let mut pr = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        assert_eq!(
            drop_step(&pr, &[], DropTargetId::REVIEW),
            DropStep::Nothing,
            "a PR open card is in Review already"
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
                c.cards.iter().chain(dev)
            })
            .find(|c| c.key() == key)
            .map(|c| serde_json::to_string(c).unwrap())
            .unwrap_or_else(|| panic!("{key} is on the board"))
    }

    /// A blocked ticket starts like any other: SHOP-20 (Backlog, waiting for SHOP-18 and SHOP-19) dropped on Dev, or
    /// started with ⌘D or Start, moves into Dev, where it waits at the top saying what it waits for; once there, a drop
    /// on Dev changes nothing.
    #[test]
    fn a_blocked_ticket_starts_and_waits_at_the_top_of_dev() {
        let flow = &otaq_core::flow::default_file().flow;
        let tickets = storyboard(Status::WaitingForYou, Status::Working, Status::Backlog);
        let t20 = tickets
            .iter()
            .find(|t| t.key.to_string() == "SHOP-20")
            .unwrap();
        assert_eq!(
            drop_step_in(flow, t20, &tickets, &DropTargetId::DEV),
            DropStep::Move
        );
        let queued = storyboard(Status::WaitingForYou, Status::Working, Status::Queued);
        let t20 = queued
            .iter()
            .find(|t| t.key.to_string() == "SHOP-20")
            .unwrap();
        assert_eq!(
            drop_step_in(flow, t20, &queued, &DropTargetId::DEV),
            DropStep::Nothing
        );
        let page = page(
            &queued,
            &facts(),
            &params(1376.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let dev = page.columns[1].dev.as_ref().unwrap();
        assert_eq!(dev.top[0].key(), "SHOP-20", "at the top of Dev");
        assert!(
            card_text(&page, "SHOP-20").contains("Waits for SHOP-18 and SHOP-19 to merge"),
            "{}",
            card_text(&page, "SHOP-20")
        );
    }

    /// Hovering a ticket others wait for lights them: SHOP-19's card lights SHOP-20 and SHOP-21 behind it, with no
    /// tooltip (its card's own line says it); a stored "Always" no longer draws every link, and the toolbar has no
    /// Dependencies switch.
    #[test]
    fn hovering_lights_what_waits_and_always_is_gone() {
        let tickets = storyboard(Status::WaitingForYou, Status::Working, Status::Queued);
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
        let shop_19 = page.chains.iter().find(|c| c.key == "SHOP-19").unwrap();
        assert_eq!(shop_19.members, ["SHOP-19", "SHOP-20", "SHOP-21"]);
        assert_eq!(shop_19.tip, None);
        let json = serde_json::to_value(&page).unwrap();
        assert!(json.get("links").is_none());
        assert!(json["toolbar"].get("dependencies").is_none());
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
        // 1 · Planning: the waiting line in Backlog, with Start all the same.
        let planning = at(Status::Backlog, Status::Backlog, Status::Backlog);
        let blocked = "Waits for SHOP-18 and SHOP-19 to merge";
        assert!(card_text(&planning, "SHOP-20").contains(blocked));
        assert!(card_text(&planning, "SHOP-20").contains("\"start\""));
        // 2 · The chain runs, then SHOP-18 in Review: started, it waits at the top of Dev with the same parents until
        // one merges.
        let waits = "Waits for SHOP-18 and SHOP-19 to merge";
        let runs = at(Status::Working, Status::Working, Status::Queued);
        assert!(card_text(&runs, "SHOP-20").contains(waits));
        assert!(card_text(&runs, "SHOP-21").contains("Waits for SHOP-20 to merge"));
        assert!(card_text(&runs, "SHOP-18").contains("SHOP-20 waits for it"));
        let review = at(Status::WaitingForYou, Status::Working, Status::Queued);
        assert!(card_text(&review, "SHOP-20").contains(waits));
        // 3 · One parent merged.
        let one = at(Status::Merged, Status::WaitingForYou, Status::Queued);
        assert!(card_text(&one, "SHOP-20").contains("Waits for SHOP-19 to merge"));
        // 4 · All parents merged: SHOP-20 works (where it started from is the sheet's); SHOP-21 still waits.
        let all = at(Status::Merged, Status::Merged, Status::Working);
        assert!(card_text(&all, "SHOP-20").contains("\"Working\""));
        assert!(card_text(&all, "SHOP-21").contains("Waits for SHOP-20 to merge"));
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
        assert!(card_text(&pr, "SHOP-20").contains("Waits for SHOP-18 to merge"));
        // An open PR is the user's to mark merged: its card's one action.
        assert!(card_text(&pr, "SHOP-18").contains("Mark merged"));
        // 6 · Closed parent never counts as merged.
        let closed = at(
            Status::Merged,
            Status::ClosedNotMerged,
            Status::ParentClosed,
        );
        let shop_20 = card_text(&closed, "SHOP-20");
        assert!(shop_20.contains("Parent SHOP-19 was closed"), "{shop_20}");
        assert!(shop_20.contains("Decide"), "{shop_20}");
        // Hovering SHOP-20 lights what waits for it.
        let chain = all.chains.iter().find(|c| c.key == "SHOP-20").unwrap();
        assert_eq!(chain.members, ["SHOP-20", "SHOP-21"]);
        assert!(planning.chains.iter().all(|c| c.key != "SHOP-22"));
    }

    /// The filter strip shows while the search filters the board: how many of how many, its chips, one filter to
    /// add while it isn't on, and Clear; nothing filters, no strip.
    #[test]
    fn the_filter_strip_follows_the_search() {
        let state = |tokens: &[&str], text: &str| FilterState {
            tokens: tokens.iter().map(|t| (*t).to_owned()).collect(),
            text: text.to_owned(),
        };
        assert_eq!(filter_strip(&state(&[], "  "), 17, 17), None);
        let strip = filter_strip(&state(&[], "checkout"), 3, 17).unwrap();
        assert_eq!(strip.showing, "Showing 3 of 17");
        assert_eq!(strip.chips[0].label, "\u{201c}checkout\u{201d}");
        assert_eq!(strip.chips[0].token, None);
        assert_eq!(strip.suggest_label.as_deref(), Some("+ is:needs-you"));
        let on = filter_strip(&state(&["is:needs-you"], ""), 2, 17).unwrap();
        assert_eq!(on.chips[0].token.as_deref(), Some("is:needs-you"));
        assert_eq!((on.suggest, on.clear.as_str()), (None, "Clear"));
    }

    /// The example's shop-web flow (*24 · The two views*): Product spec, Dev with Validate shown in it, Security,
    /// Review and the PR; seven columns and Done's line.
    fn shop_web() -> Flow {
        let yaml = "\
flow:
  - {id: product-spec, name: Product spec, kind: agent, agent: Spec writer, skill: product-spec}
  - {id: dev, name: Dev, kind: agent, agent: Rust developer, skill: work-ticket}
  - {id: validate, name: Validate, kind: check, by: agent, agent: Validator, skill: validate, rounds: 3}
  - {id: security, name: Security, kind: check, by: agent, agent: Security reviewer, skill: security-review, rounds: 3}
  - {id: review, name: Review, kind: check, by: you}
delivery: {delivers: pr, mode: click, agent: Shipper, skill: open-pr}
";
        otaq_core::flow::yaml::read(yaml).unwrap().flow
    }

    /// A 12-stage flow (`s24-04`): Product spec, Plan, Dev, Validate, Tests, Security, Perf, A11y, QA, Review, Release
    /// notes, Deploy.
    fn twelve_stages() -> Flow {
        let yaml = "\
flow:
  - {id: product-spec, name: Product spec, kind: agent, agent: Spec writer, skill: product-spec}
  - {id: plan, name: Plan, kind: agent, agent: Planner, skill: plan}
  - {id: dev, name: Dev, kind: agent, agent: Rust developer, skill: work-ticket}
  - {id: validate, name: Validate, kind: check, by: agent, agent: Validator, skill: validate, rounds: 3}
  - {id: tests, name: Tests, kind: agent, agent: Test runner, skill: e2e}
  - {id: security, name: Security, kind: check, by: agent, agent: Security reviewer, skill: security-review, rounds: 3}
  - {id: perf, name: Perf, kind: check, by: agent, agent: Perf reviewer, skill: perf, rounds: 3}
  - {id: a11y, name: A11y, kind: check, by: agent, agent: A11y reviewer, skill: a11y, rounds: 3}
  - {id: qa, name: QA, kind: check, by: you}
  - {id: review, name: Review, kind: check, by: you}
  - {id: notes, name: Release notes, kind: agent, agent: Doc writer, skill: docs}
  - {id: deploy, name: Deploy, kind: agent, agent: Deployer, skill: deploy}
delivery: {delivers: pr, mode: click, agent: Shipper, skill: open-pr}
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
            p1,
            waits,
            asks(
                at(
                    "SHOP-12",
                    "Add dark mode toggle",
                    Status::AsksYou,
                    Place::stage_of("product-spec"),
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
            card.action.as_ref().map(|a| a.label.to_string()),
            card.edge,
        )
    }

    fn cards_in(c: &ColumnPage) -> Vec<&BoardCard> {
        let dev = c.dev.iter().flat_map(|d| d.top.iter().chain(&d.working));
        c.cards.iter().chain(dev).collect()
    }

    /// BRD-02, BRD-03 on the example (`s24-03`): at 1440 (the 64 px rail, no agent panel) the six columns keep their
    /// 240 px and the board scrolls with "1 more stage", Done is its "Done 4 →" line, and each card reads as drawn.
    #[test]
    fn the_example_board_scrolls_at_1440_with_one_line_cards() {
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
                "Product spec",
                "Dev",
                "Validate",
                "Security",
                "Review"
            ]
        );
        assert_eq!(
            page.more_stages.at_start.as_deref(),
            Some("1 more stage"),
            "six columns scroll at 1440"
        );
        assert_eq!(page.done.as_ref().unwrap().text, "Done 4 \u{2192}");
        assert_eq!(page.toolbar.views.options[0].label, "Board");
        assert!(page.toolbar.views.options[0].selected);
        assert_eq!(page.list, None);

        let s = |text: &str| Some(text.to_owned());
        assert_eq!(
            line_of(&page, "SHOP-31"),
            (s("Not started"), s("Start"), None),
            "a calm Backlog card, with Start"
        );
        assert_eq!(
            line_of(&page, "SHOP-22"),
            (
                s("Asks you \u{b7} allow rm -rf .next?"),
                s("Open"),
                Some(Tone::Attention)
            )
        );
        assert_eq!(
            line_of(&page, "SHOP-12"),
            (
                s("Asks you \u{b7} follow the system setting?"),
                s("Answer"),
                Some(Tone::Attention)
            )
        );
        assert_eq!(line_of(&page, "SHOP-33"), (s("Working"), None, None));
        // No rounds on the card: they are the sheet's.
        assert_eq!(line_of(&page, "SHOP-19").0, s("Checking"));
        assert_eq!(line_of(&page, "SHOP-30").0, s("Checking"));
        assert_eq!(
            line_of(&page, "SHOP-32"),
            (
                s("Security failed 3 times \u{b7} decide"),
                s("Decide"),
                Some(Tone::Attention)
            )
        );
        let check = line_of(&page, "SHOP-18");
        assert_eq!(
            (check.0, check.1, check.2),
            (s("Ready for you"), s("Review"), Some(Tone::Review))
        );
        let pr = line_of(&page, "SHOP-26").0.unwrap();
        assert!(pr.starts_with("PR #481 open"), "{pr}");
        assert_eq!(line_of(&page, "SHOP-26").1, s("Mark merged"));
        // #36: the priority is the card's chip after its key, in every column, not the line's first word.
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
        assert_eq!(waits, "Waits for SHOP-18 to merge");
        let working = page.columns[2]
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

    /// BRD-02 on a 12-stage flow (`s24-04`): every column keeps 240 px, nothing folds by itself, and at 1440 the
    /// board scrolls with "8 more stages"; Fold empty columns folds the empty ones to rails.
    #[test]
    fn a_twelve_stage_flow_scrolls_with_eight_more_stages() {
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
        assert_eq!(open, 13, "Backlog and twelve stages");
        assert_eq!(page.more_stages.at_start.as_deref(), Some("8 more stages"));
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
        // The started tickets waiting to begin keep the first stage open.
        assert_eq!(open, ["Backlog", "Product spec"]);
        assert_eq!(folded.more_stages.at_start, None, "the rails fit");
        assert!(folded.toolbar.switch.as_ref().unwrap().selected);
        let plan = folded.columns[2].rail.as_ref().unwrap();
        assert_eq!(plan.tooltip, "Plan \u{b7} 0 tickets");
        assert!(!plan.dot);
    }

    /// `s26-10`: Product spec folded from its ⋯ is a rail with SHOP-12's needs-you dot.
    #[test]
    fn a_folded_column_has_a_dot_when_something_needs_you() {
        let flow = shop_web();
        let tickets = shop_web_tickets();
        let page = super::page(
            &tickets,
            &ProjectFacts {
                flow: &flow,
                view: view(ViewPrefs {
                    folded_columns: vec!["done".into(), "product-spec".into()],
                    ..ViewPrefs::default()
                }),
                ..facts()
            },
            &params(1440. - 64.),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let product_spec = &page.columns[1];
        assert!(product_spec.collapsed);
        let rail = product_spec.rail.as_ref().unwrap();
        assert_eq!(rail.tooltip, "Product spec \u{b7} 3 tickets, 1 needs you");
        assert!(rail.dot);
        assert_eq!(
            rail.unfold,
            Change::FoldColumn {
                column: "product-spec".into(),
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

//! The board's columns (UI-02, SHL-06, STG-08; *03 · Board & columns*; d0087, d0108): the project's flow's columns
//! with counts ([`flow_columns`]), on the default flow Backlog · Ready for Dev · Dev · Review · Done. A human
//! check's column holds its stage and the Delivery folded into it as two sections ([`SectionView`]: Ready for
//! Review and Approved), each with its own count and its own cards in its status set's order, and the column counts
//! both. Ready for Dev shows the queued cards (d0066); an agent column (Dev) the cards that need the user (and pins)
//! on top, then the Agent Working section; the "N more · working" fold that never hides needs-you cards (T-0027);
//! epic cards in Backlog; and the empty-column hints. Order comes from the core ([`otaq_core::board::order`]), so the
//! board, the service and the tests agree.
//!
//! T-0460 (BRD-02): one width rule ([`COLUMN_MIN_WIDTH`], [`fits`], [`columns_after`] for the "7 more stages" pill)
//! replaces the long-flow folding and the stage strip; a column is folded ([`is_folded`]) from its ⋯ or by Fold empty
//! columns (the project's view preferences), Done at first.
//!
//! Each child is on the board exactly once (EPC-11, d0080, EPL-01): an epic is a card in Backlog only in its Card
//! place and a delivery card in Review / Done; in flight its children are its cards, each naming the epic (and with
//! Lanes: Epic they sit in its lane, `lanes.rs`). An epic child in Backlog is a row of the epic card while the epic
//! is one, else a card of its own. In Done an epic's merged children fold into one line (EPC-16, [`DoneFold`]); its
//! closed children stay cards.

use otaq_core::agents::{Agent, AgentList, AgentModel};
use otaq_core::board::epic_flight::{EpicPlace, REVIEW_EPIC, children_of, epic_place};
use otaq_core::board::epics::EpicProgress;
use otaq_core::board::order::{ColumnOrder, fold_dev};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Kind, Status, StatusSet, TicketKey};
use otaq_core::flow::check::DEFAULT_VALIDATION_ROUNDS;
use otaq_core::flow::{CheckBy, Delivers, DeliveryMode, Flow, Stage, StageKind, StageOption};

use crate::icons::IconName;
use crate::view_models::agents::model_label;
use crate::view_models::board_page::{ColumnId, DropTargetId, ReviewSectionId};
use crate::view_models::card::{CardExtras, CardViewModel};
use crate::view_models::epic_card::EpicViewModel;
use crate::view_models::flow_columns::{ColumnKind, Columns, FlowColumn, keeps_default_look};
use crate::view_models::queue::queue_facts;
use crate::view_models::tool_problems::tool_word;
use otaq_core::agents::facts::BoardAgents;

/// The one width rule (BRD-02): a column is never narrower than this; when the flow fits, the columns share the
/// board's width, otherwise the board scrolls sideways with the "7 more stages" pill at its edge.
pub const COLUMN_MIN_WIDTH: f32 = 180.;

/// A column folded from its ⋯ (or by Fold empty columns): the narrow rail with its icon, count and name (`s26-10`).
pub const FOLDED_COLUMN_WIDTH: f32 = 52.;

/// The board's padding on each side and the gap between its columns (`board.css`; *24 · The two views*).
pub const BOARD_PADDING: f32 = 16.;
pub const COLUMN_GAP: f32 = 12.;

#[derive(Debug, Clone, PartialEq)]
pub enum CardEntry {
    Ticket(CardViewModel),
    Epic(EpicViewModel),
}

impl CardEntry {
    pub fn key(&self) -> &TicketKey {
        match self {
            Self::Ticket(t) => &t.key,
            Self::Epic(e) => &e.key,
        }
    }
}

/// Dev's parts, top to bottom. A stage folded into the column (`column: with-previous`) has no section of its own:
/// the header counts it ([`FoldedView`]) and its cards carry their stage's tag.
#[derive(Debug, Clone, PartialEq)]
pub struct DevSections {
    /// Pinned cards and cards that need the user, on top (never folded).
    pub top: Vec<CardEntry>,
    pub working: Vec<CardEntry>,
    /// Cards in the Agent Working section, the floated ones included (section counts count by status).
    pub working_count: usize,
    /// "4 more · working" while folded.
    pub fold: Option<String>,
    /// The cards behind the fold, in column order (the fold chip turns accent when one is in the highlighted chain).
    pub folded: Vec<TicketKey>,
}

/// One section of a human check's column (*Row · Review column (0086 · V)*): the human check's (Ready for Review)
/// or the Delivery's folded into it (Approved), with its own count and cards in that status set's order (§7.5).
/// Folding it is the page's state; the count stays.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionView {
    pub id: ReviewSectionId,
    /// A section after the column's own is a drop target of its own (Approved); the first is the column.
    pub target: Option<DropTargetId>,
    /// It holds a Delivery stage's cards (the Approved section).
    pub delivery: bool,
    /// "READY FOR REVIEW", "APPROVED".
    pub title: &'static str,
    /// "Ready for Review", "Approved": the fold tooltip's name.
    pub name: &'static str,
    /// "2", or "1/2" with a filter.
    pub count: String,
    pub cards: Vec<CardEntry>,
    /// Cards a filter hides in this section.
    pub hidden: usize,
    /// The quiet line of an empty section ("Nothing approved").
    pub empty: &'static str,
    /// Approved only: what the section says while a Ready for Review card is dragged and it is empty.
    pub drag_hint: Option<&'static str>,
}

/// A stage folded into an agent column (`column: with-previous`, FLW-12, Q270): the header adds "· Validating 1"
/// after the column's own count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldedView {
    /// The word its cards carry: "Validating" for a Validate check, else the stage's name.
    pub tag: String,
    /// The stage's name.
    pub name: String,
    /// "1", "1/2" under a filter.
    pub count: String,
}

/// Who runs a column's stages, on hovering its title (FLW-12; *Row · Stage kinds on the column header*): "Grill ·
/// Griller · grill" over "Claude Code · sonnet" and "Options: skip"; a folded stage adds its line ("Validate ·
/// Validator · Codex · gpt-5.5 · shown in Dev").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderTooltip {
    pub title: String,
    pub detail: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnView {
    pub id: ColumnId,
    pub kind: ColumnKind,
    /// The stage's name ("Dev", "Review", "Done · PR"), or "Backlog", "Ready for Dev", "Done".
    pub title: String,
    pub icon: IconName,
    /// The icon in the muted colour (FLW-12: a flow that isn't the default's shape); else in its status colour.
    pub quiet_icon: bool,
    /// "12" or, with a filter, "3/12" (T-0038); with an epic focused, its cards over all (Q55). Review: both
    /// sections. An agent column with a folded stage: its own stage's cards ("Dev 3 · Validating 1").
    pub count: String,
    /// The stages folded into the column, each with its count: an agent column's header names them; a human
    /// check's column shows them as its sections.
    pub folded: Vec<FoldedView>,
    /// Who runs it; `None` for Backlog, Ready for Dev and Done.
    pub tooltip: Option<HeaderTooltip>,
    /// Its cards over all, whatever the filter: an empty stage of a long flow folds to its rail.
    pub total: usize,
    /// Every column but the agent and human check columns.
    pub cards: Vec<CardEntry>,
    /// An agent column's (Dev's) parts.
    pub dev: Option<DevSections>,
    /// A human check's column: Ready for Review, then Approved.
    pub sections: Vec<SectionView>,
    /// Under the title: Ready for Dev's "Starts when ready, in priority order" ("Starts when ready" in a long flow).
    pub hint: Option<&'static str>,
    /// Shown when the column is empty.
    pub empty_hint: &'static str,
    /// Folded (BRD-02): a stage's narrow rail, Done's "Done 4 →" line ([`is_folded`]).
    pub collapsed: bool,
    /// Its cards that need the user: the folded rail's dot and "1 needs you".
    pub needs_you: usize,
    /// Cards a filter hides in this column ("6 hidden by filter").
    pub hidden: usize,
    /// Done only: one line per epic over its merged children, which follow each other in `cards` from where the
    /// first of them sorts.
    pub folds: Vec<DoneFold>,
}

/// An epic's merged children folded into one line in Done (EPC-16; *01 · Components → Done Fold*): "Wishlist · 4
/// of 6 merged", merged out of all its children. A click shows the cards; the column counts the line as one.
#[derive(Debug, Clone, PartialEq)]
pub struct DoneFold {
    pub epic: TicketKey,
    pub label: String,
    /// The folded cards, in column order.
    pub keys: Vec<TicketKey>,
}

/// Folds the merged epic children among `keys` (a column's cards in order): each epic's merged children move up
/// to where the first of them sorts, and get a [`DoneFold`]. Closed children and other cards stay where they are.
pub fn done_folds(keys: &[TicketKey], all: &[Ticket]) -> (Vec<TicketKey>, Vec<DoneFold>) {
    let folded_under = |key: &TicketKey| {
        all.iter()
            .find(|t| &t.key == key)
            .filter(|t| t.status == Status::Merged)
            .and_then(|t| t.epic.as_ref())
    };
    let mut order = Vec::with_capacity(keys.len());
    let mut folds: Vec<DoneFold> = Vec::new();
    for key in keys {
        let Some(epic) = folded_under(key) else {
            order.push(key.clone());
            continue;
        };
        if folds.iter().any(|f| &f.epic == epic) {
            continue;
        }
        let members: Vec<TicketKey> = keys
            .iter()
            .filter(|k| folded_under(k) == Some(epic))
            .cloned()
            .collect();
        let title = all
            .iter()
            .find(|t| &t.key == epic)
            .map_or_else(|| epic.to_string(), |t| t.title.clone());
        let progress = all.iter().find(|t| &t.key == epic).map_or_else(
            || {
                let n = u32::try_from(members.len()).unwrap_or(u32::MAX);
                EpicProgress {
                    merged: n,
                    total: n,
                    closed: 0,
                }
            },
            |e| EpicProgress::of(children_of(e, all).into_iter()),
        );
        order.extend(members.iter().cloned());
        folds.push(DoneFold {
            epic: epic.clone(),
            label: format!("{title} · {}", progress.label()),
            keys: members,
        });
    }
    (order, folds)
}

/// How many items `keys` make with their folds: a fold is one.
fn items(keys: &[TicketKey], folds: &[DoneFold]) -> usize {
    keys.len() - folds.iter().map(|f| f.keys.len() - 1).sum::<usize>()
}

/// A column's icon and empty hint (FLW-12): by the kind of the stage that owns it. `quiet` is a flow that isn't the
/// default's shape: an Agent stage is a bot and a human check a person (on the default flow they keep today's
/// activity and eye); an agent check is a shield, on any flow (Validate's own column, FLW-01); a Delivery is a merge
/// or a PR.
pub fn column_look(column: &FlowColumn, quiet: bool) -> (IconName, &'static str) {
    let owner = column.parts.first().map(|p| p.stage);
    match column.kind {
        ColumnKind::Backlog => (
            IconName::CircleDashed,
            "Write a ticket here: what to build and when it’s done.",
        ),
        ColumnKind::ReadyForDev => (
            IconName::Hourglass,
            "Drag a ticket here from Backlog to start it when it’s ready.",
        ),
        ColumnKind::Agent if owner.is_some_and(Stage::is_agent_check) => {
            (IconName::ShieldCheck, AGENT_CHECK_EMPTY)
        }
        ColumnKind::Agent => (
            if quiet {
                IconName::Bot
            } else {
                IconName::Activity
            },
            "Tickets move here when their agent starts. Each agent works in its own worktree.",
        ),
        ColumnKind::HumanCheck => (
            if quiet {
                IconName::UserCheck
            } else {
                IconName::Eye
            },
            "Agents move their ticket here when they submit it for your review.",
        ),
        ColumnKind::Delivery => (
            match owner.and_then(|s| s.delivery) {
                Some(d) if d.delivers == Delivers::Pr => IconName::GitPullRequest,
                _ => IconName::GitMerge,
            },
            "Approved tickets wait here until they’re delivered.",
        ),
        ColumnKind::Done => (
            IconName::CircleCheck,
            "Merged and closed tickets end up here.",
        ),
    }
}

/// An empty agent check's column (Validate's own, FLW-01). No frame draws it: the designer's notes have it.
pub const AGENT_CHECK_EMPTY: &str = "An agent checks each ticket here before it moves on.";

/// The hint under Ready for Dev's title (BRD-03, *24 · The two views*; d0065 unchanged).
pub const READY_FOR_DEV_HINT: &str = "Starts in priority order";

use otaq_core::flow::apply::Hold;
/// What a Ready for Dev card that would start says while a flow change waits for a quiet board (FLW-06, *Row · A
/// flow change waiting*): [`BoardInput::starts_held`].
pub use otaq_core::flow::apply::WAITING_FOR_THE_FLOW_CHANGE;

/// The Review column's section headers (*Row · Review column (0086 · V)*).
pub const READY_FOR_REVIEW_SECTION: &str = "READY FOR REVIEW";
pub const APPROVED_SECTION: &str = "APPROVED";

/// An empty section's quiet line: the frame's "Nothing approved", and the same for Ready for Review.
pub const NOTHING_TO_REVIEW: &str = "Nothing to review";
pub const NOTHING_APPROVED: &str = "Nothing approved";

/// The empty Approved section while a Ready for Review card is dragged (the old Approved column's hint).
pub const APPROVED_DRAG_HINT: &str = "Drag a ticket here from Ready for Review to approve it. It always asks how: Local merge or Open PR.";

/// Whether a column is folded (BRD-02): folded from its ⋯ (Done is at first, `ViewPrefs::folded_columns`), or
/// empty while Fold empty columns is on. Backlog never folds: tickets are written there.
pub fn is_folded(
    id: &str,
    kind: ColumnKind,
    total: usize,
    folded: &[String],
    fold_empty: bool,
) -> bool {
    kind != ColumnKind::Backlog && (folded.iter().any(|f| f == id) || (fold_empty && total == 0))
}

/// How the open and folded columns sit at the board's width (BRD-02): `fits` when every open column gets at least
/// [`COLUMN_MIN_WIDTH`] (they share what is left); otherwise each keeps the minimum and the board scrolls.
pub fn fits(width: f32, open: usize, folded: usize) -> bool {
    let n = open + folded;
    let gaps = n.saturating_sub(1) as f32 * COLUMN_GAP;
    2. * BOARD_PADDING + open as f32 * COLUMN_MIN_WIDTH + folded as f32 * FOLDED_COLUMN_WIDTH + gaps
        <= width
}

/// How many columns are cut off at the right edge of a board `width` wide scrolled to `left` (BRD-02's "7 more
/// stages"): `folded` says, column by column, which are rails. Nothing is cut off when the columns fit.
pub fn columns_after(width: f32, left: f32, folded: &[bool]) -> usize {
    let open = folded.iter().filter(|f| !**f).count();
    if fits(width, open, folded.len() - open) {
        return 0;
    }
    let mut x = BOARD_PADDING;
    let mut after = 0;
    for &rail in folded {
        let right = x + if rail {
            FOLDED_COLUMN_WIDTH
        } else {
            COLUMN_MIN_WIDTH
        };
        // A pixel of rounding is not cut off.
        if right > left + width - BOARD_PADDING + 1. {
            after += 1;
        }
        x = right + COLUMN_GAP;
    }
    after
}

/// The pill at the board's right edge while columns are cut off (BRD-02, `s24-04`): "7 more stages".
pub fn more_stages(n: usize) -> Option<String> {
    match n {
        0 => None,
        1 => Some("1 more stage".to_owned()),
        n => Some(format!("{n} more stages")),
    }
}

/// How many Dev cards fit before the fold, from the board's height (a card is about 96 px with its gap).
pub fn dev_budget(board_height: f32) -> usize {
    (((board_height - 120.) / 96.).floor().max(4.)) as usize
}

pub struct BoardInput<'a> {
    /// The project's flow: its columns (STG-08).
    pub flow: &'a Flow,
    pub tickets: &'a [Ticket],
    /// Tickets shown (a filter, T-0038); `None` shows all.
    pub visible: Option<&'a [TicketKey]>,
    /// Tickets at full strength while an epic is focused (an `epic:` filter, Q55): the others are shown dimmed and
    /// the counts count only these. `None`: nothing dims.
    pub strong: Option<&'a [TicketKey]>,
    pub project_base: Option<&'a str>,
    pub now_ms: i64,
    /// Dev's card budget; `None` when the user unfolded Dev.
    pub dev_budget: Option<usize>,
    /// The columns folded from their ⋯, by id (`ViewPrefs::folded_columns`; Done at first).
    pub folded: &'a [String],
    /// Fold empty columns (BRD-02).
    pub fold_empty: bool,
    /// The project's Project agents by id and current name: cards they created name them (d0068).
    pub agent_names: &'a [(String, String)],
    /// Interrupted tickets that were validating (the startup report, T-0244).
    pub was_validating: &'a [TicketKey],
    /// The cards' named agents (`board.agents`, T-0345); `None` names none.
    pub agents: Option<&'a BoardAgents>,
    /// Every agent (`agents.list`): the headers' tooltips name each stage's tool and model (FLW-12); `None` names
    /// only the agents.
    pub agent_list: Option<&'a AgentList>,
    /// The starts a waiting flow change holds back (FLW-06, FED-08): from Ready for Dev, or at its span's edge. A card
    /// it holds that would start says [`WAITING_FOR_THE_FLOW_CHANGE`] in place of "Starting…". `None` while starts
    /// run.
    pub starts_held: Option<&'a Hold>,
    /// The project is paused (PAU-03, PAU-04; d0112): the cards the pause stopped read "Paused", and a ticket that
    /// would start waits ([`KEPT_START`]).
    pub paused: bool,
}

/// A ticket that waits with a kept start while its project is paused (PAU-04, `agents::kept::starts_on_resume`), in
/// a stage it was dragged or moved into; Ready for Dev's says "starts next" (`s24-07`).
pub const KEPT_START: &str = "Paused \u{b7} starts when you resume";

/// An epic waiting at the implicit Review epic stop (a flow with no human check, d0108 rule 7, Q273): a card in the
/// Delivery's column, where the user passes it.
pub fn at_review_epic(ticket: &Ticket) -> bool {
    ticket.kind == Kind::Epic && ticket.place.is_in(REVIEW_EPIC)
}

/// Whether the ticket is a card in its column (EPC-11, EPL-01): an epic in its Card or Delivery place (in flight it
/// is its children's cards and, with Lanes: Epic, its lane); an epic child anywhere but in Backlog while its epic is
/// a card (then it is one of the card's rows). `all` is the ticket's project.
pub fn is_card(ticket: &Ticket, all: &[Ticket]) -> bool {
    if ticket.kind == Kind::Epic {
        return epic_place(ticket, all) != EpicPlace::Track;
    }
    let Some(epic) = ticket.epic.as_ref() else {
        return true;
    };
    !ticket.place.is_backlog()
        || all
            .iter()
            .find(|t| &t.key == epic)
            .is_none_or(|e| epic_place(e, all) != EpicPlace::Card)
}

/// One state's cards, in its order, with what its count counts.
struct StateCards {
    order: ColumnOrder,
    folds: Vec<DoneFold>,
    /// Items over all (a Done fold is one).
    total: usize,
    /// Items the filter shows.
    visible: usize,
    /// Items at full strength while an epic is focused.
    strong: Option<usize>,
}

impl StateCards {
    /// The cards of `column`'s `part` (or of the whole column).
    fn of(
        columns: &Columns,
        column: &str,
        part: Option<&str>,
        input: &BoardInput,
        cards: &[&TicketKey],
        shown: &[&TicketKey],
    ) -> Self {
        let all = input.tickets;
        // Order over the whole project ("blocks more" counts every dependent), cards only; the counts count the
        // cards the column shows.
        let full = columns.order(column, part, cards, all);
        let keep = |keys: &[TicketKey], of: &[&TicketKey]| -> Vec<TicketKey> {
            keys.iter().filter(|k| of.contains(k)).cloned().collect()
        };
        let (shown_cards, folds) = done_folds(&keep(&full, shown), all);
        let every = full;
        let total = items(&every, &done_folds(&every, all).1);
        let visible = items(&shown_cards, &folds);
        let strong = input.strong.map(|strong| {
            let full = keep(&shown_cards, &strong.iter().collect::<Vec<_>>());
            items(&full, &done_folds(&full, all).1)
        });
        Self {
            order: ColumnOrder {
                // The core's order type; the board only reads its cards.
                column: StatusSet::AgentStage,
                cards: shown_cards,
            },
            folds,
            total,
            visible,
            strong,
        }
    }
}

/// "12", "3/12" under a filter, or the focused epic's items over all.
fn count_text(visible: usize, total: usize, strong: Option<usize>, filtered: bool) -> String {
    match strong {
        Some(n) => format!("{n}/{total}"),
        None if filtered => format!("{visible}/{total}"),
        None => total.to_string(),
    }
}

/// The agent that opens a project's PRs: its Delivery stage's, else its first Agent stage's
/// (`agents::resolve::delivery_agent`).
fn delivery_agent(flow: &Flow) -> Option<String> {
    flow.stages
        .iter()
        .find(|s| s.is_delivery() && s.agent.is_some())
        .or_else(|| flow.first_agent_stage())
        .and_then(|s| s.agent.clone())
}

/// A ticket's card view model as the board builds it (its queue facts, signal, named agents and the pause), for the
/// board's columns and the List's rows (LST-01).
pub fn ticket_card(input: &BoardInput, t: &Ticket) -> CardViewModel {
    let all = input.tickets;
    // The default flow's board keeps today's cards (d0108, STG-06); other flows draw the M33 ones.
    let custom_flow = !crate::view_models::flow_columns::keeps_default_look(input.flow);
    // What a Queued card waits for, from the core's readiness: updates as each parent moves (T-0083). One
    // that would start says why it doesn't while starts are held (FLW-06).
    let mut queue = queue_facts(t, all);
    if input
        .starts_held
        .is_some_and(|hold| queue.starting && hold.holds(&t.place))
    {
        queue.starting = false;
        queue.waits_for = Some(WAITING_FOR_THE_FLOW_CHANGE.to_owned());
    }
    // Nothing starts while paused (PAU-04): Ready for Dev's next one "starts next" (`s24-07`), one in a stage
    // waits for Resume.
    if otaq_core::agents::kept::starts_on_resume(t, all, input.paused) {
        queue.starting = false;
        queue.waits_for = Some(
            if t.place == otaq_core::domain::Place::Waiting(None) {
                crate::view_models::card::STARTS_NEXT
            } else {
                KEPT_START
            }
            .to_owned(),
        );
    }
    let extras = CardExtras {
        waits_for: queue.waits_for,
        waits_for_merge: queue.waits_for_merge,
        starting: queue.starting,
        closed_parent: queue.closed_parent,
        project_base: input.project_base.map(str::to_owned),
        sleeve: crate::view_models::card::sleeve(t, all),
        start_failure: (t.status == Status::StartInterrupted)
            .then(|| {
                t.start_problem
                    .as_ref()
                    .map(|p| p.step.failure_label().to_owned())
            })
            .flatten(),
        held_reason: (t.status == Status::Held)
            .then(|| t.start_problem.as_ref().map(|p| p.message.clone()))
            .flatten(),
        held_by_limit: otaq_core::scheduler::held_by_tool_limit(t),
        was_validating: input.was_validating.contains(&t.key),
        // Its Delivery (FLW-15): Ready to merge or Ready for PR, and who opens the PR.
        delivers: otaq_core::board::epic_flight::delivery_of(input.flow, t).map(|d| d.delivers),
        delivery_agent: delivery_agent(input.flow),
        custom_flow,
        paused: input.paused,
        // DPV-01: the blocked line of a ticket that hasn't started, and what waits for it.
        blocked: crate::view_models::dependencies::blocked_line(t, all),
        blocks: crate::view_models::dependencies::blocks(t, all),
        agent_name: match &t.created_by {
            otaq_core::domain::Actor::ProjectAgent(id) => input
                .agent_names
                .iter()
                .find(|(agent, _)| agent == id)
                .map(|(_, name)| name.clone()),
            _ => None,
        },
        ..CardExtras::default()
    }
    .with_signal(t, input.now_ms);
    // Its named agents, holds, moves and avatars (T-0345, T-0362).
    let extras = match input.agents {
        Some(agents) => extras.with_agents(agents, &t.key),
        None => extras,
    };
    CardViewModel::from(t, &extras, input.now_ms)
}

pub fn build(input: &BoardInput) -> Vec<ColumnView> {
    let all = input.tickets;
    let cards: Vec<&TicketKey> = all
        .iter()
        .filter(|t| is_card(t, all))
        .map(|t| &t.key)
        .collect();
    let shown: Vec<&TicketKey> = match input.visible {
        Some(keys) => cards.iter().copied().filter(|k| keys.contains(k)).collect(),
        None => cards.clone(),
    };
    let filtered = input.visible.is_some();
    let entry = |key: &TicketKey| -> Option<CardEntry> {
        let t = all.iter().find(|t| &t.key == key)?;
        Some(if t.kind == Kind::Epic {
            CardEntry::Epic(EpicViewModel::from(t, all, None).with_pr_comments(t))
        } else {
            CardEntry::Ticket(ticket_card(input, t))
        })
    };
    let entries = |keys: &[TicketKey]| keys.iter().filter_map(entry).collect::<Vec<_>>();
    let columns = Columns::of(input.flow);
    let quiet = !keeps_default_look(input.flow);
    columns
        .iter()
        .map(|column| {
            let id = column.id.as_str();
            let whole = StateCards::of(&columns, id, None, input, &cards, &shown);
            // Each stage's own cards, where the column shows more than one.
            let parts: Vec<StateCards> = match column.parts.len() {
                0 | 1 => Vec::new(),
                _ => column
                    .parts
                    .iter()
                    .map(|p| StateCards::of(&columns, id, Some(&p.id), input, &cards, &shown))
                    .collect(),
            };
            let (icon, empty_hint) = column_look(column, quiet);
            // An agent column counts its own stage, then each folded one (Q270).
            let own = match (column.kind, parts.first()) {
                (ColumnKind::Agent, Some(own)) => own,
                _ => &whole,
            };
            let mut view = ColumnView {
                id: ColumnId::new(id),
                kind: column.kind,
                title: column.title.clone(),
                icon,
                quiet_icon: quiet,
                count: count_text(own.visible, own.total, own.strong, filtered),
                folded: Vec::new(),
                tooltip: header_tooltip(column, input.agent_list),
                total: whole.total,
                cards: Vec::new(),
                dev: None,
                sections: Vec::new(),
                hint: (column.kind == ColumnKind::ReadyForDev).then_some(READY_FOR_DEV_HINT),
                empty_hint,
                collapsed: is_folded(id, column.kind, whole.total, input.folded, input.fold_empty),
                needs_you: needs_you_in(&columns, id, all, &cards),
                hidden: whole.total - whole.visible,
                folds: Vec::new(),
            };
            view.folded = column
                .parts
                .iter()
                .zip(&parts)
                .skip(1)
                .map(|(part, s)| FoldedView {
                    tag: folded_tag(part.stage),
                    name: part.stage.name.clone(),
                    count: count_text(s.visible, s.total, s.strong, filtered),
                })
                .collect();
            match column.kind {
                ColumnKind::Agent => {
                    view.dev = Some(dev_sections(&whole.order, all, input.dev_budget, &entries));
                }
                // A human check's column with a stage folded into it has a section per stage (Review: Ready for
                // Review · Approved); alone, it lists its cards.
                ColumnKind::HumanCheck if !parts.is_empty() => {
                    view.sections = parts
                        .into_iter()
                        .zip(&column.parts)
                        .enumerate()
                        .map(|(i, (s, part))| section(s, column, i, part.stage, filtered, &entries))
                        .collect();
                }
                _ => {
                    view.cards = entries(&whole.order.cards);
                    view.folds = whole.folds;
                }
            }
            view
        })
        .collect()
}

/// The word a folded stage's cards carry, as its owner's header counts it: "Validating" for a check by an agent
/// named Validate (its cards' "Validating…" pill), else the stage's name ("Security").
fn folded_tag(stage: &Stage) -> String {
    if stage.is_agent_check() && stage.name == "Validate" {
        "Validating".to_owned()
    } else {
        stage.name.clone()
    }
}

/// The header's tooltip: the owner's line, its tool and model, its options, then a line per folded stage.
pub fn header_tooltip(column: &FlowColumn, agents: Option<&AgentList>) -> Option<HeaderTooltip> {
    let (owner, folded) = column.parts.split_first()?;
    let stage = owner.stage;
    let agent = |s: &Stage| -> Option<&Agent> {
        let name = s.agent.as_deref()?;
        agents?
            .agents
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
    };
    let mut detail: Vec<String> = Vec::new();
    // A Local merge runs no agent (d0108 second gap review rule 6).
    let runs_agent = stage.delivery.is_none_or(|d| d.delivers != Delivers::Local);
    if runs_agent {
        let mut runs: Vec<String> = agent(stage)
            .and_then(|a| runs_on(a, agents?))
            .into_iter()
            .collect();
        if stage.is_agent_check() {
            let rounds = stage.rounds.unwrap_or(DEFAULT_VALIDATION_ROUNDS);
            let unit = if rounds == 1 { "round" } else { "rounds" };
            runs.push(format!("up to {rounds} {unit}"));
        }
        if !runs.is_empty() {
            detail.push(runs.join(" \u{b7} "));
        }
    }
    if !stage.options.is_empty() {
        let options: Vec<&str> = stage
            .options
            .iter()
            .map(|o| match o {
                StageOption::Agent(name) => name.as_str(),
                StageOption::Skip => "skip",
            })
            .collect();
        detail.push(format!("Options: {}", options.join(", ")));
    }
    for part in folded {
        let s = part.stage;
        let mut line: Vec<String> = vec![s.name.clone()];
        line.extend(s.agent.clone());
        line.extend(agent(s).and_then(|a| runs_on(a, agents?)));
        line.push(format!("shown in {}", stage.name));
        detail.push(line.join(" \u{b7} "));
    }
    let mut title: Vec<String> = vec![stage.name.clone()];
    match stage.kind {
        StageKind::Check { by: CheckBy::You } => title.push("your check".to_owned()),
        StageKind::Delivery => {
            if let Some(delivery) = stage.delivery {
                let mode = match delivery.mode {
                    DeliveryMode::Automatic => "automatic",
                    DeliveryMode::Click => "click",
                };
                title.push(match delivery.delivers {
                    Delivers::Pr => format!("PR, {mode}"),
                    Delivers::Local => format!("Local merge, {mode}"),
                    Delivers::Either => mode.to_owned(),
                });
            }
        }
        _ => {}
    }
    if runs_agent && !stage.is_human_check() {
        title.extend(stage.agent.clone());
        title.extend(stage.skill.clone());
    }
    Some(HeaderTooltip {
        title: title.join(" \u{b7} "),
        detail,
    })
}

/// "Claude Code · sonnet" (with "· effort high" where the agent sets one): the agent's own model, else the default
/// model. `None` while neither is set.
fn runs_on(agent: &Agent, list: &AgentList) -> Option<String> {
    let (tool, model) = match &agent.model {
        AgentModel::Tool { tool, model } => (*tool, model.clone()),
        AgentModel::Default => {
            let model = list.default_model.as_ref()?;
            (model.tool, model.model.clone())
        }
    };
    let mut line = vec![
        tool_word(tool).to_owned(),
        model_label(model.as_deref(), None),
    ];
    line.extend(agent.effort.as_ref().map(|e| format!("effort {e}")));
    Some(line.join(" \u{b7} "))
}

/// How many of a column's cards need the user (the folded rail's dot, `s26-10`).
fn needs_you_in(columns: &Columns, column: &str, all: &[Ticket], cards: &[&TicketKey]) -> usize {
    columns
        .order(column, None, cards, all)
        .iter()
        .filter(|k| all.iter().any(|t| &t.key == *k && t.needs_user()))
        .count()
}

fn section(
    state: StateCards,
    column: &FlowColumn,
    index: usize,
    stage: &Stage,
    filtered: bool,
    entries: &dyn Fn(&[TicketKey]) -> Vec<CardEntry>,
) -> SectionView {
    let approved = stage.is_delivery();
    let id = column.parts[index].id.clone();
    SectionView {
        target: (index > 0).then(|| DropTargetId::new(id.clone())),
        id: ReviewSectionId(id.into()),
        delivery: approved,
        title: if approved {
            APPROVED_SECTION
        } else {
            READY_FOR_REVIEW_SECTION
        },
        name: if approved {
            "Approved"
        } else {
            "Ready for Review"
        },
        count: count_text(state.visible, state.total, state.strong, filtered),
        cards: entries(&state.order.cards),
        hidden: state.total - state.visible,
        empty: if approved {
            NOTHING_APPROVED
        } else {
            NOTHING_TO_REVIEW
        },
        drag_hint: approved.then_some(APPROVED_DRAG_HINT),
    }
}

fn dev_sections(
    order: &ColumnOrder,
    all: &[Ticket],
    budget: Option<usize>,
    entries: &dyn Fn(&[TicketKey]) -> Vec<CardEntry>,
) -> DevSections {
    let floats = |k: &TicketKey| {
        all.iter()
            .find(|t| &t.key == k)
            .is_some_and(|t| t.needs_user() || t.flags.is_pinned())
    };
    let (shown_cards, fold) = match budget {
        Some(max) => {
            let f = fold_dev(order, all, max);
            let label = f.label();
            (f.shown_cards, label)
        }
        None => (order.cards.clone(), None),
    };
    let top: Vec<TicketKey> = shown_cards.iter().filter(|k| floats(k)).cloned().collect();
    let working: Vec<TicketKey> = shown_cards.iter().filter(|k| !floats(k)).cloned().collect();
    let folded = order
        .cards
        .iter()
        .filter(|k| !shown_cards.contains(k))
        .cloned()
        .collect();
    DevSections {
        top: entries(&top),
        working: entries(&working),
        working_count: order.cards.len(),
        fold,
        folded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::{NOW, ticket};
    use otaq_core::domain::Place;
    use otaq_core::domain::Status;

    fn board() -> Vec<Ticket> {
        let mut pinned = ticket("SHOP-19", "Price table", Status::Working);
        pinned.flags.pinned_at = Some(10);
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = Kind::Epic;
        let mut child = ticket("SHOP-40.3", "Wishlist page", Status::Backlog);
        child.epic = Some("SHOP-40".parse().unwrap());
        let mut t20 = ticket("SHOP-20", "Checkout page", Status::Queued);
        t20.depends_on = vec!["SHOP-18".parse().unwrap()];
        vec![
            ticket("SHOP-24", "Update footer links", Status::Backlog),
            epic,
            child,
            pinned,
            ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou),
            ticket("SHOP-23", "Split user full name", Status::AsksYou),
            ticket("SHOP-25", "Upgrade to Next.js 15", Status::Interrupted),
            ticket("SHOP-28", "Cart badge count", Status::StartInterrupted),
            ticket("SHOP-27", "Search results paging", Status::Working),
            ticket(
                "SHOP-12",
                "Add dark mode toggle",
                Status::WorkingInBackground,
            ),
            ticket("SHOP-26", "Add sitemap", Status::Working),
            t20,
            ticket("SHOP-21", "Order confirmation email", Status::Queued),
            ticket("SHOP-29", "Coupons", Status::Held),
            ticket("SHOP-18", "Checkout API", Status::WaitingForYou),
            ticket("SHOP-17", "Add Stripe client", Status::Merged),
        ]
    }

    fn keys(entries: &[CardEntry]) -> Vec<String> {
        entries.iter().map(|e| e.key().to_string()).collect()
    }

    fn input(tickets: &[Ticket], budget: Option<usize>) -> BoardInput<'_> {
        BoardInput {
            flow: &otaq_core::flow::default_file().flow,
            tickets,
            visible: None,
            strong: None,
            project_base: Some("main"),
            now_ms: NOW,
            dev_budget: budget,
            folded: &[],
            fold_empty: false,
            agent_names: &[],
            was_validating: &[],
            agents: None,
            agent_list: None,
            starts_held: None,
            paused: false,
        }
    }

    /// STG-08 on the test flow: Grill, Dev (Validate in it), Security, QA and the Delivery each have their own
    /// column between Ready for Dev and Done; a card waiting between Grill and Dev shows in Dev's column with its
    /// waiting status, a validating one in Dev too, a Security check in Security, QA's card in QA's section.
    #[test]
    fn the_test_flow_draws_a_column_per_own_stage() {
        use otaq_core::domain::Place;
        let flow = crate::view_models::flow_columns::every_kind();
        let at = |key: &str, status, place: Place| {
            let mut t = ticket(key, "Work", status);
            t.place = place;
            t
        };
        let tickets = vec![
            at("NOTE-1", Status::Working, Place::stage_of("grill")),
            at("NOTE-2", Status::Queued, Place::Waiting(Some("dev".into()))),
            at("NOTE-3", Status::Checking, Place::stage_of("validate")),
            at("NOTE-4", Status::Checking, Place::stage_of("security")),
            at("NOTE-5", Status::WaitingForYou, Place::stage_of("qa")),
            at("NOTE-6", Status::PrOpen, Place::stage_of("ship")),
            at("NOTE-7", Status::Queued, Place::Waiting(None)),
        ];
        let columns = build(&BoardInput {
            flow: &flow,
            ..input(&tickets, None)
        });
        let ids: Vec<&str> = columns.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
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
        let dev_keys = |id: &str| {
            let dev = columns
                .iter()
                .find(|c| c.id.as_str() == id)
                .unwrap()
                .dev
                .clone()
                .unwrap();
            keys(&dev.top)
                .into_iter()
                .chain(keys(&dev.working))
                .collect::<Vec<_>>()
        };
        assert_eq!(dev_keys("grill"), ["NOTE-1"]);
        assert_eq!(
            dev_keys("dev"),
            ["NOTE-3", "NOTE-2"],
            "checks, then the card waiting before Dev"
        );
        assert_eq!(dev_keys("security"), ["NOTE-4"]);
        let qa = &columns[5];
        assert!(qa.sections.is_empty(), "nothing folded into QA: its cards");
        assert_eq!(keys(&qa.cards), ["NOTE-5"]);
        assert_eq!(keys(&columns[6].cards), ["NOTE-6"]);
        assert_eq!(keys(&columns[1].cards), ["NOTE-7"]);
        let waiting = columns[3]
            .dev
            .as_ref()
            .unwrap()
            .working
            .iter()
            .find_map(|e| match e {
                CardEntry::Ticket(vm) if vm.key.to_string() == "NOTE-2" => Some(vm.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(waiting.hover.status, Status::Queued.label());
    }

    /// T-0075: a working card that turns turquoise moves up to the top of Dev, below the pinned card, and back
    /// down when the signal clears.
    #[test]
    fn needs_you_cards_float_up_as_signals_change() {
        let mut tickets = board();
        let dev = |tickets: &[Ticket]| build(&input(tickets, None))[2].dev.clone().unwrap();
        assert!(!keys(&dev(&tickets).top).contains(&"SHOP-26".to_owned()));
        let shop26 = tickets
            .iter_mut()
            .find(|t| t.key.to_string() == "SHOP-26")
            .unwrap();
        shop26.status = Status::Idle;
        let top = keys(&dev(&tickets).top);
        assert_eq!(top[0], "SHOP-19", "pinned first");
        assert!(top.contains(&"SHOP-26".to_owned()), "{top:?}");
        let shop26 = tickets
            .iter_mut()
            .find(|t| t.key.to_string() == "SHOP-26")
            .unwrap();
        shop26.status = Status::Working;
        assert!(!keys(&dev(&tickets).top).contains(&"SHOP-26".to_owned()));
    }

    /// T-0243: a Backlog card a Project agent created names it by its current name and glows at first.
    #[test]
    fn project_agent_cards_name_their_agent() {
        let mut t = ticket("SHOP-36", "Cache product prices", Status::Backlog);
        t.created_by = otaq_core::domain::Actor::ProjectAgent("a1".into());
        t.created_at = NOW - 1_000;
        let tickets = vec![t];
        let names = [("a1".to_owned(), "Checkout slowness".to_owned())];
        let columns = build(&BoardInput {
            agent_names: &names,
            ..input(&tickets, None)
        });
        let Some(CardEntry::Ticket(vm)) = columns[0].cards.first() else {
            panic!("a Backlog card")
        };
        assert!(vm.fresh);
        assert_eq!(vm.origin.as_deref(), Some("Created by Checkout slowness"));
    }

    /// T-0083: Queued cards render from the readiness reason and update as a parent moves.
    #[test]
    fn queued_cards_say_what_they_wait_for_and_update_live() {
        let mut tickets = board();
        let card = |tickets: &[Ticket], key: &str| -> CardViewModel {
            build(&input(tickets, None))[1]
                .cards
                .clone()
                .into_iter()
                .find_map(|e| match e {
                    CardEntry::Ticket(vm) if vm.key.to_string() == key => Some(vm),
                    _ => None,
                })
                .unwrap()
        };
        // SHOP-18 is in Review: the situation line says so.
        let vm = card(&tickets, "SHOP-20");
        assert_eq!(
            vm.situation.unwrap().text,
            "waits for SHOP-18 to merge (in Review)"
        );
        // SHOP-18 merged: about to start.
        let shop18 = tickets
            .iter_mut()
            .find(|t| t.key.to_string() == "SHOP-18")
            .unwrap();
        shop18.status = Status::Merged;
        let vm = card(&tickets, "SHOP-20");
        assert_eq!(vm.flag, None);
        assert!(vm.situation.unwrap().text.starts_with("Starting…"));
    }

    /// SHOP-45 Search with its two children, and the standalone SHOP-51.
    fn search(child_status: [Status; 2]) -> Vec<Ticket> {
        let mut epic = ticket("SHOP-45", "Search", Status::Backlog);
        epic.kind = Kind::Epic;
        epic.external_id = Some("PAY-130".into());
        let child = |key: &str, title: &str, status| {
            let mut t = ticket(key, title, status);
            t.epic = Some("SHOP-45".parse().unwrap());
            t
        };
        vec![
            epic,
            child("SHOP-45.1", "Search index API", child_status[0]),
            child("SHOP-45.2", "Search results page", child_status[1]),
            ticket("SHOP-51", "Fix checkout rounding", Status::Backlog),
        ]
    }

    fn column_keys(column: &ColumnView) -> Vec<String> {
        match &column.dev {
            Some(dev) => keys(&dev.top)
                .into_iter()
                .chain(keys(&dev.working))
                .collect(),
            None => keys(&column.cards)
                .into_iter()
                .chain(column.sections.iter().flat_map(|s| keys(&s.cards)))
                .collect(),
        }
    }

    /// Each child is on the board exactly once: never a card in Backlog.
    fn no_child_card_in_backlog(columns: &[ColumnView], tickets: &[Ticket]) {
        for key in column_keys(&columns[0]) {
            let t = tickets.iter().find(|t| t.key.to_string() == key).unwrap();
            assert!(t.epic.is_none(), "{key} is an epic child card in Backlog");
        }
    }

    /// EPC-11: SHOP-45 with nothing started is one card with its children as rows, and no child cards.
    #[test]
    fn an_epic_before_it_starts_is_one_card() {
        let tickets = search([Status::Backlog, Status::Backlog]);
        let columns = build(&input(&tickets, None));
        assert_eq!(column_keys(&columns[0]), ["SHOP-45", "SHOP-51"]);
        assert_eq!(columns[0].count, "2");
        let Some(CardEntry::Epic(epic)) = columns[0].cards.first() else {
            panic!("the epic card")
        };
        assert_eq!(epic.children.len(), 2);
        assert!(epic.children.iter().all(|c| c.draggable));
        assert!(columns[1..].iter().all(|c| column_keys(c).is_empty()));
        no_child_card_in_backlog(&columns, &tickets);
    }

    /// EPC-11, EPC-12, EPL-01: SHOP-45.1 queued puts the epic in flight: no epic card, 45.1 in Ready for Dev, 45.2
    /// a Backlog card of its own (no "not queued" stop above the columns). Backlog counts
    /// what it shows.
    #[test]
    fn a_queued_child_puts_the_epic_in_flight() {
        let tickets = search([Status::Queued, Status::Backlog]);
        let columns = build(&input(&tickets, None));
        assert_eq!(column_keys(&columns[0]), ["SHOP-45.2", "SHOP-51"]);
        assert_eq!(columns[0].count, "2");
        assert_eq!(column_keys(&columns[1]), ["SHOP-45.1"]);
        assert_eq!(columns[1].count, "1");
        assert!(
            columns
                .iter()
                .all(|c| !column_keys(c).iter().any(|k| k == "SHOP-45")),
            "no epic card"
        );

        // Running children are cards as usual; a merged one is in Done.
        let tickets = search([Status::Merged, Status::Working]);
        let columns = build(&input(&tickets, None));
        assert_eq!(column_keys(&columns[0]), ["SHOP-51"]);
        assert_eq!(column_keys(&columns[2]), ["SHOP-45.2"]);
        assert_eq!(column_keys(&columns[5]), ["SHOP-45.1"]);

        // SHOP-45.1 merged and SHOP-45.2 taken back: still in flight, 45.2 is a Backlog card.
        let tickets = search([Status::Merged, Status::Backlog]);
        let columns = build(&input(&tickets, None));
        assert_eq!(column_keys(&columns[0]), ["SHOP-45.2", "SHOP-51"]);
    }

    /// EPC-14: after Park all the epic is a card again, its rows "Parked · has work" and draggable.
    #[test]
    fn after_park_all_the_epic_is_a_card_with_parked_rows() {
        let tickets = search([Status::Parked, Status::Parked]);
        let columns = build(&input(&tickets, None));
        assert_eq!(column_keys(&columns[0]), ["SHOP-45", "SHOP-51"]);
        let Some(CardEntry::Epic(epic)) = columns[0].cards.first() else {
            panic!("the epic card")
        };
        let rows: Vec<(&str, bool)> = epic
            .children
            .iter()
            .map(|c| (c.status_label, c.draggable))
            .collect();
        assert_eq!(
            rows,
            [("Parked · has work", true), ("Parked · has work", true)]
        );
        no_child_card_in_backlog(&columns, &tickets);
    }

    /// EPC-04: SHOP-40 in Review is its delivery card there, as before; its merged children stay in Done.
    #[test]
    fn an_epic_in_review_is_its_delivery_card() {
        let mut epic = ticket("SHOP-40", "Wishlist", Status::WaitingForYou);
        epic.kind = Kind::Epic;
        let mut tickets = vec![epic];
        for key in ["SHOP-40.1", "SHOP-40.2"] {
            let mut t = ticket(key, "Child", Status::Merged);
            t.epic = Some("SHOP-40".parse().unwrap());
            tickets.push(t);
        }
        let columns = build(&input(&tickets, None));
        assert!(column_keys(&columns[0]).is_empty());
        assert_eq!(columns[0].count, "0");
        assert!(matches!(
            columns[4].sections[0].cards.as_slice(),
            [CardEntry::Epic(e)] if e.key.to_string() == "SHOP-40"
        ));
        assert_eq!(column_keys(&columns[5]), ["SHOP-40.1", "SHOP-40.2"]);
    }

    /// SHOP-40 Wishlist in flight: 40.1 and 40.2 merged, 40.3 working, 40.5 closed; SHOP-17 and SHOP-30 merged
    /// on their own. `created_at` sets Done's order: 40.5, 17, 40.1, 30, 40.2.
    fn wishlist_done() -> Vec<Ticket> {
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = Kind::Epic;
        let child = |key: &str, status, created_at| {
            let mut t = ticket(key, "Child", status);
            t.epic = Some("SHOP-40".parse().unwrap());
            t.created_at = created_at;
            t
        };
        let mut t17 = ticket("SHOP-17", "Add Stripe client", Status::Merged);
        t17.created_at = 5;
        let mut t30 = ticket("SHOP-30", "Coupons", Status::Merged);
        t30.created_at = 15;
        vec![
            epic,
            child("SHOP-40.1", Status::Merged, 10),
            child("SHOP-40.2", Status::Merged, 20),
            child("SHOP-40.3", Status::Working, 30),
            child("SHOP-40.5", Status::ClosedNotMerged, 3),
            t17,
            t30,
        ]
    }

    /// EPC-16: two merged children and a closed one give one fold line ("Wishlist · 2 of 4 merged", merged out of
    /// all four children) and one card; the fold sits where 40.1 sorts, and Done counts it as one.
    #[test]
    fn merged_children_fold_into_one_line_in_done() {
        let tickets = wishlist_done();
        let columns = build(&input(&tickets, None));
        let done = &columns[5];
        assert_eq!(
            keys(&done.cards),
            ["SHOP-40.5", "SHOP-17", "SHOP-40.1", "SHOP-40.2", "SHOP-30"],
            "the merged children follow each other from where the first sorts; the closed one stays a card"
        );
        assert_eq!(
            done.folds,
            [DoneFold {
                epic: "SHOP-40".parse().unwrap(),
                // SHOP-40.5 is closed: counted apart (#31).
                label: "Wishlist · 2 of 3 merged · 1 closed".into(),
                keys: vec!["SHOP-40.1".parse().unwrap(), "SHOP-40.2".parse().unwrap()],
            }]
        );
        assert_eq!(done.count, "4", "40.5, 17, the fold and 30");
        assert!(columns[..5].iter().all(|c| c.folds.is_empty()));

        // The first merged child folds at once ("1 of 4"), and a closed child never does.
        let mut tickets = wishlist_done();
        tickets[2].status = Status::Working;
        tickets[2].place = otaq_core::domain::Place::stage_of("dev");
        let done = &build(&input(&tickets, None))[5];
        assert_eq!(done.folds.len(), 1);
        assert_eq!(done.folds[0].label, "Wishlist · 1 of 3 merged · 1 closed");
        assert_eq!(done.count, "4");

        // Under a filter the fold counts once, shown over total.
        let tickets = wishlist_done();
        let visible: Vec<TicketKey> = ["SHOP-40.2", "SHOP-17"]
            .iter()
            .map(|k| k.parse().unwrap())
            .collect();
        let done = &build(&BoardInput {
            visible: Some(&visible),
            ..input(&tickets, None)
        })[5];
        assert_eq!(keys(&done.cards), ["SHOP-17", "SHOP-40.2"]);
        assert_eq!(done.folds[0].keys.len(), 1);
        assert_eq!(done.folds[0].label, "Wishlist · 2 of 3 merged · 1 closed");
        assert_eq!((done.count.as_str(), done.hidden), ("2/4", 2));
    }

    /// SHL-06, FLW-01: six columns (Validate has its own); Review counts both sections.
    #[test]
    fn six_columns_with_counts_and_epics_in_backlog() {
        let tickets = board();
        let columns = build(&input(&tickets, None));
        let titles: Vec<_> = columns.iter().map(|c| c.title.as_str()).collect();
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
        let ids: Vec<_> = columns.iter().map(|c| c.id.clone()).collect();
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
        let counts: Vec<_> = columns.iter().map(|c| c.count.as_str()).collect();
        assert_eq!(counts, ["2", "4", "7", "0", "1", "1"]);
        // The epic is a card with SHOP-40.3 as its row, not a second Backlog card.
        assert_eq!(keys(&columns[0].cards), ["SHOP-24", "SHOP-40"]);
        assert!(
            columns[0]
                .cards
                .iter()
                .any(|c| matches!(c, CardEntry::Epic(e) if e.children.len() == 1))
        );
        // Ready for Dev: Start interrupted first, then the start order (the one waiting for SHOP-18 last).
        assert_eq!(
            keys(&columns[1].cards),
            ["SHOP-28", "SHOP-21", "SHOP-29", "SHOP-20"]
        );
        assert_eq!(columns[1].hint, Some(READY_FOR_DEV_HINT));
        assert!(columns.iter().filter(|c| c.hint.is_some()).count() == 1);
        // Only Review has sections; its empty Approved section says so.
        assert!(columns.iter().filter(|c| !c.sections.is_empty()).count() == 1);
        let review = &columns[4];
        assert!(review.cards.is_empty());
        let sections: Vec<_> = review
            .sections
            .iter()
            .map(|s| (s.id.clone(), s.title, s.count.as_str(), keys(&s.cards)))
            .collect();
        assert_eq!(
            sections,
            [
                (
                    ReviewSectionId::READY_FOR_REVIEW,
                    READY_FOR_REVIEW_SECTION,
                    "1",
                    vec!["SHOP-18".to_owned()]
                ),
                (ReviewSectionId::APPROVED, APPROVED_SECTION, "0", vec![]),
            ]
        );
        assert_eq!(review.sections[1].empty, "Nothing approved");
        assert!(
            review.sections[1]
                .drag_hint
                .unwrap()
                .starts_with("Drag a ticket here from Ready for Review")
        );
        assert_eq!(review.sections[0].drag_hint, None);
    }

    /// §7.5 in each section: pinned first (most recently pinned first), then the old columns' order; the column
    /// counts both sections, under a filter too.
    #[test]
    fn review_sections_keep_their_own_order_and_counts() {
        let mut pinned = ticket("SHOP-31", "Hotfix", Status::WaitingForYou);
        pinned.flags.pinned_at = Some(20);
        let mut older_pin = ticket("SHOP-32", "Footer", Status::WaitingForYou);
        older_pin.flags.pinned_at = Some(10);
        let mut pr = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        let tickets = vec![
            ticket("SHOP-18", "Checkout API", Status::WaitingForYou),
            older_pin,
            pinned,
            pr,
            ticket("SHOP-27", "Search paging", Status::PrOpen),
        ];
        let columns = build(&input(&tickets, None));
        let review = &columns[4];
        assert_eq!(review.count, "5");
        assert_eq!(
            keys(&review.sections[0].cards),
            ["SHOP-31", "SHOP-32", "SHOP-18"]
        );
        assert_eq!(review.sections[0].count, "3");
        assert_eq!(review.sections[1].count, "2");
        assert_eq!(keys(&review.sections[1].cards).len(), 2);

        let visible: Vec<TicketKey> = vec!["SHOP-18".parse().unwrap(), "SHOP-26".parse().unwrap()];
        let filtered = build(&BoardInput {
            visible: Some(&visible),
            ..input(&tickets, None)
        });
        let review = &filtered[4];
        assert_eq!(
            (review.count.as_str(), review.hidden),
            ("2/5", 3),
            "the column sums its sections"
        );
        assert_eq!(
            (review.sections[0].count.as_str(), review.sections[0].hidden),
            ("1/3", 2)
        );
        assert_eq!(
            (review.sections[1].count.as_str(), review.sections[1].hidden),
            ("1/2", 1)
        );
    }

    /// FLW-01 (T-0412's Done-when): the default board is Backlog · Ready for Dev · Dev · Validate · Review · Done; a
    /// validating card is in Validate, Dev has no section header, Review keeps Ready for review · Approved. A flow
    /// that folds its check into Dev (the test flow's `with-previous`) keeps the header.
    #[test]
    fn the_default_board_has_validate_in_its_own_column() {
        use otaq_core::domain::Place;
        let mut validating = ticket("SHOP-30", "Order history", Status::Checking);
        validating.place = Place::stage_of("validate");
        let tickets = vec![
            ticket("SHOP-19", "Price table", Status::Working),
            validating,
            ticket("SHOP-18", "Checkout API", Status::WaitingForYou),
        ];
        let columns = build(&input(&tickets, None));
        let titles: Vec<&str> = columns.iter().map(|c| c.title.as_str()).collect();
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
        let dev = columns[2].dev.as_ref().expect("Dev's cards");
        assert!(columns[2].folded.is_empty(), "nothing folded into Dev");
        assert_eq!(keys(&dev.working), ["SHOP-19"]);
        assert!(columns[2].sections.is_empty());
        let validate = columns[3].dev.as_ref().expect("Validate's cards");
        assert_eq!(keys(&validate.working), ["SHOP-30"]);
        assert_eq!(columns[3].count, "1");
        let review: Vec<_> = columns[4].sections.iter().map(|s| s.title).collect();
        assert_eq!(review, [READY_FOR_REVIEW_SECTION, APPROVED_SECTION]);

        let flow = crate::view_models::flow_columns::every_kind();
        let columns = build(&BoardInput {
            flow: &flow,
            ..input(&tickets, None)
        });
        let dev = columns.iter().find(|c| c.id.as_str() == "dev").unwrap();
        assert_eq!(
            dev.folded,
            [FoldedView {
                tag: "Validating".into(),
                name: "Validate".into(),
                count: "1".into()
            }]
        );
        assert_eq!(dev.count, "1", "Dev's own card");
    }

    #[test]
    fn dev_sections_float_needs_you_and_pins() {
        let tickets = board();
        let columns = build(&input(&tickets, None));
        let dev = columns[2].dev.as_ref().unwrap();
        assert_eq!(keys(&dev.top), ["SHOP-19", "SHOP-22", "SHOP-23", "SHOP-25"]);
        assert_eq!(keys(&dev.working), ["SHOP-12", "SHOP-26", "SHOP-27"]);
        assert_eq!(dev.working_count, 7);
        assert_eq!(dev.fold, None);
    }

    #[test]
    fn fold_counts_and_never_hides_needs_you() {
        let tickets = board();
        let columns = build(&input(&tickets, Some(5)));
        let dev = columns[2].dev.as_ref().unwrap();
        assert_eq!(dev.top.len(), 4, "pinned + 3 needs-you shown");
        assert_eq!(keys(&dev.working), ["SHOP-12"]);
        assert_eq!(dev.fold.as_deref(), Some("2 more · working"));
        assert_eq!(dev.folded.len(), 2, "the folded cards are named");
        let tight = build(&input(&tickets, Some(1)));
        let dev = tight[2].dev.as_ref().unwrap();
        assert_eq!(dev.top.len(), 3, "the needs-you cards are never folded");
    }

    /// BRD-02: Done is folded at first (the view preference's default) and opens from its line; a column folded from
    /// its ⋯ is a rail that counts what needs the user; Fold empty columns folds every empty column but Backlog.
    #[test]
    fn columns_fold_from_their_menu_and_when_empty() {
        let tickets = board();
        let done_folded = [otaq_core::board::settings::DONE_COLUMN.to_owned()];
        let columns = build(&BoardInput {
            folded: &done_folded,
            ..input(&tickets, None)
        });
        let folded: Vec<&str> = columns
            .iter()
            .filter(|c| c.collapsed)
            .map(|c| c.title.as_str())
            .collect();
        assert_eq!(folded, ["Done"]);
        // Dev folded from its ⋯: a rail with a dot for SHOP-22 and SHOP-25.
        let dev = ["dev".to_owned()];
        let columns = build(&BoardInput {
            folded: &dev,
            ..input(&tickets, None)
        });
        assert!(columns[2].collapsed);
        assert!(columns[2].needs_you > 0);
        assert!(!columns[4].collapsed, "Done opened");
        // Fold empty columns: nothing here is empty but Backlog stays open anyway.
        let empty: Vec<Ticket> = Vec::new();
        let columns = build(&BoardInput {
            fold_empty: true,
            ..input(&empty, None)
        });
        let open: Vec<&str> = columns
            .iter()
            .filter(|c| !c.collapsed)
            .map(|c| c.title.as_str())
            .collect();
        assert_eq!(open, ["Backlog"]);
        assert_eq!(dev_budget(900.), 8);
        assert_eq!(dev_budget(300.), 4);
    }

    /// BRD-02's one width rule: 180 px a column at least; columns that fit share the width, else the board scrolls
    /// and the pill counts the columns cut off at the right edge, fewer as it scrolls.
    #[test]
    fn one_width_rule_and_the_more_stages_pill() {
        // Seven columns at 180 with 6 gaps of 12 and 2 × 16 padding: 1364 px.
        assert!(fits(1364., 7, 0));
        assert!(!fits(1363., 7, 0));
        assert!(
            fits(1364. - 180. + 52., 6, 1),
            "a folded column is a 52 px rail"
        );
        let seven = [false; 7];
        assert_eq!(columns_after(1376., 0., &seven), 0);
        assert_eq!(columns_after(1216., 0., &seven), 1);
        assert_eq!(
            more_stages(columns_after(1216., 0., &seven)).as_deref(),
            Some("1 more stage")
        );
        let fourteen = [false; 14];
        assert_eq!(columns_after(1376., 0., &fourteen), 7);
        assert_eq!(columns_after(1376., 192. * 3., &fourteen), 4);
        assert_eq!(columns_after(1376., 192. * 14., &fourteen), 0);
        assert_eq!(more_stages(0), None);
        assert_eq!(more_stages(7).as_deref(), Some("7 more stages"));
    }

    #[test]
    fn a_filter_shows_shown_over_total() {
        let tickets = board();
        let visible: Vec<TicketKey> = vec!["SHOP-24".parse().unwrap(), "SHOP-22".parse().unwrap()];
        let columns = build(&BoardInput {
            visible: Some(&visible),
            ..input(&tickets, None)
        });
        assert_eq!(columns[0].count, "1/2", "the epic's row is not a card");
        assert_eq!(columns[0].hidden, 1);
        assert_eq!(columns[2].count, "1/7");
        assert_eq!(keys(&columns[2].dev.as_ref().unwrap().top), ["SHOP-22"]);
    }

    /// Q55: a focused epic dims the other cards; they stay in their columns, nothing says "hidden", and the count
    /// counts the cards at full strength.
    #[test]
    fn a_focused_epic_dims_and_counts_its_cards() {
        let tickets = board();
        let strong: Vec<TicketKey> = vec!["SHOP-40".parse().unwrap(), "SHOP-40.3".parse().unwrap()];
        let columns = build(&BoardInput {
            strong: Some(&strong),
            ..input(&tickets, None)
        });
        assert_eq!(
            keys(&columns[0].cards),
            ["SHOP-24", "SHOP-40"],
            "dimmed, not hidden"
        );
        assert_eq!(columns[0].count, "1/2");
        assert_eq!(columns[0].hidden, 0);
        assert_eq!(columns[2].count, "0/7");
    }

    /// FLW-12 on the default flow: today's icons in their status colours, but Validate's shield (FLW-01); a tooltip
    /// for each stage column; the Ready for Dev hint.
    #[test]
    fn the_default_board_keeps_todays_headers() {
        use crate::view_models::agents::fixtures::{agent, on};
        let tickets = board();
        let list = AgentList {
            agents: vec![
                agent("a1", "Developer", Some("sonnet")),
                on(
                    otaq_core::agent_tools::Tool::Codex,
                    agent("a2", "Validator", Some("gpt-5.5")),
                ),
            ],
            default_model: None,
            fallbacks: Vec::new(),
        };
        let columns = build(&BoardInput {
            agent_list: Some(&list),
            ..input(&tickets, None)
        });
        let looks: Vec<(IconName, bool)> = columns.iter().map(|c| (c.icon, c.quiet_icon)).collect();
        assert_eq!(
            looks,
            [
                (IconName::CircleDashed, false),
                (IconName::Hourglass, false),
                (IconName::Activity, false),
                (IconName::ShieldCheck, false),
                (IconName::Eye, false),
                (IconName::CircleCheck, false),
            ]
        );
        assert_eq!(columns[3].empty_hint, AGENT_CHECK_EMPTY);
        assert!(
            columns
                .iter()
                .all(|c| c.folded.is_empty() || c.kind == ColumnKind::HumanCheck)
        );
        let tip = |i: usize| columns[i].tooltip.clone();
        assert_eq!(tip(0), None);
        assert_eq!(tip(1), None);
        assert_eq!(
            tip(2),
            Some(HeaderTooltip {
                title: "Dev · Developer · work-ticket".into(),
                detail: vec!["Claude Code · sonnet".into()],
            })
        );
        assert_eq!(
            tip(3),
            Some(HeaderTooltip {
                title: "Validate · Validator · validate".into(),
                detail: vec!["Codex · gpt-5.5 · up to 3 rounds".into()],
            })
        );
        assert_eq!(
            tip(4),
            Some(HeaderTooltip {
                title: "Review · your check".into(),
                detail: vec!["Done · Developer · Claude Code · sonnet · shown in Review".into()],
            })
        );
        assert_eq!(tip(5), None);
        assert_eq!(columns[1].hint, Some(READY_FOR_DEV_HINT));
        assert!(columns.iter().all(|c| !c.collapsed));
    }

    /// FLW-12 on the test flow: muted kind icons (bot, shield, person, PR), Dev counting its own card then
    /// "Validating 1", the Delivery named "Done · PR", and the tooltips with agent, tool, model and options.
    #[test]
    fn the_test_flow_headers_name_kinds_folds_and_the_delivery() {
        use crate::view_models::agents::fixtures::agent;
        let flow = crate::view_models::flow_columns::every_kind();
        let at = |key: &str, status, place: Place| {
            let mut t = ticket(key, "Work", status);
            t.place = place;
            t
        };
        let tickets = vec![
            at("NOTE-1", Status::Working, Place::stage_of("dev")),
            at("NOTE-2", Status::Working, Place::stage_of("dev")),
            at("NOTE-3", Status::Checking, Place::stage_of("validate")),
            at("NOTE-4", Status::Checking, Place::stage_of("security")),
        ];
        let list = AgentList {
            agents: vec![
                agent("g", "Griller", Some("sonnet")),
                agent("r", "Rust developer", Some("sonnet")),
                agent("s", "Security reviewer", Some("opus")),
                agent("p", "Shipper", Some("haiku")),
            ],
            default_model: None,
            fallbacks: Vec::new(),
        };
        let columns = build(&BoardInput {
            flow: &flow,
            agent_list: Some(&list),
            ..input(&tickets, None)
        });
        let heads: Vec<(&str, IconName, bool)> = columns
            .iter()
            .map(|c| (c.title.as_str(), c.icon, c.quiet_icon))
            .collect();
        assert_eq!(
            heads,
            [
                ("Backlog", IconName::CircleDashed, true),
                ("Ready for Dev", IconName::Hourglass, true),
                ("Grill", IconName::Bot, true),
                ("Dev", IconName::Bot, true),
                ("Security", IconName::ShieldCheck, true),
                ("QA", IconName::UserCheck, true),
                ("Done · PR", IconName::GitPullRequest, true),
                ("Done", IconName::CircleCheck, true),
            ]
        );
        let dev = &columns[3];
        assert_eq!(dev.count, "2", "Dev's own cards");
        assert_eq!(
            dev.folded,
            [FoldedView {
                tag: "Validating".into(),
                name: "Validate".into(),
                count: "1".into()
            }]
        );
        assert_eq!(dev.total, 3);
        assert_eq!(
            dev.tooltip,
            Some(HeaderTooltip {
                title: "Dev · Rust developer · work-ticket".into(),
                detail: vec![
                    "Claude Code · sonnet".into(),
                    "Options: Quick developer".into(),
                    // Validator isn't in the list: its line names the agent only.
                    "Validate · Validator · shown in Dev".into(),
                ],
            })
        );
        let tip = |i: usize| columns[i].tooltip.clone().unwrap();
        assert_eq!(tip(2).title, "Grill · Griller · grill");
        assert_eq!(tip(2).detail, ["Claude Code · sonnet", "Options: skip"]);
        assert_eq!(
            tip(4).title,
            "Security · Security reviewer · security-review"
        );
        assert_eq!(tip(4).detail, ["Claude Code · opus · up to 2 rounds"]);
        assert_eq!(tip(5).title, "QA · your check");
        assert_eq!(tip(5).detail, ["Options: skip"]);
        assert_eq!(tip(6).title, "Done · PR, automatic · Shipper · open-pr");
        assert_eq!(tip(6).detail, ["Claude Code · haiku"]);

        // A filter counts each part on its own: "1/2 · Validating 0/1".
        let visible: Vec<TicketKey> = vec!["NOTE-1".parse().unwrap()];
        let filtered = build(&BoardInput {
            flow: &flow,
            visible: Some(&visible),
            ..input(&tickets, None)
        });
        assert_eq!(filtered[3].count, "1/2");
        assert_eq!(filtered[3].folded[0].count, "0/1");
        assert_eq!(filtered[3].hidden, 2);
    }

    /// shop-web's ten columns (*Row · A flow with many stages*): Backlog · Ready for Dev · Grill · Dev (Validate in
    /// it) · Security · QA · Review · Docs · Done · PR · Done.
    fn ten_stages() -> Flow {
        let yaml = "\
flow:
  - {id: grill, name: Grill, kind: agent, agent: Griller, skill: grill, options: [skip]}
  - {id: dev, name: Dev, kind: agent, agent: Rust developer, skill: work-ticket}
  - {id: validate, name: Validate, kind: check, by: agent, column: with-previous, agent: Validator, skill: validate, rounds: 3}
  - {id: security, name: Security, kind: check, by: agent, agent: Security reviewer, skill: security-review, rounds: 3}
  - {id: qa, name: QA, kind: check, by: you}
  - {id: review, name: Review, kind: check, by: you}
  - {id: docs, name: Docs, kind: agent, agent: Doc writer, skill: docs}
  - {id: ship, name: Done, kind: delivery, agent: Shipper, skill: open-pr, mode: automatic, delivers: pr}
";
        otaq_core::flow::yaml::read(yaml).unwrap().flow
    }

    /// BRD-02, a long flow (`s24-04`): every column keeps its 180 px, nothing folds by itself (Fold empty columns
    /// is off), Ready for Dev keeps its hint, and at 1440 the board scrolls; Fold empty columns folds Grill and Docs.
    #[test]
    fn a_long_flow_keeps_its_columns_and_scrolls() {
        let flow = ten_stages();
        let at = |key: &str, status, place: Place| {
            let mut t = ticket(key, "Work", status);
            t.place = place;
            t
        };
        let tickets = vec![
            at("SHOP-41", Status::Backlog, Place::Backlog),
            at("SHOP-35", Status::Queued, Place::Waiting(None)),
            at("SHOP-33", Status::Working, Place::stage_of("dev")),
            at("SHOP-30", Status::Checking, Place::stage_of("validate")),
            at("SHOP-19", Status::Checking, Place::stage_of("security")),
            at("SHOP-23", Status::WaitingForYou, Place::stage_of("qa")),
            at("SHOP-18", Status::WaitingForYou, Place::stage_of("review")),
            at("SHOP-26", Status::PrOpen, Place::stage_of("ship")),
        ];
        let columns = build(&BoardInput {
            flow: &flow,
            ..input(&tickets, None)
        });
        assert_eq!(columns.len(), 10);
        assert!(columns.iter().all(|c| !c.collapsed));
        assert_eq!(columns[1].hint, Some(READY_FOR_DEV_HINT));
        let rails: Vec<bool> = columns.iter().map(|c| c.collapsed).collect();
        assert_eq!(
            more_stages(columns_after(1440. - 64., 0., &rails)).as_deref(),
            Some("3 more stages")
        );
        let folded = build(&BoardInput {
            flow: &flow,
            fold_empty: true,
            ..input(&tickets, None)
        });
        let rails: Vec<&str> = folded
            .iter()
            .filter(|c| c.collapsed)
            .map(|c| c.title.as_str())
            .collect();
        assert_eq!(rails, ["Grill", "Docs", "Done"]);
    }

    /// FLW-06 (the Ready for Dev footer): while a flow change holds starts, a card that would start says why; one
    /// that waits for a parent keeps saying so.
    #[test]
    fn held_starts_say_they_wait_for_the_flow_change() {
        let tickets = board();
        let held = build(&BoardInput {
            starts_held: Some(&Hold::READY_FOR_DEV),
            ..input(&tickets, None)
        });
        let card = |key: &str| -> CardViewModel {
            held[1]
                .cards
                .iter()
                .find_map(|e| match e {
                    CardEntry::Ticket(vm) if vm.key.to_string() == key => Some(vm.clone()),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(
            card("SHOP-21").situation.unwrap().text,
            WAITING_FOR_THE_FLOW_CHANGE
        );
        assert_eq!(
            card("SHOP-20").situation.unwrap().text,
            "waits for SHOP-18 to merge (in Review)"
        );
        // Without a hold it starts.
        let free = build(&input(&tickets, None));
        let CardEntry::Ticket(vm) = &free[1].cards[1] else {
            panic!()
        };
        assert_eq!(vm.key.to_string(), "SHOP-21");
        assert!(vm.situation.clone().unwrap().text.starts_with("Starting"));
    }

    /// PAU-04 (`s24-07`): while paused nothing says "Starting…": Ready for Dev's next one "starts next", and one
    /// dragged into a stage waits for Resume.
    #[test]
    fn while_paused_a_ticket_that_would_start_waits() {
        let mut tickets = board();
        let paused = build(&BoardInput {
            paused: true,
            ..input(&tickets, None)
        });
        let line = |columns: &[ColumnView], key: &str| -> String {
            columns
                .iter()
                .flat_map(|c| {
                    let dev = c.dev.iter().flat_map(|d| d.top.iter().chain(&d.working));
                    c.cards.iter().chain(dev)
                })
                .find_map(|e| match e {
                    CardEntry::Ticket(vm) if vm.key.to_string() == key => {
                        vm.one_line.line.clone().map(|l| l.text)
                    }
                    _ => None,
                })
                .unwrap()
        };
        assert!(line(&paused, "SHOP-21").ends_with("starts next"));
        let t = tickets
            .iter_mut()
            .find(|t| t.key.to_string() == "SHOP-21")
            .unwrap();
        t.place = otaq_core::domain::Place::of(Status::Queued, Some("dev"), None);
        let paused = build(&BoardInput {
            paused: true,
            ..input(&tickets, None)
        });
        assert_eq!(
            line(&paused, "SHOP-21"),
            "Paused \u{b7} starts when you resume"
        );
    }

    /// FLW-19, Q273, EPL-01: an epic of a flow with no human check, done with its children, waits at Review epic as
    /// its card in the Delivery's column (nothing above the columns shows it).
    #[test]
    fn an_epic_at_review_epic_is_a_card_in_the_deliverys_column() {
        let mut flow = crate::view_models::flow_columns::every_kind();
        flow.stages.retain(|s| !s.is_human_check());
        let mut epic = ticket("SHOP-12", "Plugin API", Status::WaitingForYou);
        epic.kind = Kind::Epic;
        epic.place = Place::stage_of(REVIEW_EPIC);
        let mut tickets = vec![epic];
        for key in ["SHOP-12.1", "SHOP-12.2"] {
            let mut t = ticket(key, "Child", Status::Merged);
            t.epic = Some("SHOP-12".parse().unwrap());
            t.place = Place::Done;
            tickets.push(t);
        }
        let columns = build(&BoardInput {
            flow: &flow,
            ..input(&tickets, None)
        });
        let ship = columns.iter().find(|c| c.id.as_str() == "ship").unwrap();
        assert!(
            column_keys(ship).contains(&"SHOP-12".to_owned()),
            "its card"
        );

        // On the default flow the epic waits in Review as its card, as today.
        tickets[0].place = Place::stage_of("review");
        let columns = build(&input(&tickets, None));
        assert!(column_keys(&columns[4]).contains(&"SHOP-12".to_owned()));
    }
}

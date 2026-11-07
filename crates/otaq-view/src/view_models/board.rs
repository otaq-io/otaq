//! The board's columns (UI-02, SHL-06, STG-08; *03 · Board & columns*; d0087, d0108): the project's flow's columns
//! with counts ([`flow_columns`]), on the default flow Backlog · Ready for Dev · Dev · Validate · Review · Done.
//! Every stage is its own column; a human check's column (Review) also holds the tickets being delivered (To
//! deliver, PR open), after the ones waiting for you, with no section of their own. Ready for Dev shows the queued
//! cards (d0066); an agent column (Dev) the cards that need the user (and pins)
//! on top, then the Agent Working section; the "N more · working" fold that never hides needs-you cards (T-0027);
//! and the empty-column hints. Order comes from the core ([`otaq_core::board::order`]), so the
//! board, the service and the tests agree.
//!
//! T-0460 (BRD-02): one width rule ([`COLUMN_MIN_WIDTH`], [`fits`], [`columns_after`] for the "7 more stages" pill)
//! replaces the long-flow folding and the stage strip; a column is folded ([`is_folded`]) from its ⋯ or by Fold empty
//! columns (the project's view preferences), Done at first.
//!
//! One card per ticket in every column (d0133).

use otaq_core::agents::{Agent, AgentList, AgentModel};
use otaq_core::board::order::{ColumnOrder, fold_dev};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Status, StatusSet, TicketKey};
use otaq_core::flow::check::DEFAULT_VALIDATION_ROUNDS;
use otaq_core::flow::{CheckBy, Flow, Stage, StageKind, StageOption};

use crate::icons::IconName;
use crate::view_models::agents::model_label;
use crate::view_models::board_page::ColumnId;
use crate::view_models::card::{CardExtras, CardViewModel};
use crate::view_models::flow_columns::{ColumnKind, Columns, FlowColumn, keeps_default_look};
use crate::view_models::queue::queue_facts;
use crate::view_models::tool_problems::tool_word;
use otaq_core::agents::facts::BoardAgents;

/// The one width rule (BRD-02): a column is never narrower than this; when the flow fits, the columns share the
/// board's width, otherwise the board scrolls sideways with the "7 more stages" pill at its edge.
pub const COLUMN_MIN_WIDTH: f32 = 240.;

/// A column folded from its ⋯ (or by Fold empty columns): the narrow rail with its icon, count and name (`s26-10`).
pub const FOLDED_COLUMN_WIDTH: f32 = 52.;

/// The board's padding on each side and the gap between its columns (`board.css`; *24 · The two views*).
pub const BOARD_PADDING: f32 = 16.;
pub const COLUMN_GAP: f32 = 12.;

#[derive(Debug, Clone, PartialEq)]
pub enum CardEntry {
    Ticket(CardViewModel),
}

impl CardEntry {
    pub fn key(&self) -> &TicketKey {
        match self {
            Self::Ticket(t) => &t.key,
        }
    }
}

/// Dev's parts, top to bottom.
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

/// Who runs a column's stage, on hovering its title (FLW-12; *Row · Stage kinds on the column header*): "Product spec
/// · Spec writer · product-spec" over "Claude Code · sonnet" and "Options: skip".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderTooltip {
    pub title: String,
    pub detail: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnView {
    pub id: ColumnId,
    pub kind: ColumnKind,
    /// The stage's name ("Dev", "Review"), or "Backlog", "Ready for Dev", "Done".
    pub title: String,
    pub icon: IconName,
    /// The icon in the muted colour (FLW-12: a flow that isn't the default's shape); else in its status colour.
    pub quiet_icon: bool,
    /// "12" or, with a filter, "3/12" (T-0038).
    pub count: String,
    /// Who runs it; `None` for Backlog, Ready for Dev and Done.
    pub tooltip: Option<HeaderTooltip>,
    /// Its cards over all, whatever the filter: an empty stage of a long flow folds to its rail.
    pub total: usize,
    /// Every column but the agent columns.
    pub cards: Vec<CardEntry>,
    /// An agent column's (Dev's) parts.
    pub dev: Option<DevSections>,
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
}

/// A column's icon and empty hint (FLW-12): by the kind of the stage that owns it. `quiet` is a flow that isn't the
/// default's shape: an Agent stage is a bot and a human check a person (on the default flow they keep today's
/// activity and eye); an agent check is a shield, on any flow (Validate's own column, FLW-01).
pub fn column_look(column: &FlowColumn, quiet: bool) -> (IconName, &'static str) {
    let owner = column.stage;
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
    /// The project's helpers (`board.helpers`, d0133): a build stage's piece rows.
    pub helpers: &'a [otaq_core::helpers::Helper],
}

/// A ticket that waits with a kept start while its project is paused (PAU-04, `agents::kept::starts_on_resume`), in
/// a stage it was dragged or moved into; Ready for Dev's says "starts next" (`s24-07`).
pub const KEPT_START: &str = "Paused \u{b7} starts when you resume";

/// One state's cards, in its order, with what its count counts.
struct StateCards {
    order: ColumnOrder,
    /// Cards over all.
    total: usize,
    /// Cards the filter shows.
    visible: usize,
}

impl StateCards {
    /// The cards of `column`.
    fn of(
        columns: &Columns,
        column: &str,
        input: &BoardInput,
        cards: &[&TicketKey],
        shown: &[&TicketKey],
    ) -> Self {
        let all = input.tickets;
        // Order over the whole project ("blocks more" counts every dependent), cards only; the counts count the
        // cards the column shows.
        let full = columns.order(column, cards, all);
        let keep = |keys: &[TicketKey], of: &[&TicketKey]| -> Vec<TicketKey> {
            keys.iter().filter(|k| of.contains(k)).cloned().collect()
        };
        let shown_cards = keep(&full, shown);
        let total = full.len();
        let visible = shown_cards.len();
        Self {
            order: ColumnOrder {
                // The core's order type; the board only reads its cards.
                column: StatusSet::AgentStage,
                cards: shown_cards,
            },
            total,
            visible,
        }
    }
}

/// "12", or "3/12" under a filter.
fn count_text(visible: usize, total: usize, filtered: bool) -> String {
    if filtered {
        format!("{visible}/{total}")
    } else {
        total.to_string()
    }
}

/// The agent that opens a project's PRs: the one its flow's delivery names, else its first Agent stage's
/// (`agents::resolve::delivery_agent`).
fn delivery_agent(flow: &Flow) -> Option<String> {
    flow.delivery
        .agent
        .clone()
        .or_else(|| flow.first_agent_stage().and_then(|s| s.agent.clone()))
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
        skippable_check: otaq_core::flow::engine::user_may_move_past(input.flow, &t.place),
        was_validating: input.was_validating.contains(&t.key),
        // Its flow's delivery (FLW-15): Ready to merge or Ready for PR, and who opens the PR.
        delivers: Some(input.flow.delivery.delivers),
        delivery_agent: delivery_agent(input.flow),
        custom_flow,
        paused: input.paused,
        // DPV-01: the blocked line of a ticket that hasn't started, and what waits for it.
        blocked: crate::view_models::dependencies::blocked_line(t, all),
        blocks: crate::view_models::dependencies::blocks(t, all),
        stage_name: crate::view_models::plan::stage_in(t, input.flow).map(str::to_owned),
        helpers: input
            .helpers
            .iter()
            .filter(|h| h.ticket == t.key)
            .cloned()
            .collect(),
        next_stage: next_stage(t, input.flow),
        working_on: match &t.place {
            otaq_core::domain::Place::InStage(id) => input
                .flow
                .stage(id)
                .and_then(|s| s.skill.as_deref())
                .and_then(crate::view_models::planning::working_on)
                .map(str::to_owned),
            _ => None,
        },
        check_line: crate::view_models::planning::planning_check(t, input.flow)
            .map(|_| crate::view_models::planning::check_line(t)),
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
    let mut extras = match input.agents {
        Some(agents) => extras.with_agents(agents, &t.key),
        None => extras,
    };
    // Where it starts, past the stages it skips (`s30-01`: "Dev next · skips planning").
    extras.skips = crate::view_models::planning::skips_line(
        input.flow,
        t,
        extras.agents.starts.as_ref().map(|s| s.stage.as_str()),
    );
    CardViewModel::from(t, &extras, input.now_ms)
}

/// The name of the stage after the one `t` is in ("4 pieces merged · moving to Validate").
fn next_stage(t: &Ticket, flow: &Flow) -> Option<String> {
    let otaq_core::domain::Place::InStage(id) = &t.place else {
        return None;
    };
    let at = flow.stages.iter().position(|s| &s.id == id)?;
    flow.stages.get(at + 1).map(|s| s.name.clone())
}

pub fn build(input: &BoardInput) -> Vec<ColumnView> {
    let all = input.tickets;
    let cards: Vec<&TicketKey> = all.iter().map(|t| &t.key).collect();
    let shown: Vec<&TicketKey> = match input.visible {
        Some(keys) => cards.iter().copied().filter(|k| keys.contains(k)).collect(),
        None => cards.clone(),
    };
    let filtered = input.visible.is_some();
    let entry = |key: &TicketKey| -> Option<CardEntry> {
        let t = all.iter().find(|t| &t.key == key)?;
        Some(CardEntry::Ticket(ticket_card(input, t)))
    };
    let entries = |keys: &[TicketKey]| keys.iter().filter_map(entry).collect::<Vec<_>>();
    let columns = Columns::of(input.flow);
    let quiet = !keeps_default_look(input.flow);
    columns
        .iter()
        .map(|column| {
            let id = column.id.as_str();
            let whole = StateCards::of(&columns, id, input, &cards, &shown);
            let (icon, empty_hint) = column_look(column, quiet);
            let mut view = ColumnView {
                id: ColumnId::new(id),
                kind: column.kind,
                title: column.title.clone(),
                icon,
                quiet_icon: quiet,
                count: count_text(whole.visible, whole.total, filtered),
                tooltip: header_tooltip(column, input.agent_list),
                total: whole.total,
                cards: Vec::new(),
                dev: None,
                hint: (column.kind == ColumnKind::ReadyForDev).then(|| {
                    crate::view_models::planning::ready_hint(input.flow, READY_FOR_DEV_HINT)
                }),
                empty_hint,
                collapsed: is_folded(id, column.kind, whole.total, input.folded, input.fold_empty),
                needs_you: needs_you_in(&columns, id, all, &cards),
                hidden: whole.total - whole.visible,
            };
            match column.kind {
                ColumnKind::Agent => {
                    view.dev = Some(dev_sections(&whole.order, all, input.dev_budget, &entries));
                }
                _ => {
                    view.cards = entries(&whole.order.cards);
                }
            }
            view
        })
        .collect()
}

/// The header's tooltip: the stage's line, its tool and model, its options.
pub fn header_tooltip(column: &FlowColumn, agents: Option<&AgentList>) -> Option<HeaderTooltip> {
    let stage = column.stage?;
    let named = |name: &str| -> Option<&Agent> {
        agents?
            .agents
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
    };
    let agent = |s: &Stage| -> Option<&Agent> { named(s.named_agents().next()?) };
    let mut detail: Vec<String> = Vec::new();
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
    if !stage.options.is_empty() {
        let options: Vec<&str> = stage.options.iter().map(StageOption::as_str).collect();
        detail.push(format!("Options: {}", options.join(", ")));
    }
    let mut title: Vec<String> = vec![stage.name.clone()];
    if stage.kind == (StageKind::Check { by: CheckBy::You }) {
        title.push("your check".to_owned());
    }
    if !stage.is_human_check() {
        title.extend(stage.has_agent().then(|| stage.agents_line()));
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
        .order(column, cards, all)
        .iter()
        .filter(|k| all.iter().any(|t| &t.key == *k && t.needs_user()))
        .count()
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
        let mut t20 = ticket("SHOP-20", "Checkout page", Status::Queued);
        t20.depends_on = vec!["SHOP-18".parse().unwrap()];
        vec![
            ticket("SHOP-24", "Update footer links", Status::Backlog),
            ticket("SHOP-40", "Wishlist", Status::Backlog),
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
            helpers: &[],
        }
    }

    /// A card offers Move on anyway only on a check its flow marks skippable: Security failed its rounds on the test
    /// flow opens the ticket; once the flow marks Security skippable the card's action is Move on anyway.
    #[test]
    fn move_on_anyway_only_on_a_check_the_flow_marks_skippable() {
        let mut flow = crate::view_models::flow_columns::every_kind();
        let mut t = ticket("NOTE-3", "Search", Status::CheckFailed);
        t.place = otaq_core::domain::Place::stage_of("security");
        let tickets = vec![t];
        let action = |flow: &otaq_core::flow::Flow| {
            let input = BoardInput {
                flow,
                ..input(&tickets, None)
            };
            ticket_card(&input, &tickets[0])
                .footer
                .and_then(|f| f.action)
                .map(|a| a.kind)
        };
        assert_eq!(
            action(&flow),
            Some(crate::view_models::card::CardActionKind::Open)
        );
        for stage in flow.stages.iter_mut().filter(|s| s.id == "security") {
            stage.options.push(otaq_core::flow::StageOption::Skip);
        }
        assert_eq!(
            action(&flow),
            Some(crate::view_models::card::CardActionKind::MoveOnAnyway)
        );
    }

    /// STG-08 on the test flow: every stage has its own column between Ready for Dev and Done; a card waiting between
    /// Product spec and Dev shows in Dev's column with its waiting status, a validating one in Validate, a Security check
    /// in Security, QA's card in QA, and a PR open card in QA too, the last stage (the delivery is no column).
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
            at("NOTE-1", Status::Working, Place::stage_of("product-spec")),
            at("NOTE-2", Status::Queued, Place::Waiting(Some("dev".into()))),
            at("NOTE-3", Status::Checking, Place::stage_of("validate")),
            at("NOTE-4", Status::Checking, Place::stage_of("security")),
            at("NOTE-5", Status::WaitingForYou, Place::stage_of("qa")),
            at("NOTE-6", Status::PrOpen, Place::stage_of("qa")),
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
                "product-spec",
                "dev",
                "validate",
                "security",
                "qa",
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
        assert_eq!(dev_keys("product-spec"), ["NOTE-1"]);
        assert_eq!(dev_keys("dev"), ["NOTE-2"], "the card waiting before Dev");
        assert_eq!(dev_keys("validate"), ["NOTE-3"]);
        assert_eq!(dev_keys("security"), ["NOTE-4"]);
        // QA's card, then the one being delivered from QA, the last stage.
        let qa = &columns[6];
        assert_eq!(keys(&qa.cards), ["NOTE-5", "NOTE-6"]);
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
        assert_eq!(
            vm.origin.as_deref(),
            Some("Created by Noyan \u{b7} Checkout slowness")
        );
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

    /// SHL-06, FLW-01: six columns (Validate has its own); Review counts both sections.
    #[test]
    fn six_columns_with_counts() {
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
        assert_eq!(keys(&columns[0].cards), ["SHOP-24", "SHOP-40"]);
        // Ready for Dev: Start interrupted first, then the start order (the one waiting for SHOP-18 last).
        assert_eq!(
            keys(&columns[1].cards),
            ["SHOP-28", "SHOP-21", "SHOP-29", "SHOP-20"]
        );
        assert_eq!(columns[1].hint, Some(READY_FOR_DEV_HINT));
        assert!(columns.iter().filter(|c| c.hint.is_some()).count() == 1);
        // Review lists its cards: no section, no Approved part.
        let review = &columns[4];
        assert_eq!(keys(&review.cards), ["SHOP-18"]);
    }

    /// §7.5 in Review: pinned first (most recently pinned first), then the waiting order, then the tickets being
    /// delivered (PR open); the column counts them all, under a filter too.
    #[test]
    fn review_keeps_its_order_and_counts_its_delivered_tickets() {
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
        let cards = keys(&review.cards);
        assert_eq!(cards[..3], ["SHOP-31", "SHOP-32", "SHOP-18"]);
        assert_eq!(cards.len(), 5);

        let visible: Vec<TicketKey> = vec!["SHOP-18".parse().unwrap(), "SHOP-26".parse().unwrap()];
        let filtered = build(&BoardInput {
            visible: Some(&visible),
            ..input(&tickets, None)
        });
        let review = &filtered[4];
        assert_eq!((review.count.as_str(), review.hidden), ("2/5", 3));
    }

    /// FLW-01 (T-0412's Done-when): the default board is Backlog · Ready for Dev · Dev · Validate · Review · Done; a
    /// validating card is in Validate, and Review has no Approved section. Every stage is its own column on any flow.
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
        assert_eq!(keys(&dev.working), ["SHOP-19"]);
        let validate = columns[3].dev.as_ref().expect("Validate's cards");
        assert_eq!(keys(&validate.working), ["SHOP-30"]);
        assert_eq!(columns[3].count, "1");
        assert_eq!(keys(&columns[4].cards), ["SHOP-18"]);
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

    /// BRD-02's one width rule: 240 px a column at least; columns that fit share the width, else the board scrolls
    /// and the pill counts the columns cut off at the right edge, fewer as it scrolls.
    #[test]
    fn one_width_rule_and_the_more_stages_pill() {
        // Seven columns at 240 with 6 gaps of 12 and 2 × 16 padding: 1784 px.
        assert!(fits(1784., 7, 0));
        assert!(!fits(1783., 7, 0));
        assert!(
            fits(1784. - 240. + 52., 6, 1),
            "a folded column is a 52 px rail"
        );
        let seven = [false; 7];
        assert_eq!(columns_after(1796., 0., &seven), 0);
        assert_eq!(columns_after(1600., 0., &seven), 1);
        assert_eq!(
            more_stages(columns_after(1600., 0., &seven)).as_deref(),
            Some("1 more stage")
        );
        // At 1440 beside the rail, seven stages already scroll.
        assert_eq!(columns_after(1376., 0., &seven), 2);
        let fourteen = [false; 14];
        assert_eq!(columns_after(1376., 0., &fourteen), 9);
        assert_eq!(columns_after(1376., 252. * 3., &fourteen), 6);
        assert_eq!(columns_after(1376., 252. * 14., &fourteen), 0);
        assert_eq!(more_stages(0), None);
        assert_eq!(more_stages(7).as_deref(), Some("7 more stages"));
    }

    /// Spec first's board (`s30-01`; d0129, d0093): the waiting column is "Ready" with "Before the first stage", and
    /// each planning card says what it is doing, as the board words it from the flow and the card's agents.
    #[test]
    fn spec_first_cards_say_what_planning_is_doing() {
        use crate::view_models::plan::fixtures::WISHLIST;
        use otaq_core::agents::facts::{CardAgents, CardStart};
        let file = otaq_core::flow::presets::preset(otaq_core::flow::presets::SPEC_FIRST_FILE)
            .unwrap()
            .read();
        let at = |key: &str, status, place: Place| {
            let mut t = ticket(key, "Work", status);
            t.place = place;
            t
        };
        let mut planned = at(
            "SHOP-40",
            Status::WaitingForYou,
            Place::stage_of("spec-review"),
        );
        planned.plan = WISHLIST.to_owned();
        let mut asks = at("SHOP-60", Status::AsksYou, Place::stage_of("product-spec"));
        asks.signal = Some(otaq_core::board::tickets::AgentSignal {
            text: Some("Who is the site for, first?".into()),
            ..Default::default()
        });
        let tickets = vec![
            at("SHOP-35", Status::Queued, Place::Waiting(None)),
            asks,
            at("SHOP-52", Status::Working, Place::stage_of("tech-spec")),
            at(
                "SHOP-55",
                Status::Working,
                Place::stage_of("milestone-plan"),
            ),
            planned,
            at(
                "SHOP-58",
                Status::WaitingForYou,
                Place::stage_of("spec-review"),
            ),
            at("SHOP-33", Status::Working, Place::stage_of("dev")),
            at(
                "SHOP-31",
                Status::WaitingForYou,
                Place::stage_of("code-review"),
            ),
        ];
        let card = |key: &str, starts: Option<&str>| CardAgents {
            key: key.parse().unwrap(),
            running_developer: None,
            running_validator: None,
            running_developer_on: None,
            running_validator_on: None,
            holds: Vec::new(),
            starts: starts.map(|stage| CardStart {
                stage: stage.into(),
                agent: "Developer".into(),
                model: None,
            }),
            refused_agent: None,
        };
        let agents = BoardAgents {
            agent_count: 5,
            cards: vec![card("SHOP-35", Some("Dev"))],
            connected: Vec::new(),
        };
        let input = BoardInput {
            flow: &file.flow,
            agents: Some(&agents),
            ..input(&tickets, None)
        };
        let line = |key: &str| {
            let t = tickets.iter().find(|t| t.key.to_string() == key).unwrap();
            ticket_card(&input, t)
                .one_line
                .line
                .map(|l| l.text)
                .unwrap_or_default()
        };
        assert_eq!(line("SHOP-35"), "Dev next \u{b7} skips planning");
        assert_eq!(line("SHOP-60"), "Asks you: who is the site for, first?");
        assert_eq!(line("SHOP-52"), "Working \u{b7} writing the architecture");
        assert_eq!(line("SHOP-55"), "Working \u{b7} writing the plan");
        assert_eq!(
            line("SHOP-40"),
            "Your check \u{b7} the decisions and the plan"
        );
        assert_eq!(line("SHOP-58"), "Your check \u{b7} the decisions");
        // The build stage and the last check read as before.
        assert_eq!(line("SHOP-33"), "Working");
        assert!(
            !line("SHOP-31").contains("decisions"),
            "{}",
            line("SHOP-31")
        );

        let columns = build(&input);
        assert_eq!(columns[1].title, "Ready");
        assert_eq!(
            columns[1].hint,
            Some(crate::view_models::planning::BEFORE_THE_FIRST_STAGE)
        );
        // Today's board keeps its name and hint.
        let today = build(&self::input(&tickets, None));
        assert_eq!(
            (today[1].title.as_str(), today[1].hint),
            ("Ready for Dev", Some(READY_FOR_DEV_HINT))
        );
    }

    #[test]
    fn a_filter_shows_shown_over_total() {
        let tickets = board();
        let visible: Vec<TicketKey> = vec!["SHOP-24".parse().unwrap(), "SHOP-22".parse().unwrap()];
        let columns = build(&BoardInput {
            visible: Some(&visible),
            ..input(&tickets, None)
        });
        assert_eq!(columns[0].count, "1/2");
        assert_eq!(columns[0].hidden, 1);
        assert_eq!(columns[2].count, "1/7");
        assert_eq!(keys(&columns[2].dev.as_ref().unwrap().top), ["SHOP-22"]);
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
                    agent("a2", "Checker", Some("gpt-5.5")),
                ),
            ],
            noyan_agent: None,
            default_model: None,
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
                title: "Validate · Checker · validate".into(),
                detail: vec!["Codex · gpt-5.5 · up to 3 rounds".into()],
            })
        );
        assert_eq!(
            tip(4),
            Some(HeaderTooltip {
                title: "Review · your check".into(),
                detail: vec![],
            })
        );
        assert_eq!(tip(5), None);
        assert_eq!(columns[1].hint, Some(READY_FOR_DEV_HINT));
        assert!(columns.iter().all(|c| !c.collapsed));
    }

    /// FLW-12 on the test flow: muted kind icons (bot, shield, person), every stage its own column, and the tooltips
    /// with agent, tool, model and options.
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
                agent("g", "Spec writer", Some("sonnet")),
                agent("r", "Rust developer", Some("sonnet")),
                agent("s", "Security reviewer", Some("opus")),
                agent("p", "Shipper", Some("haiku")),
            ],
            noyan_agent: None,
            default_model: None,
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
                ("Ready", IconName::Hourglass, true),
                ("Product spec", IconName::Bot, true),
                ("Dev", IconName::Bot, true),
                ("Validate", IconName::ShieldCheck, true),
                ("Security", IconName::ShieldCheck, true),
                ("QA", IconName::UserCheck, true),
                ("Done", IconName::CircleCheck, true),
            ]
        );
        let dev = &columns[3];
        assert_eq!(dev.count, "2", "Dev's own cards");
        assert_eq!(dev.total, 2);
        assert_eq!(
            dev.tooltip,
            Some(HeaderTooltip {
                title: "Dev · Rust developer · work-ticket".into(),
                detail: vec!["Claude Code · sonnet".into()],
            })
        );
        assert_eq!(columns[4].count, "1", "Validate's own column");
        let tip = |i: usize| columns[i].tooltip.clone().unwrap();
        assert_eq!(tip(2).title, "Product spec · Spec writer · product-spec");
        assert_eq!(tip(2).detail, ["Claude Code · sonnet", "Options: skip"]);
        assert_eq!(
            tip(5).title,
            "Security · Security reviewer · team-security-review"
        );
        // A check's one agent and where it runs, then the stage's rounds.
        assert_eq!(tip(5).detail, ["Claude Code · opus \u{b7} up to 2 rounds"]);
        assert_eq!(tip(6).title, "QA · your check");
        assert_eq!(tip(6).detail, ["Options: skip"]);

        let visible: Vec<TicketKey> = vec!["NOTE-1".parse().unwrap()];
        let filtered = build(&BoardInput {
            flow: &flow,
            visible: Some(&visible),
            ..input(&tickets, None)
        });
        assert_eq!(filtered[3].count, "1/2");
        assert_eq!(filtered[3].hidden, 1);
    }

    /// shop-web's ten columns (*Row · A flow with many stages*): Backlog · Ready for Dev · Product spec · Dev · Validate
    /// · Security · QA · Review · Docs · Done, delivered by a PR after Docs.
    fn ten_stages() -> Flow {
        let yaml = "\
flow:
  - {id: product-spec, name: Product spec, kind: agent, agent: Spec writer, skill: product-spec, options: [skip]}
  - {id: dev, name: Dev, kind: agent, agent: Rust developer, skill: work-ticket}
  - {id: validate, name: Validate, kind: check, by: agent, agent: Validator, skill: validate, rounds: 3}
  - {id: security, name: Security, kind: check, by: agent, agent: Security reviewer, skill: security-review, rounds: 3}
  - {id: qa, name: QA, kind: check, by: you}
  - {id: review, name: Review, kind: check, by: you}
  - {id: docs, name: Docs, kind: agent, agent: Doc writer, skill: docs}
delivery: {delivers: pr, mode: automatic, agent: Shipper, skill: open-pr}
";
        otaq_core::flow::yaml::read(yaml).unwrap().flow
    }

    /// BRD-02, a long flow (`s24-04`): every column keeps its 240 px, nothing folds by itself (Fold empty columns
    /// is off), Ready for Dev keeps its hint, and at 1440 the board scrolls; Fold empty columns folds Product spec (Docs
    /// holds the PR open card, delivered from the last stage).
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
            at("SHOP-26", Status::PrOpen, Place::stage_of("docs")),
        ];
        let columns = build(&BoardInput {
            flow: &flow,
            ..input(&tickets, None)
        });
        assert_eq!(columns.len(), 10);
        assert!(columns.iter().all(|c| !c.collapsed));
        // Product spec comes first: "Ready" under "Before the first stage" (d0093, Q515).
        assert_eq!(
            columns[1].hint,
            Some(crate::view_models::planning::BEFORE_THE_FIRST_STAGE)
        );
        let rails: Vec<bool> = columns.iter().map(|c| c.collapsed).collect();
        assert_eq!(
            more_stages(columns_after(1440. - 64., 0., &rails)).as_deref(),
            Some("5 more stages")
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
        assert_eq!(rails, ["Product spec", "Done"]);
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
        let CardEntry::Ticket(vm) = &free[1].cards[1];
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
}

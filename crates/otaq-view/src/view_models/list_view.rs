//! The List view (LST-01, LST-02; *24 · The two views*, `s24-03`, `s24-04`, `s24-05`): the project's tickets as rows
//! under the header TICKET · WHERE IN THE FLOW · WHAT'S HAPPENING · WHO. A row is the ticket's card in one line (its
//! key, title, the card's status line, who, time and its one action, from [`board::ticket_card`] so both views say
//! the same), its status icon and its track: one step per stage of the flow, the steps it passed filled, its own in
//! its state's colour. Rows are grouped by the project's Group by ([`GroupBy`]): **Stage** (flow order: Backlog with
//! "drag a row to Ready, or Start", Ready with "starts in priority order", each stage, Finished with "4 this week ·
//! show"), **Status** (Needs you · Working · Waiting to start · Backlog · Open PRs · Finished this week) or **Agent**;
//! Hide empty stages leaves empty groups out. Within a group the board's order holds (the core's, d0065 for Ready), so
//! answering a ticket changes its row's edge and words, not its place.
//!
//! Starting (LST-02): a Backlog row has **Start**, the drop on the first agent column that the card's ⌘D sends; with
//! Group by Stage each group that is a board drop target says so (`target`), so a row dragged onto Ready queues it with
//! the drop zone's words (the drag's verdicts are `board.drag`'s, as on the Board). The row menu is the card's
//! (`board.menu`).
//!
//! **Group by Epic** (EPL-02) draws T-0462's [`lanes::by_epic`]: one "Epics" group whose entries are each epic's parent
//! row ([`Parent`]: its track, "1 of 5 merged · Wishlist lead is re-planning", the lead, the fold) with its children's
//! rows nested under it, then "No epic" with the rest.

use otaq_core::board::needs_you::{group_of, waits_for_click};
use otaq_core::board::settings::ViewPrefs;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Kind, Place, Status, TicketKey};
use otaq_core::flow::{Delivers, Stage};
use otaq_core::project_agents::lead::Lead;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::theme::Palette;
use crate::view_models::board::{self, BoardInput};
use crate::view_models::board_page::DropTargetId;
use crate::view_models::card::Tone;
use crate::view_models::card_page::TicketCardView;
use crate::view_models::flow_columns::{self, ColumnKind, Columns};
use crate::view_models::lanes;
use crate::view_models::status::{self, PillView};
use crate::view_models::view_prefs::{Change, GroupBy};

/// The header row (`s24-03`).
pub const HEADER_TICKET: &str = "TICKET";
pub const HEADER_FLOW: &str = "WHERE IN THE FLOW";
pub const HEADER_HAPPENING: &str = "WHAT\u{2019}S HAPPENING";
pub const HEADER_WHO: &str = "WHO";

/// Group by Stage's fixed groups and their hints (LST-01; d0065: Ready's order is the Board's).
pub const BACKLOG: &str = "Backlog";
pub const BACKLOG_HINT: &str = "drag a row to Ready, or Start";
pub const READY: &str = "Ready";
pub const READY_HINT: &str = "starts in priority order";
/// A finished ticket's place on its track.
pub const DONE: &str = "Done";
/// The group of finished tickets, whatever the stages are named: the default flow's last stage is itself Done, so
/// the two never read alike (#53, Q468).
pub const FINISHED: &str = "Finished";
/// Finished's hint while its rows are folded, and while they show.
pub const DONE_SHOW: &str = "{n} this week \u{b7} show";
pub const DONE_HIDE: &str = "{n} this week \u{b7} hide";

/// Group by Status's groups (LST-01), in order.
pub const NEEDS_YOU: &str = "Needs you";
pub const WORKING: &str = "Working";
pub const WAITING_TO_START: &str = "Waiting to start";
pub const OPEN_PRS: &str = "Open PRs";
pub const FINISHED_THIS_WEEK: &str = "Finished this week";

/// Group by Agent's groups besides each agent: not started yet, and the stages you check.
pub const NOT_STARTED: &str = "Not started";
pub const YOU: &str = "You";

/// An empty group's line.
pub const NOTHING_HERE: &str = "Nothing here";

/// A Backlog row's button.
pub const START: &str = "Start";

/// The track's place before the flow (Backlog).
pub const NO_PLACE: &str = "\u{2014}";

/// How far back "this week" reaches: Done lists what finished in the last seven days.
pub const WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// The List as the page draws it. `ListView` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ListView")]
pub struct ListView {
    pub header: Header,
    pub groups: Vec<Group>,
}

/// The column names over the rows. `ListHeader` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ListHeader")]
pub struct Header {
    pub ticket: String,
    pub flow: String,
    pub happening: String,
    pub who: String,
}

/// One group of rows: its head ("Backlog 3 · drag a row to Ready, or Start") and its rows. `ListGroup` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ListGroup")]
pub struct Group {
    /// "backlog", "ready_for_dev", a stage's id, "done"; "needs_you"…; "agent:Rust developer".
    pub id: String,
    pub title: String,
    pub icon: IconName,
    /// The title's colour: Needs you's and Working's in Group by Status; `null` is the primary text.
    pub tone: Option<Tone>,
    /// "3", or "1/3" while a filter hides some.
    pub count: String,
    pub hint: Option<String>,
    /// Done's "show" / "hide": the change a click on the hint sends (Done's fold, shared with the Board's "Done 4 →").
    pub toggle: Option<Change>,
    /// Its rows are folded away (Done until "show").
    pub folded: bool,
    /// "Nothing here" when it has no row to show.
    pub empty: Option<String>,
    /// With Group by Stage, the board drop target the group stands for (LST-02): a row dropped on Ready queues it.
    /// `null` for a stage shown in another's column (Validate in Dev) and in the other groupings.
    pub target: Option<DropTargetId>,
    pub entries: Vec<Entry>,
}

impl Group {
    /// Its ticket rows, top to bottom: a parent's children follow it.
    pub fn rows(&self) -> impl Iterator<Item = &Row> {
        self.entries.iter().flat_map(|e| match e {
            Entry::Ticket { row } => std::slice::from_ref(row.as_ref()).iter(),
            Entry::Parent { parent } => parent.children.iter(),
        })
    }
}

/// A line of a group. `ListEntry` on the page. T-0462 adds the epic's parent row (with its children) for Group by
/// Epic (EPL-02).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "ListEntry")]
pub enum Entry {
    Ticket {
        row: Box<Row>,
    },
    /// Group by Epic: an epic's parent row and its children's rows.
    Parent {
        parent: Box<Parent>,
    },
}

/// An epic's parent row with its children (EPL-02): T-0462's [`lanes::ParentRow`] (key, External ID, name, "1 of 5
/// merged · Wishlist lead is re-planning", the lead, needs you), its track on the List's steps, and the fold (shared
/// with its lane's, `ViewPrefs::folded_lanes`). `ListParent` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ListParent")]
pub struct Parent {
    pub row: lanes::ParentRow,
    pub track: Track,
    pub folded: bool,
    pub fold: Change,
    /// "Fold" / "Unfold": the chevron's name.
    pub fold_label: String,
    /// Its children's rows in key order; none while folded.
    pub children: Vec<Row>,
}

/// A ticket's row (LST-01). `ListRow` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ListRow")]
pub struct Row {
    /// The card it stands for: key, epic, title, status line, who, time, the coloured edge and the one action.
    pub card: TicketCardView,
    /// The status icon before the key: the status pill's icon and colour.
    pub icon: IconName,
    pub palette: Palette,
    pub track: Track,
    /// Backlog rows' Start (LST-02).
    pub start: Option<RowStart>,
    pub needs_you: bool,
    /// An epic is focused and this isn't one of its tickets (Q55): the row dims.
    pub dim: bool,
}

/// Start on a Backlog row: the drop on `target` (the first agent column) that ⌘D sends from the Board.
/// `ListRowStart` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ListRowStart")]
pub struct RowStart {
    pub label: String,
    pub target: DropTargetId,
}

/// Where the ticket is in its flow: one step per stage, then the place's name ("Dev", "Ready", "—"). `ListTrack` on
/// the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ListTrack")]
pub struct Track {
    pub steps: Vec<Step>,
    pub label: String,
}

/// One stage on the track: passed (`approved`), its own in its state's colour, or ahead (`null`). `ListTrackStep` on
/// the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ListTrackStep")]
pub struct Step {
    /// The stage's name, for the step's tooltip.
    pub name: String,
    pub fill: Option<Palette>,
}

/// The track of `ticket` on `columns`' flow: Backlog and Ready have no step yet, Done passed them all.
pub fn track(columns: &Columns, ticket: &Ticket) -> Track {
    let flow = columns.flow();
    let at = columns
        .stage_of(ticket)
        .and_then(|stage| flow.stages.iter().position(|s| s.id == stage.id));
    let here = PillView::of(status::shown(ticket)).palette;
    let done = ticket.place == Place::Done;
    let steps = flow
        .stages
        .iter()
        .enumerate()
        .map(|(i, stage)| Step {
            name: stage.name.clone(),
            fill: match at {
                _ if done => Some(Palette::Approved),
                Some(at) if i < at => Some(Palette::Approved),
                Some(at) if i == at => Some(here),
                _ => None,
            },
        })
        .collect();
    let label = match &ticket.place {
        Place::Backlog => NO_PLACE.to_owned(),
        Place::Waiting(None) => READY.to_owned(),
        Place::Done => DONE.to_owned(),
        Place::Waiting(Some(_)) | Place::InStage(_) => at
            .map(|i| flow.stages[i].name.clone())
            .unwrap_or_else(|| NO_PLACE.to_owned()),
    };
    Track { steps, label }
}

/// A row before it is put in its group: whether the filter shows it.
struct Listed<'t> {
    ticket: &'t Ticket,
    row: Row,
    shown: bool,
}

/// A group before its words: what Group by Stage gives each of its groups.
struct Bucket<'t> {
    id: String,
    title: String,
    icon: IconName,
    tone: Option<Tone>,
    hint: Option<String>,
    target: Option<DropTargetId>,
    /// A stage of the flow (Hide empty stages leaves it out when empty); Backlog, Ready and Done always show.
    stage: bool,
    done: bool,
    rows: Vec<Listed<'t>>,
}

impl<'t> Bucket<'t> {
    fn new(id: impl Into<String>, title: impl Into<String>, icon: IconName) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            icon,
            tone: None,
            hint: None,
            target: None,
            stage: false,
            done: false,
            rows: Vec::new(),
        }
    }
}

/// The icon of a stage's group: what it is (an agent, an agent's check, your check, a delivery).
fn stage_icon(stage: &Stage) -> IconName {
    if stage.is_agent_check() {
        IconName::ShieldCheck
    } else if stage.is_human_check() {
        IconName::UserCheck
    } else if stage.is_delivery() {
        match stage.delivery.map(|d| d.delivers) {
            Some(Delivers::Pr) => IconName::GitPullRequest,
            _ => IconName::GitMerge,
        }
    } else {
        IconName::Bot
    }
}

/// The board drop target a stage stands for: its column when it has one, a section of a human check's column; none
/// for a stage folded into an agent column (Validate in Dev).
fn stage_target(columns: &Columns, stage: &Stage) -> Option<DropTargetId> {
    columns.iter().find_map(|c| {
        let index = c.parts.iter().position(|p| p.stage.id == stage.id)?;
        match index {
            0 => Some(DropTargetId::new(c.id.clone())),
            _ if c.has_sections() => Some(DropTargetId::new(c.parts[index].id.clone())),
            _ => None,
        }
    })
}

/// Whether the ticket waits for the user as the Needs you strip counts it (NYS-01, d0114): its question, a stop, your
/// check or a Delivery's click.
pub(crate) fn needs_you(ticket: &Ticket) -> bool {
    group_of(ticket.status).is_some() && waits_for_click(ticket) && !ticket.flags.your_turn
}

/// Whether `ticket` finished in the week before `now_ms`.
fn this_week(ticket: &Ticket, now_ms: i64) -> bool {
    now_ms - ticket.status_since.unwrap_or(ticket.updated_at) <= WEEK_MS
}

/// `t`'s row on `columns`' flow (LST-01): its card from [`board::ticket_card`], its status icon and track, Start
/// on a Backlog row (the drop on `start`, the first agent column) and whether it needs you. The epic sheet's Tickets
/// tab draws its children with it (SHT-07).
pub fn row(input: &BoardInput, columns: &Columns, start: Option<&DropTargetId>, t: &Ticket) -> Row {
    let card = TicketCardView::from_vm(board::ticket_card(input, t), t);
    let pill = PillView::of(status::shown(t));
    // A blocked row leads with the link, in the line's amber (DPV-01, `s24-10`).
    let (icon, palette) = match card
        .status
        .as_ref()
        .and_then(|s| s.icon.map(|i| (i, s.dot)))
    {
        Some(blocked) => blocked,
        None => (pill.icon, pill.palette),
    };
    Row {
        card,
        icon,
        palette,
        track: track(columns, t),
        start: start
            .filter(|_| t.place == Place::Backlog)
            .map(|target| RowStart {
                label: START.to_owned(),
                target: target.clone(),
            }),
        needs_you: needs_you(t),
        dim: input.strong.is_some_and(|strong| !strong.contains(&t.key)),
    }
}

/// The first agent column: where a Backlog row's Start drops it (⌘D's drop on the Board).
pub fn start_target(columns: &Columns) -> Option<DropTargetId> {
    columns
        .first(ColumnKind::Agent)
        .map(|c| DropTargetId::new(c.id.clone()))
}

/// The List for `input`'s project, grouped and folded by `prefs`.
pub fn view<'a>(input: &BoardInput<'a>, prefs: &ViewPrefs, leads: &[Lead]) -> ListView {
    let columns = Columns::of(input.flow);
    let all = input.tickets;
    let start = start_target(&columns);
    let listed = |t: &'_ Ticket| {
        t.kind != Kind::Epic && (t.place != Place::Done || this_week(t, input.now_ms))
    };
    let row = |t: &Ticket| row(input, &columns, start.as_ref(), t);
    let header = Header {
        ticket: HEADER_TICKET.to_owned(),
        flow: HEADER_FLOW.to_owned(),
        happening: HEADER_HAPPENING.to_owned(),
        who: HEADER_WHO.to_owned(),
    };
    let group_by: GroupBy = prefs.group_by.into();
    if group_by == GroupBy::Epic {
        return ListView {
            header,
            groups: by_epic(input, prefs, leads, &columns, &listed, &row),
        };
    }
    // Group by Stage, in flow order and in each group the board's order; the other groupings re-sort these rows.
    let keys: Vec<&TicketKey> = all.iter().filter(|t| listed(t)).map(|t| &t.key).collect();
    let bucket_of = |mut bucket: Bucket<'a>, column: &str, part: Option<&str>| -> Bucket<'a> {
        bucket.rows = columns
            .order(column, part, &keys, all)
            .iter()
            .filter_map(|k| all.iter().find(|t| &t.key == k))
            .map(|t| Listed {
                ticket: t,
                row: row(t),
                shown: input.visible.is_none_or(|v| v.contains(&t.key)),
            })
            .collect::<Vec<_>>();
        bucket
    };
    let mut stages: Vec<Bucket<'a>> = Vec::new();
    stages.push(bucket_of(
        Bucket {
            hint: Some(BACKLOG_HINT.to_owned()),
            target: Some(DropTargetId::BACKLOG),
            ..Bucket::new(flow_columns::BACKLOG, BACKLOG, IconName::CircleDashed)
        },
        flow_columns::BACKLOG,
        None,
    ));
    stages.push(bucket_of(
        Bucket {
            hint: Some(READY_HINT.to_owned()),
            target: Some(DropTargetId::READY_FOR_DEV),
            ..Bucket::new(flow_columns::READY_FOR_DEV, READY, IconName::Hourglass)
        },
        flow_columns::READY_FOR_DEV,
        None,
    ));
    for column in columns.iter() {
        for part in &column.parts {
            stages.push(bucket_of(
                Bucket {
                    target: stage_target(&columns, part.stage),
                    stage: true,
                    ..Bucket::new(
                        part.stage.id.clone(),
                        part.stage.name.clone(),
                        stage_icon(part.stage),
                    )
                },
                &column.id,
                Some(&part.id),
            ));
        }
    }
    stages.push(bucket_of(
        Bucket {
            target: Some(DropTargetId::DONE),
            done: true,
            ..Bucket::new(flow_columns::DONE, FINISHED, IconName::CircleCheck)
        },
        flow_columns::DONE,
        None,
    ));
    let buckets = match group_by {
        GroupBy::Stage | GroupBy::Epic => stages,
        GroupBy::Status => by_status(stages),
        GroupBy::Agent => by_agent(stages, &columns),
    };
    let done_folded = prefs.folded_columns.iter().any(|c| c == flow_columns::DONE);
    ListView {
        header,
        groups: buckets
            .into_iter()
            .filter(|b| {
                let empty = b.rows.is_empty();
                // Hide empty stages (LST-01): Group by Stage keeps Backlog, Ready and Done; the others drop any
                // empty group.
                !(prefs.hide_empty_stages && empty && (b.stage || group_by != GroupBy::Stage))
            })
            .map(|b| group(b, input.visible.is_some(), done_folded))
            .collect(),
    }
}

/// A bucket in words: its count, Done's fold and hint, the empty line.
fn group(bucket: Bucket<'_>, filtered: bool, done_folded: bool) -> Group {
    let total = bucket.rows.len();
    let shown: Vec<Row> = bucket
        .rows
        .into_iter()
        .filter(|l| l.shown)
        .map(|l| l.row)
        .collect();
    let count = if filtered {
        format!("{}/{total}", shown.len())
    } else {
        total.to_string()
    };
    let folded = bucket.done && done_folded && total > 0;
    let (hint, toggle) = match (bucket.done, total) {
        (true, 0) | (false, _) => (bucket.hint, None),
        (true, n) => (
            Some(if folded { DONE_SHOW } else { DONE_HIDE }.replace("{n}", &n.to_string())),
            Some(Change::FoldColumn {
                column: flow_columns::DONE.to_owned(),
                folded: !folded,
            }),
        ),
    };
    Group {
        id: bucket.id,
        title: bucket.title,
        icon: bucket.icon,
        tone: bucket.tone,
        count,
        hint,
        toggle,
        folded,
        empty: (shown.is_empty() && !folded).then(|| NOTHING_HERE.to_owned()),
        target: bucket.target,
        entries: if folded {
            Vec::new()
        } else {
            shown
                .into_iter()
                .map(|row| Entry::Ticket { row: Box::new(row) })
                .collect()
        },
    }
}

/// Group by Status (LST-01): Needs you · Working · Waiting to start · Backlog · Open PRs · Finished this week, each in
/// flow order.
fn by_status(stages: Vec<Bucket<'_>>) -> Vec<Bucket<'_>> {
    let mut groups = vec![
        Bucket {
            tone: Some(Tone::Attention),
            ..Bucket::new("needs_you", NEEDS_YOU, IconName::Hand)
        },
        Bucket {
            tone: Some(Tone::Working),
            ..Bucket::new("working", WORKING, IconName::LoaderCircle)
        },
        Bucket {
            tone: Some(Tone::Muted),
            ..Bucket::new("waiting", WAITING_TO_START, IconName::Hourglass)
        },
        Bucket {
            tone: Some(Tone::Muted),
            ..Bucket::new("backlog", BACKLOG, IconName::CircleDashed)
        },
        Bucket {
            tone: Some(Tone::Muted),
            ..Bucket::new("open_prs", OPEN_PRS, IconName::GitPullRequest)
        },
        Bucket {
            tone: Some(Tone::Muted),
            done: true,
            ..Bucket::new("done", FINISHED_THIS_WEEK, IconName::CircleCheck)
        },
    ];
    for listed in stages.into_iter().flat_map(|b| b.rows) {
        let t = listed.ticket;
        let at = match &t.place {
            Place::Done => 5,
            _ if needs_you(t) => 0,
            Place::Backlog => 3,
            Place::Waiting(_) => 2,
            _ if t.status == Status::PrOpen => 4,
            _ => 1,
        };
        groups[at].rows.push(listed);
    }
    groups
}

/// Group by Agent: Not started, then each agent in the flow's order (the one the row names, else its stage's), You for
/// the stages you check, Finished this week.
fn by_agent<'t>(stages: Vec<Bucket<'t>>, columns: &Columns) -> Vec<Bucket<'t>> {
    let mut not_started = Bucket::new("not_started", NOT_STARTED, IconName::CircleDashed);
    let mut agents: Vec<Bucket> = Vec::new();
    let mut you = Bucket::new("you", YOU, IconName::UserCheck);
    let mut done = Bucket {
        done: true,
        ..Bucket::new("done", FINISHED_THIS_WEEK, IconName::CircleCheck)
    };
    // Every agent of the flow has its group, in order, even before a ticket reaches it.
    for stage in &columns.flow().stages {
        if let Some(agent) = &stage.agent
            && !agents.iter().any(|b| &b.title == agent)
        {
            agents.push(Bucket::new(
                format!("agent:{agent}"),
                agent.clone(),
                IconName::Bot,
            ));
        }
    }
    for listed in stages.into_iter().flat_map(|b| b.rows) {
        let t = listed.ticket;
        let stage = columns.stage_of(t);
        match &t.place {
            Place::Done => done.rows.push(listed),
            Place::Backlog | Place::Waiting(None) => not_started.rows.push(listed),
            _ => match listed
                .row
                .card
                .who
                .clone()
                .or_else(|| stage.and_then(|s| s.agent.clone()))
            {
                Some(agent) => {
                    let at = match agents.iter().position(|b| b.title == agent) {
                        Some(at) => at,
                        None => {
                            agents.push(Bucket::new(
                                format!("agent:{agent}"),
                                agent.clone(),
                                IconName::Bot,
                            ));
                            agents.len() - 1
                        }
                    };
                    agents[at].rows.push(listed);
                }
                None => you.rows.push(listed),
            },
        }
    }
    std::iter::once(not_started)
        .chain(agents)
        .chain([you, done])
        .collect()
}

/// Group by Epic (EPL-02, T-0462's [`lanes::by_epic`]): "Epics 2" with each epic's parent row and, unless it is
/// folded, its children's rows; then "No epic" with the other tickets. Rows are those the other groupings list (no
/// epic ticket, Done only this week's).
fn by_epic(
    input: &BoardInput,
    prefs: &ViewPrefs,
    leads: &[Lead],
    columns: &Columns,
    listed: &dyn Fn(&Ticket) -> bool,
    row: &dyn Fn(&Ticket) -> Row,
) -> Vec<Group> {
    let by = lanes::by_epic(input.tickets, input.flow, leads, prefs, input.visible);
    let rows = |keys: &[String]| -> Vec<Row> {
        keys.iter()
            .filter_map(|k| input.tickets.iter().find(|t| t.key.to_string() == *k))
            .filter(|t| listed(t))
            .map(row)
            .collect()
    };
    let parents: Vec<Entry> = by
        .groups
        .into_iter()
        .map(|g| {
            let track = parent_track(columns, &g.parent);
            Entry::Parent {
                parent: Box::new(Parent {
                    children: if g.folded {
                        Vec::new()
                    } else {
                        rows(&g.children)
                    },
                    track,
                    row: g.parent,
                    folded: g.folded,
                    fold: g.fold,
                    fold_label: g.fold_label,
                }),
            }
        })
        .collect();
    let rest: Vec<Entry> = rows(&by.rest)
        .into_iter()
        .map(|row| Entry::Ticket { row: Box::new(row) })
        .collect();
    let group = |id: &str, title: String, icon, count: String, entries: Vec<Entry>| Group {
        id: id.to_owned(),
        title,
        icon,
        tone: None,
        count,
        hint: None,
        toggle: None,
        folded: false,
        empty: entries.is_empty().then(|| NOTHING_HERE.to_owned()),
        target: None,
        entries,
    };
    let rest_count = rest.len().to_string();
    [
        group(
            "epics",
            by.epics_title,
            IconName::Layers,
            by.epics_count,
            parents,
        ),
        group(
            "no_epic",
            by.rest_title,
            IconName::CircleDashed,
            rest_count,
            rest,
        ),
    ]
    .into_iter()
    .filter(|g| !(prefs.hide_empty_stages && g.entries.is_empty()))
    .collect()
}

/// A parent row's track on the List's steps (T-0462's [`lanes::RowTrack`] counts the board's columns): the stages
/// before the epic's column passed, its own in the working (or needs-you) colour.
fn parent_track(columns: &Columns, parent: &lanes::ParentRow) -> Track {
    let flow = columns.flow();
    let column = columns.iter().nth(parent.track.at);
    let at = column
        .and_then(|c| c.parts.first())
        .and_then(|p| flow.stages.iter().position(|s| s.id == p.stage.id));
    let done = column.is_some_and(|c| c.kind == ColumnKind::Done);
    let here = if parent.needs_you {
        Palette::Attn
    } else {
        Palette::Working
    };
    let steps = flow
        .stages
        .iter()
        .enumerate()
        .map(|(i, stage)| Step {
            name: stage.name.clone(),
            fill: match at {
                _ if done => Some(Palette::Approved),
                Some(at) if i < at => Some(Palette::Approved),
                Some(at) if i == at => Some(here),
                _ => None,
            },
        })
        .collect();
    let label = match (column.map(|c| c.kind), at) {
        (Some(ColumnKind::Backlog) | None, _) => NO_PLACE.to_owned(),
        (Some(ColumnKind::ReadyForDev), _) => READY.to_owned(),
        (Some(ColumnKind::Done), _) => DONE.to_owned(),
        (_, Some(i)) => flow.stages[i].name.clone(),
        (_, None) => parent.track.label.clone(),
    };
    Track { steps, label }
}

/// The List's states for the Gallery (`gallery.list_view`), on the example's shop-web: by Stage (Done folded, then
/// opened), by Status, by Agent, by Epic, and Hide empty stages on. `ListGallery` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ListGallery")]
pub struct Gallery {
    pub states: Vec<GalleryState>,
}

/// One drawn state. `ListGalleryState` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ListGalleryState")]
pub struct GalleryState {
    pub label: &'static str,
    pub view: ListView,
}

pub fn gallery() -> Gallery {
    use otaq_core::board::settings::GroupBy as G;
    use samples::{input, prefs, shop_web, tickets};
    let flow = shop_web();
    let mut tickets = tickets();
    // Wishlist's child at work, for Group by Epic.
    let mut child = crate::view_models::card_fixtures::ticket(
        "SHOP-40.3",
        "Wishlist page",
        otaq_core::domain::Status::Working,
    );
    child.place = Place::stage_of("dev");
    child.epic = Some("SHOP-40".parse().expect("fixture key"));
    tickets.push(child);
    let draw = |prefs: ViewPrefs| view(&input(&flow, &tickets), &prefs, &[]);
    Gallery {
        states: vec![
            GalleryState {
                label: "By stage, Done folded (s24-03)",
                view: draw(prefs(G::Stage)),
            },
            GalleryState {
                label: "By status (s24-04)",
                view: draw(prefs(G::Status)),
            },
            GalleryState {
                label: "By agent",
                view: draw(prefs(G::Agent)),
            },
            GalleryState {
                label: "By epic (s24-06)",
                view: draw(prefs(G::Epic)),
            },
            GalleryState {
                label: "By stage, Done shown, Hide empty stages",
                view: draw(ViewPrefs {
                    folded_columns: Vec::new(),
                    hide_empty_stages: true,
                    ..prefs(G::Stage)
                }),
            },
        ],
    }
}

/// The example's shop-web (`tickets/m38-tickets.md` → The example, as `s24-03` draws it), for the Gallery and the
/// tests.
pub mod samples {
    use otaq_core::board::settings::{BoardView, ViewPrefs};
    use otaq_core::board::tickets::{AgentSignal, Ticket};
    use otaq_core::domain::{Kind, Place, Priority, Status, ValidationRound};
    use otaq_core::flow::Flow;

    use crate::view_models::board::BoardInput;
    use crate::view_models::card_fixtures::{NOW, ticket};

    /// The example's shop-web flow (`s24-03`): Grill, Dev with Validate shown in it, Security, Review, PR.
    pub fn shop_web() -> Flow {
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

    fn at(key: &str, title: &str, status: Status, place: Place) -> Ticket {
        let mut t = ticket(key, title, status);
        t.place = place;
        t
    }

    fn asks(mut t: Ticket, text: &str) -> Ticket {
        t.signal = Some(AgentSignal {
            text: Some(text.into()),
            since: Some(NOW - 60_000),
            wakeup_at: None,
        });
        t
    }

    /// The example's shop-web tickets as the List draws them (`s24-03`).
    pub fn tickets() -> Vec<Ticket> {
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = Kind::Epic;
        let mut hotfix = at(
            "SHOP-31",
            "Hotfix: checkout crash",
            Status::Backlog,
            Place::Backlog,
        );
        hotfix.priority = Priority::Urgent;
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
        let working = |key: &str, title: &str, stage: &str, ago: i64| {
            let mut t = at(key, title, Status::Working, Place::stage_of(stage));
            t.flags.last_signal_at = Some(NOW - ago);
            t
        };
        let checking = |key: &str, title: &str, stage: &str, check: &str| {
            let mut t = at(key, title, Status::Checking, Place::stage_of(stage));
            t.flags.check = Some(check.into());
            t.flags.validating_round = Some(ValidationRound { round: 1, of: 3 });
            t
        };
        let mut failed = at(
            "SHOP-32",
            "Address autocomplete",
            Status::CheckFailed,
            Place::stage_of("security"),
        );
        failed.validation_round = 3;
        failed.flags.check = Some("Security".into());
        let mut pr = at(
            "SHOP-26",
            "Add sitemap",
            Status::PrOpen,
            Place::stage_of("pr"),
        );
        pr.flags.pr_number = Some(481);
        let mut tickets = vec![
            epic,
            hotfix,
            at(
                "SHOP-41",
                "Fix README typo",
                Status::Backlog,
                Place::Backlog,
            ),
            at(
                "SHOP-24",
                "Update footer links",
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
                    Place::stage_of("grill"),
                ),
                "Follow the system setting?",
            ),
            working("SHOP-44", "Saved carts", "grill", 60_000),
            asks(
                at(
                    "SHOP-22",
                    "Fix flaky login redirect",
                    Status::NeedsYou,
                    Place::stage_of("dev"),
                ),
                "Allow rm -rf .next && pnpm build?",
            ),
            working("SHOP-33", "Shipping estimate", "dev", 180_000),
            working("SHOP-29", "Coupon field", "dev", 480_000),
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
        for (i, key) in ["SHOP-1", "SHOP-2", "SHOP-3", "SHOP-4"]
            .into_iter()
            .enumerate()
        {
            let mut t = at(key, "Shipped", Status::Merged, Place::Done);
            t.status_since = Some(NOW - (i as i64 + 1) * 24 * 60 * 60 * 1000);
            tickets.push(t);
        }
        // Merged a month ago: not this week's.
        let mut old = at("SHOP-2", "Shipped long ago", Status::Merged, Place::Done);
        old.key = "SHOP-5".parse().unwrap();
        old.status_since = Some(NOW - 30 * 24 * 60 * 60 * 1000);
        tickets.push(old);
        tickets
    }

    pub fn input<'a>(flow: &'a Flow, tickets: &'a [Ticket]) -> BoardInput<'a> {
        BoardInput {
            flow,
            tickets,
            visible: None,
            strong: None,
            project_base: Some("main"),
            now_ms: NOW,
            dev_budget: None,
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

    pub fn prefs(group_by: otaq_core::board::settings::GroupBy) -> ViewPrefs {
        ViewPrefs {
            view: BoardView::List,
            group_by,
            ..ViewPrefs::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::samples::{input, prefs, shop_web, tickets};
    use super::*;
    use crate::view_models::card_fixtures::ticket;

    /// Each group's title, count and hint, then its rows' keys.
    fn outline(list: &ListView) -> Vec<(String, String, Option<String>, Vec<String>)> {
        list.groups
            .iter()
            .map(|g| {
                (
                    g.title.clone(),
                    g.count.clone(),
                    g.hint.clone(),
                    g.entries
                        .iter()
                        .flat_map(|e| match e {
                            Entry::Ticket { row } => vec![row.card.key.clone()],
                            Entry::Parent { parent } => {
                                std::iter::once(format!("{} >", parent.row.key))
                                    .chain(parent.children.iter().map(|r| r.card.key.clone()))
                                    .collect()
                            }
                        })
                        .collect(),
                )
            })
            .collect()
    }

    fn row<'l>(list: &'l ListView, key: &str) -> &'l Row {
        list.groups
            .iter()
            .flat_map(Group::rows)
            .find(|r| r.card.key == key)
            .unwrap_or_else(|| panic!("no row {key}"))
    }

    fn view(input: &BoardInput, prefs: &ViewPrefs) -> ListView {
        super::view(input, prefs, &[])
    }

    fn s(text: &str) -> Option<String> {
        Some(text.to_owned())
    }

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|k| (*k).to_owned()).collect()
    }

    /// LST-01 by Stage (`s24-03`): the header, Backlog to Done in flow order with each stage its own group, the
    /// counts, the hints, the board's order in each group, Done folded with "4 this week · show".
    #[test]
    fn the_example_by_stage() {
        use otaq_core::board::settings::GroupBy as G;
        let flow = shop_web();
        let tickets = tickets();
        let list = view(&input(&flow, &tickets), &prefs(G::Stage));
        assert_eq!(
            list.header,
            Header {
                ticket: "TICKET".into(),
                flow: "WHERE IN THE FLOW".into(),
                happening: "WHAT\u{2019}S HAPPENING".into(),
                who: "WHO".into(),
            }
        );
        assert_eq!(
            outline(&list),
            [
                (
                    "Backlog".into(),
                    "3".into(),
                    s("drag a row to Ready, or Start"),
                    keys(&["SHOP-24", "SHOP-31", "SHOP-41"])
                ),
                (
                    "Ready".into(),
                    "2".into(),
                    s("starts in priority order"),
                    keys(&["SHOP-35", "SHOP-20"])
                ),
                (
                    "Grill".into(),
                    "2".into(),
                    None,
                    keys(&["SHOP-12", "SHOP-44"])
                ),
                (
                    "Dev".into(),
                    "3".into(),
                    None,
                    keys(&["SHOP-22", "SHOP-29", "SHOP-33"])
                ),
                ("Validate".into(), "1".into(), None, keys(&["SHOP-30"])),
                (
                    "Security".into(),
                    "2".into(),
                    None,
                    keys(&["SHOP-32", "SHOP-19"])
                ),
                ("Review".into(), "1".into(), None, keys(&["SHOP-18"])),
                ("PR open".into(), "1".into(), None, keys(&["SHOP-26"])),
                (
                    "Finished".into(),
                    "4".into(),
                    s("4 this week \u{b7} show"),
                    keys(&[])
                ),
            ]
        );
        let done = list.groups.last().unwrap();
        assert!(done.folded && done.empty.is_none());
        assert_eq!(
            done.toggle,
            Some(Change::FoldColumn {
                column: "done".into(),
                folded: false
            })
        );
        // Drop targets (LST-02): Ready queues; Validate, shown in Dev's column on the Board, takes no drop.
        let target = |title: &str| {
            list.groups
                .iter()
                .find(|g| g.title == title)
                .unwrap()
                .target
                .as_ref()
                .map(|t| t.as_str().to_owned())
        };
        assert_eq!(target("Ready"), s("ready_for_dev"));
        assert_eq!(target("Dev"), s("dev"));
        assert_eq!(target("Validate"), None);
        assert_eq!(target("Review"), s("review"));
        assert_eq!(target("PR open"), s("pr"));

        // The rows say what the cards say (BRD-03).
        let line = |key: &str| {
            let r = row(&list, key);
            (
                r.card.status.as_ref().map(|l| l.text.clone()),
                r.card.action.as_ref().map(|a| a.label.to_owned()),
                r.card.edge,
            )
        };
        assert_eq!(
            line("SHOP-12"),
            (
                s("Asks you: follow the system setting?"),
                s("Reply"),
                Some(Tone::Attention)
            )
        );
        assert_eq!(line("SHOP-32").1, s("Decide"));
        assert_eq!(
            line("SHOP-18"),
            (s("Your check"), s("Review"), Some(Tone::Review))
        );
        assert_eq!(line("SHOP-30").0, s("Validating \u{b7} round 1 of 3"));
        assert_eq!(row(&list, "SHOP-33").card.time.as_ref().unwrap().text, "3m");
        assert_eq!(
            row(&list, "SHOP-12").icon,
            IconName::MessageCircleQuestionMark
        );
        assert_eq!(row(&list, "SHOP-12").palette, Palette::Attn);
        assert!(row(&list, "SHOP-12").needs_you && !row(&list, "SHOP-33").needs_you);

        // Done's "show" opens this week's four; the month-old one isn't listed.
        let open = view(
            &input(&flow, &tickets),
            &ViewPrefs {
                folded_columns: Vec::new(),
                ..prefs(G::Stage)
            },
        );
        let done = open.groups.last().unwrap();
        assert_eq!(done.hint, s("4 this week \u{b7} hide"));
        assert_eq!(done.entries.len(), 4);
        assert_eq!(
            done.toggle,
            Some(Change::FoldColumn {
                column: "done".into(),
                folded: true
            })
        );
    }

    /// #39: a row shows its ticket's priority chip, in Backlog and Ready alike; Normal shows none.
    #[test]
    fn rows_show_the_priority() {
        use otaq_core::board::settings::GroupBy as G;
        let flow = shop_web();
        let tickets = tickets();
        let list = view(&input(&flow, &tickets), &prefs(G::Stage));
        let chip = |key| {
            row(&list, key)
                .card
                .priority
                .as_ref()
                .map(|p| p.short.clone())
        };
        assert_eq!(chip("SHOP-31").as_deref(), Some("P1"));
        assert_eq!(chip("SHOP-35").as_deref(), Some("P1"));
        assert_eq!(chip("SHOP-20"), None);
    }

    /// #53 (Q468): with the default flow, whose last stage is named Done, the stage group says "Done" and the group
    /// of finished tickets says "Finished", so the two never read alike; by Status it is "Finished this week".
    #[test]
    fn the_finished_group_is_never_called_done() {
        use otaq_core::board::settings::GroupBy as G;
        let flow = otaq_core::flow::default::default_flow();
        let tickets = tickets();
        let titles = |group_by| -> Vec<String> {
            view(&input(&flow, &tickets), &prefs(group_by))
                .groups
                .iter()
                .map(|g| g.title.clone())
                .collect()
        };
        let by_stage = titles(G::Stage);
        assert_eq!(by_stage.iter().filter(|t| *t == "Done").count(), 1);
        assert_eq!(by_stage.last().map(String::as_str), Some(FINISHED));
        let by_status = titles(G::Status);
        assert_eq!(
            by_status.last().map(String::as_str),
            Some(FINISHED_THIS_WEEK)
        );
        assert!(!by_status.iter().any(|t| t.starts_with("Done")));
    }

    /// Hide empty stages leaves out an empty stage, never Backlog, Ready or Finished; a filter counts "1/3".
    #[test]
    fn hide_empty_stages_and_a_filter() {
        use otaq_core::board::settings::GroupBy as G;
        let flow = shop_web();
        let tickets: Vec<Ticket> = tickets()
            .into_iter()
            .filter(|t| !matches!(t.place, Place::InStage(_)) || t.key.to_string() == "SHOP-12")
            .collect();
        let shown: Vec<String> = view(
            &input(&flow, &tickets),
            &ViewPrefs {
                hide_empty_stages: true,
                ..prefs(G::Stage)
            },
        )
        .groups
        .iter()
        .map(|g| g.title.clone())
        .collect();
        assert_eq!(shown, ["Backlog", "Ready", "Grill", "Finished"]);
        let all = view(&input(&flow, &tickets), &prefs(G::Stage));
        let dev = all.groups.iter().find(|g| g.title == "Dev").unwrap();
        assert_eq!(
            (dev.count.as_str(), dev.empty.as_deref()),
            ("0", Some("Nothing here"))
        );

        let visible: Vec<TicketKey> = vec!["SHOP-41".parse().unwrap()];
        let filtered = view(
            &BoardInput {
                visible: Some(&visible),
                ..input(&flow, &tickets)
            },
            &prefs(G::Stage),
        );
        let backlog = &filtered.groups[0];
        assert_eq!(backlog.count, "1/3");
        assert_eq!(backlog.entries.len(), 1);
    }

    /// LST-01 by Status (`s24-04`): Needs you · Working · Waiting to start · Backlog · Open PRs · Finished this week, with
    /// their counts and colours.
    #[test]
    fn the_example_by_status() {
        use otaq_core::board::settings::GroupBy as G;
        let flow = shop_web();
        let tickets = tickets();
        let list = view(&input(&flow, &tickets), &prefs(G::Status));
        assert_eq!(
            outline(&list),
            [
                (
                    "Needs you".into(),
                    "4".into(),
                    None,
                    keys(&["SHOP-12", "SHOP-22", "SHOP-32", "SHOP-18"])
                ),
                (
                    "Working".into(),
                    "5".into(),
                    None,
                    keys(&["SHOP-44", "SHOP-29", "SHOP-33", "SHOP-30", "SHOP-19"])
                ),
                (
                    "Waiting to start".into(),
                    "2".into(),
                    None,
                    keys(&["SHOP-35", "SHOP-20"])
                ),
                (
                    "Backlog".into(),
                    "3".into(),
                    None,
                    keys(&["SHOP-24", "SHOP-31", "SHOP-41"])
                ),
                ("Open PRs".into(), "1".into(), None, keys(&["SHOP-26"])),
                (
                    "Finished this week".into(),
                    "4".into(),
                    s("4 this week \u{b7} show"),
                    keys(&[])
                ),
            ]
        );
        assert_eq!(list.groups[0].tone, Some(Tone::Attention));
        assert_eq!(list.groups[1].tone, Some(Tone::Working));
        assert_eq!(list.groups[2].tone, Some(Tone::Muted));
        // Only Group by Stage's groups are drop targets.
        assert!(list.groups.iter().all(|g| g.target.is_none()));
    }

    /// Group by Agent: Not started, each agent of the flow in order, You, Finished this week.
    #[test]
    fn by_agent_follows_the_flow() {
        use otaq_core::board::settings::GroupBy as G;
        let flow = shop_web();
        let tickets = tickets();
        let list = view(&input(&flow, &tickets), &prefs(G::Agent));
        let groups: Vec<(String, String)> = list
            .groups
            .iter()
            .map(|g| (g.title.clone(), g.count.clone()))
            .collect();
        assert_eq!(
            groups,
            [
                ("Not started".into(), "5".into()),
                ("Griller".into(), "2".into()),
                ("Rust developer".into(), "3".into()),
                ("Validator".into(), "1".into()),
                ("Security reviewer".into(), "2".into()),
                ("Shipper".into(), "1".into()),
                ("You".into(), "1".into()),
                ("Finished this week".into(), "4".into()),
            ]
        );
    }

    /// A row's track (LST-01): one step per stage, the passed ones filled, its own in its state's colour, then its
    /// place; Backlog "—", Ready "Ready", Done all passed.
    #[test]
    fn a_rows_track() {
        use otaq_core::board::settings::GroupBy as G;
        let flow = shop_web();
        let tickets = tickets();
        let list = view(
            &input(&flow, &tickets),
            &ViewPrefs {
                folded_columns: Vec::new(),
                ..prefs(G::Stage)
            },
        );
        let fills = |key: &str| -> (Vec<Option<Palette>>, String) {
            let t = &row(&list, key).track;
            (t.steps.iter().map(|s| s.fill).collect(), t.label.clone())
        };
        let a = Some(Palette::Approved);
        assert_eq!(
            fills("SHOP-22"),
            (
                vec![a, Some(Palette::Attn), None, None, None, None],
                "Dev".into()
            )
        );
        assert_eq!(
            fills("SHOP-30"),
            (
                vec![a, a, Some(Palette::Working), None, None, None],
                "Validate".into()
            )
        );
        assert_eq!(fills("SHOP-32").0[3], Some(Palette::Attn));
        assert_eq!(fills("SHOP-18").0[4], Some(Palette::Review));
        assert_eq!(fills("SHOP-31"), (vec![None; 6], "\u{2014}".into()));
        assert_eq!(fills("SHOP-35"), (vec![None; 6], "Ready".into()));
        assert_eq!(fills("SHOP-1"), (vec![a; 6], "Done".into()));
        let names: Vec<&str> = row(&list, "SHOP-22")
            .track
            .steps
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["Grill", "Dev", "Validate", "Security", "Review", "PR open"]
        );
    }

    /// LST-02: Backlog rows have Start, the drop on the first agent column that ⌘D sends; other rows don't.
    #[test]
    fn a_blocked_row_leads_with_the_link_and_a_blocker_says_blocks_n() {
        use otaq_core::board::settings::GroupBy as G;
        let flow = shop_web();
        let tickets = tickets();
        let list = view(&input(&flow, &tickets), &prefs(G::Stage));
        // DPV-01 (`s24-10`): SHOP-20 waits for SHOP-18, in amber with the link; SHOP-18 "blocks 1".
        let blocked = row(&list, "SHOP-20");
        assert_eq!(
            (blocked.icon, blocked.palette),
            (IconName::Link, Palette::Paused)
        );
        let line = blocked.card.status.as_ref().unwrap();
        assert_eq!(
            (line.text.as_str(), line.icon),
            (
                "Blocked \u{b7} waits for SHOP-18 to merge",
                Some(IconName::Link)
            )
        );
        let blocker = row(&list, "SHOP-18");
        assert_eq!(blocker.card.blocks.as_ref().unwrap().tag, "blocks 1");
        assert_eq!(blocker.card.blocks.as_ref().unwrap().line, "Blocks SHOP-20");
        assert_ne!(blocker.icon, IconName::Link);
    }

    #[test]
    fn backlog_rows_start() {
        use otaq_core::board::settings::GroupBy as G;
        let flow = shop_web();
        let tickets = tickets();
        let list = view(&input(&flow, &tickets), &prefs(G::Stage));
        assert_eq!(
            row(&list, "SHOP-41").start,
            Some(RowStart {
                label: "Start".into(),
                target: DropTargetId::new("grill"),
            })
        );
        assert_eq!(row(&list, "SHOP-35").start, None);
        assert_eq!(row(&list, "SHOP-12").start, None);
        // On the default flow it is Dev, as ⌘D's drop.
        let default = otaq_core::flow::default_flow();
        let mut backlog = ticket("NOTE-1", "Sort notes", Status::Backlog);
        backlog.place = Place::Backlog;
        let one = [backlog];
        let list = view(&input(&default, &one), &prefs(G::Stage));
        assert_eq!(
            row(&list, "NOTE-1").start.as_ref().unwrap().target,
            DropTargetId::DEV
        );
    }

    /// EPL-02 in the List (T-0462's `by_epic`): "Epics" with Wishlist's parent row (its line, its track) and its child
    /// nested, then "No epic" with the rest; a folded parent hides its children.
    #[test]
    fn by_epic_nests_the_children_under_their_parent() {
        use otaq_core::board::settings::GroupBy as G;
        let flow = shop_web();
        let mut tickets = tickets();
        let mut child = ticket("SHOP-40.3", "Wishlist page", Status::Working);
        child.place = Place::stage_of("dev");
        child.epic = Some("SHOP-40".parse().unwrap());
        tickets.push(child);
        // Checkout slowness: an epic the Project agent split (d0091), its child waiting in Backlog.
        let mut slowness = ticket("SHOP-50", "Checkout slowness", Status::Backlog);
        slowness.kind = Kind::Epic;
        slowness.created_by = otaq_core::domain::Actor::ProjectAgent("pa-1".into());
        tickets.push(slowness);
        let mut index = ticket("SHOP-50.1", "Add an index on orders", Status::Backlog);
        index.place = Place::Backlog;
        index.epic = Some("SHOP-50".parse().unwrap());
        tickets.push(index);
        let list = view(&input(&flow, &tickets), &prefs(G::Epic));
        let groups = outline(&list);
        assert_eq!((groups[0].0.as_str(), groups[0].1.as_str()), ("Epics", "2"));
        assert!(
            groups[0].3.starts_with(&keys(&["SHOP-40 >", "SHOP-40.3"])),
            "{:?}",
            groups[0].3
        );
        assert!(
            groups[0].3.ends_with(&keys(&["SHOP-50 >", "SHOP-50.1"])),
            "{:?}",
            groups[0].3
        );
        let slowness = list.groups[0]
            .entries
            .iter()
            .find_map(|e| match e {
                Entry::Parent { parent } if parent.row.key == "SHOP-50" => Some(parent),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            slowness.row.line,
            "0 of 1 merged \u{b7} from the Project agent"
        );
        assert_eq!(slowness.track.label, "\u{2014}");
        assert_eq!(slowness.children[0].start.as_ref().unwrap().label, "Start");
        assert_eq!(groups[1].0, "No epic");
        assert!(!groups[1].3.contains(&"SHOP-40.3".to_owned()));
        assert!(groups[1].3.contains(&"SHOP-12".to_owned()));
        let Entry::Parent { parent } = &list.groups[0].entries[0] else {
            panic!("a parent row")
        };
        assert!(
            parent.row.line.starts_with("0 of 1 merged"),
            "{}",
            parent.row.line
        );
        assert_eq!(parent.track.label, "Dev");
        assert_eq!(parent.track.steps[0].fill, Some(Palette::Approved));
        assert_eq!(parent.track.steps[1].fill, Some(Palette::Working));
        assert_eq!(
            parent.fold,
            Change::FoldLane {
                epic: "SHOP-40".into(),
                folded: true
            }
        );
        let folded = view(
            &input(&flow, &tickets),
            &ViewPrefs {
                folded_lanes: vec!["SHOP-40".into()],
                ..prefs(G::Epic)
            },
        );
        let Entry::Parent { parent } = &folded.groups[0].entries[0] else {
            panic!("a parent row")
        };
        assert!(parent.folded && parent.children.is_empty());
    }
}

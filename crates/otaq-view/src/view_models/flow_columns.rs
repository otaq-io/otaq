//! The board's columns from the project's flow (STG-08, d0108; d0004, d0087): Backlog, one column per stage (its id
//! the stage's id, its title the stage's name), and Done, the fixed ends of every flow. Every stage is its own column:
//! the delivery is a setting of the flow, not a stage, and a ticket being delivered (To deliver, PR open) stays in
//! the last stage's column (Review on the default flow), with no section of its own. On the default flow these are
//! today's columns with today's ids, so the page and its stored state (folds, scroll places) read as before.
//!
//! Where a card shows comes from its place: Backlog, Done, or the column of the stage it is in or waits before. A
//! started ticket that can't begin yet waits at the top of the first stage's column and says why (a dependency, a free
//! agent, a flow change); a card waiting between Product spec and Dev shows in Dev's column the same way. A card in a stage its flow no longer has (Validation turned off while it
//! validated) shows where its status's kind does, as the core's drag rules see it.

use otaq_core::board::moves::{DropOn, DropTarget};
use otaq_core::board::order::sort_column;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Place, Status, StatusSet, TicketKey};
use otaq_core::flow::engine::{Target, status_set};
use otaq_core::flow::{CheckBy, Flow, Stage, StageKind};
use serde::Serialize;
use ts_rs::TS;

/// The ids of the columns every flow has.
pub const BACKLOG: &str = "backlog";
pub const DONE: &str = "done";

/// What a column holds, from the kind of its stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "BoardColumnKind")]
pub enum ColumnKind {
    Backlog,
    /// An Agent stage or an agent check: agents work there (today's Dev).
    Agent,
    /// A human check (today's Review).
    HumanCheck,
    Done,
}

impl ColumnKind {
    fn of(kind: StageKind) -> Self {
        match kind {
            StageKind::Agent | StageKind::Check { by: CheckBy::Agent } => Self::Agent,
            StageKind::Check { by: CheckBy::You } => Self::HumanCheck,
        }
    }
}

/// One column of the board.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowColumn<'f> {
    pub id: String,
    pub kind: ColumnKind,
    /// "Backlog", the stage's name, "Done".
    pub title: String,
    /// The stage it shows; `None` for Backlog and Done.
    pub stage: Option<&'f Stage>,
}

/// A flow's columns, left to right.
#[derive(Debug, Clone, PartialEq)]
pub struct Columns<'f> {
    flow: &'f Flow,
    columns: Vec<FlowColumn<'f>>,
    /// Where started tickets waiting to begin start, by the stage's name, past the stages they skip
    /// ([`Columns::with_starts`]).
    starts: Vec<(TicketKey, String)>,
}

impl<'f> Columns<'f> {
    pub fn of(flow: &'f Flow) -> Self {
        let fixed = |id: &str, kind, title: &str| FlowColumn {
            id: id.to_owned(),
            kind,
            title: title.to_owned(),
            stage: None,
        };
        let mut columns = vec![fixed(BACKLOG, ColumnKind::Backlog, "Backlog")];
        columns.extend(flow.stages.iter().map(|stage| FlowColumn {
            id: stage.id.clone(),
            kind: ColumnKind::of(stage.kind),
            title: stage.name.clone(),
            stage: Some(stage),
        }));
        columns.push(fixed(DONE, ColumnKind::Done, "Done"));
        Self {
            flow,
            columns,
            starts: Vec::new(),
        }
    }

    /// These columns knowing where each waiting ticket starts (the stage's name, past the stages it skips). A started
    /// ticket that can't begin waits at the top of the first stage's column only while it is queued, its line saying
    /// where it starts ("Dev next · skips planning"); held, or waiting for an agent of the stage it starts at, it
    /// shows in that stage's column, never in one it skips.
    pub fn with_starts<'k>(
        mut self,
        starts: impl IntoIterator<Item = (&'k TicketKey, &'k str)>,
    ) -> Self {
        self.starts = starts
            .into_iter()
            .map(|(key, stage)| (key.clone(), stage.to_owned()))
            .collect();
        self
    }

    pub fn flow(&self) -> &'f Flow {
        self.flow
    }

    pub fn iter(&self) -> impl Iterator<Item = &FlowColumn<'f>> {
        self.columns.iter()
    }

    pub fn get(&self, id: &str) -> Option<&FlowColumn<'f>> {
        self.columns.iter().find(|c| c.id == id)
    }

    /// The first column of this kind (the first agent column is where ⌘D's "start" drops).
    pub fn first(&self, kind: ColumnKind) -> Option<&FlowColumn<'f>> {
        self.columns.iter().find(|c| c.kind == kind)
    }

    /// Every drop target, in board order: every column.
    pub fn targets(&self) -> Vec<String> {
        self.columns.iter().map(|c| c.id.clone()).collect()
    }

    /// The stage a card in `place` with its status shows as: the stage it is in or waits before (a started ticket
    /// waiting to begin, the first Agent stage); for a stage the flow doesn't have, the one its status's kind shows in
    /// (as the core's drag rules see it), a ticket being delivered in the last stage.
    pub fn stage_of(&self, ticket: &Ticket) -> Option<&'f Stage> {
        if ticket.place == Place::Waiting(None)
            && ticket.status != Status::Queued
            && let Some((_, name)) = self.starts.iter().find(|(key, _)| key == &ticket.key)
            && let Some(stage) = self.flow.stages.iter().find(|s| &s.name == name)
        {
            return Some(stage);
        }
        ticket.shown_stage(self.flow)
    }

    /// The column `ticket`'s card shows in: its id.
    pub fn spot(&self, ticket: &Ticket) -> &str {
        match &ticket.place {
            Place::Backlog => BACKLOG,
            Place::Done => DONE,
            Place::Waiting(_) | Place::InStage(_) => self
                .stage_of(ticket)
                .and_then(|stage| self.get(&stage.id))
                .map_or(BACKLOG, |c| c.id.as_str()),
        }
    }

    /// The column `ticket`'s card shows in.
    pub fn column_of(&self, ticket: &Ticket) -> &FlowColumn<'f> {
        let id = self.spot(ticket);
        self.columns
            .iter()
            .find(|c| c.id == id)
            .unwrap_or(&self.columns[0])
    }

    /// The core's drop target a page target stands for (FLW-18): Backlog and Done as themselves, a
    /// stage's column as the board target that means that stage in the flow (the first Agent stage, the last check
    /// by you), else the stage itself. Every stage takes a drop; `None` only for an id the board doesn't have.
    pub fn drop_target(&self, id: &str) -> Option<DropOn> {
        match id {
            BACKLOG => return Some(DropTarget::Backlog.into()),
            DONE => return Some(DropTarget::Done.into()),
            _ => {}
        }
        let stage = self.get(id)?.stage?;
        Some(match stage_target(self.flow, stage) {
            Some(target) => target.into(),
            None => DropOn::stage(&stage.id),
        })
    }

    /// The page target a core drop target is on this board (`None` when the flow has no such stage).
    pub fn page_target(&self, target: DropTarget) -> Option<String> {
        let target = DropOn::from(target);
        self.targets()
            .into_iter()
            .find(|id| self.drop_target(id).as_ref() == Some(&target))
    }

    /// The cards of `column` among `cards`, in board order (SDD §7.5): each status set's order from the core, in the
    /// order an agent column reads (cards waiting to begin its stage at the top, then working, then checks); a human
    /// check's column has its tickets being delivered after the ones waiting for you.
    pub fn order(&self, column: &str, cards: &[&TicketKey], all: &[Ticket]) -> Vec<TicketKey> {
        let members: Vec<&Ticket> = all
            .iter()
            .filter(|t| cards.contains(&&t.key))
            .filter(|t| self.spot(t) == column)
            .collect();
        let mut keys: Vec<TicketKey> = Vec::with_capacity(members.len());
        for set in SET_ORDER {
            if !members.iter().any(|t| t.status.set() == set) {
                continue;
            }
            for key in sort_column(set, all).cards {
                if members.iter().any(|t| t.key == key) && !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        keys
    }
}

/// Whether `flow` keeps the default flow's shape (its stages' ids and kinds, in order; FLW-05's "shape"), with or
/// without some of them (a flow without Validate): its board keeps today's look (d0108: "a project that keeps the
/// default flow looks exactly like today's board"), the column icons in their status colours. Any other flow shows
/// each stage's kind as one muted icon (FLW-12). A new agent, model or delivery is no shape change.
pub fn keeps_default_look(flow: &Flow) -> bool {
    let shape = |s: &Stage| (s.id.clone(), s.kind);
    let default = otaq_core::flow::default_flow();
    let mut left = default.stages.iter().map(shape);
    flow.stages
        .iter()
        .map(shape)
        .all(|stage| left.any(|d| d == stage))
}

/// The kind of place `ticket` is in, as the set of statuses it allows (STG-04): Backlog, waiting, Done, the
/// delivery for a ticket being delivered, or the kind of its stage. The stage is looked up in the default flow,
/// whose ids every project's flow uses in M31 (the derived flow only drops Validate); a stage it doesn't know goes
/// by the status's own set. Cards and sheets decide by it rather than by a column (STG-08).
pub fn place_set(ticket: &Ticket) -> StatusSet {
    match &ticket.place {
        Place::Backlog => StatusSet::Backlog,
        Place::Waiting(_) => StatusSet::Waiting,
        Place::Done => StatusSet::Done,
        Place::InStage(_) if ticket.status.set() == StatusSet::Delivery => StatusSet::Delivery,
        Place::InStage(id) => otaq_core::flow::default_file()
            .flow
            .stage(id)
            .map_or_else(|| ticket.status.set(), |s| status_set(s.kind)),
    }
}

/// The order of status sets inside one column: a ticket that waits to begin a stage is at the top of its column, then
/// the column's own sets (Review its tickets being delivered after the ones waiting for you).
const SET_ORDER: [StatusSet; 7] = [
    StatusSet::Waiting,
    StatusSet::Backlog,
    StatusSet::AgentStage,
    StatusSet::AgentCheck,
    StatusSet::HumanCheck,
    StatusSet::Delivery,
    StatusSet::Done,
];

/// The core target that means `stage` in `flow`, if any.
fn stage_target(flow: &Flow, stage: &Stage) -> Option<DropTarget> {
    DropTarget::ALL
        .iter()
        .copied()
        .find(|t| t.in_flow(flow) == Some(Target::Stage(stage)))
}

/// The core's test flow (`tests/flows/every-kind.yaml` of the core crate): every kind of stage.
#[cfg(test)]
pub fn every_kind() -> Flow {
    let core = format!("{}-core", otaq_core::naming::COMMAND_NAME);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(core)
        .join("tests/flows/every-kind.yaml");
    otaq_core::flow::yaml::read(&std::fs::read_to_string(path).unwrap())
        .unwrap()
        .flow
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::domain::Status;
    use otaq_core::flow::default_flow;

    fn ids(columns: &Columns) -> Vec<String> {
        columns.iter().map(|c| c.id.clone()).collect()
    }

    /// STG-08, FLW-01: the default flow's five columns, Backlog and Done at the ends, Validate's own, and no Approved
    /// section: a ticket being delivered is in Review's column. A started ticket waiting to begin is in Dev's.
    #[test]
    fn the_default_flow_gives_todays_columns() {
        let flow = default_flow();
        let columns = Columns::of(&flow);
        // FLW-01: Validate has its own column between Dev and Review.
        assert_eq!(
            ids(&columns),
            ["backlog", "dev", "validate", "review", "done"]
        );
        let titles: Vec<&str> = columns.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["Backlog", "Dev", "Validate", "Review", "Done"]);
        for status in [Status::Queued, Status::WaitingForSlot, Status::Held] {
            let waiting = ticket("SHOP-20", "Checkout page", status);
            assert_eq!(columns.spot(&waiting), "dev", "{status}");
        }
        assert_eq!(columns.targets(), ids(&columns), "no section is a target");
        for (id, target) in [
            ("backlog", DropTarget::Backlog),
            ("dev", DropTarget::Dev),
            ("review", DropTarget::ReadyForReview),
            ("done", DropTarget::Done),
        ] {
            assert_eq!(columns.drop_target(id), Some(target.into()), "{id}");
            assert_eq!(columns.page_target(target).as_deref(), Some(id));
        }
        // Every stage takes a drop (FLW-18): Validate by its id.
        assert_eq!(
            columns.drop_target("validate"),
            Some(DropOn::stage("validate"))
        );
        assert_eq!(columns.drop_target("nowhere"), None);
        for status in [Status::ToDeliver, Status::PrOpen, Status::MergedInto] {
            let mut t = ticket("SHOP-18", "Checkout", status);
            t.place = Place::stage_of("review");
            assert_eq!(columns.spot(&t), "review", "{status}");
            // An older row still in the Delivery stage shows there too.
            t.place = Place::stage_of("deliver");
            assert_eq!(columns.spot(&t), "review", "{status}");
        }

        // A flow without Validate has no Validate column (d0108 changelog 2026-10-20).
        let mut off = default_flow();
        off.stages.retain(|s| s.id != "validate");
        assert_eq!(
            ids(&Columns::of(&off)),
            ["backlog", "dev", "review", "done"]
        );
    }

    /// STG-08 on the test flow: every stage a column of its own, Validate too; a started ticket waiting to begin shows
    /// in Product spec's column, a card waiting between Product spec and Dev in Dev's.
    #[test]
    fn the_test_flow_gives_a_column_per_stage() {
        let flow = every_kind();
        let columns = Columns::of(&flow);
        assert_eq!(
            ids(&columns),
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
        let kinds: Vec<ColumnKind> = columns.iter().map(|c| c.kind).collect();
        use ColumnKind as K;
        assert_eq!(
            kinds,
            [
                K::Backlog,
                K::Agent,
                K::Agent,
                K::Agent,
                K::Agent,
                K::HumanCheck,
                K::Done
            ]
        );
        let titles: Vec<&str> = columns.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Backlog",
                "Product spec",
                "Dev",
                "Validate",
                "Security",
                "QA",
                "Done"
            ]
        );

        let mut waiting = ticket("NOTE-2", "Export as CSV", Status::Queued);
        waiting.place = Place::Waiting(Some("dev".into()));
        assert_eq!(columns.spot(&waiting), "dev");
        let mut validating = ticket("NOTE-4", "Search notes", Status::Checking);
        validating.place = Place::stage_of("validate");
        assert_eq!(columns.spot(&validating), "validate");
        let mut checking = ticket("NOTE-5", "Security", Status::Checking);
        checking.place = Place::stage_of("security");
        assert_eq!(columns.spot(&checking), "security");
        let ready = ticket("NOTE-6", "Ready", Status::Queued);
        assert_eq!(columns.spot(&ready), "product-spec");
        let mut pr = ticket("NOTE-7", "Sync", Status::PrOpen);
        pr.place = Place::stage_of("qa");
        assert_eq!(columns.spot(&pr), "qa");
    }

    /// An older flow file that put a stage in the previous column (`column: with-previous`) gives that stage its own
    /// column now.
    #[test]
    fn an_old_with_previous_stage_has_its_own_column() {
        let core = format!("{}-core", otaq_core::naming::COMMAND_NAME);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(core)
            .join("tests/flows/several-check-agents.yaml");
        let file = otaq_core::flow::yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
        let columns = Columns::of(&file.flow);
        assert!(ids(&columns).contains(&"validate".to_owned()));
        assert_eq!(columns.iter().count(), file.flow.stages.len() + 2);
    }

    /// A card in a stage the flow doesn't have shows where its kind does: Validation turned off mid-check.
    #[test]
    fn a_card_in_a_removed_stage_shows_by_its_kind() {
        let mut flow = default_flow();
        flow.stages.retain(|s| s.id != "validate");
        let columns = Columns::of(&flow);
        let mut t = ticket("API-1", "Rate limit", Status::Checking);
        t.place = Place::stage_of("validate");
        assert_eq!(columns.spot(&t), "dev");
    }
}

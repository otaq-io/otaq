//! The board's columns from the project's flow (STG-08, d0108; d0004, d0066, d0087): Backlog, Ready for Dev, one
//! column per stage with `column: own` (its id the stage's id, its title the stage's name), and Done. A
//! `with-previous` stage shows in the column of the nearest earlier stage with its own (its **owner**): an agent
//! check's cards are part of the agent column (Dev's "Working · Validating"), and a stage folded into a human
//! check's column is a section of its own after the owner's, with its own drop target (Review's "Ready for review
//! · Approved"). On the default flow these are today's five columns with today's ids, and the Delivery's section
//! is `approved`, so the page and its stored state (folds, scroll places) read as before.
//!
//! Where a card shows comes from its place: Backlog, Ready for Dev (waiting before the first Agent stage), Done,
//! or the column of the stage it is in, or waits before (a card waiting between Product spec and Dev shows in Dev's
//! column with its waiting status). A card in a stage its flow no longer has (Validation turned off while it
//! validated) shows where its status's kind does, as the core's drag rules see it.

use otaq_core::board::moves::{DropOn, DropTarget};
use otaq_core::board::order::sort_column;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Place, StatusSet, TicketKey};
use otaq_core::flow::engine::{Target, status_set};
use otaq_core::flow::{CheckBy, Column, Delivers, Flow, Stage, StageKind};
use serde::Serialize;
use ts_rs::TS;

/// The ids of the columns every flow has.
pub const BACKLOG: &str = "backlog";
pub const READY_FOR_DEV: &str = "ready_for_dev";
pub const DONE: &str = "done";

/// The section ids today's board gives the human check before the Delivery and the Delivery in its column (the
/// page stores folded sections by them).
pub const READY_FOR_REVIEW: &str = "ready_for_review";
pub const APPROVED: &str = "approved";

/// What a column holds, from the kind of the stage that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "BoardColumnKind")]
pub enum ColumnKind {
    Backlog,
    /// Waiting before the first Agent stage (d0066).
    ReadyForDev,
    /// An Agent stage or an agent check owns it: agents work there (today's Dev).
    Agent,
    /// A human check owns it (today's Review).
    HumanCheck,
    /// A Delivery stage owns it.
    Delivery,
    Done,
}

impl ColumnKind {
    fn of(kind: StageKind) -> Self {
        match kind {
            StageKind::Agent | StageKind::Check { by: CheckBy::Agent } => Self::Agent,
            StageKind::Check { by: CheckBy::You } => Self::HumanCheck,
            StageKind::Delivery => Self::Delivery,
        }
    }
}

/// A part of a column with its own cards: the stage it shows and the id it goes by (a section's id and drop
/// target; the owner's part goes by the column's id as a drop target).
#[derive(Debug, Clone, PartialEq)]
pub struct Part<'f> {
    pub id: String,
    pub stage: &'f Stage,
}

/// One column of the board.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowColumn<'f> {
    pub id: String,
    pub kind: ColumnKind,
    /// "Backlog", "Ready for Dev", the stage's name ("Done · PR" for a Delivery, [`delivery_title`]), "Done".
    pub title: String,
    /// The stages it shows: its owner, then the `with-previous` stages folded into it. Empty for Backlog, Ready
    /// for Dev and Done.
    pub parts: Vec<Part<'f>>,
}

impl FlowColumn<'_> {
    /// Its sections, each with its own count and cards: a human check's column has one per stage it shows.
    pub fn has_sections(&self) -> bool {
        self.kind == ColumnKind::HumanCheck
    }

    /// The drop targets it is, in order: the column itself, then each section after the owner's.
    pub fn targets(&self) -> Vec<String> {
        let sections = self.has_sections().then(|| self.parts.iter().skip(1));
        std::iter::once(self.id.clone())
            .chain(sections.into_iter().flatten().map(|p| p.id.clone()))
            .collect()
    }
}

/// A flow's columns, left to right.
#[derive(Debug, Clone, PartialEq)]
pub struct Columns<'f> {
    flow: &'f Flow,
    columns: Vec<FlowColumn<'f>>,
}

/// Where a card shows: its column, and in a column with sections, its part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot<'c> {
    pub column: &'c str,
    pub part: Option<&'c str>,
}

impl<'f> Columns<'f> {
    pub fn of(flow: &'f Flow) -> Self {
        let fixed = |id: &str, kind, title: &str| FlowColumn {
            id: id.to_owned(),
            kind,
            title: title.to_owned(),
            parts: Vec::new(),
        };
        let mut columns = vec![
            fixed(BACKLOG, ColumnKind::Backlog, "Backlog"),
            fixed(READY_FOR_DEV, ColumnKind::ReadyForDev, "Ready for Dev"),
        ];
        for (index, stage) in flow.stages.iter().enumerate() {
            let part = Part {
                id: part_id(flow, stage),
                stage,
            };
            match flow.column_owner(index) {
                Some(owner) if owner != index && stage.column == Column::WithPrevious => {
                    if let Some(column) = columns.last_mut() {
                        column.parts.push(part);
                    }
                }
                _ => columns.push(FlowColumn {
                    id: stage.id.clone(),
                    kind: ColumnKind::of(stage.kind),
                    title: delivery_title(stage).unwrap_or_else(|| stage.name.clone()),
                    parts: vec![part],
                }),
            }
        }
        columns.push(fixed(DONE, ColumnKind::Done, "Done"));
        Self { flow, columns }
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

    /// Every drop target, in board order.
    pub fn targets(&self) -> Vec<String> {
        self.columns.iter().flat_map(FlowColumn::targets).collect()
    }

    /// The stage a card in `place` with its status shows as: the stage it is in or waits before; for a stage the
    /// flow doesn't have, the one its status's kind shows in (as the core's drag rules see it).
    pub fn stage_of(&self, ticket: &Ticket) -> Option<&'f Stage> {
        let id = match &ticket.place {
            Place::Backlog | Place::Waiting(None) | Place::Done => return None,
            Place::Waiting(Some(id)) | Place::InStage(id) => id,
        };
        if let Some(stage) = self.flow.stage(id) {
            return Some(stage);
        }
        // A stage the flow doesn't have: the ticket shows where its status belongs.
        let targets: &[DropTarget] = match ticket.status.set() {
            StatusSet::HumanCheck => &[DropTarget::ReadyForReview, DropTarget::Approved],
            StatusSet::Delivery => &[DropTarget::Approved],
            _ => &[DropTarget::Dev],
        };
        targets.iter().find_map(|t| match t.in_flow(self.flow) {
            Some(Target::Stage(stage)) => Some(stage),
            _ => None,
        })
    }

    /// Where `ticket`'s card shows.
    pub fn spot(&self, ticket: &Ticket) -> Spot<'_> {
        let fixed = |column| Spot { column, part: None };
        let stage = match &ticket.place {
            Place::Backlog => return fixed(BACKLOG),
            Place::Waiting(None) => return fixed(READY_FOR_DEV),
            Place::Done => return fixed(DONE),
            Place::Waiting(Some(_)) | Place::InStage(_) => self.stage_of(ticket),
        };
        let found = stage.and_then(|stage| {
            self.columns.iter().find_map(|c| {
                let part = c.parts.iter().find(|p| p.stage.id == stage.id)?;
                Some(Spot {
                    column: c.id.as_str(),
                    part: Some(part.id.as_str()),
                })
            })
        });
        found.unwrap_or_else(|| fixed(BACKLOG))
    }

    /// The column `ticket`'s card shows in.
    pub fn column_of(&self, ticket: &Ticket) -> &FlowColumn<'f> {
        let id = self.spot(ticket).column;
        self.columns
            .iter()
            .find(|c| c.id == id)
            .unwrap_or(&self.columns[0])
    }

    /// The core's drop target a page target stands for (FLW-18): Backlog, Ready for Dev and Done as themselves, a
    /// stage's column or section as the board target that means that stage in the flow (the first Agent stage, the
    /// human check before the Delivery, the Delivery), else the stage itself. Every stage takes a drop; `None` only
    /// for an id the board doesn't have.
    pub fn drop_target(&self, id: &str) -> Option<DropOn> {
        match id {
            BACKLOG => return Some(DropTarget::Backlog.into()),
            READY_FOR_DEV => return Some(DropTarget::ReadyForDev.into()),
            DONE => return Some(DropTarget::Done.into()),
            _ => {}
        }
        let stage = self.columns.iter().find_map(|c| {
            if c.id == id {
                c.parts.first().map(|p| p.stage)
            } else if c.has_sections() {
                c.parts.iter().skip(1).find(|p| p.id == id).map(|p| p.stage)
            } else {
                None
            }
        })?;
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

    /// The cards of `part` (or, without one, the whole of `column`) among `cards`, in board order (SDD §7.5):
    /// each status set's order from the core, in the order an agent column reads (working, then checks, then
    /// cards waiting before its stage).
    pub fn order(
        &self,
        column: &str,
        part: Option<&str>,
        cards: &[&TicketKey],
        all: &[Ticket],
    ) -> Vec<TicketKey> {
        let members: Vec<&Ticket> = all
            .iter()
            .filter(|t| cards.contains(&&t.key))
            .filter(|t| {
                let spot = self.spot(t);
                spot.column == column && (part.is_none() || spot.part == part)
            })
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

/// A Delivery column's title names what it delivers (FLW-12, Q272): "Done · PR", "Done · Local merge"; a click
/// Delivery that lets the user pick (`either`) keeps its name. `None` for other stages.
pub fn delivery_title(stage: &Stage) -> Option<String> {
    let how = match stage.delivery?.delivers {
        Delivers::Pr => "PR",
        Delivers::Local => "Local merge",
        Delivers::Either => return Some(stage.name.clone()),
    };
    Some(format!("{} \u{b7} {how}", stage.name))
}

/// Whether `flow` keeps the default flow's shape (its stages' ids, kinds and columns, in order; FLW-05's "shape"),
/// with or without some of them (a flow without Validate): its board keeps today's look (d0108: "a project that
/// keeps the default flow looks exactly like today's board"), the column icons in their status colours. Any other
/// flow shows each stage's kind as one muted icon (FLW-12). A new agent or model is no shape change.
pub fn keeps_default_look(flow: &Flow) -> bool {
    let shape = |s: &Stage| (s.id.clone(), s.kind, s.column);
    let default = otaq_core::flow::default_flow();
    let mut left = default.stages.iter().map(shape);
    flow.stages
        .iter()
        .map(shape)
        .all(|stage| left.any(|d| d == stage))
}

/// The kind of place `ticket` is in, as the set of statuses it allows (STG-04): Backlog, waiting, Done, or the
/// kind of its stage. The stage is looked up in the default flow, whose ids every project's flow uses in M31 (the
/// derived flow only drops Validate); a stage it doesn't know goes by the status's own set. Cards and sheets decide
/// by it rather than by a column (STG-08).
pub fn place_set(ticket: &Ticket) -> StatusSet {
    match &ticket.place {
        Place::Backlog => StatusSet::Backlog,
        Place::Waiting(_) => StatusSet::Waiting,
        Place::Done => StatusSet::Done,
        Place::InStage(id) => otaq_core::flow::default_file()
            .flow
            .stage(id)
            .map_or_else(|| ticket.status.set(), |s| status_set(s.kind)),
    }
}

/// The order of status sets inside one column (on the default flow every column, and every section, holds one).
const SET_ORDER: [StatusSet; 7] = [
    StatusSet::Backlog,
    StatusSet::AgentStage,
    StatusSet::AgentCheck,
    StatusSet::HumanCheck,
    StatusSet::Delivery,
    StatusSet::Done,
    StatusSet::Waiting,
];

/// The core target that means `stage` in `flow`, if any.
fn stage_target(flow: &Flow, stage: &Stage) -> Option<DropTarget> {
    DropTarget::ALL
        .iter()
        .copied()
        .find(|t| t.in_flow(flow) == Some(Target::Stage(stage)))
}

/// A stage's part id: today's section names for the human check before the Delivery and the Delivery, else its
/// id.
fn part_id(flow: &Flow, stage: &Stage) -> String {
    match stage_target(flow, stage) {
        Some(DropTarget::ReadyForReview) => READY_FOR_REVIEW.to_owned(),
        Some(DropTarget::Approved) => APPROVED.to_owned(),
        _ => stage.id.clone(),
    }
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

    /// STG-08, FLW-01: the default flow's six columns with today's ids and Validate's own, and the Delivery a
    /// section of Review whose drop target is `approved`.
    #[test]
    fn the_default_flow_gives_todays_columns() {
        let flow = default_flow();
        let columns = Columns::of(&flow);
        // FLW-01: Validate has its own column between Dev and Review.
        assert_eq!(
            ids(&columns),
            [
                "backlog",
                "ready_for_dev",
                "dev",
                "validate",
                "review",
                "done"
            ]
        );
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
        let parts = |id: &str| -> Vec<(String, String)> {
            columns
                .get(id)
                .unwrap()
                .parts
                .iter()
                .map(|p| (p.id.clone(), p.stage.id.clone()))
                .collect()
        };
        assert_eq!(parts("dev"), [("dev".into(), "dev".into())]);
        assert_eq!(parts("validate"), [("validate".into(), "validate".into())]);
        assert_eq!(
            parts("review"),
            [
                ("ready_for_review".into(), "review".into()),
                ("approved".into(), "deliver".into())
            ]
        );
        assert_eq!(
            columns.targets(),
            [
                "backlog",
                "ready_for_dev",
                "dev",
                "validate",
                "review",
                "approved",
                "done"
            ]
        );
        for (id, target) in [
            ("backlog", DropTarget::Backlog),
            ("ready_for_dev", DropTarget::ReadyForDev),
            ("dev", DropTarget::Dev),
            ("review", DropTarget::ReadyForReview),
            ("approved", DropTarget::Approved),
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

        // A flow without Validate has no Validate column (d0108 changelog 2026-10-20).
        let mut off = default_flow();
        off.stages.retain(|s| s.id != "validate");
        assert_eq!(
            ids(&Columns::of(&off)),
            ["backlog", "ready_for_dev", "dev", "review", "done"]
        );
    }

    /// STG-08 on the test flow: Product spec, Dev with Validate in it, Security, QA and the Delivery stage each a
    /// column of their own; a card waiting between Product spec and Dev shows in Dev's column.
    #[test]
    fn the_test_flow_gives_a_column_per_own_stage() {
        let flow = every_kind();
        let columns = Columns::of(&flow);
        assert_eq!(
            ids(&columns),
            [
                "backlog",
                "ready_for_dev",
                "product-spec",
                "dev",
                "security",
                "qa",
                "ship",
                "done"
            ]
        );
        let kinds: Vec<ColumnKind> = columns.iter().map(|c| c.kind).collect();
        use ColumnKind as K;
        assert_eq!(
            kinds,
            [
                K::Backlog,
                K::ReadyForDev,
                K::Agent,
                K::Agent,
                K::Agent,
                K::HumanCheck,
                K::Delivery,
                K::Done
            ]
        );
        let titles: Vec<&str> = columns.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Backlog",
                "Ready for Dev",
                "Product spec",
                "Dev",
                "Security",
                "QA",
                "Done \u{b7} PR",
                "Done"
            ]
        );
        let dev: Vec<&str> = columns
            .get("dev")
            .unwrap()
            .parts
            .iter()
            .map(|p| p.stage.id.as_str())
            .collect();
        assert_eq!(dev, ["dev", "validate"]);

        let mut waiting = ticket("NOTE-2", "Export as CSV", Status::Queued);
        waiting.place = Place::Waiting(Some("dev".into()));
        assert_eq!(columns.spot(&waiting).column, "dev");
        let mut validating = ticket("NOTE-4", "Search notes", Status::Checking);
        validating.place = Place::stage_of("validate");
        assert_eq!(
            columns.spot(&validating),
            Spot {
                column: "dev",
                part: Some("validate")
            }
        );
        let mut checking = ticket("NOTE-5", "Security", Status::Checking);
        checking.place = Place::stage_of("security");
        assert_eq!(columns.spot(&checking).column, "security");
        let ready = ticket("NOTE-6", "Ready", Status::Queued);
        assert_eq!(columns.spot(&ready).column, "ready_for_dev");
    }

    /// A card in a stage the flow doesn't have shows where its kind does: Validation turned off mid-check.
    #[test]
    fn a_card_in_a_removed_stage_shows_by_its_kind() {
        let mut flow = default_flow();
        flow.stages.retain(|s| s.id != "validate");
        let columns = Columns::of(&flow);
        let mut t = ticket("API-1", "Rate limit", Status::Checking);
        t.place = Place::stage_of("validate");
        assert_eq!(columns.spot(&t).column, "dev");
    }
}

//! What an Epic Card shows (*01 · Components → Card component → Epic Card*; *04 → Row · epics*, *Row · epics as
//! delivery units (0059)*): key, External ID chip, the epic pill, title, progress ("1 / 3 merged", one segment
//! per child), its branch, the child list with statuses, "Add ticket to epic", and the actions of its later
//! states (Review feature in M6, Mark merged / Mark closed in M10). EPC-01, d0039, d0059.
//!
//! From M16 (d0080) the card is the epic in its **Card** place: its children are rows inside it, never separate
//! Backlog cards. A row is draggable while its child can be queued from there (never started, or Parked after
//! Park all, its status "Parked · has work", EPC-14).
//!
//! From M17 (d0083, T-0289) a Backlog epic card has a **Lead** row under the branch line: its lead's
//! [`LeadStatusView`] in the full form, or **Open lead** when it has none (**Split with the Project agent** while it
//! has no children either, LEAD-13, d0091). A review card keeps the ended lead
//! ("Wishlist lead · ended · in review"). Child rows the lead created say "added by Wishlist lead", and a lead that
//! asks you turns the card's border turquoise ([`EpicViewModel::with_lead`]).

use crate::view_models::flow_columns::place_set;
use otaq_core::board::epic_flight::{self, EpicPlace};
use otaq_core::board::epics::progress_of;
use otaq_core::board::tickets::{Base, Ticket};
use otaq_core::domain::StatusSet;
use otaq_core::domain::{Actor, Status, TicketKey};
use otaq_core::project_agents::lead::Lead;

use crate::icons::IconName;
use crate::theme::Status as Legend;
use crate::view_models::card::{CardAction, CardActionKind, Pill};
use crate::view_models::lead_status::LeadStatusView;

/// How a child shows in the list and in the progress bar. `EpicChildState` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "EpicChildState")]
pub enum ChildState {
    Merged,
    Closed,
    Working,
    NotStarted,
}

impl ChildState {
    pub fn of(status: Status) -> Self {
        match status {
            Status::Merged => Self::Merged,
            Status::ClosedNotMerged => Self::Closed,
            s if s.set() == StatusSet::Backlog || s.is_queued() => Self::NotStarted,
            _ => Self::Working,
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::Merged => IconName::CircleCheck,
            Self::Closed => IconName::CircleSlash,
            Self::Working => IconName::Activity,
            Self::NotStarted => IconName::CircleDashed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildRow {
    pub key: TicketKey,
    pub title: String,
    pub state: ChildState,
    pub status_label: &'static str,
    /// The row can be dragged out of the card to queue only this child (EPC-12, EPC-14): a never-started or
    /// Parked child of an epic in its Card place.
    pub draggable: bool,
    /// "added by Wishlist lead": the epic's lead created it (d0083). Rows the user added have none.
    pub origin: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EpicViewModel {
    pub key: TicketKey,
    pub pinned: bool,
    /// Hidden when empty.
    pub external_id: Option<String>,
    /// The Priority chip after the External ID; `None` for Normal (d0065).
    pub priority: Option<otaq_core::domain::Priority>,
    pub pill: Pill,
    pub title: String,
    /// One per child, in key order.
    pub segments: Vec<ChildState>,
    pub progress: String,
    pub branch: String,
    pub children: Vec<ChildRow>,
    /// "Add ticket to epic" (not once the epic is done or closed).
    pub can_add: bool,
    pub actions: Vec<CardAction>,
    pub faded: Option<f32>,
    /// The Lead row under the branch line (T-0289); `None` on a done or closed card, and on a review card whose
    /// epic never had a lead.
    pub lead: Option<LeadStatusView>,
}

impl EpicViewModel {
    /// `tickets` is the epic's project (its children are found by their epic key). `branch` is the epic's
    /// branch name once it has one (the temporary `otaq/SHOP-40` until the first push, d0050).
    pub fn from(epic: &Ticket, tickets: &[Ticket], branch: Option<&str>) -> Self {
        let mut children: Vec<&Ticket> = tickets
            .iter()
            .filter(|t| t.epic.as_ref() == Some(&epic.key))
            .collect();
        children.sort_by(|a, b| a.key.cmp(&b.key));
        let progress = progress_of(&epic.key, tickets);
        let card_place = epic_flight::epic_place(epic, tickets) == EpicPlace::Card;
        let pill = match epic.status {
            Status::WaitingForYou => Pill {
                legend: Legend::ReadyForReview,
                label: "Ready for Review".into(),
            },
            Status::PrOpen => Pill {
                legend: Legend::PrOpen,
                label: epic
                    .flags
                    .pr_number
                    .map_or_else(|| "PR open".into(), |n| format!("PR #{n}")),
            },
            Status::Merged => Pill {
                legend: Legend::Done,
                label: "Epic · Done".into(),
            },
            Status::ClosedNotMerged => Pill {
                legend: Legend::ClosedNotMerged,
                label: "Closed · not merged".into(),
            },
            _ => Pill {
                legend: Legend::Epic,
                label: "Epic".into(),
            },
        };
        let base = match &epic.base {
            Base::Branch(b) => b.clone(),
            Base::Epic(e) => e.to_string(),
        };
        let branch_name = branch
            .map(str::to_owned)
            .unwrap_or_else(|| otaq_core::naming::branch(&epic.key.to_string()));
        let branch = match epic.status {
            Status::Merged | Status::PrOpen => branch_name,
            Status::ClosedNotMerged => format!("{branch_name} · kept"),
            _ => format!("{branch_name} · on {base}"),
        };
        let actions = match epic.status {
            Status::WaitingForYou => vec![CardAction {
                kind: CardActionKind::Review,
                label: "Review feature",
                icon: Some(IconName::Eye),
                primary: true,
            }],
            Status::PrOpen => vec![
                CardAction {
                    kind: CardActionKind::MarkMerged,
                    label: "Mark merged",
                    icon: None,
                    primary: false,
                },
                CardAction {
                    kind: CardActionKind::MarkClosed,
                    label: "Mark closed",
                    icon: None,
                    primary: false,
                },
            ],
            _ => Vec::new(),
        };
        let done = matches!(epic.status, Status::Merged | Status::ClosedNotMerged);
        Self {
            key: epic.key.clone(),
            pinned: epic.flags.is_pinned(),
            external_id: epic.external_id.clone().filter(|e| !e.is_empty()),
            priority: crate::view_models::priority::on_card(epic.priority),
            pill,
            title: epic.title.clone(),
            segments: children.iter().map(|c| ChildState::of(c.status)).collect(),
            progress: progress.label(),
            branch,
            children: children
                .iter()
                .map(|c| ChildRow {
                    key: c.key.clone(),
                    title: c.title.clone(),
                    state: ChildState::of(c.status),
                    status_label: c.status.label(),
                    draggable: card_place
                        && (epic_flight::is_never_started(c) || epic_flight::is_parked(c)),
                    origin: None,
                })
                .collect(),
            can_add: !done && epic.status != Status::PrOpen,
            actions,
            faded: done.then_some(0.55),
            lead: None,
        }
    }

    /// On the board (FLW-16, `s03-46`): an open PR reads "PR #77 open" and its comments become a child (Add a child
    /// from PR comments · Mark merged; Mark closed stays in the panel).
    pub fn with_pr_comments(mut self, epic: &Ticket) -> Self {
        if epic.status != Status::PrOpen {
            return self;
        }
        if let Some(n) = epic.flags.pr_number {
            self.pill.label = format!("PR #{n} open");
        }
        self.actions = vec![
            CardAction {
                kind: CardActionKind::AddChildFromPr,
                label: "Add a child from PR comments",
                icon: None,
                primary: false,
            },
            CardAction {
                kind: CardActionKind::MarkMerged,
                label: "Mark merged",
                icon: None,
                primary: false,
            },
        ];
        self
    }

    /// The epic's lead on its card (LEAD-11, LEAD-02): `lead` is its lead, if it has one. In Backlog the Lead row
    /// shows it, or Open lead; in Ready for Review, Approved and PR open only a lead it had ("ended · in review").
    pub fn with_lead(mut self, epic: &Ticket, tickets: &[Ticket], lead: Option<&Lead>) -> Self {
        self.set_lead(epic, tickets, lead);
        self
    }

    /// [`with_lead`](Self::with_lead) in place.
    pub fn set_lead(&mut self, epic: &Ticket, tickets: &[Ticket], lead: Option<&Lead>) {
        let in_review = matches!(place_set(epic), StatusSet::HumanCheck | StatusSet::Delivery);
        self.lead = match () {
            _ if epic.place.is_backlog() => Some(LeadStatusView::of_epic(epic, tickets, lead)),
            _ if in_review && epic.status != Status::ClosedNotMerged => {
                lead.map(|l| LeadStatusView::of(epic, Some(l)).on_review_card())
            }
            _ => None,
        };
        if let Some(lead) = lead {
            let by = Actor::ProjectAgent(lead.agent.id.clone());
            for row in &mut self.children {
                let created_by_lead = tickets
                    .iter()
                    .any(|t| t.key == row.key && t.created_by == by);
                row.origin = created_by_lead.then(|| format!("added by {}", lead.name));
            }
        }
    }

    /// The card's border is turquoise: its lead asks you.
    pub fn lead_asks(&self) -> bool {
        self.lead.as_ref().is_some_and(LeadStatusView::asks_you)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::domain::Kind;

    fn wishlist() -> Vec<Ticket> {
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = Kind::Epic;
        epic.external_id = Some("PAY-123".into());
        let child = |key: &str, title: &str, status| {
            let mut t = ticket(key, title, status);
            t.epic = Some("SHOP-40".parse().unwrap());
            t
        };
        vec![
            epic,
            child("SHOP-40.2", "Wishlist button", Status::Working),
            child("SHOP-40.1", "Wishlist API", Status::Merged),
            child("SHOP-40.3", "Wishlist page", Status::Backlog),
        ]
    }

    /// #31: a child closed on purpose is counted apart on the card and the sheet's head.
    #[test]
    fn a_closed_child_is_counted_apart() {
        let mut tickets = wishlist();
        tickets[1].status = Status::Merged;
        tickets[3].status = Status::ClosedNotMerged;
        let vm = EpicViewModel::from(&tickets[0], &tickets, None);
        assert_eq!(vm.progress, "2 merged \u{b7} 1 closed");
        let head = crate::view_models::epic_sheet::head(&tickets[0], &tickets, None);
        assert_eq!(head.progress, "2 merged \u{b7} 1 closed");
        tickets[1].status = Status::Working;
        let vm = EpicViewModel::from(&tickets[0], &tickets, None);
        assert_eq!(vm.progress, "1 of 2 merged \u{b7} 1 closed");
    }

    #[test]
    fn shop_40_wishlist() {
        let tickets = wishlist();
        let vm = EpicViewModel::from(&tickets[0], &tickets, None);
        assert_eq!(vm.progress, "1 of 3 merged");
        assert_eq!(vm.external_id.as_deref(), Some("PAY-123"));
        assert_eq!(vm.pill.label, "Epic");
        assert_eq!(
            vm.segments,
            [
                ChildState::Merged,
                ChildState::Working,
                ChildState::NotStarted
            ]
        );
        let keys: Vec<String> = vm.children.iter().map(|c| c.key.to_string()).collect();
        assert_eq!(keys, ["SHOP-40.1", "SHOP-40.2", "SHOP-40.3"]);
        assert_eq!(vm.children[1].state.icon(), IconName::Activity);
        assert_eq!(
            vm.branch,
            format!("{} · on main", otaq_core::naming::branch("SHOP-40"))
        );
        assert!(vm.can_add);
        assert!(vm.actions.is_empty());
        assert!(
            vm.children.iter().all(|c| !c.draggable),
            "SHOP-40.2 is working: a track, no rows to drag"
        );
    }

    /// EPC-11, EPC-14: before anything starts every row can be dragged out; after Park all the rows say
    /// "Parked · has work" and stay draggable (dragging one resumes it); a merged child's row never is.
    #[test]
    fn rows_know_which_can_be_dragged() {
        let mut tickets = wishlist();
        for t in &mut tickets[1..] {
            t.status = Status::Backlog;
            t.place = otaq_core::domain::Place::Backlog;
        }
        let vm = EpicViewModel::from(&tickets[0], &tickets, None);
        assert!(vm.children.iter().all(|c| c.draggable));
        assert!(vm.children.iter().all(|c| c.status_label == "Backlog"));

        tickets[1].status = Status::Parked;
        tickets[2].status = Status::Merged;
        tickets[2].place = otaq_core::domain::Place::Done;
        tickets[3].status = Status::Parked;
        let vm = EpicViewModel::from(&tickets[0], &tickets, None);
        let rows: Vec<(String, &str, bool)> = vm
            .children
            .iter()
            .map(|c| (c.key.to_string(), c.status_label, c.draggable))
            .collect();
        assert_eq!(
            rows,
            [
                ("SHOP-40.1".to_owned(), "Merged", false),
                ("SHOP-40.2".to_owned(), "Parked · has work", true),
                ("SHOP-40.3".to_owned(), "Parked · has work", true),
            ]
        );

        tickets[0].status = Status::WaitingForYou;
        tickets[0].place = otaq_core::domain::Place::stage_of("review");
        let vm = EpicViewModel::from(&tickets[0], &tickets, None);
        assert!(
            vm.children.iter().all(|c| !c.draggable),
            "a delivery card's rows stay put"
        );
    }

    #[test]
    fn external_id_hidden_when_empty_and_later_states() {
        let mut tickets = wishlist();
        tickets[0].external_id = Some(String::new());
        tickets[0].status = Status::WaitingForYou;
        let vm = EpicViewModel::from(&tickets[0], &tickets, None);
        assert_eq!(vm.external_id, None);
        assert_eq!(vm.actions[0].label, "Review feature");

        tickets[0].status = Status::PrOpen;
        tickets[0].flags.pr_number = Some(490);
        let vm = EpicViewModel::from(&tickets[0], &tickets, Some("feature/PAY-123-wishlist"));
        assert_eq!(vm.pill.label, "PR #490");
        assert_eq!(vm.branch, "feature/PAY-123-wishlist");
        assert_eq!(vm.actions.len(), 2);
        // FLW-16 (`s03-46`): on the board, Add a child from PR comments · Mark merged.
        let custom = vm.clone().with_pr_comments(&tickets[0]);
        assert_eq!(custom.pill.label, "PR #490 open");
        let labels: Vec<_> = custom.actions.iter().map(|a| a.label).collect();
        assert_eq!(labels, ["Add a child from PR comments", "Mark merged"]);
        assert!(!vm.can_add);

        tickets[0].status = Status::ClosedNotMerged;
        let vm = EpicViewModel::from(&tickets[0], &tickets, None);
        assert_eq!(vm.pill.label, "Closed · not merged");
        assert_eq!(vm.faded, Some(0.55));
        assert!(vm.branch.ends_with(" · kept"));
    }

    /// *Row · Epic Card with a lead (0083)*: Open lead with no lead; the Lead row, the rows the lead added and the
    /// turquoise border; the review card's short ended words; nothing once done.
    #[test]
    fn the_lead_row_open_lead_and_rows_added_by_the_lead() {
        use crate::view_models::lead_status::{self, ActionKind};
        use otaq_core::project_agents::lead::LeadStatus;

        let mut tickets = wishlist();
        for t in &mut tickets[1..] {
            t.status = Status::Backlog;
            t.place = otaq_core::domain::Place::Backlog;
        }
        let epic = tickets[0].clone();
        let vm = EpicViewModel::from(&epic, &tickets, None).with_lead(&epic, &tickets, None);
        let lead = vm.lead.as_ref().expect("a Lead row");
        assert_eq!(lead.full, "Open lead");
        assert_eq!(lead.action.as_ref().map(|a| a.kind), Some(ActionKind::Open));
        assert!(vm.children.iter().all(|c| c.origin.is_none()));
        assert!(!vm.lead_asks());

        // No children (LEAD-13, d0091): Split with the Project agent in Open lead's place.
        let alone = [epic.clone()];
        let vm = EpicViewModel::from(&epic, &alone, None).with_lead(&epic, &alone, None);
        let lead = vm.lead.as_ref().expect("a Lead row");
        assert_eq!(lead.full, "Split with the Project agent");
        let action = lead.action.as_ref().unwrap();
        assert_eq!(action.kind, ActionKind::Split);
        assert_eq!(
            action.split.as_ref().map(|s| s.message.as_str()),
            Some("Split SHOP-40 into tickets")
        );

        let idle = lead_status::sample(&epic, LeadStatus::Idle, 0);
        tickets[1].created_by = Actor::ProjectAgent(idle.agent.id.clone());
        tickets[3].created_by = Actor::ProjectAgent(idle.agent.id.clone());
        let vm = EpicViewModel::from(&epic, &tickets, None).with_lead(&epic, &tickets, Some(&idle));
        assert_eq!(vm.lead.as_ref().unwrap().full, "Wishlist lead · idle");
        let origins: Vec<Option<&str>> = vm.children.iter().map(|c| c.origin.as_deref()).collect();
        assert_eq!(
            origins,
            [
                None,
                Some("added by Wishlist lead"),
                Some("added by Wishlist lead")
            ],
            "rows sort by key: 40.1 (user), 40.2, 40.3"
        );
        assert!(!vm.lead_asks());

        let asks = lead_status::sample(&epic, LeadStatus::AsksYou, 0);
        let vm = EpicViewModel::from(&epic, &tickets, None).with_lead(&epic, &tickets, Some(&asks));
        assert!(vm.lead_asks());

        let mut review = epic.clone();
        review.status = Status::WaitingForYou;
        review.place = otaq_core::domain::Place::stage_of("review");
        let ended = lead_status::sample(&review, LeadStatus::EndedInReview, 0);
        let vm =
            EpicViewModel::from(&review, &tickets, None).with_lead(&review, &tickets, Some(&ended));
        assert_eq!(vm.lead.unwrap().full, "Wishlist lead · ended · in review");
        let vm = EpicViewModel::from(&review, &tickets, None).with_lead(&review, &tickets, None);
        assert_eq!(vm.lead, None, "no Open lead in review");

        let mut done = review.clone();
        done.status = Status::Merged;
        done.place = otaq_core::domain::Place::Done;
        let vm =
            EpicViewModel::from(&done, &tickets, None).with_lead(&done, &tickets, Some(&ended));
        assert_eq!(vm.lead, None);
    }
}

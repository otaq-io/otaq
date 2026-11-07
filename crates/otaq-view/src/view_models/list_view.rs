//! The List: the board as rows. Board and List are two layouts of the same board, so the List shows the board's
//! groups in the board's places: each stage of the flow in flow order, then Backlog, then Done; a stage folded on the
//! board (its ⋯, or Fold empty columns while it is empty) is folded here too, and Done folded is one line at the end,
//! "Done · 3". A row says what the card says ([`board::ticket_card`]: key, title, the status line, who, time and the
//! one action), and what needs you stands out only by the card's coloured edge.
//!
//! Inside a stage the rows keep the board's order with one difference: the board floats a card that needs you to the
//! top of its column, the List doesn't, so answering a ticket changes its row's colour and words, never its place. A
//! started ticket waiting to begin is still at the top of its stage.
//!
//! Each group is the drop target of its board column: a row dragged onto a stage is the card dropped on that column
//! (the drag's verdicts are `board.drag`'s). The row menu is the card's (`board.menu`).

use std::cmp::Reverse;

use otaq_core::board::order::{blocked_counts, start_order};
use otaq_core::board::settings::ViewPrefs;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Place, StatusSet, TicketKey};
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::board::{self, BoardInput};
use crate::view_models::board_page::DropTargetId;
use crate::view_models::card_page::TicketCardView;
use crate::view_models::flow_columns::{self, Columns};
use crate::view_models::view_prefs::Change;

/// The group of tickets in Backlog, as the board's column.
pub const BACKLOG: &str = "Backlog";
/// The group of finished tickets, as the board's column.
pub const DONE: &str = "Done";
/// Done folded: one line at the end ("Done · 3").
pub const DONE_LINE: &str = "Done \u{b7} {n}";
/// An open group with no row to show.
pub const NOTHING_HERE: &str = "Nothing here";

/// The List as the page draws it. `ListView` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ListView")]
pub struct ListView {
    pub groups: Vec<Group>,
    /// Done while it is folded: one line at the end; `null` while Done is a group of its own.
    pub done: Option<DoneLine>,
}

/// Done folded at the end of the List: "Done · 3"; a click opens it. `ListDoneLine` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ListDoneLine")]
pub struct DoneLine {
    pub text: String,
    pub unfold: Change,
    /// It stays Done's drop target.
    pub target: DropTargetId,
}

/// One group of rows: its head ("Dev 5", with the fold's chevron) and its rows. `ListGroup` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ListGroup")]
pub struct Group {
    /// The board column's id: a stage's id, "backlog", "done".
    pub id: String,
    pub title: String,
    /// "3", or "1/3" while a filter hides some.
    pub count: String,
    /// Its rows are folded away: the head alone, its chevron pointing right.
    pub folded: bool,
    /// The change a click on the head sends (the board column's fold); `null` for an empty stage folded by Fold empty
    /// columns, which has nothing to open.
    pub toggle: Option<Change>,
    /// "Nothing here" when it is open and has no row to show.
    pub empty: Option<String>,
    /// The board column it stands for, as a drop target.
    pub target: DropTargetId,
    pub entries: Vec<Entry>,
}

impl Group {
    /// Its ticket rows, top to bottom.
    pub fn rows(&self) -> impl Iterator<Item = &Row> {
        self.entries
            .iter()
            .map(|Entry::Ticket { row }| row.as_ref())
    }
}

/// A line of a group. `ListEntry` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "ListEntry")]
pub enum Entry {
    Ticket { row: Box<Row> },
}

/// A ticket's row. `ListRow` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ListRow")]
pub struct Row {
    /// The card it stands for: key, title, status line, who, time, the coloured edge (only while it needs you) and
    /// the one action (Start on a Backlog row).
    pub card: TicketCardView,
}

/// `t`'s row: its card from [`board::ticket_card`].
pub fn row(input: &BoardInput, t: &Ticket) -> Row {
    Row {
        card: TicketCardView::from_vm(board::ticket_card(input, t), t),
    }
}

/// Where a ticket's status puts it within its column, as the board's order has it: waiting to begin first, then the
/// stage's own work, the user's check, delivery, done.
fn set_rank(t: &Ticket) -> usize {
    const ORDER: [StatusSet; 6] = [
        StatusSet::Waiting,
        StatusSet::Backlog,
        StatusSet::AgentStage,
        StatusSet::HumanCheck,
        StatusSet::Delivery,
        StatusSet::Done,
    ];
    let set = t.status.set().board_group();
    ORDER.iter().position(|s| *s == set).unwrap_or(ORDER.len())
}

/// A stage's rows in the board's order, without the board's lift of what needs the user: waiting tickets in their
/// start order, the others pinned first, then by how many wait for them, then oldest first. Nothing in it changes
/// when a ticket is answered, so answering never moves a row.
fn stage_order(keys: Vec<TicketKey>, all: &[Ticket]) -> Vec<TicketKey> {
    let blocks = blocked_counts(all);
    let mut tickets: Vec<&Ticket> = keys
        .iter()
        .filter_map(|k| all.iter().find(|t| &t.key == k))
        .collect();
    tickets.sort_by(|a, b| {
        set_rank(a).cmp(&set_rank(b)).then_with(|| {
            if a.status.set() == StatusSet::Waiting {
                start_order(a, all).cmp(&start_order(b, all))
            } else {
                let key = |t: &Ticket| {
                    (
                        Reverse(t.flags.pinned_at.unwrap_or(0)),
                        Reverse(blocks.get(&t.key).copied().unwrap_or(0)),
                        t.created_at,
                        t.key.clone(),
                    )
                };
                key(a).cmp(&key(b))
            }
        })
    });
    tickets.into_iter().map(|t| t.key.clone()).collect()
}

/// The List for `input`'s project, folded as the board is by `prefs`.
pub fn view(input: &BoardInput, prefs: &ViewPrefs) -> ListView {
    let columns = Columns::of(input.flow);
    let all = input.tickets;
    let keys: Vec<&TicketKey> = all.iter().map(|t| &t.key).collect();
    let filtered = input.visible.is_some();
    let shown = |k: &TicketKey| input.visible.is_none_or(|v| v.contains(k));
    let folded_on_board = |id: &str| prefs.folded_columns.iter().any(|c| c == id);
    let group = |id: &str, title: &str, order: Vec<TicketKey>| -> Group {
        let total = order.len();
        let rows: Vec<Row> = order
            .iter()
            .filter(|k| shown(k))
            .filter_map(|k| all.iter().find(|t| &t.key == k))
            .map(|t| row(input, t))
            .collect();
        let count = if filtered {
            format!("{}/{total}", rows.len())
        } else {
            total.to_string()
        };
        let by_hand = folded_on_board(id);
        let folded = by_hand || (prefs.fold_empty_columns && total == 0);
        let toggle = (by_hand || !folded).then(|| Change::FoldColumn {
            column: id.to_owned(),
            folded: !folded,
        });
        Group {
            id: id.to_owned(),
            title: title.to_owned(),
            count,
            folded,
            toggle,
            empty: (!folded && rows.is_empty()).then(|| NOTHING_HERE.to_owned()),
            target: DropTargetId::new(id.to_owned()),
            entries: if folded {
                Vec::new()
            } else {
                rows.into_iter()
                    .map(|row| Entry::Ticket { row: Box::new(row) })
                    .collect()
            },
        }
    };
    let mut groups: Vec<Group> = columns
        .iter()
        .filter(|c| c.stage.is_some())
        .map(|c| {
            group(
                &c.id,
                &c.title,
                stage_order(columns.order(&c.id, &keys, all), all),
            )
        })
        .collect();
    groups.push(group(
        flow_columns::BACKLOG,
        BACKLOG,
        columns.order(flow_columns::BACKLOG, &keys, all),
    ));
    let done = group(
        flow_columns::DONE,
        DONE,
        columns.order(flow_columns::DONE, &keys, all),
    );
    // Done folded is a line at the end, as on the board; open, it is the last group.
    let done = if done.folded {
        Some(DoneLine {
            text: DONE_LINE.replace("{n}", &done.count),
            unfold: Change::FoldColumn {
                column: flow_columns::DONE.to_owned(),
                folded: false,
            },
            target: DropTargetId::DONE,
        })
    } else {
        groups.push(done);
        None
    };
    ListView { groups, done }
}

/// The List's states for the Gallery (`gallery.list_view`), on the example's shop-web. `ListGallery` on the page.
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
    use samples::{input, prefs, shop_web, tickets};
    let flow = shop_web();
    let tickets = tickets();
    let draw = |prefs: ViewPrefs| view(&input(&flow, &tickets), &prefs);
    Gallery {
        states: vec![
            GalleryState {
                label: "Done folded",
                view: draw(prefs()),
            },
            GalleryState {
                label: "Done open, Security folded",
                view: draw(ViewPrefs {
                    folded_columns: vec!["security".to_owned()],
                    ..prefs()
                }),
            },
        ],
    }
}

/// The example's shop-web (`tickets/m38-tickets.md` → The example), for the Gallery and the tests.
pub mod samples {
    use otaq_core::board::settings::{BoardView, ViewPrefs};
    use otaq_core::board::tickets::{AgentSignal, Ticket};
    use otaq_core::domain::{Place, Priority, Status, ValidationRound};
    use otaq_core::flow::Flow;

    use crate::view_models::board::BoardInput;
    use crate::view_models::card_fixtures::{NOW, ticket};

    /// The example's shop-web flow: Product spec, Dev, Validate, Security, Review, then a PR.
    pub fn shop_web() -> Flow {
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

    /// The example's shop-web tickets.
    pub fn tickets() -> Vec<Ticket> {
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
            Place::stage_of("review"),
        );
        pr.flags.pr_number = Some(481);
        let mut tickets = vec![
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
                    Place::stage_of("product-spec"),
                ),
                "Follow the system setting?",
            ),
            working("SHOP-44", "Saved carts", "product-spec", 60_000),
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
        // Merged a month ago.
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
            helpers: &[],
        }
    }

    pub fn prefs() -> ViewPrefs {
        ViewPrefs {
            view: BoardView::List,
            list_layout: true,
            ..ViewPrefs::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::samples::{input, prefs, shop_web, tickets};
    use super::*;
    use crate::view_models::card::{CardActionKind, Tone};
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::domain::Status;

    /// Each group's title, count, whether it is folded, then its rows' keys.
    fn outline(list: &ListView) -> Vec<(String, String, bool, Vec<String>)> {
        list.groups
            .iter()
            .map(|g| {
                (
                    g.title.clone(),
                    g.count.clone(),
                    g.folded,
                    g.rows().map(|r| r.card.key.clone()).collect(),
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

    /// Where `key`'s row is: its group's title and its index there.
    fn place_of(list: &ListView, key: &str) -> (String, usize) {
        list.groups
            .iter()
            .find_map(|g| {
                g.rows()
                    .position(|r| r.card.key == key)
                    .map(|i| (g.title.clone(), i))
            })
            .unwrap_or_else(|| panic!("no row {key}"))
    }

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|k| (*k).to_owned()).collect()
    }

    /// The stages in flow order, then Backlog, then Done folded to its line; inside each stage the board's order
    /// with a started ticket waiting to begin at the top.
    #[test]
    fn groups_are_the_stages_in_flow_order_then_backlog_then_done() {
        let flow = shop_web();
        let tickets = tickets();
        let list = view(&input(&flow, &tickets), &prefs());
        assert_eq!(
            outline(&list),
            [
                (
                    "Product spec".into(),
                    "4".into(),
                    false,
                    keys(&["SHOP-35", "SHOP-20", "SHOP-12", "SHOP-44"])
                ),
                (
                    "Dev".into(),
                    "3".into(),
                    false,
                    keys(&["SHOP-22", "SHOP-29", "SHOP-33"])
                ),
                ("Validate".into(), "1".into(), false, keys(&["SHOP-30"])),
                (
                    "Security".into(),
                    "2".into(),
                    false,
                    keys(&["SHOP-19", "SHOP-32"])
                ),
                (
                    "Review".into(),
                    "2".into(),
                    false,
                    keys(&["SHOP-18", "SHOP-26"])
                ),
                (
                    "Backlog".into(),
                    "3".into(),
                    false,
                    keys(&["SHOP-24", "SHOP-31", "SHOP-41"])
                ),
            ]
        );
        assert_eq!(
            list.done,
            Some(DoneLine {
                text: "Done \u{b7} 5".into(),
                unfold: Change::FoldColumn {
                    column: "done".into(),
                    folded: false
                },
                target: DropTargetId::DONE,
            })
        );
        // Each group is its board column's drop target.
        let targets: Vec<&str> = list.groups.iter().map(|g| g.target.as_str()).collect();
        assert_eq!(
            targets,
            [
                "product-spec",
                "dev",
                "validate",
                "security",
                "review",
                "backlog"
            ]
        );
    }

    /// A row says what the card says, and only what needs you has the coloured edge.
    #[test]
    fn rows_use_the_cards_words() {
        let flow = shop_web();
        let tickets = tickets();
        let list = view(&input(&flow, &tickets), &prefs());
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
                Some("Asks you \u{b7} follow the system setting?".into()),
                Some("Answer".into()),
                Some(Tone::Attention)
            )
        );
        assert_eq!(line("SHOP-18").2, Some(Tone::Review));
        assert_eq!(line("SHOP-30").0.as_deref(), Some("Checking"));
        assert_eq!(line("SHOP-33").2, None);
        assert_eq!(row(&list, "SHOP-33").card.time.as_ref().unwrap().text, "3m");
        let blocked = row(&list, "SHOP-20").card.status.as_ref().unwrap();
        assert_eq!(blocked.text, "Waits for SHOP-18 to merge");
        // A Backlog row's one action is the card's Start.
        let start = row(&list, "SHOP-41").card.action.as_ref().unwrap();
        assert_eq!(start.kind, CardActionKind::Start);
        // Each row is the board's card for that ticket, word for word.
        let t = tickets
            .iter()
            .find(|t| t.key.to_string() == "SHOP-22")
            .unwrap();
        assert_eq!(
            row(&list, "SHOP-22").card,
            TicketCardView::from_vm(board::ticket_card(&input(&flow, &tickets), t), t)
        );
    }

    /// Answering a ticket changes its row's colour and words, never its place, even where the board floats what
    /// needs you to the top.
    #[test]
    fn answering_changes_the_row_not_its_place() {
        let flow = shop_web();
        let mut tickets = tickets();
        // A later ticket in Dev that asks: the board lifts it over the working ones, the List keeps it last.
        let mut late = ticket("SHOP-50", "Late question", Status::AsksYou);
        late.place = Place::stage_of("dev");
        late.created_at = 10;
        tickets.push(late);
        let before = view(&input(&flow, &tickets), &prefs());
        assert_eq!(place_of(&before, "SHOP-50"), ("Dev".into(), 3));
        assert_eq!(place_of(&before, "SHOP-22"), ("Dev".into(), 0));
        assert_eq!(row(&before, "SHOP-50").card.edge, Some(Tone::Attention));

        // Both answered: back at work.
        for t in tickets.iter_mut() {
            if ["SHOP-50", "SHOP-22", "SHOP-12"].contains(&t.key.to_string().as_str()) {
                t.status = Status::Working;
                t.signal = None;
            }
        }
        let after = view(&input(&flow, &tickets), &prefs());
        for key in ["SHOP-50", "SHOP-22", "SHOP-12"] {
            assert_eq!(place_of(&after, key), place_of(&before, key), "{key}");
            assert_eq!(row(&after, key).card.edge, None, "{key}");
        }
        assert_eq!(
            row(&after, "SHOP-50")
                .card
                .status
                .as_ref()
                .map(|s| s.text.as_str()),
            Some("Working")
        );
    }

    /// The List folds as the board does: a column folded by its ⋯ is a folded group whose head opens it; Fold empty
    /// columns folds an empty stage, with nothing to open; Done open is the last group.
    #[test]
    fn folds_follow_the_board() {
        let flow = shop_web();
        let tickets: Vec<Ticket> = tickets()
            .into_iter()
            .filter(|t| t.key.to_string() != "SHOP-30")
            .collect();
        let list = view(
            &input(&flow, &tickets),
            &ViewPrefs {
                folded_columns: vec!["security".into()],
                fold_empty_columns: true,
                ..prefs()
            },
        );
        let group = |title: &str| list.groups.iter().find(|g| g.title == title).unwrap();
        let security = group("Security");
        assert!(security.folded && security.entries.is_empty() && security.empty.is_none());
        assert_eq!(security.count, "2");
        assert_eq!(
            security.toggle,
            Some(Change::FoldColumn {
                column: "security".into(),
                folded: false
            })
        );
        let validate = group("Validate");
        assert!(validate.folded && validate.toggle.is_none());
        assert_eq!(list.done, None);
        let done = list.groups.last().unwrap();
        assert_eq!((done.title.as_str(), done.count.as_str()), ("Done", "5"));
        assert_eq!(
            done.toggle,
            Some(Change::FoldColumn {
                column: "done".into(),
                folded: true
            })
        );
        // Without Fold empty columns an empty stage is open and says so.
        let open = view(
            &input(&flow, &tickets),
            &ViewPrefs {
                folded_columns: Vec::new(),
                ..prefs()
            },
        );
        let validate = open.groups.iter().find(|g| g.title == "Validate").unwrap();
        assert!(!validate.folded);
        assert_eq!(validate.empty.as_deref(), Some("Nothing here"));
        assert_eq!(
            validate.toggle,
            Some(Change::FoldColumn {
                column: "validate".into(),
                folded: true
            })
        );
    }

    /// A filter counts "1/3" and shows only the rows it keeps.
    #[test]
    fn a_filter_counts_what_it_shows() {
        let flow = shop_web();
        let tickets = tickets();
        let visible: Vec<TicketKey> = vec!["SHOP-41".parse().unwrap()];
        let list = view(
            &BoardInput {
                visible: Some(&visible),
                ..input(&flow, &tickets)
            },
            &prefs(),
        );
        let backlog = list.groups.iter().find(|g| g.title == "Backlog").unwrap();
        assert_eq!(backlog.count, "1/3");
        assert_eq!(backlog.entries.len(), 1);
        let dev = list.groups.iter().find(|g| g.title == "Dev").unwrap();
        assert_eq!(dev.count, "0/3");
    }

    /// A stored Group by from before (Status, Agent) changes nothing: the List is always grouped by stage.
    #[test]
    fn a_stored_group_by_is_ignored() {
        use otaq_core::board::settings::GroupBy;
        let flow = shop_web();
        let tickets = tickets();
        let by_stage = view(&input(&flow, &tickets), &prefs());
        for group_by in [GroupBy::Status, GroupBy::Agent] {
            let old = view(
                &input(&flow, &tickets),
                &ViewPrefs {
                    group_by,
                    hide_empty_stages: true,
                    ..prefs()
                },
            );
            assert_eq!(old, by_stage);
        }
    }
}

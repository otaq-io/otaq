//! A ticket's **Plan** in the sheet and on the card (d0133; canvas `s30-01`): the Markdown section under the
//! description in Overview, written by the planning stage (Milestone plan) or Noyan, which the user reads and edits
//! like the description (d0078, `s25-04`'s rules: Save · Discard, and leaving with changes asks first).
//!
//! - **No plan** ("Most tickets have no plan"): one quiet dashed line, "No plan. Milestone plan or Noyan writes one
//!   when the work has pieces.", with **Write a plan**.
//! - **Who wrote it** ([`written_by`]): "Written by Milestone plan · 2:40 pm", "Written by Noyan · 11:05 am", "Written
//!   by Milestone plan · edited by you · 2:58 pm" (the user saved over a plan someone else wrote: the writers come
//!   from History's Plan changes, each naming its `plan_by`), then " · approved at Spec review" once a human check
//!   passed after the last write (History's Passed event).
//! - **Being written** ([`writing_now`]): the ticket is in the stage whose agent wrote the plan last, so the header
//!   says "Written by Milestone plan · writing now", the text ends with "writing the next piece…", and the card says
//!   "Plan · 2 pieces so far" (the count follows the pieces as they arrive).
//! - **On the card** ([`card_line`]): "Plan · 4 pieces", quiet; a plan with no Pieces list shows no line.
//!
//! The pieces come from core's lenient parse ([`otaq_core::domain::plan::pieces`]); each [`PieceView`] carries its
//! number, display name ("40.3", never a ticket key) and what it waits for, so a status column can sit beside them
//! (T-0541). Statuses are Otaq's own records and never part of the text.

use otaq_core::board::tickets::{EventKind, HistoryEvent, Ticket, TicketUpdate};
use otaq_core::domain::Place;
use otaq_core::domain::plan::{self, PlanBy};
use otaq_core::flow::model::Flow;
use serde::Serialize;
use ts_rs::TS;

use crate::keymap::Platform;
use crate::view_models::clock;
use crate::view_models::overview::{self, DescriptionState, LeaveDialog, SaveBar};

pub const PLAN: &str = "Plan";
/// The empty state's one line (`s30-01`, "Most tickets have no plan").
pub const NO_PLAN: &str = "No plan. Milestone plan or Noyan writes one when the work has pieces.";
pub const WRITE_A_PLAN: &str = "Write a plan";
/// Under the text while the planning stage's agent writes it (`s30-01`, "The plan, being written").
pub const WRITING_NEXT: &str = "writing the next piece\u{2026}";
/// The editor's placeholder while the Plan is empty.
pub const PLACEHOLDER: &str = "The why, then a Pieces list: \u{201c}1. Wishlist API\u{201d}, \u{201c}2. Wishlist \
                               button \u{2014} waits for 1\u{201d}";
pub const CHANGED: &str = "Plan changed";

/// One piece of the Plan's Pieces list as the sheet names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PlanPieceView")]
pub struct PieceView {
    pub number: u32,
    /// "40.3": the display name of piece 3 of SHOP-40 (audit answer 2).
    pub name: String,
    pub title: String,
    /// "waits for 1", "waits for 2, 3"; `None` when it waits for nothing.
    pub waits: Option<String>,
}

/// The empty state: the dashed line and its button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PlanEmpty")]
pub struct Empty {
    pub line: String,
    pub write: String,
}

/// Overview's PLAN section (`PlanSectionView` on the page).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PlanSectionView")]
pub struct PlanSection {
    /// "Plan" (the page sets it in capitals, as every section label).
    pub label: String,
    /// The Plan as written (Markdown); empty without one.
    pub markdown: String,
    /// No plan: the quiet line and Write a plan.
    pub empty: Option<Empty>,
    /// "Written by Milestone plan · 2:40 pm"; `None` without a plan.
    pub written_by: Option<String>,
    /// The header while the editor is open: "Written by Milestone plan · editing", "Editing" for a new plan.
    pub editing: String,
    /// The planning stage's agent is writing it now.
    pub writing_now: bool,
    /// "writing the next piece…" under the text while it is written.
    pub writing_next: Option<String>,
    /// The Pieces list as parsed (empty when there is none or it doesn't parse).
    pub pieces: Vec<PieceView>,
    pub click_to_edit: String,
    pub placeholder: String,
    /// "Plan changed · Discard · Save ⌘S".
    pub bar: SaveBar,
    pub leave: LeaveDialog,
    /// Once the ticket split its work (d0133, `s30-03`): each piece's status as Otaq records it, in its own column
    /// beside the text ("Status · from Otaq"), never part of the Plan; set by the service's helpers
    /// ([`crate::view_models::pieces::plan_statuses`]).
    pub statuses: Option<crate::view_models::pieces::PlanStatuses>,
}

/// What the section knows beyond the ticket: the name of the stage the ticket is in, its History, and the time.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    /// The name of the stage the ticket is in (not waiting before it); `None` outside a stage.
    pub stage: Option<&'a str>,
    /// The ticket's History, in any order.
    pub events: &'a [HistoryEvent],
    pub now_ms: i64,
}

/// The name of the stage `ticket` is in, from its project's flow; `None` while it waits before one, or outside.
pub fn stage_in<'a>(ticket: &Ticket, flow: &'a Flow) -> Option<&'a str> {
    match &ticket.place {
        Place::InStage(id) => flow.stage(id).map(|s| s.name.as_str()),
        _ => None,
    }
}

/// The planning stage's agent is writing the Plan: the ticket is in the stage that wrote it last.
pub fn writing_now(ticket: &Ticket, stage: Option<&str>) -> bool {
    match (&ticket.plan_by, stage) {
        (Some(PlanBy::Stage(by)), Some(stage)) => !ticket.plan.is_empty() && by == stage,
        _ => false,
    }
}

fn pieces_count(n: usize) -> String {
    match n {
        1 => "1 piece".to_owned(),
        n => format!("{n} pieces"),
    }
}

/// The card's quiet line: "Plan · 4 pieces", "Plan · 2 pieces so far" while it is written; none without pieces.
pub fn card_line(ticket: &Ticket, stage: Option<&str>) -> Option<String> {
    let n = plan::pieces(&ticket.plan).len();
    if n == 0 {
        return None;
    }
    let so_far = if writing_now(ticket, stage) {
        " so far"
    } else {
        ""
    };
    Some(format!("{PLAN} \u{b7} {}{so_far}", pieces_count(n)))
}

/// The Plan's writers in History, oldest first: `Some(writer)` for a write, `None` for a Plan cleared.
fn writes(events: &[HistoryEvent]) -> Vec<Option<PlanBy>> {
    let mut plan_events: Vec<&HistoryEvent> = events
        .iter()
        .filter(|e| match e.kind {
            EventKind::Created => e.payload.get("plan_by").is_some(),
            EventKind::Updated => e
                .payload
                .get("changes")
                .and_then(|c| c.as_array())
                .is_some_and(|c| {
                    c.iter()
                        .any(|c| c.get("field").and_then(|f| f.as_str()) == Some("plan"))
                }),
            _ => false,
        })
        .collect();
    plan_events.sort_by_key(|e| e.at);
    plan_events
        .into_iter()
        .map(|e| {
            // A Plan cleared has no writer.
            let cleared = e.kind == EventKind::Updated
                && e.payload["changes"].as_array().is_some_and(|c| {
                    c.iter().any(|c| {
                        c["field"].as_str() == Some("plan") && c["after"].as_str() == Some("")
                    })
                });
            if cleared {
                return None;
            }
            e.payload
                .get("plan_by")
                .and_then(|b| b.as_str())
                .and_then(|b| b.parse().ok())
        })
        .collect()
}

/// Who wrote the Plan the user last saved over: the last writer other than the user since it was last cleared.
fn original_writer(events: &[HistoryEvent]) -> Option<PlanBy> {
    let mut writer = None;
    for write in writes(events) {
        match write {
            None => writer = None,
            Some(PlanBy::User(_)) => {}
            Some(by) => writer = Some(by),
        }
    }
    writer
}

/// "2:40 pm" today, "Oct 12, 2:40 pm" another day.
fn at_label(at: i64, now: i64) -> String {
    let (Some(t), Some(today)) = (
        otaq_core::local_time::local(at),
        otaq_core::local_time::local(now),
    ) else {
        return String::new();
    };
    let time = clock::clock_12h(t.hour, t.minute);
    if (t.year, t.month, t.day) == (today.year, today.month, today.day) {
        time
    } else {
        format!("{}, {time}", clock::month_day(&t))
    }
}

/// "Written by Milestone plan", "Written by Milestone plan · edited by you", "Written by you": who, without when.
fn who_wrote(ticket: &Ticket, events: &[HistoryEvent]) -> Option<String> {
    let by = ticket.plan_by.as_ref()?;
    Some(match (by, original_writer(events)) {
        (PlanBy::User(_), Some(original)) => {
            format!("Written by {} \u{b7} edited by you", original.label())
        }
        (by, _) => format!("Written by {}", by.label()),
    })
}

/// The human check the Plan passed: the latest Passed event after its last write ("Spec review").
fn approved_at(ticket: &Ticket, events: &[HistoryEvent]) -> Option<String> {
    let written = ticket.plan_at?;
    events
        .iter()
        .filter(|e| e.kind == EventKind::Passed && e.at >= written)
        .max_by_key(|e| e.at)
        .and_then(|e| e.payload.get("stage")?.as_str().map(str::to_owned))
}

/// The header: "Written by Milestone plan · 2:40 pm", "… · edited by you · 2:58 pm", "… · writing now", "… · approved
/// at Spec review". `None` without a plan.
pub fn written_by(ticket: &Ticket, facts: &Facts) -> Option<String> {
    if ticket.plan.is_empty() {
        return None;
    }
    let who = who_wrote(ticket, facts.events)?;
    if writing_now(ticket, facts.stage) {
        return Some(format!("{who} \u{b7} writing now"));
    }
    let mut out = who;
    if let Some(at) = ticket.plan_at {
        out.push_str(&format!(" \u{b7} {}", at_label(at, facts.now_ms)));
    }
    if let Some(stage) = approved_at(ticket, facts.events) {
        out.push_str(&format!(" \u{b7} approved at {stage}"));
    }
    Some(out)
}

/// The Pieces list, named for the ticket ("40.3").
pub fn pieces(ticket: &Ticket) -> Vec<PieceView> {
    plan::pieces(&ticket.plan)
        .into_iter()
        .map(|p| PieceView {
            number: p.number,
            name: plan::piece_name(&ticket.key, p.number),
            waits: (!p.waits_for.is_empty()).then(|| {
                let numbers: Vec<String> = p.waits_for.iter().map(u32::to_string).collect();
                format!("waits for {}", numbers.join(", "))
            }),
            title: p.title,
        })
        .collect()
}

/// The tab row's bar while the Plan differs from what is saved: "Plan changed · Discard · Save ⌘S".
pub fn save_bar(platform: Platform) -> SaveBar {
    SaveBar {
        changed: CHANGED.to_owned(),
        ..overview::save_bar(DescriptionState::Changed, platform)
            .expect("a changed text has the bar")
    }
}

/// The question before leaving a changed Plan (T-0511's rules: Save · Discard changes · Keep editing).
pub fn leave_dialog(key: &str) -> LeaveDialog {
    LeaveDialog {
        title: "Discard your plan changes?".to_owned(),
        body: format!("{key}\u{2019}s plan has changes you haven\u{2019}t saved."),
        ..overview::leave_dialog(key)
    }
}

/// What Save sends for the Plan as typed (`editor.save`, target `plan`): only the Plan; an untouched one (or an empty
/// one still empty) is never saved. An empty text clears the Plan.
pub fn update(ticket: &Ticket, text: &str) -> Option<TicketUpdate> {
    let unchanged = text == ticket.plan || (text.trim().is_empty() && ticket.plan.is_empty());
    (!unchanged).then(|| TicketUpdate {
        plan: Some(text.to_owned()),
        ..TicketUpdate::default()
    })
}

pub fn section(ticket: &Ticket, facts: &Facts, platform: Platform) -> PlanSection {
    let key = ticket.key.to_string();
    let has_plan = !ticket.plan.is_empty();
    let writing = has_plan && writing_now(ticket, facts.stage);
    PlanSection {
        label: PLAN.to_owned(),
        markdown: ticket.plan.clone(),
        empty: (!has_plan).then(|| Empty {
            line: NO_PLAN.to_owned(),
            write: WRITE_A_PLAN.to_owned(),
        }),
        written_by: written_by(ticket, facts),
        editing: match who_wrote(ticket, facts.events).filter(|_| has_plan) {
            Some(who) => format!("{who} \u{b7} editing"),
            None => "Editing".to_owned(),
        },
        writing_now: writing,
        writing_next: writing.then(|| WRITING_NEXT.to_owned()),
        pieces: pieces(ticket),
        click_to_edit: overview::CLICK_TO_EDIT.to_owned(),
        placeholder: PLACEHOLDER.to_owned(),
        bar: save_bar(platform),
        leave: leave_dialog(&key),
        statuses: None,
    }
}

/// The gallery's Plan sections, as `s30-01` draws them.
pub mod fixtures {
    use otaq_core::board::tickets::{EventKind, HistoryEvent, Ticket};
    use otaq_core::domain::plan::PlanBy;
    use otaq_core::domain::{Actor, Place, Status};
    use serde_json::json;

    use crate::view_models::card_fixtures;

    /// SHOP-40's plan (`s30-01`, "Editable like the description").
    pub const WISHLIST: &str = "Shoppers lose items they meant to buy. A wishlist keeps them across visits, and a \
link lets someone else see it, signed out.\n\n## Pieces\n\n1. Wishlist API\n   Store and read a shopper\u{2019}s \
wishlist, per account.\n2. Wishlist button \u{2014} waits for 1\n   A heart on every product card adds or removes the \
item.\n3. Wishlist page \u{2014} waits for 1\n   /wishlist lists the saved items, with an empty state.\n4. Share link \
\u{2014} waits for 2, 3\n   A read-only link to the wishlist that works signed out.\n";

    /// SHOP-55's plan while Milestone plan writes it (`s30-01`, "The plan, being written").
    pub const GIFT_CARDS: &str = "Shoppers ask for gift cards before the holidays.\n\n## Pieces\n\n1. Gift card \
model\n   Codes, balances and expiry, per shop.\n2. Redeem at checkout \u{2014} waits for 1\n   Enter a code; the \
balance pays first, the card the rest.\n";

    /// A ticket with `plan` written by `by` at `at`, in `place`.
    pub fn ticket(key: &str, plan: &str, by: Option<PlanBy>, at: i64, place: Place) -> Ticket {
        let mut t = card_fixtures::ticket(key, "Wishlist", Status::Working);
        t.plan = plan.to_owned();
        t.plan_at = by.as_ref().map(|_| at);
        t.plan_by = by;
        t.place = place;
        t
    }

    /// A Plan change in History by `by` at `at`.
    pub fn write(by: &PlanBy, at: i64, before: &str, after: &str) -> HistoryEvent {
        let actor = match by {
            PlanBy::User(name) => Actor::User(name.clone()),
            PlanBy::Noyan(id) => Actor::ProjectAgent(id.clone()),
            PlanBy::Stage(_) => Actor::Agent("SHOP-40".parse().expect("key")),
        };
        // A Plan cleared names no writer (core's `plan_by` goes with it).
        let mut payload = json!({"changes": [{"field": "plan", "before": before, "after": after}]});
        if !after.is_empty() {
            payload["plan_by"] = json!(by.to_string());
        }
        HistoryEvent {
            kind: EventKind::Updated,
            actor,
            at,
            payload,
        }
    }

    /// The human check `stage` passed at `at`.
    pub fn passed(stage: &str, at: i64) -> HistoryEvent {
        HistoryEvent {
            kind: EventKind::Passed,
            actor: Actor::User("me".into()),
            at,
            payload: json!({"stage": stage, "stage_id": stage.to_lowercase().replace(' ', "-")}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{GIFT_CARDS, WISHLIST, passed, ticket, write};
    use super::*;
    use crate::view_models::overview::{
        Leave, LeaveChoice, LeaveStep, after_save, leave_step, leaving, state,
    };

    const MIN: i64 = 60_000;

    fn milestone() -> PlanBy {
        PlanBy::Stage("Milestone plan".into())
    }

    fn you() -> PlanBy {
        PlanBy::User("me".into())
    }

    /// Today at `hour:minute` local time, and the clock label the header gives it.
    fn today(hour: u32, minute: u32) -> (i64, String) {
        let now = 1_791_547_200_000;
        let at = otaq_core::local_time::on_same_day_at(now, hour, minute).unwrap();
        (at, clock::clock_12h(hour, minute))
    }

    fn in_stage(stage: &str) -> Place {
        Place::InStage(stage.into())
    }

    /// `s30-01` "Most tickets have no plan": the quiet line with Write a plan; no header, no pieces, no card line.
    #[test]
    fn the_empty_state() {
        let t = ticket("SHOP-33", "", None, 0, in_stage("dev"));
        let (now, _) = today(15, 0);
        let s = section(
            &t,
            &Facts {
                stage: Some("Dev"),
                events: &[],
                now_ms: now,
            },
            Platform::Mac,
        );
        let empty = s.empty.expect("no plan");
        assert_eq!(
            empty.line,
            "No plan. Milestone plan or Noyan writes one when the work has pieces."
        );
        assert_eq!(empty.write, "Write a plan");
        assert_eq!(s.label, "Plan");
        assert_eq!(s.written_by, None);
        assert_eq!(s.editing, "Editing");
        assert!(s.pieces.is_empty() && !s.writing_now && s.writing_next.is_none());
        assert_eq!(card_line(&t, Some("Dev")), None);
    }

    /// A plan with pieces: the Markdown as written, the pieces with their display names and waits.
    #[test]
    fn a_plan_with_pieces() {
        let (at, _) = today(14, 40);
        let t = ticket(
            "SHOP-40",
            WISHLIST,
            Some(milestone()),
            at,
            in_stage("spec-review"),
        );
        let s = section(
            &t,
            &Facts {
                stage: Some("Spec review"),
                events: &[write(&milestone(), at, "", WISHLIST)],
                now_ms: at + MIN,
            },
            Platform::Mac,
        );
        assert_eq!(s.empty, None);
        assert_eq!(s.markdown, WISHLIST);
        let rows: Vec<(&str, &str, Option<&str>)> = s
            .pieces
            .iter()
            .map(|p| (p.name.as_str(), p.title.as_str(), p.waits.as_deref()))
            .collect();
        assert_eq!(
            rows,
            [
                ("40.1", "Wishlist API", None),
                ("40.2", "Wishlist button", Some("waits for 1")),
                ("40.3", "Wishlist page", Some("waits for 1")),
                ("40.4", "Share link", Some("waits for 2, 3")),
            ]
        );
        assert_eq!(s.editing, "Written by Milestone plan \u{b7} editing");
        assert_eq!(s.click_to_edit, "Click to edit");
    }

    /// `s30-01` "Who wrote it": Milestone plan, Noyan, and Milestone plan edited by you, each with its time.
    #[test]
    fn each_written_by_header() {
        let (planned, planned_at) = today(14, 40);
        let (edited, edited_at) = today(14, 58);
        let (filed, filed_at) = today(11, 5);
        let now = edited + MIN;
        let facts = |events| Facts {
            stage: Some("Spec review"),
            events,
            now_ms: now,
        };

        let by_stage = ticket(
            "SHOP-40",
            WISHLIST,
            Some(milestone()),
            planned,
            in_stage("spec-review"),
        );
        let events = [write(&milestone(), planned, "", WISHLIST)];
        assert_eq!(
            written_by(&by_stage, &facts(&events)),
            Some(format!("Written by Milestone plan \u{b7} {planned_at}"))
        );

        let noyan = PlanBy::Noyan("noyan-1".into());
        let by_noyan = ticket(
            "SHOP-41",
            WISHLIST,
            Some(noyan.clone()),
            filed,
            Place::Waiting(None),
        );
        let events = [write(&noyan, filed, "", WISHLIST)];
        assert_eq!(
            written_by(&by_noyan, &facts(&events)),
            Some(format!("Written by Noyan \u{b7} {filed_at}"))
        );

        let edited_plan = WISHLIST.replace(
            "works signed out.\n",
            "works signed out, no prices out of stock.\n",
        );
        let by_you = ticket(
            "SHOP-40",
            &edited_plan,
            Some(you()),
            edited,
            in_stage("spec-review"),
        );
        let events = [
            write(&milestone(), planned, "", WISHLIST),
            write(&you(), edited, WISHLIST, &edited_plan),
        ];
        assert_eq!(
            written_by(&by_you, &facts(&events)),
            Some(format!(
                "Written by Milestone plan \u{b7} edited by you \u{b7} {edited_at}"
            ))
        );
        assert_eq!(
            section(&by_you, &facts(&events), Platform::Mac).editing,
            "Written by Milestone plan \u{b7} edited by you \u{b7} editing"
        );

        // Your own plan, nobody else's before it: "Written by you".
        let yours = ticket("SHOP-42", WISHLIST, Some(you()), edited, Place::Backlog);
        let events = [write(&you(), edited, "", WISHLIST)];
        assert_eq!(
            written_by(&yours, &facts(&events)),
            Some(format!("Written by you \u{b7} {edited_at}"))
        );

        // Cleared, then written by you: the earlier writer no longer counts.
        let events = [
            write(&milestone(), planned, "", WISHLIST),
            write(&you(), planned + MIN, WISHLIST, ""),
            write(&you(), edited, "", WISHLIST),
        ];
        assert_eq!(
            written_by(&yours, &facts(&events)),
            Some(format!("Written by you \u{b7} {edited_at}"))
        );
    }

    /// Another day than today names the day.
    #[test]
    fn an_older_plan_names_its_day() {
        let (at, at_label_today) = today(14, 40);
        let now = at + 3 * 86_400_000;
        let t = otaq_core::local_time::local(at).unwrap();
        assert_eq!(
            super::at_label(at, now),
            format!("{}, {at_label_today}", clock::month_day(&t))
        );
    }

    /// "approved at Spec review": a human check passed after the last write; one before it doesn't count.
    #[test]
    fn approved_at_the_human_check_that_followed() {
        let (planned, planned_at) = today(14, 40);
        let t = ticket(
            "SHOP-40",
            WISHLIST,
            Some(milestone()),
            planned,
            in_stage("dev"),
        );
        let events = [
            passed("Spec review", planned - 10 * MIN),
            write(&milestone(), planned, "", WISHLIST),
            passed("Spec review", planned + 20 * MIN),
        ];
        let facts = Facts {
            stage: Some("Dev"),
            events: &events,
            now_ms: planned + 30 * MIN,
        };
        assert_eq!(
            written_by(&t, &facts),
            Some(format!(
                "Written by Milestone plan \u{b7} {planned_at} \u{b7} approved at Spec review"
            ))
        );
        let before_only = [
            passed("Spec review", planned - 10 * MIN),
            write(&milestone(), planned, "", WISHLIST),
        ];
        assert_eq!(
            written_by(
                &t,
                &Facts {
                    events: &before_only,
                    ..facts
                }
            ),
            Some(format!("Written by Milestone plan \u{b7} {planned_at}"))
        );
    }

    /// `s30-01` "The plan, being written": in Milestone plan, the stage that writes it: "writing now", "writing the
    /// next piece…", and the card's "so far".
    #[test]
    fn being_written() {
        let (at, _) = today(14, 40);
        let t = ticket(
            "SHOP-55",
            GIFT_CARDS,
            Some(milestone()),
            at,
            in_stage("milestone-plan"),
        );
        let facts = Facts {
            stage: Some("Milestone plan"),
            events: &[],
            now_ms: at + MIN,
        };
        let s = section(&t, &facts, Platform::Mac);
        assert!(s.writing_now);
        assert_eq!(
            s.written_by.as_deref(),
            Some("Written by Milestone plan \u{b7} writing now")
        );
        assert_eq!(
            s.writing_next.as_deref(),
            Some("writing the next piece\u{2026}")
        );
        assert_eq!(
            card_line(&t, Some("Milestone plan")).as_deref(),
            Some("Plan \u{b7} 2 pieces so far")
        );
        // A third piece arrives: the count follows.
        let mut more = t.clone();
        more.plan.push_str("3. Refunds \u{2014} waits for 2\n");
        assert_eq!(
            card_line(&more, Some("Milestone plan")).as_deref(),
            Some("Plan \u{b7} 3 pieces so far")
        );
        // Waiting before the stage, or in another stage: not being written.
        assert!(!writing_now(&t, None));
        assert!(!writing_now(&t, Some("Spec review")));
    }

    /// The card's line: "Plan · 4 pieces" with pieces, "1 piece" for one, none for a plan without a Pieces list.
    #[test]
    fn the_card_line_and_its_absence() {
        let t = ticket(
            "SHOP-40",
            WISHLIST,
            Some(milestone()),
            1,
            in_stage("spec-review"),
        );
        assert_eq!(
            card_line(&t, Some("Spec review")).as_deref(),
            Some("Plan \u{b7} 4 pieces")
        );
        let one = ticket(
            "SHOP-40",
            "Why.\n\n**Pieces**\n\n1. All of it\n",
            Some(you()),
            1,
            Place::Backlog,
        );
        assert_eq!(
            card_line(&one, None).as_deref(),
            Some("Plan \u{b7} 1 piece")
        );
        let prose = ticket(
            "SHOP-58",
            "Rate-limit by IP, then by account.",
            Some(milestone()),
            1,
            Place::Backlog,
        );
        assert_eq!(card_line(&prose, None), None);
    }

    /// Editing like the description: the plan's own bar and leave question, with the description's rules (T-0511).
    #[test]
    fn save_and_the_leave_dialog_for_the_plan() {
        let bar = save_bar(Platform::Mac);
        assert_eq!(
            (
                bar.changed.as_str(),
                bar.discard.as_str(),
                bar.save.as_str(),
                bar.keys.as_str()
            ),
            ("Plan changed", "Discard", "Save", "\u{2318}S")
        );
        assert_eq!(save_bar(Platform::WinLinux).keys, "Ctrl+S");
        let d = leave_dialog("SHOP-40");
        assert_eq!(d.title, "Discard your plan changes?");
        assert_eq!(
            d.body,
            "SHOP-40\u{2019}s plan has changes you haven\u{2019}t saved."
        );
        assert_eq!(
            (d.save.as_str(), d.discard.as_str(), d.keep.as_str()),
            ("Save", "Discard changes", "Keep editing")
        );
        // The same rules as the description: only changes ask, Save saves then goes on, a failed save stays.
        let edited = format!("{WISHLIST}5. Emails \u{2014} waits for 4\n");
        assert_eq!(leaving(state(WISHLIST, Some(WISHLIST)), false), Leave::Go);
        assert_eq!(leaving(state(WISHLIST, Some(&edited)), false), Leave::Ask);
        assert_eq!(leave_step(LeaveChoice::Save), LeaveStep::SaveFirst);
        assert_eq!(after_save(true), LeaveStep::Go);
        assert_eq!(after_save(false), LeaveStep::Stay);
        // Save sends only the Plan; untouched, nothing.
        let t = ticket(
            "SHOP-40",
            WISHLIST,
            Some(milestone()),
            1,
            in_stage("spec-review"),
        );
        assert_eq!(update(&t, WISHLIST), None);
        let sent = update(&t, &edited).expect("changed");
        assert_eq!(sent.plan.as_deref(), Some(edited.as_str()));
        assert_eq!((sent.body, sent.title), (None, None));
        let none = ticket("SHOP-33", "", None, 0, Place::Backlog);
        assert_eq!(update(&none, "\n"), None);
        assert_eq!(update(&t, "").and_then(|u| u.plan).as_deref(), Some(""));
    }

    /// Activity lists every Plan change (d0109): written by the stage, edited by you (its before / after), cleared;
    /// under the Description filter.
    #[test]
    fn activity_rows() {
        use crate::view_models::history::{self, Filter, Kind, LineKind};
        use crate::view_models::ticket_comments::Mark;
        let edited = WISHLIST.replace(
            "works signed out.\n",
            "works signed out, no prices out of stock.\n",
        );
        let events = [
            write(&milestone(), 1_000, "", WISHLIST),
            write(&you(), 2_000, WISHLIST, &edited),
            write(&you(), 3_000, &edited, ""),
        ];
        let rows = history::oldest_first(history::rows(&events, &[], &[]));
        let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Milestone plan wrote the plan \u{b7} 4 pieces",
                "You edited the plan \u{b7} 1 line changed",
                "You cleared the plan",
            ]
        );
        assert!(rows.iter().all(|r| r.kind == Kind::Description));
        assert_eq!(history::filtered(&rows, Filter::Description).len(), 3);
        assert_eq!(
            (rows[0].mark.clone(), rows[1].mark.clone()),
            (Mark::Agent, Mark::You)
        );
        let opens = rows[1].opens.as_ref().expect("a before / after");
        assert_eq!(opens.label, "PLAN");
        assert_eq!(opens.caption.as_deref(), Some("Before / after"));
        let added: Vec<&str> = opens
            .lines
            .iter()
            .filter(|l| l.kind == LineKind::Added)
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(
            added,
            ["   A read-only link to the wishlist that works signed out, no prices out of stock."]
        );
    }
}

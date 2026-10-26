//! The Apply now window (APN-02, d0108 "A flow change waits for a quiet board" with its Apply now of 2026-10-21;
//! *11 → Row · Apply now* `s11-17`/`s11-18`) over the service's plan (`flow.apply_now_plan`,
//! [`otaq_core::flow::apply_now`]): "Apply the flow change now?", the change's summary, **Moves** (one line per ticket
//! in a removed stage, each with a picker of Backlog, Ready for Dev and the new flow's stages, on its default),
//! **Keep their place** (collapsed to one line past 3: "SHOP-15 · in Dev · keeps running, and 6 more"), one hint line
//! for the current picks ("Back to Dev: Dev resumes SHOP-19’s earlier session.", or the forward one), Cancel and
//! Apply now. The picks are the page's until Apply now; a pick that isn't one of its ticket's choices any more falls
//! back to the default. A stale answer (`refreshed`) redraws the window with a line naming what changed
//! ([`changed_line`], from the plan the window showed, [`Basis`], and the fresh one).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use otaq_core::board::moves::{DropOn, DropTarget};
use otaq_core::domain::TicketKey;
use otaq_core::flow::apply_now::{
    ApplyNowPlan, FORWARD_HINT, KeptTicket, MoveChoice, PlannedMove, Refreshed, Way,
};
use otaq_core::flow::{CheckBy, Delivers, Flow, StageKind};

use crate::icons::IconName;
use crate::view_models::menu::MenuItem;

/// The window's title.
pub const TITLE: &str = "Apply the flow change now?";
/// The section over the moves.
pub const MOVES: &str = "Moves";
/// The confirm button (and the banner's).
pub const APPLY_NOW: &str = "Apply now";
pub const CANCEL: &str = "Cancel";
/// Keep their place is one line past this many tickets.
pub const KEEP_SHOWN: usize = 3;

/// The picks so far: a ticket's key → the id of its chosen entry ([`choice_id`]).
pub type Picks = BTreeMap<String, String>;

/// The window as drawn.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ApplyNowView")]
pub struct View {
    pub project: String,
    /// "Apply the flow change now?".
    pub title: String,
    /// "Removes Product spec and Security from shop-web’s flow."
    pub summary: String,
    /// After a stale Apply now: what changed on the board ("The board changed: SHOP-41 entered Product spec."). Shown
    /// above the moves.
    pub changed: Option<String>,
    /// "Moves"; empty moves draw no section (a change that removes no stage).
    pub moves_label: String,
    pub moves: Vec<MoveRow>,
    /// "Keep their place · 7"; `None` with no ticket to keep.
    pub keep_label: Option<String>,
    /// Every kept ticket, in key order: drawn when there are at most 3, or once opened.
    pub keep: Vec<KeepRow>,
    /// Past 3, the one line it starts collapsed to ("SHOP-15 · in Dev · keeps running, and 6 more").
    pub keep_collapsed: Option<String>,
    /// The hint line for the current picks.
    pub hint: Option<String>,
    pub cancel: String,
    pub apply: String,
    /// The plan's stamp, sent back with Apply now.
    pub stamp: String,
    /// What the window shows, sent back with Apply now, so a stale answer can say what changed.
    pub basis: Basis,
}

/// One ticket in a removed stage.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ApplyNowMove")]
pub struct MoveRow {
    pub ticket: String,
    pub title: String,
    /// "in Product spec (removed) · agent will stop", "in Security (removed) · failed ×3".
    pub line: String,
    /// The current pick's id ([`choice_id`]) and name, as the picker shows it.
    pub picked: String,
    pub picked_name: String,
    pub picked_icon: IconName,
    /// The picker's entries: Backlog, Ready for Dev, then the new flow's stages, the pick checked.
    pub choices: Vec<MenuItem>,
}

/// One ticket that keeps its place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ApplyNowKeep")]
pub struct KeepRow {
    pub ticket: String,
    pub title: String,
    /// "in Dev · keeps running", "in Review · keeps its place".
    pub line: String,
}

/// What the window showed: each ticket with its stage and whether its agent runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "ApplyNowBasis")]
pub struct Basis {
    pub summary: String,
    pub moves: Vec<BasisTicket>,
    pub keep: Vec<BasisTicket>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "ApplyNowBasisTicket")]
pub struct BasisTicket {
    pub ticket: String,
    pub stage: String,
    pub running: bool,
}

/// A choice's id as the page sends it back: `backlog`, `ready_for_dev` or `stage:<id>`.
pub fn choice_id(to: &DropOn) -> String {
    match to {
        DropOn::Board(target) => target.to_string(),
        DropOn::Stage { stage } => format!("stage:{stage}"),
    }
}

/// The choice an id stands for ([`choice_id`]'s reverse).
pub fn choice_of(id: &str) -> Option<DropOn> {
    match id.strip_prefix("stage:") {
        Some(stage) if !stage.is_empty() => Some(DropOn::stage(stage)),
        Some(_) => None,
        None => match id {
            "backlog" => Some(DropTarget::Backlog.into()),
            "ready_for_dev" => Some(DropTarget::ReadyForDev.into()),
            _ => None,
        },
    }
}

/// The choice picked for `mv`: the page's pick when it is one of its choices, else the default.
pub fn picked<'a>(mv: &'a PlannedMove, picks: &Picks) -> Option<&'a MoveChoice> {
    picks
        .get(&mv.ticket.to_string())
        .and_then(|id| choice_of(id))
        .and_then(|to| mv.choice(&to))
        .or_else(|| mv.choice(&mv.default))
}

/// The window for `plan` with the user's picks; `flow` (the new flow) gives the picker's stage icons.
pub fn view(
    plan: &ApplyNowPlan,
    flow: Option<&Flow>,
    picks: &Picks,
    changed: Option<String>,
) -> View {
    let moves = plan
        .moves
        .iter()
        .map(|mv| move_row(mv, flow, picks))
        .collect();
    let keep: Vec<KeepRow> = plan.keep.iter().map(keep_row).collect();
    let keep_collapsed = (keep.len() > KEEP_SHOWN).then(|| {
        let first = &keep[0];
        format!(
            "{} \u{b7} {}, and {} more",
            first.ticket,
            first.line,
            keep.len() - 1
        )
    });
    View {
        project: plan.project.clone(),
        title: TITLE.to_owned(),
        summary: sentence(&plan.summary),
        changed,
        moves_label: MOVES.to_owned(),
        moves,
        keep_label: (!keep.is_empty()).then(|| format!("Keep their place \u{b7} {}", keep.len())),
        keep,
        keep_collapsed,
        hint: hint(plan, picks),
        cancel: CANCEL.to_owned(),
        apply: APPLY_NOW.to_owned(),
        stamp: plan.stamp.clone(),
        basis: basis(plan),
    }
}

/// `text` ending in a full stop.
fn sentence(text: &str) -> String {
    match text.ends_with(['.', '!', '?']) || text.is_empty() {
        true => text.to_owned(),
        false => format!("{text}."),
    }
}

/// "in Product spec (removed) · agent will stop", "waiting for Product spec (removed)", "in Security (removed) · failed
/// ×3".
pub fn move_line(mv: &PlannedMove) -> String {
    let at = match mv.waiting {
        true => format!("waiting for {} (removed)", mv.stage_name),
        false => format!("in {} (removed)", mv.stage_name),
    };
    match (mv.agent_will_stop, &mv.state) {
        (true, _) => format!("{at} \u{b7} agent will stop"),
        (false, Some(state)) => format!("{at} \u{b7} {state}"),
        (false, None) => at,
    }
}

fn keep_line(kept: &KeptTicket) -> String {
    let what = match kept.keeps_running {
        true => "keeps running",
        false => "keeps its place",
    };
    format!("in {} \u{b7} {what}", kept.stage_name)
}

fn keep_row(kept: &KeptTicket) -> KeepRow {
    KeepRow {
        ticket: kept.ticket.to_string(),
        title: kept.title.clone(),
        line: keep_line(kept),
    }
}

/// A choice's icon, as its column's (*11 → Row · Apply now*'s open picker): Backlog a dashed circle, Ready for Dev an
/// hourglass, an Agent stage a bot, an agent check a shield, a human check a person, a Delivery a PR or a merge.
fn choice_icon(to: &DropOn, flow: Option<&Flow>) -> IconName {
    let stage = match to {
        DropOn::Board(DropTarget::Backlog) => return IconName::CircleDashed,
        DropOn::Board(_) => return IconName::Hourglass,
        DropOn::Stage { stage } => flow.and_then(|f| f.stage(stage)),
    };
    match stage.map(|s| (s.kind, s.delivery.map(|d| d.delivers))) {
        Some((StageKind::Check { by: CheckBy::Agent }, _)) => IconName::ShieldCheck,
        Some((StageKind::Check { by: CheckBy::You }, _)) => IconName::UserCheck,
        Some((StageKind::Delivery, Some(Delivers::Pr))) => IconName::GitPullRequest,
        Some((StageKind::Delivery, _)) => IconName::GitMerge,
        _ => IconName::Bot,
    }
}

fn move_row(mv: &PlannedMove, flow: Option<&Flow>, picks: &Picks) -> MoveRow {
    let pick = picked(mv, picks);
    let picked_to = pick.map_or_else(|| mv.default.clone(), |c| c.to.clone());
    // A line between Ready for Dev and the flow's stages.
    let first_stage = mv
        .choices
        .iter()
        .position(|c| matches!(c.to, DropOn::Stage { .. }));
    let choices = mv
        .choices
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut item =
                MenuItem::new(choice_id(&c.to), c.name.clone()).icon(choice_icon(&c.to, flow));
            item.checked = c.to == picked_to;
            item.separated = i > 0 && Some(i) == first_stage;
            item
        })
        .collect();
    MoveRow {
        ticket: mv.ticket.to_string(),
        title: mv.title.clone(),
        line: move_line(mv),
        picked: choice_id(&picked_to),
        picked_name: pick.map_or_else(String::new, |c| c.name.clone()),
        picked_icon: choice_icon(&picked_to, flow),
        choices,
    }
}

/// The one hint line for the current picks: the first pick back to an agent's stage ("Back to Dev: Dev resumes
/// SHOP-19’s earlier session."), else any forward pick ("Moving forward skips the stages in between, as on the card
/// (“Skipped Validate · by you”)."), naming the first stage that pick skips.
pub fn hint(plan: &ApplyNowPlan, picks: &Picks) -> Option<String> {
    let chosen: Vec<(&PlannedMove, &MoveChoice)> = plan
        .moves
        .iter()
        .filter_map(|mv| picked(mv, picks).map(|c| (mv, c)))
        .collect();
    let back = chosen.iter().find_map(|(_, c)| match (c.way, &c.hint) {
        (Way::Back, Some(hint)) => Some(format!("Back to {}: {}.", c.name, hint)),
        _ => None,
    });
    back.or_else(|| {
        chosen
            .iter()
            .find(|(_, c)| c.way == Way::Forward)
            .map(|(mv, c)| {
                let skipped = mv
                    .choices
                    .iter()
                    .take_while(|o| o.to != c.to)
                    .find(|o| matches!(o.way, Way::Next | Way::Forward));
                match skipped {
                    Some(s) => format!(
                        "{FORWARD_HINT} (\u{201c}Skipped {} \u{b7} by you\u{201d}).",
                        s.name
                    ),
                    None => format!("{FORWARD_HINT}."),
                }
            })
    })
}

/// What a window on `plan` shows, for [`changed_line`] later.
pub fn basis(plan: &ApplyNowPlan) -> Basis {
    Basis {
        summary: plan.summary.clone(),
        moves: plan
            .moves
            .iter()
            .map(|m| BasisTicket {
                ticket: m.ticket.to_string(),
                stage: m.stage_name.clone(),
                running: m.agent_will_stop,
            })
            .collect(),
        keep: plan
            .keep
            .iter()
            .map(|k| BasisTicket {
                ticket: k.ticket.to_string(),
                stage: k.stage_name.clone(),
                running: k.keeps_running,
            })
            .collect(),
    }
}

/// The line a refreshed window shows (APN-04): what changed between the plan it showed (`before`) and the fresh one,
/// one part per ticket in key order ("The board changed: SHOP-41 entered Product spec · SHOP-15 is no longer
/// working."); a pick that isn't a choice any more names its ticket and where it starts now.
pub fn changed_line(before: &Basis, after: &ApplyNowPlan, why: &Refreshed) -> String {
    if let Refreshed::NotAChoice { ticket } = why {
        let start = after
            .moves
            .iter()
            .find(|m| m.ticket == *ticket)
            .and_then(|m| m.choice(&m.default));
        return match start {
            Some(c) => format!(
                "The pick for {ticket} isn\u{2019}t a choice any more; it starts on {}.",
                c.name
            ),
            None => format!("{ticket} has nothing to place any more."),
        };
    }
    let now = basis(after);
    let find = |list: &[BasisTicket], key: &str| list.iter().find(|t| t.ticket == key).cloned();
    let mut keys: Vec<TicketKey> = before
        .moves
        .iter()
        .chain(&before.keep)
        .chain(&now.moves)
        .chain(&now.keep)
        .filter_map(|t| t.ticket.parse().ok())
        .collect();
    keys.sort();
    keys.dedup();
    let mut parts = Vec::new();
    for key in keys {
        let key = key.to_string();
        let was = find(&before.moves, &key)
            .map(|t| (t, true))
            .or_else(|| find(&before.keep, &key).map(|t| (t, false)));
        let is = find(&now.moves, &key)
            .map(|t| (t, true))
            .or_else(|| find(&now.keep, &key).map(|t| (t, false)));
        let part = match (was, is) {
            (None, Some((t, true))) => Some(format!("{key} entered {}", t.stage)),
            (None, Some((t, false))) => Some(format!("{key} started in {}", t.stage)),
            (Some(_), None) => Some(format!("{key} is no longer working")),
            (Some((a, _)), Some((b, _))) if a.stage != b.stage => {
                Some(format!("{key} moved to {}", b.stage))
            }
            (Some((a, _)), Some((b, _))) if a.running && !b.running => {
                Some(format!("{key}\u{2019}s agent stopped"))
            }
            (Some((a, _)), Some((b, _))) if !a.running && b.running => {
                Some(format!("{key}\u{2019}s agent started"))
            }
            _ => None,
        };
        parts.extend(part);
    }
    if parts.is_empty() {
        return match before.summary != now.summary {
            true => format!("The waiting change changed: {}", sentence(&now.summary)),
            false => "The board changed while this was open.".to_owned(),
        };
    }
    format!("The board changed: {}.", parts.join(" \u{b7} "))
}

#[cfg(test)]
mod tests {
    use otaq_core::domain::Status;

    use super::*;

    const DEV_RESUMES: &str = "Dev resumes SHOP-19\u{2019}s earlier session";

    fn key(k: &str) -> TicketKey {
        k.parse().unwrap()
    }

    /// shop-web's new flow after the change: Dev, Validate, Review, Done · PR.
    fn new_flow() -> Flow {
        let yaml = "\
flow:
  - {id: dev, name: Dev, kind: agent, agent: Rust developer, skill: work-ticket}
  - {id: validate, name: Validate, kind: check, by: agent, agents: [Validator], skill: validate, rounds: 3}
  - {id: review, name: Review, kind: check, by: you}
  - {id: ship, name: Done, kind: delivery, agent: Shipper, skill: open-pr, mode: automatic, delivers: pr}
";
        otaq_core::flow::yaml::read(yaml).unwrap().flow
    }

    fn choice(to: DropOn, name: &str, way: Way, hint: Option<&str>) -> MoveChoice {
        MoveChoice {
            to,
            name: name.to_owned(),
            way,
            hint: hint.map(str::to_owned),
        }
    }

    /// Backlog, Ready for Dev, then Dev · Validate · Review · Done · PR with `next` the first stage after the removed
    /// one's place (earlier ones back, later ones forward).
    fn choices(next: &str, ticket: &str) -> Vec<MoveChoice> {
        let mut list = vec![
            choice(DropTarget::Backlog.into(), "Backlog", Way::Backlog, None),
            choice(
                DropTarget::ReadyForDev.into(),
                "Ready for Dev",
                Way::ReadyForDev,
                None,
            ),
        ];
        let stages = [
            ("dev", "Dev"),
            ("validate", "Validate"),
            ("review", "Review"),
            ("ship", "Done \u{b7} PR"),
        ];
        let at = stages.iter().position(|(id, _)| *id == next).unwrap();
        for (i, (id, name)) in stages.iter().enumerate() {
            let (way, hint) = match i.cmp(&at) {
                std::cmp::Ordering::Less => (
                    Way::Back,
                    match *id {
                        "dev" => Some(format!("Dev resumes {ticket}\u{2019}s earlier session")),
                        "validate" => Some(format!("Validate runs again on {ticket}")),
                        _ => None,
                    },
                ),
                std::cmp::Ordering::Equal => (Way::Next, None),
                std::cmp::Ordering::Greater => (Way::Forward, Some(FORWARD_HINT.to_owned())),
            };
            list.push(choice(DropOn::stage(*id), name, way, hint.as_deref()));
        }
        list
    }

    fn mv(ticket: &str, title: &str, stage: &str, next: &str) -> PlannedMove {
        PlannedMove {
            ticket: key(ticket),
            title: title.to_owned(),
            stage: stage.to_lowercase(),
            stage_name: stage.to_owned(),
            waiting: false,
            agent_will_stop: false,
            status: Status::Working,
            state: None,
            choices: choices(next, ticket),
            default: DropOn::stage(next),
        }
    }

    fn kept(ticket: &str, title: &str, stage: &str, running: bool) -> KeptTicket {
        KeptTicket {
            ticket: key(ticket),
            title: title.to_owned(),
            stage: stage.to_lowercase(),
            stage_name: stage.to_owned(),
            keeps_running: running,
        }
    }

    fn plan(summary: &str, moves: Vec<PlannedMove>, keep: Vec<KeptTicket>) -> ApplyNowPlan {
        ApplyNowPlan {
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            summary: summary.to_owned(),
            changes: Vec::new(),
            removed: Vec::new(),
            moves,
            keep,
            stamp: "s1".into(),
        }
    }

    fn seven_kept() -> Vec<KeptTicket> {
        let mut keep = vec![kept("SHOP-15", "Login rate limit", "Dev", true)];
        for n in [33, 34, 35, 36, 37, 38] {
            keep.push(kept(&format!("SHOP-{n}"), "Work", "Dev", true));
        }
        keep
    }

    fn picks(list: &[(&str, &str)]) -> Picks {
        list.iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    /// `s11-17` "Apply now: one ticket to place": SHOP-23 in QA (a removed human check), on Review; seven tickets keep
    /// their place, collapsed to one line; no hint.
    #[test]
    fn one_ticket_in_a_removed_human_check() {
        let p = plan(
            "Removes QA (your check) from shop-web\u{2019}s flow",
            vec![mv("SHOP-23", "Split user full name", "QA", "review")],
            seven_kept(),
        );
        let flow = new_flow();
        let v = view(&p, Some(&flow), &Picks::new(), None);
        assert_eq!(v.title, "Apply the flow change now?");
        assert_eq!(
            v.summary,
            "Removes QA (your check) from shop-web\u{2019}s flow."
        );
        assert_eq!(v.moves_label, "Moves");
        assert_eq!(v.moves.len(), 1);
        let m = &v.moves[0];
        assert_eq!(
            (m.ticket.as_str(), m.title.as_str()),
            ("SHOP-23", "Split user full name")
        );
        assert_eq!(m.line, "in QA (removed)");
        assert_eq!(
            (m.picked.as_str(), m.picked_name.as_str()),
            ("stage:review", "Review")
        );
        assert_eq!(m.picked_icon, IconName::UserCheck);
        assert_eq!(v.keep_label.as_deref(), Some("Keep their place \u{b7} 7"));
        assert_eq!(
            v.keep_collapsed.as_deref(),
            Some("SHOP-15 \u{b7} in Dev \u{b7} keeps running, and 6 more")
        );
        // Opened, every kept ticket is a row.
        assert_eq!(v.keep.len(), 7);
        assert_eq!(v.keep[0].line, "in Dev \u{b7} keeps running");
        assert_eq!(v.hint, None);
        assert_eq!(v.changed, None);
        assert_eq!(
            (v.cancel.as_str(), v.apply.as_str()),
            ("Cancel", "Apply now")
        );
        assert_eq!(v.stamp, "s1");
    }

    /// The picker (`s11-18`'s open one): Backlog, Ready for Dev, a line, then the new flow's stages with their
    /// columns' icons, the pick checked.
    #[test]
    fn the_picker_lists_every_place_with_the_pick_checked() {
        let p = plan(
            "",
            vec![mv("SHOP-19", "Price table", "Security", "review")],
            Vec::new(),
        );
        let flow = new_flow();
        let v = view(&p, Some(&flow), &picks(&[("SHOP-19", "stage:dev")]), None);
        let items = &v.moves[0].choices;
        let shown: Vec<(&str, &str, Option<IconName>, bool, bool)> = items
            .iter()
            .map(|i| {
                (
                    i.id.as_str(),
                    i.label.as_str(),
                    i.icon,
                    i.checked,
                    i.separated,
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                (
                    "backlog",
                    "Backlog",
                    Some(IconName::CircleDashed),
                    false,
                    false
                ),
                (
                    "ready_for_dev",
                    "Ready for Dev",
                    Some(IconName::Hourglass),
                    false,
                    false
                ),
                ("stage:dev", "Dev", Some(IconName::Bot), true, true),
                (
                    "stage:validate",
                    "Validate",
                    Some(IconName::ShieldCheck),
                    false,
                    false
                ),
                (
                    "stage:review",
                    "Review",
                    Some(IconName::UserCheck),
                    false,
                    false
                ),
                (
                    "stage:ship",
                    "Done \u{b7} PR",
                    Some(IconName::GitPullRequest),
                    false,
                    false
                ),
            ]
        );
        assert_eq!(v.keep_label, None);
        assert_eq!(v.keep_collapsed, None);
    }

    /// `s11-17` "Four tickets, two removed stages": two running agents say they stop, a failed check shows its state;
    /// four kept tickets are past 3, so they start collapsed, and opened show their lines.
    #[test]
    fn four_tickets_in_two_removed_stages() {
        let mut moves = vec![
            mv("SHOP-12", "Add dark mode toggle", "Product spec", "dev"),
            mv("SHOP-44", "Saved carts", "Product spec", "dev"),
            mv("SHOP-19", "Price table", "Security", "review"),
            mv("SHOP-32", "Address autocomplete", "Security", "review"),
        ];
        moves[0].agent_will_stop = true;
        moves[1].agent_will_stop = true;
        moves[2].agent_will_stop = true;
        moves[3].state = Some("failed \u{d7}3".into());
        let p = plan(
            "Removes Product spec and Security from shop-web\u{2019}s flow",
            moves,
            vec![
                kept("SHOP-15", "Login rate limit", "Dev", true),
                kept("SHOP-33", "Shipping estimate", "Dev", true),
                kept("SHOP-18", "Checkout API", "Review", false),
                kept("SHOP-23", "Split user full name", "Review", false),
            ],
        );
        let v = view(&p, Some(&new_flow()), &Picks::new(), None);
        assert_eq!(
            v.summary,
            "Removes Product spec and Security from shop-web\u{2019}s flow."
        );
        let lines: Vec<(&str, &str, &str)> = v
            .moves
            .iter()
            .map(|m| (m.ticket.as_str(), m.line.as_str(), m.picked_name.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                (
                    "SHOP-12",
                    "in Product spec (removed) \u{b7} agent will stop",
                    "Dev"
                ),
                (
                    "SHOP-44",
                    "in Product spec (removed) \u{b7} agent will stop",
                    "Dev"
                ),
                (
                    "SHOP-19",
                    "in Security (removed) \u{b7} agent will stop",
                    "Review"
                ),
                (
                    "SHOP-32",
                    "in Security (removed) \u{b7} failed \u{d7}3",
                    "Review"
                ),
            ]
        );
        assert_eq!(v.keep_label.as_deref(), Some("Keep their place \u{b7} 4"));
        assert_eq!(
            v.keep_collapsed.as_deref(),
            Some("SHOP-15 \u{b7} in Dev \u{b7} keeps running, and 3 more")
        );
        let kept: Vec<(&str, &str, &str)> = v
            .keep
            .iter()
            .map(|k| (k.ticket.as_str(), k.title.as_str(), k.line.as_str()))
            .collect();
        assert_eq!(
            kept,
            [
                ("SHOP-15", "Login rate limit", "in Dev \u{b7} keeps running"),
                (
                    "SHOP-33",
                    "Shipping estimate",
                    "in Dev \u{b7} keeps running"
                ),
                (
                    "SHOP-18",
                    "Checkout API",
                    "in Review \u{b7} keeps its place"
                ),
                (
                    "SHOP-23",
                    "Split user full name",
                    "in Review \u{b7} keeps its place"
                ),
            ]
        );
        // On the defaults nothing goes back or skips: no hint.
        assert_eq!(v.hint, None);
        // Three kept tickets show as rows, no collapsed line.
        let three = plan("x", Vec::new(), p.keep[..3].to_vec());
        let v = view(&three, None, &Picks::new(), None);
        assert_eq!(v.keep_collapsed, None);
        assert_eq!(v.keep.len(), 3);
        assert!(v.moves.is_empty());
    }

    /// `s11-18`/`s11-17` "A removed stage’s ticket picked back to Dev": the hint names Dev resuming the session; a
    /// forward pick instead names the first stage it skips; a pick that isn't a choice falls back to the default.
    #[test]
    fn the_hint_follows_the_picks() {
        let mut moves = vec![mv("SHOP-19", "Price table", "Security", "review")];
        moves[0].agent_will_stop = true;
        let p = plan(
            "Removes Security from shop-web\u{2019}s flow",
            moves,
            seven_kept(),
        );
        let back = view(&p, None, &picks(&[("SHOP-19", "stage:dev")]), None);
        assert_eq!(back.moves[0].picked_name, "Dev");
        assert_eq!(
            back.hint.as_deref(),
            Some(format!("Back to Dev: {DEV_RESUMES}.").as_str())
        );
        assert_eq!(
            back.keep_collapsed.as_deref(),
            Some("SHOP-15 \u{b7} in Dev \u{b7} keeps running, and 6 more")
        );

        let forward = view(&p, None, &picks(&[("SHOP-19", "stage:ship")]), None);
        assert_eq!(
            forward.hint.as_deref(),
            Some(
                "Moving forward skips the stages in between, as on the card (\u{201c}Skipped Review \u{b7} by \
                 you\u{201d})."
            )
        );
        // A back pick wins over a forward one; Backlog and Ready for Dev say nothing.
        let mut two = p.clone();
        two.moves
            .push(mv("SHOP-12", "Add dark mode toggle", "Product spec", "dev"));
        let both = view(
            &two,
            None,
            &picks(&[("SHOP-12", "stage:ship"), ("SHOP-19", "stage:dev")]),
            None,
        );
        assert_eq!(
            both.hint.as_deref(),
            Some(format!("Back to Dev: {DEV_RESUMES}.").as_str())
        );
        let parked = view(&p, None, &picks(&[("SHOP-19", "backlog")]), None);
        assert_eq!(parked.hint, None);
        assert_eq!(parked.moves[0].picked, "backlog");
        // Back to a human check: no hint line.
        let review_back = {
            let mut q = p.clone();
            q.moves[0].choices = choices("ship", "SHOP-19");
            q.moves[0].default = DropOn::stage("ship");
            view(&q, None, &picks(&[("SHOP-19", "stage:review")]), None)
        };
        assert_eq!(review_back.hint, None);
        // Not a choice (or nonsense): the default.
        let stale = view(
            &p,
            None,
            &picks(&[("SHOP-19", "stage:product-spec"), ("SHOP-99", "x")]),
            None,
        );
        assert_eq!(stale.moves[0].picked, "stage:review");
        assert_eq!(stale.hint, None);
    }

    /// A merged change that removes no stage: its own words as the summary, nothing to move, the tickets that keep
    /// their place; a waiting ticket says so.
    #[test]
    fn a_merged_change_with_nothing_to_move() {
        let p = plan(
            "QA after Security \u{b7} Review can be skipped",
            Vec::new(),
            vec![kept("SHOP-15", "Login rate limit", "Dev", true)],
        );
        let v = view(&p, None, &Picks::new(), None);
        assert_eq!(v.summary, "QA after Security \u{b7} Review can be skipped.");
        assert!(v.moves.is_empty());
        assert_eq!(v.keep_label.as_deref(), Some("Keep their place \u{b7} 1"));
        let mut waiting = mv("SHOP-35", "Slow checkout query", "Product spec", "dev");
        waiting.waiting = true;
        assert_eq!(move_line(&waiting), "waiting for Product spec (removed)");
    }

    /// APN-04: the board changed under the window. The fresh plan redraws it with a line naming what changed: a ticket
    /// that entered a removed stage, one that is through, one whose agent stopped; the picks still valid stay.
    #[test]
    fn a_refresh_names_what_changed() {
        let mut before_moves = vec![mv("SHOP-19", "Price table", "Security", "review")];
        before_moves[0].agent_will_stop = true;
        let before = plan(
            "Removes Product spec and Security from shop-web\u{2019}s flow",
            before_moves,
            vec![
                kept("SHOP-15", "Login rate limit", "Dev", true),
                kept("SHOP-33", "Shipping estimate", "Dev", true),
            ],
        );
        let shown = view(&before, None, &Picks::new(), None).basis;
        let mut after = before.clone();
        after.stamp = "s2".into();
        after.moves[0].agent_will_stop = false;
        after
            .moves
            .insert(0, mv("SHOP-41", "Hotfix", "Product spec", "dev"));
        after.keep.retain(|k| k.ticket != key("SHOP-33"));
        let line = changed_line(&shown, &after, &Refreshed::BoardChanged);
        assert_eq!(
            line,
            "The board changed: SHOP-19\u{2019}s agent stopped \u{b7} SHOP-33 is no longer working \u{b7} SHOP-41 \
             entered Product spec."
        );
        let v = view(
            &after,
            None,
            &picks(&[("SHOP-19", "stage:dev")]),
            Some(line.clone()),
        );
        assert_eq!(v.changed.as_deref(), Some(line.as_str()));
        assert_eq!(v.stamp, "s2");
        assert_eq!(v.moves[1].picked, "stage:dev");
        assert_eq!(v.moves[0].picked, "stage:dev");

        // A ticket moved on into a kept stage, and one newly started.
        let mut later = before.clone();
        later.moves.clear();
        later
            .keep
            .push(kept("SHOP-19", "Price table", "Review", false));
        later.keep.push(kept("SHOP-50", "New", "Dev", true));
        assert_eq!(
            changed_line(&shown, &later, &Refreshed::BoardChanged),
            "The board changed: SHOP-19 moved to Review \u{b7} SHOP-50 started in Dev."
        );
        // Nothing to name: the change itself was merged, or only the stamp moved.
        let mut merged = before.clone();
        merged.summary = "Removes Product spec, Security and QA from shop-web\u{2019}s flow".into();
        assert_eq!(
            changed_line(&shown, &merged, &Refreshed::BoardChanged),
            "The waiting change changed: Removes Product spec, Security and QA from shop-web\u{2019}s flow."
        );
        assert_eq!(
            changed_line(&shown, &before, &Refreshed::BoardChanged),
            "The board changed while this was open."
        );
        // A pick that isn't a choice any more.
        assert_eq!(
            changed_line(
                &shown,
                &before,
                &Refreshed::NotAChoice {
                    ticket: key("SHOP-19")
                }
            ),
            "The pick for SHOP-19 isn\u{2019}t a choice any more; it starts on Review."
        );
    }

    #[test]
    fn choice_ids_go_both_ways() {
        for to in [
            DropOn::from(DropTarget::Backlog),
            DropTarget::ReadyForDev.into(),
            DropOn::stage("dev"),
        ] {
            assert_eq!(choice_of(&choice_id(&to)), Some(to));
        }
        assert_eq!(choice_of("stage:"), None);
        assert_eq!(choice_of("dev"), None);
    }
}

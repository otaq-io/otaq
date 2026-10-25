//! A Check stage with several check agents on the card and in the sheet (d0130 and its row 17, 0108 rule 9; canvas
//! `s29-04` "On the card", `s30-05` "When a check agent doesn't finish"): the round's line ("Validate · round 1 of 3",
//! "Validate · Performance waits for its plan", "Back in Dev · Validate failed · 2 reports", "Validate · Performance
//! ended without a verdict", "Validate · round 1 again · the code changed"), one row per check agent with its tool and
//! where it stands ("Security passed", "Performance running", "held · plan limit"), and the note after the card's one
//! action ("Use fallback · for Performance only", "Retry · re-runs Performance only"). A round of one check agent has
//! none of it: the card reads as today.

use otaq_core::agents::HoldReason;
use otaq_core::agents::facts::CardHold;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{CheckAgentFlag, CheckRoundFlag, Status, ValidationOutcome};
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::card::VALIDATE;
use crate::view_models::tool_avatar::ToolAvatarView;

/// Where one check agent of the round stands. `CheckAgentStep` on the page: its icon and colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "CheckAgentStep")]
pub enum Step {
    Running,
    /// Green, a check mark.
    Passed,
    /// Red, a cross.
    Failed,
    /// Amber, paused: held for its tool (its plan limit).
    Held,
    /// Ended without a verdict, or its verdict was discarded.
    NoVerdict,
    /// Its row ended without counting: cancelled, stopped, skipped.
    Ended,
}

/// One check agent's row: "CC Security passed". `CheckAgentLine` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CheckAgentLine")]
pub struct Line {
    pub avatar: Option<ToolAvatarView>,
    /// "Security": the short name ([`short_names`]).
    pub name: String,
    /// "passed", "running", "held · plan limit", "no verdict".
    pub state: String,
    pub step: Step,
}

/// The round on the card or in the sheet: its line, a row per check agent, the note under them. `CheckRoundView` on
/// the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CheckRoundView")]
pub struct RoundView {
    /// "Validate · round 1 of 3".
    pub title: String,
    pub agents: Vec<Line>,
    /// "Developer resumes with both reports.", "A change to the code cancelled the round; no verdict counted."
    pub note: Option<String>,
}

/// The short names the card shows ("Security" for "Security reviewer", `s29-04`): each agent's first word while
/// those differ, else the names as the stage lists them.
pub fn short_names(names: &[&str]) -> Vec<String> {
    let first = |n: &str| n.split_whitespace().next().unwrap_or(n).to_owned();
    let firsts: Vec<String> = names.iter().map(|n| first(n)).collect();
    let distinct = firsts
        .iter()
        .enumerate()
        .all(|(i, f)| !firsts[..i].iter().any(|g| g.eq_ignore_ascii_case(f)));
    match distinct {
        true => firsts,
        false => names.iter().map(|n| (*n).to_owned()).collect(),
    }
}

/// "Performance", "Security and Performance", "Security, Lint and Performance".
pub fn joined(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// "both reports", "all three reports" (0093: counts spelled out in a sentence).
fn reports_in_words(n: usize) -> String {
    const WORDS: [&str; 9] = [
        "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    match n {
        0 | 1 => "the report".to_owned(),
        2 => "both reports".to_owned(),
        n => match WORDS.get(n - 2) {
            Some(word) => format!("all {word} reports"),
            None => format!("all {n} reports"),
        },
    }
}

/// The round of several check agents the card shows, while it is in the check or back from it (none for one agent).
pub fn round_of(ticket: &Ticket) -> Option<&CheckRoundFlag> {
    ticket
        .flags
        .check_round
        .as_ref()
        .filter(|r| r.agents.len() > 1)
}

/// The check's name: "Validate", "Security".
fn check_of(ticket: &Ticket) -> &str {
    ticket.flags.check.as_deref().unwrap_or(VALIDATE)
}

/// The validator hold of one of the round's agents: the one it would have started (d0130 row 17: a hold is per held
/// agent).
pub fn held_agent<'a>(
    round: &CheckRoundFlag,
    holds: &'a [CardHold],
) -> Option<(usize, &'a CardHold)> {
    holds
        .iter()
        .filter(|h| h.role == otaq_core::agents::SessionRole::Validator)
        .find_map(|h| {
            let name = h.check_agent.as_deref().or(h.agent.as_deref())?;
            let i = round
                .agents
                .iter()
                .position(|a| a.name.eq_ignore_ascii_case(name) && a.outcome.is_none())?;
            Some((i, h))
        })
}

fn step_of(agent: &CheckAgentFlag, held: bool) -> Step {
    use ValidationOutcome as O;
    match agent.outcome {
        None if held => Step::Held,
        None => Step::Running,
        Some(O::Pass) => Step::Passed,
        Some(O::Fail) => Step::Failed,
        Some(O::NoResult | O::Discarded | O::Interrupted) => Step::NoVerdict,
        Some(O::Cancelled | O::Stopped | O::Skipped) => Step::Ended,
    }
}

/// "held · plan limit"; "held · Codex not available" for another hold.
fn held_words(hold: &CardHold) -> String {
    match hold.reason {
        HoldReason::ToolLimit => "held \u{b7} plan limit".to_owned(),
        reason => {
            let tool = crate::view_models::tool_avatar::AvatarTool::from(hold.tool).name();
            format!(
                "held \u{b7} {}",
                crate::view_models::card::hold_reason_text(reason, tool)
            )
        }
    }
}

fn state_words(agent: &CheckAgentFlag, step: Step) -> String {
    use ValidationOutcome as O;
    match (step, agent.outcome) {
        (Step::Running, _) => "running".to_owned(),
        (Step::Passed, _) => "passed".to_owned(),
        (Step::Failed, _) => "failed".to_owned(),
        (Step::NoVerdict, _) => "no verdict".to_owned(),
        (Step::Ended, Some(O::Cancelled)) => "cancelled".to_owned(),
        (Step::Ended, Some(O::Skipped)) => "skipped".to_owned(),
        (Step::Ended, _) => "stopped".to_owned(),
        (Step::Held, _) => "held".to_owned(),
    }
}

/// Each check agent's row, in the stage's order.
pub fn lines(round: &CheckRoundFlag, holds: &[CardHold]) -> Vec<Line> {
    let names: Vec<&str> = round.agents.iter().map(|a| a.name.as_str()).collect();
    let short = short_names(&names);
    let held = held_agent(round, holds);
    round
        .agents
        .iter()
        .zip(short)
        .enumerate()
        .map(|(i, (agent, name))| {
            let hold = held.filter(|(h, _)| *h == i).map(|(_, h)| h);
            let step = step_of(agent, hold.is_some());
            Line {
                avatar: agent
                    .tool
                    .map(|t| ToolAvatarView::of(t, agent.model.as_deref())),
                name,
                state: match hold {
                    Some(hold) => held_words(hold),
                    None => state_words(agent, step),
                },
                step,
            }
        })
        .collect()
}

/// The short names of the agents at `step`.
fn named(round: &CheckRoundFlag, step: Step, holds: &[CardHold]) -> Vec<String> {
    lines(round, holds)
        .into_iter()
        .filter(|l| l.step == step)
        .map(|l| l.name)
        .collect()
}

/// The round's line on the card (`s29-04`, `s30-05`): back in the Agent stage after it failed, an agent held, agents
/// that ended without a verdict, a round again after a change, else the round.
pub fn title(ticket: &Ticket, round: &CheckRoundFlag, holds: &[CardHold]) -> String {
    let check = check_of(ticket);
    if let Some(stage) = &round.back_in {
        let reports = round
            .agents
            .iter()
            .filter(|a| {
                matches!(
                    a.outcome,
                    Some(ValidationOutcome::Pass | ValidationOutcome::Fail)
                )
            })
            .count();
        let reports = match reports {
            1 => "1 report".to_owned(),
            n => format!("{n} reports"),
        };
        return format!("Back in {stage} \u{b7} {check} failed \u{b7} {reports}");
    }
    if let Some((i, hold)) = held_agent(round, holds) {
        let names: Vec<&str> = round.agents.iter().map(|a| a.name.as_str()).collect();
        let name = &short_names(&names)[i];
        return match hold.reason {
            HoldReason::ToolLimit => format!("{check} \u{b7} {name} waits for its plan"),
            _ => format!("{check} \u{b7} {name} can\u{2019}t start"),
        };
    }
    let no_verdict = named(round, Step::NoVerdict, holds);
    if ticket.status == Status::CheckNoResult && !no_verdict.is_empty() {
        return format!(
            "{check} \u{b7} {} ended without a verdict",
            joined(&no_verdict)
        );
    }
    if round.again {
        return format!(
            "{check} \u{b7} round {} again \u{b7} the code changed",
            round.round
        );
    }
    match ticket
        .flags
        .validating_round
        .or(ticket.flags.validation_round)
    {
        Some(r) => format!(
            "{check} \u{b7} round {} of {}",
            round.round.max(r.round),
            r.of
        ),
        None => format!("{check} \u{b7} round {}", round.round),
    }
}

/// The note under the rows: who resumes with the reports once the round failed ("Developer resumes with both
/// reports."), or that a change cancelled the round before this one.
pub fn note(round: &CheckRoundFlag, fixer: Option<&str>) -> Option<String> {
    if round.back_in.is_some() {
        let reports = round
            .agents
            .iter()
            .filter(|a| {
                matches!(
                    a.outcome,
                    Some(ValidationOutcome::Pass | ValidationOutcome::Fail)
                )
            })
            .count();
        return Some(format!(
            "{} resumes with {}.",
            fixer.unwrap_or("The agent"),
            reports_in_words(reports)
        ));
    }
    round
        .again
        .then(|| "A change to the code cancelled the round; no verdict counted.".to_owned())
}

/// The round on the card or in the sheet.
pub fn view(ticket: &Ticket, holds: &[CardHold], fixer: Option<&str>) -> Option<RoundView> {
    let round = round_of(ticket)?;
    Some(RoundView {
        title: title(ticket, round, holds),
        agents: lines(round, holds),
        note: note(round, fixer),
    })
}

/// The words after the card's one action, naming the one agent it acts on: "for Performance only" after Use
/// fallback, "re-runs Performance only" after Retry (d0130 row 17).
pub fn action_note(ticket: &Ticket, holds: &[CardHold]) -> Option<String> {
    let round = round_of(ticket)?;
    if round.back_in.is_some() {
        return None;
    }
    if let Some((i, hold)) = held_agent(round, holds)
        && hold.can_use_fallback
    {
        let names: Vec<&str> = round.agents.iter().map(|a| a.name.as_str()).collect();
        return Some(format!("for {} only", short_names(&names)[i]));
    }
    let no_verdict = named(round, Step::NoVerdict, holds);
    (ticket.status == Status::CheckNoResult && !no_verdict.is_empty())
        .then(|| format!("re-runs {} only", joined(&no_verdict)))
}

/// The sheet header's label while the card is in a check of several agents: "Validate · 2 check agents" (`s29-04`).
pub fn header_label(ticket: &Ticket) -> Option<String> {
    let round = round_of(ticket).filter(|r| r.back_in.is_none())?;
    Some(format!(
        "{} \u{b7} {} check agents",
        check_of(ticket),
        round.agents.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names_are_first_words_while_they_differ() {
        assert_eq!(
            short_names(&["Security reviewer", "Performance reviewer"]),
            ["Security", "Performance"]
        );
        assert_eq!(
            short_names(&["Code reviewer", "Code style"]),
            ["Code reviewer", "Code style"]
        );
        assert_eq!(short_names(&["Validator", "Lint"]), ["Validator", "Lint"]);
    }

    #[test]
    fn names_join_and_reports_count_in_words() {
        let s = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(joined(&s(&["Performance"])), "Performance");
        assert_eq!(
            joined(&s(&["Security", "Performance"])),
            "Security and Performance"
        );
        assert_eq!(
            joined(&s(&["Security", "Lint", "Performance"])),
            "Security, Lint and Performance"
        );
        assert_eq!(reports_in_words(2), "both reports");
        assert_eq!(reports_in_words(3), "all three reports");
    }

    fn card(name: &str) -> crate::view_models::card_page::TicketCardView {
        use crate::view_models::card_fixtures::{self, NOW};
        let v = card_fixtures::check_agent_variants()
            .into_iter()
            .find(|v| v.name == name)
            .unwrap_or_else(|| panic!("no variant {name}"));
        crate::view_models::card_page::TicketCardView::of(&v.ticket, &v.extras, NOW)
    }

    /// A row as the tests read it: avatar letters, name, state, step.
    type Row = (Option<String>, String, String, Step);
    /// The card as the tests read it: its line, rows, note, and action with the action's note.
    type Read = (
        String,
        Vec<Row>,
        Option<String>,
        Option<(&'static str, String)>,
    );

    /// The card's line, its rows ("CC Security passed"), the note, its action and the action's note.
    fn read(name: &str) -> Read {
        let c = card(name);
        let check = c.check.clone().expect("the round");
        assert_eq!(
            c.status.as_ref().map(|l| l.text.as_str()),
            Some(check.title.as_str())
        );
        assert_eq!(
            (c.avatar.clone(), c.who.clone()),
            (None, None),
            "the rows name the agents"
        );
        (
            check.title,
            check
                .agents
                .into_iter()
                .map(|l| (l.avatar.map(|a| a.letters), l.name, l.state, l.step))
                .collect(),
            check.note,
            c.action
                .map(|a| (a.label, c.action_note.unwrap_or_default())),
        )
    }

    fn row(
        letters: &str,
        name: &str,
        state: &str,
        step: Step,
    ) -> (Option<String>, String, String, Step) {
        (Some(letters.into()), name.into(), state.into(), step)
    }

    /// `s29-04` "On the card": two agents running, the round's line and one row each.
    #[test]
    fn shop_33_two_check_agents_running() {
        let (title, rows, note, action) = read("Validate · two check agents · running");
        assert_eq!(title, "Validate · round 1 of 3");
        assert_eq!(
            rows,
            [
                row("CC", "Security", "passed", Step::Passed),
                row("CX", "Performance", "running", Step::Running),
            ]
        );
        assert_eq!((note, action), (None, None));
    }

    /// d0130 row 17: a hold is per held agent; Use fallback moves only that one.
    #[test]
    fn shop_33_one_held_at_its_plan_limit() {
        let (title, rows, _, action) = read("Validate · one held at its plan limit");
        assert_eq!(title, "Validate · Performance waits for its plan");
        assert_eq!(
            rows[1],
            row("CX", "Performance", "held · plan limit", Step::Held)
        );
        assert_eq!(rows[0].2, "passed");
        assert_eq!(
            action,
            Some(("Use fallback", "for Performance only".to_owned()))
        );
        let kind = card("Validate · one held at its plan limit")
            .action
            .map(|a| a.kind);
        assert_eq!(
            kind,
            Some(crate::view_models::card::CardActionKind::UseValidatorFallback)
        );
    }

    /// d0130: the card goes back once both gave their verdict, with both reports.
    #[test]
    fn shop_33_both_failed_back_in_dev() {
        let (title, rows, note, action) = read("Validate · both gave a verdict · back in Dev");
        assert_eq!(title, "Back in Dev · Validate failed · 2 reports");
        assert_eq!(
            rows,
            [
                row("CC", "Security", "failed", Step::Failed),
                row("CX", "Performance", "failed", Step::Failed),
            ]
        );
        assert_eq!(
            note.as_deref(),
            Some("Developer resumes with both reports.")
        );
        assert_eq!(action, None);
    }

    /// `s30-05` "When a check agent doesn't finish": Retry re-runs only the agent with no verdict.
    #[test]
    fn shop_33_one_ended_without_a_verdict() {
        let (title, rows, _, action) = read("Validate · one ended without a verdict");
        assert_eq!(title, "Validate · Performance ended without a verdict");
        assert_eq!(
            rows[1],
            row("CX", "Performance", "no verdict", Step::NoVerdict)
        );
        assert_eq!(
            action,
            Some(("Retry", "re-runs Performance only".to_owned()))
        );
        let kind = card("Validate · one ended without a verdict")
            .action
            .map(|a| a.kind);
        assert_eq!(
            kind,
            Some(crate::view_models::card::CardActionKind::RetryValidation)
        );
    }

    /// 0108 rule 9, d0130 row 17: a change cancels the whole round; the next one says so.
    #[test]
    fn shop_33_round_again_after_a_change() {
        let (title, rows, note, _) = read("Validate · round 1 again · the code changed");
        assert_eq!(title, "Validate · round 1 again · the code changed");
        assert!(rows.iter().all(|r| r.2 == "running"));
        assert_eq!(
            note.as_deref(),
            Some("A change to the code cancelled the round; no verdict counted.")
        );
    }

    /// d0130: "A stage with one check agent works exactly as today."
    #[test]
    fn one_check_agent_reads_as_today() {
        let c = card("Validate · one check agent · as today");
        assert_eq!(c.check, None);
        assert_eq!(c.action_note, None);
        assert_eq!(
            c.status.map(|l| l.text).as_deref(),
            Some("Validating · round 1 of 3")
        );
    }

    /// The sheet's header label while the card is in the check: "Validate · 2 check agents" (`s29-04`); none back
    /// in Dev or with one agent.
    #[test]
    fn the_sheet_header_names_the_check_agents() {
        use crate::view_models::card_fixtures::check_agent_variants;
        let label = |name: &str| {
            let v = check_agent_variants()
                .into_iter()
                .find(|v| v.name == name)
                .unwrap();
            header_label(&v.ticket)
        };
        assert_eq!(
            label("Validate · two check agents · running").as_deref(),
            Some("Validate · 2 check agents")
        );
        assert_eq!(label("Validate · both gave a verdict · back in Dev"), None);
        assert_eq!(label("Validate · one check agent · as today"), None);
    }
}

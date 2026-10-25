//! A ticket's stage choices and its path (AGM-09, AGM-08, AGM-03; d0108's "Options" and smaller rule 5, d0093,
//! d0077; *08 → Row · Stage choices (update AG §9)*, *05 → Row · Stage choices and path (update AG §9)*, *05 → Row ·
//! Stage choices and path, finished and set by you*), from the facts of `ticket.choices` ([`TicketChoices`]) and
//! `ticket.agents` ([`TicketAgents`]). They replace the Developer, Validator and Epic lead rows.
//!
//! - **Choices** ([`form_choices`], [`sheet_choices`]): a picker only for the stages with options ("Dev: Rust
//!   developer ▾", "Review: required ▾"); the ⌘N form preselects each stage's own agent. A stage that started is
//!   locked. Who set them: "set by Noyan" (or "the Epic lead") above the rows; a change by a ticket's
//!   agent says who and why in violet ("Dev: Quick developer → Rust developer · by Griller · touches session
//!   handling"), one you made says it's yours ("… · by you"). None on a flow without options (the default flow).
//! - **The path** ([`path`]): every stage the ticket goes through with its agent and model; stages already run with
//!   the exact model and a lock, failed rounds kept with what they found, skipped stages marked ("Skipped Grill (this
//!   ticket's choice)"), and a one-line summary ("Grill skipped · Dev · Quick developer · haiku · … · Review
//!   skipped · Done"; "Back in Dev after Security failed · then Validate · Security (round 2 of 3) · Review · Done").
//! - **The token line** in the sheet's header (VAL-14, Q-G): "Strong 61k · Light 23k (1 round)"; with several tools
//!   "Strong · Claude Code 58k · Verifier · Copilot 9k (1 round)".
//! - **History lines**: session starts and ends with the agent and the exact model, M31's row changes and moves as
//!   they were recorded, and a Done ticket's "Developer: Light (deleted) · claude-haiku-5-5 · 48k".

use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::follow::FollowChoice;
use otaq_core::agent_tools::models::{ModelKind, ModelList};
use otaq_core::agents::facts::{PathKind, PathStage, TicketAgents};
use otaq_core::agents::mark::is_swap;
use otaq_core::agents::store::SessionAgentRun;
use otaq_core::agents::{Agent, ModelRef, SessionRole};
use otaq_core::board::stage_choices::{SetBy, StageOptions, TicketChoices};
use otaq_core::board::stage_runs::StageRun;
use otaq_core::domain::{Kind, Place, Status, StatusSet};
use otaq_core::flow::engine::RunResult;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::agents::{TOOL_DEFAULT, is_tool_default};
use crate::view_models::card::tokens_label;
use crate::view_models::menu::MenuItem;
use crate::view_models::tool_problems::{self, Callout, Tools, tool_word};

/// The ⌘N form's field label.
pub const FORM_HEADING: &str = "Stage choices";
/// The sheet's block.
pub const SHEET_HEADING: &str = "Choices";
pub const PATH_HEADING: &str = "Path";
/// A row's value when nothing is known.
const DASH: &str = "\u{2014}";

// ---------------------------------------------------------------------------------------------------------------
// Models as the choices, the path and the cards name them

/// A model as the path and the cards name it: the text passed ("sonnet", "gpt-5.5"), Otaq's own follow choices by
/// their list name ("GPT Sol", "Auto · balance"), the tool default as "tool default".
pub fn short_model(model: Option<&str>, list: Option<&ModelList>) -> String {
    if is_tool_default(model, list) {
        return TOOL_DEFAULT.to_owned();
    }
    let id = model.unwrap_or_default().trim();
    match FollowChoice::parse(id) {
        Some(follow) => match list.and_then(|l| l.entry(id)) {
            Some(entry) => entry.display_name.clone(),
            None => match follow {
                FollowChoice::Newest { family } => format!("newest {family}"),
                FollowChoice::Auto { tier } => tier.label(),
            },
        },
        None => id.to_owned(),
    }
}

/// A tool and model ([`ModelRef`]) as [`short_model`] names it.
pub fn short_model_ref(model: &ModelRef, tools: &Tools) -> String {
    short_model(model.model.as_deref(), tools.list(model.tool))
}

/// What an agent runs on now, by name ("sonnet"), with the tool's effort where it has its own model ("opus · effort
/// high"); "no default model" while it runs on the default model and none is set.
fn agent_model(agent: &Agent, tools: &Tools, with_effort: bool) -> String {
    match agent.model.on(tools.default_model.as_ref()) {
        Some(on) => {
            let model = short_model_ref(&on, tools);
            match agent.effort.as_deref().filter(|_| with_effort) {
                Some(effort) if agent.model.tool().is_some() => {
                    format!("{model} \u{b7} effort {effort}")
                }
                _ => model,
            }
        }
        None => "no default model".to_owned(),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Choices

/// The line under a choice someone changed: an agent's in violet with its reason, yours neutral (d0077).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "StageChoiceChange")]
pub struct ChoiceChange {
    /// "Dev: Quick developer → Rust developer · by Griller · touches session handling".
    pub text: String,
    /// An agent's: violet.
    pub agent_made: bool,
    pub icon: IconName,
}

/// One stage's choice.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "StageChoiceRow")]
pub struct ChoiceRow {
    /// The stage's id, sent back with a pick.
    pub stage_id: String,
    /// "Dev:".
    pub label: String,
    /// "Rust developer", "required", "skip".
    pub value: String,
    /// The picker (the select's menu: each item's id is the word a pick sends, the current one checked); empty once
    /// the stage started.
    pub items: Vec<MenuItem>,
    /// Started: the lock's tooltip ("Locked: Grill has started").
    pub lock: Option<String>,
    pub change: Option<ChoiceChange>,
}

/// The ⌘N form's "Stage choices" field, or the sheet's Choices. `StageChoicesView` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "StageChoicesView")]
pub struct ChoicesView {
    pub heading: &'static str,
    /// "set by Noyan", "set by the Epic lead".
    pub set_by: Option<String>,
    pub rows: Vec<ChoiceRow>,
    /// The form's note under the pickers.
    pub note: Option<String>,
}

/// "Locked: Grill has started".
pub fn locked(stage: &str) -> String {
    format!("Locked: {stage} has started")
}

fn items(options: &StageOptions, current: &str) -> Vec<MenuItem> {
    options
        .words()
        .into_iter()
        .map(|word| MenuItem::new(word.clone(), word.clone()).checked(word == current))
        .collect()
}

/// The change line of a choice the user or a ticket's own agent made (the Project agent's and an epic lead's say so
/// above the rows): "Dev: Rust developer → Quick developer · by you".
fn change(options: &StageOptions) -> Option<ChoiceChange> {
    let choice = options.choice.as_ref()?;
    let (who, reason, agent_made) = match &choice.set_by {
        SetBy::User => ("you".to_owned(), None, false),
        SetBy::Agent { name, reason } => (name.clone(), Some(reason.as_str()), true),
        SetBy::ProjectAgent { .. } | SetBy::EpicLead { .. } => return None,
    };
    let now = options.label();
    let from = choice.from.clone().unwrap_or_else(|| options.usual_label());
    let mut text = match from == now {
        true => format!("{}: {now} \u{b7} by {who}", options.stage),
        false => format!("{}: {from} \u{2192} {now} \u{b7} by {who}", options.stage),
    };
    if let Some(reason) = reason.map(str::trim).filter(|r| !r.is_empty()) {
        text.push_str(&format!(" \u{b7} {reason}"));
    }
    Some(ChoiceChange {
        text,
        agent_made,
        icon: if agent_made {
            IconName::Bot
        } else {
            IconName::User
        },
    })
}

/// Who set the ticket's choices when it was made: "set by Noyan", "set by the Epic lead" (the role,
/// never the session's name).
fn set_by_line(stages: &[StageOptions]) -> Option<String> {
    let by = |pick: fn(&SetBy) -> bool| {
        stages
            .iter()
            .filter_map(|s| s.choice.as_ref())
            .any(|c| pick(&c.set_by))
    };
    if by(|s| matches!(s, SetBy::ProjectAgent { .. })) {
        Some("set by Noyan".to_owned())
    } else if by(|s| matches!(s, SetBy::EpicLead { .. })) {
        Some("set by the Epic lead".to_owned())
    } else {
        None
    }
}

fn rows(stages: &[StageOptions], with_changes: bool) -> Vec<ChoiceRow> {
    stages
        .iter()
        .map(|options| {
            let value = options.label();
            ChoiceRow {
                stage_id: options.stage_id.clone(),
                label: format!("{}:", options.stage),
                items: match options.locked {
                    true => Vec::new(),
                    false => items(options, &value),
                },
                lock: options.locked.then(|| locked(&options.stage)),
                change: with_changes.then(|| change(options)).flatten(),
                value,
            }
        })
        .collect()
}

/// The ⌘N form's "Stage choices" under Priority (`s08-29`): one picker per stage with options, each on its stage's
/// own agent; `None` on a flow without options (the default flow) and for an epic. `project` is the project's name.
pub fn form_choices(choices: &TicketChoices, project: &str) -> Option<ChoicesView> {
    if choices.stages.is_empty() {
        return None;
    }
    Some(ChoicesView {
        heading: FORM_HEADING,
        set_by: None,
        rows: rows(&choices.stages, false),
        note: Some(format!(
            "Only stages with options in {project}\u{2019}s flow. You can change them until that stage starts."
        )),
    })
}

/// The sheet's Choices (`s05-36`, `s05-38`): the ticket's choices with who set them; `None` on a flow without
/// options and for an epic.
pub fn sheet_choices(t: &TicketAgents) -> Option<ChoicesView> {
    if t.kind == Kind::Epic || t.choices.is_empty() {
        return None;
    }
    Some(ChoicesView {
        heading: SHEET_HEADING,
        set_by: set_by_line(&t.choices),
        rows: rows(&t.choices, true),
        note: None,
    })
}

/// The ⌘N form's picks to send with the new ticket: each stage whose pick isn't its own agent, as `Stage=choice`.
pub fn picks_to_send(choices: &TicketChoices, picks: &[(String, String)]) -> Vec<(String, String)> {
    picks
        .iter()
        .filter(|(stage, pick)| {
            choices
                .stages
                .iter()
                .find(|s| &s.stage_id == stage)
                .is_some_and(|s| s.words().contains(pick) && *pick != s.usual_label())
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------
// The path

/// How a stage of the path stands. `TicketPathState` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "TicketPathState")]
pub enum PathState {
    /// Ran and finished (done, passed).
    Passed,
    /// A check failed it (the round stays in the path).
    Failed,
    /// Stopped before it ended, or ended without a verdict.
    Stopped,
    /// Where the ticket is now.
    Running,
    /// Still to come.
    ToCome,
    /// The user's check, still to come.
    YourCheck,
    /// The ticket skips it.
    Skipped,
}

impl PathState {
    /// The row's icon (`s05-36`): a filled check, a cross, activity, a dashed circle, the person, a plain circle.
    pub fn icon(self) -> IconName {
        match self {
            Self::Passed => IconName::CircleCheck,
            Self::Failed => IconName::CircleX,
            Self::Stopped => IconName::CircleMinus,
            Self::Running => IconName::Activity,
            Self::ToCome => IconName::CircleDashed,
            Self::YourCheck => IconName::UserCheck,
            Self::Skipped => IconName::Circle,
        }
    }
}

/// One row of the path. `TicketPathRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TicketPathRow")]
pub struct PathRow {
    /// "Dev".
    pub stage: String,
    pub state: PathState,
    pub icon: IconName,
    /// Where the ticket is now: the name in bold.
    pub current: bool,
    /// "Rust developer · working", "failed round 1 · unescaped SQL in search", "your check".
    pub detail: String,
    /// "sonnet", "claude-opus-5-5 · high", "—" (mono).
    pub model: String,
    /// A run that ended: its model can't change any more (the lock's tooltip).
    pub lock: Option<&'static str>,
}

/// The path. `TicketPathView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TicketPathView")]
pub struct PathView {
    pub heading: &'static str,
    /// The path on one line.
    pub summary: String,
    pub rows: Vec<PathRow>,
}

/// The lock's tooltip on a stage that ran.
pub const RAN_LOCKED: &str = "Locked: it ran on this model";

/// A path stage's own words before it runs: "Validator · shown in Dev · up to 3 rounds", "Shipper · PR, automatic",
/// "your check".
fn delivery_words(stage: &PathStage) -> String {
    let Some(d) = &stage.delivery else {
        return String::new();
    };
    let what = match d.delivers.as_str() {
        "pr" => "PR",
        "local" => "Local merge",
        _ => "Local merge or PR",
    };
    match d.automatic {
        true => format!("{what}, automatic"),
        false => format!("{what}, your click"),
    }
}

/// Whether the stage's agent shows in the summary and a row: every stage with one, but a Delivery only when it opens
/// a PR by itself (a click-mode Delivery runs its agent only on the click).
fn runs_agent(stage: &PathStage) -> bool {
    stage.agent.is_some()
        && match &stage.delivery {
            Some(d) => d.automatic && d.delivers == "pr",
            None => true,
        }
}

fn agent_name(stage: &PathStage) -> String {
    stage
        .agent
        .as_ref()
        .map_or_else(|| DASH.to_owned(), |a| a.name.clone())
}

/// "Skipped Grill (this ticket's choice)"; a skipped check of yours waits for your click at the Delivery.
fn skipped_detail(stage: &PathStage, delivery: Option<&str>, delivered: bool) -> String {
    match (stage.kind, delivery) {
        (PathKind::HumanCheck, Some(at)) => format!(
            "{} skipped \u{b7} {} for your click at {at}",
            stage.stage,
            if delivered { "waited" } else { "waits" }
        ),
        _ => format!("Skipped {} (this ticket\u{2019}s choice)", stage.stage),
    }
}

/// The exact model a run ran on, with the effort its session passed ("claude-opus-5-5 · high").
fn ran_model(run: &StageRun, sessions: &[SessionAgentRun]) -> String {
    let session = run
        .session
        .as_deref()
        .and_then(|id| sessions.iter().find(|s| s.id == id));
    let model = run
        .model
        .clone()
        .or_else(|| session.and_then(|s| s.model_ran.clone().or_else(|| s.model.clone())));
    match (model, session.and_then(|s| s.effort.clone())) {
        (Some(model), Some(effort)) => format!("{model} \u{b7} {effort}"),
        (Some(model), None) => model,
        (None, _) => DASH.to_owned(),
    }
}

/// The model a running stage's session passed ("sonnet"), else its agent's now.
fn running_model(
    run: Option<&StageRun>,
    stage: &PathStage,
    sessions: &[SessionAgentRun],
    tools: &Tools,
) -> String {
    let session = run
        .and_then(|r| r.session.as_deref())
        .and_then(|id| sessions.iter().find(|s| s.id == id));
    match (session, &stage.agent) {
        (Some(s), _) if s.model.is_some() || s.model_ran.is_none() => {
            let list = s.tool.and_then(|t| tools.list(t));
            short_model(s.model.as_deref(), list)
        }
        (Some(s), _) => s.model_ran.clone().unwrap_or_else(|| DASH.to_owned()),
        (None, Some(agent)) if runs_agent(stage) => agent_model(agent, tools, false),
        _ => DASH.to_owned(),
    }
}

/// The agent's model as a row to come shows it ("opus · effort high"), "—" for a stage without one.
fn to_come_model(stage: &PathStage, tools: &Tools) -> String {
    match &stage.agent {
        Some(agent) if runs_agent(stage) => agent_model(agent, tools, true),
        _ => DASH.to_owned(),
    }
}

/// The index in the flow where the stages still to come start, and the stage the ticket is in now (`None` while it
/// waits or is done).
fn now_at(t: &TicketAgents) -> (usize, Option<usize>) {
    let index_of = |id: &str| t.path.iter().position(|s| s.stage_id == id);
    match &t.place {
        Some(Place::InStage(id)) => match index_of(id) {
            Some(i) => (i + 1, Some(i)),
            None => (t.path.len(), None),
        },
        Some(Place::Waiting(Some(id))) => (index_of(id).unwrap_or(0), None),
        Some(Place::Done) => (t.path.len(), None),
        // Parked with runs: it goes back to the stage it left (0108's un-park).
        Some(Place::Backlog) => match t.runs.last() {
            Some(last) => (index_of(&last.stage).unwrap_or(0), None),
            None => (0, None),
        },
        _ => (0, None),
    }
}

/// The ticket's path (`s05-36`, `s05-38`): the runs so far in order, where it is now, then the stages to come.
/// `None` for an epic, and for a ticket whose flow wasn't read.
pub fn path(t: &TicketAgents, tools: &Tools) -> Option<PathView> {
    if t.kind == Kind::Epic || t.path.is_empty() {
        return None;
    }
    let stages = shown_stages(&t.path);
    let stage_of = |id: &str| stages.iter().find(|s| s.stage_id == id);
    let delivery = stages
        .iter()
        .find(|s| s.kind == PathKind::Delivery)
        .map(|s| s.stage.as_str());
    let finished = t.status.is_some_and(|s| s.set() == StatusSet::Done);
    let (to_come_from, current) = now_at(t);
    // Runs of each stage, to number its rounds.
    let runs_of = |id: &str| t.runs.iter().filter(|r| r.stage == id).count();
    let mut rows = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut last_failed: Option<&str> = None;
    let mut running = false;
    for run in &t.runs {
        let Some(stage) = stage_of(&run.stage) else {
            continue;
        };
        let nth = seen.iter().filter(|s| **s == stage.stage_id).count() + 1;
        seen.push(&stage.stage_id);
        let agent = run.agent.clone().unwrap_or_else(|| agent_name(stage));
        let round = run.round.unwrap_or(nth as u32);
        let row = |state: PathState, detail: String, model: String, lock: bool| PathRow {
            stage: stage.stage.clone(),
            state,
            icon: state.icon(),
            current: false,
            detail,
            model,
            lock: lock.then_some(RAN_LOCKED),
        };
        let Some(result) = run.result else {
            // The run going on now.
            running = true;
            let again = last_failed.is_some() && nth > 1;
            let detail = match stage.kind {
                PathKind::HumanCheck => "your check".to_owned(),
                PathKind::AgentCheck => match stage.rounds {
                    Some(of) => format!("{agent} \u{b7} round {round} of {of}"),
                    None => format!("{agent} \u{b7} round {round}"),
                },
                PathKind::Delivery if runs_agent(stage) => {
                    format!("{agent} \u{b7} {}", delivery_words(stage))
                }
                PathKind::Delivery => delivery_words(stage),
                PathKind::Agent if again => format!("{agent} \u{b7} again, with the findings"),
                PathKind::Agent => format!("{agent} \u{b7} working"),
            };
            rows.push(PathRow {
                current: true,
                ..row(
                    if stage.kind == PathKind::HumanCheck {
                        PathState::YourCheck
                    } else {
                        PathState::Running
                    },
                    detail,
                    running_model(Some(run), stage, &t.sessions, tools),
                    false,
                )
            });
            continue;
        };
        let several = runs_of(&stage.stage_id) > 1;
        let numbered = |text: String| match several {
            true => format!("{text} \u{b7} round {nth}"),
            false => text,
        };
        let pushed = match result {
            RunResult::Skipped => row(
                PathState::Skipped,
                match stage.skipped {
                    true => skipped_detail(stage, delivery, finished),
                    false => format!("Skipped {} \u{b7} by you", stage.stage),
                },
                DASH.to_owned(),
                false,
            ),
            RunResult::Failed => {
                last_failed = Some(&stage.stage);
                let found = run
                    .summary
                    .as_deref()
                    .and_then(|s| s.lines().map(str::trim).find(|l| !l.is_empty()))
                    .map(|s| s.trim_end_matches('.'));
                let detail = match (stage.kind, found) {
                    (PathKind::HumanCheck, _) => "your check \u{b7} failed".to_owned(),
                    (_, Some(found)) => format!("failed round {round} \u{b7} {found}"),
                    (_, None) => format!("failed round {round}"),
                };
                row(PathState::Failed, detail, ran_model(run, &t.sessions), true)
            }
            RunResult::Passed => {
                last_failed = None;
                let detail = match stage.kind {
                    PathKind::HumanCheck => "your check \u{b7} passed".to_owned(),
                    _ => format!("{agent} \u{b7} passed round {round}"),
                };
                row(PathState::Passed, detail, ran_model(run, &t.sessions), true)
            }
            RunResult::Done => {
                let detail = match stage.kind {
                    PathKind::Delivery if t.status == Some(Status::ClosedNotMerged) => {
                        "Closed".to_owned()
                    }
                    PathKind::Delivery => match run.session {
                        Some(_) => format!("{agent} \u{b7} delivered"),
                        None => "Delivered".to_owned(),
                    },
                    _ => numbered(agent),
                };
                let model = match run.session {
                    Some(_) => ran_model(run, &t.sessions),
                    None => DASH.to_owned(),
                };
                row(PathState::Passed, detail, model, true)
            }
            RunResult::Cancelled | RunResult::NoResult => row(
                PathState::Stopped,
                format!(
                    "{agent} \u{b7} {}",
                    if result == RunResult::NoResult {
                        "no result"
                    } else {
                        "stopped"
                    }
                ),
                ran_model(run, &t.sessions),
                true,
            ),
        };
        rows.push(pushed);
    }
    // In a stage with no open run (a check waiting for its round, an Interrupted session): it is where the ticket is.
    if !running && let Some(i) = current {
        let stage = &stages[i];
        let state = match stage.kind {
            PathKind::HumanCheck => PathState::YourCheck,
            _ => PathState::Running,
        };
        rows.push(PathRow {
            stage: stage.stage.clone(),
            state,
            icon: state.icon(),
            current: true,
            detail: match stage.kind {
                PathKind::HumanCheck => "your check".to_owned(),
                _ => agent_name(stage),
            },
            model: running_model(None, stage, &t.sessions, tools),
            lock: None,
        });
    }
    // The stages still to come.
    let failed_rounds = |id: &str| {
        t.runs
            .iter()
            .filter(|r| r.stage == id && r.result == Some(RunResult::Failed))
            .count() as u32
    };
    let ran_before = |id: &str| t.runs.iter().any(|r| r.stage == id);
    let mut summary_rest = Vec::new();
    for stage in stages.iter().skip(to_come_from) {
        let shown_in = stage
            .shown_in
            .as_deref()
            .map(|s| format!(" \u{b7} shown in {s}"))
            .unwrap_or_default();
        let fails = failed_rounds(&stage.stage_id);
        let (state, detail, part) = if stage.skipped {
            (
                PathState::Skipped,
                skipped_detail(stage, delivery, false),
                format!("{} skipped", stage.stage),
            )
        } else {
            match stage.kind {
                PathKind::HumanCheck => (
                    PathState::YourCheck,
                    "your check".to_owned(),
                    stage.stage.clone(),
                ),
                PathKind::AgentCheck if fails > 0 => {
                    let of = stage.rounds.unwrap_or(fails + 1);
                    (
                        PathState::ToCome,
                        format!("round {} of {of}", fails + 1),
                        format!("{} (round {} of {of})", stage.stage, fails + 1),
                    )
                }
                PathKind::AgentCheck | PathKind::Agent if ran_before(&stage.stage_id) => (
                    PathState::ToCome,
                    format!("again{shown_in}"),
                    stage.stage.clone(),
                ),
                PathKind::AgentCheck => (
                    PathState::ToCome,
                    match stage.rounds {
                        Some(n) => {
                            format!("{}{shown_in} \u{b7} up to {n} rounds", agent_name(stage))
                        }
                        None => format!("{}{shown_in}", agent_name(stage)),
                    },
                    stage.stage.clone(),
                ),
                PathKind::Delivery if runs_agent(stage) => (
                    PathState::ToCome,
                    format!("{} \u{b7} {}", agent_name(stage), delivery_words(stage)),
                    stage.stage.clone(),
                ),
                PathKind::Delivery => (
                    PathState::ToCome,
                    delivery_words(stage),
                    stage.stage.clone(),
                ),
                PathKind::Agent => (
                    PathState::ToCome,
                    format!("{}{shown_in}", agent_name(stage)),
                    stage.stage.clone(),
                ),
            }
        };
        summary_rest.push(part);
        rows.push(PathRow {
            stage: stage.stage.clone(),
            state,
            icon: state.icon(),
            current: false,
            detail,
            model: match state {
                PathState::Skipped | PathState::YourCheck => DASH.to_owned(),
                _ => to_come_model(stage, tools),
            },
            lock: None,
        });
    }
    // Back in an Agent stage after a check failed: "Back in Dev after Security failed · then Validate · …".
    let back = current
        .map(|i| &stages[i])
        .filter(|s| s.kind == PathKind::Agent)
        .zip(last_failed);
    let summary = match back {
        Some((now, failed)) => {
            let mut text = format!("Back in {} after {failed} failed", now.stage);
            if !summary_rest.is_empty() {
                text.push_str(&format!(" \u{b7} then {}", summary_rest.join(" \u{b7} ")));
            }
            text
        }
        None => stages
            .iter()
            .map(|stage| summary_part(stage, tools))
            .collect::<Vec<_>>()
            .join(" \u{b7} "),
    };
    Some(PathView {
        heading: PATH_HEADING,
        summary,
        rows,
    })
}

/// The path's stages as they read for this ticket: a ticket that skips a check of yours reaches its Delivery in
/// click mode (d0108), so that Delivery waits for your click.
fn shown_stages(path: &[PathStage]) -> Vec<PathStage> {
    let skips_yours = path
        .iter()
        .any(|s| s.kind == PathKind::HumanCheck && s.skipped);
    path.iter()
        .cloned()
        .map(|mut stage| {
            if skips_yours && let Some(d) = &mut stage.delivery {
                d.automatic = false;
            }
            stage
        })
        .collect()
}

/// A stage in the one-line path: "Dev · Quick developer · haiku", "Grill skipped", "Review", "Done".
fn summary_part(stage: &PathStage, tools: &Tools) -> String {
    if stage.skipped {
        return format!("{} skipped", stage.stage);
    }
    match &stage.agent {
        Some(agent) if runs_agent(stage) && stage.kind != PathKind::HumanCheck => format!(
            "{} \u{b7} {} \u{b7} {}",
            stage.stage,
            agent.name,
            agent_model(agent, tools, false)
        ),
        _ => stage.stage.clone(),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Sessions as the token line and History name them

/// What a model passed resolves to today, from the list ("opus" → "claude-opus-5-5"; tool default → the list's
/// default entry's): the started row's model until the tool reports the one that ran.
fn resolves_to(model: Option<&str>, models: Option<&ModelList>) -> Option<String> {
    let list = models?;
    let entry = match model {
        Some(model) => list.entry(model)?,
        None => list.tool_default()?,
    };
    entry.resolves_to.clone().or_else(|| match entry.kind {
        ModelKind::Exact => Some(entry.id.clone()),
        _ => None,
    })
}

/// The exact model a session ran on: the tool's report, else what its model resolves to, else the model passed
/// (or "tool default").
fn exact_model(session: &SessionAgentRun, models: Option<&ModelList>) -> String {
    session
        .model_ran
        .clone()
        .or_else(|| resolves_to(session.model.as_deref(), models))
        .or_else(|| session.model.clone())
        .unwrap_or_else(|| TOOL_DEFAULT.to_owned())
}

/// The display name of a follow choice ("Sonnet" for `sonnet`), for "Sonnet → claude-sonnet-5-6".
fn follow_name(model: Option<&str>, models: Option<&ModelList>) -> Option<String> {
    let entry = models?.entry(model?)?;
    (entry.kind == ModelKind::Follow).then(|| entry.display_name.clone())
}

/// The tool a session ran on: its own, else its agent's now, else the first.
fn session_tool(t: &TicketAgents, session: &SessionAgentRun, tools: &Tools) -> Tool {
    session
        .tool
        .or_else(|| {
            let id = session.agent_id.as_deref()?;
            t.agents
                .iter()
                .find(|a| a.id == id)
                .and_then(|a| tools.tool_of(a))
        })
        .unwrap_or(Tool::ALL[0])
}

/// Whether a line about `name` on `tool` names the tool: only with several tools connected, and never when it is
/// the agent's name (AGT-04: "Codex · gpt-5.5 · started").
fn names_tool(tools: &Tools, tool: Tool, name: &str) -> bool {
    tools.several()
        && !name.eq_ignore_ascii_case(tool.display_name())
        && !name.eq_ignore_ascii_case(tool_word(tool))
}

/// "Strong · Claude Code", "Verifier · Copilot": a name with its tool in a sentence, where it is named.
fn name_with_tool(tools: &Tools, tool: Tool, name: &str) -> String {
    if names_tool(tools, tool, name) {
        format!("{name} \u{b7} {}", tool_word(tool))
    } else {
        name.to_owned()
    }
}

/// "asked for gpt-5.5, ran on auto" (AGT-05, d0088): the session's tool swaps a refused model quietly, the model
/// passed was an exact one, and the tool reported another. Copilot's Auto or a follow choice picking a model is
/// never a swap (Q-J).
pub fn swap_note(session: &SessionAgentRun, tool: Tool, tools: &Tools) -> Option<String> {
    let asked = session.model.as_deref()?;
    let ran = session.model_ran.as_deref()?;
    let exact = match tools.list(tool).and_then(|l| l.entry(asked)) {
        Some(entry) => entry.kind == ModelKind::Exact,
        None => FollowChoice::parse(asked).is_none(),
    };
    // The gate's own rule (`mark::is_swap`): a variant of the asked model (`#high`) is the same model.
    is_swap(tool.swaps_refused_model(), exact, Some(asked), ran)
        .then(|| format!("asked for {asked}, ran on {ran}"))
}

fn sessions_of(
    t: &TicketAgents,
    role: SessionRole,
) -> impl DoubleEndedIterator<Item = &SessionAgentRun> {
    t.sessions.iter().filter(move |s| s.role == role)
}

/// Rounds that counted (a pass or a fail), and the one running.
fn counted_rounds(t: &TicketAgents) -> u32 {
    sessions_of(t, SessionRole::Validator)
        .filter(|s| s.running || matches!(s.round_outcome.as_deref(), Some("pass" | "fail")))
        .count() as u32
}

fn finished(t: &TicketAgents) -> bool {
    t.status.is_some_and(|s| s.set() == StatusSet::Done)
}

/// The callouts of `t`'s held sessions (§7.2, SCH-04 `[M23]`): a model that can't run, a tool that isn't available
/// or is at its limit, each with Use fallback and Open agent….
pub fn callouts(t: &TicketAgents, tools: &Tools) -> Vec<Callout> {
    tool_problems::hold_callouts(&t.holds, &t.agents, tools)
}

/// M31's recorded row changes: "set by you" and "Let agents pick again", as History quotes them.
const SET_BY_YOU: &str = "set by you";
const LET_AGENTS_PICK: &str = "Let agents pick again";

// ---------------------------------------------------------------------------------------------------------------
// The token line

fn tokens_text(tokens: Option<u64>) -> String {
    tokens.map_or_else(|| "\u{2014}".to_owned(), tokens_label)
}

/// One part of the line: each agent in turn with its tokens ([`TicketAgents::tokens`]), "Fable test 2k → Strong
/// 50k"; a part on no agent is named after the role.
fn agent_part(t: &TicketAgents, role: SessionRole, tools: &Tools) -> Option<String> {
    let parts: Vec<String> = t
        .tokens
        .iter()
        .filter(|p| p.role == role)
        .map(|p| {
            let name = p.agent.clone().unwrap_or_else(|| role.label().to_owned());
            let name = match p.tool {
                Some(tool) => name_with_tool(tools, tool, &name),
                None => name,
            };
            format!("{name} {}", tokens_text(p.tokens.map(|t| t.shown())))
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" \u{2192} "))
}

/// The sheet header's tokens by agent (VAL-14, d0086, Q-G): "Strong 182k · no validation yet", "Strong 61k · Light
/// 23k (1 round)", "Strong 61k · Fable test 1k (no result)"; an epic's lead "Strong 48k". `None` before any agent
/// session. The tool is never named while one tool is connected; with several, "Strong · Claude Code 58k ·
/// Verifier · Copilot 9k (1 round)" (USG-10, d0086), the tool left out where it is the agent's name.
pub fn token_line(t: &TicketAgents, tools: &Tools) -> Option<String> {
    if t.kind == Kind::Epic {
        return agent_part(t, SessionRole::EpicLead, tools);
    }
    let developer = agent_part(t, SessionRole::Developer, tools)?;
    let Some(validator) = agent_part(t, SessionRole::Validator, tools) else {
        return Some(format!("{developer} \u{b7} no validation yet"));
    };
    let rounds = counted_rounds(t);
    let suffix = if rounds == 0 {
        "(no result)".to_owned()
    } else {
        format!(
            "({rounds} {})",
            if rounds == 1 { "round" } else { "rounds" }
        )
    };
    Some(format!("{developer} \u{b7} {validator} {suffix}"))
}

// ---------------------------------------------------------------------------------------------------------------
// History

/// A History line of the agents (`history.rs` adds them to Activity's timeline).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryLine {
    /// A session's own rows (Q432, `s25-07`): its start "Grill started · Griller" with "Claude Code · sonnet" under
    /// it ("Claude Code · claude-sonnet-5-6 (Sonnet → claude-sonnet-5-6)" when it moved on) and the swap note ("asked
    /// for gpt-5.5, ran on auto"), and its end "Grill finished · Griller".
    Session {
        session: String,
        started: String,
        detail: String,
        note: Option<String>,
        ended: String,
    },
    /// A line of its own: "Session ended · Sonnet · claude-sonnet-5-5 · 54k", "Developer: Light (deleted) ·
    /// claude-haiku-5-5 · 48k".
    Line {
        at: i64,
        title: String,
        tokens: String,
    },
}

/// The stage a session ran ("Grill"), from the ticket's stage runs; its role ("Developer") when no run names it.
fn session_stage(t: &TicketAgents, session: &SessionAgentRun) -> String {
    t.runs
        .iter()
        .rev()
        .find(|r| r.session.as_deref() == Some(session.id.as_str()))
        .map(|r| {
            t.path
                .iter()
                .find(|p| p.stage_id == r.stage)
                .map(|p| p.stage.clone())
                .or_else(|| {
                    t.stages
                        .iter()
                        .find(|s| s.stage_id == r.stage)
                        .map(|s| s.stage.clone())
                })
                .unwrap_or_else(|| r.stage.clone())
        })
        .unwrap_or_else(|| session.role.label().to_owned())
}

/// Each named session's start and end with its stage, tool and exact model, and a Done or Closed ticket's agents as
/// they ran.
pub fn history_lines(t: &TicketAgents, tools: &Tools) -> Vec<HistoryLine> {
    let mut lines = Vec::new();
    for session in &t.sessions {
        let Some(name) = session.agent_name.clone().filter(|n| !n.is_empty()) else {
            continue;
        };
        let tool = session_tool(t, session, tools);
        let models = tools.list(tool);
        let exact = exact_model(session, models);
        let stage = session_stage(t, session);
        let mut detail = format!("{} \u{b7} {exact}", tool_word(tool));
        if let Some(moved) = &session.moved_on {
            let follow = follow_name(session.model.as_deref(), models).unwrap_or(name.clone());
            detail.push_str(&format!(" ({follow} \u{2192} {})", moved.now));
        }
        lines.push(HistoryLine::Session {
            session: session.id.clone(),
            started: format!("{stage} started \u{b7} {name}"),
            detail,
            note: swap_note(session, tool, tools),
            ended: format!("{stage} finished \u{b7} {name}"),
        });
    }
    if finished(t) {
        for role in SessionRole::ALL {
            let ran: Vec<&SessionAgentRun> = sessions_of(t, *role).collect();
            let Some(last) = ran.last() else { continue };
            let tool = session_tool(t, last, tools);
            let exact = exact_model(last, tools.list(tool));
            let deleted = last
                .agent_id
                .as_deref()
                .is_some_and(|id| !t.agents.iter().any(|a| a.id == id));
            let name = last.agent_name.clone().unwrap_or_default();
            let tokens: Option<u64> = ran.iter().filter_map(|s| s.tokens).reduce(|a, b| a + b);
            let exact = if names_tool(tools, tool, &name) {
                format!("{} \u{b7} {exact}", tool_word(tool))
            } else {
                exact
            };
            lines.push(HistoryLine::Line {
                at: last.ended_at.unwrap_or(last.started_at),
                title: format!(
                    "{}: {name}{} \u{b7} {exact} \u{b7} {}",
                    role.label(),
                    if deleted { " (deleted)" } else { "" },
                    tokens_text(tokens)
                ),
                tokens: "\u{2014}".to_owned(),
            });
        }
    }
    lines
}

fn text(p: &serde_json::Value, field: &str) -> Option<String> {
    p.get(field)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// A recorded role ("developer", "lead") as History says it.
fn role_label(p: &serde_json::Value) -> &'static str {
    text(p, "role")
        .and_then(|r| r.parse::<SessionRole>().ok())
        .map_or("Agent", SessionRole::label)
}

/// Why a recorded row moved: "Claude Code limit", "Codex not available"; `None` for the user's own move.
fn move_reason(reason: &str, tool: Option<Tool>) -> Option<String> {
    let tool = tool.map_or("its tool", tool_word);
    match reason {
        "tool_limit" => Some(format!("{tool} limit")),
        "tool_unavailable" => Some(format!("{tool} not available")),
        _ => None,
    }
}

/// A recorded row's move (AGT-08, M31's `agent_row_set` with `moved_reason`): "Moved to Strong GPT (Codex): Claude
/// Code limit" by the automatic fallback; "Developer: Strong GPT (moved by you from Strong · Claude Code limit)" by
/// Use fallback or Move waiting to…. `None` for a row change that isn't a move.
pub fn move_line(p: &serde_json::Value) -> Option<String> {
    let reason = text(p, "moved_reason")?;
    let role = role_label(p);
    let agent = text(p, "agent").unwrap_or_default();
    let from = text(p, "from").unwrap_or_default();
    let why = move_reason(&reason, text(p, "moved_tool").and_then(|t| t.parse().ok()));
    let by_user = p
        .get("moved_by_user")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if by_user || why.is_none() {
        return Some(match why {
            Some(why) => format!("{role}: {agent} (moved by you from {from} \u{b7} {why})"),
            None => format!("{role}: {agent} (moved by you from {from})"),
        });
    }
    let target = match text(p, "tool").and_then(|t| t.parse::<Tool>().ok()) {
        Some(tool) => format!("{agent} ({})", tool_word(tool)),
        None => agent,
    };
    Some(format!("Moved to {target}: {}", why.unwrap_or_default()))
}

/// Use fallback's History line (AGM-03, `fallback_used`): "Developer started on Codex: Claude Code limit (by
/// you)".
pub fn fallback_line(p: &serde_json::Value) -> String {
    let tool = |field| {
        text(p, field)
            .and_then(|t| t.parse::<Tool>().ok())
            .map_or("its fallback", tool_word)
    };
    let why = text(p, "reason")
        .and_then(|r| move_reason(&r, text(p, "from_tool").and_then(|t| t.parse().ok())))
        .or_else(|| {
            (text(p, "reason").as_deref() == Some("model_unavailable"))
                .then(|| "Model unavailable".to_owned())
        });
    match why {
        Some(why) => format!(
            "{} started on {}: {why} (by you)",
            role_label(p),
            tool("tool")
        ),
        None => format!("{} started on {} (by you)", role_label(p), tool("tool")),
    }
}

/// A recorded row change's History line (M31's `agent_row_set`): "Developer: Strong (set by you)", "Developer: Let
/// agents pick again (by you)"; an agent's pick "Developer: Light (picked by Checkout slowness)" with its reason; on
/// an epic for a child "Wishlist lead picked Standard for SHOP-40.4" or "Wishlist lead re-picked SHOP-40.3: Strong →
/// Light".
pub fn row_change_line(p: &serde_json::Value) -> (String, Option<String>) {
    if let Some(line) = move_line(p) {
        return (line, None);
    }
    let role = role_label(p);
    let agent = text(p, "agent");
    let from = text(p, "from");
    let reason = text(p, "reason");
    match (text(p, "set_by").as_deref(), text(p, "picked_by")) {
        (Some("agent"), Some(by)) => {
            let agent = agent.unwrap_or_default();
            match (text(p, "child"), from) {
                (Some(child), Some(from)) => (
                    format!("{by} re-picked {child}: {from} \u{2192} {agent}"),
                    reason,
                ),
                (Some(child), None) => (format!("{by} picked {agent} for {child}"), reason),
                (None, _) => (format!("{role}: {agent} (picked by {by})"), reason),
            }
        }
        _ => match agent {
            Some(agent) => (format!("{role}: {agent} ({SET_BY_YOU})"), None),
            None => (format!("{role}: {LET_AGENTS_PICK} (by you)"), None),
        },
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The gallery's "05/08 · Stage choices and the path"

/// One sample: its caption, the form's or the sheet's choices, the path and the header's token line.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "StageChoicesSample")]
pub struct GallerySample {
    pub caption: &'static str,
    pub choices: Option<ChoicesView>,
    pub path: Option<PathView>,
    pub token_line: Option<String>,
    /// The held sessions' callouts above the bar (M23).
    pub callouts: Vec<Callout>,
}

/// The states of `s08-29`, `s05-36` and `s05-38`, and the default flow's path.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "StageChoicesGallery")]
pub struct Gallery {
    pub samples: Vec<GallerySample>,
}

pub fn gallery() -> Gallery {
    use fixtures::*;
    let tools = tools();
    let sheet = |caption, t: TicketAgents| GallerySample {
        caption,
        choices: sheet_choices(&t),
        path: path(&t, &tools),
        token_line: token_line(&t, &tools),
        callouts: callouts(&t, &tools),
    };
    Gallery {
        samples: vec![
            GallerySample {
                caption: "\u{2318}N \u{b7} Stage choices (shop-web)",
                choices: form_choices(&new_ticket(), "shop-web"),
                path: None,
                token_line: None,
                callouts: Vec::new(),
            },
            sheet("Mid-flow (SHOP-29)", shop_29()),
            sheet("After a Fail (SHOP-29)", shop_29_after_fail()),
            sheet("Finished (SHOP-41)", shop_41()),
            sheet("A choice you set (SHOP-31)", shop_31()),
            sheet("The default flow (NOTE-2)", note_2()),
        ],
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Fixtures (the gallery and the tests)

pub mod fixtures {
    //! The sample of *update AG*: shop-web's flow (Grill · Dev · Validate · Security · Review · Done · PR,
    //! automatic) with Griller and Rust developer on sonnet, Quick developer and Shipper on haiku, Validator on Codex
    //! · gpt-5.5, Security reviewer on opus · effort high; and pocket-notes on the default flow (Developer on haiku,
    //! Validator on opus).

    use otaq_core::agent_tools::Tool;
    use otaq_core::agent_tools::models::{ModelEntry, ModelKind, ModelList};
    use otaq_core::agents::facts::{PathDelivery, PathKind, PathStage, TicketAgents};
    use otaq_core::agents::store::SessionAgentRun;
    use otaq_core::agents::{Agent, AgentModel, CameFrom, SessionRole};
    use otaq_core::board::stage_choices::{
        Choice, OptionAgent, SetBy, StageChoice, StageOptions, TicketChoices,
    };
    use otaq_core::board::stage_runs::StageRun;
    use otaq_core::domain::{Kind, Place, Status};
    use otaq_core::flow::engine::RunResult;

    use crate::view_models::tool_problems::Tools;

    pub const CLAUDE: Tool = Tool::ALL[0];
    pub const CODEX: Tool = Tool::ALL[1];

    fn entry(id: &str, display: &str, kind: ModelKind, resolves: Option<&str>) -> ModelEntry {
        ModelEntry {
            id: id.into(),
            display_name: display.into(),
            description: None,
            resolves_to: resolves.map(str::to_owned),
            kind,
            family: None,
            released_at: None,
        }
    }

    /// Claude Code's list and Codex's.
    pub fn tools() -> Tools {
        let claude = ModelList {
            entries: vec![
                entry(
                    "default",
                    "Default (recommended)",
                    ModelKind::ToolDefault,
                    Some("claude-opus-5-5"),
                ),
                entry("opus", "Opus", ModelKind::Follow, Some("claude-opus-5-5")),
                entry(
                    "sonnet",
                    "Sonnet",
                    ModelKind::Follow,
                    Some("claude-sonnet-5-5"),
                ),
                entry(
                    "haiku",
                    "Haiku",
                    ModelKind::Follow,
                    Some("claude-haiku-5-5"),
                ),
            ],
        };
        let codex = ModelList {
            entries: vec![entry("gpt-5.5", "gpt-5.5", ModelKind::Exact, None)],
        };
        let mut tools = Tools::one(Some(claude));
        tools.lists.push((CODEX, codex));
        tools
    }

    /// An agent on `tool` · `model`.
    pub fn agent(id: &str, name: &str, tool: Tool, model: &str) -> Agent {
        Agent {
            id: id.into(),
            name: name.into(),
            instructions: String::new(),
            skills: Vec::new(),
            imported: Vec::new(),
            model: AgentModel::Tool {
                tool,
                model: Some(model.into()),
            },
            effort: None,
            role: None,
            came_from: CameFrom::You,
            mark: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    pub fn shop_agents() -> Vec<Agent> {
        let mut security = agent("security", "Security reviewer", CLAUDE, "opus");
        security.effort = Some("high".into());
        vec![
            agent("griller", "Griller", CLAUDE, "sonnet"),
            agent("rust", "Rust developer", CLAUDE, "sonnet"),
            agent("quick", "Quick developer", CLAUDE, "haiku"),
            agent("validator", "Validator", CODEX, "gpt-5.5"),
            security,
            agent("shipper", "Shipper", CLAUDE, "haiku"),
        ]
    }

    fn find(agents: &[Agent], id: &str) -> Option<Agent> {
        agents.iter().find(|a| a.id == id).cloned()
    }

    fn stage(id: &str, name: &str, kind: PathKind, agent: Option<Agent>) -> PathStage {
        PathStage {
            stage_id: id.into(),
            stage: name.into(),
            kind,
            shown_in: None,
            rounds: None,
            delivery: None,
            agent,
            skipped: false,
        }
    }

    /// shop-web's path with Dev on `dev`.
    pub fn shop_path(dev: &str) -> Vec<PathStage> {
        let a = shop_agents();
        vec![
            stage("grill", "Grill", PathKind::Agent, find(&a, "griller")),
            stage("dev", "Dev", PathKind::Agent, find(&a, dev)),
            PathStage {
                shown_in: Some("Dev".into()),
                rounds: Some(3),
                ..stage(
                    "validate",
                    "Validate",
                    PathKind::AgentCheck,
                    find(&a, "validator"),
                )
            },
            PathStage {
                rounds: Some(3),
                ..stage(
                    "security",
                    "Security",
                    PathKind::AgentCheck,
                    find(&a, "security"),
                )
            },
            stage("review", "Review", PathKind::HumanCheck, None),
            PathStage {
                delivery: Some(PathDelivery {
                    automatic: true,
                    delivers: "pr".into(),
                }),
                ..stage("ship", "Done", PathKind::Delivery, find(&a, "shipper"))
            },
        ]
    }

    fn option(name: &str, id: &str) -> OptionAgent {
        OptionAgent {
            name: name.into(),
            agent_id: Some(id.into()),
        }
    }

    /// shop-web's stages with options, nobody chose.
    pub fn shop_options() -> Vec<StageOptions> {
        let options = |id: &str, name: &str, usual: Option<OptionAgent>| StageOptions {
            stage_id: id.into(),
            stage: name.into(),
            usual,
            agents: Vec::new(),
            can_skip: true,
            choice: None,
            locked: false,
        };
        vec![
            options("grill", "Grill", Some(option("Griller", "griller"))),
            StageOptions {
                agents: vec![option("Quick developer", "quick")],
                can_skip: false,
                ..options("dev", "Dev", Some(option("Rust developer", "rust")))
            },
            options("review", "Review", None),
        ]
    }

    /// The ⌘N form's choices in shop-web.
    pub fn new_ticket() -> TicketChoices {
        TicketChoices {
            key: None,
            stages: shop_options(),
        }
    }

    pub fn chose(choice: Choice, set_by: SetBy, from: Option<&str>) -> Option<StageChoice> {
        Some(StageChoice {
            stage_id: String::new(),
            choice,
            set_by,
            set_at: 1,
            from: from.map(str::to_owned),
        })
    }

    pub fn by_project_agent(reason: &str) -> SetBy {
        SetBy::ProjectAgent {
            name: "Checkout slowness".into(),
            reason: reason.into(),
        }
    }

    /// One stage run.
    pub fn run(
        stage: &str,
        session: Option<&str>,
        agent: Option<&str>,
        model: Option<&str>,
        result: Option<RunResult>,
    ) -> StageRun {
        StageRun {
            stage: stage.into(),
            kind: "agent".into(),
            session: session.map(str::to_owned),
            agent: agent.map(str::to_owned),
            model: model.map(str::to_owned),
            round: None,
            result,
            summary: None,
            started_at: 1,
            ended_at: result.map(|_| 2),
        }
    }

    pub fn round(mut r: StageRun, n: u32, summary: Option<&str>) -> StageRun {
        r.kind = "check".into();
        r.round = Some(n);
        r.summary = summary.map(str::to_owned);
        r
    }

    /// A session of `role` on `agent`, passing `model` and running on `ran`.
    pub fn session(
        id: &str,
        role: SessionRole,
        agent: &str,
        tool: Tool,
        model: Option<&str>,
        ran: Option<&str>,
    ) -> SessionAgentRun {
        SessionAgentRun {
            id: id.into(),
            role,
            running: false,
            started_at: 1,
            ended_at: Some(2),
            tool: Some(tool),
            agent_id: None,
            agent_name: Some(agent.into()),
            model: model.map(str::to_owned),
            model_ran: ran.map(str::to_owned),
            effort: None,
            tokens: Some(10_000),
            round_outcome: None,
            moved_on: None,
        }
    }

    pub fn ticket(
        key: &str,
        status: Status,
        place: Place,
        path: Vec<PathStage>,
        choices: Vec<StageOptions>,
        runs: Vec<StageRun>,
        sessions: Vec<SessionAgentRun>,
    ) -> TicketAgents {
        TicketAgents {
            key: Some(key.parse().expect("key")),
            kind: Kind::Ticket,
            status: Some(status),
            agents: shop_agents(),
            stages: Vec::new(),
            lead: None,
            sessions,
            tokens: Vec::new(),
            holds: Vec::new(),
            place: Some(place),
            path,
            runs,
            choices,
        }
    }

    /// SHOP-29 "Coupon field" in Dev on Rust developer after Grill, the Project agent's choices, Griller's change of
    /// Dev (`s05-36`).
    pub fn shop_29() -> TicketAgents {
        let mut choices = shop_options();
        choices[0].choice = chose(Choice::Usual, by_project_agent("needs questions"), None);
        choices[0].locked = true;
        choices[1].choice = chose(
            Choice::Usual,
            SetBy::Agent {
                name: "Griller".into(),
                reason: "touches session handling".into(),
            },
            Some("Quick developer"),
        );
        choices[1].locked = true;
        ticket(
            "SHOP-29",
            Status::Working,
            Place::InStage("dev".into()),
            shop_path("rust"),
            choices,
            vec![
                run(
                    "grill",
                    Some("g"),
                    Some("Griller"),
                    Some("claude-sonnet-5-5"),
                    Some(RunResult::Done),
                ),
                run("dev", Some("d"), Some("Rust developer"), None, None),
            ],
            vec![
                session(
                    "g",
                    SessionRole::Developer,
                    "Griller",
                    CLAUDE,
                    Some("sonnet"),
                    Some("claude-sonnet-5-5"),
                ),
                SessionAgentRun {
                    running: true,
                    ended_at: None,
                    ..session(
                        "d",
                        SessionRole::Developer,
                        "Rust developer",
                        CLAUDE,
                        Some("sonnet"),
                        Some("claude-sonnet-5-5"),
                    )
                },
            ],
        )
    }

    /// SHOP-29 back in Dev after Security failed round 1 (`s05-36`, "After a Fail").
    pub fn shop_29_after_fail() -> TicketAgents {
        let mut t = shop_29();
        t.choices[1].choice = chose(Choice::Usual, by_project_agent("session handling"), None);
        let mut security = session(
            "s",
            SessionRole::Validator,
            "Security reviewer",
            CLAUDE,
            Some("opus"),
            Some("claude-opus-5-5"),
        );
        security.effort = Some("high".into());
        t.runs = vec![
            t.runs[0].clone(),
            run(
                "dev",
                Some("d1"),
                Some("Rust developer"),
                Some("claude-sonnet-5-5"),
                Some(RunResult::Done),
            ),
            round(
                run(
                    "validate",
                    Some("v"),
                    Some("Validator"),
                    Some("gpt-5.5"),
                    Some(RunResult::Passed),
                ),
                1,
                None,
            ),
            round(
                run(
                    "security",
                    Some("s"),
                    Some("Security reviewer"),
                    Some("claude-opus-5-5"),
                    Some(RunResult::Failed),
                ),
                1,
                Some("Unescaped SQL in search.\nlib/search.rs:40"),
            ),
            run("dev", Some("d"), Some("Rust developer"), None, None),
        ];
        t.sessions.push(security);
        t
    }

    /// SHOP-41 "Fix README typo", merged: Grill and Review skipped, Dev on Quick developer (`s05-38`).
    pub fn shop_41() -> TicketAgents {
        let mut choices = shop_options();
        let typo = by_project_agent("typo fix");
        for c in &mut choices {
            c.locked = true;
        }
        choices[0].choice = chose(Choice::Skip, typo.clone(), None);
        choices[1].choice = chose(
            Choice::Agent {
                id: "quick".into(),
                name: "Quick developer".into(),
            },
            typo.clone(),
            None,
        );
        choices[2].choice = chose(Choice::Skip, typo, None);
        let mut path = shop_path("quick");
        path[0].skipped = true;
        path[4].skipped = true;
        let mut security = session(
            "s",
            SessionRole::Validator,
            "Security reviewer",
            CLAUDE,
            Some("opus"),
            Some("claude-opus-5-5"),
        );
        security.effort = Some("high".into());
        ticket(
            "SHOP-41",
            Status::Merged,
            Place::Done,
            path,
            choices,
            vec![
                run("grill", None, None, None, Some(RunResult::Skipped)),
                run(
                    "dev",
                    Some("d"),
                    Some("Quick developer"),
                    Some("claude-haiku-5-5"),
                    Some(RunResult::Done),
                ),
                round(
                    run(
                        "validate",
                        Some("v"),
                        Some("Validator"),
                        Some("gpt-5.5"),
                        Some(RunResult::Passed),
                    ),
                    1,
                    None,
                ),
                round(
                    run(
                        "security",
                        Some("s"),
                        Some("Security reviewer"),
                        Some("claude-opus-5-5"),
                        Some(RunResult::Passed),
                    ),
                    1,
                    None,
                ),
                run("review", None, None, None, Some(RunResult::Skipped)),
                run(
                    "ship",
                    Some("p"),
                    Some("Shipper"),
                    Some("claude-haiku-5-5"),
                    Some(RunResult::Done),
                ),
            ],
            vec![security],
        )
    }

    /// SHOP-31 "Hotfix: checkout crash" in Backlog: you chose Quick developer for Dev (`s05-38`).
    pub fn shop_31() -> TicketAgents {
        let mut choices = shop_options();
        choices[1].choice = chose(
            Choice::Agent {
                id: "quick".into(),
                name: "Quick developer".into(),
            },
            SetBy::User,
            None,
        );
        ticket(
            "SHOP-31",
            Status::Backlog,
            Place::Backlog,
            shop_path("quick"),
            choices,
            Vec::new(),
            Vec::new(),
        )
    }

    /// pocket-notes on the default flow: Developer on haiku, Validator on opus.
    pub fn default_path() -> Vec<PathStage> {
        let developer = agent("developer", "Developer", CLAUDE, "haiku");
        vec![
            stage("dev", "Dev", PathKind::Agent, Some(developer.clone())),
            PathStage {
                shown_in: Some("Dev".into()),
                rounds: Some(2),
                ..stage(
                    "validate",
                    "Validate",
                    PathKind::AgentCheck,
                    Some(agent("validator", "Validator", CLAUDE, "opus")),
                )
            },
            stage("review", "Review", PathKind::HumanCheck, None),
            PathStage {
                shown_in: Some("Review".into()),
                delivery: Some(PathDelivery {
                    automatic: false,
                    delivers: "either".into(),
                }),
                ..stage("deliver", "Done", PathKind::Delivery, Some(developer))
            },
        ]
    }

    /// NOTE-2 in Ready for Dev on the default flow: no choices, only the path.
    pub fn note_2() -> TicketAgents {
        ticket(
            "NOTE-2",
            Status::Queued,
            Place::Waiting(None),
            default_path(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::board::stage_choices::Choice;
    use serde_json::json;

    use super::fixtures::*;
    use super::*;

    fn texts(p: &PathView) -> Vec<(String, String, String, bool)> {
        p.rows
            .iter()
            .map(|r| {
                (
                    r.stage.clone(),
                    r.detail.clone(),
                    r.model.clone(),
                    r.lock.is_some(),
                )
            })
            .collect()
    }

    fn row(stage: &str, detail: &str, model: &str, lock: bool) -> (String, String, String, bool) {
        (stage.into(), detail.into(), model.into(), lock)
    }

    /// AGM-09, `s08-29`: the ⌘N form's three pickers on the test flow, each on its stage's own agent.
    #[test]
    fn the_form_has_a_picker_per_stage_with_options() {
        let view = form_choices(&new_ticket(), "shop-web").expect("choices");
        assert_eq!(view.heading, "Stage choices");
        let shown: Vec<_> = view
            .rows
            .iter()
            .map(|r| (r.label.as_str(), r.value.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                // FLW-22 (Q290): a skippable Agent stage shows its agent, a skippable human check "required".
                ("Grill:", "Griller"),
                ("Dev:", "Rust developer"),
                ("Review:", "required")
            ]
        );
        let words: Vec<Vec<&str>> = view
            .rows
            .iter()
            .map(|r| r.items.iter().map(|i| i.label.as_str()).collect())
            .collect();
        assert_eq!(
            words,
            [
                vec!["Griller", "skip"],
                vec!["Rust developer", "Quick developer"],
                vec!["required", "skip"]
            ]
        );
        assert!(
            view.rows
                .iter()
                .all(|r| r.lock.is_none() && r.change.is_none())
        );
        assert_eq!(
            view.note.as_deref(),
            Some(
                "Only stages with options in shop-web\u{2019}s flow. You can change them until that stage starts."
            )
        );
        // Only the picks that aren't the stage's own agent go with the new ticket.
        let picks = picks_to_send(
            &new_ticket(),
            &[
                ("grill".into(), "Griller".into()),
                ("dev".into(), "Quick developer".into()),
                ("review".into(), "skip".into()),
                ("dev".into(), "Nobody".into()),
            ],
        );
        assert_eq!(
            picks,
            [
                ("dev".to_owned(), "Quick developer".to_owned()),
                ("review".to_owned(), "skip".to_owned())
            ]
        );
    }

    /// AGM-09, `s05-36`: mid-flow, the started stages are locked; Griller's change says who and why in violet; the
    /// path by agent and model, the stage that ran with its exact model and a lock.
    #[test]
    fn mid_flow_the_panel_shows_the_choices_and_the_path() {
        let t = shop_29();
        let choices = sheet_choices(&t).expect("choices");
        assert_eq!(choices.heading, "Choices");
        assert_eq!(choices.set_by.as_deref(), Some("set by Noyan"));
        let locks: Vec<_> = choices.rows.iter().map(|r| r.lock.as_deref()).collect();
        assert_eq!(
            locks,
            [
                Some("Locked: Grill has started"),
                Some("Locked: Dev has started"),
                None
            ]
        );
        assert!(choices.rows[0].items.is_empty() && choices.rows[1].items.is_empty());
        assert_eq!(choices.rows[2].value, "required");
        assert_eq!(
            choices.rows[1].change,
            Some(ChoiceChange {
                text: "Dev: Quick developer \u{2192} Rust developer \u{b7} by Griller \u{b7} touches session \
                       handling"
                    .into(),
                agent_made: true,
                icon: IconName::Bot,
            })
        );
        assert!(choices.rows[0].change.is_none());
        let p = path(&t, &tools()).expect("path");
        assert_eq!(p.heading, "Path");
        assert_eq!(
            p.summary,
            "Grill \u{b7} Griller \u{b7} sonnet \u{b7} Dev \u{b7} Rust developer \u{b7} sonnet \u{b7} Validate \
             \u{b7} Validator \u{b7} gpt-5.5 \u{b7} Security \u{b7} Security reviewer \u{b7} opus \u{b7} Review \
             \u{b7} Done \u{b7} Shipper \u{b7} haiku"
        );
        assert_eq!(
            texts(&p),
            [
                row("Grill", "Griller", "claude-sonnet-5-5", true),
                row("Dev", "Rust developer \u{b7} working", "sonnet", false),
                row(
                    "Validate",
                    "Validator \u{b7} shown in Dev \u{b7} up to 3 rounds",
                    "gpt-5.5",
                    false
                ),
                row(
                    "Security",
                    "Security reviewer \u{b7} up to 3 rounds",
                    "opus \u{b7} effort high",
                    false
                ),
                row("Review", "your check", "\u{2014}", false),
                row("Done", "Shipper \u{b7} PR, automatic", "haiku", false),
            ]
        );
        let states: Vec<_> = p.rows.iter().map(|r| (r.state, r.current)).collect();
        assert_eq!(
            states,
            [
                (PathState::Passed, false),
                (PathState::Running, true),
                (PathState::ToCome, false),
                (PathState::ToCome, false),
                (PathState::YourCheck, false),
                (PathState::ToCome, false),
            ]
        );
    }

    /// `s05-36` "After a Fail": the failed round stays with what it found; Dev runs again, every later stage too.
    #[test]
    fn a_failed_round_stays_in_the_path() {
        let t = shop_29_after_fail();
        let p = path(&t, &tools()).expect("path");
        assert_eq!(
            p.summary,
            "Back in Dev after Security failed \u{b7} then Validate \u{b7} Security (round 2 of 3) \u{b7} Review \
             \u{b7} Done"
        );
        assert_eq!(
            texts(&p),
            [
                row("Grill", "Griller", "claude-sonnet-5-5", true),
                row(
                    "Dev",
                    "Rust developer \u{b7} round 1",
                    "claude-sonnet-5-5",
                    true
                ),
                row(
                    "Validate",
                    "Validator \u{b7} passed round 1",
                    "gpt-5.5",
                    true
                ),
                row(
                    "Security",
                    "failed round 1 \u{b7} Unescaped SQL in search",
                    "claude-opus-5-5 \u{b7} high",
                    true
                ),
                row(
                    "Dev",
                    "Rust developer \u{b7} again, with the findings",
                    "sonnet",
                    false
                ),
                row("Validate", "again \u{b7} shown in Dev", "gpt-5.5", false),
                row("Security", "round 2 of 3", "opus \u{b7} effort high", false),
                row("Review", "your check", "\u{2014}", false),
                row("Done", "Shipper \u{b7} PR, automatic", "haiku", false),
            ]
        );
        assert_eq!(p.rows[3].state, PathState::Failed);
        assert_eq!(p.rows[3].icon, IconName::CircleX);
        // The Project agent's choices: no change line.
        let choices = sheet_choices(&t).expect("choices");
        assert!(choices.rows.iter().all(|r| r.change.is_none()));
    }

    /// `s05-38` "Finished": the skipped stages marked, every stage that ran with its exact model and a lock; Review
    /// skipped, so Done waited for your click.
    #[test]
    fn a_finished_ticket_shows_what_ran() {
        let t = shop_41();
        let choices = sheet_choices(&t).expect("choices");
        let shown: Vec<_> = choices
            .rows
            .iter()
            .map(|r| (r.value.as_str(), r.lock.is_some(), r.items.is_empty()))
            .collect();
        assert_eq!(
            shown,
            [
                ("skip", true, true),
                ("Quick developer", true, true),
                ("skip", true, true)
            ]
        );
        let p = path(&t, &tools()).expect("path");
        assert_eq!(
            p.summary,
            "Grill skipped \u{b7} Dev \u{b7} Quick developer \u{b7} haiku \u{b7} Validate \u{b7} Validator \u{b7} \
             gpt-5.5 \u{b7} Security \u{b7} Security reviewer \u{b7} opus \u{b7} Review skipped \u{b7} Done"
        );
        assert_eq!(
            texts(&p),
            [
                row(
                    "Grill",
                    "Skipped Grill (this ticket\u{2019}s choice)",
                    "\u{2014}",
                    false
                ),
                row("Dev", "Quick developer", "claude-haiku-5-5", true),
                row(
                    "Validate",
                    "Validator \u{b7} passed round 1",
                    "gpt-5.5",
                    true
                ),
                row(
                    "Security",
                    "Security reviewer \u{b7} passed round 1",
                    "claude-opus-5-5 \u{b7} high",
                    true
                ),
                row(
                    "Review",
                    "Review skipped \u{b7} waited for your click at Done",
                    "\u{2014}",
                    false
                ),
                row("Done", "Shipper \u{b7} delivered", "claude-haiku-5-5", true),
            ]
        );
        assert!(p.rows.iter().all(|r| !r.current));
    }

    /// `s05-38` "A choice you set": yours says so, neutral; nothing is locked before the ticket starts.
    #[test]
    fn a_choice_you_set_says_its_yours() {
        let t = shop_31();
        let choices = sheet_choices(&t).expect("choices");
        assert_eq!(choices.set_by, None);
        assert_eq!(
            choices.rows[1].change,
            Some(ChoiceChange {
                text: "Dev: Rust developer \u{2192} Quick developer \u{b7} by you".into(),
                agent_made: false,
                icon: IconName::User,
            })
        );
        assert!(
            choices
                .rows
                .iter()
                .all(|r| r.lock.is_none() && !r.items.is_empty())
        );
        assert!(
            choices.rows[1]
                .items
                .iter()
                .any(|i| i.checked && i.id == "Quick developer")
        );
        let p = path(&t, &tools()).expect("path");
        assert_eq!(
            p.summary,
            "Grill \u{b7} Griller \u{b7} sonnet \u{b7} Dev \u{b7} Quick developer \u{b7} haiku \u{b7} Validate \
             \u{b7} Validator \u{b7} gpt-5.5 \u{b7} Security \u{b7} Security reviewer \u{b7} opus \u{b7} Review \
             \u{b7} Done \u{b7} Shipper \u{b7} haiku"
        );
        assert!(p.rows.iter().all(|r| r.state != PathState::Running));
    }

    /// AGM-09: on the default flow no stage has options, so the ticket shows only its path.
    #[test]
    fn the_default_flow_shows_only_the_path() {
        let t = note_2();
        assert_eq!(sheet_choices(&t), None);
        let none = TicketChoices {
            key: None,
            stages: Vec::new(),
        };
        assert_eq!(form_choices(&none, "pocket-notes"), None);
        let p = path(&t, &tools()).expect("path");
        assert_eq!(
            p.summary,
            "Dev \u{b7} Developer \u{b7} haiku \u{b7} Validate \u{b7} Validator \u{b7} opus \u{b7} Review \u{b7} Done"
        );
        assert_eq!(
            texts(&p),
            [
                row("Dev", "Developer", "haiku", false),
                row(
                    "Validate",
                    "Validator \u{b7} shown in Dev \u{b7} up to 2 rounds",
                    "opus",
                    false
                ),
                row("Review", "your check", "\u{2014}", false),
                row("Done", "Local merge or PR, your click", "\u{2014}", false),
            ]
        );
    }

    /// An epic has neither choices nor a path.
    #[test]
    fn an_epic_has_no_choices_and_no_path() {
        let mut t = shop_29();
        t.kind = Kind::Epic;
        assert_eq!(sheet_choices(&t), None);
        assert_eq!(path(&t, &tools()), None);
    }

    #[test]
    fn models_read_short() {
        let tools = tools();
        assert_eq!(short_model(None, None), "tool default");
        assert_eq!(
            short_model(Some("default"), tools.list(CLAUDE)),
            "tool default"
        );
        assert_eq!(short_model(Some("sonnet"), tools.list(CLAUDE)), "sonnet");
        assert_eq!(short_model(Some("newest:gpt-sol"), None), "newest gpt-sol");
        assert_eq!(
            short_model(Some("auto@balance"), None),
            "Auto \u{b7} balance"
        );
    }

    #[test]
    fn a_stored_choice_reads_as_its_label() {
        let mut options = shop_options();
        options[0].choice = chose(Choice::Skip, SetBy::User, Some("Griller"));
        assert_eq!(
            change(&options[0]).map(|c| c.text),
            Some("Grill: Griller \u{2192} skip \u{b7} by you".to_owned())
        );
    }

    /// M31's recorded row changes still read in History.
    #[test]
    fn recorded_row_changes_read_as_history_lines() {
        assert_eq!(
            row_change_line(&json!({"role": "developer", "agent": "Strong", "set_by": "user"})).0,
            "Developer: Strong (set by you)"
        );
        assert_eq!(
            row_change_line(&json!({"role": "validator", "set_by": "user"})).0,
            "Validator: Let agents pick again (by you)"
        );
        assert_eq!(
            row_change_line(&json!({
                "role": "developer", "agent": "Light", "set_by": "agent",
                "picked_by": "Checkout slowness", "reason": "footer text only"
            })),
            (
                "Developer: Light (picked by Checkout slowness)".to_owned(),
                Some("footer text only".to_owned())
            )
        );
        assert_eq!(
            move_line(&json!({
                "role": "developer", "agent": "Strong GPT", "from": "Strong", "tool": "codex",
                "moved_reason": "tool_limit", "moved_tool": "claude"
            }))
            .as_deref(),
            Some("Moved to Strong GPT (Codex): Claude Code limit")
        );
    }

    #[test]
    fn use_fallback_reads_as_a_history_line() {
        assert_eq!(
            fallback_line(&json!({
                "role": "developer", "tool": "codex", "from_tool": "claude", "reason": "tool_limit"
            })),
            "Developer started on Codex: Claude Code limit (by you)"
        );
    }

    #[test]
    fn the_token_line_and_history_name_each_agent() {
        let tools = tools();
        let mut t = shop_41();
        t.sessions = vec![
            session(
                "d",
                SessionRole::Developer,
                "Quick developer",
                CLAUDE,
                Some("haiku"),
                Some("claude-haiku-5-5"),
            ),
            SessionAgentRun {
                round_outcome: Some("pass".into()),
                ..session(
                    "v",
                    SessionRole::Validator,
                    "Validator",
                    CLAUDE,
                    Some("opus"),
                    Some("claude-opus-5-5"),
                )
            },
        ];
        t.tokens = Vec::new();
        assert_eq!(token_line(&t, &tools), None);
        let lines = history_lines(&t, &tools);
        let titles: Vec<_> = lines
            .iter()
            .map(|l| match l {
                HistoryLine::Session { started, .. } => started.as_str(),
                HistoryLine::Line { title, .. } => title.as_str(),
            })
            .collect();
        assert_eq!(
            titles,
            [
                "Dev started \u{b7} Quick developer",
                "Validate started \u{b7} Validator",
                "Developer: Quick developer \u{b7} claude-haiku-5-5 \u{b7} 10k",
                "Validator: Validator \u{b7} claude-opus-5-5 \u{b7} 10k",
            ]
        );
        let HistoryLine::Session { detail, ended, .. } = &lines[0] else {
            panic!("a session's rows first");
        };
        assert_eq!(detail, "Claude Code \u{b7} claude-haiku-5-5");
        assert_eq!(ended, "Dev finished \u{b7} Quick developer");
        // A session no stage run names reads by its role.
        t.runs.clear();
        let HistoryLine::Session { started, .. } = &history_lines(&t, &tools)[0] else {
            panic!("a session's rows first");
        };
        assert_eq!(started, "Developer started \u{b7} Quick developer");
    }

    #[test]
    fn the_gallery_has_every_state() {
        let g = gallery();
        assert_eq!(g.samples.len(), 6);
        assert!(g.samples[0].choices.is_some() && g.samples[0].path.is_none());
        assert!(g.samples[1..].iter().all(|s| s.path.is_some()));
        assert!(g.samples[5].choices.is_none());
    }
}

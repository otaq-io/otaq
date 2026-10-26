//! Planning on the board and its human check in the sheet (d0129, d0133, d0093, d0109; canvas `s30-01`, `s30-02`,
//! `s30-05`). Nothing here knows a preset (d0108: no code path only presets use): **planning** is what a human check
//! that isn't the last one before the Delivery reviews ([`checks_planning`]: Spec review, before Code review), and
//! **what planning did** is the Agent stages that ran since the previous human check (or the ticket's start).
//!
//! - **The waiting column** ([`ready_hint`]): "Ready" with "Before the first stage" when the flow's first Agent stage
//!   isn't Dev ([`otaq_core::flow::Flow::waiting_name`]); the default flow keeps "Ready for Dev".
//! - **Cards**: a stage running a skill Otaq ships says what it is doing ("Working · writing the architecture",
//!   [`working_on`]); at a planning check "Your check · the decisions and the plan" (or "· the decisions" with no
//!   pieces, [`check_line`]); a ticket waiting in Ready that starts past planning "Dev next · skips planning"
//!   ([`skips_line`]).
//! - **Overview at a planning check**: WHAT PLANNING DID, one row per Agent stage that ran ([`did`]): "Product spec ·
//!   Spec writer" over "decisions/0140-share.md; DESIGN.md", Milestone plan's "the Plan, 4 pieces; …", then "The docs
//!   are on SHOP-40's branch (Files tab) and ship with its code."
//! - **Files at a planning check**: the source line "otaq/SHOP-40 · what planning committed" with each stage's count
//!   ([`source_stages`]) and a file's "added by Product spec · on SHOP-40's branch, not on main yet" ([`file_tag`]).
//! - **The review bar**: "Your check · Spec review · the decisions on Files and the plan here" ([`bar_line`]).

use otaq_core::board::stage_files::{StageFile, StageFiles};
use otaq_core::board::stage_runs::StageRun;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Place, Status, plan};
use otaq_core::flow::engine::{RunResult, next_human_check};
use otaq_core::flow::model::READY;
use otaq_core::flow::{Flow, Stage};
use otaq_core::git::diff::FileStatus;
use serde::Serialize;
use ts_rs::TS;

/// The sub-label under "Ready" (`s30-01`).
pub const BEFORE_THE_FIRST_STAGE: &str = "Before the first stage";
pub const WHAT_PLANNING_DID: &str = "What planning did";
/// The Files tab's source line after the branch.
pub const WHAT_PLANNING_COMMITTED: &str = "what planning committed";
/// A row's words when its stage committed nothing and left no comment.
pub const NOTHING_COMMITTED: &str = "nothing committed";
/// How many files a row of What planning did names before "+2 more".
const NAMED_FILES: usize = 3;

/// The hint under the waiting column's title: "Before the first stage" under "Ready"; `default` (today's "Starts in
/// priority order") under "Ready for Dev".
pub fn ready_hint(flow: &Flow, default: &'static str) -> &'static str {
    match flow.waiting_name() {
        READY => BEFORE_THE_FIRST_STAGE,
        _ => default,
    }
}

/// A human check with another human check after it: it reviews planning, not the code that ships (Spec review before
/// Code review).
pub fn checks_planning(flow: &Flow, stage: &Stage) -> bool {
    stage.is_human_check() && next_human_check(flow, &stage.id).is_some()
}

/// The planning check `ticket` waits in, if it does.
pub fn planning_check<'f>(ticket: &Ticket, flow: &'f Flow) -> Option<&'f Stage> {
    if ticket.status != Status::WaitingForYou {
        return None;
    }
    let Place::InStage(id) = &ticket.place else {
        return None;
    };
    flow.stage(id).filter(|s| checks_planning(flow, s))
}

/// The Plan has pieces: the check reviews "the decisions and the plan".
fn has_pieces(ticket: &Ticket) -> bool {
    !plan::pieces(&ticket.plan).is_empty()
}

/// The card's words at a planning check (`s30-01`): "the decisions and the plan", or "the decisions" with no pieces.
pub fn check_line(ticket: &Ticket) -> String {
    match has_pieces(ticket) {
        true => "the decisions and the plan".to_owned(),
        false => "the decisions".to_owned(),
    }
}

/// What an agent running a skill Otaq ships is doing, in Otaq's words (as its captions, Q-B): "writing the
/// architecture". `None` for any other skill (the card says "Working").
pub fn working_on(skill: &str) -> Option<&'static str> {
    Some(match skill {
        "product-spec" => "writing the decisions",
        "tech-spec" => "writing the architecture",
        "milestone-plan" => "writing the plan",
        _ => return None,
    })
}

/// The card's words for a ticket waiting in the waiting column that starts past the flow's first Agent stage
/// (`s30-01`'s SHOP-35): "Dev next · skips planning" when it skips a human check too (a whole round of planning),
/// else the stages it skips ("Dev next · skips Product spec"). `starts` is the name of the stage it starts at.
pub fn skips_line(flow: &Flow, ticket: &Ticket, starts: Option<&str>) -> Option<String> {
    if ticket.place != Place::Waiting(None) {
        return None;
    }
    let first = flow.stages.iter().position(Stage::is_agent_stage)?;
    let at = flow
        .stages
        .iter()
        .position(|s| Some(s.name.as_str()) == starts)?;
    let skipped = flow.stages.get(first..at)?;
    if skipped.is_empty() {
        return None;
    }
    let what = if skipped.iter().any(Stage::is_human_check) {
        "planning".to_owned()
    } else {
        names(
            &skipped
                .iter()
                .filter(|s| s.is_agent_stage())
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
        )
    };
    Some(format!("{} next \u{b7} skips {what}", flow.stages[at].name))
}

/// "Product spec", "Product spec and Docs", "Product spec, Docs and Lint".
fn names(list: &[&str]) -> String {
    match list {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

// ---- What planning did ----

/// The Agent stage runs since the previous human check (or the ticket's start), oldest first: what the check at
/// `check` (a stage id) reviews. Its own earlier rounds (a Send back walked the stages again) and checks the ticket
/// skipped don't end it; skipped Agent stages didn't run.
pub fn planning_runs<'r>(runs: &'r [StageRun], check: &str) -> Vec<&'r StageRun> {
    let mut out = Vec::new();
    for run in runs.iter().rev() {
        match run.kind.as_str() {
            "you" if run.stage == check || run.result == Some(RunResult::Skipped) => {}
            "you" => break,
            "agent" if run.result != Some(RunResult::Skipped) => out.push(run),
            _ => {}
        }
    }
    out.reverse();
    out
}

/// One stage's share: its name, its latest run, and the files its runs committed.
struct Share<'a> {
    stage: String,
    last: &'a StageRun,
    files: Vec<&'a StageFile>,
}

/// Each stage of the runs, in the order it first ran, with what its runs committed (a file once).
fn shares<'a>(flow: &Flow, runs: &[&'a StageRun], files: &'a StageFiles) -> Vec<Share<'a>> {
    let mut out: Vec<Share<'a>> = Vec::new();
    for run in runs {
        let committed = files
            .runs
            .iter()
            .filter(|f| f.stage == run.stage && f.started_at == run.started_at)
            .flat_map(|f| &f.files);
        let name = flow
            .stage(&run.stage)
            .map_or_else(|| run.stage.clone(), |s| s.name.clone());
        let share = match out.iter().position(|s| s.stage == name) {
            Some(at) => {
                out[at].last = run;
                &mut out[at]
            }
            None => {
                out.push(Share {
                    stage: name,
                    last: run,
                    files: Vec::new(),
                });
                out.last_mut().expect("just pushed")
            }
        };
        for file in committed {
            if !share.files.iter().any(|f| f.path == file.path) {
                share.files.push(file);
            }
        }
    }
    out
}

/// One row of What planning did (`s30-02`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PlanningRow")]
pub struct DidRow {
    /// "Product spec".
    pub stage: String,
    /// "Spec writer": the agent of its latest run.
    pub agent: Option<String>,
    /// "the Plan, 4 pieces; docs/milestones/wishlist.md", "decisions/0140-share.md; DESIGN.md", or the first line of its
    /// stage comment when it committed nothing.
    pub what: String,
    /// The first line of the comment its latest run ended with (d0109), when `what` doesn't already say it.
    pub comment: Option<String>,
}

/// Overview's WHAT PLANNING DID (`PlanningDid` on the page).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PlanningDid")]
pub struct Did {
    /// "What planning did" (the page sets it in capitals).
    pub label: String,
    pub rows: Vec<DidRow>,
    /// "The docs are on SHOP-40's branch (Files tab) and ship with its code."; `None` when nothing was committed.
    pub note: Option<String>,
}

/// A comment's first line.
fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_owned)
}

/// "a.md; b.md; c.md +2 more".
fn files_text(files: &[&StageFile]) -> Option<String> {
    if files.is_empty() {
        return None;
    }
    let named: Vec<&str> = files
        .iter()
        .take(NAMED_FILES)
        .map(|f| f.path.as_str())
        .collect();
    let mut text = named.join("; ");
    if files.len() > NAMED_FILES {
        text.push_str(&format!(" +{} more", files.len() - NAMED_FILES));
    }
    Some(text)
}

/// "the Plan, 4 pieces", "the Plan, 1 piece", "the Plan".
fn plan_text(ticket: &Ticket) -> String {
    match plan::pieces(&ticket.plan).len() {
        0 => "the Plan".to_owned(),
        1 => "the Plan, 1 piece".to_owned(),
        n => format!("the Plan, {n} pieces"),
    }
}

/// What planning did at the planning check `check`, from the ticket's stage runs, what each run committed and the
/// name of the stage that wrote the Plan (`plan_writer`, from History); `None` when no Agent stage ran since the
/// previous human check.
pub fn did(
    ticket: &Ticket,
    flow: &Flow,
    check: &Stage,
    runs: &[StageRun],
    files: &StageFiles,
    plan_writer: Option<&str>,
) -> Option<Did> {
    let window = planning_runs(runs, &check.id);
    let shares = shares(flow, &window, files);
    if shares.is_empty() {
        return None;
    }
    let committed = shares.iter().any(|s| !s.files.is_empty());
    let rows = shares
        .iter()
        .map(|s| {
            let comment = s.last.summary.as_deref().and_then(first_line);
            let parts: Vec<String> = (plan_writer == Some(s.stage.as_str())
                && !ticket.plan.is_empty())
            .then(|| plan_text(ticket))
            .into_iter()
            .chain(files_text(&s.files))
            .collect();
            let (what, comment) = match (parts.is_empty(), comment) {
                (false, comment) => (parts.join("; "), comment),
                (true, Some(comment)) => (comment, None),
                (true, None) => (NOTHING_COMMITTED.to_owned(), None),
            };
            DidRow {
                stage: s.stage.clone(),
                agent: s.last.agent.clone(),
                what,
                comment,
            }
        })
        .collect();
    Some(Did {
        label: WHAT_PLANNING_DID.to_owned(),
        rows,
        note: committed.then(|| {
            format!(
                "The docs are on {}\u{2019}s branch (Files tab) and ship with its code.",
                ticket.key
            )
        }),
    })
}

// ---- Files ----

/// The source line's stages at a planning check (`s30-02`): "Product spec · 2", "Tech spec · 2", "Milestone plan · 1"
/// (files each committed); empty when none committed.
pub fn source_stages(
    flow: &Flow,
    check: &Stage,
    runs: &[StageRun],
    files: &StageFiles,
) -> Vec<String> {
    shares(flow, &planning_runs(runs, &check.id), files)
        .iter()
        .filter(|s| !s.files.is_empty())
        .map(|s| format!("{} \u{b7} {}", s.stage, s.files.len()))
        .collect()
}

/// "otaq/SHOP-40 · what planning committed".
pub fn source_subtitle(branch: &str) -> String {
    format!("{branch} \u{b7} {WHAT_PLANNING_COMMITTED}")
}

/// A file's line at a planning check: "added by Product spec · on SHOP-40's branch, not on main yet" (the first stage
/// that committed it); `None` for a file planning didn't commit.
pub fn file_tag(
    ticket: &Ticket,
    flow: &Flow,
    check: &Stage,
    runs: &[StageRun],
    files: &StageFiles,
    path: &str,
    base_branch: &str,
) -> Option<String> {
    let shares = shares(flow, &planning_runs(runs, &check.id), files);
    let (stage, file) = shares
        .iter()
        .find_map(|s| Some((&s.stage, s.files.iter().find(|f| f.path == path)?)))?;
    let verb = match file.status {
        FileStatus::Added => "added",
        FileStatus::Deleted => "deleted",
        FileStatus::Modified | FileStatus::Renamed => "changed",
    };
    Some(format!(
        "{verb} by {stage} \u{b7} on {}\u{2019}s branch, not on {base_branch} yet",
        ticket.key
    ))
}

// ---- The review bar ----

/// The sheet tab the review bar sits under, for where it points (`s30-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarTab {
    Overview,
    Files,
    Other,
}

/// The bar's title at a planning check: "Your check · Spec review".
pub fn bar_title(check: &Stage) -> String {
    format!(
        "{} \u{b7} {}",
        crate::view_models::card::YOUR_CHECK,
        check.name
    )
}

/// The bar's line at a planning check, pointing at where each part is read: "· the decisions on Files and the plan
/// here" on Overview, "· the decisions here and the plan on Overview" on Files; without pieces only the decisions.
pub fn bar_line(ticket: &Ticket, tab: BarTab) -> String {
    let decisions = match tab {
        BarTab::Files => "the decisions here",
        _ => "the decisions on Files",
    };
    let line = match (has_pieces(ticket), tab) {
        (false, _) => decisions.to_owned(),
        (true, BarTab::Overview) => format!("{decisions} and the plan here"),
        (true, _) => format!("{decisions} and the plan on Overview"),
    };
    format!("\u{b7} {line}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use crate::view_models::plan::fixtures::WISHLIST;
    use otaq_core::board::stage_files::RunFiles;
    use otaq_core::flow::presets::{SPEC_FIRST_FILE, preset};

    fn spec_first() -> Flow {
        preset(SPEC_FIRST_FILE).expect("Spec first").read().flow
    }

    fn at(status: Status, place: Place) -> Ticket {
        let mut t = ticket("SHOP-40", "Wishlist", status);
        t.place = place;
        t
    }

    fn run(stage: &str, kind: &str, agent: Option<&str>, at: i64, result: RunResult) -> StageRun {
        StageRun {
            stage: stage.into(),
            kind: kind.into(),
            session: None,
            agent: agent.map(str::to_owned),
            model: None,
            round: None,
            result: Some(result),
            summary: None,
            started_at: at,
            ended_at: None,
        }
    }

    fn files(stage: &str, at: i64, list: &[(&str, FileStatus)]) -> RunFiles {
        RunFiles {
            stage: stage.into(),
            started_at: at,
            files: list
                .iter()
                .map(|(path, status)| StageFile {
                    path: (*path).into(),
                    status: *status,
                })
                .collect(),
        }
    }

    /// SHOP-40's planning (`s30-02`): Product spec, Tech spec, Milestone plan ran; Spec review waits.
    fn shop40_runs() -> (Vec<StageRun>, StageFiles) {
        let mut ps = run(
            "product-spec",
            "agent",
            Some("Spec writer"),
            1,
            RunResult::Done,
        );
        ps.summary = Some("Recorded the share link.\nMore lines.".into());
        let runs = vec![
            ps,
            run("tech-spec", "agent", Some("Architect"), 2, RunResult::Done),
            run(
                "milestone-plan",
                "agent",
                Some("Milestone planner"),
                3,
                RunResult::Done,
            ),
            run("spec-review", "you", None, 4, RunResult::Done),
        ];
        let committed = StageFiles {
            runs: vec![
                files(
                    "product-spec",
                    1,
                    &[
                        ("decisions/0140-wishlist-share-link.md", FileStatus::Added),
                        ("DESIGN.md", FileStatus::Modified),
                    ],
                ),
                files(
                    "tech-spec",
                    2,
                    &[
                        ("decisions/0141-wishlist-storage.md", FileStatus::Added),
                        ("SDD.md", FileStatus::Modified),
                    ],
                ),
                files(
                    "milestone-plan",
                    3,
                    &[("docs/milestones/wishlist.md", FileStatus::Added)],
                ),
            ],
        };
        (runs, committed)
    }

    /// The waiting column: "Ready" under "Before the first stage" in Spec first; the default flow keeps "Ready for
    /// Dev" and its hint.
    #[test]
    fn the_waiting_column_is_ready_when_dev_isnt_first() {
        let spec = spec_first();
        assert_eq!(spec.waiting_name(), "Ready");
        assert_eq!(
            ready_hint(&spec, "Starts in priority order"),
            "Before the first stage"
        );
        let default = otaq_core::flow::default_flow();
        assert_eq!(default.waiting_name(), "Ready for Dev");
        assert_eq!(
            ready_hint(&default, "Starts in priority order"),
            "Starts in priority order"
        );
    }

    /// Spec review checks planning (Code review comes after it); Code review and the default flow's Review don't.
    #[test]
    fn a_check_before_another_check_reviews_planning() {
        let spec = spec_first();
        let check = |id: &str| checks_planning(&spec, spec.stage(id).unwrap());
        assert!(check("spec-review"));
        assert!(!check("code-review"));
        assert!(!check("product-spec"));
        let default = otaq_core::flow::default_flow();
        assert!(default.stages.iter().all(|s| !checks_planning(&default, s)));

        let mut waits = at(Status::WaitingForYou, Place::InStage("spec-review".into()));
        assert_eq!(
            planning_check(&waits, &spec).map(|s| s.name.as_str()),
            Some("Spec review")
        );
        waits.place = Place::InStage("code-review".into());
        assert_eq!(planning_check(&waits, &spec), None);
    }

    /// The card lines (`s30-01`): the check's words with and without pieces, what a shipped skill is doing.
    #[test]
    fn the_card_says_what_planning_is_doing() {
        let mut t = at(Status::WaitingForYou, Place::InStage("spec-review".into()));
        assert_eq!(check_line(&t), "the decisions");
        t.plan = WISHLIST.to_owned();
        assert_eq!(check_line(&t), "the decisions and the plan");
        assert_eq!(working_on("tech-spec"), Some("writing the architecture"));
        assert_eq!(working_on("milestone-plan"), Some("writing the plan"));
        assert_eq!(working_on("product-spec"), Some("writing the decisions"));
        assert_eq!(working_on("work-ticket"), None);
    }

    /// A ticket waiting in Ready that starts at Dev skips planning (`s30-01`'s SHOP-35); one that starts at the first
    /// stage says nothing; a flow without a check in the skipped stages names them.
    #[test]
    fn a_ticket_that_skips_planning_says_where_it_starts() {
        let spec = spec_first();
        let ready = at(Status::Queued, Place::Waiting(None));
        assert_eq!(
            skips_line(&spec, &ready, Some("Dev")).as_deref(),
            Some("Dev next \u{b7} skips planning")
        );
        assert_eq!(skips_line(&spec, &ready, Some("Product spec")), None);
        assert_eq!(skips_line(&spec, &ready, None), None);
        let waiting_before_dev = at(Status::Queued, Place::Waiting(Some("dev".into())));
        assert_eq!(skips_line(&spec, &waiting_before_dev, Some("Dev")), None);

        // Product spec and Tech spec only, no check between: the stages by name.
        let mut short = spec.clone();
        short
            .stages
            .retain(|s| !["milestone-plan", "spec-review"].contains(&s.id.as_str()));
        assert_eq!(
            skips_line(&short, &ready, Some("Dev")).as_deref(),
            Some("Dev next \u{b7} skips Product spec and Tech spec")
        );
    }

    /// Only the Agent stages since the previous human check: an earlier round of the same check and skipped stages
    /// don't end or join it; a passed check before does end it.
    #[test]
    fn planning_runs_are_the_stages_since_the_previous_check() {
        let runs = [
            run("dev", "agent", None, 1, RunResult::Done),
            run("code-review", "you", None, 2, RunResult::Passed),
            run("product-spec", "agent", None, 3, RunResult::Done),
            run("tech-spec", "agent", None, 4, RunResult::Skipped),
            run("spec-review", "you", None, 5, RunResult::Failed),
            run("product-spec", "agent", None, 6, RunResult::Done),
            run("spec-review", "you", None, 7, RunResult::Done),
        ];
        let got: Vec<(&str, i64)> = planning_runs(&runs, "spec-review")
            .iter()
            .map(|r| (r.stage.as_str(), r.started_at))
            .collect();
        assert_eq!(got, [("product-spec", 3), ("product-spec", 6)]);
    }

    /// What planning did (`s30-02`): one row per stage that ran, its agent, what it committed (Milestone plan's Plan
    /// first), the first line of its comment, and the note about the branch.
    #[test]
    fn what_planning_did_comes_from_comments_and_commits() {
        let spec = spec_first();
        let mut t = at(Status::WaitingForYou, Place::InStage("spec-review".into()));
        t.plan = WISHLIST.to_owned();
        let (mut runs, committed) = shop40_runs();
        let check = spec.stage("spec-review").unwrap();
        let did = did(&t, &spec, check, &runs, &committed, Some("Milestone plan")).unwrap();
        assert_eq!(did.label, "What planning did");
        let rows: Vec<(&str, Option<&str>, &str, Option<&str>)> = did
            .rows
            .iter()
            .map(|r| {
                (
                    r.stage.as_str(),
                    r.agent.as_deref(),
                    r.what.as_str(),
                    r.comment.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "Product spec",
                    Some("Spec writer"),
                    "decisions/0140-wishlist-share-link.md; DESIGN.md",
                    Some("Recorded the share link.")
                ),
                (
                    "Tech spec",
                    Some("Architect"),
                    "decisions/0141-wishlist-storage.md; SDD.md",
                    None
                ),
                (
                    "Milestone plan",
                    Some("Milestone planner"),
                    "the Plan, 4 pieces; docs/milestones/wishlist.md",
                    None
                ),
            ]
        );
        assert_eq!(
            did.note.as_deref(),
            Some("The docs are on SHOP-40\u{2019}s branch (Files tab) and ship with its code.")
        );

        // A stage that committed nothing says its comment; with neither, "nothing committed"; no note then.
        runs[1].summary = Some("Kept the schema as it is.".into());
        let empty = StageFiles::default();
        let did = super::did(&t, &spec, check, &runs, &empty, None).unwrap();
        let what: Vec<&str> = did.rows.iter().map(|r| r.what.as_str()).collect();
        assert_eq!(
            what,
            [
                "Recorded the share link.",
                "Kept the schema as it is.",
                "nothing committed"
            ]
        );
        assert_eq!(did.note, None);
        // Nothing ran since the last check: no section.
        assert_eq!(
            super::did(&t, &spec, check, &runs[3..], &committed, None),
            None
        );
    }

    /// Files at Spec review (`s30-02`): the source line with each stage's count and a file's tag.
    #[test]
    fn files_show_what_each_stage_committed() {
        let spec = spec_first();
        let t = at(Status::WaitingForYou, Place::InStage("spec-review".into()));
        let (runs, committed) = shop40_runs();
        let check = spec.stage("spec-review").unwrap();
        assert_eq!(
            source_stages(&spec, check, &runs, &committed),
            [
                "Product spec \u{b7} 2",
                "Tech spec \u{b7} 2",
                "Milestone plan \u{b7} 1"
            ]
        );
        let branch = otaq_core::naming::branch("SHOP-40");
        assert_eq!(
            source_subtitle(&branch),
            format!("{branch} \u{b7} what planning committed")
        );
        let tag = |path: &str| file_tag(&t, &spec, check, &runs, &committed, path, "main");
        assert_eq!(
            tag("decisions/0140-wishlist-share-link.md").as_deref(),
            Some("added by Product spec \u{b7} on SHOP-40\u{2019}s branch, not on main yet")
        );
        assert_eq!(
            tag("SDD.md").as_deref(),
            Some("changed by Tech spec \u{b7} on SHOP-40\u{2019}s branch, not on main yet")
        );
        assert_eq!(tag("src/app.ts"), None);
    }

    /// The bar's words at Spec review, under Overview, Files and the other tabs, with and without pieces.
    #[test]
    fn the_bar_points_at_where_each_part_is_read() {
        let spec = spec_first();
        let mut t = at(Status::WaitingForYou, Place::InStage("spec-review".into()));
        assert_eq!(
            bar_title(spec.stage("spec-review").unwrap()),
            "Your check \u{b7} Spec review"
        );
        assert_eq!(
            bar_line(&t, BarTab::Overview),
            "\u{b7} the decisions on Files"
        );
        assert_eq!(bar_line(&t, BarTab::Files), "\u{b7} the decisions here");
        t.plan = WISHLIST.to_owned();
        assert_eq!(
            bar_line(&t, BarTab::Overview),
            "\u{b7} the decisions on Files and the plan here"
        );
        assert_eq!(
            bar_line(&t, BarTab::Files),
            "\u{b7} the decisions here and the plan on Overview"
        );
        assert_eq!(
            bar_line(&t, BarTab::Other),
            "\u{b7} the decisions on Files and the plan on Overview"
        );
    }
}

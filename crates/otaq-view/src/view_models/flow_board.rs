//! Edit flow on the board: the board itself becomes the flow's editor, in place. Its columns are the draft's stages
//! between the fixed ends, Backlog ("Every flow starts here") and Done ("Every flow ends here", which carries how work
//! leaves the flow); the board's cards stay where they are, dimmed. Each stage's column header becomes the stage: its
//! name, what happens, its agent and skill, and for a check its rounds and where its fails go. A column header is
//! dragged to reorder, × removes it, **+** between two columns opens the palette of ready stages (data shipped with
//! the app, [`otaq_core::flow::templates`]), and a click opens the stage's details: Name, What happens, What it's
//! for (the stage's description, which every agent reads; there is no separate instructions field), Agent, Skill
//! (Pick a skill… or Ask the agent to write it), Fails go to, Rounds (−/+, 1–5) and whether tickets may skip it.
//! Clicking Done opens How work leaves the flow: the flow's delivery, edited with the same rules as the project's
//! Repository card.
//!
//! The bar on top says what is being edited and how many changes the draft holds, with Watch a ticket (a pretend
//! ticket walks the draft step by step, passing or failing a check, running nothing), Start from… (Suggest a flow
//! for the project first, then the shipped flows, another project's and Import), ⋯ (Export, Import, Copy from another
//! project, Go back to the previous flow), Cancel and Save flow. A suggestion from Noyan's agent shows its changes
//! marked on their columns, with Why these changes, Ask for changes…, Discard and Save flow.
//!
//! Every edit goes to the project's draft as the overlay editor's did ([`super::flow_editor::edited`],
//! `flow.draft_update`); nothing changes on the board until the user saves. What a saved change does to moving
//! tickets is the core's: adding or editing a stage applies at once; removing, reordering or changing a stage's kind
//! waits until no ticket is in the stages it touches; a whole other flow waits for the whole board.

use otaq_core::flow::draft::{DraftApplies, Mark};
use otaq_core::flow::model::{
    CheckBy, Delivers, DeliveryMode, Flow, FlowFile, Stage, StageKind, StageOption,
};
use otaq_core::flow::presets::presets;
use otaq_core::flow::proposal::{Change, ProposedBy};
use otaq_core::flow::templates::templates;
use otaq_core::project_agents::AgentState;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::agent_page::Choice;
use crate::view_models::flow_columns::{BACKLOG, DONE};
use crate::view_models::flow_editor::{
    self as editor, AgentField, CHECK_ROUNDS, Choices, EditorKind, Facts, FailToField, FixRow,
    MarkView, Pick, SkillField,
};
use crate::view_models::hint::HintId;
use crate::view_models::project_settings::{DELIVERS_FIELD, EITHER_NOTE, MODE_FIELD};
use crate::view_models::tool_avatar::ToolAvatarView;

/// The kinds of stage, as the editor names them.
pub fn kind_label(kind: EditorKind) -> &'static str {
    match kind {
        EditorKind::Works => "An agent does the work",
        EditorKind::AgentChecks => "An agent checks it",
        EditorKind::YouCheck => "You check it",
    }
}

/// "Every flow starts here" / "Every flow ends here": the fixed ends, which can't be moved or removed.
pub const STARTS_HERE: &str = "Every flow starts here";
pub const ENDS_HERE: &str = "Every flow ends here";
/// Under a stage the flow has now, in its details: removing it waits for the tickets in it.
pub const REMOVING_WAITS: &str = "Removing waits until no ticket is in it";
/// The Rounds field's words after its number.
pub const ROUNDS_NOTE: &str = "fails in a row, then it waits for you";

/// What a column is: one of the fixed ends, or a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FlowBoardColumnKind")]
pub enum ColumnKind {
    Backlog,
    Stage,
    Done,
}

/// **+** after a column: what it adds after, and the palette's title for that place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowBoardGap")]
pub struct Gap {
    /// The stage the new one goes after; `None`: first, right after Backlog.
    pub after: Option<String>,
    /// "Add a stage between Dev and Validate".
    pub title: String,
}

/// One column of the editor. `FlowBoardColumn` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardColumn")]
pub struct Column {
    /// The stage's id; `backlog` and `done` for the fixed ends (the board's own column ids, so its cards show under
    /// the right header).
    pub id: String,
    pub kind: ColumnKind,
    pub name: String,
    pub icon: IconName,
    /// "An agent does the work", "Every flow starts here".
    pub what: String,
    pub avatar: Option<ToolAvatarView>,
    /// Its agent ("Strong"), "You" for a check by you.
    pub who: Option<String>,
    pub skill: Option<String>,
    /// "3 rounds · fails go to Dev", "Fails go to Dev".
    pub check_line: Option<String>,
    /// Done's line: how work leaves the flow ("Merge or PR · on Approve").
    pub delivery: Option<String>,
    /// What it's for, shown on hovering its header.
    pub description: Option<String>,
    /// "New" for a stage this draft adds, or the mark of a suggestion's change ("Added", "Changed").
    pub mark: Option<MarkView>,
    /// "You don't have the skill `security-reviw`": amber, it still saves.
    pub warning: Option<String>,
    /// "needs a skill", or a problem that keeps Save off.
    pub problem: Option<String>,
    pub selected: bool,
    /// Stages move and go; Backlog and Done don't.
    pub movable: bool,
    /// Its place among the stages (the drag's), `None` for the fixed ends.
    pub index: Option<usize>,
    /// **+** after it; `None` after Done.
    pub add: Option<Gap>,
}

/// What happens: one of the three kinds.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardKindOption")]
pub struct KindOption {
    pub kind: EditorKind,
    pub label: &'static str,
}

/// An agent check's Rounds: −/+ between 1 and 5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowBoardRounds")]
pub struct Rounds {
    pub label: &'static str,
    pub value: u32,
    pub min: u32,
    pub max: u32,
    pub note: &'static str,
}

/// Tickets may skip this stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowBoardSkip")]
pub struct Skip {
    pub label: &'static str,
    pub note: &'static str,
    pub on: bool,
}

/// A stage's details. `FlowBoardDetails` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardDetails")]
pub struct Details {
    pub stage: String,
    /// "Validate", or "New stage" for one this draft adds.
    pub title: String,
    pub name_label: &'static str,
    pub name: String,
    pub kind_label: &'static str,
    pub kinds: Vec<KindOption>,
    pub kind: Option<EditorKind>,
    pub description_label: &'static str,
    pub description: String,
    pub description_hint: &'static str,
    pub agent: Option<AgentField>,
    pub skill: Option<SkillField>,
    /// "Otaq's own" beside one of Otaq's skills.
    pub skill_tag: Option<String>,
    pub skill_hint: &'static str,
    pub fail_to: Option<FailToField>,
    pub rounds: Option<Rounds>,
    pub skip: Skip,
    /// A problem no pick fixes.
    pub problem: Option<String>,
    pub fixes: Vec<FixRow>,
    pub mark: Option<MarkView>,
    pub remove: &'static str,
    /// [`REMOVING_WAITS`] for a stage the flow has now.
    pub remove_note: Option<&'static str>,
}

/// How work leaves the flow: Done's panel. `FlowBoardDelivery` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardDelivery")]
pub struct DeliveryPanel {
    pub title: &'static str,
    pub delivers_label: &'static str,
    /// The field `delivery.delivers` and its value ids `local`, `pr`, `either`.
    pub delivers_field: &'static str,
    pub delivers: Vec<Choice>,
    pub delivers_note: Option<String>,
    pub mode_label: &'static str,
    pub mode_field: &'static str,
    /// On Approve · Automatically; empty for Either, which is chosen at your click.
    pub modes: Vec<Choice>,
    pub mode_note: Option<&'static str>,
    pub note: &'static str,
}

/// One ready stage of the palette. `FlowBoardTemplate` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardTemplate")]
pub struct Template {
    /// What [`super::flow_editor::StageEdit::AddTemplate`] sends.
    pub id: String,
    pub title: String,
    pub line: String,
    pub icon: IconName,
    /// "Uses a skill you add: `test-in-browser`": a quiet line, not a warning, for a ready stage whose skill neither
    /// Otaq ships nor the user has. Otaq never copies skills, so the user brings their own; it can still be added,
    /// and the added stage carries the usual amber warning until the skill is found.
    pub skill_note: Option<String>,
}

/// The palette **+** opens. `FlowBoardPalette` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardPalette")]
pub struct Palette {
    pub templates: Vec<Template>,
    pub foot: &'static str,
}

/// An entry of Start from… or ⋯.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardEntry")]
pub struct Entry {
    pub key: String,
    pub name: String,
    /// "Dev → Validate → Review · merge or PR on Approve".
    pub line: String,
    pub pick: Pick,
}

/// Suggest a flow for <project>….
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowBoardSuggest")]
pub struct Suggest {
    pub label: String,
    pub text: String,
    /// What the agent is asked (the `flow` skill's first message).
    pub message: String,
    /// Why it can't be asked now ("Strong is drafting one"); `None` when it can.
    pub off: Option<String>,
}

/// Start from…. `FlowBoardStartFrom` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardStartFrom")]
pub struct StartFrom {
    pub suggest: Suggest,
    pub shipped_label: String,
    pub presets: Vec<Entry>,
    pub more_label: &'static str,
    /// "Same as billing-api": each other project's flow.
    pub others: Vec<Entry>,
    pub import: &'static str,
    pub import_line: &'static str,
    pub foot: &'static str,
}

/// ⋯ in the bar. `FlowBoardMore` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardMore")]
pub struct More {
    pub export: &'static str,
    /// "shop-web.flow.yaml · stages, agents, skills, delivery".
    pub export_line: String,
    /// The save dialog's file name.
    pub export_file: String,
    pub export_title: String,
    pub import: &'static str,
    pub import_line: &'static str,
    pub import_title: &'static str,
    pub copy: &'static str,
    /// Go back to the previous flow, when there is one.
    pub previous: Option<&'static str>,
    pub previous_line: &'static str,
}

/// One line of Why these changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowBoardWhy")]
pub struct Why {
    pub number: usize,
    /// "Added Test in the browser".
    pub title: String,
    pub why: Option<String>,
}

/// A suggestion from Noyan's agent, shown marked. `FlowBoardSuggestion` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardSuggestion")]
pub struct Suggestion {
    /// "Strong's suggestion for shop-web".
    pub title: String,
    /// "3 changes, marked · nothing changes until you save".
    pub line: String,
    pub whys_title: &'static str,
    pub whys: Vec<Why>,
    pub ask: &'static str,
    /// "Tell Strong what to change…".
    pub ask_placeholder: String,
    pub discard: &'static str,
}

/// While Noyan's agent drafts a flow: "Strong is drafting a flow for shop-web…", with Show (its conversation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowBoardDrafting")]
pub struct Drafting {
    pub text: String,
    pub agent: String,
    pub show: &'static str,
}

/// One step of Watch a ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowBoardWatchStep")]
pub struct WatchStep {
    /// The column the pretend ticket is in.
    pub column: String,
    /// The column's name, for the path.
    pub name: String,
    /// What its card says there: "Working · Strong", "Checking · round 2 of 3".
    pub state: String,
    pub text: String,
}

/// Watch a ticket: a pretend ticket walks the draft. `FlowBoardWatch` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardWatch")]
pub struct Watch {
    pub label: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub passes_label: &'static str,
    pub fails_label: &'static str,
    pub passes: Vec<WatchStep>,
    /// Empty when no stage checks the work.
    pub fails: Vec<WatchStep>,
    pub ticket_label: &'static str,
    pub ticket_title: &'static str,
    /// "Step {n} of {m}".
    pub step_of: &'static str,
    pub back: &'static str,
    pub next: &'static str,
    pub close: &'static str,
}

/// The bar on top. `FlowBoardBar` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardBar")]
pub struct Bar {
    /// "Editing shop-web's flow".
    pub title: String,
    /// "Drag to reorder · click a stage for details · + to add", until something changes.
    pub line: &'static str,
    /// "1 change", once something changed.
    pub changes: Option<String>,
    /// "added Security review".
    pub change_words: Option<String>,
    pub undo: &'static str,
    pub start_from: &'static str,
    pub more: &'static str,
    pub cancel: &'static str,
    pub save: &'static str,
    pub save_enabled: bool,
    /// Why Save is off.
    pub reason: Option<String>,
    /// When the draft would reach tickets once saved.
    pub applies: Option<String>,
}

/// What Save flow's toast says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowBoardSaved")]
pub struct Saved {
    pub title: &'static str,
    pub text: String,
}

/// Everything the editor draws. `FlowBoardView` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardView")]
pub struct View {
    pub project: String,
    /// The draft's proposal (Save takes it); `0` while nothing changed.
    pub proposal: u32,
    /// The draft's document, for Undo.
    pub yaml: String,
    pub bar: Bar,
    pub columns: Vec<Column>,
    pub details: Option<Details>,
    pub delivery: Option<DeliveryPanel>,
    pub palette: Palette,
    pub start_from: StartFrom,
    pub more: More,
    pub suggestion: Option<Suggestion>,
    pub drafting: Option<Drafting>,
    pub watch: Watch,
    pub saved: Saved,
    pub hint: HintId,
    /// The picks as Save sends them.
    pub choices: Choices,
}

/// What the board's editor reads besides the overlay editor's facts.
pub struct BoardFacts<'a> {
    pub base_branch: &'a str,
    /// The other projects: key, name, and their flow when it reads.
    pub others: &'a [(String, String, Option<FlowFile>)],
    /// The project has a flow before the one in use.
    pub previous: bool,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "merge or PR on Approve", "merges by itself": how a flow's work leaves it, in a few words.
fn delivery_words(flow: &Flow) -> String {
    let d = &flow.delivery;
    match (d.mode, d.delivers) {
        (DeliveryMode::Automatic, Delivers::Local) => "merges by itself".to_owned(),
        (DeliveryMode::Automatic, Delivers::Pr) => "opens a PR by itself".to_owned(),
        (_, Delivers::Local) => "merge on Approve".to_owned(),
        (_, Delivers::Pr) => "PR on Approve".to_owned(),
        (_, Delivers::Either) => "merge or PR on Approve".to_owned(),
    }
}

/// Done's line: "Merge or PR · on Approve", "Pull request · automatically".
fn done_line(flow: &Flow) -> String {
    let d = &flow.delivery;
    let what = match d.delivers {
        Delivers::Local => "Merge",
        Delivers::Pr => "Pull request",
        Delivers::Either => "Merge or PR",
    };
    let when = match d.mode {
        DeliveryMode::Automatic if d.delivers != Delivers::Either => "automatically",
        _ => "on Approve",
    };
    format!("{what} \u{b7} {when}")
}

/// "Dev → Validate → Review · merge or PR on Approve": a flow's stages and delivery.
pub fn chain(flow: &Flow) -> String {
    let stages: Vec<&str> = flow.stages.iter().map(|s| s.name.as_str()).collect();
    let stages = if stages.is_empty() {
        "No stages".to_owned()
    } else {
        stages.join(" \u{2192} ")
    };
    format!("{stages} \u{b7} {}", delivery_words(flow))
}

fn stage_kind(stage: &Stage) -> EditorKind {
    match stage.kind {
        StageKind::Agent => EditorKind::Works,
        StageKind::Check { by: CheckBy::Agent } => EditorKind::AgentChecks,
        StageKind::Check { by: CheckBy::You } => EditorKind::YouCheck,
    }
}

/// A stage this draft adds by the user's hand (not in the flow in use, not a suggestion's, not another flow copied
/// in).
fn added_by_hand(f: &Facts, stage: &Stage) -> bool {
    f.in_use.flow.stage(&stage.id).is_none()
        && !f.swap
        && editor::mark_of(f.marks, &stage.id).is_none()
}

fn new_mark() -> MarkView {
    MarkView {
        label: "New",
        why: "Added in this draft".to_owned(),
        undo: "Undo",
    }
}

/// "3 rounds · fails go to Dev", "Fails go to Dev".
fn check_line(flow: &Flow, stage: &Stage) -> Option<String> {
    let to = editor::fails_to(flow, stage)?;
    Some(match stage.kind {
        StageKind::Check { by: CheckBy::Agent } => format!(
            "{} \u{b7} fails go to {}",
            plural(
                stage.rounds.unwrap_or(CHECK_ROUNDS) as usize,
                "round",
                "rounds"
            ),
            to.name
        ),
        _ => format!("Fails go to {}", to.name),
    })
}

fn gap_title(before: &str, after: &str) -> String {
    format!("Add a stage between {before} and {after}")
}

fn columns(f: &Facts) -> Vec<Column> {
    let flow = &f.file.flow;
    let stages = &flow.stages;
    let first_after = stages.first().map_or("Done", |s| s.name.as_str());
    let mut out = vec![Column {
        id: BACKLOG.to_owned(),
        kind: ColumnKind::Backlog,
        name: editor::BACKLOG.to_owned(),
        icon: IconName::CircleDashed,
        what: STARTS_HERE.to_owned(),
        avatar: None,
        who: None,
        skill: None,
        check_line: None,
        delivery: None,
        description: Some(editor::READY_CAPTION.to_owned()),
        mark: None,
        warning: None,
        problem: None,
        selected: false,
        movable: false,
        index: None,
        add: Some(Gap {
            after: None,
            title: gap_title(editor::BACKLOG, first_after),
        }),
    }];
    for (i, stage) in stages.iter().enumerate() {
        let next = stages.get(i + 1).map_or("Done", |s| s.name.as_str());
        let (avatar, who) = match stage.kind {
            StageKind::Check { by: CheckBy::You } => (None, Some("You".to_owned())),
            _ => match editor::the_agent(stage) {
                Some(name) => (editor::agent_on(name, f).0, Some(name.clone())),
                None => (None, None),
            },
        };
        let mark = editor::mark_of(f.marks, &stage.id)
            .map(editor::mark_view)
            .or_else(|| added_by_hand(f, stage).then(new_mark));
        out.push(Column {
            id: stage.id.clone(),
            kind: ColumnKind::Stage,
            name: stage.name.clone(),
            icon: editor::kind_icon(stage),
            what: kind_label(stage_kind(stage)).to_owned(),
            avatar,
            who,
            skill: stage.skill.clone(),
            check_line: check_line(flow, stage),
            delivery: None,
            description: stage.description.clone(),
            mark,
            warning: editor::unknown_skill(f, stage).or_else(|| editor::missing_skill(f, stage)),
            problem: editor::step_problem(f, stage),
            selected: f.selected == Some(stage.id.as_str()),
            movable: true,
            index: Some(i),
            add: Some(Gap {
                after: Some(stage.id.clone()),
                title: gap_title(&stage.name, next),
            }),
        });
    }
    out.push(Column {
        id: DONE.to_owned(),
        kind: ColumnKind::Done,
        name: "Done".to_owned(),
        icon: IconName::CircleCheck,
        what: ENDS_HERE.to_owned(),
        avatar: None,
        who: None,
        skill: None,
        check_line: None,
        delivery: Some(done_line(flow)),
        description: Some(DELIVERY_NOTE.to_owned()),
        mark: None,
        warning: None,
        problem: None,
        selected: done_selected(f),
        movable: false,
        index: None,
        add: None,
    });
    out
}

/// How work leaves the flow's last line.
pub const DELIVERY_NOTE: &str = "Part of the flow, so it travels in the flow file. The project\u{2019}s Repository \
     card shows the same setting.";

fn details(f: &Facts, stage: &Stage) -> Details {
    let d = editor::details(f, stage);
    let kind = stage_kind(stage);
    let in_use = f.in_use.flow.stage(&stage.id).is_some();
    let skill_tag = stage
        .skill
        .as_deref()
        .filter(|s| otaq_core::skills::found::is_shipped(s))
        .map(|_| format!("{}\u{2019}s own", otaq_core::naming::PRODUCT_NAME));
    Details {
        stage: stage.id.clone(),
        title: if added_by_hand(f, stage) {
            editor::NEW_STAGE.to_owned()
        } else {
            stage.name.clone()
        },
        name_label: "Name",
        name: stage.name.clone(),
        kind_label: "What happens",
        kinds: [
            EditorKind::Works,
            EditorKind::AgentChecks,
            EditorKind::YouCheck,
        ]
        .into_iter()
        .map(|kind| KindOption {
            kind,
            label: kind_label(kind),
        })
        .collect(),
        kind: Some(kind),
        description_label: "What it\u{2019}s for",
        description: stage.description.clone().unwrap_or_default(),
        description_hint: "One or two sentences: what this stage settles, and when it matters. Agents read it to \
             understand the flow; people see it on the column header and when skipping.",
        agent: d.agent.map(|a| AgentField {
            hint: "Any of your agents. A ticket may pick another.",
            ..a
        }),
        skill: d.skill,
        skill_tag,
        skill_hint: "How the agent does it.",
        fail_to: d.fail_to,
        rounds: stage.is_agent_check().then(|| Rounds {
            label: "Rounds",
            value: stage.rounds.unwrap_or(CHECK_ROUNDS),
            min: *otaq_core::flow::check::VALIDATION_ROUNDS.start(),
            max: *otaq_core::flow::check::VALIDATION_ROUNDS.end(),
            note: ROUNDS_NOTE,
        }),
        skip: Skip {
            label: "Tickets may skip this stage",
            note: "\u{b7} chosen per ticket",
            on: stage.options.contains(&StageOption::Skip),
        },
        problem: d.problem,
        fixes: d.fixes,
        mark: d.mark,
        remove: "Remove stage",
        remove_note: in_use.then_some(REMOVING_WAITS),
    }
}

fn delivery_panel(f: &Facts, base: &str) -> DeliveryPanel {
    let d = &f.file.flow.delivery;
    let choice = |id: &str, label: String, selected: bool| Choice {
        id: id.to_owned(),
        label,
        selected,
    };
    let either = d.delivers == Delivers::Either;
    DeliveryPanel {
        title: "How work leaves the flow",
        delivers_label: "Deliver as",
        delivers_field: DELIVERS_FIELD,
        delivers: vec![
            choice("local", format!("Merge into {base}"), d.delivers == Delivers::Local),
            choice("pr", "Pull request".to_owned(), d.delivers == Delivers::Pr),
            choice("either", "Either".to_owned(), either),
        ],
        delivers_note: either.then(|| {
            format!("{EITHER_NOTE} Approve offers both, Approve & merge and Approve & open PR.")
        }),
        mode_label: "When",
        mode_field: MODE_FIELD,
        modes: if either {
            Vec::new()
        } else {
            vec![
                choice("click", "On Approve".to_owned(), d.mode == DeliveryMode::Click),
                choice(
                    "automatic",
                    "Automatically".to_owned(),
                    d.mode == DeliveryMode::Automatic,
                ),
            ]
        },
        mode_note: (!either && d.mode == DeliveryMode::Automatic).then_some(
            "Automatically: delivered as soon as the last stage passes, without Approve. Solo autopilot merges this \
             way; Team PR opens a PR this way.",
        ),
        note: DELIVERY_NOTE,
    }
}

fn template_icon(stage: &Stage) -> IconName {
    match stage.kind {
        StageKind::Agent if stage.skill.is_none() => IconName::SquareDashed,
        StageKind::Agent => IconName::Bot,
        StageKind::Check { by: CheckBy::Agent } => IconName::ShieldCheck,
        StageKind::Check { by: CheckBy::You } => IconName::UserCheck,
    }
}

fn palette(f: &Facts) -> Palette {
    let has = |skill: &str| {
        otaq_core::skills::found::is_shipped(skill) || f.skills.iter().any(|s| s.name == skill)
    };
    Palette {
        templates: templates()
            .iter()
            .map(|t| Template {
                id: t.id().to_owned(),
                title: t.title.clone(),
                line: t.line.clone(),
                icon: template_icon(t.stage()),
                skill_note: t
                    .stage()
                    .skill
                    .as_deref()
                    .filter(|s| !has(s))
                    .map(skill_you_add),
            })
            .collect(),
        foot: "Each comes with a description, an agent and a skill; change any of them after.",
    }
}

/// A ready stage's skill the user brings: "Uses a skill you add: `security-check`".
fn skill_you_add(skill: &str) -> String {
    format!("Uses a skill you add: `{skill}`")
}

/// The agent Noyan uses, which drafts a suggestion: its name, else "Noyan".
fn suggester(f: &Facts) -> String {
    f.new_agent
        .map_or_else(|| "Noyan".to_owned(), |a| a.name.clone())
}

/// What Suggest a flow asks Noyan's agent, on the `flow` skill.
pub fn suggest_message(project_name: &str) -> String {
    format!(
        "Suggest a flow for {project_name}. Read this repository and how the project works, then propose a whole flow \
         that fits it, with a short why on each change."
    )
}

/// Noyan's agent is drafting a flow: the editor's agent runs and nothing it proposed shows yet.
fn drafting(f: &Facts) -> Option<Drafting> {
    let agent = f.agent?;
    let running = matches!(agent.state, AgentState::Running) && f.marks.is_empty();
    running.then(|| Drafting {
        text: format!(
            "{} is drafting a flow for {}\u{2026}",
            agent.name, f.project_name
        ),
        agent: agent.id.clone(),
        show: "Show",
    })
}

fn start_from(f: &Facts, b: &BoardFacts) -> StartFrom {
    let name = suggester(f);
    let busy = drafting(f).map(|d| d.text);
    StartFrom {
        suggest: Suggest {
            label: format!("Suggest a flow for {}\u{2026}", f.project_name),
            text: format!(
                "{name} reads the project and drafts a flow for it. Its changes come marked; you keep them, ask for \
                 others, or discard."
            ),
            message: suggest_message(f.project_name),
            off: busy,
        },
        shipped_label: format!(
            "SHIPPED WITH {}",
            otaq_core::naming::PRODUCT_NAME.to_uppercase()
        ),
        presets: presets()
            .iter()
            .map(|p| Entry {
                key: p.file.to_owned(),
                name: p.name.clone(),
                line: chain(&p.read().flow),
                pick: Pick::Preset {
                    file: p.file.to_owned(),
                },
            })
            .collect(),
        more_label: "MORE",
        others: b
            .others
            .iter()
            .filter_map(|(key, name, file)| {
                Some(Entry {
                    key: format!("project:{key}"),
                    name: format!("Same as {name}"),
                    line: chain(&file.as_ref()?.flow),
                    pick: Pick::Project {
                        project: key.clone(),
                    },
                })
            })
            .collect(),
        import: "Import a flow file\u{2026}",
        import_line: "A .flow.yaml someone shared",
        foot: "Replaces the stages between Backlog and Done, and how work is delivered. Nothing changes until you save.",
    }
}

/// The file name Export offers: "shop-web.flow.yaml".
pub fn export_file(project_name: &str) -> String {
    let mut name: String = project_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if name.trim_matches('-').is_empty() {
        name = "flow".to_owned();
    }
    format!("{name}.flow.yaml")
}

fn more(f: &Facts, b: &BoardFacts) -> More {
    let file = export_file(f.project_name);
    More {
        export: "Export flow file\u{2026}",
        export_line: format!("{file} \u{b7} stages, agents, skills, delivery"),
        export_title: format!("Export {}\u{2019}s flow", f.project_name),
        export_file: file,
        import: "Import a flow file\u{2026}",
        import_line: "Opens as a draft; nothing changes until you save",
        import_title: "Import a flow file",
        copy: "Copy from another project\u{2026}",
        previous: b.previous.then_some("Go back to the previous flow"),
        previous_line: "Opens as a draft",
    }
}

/// "added Security review", "removed Validate": one change, in a few words.
fn change_words(change: &otaq_core::flow::proposal::StageChange) -> String {
    let what = match &change.change {
        Change::Added { .. } => "added",
        Change::Removed => "removed",
        Change::Moved { .. } => "moved",
        Change::Renamed { .. } => "renamed",
        _ => "changed",
    };
    format!("{what} {}", change.stage)
}

/// The draft's changes, each stage once, in the words of its first change.
fn changes(f: &Facts) -> Vec<String> {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for c in &f.proposal.diff.stages {
        if seen.contains(&c.stage_id.as_str()) {
            continue;
        }
        seen.push(&c.stage_id);
        out.push(change_words(c));
    }
    out
}

fn bar(f: &Facts) -> Bar {
    let footer = editor::footer(f);
    let same = f.proposal.diff.same;
    let words = changes(f);
    let count = editor::changed_stages(f.proposal);
    Bar {
        title: format!("Editing {}\u{2019}s flow", f.project_name),
        line: "Drag to reorder \u{b7} click a stage for details \u{b7} + to add",
        changes: (!same).then(|| plural(count, "change", "changes")),
        change_words: (!same && !words.is_empty()).then(|| match words.len() {
            1 | 2 => words.join(", "),
            _ => format!("{}, {}\u{2026}", words[0], words[1]),
        }),
        undo: "Undo",
        start_from: "Start from\u{2026}",
        more: "More",
        cancel: "Cancel",
        save: "Save flow",
        save_enabled: !same && footer.apply_enabled,
        reason: footer.reason,
        applies: editor::change_line(f),
    }
}

fn suggestion(f: &Facts) -> Option<Suggestion> {
    let ProposedBy::ProjectAgent { name, .. } = &f.proposal.proposed_by else {
        return None;
    };
    if f.marks.is_empty() && !f.swap {
        return None;
    }
    let count = f.marks.len().max(editor::changed_stages(f.proposal));
    let whys = f
        .marks
        .iter()
        .enumerate()
        .map(|(i, m)| Why {
            number: i + 1,
            title: format!(
                "{} {}",
                match m.mark {
                    Mark::Added | Mark::New => "Added",
                    Mark::Changed => "Changed",
                    Mark::Moved => "Moved",
                    Mark::Removed => "Removed",
                },
                m.stage
            ),
            why: m.why.clone(),
        })
        .collect();
    Some(Suggestion {
        title: format!("{name}\u{2019}s suggestion for {}", f.project_name),
        line: format!(
            "{}, marked \u{b7} nothing changes until you save",
            plural(count, "change", "changes")
        ),
        whys_title: "Why these changes",
        whys,
        ask: "Ask for changes\u{2026}",
        ask_placeholder: format!("Tell {name} what to change\u{2026}"),
        discard: "Discard",
    })
}

/// What the pretend ticket's card says in a stage it passes, and what happens there.
fn pass_step(flow: &Flow, stage: &Stage, round: u32) -> WatchStep {
    let agent = editor::the_agent(stage).map_or("An agent", String::as_str);
    let (state, text) = match stage.kind {
        StageKind::Agent => {
            let skill = stage
                .skill
                .as_deref()
                .map_or(String::new(), |s| format!(" with {s}"));
            let skip = if stage.options.contains(&StageOption::Skip) {
                " A ticket may skip it."
            } else {
                ""
            };
            (
                format!("Working \u{b7} {agent}"),
                format!("{agent} works on it in {}{skill}.{skip}", stage.name),
            )
        }
        StageKind::Check { by: CheckBy::Agent } => (
            format!(
                "Checking \u{b7} round {round} of {}",
                stage.rounds.unwrap_or(CHECK_ROUNDS)
            ),
            if round > 1 {
                format!("{agent} checks it again in {} and passes it.", stage.name)
            } else {
                format!("{agent} checks it in {} and passes it.", stage.name)
            },
        ),
        StageKind::Check { by: CheckBy::You } => {
            let back =
                editor::fails_to(flow, stage).map_or("the stage before", |s| s.name.as_str());
            (
                "Ready for you".to_owned(),
                format!(
                    "It waits in {} for you: you pass it, or send it back to {back} with a note.",
                    stage.name
                ),
            )
        }
    };
    WatchStep {
        column: stage.id.clone(),
        name: stage.name.clone(),
        state,
        text,
    }
}

fn backlog_step() -> WatchStep {
    WatchStep {
        column: BACKLOG.to_owned(),
        name: editor::BACKLOG.to_owned(),
        state: "Not started".to_owned(),
        text: "It waits in Backlog until you start it; Start puts it into the first stage."
            .to_owned(),
    }
}

fn done_step(flow: &Flow, base: &str) -> WatchStep {
    let d = &flow.delivery;
    let automatic = d.mode == DeliveryMode::Automatic && d.delivers != Delivers::Either;
    let (state, text) = match (automatic, d.delivers) {
        (true, Delivers::Pr) => (
            "PR open".to_owned(),
            "A pull request opens by itself once the last stage passes; your team reviews it there.".to_owned(),
        ),
        (true, _) => (
            format!("Merged into {base}"),
            format!("It is merged into {base} by itself once the last stage passes."),
        ),
        (false, Delivers::Pr) => (
            "PR open".to_owned(),
            "You approve it and a pull request opens; it is done once it is merged.".to_owned(),
        ),
        (false, Delivers::Local) => (
            format!("Merged into {base}"),
            format!("You approve it and it is merged into {base}."),
        ),
        (false, Delivers::Either) => (
            format!("Merged into {base}"),
            format!("You approve it, merging into {base} or opening a pull request."),
        ),
    };
    WatchStep {
        column: DONE.to_owned(),
        name: "Done".to_owned(),
        state,
        text,
    }
}

/// The pretend ticket's walk through the draft: every stage passing; or, with `fail`, the first check failing it
/// once, back to where its fails go, then on again.
pub fn walk(flow: &Flow, base: &str, fail: bool) -> Vec<WatchStep> {
    let mut steps = vec![backlog_step()];
    let check = flow
        .stages
        .iter()
        .position(|s| s.is_agent_check())
        .or_else(|| {
            flow.stages
                .iter()
                .position(|s| matches!(s.kind, StageKind::Check { .. }))
        });
    if fail && check.is_none() {
        return Vec::new();
    }
    for (i, stage) in flow.stages.iter().enumerate() {
        if fail && Some(i) == check {
            let back = editor::fails_to(flow, stage);
            // Fails go back to a stage before the check; a draft naming one after it (the flow check says so on the
            // stage) walks on as if it passed, so the editor always draws.
            let back_at = back.and_then(|b| flow.position(&b.id)).filter(|&at| at < i);
            let (Some(back), Some(back_at)) = (back, back_at) else {
                steps.push(pass_step(flow, stage, 1));
                continue;
            };
            let rounds = stage.rounds.unwrap_or(CHECK_ROUNDS);
            let agent = editor::the_agent(stage).map_or("the checker", String::as_str);
            let (state, text) = match stage.kind {
                StageKind::Check { by: CheckBy::Agent } => (
                    format!("Checking \u{b7} round 1 of {rounds}"),
                    format!(
                        "{} fails it once, so it goes back to {} with {agent}\u{2019}s notes. After {rounds} fails \
                         in a row it would wait for you.",
                        stage.name, back.name
                    ),
                ),
                _ => (
                    "Ready for you".to_owned(),
                    format!("You send it back to {} with a note.", back.name),
                ),
            };
            steps.push(WatchStep {
                column: stage.id.clone(),
                name: stage.name.clone(),
                state,
                text,
            });
            let fixer = editor::the_agent(back).map_or("An agent", String::as_str);
            steps.push(WatchStep {
                column: back.id.clone(),
                name: back.name.clone(),
                state: format!("Working \u{b7} {fixer}"),
                text: format!(
                    "{fixer} fixes it in {} and it comes back to {}.",
                    back.name, stage.name
                ),
            });
            for between in &flow.stages[back_at + 1..i] {
                steps.push(pass_step(flow, between, 1));
            }
            steps.push(pass_step(flow, stage, 2));
            continue;
        }
        steps.push(pass_step(flow, stage, 1));
    }
    steps.push(done_step(flow, base));
    steps
}

fn watch(f: &Facts, base: &str) -> Watch {
    Watch {
        label: "Watch a ticket",
        title: "Watch a ticket go through",
        subtitle: "the draft, not the saved flow",
        passes_label: "It passes",
        fails_label: "A check fails",
        passes: walk(&f.file.flow, base, false),
        fails: walk(&f.file.flow, base, true),
        ticket_label: "YOUR TICKET",
        ticket_title: "A ticket like yours",
        step_of: "Step {n} of {m}",
        back: "Back",
        next: "Next",
        close: "Close",
    }
}

/// Save flow's toast: applied now, or saved and waiting.
fn saved(f: &Facts) -> Saved {
    let added: Vec<&str> = f
        .proposal
        .diff
        .stages
        .iter()
        .filter(|c| matches!(c.change, Change::Added { .. }))
        .map(|c| c.stage.as_str())
        .collect();
    match f.applies {
        DraftApplies::AtOnce | DraftApplies::Nothing => Saved {
            title: "Flow saved",
            text: match added.as_slice() {
                [one] => format!(
                    "Applied now. Tickets that haven\u{2019}t reached {one} will go through it; tickets past it \
                     finish as they are."
                ),
                _ => "Applied now. Tickets that haven\u{2019}t reached a changed stage follow it; tickets in it or \
                      past it finish as they are."
                    .to_owned(),
            },
        },
        DraftApplies::Span { inside, .. } | DraftApplies::WholeBoard { inside } if inside.is_empty() => Saved {
            title: "Flow saved",
            text: "Applied now: no ticket was in the stages it changes.".to_owned(),
        },
        _ => Saved {
            title: "Saved \u{b7} waiting",
            text: editor::change_line(f).unwrap_or_default(),
        },
    }
}

/// Done is selected: How work leaves the flow shows. The delivery's old step id (a toast's "use your own Open PR
/// skill") opens it too.
fn done_selected(f: &Facts) -> bool {
    matches!(f.selected, Some(id) if id == DONE || id == editor::DELIVERY_ID)
}

/// Everything the board's editor draws: `selected` is a stage's id, or `done` for How work leaves the flow.
pub fn view(f: &Facts, b: &BoardFacts) -> View {
    let selected = f.selected.and_then(|id| f.file.flow.stage(id));
    View {
        project: f.project.to_owned(),
        proposal: f.proposal.id,
        yaml: otaq_core::flow::yaml::write(f.file),
        bar: bar(f),
        columns: columns(f),
        details: selected.map(|s| details(f, s)),
        delivery: done_selected(f).then(|| delivery_panel(f, b.base_branch)),
        palette: palette(f),
        start_from: start_from(f, b),
        more: more(f, b),
        suggestion: suggestion(f),
        drafting: drafting(f),
        watch: watch(f, b.base_branch),
        saved: saved(f),
        hint: HintId::FlowColumns,
        choices: editor::choices_kept(f),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::flow::presets::preset;

    fn default_flow() -> Flow {
        presets()[0].read().flow
    }

    #[test]
    fn a_flow_reads_as_its_stages_and_how_it_delivers() {
        assert_eq!(
            chain(&default_flow()),
            "Dev \u{2192} Validate \u{2192} Review \u{b7} merge or PR on Approve"
        );
        let solo = preset("solo-autopilot.yaml").unwrap().read().flow;
        assert!(
            chain(&solo).ends_with("merges by itself"),
            "{}",
            chain(&solo)
        );
        assert_eq!(done_line(&default_flow()), "Merge or PR \u{b7} on Approve");
    }

    #[test]
    fn a_ticket_that_passes_walks_every_stage_once() {
        let steps = walk(&default_flow(), "main", false);
        let path: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(path, ["Backlog", "Dev", "Validate", "Review", "Done"]);
        assert_eq!(steps[2].state, "Checking \u{b7} round 1 of 3");
    }

    #[test]
    fn a_failed_check_goes_back_to_where_its_fails_go_and_comes_round_again() {
        let steps = walk(&default_flow(), "main", true);
        let path: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            path,
            [
                "Backlog", "Dev", "Validate", "Dev", "Validate", "Review", "Done"
            ]
        );
        assert!(
            steps[2].text.contains("goes back to Dev"),
            "{}",
            steps[2].text
        );
        assert!(
            steps[2]
                .text
                .contains("After 3 fails in a row it would wait for you")
        );
        assert_eq!(steps[4].state, "Checking \u{b7} round 2 of 3");
    }

    /// A draft whose check sends its fails to a stage after it (Validate dragged in front of Dev with its fails set
    /// to Dev) still walks: the check passes it on, and nothing goes back.
    #[test]
    fn a_check_whose_fails_go_to_a_later_stage_still_walks() {
        let mut flow = default_flow();
        let validate = flow.position("validate").unwrap();
        flow.stages[validate].fail_to = Some("dev".to_owned());
        let moved = flow.stages.remove(validate);
        flow.stages.insert(0, moved);
        let steps = walk(&flow, "main", true);
        let path: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(path, ["Backlog", "Validate", "Dev", "Review", "Done"]);
    }

    #[test]
    fn a_flow_with_no_check_has_nothing_to_fail() {
        let mut flow = default_flow();
        flow.stages.retain(|s| s.is_agent_stage());
        assert!(walk(&flow, "main", true).is_empty());
    }

    #[test]
    fn the_export_file_is_named_after_the_project() {
        assert_eq!(export_file("shop-web"), "shop-web.flow.yaml");
        assert_eq!(export_file("My app"), "My-app.flow.yaml");
    }
}

//! Edit flow on the board: the board itself becomes the flow's editor, in place. Its columns are the draft's stages
//! between the fixed ends, Backlog ("Every flow starts here") and Done ("Every flow ends here", which carries how work
//! leaves the flow); the board's cards stay where they are, dimmed. Each stage's column header becomes the stage: its
//! name, what happens, its agent and skill, and for a check its rounds and where its fails go. A column header is
//! dragged to reorder, × removes it, **+** between two columns opens the palette of ready stages (data shipped with
//! the app, [`otaq_core::flow::templates`]), and a click opens the stage's details: Name, What happens, What it's
//! for (the stage's description, which every agent reads; there is no separate instructions field), Agent, Skill
//! (Pick a skill… or Ask the agent to write it), Fails go to, Rounds (−/+, 1–5) and whether tickets may skip it.
//! Clicking Done opens How work leaves the flow: the flow's delivery, edited with the same rules as the project's
//! Repository card, then Who does it: the model, effort and instructions that write the pull request (UX-69, A-5),
//! with a stage's model menu and effort rules; the Done tile shows that model. A conflict or a failed merge check is
//! never its work: it goes back to the last agent stage (UX-24).
//!
//! The bar on top says what is being edited and how many changes the draft holds, with Watch a ticket (a pretend
//! ticket walks the draft step by step, passing or failing a check, running nothing), Start from… (the shipped flows,
//! another project's and Import), ⋯ (Export, Import, Copy from another project, Go back to the previous flow), Cancel
//! and Save flow. A box docked at the editor's foot follows Noyan's conversation ([`Ask`]): it asks Noyan's agent for a
//! change in the user's words, or for a whole flow; while the agent drafts it says so, with Show and Stop; when the
//! agent asks the user a question, the box becomes the answer box; and while its suggestion shows, marked on the
//! columns with Why these changes, the box asks for changes to it. The bar then has Watch a ticket, Discard and Save
//! flow.
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
use otaq_core::signals::prompt_line;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::machine::this_machine;
use crate::view_models::agent_panel::{self, LABEL as NOYAN};
use crate::view_models::agent_question::{self, AgentQuestion};
use crate::view_models::agents::Choice;
use crate::view_models::flow_columns::{BACKLOG, DONE};
use crate::view_models::flow_editor::{
    self as editor, CHECK_ROUNDS, EditorKind, Facts, FailToField, MarkView, Pick, SkillField, Who,
};
use crate::view_models::hint::HintId;
use crate::view_models::model_effort;
use crate::view_models::project_settings::{DELIVERS_FIELD, EITHER_NOTE, MODE_FIELD, SKILL_FIELD};
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
/// Under a stage the flow has now with tickets in it, in its details: removing it waits for them. With none in it, the
/// stage goes at once and nothing is said.
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
    /// Who does it (A-1): its model and effort beside the tool's avatar ("gpt-5.5 · medium"), "You" for a check by
    /// you.
    pub who: Option<String>,
    /// The model's tool isn't set up on this Mac (A-4): the who line in amber, this on hover ("Codex isn't set up on
    /// this Mac. …").
    pub who_off: Option<String>,
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
    /// Who does the stage (UX-65): Model, Effort and Instructions; the Skill field is [`Self::skill`].
    pub who: Option<Who>,
    pub skill: Option<SkillField>,
    /// "Otaq's own" beside one of Otaq's skills.
    pub skill_tag: Option<String>,
    pub skill_hint: &'static str,
    pub fail_to: Option<FailToField>,
    pub rounds: Option<Rounds>,
    pub skip: Skip,
    pub problem: Option<String>,
    pub mark: Option<MarkView>,
    pub remove: &'static str,
    /// [`REMOVING_WAITS`] for a stage the flow has now with tickets in it.
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
    /// On Approve · Automatically, always shown, as on the project's Repository card.
    pub modes: Vec<Choice>,
    /// Either is chosen at your click: the modes stay on On Approve, switched off ([`EITHER_NOTE`] says why).
    pub modes_off: bool,
    pub mode_note: Option<&'static str>,
    /// Who does it (UX-69, A-5): the model, effort and instructions that write the pull request (and a merge's
    /// message), with the same model menu and effort rules as a stage's. It never fixes a conflict: that goes back to
    /// the last agent stage (UX-24). `None` for a local merge that names no model (its message is Otaq's plain one).
    pub who: Option<Who>,
    /// Under Who does it's heading: what Done's model writes, and when the last agent stage's session writes it
    /// instead ("Writes the pull request: its title and description. With Dev's model and effort …"). `None` with
    /// [`Self::who`].
    pub writes: Option<String>,
    /// The skill it opens a PR with; `None` for a local merge, which opens none.
    pub skill: Option<DeliverySkill>,
    pub note: &'static str,
}

/// Done panel's PR skill: the user's own Open PR skill by name, or empty for Otaq's own. `FlowBoardDeliverySkill` on
/// the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardDeliverySkill")]
pub struct DeliverySkill {
    pub label: &'static str,
    /// The field `delivery.skill`; an empty value sends Otaq's own.
    pub field: &'static str,
    pub value: String,
    /// "otaq:open-pr": Otaq's own, which runs while the field is empty.
    pub placeholder: &'static str,
    pub hint: &'static str,
    /// "You don't have the skill `my-pr`", amber; it still saves.
    pub warning: Option<String>,
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
    /// "You don't have the skill `test-in-browser`": the usual missing-skill warning, amber, for a ready stage whose
    /// skill neither Otaq ships nor the user has; it can still be added.
    pub warning: Option<String>,
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

/// The box docked at the editor's foot, the board's box for new work in Edit flow. It follows Noyan's conversation
/// ([`AskState`]): the user asks Noyan's agent for a change in their own words (↵), or for a whole flow (the shortcut
/// beside the words); while the agent drafts, the box says so; while its suggestion shows, ↵ asks for changes to it.
/// `FlowBoardAsk` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowBoardAsk")]
pub struct Ask {
    /// "Ask Noyan to change this flow…", or "Ask for changes to Strong's proposal…" while a suggestion shows.
    pub placeholder: String,
    /// The whole-flow shortcut, "Suggest a whole flow"; `None` while a suggestion shows.
    pub whole: Option<&'static str>,
    /// Why the shortcut can't be asked now ("Strong is drafting one"); `None` when it can.
    pub off: Option<String>,
    /// Who answers: "Noyan · Strong · Claude Code".
    pub agent: String,
    /// "Proposes a draft · nothing changes until you save"; while the agent drafts, "Drag, + and × still work
    /// meanwhile".
    pub line: &'static str,
    /// The send button once something is typed: "Ask Noyan", or "Ask for changes".
    pub send: String,
    /// The send key: "↵".
    pub keys: &'static str,
    pub state: AskState,
}

/// Where Noyan's conversation is, as the box follows it. `FlowBoardAskState` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "FlowBoardAskState")]
pub enum AskState {
    /// The user asks for a change, or a whole flow: `flow_board.suggest`.
    Change,
    /// Noyan's agent drafts: the box shows it with Show and Stop, in the words' place.
    Drafting(Drafting),
    /// Noyan's agent asks the user a question: the box is the answer box.
    Asking(Asking),
    /// Its suggestion shows: the words go to the same conversation, `flow_board.ask_changes`.
    Proposal(ProposalAsk),
}

/// The box while a suggestion shows: Show conversation opens the agent that made it. `FlowBoardProposalAsk` on the
/// page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowBoardProposalAsk")]
pub struct ProposalAsk {
    /// The agent that made the suggestion, for Show conversation.
    pub agent: String,
    pub show: &'static str,
}

/// Start from…. `FlowBoardStartFrom` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowBoardStartFrom")]
pub struct StartFrom {
    pub shipped_label: String,
    pub presets: Vec<Entry>,
    pub more_label: &'static str,
    /// "Same as billing-api": each other project's flow.
    pub others: Vec<Entry>,
    pub import: &'static str,
    pub import_line: &'static str,
    pub foot: String,
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
    pub discard: &'static str,
}

/// While Noyan's agent drafts: "Strong is drafting a change to shop-web's flow…" in the box, with Show (its
/// conversation) and Stop (it ends, and the draft keeps what it proposed). While it waits on the user, at a permission
/// prompt or a question, Needs you takes the line's place: "Needs you" with "Strong wants your permission: Allow npm
/// test?" and Answer; the line comes back once it works again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowBoardDrafting")]
pub struct Drafting {
    /// "Needs you" while the agent waits on the user; `None` while it drafts.
    pub needs_you: Option<&'static str>,
    /// One line: a waiting prompt or question shows its first line only, cut with "…".
    pub text: String,
    /// While it waits: the line with the prompt or question whole (up to a few hundred characters), for its tooltip.
    pub tooltip: Option<String>,
    pub agent: String,
    pub show: &'static str,
    pub stop: &'static str,
}

/// While Noyan's agent asks the user a question (`otaq ask human`), the box is the answer box: the question with
/// its options, Answer in terminal and Send answer, as above its terminal. It grows with the question up to 55% of
/// the editor's height and scrolls the question's text past that; ⤢ opens it up to the editor's full height over a
/// scrim, and ⤡, Esc or sending brings it back. A permission prompt isn't a question: it stays the drafting row's
/// Needs you. `FlowBoardAsking` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowBoardAsking")]
pub struct Asking {
    /// "Needs you · Strong asks".
    pub head: String,
    /// The question and how to answer it; Send answer goes to the agent (`agent_panel.answer`).
    pub question: AgentQuestion,
    /// ⤢'s label: "Open larger".
    pub expand: &'static str,
    /// ⤡'s label: "Back to the box".
    pub collapse: &'static str,
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
    pub ask: Ask,
    pub suggestion: Option<Suggestion>,
    pub watch: Watch,
    pub saved: Saved,
    pub hint: HintId,
}

/// What the board's editor reads besides the overlay editor's facts.
pub struct BoardFacts<'a> {
    pub base_branch: &'a str,
    /// The other projects: key, name, and their flow when it reads.
    pub others: &'a [(String, String, Option<FlowFile>)],
    /// The project has a flow before the one in use.
    pub previous: bool,
    /// The stages of the flow in use with tickets in them (held before them included), by id.
    pub occupied: &'a [String],
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
        who_off: None,
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
        let (avatar, who, who_off) = match stage.kind {
            StageKind::Check { by: CheckBy::You } => (None, Some("You".to_owned()), None),
            _ => match model_effort::tile(
                stage.who.model.as_deref(),
                stage.who.effort.as_deref(),
                f.tools,
            ) {
                Some((avatar, line, off)) => (avatar, Some(line), off),
                None => (None, None, None),
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
            who_off,
            skill: stage.skill.clone(),
            check_line: check_line(flow, stage),
            delivery: None,
            description: stage.description.clone(),
            mark,
            warning: editor::stage_skill_warning(f, stage),
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
    // Done's tile shows the model that writes the pull request (A-5).
    let (avatar, who, who_off) = match model_effort::tile(
        flow.delivery.who.model.as_deref(),
        flow.delivery.who.effort.as_deref(),
        f.tools,
    ) {
        Some((avatar, line, off)) => (avatar, Some(line), off),
        None => (None, None, None),
    };
    out.push(Column {
        id: DONE.to_owned(),
        kind: ColumnKind::Done,
        name: "Done".to_owned(),
        icon: IconName::CircleCheck,
        what: ENDS_HERE.to_owned(),
        avatar,
        who,
        who_off,
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

/// Under a stage's Skill field (A-2).
pub const SKILL_HINT: &str =
    "How the agent does it. Empty uses the built-in skill for this kind of stage.";

/// How work leaves the flow's last line.
pub const DELIVERY_NOTE: &str = "Part of the flow, so it travels in the flow file. The project\u{2019}s Repository \
     card shows the same setting.";

/// The model-and-effort pairs the flows in use already have, for the model menu's top (A-3): this project's and every
/// other project's.
fn uses(f: &Facts, b: &BoardFacts) -> Vec<model_effort::Use> {
    let mut out = model_effort::uses_in(f.project_name, &f.in_use.flow);
    for (_, name, file) in b.others {
        if let Some(file) = file {
            out.extend(model_effort::uses_in(name, &file.flow));
        }
    }
    out
}

fn details(f: &Facts, b: &BoardFacts, stage: &Stage) -> Details {
    let d = editor::details(f, stage, &uses(f, b));
    let kind = stage_kind(stage);
    let has_tickets = f.in_use.flow.stage(&stage.id).is_some() && b.occupied.contains(&stage.id);
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
        who: d.who,
        skill: d.skill,
        skill_tag,
        skill_hint: SKILL_HINT,
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
        mark: d.mark,
        remove: "Remove stage",
        remove_note: has_tickets.then_some(REMOVING_WAITS),
    }
}

fn delivery_panel(f: &Facts, b: &BoardFacts) -> DeliveryPanel {
    let base = b.base_branch;
    let d = &f.file.flow.delivery;
    let choice = |id: &str, label: String, selected: bool| Choice {
        id: id.to_owned(),
        label,
        selected,
    };
    let either = d.delivers == Delivers::Either;
    let names_model = d.can_open_pr() || d.who.model.is_some();
    DeliveryPanel {
        title: "How work leaves the flow",
        delivers_label: "Deliver as",
        delivers_field: DELIVERS_FIELD,
        delivers: vec![
            choice("local", format!("Merge into {base}"), d.delivers == Delivers::Local),
            choice("pr", "Pull request".to_owned(), d.delivers == Delivers::Pr),
            choice("either", "Either".to_owned(), either),
        ],
        // The same words and switches as the project's Repository card: with Either the modes show On Approve,
        // switched off, and the line says why.
        delivers_note: either.then(|| EITHER_NOTE.to_owned()),
        mode_label: "When",
        mode_field: MODE_FIELD,
        modes: vec![
            choice(
                "click",
                "On Approve".to_owned(),
                either || d.mode == DeliveryMode::Click,
            ),
            choice(
                "automatic",
                "Automatically".to_owned(),
                !either && d.mode == DeliveryMode::Automatic,
            ),
        ],
        modes_off: either,
        who: names_model.then(|| delivery_who(f, &uses(f, b))),
        writes: names_model.then(|| delivery_writes(&f.file.flow)),
        skill: d.can_open_pr().then(|| delivery_skill(f)),
        mode_note: (!either && d.mode == DeliveryMode::Automatic).then_some(
            "Automatically: delivered as soon as the last stage passes, without Approve. Solo autopilot merges this \
             way; Team PR opens a PR this way.",
        ),
        note: DELIVERY_NOTE,
    }
}

/// What Done's model writes, by how work leaves the flow (UX-69). Not conflicts: they go back to the last agent stage
/// (UX-24).
pub const DELIVERY_WRITES_PR: &str = "Writes the pull request: its title and description.";
pub const DELIVERY_WRITES_EITHER: &str =
    "Writes the pull request\u{2019}s title and description, or the merge message.";
pub const DELIVERY_WRITES_MERGE: &str = "Writes the merge message.";
/// Done's Model hint.
pub fn delivery_hint() -> String {
    format!(
        "Any model of a tool on {}. Every ticket\u{2019}s delivery uses it; it changes only here.",
        this_machine()
    )
}
pub const DELIVERY_INSTRUCTIONS_HINT: &str = "Added to its skill for every ticket it delivers.";

/// What Done's model writes (UX-69) and, with a last agent stage, when that stage's session writes it instead (it has
/// the context).
fn delivery_writes(flow: &Flow) -> String {
    let writes = match flow.delivery.delivers {
        Delivers::Pr => DELIVERY_WRITES_PR,
        Delivers::Either => DELIVERY_WRITES_EITHER,
        Delivers::Local => DELIVERY_WRITES_MERGE,
    };
    match otaq_core::flow::engine::back_from_delivery(flow, &[]) {
        Some(last) => format!(
            "{writes} With {name}\u{2019}s model and effort and no instructions, {name}\u{2019}s session writes it: \
             it has the ticket\u{2019}s context.",
            name = last.name
        ),
        None => writes.to_owned(),
    }
}

/// Done panel's Who does it (UX-69, A-5): Model, Effort and Instructions · optional, the stage's field.
fn delivery_who(f: &Facts, uses: &[model_effort::Use]) -> Who {
    let who = &f.file.flow.delivery.who;
    Who {
        heading: editor::WHO_HEADING,
        picker: model_effort::view(
            who.model.as_deref(),
            who.effort.as_deref(),
            uses,
            f.tools,
            model_effort::Place {
                hint: &delivery_hint(),
                waits_at: None,
            },
        ),
        warning: None,
        instructions_label: editor::INSTRUCTIONS_LABEL,
        instructions: who.instructions.clone().unwrap_or_default(),
        instructions_hint: DELIVERY_INSTRUCTIONS_HINT,
    }
}

/// Done panel's PR skill: where the user's own Open PR skill goes.
fn delivery_skill(f: &Facts) -> DeliverySkill {
    let skill = f.file.flow.delivery.skill.clone().unwrap_or_default();
    let has =
        otaq_core::skills::found::is_shipped(&skill) || f.skills.iter().any(|s| s.name == skill);
    DeliverySkill {
        label: "PR skill",
        field: SKILL_FIELD,
        warning: (!skill.is_empty() && !has)
            .then(|| otaq_core::flow::proposal::unknown_skill_text(&skill)),
        value: skill,
        placeholder: otaq_core::board::settings::WorkflowStep::OpenPr.own_skill(),
        hint: "Your own Open PR skill, by name. Empty: the built-in open-pr runs.",
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
                warning: t
                    .stage()
                    .skill
                    .as_deref()
                    .filter(|s| !has(s))
                    .map(otaq_core::flow::proposal::unknown_skill_text),
            })
            .collect(),
        foot: "Each comes with a description, an agent and a skill; change any of them after.",
    }
}

/// The box at the editor's foot, following Noyan's conversation. A question waits on the user first; then Noyan's agent
/// drafting, also while its suggestion still shows on the columns (it redrafts on the user's Ask for changes, UX-63);
/// then the suggestion, whose answer the box sends. Noyan's agent drafts one change at a time: while it drafts, nothing
/// more is asked.
fn ask(f: &Facts) -> Ask {
    let (name, tool, _) = agent_panel::starts_on(f.new_agent, f.tools);
    let agent = if name.eq_ignore_ascii_case(tool.display_name()) {
        format!("{NOYAN} \u{b7} {name}")
    } else {
        format!("{NOYAN} \u{b7} {name} \u{b7} {}", tool.display_name())
    };
    let change = Ask {
        placeholder: format!("Ask {NOYAN} to change this flow\u{2026}"),
        whole: Some("Suggest a whole flow"),
        off: None,
        agent,
        line: "Proposes a draft \u{b7} nothing changes until you save",
        send: format!("Ask {NOYAN}"),
        keys: "\u{21b5}",
        state: AskState::Change,
    };
    // A question waits on the user whatever else shows: the box answers it first.
    if let Some(asking) = asking(f) {
        return Ask {
            off: f
                .agent
                .map(|a| format!("{} is drafting one", agent_panel::ran_on(a, f.tools))),
            state: AskState::Asking(asking),
            ..change
        };
    }
    if let (Some(d), Some(a)) = (drafting(f), f.agent) {
        return Ask {
            off: Some(format!(
                "{} is drafting one",
                agent_panel::ran_on(a, f.tools)
            )),
            line: "Drag, + and \u{d7} still work meanwhile",
            state: AskState::Drafting(d),
            ..change
        };
    }
    if let Some((by, by_name)) = suggested_by(f).filter(|_| suggestion(f).is_some()) {
        return Ask {
            placeholder: format!("Ask for changes to {by_name}\u{2019}s proposal\u{2026}"),
            whole: None,
            line: "Goes to the same conversation \u{b7} nothing changes until you save",
            send: "Ask for changes".to_owned(),
            state: AskState::Proposal(ProposalAsk {
                agent: by.to_owned(),
                show: "Show conversation",
            }),
            ..change
        };
    }
    change
}

/// The Project agent that made the draft's proposal, and the name the box and the bar give it: the agent its
/// conversation runs on ("Architect"), never the conversation's title (UX-63); `None` for the user's own.
fn suggested_by<'a>(f: &Facts<'a>) -> Option<(&'a str, String)> {
    match &f.proposal.proposed_by {
        ProposedBy::ProjectAgent { agent, tool, .. } => {
            let name = match f.agent.filter(|a| a.id == *agent) {
                Some(a) => agent_panel::ran_on(a, f.tools),
                // The editor doesn't follow its conversation: the tool it ran on.
                None => crate::view_models::tool_avatar::AvatarTool::from(*tool)
                    .name()
                    .to_owned(),
            };
            Some((agent.as_str(), name))
        }
        ProposedBy::User => None,
    }
}

/// The answer box, while Noyan's agent runs and asks the user a question.
fn asking(f: &Facts) -> Option<Asking> {
    let agent = f.agent?;
    Some(Asking {
        head: format!(
            "Needs you \u{b7} {} asks",
            agent_panel::ran_on(agent, f.tools)
        ),
        question: agent_question::of_agent(agent)?,
        expand: "Open larger",
        collapse: "Back to the box",
    })
}

/// What Suggest a whole flow asks Noyan's agent, on the `flow` skill.
pub fn suggest_message(project_name: &str) -> String {
    format!(
        "Suggest a flow for {project_name}. Read this repository and how the project works, then propose a whole flow \
         that fits it, with a short why on each change."
    )
}

/// Noyan's agent is drafting a flow: the editor's agent is mid-turn (or waits on a prompt or a question it asked
/// along the way) and nothing shows yet, or what shows is its own proposal, which it redrafts on the user's Ask for
/// changes (the proposal stays on the columns meanwhile, and Stop keeps it, UX-63). An agent whose turn ended is idle,
/// not drafting: its session stays open after it proposed, so once its suggestion is discarded or saved the line goes
/// with it.
/// A waiting agent mustn't look busy: while it stops at a permission prompt or a question, the line says Needs you
/// and what it waits on instead.
fn drafting(f: &Facts) -> Option<Drafting> {
    let agent = f.agent?;
    let working = agent.mid_turn || agent.needs_you.is_some() || agent.question.is_some();
    let its_own = suggested_by(f).is_some_and(|(by, _)| by == agent.id);
    let drafting =
        matches!(agent.state, AgentState::Running) && working && (f.marks.is_empty() || its_own);
    if !drafting {
        return None;
    }
    // The agent it runs on ("Architect"), not the conversation's title (UX-63).
    let name = agent_panel::ran_on(agent, f.tools);
    // The line stays one line: what it waits on shows its first line (a command that writes a whole file would
    // otherwise push the board down), and the tooltip has it whole.
    let waiting = match (&agent.question, &agent.needs_you) {
        (Some(q), _) => Some((format!("{name} asks: "), q.text.as_str())),
        (None, Some(prompt)) => Some((format!("{name} wants your permission: "), prompt.as_str())),
        (None, None) => None,
    };
    Some(match waiting {
        Some((lead, what)) => Drafting {
            needs_you: Some("Needs you"),
            text: format!("{lead}{}", prompt_line(what)),
            tooltip: Some(format!("{lead}{}", tooltip_text(what))),
            agent: agent.id.clone(),
            show: "Answer",
            stop: "Stop",
        },
        None => Drafting {
            needs_you: None,
            text: drafting_text(&name, f.project_name),
            tooltip: None,
            agent: agent.id.clone(),
            show: "Show",
            stop: "Stop",
        },
    })
}

/// The most of a waiting prompt a tooltip shows (this Needs you line's, the Needs you strip's chips): a whole SKILL.md
/// would be taller than the window; the terminal has it all.
pub(crate) const TOOLTIP_MAX_CHARS: usize = 600;

/// What the agent waits on, whole for the tooltip, cut with "…" past [`TOOLTIP_MAX_CHARS`].
pub(crate) fn tooltip_text(what: &str) -> String {
    let what = what.trim();
    match what.char_indices().nth(TOOLTIP_MAX_CHARS) {
        Some((cut, _)) => format!("{}\u{2026}", what[..cut].trim_end()),
        None => what.to_owned(),
    }
}

/// "Strong is drafting a change to shop-web's flow…".
fn drafting_text(agent: &str, project_name: &str) -> String {
    format!("{agent} is drafting a change to {project_name}\u{2019}s flow\u{2026}")
}

/// Start from…'s last line (A-7).
pub fn start_from_foot() -> String {
    format!(
        "Replaces the stages between Backlog and Done, and how work is delivered. Shipped flows use the default \
         model of the tools on {}: what your account is set to. Change any stage after. Nothing changes until you \
         save.",
        this_machine()
    )
}

/// A shipped flow's line in Start from… (A-7): "Dev → Validate → Review, each on your tools’ default model · merge or
/// PR on Approve".
pub fn shipped_chain(flow: &Flow) -> String {
    with_words(flow, "each on your tools\u{2019} default model")
}

/// Same as <project>'s line (A-7): "Dev → Validate → Review, each with the model, effort and skill it has there · …".
pub fn copied_chain(flow: &Flow) -> String {
    with_words(flow, "each with the model, effort and skill it has there")
}

/// [`chain`] with `words` after the stages, when an agent works or checks one of them.
fn with_words(flow: &Flow, words: &str) -> String {
    if !flow.stages.iter().any(Stage::runs_agent) {
        return chain(flow);
    }
    let stages: Vec<&str> = flow.stages.iter().map(|s| s.name.as_str()).collect();
    format!(
        "{}, {words} \u{b7} {}",
        stages.join(" \u{2192} "),
        delivery_words(flow)
    )
}

fn start_from(b: &BoardFacts) -> StartFrom {
    StartFrom {
        shipped_label: format!(
            "SHIPPED WITH {}",
            otaq_core::naming::PRODUCT_NAME.to_uppercase()
        ),
        presets: presets()
            .iter()
            .map(|p| Entry {
                key: p.file.to_owned(),
                name: p.name.clone(),
                line: shipped_chain(&p.read().flow),
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
                    line: copied_chain(&file.as_ref()?.flow),
                    pick: Pick::Project {
                        project: key.clone(),
                    },
                })
            })
            .collect(),
        import: "Import a flow file\u{2026}",
        import_line: "A .flow.yaml someone shared",
        foot: start_from_foot(),
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
        export_line: format!("{file} \u{b7} stages, models, skills, delivery"),
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
    let (_, name) = suggested_by(f)?;
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
        discard: "Discard",
    })
}

/// What the pretend ticket's card says in a stage it passes, and what happens there.
fn pass_step(flow: &Flow, stage: &Stage, round: u32) -> WatchStep {
    let who = editor::stage_who(stage);
    let agent = who.as_deref().unwrap_or("An agent");
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
            let who = editor::stage_who(stage);
            let agent = who.as_deref().unwrap_or("the checker");
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
            let fixer = editor::stage_who(back);
            let fixer = fixer.as_deref().unwrap_or("An agent");
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
        details: selected.map(|s| details(f, b, s)),
        delivery: done_selected(f).then(|| delivery_panel(f, b)),
        palette: palette(f),
        start_from: start_from(b),
        more: more(f, b),
        ask: ask(f),
        suggestion: suggestion(f),
        watch: watch(f, b.base_branch),
        saved: saved(f),
        hint: HintId::FlowColumns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Noyan's conversation `c1`, titled `title` (the agent panel's name for it), on the named agent `on`: the box
    /// and the bar name the agent, never the title (UX-63).
    fn noyan(title: &str, on: &str) -> otaq_core::project_agents::ProjectAgent {
        let mut a = crate::view_models::noyan_button::fixtures::agent("c1", title, 0);
        a.session = Some(crate::view_models::agent_panel::session_on(on));
        a
    }
    use otaq_core::flow::presets::preset;

    /// Why Suggest a whole flow can't be asked now, as the box at the editor's foot says.
    fn suggest_off(f: &Facts) -> Option<String> {
        ask(f).off
    }

    /// The board's facts with `occupied`'s stages holding tickets.
    fn board(occupied: &[String]) -> BoardFacts<'_> {
        BoardFacts {
            base_branch: "main",
            others: &[],
            previous: false,
            occupied,
        }
    }

    /// The note beside Remove stage says removing waits only while tickets are in the stage; with none in it the
    /// stage goes at once, so no note shows (a stage only the draft has never shows it).
    #[test]
    fn the_remove_note_shows_only_with_tickets_in_the_stage() {
        use crate::view_models::flow_editor::fixtures::{proposal, shop_web};

        let file = shop_web();
        let draft = proposal(&file, true);
        let tools = crate::view_models::tool_problems::Tools::default();
        let f = Facts {
            project: "SHOP",
            project_name: "shop",
            proposal: &draft,
            marks: &[],
            swap: false,
            applies: &DraftApplies::Nothing,
            file: &file,
            in_use: &file,
            tools: &tools,
            skills: &[],
            writing: &[],
            written: &[],
            selected: Some("validate"),
            agent: None,
            new_agent: None,
        };
        let note = |occupied: &[String]| {
            view(&f, &board(occupied))
                .details
                .expect("Validate's panel")
                .remove_note
        };
        assert_eq!(note(&[]), None, "no ticket in Validate");
        assert_eq!(note(&["dev".to_owned()]), None, "tickets in another stage");
        assert_eq!(note(&["validate".to_owned()]), Some(REMOVING_WAITS));
        let panel = view(&f, &board(&[])).details.expect("Validate's panel");
        assert_eq!(
            panel.remove, "Remove stage",
            "Remove stage stays either way"
        );
    }

    /// A-1…A-4: each tile shows its model and effort beside the tool's avatar ("You" for a check by you); the
    /// panel's Who does it offers the set-up tools with the pairs the flows use on top; a model whose tool isn't set
    /// up is marked on its tile and in its panel, kept as it is.
    #[test]
    fn who_does_it_shows_on_the_tile_and_in_the_panel() {
        use crate::view_models::agents::fixtures::all_lists;
        use crate::view_models::flow_editor::fixtures::{proposal, shop_web};
        use crate::view_models::flow_editor::{StageEdit, edited};
        use otaq_core::agent_tools::Tool;

        let s = |t: &str| t.to_owned();
        let (file, _) = edited(
            &shop_web(),
            &StageEdit::Pair {
                stage: s("validate"),
                model: s("codex:gpt-5.5"),
                effort: Some(s("medium")),
            },
        )
        .unwrap();
        let (file, _) = edited(
            &file,
            &StageEdit::Pair {
                stage: s("dev"),
                model: s("claude:opus"),
                effort: Some(s("high")),
            },
        )
        .unwrap();
        let draft = proposal(&file, true);
        let mut tools = crate::view_models::tool_problems::Tools {
            connected: vec![Tool::Claude, Tool::Codex],
            lists: all_lists(),
            ..Default::default()
        };
        let other = (s("BILL"), s("billing-api"), Some(file.clone()));
        let others = [other];
        let b = BoardFacts {
            base_branch: "main",
            others: &others,
            previous: false,
            occupied: &[],
        };
        let render = |tools: &crate::view_models::tool_problems::Tools| {
            view(
                &Facts {
                    project: "SHOP",
                    project_name: "shop-web",
                    proposal: &draft,
                    marks: &[],
                    swap: false,
                    applies: &DraftApplies::Nothing,
                    file: &file,
                    in_use: &file,
                    tools,
                    skills: &[],
                    writing: &[],
                    written: &[],
                    selected: Some("validate"),
                    agent: None,
                    new_agent: None,
                },
                &b,
            )
        };
        let v = render(&tools);
        let tile = |v: &View, id: &str| {
            let c = v.columns.iter().find(|c| c.id == id).unwrap();
            (
                c.avatar.as_ref().map(|a| a.letters.clone()),
                c.who.clone(),
                c.who_off.is_some(),
            )
        };
        assert_eq!(
            tile(&v, "dev"),
            (Some(s("CC")), Some(s("opus \u{b7} high")), false)
        );
        assert_eq!(
            tile(&v, "validate"),
            (Some(s("CX")), Some(s("gpt-5.5 \u{b7} medium")), false)
        );
        assert_eq!(tile(&v, "review"), (None, Some(s("You")), false));
        let who = v
            .details
            .as_ref()
            .unwrap()
            .who
            .as_ref()
            .expect("Validate's Who does it");
        assert_eq!(who.picker.value.text, "Codex \u{b7} gpt-5.5");
        assert_eq!(who.instructions_label, "Instructions \u{b7} optional");
        let places: Vec<&str> = who
            .picker
            .menu
            .pairs
            .iter()
            .map(|p| p.places.as_str())
            .collect();
        assert_eq!(
            places,
            [
                "Dev in shop-web and billing-api",
                "Validate in shop-web and billing-api"
            ]
        );
        assert!(who.picker.menu.pairs[1].selected);

        // A-4: Codex signed out. Validate keeps its model, marked; tickets wait there.
        tools.connected = vec![Tool::Claude];
        let v = render(&tools);
        assert_eq!(
            tile(&v, "validate"),
            (Some(s("CX")), Some(s("gpt-5.5 \u{b7} medium")), true)
        );
        let who = v.details.as_ref().unwrap().who.as_ref().unwrap();
        let off = who.picker.not_set_up.as_ref().expect("marked");
        assert_eq!(
            off.text,
            format!(
                "Codex isn\u{2019}t set up on {}. Tickets wait at Validate until you set it up or pick another \
                 model.",
                this_machine()
            )
        );
        // Codex's pair isn't offered while Codex isn't set up (tuq 014).
        assert_eq!(who.picker.menu.pairs.len(), 1);
    }

    /// A-5, UX-69: clicking Done shows, under Deliver as and When, the model, effort and instructions that write the
    /// pull request, with a stage's menu (the pairs the flows use on top) and Effort; the Done tile shows the model.
    /// Its text says it writes the pull request and when Dev's session writes it instead; never that it fixes
    /// conflicts (UX-24).
    #[test]
    fn dones_panel_picks_the_model_that_writes_the_pull_request() {
        use crate::view_models::agents::fixtures::all_lists;
        use crate::view_models::flow_editor::fixtures::{proposal, shop_web};
        use crate::view_models::flow_editor::{StageEdit, edited};
        use otaq_core::agent_tools::Tool;

        let s = |t: &str| t.to_owned();
        let apply = |file: &FlowFile, edit: StageEdit| edited(file, &edit).unwrap().0;
        let file = apply(
            &shop_web(),
            StageEdit::Pair {
                stage: s("dev"),
                model: s("claude:opus"),
                effort: Some(s("high")),
            },
        );
        let file = apply(
            &file,
            StageEdit::DeliveryPair {
                model: s("codex:gpt-5.5"),
                effort: Some(s("medium")),
            },
        );
        let file = apply(
            &file,
            StageEdit::DeliveryInstructions {
                instructions: s("Keep the title under 70 characters."),
            },
        );
        let draft = proposal(&file, true);
        let tools = crate::view_models::tool_problems::Tools {
            connected: vec![Tool::Claude, Tool::Codex],
            lists: all_lists(),
            ..Default::default()
        };
        let f = Facts {
            project: "SHOP",
            project_name: "shop-web",
            proposal: &draft,
            marks: &[],
            swap: false,
            applies: &DraftApplies::Nothing,
            file: &file,
            in_use: &file,
            tools: &tools,
            skills: &[],
            writing: &[],
            written: &[],
            selected: Some(DONE),
            agent: None,
            new_agent: None,
        };
        let v = view(&f, &board(&[]));
        let done = v.columns.iter().find(|c| c.id == DONE).unwrap();
        assert_eq!(
            (
                done.avatar.as_ref().map(|a| a.letters.as_str()),
                done.who.as_deref()
            ),
            (Some("CX"), Some("gpt-5.5 \u{b7} medium"))
        );
        let panel = v.delivery.expect("Done's panel");
        let who = panel.who.as_ref().expect("a PR names its model");
        assert_eq!(who.heading, "Who does it");
        assert_eq!(who.picker.value.text, "Codex \u{b7} gpt-5.5");
        assert_eq!(who.instructions, "Keep the title under 70 characters.");
        let effort = who.picker.effort.as_ref().expect("Codex has levels");
        let picked: Vec<&str> = effort
            .levels
            .iter()
            .filter(|l| l.selected)
            .map(|l| l.id.as_str())
            .collect();
        assert_eq!(picked, ["medium"]);
        let pairs: Vec<&str> = who
            .picker
            .menu
            .pairs
            .iter()
            .map(|p| p.places.as_str())
            .collect();
        assert!(pairs.contains(&"Done in shop-web"), "{pairs:?}");
        assert_eq!(
            panel.writes.as_deref(),
            Some(
                "Writes the pull request: its title and description. With Dev\u{2019}s model and effort and no \
                 instructions, Dev\u{2019}s session writes it: it has the ticket\u{2019}s context."
            )
        );
        let text = format!(
            "{:?} {} {}",
            panel.writes, who.picker.hint, who.instructions_hint
        )
        .to_lowercase();
        assert!(
            !text.contains("conflict") && !text.contains("merge check"),
            "{text}"
        );

        // A local merge's model writes its message.
        let local = apply(
            &file,
            StageEdit::Delivery {
                field: s(DELIVERS_FIELD),
                value: s("local"),
            },
        );
        let writes = delivery_writes(&local.flow);
        assert!(
            writes.starts_with("Writes the merge message. With Dev"),
            "{writes}"
        );
    }

    /// A-7: shipped flows say each stage is on the tools' default model; Same as <project> copies each stage's.
    #[test]
    fn start_from_says_whose_models_each_flow_brings() {
        let b = board(&[]);
        let start = start_from(&b);
        assert_eq!(
            start.presets[0].line,
            "Dev \u{2192} Validate \u{2192} Review, each on your tools\u{2019} default model \u{b7} merge or PR on \
             Approve"
        );
        assert_eq!(
            copied_chain(&default_flow()),
            "Dev \u{2192} Validate \u{2192} Review, each with the model, effort and skill it has there \u{b7} merge \
             or PR on Approve"
        );
        assert!(
            start
                .foot
                .contains(&format!("default model of the tools on {}", this_machine()))
        );
    }

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

    /// A stage whose Codex skill's SKILL.md starts with a byte order mark warns on its card and its panel, as a
    /// missing skill does: Codex finds the skill but doesn't load it. Without the mark, neither warns.
    #[test]
    fn a_stage_whose_skill_has_a_byte_order_mark_warns() {
        use crate::view_models::flow_editor::SkillState;
        use crate::view_models::flow_editor::fixtures::{proposal, shop_web, users_skill};
        use otaq_core::agent_tools::Tool;
        use otaq_core::skills::found::{FoundSkill, SkillScope};

        let mut file = shop_web();
        file.flow
            .stages
            .iter_mut()
            .find(|s| s.id == "validate")
            .expect("Validate")
            .skill = Some("retest-docs".into());
        let draft = proposal(&file, true);
        for s in file.flow.stages.iter_mut().filter(|s| s.runs_agent()) {
            s.who = otaq_core::flow::WhoDoesIt::on(Tool::Codex.as_str());
        }
        let tools = crate::view_models::tool_problems::Tools::default();
        let warnings = |skill: &FoundSkill| {
            let skills = [skill.clone()];
            let f = Facts {
                project: "SHOP",
                project_name: "shop",
                proposal: &draft,
                marks: &[],
                swap: false,
                applies: &DraftApplies::Nothing,
                file: &file,
                in_use: &file,
                tools: &tools,
                skills: &skills,
                writing: &[],
                written: &[],
                selected: Some("validate"),
                agent: None,
                new_agent: None,
            };
            let card = columns(&f)
                .into_iter()
                .find(|c| c.id == "validate")
                .expect("Validate's card")
                .warning;
            let stage = file.flow.stage("validate").expect("Validate");
            let panel = match details(&f, &board(&[]), stage)
                .skill
                .expect("a Skill field")
                .state
            {
                SkillState::Set { warning, .. } => warning,
                other => panic!("the skill is set: {other:?}"),
            };
            (card, panel)
        };

        let mut docs = users_skill(
            "retest-docs",
            "Writes the docs.",
            SkillScope::Personal,
            "~/.codex/skills",
            &[],
        );
        docs.skipped = vec![Tool::Codex];
        let text = "Codex doesn\u{2019}t load the skill `$retest-docs`: its SKILL.md starts with a byte order mark";
        let (card, panel) = warnings(&docs);
        assert_eq!(card.as_deref(), Some(text), "the card warns");
        assert_eq!(panel.as_deref(), Some(text), "the panel warns");

        // Without the mark Codex has it: no warning.
        docs.skipped.clear();
        docs.tools = vec![Tool::Codex];
        docs.places[0].tools = vec![Tool::Codex];
        assert_eq!(warnings(&docs), (None, None));
    }

    /// The box follows Noyan's conversation: asking for a change, then the agent drafting it (Show and Stop, the
    /// shortcut off with why), then its proposal on the columns, where the box asks for changes to it with no
    /// shortcut, and Show conversation opens the agent that made it.
    #[test]
    fn the_box_follows_noyans_conversation() {
        use crate::view_models::flow_editor::fixtures::{proposal, shop_web};
        use crate::view_models::noyan_button::fixtures::agent;
        use otaq_core::flow::draft::StageMark;

        let file = shop_web();
        let mut draft = proposal(&file, false);
        let tools = crate::view_models::tool_problems::Tools::default();
        let marks = [StageMark {
            stage_id: "review".into(),
            stage: "Review".into(),
            mark: Mark::Changed,
            by: "Strong".into(),
            why: None,
        }];
        let facts = |draft: &'_ otaq_core::flow::proposal::Proposal,
                     marks: &'_ [StageMark],
                     a: Option<&otaq_core::project_agents::ProjectAgent>|
         -> Ask {
            ask(&Facts {
                project: "SHOP",
                project_name: "shop-web",
                proposal: draft,
                marks,
                swap: false,
                applies: &DraftApplies::Nothing,
                file: &file,
                in_use: &file,
                tools: &tools,
                skills: &[],
                writing: &[],
                written: &[],
                selected: None,
                agent: a,
                new_agent: None,
            })
        };

        let change = facts(&draft, &[], None);
        assert_eq!(change.state, AskState::Change);
        assert_eq!(change.placeholder, "Ask Noyan to change this flow\u{2026}");
        assert_eq!(change.whole, Some("Suggest a whole flow"));
        assert_eq!(change.off, None);

        let mut a = noyan("New conversation", "Strong");
        a.mid_turn = true;
        let drafting = facts(&draft, &[], Some(&a));
        let AskState::Drafting(d) = &drafting.state else {
            panic!("{drafting:?}");
        };
        assert_eq!(
            d.text,
            "Strong is drafting a change to shop-web\u{2019}s flow\u{2026}"
        );
        assert_eq!((d.agent.as_str(), d.show, d.stop), ("c1", "Show", "Stop"));
        assert_eq!(drafting.off.as_deref(), Some("Strong is drafting one"));
        assert_eq!(drafting.line, "Drag, + and \u{d7} still work meanwhile");

        // Its proposal shows: the agent ended its turn, waiting for the answer the box sends.
        a.mid_turn = false;
        draft.proposed_by = ProposedBy::ProjectAgent {
            agent: "c1".into(),
            session: "s1".into(),
            name: "Add Security stage".into(),
            tool: otaq_core::agent_tools::Tool::Claude,
        };
        let proposed = facts(&draft, &marks, Some(&a));
        assert_eq!(
            proposed.state,
            AskState::Proposal(ProposalAsk {
                agent: "c1".into(),
                show: "Show conversation",
            })
        );
        assert_eq!(
            proposed.placeholder,
            "Ask for changes to Strong\u{2019}s proposal\u{2026}"
        );
        assert_eq!(proposed.whole, None);
        assert_eq!(proposed.off, None);
        assert_eq!(proposed.send, "Ask for changes");
        // The box doesn't follow its conversation: the tool it ran on, still not the conversation's title.
        assert_eq!(
            facts(&draft, &marks, None).placeholder,
            "Ask for changes to Claude Code\u{2019}s proposal\u{2026}"
        );
        // The bar's title names the agent too.
        let title = suggestion(&Facts {
            project: "SHOP",
            project_name: "shop-web",
            proposal: &draft,
            marks: &marks,
            swap: false,
            applies: &DraftApplies::Nothing,
            file: &file,
            in_use: &file,
            tools: &tools,
            skills: &[],
            writing: &[],
            written: &[],
            selected: None,
            agent: Some(&a),
            new_agent: None,
        })
        .map(|s| s.title);
        assert_eq!(
            title.as_deref(),
            Some("Strong\u{2019}s suggestion for shop-web")
        );

        // Asked for changes: the proposal stays on the columns (its marks too) while the agent redrafts it, and the
        // box says so with Show and Stop (UX-63: Stop keeps what it proposed).
        a.mid_turn = true;
        let redrafting = facts(&draft, &marks, Some(&a));
        let AskState::Drafting(d) = &redrafting.state else {
            panic!("{redrafting:?}");
        };
        assert_eq!(
            d.text,
            "Strong is drafting a change to shop-web\u{2019}s flow\u{2026}"
        );
        assert_eq!((d.agent.as_str(), d.show, d.stop), ("c1", "Show", "Stop"));
        assert_eq!(redrafting.whole, Some("Suggest a whole flow"));
        assert_eq!(redrafting.off.as_deref(), Some("Strong is drafting one"));
        // Another conversation at work (one writing a stage's skill) leaves the proposal's box as it is.
        let mut other = agent("c2", "Strong", 0);
        other.mid_turn = true;
        assert!(matches!(
            facts(&draft, &marks, Some(&other)).state,
            AskState::Proposal(_)
        ));
        // Stopped, or its turn ended: the proposal's box again.
        a.state = AgentState::Ended;
        assert!(matches!(
            facts(&draft, &marks, Some(&a)).state,
            AskState::Proposal(_)
        ));
    }

    /// When Noyan's agent asks the user a question, the box becomes the answer box: the question with its options and
    /// the words to answer it, under "Needs you · Strong asks". A permission prompt isn't a question: the drafting row
    /// keeps its Needs you with Answer. Once answered, the agent drafts again.
    #[test]
    fn the_box_answers_noyans_question() {
        use crate::view_models::flow_editor::fixtures::{proposal, shop_web};

        let file = shop_web();
        let draft = proposal(&file, true);
        let tools = crate::view_models::tool_problems::Tools::default();
        let box_of = |a: &otaq_core::project_agents::ProjectAgent| {
            ask(&Facts {
                project: "SHOP",
                project_name: "shop-web",
                proposal: &draft,
                marks: &[],
                swap: false,
                applies: &DraftApplies::Nothing,
                file: &file,
                in_use: &file,
                tools: &tools,
                skills: &[],
                writing: &[],
                written: &[],
                selected: None,
                agent: Some(a),
                new_agent: None,
            })
        };

        let mut a = noyan("New conversation", "Strong");
        a.mid_turn = true;
        a.question = Some(otaq_core::board::questions::HumanQuestion {
            id: "q1".into(),
            text: "Should the security review run before or after Validate?".into(),
            options: vec!["After Validate".into(), "Before Validate".into()],
            recommended: vec![1],
            multiple: false,
            notes: vec![
                Some("Only code that passes the tests gets reviewed.".into()),
                None,
            ],
            asked_at: 0,
            send_back: None,
        });
        let asking = box_of(&a);
        let AskState::Asking(q) = &asking.state else {
            panic!("{asking:?}");
        };
        assert_eq!(q.head, "Needs you \u{b7} Strong asks");
        assert_eq!((q.expand, q.collapse), ("Open larger", "Back to the box"));
        assert_eq!(q.question.agent, "c1");
        assert_eq!(q.question.project, a.project);
        assert_eq!(
            q.question.answer.line,
            "Should the security review run before or after Validate?"
        );
        let labels: Vec<_> = q
            .question
            .answer
            .rows
            .iter()
            .map(|r| r.label.as_str())
            .collect();
        assert_eq!(labels[..2], ["After Validate", "Before Validate"]);
        assert!(q.question.answer.rows[0].picked);
        assert_eq!(q.question.answer.send, "Send answer");
        assert_eq!(q.question.answer.in_terminal, "Answer in terminal");
        assert_eq!(asking.off.as_deref(), Some("Strong is drafting one"));

        // A permission prompt keeps the drafting row's Needs you.
        a.question = None;
        a.needs_you = Some("Allow npm test?".into());
        let AskState::Drafting(d) = box_of(&a).state else {
            panic!("a permission prompt is not a question");
        };
        assert_eq!(d.needs_you, Some("Needs you"));
        assert_eq!(d.text, "Strong wants your permission: Allow npm test?");
        assert_eq!(d.show, "Answer");

        // Answered: it drafts again.
        a.needs_you = None;
        let AskState::Drafting(d) = box_of(&a).state else {
            panic!("drafting again");
        };
        assert_eq!(d.needs_you, None);

        // An ended agent asks nothing.
        a.question = Some(otaq_core::board::questions::HumanQuestion {
            id: "q2".into(),
            text: "Keep Review?".into(),
            options: Vec::new(),
            recommended: Vec::new(),
            multiple: false,
            notes: Vec::new(),
            asked_at: 0,
            send_back: None,
        });
        a.state = AgentState::Ended;
        assert_eq!(box_of(&a).state, AskState::Change);
    }

    /// The box's drafting state shows only while Noyan's agent works on the draft. After Discard or Save the draft
    /// has no marks and the agent's turn has ended (its session stays open, idle): the box asks again, and Suggest can
    /// be asked again.
    #[test]
    fn the_drafting_line_goes_when_the_draft_does() {
        use crate::view_models::flow_editor::fixtures::{proposal, shop_web};

        let file = shop_web();
        let draft = proposal(&file, true);
        let tools = crate::view_models::tool_problems::Tools::default();
        let line = |a: &otaq_core::project_agents::ProjectAgent| {
            let f = Facts {
                project: "SHOP",
                project_name: "shop",
                proposal: &draft,
                marks: &[],
                swap: false,
                applies: &DraftApplies::Nothing,
                file: &file,
                in_use: &file,
                tools: &tools,
                skills: &[],
                writing: &[],
                written: &[],
                selected: None,
                agent: Some(a),
                new_agent: None,
            };
            (drafting(&f).map(|d| d.text), suggest_off(&f))
        };

        let mut a = noyan("Shop repo flow", "Architect");
        a.mid_turn = true;
        assert_eq!(
            line(&a).0.as_deref(),
            Some("Architect is drafting a change to shop\u{2019}s flow\u{2026}"),
            "while its turn runs"
        );
        a.mid_turn = false;
        a.needs_you = Some("Allow npm test?".into());
        assert!(
            line(&a).0.is_some(),
            "a permission prompt along the way is still drafting"
        );

        // Discarded or saved: no marks, and the agent sits idle with its session open.
        a.needs_you = None;
        assert_eq!(line(&a), (None, None), "an idle agent drafts nothing");
        a.state = AgentState::Ended;
        a.mid_turn = true;
        assert_eq!(line(&a), (None, None), "nor does an ended one");
    }

    /// Once the agent has written a stage's skill, the stage's column no longer says the user doesn't have it: it
    /// warns that the skill is written but not committed yet (Apply doesn't commit it), still only a warning.
    #[test]
    fn a_written_skill_warns_it_is_not_committed_yet() {
        use crate::view_models::flow_editor::fixtures::{plan_draft, written_plan};
        use otaq_core::flow::proposal::{Level, Warning};

        let (file, mut draft) = plan_draft(Some("plan-ticket"));
        draft.diff.warnings = vec![Warning {
            level: Level::Amber,
            stage: Some("Plan".into()),
            text: "You don't have the skill `plan-ticket`".into(),
        }];
        let tools = crate::view_models::tool_problems::Tools::default();
        let written = [written_plan()];
        let plan_warning = |written: &[otaq_core::flow::draft::WrittenSkill]| {
            let f = Facts {
                project: "SHOP",
                project_name: "shop",
                proposal: &draft,
                marks: &[],
                swap: false,
                applies: &DraftApplies::AtOnce,
                file: &file,
                in_use: &file,
                tools: &tools,
                skills: &[],
                writing: &[],
                written,
                selected: None,
                agent: None,
                new_agent: None,
            };
            columns(&f)
                .into_iter()
                .find(|c| c.id == "plan")
                .and_then(|c| c.warning)
        };
        assert_eq!(
            plan_warning(&[]).as_deref(),
            Some("You don't have the skill `plan-ticket`")
        );
        assert_eq!(
            plan_warning(&written).as_deref(),
            Some("`plan-ticket` is written but not committed yet: commit it so agents get it")
        );
    }

    /// While the drafting agent waits on the user, at a permission prompt or a question, Needs you takes the line's
    /// place (Suggest stays off); once the user answers and it works again, "drafting a change" comes back.
    #[test]
    fn needs_you_replaces_the_drafting_line_while_the_agent_waits() {
        use crate::view_models::flow_editor::fixtures::{proposal, shop_web};

        let file = shop_web();
        let draft = proposal(&file, true);
        let tools = crate::view_models::tool_problems::Tools::default();
        let line = |a: &otaq_core::project_agents::ProjectAgent| {
            let f = Facts {
                project: "SHOP",
                project_name: "shop",
                proposal: &draft,
                marks: &[],
                swap: false,
                applies: &DraftApplies::Nothing,
                file: &file,
                in_use: &file,
                tools: &tools,
                skills: &[],
                writing: &[],
                written: &[],
                selected: None,
                agent: Some(a),
                new_agent: None,
            };
            let d = drafting(&f).unwrap();
            (d.needs_you, d.text, d.show, suggest_off(&f))
        };
        let drafting_line =
            "Architect is drafting a change to shop\u{2019}s flow\u{2026}".to_owned();
        let busy = Some("Architect is drafting one".to_owned());

        let mut a = noyan("Shop repo flow", "Architect");
        a.mid_turn = true;
        assert_eq!(
            line(&a),
            (None, drafting_line.clone(), "Show", busy.clone())
        );

        // At a permission prompt (its turn hasn't ended).
        a.needs_you = Some("Allow npm test?".into());
        assert_eq!(
            line(&a),
            (
                Some("Needs you"),
                "Architect wants your permission: Allow npm test?".to_owned(),
                "Answer",
                busy.clone()
            )
        );

        // Answered: it works again, and the line is back.
        a.needs_you = None;
        assert_eq!(
            line(&a),
            (None, drafting_line.clone(), "Show", busy.clone())
        );

        // A question for the user.
        a.question = Some(otaq_core::board::questions::HumanQuestion {
            id: "q1".into(),
            text: "Keep the Validate stage?".into(),
            options: Vec::new(),
            recommended: Vec::new(),
            multiple: false,
            notes: Vec::new(),
            asked_at: 0,
            send_back: None,
        });
        assert_eq!(
            line(&a),
            (
                Some("Needs you"),
                "Architect asks: Keep the Validate stage?".to_owned(),
                "Answer",
                busy.clone()
            )
        );
        a.question = None;
        assert_eq!(line(&a), (None, drafting_line, "Show", busy));
    }

    /// A permission prompt for a long, many-line command (Codex writing a whole SKILL.md with
    /// `@' … '@ | Set-Content`) keeps Needs you on one line: the command's first line with "…", and the tooltip
    /// has it whole.
    #[test]
    fn needs_you_for_a_long_command_stays_one_line() {
        use crate::view_models::flow_editor::fixtures::{proposal, shop_web};

        let file = shop_web();
        let draft = proposal(&file, true);
        let tools = crate::view_models::tool_problems::Tools::default();
        let line = |a: &otaq_core::project_agents::ProjectAgent| {
            let d = drafting(&Facts {
                project: "SHOP",
                project_name: "shop",
                proposal: &draft,
                marks: &[],
                swap: false,
                applies: &DraftApplies::Nothing,
                file: &file,
                in_use: &file,
                tools: &tools,
                skills: &[],
                writing: &[],
                written: &[],
                selected: None,
                agent: Some(a),
                new_agent: None,
            })
            .unwrap();
            (d.text, d.tooltip.unwrap_or_default())
        };
        let body = "Update the docs for the ticket. ".repeat(60);
        let mut a = noyan("Docs stage skill", "Architect");
        a.mid_turn = true;
        a.needs_you = Some(format!(
            "Run command: @'\n---\nname: update-docs\ndescription: Update project documentation\n---\n\n{body}\n'@ \
             | Set-Content -LiteralPath .agents\\skills\\update-docs\\SKILL.md -Encoding utf8"
        ));
        let (text, tooltip) = line(&a);
        assert_eq!(
            text,
            "Architect wants your permission: Run command: @' \u{2026}"
        );
        // The tooltip has the command on its lines, up to a height that fits the window.
        assert!(
            tooltip.starts_with(
                "Architect wants your permission: Run command: @'\n---\nname: update-docs\n"
            ),
            "{tooltip}"
        );
        assert!(tooltip.ends_with('\u{2026}'), "{tooltip}");
        assert!(tooltip.chars().count() < 700, "{tooltip}");
        // A short one is whole in the tooltip.
        a.needs_you = Some("Run command: git status\ngit diff".into());
        assert_eq!(
            line(&a),
            (
                "Architect wants your permission: Run command: git status \u{2026}".to_owned(),
                "Architect wants your permission: Run command: git status\ngit diff".to_owned()
            )
        );

        // One long line: cut with "…" to a bounded length.
        a.needs_you = Some(format!("Run command: {}", body.trim()));
        let (text, _) = line(&a);
        assert!(!text.contains('\n'), "{text}");
        assert!(text.ends_with('\u{2026}'), "{text}");
        assert!(text.chars().count() < 250, "{text}");
    }

    #[test]
    fn the_export_file_is_named_after_the_project() {
        assert_eq!(export_file("shop-web"), "shop-web.flow.yaml");
        assert_eq!(export_file("My app"), "My-app.flow.yaml");
    }
}

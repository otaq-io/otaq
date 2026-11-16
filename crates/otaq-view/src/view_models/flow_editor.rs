//! The flow editor's parts (T-0471; FED-01, FED-03…FED-07; d0115, d0108, d0087, d0086) that Edit flow on the board
//! ([`super::flow_board`]) draws a project's draft with: a stage's model (read-only, [`ModelLine`]), skill and Fails
//! go to fields ([`Details`]),
//! the stage's problems and their fixes, the change line and the footer, and every edit the user makes to the draft.
//!
//! Every user edit is a [`StageEdit`] the desktop applies to the draft's document with [`edited`] and sends whole as
//! `flow.draft_update`; Start from, Undo and Undo all are the draft's own edits.
//!
//! **Captions** (d0115 "No description field"): Otaq's shipped skills are written for agents, so their captions are
//! Otaq's own words ([`shipped_caption`], Q-B).
//!
//! **Skills** (T-0472; FED-05, FED-06): a stage added from the palette of ready stages ([`StageEdit::AddTemplate`])
//! comes with its model and skill, or none for a Blank stage. An agent stage with no skill offers **Pick a skill…**
//! (the skills the user already has and Otaq's own, from `skills.list`, searchable, or a name typed by hand) and **Ask
//! the agent to write it** (Noyan's agent writes it into the project's checkout, which Otaq doesn't commit); while it
//! writes the field says so with Stop, then shows the skill tinted, where it was saved, with Open and Undo
//! ([`SkillState`]). A stage whose skill nobody has shows "You don't have the skill `<name>`" as a warning (the
//! draft's), with "Pick one, or Ask the agent to write it." under it ([`SkillFix`]); once the agent has written it,
//! the warning says "`<name>` is written but not committed yet: commit it so agents get it". It never keeps Save
//! off. Pick a skill… always ends with Ask the agent to write it…, so a stage that already names a skill can have one
//! written too, and shows it being written. A stage whose model's tool lacks it shows "GitHub Copilot doesn’t have the
//! skill `/<name>`", only a warning too. A stage with no skill saves: Otaq's own skill for that stage runs
//! (`work-ticket`, `validate`), and the field says so. **Ask
//! the agent to write it** sends nothing (Q441): it puts "Write a skill for the Plan stage: " in the agent's box
//! ([`write_skill_prefill`]) for the user to finish and send.
//!
//! **Who does a stage** (UX-65): its model, effort and instructions come with the flow; until Edit flow can change
//! them, a stage's details show them read-only ([`ModelLine`]).
//!
//! **Checks** (T-0529; d0131): a check's details have **Fails go to** ([`FailToField`]): the Agent stages before it,
//! the nearest marked and picked by default (the flow file leaves `fail_to` out for it). A `fail_to` that no longer
//! fits, and a stage with no model, are the stage's problems: they show in the stage's details and keep Save off until
//! fixed.

use otaq_core::agent_tools::Tool;
use otaq_core::agents::{AgentModel, NoyanStartsOn};
use otaq_core::flow::check::{VALIDATION_ROUNDS, fail_to_message};
use otaq_core::flow::draft::{DraftApplies, Inside, Mark, SkillWriting, StageMark, WrittenSkill};
use otaq_core::flow::model::{CheckBy, Flow, FlowFile, Stage, StageKind, StageOption, WhoDoesIt};
use otaq_core::flow::proposal::{
    About, ApplyChoices, Change, NEAREST, Proposal, ProposalProblem, missing_skill_text,
    skipped_skill_text, unknown_skill_text,
};
use otaq_core::flow::write_skill::saved_text;
use otaq_core::project_agents::ProjectAgent;
use otaq_core::skills::found::{FoundSkill, SkillScope};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::menu::{MenuItem, MenuSearch};
use crate::view_models::tool_avatar::ToolAvatarView;
use crate::view_models::tool_problems::Tools;

/// The editor's three kinds a step can be switched between (Delivery stays the last step's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FlowEditorKind")]
pub enum EditorKind {
    /// "An agent works".
    Works,
    /// "An agent checks".
    AgentChecks,
    /// "You check".
    YouCheck,
}

/// One edit of the user's in the flow part (`flow_editor.edit`'s `edit`). `FlowEditorStageEdit` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "edit", rename_all = "snake_case")]
#[ts(rename = "FlowEditorStageEdit")]
pub enum StageEdit {
    Rename {
        stage: String,
        name: String,
    },
    Kind {
        stage: String,
        kind: EditorKind,
    },
    /// A ticket can …: `option` is `skip` (the only option; any other is refused).
    Option {
        stage: String,
        option: String,
        on: bool,
    },
    /// Drag to reorder: the stage at index `to` among the stages (never after the Delivery stage).
    Move {
        stage: String,
        to: usize,
    },
    /// Remove stage….
    Remove {
        stage: String,
    },
    /// Pick a skill…: the stage runs the skill `skill` by its name (picked, or typed by hand); nothing is copied.
    Skill {
        stage: String,
        skill: String,
    },
    /// An agent check's Rounds (1–5, d0063).
    Rounds {
        stage: String,
        rounds: u32,
    },
    /// A check's Fails go to (d0131): an Agent stage before it by id; `None` for the nearest one.
    FailTo {
        stage: String,
        #[serde(default)]
        #[ts(optional)]
        to: Option<String>,
    },
    /// What it's for: the stage's description, which every agent reads (empty: none).
    Description {
        stage: String,
        description: String,
    },
    /// How work leaves the flow: Deliver as or When, with the same rules as the project's Repository card
    /// ([`crate::view_models::project_settings::deliver_as`]).
    Delivery {
        field: String,
        value: String,
    },
    /// **+** between two columns: one of the shipped ready stages ([`otaq_core::flow::templates`]) after `after`
    /// (`None`: first, right after Backlog), with its description, model and skill.
    AddTemplate {
        #[serde(default)]
        #[ts(optional)]
        after: Option<String>,
        template: String,
    },
}

/// What picking a Start from entry copies into the draft. `FlowEditorPick` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "pick", rename_all = "snake_case")]
#[ts(rename = "FlowEditorPick")]
pub enum Pick {
    /// The flow in use (`flow_in_use`).
    FlowInUse,
    /// A shipped preset (`start_from` a preset file).
    Preset { file: String },
    /// A tool's finished proposal under Made for <project> (`entry`).
    Entry { proposal: u32 },
    /// Another project's flow (Import a flow… → from a project).
    Project { project: String },
}

/// Apply's answers (`flow.apply`'s `choices`): none are asked any more (a stage's model is picked in the stage).
pub fn apply_choices() -> ApplyChoices {
    ApplyChoices::default()
}

/// The model a stage switched to an agent's kind starts on when it has none: `default`, the user's default model's
/// tool once the flow is saved (UX-68).
pub const NEW_STAGE_MODEL: &str = "default";
/// The skill a stage switched to each kind takes with the kind's agent (its own); a new stage starts with none
/// (FED-05).
pub const WORKS_SKILL: &str = otaq_core::agent_tools::WORK_TICKET_SKILL;
pub const CHECKS_SKILL: &str = otaq_core::own_skill!("validate");
/// An agent check's fails in a row before the card stops (the presets' 3).
pub const CHECK_ROUNDS: u32 = 3;
pub const NEW_STAGE: &str = "New stage";

/// The fixed start of every flow, as its step reads.
pub const BACKLOG: &str = "Backlog";
/// The id the delivery's step row carries, last: a setting of the flow, not a stage.
pub const DELIVERY_ID: &str = otaq_core::agents::facts::DELIVERY_STEP_ID;

/// The step's fixed captions (`s27-03`, `s27-05`).
pub const READY_CAPTION: &str =
    "Every flow starts here. Start a ticket and it goes into the first stage.";
pub const PR_CAPTION: &str = "A pull request opens on GitHub; your team reviews it there.";

/// Shipped skills' captions in Otaq's words (Q-B): their SKILL.md descriptions are written for the agent. Only Otaq's
/// own, named in their namespace (`otaq:work-ticket`, `tuq:product-spec`); a bare name is the user's skill.
pub fn shipped_caption(skill: &str, back: Option<&str>) -> Option<String> {
    Some(match otaq_core::skills::otaqs_or_tuqs(skill)? {
        "work-ticket" => {
            "An agent writes the code in the ticket\u{2019}s own copy of the repo (a worktree)."
                .to_owned()
        }
        "product-spec" => {
            "An agent asks you what to build, for whom and why, then writes down what you agreed."
                .to_owned()
        }
        "tech-spec" => {
            "An agent works out how to build it, asking you where a choice matters, and writes it down."
                .to_owned()
        }
        "milestone-plan" => {
            "An agent writes the ticket\u{2019}s plan: the pieces, their order and what each waits for."
                .to_owned()
        }
        "validate" => format!(
            "A checking agent runs the tests and lint. If something fails, the ticket goes back to {}.",
            back.unwrap_or("the stage before")
        ),
        "open-pr" => PR_CAPTION.to_owned(),
        // tuq's step-back: with no tool's tuq installed, its row has no description of its own (F-D-2).
        "step-back" => {
            "An agent steps back from a feature\u{2019}s decisions and names the ones adding the most complication."
                .to_owned()
        }
        // Otaq's workflow skills, in a sentence instead of their instructions to the agent (F-D-1).
        "address-review" => {
            "An agent fixes what you sent back from review, then hands the ticket back to you.".to_owned()
        }
        "answer-question" => {
            "An agent answers a linked ticket\u{2019}s question from this finished ticket\u{2019}s work, changing nothing."
                .to_owned()
        }
        "check-ticket" => format!(
            "{} starts it before a check stage\u{2019}s own skill: the checking agent changes nothing and gives its verdict.",
            otaq_core::naming::PRODUCT_NAME
        ),
        "commit" => "An agent commits every change in the ticket\u{2019}s own copy of the repo.".to_owned(),
        "flow" => "Noyan changes the project\u{2019}s flow: its agents and stages.".to_owned(),
        "noyan" => "Noyan, the project\u{2019}s coordinator, turns what you ask for into tickets.".to_owned(),
        "push" => {
            "An agent pushes the ticket\u{2019}s new commits to its pull request once their messages are confirmed."
                .to_owned()
        }
        "restack" => {
            "An agent brings the ticket\u{2019}s branch up to date with the branch it merges into.".to_owned()
        }
        _ => return None,
    })
}

/// A step's mark from the Project agent: "Added", "New", "Changed", "Moved", with its why line and Undo.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorMark")]
pub struct MarkView {
    pub label: &'static str,
    /// "Changed by Checkout slowness: From 41 /security-review runs in shop-web" (hover).
    pub why: String,
    pub undo: &'static str,
}

/// Who does a stage, read-only (UX-65; Edit flow's Who does it picks it later): its tool's avatar, its model and
/// effort, its instructions, and a warning when its tool doesn't have the stage's skill. `FlowEditorModelLine` on the
/// page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorModelLine")]
pub struct ModelLine {
    pub label: &'static str,
    pub avatar: Option<ToolAvatarView>,
    /// "Claude Code · opus · effort high", "default", "—" for a stage with no model yet.
    pub value: String,
    /// The stage's own instructions (UX-67), as the flow file says them; `None` with none.
    pub instructions: Option<String>,
    /// "Copilot doesn’t have the skill `/team-validate`", amber: the model's tool doesn't have the stage's skill. Only
    /// a warning, Apply still works.
    pub warning: Option<String>,
    pub hint: &'static str,
}

/// A check's Fails go to (d0131): the Agent stages before it under a header, "nearest" on the nearest, the stage its
/// fails go to checked. `FlowEditorFailTo` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorFailTo")]
pub struct FailToField {
    pub label: &'static str,
    /// The stage's name.
    pub value: String,
    /// The header, then one item per stage, its id the stage's.
    pub items: Vec<MenuItem>,
    /// The nearest stage's id: what picking it sends (as `to` left out).
    pub nearest: String,
    pub explanation: &'static str,
    /// "Spec review's fails go to Validate, which isn't an Agent stage before it": its stage moved or changed kind.
    pub problem: Option<String>,
}

/// What the Skill field shows (FED-06). `FlowEditorSkillState` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(rename = "FlowEditorSkillState")]
pub enum SkillState {
    /// The stage's skill (clicking it picks another); `warning` "You don't have the skill `<name>`" when nobody has it
    /// (amber, Apply still works), or why a tool that finds it doesn't load it (a byte order mark), with `fix` under it:
    /// "Pick one, or Ask the agent to write it."
    Set {
        skill: String,
        warning: Option<String>,
        fix: Option<SkillFix>,
    },
    /// "No skill of yours: Plan runs Otaq’s `work-ticket`.", with Pick a skill… and Ask the agent to write it, which
    /// puts `prefill` in the agent's box ([`write_skill_prefill`], Q441).
    Empty {
        text: String,
        pick: &'static str,
        ask: &'static str,
        hint: &'static str,
        prefill: String,
    },
    /// "Being written by Noyan…" with Stop.
    Writing {
        text: &'static str,
        stop: &'static str,
    },
    /// The skill the agent wrote, tinted: "Written by the agent", where it was saved, Open (its folder), Undo.
    Written {
        skill: String,
        note: &'static str,
        /// "Saved in `.claude/skills/plan-ticket` in this project. Commit it so agents get it in their worktrees.";
        /// `None` for one an older Otaq wrote, which has no folder in the project.
        saved: Option<String>,
        /// "Written by Checkout slowness" (hover).
        why: String,
        /// Opens its folder; `None` for one an older Otaq wrote, which has no folder in the project.
        open: Option<&'static str>,
        undo: &'static str,
    },
}

/// Pick a skill…: the skills the user already has (personal ones, then the project's), unmarked, then Otaq's own
/// under an "Otaq" header with Otaq's mark, each with an "Otaq" badge (decision 108), searchable. A user's skill's
/// second line says where it lives, which tools have it and what it does ("Personal · Claude Code · Lists the
/// steps…"), an Otaq row's what it does (every tool has Otaq's own); an item's id is the skill's name. Never empty:
/// Otaq's own are always there. `FlowEditorSkillPicker` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorSkillPicker")]
pub struct SkillPicker {
    pub title: &'static str,
    pub search: MenuSearch,
    pub items: Vec<MenuItem>,
    /// The last item while searching, for a name typed by hand: "Use `{name}`", `{name}` the typed text (its id).
    pub use_typed: &'static str,
    /// Always the very last row, searching or not: "Ask the agent to write it…" (id [`ASK_TO_WRITE`]), which puts
    /// `prefill` in the agent's box ([`write_skill_prefill`]); nothing is sent before the user sends.
    pub ask: MenuItem,
    pub prefill: String,
}

/// The line under "You don't have the skill `<name>`": "**Pick one**, or **Ask the agent to write it**." Pick one
/// opens Pick a skill…; Ask the agent to write it puts the picker's `prefill` in the agent's box, as the empty
/// field's button does. `FlowEditorSkillFix` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorSkillFix")]
pub struct SkillFix {
    pub pick: &'static str,
    /// Between the two: ", or ".
    pub or: &'static str,
    pub ask: &'static str,
    /// After the second: ".".
    pub end: &'static str,
}

/// The fix line under a missing skill's warning.
pub const SKILL_FIX: SkillFix = SkillFix {
    pick: "Pick one",
    or: ", or ",
    ask: "Ask the agent to write it",
    end: ".",
};

/// Pick a skill…'s "Ask the agent to write it…" row's id, which no skill can have (a skill's name is letters, digits
/// and dashes).
pub const ASK_TO_WRITE: &str = ":ask-the-agent";

/// The Skill field (FED-06).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorSkillField")]
pub struct SkillField {
    pub label: &'static str,
    pub state: SkillState,
    pub picker: SkillPicker,
}

/// The parts of a stage's details (FED-04) the board's editor takes over: who does it, its skill and fails go to
/// fields, its problem and its mark.
#[derive(Debug, Clone, PartialEq)]
pub struct Details {
    /// Who does the stage (an Agent stage's, an agent check's).
    pub model: Option<ModelLine>,
    pub skill: Option<SkillField>,
    pub fail_to: Option<FailToField>,
    pub problem: Option<String>,
    pub mark: Option<MarkView>,
}

/// The editor's buttons. `FlowEditorFooter` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorFooter")]
pub struct Footer {
    /// "Cancel": drops the draft.
    pub cancel: &'static str,
    /// "No changes", "Apply to shop-web".
    pub apply: String,
    pub apply_enabled: bool,
    /// Why Apply is off: the first problem.
    pub reason: Option<String>,
}

/// What the board's editor reads of a project's draft ([`super::flow_board`]).
pub struct Facts<'a> {
    pub project: &'a str,
    pub project_name: &'a str,
    /// The draft (`flow.draft`).
    pub proposal: &'a Proposal,
    pub marks: &'a [StageMark],
    pub swap: bool,
    pub applies: &'a DraftApplies,
    /// The draft's document, read.
    pub file: &'a FlowFile,
    /// The flow in use.
    pub in_use: &'a FlowFile,
    pub tools: &'a Tools,
    /// The skills the user already has and Otaq's own (`skills.list` for the project): Pick a skill… and captions.
    pub skills: &'a [FoundSkill],
    /// The stages whose skill the Project agent is writing, and the skills it wrote (the draft's).
    pub writing: &'a [SkillWriting],
    pub written: &'a [WrittenSkill],
    pub selected: Option<&'a str>,
    /// The agent drafting a suggestion, once one was asked.
    pub agent: Option<&'a ProjectAgent>,
    /// "Noyan starts on".
    pub new_agent: Option<&'a NoyanStartsOn>,
}

pub(crate) fn kind_icon(stage: &Stage) -> IconName {
    match stage.kind {
        StageKind::Agent => IconName::Bot,
        StageKind::Check { by: CheckBy::Agent } => IconName::ShieldCheck,
        StageKind::Check { by: CheckBy::You } => IconName::UserCheck,
    }
}

pub(crate) fn editor_kind(stage: &Stage) -> Option<EditorKind> {
    match stage.kind {
        StageKind::Agent => Some(EditorKind::Works),
        StageKind::Check { by: CheckBy::Agent } => Some(EditorKind::AgentChecks),
        StageKind::Check { by: CheckBy::You } => Some(EditorKind::YouCheck),
    }
}

/// The tool and model `who` runs on: its own, or the default model's for `default`.
pub(crate) fn who_on(who: &WhoDoesIt, f: &Facts) -> Option<otaq_core::agents::ModelRef> {
    AgentModel::parse(who.model.as_deref()?)
        .ok()?
        .on(f.tools.default_model.as_ref())
}

/// The avatar and model of `stage`'s model: its tool and model; `None` for a stage with no model yet.
pub(crate) fn stage_on(stage: &Stage, f: &Facts) -> (Option<ToolAvatarView>, Option<String>) {
    match who_on(&stage.who, f).filter(|_| stage.runs_agent()) {
        Some(on) => (
            Some(ToolAvatarView::of(on.tool, on.model.as_deref())),
            on.model,
        ),
        None => (None, None),
    }
}

/// Who works or checks `stage`, as a step names it: its tool ("Claude Code"); `None` for a check by you or a stage
/// with no model yet.
pub(crate) fn stage_who(stage: &Stage) -> Option<String> {
    crate::view_models::agents::stage_tool(stage, None)
        .map(|t| crate::view_models::tool_problems::tool_word(t).to_owned())
        .or_else(|| stage.model().map(str::to_owned))
}

fn mark_label(mark: Mark) -> &'static str {
    match mark {
        Mark::Added => "Added",
        Mark::New => "New",
        Mark::Changed => "Changed",
        Mark::Moved => "Moved",
        Mark::Removed => "Removed",
    }
}

pub(crate) fn mark_view(mark: &StageMark) -> MarkView {
    let why = match &mark.why {
        Some(why) => format!("Changed by {}: {why}", mark.by),
        None => format!("Changed by {}", mark.by),
    };
    MarkView {
        label: mark_label(mark.mark),
        why,
        undo: "Undo",
    }
}

pub(crate) fn mark_of<'m>(marks: &'m [StageMark], id: &str) -> Option<&'m StageMark> {
    marks
        .iter()
        .find(|m| m.stage_id == id && m.mark != Mark::Removed)
}

/// A problem shows on `stage`: it is about the stage.
fn shows_on(problem: &ProposalProblem, stage: &Stage) -> bool {
    match &problem.about {
        About::Stage { id, .. } => *id == stage.id,
        About::Flow => false,
    }
}

/// What a problem says on its step and as Apply's reason.
pub(crate) fn problem_text(_f: &Facts, problem: &ProposalProblem) -> String {
    problem.message.clone()
}

/// The first problem Apply still waits for.
pub(crate) fn left<'a>(f: &Facts<'a>) -> Option<&'a ProposalProblem> {
    f.proposal.problems.first()
}

/// A step's problem, in the error colour. A stage with no skill has none: Otaq's own skill for it runs.
pub(crate) fn step_problem(f: &Facts, stage: &Stage) -> Option<String> {
    f.proposal
        .problems
        .iter()
        .find(|p| shows_on(p, stage))
        .map(|p| problem_text(f, p))
}

/// The tool `stage` runs on: its model's own, or the default model's for `default`. `None` when it isn't known.
fn agent_tool(f: &Facts, stage: &Stage) -> Option<Tool> {
    who_on(&stage.who, f).map(|on| on.tool)
}

/// The tools that have the stage's skill for the project (S5, S6), from `skills.list`: `None` when every tool may
/// run it — a stage with no skill or with one of Otaq's own (Otaq's own skills are in every tool), or a skill
/// nobody has, which the step already warns about ("You don't have the skill").
fn skill_holders<'f>(f: &'f Facts, stage: &Stage) -> Option<&'f [Tool]> {
    let skill = stage.skill.as_deref()?.trim();
    if skill.is_empty() || otaq_core::skills::found::is_shipped(skill) {
        return None;
    }
    f.skills
        .iter()
        .find(|s| !s.otaq && s.name == skill)
        .map(|s| s.tools.as_slice())
        .filter(|tools| !tools.is_empty())
}

/// "Copilot doesn’t have the skill `/team-validate`" on a stage whose model's tool doesn't have its skill: only a
/// warning, the flow still applies.
pub(crate) fn missing_skill(f: &Facts, stage: &Stage) -> Option<String> {
    let tools = skill_holders(f, stage)?;
    let tool = agent_tool(f, stage)?;
    let skill = stage.skill.as_deref()?.trim();
    // A tool that finds the skill but skips it says why on the Skill field ([`skipped_skill`]) instead.
    if found_skill(f, skill).is_some_and(|s| s.skipped.contains(&tool)) {
        return None;
    }
    (!tools.contains(&tool)).then(|| missing_skill_text(tool, skill))
}

/// The user's own skill `skill` from `skills.list`, if they have it.
fn found_skill<'f>(f: &'f Facts, skill: &str) -> Option<&'f FoundSkill> {
    f.skills.iter().find(|s| !s.otaq && s.name == skill)
}

/// "Codex doesn’t load the skill `$retest-docs`: its SKILL.md starts with a byte order mark" on a stage whose
/// skill a tool finds but doesn't load: when no tool loads it, or when the stage's agent's tool is one that skips it.
/// Only a warning, the flow still applies; a ticket reaching the stage is held.
pub(crate) fn skipped_skill(f: &Facts, stage: &Stage) -> Option<String> {
    let skill = stage.skill.as_deref()?.trim();
    if skill.is_empty() || otaq_core::skills::found::is_shipped(skill) {
        return None;
    }
    let found = found_skill(f, skill)?;
    if found.skipped.is_empty() {
        return None;
    }
    if found.tools.is_empty() {
        return Some(skipped_skill_text(&found.skipped, skill));
    }
    let tool = agent_tool(f, stage)?;
    found
        .skipped
        .contains(&tool)
        .then(|| skipped_skill_text(&[tool], skill))
}

/// The warning on a stage's card for its skill: one nobody has (or the agent wrote and nobody has committed yet), one
/// a tool skips, or one its agent's tool lacks.
pub(crate) fn stage_skill_warning(f: &Facts, stage: &Stage) -> Option<String> {
    unknown_skill(f, stage)
        .or_else(|| skipped_skill(f, stage))
        .or_else(|| missing_skill(f, stage))
}

/// Who does `stage`, read-only ([`ModelLine`]); `None` for a check by you.
pub(crate) fn model_line(f: &Facts, stage: &Stage) -> Option<ModelLine> {
    if !stage.runs_agent() {
        return None;
    }
    let (avatar, _) = stage_on(stage, f);
    let lists = crate::view_models::agents::AgentTools {
        connected: f.tools.connected.clone(),
        lists: f.tools.lists.clone(),
        failed: Vec::new(),
    };
    Some(ModelLine {
        label: "Model",
        avatar,
        value: crate::view_models::agents::model_text_line(
            stage.who.model.as_deref(),
            stage.who.effort.as_deref(),
            &lists,
            f.tools.default_model.as_ref(),
        ),
        instructions: stage.who.instructions.clone(),
        warning: missing_skill(f, stage),
        hint: "Set in the flow file: the stage\u{2019}s `model:`, `effort:` and `instructions:`.",
    })
}

/// Where a check's fails go, as the step and its field show it (d0131): the stage its `fail_to` names (even one that
/// doesn't fit: its problem says why), else the nearest Agent stage before it. `None` for a stage that isn't a check.
pub(crate) fn fails_to<'a>(flow: &'a Flow, stage: &Stage) -> Option<&'a Stage> {
    if !matches!(stage.kind, StageKind::Check { .. }) {
        return None;
    }
    stage
        .fail_to
        .as_deref()
        .and_then(|id| flow.stage(id))
        .or_else(|| flow.fail_to_of(stage))
}

/// "A fail resumes that stage’s session…" (d0131, `s29-04`).
pub const FAIL_TO_EXPLANATION: &str = "A fail resumes that stage\u{2019}s session with the report or your comments; \
     the card then walks every stage after it again. To send one card somewhere else, drag it back.";

/// A check's Fails go to: the Agent stages before it (d0131); `None` with none before it (the flow check says so).
pub(crate) fn fail_to_field(f: &Facts, stage: &Stage) -> Option<FailToField> {
    let flow = &f.file.flow;
    let at = flow.position(&stage.id)?;
    if !matches!(stage.kind, StageKind::Check { .. }) {
        return None;
    }
    let before: Vec<&Stage> = flow.stages[..at]
        .iter()
        .filter(|s| s.is_agent_stage())
        .collect();
    let nearest = before.last()?;
    let target = fails_to(flow, stage)?;
    let mut items = vec![MenuItem::header(
        "STAGES WHERE AN AGENT WORKS, BEFORE THIS ONE",
    )];
    items.extend(before.iter().map(|s| {
        let item = MenuItem::new(s.id.clone(), s.name.clone()).checked(s.id == target.id);
        match s.id == nearest.id {
            true => item.hint(NEAREST),
            false => item,
        }
    }));
    let fits = before.iter().any(|s| s.id == target.id);
    Some(FailToField {
        label: "Fails go to",
        value: target.name.clone(),
        items,
        nearest: nearest.id.clone(),
        explanation: FAIL_TO_EXPLANATION,
        problem: (!fits).then(|| fail_to_message(stage, target)),
    })
}

/// Whether the Project agent is writing the skill of the stage `id`.
pub(crate) fn writing(f: &Facts, id: &str) -> bool {
    f.writing.iter().any(|w| w.stage_id == id)
}

/// The skill the Project agent wrote for `stage`, while the stage still runs it.
pub(crate) fn written_of<'f>(f: &'f Facts, stage: &Stage) -> Option<&'f WrittenSkill> {
    f.written
        .iter()
        .find(|w| w.stage_id == stage.id && stage.skill.as_deref() == Some(w.skill.as_str()))
}

/// What Ask the agent to write it and the Write the skill chip put in the agent's box (FED-06, Q441): "Write a skill
/// for the Plan stage: ", the caret after it. The user adds what the stage should do and sends; nothing is sent
/// before.
pub fn write_skill_prefill(file: &FlowFile, stage: &str) -> String {
    let name = file.flow.stage(stage).map_or(stage, |s| s.name.as_str());
    format!("Write a skill for the {name} stage: ")
}

/// Where one of the user's skills in Pick a skill… lives, its second line's start: "Personal", "This project",
/// "Plugin" (Otaq's own say whose they are with their badge instead).
fn skill_place(skill: &FoundSkill) -> String {
    match skill.places.first().map(|p| p.scope) {
        Some(SkillScope::Project) => "This project".to_owned(),
        Some(SkillScope::Plugin) => "Plugin".to_owned(),
        _ => "Personal".to_owned(),
    }
}

/// The tools that have one of the user's skills, for Pick a skill…'s second line (S6): "Claude Code, Codex"; a
/// project skill no tool reads until it is committed says so, and one the only tools that find it skip says why.
fn skill_tools(skill: &FoundSkill) -> String {
    if skill.tools.is_empty() {
        if !skill.skipped.is_empty() {
            return format!(
                "{} doesn\u{2019}t load it: its SKILL.md starts with a byte order mark",
                otaq_core::flow::proposal::tool_names(&skill.skipped)
            );
        }
        return "Not committed yet: no tool has it".to_owned();
    }
    otaq_core::flow::proposal::tool_names(&skill.tools)
}

/// Pick a skill…'s items (decision 108): the user's own skills first, unmarked (personal ones, then the project's,
/// each by name), then a group headed "Otaq" with Otaq's mark listing Otaq's own: its `otaq:` workflow skills and
/// tuq's `tuq:` skills (decision 104: tuq counts as Otaq's, whoever installed it), each by its namespaced name with an
/// "Otaq" badge; the stage's skill checked. The page's search keeps the header over the Otaq rows it shows, and the
/// badge on each. A user's skill's second line is where it lives, the tools that have it, then its description; an
/// Otaq row's says what it does (Otaq's own words for its shipped skills): every tool has it.
fn skill_choices(f: &Facts, stage: &Stage) -> Vec<MenuItem> {
    let checked = |skill: &FoundSkill| stage.skill.as_deref() == Some(skill.name.as_str());
    // Otaq's `check-ticket` is a checker's workflow skill, never a stage's own (decision 113): Otaq starts it before a
    // check stage's task skill. Listed only on a stage that already names it.
    let mut skills: Vec<&FoundSkill> = f
        .skills
        .iter()
        .filter(|s| s.name != otaq_core::agent_tools::CHECK_TICKET_SKILL || checked(s))
        .collect();
    let order = |s: &FoundSkill| match (s.otaq, s.places.first().map(|p| p.scope)) {
        (true, _) => 2,
        (false, Some(SkillScope::Project)) => 1,
        (false, _) => 0,
    };
    skills.sort_by_key(|s| (order(s), s.name.to_lowercase()));
    let (otaqs, users): (Vec<&FoundSkill>, Vec<&FoundSkill>) =
        skills.into_iter().partition(|s| s.otaq);
    let mut items: Vec<MenuItem> = users
        .into_iter()
        .map(|skill| {
            let place = format!("{} \u{b7} {}", skill_place(skill), skill_tools(skill));
            let hint = match &skill.description {
                Some(d) => format!("{place} \u{b7} {d}"),
                None => place,
            };
            MenuItem::new(skill.name.as_str(), skill.name.as_str())
                .hint(hint)
                .checked(checked(skill))
        })
        .collect();
    if !otaqs.is_empty() {
        let mut header = MenuItem::header(otaq_core::naming::PRODUCT_NAME).mark();
        header.separated = !items.is_empty();
        items.push(header);
    }
    items.extend(otaqs.into_iter().map(|skill| {
        let item = MenuItem::new(skill.name.as_str(), skill.name.as_str())
            .badge(otaq_core::naming::PRODUCT_NAME)
            .checked(checked(skill));
        match shipped_caption(&skill.name, None).or_else(|| skill.description.clone()) {
            Some(d) => item.hint(d),
            None => item,
        }
    }));
    items
}

/// "You don't have the skill `<name>`" on `stage`: the draft's warning for its skill, which neither Otaq ships nor
/// the user has. Only a warning: Apply still works. A skill the agent wrote for the stage is in the project's checkout,
/// but no tool reads it until the user commits it, which Apply doesn't do: the warning says so instead
/// ([`uncommitted_skill_text`]).
pub(crate) fn unknown_skill(f: &Facts, stage: &Stage) -> Option<String> {
    let skill = stage.skill.as_deref()?.trim();
    // Otaq's own skills never warn.
    if otaq_core::skills::found::is_shipped(skill) {
        return None;
    }
    let text = unknown_skill_text(skill);
    let warned = f
        .proposal
        .diff
        .warnings
        .iter()
        .any(|w| w.stage.as_deref() == Some(stage.name.as_str()) && w.text == text);
    match (warned, written_of(f, stage)) {
        (false, _) => None,
        (true, Some(_)) => Some(uncommitted_skill_text(skill)),
        (true, None) => Some(text),
    }
}

/// The warning on a stage whose skill the agent wrote into the project's checkout, which Otaq doesn't commit:
/// "`update-docs` is written but not committed yet: commit it so agents get it".
pub(crate) fn uncommitted_skill_text(skill: &str) -> String {
    format!("`{skill}` is written but not committed yet: commit it so agents get it")
}

/// The Skill field of a stage an agent runs (FED-06): set, empty with Pick and Ask, being written, or written.
pub(crate) fn skill_field(f: &Facts, stage: &Stage) -> Option<SkillField> {
    if !stage.runs_agent() {
        return None;
    }
    let state = if let Some(written) = written_of(f, stage) {
        SkillState::Written {
            skill: written.skill.clone(),
            note: "Written by the agent",
            saved: (!written.folder.is_empty()).then(|| saved_text(&written.folder)),
            why: format!("Written by {}", written.by),
            open: (!written.folder.is_empty()).then_some("Open"),
            undo: "Undo",
        }
    } else if writing(f, &stage.id) {
        // A stage that already names a skill (one nobody has, say) shows it being written too: Ask the agent to write
        // it is offered there, and the written skill takes that name's place.
        SkillState::Writing {
            text: "Being written by Noyan\u{2026}",
            stop: "Stop",
        }
    } else if let Some(skill) = &stage.skill {
        let warning = unknown_skill(f, stage).or_else(|| skipped_skill(f, stage));
        SkillState::Set {
            skill: skill.clone(),
            fix: warning.is_some().then_some(SKILL_FIX),
            warning,
        }
    } else {
        SkillState::Empty {
            text: format!(
                "No skill of yours: {} runs {}\u{2019}s `{}`.",
                stage.name,
                otaq_core::naming::PRODUCT_NAME,
                stage.own_skill().unwrap_or_default()
            ),
            pick: "Pick a skill\u{2026}",
            ask: "Ask the agent to write it",
            hint: "Pick: your own skills, searchable. Ask: starts Noyan on the right and uses tokens.",
            prefill: write_skill_prefill(f.file, &stage.id),
        }
    };
    Some(SkillField {
        label: "Skill",
        state,
        picker: SkillPicker {
            title: "Pick a skill\u{2026}",
            search: MenuSearch {
                placeholder: "Search your skills\u{2026}".to_owned(),
                empty: "No matching skills".to_owned(),
            },
            items: skill_choices(f, stage),
            use_typed: "Use `{name}`",
            ask: MenuItem::new(ASK_TO_WRITE, "Ask the agent to write it\u{2026}")
                .icon(IconName::Sparkles)
                .hint("Starts Noyan on the right and uses tokens.")
                .separated(),
            prefill: write_skill_prefill(f.file, &stage.id),
        },
    })
}

pub(crate) fn details(f: &Facts, stage: &Stage) -> Details {
    let fail_to = fail_to_field(f, stage);
    Details {
        model: model_line(f, stage),
        skill: skill_field(f, stage),
        problem: f
            .proposal
            .problems
            .iter()
            .filter(|p| shows_on(p, stage))
            // Fails go to shows its own.
            .find(|p| fail_to.as_ref().and_then(|t| t.problem.as_ref()) != Some(&p.message))
            .map(|p| p.message.clone()),
        fail_to,
        mark: mark_of(f.marks, &stage.id).map(mark_view),
    }
}

/// The stages the draft changes (each once).
pub(crate) fn changed_stages(proposal: &Proposal) -> usize {
    let mut ids: Vec<&str> = proposal
        .diff
        .stages
        .iter()
        .map(|c| c.stage_id.as_str())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids.len().max(usize::from(!proposal.diff.same))
}

/// "Security moved before Validate.", "Docs removed.": what a reorder or removal does, for the change line.
fn shape_words(f: &Facts) -> String {
    let stages = &f.file.flow.stages;
    let mut words: Vec<String> = Vec::new();
    for change in &f.proposal.diff.stages {
        match &change.change {
            Change::Moved { .. } => {
                let at = f.file.flow.position(&change.stage_id);
                let next = at.and_then(|i| stages.get(i + 1));
                let prev = at
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| stages.get(i));
                words.push(match (next, prev) {
                    (Some(next), _) => format!("{} moved before {}.", change.stage, next.name),
                    (None, Some(prev)) => format!("{} moved after {}.", change.stage, prev.name),
                    (None, None) => format!("{} moved.", change.stage),
                });
            }
            Change::Removed => words.push(format!("{} removed.", change.stage)),
            _ => {}
        }
    }
    words.dedup();
    words.join(" ")
}

/// "the 6 there now (4 in Dev, 2 in Security)": the tickets inside, by stage in the order they come, each stage by
/// the name the user sees (a removed stage's is in the flow in use, a new one's in the draft), never its id.
fn inside_words(f: &Facts, inside: &[Inside]) -> String {
    let mut by: Vec<(&str, usize)> = Vec::new();
    for i in inside {
        match by.iter_mut().find(|(s, _)| *s == i.stage) {
            Some((_, n)) => *n += 1,
            None => by.push((&i.stage, 1)),
        }
    }
    let name = |id: &str| -> String {
        f.in_use
            .flow
            .stage(id)
            .or_else(|| f.file.flow.stage(id))
            .map_or_else(|| id.to_owned(), |s| s.name.clone())
    };
    by.iter()
        .map(|(stage, n)| format!("{n} in {}", name(stage)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The change line (FED-08; `s27-07`): which part waits, how many tickets are in it, and that Apply now is there.
pub(crate) fn change_line(f: &Facts) -> Option<String> {
    Some(match f.applies {
        DraftApplies::Nothing => return None,
        DraftApplies::AtOnce => {
            "This applies at once: tickets that haven\u{2019}t reached a changed step follow it; \
                                  tickets in it or past it finish as they are."
                .to_owned()
        }
        DraftApplies::Span { span, inside } => {
            let what = shape_words(f);
            let what = if what.is_empty() {
                String::new()
            } else {
                format!("{what} ")
            };
            let (first, last) = (&span.first, &span.last);
            if inside.is_empty() {
                format!("{what}No ticket is between {first} and {last} now, so it applies at once.")
            } else {
                let there = match inside.len() {
                    1 => format!(
                        "the ticket there now ({}) finishes",
                        inside_words(f, inside)
                    ),
                    n => format!("the {n} there now ({}) finish", inside_words(f, inside)),
                };
                format!(
                    "{what}This waits until no ticket is between {first} and {last}: {there} on the current \
                     order, and until then no ticket enters {first} to {last}. Tickets before {first} keep moving. \
                     The banner offers Apply now."
                )
            }
        }
        DraftApplies::WholeBoard { inside } if inside.is_empty() => {
            "Another flow: no ticket is at work, so it applies at once.".to_owned()
        }
        DraftApplies::WholeBoard { inside } => format!(
            "Another flow: this waits for the whole board. {} at work now finish on the current flow, and no \
             ticket starts until then. The banner offers Apply now.",
            match inside.len() {
                1 => "The ticket".to_owned(),
                n => format!("The {n} tickets"),
            }
        ),
        DraftApplies::WithTheWaitingChange => {
            "A flow change already waits: this one applies together with it.".to_owned()
        }
    })
}

pub(crate) fn footer(f: &Facts) -> Footer {
    // "<problem> · being written" while the agent writes the stage's skill (`s27-06`); with no problem left, Save
    // still waits for a skill being written ("Plan's skill is being written"), so the written skill isn't lost.
    let reason = left(f)
        .map(|p| match &p.about {
            About::Stage { id, .. } if writing(f, id) => {
                format!("{} \u{b7} being written", p.message)
            }
            _ => problem_text(f, p),
        })
        .or_else(|| {
            f.file
                .flow
                .stages
                .iter()
                .find(|s| writing(f, &s.id))
                .map(|s| format!("{}\u{2019}s skill is being written", s.name))
        });
    if f.proposal.diff.same {
        Footer {
            cancel: "Cancel",
            apply: "No changes".to_owned(),
            apply_enabled: false,
            reason: None,
        }
    } else {
        Footer {
            cancel: "Cancel",
            apply: format!("Apply to {}", f.project_name),
            apply_enabled: reason.is_none(),
            reason,
        }
    }
}

/// A stage id from a name, unique in `file`: `Plan` → `plan`, `plan-2`.
fn fresh_id(file: &FlowFile, name: &str) -> String {
    let mut base = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            base.push(c);
        } else if !base.is_empty() && !base.ends_with('-') {
            base.push('-');
        }
    }
    let base = match base.trim_end_matches('-') {
        "" => "stage".to_owned(),
        b => b.to_owned(),
    };
    let mut id = base.clone();
    let mut n = 2;
    while file.flow.stage(&id).is_some() {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// A name no stage has: "New stage", "New stage 2".
fn fresh_name(file: &FlowFile) -> String {
    let taken = |name: &str| {
        file.flow
            .stages
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case(name))
    };
    let mut name = NEW_STAGE.to_owned();
    let mut n = 2;
    while taken(&name) {
        name = format!("{NEW_STAGE} {n}");
        n += 1;
    }
    name
}

/// `stage` as the kind `kind`, keeping who does it, and its skill unless it was the old kind's own.
fn with_kind(stage: &Stage, kind: EditorKind) -> Stage {
    let mut s = stage.clone();
    let was_default = |skill: &str| stage.skill.as_deref().is_none_or(|k| k == skill);
    let defaults = match editor_kind(stage) {
        Some(EditorKind::Works) => was_default(WORKS_SKILL),
        Some(EditorKind::AgentChecks) => was_default(CHECKS_SKILL),
        _ => true,
    };
    let skill = match kind {
        EditorKind::Works => WORKS_SKILL,
        EditorKind::AgentChecks => CHECKS_SKILL,
        EditorKind::YouCheck => "",
    };
    match kind {
        EditorKind::YouCheck => {
            s.kind = StageKind::Check { by: CheckBy::You };
            s.who = WhoDoesIt::default();
            s.skill = None;
            s.rounds = None;
            s.options.retain(|o| *o == StageOption::Skip);
        }
        _ => {
            s.kind = match kind {
                EditorKind::Works => StageKind::Agent,
                _ => StageKind::Check { by: CheckBy::Agent },
            };
            if kind == EditorKind::Works {
                // Only a check has a fails-go-to stage (d0131).
                s.fail_to = None;
            }
            // A stage with no skill yet keeps none: the user picks or has one written (FED-05, FED-06).
            let skilless = stage.runs_agent() && stage.skill.is_none();
            // Who does it stays; a stage that had no one (a check by you) starts on the default model.
            if s.who.model.is_none() {
                s.who = WhoDoesIt::on(NEW_STAGE_MODEL);
            }
            if defaults {
                s.skill = (!skilless).then(|| skill.to_owned());
            }
            s.rounds =
                (kind == EditorKind::AgentChecks).then(|| stage.rounds.unwrap_or(CHECK_ROUNDS));
        }
    }
    s
}

/// `name`, or "<name> 2", "<name> 3"… when another stage than the one at `but` has it.
fn unique_name(file: &FlowFile, name: &str, but: usize) -> String {
    let taken = |n: &str| {
        file.flow
            .stages
            .iter()
            .enumerate()
            .any(|(i, s)| i != but && s.name.eq_ignore_ascii_case(n))
    };
    let mut out = name.to_owned();
    let mut n = 2;
    while taken(&out) {
        out = format!("{name} {n}");
        n += 1;
    }
    out
}

/// A reorder that leaves a check with no stage where an agent works before it is refused, in words the user reads
/// on the toast: a check sends its fails back to such a stage, so it can't come first. Only a check that had one
/// before the move counts, so a flow already like that can still be put right by dragging.
fn refuse_stranded_check(before: &Flow, after: &Flow, moved: &str) -> Result<(), String> {
    let agent_before = |flow: &Flow, id: &str| {
        flow.position(id)
            .is_some_and(|at| flow.stages[..at].iter().any(Stage::is_agent_stage))
    };
    let stranded = after.stages.iter().find(|s| {
        matches!(s.kind, StageKind::Check { .. })
            && agent_before(before, &s.id)
            && !agent_before(after, &s.id)
    });
    let Some(check) = stranded else {
        return Ok(());
    };
    let why = "a check sends its fails back to a stage where an agent works, so one has to come before it";
    if check.id == moved {
        let first = after
            .first_agent_stage()
            .map_or("the stages where an agent works", |s| s.name.as_str());
        Err(format!(
            "{} can\u{2019}t go before {first}: {why}",
            check.name
        ))
    } else {
        let name = after.stage(moved).map_or(moved, |s| s.name.as_str());
        Err(format!(
            "{name} can\u{2019}t go after {}: {why}",
            check.name
        ))
    }
}

/// The draft's document after `edit`, and the stage to select after it (a new one). Refused with why when the edit
/// doesn't fit the flow (a stage that isn't there).
pub fn edited(file: &FlowFile, edit: &StageEdit) -> Result<(FlowFile, Option<String>), String> {
    let mut out = file.clone();
    let missing = |id: &str| format!("The flow has no stage {id}");
    let at = |id: &str| out.flow.position(id).ok_or_else(|| missing(id));
    let mut select = None;
    match edit {
        StageEdit::Rename { stage, name } => {
            let i = at(stage)?;
            let name = name.trim();
            if name.is_empty() {
                return Err("A stage needs a name".to_owned());
            }
            out.flow.stages[i].name = name.to_owned();
        }
        StageEdit::Kind { stage, kind } => {
            let i = at(stage)?;
            out.flow.stages[i] = with_kind(&out.flow.stages[i], *kind);
        }
        StageEdit::Option { stage, option, on } => {
            let i = at(stage)?;
            let s = &mut out.flow.stages[i];
            if option != StageOption::SKIP {
                return Err(format!(
                    "{}\u{2019}s only option is skip: a ticket runs it on the stage\u{2019}s own model",
                    s.name
                ));
            }
            let option = StageOption::Skip;
            s.options.retain(|o| *o != option);
            if *on {
                s.options.push(option);
            }
        }
        StageEdit::Move { stage, to } => {
            let i = at(stage)?;
            let before = out.flow.clone();
            let moved = out.flow.stages.remove(i);
            let end = out.flow.stages.len();
            out.flow.stages.insert((*to).min(end), moved);
            refuse_stranded_check(&before, &out.flow, stage)?;
        }
        StageEdit::Remove { stage } => {
            let i = at(stage)?;
            let gone = out.flow.stages.remove(i);
            // A check whose fails went there sends them to the nearest Agent stage again (d0131).
            for s in &mut out.flow.stages {
                if s.fail_to.as_deref() == Some(gone.id.as_str()) {
                    s.fail_to = None;
                }
            }
        }
        StageEdit::Skill { stage, skill } => {
            let i = at(stage)?;
            if !out.flow.stages[i].runs_agent() {
                return Err(format!("No agent runs {}", out.flow.stages[i].name));
            }
            let skill = skill.trim();
            if skill.is_empty() {
                return Err("Pick a skill".to_owned());
            }
            out.flow.stages[i].skill = Some(skill.to_owned());
        }
        StageEdit::Rounds { stage, rounds } => {
            let i = at(stage)?;
            let s = &mut out.flow.stages[i];
            if !s.is_agent_check() {
                return Err(format!("{} isn\u{2019}t an agent check", s.name));
            }
            if !VALIDATION_ROUNDS.contains(rounds) {
                return Err("A check has 1 to 5 rounds".to_owned());
            }
            s.rounds = Some(*rounds);
        }
        StageEdit::FailTo { stage, to } => {
            let i = at(stage)?;
            let check = &out.flow.stages[i];
            if !matches!(check.kind, StageKind::Check { .. }) {
                return Err(format!("{} isn\u{2019}t a check", check.name));
            }
            let nearest = out.flow.stages[..i]
                .iter()
                .rev()
                .find(|s| s.is_agent_stage())
                .map(|s| s.id.clone());
            let to = match to {
                Some(id) => {
                    let target = out.flow.stages[..i]
                        .iter()
                        .find(|s| s.id == *id && s.is_agent_stage());
                    if target.is_none() {
                        let name = out.flow.stage(id).map_or(id.as_str(), |s| s.name.as_str());
                        return Err(format!(
                            "{name} isn\u{2019}t a stage where an agent works before {}",
                            check.name
                        ));
                    }
                    // The nearest is the default: the file leaves it out (d0131).
                    Some(id.clone()).filter(|id| Some(id) != nearest.as_ref())
                }
                None => None,
            };
            out.flow.stages[i].fail_to = to;
        }
        StageEdit::Description { stage, description } => {
            let i = at(stage)?;
            let text = description.trim();
            out.flow.stages[i].description = (!text.is_empty()).then(|| text.to_owned());
        }
        StageEdit::Delivery { field, value } => {
            out.flow.delivery =
                crate::view_models::project_settings::deliver_as(&out.flow.delivery, field, value)?;
        }
        StageEdit::AddTemplate { after, template } => {
            let index = match after {
                Some(id) => at(id)? + 1,
                None => 0,
            };
            let ready = otaq_core::flow::templates::template(template)
                .ok_or_else(|| format!("No ready stage {template}"))?;
            let mut stage = ready.stage().clone();
            // Blank stage takes the next free "New stage" name; a ready stage keeps its own, numbered if taken.
            stage.name = if stage.skill.is_none() && stage.description.is_none() {
                fresh_name(&out)
            } else {
                unique_name(&out, &stage.name, usize::MAX)
            };
            stage.id = fresh_id(&out, &stage.name);
            select = Some(stage.id.clone());
            out.flow
                .stages
                .insert(index.min(out.flow.stages.len()), stage);
        }
    }
    Ok((out, select))
}

/// shop-web's flow and the draft around it, for the tests.
#[cfg(test)]
pub(crate) mod fixtures {
    use otaq_core::flow::draft::WrittenSkill;
    use otaq_core::flow::model::FlowFile;
    use otaq_core::flow::proposal::{
        About, FlowDiff, Proposal, ProposalProblem, ProposedBy, Source,
    };

    use otaq_core::agent_tools::Tool;
    use otaq_core::skills::found::{self, FoundSkill, Homes, SkillPlace, SkillScope};

    use super::{StageEdit, edited};

    /// Otaq's own skills only: what `skills.list` finds for a user with none of their own.
    pub fn otaq_skills() -> Vec<FoundSkill> {
        found::list(&Homes::default(), None)
    }

    /// One of the user's skills, found in `folder`.
    pub fn users_skill(
        name: &str,
        description: &str,
        scope: SkillScope,
        folder: &str,
        tools: &[Tool],
    ) -> FoundSkill {
        FoundSkill {
            name: name.into(),
            description: Some(description.into()),
            otaq: false,
            places: vec![SkillPlace {
                scope,
                folder: folder.into(),
                path: format!("{folder}/{name}").into(),
                real: None,
                tools: tools.to_vec(),
            }],
            tools: tools.to_vec(),
            skipped: Vec::new(),
        }
    }

    /// What `skills.list` finds for shop-web: the user's own `security-review`, the project's `release-notes`, and
    /// Otaq's own.
    pub fn found_skills() -> Vec<FoundSkill> {
        let mut skills = otaq_skills();
        skills.push(users_skill(
            "security-review",
            "A security agent reviews the change for risky code.",
            SkillScope::Personal,
            "/Users/me/.claude/skills",
            // Claude Code, the first tool.
            &Tool::ALL[..1],
        ));
        skills.push(users_skill(
            "release-notes",
            "Writes the release notes from the merged tickets.",
            SkillScope::Project,
            "/code/shop-web/.claude/skills",
            &found::project_tools(&[".claude", "skills"]),
        ));
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    /// The draft's proposal of `new` against `base`: its changes, and the flow check's problems on their stages.
    pub fn checked_proposal(base: &FlowFile, new: &FlowFile) -> Proposal {
        let mut draft = proposal(new, false);
        draft.diff.stages = otaq_core::flow::proposal::stage_changes(&base.flow, &new.flow, false);
        if let Err(found) = otaq_core::flow::check(&new.flow) {
            draft.problems = found
                .into_iter()
                .map(|p| ProposalProblem {
                    about: p.stage.as_deref().map_or(About::Flow, |id| About::Stage {
                        id: id.to_owned(),
                        name: new
                            .flow
                            .stage(id)
                            .map(|s| s.name.clone())
                            .unwrap_or_default(),
                    }),
                    message: p.message,
                })
                .collect();
        }
        draft
    }

    /// shop-web with Product spec before Dev and a Security check (the ticket file's example): 7 steps.
    pub const SHOP_WEB: &str = "agents:
  - name: Spec writer
    skills: [product-spec]
    model: default
  - name: Rust developer
    skills: [work-ticket]
    model: default
  - name: Checker
    skills: [validate]
    model: default
  - name: Security reviewer
    skills: [security-review]
    model: default
flow:
  - id: product-spec
    name: Product spec
    kind: agent
    column: own
    agent: Spec writer
    skill: product-spec
  - id: dev
    name: Dev
    kind: agent
    column: own
    agent: Rust developer
    skill: work-ticket
  - id: validate
    name: Validate
    kind: check
    by: agent
    column: own
    agent: Checker
    skill: validate
    rounds: 3
  - id: security
    name: Security
    kind: check
    by: agent
    column: own
    agent: Security reviewer
    skill: security-review
    rounds: 3
  - id: review
    name: Review
    kind: check
    by: you
    column: own
  - id: deliver
    name: Done
    kind: delivery
    column: with-previous
    agent: Rust developer
    skill: open-pr
    mode: click
    delivers: pr
";

    /// shop-web's flow as stored before Otaq's skills were renamed, renamed as the upgrade renames it
    /// (`otaq:work-ticket`, `tuq:product-spec`).
    pub fn shop_web() -> FlowFile {
        let mut file = otaq_core::flow::yaml::read(SHOP_WEB).expect("shop-web's flow reads");
        otaq_core::flow::skill_names::name_own_skills(&mut file);
        file
    }

    pub fn otaq_default() -> FlowFile {
        otaq_core::flow::presets::presets()[0].read()
    }

    /// shop-web with a Blank stage named Plan after Product spec, its skill `skill` (none: Otaq's own `work-ticket`
    /// runs), and the draft's proposal.
    pub fn plan_draft(skill: Option<&str>) -> (FlowFile, Proposal) {
        let (mut file, new) = edited(
            &shop_web(),
            &StageEdit::AddTemplate {
                after: Some("product-spec".into()),
                template: "new-stage".into(),
            },
        )
        .expect("a stage is added");
        let new = new.expect("the new stage");
        let plan = file
            .flow
            .stages
            .iter_mut()
            .find(|s| s.id == new)
            .expect("Plan");
        plan.id = "plan".into();
        plan.name = "Plan".into();
        plan.skill = skill.map(str::to_owned);
        let draft = proposal(&file, false);
        (file, draft)
    }

    /// `plan-ticket`, written for Plan by the Project agent Checkout slowness.
    pub fn written_plan() -> WrittenSkill {
        WrittenSkill {
            stage_id: "plan".into(),
            skill: "plan-ticket".into(),
            folder: ".claude/skills/plan-ticket".into(),
            fingerprint: String::new(),
            made: Vec::new(),
            by: "Checkout slowness".into(),
            was: None,
        }
    }

    /// `file` as the draft's proposal: unchanged when `same`.
    pub fn proposal(file: &FlowFile, same: bool) -> Proposal {
        Proposal {
            id: 0,
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            source: Source::Document { file: None },
            proposed_by: ProposedBy::User,
            yaml: otaq_core::flow::yaml::write(file),
            problems: Vec::new(),
            diff: FlowDiff {
                against_waiting: false,
                same,
                stages: Vec::new(),
                warnings: Vec::new(),
                waits: false,
            },
            why: Vec::new(),
            created_at: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::agent_tools::connected::AgentToolState;
    use otaq_core::flow::apply::Span;
    use otaq_core::flow::proposal::{Applies, Level, ProposalProblem, StageChange, Warning};

    use super::fixtures::*;
    use super::*;
    use crate::view_models::tool_problems::fixtures::{connected, status};

    struct Draft {
        proposal: Proposal,
        marks: Vec<StageMark>,
        applies: DraftApplies,
        swap: bool,
    }

    fn draft(file: &FlowFile, same: bool) -> Draft {
        Draft {
            proposal: proposal(file, same),
            marks: Vec::new(),
            applies: if same {
                DraftApplies::Nothing
            } else {
                DraftApplies::AtOnce
            },
            swap: false,
        }
    }

    struct Env {
        tools: Tools,
        skills: Vec<FoundSkill>,
        writing: Vec<SkillWriting>,
        written: Vec<WrittenSkill>,
    }

    /// Claude Code and OpenCode connected.
    fn tools() -> Tools {
        Tools::from_statuses(vec![
            connected(Tool::ALL[0]),
            status(Tool::ALL[1], AgentToolState::NotInstalled),
            status(Tool::ALL[2], AgentToolState::NotInstalled),
            connected(Tool::ALL[3]),
        ])
    }

    fn env() -> Env {
        Env {
            tools: tools(),
            skills: found_skills(),
            writing: Vec::new(),
            written: Vec::new(),
        }
    }

    fn edited(file: &FlowFile, edit: &StageEdit) -> Result<(FlowFile, Option<String>), String> {
        super::edited(file, edit)
    }

    /// A Blank stage from the palette after `after` (`None`: first).
    fn add_blank(after: Option<&str>) -> StageEdit {
        StageEdit::AddTemplate {
            after: after.map(str::to_owned),
            template: "new-stage".into(),
        }
    }

    /// What the board's editor takes from these parts: the selected stage's details, the footer, the change line and
    /// the picks Save sends.
    struct Rendered {
        details: Option<Details>,
        footer: Footer,
        change_line: Option<String>,
        /// Each stage's problem, as its column shows it.
        problems: Vec<(String, Option<String>)>,
        /// Each stage's skill warning, as its column shows it.
        warnings: Vec<(String, Option<String>)>,
    }

    impl Rendered {
        fn problem(&self, id: &str) -> Option<String> {
            self.problems
                .iter()
                .find(|(s, _)| s == id)
                .and_then(|(_, p)| p.clone())
        }

        fn warning(&self, id: &str) -> Option<String> {
            self.warnings
                .iter()
                .find(|(s, _)| s == id)
                .and_then(|(_, w)| w.clone())
        }
    }

    fn render(
        e: &Env,
        d: &Draft,
        file: &FlowFile,
        in_use: &FlowFile,
        selected: Option<&str>,
    ) -> Rendered {
        let f = Facts {
            project: "SHOP",
            project_name: "shop-web",
            proposal: &d.proposal,
            marks: &d.marks,
            swap: d.swap,
            applies: &d.applies,
            file,
            in_use,
            tools: &e.tools,
            skills: &e.skills,
            writing: &e.writing,
            written: &e.written,
            selected,
            agent: None,
            new_agent: None,
        };
        Rendered {
            details: selected
                .and_then(|id| file.flow.stage(id))
                .map(|stage| details(&f, stage)),
            footer: footer(&f),
            change_line: change_line(&f),
            problems: file
                .flow
                .stages
                .iter()
                .map(|s| (s.id.clone(), step_problem(&f, s)))
                .collect(),
            warnings: file
                .flow
                .stages
                .iter()
                .map(|s| (s.id.clone(), stage_skill_warning(&f, s)))
                .collect(),
        }
    }

    #[test]
    fn the_footer_says_whether_there_is_anything_to_save() {
        let e = env();
        let file = shop_web();
        let v = render(&e, &draft(&file, true), &file, &file, None);
        assert_eq!(
            (v.footer.cancel, v.footer.apply.as_str()),
            ("Cancel", "No changes")
        );
        assert!(!v.footer.apply_enabled);
        assert_eq!(v.change_line, None);
        let v = render(&e, &draft(&file, false), &file, &file, None);
        assert_eq!(v.footer.apply, "Apply to shop-web");
        assert!(v.footer.apply_enabled);
    }
    /// d0115, d0129: Spec first's planning skills show Otaq's own captions, not their agent-facing descriptions.
    #[test]
    fn the_planning_skills_show_otaqs_own_captions() {
        let captions: Vec<Option<String>> =
            ["tuq:product-spec", "tuq:tech-spec", "tuq:milestone-plan"]
                .into_iter()
                .map(|skill| shipped_caption(skill, None))
                .collect();
        assert_eq!(
            captions,
            [
                Some(
                    "An agent asks you what to build, for whom and why, then writes down what you agreed."
                        .to_owned()
                ),
                Some(
                    "An agent works out how to build it, asking you where a choice matters, and writes it down."
                        .to_owned()
                ),
                Some(
                    "An agent writes the ticket\u{2019}s plan: the pieces, their order and what each waits for."
                        .to_owned()
                ),
            ]
        );
    }

    #[test]
    fn a_stages_details() {
        let e = env();
        let file = shop_web();
        let d = draft(&file, true);
        let v = render(&e, &d, &file, &file, Some("dev"));
        let details = v.details.expect("Dev's details");
        let model = details.model.clone().expect("a model");
        // An older file's agent on `default` reads as the stage's own `default` model.
        assert_eq!(model.value, "default");
        assert_eq!(model.label, "Model");
        assert_eq!(
            details.skill.map(|s| s.state),
            Some(SkillState::Set {
                skill: otaq_core::agent_tools::WORK_TICKET_SKILL.into(),
                warning: None,
                fix: None,
            })
        );
        let v = render(&e, &d, &file, &file, Some("review"));
        let details = v.details.expect("Review's details");
        assert!(details.model.is_none() && details.skill.is_none());
    }

    #[test]
    fn a_proposal_tinted_with_undo() {
        let e = env();
        let base = otaq_default();
        let mut file = base.clone();
        let (with_product_spec, _) = edited(&file, &add_blank(None)).unwrap();
        file = with_product_spec;
        file.flow.stages[0].name = "Product spec".into();
        file.flow.stages[0].id = "product-spec".into();
        file.flow.stages[0].skill = Some("product-spec".into());
        file.flow.stages[0].who = otaq_core::flow::WhoDoesIt::on("claude");
        let mut d = draft(&file, false);
        d.marks = vec![StageMark {
            stage_id: "product-spec".into(),
            stage: "Product spec".into(),
            mark: Mark::Added,
            by: "Claude Code".into(),
            why: Some("From 12 /interview-me runs".into()),
        }];
        let v = render(&e, &d, &file, &base, Some("product-spec"));
        let mark = v.details.unwrap().mark.expect("Product spec is tinted");
        assert_eq!(mark.label, "Added");
        assert_eq!(
            mark.why,
            "Changed by Claude Code: From 12 /interview-me runs"
        );
        assert_eq!(mark.undo, "Undo");
        let v = render(&e, &d, &file, &base, Some("dev"));
        assert!(v.details.unwrap().mark.is_none());
    }

    #[test]
    fn the_change_line_names_the_span_and_the_tickets_in_it() {
        let e = env();
        let in_use = shop_web();
        let (file, _) = edited(
            &in_use,
            &StageEdit::Move {
                stage: "security".into(),
                to: 2,
            },
        )
        .unwrap();
        let mut d = draft(&file, false);
        d.proposal.diff.stages.push(StageChange {
            stage_id: "security".into(),
            stage: "Security".into(),
            change: Change::Moved {
                after: Some("dev".into()),
            },
            text: String::new(),
            label: String::new(),
            applies: Applies::Span {
                first: "Dev".into(),
                last: "Review".into(),
            },
        });
        let inside = |key: &str, stage: &str| Inside {
            ticket: key.parse().unwrap(),
            stage: stage.into(),
        };
        d.applies = DraftApplies::Span {
            span: Span {
                stages: vec![
                    "dev".into(),
                    "validate".into(),
                    "security".into(),
                    "review".into(),
                ],
                first: "Dev".into(),
                last: "Review".into(),
            },
            inside: vec![
                inside("SHOP-22", "dev"),
                inside("SHOP-33", "dev"),
                inside("SHOP-29", "dev"),
                inside("SHOP-30", "dev"),
                inside("SHOP-19", "security"),
                inside("SHOP-32", "security"),
            ],
        };
        let v = render(&e, &d, &file, &in_use, None);
        assert_eq!(
            v.change_line.as_deref(),
            Some(
                "Security moved before Validate. This waits until no ticket is between Dev and Review: the 6 there \
                 now (4 in Dev, 2 in Security) finish on the current order, and until then no ticket enters Dev to \
                 Review. Tickets before Dev keep moving. The banner offers Apply now."
            )
        );
        assert_eq!(v.footer.apply, "Apply to shop-web");
        assert!(v.footer.apply_enabled);
    }

    #[test]
    fn the_change_line_names_a_removed_stage_holding_a_ticket_by_its_name() {
        let e = env();
        let in_use = shop_web();
        let (file, _) = edited(
            &in_use,
            &StageEdit::Remove {
                stage: "validate".into(),
            },
        )
        .unwrap();
        let mut d = draft(&file, false);
        d.proposal.diff.stages.push(StageChange {
            stage_id: "validate".into(),
            stage: "Validate".into(),
            change: Change::Removed,
            text: String::new(),
            label: String::new(),
            applies: Applies::Span {
                first: "Validate".into(),
                last: "Validate".into(),
            },
        });
        d.applies = DraftApplies::Span {
            span: Span {
                stages: vec!["validate".into()],
                first: "Validate".into(),
                last: "Validate".into(),
            },
            inside: vec![Inside {
                ticket: "SHOP-22".parse().unwrap(),
                stage: "validate".into(),
            }],
        };
        let v = render(&e, &d, &file, &in_use, None);
        let line = v.change_line.expect("the draft waits");
        assert!(
            line.contains("the ticket there now (1 in Validate) finishes"),
            "{line}"
        );
        assert!(!line.contains("in validate"), "{line}");
    }

    #[test]
    fn problems_show_on_their_stage_and_keep_apply_off() {
        let e = env();
        let file = shop_web();
        let mut d = draft(&file, false);
        d.proposal.problems.push(ProposalProblem {
            about: About::Stage {
                id: "security".into(),
                name: "Security".into(),
            },
            message: "Security names the agent Security reviewer, which isn't one of your agents"
                .into(),
        });
        let v = render(&e, &d, &file, &file, None);
        assert_eq!(
            v.problem("security").as_deref(),
            Some("Security names the agent Security reviewer, which isn't one of your agents")
        );
        assert!(!v.footer.apply_enabled);
        assert_eq!(v.footer.reason, v.problem("security"));
    }

    /// Dragging a check in front of every stage where an agent works, or the last such stage after a check, is
    /// refused with why; a move that keeps an Agent stage before every check goes.
    #[test]
    fn a_reorder_that_leaves_a_check_first_is_refused_with_why() {
        let file = otaq_default();
        let mv = |f: &FlowFile, stage: &str, to: usize| {
            edited(
                f,
                &StageEdit::Move {
                    stage: stage.into(),
                    to,
                },
            )
        };
        assert_eq!(
            mv(&file, "validate", 0).unwrap_err(),
            "Validate can\u{2019}t go before Dev: a check sends its fails back to a stage where an agent works, \
             so one has to come before it"
        );
        assert_eq!(
            mv(&file, "dev", 2).unwrap_err(),
            "Dev can\u{2019}t go after Validate: a check sends its fails back to a stage where an agent works, so \
             one has to come before it"
        );

        // With a second Agent stage in front, Validate may go before Dev.
        let (added, _) = edited(&file, &add_blank(None)).unwrap();
        let (moved, _) = mv(&added, "validate", 1).unwrap();
        let ids: Vec<&str> = moved.flow.stages.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["new-stage", "validate", "dev", "review"]);
    }

    #[test]
    fn edits_change_the_document() {
        let file = shop_web();
        let ids = |f: &FlowFile| {
            f.flow
                .stages
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>()
        };
        // Add after Product spec: a new agent stage on the default model and no skill (Blank), selected.
        let (added, select) = edited(&file, &add_blank(Some("product-spec"))).unwrap();
        assert_eq!(select.as_deref(), Some("new-stage"));
        assert_eq!(ids(&added)[..3], ["product-spec", "new-stage", "dev"]);
        let new = added.flow.stage("new-stage").unwrap();
        assert_eq!(
            (new.who.model.as_deref(), new.skill.as_deref()),
            (Some("default"), None)
        );
        let (again, select) = edited(&added, &add_blank(None)).unwrap();
        assert_eq!(select.as_deref(), Some("new-stage-2"));
        assert_eq!(again.flow.stages[0].name, "New stage 2");
        // The document reads back.
        assert!(otaq_core::flow::yaml::read(&otaq_core::flow::yaml::write(&again)).is_ok());
        // Move: never past the Delivery.
        let (moved, _) = edited(
            &file,
            &StageEdit::Move {
                stage: "product-spec".into(),
                to: 99,
            },
        )
        .unwrap();
        assert_eq!(
            ids(&moved),
            ["dev", "validate", "security", "review", "product-spec"]
        );
        assert!(
            edited(
                &file,
                &StageEdit::Move {
                    stage: "deliver".into(),
                    to: 0
                }
            )
            .is_err()
        );
        assert!(
            edited(
                &file,
                &StageEdit::Remove {
                    stage: "deliver".into()
                }
            )
            .is_err()
        );
        let (removed, _) = edited(
            &file,
            &StageEdit::Remove {
                stage: "security".into(),
            },
        )
        .unwrap();
        assert!(removed.flow.stage("security").is_none());
        // Kind: a check by you drops the agent; back to an agent check takes the kind's defaults.
        let (you, _) = edited(
            &file,
            &StageEdit::Kind {
                stage: "validate".into(),
                kind: EditorKind::YouCheck,
            },
        )
        .unwrap();
        let s = you.flow.stage("validate").unwrap();
        assert!(
            s.is_human_check()
                && s.who.model.is_none()
                && s.rounds.is_none()
                && s.fail_to.is_none()
        );
        let (back, _) = edited(
            &you,
            &StageEdit::Kind {
                stage: "validate".into(),
                kind: EditorKind::AgentChecks,
            },
        )
        .unwrap();
        let s = back.flow.stage("validate").unwrap();
        assert_eq!(
            (s.who.model.as_deref(), s.skill.as_deref(), s.rounds),
            (
                Some("default"),
                Some(otaq_core::own_skill!("validate")),
                Some(3)
            )
        );
        assert!(otaq_core::flow::yaml::read(&otaq_core::flow::yaml::write(&back)).is_ok());
        // Rename, an option.
        let (renamed, _) = edited(
            &file,
            &StageEdit::Rename {
                stage: "dev".into(),
                name: " Build ".into(),
            },
        )
        .unwrap();
        assert_eq!(renamed.flow.stage("dev").unwrap().name, "Build");
        assert!(
            edited(
                &file,
                &StageEdit::Rename {
                    stage: "dev".into(),
                    name: " ".into()
                }
            )
            .is_err()
        );
        let (skip, _) = edited(
            &file,
            &StageEdit::Option {
                stage: "review".into(),
                option: "skip".into(),
                on: true,
            },
        )
        .unwrap();
        assert_eq!(
            skip.flow.stage("review").unwrap().options,
            [StageOption::Skip]
        );
        let (unskip, _) = edited(
            &skip,
            &StageEdit::Option {
                stage: "review".into(),
                option: "skip".into(),
                on: false,
            },
        )
        .unwrap();
        assert!(unskip.flow.stage("review").unwrap().options.is_empty());
        // Skip is the only option (a ticket can't pick a model, UX-72): any other is refused.
        assert_eq!(
            edited(
                &file,
                &StageEdit::Option {
                    stage: "dev".into(),
                    option: "Quick developer".into(),
                    on: true,
                },
            )
            .unwrap_err(),
            "Dev\u{2019}s only option is skip: a ticket runs it on the stage\u{2019}s own model"
        );
    }

    fn plan_details(v: &Rendered) -> &Details {
        v.details.as_ref().expect("Plan's details")
    }

    /// A new stage with no skill has an empty Skill field with both options, saying Otaq's own skill for the stage
    /// runs; nothing marks its column and Apply stays on: a stage with no skill runs Otaq's own.
    #[test]
    fn a_stage_without_a_skill_runs_otaqs_own_and_saves() {
        let e = env();
        let shop = shop_web();
        let (file, d) = plan_draft(None);
        let d = Draft {
            proposal: d,
            ..draft(&file, false)
        };
        let v = render(&e, &d, &file, &shop, Some("plan"));
        let details = plan_details(&v);
        assert_eq!(details.model.as_ref().unwrap().value, "default");
        let skill = details.skill.as_ref().unwrap();
        assert_eq!(
            skill.state,
            SkillState::Empty {
                text: format!(
                    "No skill of yours: Plan runs {}\u{2019}s `{}`.",
                    otaq_core::naming::PRODUCT_NAME,
                    otaq_core::agent_tools::WORK_TICKET_SKILL
                ),
                pick: "Pick a skill\u{2026}",
                ask: "Ask the agent to write it",
                hint: "Pick: your own skills, searchable. Ask: starts Noyan on the right and uses tokens.",
                prefill: "Write a skill for the Plan stage: ".into(),
            }
        );
        // Pick a skill…: the user's own first, nothing checked yet.
        assert_eq!(skill.picker.items[0].id, "security-review");
        assert!(!skill.picker.items.iter().any(|i| i.checked));
        // Nothing marks the column, and Apply is on.
        assert_eq!(v.problem("plan"), None);
        assert!(v.footer.apply_enabled);
        assert_eq!(v.footer.reason, None);
        // Q441: the request is only prefilled, the caret after it; the user finishes it and sends.
        assert_eq!(
            write_skill_prefill(&file, "plan"),
            "Write a skill for the Plan stage: "
        );
    }

    /// Pick a skill… lists the skills the user already has, personal ones then the project's, then Otaq's own,
    /// labelled as Otaq's; each second line says where it lives and what it does. A typed name is its last item.
    #[test]
    fn pick_a_skill_lists_personal_then_project_then_otaqs_own() {
        let e = env();
        let shop = shop_web();
        let (file, d) = plan_draft(None);
        let d = Draft {
            proposal: d,
            ..draft(&file, false)
        };
        let v = render(&e, &d, &file, &shop, Some("plan"));
        let picker = &plan_details(&v).skill.as_ref().unwrap().picker;
        let items: Vec<(&str, Option<&str>)> = picker
            .items
            .iter()
            .map(|i| (i.id.as_str(), i.hint.as_deref()))
            .collect();
        assert_eq!(
            items[..2],
            [
                (
                    "security-review",
                    Some(
                        "Personal \u{b7} Claude Code \u{b7} A security agent reviews the change for risky code."
                    )
                ),
                (
                    "release-notes",
                    Some(
                        "This project \u{b7} Claude Code, GitHub Copilot, OpenCode \u{b7} Writes the release notes \
                         from the merged tickets."
                    )
                ),
            ]
        );
        // The user's own are unmarked; then the "Otaq" group, its header with Otaq's mark (decision 108).
        assert!(
            picker.items[..2]
                .iter()
                .all(|i| i.badge.is_none() && !i.mark && !i.header)
        );
        let header = &picker.items[2];
        assert!(header.header && header.mark && header.separated);
        assert_eq!(header.label, otaq_core::naming::PRODUCT_NAME);
        let own = |folder: &str| otaq_core::skills::own_name(folder);
        assert_eq!(items[3].0, own("address-review"));
        let otaqs = &picker.items[3..];
        // Otaq's workflow skills and tuq's (decision 104), tuq's as tuq is known to have them, by their namespaced
        // names, each with the "Otaq" badge. All but `check-ticket`: a checker's workflow skill, never a stage's own
        // (decision 113).
        assert_eq!(
            otaqs.len(),
            otaq_core::agents::shipped::SKILLS.len() - 1 + otaq_core::tuq::KNOWN.len()
        );
        let ids: Vec<&str> = otaqs.iter().map(|i| i.id.as_str()).collect();
        assert!(!ids.contains(&otaq_core::agent_tools::CHECK_TICKET_SKILL));
        let product_spec = format!("{}:product-spec", otaq_core::tuq::NAMESPACE);
        assert!(
            ids.contains(&own("validate").as_str())
                && ids.contains(&own("work-ticket").as_str())
                && ids.contains(&product_spec.as_str())
        );
        assert!(otaqs.iter().all(|i| i.label == i.id
            && !i.header
            && i.badge.as_deref() == Some(otaq_core::naming::PRODUCT_NAME)));
        // Its second line says what it does, in Otaq's words where it has them; no "Otaq's ·" before it.
        let hint = |id: &str| otaqs.iter().find(|i| i.id == id).unwrap().hint.clone();
        assert_eq!(hint(&product_spec), shipped_caption(&product_spec, None));
        assert!(otaqs.iter().all(|i| !i.hint.as_deref().is_some_and(|h| {
            h.starts_with(&format!("{}\u{2019}s", otaq_core::naming::PRODUCT_NAME))
        })));
        assert_eq!(picker.search.placeholder, "Search your skills\u{2026}");
        assert_eq!(picker.use_typed, "Use `{name}`");

        // With none of the user's own, Otaq's own still show: the list is never empty, and starts with the group's
        // header, nothing above it to separate it from.
        let mut e = e;
        e.skills = otaq_skills();
        let v = render(&e, &d, &file, &shop, Some("plan"));
        let picker = &plan_details(&v).skill.as_ref().unwrap().picker;
        assert_eq!(
            picker.items.len(),
            1 + otaq_core::agents::shipped::SKILLS.len() - 1 + otaq_core::tuq::KNOWN.len()
        );
        assert!(picker.items[0].header && picker.items[0].mark && !picker.items[0].separated);
        assert!(
            picker.items[1..]
                .iter()
                .all(|i| !i.disabled && !i.id.is_empty())
        );
    }

    /// F-D-1, F-D-2 (decision 108: "its second line says what it does"): with no tool's tuq installed, every row of
    /// the "Otaq" group says what it does in Otaq's words, tuq's `step-back` too, and none reads as instructions to
    /// the agent.
    #[test]
    fn every_otaq_row_says_what_it_does_without_tuq_installed() {
        let mut e = env();
        e.skills = otaq_skills();
        let step_back = format!("{}:step-back", otaq_core::tuq::NAMESPACE);
        assert!(
            e.skills
                .iter()
                .any(|s| s.name == step_back && s.description.is_none())
        );
        let (file, d) = plan_draft(None);
        let d = Draft {
            proposal: d,
            ..draft(&file, false)
        };
        let v = render(&e, &d, &file, &shop_web(), Some("plan"));
        let picker = &plan_details(&v).skill.as_ref().unwrap().picker;
        let otaqs: Vec<&MenuItem> = picker.items.iter().filter(|i| !i.header).collect();
        assert!(!otaqs.is_empty());
        for row in &otaqs {
            assert_eq!(row.hint, shipped_caption(&row.id, None), "{}", row.id);
            let hint = row.hint.as_deref().unwrap();
            assert!(
                !hint.contains('$') && !hint.contains('`') && hint.len() < 120,
                "{}: {hint}",
                row.id
            );
        }
        assert_eq!(
            otaqs.iter().find(|i| i.id == step_back).unwrap().hint,
            shipped_caption(&step_back, None)
        );
        // A check stage's `check-ticket`, listed when a stage names it, has Otaq's words too.
        assert!(shipped_caption(otaq_core::agent_tools::CHECK_TICKET_SKILL, None).is_some());
        // The user's own skills keep their own descriptions: a bare name is never captioned.
        assert_eq!(shipped_caption("step-back", None), None);
        assert_eq!(shipped_caption("address-review", None), None);
    }

    /// Decision 113: Otaq's `check-ticket` is never offered as a stage's skill (Otaq starts it before a check stage's
    /// task skill), but a stage that already names it still shows it, checked.
    #[test]
    fn check_ticket_shows_only_on_a_stage_that_names_it() {
        let mut e = env();
        e.skills = otaq_skills();
        let shop = shop_web();
        let check_ticket = otaq_core::agent_tools::CHECK_TICKET_SKILL;
        let listed = |skill: Option<&str>| {
            let (file, d) = plan_draft(skill);
            let d = Draft {
                proposal: d,
                ..draft(&file, false)
            };
            let v = render(&e, &d, &file, &shop, Some("plan"));
            plan_details(&v)
                .skill
                .as_ref()
                .unwrap()
                .picker
                .items
                .iter()
                .find(|i| i.id == check_ticket)
                .map(|i| i.checked)
        };
        assert_eq!(listed(None), None);
        assert_eq!(listed(Some(otaq_core::own_skill!("validate"))), None);
        assert_eq!(listed(Some(check_ticket)), Some(true));
    }

    /// Decision 108: a personal `validate` is the user's, listed above the "Otaq" group, unmarked, apart from Otaq's
    /// `otaq:validate`, which is in the group with its badge.
    #[test]
    fn a_personal_validate_sits_above_otaqs_unmarked() {
        let mut e = env();
        e.skills.push(users_skill(
            "validate",
            "Checks the build.",
            SkillScope::Personal,
            "/Users/me/.claude/skills",
            &[Tool::Claude],
        ));
        let shop = shop_web();
        let (file, d) = plan_draft(None);
        let d = Draft {
            proposal: d,
            ..draft(&file, false)
        };
        let v = render(&e, &d, &file, &shop, Some("plan"));
        let items = &plan_details(&v).skill.as_ref().unwrap().picker.items;
        let at = |id: &str| items.iter().position(|i| i.id == id && !i.header).unwrap();
        let group = items.iter().position(|i| i.header).unwrap();
        let mine = &items[at("validate")];
        assert!(at("validate") < group);
        assert_eq!(mine.badge, None);
        assert_eq!(
            mine.hint.as_deref(),
            Some("Personal \u{b7} Claude Code \u{b7} Checks the build.")
        );
        let otaqs = &items[at(&otaq_core::skills::own_name("validate"))];
        assert!(at(&otaqs.id) > group);
        assert_eq!(
            otaqs.badge.as_deref(),
            Some(otaq_core::naming::PRODUCT_NAME)
        );
    }

    /// A personal Codex skill says Codex has it; one Codex finds but skips (its SKILL.md starts with a byte order
    /// mark) says why, not "Not committed yet".
    #[test]
    fn pick_a_skill_says_codex_has_or_skips_a_personal_skill() {
        let mut e = env();
        let codex = "/Users/me/.codex/skills";
        let mut skipped = users_skill("bommed", "B.", SkillScope::Personal, codex, &[]);
        skipped.skipped = vec![Tool::Codex];
        e.skills = vec![
            users_skill(
                "reltest-docs",
                "R.",
                SkillScope::Personal,
                codex,
                &[Tool::Codex],
            ),
            skipped,
        ];
        let shop = shop_web();
        let (file, d) = plan_draft(None);
        let d = Draft {
            proposal: d,
            ..draft(&file, false)
        };
        let v = render(&e, &d, &file, &shop, Some("plan"));
        let picker = &plan_details(&v).skill.as_ref().unwrap().picker;
        let hint = |id: &str| {
            picker
                .items
                .iter()
                .find(|i| i.id == id)
                .and_then(|i| i.hint.clone())
        };
        assert_eq!(
            hint("reltest-docs").as_deref(),
            Some("Personal \u{b7} Codex \u{b7} R.")
        );
        assert_eq!(
            hint("bommed").as_deref(),
            Some(
                "Personal \u{b7} Codex doesn\u{2019}t load it: its SKILL.md starts with a byte order mark \u{b7} B."
            )
        );
    }

    /// A name typed by hand is the stage's skill as it is, with nothing imported or copied; a skill nobody has shows
    /// "You don't have the skill `<name>`" (the draft's warning) in its Skill field and on its column, amber, and Apply
    /// still works. Otaq's own skills never warn.
    #[test]
    fn a_typed_name_works_and_a_skill_nobody_has_warns() {
        let e = env();
        let shop = shop_web();
        let (typed, _) = edited(
            &shop,
            &StageEdit::Skill {
                stage: "validate".into(),
                skill: "  team-validate ".into(),
            },
        )
        .unwrap();
        assert_eq!(
            typed.flow.stage("validate").unwrap().skill.as_deref(),
            Some("team-validate")
        );
        let mut d = draft(&typed, false);
        d.proposal.diff.warnings = vec![Warning {
            level: Level::Amber,
            stage: Some("Validate".into()),
            text: "You don't have the skill `team-validate`".into(),
        }];
        let v = render(&e, &d, &typed, &shop, Some("validate"));
        let field = v.details.as_ref().unwrap().skill.as_ref().unwrap();
        assert_eq!(
            field.state,
            SkillState::Set {
                skill: "team-validate".into(),
                warning: Some("You don't have the skill `team-validate`".into()),
                fix: Some(SKILL_FIX),
            }
        );
        assert_eq!(
            v.warning("validate").as_deref(),
            Some("You don't have the skill `team-validate`")
        );
        assert_eq!(v.problem("validate"), None);
        assert!(v.footer.apply_enabled, "only a warning");
        assert!(
            v.warnings
                .iter()
                .filter(|(id, _)| id != "validate")
                .all(|(_, w)| w.is_none())
        );

        // `otaq:validate` is Otaq's own: no warning, even one the draft says.
        let mut d = draft(&shop, false);
        d.proposal.diff.warnings = vec![Warning {
            level: Level::Amber,
            stage: Some("Validate".into()),
            text: "You don't have the skill `validate`".into(),
        }];
        let v = render(&e, &d, &shop, &shop, Some("validate"));
        assert_eq!(
            v.details.as_ref().unwrap().skill.as_ref().unwrap().state,
            SkillState::Set {
                skill: otaq_core::own_skill!("validate").into(),
                warning: None,
                fix: None,
            }
        );
        assert!(v.warnings.iter().all(|(_, w)| w.is_none()));
    }

    /// FED-06 (`s27-06`): while the agent writes it the field says so with Stop, and Apply's reason says it too;
    /// once written, the skill is tinted with Open and Undo, and Apply is on.
    #[test]
    fn the_skill_being_written_then_written() {
        let mut e = env();
        let shop = shop_web();
        let (file, d) = plan_draft(None);
        let d = Draft {
            proposal: d,
            ..draft(&file, false)
        };
        e.writing = vec![SkillWriting {
            folder: None,
            stage_id: "plan".into(),
            agent: Some("pa-1".into()),
            made: Vec::new(),
        }];
        let v = render(&e, &d, &file, &shop, Some("plan"));
        assert_eq!(
            plan_details(&v).skill.as_ref().unwrap().state,
            SkillState::Writing {
                text: "Being written by Noyan\u{2026}",
                stop: "Stop",
            }
        );
        assert_eq!(
            v.footer.reason.as_deref(),
            Some("Plan\u{2019}s skill is being written")
        );
        assert!(!v.footer.apply_enabled);

        let (file, d) = plan_draft(Some("plan-ticket"));
        let d = Draft {
            proposal: d,
            ..draft(&file, false)
        };
        e.writing.clear();
        e.written = vec![written_plan()];
        let v = render(&e, &d, &file, &shop, Some("plan"));
        assert_eq!(
            plan_details(&v).skill.as_ref().unwrap().state,
            SkillState::Written {
                skill: "plan-ticket".into(),
                note: "Written by the agent",
                saved: Some(
                    "Saved in `.claude/skills/plan-ticket` in this project. Commit it so agents get it in their \
                     worktrees."
                        .into()
                ),
                why: "Written by Checkout slowness".into(),
                open: Some("Open"),
                undo: "Undo",
            }
        );
        assert_eq!(v.problem("plan"), None);
        assert!(v.footer.apply_enabled);
        assert_eq!(v.footer.reason, None);
        // No tool reads it until it is committed, so the draft warns the user doesn't have it: the stage's warning
        // says it is written but not committed yet instead, and stays only a warning.
        let mut d = d;
        d.proposal.diff.warnings = vec![Warning {
            level: Level::Amber,
            stage: Some("Plan".into()),
            text: "You don't have the skill `plan-ticket`".into(),
        }];
        let v = render(&e, &d, &file, &shop, Some("plan"));
        assert_eq!(
            v.warning("plan").as_deref(),
            Some("`plan-ticket` is written but not committed yet: commit it so agents get it")
        );
        assert!(v.footer.apply_enabled);
        // One an older Otaq wrote has no folder in the project: no saved line, and no Open.
        let mut old = e.written.clone();
        old[0].folder.clear();
        let e_old = Env {
            written: old,
            ..env()
        };
        let state = plan_details(&render(&e_old, &d, &file, &shop, Some("plan")))
            .skill
            .clone()
            .unwrap()
            .state;
        assert!(matches!(
            state,
            SkillState::Written {
                saved: None,
                open: None,
                ..
            }
        ));
        // Another skill picked over it: the field shows that one, untinted.
        let mut picked = file.clone();
        picked.flow.stages[1].skill = Some("security-review".into());
        let v = render(&e, &d, &picked, &shop, Some("plan"));
        assert_eq!(
            plan_details(&v).skill.as_ref().unwrap().state,
            SkillState::Set {
                skill: "security-review".into(),
                warning: None,
                fix: None,
            }
        );
    }

    /// Ask the agent to write it is offered on a stage that already has a skill too: as Pick a skill…'s last row
    /// (searching or not, it puts "Write a skill for the Plan stage: " in the agent's box), and under a missing skill's
    /// warning ("Pick one, or Ask the agent to write it."). Once the user sends, the stage shows it being written in
    /// place of the name nobody has, and Apply waits for it.
    #[test]
    fn a_stage_with_a_skill_nobody_has_can_ask_the_agent_to_write_it() {
        let mut e = env();
        let shop = shop_web();
        let (file, p) = plan_draft(Some("plan-steps"));
        let mut d = Draft {
            proposal: p,
            ..draft(&file, false)
        };
        d.proposal.diff.warnings = vec![Warning {
            level: Level::Amber,
            stage: Some("Plan".into()),
            text: "You don't have the skill `plan-steps`".into(),
        }];
        let v = render(&e, &d, &file, &shop, Some("plan"));
        let field = plan_details(&v).skill.clone().unwrap();
        assert_eq!(
            field.state,
            SkillState::Set {
                skill: "plan-steps".into(),
                warning: Some("You don't have the skill `plan-steps`".into()),
                fix: Some(SkillFix {
                    pick: "Pick one",
                    or: ", or ",
                    ask: "Ask the agent to write it",
                    end: ".",
                }),
            }
        );
        assert_eq!(field.picker.ask.id, ASK_TO_WRITE);
        assert_eq!(field.picker.ask.label, "Ask the agent to write it\u{2026}");
        assert!(field.picker.ask.separated);
        assert_eq!(field.picker.prefill, "Write a skill for the Plan stage: ");
        // No skill can be named like the row's id, so choosing it never sets a skill.
        assert!(field.picker.items.iter().all(|i| i.id != ASK_TO_WRITE));

        // The user's send starts the writer: the field says it is being written, and Apply waits for it.
        e.writing = vec![SkillWriting {
            folder: None,
            stage_id: "plan".into(),
            agent: Some("pa-1".into()),
            made: Vec::new(),
        }];
        let v = render(&e, &d, &file, &shop, Some("plan"));
        assert_eq!(
            plan_details(&v).skill.as_ref().unwrap().state,
            SkillState::Writing {
                text: "Being written by Noyan\u{2026}",
                stop: "Stop",
            }
        );
        assert!(!v.footer.apply_enabled);
        assert_eq!(
            v.footer.reason.as_deref(),
            Some("Plan\u{2019}s skill is being written")
        );
    }

    /// A ready stage from the palette comes with its description, model and skill; Blank takes the next free "New
    /// stage" name and no skill. A kind switch keeps an empty skill empty. Pick a skill… gives the stage the skill.
    #[test]
    fn ready_stages_and_a_picked_skill_change_the_document() {
        let ready = |template: &str| {
            edited(
                &shop_web(),
                &StageEdit::AddTemplate {
                    after: Some("dev".into()),
                    template: template.into(),
                },
            )
            .unwrap()
        };
        let (tech, id) = ready("tech-spec");
        let s = tech.flow.stage(id.as_deref().unwrap()).unwrap();
        assert_eq!(
            (s.name.as_str(), s.who.model.as_deref(), s.skill.as_deref()),
            ("Tech spec", Some("default"), Some("tuq:tech-spec"))
        );
        assert!(s.description.is_some());
        // shop-web has a Product spec already: the ready one is numbered.
        let (spec, id) = ready("product-spec");
        assert_eq!(
            spec.flow.stage(id.as_deref().unwrap()).unwrap().name,
            "Product spec 2"
        );
        let (blank, id) = ready("new-stage");
        let s = blank.flow.stage(id.as_deref().unwrap()).unwrap();
        assert_eq!((s.name.as_str(), s.skill.as_deref()), ("New stage", None));
        assert!(
            edited(
                &shop_web(),
                &StageEdit::AddTemplate {
                    after: None,
                    template: "nope".into()
                }
            )
            .is_err()
        );

        // A kind switch keeps Plan's empty skill empty.
        let (plan, _) = plan_draft(None);
        let (checks, _) = edited(
            &plan,
            &StageEdit::Kind {
                stage: "plan".into(),
                kind: EditorKind::AgentChecks,
            },
        )
        .unwrap();
        let s = checks.flow.stage("plan").unwrap();
        assert_eq!(
            (s.who.model.as_deref(), s.skill.as_deref()),
            (Some("default"), None)
        );
        // The draft with no skill reads back and passes the checks: Otaq's own skill for the stage runs.
        let again = otaq_core::flow::yaml::read(&otaq_core::flow::yaml::write(&plan)).unwrap();
        assert_eq!(otaq_core::flow::check(&again.flow), Ok(()));
        // Pick a skill…: Plan runs security-review.
        let (picked, _) = edited(
            &plan,
            &StageEdit::Skill {
                stage: "plan".into(),
                skill: "security-review".into(),
            },
        )
        .unwrap();
        assert_eq!(
            picked.flow.stage("plan").unwrap().skill.as_deref(),
            Some("security-review")
        );
        assert!(
            edited(
                &plan,
                &StageEdit::Skill {
                    stage: "review".into(),
                    skill: "security-review".into()
                }
            )
            .is_err(),
            "no agent runs Review"
        );
    }

    /// The draft of `file` against shop-web, checked: the flow check's problems on their stages (as the core's
    /// proposal has them), the changes from shop-web.
    fn checked(file: &FlowFile) -> Draft {
        let mut d = draft(file, false);
        d.proposal = checked_proposal(&shop_web(), file);
        d
    }

    /// `edits` in turn on shop-web.
    fn shop_with(edits: &[StageEdit]) -> FlowFile {
        edits.iter().fold(shop_web(), |file, edit| {
            edited(&file, edit).expect("the edit applies").0
        })
    }

    fn fail_to(stage: &str, to: Option<&str>) -> StageEdit {
        StageEdit::FailTo {
            stage: stage.into(),
            to: to.map(str::to_owned),
        }
    }

    /// d0131: Fails go to lists the Agent stages before the check, the nearest marked and picked by default; picking
    /// another sets `fail_to`, picking the nearest leaves it out; a stage that isn't one is refused.
    #[test]
    fn fails_go_to_offers_the_agent_stages_before_the_check() {
        let e = env();
        let file = shop_web();
        let d = draft(&file, true);
        let v = render(&e, &d, &file, &file, Some("review"));
        let details = v.details.expect("Review's details");
        let field = details.fail_to.expect("a check's Fails go to");
        assert_eq!(field.label, "Fails go to");
        assert_eq!(field.value, "Dev");
        assert_eq!(field.nearest, "dev");
        let items: Vec<(&str, &str, Option<&str>, bool, bool)> = field
            .items
            .iter()
            .map(|i| {
                (
                    i.id.as_str(),
                    i.label.as_str(),
                    i.hint.as_deref(),
                    i.checked,
                    i.header,
                )
            })
            .collect();
        assert_eq!(
            items,
            [
                (
                    "",
                    "STAGES WHERE AN AGENT WORKS, BEFORE THIS ONE",
                    None,
                    false,
                    true
                ),
                ("product-spec", "Product spec", None, false, false),
                ("dev", "Dev", Some("nearest"), true, false),
            ]
        );
        assert_eq!(
            field.explanation,
            "A fail resumes that stage\u{2019}s session with the report or your comments; the card then walks every \
             stage after it again. To send one card somewhere else, drag it back."
        );
        assert_eq!(field.problem, None);
        // A check by you has no model.
        assert!(details.model.is_none());
        // An Agent stage has none.
        let v = render(&e, &d, &file, &file, Some("dev"));
        assert!(v.details.expect("Dev's details").fail_to.is_none());

        // Picking Product spec sets it, and it shows; picking the nearest leaves it out again.
        let (to_product_spec, _) = edited(&file, &fail_to("review", Some("product-spec"))).unwrap();
        assert_eq!(
            to_product_spec
                .flow
                .stage("review")
                .unwrap()
                .fail_to
                .as_deref(),
            Some("product-spec")
        );
        let d = checked(&to_product_spec);
        let v = render(&e, &d, &to_product_spec, &file, Some("review"));
        let field = v.details.unwrap().fail_to.unwrap();
        assert_eq!(field.value, "Product spec");
        assert!(field.items[1].checked && !field.items[2].checked);
        assert!(v.footer.apply_enabled);
        assert_eq!(
            v.change_line.as_deref(),
            Some(
                "This applies at once: tickets that haven\u{2019}t reached a changed step follow it; tickets in it \
                 or past it finish as they are."
            )
        );
        for nearest in [Some("dev"), None] {
            let (back, _) = edited(&to_product_spec, &fail_to("review", nearest)).unwrap();
            assert_eq!(back.flow.stage("review").unwrap().fail_to, None);
        }
        // Read back from the flow file by name.
        let yaml = otaq_core::flow::yaml::write(&to_product_spec);
        assert!(yaml.contains("fail_to: Product spec"), "{yaml}");

        for (stage, to, why) in [
            (
                "review",
                "validate",
                "Validate isn\u{2019}t a stage where an agent works before Review",
            ),
            (
                "validate",
                "review",
                "Review isn\u{2019}t a stage where an agent works before Validate",
            ),
            ("dev", "product-spec", "Dev isn\u{2019}t a check"),
        ] {
            assert_eq!(
                edited(&file, &fail_to(stage, Some(to))).unwrap_err(),
                why,
                "{stage} → {to}"
            );
        }
        // Removing Product spec sends Review's fails to the nearest again.
        let (gone, _) = edited(
            &to_product_spec,
            &StageEdit::Remove {
                stage: "product-spec".into(),
            },
        )
        .unwrap();
        assert_eq!(gone.flow.stage("review").unwrap().fail_to, None);
    }

    /// d0131: a check whose fails go to a stage that no longer fits (it moved after the check) shows the problem on
    /// its Fails go to, not twice, and keeps Apply off until another stage is picked.
    #[test]
    fn a_fail_to_problem_keeps_apply_off_until_fixed() {
        let e = env();
        let file = shop_with(&[
            fail_to("review", Some("product-spec")),
            StageEdit::Move {
                stage: "product-spec".into(),
                to: 4,
            },
        ]);
        let d = checked(&file);
        let v = render(&e, &d, &file, &shop_web(), Some("review"));
        let message = "Review's fails go to Product spec, which isn't an Agent stage before it";
        assert_eq!(v.problem("review").as_deref(), Some(message));
        assert!(!v.footer.apply_enabled);
        assert_eq!(v.footer.reason.as_deref(), Some(message));
        let details = v.details.as_ref().unwrap();
        let field = details.fail_to.as_ref().unwrap();
        assert_eq!(field.value, "Product spec");
        assert_eq!(field.problem.as_deref(), Some(message));
        assert!(
            field.items.iter().all(|i| !i.checked),
            "Product spec isn't offered"
        );
        assert_eq!(details.problem, None, "shown once, on its field");

        let (fixed, _) = edited(&file, &fail_to("review", Some("dev"))).unwrap();
        let d = checked(&fixed);
        let v = render(&e, &d, &fixed, &shop_web(), Some("review"));
        assert_eq!(v.details.unwrap().fail_to.unwrap().problem, None);
        assert!(v.footer.apply_enabled);
    }
}

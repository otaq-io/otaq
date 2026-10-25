//! The flow editor (T-0471; FED-01, FED-03, FED-04, FED-07; d0115 "One flow editor", d0108, d0087, d0086; *27 → Row ·
//! One component* `s27-02`, *Row · In onboarding* `s27-03`, *Row · The agent changes it* `s27-04`, *Row · In the
//! project* `s27-05`, *Row · Changes that wait* `s27-07`): one component for onboarding's Flow step and for changing a
//! project's flow, in three parts.
//!
//! - **Start from** (left): the flow in use (in the project), the four presets, **Made for <project>** (an Ask per
//!   connected tool, each finished proposal its own entry, never a session count) and **Import a flow…** (a file, or
//!   another project's flow). Picking one copies it into the draft ([`Pick`]).
//! - **The flow** (middle): the animation of exactly what is in the draft ("Watch it", folded while a step's details
//!   are open), the steps as a list (drag to reorder, **+** between them on hover, **Add a stage**), the selected
//!   step's [`Details`] (Name, Kind, agent, Skill, "A ticket can", Remove stage…) and the legend of the four kinds.
//!   A step the Project agent changed is tinted with its mark and an Undo (the draft's marks, T-0470); problems from
//!   the draft's check show on their step and keep Apply off with the reason. The change line says when the draft
//!   would reach tickets ([`DraftApplies`], FED-08).
//! - **The Project agent** (right, [`AgentPanel`]): before anything is asked, "Not started", "<tool> · <model> ·
//!   starts when you ask", Otaq's own hint and fixed chips. **Nothing starts until the user types or clicks a chip**
//!   (d0087); then it is the project's Project agent on the `flow` skill: its state, tokens and terminal.
//!
//! Every user edit is a [`StageEdit`] the desktop applies to the draft's document with [`edited`] and sends whole as
//! `flow.draft_update`; Start from, Undo and Undo all are the draft's own edits.
//!
//! **Captions** (d0115 "No description field"): a step's caption is its skill's description; Ready for Dev, a check
//! by you and Delivery have Otaq's fixed caption for their kind. Otaq's shipped skills are written for agents, so
//! their captions are Otaq's own words ([`shipped_caption`], Q-B).
//!
//! **Templates and skills** (T-0472; FED-05, FED-06; d0115 "Stage templates fill the shape, the user supplies the
//! skill"; `s26-09`, `s27-05`, `s27-06`): a stage the user added has **Start from** templates in its details
//! ([`Templates`]): Plan, Security check, Visual check, Docs and Blank fill only the name and the kind; Grill,
//! Validate, Review and PR delivery come from Otaq's presets complete, skill included. An agent stage with no skill
//! offers **Pick a skill…** (the user's imported skills, searchable) and **Ask the agent to write it** (the Project
//! agent writes it in Otaq's folder and hands it in as Otaq's copy, d0092, never the repo, d0023); while it writes the
//! field says so with Stop, then shows the skill tinted with Open and Undo ([`SkillState`]). Apply stays off while an
//! agent stage has no skill ("Plan needs a skill"). **Ask the agent to write it** sends nothing (Q441): it puts "Write a
//! skill for the Plan stage: " in the panel's box ([`write_skill_prefill`]) for the user to finish and send.
//!
//! **Problems that need a pick** (Q426; FED-04, FLW-04's controls): a step whose problem has a fix shows it in its
//! details ([`FixRow`]): an import clash's Keep mine (the default) · Take theirs · Add as "<name> (<file>)", the model
//! menu for an agent whose model isn't listed or whose tool isn't connected, and "Import its folder…" for a missing
//! skill. The page keeps the picks ([`Choices`]) until Apply sends them, as the preview did; a picked problem no
//! longer marks its step, and Apply turns on once none is left.
//!
//! **Checks** (T-0529; d0131, d0130; `s29-04` "Fails go to" and "One check, several agents"): every check in the
//! steps list says where its fails go ("fails go to Dev"), and an agent check with several agents says "2 check
//! agents" in the agent's place. A check's details have **Fails go to** ([`FailToField`]): the Agent stages before it,
//! the nearest marked and picked by default (the flow file leaves `fail_to` out for it). An agent check's details list
//! its **Check agents** ([`CheckAgentsField`]: add, remove), one **Rounds** limit for the stage ([`RoundsField`]) and
//! the rule text; the change line says what a new set of check agents does ("Validate now runs Security reviewer and
//! Performance reviewer side by side…"). A `fail_to` that no longer fits, and an agent check left with no agent, are
//! the check's problems: they show in the step's details and keep Apply off until fixed.

use std::collections::{BTreeMap, HashSet};

use otaq_core::agent_tools::Tool;
use otaq_core::agents::{Agent, AgentModel, shipped};
use otaq_core::flow::check::{VALIDATION_ROUNDS, fail_to_message};
use otaq_core::flow::draft::{DraftApplies, Inside, Mark, SkillWriting, StageMark, WrittenSkill};
use otaq_core::flow::model::{
    CheckBy, Column, Delivers, Flow, FlowAgent, FlowFile, Stage, StageKind, StageOption,
};
use otaq_core::flow::presets::{preset, presets};
use otaq_core::flow::proposal::{
    About, AgentChange, AgentChangeKind, ApplyChoices, Change, ClashChoice, Fix, NEAREST, Proposal,
    ProposalProblem, Source, flow_agent_of,
};
use otaq_core::project_agents::first_flow::{FlowSetup, FlowSetupRow};
use otaq_core::project_agents::{AgentState, ProjectAgent};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::theme::Status as Legend;
use crate::view_models::agent_page::{Choice, DEFAULT_MODEL};
use crate::view_models::agents::{DEFAULT, ListState};
use crate::view_models::card::tokens_label;
use crate::view_models::menu::{MenuItem, MenuSearch};
use crate::view_models::model_picker::{self, Picker};
use crate::view_models::status::PillView;
use crate::view_models::tool_avatar::ToolAvatarView;
use crate::view_models::tool_problems::Tools;

/// Where the editor opened: onboarding's Flow step (Use this flow · Skip, use the default) or the project (Cancel ·
/// Apply). `FlowEditorFrom` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FlowEditorFrom")]
pub enum Opened {
    Onboarding,
    Project,
}

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
    Agent {
        stage: String,
        agent: String,
    },
    /// A ticket can …: `option` is `skip` or another agent's name.
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
    /// **+** after a step, or Add a stage: a new stage after `after` (`None`: first, right after Ready for Dev).
    Add {
        #[serde(default)]
        #[ts(optional)]
        after: Option<String>,
    },
    /// Remove stage….
    Remove {
        stage: String,
    },
    /// Pick a skill…: the stage runs the user's imported skill `skill` (by its name), which its agent gets.
    Skill {
        stage: String,
        skill: String,
    },
    /// A template picked in a new stage's details (FED-05).
    Template {
        stage: String,
        template: Template,
    },
    /// **Add a check agent** on an agent check (d0130).
    AddCheckAgent {
        stage: String,
        agent: String,
    },
    /// A check agent's remove.
    RemoveCheckAgent {
        stage: String,
        agent: String,
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
}

/// The templates of a new stage's details (FED-05, `s26-09`, `s27-05`). `FlowEditorTemplate` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FlowEditorTemplate")]
pub enum Template {
    Plan,
    Security,
    Visual,
    Docs,
    Blank,
    Grill,
    Validate,
    Review,
    PrDelivery,
}

impl Template {
    /// "name and kind; no skill".
    pub const PLAIN: [Self; 5] = [
        Self::Plan,
        Self::Security,
        Self::Visual,
        Self::Docs,
        Self::Blank,
    ];
    /// "From Otaq's presets · complete, skill included".
    pub const FROM_PRESETS: [Self; 4] =
        [Self::Grill, Self::Validate, Self::Review, Self::PrDelivery];

    pub fn label(self) -> &'static str {
        match self {
            Self::Plan => "Plan",
            Self::Security => "Security check",
            Self::Visual => "Visual check",
            Self::Docs => "Docs",
            Self::Blank => "Blank",
            Self::Grill => "Grill",
            Self::Validate => "Validate",
            Self::Review => "Review",
            Self::PrDelivery => "PR delivery",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::Plan => IconName::ListChecks,
            Self::Security | Self::Validate => IconName::ShieldCheck,
            Self::Visual => IconName::ScanEye,
            Self::Docs => IconName::BookOpen,
            Self::Blank => IconName::SquareDashed,
            Self::Grill => IconName::Bot,
            Self::Review => IconName::UserCheck,
            Self::PrDelivery => IconName::GitPullRequest,
        }
    }

    /// A plain template's stage name and kind (Q-A: the label is the name; Security and Visual check are agent
    /// checks); `None` for one from the presets.
    fn shape(self) -> Option<(&'static str, EditorKind)> {
        Some(match self {
            Self::Plan => ("Plan", EditorKind::Works),
            Self::Security => ("Security check", EditorKind::AgentChecks),
            Self::Visual => ("Visual check", EditorKind::AgentChecks),
            Self::Docs => ("Docs", EditorKind::Works),
            Self::Blank => (NEW_STAGE, EditorKind::Works),
            _ => return None,
        })
    }

    /// The shipped preset and stage a preset template copies: Grill from Grill first, Validate and Review from Otaq
    /// default, the PR delivery from Team PR.
    fn preset_source(self) -> Option<(&'static str, &'static str)> {
        Some(match self {
            Self::Grill => ("grill-first.yaml", "grill"),
            Self::Validate => (otaq_core::naming::DEFAULT_FLOW_FILE, "validate"),
            Self::Review => (otaq_core::naming::DEFAULT_FLOW_FILE, "review"),
            Self::PrDelivery => ("team-pr.yaml", "deliver"),
            _ => return None,
        })
    }

    /// The preset's stage and the preset's own agents (to bring the ones the user doesn't have).
    fn preset_stage(self) -> Option<(Stage, Vec<FlowAgent>)> {
        let (file, id) = self.preset_source()?;
        let read = preset(file)?.read();
        let stage = read.flow.stage(id)?.clone();
        Some((stage, read.agents))
    }
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

/// The user's picks for the draft's problems (Q426; FLW-04's, field for field), which the page keeps until Apply sends
/// them (`flow.apply`'s choices): a pick per import clash by the agent's name (Keep mine when left out), a folder per
/// missing skill, a local model per agent whose model isn't listed or whose tool isn't connected (`default`,
/// `claude`, `claude:opus`). The view is rebuilt with them on every pick. `FlowEditorChoices` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(rename = "FlowEditorChoices")]
pub struct Choices {
    #[serde(default)]
    pub clashes: BTreeMap<String, ClashPick>,
    #[serde(default)]
    pub skill_folders: BTreeMap<String, String>,
    #[serde(default)]
    pub models: BTreeMap<String, String>,
}

/// One import clash's pick (the core's `ClashChoice`). `FlowEditorClashPick` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FlowEditorClashPick")]
pub enum ClashPick {
    #[default]
    KeepMine,
    TakeTheirs,
    AddAs,
}

impl From<ClashPick> for ClashChoice {
    fn from(pick: ClashPick) -> Self {
        match pick {
            ClashPick::KeepMine => Self::KeepMine,
            ClashPick::TakeTheirs => Self::TakeTheirs,
            ClashPick::AddAs => Self::AddAs,
        }
    }
}

impl Choices {
    /// What Apply sends (`flow.apply`'s `choices`).
    pub fn apply_choices(&self) -> ApplyChoices {
        ApplyChoices {
            clashes: self
                .clashes
                .iter()
                .map(|(name, pick)| (name.clone(), (*pick).into()))
                .collect(),
            skill_folders: self.skill_folders.clone(),
            models: self.models.clone(),
        }
    }

    /// The pick `problem` asks for is there (a problem with no fix never is).
    pub fn fix(&self, problem: &ProposalProblem) -> bool {
        match &problem.fix {
            Some(Fix::ImportSkillFolder { skill }) => self.skill_folders.contains_key(skill),
            Some(Fix::PickModel { agent }) => self.models.contains_key(agent),
            None => false,
        }
    }
}

/// A local model for an agent (FLW-04's, with M32's picker `s15-30`): the default model or a connected tool, then that
/// tool's list. `FlowEditorModelFix` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowEditorModelFix")]
pub struct ModelFix {
    pub agent: String,
    pub tool_label: &'static str,
    /// The default model, then each connected tool; ids `default`, `claude`…
    pub tools: Vec<Choice>,
    /// The tool's picker once one is picked.
    pub picker: Option<Picker>,
    pub model_label: &'static str,
}

/// What a fix offers (Q426). `FlowEditorFixControl` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "FlowEditorFixControl")]
pub enum FixControl {
    /// Keep mine · Take theirs · Add as "<name> (<file>)" (ids `keep_mine`, `take_theirs`, `add_as`), selected as
    /// picked.
    Clash { agent: String, options: Vec<Choice> },
    /// "Import its folder…": the system folder picker, then [`Choices::skill_folders`].
    Folder {
        skill: String,
        label: &'static str,
        /// The folder picker's title.
        picker_title: String,
        /// The folder picked.
        picked: Option<String>,
    },
    /// The model menu ([`Choices::models`]).
    Model { fix: ModelFix },
}

/// A problem's fix, or an import clash, in a step's details (or under the flow when no step runs its agent).
/// `FlowEditorFix` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowEditorFix")]
pub struct FixRow {
    /// "security-review isn’t on this machine", "Security reviewer: opus isn’t in Claude Code’s model list",
    /// "Security reviewer is in the file and already here, with other instructions."
    pub text: String,
    /// Apply no longer waits for it: picked (a clash, Keep mine by default, never keeps Apply off).
    pub fixed: bool,
    pub control: FixControl,
}

/// The agent's default for each kind (FED-04: "Developer for An agent works, Validator for An agent checks").
pub const WORKS_AGENT: &str = "Developer";
pub const CHECKS_AGENT: &str = "Validator";
/// The skill a stage switched to each kind takes with the kind's agent (its own); a new stage starts with none
/// (FED-05).
pub const WORKS_SKILL: &str = "work-ticket";
pub const CHECKS_SKILL: &str = "validate";
/// An agent check's fails in a row before the card stops (the presets' 3).
pub const CHECK_ROUNDS: u32 = 3;
pub const NEW_STAGE: &str = "New stage";

/// The id a step row of Ready for Dev carries (it isn't a stage).
pub const READY_ID: &str = "ready";

/// The panel's line before anything was asked.
pub const STARTS_WHEN_YOU_ASK: &str = "\u{b7} starts when you ask";
pub const PANEL_TITLE: &str = "Noyan \u{b7} the flow";
pub const NOT_STARTED: &str = "Not started";
pub const TYPING_GOES: &str = "Typing goes to the agent";

/// The step's fixed captions (`s27-03`, `s27-05`).
pub const READY_CAPTION: &str =
    "The ticket you wrote waits here until an agent is free. Tickets start in priority order.";
pub const YOU_CHECK_CAPTION: &str =
    "You look at the change and approve it, or send it back with a note.";
pub const MERGE_CAPTION: &str = "Approved work is merged into main. The ticket is done.";
pub const PR_CAPTION: &str = "A pull request opens on GitHub; your team reviews it there.";

/// Shipped skills' captions in Otaq's words (Q-B): their SKILL.md descriptions are written for the agent.
pub fn shipped_caption(skill: &str, back: Option<&str>) -> Option<String> {
    Some(match skill {
        "work-ticket" => {
            "An agent writes the code in the ticket\u{2019}s own copy of the repo (a worktree)."
                .to_owned()
        }
        "grill" => {
            "An agent asks you questions until the ticket is clear, before any code is written."
                .to_owned()
        }
        "validate" => format!(
            "A checking agent runs the tests and lint. If something fails, the ticket goes back to {}.",
            back.unwrap_or("the stage before")
        ),
        "open-pr" => PR_CAPTION.to_owned(),
        _ => return None,
    })
}

/// A preset's line in Start from (`s27-03`), by its file.
fn preset_line(file: &str) -> &'static str {
    match file {
        "solo-autopilot.yaml" => "No review; merges when checks pass",
        "grill-first.yaml" => "Settle the ticket before code",
        "team-pr.yaml" => "Your team reviews the PR",
        _ => "Agents build and check; you review",
    }
}

/// A badge beside an entry's name: "In use" (approved colours) or "New" (a tool's proposal).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorBadge")]
pub struct Badge {
    pub label: &'static str,
    pub pill: PillView,
}

/// One entry of Start from. `FlowEditorEntry` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorEntry")]
pub struct Entry {
    /// The React key: `in-use`, the preset's file, `proposal-3`.
    pub key: String,
    /// "shop-web’s flow", "Otaq default", "By Claude Code".
    pub name: String,
    pub badge: Option<Badge>,
    /// Its steps, Ready for Dev counted.
    pub count: usize,
    pub line: String,
    /// What the draft holds now: outlined.
    pub chosen: bool,
    pub avatar: Option<ToolAvatarView>,
    pub pick: Pick,
}

/// A connected tool's row under Made for <project>. `FlowEditorToolRow` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorToolRow")]
pub struct ToolRow {
    #[ts(type = "string")]
    pub tool: Tool,
    pub avatar: ToolAvatarView,
    pub name: &'static str,
    /// "Connected", "Connected · not asked", "Working", "Plan limit", "Not signed in", "Ended without a proposal".
    pub status: String,
    /// "Ask"; `None` while it works or can't be asked.
    pub ask: Option<&'static str>,
}

/// Made for <project> (FED-03). `FlowEditorMadeFor` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorMadeFor")]
pub struct MadeFor {
    /// "MADE FOR SHOP-WEB".
    pub title: String,
    pub text: &'static str,
    /// Each finished proposal: "By Claude Code · From your Claude Code sessions".
    pub entries: Vec<Entry>,
    pub tools: Vec<ToolRow>,
}

/// Import a flow…: a file, or another project's flow. `FlowEditorImport` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorImport")]
pub struct Import {
    pub label: &'static str,
    /// `file`, then a header and `project:<KEY>` per other project.
    pub menu: Vec<MenuItem>,
    /// The open dialog's title.
    pub file_title: &'static str,
}

/// Start from (left). `FlowEditorStartFrom` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorStartFrom")]
pub struct StartFrom {
    pub title: &'static str,
    /// The flow in use (in the project), then the presets.
    pub entries: Vec<Entry>,
    pub made_for: MadeFor,
    pub import: Import,
    pub note: &'static str,
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

/// One tile of the animation and its caption. `FlowEditorTile` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorTile")]
pub struct Tile {
    pub id: String,
    pub name: String,
    pub icon: IconName,
    /// "Waits", "Agent", "Check", "You", "Delivery" (long flows); "An agent works"… (five steps or fewer).
    pub kind: String,
    /// What the walking ticket says there: "Waiting", "Working · Developer", "Your check", "PR open".
    pub ticket: String,
    pub tone: PillView,
    /// "Dev · an agent works".
    pub caption_title: String,
    /// "New step" for a step the agent added.
    pub caption_mark: Option<&'static str>,
    pub caption: Option<String>,
}

/// One step of the list. `FlowEditorStep` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorStep")]
pub struct StepRow {
    /// The stage's id; [`READY_ID`] for Ready for Dev.
    pub id: String,
    pub name: String,
    pub icon: IconName,
    /// "Waits", "Agent works", "Agent checks", "You check", "Delivery · PR".
    pub kind: String,
    /// The agent's avatar and name, "You" for a check by you, "PR" / "merge" for the Delivery.
    pub avatar: Option<ToolAvatarView>,
    pub who: Option<String>,
    pub mark: Option<MarkView>,
    /// A stage the flow in use doesn't have, added in the editor (a column's Add a stage after <stage>, **+**, Add a
    /// stage): drawn dashed until it applies (FED-09, `s27-05`). A stage the agent added has its mark instead.
    pub added: bool,
    /// Its skill the Project agent wrote: "New", tinted, with Undo (FED-06, `s27-06`).
    pub written: Option<MarkView>,
    /// A check's "fails go to Dev" under its kind (d0131).
    pub fails_to: Option<String>,
    /// Its problem from the check, in the error colour ("needs a skill").
    pub problem: Option<String>,
    pub selected: bool,
    /// Ready for Dev and the Delivery stay where they are.
    pub draggable: bool,
    /// Ready for Dev isn't a stage: no details.
    pub selectable: bool,
    /// The caption on hover.
    pub caption: Option<String>,
    /// What **+** below this step sends as `add`'s `after` (`None` for Ready for Dev: first). `None` after the
    /// Delivery, which nothing follows ([`StepRow::can_add_after`] false).
    pub add_after: Option<String>,
    pub can_add_after: bool,
}

/// One kind of the legend: "An agent works · writes code, asks you questions".
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorLegendRow")]
pub struct LegendRow {
    pub icon: IconName,
    pub name: &'static str,
    pub text: &'static str,
}

/// A segment of the Kind control.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorKindOption")]
pub struct KindOption {
    pub kind: EditorKind,
    pub label: &'static str,
}

/// The agent field: who runs it, the picker, and the hint.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorAgentField")]
pub struct AgentField {
    pub label: &'static str,
    pub avatar: Option<ToolAvatarView>,
    pub name: String,
    /// "default for An agent works", or the model ("gpt-5.5").
    pub note: Option<String>,
    /// Every agent: the stored ones, then those the draft brings ("new").
    pub items: Vec<MenuItem>,
    pub hint: &'static str,
}

/// One check agent of an agent check: its tool badge, name and model, and its remove (`s29-04`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorCheckAgent")]
pub struct CheckAgentRow {
    pub name: String,
    pub avatar: Option<ToolAvatarView>,
    pub model: Option<String>,
    /// "Remove Security reviewer" (the x's label).
    pub remove: String,
}

/// An agent check's Check agents (d0130): one row per agent, **Add a check agent** (the agents not listed yet) and the
/// note. `FlowEditorCheckAgents` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorCheckAgents")]
pub struct CheckAgentsField {
    pub label: &'static str,
    pub rows: Vec<CheckAgentRow>,
    pub add: &'static str,
    /// The stored agents, then the ones only the draft brings ("new"), but those already listed; empty: Add is off.
    pub add_items: Vec<MenuItem>,
    pub note: &'static str,
}

/// An agent check's Rounds: one limit for the stage, whichever agent fails (d0130). `FlowEditorRounds` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorRounds")]
pub struct RoundsField {
    pub label: &'static str,
    pub value: String,
    /// 1–5, the current one checked; the id is the number.
    pub items: Vec<MenuItem>,
    pub note: &'static str,
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
    /// The stage's skill (clicking it picks another).
    Set { skill: String },
    /// "No skill yet. An agent stage needs one: it’s what the agent follows in Plan.", with Pick a skill… and Ask the
    /// agent to write it, which puts `prefill` in the agent's box ([`write_skill_prefill`], Q441).
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
    /// The skill the agent wrote, tinted: "Written by the agent", Open, Undo.
    Written {
        skill: String,
        note: &'static str,
        /// "Written by Checkout slowness" (hover).
        why: String,
        open: &'static str,
        undo: &'static str,
    },
}

/// Pick a skill…: the user's imported skills (any agent's), searchable, each with its description as a second line;
/// the item's id is the skill's name. With none, one disabled line says so (Q-C: not drawn). `FlowEditorSkillPicker`
/// on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorSkillPicker")]
pub struct SkillPicker {
    pub title: &'static str,
    pub search: MenuSearch,
    pub items: Vec<MenuItem>,
}

/// The Skill field (FED-06).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorSkillField")]
pub struct SkillField {
    pub label: &'static str,
    pub state: SkillState,
    pub picker: SkillPicker,
}

/// One template of a new stage's Start from. `FlowEditorTemplateChip` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorTemplateChip")]
pub struct TemplateChip {
    pub template: Template,
    pub icon: IconName,
    pub label: &'static str,
    pub chosen: bool,
}

/// A new stage's Start from (FED-05, `s27-05`): "New stage after Grill", the plain templates and the presets' ones.
/// `FlowEditorTemplates` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorTemplates")]
pub struct Templates {
    /// "New stage after Grill".
    pub title: String,
    pub label: &'static str,
    /// "name and kind; no skill".
    pub plain_note: &'static str,
    pub plain: Vec<TemplateChip>,
    /// "From Otaq’s presets".
    pub presets_label: String,
    /// "complete, skill included".
    pub presets_note: &'static str,
    pub presets: Vec<TemplateChip>,
}

/// One line of "A ticket can".
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorCanRow")]
pub struct CanRow {
    /// `skip`, an agent's name, or `back` (fixed: the flow sends failed checks here).
    pub option: String,
    pub label: String,
    pub on: bool,
    pub enabled: bool,
}

/// The selected step's details (FED-04). `FlowEditorDetails` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorDetails")]
pub struct Details {
    pub stage: String,
    /// "Dev".
    pub title: String,
    pub icon: IconName,
    pub remove: Option<&'static str>,
    pub name_label: &'static str,
    pub name: String,
    pub kind_label: &'static str,
    /// The three kinds; empty for the Delivery (its kind is fixed, [`Details::fixed_kind`]).
    pub kinds: Vec<KindOption>,
    pub kind: Option<EditorKind>,
    pub fixed_kind: Option<&'static str>,
    /// A stage the user added: its templates (FED-05).
    pub templates: Option<Templates>,
    /// The agent of a stage one agent runs; an agent check has [`Details::check_agents`] instead.
    pub agent: Option<AgentField>,
    pub check_agents: Option<CheckAgentsField>,
    pub skill: Option<SkillField>,
    pub rounds: Option<RoundsField>,
    pub fail_to: Option<FailToField>,
    /// An agent check's rule (d0130).
    pub rule: Option<&'static str>,
    pub can_label: &'static str,
    pub can: Vec<CanRow>,
    /// Its problem that no pick fixes (a problem with a fix shows in [`Details::fixes`]).
    pub problem: Option<String>,
    /// The picks its problems and its agents' import clashes need (Q426).
    pub fixes: Vec<FixRow>,
    pub mark: Option<MarkView>,
}

/// The flow (middle). `FlowEditorFlow` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorFlow")]
pub struct FlowPart {
    /// "shop-web’s flow".
    pub title: String,
    /// "7 steps · in use", "8 steps · 1 change, not applied", "5 steps · starting from Otaq default".
    pub summary: String,
    pub watch: &'static str,
    /// A step's details are open: the animation folds to one line.
    pub folded: bool,
    pub fold_note: &'static str,
    /// The walking ticket's key, from the project's own key: "SHOP-1", "DEMO-1".
    pub ticket_key: String,
    pub tiles: Vec<Tile>,
    pub steps: Vec<StepRow>,
    pub add_stage: &'static str,
    /// What Add a stage sends as `after`: the stage before the Delivery.
    pub add_stage_after: Option<String>,
    pub hint: &'static str,
    pub legend_title: &'static str,
    pub legend: Vec<LegendRow>,
    pub details: Option<Details>,
    /// Problems about the whole flow or an agent no step runs (a stage's, and its agents', show on it).
    pub problems: Vec<String>,
    /// The picks of those (an agent no step runs: its clash, its model, its skill's folder).
    pub fixes: Vec<FixRow>,
    /// When the draft would reach tickets (FED-08); `None` with nothing to apply and in onboarding.
    pub change_line: Option<String>,
    /// "Undo all" while the agent's changes can be taken back.
    pub undo_all: Option<&'static str>,
}

/// A fixed chip in the agent panel: the words it sends, or the tool picker. `FlowEditorChipAction` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "FlowEditorChipAction")]
pub enum ChipAction {
    /// The chip's words are the first message (asking).
    Send { message: String },
    /// "Make one for shop-web…": which tool should look.
    ChooseTool,
    /// "Ask Claude Code": the only connected tool.
    Ask {
        #[ts(type = "string")]
        tool: Tool,
    },
    /// "Write the skill": as the stage's Ask the agent to write it (FED-06): `prefill` goes in the box, nothing is sent
    /// (Q441).
    WriteSkill { stage: String, prefill: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorChip")]
pub struct Chip {
    pub label: String,
    pub action: ChipAction,
}

/// "Which tool should read how you work?" (`s27-03`). `FlowEditorChooser` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorChooser")]
pub struct Chooser {
    pub title: &'static str,
    pub text: &'static str,
    pub tools: Vec<ChooserTool>,
    /// "Both, and compare · Two flows under Made for shop-web"; `None` in the project (Q-A).
    pub both: Option<(String, String)>,
    pub foot: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorChooserTool")]
pub struct ChooserTool {
    #[ts(type = "string")]
    pub tool: Tool,
    pub avatar: ToolAvatarView,
    pub name: &'static str,
    pub detail: &'static str,
}

/// The agent panel (FED-07). `FlowEditorAgent` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorAgent")]
pub struct AgentPanel {
    pub title: &'static str,
    pub pill: PillView,
    pub avatar: Option<ToolAvatarView>,
    /// "Claude Code · sonnet".
    pub line: String,
    /// "· starts when you ask", "· 12k".
    pub note: String,
    /// The running agent's id; `None` before anything was asked.
    pub agent: Option<String>,
    /// Its session, which the terminal attaches to.
    pub session: Option<String>,
    /// Otaq's own hint (paragraphs), before anything is asked.
    pub hint: Vec<String>,
    pub chips: Vec<Chip>,
    pub chooser: Option<Chooser>,
    pub foot: &'static str,
    pub keys: Option<String>,
    pub keys_label: &'static str,
}

/// The editor's buttons. `FlowEditorFooter` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorFooter")]
pub struct Footer {
    /// "Cancel" (drops the draft), "Skip, use the default" (onboarding).
    pub cancel: &'static str,
    /// "No changes", "Apply to shop-web", "Use this flow".
    pub apply: String,
    pub apply_enabled: bool,
    /// Why Apply is off: the first problem.
    pub reason: Option<String>,
}

/// Everything the editor draws. `FlowEditorView` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorView")]
pub struct View {
    pub project: String,
    pub from: Opened,
    /// The draft's proposal (Apply takes it); `0` while it is the flow as it will be.
    pub proposal: u32,
    pub start_from: StartFrom,
    pub flow: FlowPart,
    pub agent: AgentPanel,
    pub footer: Footer,
    /// The picks as Apply sends them: every clash with its pick (Keep mine by default), only the folders and models
    /// still asked for.
    pub choices: Choices,
}

/// What [`view`] reads.
pub struct Facts<'a> {
    pub project: &'a str,
    pub project_name: &'a str,
    pub opened: Opened,
    /// The draft (`flow.draft`).
    pub proposal: &'a Proposal,
    pub marks: &'a [StageMark],
    pub can_undo_all: bool,
    pub swap: bool,
    pub applies: &'a DraftApplies,
    /// The draft's document, read.
    pub file: &'a FlowFile,
    /// The flow as it will be (the In use entry's count).
    pub in_use: &'a FlowFile,
    /// Every stored agent.
    pub stored: &'a [Agent],
    pub tools: &'a Tools,
    /// The project's tools and open proposals (`onboarding.flow_setup`).
    pub setup: &'a FlowSetup,
    /// The other projects (key, name), for Import a flow… → from a project.
    pub projects: &'a [(String, String)],
    /// Imported skills' descriptions, by the skill's name.
    pub captions: &'a BTreeMap<String, String>,
    /// The stages whose skill the Project agent is writing, and the skills it wrote (the draft's).
    pub writing: &'a [SkillWriting],
    pub written: &'a [WrittenSkill],
    pub selected: Option<&'a str>,
    /// The panel's agent, once one was asked.
    pub agent: Option<&'a ProjectAgent>,
    /// The Project agent role's agent: what the first message starts.
    pub new_agent: Option<&'a Agent>,
    pub keys: &'a KeyLabels,
    /// The user's picks so far (the page's, until Apply).
    pub choices: &'a Choices,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The step count Start from and the summary show: the stages and Ready for Dev.
fn steps(file: &FlowFile) -> usize {
    file.flow.stages.len() + 1
}

fn kind_icon(stage: &Stage) -> IconName {
    match stage.kind {
        StageKind::Agent => IconName::Bot,
        StageKind::Check { by: CheckBy::Agent } => IconName::ShieldCheck,
        StageKind::Check { by: CheckBy::You } => IconName::UserCheck,
        StageKind::Delivery => match stage.delivery.map(|d| d.delivers) {
            Some(Delivers::Pr) => IconName::GitPullRequest,
            _ => IconName::GitMerge,
        },
    }
}

fn editor_kind(stage: &Stage) -> Option<EditorKind> {
    match stage.kind {
        StageKind::Agent => Some(EditorKind::Works),
        StageKind::Check { by: CheckBy::Agent } => Some(EditorKind::AgentChecks),
        StageKind::Check { by: CheckBy::You } => Some(EditorKind::YouCheck),
        StageKind::Delivery => None,
    }
}

fn kind_label(kind: EditorKind) -> &'static str {
    match kind {
        EditorKind::Works => "An agent works",
        EditorKind::AgentChecks => "An agent checks",
        EditorKind::YouCheck => "You check",
    }
}

/// "PR" or "merge": what the Delivery does.
fn delivers_word(stage: &Stage) -> &'static str {
    match stage.delivery.map(|d| d.delivers) {
        Some(Delivers::Pr) => "PR",
        _ => "merge",
    }
}

/// The list's kind word: "Agent works", "Agent checks", "You check", "Delivery · PR".
fn list_kind(stage: &Stage) -> String {
    match stage.kind {
        StageKind::Agent => "Agent works".to_owned(),
        StageKind::Check { by: CheckBy::Agent } => "Agent checks".to_owned(),
        StageKind::Check { by: CheckBy::You } => "You check".to_owned(),
        StageKind::Delivery => format!("Delivery \u{b7} {}", delivers_word(stage)),
    }
}

/// The caption title's kind: "an agent works", "you check", "delivery".
fn caption_kind(stage: &Stage) -> &'static str {
    match stage.kind {
        StageKind::Agent => "an agent works",
        StageKind::Check { by: CheckBy::Agent } => "an agent checks",
        StageKind::Check { by: CheckBy::You } => "you check",
        StageKind::Delivery => "delivery",
    }
}

/// A step's caption (d0115): its skill's description, Otaq's fixed caption for a check by you and the Delivery.
fn caption(file: &FlowFile, stage: &Stage, captions: &BTreeMap<String, String>) -> Option<String> {
    match stage.kind {
        StageKind::Check { by: CheckBy::You } => Some(YOU_CHECK_CAPTION.to_owned()),
        StageKind::Delivery => Some(
            match stage.delivery.map(|d| d.delivers) {
                Some(Delivers::Pr) => PR_CAPTION,
                _ => MERGE_CAPTION,
            }
            .to_owned(),
        ),
        _ => {
            let skill = stage.skill.as_deref()?;
            captions.get(skill).cloned().or_else(|| {
                let back = file.flow.fail_to_of(stage).map(|s| s.name.as_str());
                shipped_caption(skill, back)
            })
        }
    }
}

/// The agent a stage names, as stored (its tool for the avatar).
fn stored_agent<'a>(stored: &'a [Agent], name: &str) -> Option<&'a Agent> {
    stored.iter().find(|a| a.name.eq_ignore_ascii_case(name))
}

/// The avatar and model of the agent `name`: a stored one's tool and model; one the draft brings has none yet.
fn agent_on(name: &str, f: &Facts) -> (Option<ToolAvatarView>, Option<String>) {
    let Some(agent) = stored_agent(f.stored, name) else {
        return (None, None);
    };
    match agent.model.on(f.tools.default_model.as_ref()) {
        Some(on) => (
            Some(ToolAvatarView::of(on.tool, on.model.as_deref())),
            on.model,
        ),
        None => (None, None),
    }
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

fn mark_view(mark: &StageMark) -> MarkView {
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

fn mark_of<'m>(marks: &'m [StageMark], id: &str) -> Option<&'m StageMark> {
    marks
        .iter()
        .find(|m| m.stage_id == id && m.mark != Mark::Removed)
}

/// The stage's one agent: an Agent stage's or a Delivery's, an agent check's first check agent (a kind change keeps
/// it; its options replace it).
fn the_agent(stage: &Stage) -> Option<&String> {
    stage.named_agents().next()
}

/// "Security reviewer", or "2 check agents" for an agent check with several (`s29-04`).
fn who_runs(stage: &Stage) -> Option<String> {
    match stage.check_agents() {
        several @ [_, _, ..] => Some(format!("{} check agents", several.len())),
        _ => the_agent(stage).cloned(),
    }
}

/// Sets the agent [`the_agent`] reads: an agent check's first check agent, else the stage's agent.
fn set_agent(stage: &mut Stage, agent: Option<String>) {
    if stage.is_agent_check() {
        stage.agent = None;
        match (agent, stage.agents.is_empty()) {
            (Some(name), true) => stage.agents.push(name),
            (Some(name), false) => stage.agents[0] = name,
            (None, _) => stage.agents.clear(),
        }
    } else {
        stage.agents.clear();
        stage.agent = agent;
    }
}

/// The stages that run the agent `name`, or offer it as an option: where its problems and its clash show (Q426).
fn agent_stages<'a>(file: &'a FlowFile, name: &'a str) -> impl Iterator<Item = &'a Stage> + 'a {
    file.flow
        .stages
        .iter()
        .filter(move |s| s.every_agent().any(|a| a.eq_ignore_ascii_case(name)))
}

/// A problem shows on `stage`: it is about the stage, or about an agent the stage runs or offers.
fn shows_on(file: &FlowFile, problem: &ProposalProblem, stage: &Stage) -> bool {
    match &problem.about {
        About::Stage { id, .. } => *id == stage.id,
        About::Agent { name } => agent_stages(file, name).any(|s| s.id == stage.id),
        About::Flow => false,
    }
}

/// A problem shows on a step (else under the flow).
fn on_a_step(file: &FlowFile, problem: &ProposalProblem) -> bool {
    match &problem.about {
        About::Stage { .. } => true,
        About::Agent { name } => agent_stages(file, name).next().is_some(),
        About::Flow => false,
    }
}

/// "Security reviewer: Claude Code isn’t connected", "Security reviewer: opus isn’t in Claude Code’s model list"
/// (FLW-04's words); the check's own message otherwise.
fn model_problem(f: &Facts, agent: &str, message: &str) -> String {
    let model = f
        .file
        .agents
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(agent))
        .map(|a| shipped::model_of(&a.model));
    match model {
        Some(Ok(AgentModel::Tool { tool, .. })) if !f.tools.connected.contains(&tool) => {
            format!("{agent}: {} isn\u{2019}t connected", tool.display_name())
        }
        Some(Ok(AgentModel::Tool {
            tool,
            model: Some(model),
        })) => format!(
            "{agent}: {model} isn\u{2019}t in {}\u{2019}s model list",
            tool.display_name()
        ),
        _ => message.to_owned(),
    }
}

/// What a problem says on its step and as Apply's reason: a fix's own words, else the check's message.
fn problem_text(f: &Facts, problem: &ProposalProblem) -> String {
    match &problem.fix {
        // A folder that can't be imported keeps the check's reason.
        Some(Fix::ImportSkillFolder { skill })
            if !problem.message.contains("can't be imported") =>
        {
            format!("{skill} isn\u{2019}t on this machine")
        }
        Some(Fix::PickModel { agent }) => model_problem(f, agent, &problem.message),
        _ => problem.message.clone(),
    }
}

/// The first problem Apply still waits for: none picked for it.
fn left<'a>(f: &Facts<'a>) -> Option<&'a ProposalProblem> {
    f.proposal.problems.iter().find(|p| !f.choices.fix(p))
}

/// A step's problem (the first one no pick fixed yet), in the error colour; "needs a skill" for an agent stage with
/// none.
fn step_problem(f: &Facts, stage: &Stage) -> Option<String> {
    if stage.needs_skill() {
        return Some(NEEDS_A_SKILL.to_owned());
    }
    f.proposal
        .problems
        .iter()
        .find(|p| shows_on(f.file, p, stage) && !f.choices.fix(p))
        .map(|p| problem_text(f, p))
}

/// Where `tool`'s model list stands for the picker.
fn list_state(tools: &Tools, tool: Tool) -> ListState<'_> {
    match tools.list(tool) {
        Some(list) => ListState::Listed(list),
        None if tools.connected.contains(&tool) => ListState::Loading,
        None => ListState::Unread,
    }
}

fn model_fix(f: &Facts, agent: &str) -> ModelFix {
    let picked = f
        .choices
        .models
        .get(agent)
        .and_then(|m| shipped::model_of(m).ok());
    let tool = match &picked {
        Some(AgentModel::Tool { tool, .. }) => Some(*tool),
        _ => None,
    };
    let mut tools = vec![Choice {
        id: DEFAULT.to_owned(),
        label: DEFAULT_MODEL.to_owned(),
        selected: picked == Some(AgentModel::Default),
    }];
    tools.extend(
        Tool::ALL
            .iter()
            .filter(|t| f.tools.connected.contains(t))
            .map(|t| Choice {
                id: t.as_str().to_owned(),
                label: t.display_name().to_owned(),
                selected: tool == Some(*t),
            }),
    );
    let picker = tool.map(|tool| {
        let model = match &picked {
            Some(AgentModel::Tool { model, .. }) => model.as_deref(),
            _ => None,
        };
        model_picker::picker(tool, list_state(f.tools, tool), model, false, None)
    });
    ModelFix {
        agent: agent.to_owned(),
        tool_label: "Tool",
        tools,
        picker,
        model_label: "Model",
    }
}

/// "Rust developer, Security reviewer": `words` joined with "and" before the last.
fn joined_words(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// An import clash's row (second gap review rule 3): Keep mine (the default) · Take theirs · Add as "<name> (<file>)".
fn clash_row(f: &Facts, changed: &AgentChange) -> Option<FixRow> {
    let clash = changed.clash.as_ref()?;
    let place = match &f.proposal.source {
        Source::Preset { .. } => "the preset",
        Source::AgentsFile => "the shipped agents file",
        _ => "the file",
    };
    let fields: Vec<&str> = changed.fields.iter().map(|f| f.field.as_str()).collect();
    let pick = f
        .choices
        .clashes
        .get(&changed.name)
        .copied()
        .unwrap_or_default();
    let mut options = vec![
        Choice {
            id: "keep_mine".to_owned(),
            label: "Keep mine".to_owned(),
            selected: pick == ClashPick::KeepMine,
        },
        Choice {
            id: "take_theirs".to_owned(),
            label: "Take theirs".to_owned(),
            selected: pick == ClashPick::TakeTheirs,
        },
    ];
    if let Some(add_as) = &clash.add_as {
        options.push(Choice {
            id: "add_as".to_owned(),
            label: format!("Add as \u{201c}{add_as}\u{201d}"),
            selected: pick == ClashPick::AddAs,
        });
    }
    Some(FixRow {
        text: format!(
            "{} is in {place} and already here, with other {}.",
            changed.name,
            joined_words(&fields)
        ),
        fixed: true,
        control: FixControl::Clash {
            agent: changed.name.clone(),
            options,
        },
    })
}

/// The import clashes of the draft (agents it changes that it brings from a file or a preset).
fn clashes<'a>(f: &'a Facts) -> impl Iterator<Item = &'a AgentChange> + 'a {
    f.proposal
        .diff
        .agents
        .iter()
        .filter(|a| a.change == AgentChangeKind::Changed && a.clash.is_some())
}

/// The fixes of `problems` (a skill both a stage and its agent name is one fix), then the clashes of `agents`.
fn fix_rows<'a>(
    f: &Facts,
    problems: impl Iterator<Item = &'a ProposalProblem>,
    agents: impl Iterator<Item = &'a AgentChange>,
) -> Vec<FixRow> {
    let mut asked = HashSet::new();
    let mut rows: Vec<FixRow> = problems
        .filter_map(|p| {
            let fixed = f.choices.fix(p);
            match p.fix.as_ref()? {
                Fix::ImportSkillFolder { skill } => asked
                    .insert(format!("skill:{}", skill.to_lowercase()))
                    .then(|| FixRow {
                        text: problem_text(f, p),
                        fixed,
                        control: FixControl::Folder {
                            skill: skill.clone(),
                            label: "Import its folder\u{2026}",
                            picker_title: format!("The folder of {skill}"),
                            picked: f.choices.skill_folders.get(skill).cloned(),
                        },
                    }),
                Fix::PickModel { agent } => asked
                    .insert(format!("model:{}", agent.to_lowercase()))
                    .then(|| FixRow {
                        text: problem_text(f, p),
                        fixed,
                        control: FixControl::Model {
                            fix: model_fix(f, agent),
                        },
                    }),
            }
        })
        .collect();
    rows.extend(agents.filter_map(|a| clash_row(f, a)));
    rows
}

/// A step's fixes: its problems' and those of the agents it runs or offers, then their clashes.
fn stage_fixes(f: &Facts, stage: &Stage) -> Vec<FixRow> {
    fix_rows(
        f,
        f.proposal
            .problems
            .iter()
            .filter(|p| shows_on(f.file, p, stage)),
        clashes(f).filter(|a| agent_stages(f.file, &a.name).any(|s| s.id == stage.id)),
    )
}

/// The picks as Apply sends them: each clash's pick (Keep mine by default); a folder or a model only where one is
/// still asked for.
fn choices_kept(f: &Facts) -> Choices {
    let picks = f.choices;
    let mut kept = Choices::default();
    for a in clashes(f) {
        kept.clashes.insert(
            a.name.clone(),
            picks.clashes.get(&a.name).copied().unwrap_or_default(),
        );
    }
    for p in &f.proposal.problems {
        match &p.fix {
            Some(Fix::ImportSkillFolder { skill }) => {
                if let Some(folder) = picks.skill_folders.get(skill) {
                    kept.skill_folders.insert(skill.clone(), folder.clone());
                }
            }
            Some(Fix::PickModel { agent }) => {
                if let Some(model) = picks.models.get(agent) {
                    kept.models.insert(agent.clone(), model.clone());
                }
            }
            None => {}
        }
    }
    kept
}

/// What the walking ticket says in a stage, and its colour.
fn ticket_state(stage: &Stage, long: bool) -> (String, PillView) {
    let who = |word: &str| match (long, who_runs(stage)) {
        (true, Some(agent)) => format!("{word} \u{b7} {agent}"),
        _ => word.to_owned(),
    };
    match stage.kind {
        StageKind::Agent if stage.skill.as_deref() == Some("grill") => (
            "Asks you a question".to_owned(),
            PillView::legend(Legend::AsksYou, None),
        ),
        StageKind::Agent => (who("Working"), PillView::legend(Legend::AgentWorking, None)),
        StageKind::Check { by: CheckBy::Agent } => (
            who("Checking"),
            PillView::legend(Legend::AgentWorking, None),
        ),
        StageKind::Check { by: CheckBy::You } => (
            if long {
                "Your check \u{b7} You".to_owned()
            } else {
                "Your check".to_owned()
            },
            PillView::legend(Legend::ReadyForReview, None),
        ),
        StageKind::Delivery => (
            match stage.delivery.map(|d| d.delivers) {
                Some(Delivers::Pr) => "PR open",
                _ => "Merged",
            }
            .to_owned(),
            PillView::legend(Legend::Approved, None),
        ),
    }
}

/// The animation's tile kind: long words while the flow has five steps or fewer (`s27-03`), short ones beyond.
fn tile_kind(stage: &Stage, long: bool) -> String {
    match (stage.kind, long) {
        (StageKind::Agent, true) => "An agent works",
        (StageKind::Agent, false) => "Agent",
        (StageKind::Check { by: CheckBy::Agent }, true) => "An agent checks",
        (StageKind::Check { by: CheckBy::Agent }, false) => "Check",
        (StageKind::Check { by: CheckBy::You }, true) => "You check",
        (StageKind::Check { by: CheckBy::You }, false) => "You",
        (StageKind::Delivery, _) => "Delivery",
    }
    .to_owned()
}

fn tiles(f: &Facts) -> Vec<Tile> {
    let long = steps(f.file) <= 5;
    let ready = Tile {
        id: READY_ID.to_owned(),
        name: if long { "Ready for Dev" } else { "Ready" }.to_owned(),
        icon: IconName::Hourglass,
        kind: "Waits".to_owned(),
        ticket: "Waiting".to_owned(),
        tone: PillView::legend(Legend::Queued, None),
        caption_title: "Ready for Dev \u{b7} waits".to_owned(),
        caption_mark: None,
        caption: Some(READY_CAPTION.to_owned()),
    };
    let stages = f.file.flow.stages.iter().map(|stage| {
        let (ticket, tone) = ticket_state(stage, long);
        let added =
            mark_of(f.marks, &stage.id).is_some_and(|m| matches!(m.mark, Mark::Added | Mark::New));
        Tile {
            id: stage.id.clone(),
            name: stage.name.clone(),
            icon: kind_icon(stage),
            kind: tile_kind(stage, long),
            ticket,
            tone,
            caption_title: format!("{} \u{b7} {}", stage.name, caption_kind(stage)),
            caption_mark: added.then_some("New step \u{b7}"),
            caption: caption(f.file, stage, f.captions),
        }
    });
    std::iter::once(ready).chain(stages).collect()
}

/// Where **+** after the step at `index` (of the stages) adds, and whether it can: never after the Delivery.
fn steps_list(f: &Facts) -> Vec<StepRow> {
    let stages = &f.file.flow.stages;
    let ready = StepRow {
        id: READY_ID.to_owned(),
        name: "Ready for Dev".to_owned(),
        icon: IconName::Hourglass,
        kind: "Waits".to_owned(),
        avatar: None,
        who: None,
        mark: None,
        added: false,
        written: None,
        fails_to: None,
        problem: None,
        selected: false,
        draggable: false,
        selectable: false,
        caption: Some(READY_CAPTION.to_owned()),
        add_after: None,
        can_add_after: !stages.first().is_some_and(Stage::is_delivery),
    };
    let rows = stages.iter().map(|stage| {
        let (avatar, who) = match stage.kind {
            StageKind::Check { by: CheckBy::You } => (None, Some("You".to_owned())),
            StageKind::Delivery => (None, Some(delivers_word(stage).to_owned())),
            // "needs a skill" takes the agent's place (`s27-05`).
            _ if stage.needs_skill() => (None, None),
            _ if stage.check_agents().len() > 1 => (None, who_runs(stage)),
            _ => match the_agent(stage) {
                Some(name) => (agent_on(name, f).0, Some(name.clone())),
                None => (None, None),
            },
        };
        let problem = step_problem(f, stage);
        StepRow {
            id: stage.id.clone(),
            name: stage.name.clone(),
            icon: kind_icon(stage),
            kind: list_kind(stage),
            avatar,
            who,
            mark: mark_of(f.marks, &stage.id).map(mark_view),
            added: mark_of(f.marks, &stage.id).is_none()
                && f.in_use.flow.stage(&stage.id).is_none(),
            written: written_of(f, stage).map(written_view),
            fails_to: fails_to(&f.file.flow, stage).map(|t| format!("fails go to {}", t.name)),
            problem,
            selected: f.selected == Some(stage.id.as_str()),
            draggable: !stage.is_delivery(),
            selectable: true,
            caption: caption(f.file, stage, f.captions),
            add_after: Some(stage.id.clone()),
            can_add_after: !stage.is_delivery(),
        }
    });
    std::iter::once(ready).chain(rows).collect()
}

fn legend() -> Vec<LegendRow> {
    vec![
        LegendRow {
            icon: IconName::Bot,
            name: "An agent works",
            text: "writes code, asks you questions",
        },
        LegendRow {
            icon: IconName::ShieldCheck,
            name: "An agent checks",
            text: "tests, security; sends it back",
        },
        LegendRow {
            icon: IconName::UserCheck,
            name: "You check",
            text: "approve or send back",
        },
        LegendRow {
            icon: IconName::GitMerge,
            name: "Delivery",
            text: "merge, or open a PR",
        },
    ]
}

/// The agents the field offers: the stored ones, then the ones only the draft brings ("new").
fn agent_items(f: &Facts, current: &str) -> Vec<MenuItem> {
    let stored = f.stored.iter().map(|a| {
        MenuItem::new(a.name.clone(), a.name.clone()).checked(a.name.eq_ignore_ascii_case(current))
    });
    let brought = f
        .file
        .agents
        .iter()
        .filter(|a: &&FlowAgent| stored_agent(f.stored, &a.name).is_none())
        .map(|a| {
            MenuItem::new(a.name.clone(), a.name.clone())
                .hint("new")
                .checked(a.name.eq_ignore_ascii_case(current))
        });
    stored.chain(brought).collect()
}

fn agent_field(f: &Facts, stage: &Stage) -> Option<AgentField> {
    if stage.is_agent_check() {
        return None;
    }
    let name = the_agent(stage)?;
    let (avatar, model) = agent_on(name, f);
    let default = match stage.kind {
        StageKind::Agent => Some((WORKS_AGENT, "An agent works")),
        _ => None,
    };
    let note = match default {
        Some((agent, kind)) if agent.eq_ignore_ascii_case(name) => {
            Some(format!("default for {kind}"))
        }
        _ => model,
    };
    Some(AgentField {
        label: "Agent",
        avatar,
        name: name.to_owned(),
        note,
        items: agent_items(f, name),
        hint: "Change it here, or manage agents in Settings \u{2192} Agents.",
    })
}

/// "A ticket can": skipping it, each other agent it may use, and (fixed) coming back here when a later check fails.
fn can_rows(f: &Facts, stage: &Stage) -> Vec<CanRow> {
    let mut rows = Vec::new();
    let runs_agent = the_agent(stage).is_some() && !stage.is_delivery();
    if runs_agent {
        let current = the_agent(stage).map(String::as_str).unwrap_or_default();
        let named: Vec<String> = f
            .stored
            .iter()
            .map(|a| a.name.clone())
            .chain(f.file.agents.iter().map(|a| a.name.clone()))
            .collect();
        let mut seen: Vec<String> = Vec::new();
        for name in named {
            if name.eq_ignore_ascii_case(current)
                || seen.iter().any(|s| s.eq_ignore_ascii_case(&name))
            {
                continue;
            }
            let on = stage
                .options
                .iter()
                .any(|o| matches!(o, StageOption::Agent(a) if a.eq_ignore_ascii_case(&name)));
            // The ones it allows, and those of the same kind's default it could.
            if on {
                rows.push(CanRow {
                    label: format!("use {name} instead"),
                    option: name.clone(),
                    on,
                    enabled: true,
                });
            }
            seen.push(name);
        }
    }
    if !stage.is_delivery() {
        rows.push(CanRow {
            option: StageOption::SKIP.to_owned(),
            label: "skip it".to_owned(),
            on: stage.options.contains(&StageOption::Skip),
            enabled: true,
        });
    }
    if stage.is_agent_stage() {
        let later = f.file.flow.position(&stage.id).map_or(0, |p| p + 1);
        let back = f.file.flow.stages[later.min(f.file.flow.stages.len())..]
            .iter()
            .filter(|s| matches!(s.kind, StageKind::Check { .. }))
            .any(|s| f.file.flow.fail_to_of(s).is_some_and(|b| b.id == stage.id));
        if back {
            rows.push(CanRow {
                option: "back".to_owned(),
                label: "come back here when a later check fails".to_owned(),
                on: true,
                enabled: false,
            });
        }
    }
    rows
}

/// Where a check's fails go, as the step and its field show it (d0131): the stage its `fail_to` names (even one that
/// doesn't fit: its problem says why), else the nearest Agent stage before it. `None` for a stage that isn't a check.
fn fails_to<'a>(flow: &'a Flow, stage: &Stage) -> Option<&'a Stage> {
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

/// An agent check's rule (d0130, `s29-04`).
pub const CHECK_AGENTS_RULE: &str = "Passes when every check agent passes. If one fails, the card goes back once all \
     have given their verdict, with every report together; the next round runs them all again.";

/// A check's Fails go to: the Agent stages before it (d0131); `None` with none before it (the flow check says so).
fn fail_to_field(f: &Facts, stage: &Stage) -> Option<FailToField> {
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

/// An agent check's Check agents (d0130).
fn check_agents_field(f: &Facts, stage: &Stage) -> Option<CheckAgentsField> {
    if !stage.is_agent_check() {
        return None;
    }
    let listed = stage.check_agents();
    let rows = listed
        .iter()
        .map(|name| {
            let (avatar, model) = agent_on(name, f);
            CheckAgentRow {
                name: name.clone(),
                avatar,
                model,
                remove: format!("Remove {name}"),
            }
        })
        .collect();
    let add_items = agent_items(f, "")
        .into_iter()
        .filter(|i| !listed.iter().any(|a| a.eq_ignore_ascii_case(&i.id)))
        .collect();
    Some(CheckAgentsField {
        label: "Check agents",
        rows,
        add: "Add a check agent",
        add_items,
        note: "They run side by side, each read-only.",
    })
}

/// An agent check's Rounds (d0063, d0130).
fn rounds_field(stage: &Stage) -> Option<RoundsField> {
    if !stage.is_agent_check() {
        return None;
    }
    let rounds = stage.rounds.unwrap_or(CHECK_ROUNDS);
    Some(RoundsField {
        label: "Rounds",
        value: rounds.to_string(),
        items: VALIDATION_ROUNDS
            .map(|n| MenuItem::new(n.to_string(), n.to_string()).checked(n == rounds))
            .collect(),
        note: "for the stage, whichever fails",
    })
}

/// A step's "needs a skill" (`s27-05`).
pub const NEEDS_A_SKILL: &str = "needs a skill";

/// Whether the Project agent is writing the skill of the stage `id`.
fn writing(f: &Facts, id: &str) -> bool {
    f.writing.iter().any(|w| w.stage_id == id)
}

/// The skill the Project agent wrote for `stage`, while the stage still runs it.
fn written_of<'f>(f: &'f Facts, stage: &Stage) -> Option<&'f WrittenSkill> {
    f.written
        .iter()
        .find(|w| w.stage_id == stage.id && stage.skill.as_deref() == Some(w.skill.as_str()))
}

fn written_view(written: &WrittenSkill) -> MarkView {
    MarkView {
        label: "New",
        why: format!("Skill written by {}", written.by),
        undo: "Undo",
    }
}

/// What Ask the agent to write it and the Write the skill chip put in the agent's box (FED-06, Q441): "Write a skill
/// for the Plan stage: ", the caret after it. The user adds what the stage should do and sends; nothing is sent
/// before.
pub fn write_skill_prefill(file: &FlowFile, stage: &str) -> String {
    let name = file.flow.stage(stage).map_or(stage, |s| s.name.as_str());
    format!("Write a skill for the {name} stage: ")
}

/// The user's imported skills, each once, by name (Pick a skill…).
fn skill_choices(f: &Facts, stage: &Stage) -> Vec<MenuItem> {
    let mut names: Vec<&str> = f
        .stored
        .iter()
        .flat_map(|a| &a.imported)
        .map(|i| i.name.as_str())
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    if names.is_empty() {
        return vec![
            MenuItem::new(
                "",
                "No skills of yours yet. Import one in Settings \u{2192} Agents.",
            )
            .disabled(true),
        ];
    }
    names
        .into_iter()
        .map(|name| {
            let item = MenuItem::new(name, name).checked(
                stage
                    .skill
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(name)),
            );
            match f.captions.get(name) {
                Some(caption) => item.hint(caption.clone()),
                None => item,
            }
        })
        .collect()
}

/// The Skill field of a stage an agent runs (FED-06): set, empty with Pick and Ask, being written, or written.
fn skill_field(f: &Facts, stage: &Stage) -> Option<SkillField> {
    if the_agent(stage).is_none() && !stage.is_agent_check() {
        return None;
    }
    let state = if let Some(written) = written_of(f, stage) {
        SkillState::Written {
            skill: written.skill.clone(),
            note: "Written by the agent",
            why: format!("Written by {}", written.by),
            open: "Open",
            undo: "Undo",
        }
    } else if stage.skill.is_none() && writing(f, &stage.id) {
        SkillState::Writing {
            text: "Being written by Noyan\u{2026}",
            stop: "Stop",
        }
    } else if let Some(skill) = &stage.skill {
        SkillState::Set {
            skill: skill.clone(),
        }
    } else {
        SkillState::Empty {
            text: format!(
                "No skill yet. An agent stage needs one: it\u{2019}s what the agent follows in {}.",
                stage.name
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
        },
    })
}

/// A new stage's Start from (FED-05): for a stage the user added in this draft (not in the flow in use, not the
/// agent's, not a swap), but the Delivery.
fn templates(f: &Facts, stage: &Stage) -> Option<Templates> {
    let added = f.in_use.flow.stage(&stage.id).is_none()
        && !f.swap
        && mark_of(f.marks, &stage.id).is_none()
        && !stage.is_delivery();
    if !added {
        return None;
    }
    let at = f.file.flow.position(&stage.id)?;
    let after = at
        .checked_sub(1)
        .map_or("Ready for Dev", |i| f.file.flow.stages[i].name.as_str());
    let kind = editor_kind(stage);
    let chosen = |template: Template| match template.shape() {
        Some(_) if template == Template::Blank => stage.name.starts_with(NEW_STAGE),
        Some((name, k)) => stage.name == name && kind == Some(k),
        None => template.preset_stage().is_some_and(|(p, _)| {
            stage.name.eq_ignore_ascii_case(&p.name)
                && stage.kind == p.kind
                && stage.skill == p.skill
        }),
    };
    let chip = |template: Template| TemplateChip {
        template,
        icon: template.icon(),
        label: template.label(),
        chosen: chosen(template),
    };
    Some(Templates {
        title: format!("New stage after {after}"),
        label: "Start from",
        plain_note: "name and kind; no skill",
        plain: Template::PLAIN.into_iter().map(chip).collect(),
        presets_label: format!("From {}\u{2019}s presets", otaq_core::naming::PRODUCT_NAME),
        presets_note: "complete, skill included",
        presets: Template::FROM_PRESETS.into_iter().map(chip).collect(),
    })
}

fn details(f: &Facts, stage: &Stage) -> Details {
    let kind = editor_kind(stage);
    let kinds = match kind {
        Some(_) => [
            EditorKind::Works,
            EditorKind::AgentChecks,
            EditorKind::YouCheck,
        ]
        .into_iter()
        .map(|k| KindOption {
            kind: k,
            label: kind_label(k),
        })
        .collect(),
        None => Vec::new(),
    };
    let fail_to = fail_to_field(f, stage);
    Details {
        stage: stage.id.clone(),
        title: stage.name.clone(),
        icon: kind_icon(stage),
        remove: (!stage.is_delivery()).then_some("Remove stage"),
        name_label: "Name",
        name: stage.name.clone(),
        kind_label: "Kind",
        kinds,
        kind,
        fixed_kind: stage.is_delivery().then_some("Delivery"),
        templates: templates(f, stage),
        agent: agent_field(f, stage),
        check_agents: check_agents_field(f, stage),
        skill: skill_field(f, stage),
        rounds: rounds_field(stage),
        problem: f
            .proposal
            .problems
            .iter()
            .filter(|p| p.fix.is_none() && shows_on(f.file, p, stage))
            // Fails go to shows its own.
            .find(|p| fail_to.as_ref().and_then(|t| t.problem.as_ref()) != Some(&p.message))
            .map(|p| p.message.clone()),
        fail_to,
        rule: stage.is_agent_check().then_some(CHECK_AGENTS_RULE),
        can_label: "A ticket can",
        can: can_rows(f, stage),
        fixes: stage_fixes(f, stage),
        mark: mark_of(f.marks, &stage.id).map(mark_view),
    }
}

/// The source's name in onboarding's summary: "Otaq default", "Grill first", "By Claude Code".
fn source_name(f: &Facts) -> String {
    if let Some(tool) = f.proposal.proposed_by.tool().filter(|_| f.swap) {
        return format!("By {}", tool.display_name());
    }
    match &f.proposal.source {
        Source::Preset { file } => presets()
            .iter()
            .find(|p| p.file == file)
            .map_or_else(|| file.clone(), |p| p.name.clone()),
        Source::Project { name, .. } => format!("{name}\u{2019}s flow"),
        Source::Document { file: Some(file) } => file.clone(),
        _ => presets()[0].name.clone(),
    }
}

/// The stages the draft changes (each once).
fn changed_stages(proposal: &Proposal) -> usize {
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

fn summary(f: &Facts) -> String {
    let count = plural(steps(f.file), "step", "steps");
    match f.opened {
        Opened::Onboarding => match f.marks.first() {
            Some(mark) => format!("{count} \u{b7} {}, changed by {}", source_name(f), mark.by),
            None => format!("{count} \u{b7} starting from {}", source_name(f)),
        },
        Opened::Project if f.proposal.diff.same => format!("{count} \u{b7} in use"),
        Opened::Project => format!(
            "{count} \u{b7} {}, not applied",
            plural(changed_stages(f.proposal), "change", "changes")
        ),
    }
}

/// "Security moved before Validate.", "Docs removed.": what a reorder or removal does, for the change line.
fn shape_words(f: &Facts) -> String {
    let stages = &f.file.flow.stages;
    let mut words: Vec<String> = Vec::new();
    for change in &f.proposal.diff.stages {
        match &change.change {
            Change::Moved { .. } => {
                let at = f.file.flow.position(&change.stage_id);
                let next = at
                    .and_then(|i| stages.get(i + 1))
                    .filter(|s| !s.is_delivery());
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

/// "the 6 there now (4 in Dev, 2 in Security)": the tickets inside, by stage in the order they come.
fn inside_words(inside: &[Inside]) -> String {
    let mut by: Vec<(&str, usize)> = Vec::new();
    for i in inside {
        match by.iter_mut().find(|(s, _)| *s == i.stage) {
            Some((_, n)) => *n += 1,
            None => by.push((&i.stage, 1)),
        }
    }
    by.iter()
        .map(|(stage, n)| format!("{n} in {stage}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// "Validate now runs Security reviewer and Performance reviewer side by side. It applies at once: the next round of
/// a ticket at Validate runs both." for each agent check whose check agents the draft changes (d0130, d0115: a stage's
/// agents apply when it next starts, `s29-04`); one sentence per stage.
fn check_agents_words(f: &Facts) -> Vec<String> {
    f.proposal
        .diff
        .stages
        .iter()
        .filter_map(|c| match &c.change {
            Change::Agents { to, .. } if !to.is_empty() => {
                let names: Vec<&str> = to.iter().map(String::as_str).collect();
                let stage = &c.stage;
                let (side, them) = match names.len() {
                    1 => ("", "it".to_owned()),
                    2 => (" side by side", "both".to_owned()),
                    n => (" side by side", format!("all {n}")),
                };
                Some(format!(
                    "{stage} now runs {}{side}. It applies at once: the next round of a ticket at {stage} runs \
                     {them}.",
                    joined_words(&names)
                ))
            }
            _ => None,
        })
        .collect()
}

/// The change line (FED-08; `s27-07`): which part waits, how many tickets are in it, and that Apply now is there.
fn change_line(f: &Facts) -> Option<String> {
    if f.opened == Opened::Onboarding {
        return None;
    }
    Some(match f.applies {
        DraftApplies::Nothing => return None,
        DraftApplies::AtOnce => {
            let mut words = check_agents_words(f);
            // Anything else changed: the line for it too.
            let others = f
                .proposal
                .diff
                .stages
                .iter()
                .any(|c| !matches!(c.change, Change::Agents { .. }));
            if words.is_empty() || others {
                words.push(
                    "This applies at once: tickets that haven\u{2019}t reached a changed step follow it; tickets \
                     in it or past it finish as they are."
                        .to_owned(),
                );
            }
            words.join(" ")
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
                    1 => format!("the ticket there now ({}) finishes", inside_words(inside)),
                    n => format!("the {n} there now ({}) finish", inside_words(inside)),
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

/// The problems no step shows: about the whole flow, or an agent no step runs or offers.
fn problems(f: &Facts) -> Vec<String> {
    f.proposal
        .problems
        .iter()
        .filter(|p| p.fix.is_none() && !on_a_step(f.file, p))
        .map(|p| p.message.clone())
        .collect()
}

/// The fixes no step shows (an agent no step runs or offers).
fn flow_fixes(f: &Facts) -> Vec<FixRow> {
    fix_rows(
        f,
        f.proposal.problems.iter().filter(|p| !on_a_step(f.file, p)),
        clashes(f).filter(|a| agent_stages(f.file, &a.name).next().is_none()),
    )
}

fn flow_part(f: &Facts) -> FlowPart {
    let selected = f.selected.and_then(|id| f.file.flow.stage(id));
    let stages = &f.file.flow.stages;
    let before_delivery = stages
        .iter()
        .rev()
        .find(|s| !s.is_delivery())
        .map(|s| s.id.clone());
    FlowPart {
        title: format!("{}\u{2019}s flow", f.project_name),
        summary: summary(f),
        watch: "Watch it",
        folded: selected.is_some(),
        fold_note: "The animation folds while you edit a stage",
        ticket_key: format!("{}-1", f.project),
        tiles: tiles(f),
        steps: steps_list(f),
        add_stage: "Add a stage",
        add_stage_after: before_delivery,
        hint: "Click a step to edit it, drag to reorder, or ask the agent.",
        legend_title: "Every step is one of four kinds:",
        legend: legend(),
        details: selected.map(|s| details(f, s)),
        problems: problems(f),
        fixes: flow_fixes(f),
        change_line: change_line(f),
        undo_all: f.can_undo_all.then_some("Undo all"),
    }
}

fn in_use_badge() -> Badge {
    Badge {
        label: "In use",
        pill: PillView::legend(Legend::Approved, Some("In use")),
    }
}

fn new_badge() -> Badge {
    Badge {
        label: "New",
        pill: PillView::legend(Legend::ReadyForReview, Some("New")),
    }
}

/// Which entry the draft holds: a preset copied in, a tool's proposal, else (unchanged or edited) the flow in use
/// (the default in onboarding).
fn chosen(f: &Facts, pick: &Pick) -> bool {
    let tool = f.proposal.proposed_by.tool().filter(|_| f.swap);
    match (pick, &f.proposal.source) {
        (Pick::Entry { .. }, _) => false,
        (_, _) if tool.is_some() => false,
        (Pick::Preset { file }, Source::Preset { file: from }) if f.swap => file == from,
        (Pick::FlowInUse, _) => !f.swap,
        (Pick::Preset { file }, _) => {
            f.opened == Opened::Onboarding && !f.swap && *file == presets()[0].file
        }
        _ => false,
    }
}

fn start_from(f: &Facts) -> StartFrom {
    let mut entries = Vec::new();
    if f.opened == Opened::Project {
        let pick = Pick::FlowInUse;
        entries.push(Entry {
            key: "in-use".to_owned(),
            name: format!("{}\u{2019}s flow", f.project_name),
            badge: Some(in_use_badge()),
            count: steps(f.in_use),
            line: "In use \u{b7} the board\u{2019}s columns".to_owned(),
            chosen: chosen(f, &pick),
            avatar: None,
            pick,
        });
    }
    for preset in presets() {
        let pick = Pick::Preset {
            file: preset.file.to_owned(),
        };
        entries.push(Entry {
            key: preset.file.to_owned(),
            name: preset.name.clone(),
            badge: None,
            count: steps(&preset.read()),
            line: preset_line(preset.file).to_owned(),
            chosen: chosen(f, &pick),
            avatar: None,
            pick,
        });
    }
    let mut menu = vec![MenuItem::new("file", "A file\u{2026}").icon(IconName::Upload)];
    if !f.projects.is_empty() {
        menu.push(MenuItem::header("FROM A PROJECT").separated());
        menu.extend(f.projects.iter().map(|(key, name)| {
            MenuItem::new(format!("project:{key}"), format!("{name}\u{2019}s flow"))
        }));
    }
    StartFrom {
        title: "START FROM",
        entries,
        made_for: made_for(f),
        import: Import {
            label: "Import a flow\u{2026}",
            menu,
            file_title: "Import a flow",
        },
        note: "Starting from one copies it here; nothing changes until you use it.",
    }
}

fn made_for(f: &Facts) -> MadeFor {
    let draft_tool = f.proposal.proposed_by.tool().filter(|_| f.swap);
    let entries: Vec<Entry> = f
        .setup
        .proposals
        .iter()
        .filter(|p| p.id != f.proposal.id)
        .filter_map(|p| Some((p, p.proposed_by.tool()?)))
        .map(|(p, tool)| {
            let pick = Pick::Entry { proposal: p.id };
            let count = otaq_core::flow::yaml::read(&p.yaml).map_or(0, |file| steps(&file));
            Entry {
                key: format!("proposal-{}", p.id),
                name: format!("By {}", tool.display_name()),
                badge: Some(new_badge()),
                count,
                line: format!("From your {} sessions", tool.display_name()),
                chosen: draft_tool == Some(tool),
                avatar: Some(ToolAvatarView::of(tool, None)),
                pick,
            }
        })
        .collect();
    let any_proposed = !entries.is_empty();
    let working = f
        .agent
        .filter(|a| a.state == AgentState::Running && a.mid_turn)
        .map(|a| a.tool);
    let tools = f
        .setup
        .tools
        .iter()
        .filter(|t| !matches!(t.row, FlowSetupRow::Proposed { .. }))
        .map(|t| {
            let (status, ask) = match &t.row {
                FlowSetupRow::Working { .. } => ("Working".to_owned(), None),
                _ if f.opened == Opened::Project && working == Some(t.tool) => {
                    ("Working".to_owned(), None)
                }
                FlowSetupRow::PlanLimit { .. } => ("Plan limit".to_owned(), None),
                FlowSetupRow::NotSignedIn => ("Not signed in".to_owned(), None),
                FlowSetupRow::Ended { .. } => ("Ended without a proposal".to_owned(), Some("Ask")),
                _ if any_proposed => ("Connected \u{b7} not asked".to_owned(), Some("Ask")),
                _ => ("Connected".to_owned(), Some("Ask")),
            };
            ToolRow {
                tool: t.tool,
                avatar: ToolAvatarView::of(t.tool, None),
                name: t.tool.display_name(),
                status,
                ask,
            }
        })
        .collect();
    MadeFor {
        title: format!("MADE FOR {}", f.project_name.to_uppercase()),
        text: "Ask a connected tool; it reads how you use it.",
        entries,
        tools,
    }
}

/// The connected tools the chooser and the chips offer.
fn askable(f: &Facts) -> Vec<Tool> {
    f.setup
        .tools
        .iter()
        .filter(|t| !matches!(t.row, FlowSetupRow::NotSignedIn))
        .map(|t| t.tool)
        .collect()
}

fn chooser(f: &Facts, tools: &[Tool]) -> Option<Chooser> {
    (tools.len() > 1).then(|| Chooser {
        title: "Which tool should read how you work?",
        text: "Each reads only its own history on this Mac, and this repo.",
        tools: tools
            .iter()
            .map(|&tool| ChooserTool {
                tool,
                avatar: ToolAvatarView::of(tool, None),
                name: tool.display_name(),
                detail: "Reads its own history on this Mac",
            })
            .collect(),
        both: (f.opened == Opened::Onboarding).then(|| {
            (
                "Both, and compare".to_owned(),
                format!("Two flows under Made for {}", f.project_name),
            )
        }),
        foot: "Uses tokens \u{b7} read by that tool\u{2019}s provider",
    })
}

/// The fixed chips (d0115: "fixed text written by Otaq"): about the selected step, else about the flow.
fn chips(f: &Facts, tools: &[Tool]) -> Vec<Chip> {
    let stages = &f.file.flow.stages;
    let send = |label: String| Chip {
        action: ChipAction::Send {
            message: label.clone(),
        },
        label,
    };
    let mut chips = Vec::new();
    let selected = f.selected.and_then(|id| f.file.flow.stage(id));
    if let Some(skill) = selected.and_then(|s| s.skill.as_deref()) {
        chips.push(send(format!("What does {skill} do?")));
    }
    match f.opened {
        Opened::Onboarding => {
            match tools {
                [] => {}
                [tool] => chips.push(Chip {
                    label: format!("Ask {}", tool.display_name()),
                    action: ChipAction::Ask { tool: *tool },
                }),
                _ => chips.push(Chip {
                    label: format!("Make one for {}\u{2026}", f.project_name),
                    action: ChipAction::ChooseTool,
                }),
            }
            let check = stages
                .iter()
                .find(|s| s.is_agent_check())
                .or_else(|| stages.first());
            if let (Some(check), None) = (check, selected) {
                chips.push(send(format!("What\u{2019}s {}?", check.name)));
            }
        }
        Opened::Project => {
            let why = stages
                .iter()
                .rev()
                .find(|s| s.is_agent_check())
                .or_else(|| stages.iter().find(|s| s.is_agent_stage()));
            if let (Some(why), None) = (why, selected) {
                chips.push(send(format!("Why is {} here?", why.name)));
            }
            if let Some(review) = stages
                .iter()
                .find(|s| s.is_human_check() && !s.options.contains(&StageOption::Skip))
            {
                chips.push(send(format!("Make {} optional", review.name)));
            }
        }
    }
    chips
}

/// "Claude Code · sonnet": what the first message starts on (the Project agent role's agent).
fn starts_on(f: &Facts) -> (Option<ToolAvatarView>, String) {
    let on = f
        .new_agent
        .and_then(|a| a.model.on(f.tools.default_model.as_ref()));
    match on {
        Some(on) => {
            let model = on
                .model
                .clone()
                .unwrap_or_else(|| "tool default".to_owned());
            (
                Some(ToolAvatarView::of(on.tool, on.model.as_deref())),
                format!("{} \u{b7} {model}", on.tool.display_name()),
            )
        }
        None => {
            let tool = Tool::ALL[0];
            (
                Some(ToolAvatarView::of(tool, None)),
                tool.display_name().to_owned(),
            )
        }
    }
}

fn panel_pill(agent: &ProjectAgent) -> PillView {
    let pill = |legend: Legend, icon: IconName, label: &str| PillView {
        icon,
        label: label.to_owned(),
        ..PillView::legend(legend, None)
    };
    match agent.state {
        AgentState::Running if agent.mid_turn && agent.needs_you.is_none() => {
            pill(Legend::AgentWorking, IconName::Activity, "Working")
        }
        AgentState::Running if agent.needs_you.is_some() => {
            pill(Legend::NeedsYou, IconName::Hand, "Waiting for you")
        }
        AgentState::Running => pill(
            Legend::AsksYou,
            IconName::MessageCircleQuestionMark,
            "Waiting for you",
        ),
        AgentState::Paused => pill(Legend::Queued, IconName::CirclePause, "Paused"),
        _ => pill(Legend::Queued, IconName::CircleStop, "Ended"),
    }
}

fn agent_panel(f: &Facts) -> AgentPanel {
    let keys = f.keys.label(KeyAction::FocusAgentPanel);
    // An agent that ended leaves the panel as before anything was asked: the next words start a new one.
    let running = f.agent.filter(|a| a.state == AgentState::Running);
    if let Some(agent) = running {
        let model = agent
            .model
            .clone()
            .unwrap_or_else(|| "tool default".to_owned());
        return AgentPanel {
            title: PANEL_TITLE,
            pill: panel_pill(agent),
            avatar: Some(ToolAvatarView::of(agent.tool, agent.model.as_deref())),
            line: format!("{} \u{b7} {model}", agent.tool.display_name()),
            note: format!("\u{b7} {}", tokens_label(agent.tokens.shown())),
            agent: Some(agent.id.clone()),
            session: agent.session.as_ref().map(|s| s.id.clone()),
            hint: Vec::new(),
            chips: Vec::new(),
            chooser: None,
            foot: TYPING_GOES,
            keys,
            keys_label: "focus",
        };
    }
    let tools = askable(f);
    let (avatar, line) = starts_on(f);
    let needs = f
        .selected
        .and_then(|id| f.file.flow.stage(id))
        .filter(|s| s.needs_skill() && !writing(f, &s.id));
    let hint = match f.opened {
        // The selected stage needs a skill (`s27-05`): the panel offers to write it.
        _ if needs.is_some() => vec![format!(
            "{} needs a skill. Ask here to have one written (it reads this repo and your other skills, and uses \
             tokens), or pick one of yours in the stage.",
            needs.map_or("", |s| s.name.as_str())
        )],
        Opened::Onboarding => {
            let check = f
                .file
                .flow
                .stages
                .iter()
                .find(|s| s.is_agent_check())
                .map_or("Validate", |s| s.name.as_str());
            vec![
                format!(
                    "Ask about the flow, or to change it: \u{201c}what does {check} do?\u{201d}, \u{201c}add a \
                     security check\u{201d}."
                ),
                format!(
                    "To have one made for {}, ask a connected tool in Start from. Nothing runs, and no tokens are \
                     used, until you ask.",
                    f.project_name
                ),
            ]
        }
        Opened::Project => vec![format!(
            "Ask about {}\u{2019}s flow, or to change it. Nothing on the board changes until you apply, and \
             nothing runs until you ask.",
            f.project_name
        )],
    };
    AgentPanel {
        title: PANEL_TITLE,
        pill: PillView {
            icon: IconName::CircleDashed,
            label: NOT_STARTED.to_owned(),
            ..PillView::legend(Legend::Queued, None)
        },
        avatar,
        line,
        note: STARTS_WHEN_YOU_ASK.to_owned(),
        agent: None,
        session: None,
        hint,
        chips: match needs {
            Some(stage) => vec![
                Chip {
                    label: "Write the skill".to_owned(),
                    action: ChipAction::WriteSkill {
                        stage: stage.id.clone(),
                        prefill: write_skill_prefill(f.file, &stage.id),
                    },
                },
                Chip {
                    label: format!("What should {} do?", stage.name),
                    action: ChipAction::Send {
                        message: format!("What should {} do?", stage.name),
                    },
                },
            ],
            None => chips(f, &tools),
        },
        chooser: chooser(f, &tools),
        foot: TYPING_GOES,
        keys,
        keys_label: "focus",
    }
}

fn footer(f: &Facts) -> Footer {
    // "Plan needs a skill · being written" while the agent writes it (`s27-06`).
    let reason = left(f).map(|p| match &p.about {
        About::Stage { id, .. } if writing(f, id) => format!("{} \u{b7} being written", p.message),
        _ => problem_text(f, p),
    });
    match f.opened {
        Opened::Onboarding => Footer {
            cancel: "Skip, use the default",
            apply: "Use this flow".to_owned(),
            apply_enabled: reason.is_none(),
            reason,
        },
        Opened::Project if f.proposal.diff.same => Footer {
            cancel: "Cancel",
            apply: "No changes".to_owned(),
            apply_enabled: false,
            reason: None,
        },
        Opened::Project => Footer {
            cancel: "Cancel",
            apply: format!("Apply to {}", f.project_name),
            apply_enabled: reason.is_none(),
            reason,
        },
    }
}

pub fn view(f: &Facts) -> View {
    View {
        project: f.project.to_owned(),
        from: f.opened,
        proposal: f.proposal.id,
        start_from: start_from(f),
        flow: flow_part(f),
        agent: agent_panel(f),
        footer: footer(f),
        choices: choices_kept(f),
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

/// `stage` as the kind `kind`, keeping its agent and skill unless they were the old kind's defaults.
fn with_kind(stage: &Stage, kind: EditorKind) -> Stage {
    let mut s = stage.clone();
    let was_default = |agent: &str, skill: &str| {
        the_agent(stage).is_none_or(|a| a.eq_ignore_ascii_case(agent))
            && stage.skill.as_deref().is_none_or(|k| k == skill)
    };
    let defaults = match editor_kind(stage) {
        Some(EditorKind::Works) => was_default(WORKS_AGENT, WORKS_SKILL),
        Some(EditorKind::AgentChecks) => was_default(CHECKS_AGENT, CHECKS_SKILL),
        _ => true,
    };
    let (agent, skill) = match kind {
        EditorKind::Works => (WORKS_AGENT, WORKS_SKILL),
        EditorKind::AgentChecks => (CHECKS_AGENT, CHECKS_SKILL),
        EditorKind::YouCheck => ("", ""),
    };
    match kind {
        EditorKind::YouCheck => {
            s.kind = StageKind::Check { by: CheckBy::You };
            set_agent(&mut s, None);
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
            let skilless = the_agent(stage).is_some() && stage.skill.is_none();
            // The agent moves to the new kind's field (an agent check lists it, d0130).
            let kept = the_agent(stage).cloned();
            set_agent(&mut s, kept);
            if defaults || the_agent(&s).is_none() {
                set_agent(&mut s, Some(agent.to_owned()));
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

/// A plain template (FED-05): `stage` with its name and kind, the kind's agent, no skill, no options; a new id from
/// the name (the stage is new).
fn plain_stage(file: &FlowFile, at: usize, name: &str, kind: EditorKind) -> Stage {
    let name = unique_name(file, name, at);
    let mut others = file.clone();
    others.flow.stages.remove(at);
    let (agent, rounds) = match kind {
        EditorKind::Works => (Some(WORKS_AGENT), None),
        EditorKind::AgentChecks => (Some(CHECKS_AGENT), Some(CHECK_ROUNDS)),
        EditorKind::YouCheck => (None, None),
    };
    let mut stage = Stage {
        id: fresh_id(&others, &name),
        name,
        kind: match kind {
            EditorKind::Works => StageKind::Agent,
            EditorKind::AgentChecks => StageKind::Check { by: CheckBy::Agent },
            EditorKind::YouCheck => StageKind::Check { by: CheckBy::You },
        },
        column: Column::Own,
        agent: None,
        agents: Vec::new(),
        skill: None,
        options: Vec::new(),
        rounds,
        fail_to: None,
        delivery: None,
        why: None,
    };
    set_agent(&mut stage, agent.map(str::to_owned));
    stage
}

/// The agents `stage` names that neither the document nor `stored` has, from `brought` (a preset's): they come with
/// it, created on Apply.
fn bring_agents(out: &mut FlowFile, stage: &Stage, brought: &[FlowAgent], stored: &[Agent]) {
    for name in stage.every_agent() {
        let known = out.agents.iter().any(|a| a.name.eq_ignore_ascii_case(name))
            || stored_agent(stored, name).is_some();
        if let Some(agent) = brought
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .filter(|_| !known)
        {
            out.agents.push(agent.clone());
        }
    }
}

/// The draft's document after `edit`, and the stage to select after it (a new one). Refused with why when the edit
/// doesn't fit the flow (a stage that isn't there, the Delivery moved or removed). `stored` are the user's agents: a
/// picked skill's stage agent, and a preset template's agents the user doesn't have.
pub fn edited(
    file: &FlowFile,
    edit: &StageEdit,
    stored: &[Agent],
) -> Result<(FlowFile, Option<String>), String> {
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
            if out.flow.stages[i].is_delivery() {
                return Err("The Delivery stage stays the last one".to_owned());
            }
            out.flow.stages[i] = with_kind(&out.flow.stages[i], *kind);
        }
        StageEdit::Agent { stage, agent } => {
            let i = at(stage)?;
            let s = &mut out.flow.stages[i];
            if the_agent(s).is_none() {
                return Err(format!("No agent runs {}", s.name));
            }
            set_agent(s, Some(agent.clone()));
            s.options
                .retain(|o| !matches!(o, StageOption::Agent(a) if a.eq_ignore_ascii_case(agent)));
        }
        StageEdit::Option { stage, option, on } => {
            let i = at(stage)?;
            let s = &mut out.flow.stages[i];
            let option = if option == StageOption::SKIP {
                StageOption::Skip
            } else {
                StageOption::Agent(option.clone())
            };
            s.options.retain(|o| *o != option);
            if *on {
                s.options.push(option);
            }
        }
        StageEdit::Move { stage, to } => {
            let i = at(stage)?;
            if out.flow.stages[i].is_delivery() {
                return Err("The Delivery stage stays the last one".to_owned());
            }
            let moved = out.flow.stages.remove(i);
            let delivery = out
                .flow
                .stages
                .iter()
                .position(Stage::is_delivery)
                .unwrap_or(out.flow.stages.len());
            out.flow.stages.insert((*to).min(delivery), moved);
        }
        StageEdit::Add { after } => {
            let index = match after {
                Some(id) => at(id)? + 1,
                None => 0,
            };
            let delivery = out
                .flow
                .stages
                .iter()
                .position(Stage::is_delivery)
                .unwrap_or(out.flow.stages.len());
            let name = fresh_name(&out);
            let id = fresh_id(&out, &name);
            out.flow.stages.insert(
                index.min(delivery),
                Stage {
                    id: id.clone(),
                    name,
                    kind: StageKind::Agent,
                    column: Column::Own,
                    agent: Some(WORKS_AGENT.to_owned()),
                    agents: Vec::new(),
                    // Blank: the user picks the skill or has it written (FED-05, FED-06).
                    skill: None,
                    options: Vec::new(),
                    rounds: None,
                    fail_to: None,
                    delivery: None,
                    why: None,
                },
            );
            select = Some(id);
        }
        StageEdit::Remove { stage } => {
            let i = at(stage)?;
            if out.flow.stages[i].is_delivery() {
                return Err("The Delivery stage stays: a flow ends with one".to_owned());
            }
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
            let runs: Vec<String> = out.flow.stages[i].named_agents().cloned().collect();
            if runs.is_empty() && !out.flow.stages[i].is_agent_check() {
                return Err(format!("No agent runs {}", out.flow.stages[i].name));
            }
            let skill = skill.trim();
            if skill.is_empty() {
                return Err("Pick a skill".to_owned());
            }
            out.flow.stages[i].skill = Some(skill.to_owned());
            // The stage runs its agents' skill of that name (each check agent its own, d0130): each gets it unless it
            // has it.
            let has = |skills: &[String]| skills.iter().any(|s| s.eq_ignore_ascii_case(skill));
            for agent in runs {
                match out
                    .agents
                    .iter_mut()
                    .find(|a| a.name.eq_ignore_ascii_case(&agent))
                {
                    Some(a) if !has(&a.skills) => a.skills.push(skill.to_owned()),
                    Some(_) => {}
                    None => {
                        if let Some(mine) = stored_agent(stored, &agent) {
                            let mut a = flow_agent_of(mine);
                            if !has(&a.skills) {
                                a.skills.push(skill.to_owned());
                                out.agents.push(a);
                            }
                        }
                    }
                }
            }
        }
        StageEdit::AddCheckAgent { stage, agent } => {
            let i = at(stage)?;
            let s = &mut out.flow.stages[i];
            if !s.is_agent_check() {
                return Err(format!("{} isn\u{2019}t an agent check", s.name));
            }
            let agent = agent.trim();
            if agent.is_empty() {
                return Err("Pick an agent".to_owned());
            }
            if s.agents.iter().any(|a| a.eq_ignore_ascii_case(agent)) {
                return Err(format!("{agent} already checks {}", s.name));
            }
            s.agents.push(agent.to_owned());
            s.options
                .retain(|o| !matches!(o, StageOption::Agent(a) if a.eq_ignore_ascii_case(agent)));
        }
        StageEdit::RemoveCheckAgent { stage, agent } => {
            let i = at(stage)?;
            let s = &mut out.flow.stages[i];
            let before = s.agents.len();
            // The last one may go too: the check then says the stage needs one, and Apply waits (d0130).
            s.agents.retain(|a| !a.eq_ignore_ascii_case(agent));
            if s.agents.len() == before {
                return Err(format!("{agent} doesn\u{2019}t check {}", s.name));
            }
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
        StageEdit::Template { stage, template } => {
            let i = at(stage)?;
            if out.flow.stages[i].is_delivery() {
                return Err("The Delivery stage stays the last one".to_owned());
            }
            match (template.shape(), template.preset_stage()) {
                (Some((name, kind)), _) => {
                    let new = plain_stage(&out, i, name, kind);
                    select = Some(new.id.clone());
                    out.flow.stages[i] = new;
                }
                (None, Some((from, agents))) if from.is_delivery() => {
                    // The flow has one Delivery: PR delivery replaces it, and the new stage goes.
                    let d = out
                        .flow
                        .stages
                        .iter()
                        .position(Stage::is_delivery)
                        .ok_or_else(|| "The flow has no Delivery stage".to_owned())?;
                    let new = Stage {
                        id: out.flow.stages[d].id.clone(),
                        name: out.flow.stages[d].name.clone(),
                        why: None,
                        ..from
                    };
                    bring_agents(&mut out, &new, &agents, stored);
                    select = Some(new.id.clone());
                    out.flow.stages[d] = new;
                    out.flow.stages.remove(i);
                }
                (None, Some((from, agents))) => {
                    let name = unique_name(&out, &from.name, i);
                    let mut others = out.clone();
                    others.flow.stages.remove(i);
                    let new = Stage {
                        id: fresh_id(&others, &name),
                        name,
                        why: None,
                        ..from
                    };
                    bring_agents(&mut out, &new, &agents, stored);
                    select = Some(new.id.clone());
                    out.flow.stages[i] = new;
                }
                (None, None) => return Err(format!("No preset has {}", template.label())),
            }
        }
    }
    Ok((out, select))
}

/// The gallery's editors: shop-web's flow in the project, Otaq default in onboarding with two tools.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowEditorGallery")]
pub struct Gallery {
    pub cells: Vec<(String, View)>,
}

pub fn gallery() -> Gallery {
    use fixtures::*;
    let tools = crate::view_models::flow_setup_page::fixtures::tools();
    let setup = crate::view_models::flow_setup_page::fixtures::setup(
        FlowSetupRow::NotAsked,
        FlowSetupRow::NotAsked,
    );
    let keys = KeyLabels::default();
    let captions = BTreeMap::from([(
        "security-review".to_owned(),
        "A security agent reviews the change for risky code.".to_owned(),
    )]);
    let shop = shop_web();
    let default = otaq_default();
    let stored = stored();
    let none = DraftApplies::Nothing;
    let in_project = proposal(&shop, true);
    let in_onboarding = proposal(&default, true);
    let at_once = DraftApplies::AtOnce;
    let (needs, needs_proposal) = plan_draft(None);
    let (wrote, wrote_proposal) = plan_draft(Some("plan-ticket"));
    let writing = [SkillWriting {
        stage_id: "plan".into(),
        agent: None,
    }];
    let written = [written_plan()];
    let no_choices = Choices::default();
    let imported_proposal = imported(&shop);
    let two_agents = checks(
        &shop,
        &[StageEdit::AddCheckAgent {
            stage: "validate".into(),
            agent: "Security reviewer".into(),
        }],
    );
    let review_to_grill = checks(
        &shop,
        &[StageEdit::FailTo {
            stage: "review".into(),
            to: Some("grill".into()),
        }],
    );
    let check = |(file, proposal): &'_ (FlowFile, Proposal), selected: &'_ str| -> View {
        view(&Facts {
            project: "SHOP",
            project_name: "shop-web",
            opened: Opened::Project,
            proposal,
            marks: &[],
            can_undo_all: false,
            swap: false,
            applies: &at_once,
            file,
            in_use: &shop,
            stored: &stored,
            tools: &tools,
            setup: &setup,
            projects: &[],
            captions: &captions,
            writing: &[],
            written: &[],
            selected: Some(selected),
            agent: None,
            new_agent: None,
            keys: &keys,
            choices: &no_choices,
        })
    };
    let plan = |proposal: &'_ Proposal,
                file: &'_ FlowFile,
                writing: &'_ [SkillWriting],
                written: &'_ [WrittenSkill]|
     -> View {
        view(&Facts {
            project: "SHOP",
            project_name: "shop-web",
            opened: Opened::Project,
            proposal,
            marks: &[],
            can_undo_all: false,
            swap: false,
            applies: &at_once,
            file,
            in_use: &shop,
            stored: &stored,
            tools: &tools,
            setup: &setup,
            projects: &[],
            captions: &captions,
            writing,
            written,
            selected: Some("plan"),
            agent: None,
            new_agent: None,
            keys: &keys,
            choices: &no_choices,
        })
    };
    let facts = |opened, proposal: &'_ Proposal, file: &'_ FlowFile| -> View {
        view(&Facts {
            project: "SHOP",
            project_name: "shop-web",
            opened,
            proposal,
            marks: &[],
            can_undo_all: false,
            swap: false,
            applies: &none,
            file,
            in_use: file,
            stored: &stored,
            tools: &tools,
            setup: &setup,
            projects: &[],
            captions: &captions,
            writing: &[],
            written: &[],
            selected: None,
            agent: None,
            new_agent: None,
            keys: &keys,
            choices: &no_choices,
        })
    };
    Gallery {
        cells: vec![
            (
                "In the project".to_owned(),
                facts(Opened::Project, &in_project, &shop),
            ),
            (
                "In onboarding".to_owned(),
                facts(Opened::Onboarding, &in_onboarding, &default),
            ),
            (
                "A new stage: Plan picked".to_owned(),
                plan(&needs_proposal, &needs, &[], &[]),
            ),
            (
                "Its skill being written".to_owned(),
                plan(&needs_proposal, &needs, &writing, &[]),
            ),
            (
                "Its skill written".to_owned(),
                plan(&wrote_proposal, &wrote, &[], &written),
            ),
            (
                "An imported file: a skill to import, a clash".to_owned(),
                view(&Facts {
                    project: "SHOP",
                    project_name: "shop-web",
                    opened: Opened::Project,
                    proposal: &imported_proposal,
                    marks: &[],
                    can_undo_all: false,
                    swap: false,
                    applies: &at_once,
                    file: &shop,
                    in_use: &default,
                    stored: &stored,
                    tools: &tools,
                    setup: &setup,
                    projects: &[],
                    captions: &captions,
                    writing: &[],
                    written: &[],
                    selected: Some("security"),
                    agent: None,
                    new_agent: None,
                    keys: &keys,
                    choices: &no_choices,
                }),
            ),
            (
                "One check, several agents".to_owned(),
                check(&two_agents, "validate"),
            ),
            (
                "Fails go to on a check by you".to_owned(),
                check(&review_to_grill, "review"),
            ),
        ],
    }
}

/// shop-web's flow and the draft around it, for the tests and the gallery.
pub mod fixtures {
    use otaq_core::agents::{Agent, AgentModel, CameFrom, ImportedSkill, SkillRef};
    use otaq_core::flow::draft::WrittenSkill;
    use otaq_core::flow::model::FlowFile;
    use otaq_core::flow::proposal::{
        About, AgentChange, AgentChangeKind, AgentField, Clash, Fix, FlowDiff, Proposal,
        ProposalProblem, ProposedBy, Source,
    };

    use super::{StageEdit, Template, edited};

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
                    fix: None,
                })
                .collect();
        }
        draft
    }

    /// `file` after `edits`, and its draft's proposal against `file` ([`checked_proposal`]).
    pub fn checks(file: &FlowFile, edits: &[StageEdit]) -> (FlowFile, Proposal) {
        let new = edits.iter().fold(file.clone(), |f, edit| {
            edited(&f, edit, &stored()).expect("the edit applies").0
        });
        let draft = checked_proposal(file, &new);
        (new, draft)
    }

    /// shop-web on Grill first with a Security check (the ticket file's example): 7 steps.
    pub const SHOP_WEB: &str = "agents:
  - name: Griller
    skills: [grill]
    model: default
  - name: Rust developer
    skills: [work-ticket]
    model: default
  - name: Validator
    skills: [validate]
    model: default
  - name: Security reviewer
    skills: [security-review]
    model: default
flow:
  - id: grill
    name: Grill
    kind: agent
    column: own
    agent: Griller
    skill: grill
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
    agents: [Validator]
    skill: validate
    rounds: 3
  - id: security
    name: Security
    kind: check
    by: agent
    column: own
    agents: [Security reviewer]
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

    pub fn shop_web() -> FlowFile {
        otaq_core::flow::yaml::read(SHOP_WEB).expect("shop-web's flow reads")
    }

    pub fn otaq_default() -> FlowFile {
        otaq_core::flow::presets::presets()[0].read()
    }

    fn agent(id: &str, name: &str, skill: &str) -> Agent {
        Agent {
            id: id.to_owned(),
            name: name.to_owned(),
            instructions: String::new(),
            skills: vec![SkillRef::Shipped(skill.to_owned())],
            imported: Vec::new(),
            model: AgentModel::Default,
            effort: None,
            role: None,
            came_from: CameFrom::AgentsFile,
            mark: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    /// The stored agents: Otaq's and shop-web's; the Security reviewer has the user's imported `security-review`.
    pub fn stored() -> Vec<Agent> {
        let mut security = agent("a5", "Security reviewer", "validate");
        security.skills.push(SkillRef::Imported("s1".into()));
        security.imported.push(ImportedSkill {
            id: "s1".into(),
            name: "security-review".into(),
            original: "/Users/me/.claude/skills/security-review".into(),
            imported_at: 0,
            fingerprint: String::new(),
            warnings: Vec::new(),
        });
        vec![
            agent("a1", "Developer", "work-ticket"),
            agent("a2", "Validator", "validate"),
            agent("a3", "Griller", "grill"),
            agent("a4", "Rust developer", "work-ticket"),
            security,
        ]
    }

    /// shop-web with Plan picked after Grill (`s27-05`), its skill `skill` (none: "Plan needs a skill"), and the
    /// draft's proposal.
    pub fn plan_draft(skill: Option<&str>) -> (FlowFile, Proposal) {
        let (added, new) = edited(
            &shop_web(),
            &StageEdit::Add {
                after: Some("grill".into()),
            },
            &stored(),
        )
        .expect("a stage is added");
        let (mut file, _) = edited(
            &added,
            &StageEdit::Template {
                stage: new.expect("the new stage"),
                template: Template::Plan,
            },
            &stored(),
        )
        .expect("Plan fills it");
        let plan = file.flow.stages.iter_mut().find(|s| s.id == "plan");
        plan.expect("Plan").skill = skill.map(str::to_owned);
        let mut draft = proposal(&file, false);
        if skill.is_none() {
            draft.problems.push(ProposalProblem {
                about: About::Stage {
                    id: "plan".into(),
                    name: "Plan".into(),
                },
                message: "Plan needs a skill".into(),
                fix: None,
            });
        }
        (file, draft)
    }

    /// `plan-ticket`, written for Plan by the Project agent Checkout slowness.
    pub fn written_plan() -> WrittenSkill {
        WrittenSkill {
            stage_id: "plan".into(),
            skill: "plan-ticket".into(),
            copy: "c1".into(),
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
                agents: Vec::new(),
                warnings: Vec::new(),
                removed_choices: Vec::new(),
                waits: false,
            },
            why: Vec::new(),
            created_at: 0,
        }
    }

    /// shop-web's flow imported from team.yaml (Q426): Security's skill isn't on this machine, and the file's Security
    /// reviewer clashes with the user's.
    pub fn imported(file: &FlowFile) -> Proposal {
        let missing = |about: About| ProposalProblem {
            about,
            message: "Security names the skill `security-review`, which isn't on this machine"
                .into(),
            fix: Some(Fix::ImportSkillFolder {
                skill: "security-review".into(),
            }),
        };
        let mut proposal = proposal(file, false);
        proposal.source = Source::Document {
            file: Some("team.yaml".into()),
        };
        proposal.problems = vec![
            missing(About::Stage {
                id: "security".into(),
                name: "Security".into(),
            }),
            missing(About::Agent {
                name: "Security reviewer".into(),
            }),
        ];
        proposal.diff.agents = vec![AgentChange {
            name: "Security reviewer".into(),
            change: AgentChangeKind::Changed,
            fields: vec![AgentField {
                field: "instructions".into(),
                from: String::new(),
                to: "Look for risky code.".into(),
            }],
            used_in: Vec::new(),
            clash: Some(Clash {
                add_as: Some("Security reviewer (team.yaml)".into()),
            }),
        }];
        proposal
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::flow::apply::Span;
    use otaq_core::flow::proposal::{Applies, ProposalProblem, ProposedBy, StageChange};
    use otaq_core::project_agents::first_flow::FlowSetupTool;

    use super::fixtures::*;
    use super::*;
    use crate::view_models::flow_setup_page::fixtures as setup;

    struct Draft {
        proposal: Proposal,
        marks: Vec<StageMark>,
        applies: DraftApplies,
        swap: bool,
        undo: bool,
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
            undo: false,
        }
    }

    struct Env {
        stored: Vec<Agent>,
        tools: Tools,
        setup: FlowSetup,
        keys: KeyLabels,
        captions: BTreeMap<String, String>,
        writing: Vec<SkillWriting>,
        written: Vec<WrittenSkill>,
        choices: Choices,
    }

    fn env(setup: FlowSetup) -> Env {
        Env {
            stored: stored(),
            tools: setup::tools(),
            setup,
            keys: KeyLabels::default(),
            captions: BTreeMap::from([(
                "security-review".to_owned(),
                "A security agent reviews the change for risky code.".to_owned(),
            )]),
            writing: Vec::new(),
            written: Vec::new(),
            choices: Choices::default(),
        }
    }

    /// [`super::edited`] with the stored agents.
    fn edited(file: &FlowFile, edit: &StageEdit) -> Result<(FlowFile, Option<String>), String> {
        super::edited(file, edit, &stored())
    }

    fn not_asked() -> FlowSetup {
        setup::setup(FlowSetupRow::NotAsked, FlowSetupRow::NotAsked)
    }

    #[allow(clippy::too_many_arguments)]
    fn render(
        e: &Env,
        opened: Opened,
        d: &Draft,
        file: &FlowFile,
        in_use: &FlowFile,
        selected: Option<&str>,
        agent: Option<&ProjectAgent>,
    ) -> View {
        view(&Facts {
            project: "SHOP",
            project_name: "shop-web",
            opened,
            proposal: &d.proposal,
            marks: &d.marks,
            can_undo_all: d.undo,
            swap: d.swap,
            applies: &d.applies,
            file,
            in_use,
            stored: &e.stored,
            tools: &e.tools,
            setup: &e.setup,
            projects: &[("BILL".to_owned(), "billing-api".to_owned())],
            captions: &e.captions,
            writing: &e.writing,
            written: &e.written,
            selected,
            agent,
            new_agent: None,
            keys: &e.keys,
            choices: &e.choices,
        })
    }

    fn names(v: &View) -> Vec<&str> {
        v.flow.steps.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn the_editor_from_otaq_default_in_onboarding() {
        let e = env(not_asked());
        let file = otaq_default();
        let d = draft(&file, true);
        let v = render(&e, Opened::Onboarding, &d, &file, &file, None, None);
        assert_eq!(
            names(&v),
            ["Ready for Dev", "Dev", "Validate", "Review", "Done"]
        );
        assert_eq!(v.flow.title, "shop-web\u{2019}s flow");
        let default = format!("{} default", otaq_core::naming::PRODUCT_NAME);
        assert_eq!(
            v.flow.summary,
            format!("5 steps \u{b7} starting from {default}")
        );
        let entries: Vec<(&str, usize, bool)> = v
            .start_from
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.count, e.chosen))
            .collect();
        assert_eq!(
            entries,
            [
                (default.as_str(), 5, true),
                ("Solo autopilot", 4, false),
                ("Grill first", 6, false),
                ("Team PR", 4, false)
            ]
        );
        assert_eq!(
            v.start_from.entries[0].line,
            "Agents build and check; you review"
        );
        // The animation: the project's own key, long words for five steps, fixed captions for Ready, You check and
        // Delivery.
        assert_eq!(v.flow.ticket_key, "SHOP-1");
        let tiles: Vec<(&str, &str, &str)> = v
            .flow
            .tiles
            .iter()
            .map(|t| (t.name.as_str(), t.kind.as_str(), t.ticket.as_str()))
            .collect();
        assert_eq!(
            tiles,
            [
                ("Ready for Dev", "Waits", "Waiting"),
                ("Dev", "An agent works", "Working \u{b7} Developer"),
                ("Validate", "An agent checks", "Checking \u{b7} Validator"),
                ("Review", "You check", "Your check \u{b7} You"),
                ("Done", "Delivery", "Merged")
            ]
        );
        assert_eq!(v.flow.tiles[0].caption.as_deref(), Some(READY_CAPTION));
        assert_eq!(
            v.flow.tiles[2].caption.as_deref(),
            Some(
                "A checking agent runs the tests and lint. If something fails, the ticket goes back to Dev."
            )
        );
        assert_eq!(v.flow.tiles[3].caption.as_deref(), Some(YOU_CHECK_CAPTION));
        assert_eq!(v.flow.tiles[4].caption.as_deref(), Some(MERGE_CAPTION));
        assert_eq!(v.flow.change_line, None);
        assert_eq!(v.footer.cancel, "Skip, use the default");
        assert_eq!(v.footer.apply, "Use this flow");
        assert!(v.footer.apply_enabled);
        assert!(v.flow.details.is_none() && !v.flow.folded);
    }

    #[test]
    fn the_editor_from_shop_webs_flow_in_the_project() {
        let e = env(not_asked());
        let file = shop_web();
        let d = draft(&file, true);
        let v = render(&e, Opened::Project, &d, &file, &file, None, None);
        assert_eq!(
            names(&v),
            [
                "Ready for Dev",
                "Grill",
                "Dev",
                "Validate",
                "Security",
                "Review",
                "Done"
            ]
        );
        assert_eq!(v.flow.summary, "7 steps \u{b7} in use");
        let in_use = &v.start_from.entries[0];
        assert_eq!(
            (
                in_use.name.as_str(),
                in_use.count,
                in_use.chosen,
                in_use.line.as_str()
            ),
            (
                "shop-web\u{2019}s flow",
                7,
                true,
                "In use \u{b7} the board\u{2019}s columns"
            )
        );
        assert_eq!(in_use.badge.as_ref().map(|b| b.label), Some("In use"));
        assert!(v.start_from.entries[1..].iter().all(|e| !e.chosen));
        let kinds: Vec<&str> = v.flow.steps.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "Waits",
                "Agent works",
                "Agent works",
                "Agent checks",
                "Agent checks",
                "You check",
                "Delivery \u{b7} PR"
            ]
        );
        assert_eq!(v.flow.steps[2].who.as_deref(), Some("Rust developer"));
        assert_eq!(v.flow.steps[6].who.as_deref(), Some("PR"));
        assert!(!v.flow.steps[0].draggable && !v.flow.steps[6].draggable);
        assert!(!v.flow.steps[6].can_add_after);
        assert_eq!(v.flow.add_stage_after.as_deref(), Some("review"));
        // Short tile words past five steps; the Security caption is its imported skill's description.
        assert_eq!(v.flow.tiles[1].ticket, "Asks you a question");
        assert_eq!(v.flow.tiles[4].kind, "Check");
        assert_eq!(
            v.flow.tiles[4].caption.as_deref(),
            Some("A security agent reviews the change for risky code.")
        );
        assert_eq!(v.flow.tiles[6].caption.as_deref(), Some(PR_CAPTION));
        assert_eq!(v.flow.tiles[6].ticket, "PR open");
        assert_eq!(v.footer.apply, "No changes");
        assert!(!v.footer.apply_enabled);
        assert_eq!(v.footer.cancel, "Cancel");
        let chips: Vec<&str> = v.agent.chips.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(chips, ["Why is Security here?", "Make Review optional"]);
        assert_eq!(
            v.start_from
                .import
                .menu
                .iter()
                .map(|m| m.label.as_str())
                .collect::<Vec<_>>(),
            [
                "A file\u{2026}",
                "FROM A PROJECT",
                "billing-api\u{2019}s flow"
            ]
        );
    }

    #[test]
    fn a_steps_details() {
        let e = env(not_asked());
        let file = shop_web();
        let d = draft(&file, true);
        let v = render(&e, Opened::Project, &d, &file, &file, Some("dev"), None);
        assert!(v.flow.folded);
        assert!(v.flow.steps[2].selected);
        let details = v.flow.details.expect("Dev's details");
        assert_eq!(details.title, "Dev");
        assert_eq!(details.name, "Dev");
        assert_eq!(details.kind, Some(EditorKind::Works));
        assert_eq!(
            details.kinds.iter().map(|k| k.label).collect::<Vec<_>>(),
            ["An agent works", "An agent checks", "You check"]
        );
        let agent = details.agent.expect("an agent");
        assert_eq!(agent.name, "Rust developer");
        assert_eq!(
            agent.hint,
            "Change it here, or manage agents in Settings \u{2192} Agents."
        );
        assert!(
            agent
                .items
                .iter()
                .any(|i| i.label == "Rust developer" && i.checked)
        );
        assert_eq!(
            details.skill.map(|s| s.state),
            Some(SkillState::Set {
                skill: "work-ticket".into()
            })
        );
        let can: Vec<(&str, bool, bool)> = details
            .can
            .iter()
            .map(|c| (c.label.as_str(), c.on, c.enabled))
            .collect();
        assert_eq!(
            can,
            [
                ("skip it", false, true),
                ("come back here when a later check fails", true, false)
            ]
        );
        assert_eq!(details.remove, Some("Remove stage"));
        // The chip asks about the selected step's skill.
        assert_eq!(v.agent.chips[0].label, "What does work-ticket do?");

        // The Delivery: its kind is fixed and it can't be removed; Review has no agent.
        let v = render(&e, Opened::Project, &d, &file, &file, Some("deliver"), None);
        let details = v.flow.details.expect("Done's details");
        assert_eq!(details.fixed_kind, Some("Delivery"));
        assert!(details.kinds.is_empty() && details.remove.is_none());
        let v = render(&e, Opened::Project, &d, &file, &file, Some("review"), None);
        let details = v.flow.details.expect("Review's details");
        assert!(details.agent.is_none() && details.skill.is_none());
        assert_eq!(details.kind, Some(EditorKind::YouCheck));
    }

    #[test]
    fn which_tool_with_two_tools_and_with_one() {
        let e = env(not_asked());
        let file = otaq_default();
        let d = draft(&file, true);
        let v = render(&e, Opened::Onboarding, &d, &file, &file, None, None);
        let tools: Vec<(&str, &str, Option<&str>)> = v
            .start_from
            .made_for
            .tools
            .iter()
            .map(|t| (t.name, t.status.as_str(), t.ask))
            .collect();
        assert_eq!(
            tools,
            [
                ("Claude Code", "Connected", Some("Ask")),
                ("OpenCode", "Connected", Some("Ask"))
            ]
        );
        assert_eq!(v.start_from.made_for.title, "MADE FOR SHOP-WEB");
        assert_eq!(v.agent.chips[0].label, "Make one for shop-web\u{2026}");
        assert_eq!(v.agent.chips[0].action, ChipAction::ChooseTool);
        let chooser = v.agent.chooser.as_ref().expect("the chooser");
        assert_eq!(chooser.title, "Which tool should read how you work?");
        assert_eq!(
            chooser.tools.iter().map(|t| t.name).collect::<Vec<_>>(),
            ["Claude Code", "OpenCode"]
        );
        assert_eq!(
            chooser.both,
            Some((
                "Both, and compare".to_owned(),
                "Two flows under Made for shop-web".to_owned()
            ))
        );
        assert_eq!(
            chooser.foot,
            "Uses tokens \u{b7} read by that tool\u{2019}s provider"
        );
        // No session counts or "most used" anywhere: tools by name only.
        let text = serde_json::to_string(&v.start_from.made_for).unwrap()
            + &serde_json::to_string(&v.agent).unwrap();
        assert!(!text.to_lowercase().contains("most used"));
        assert!(
            !text.contains("sessions"),
            "no session counts before a proposal"
        );

        // One tool: "Ask Claude Code", no chooser.
        let mut one = not_asked();
        one.tools.truncate(1);
        let e = env(one);
        let v = render(&e, Opened::Onboarding, &d, &file, &file, None, None);
        assert_eq!(v.agent.chips[0].label, "Ask Claude Code");
        assert!(v.agent.chooser.is_none());

        // A proposal from Claude Code is its own entry; OpenCode can still be asked.
        let mut asked = setup::setup(
            FlowSetupRow::Proposed {
                agent: setup::CC.into(),
                proposal: 4,
            },
            FlowSetupRow::NotAsked,
        );
        let mut entry = proposal(&shop_web(), false);
        entry.id = 4;
        entry.proposed_by = ProposedBy::ProjectAgent {
            agent: setup::CC.into(),
            session: "s".into(),
            name: "Flow setup \u{b7} Claude Code".into(),
            tool: setup::CLAUDE,
        };
        asked.proposals.push(entry);
        let e = env(asked);
        let v = render(&e, Opened::Onboarding, &d, &file, &file, None, None);
        let made = &v.start_from.made_for;
        assert_eq!(made.entries.len(), 1);
        assert_eq!(
            (
                made.entries[0].name.as_str(),
                made.entries[0].line.as_str(),
                made.entries[0].count
            ),
            ("By Claude Code", "From your Claude Code sessions", 7)
        );
        assert_eq!(made.entries[0].pick, Pick::Entry { proposal: 4 });
        assert_eq!(
            made.tools
                .iter()
                .map(|t| (t.name, t.status.as_str()))
                .collect::<Vec<_>>(),
            [("OpenCode", "Connected \u{b7} not asked")]
        );
        // In the project there is no Both (Q-A).
        let e = env(not_asked());
        let v = render(&e, Opened::Project, &d, &file, &file, None, None);
        assert!(v.agent.chooser.is_some_and(|c| c.both.is_none()));
        let _ = FlowSetupTool {
            tool: setup::CLAUDE,
            row: FlowSetupRow::NotAsked,
        };
    }

    #[test]
    fn a_proposal_tinted_with_undo() {
        let e = env(not_asked());
        let base = otaq_default();
        let mut file = base.clone();
        let (with_grill, _) = edited(&file, &StageEdit::Add { after: None }).unwrap();
        file = with_grill;
        file.flow.stages[0].name = "Grill".into();
        file.flow.stages[0].id = "grill".into();
        file.flow.stages[0].skill = Some("grill".into());
        file.flow.stages[0].agent = Some("Griller".into());
        file.flow
            .stages
            .last_mut()
            .unwrap()
            .delivery
            .as_mut()
            .unwrap()
            .delivers = Delivers::Pr;
        let mut d = draft(&file, false);
        d.marks = vec![
            StageMark {
                stage_id: "grill".into(),
                stage: "Grill".into(),
                mark: Mark::Added,
                by: "Claude Code".into(),
                why: Some("From 12 /grill-me runs".into()),
            },
            StageMark {
                stage_id: "deliver".into(),
                stage: "Done".into(),
                mark: Mark::Changed,
                by: "Claude Code".into(),
                why: None,
            },
        ];
        d.undo = true;
        let v = render(&e, Opened::Onboarding, &d, &file, &base, None, None);
        let grill = &v.flow.steps[1];
        let mark = grill.mark.as_ref().expect("Grill is tinted");
        assert_eq!(mark.label, "Added");
        assert_eq!(mark.why, "Changed by Claude Code: From 12 /grill-me runs");
        assert_eq!(mark.undo, "Undo");
        assert_eq!(
            v.flow.steps.last().unwrap().mark.as_ref().map(|m| m.label),
            Some("Changed")
        );
        assert!(v.flow.steps[2].mark.is_none());
        assert_eq!(v.flow.tiles[1].caption_mark, Some("New step \u{b7}"));
        assert_eq!(v.flow.undo_all, Some("Undo all"));
        assert_eq!(
            v.flow.summary,
            format!(
                "6 steps \u{b7} {} default, changed by Claude Code",
                otaq_core::naming::PRODUCT_NAME
            )
        );
    }

    #[test]
    fn the_idle_agent_panel_starts_nothing() {
        let e = env(not_asked());
        let file = shop_web();
        let d = draft(&file, true);
        let v = render(&e, Opened::Project, &d, &file, &file, None, None);
        let panel = &v.agent;
        assert_eq!(panel.title, "Noyan \u{b7} the flow");
        assert_eq!(panel.pill.label, "Not started");
        assert_eq!(panel.note, "\u{b7} starts when you ask");
        assert!(panel.agent.is_none() && panel.session.is_none());
        assert_eq!(
            panel.hint,
            [
                "Ask about shop-web\u{2019}s flow, or to change it. Nothing on the board changes until you apply, and \
              nothing runs until you ask."
            ]
        );
        assert_eq!(panel.foot, "Typing goes to the agent");
        // The chips' words are fixed: each sends itself as the first message.
        assert!(
            panel
                .chips
                .iter()
                .all(|c| matches!(&c.action, ChipAction::Send { message } if *message == c.label))
        );

        // Once asked: its state, tokens and terminal; the chips go.
        let agent = ProjectAgent {
            id: "pa-1".into(),
            project: "SHOP".into(),
            name: "Flow".into(),
            name_source: None,
            state: AgentState::Running,
            tool: setup::CLAUDE,
            tool_session_id: None,
            model: Some("sonnet".into()),
            got_request: true,
            mid_turn: true,
            limit_stopped: false,
            created_at: 0,
            ended_at: None,
            closed: false,
            question: None,
            needs_you: None,
            focused: false,
            tokens: otaq_core::usage::Tokens {
                input: 12_000,
                ..Default::default()
            },
            session: None,
            epic: None,
            turn_ended_at: None,
        };
        let v = render(&e, Opened::Project, &d, &file, &file, None, Some(&agent));
        assert_eq!(v.agent.pill.label, "Working");
        assert_eq!(v.agent.line, "Claude Code \u{b7} sonnet");
        assert_eq!(v.agent.note, "\u{b7} 12k");
        assert_eq!(v.agent.agent.as_deref(), Some("pa-1"));
        assert!(v.agent.chips.is_empty());
        let waiting = ProjectAgent {
            mid_turn: false,
            ..agent
        };
        let v = render(&e, Opened::Project, &d, &file, &file, None, Some(&waiting));
        assert_eq!(v.agent.pill.label, "Waiting for you");
    }

    #[test]
    fn the_change_line_names_the_span_and_the_tickets_in_it() {
        let e = env(not_asked());
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
                inside("SHOP-22", "Dev"),
                inside("SHOP-33", "Dev"),
                inside("SHOP-29", "Dev"),
                inside("SHOP-30", "Dev"),
                inside("SHOP-19", "Security"),
                inside("SHOP-32", "Security"),
            ],
        };
        let v = render(&e, Opened::Project, &d, &file, &in_use, None, None);
        assert_eq!(
            v.flow.change_line.as_deref(),
            Some(
                "Security moved before Validate. This waits until no ticket is between Dev and Review: the 6 there \
                 now (4 in Dev, 2 in Security) finish on the current order, and until then no ticket enters Dev to \
                 Review. Tickets before Dev keep moving. The banner offers Apply now."
            )
        );
        assert_eq!(v.flow.summary, "7 steps \u{b7} 1 change, not applied");
        assert_eq!(v.footer.apply, "Apply to shop-web");
        assert!(v.footer.apply_enabled);
    }

    #[test]
    fn problems_show_on_their_step_and_keep_apply_off() {
        let e = env(not_asked());
        let file = shop_web();
        let mut d = draft(&file, false);
        d.proposal.problems.push(ProposalProblem {
            about: About::Stage {
                id: "security".into(),
                name: "Security".into(),
            },
            message: "Security names the agent Security reviewer, which isn't one of your agents"
                .into(),
            fix: None,
        });
        let v = render(&e, Opened::Project, &d, &file, &file, None, None);
        assert_eq!(
            v.flow.steps[4].problem.as_deref(),
            Some("Security names the agent Security reviewer, which isn't one of your agents")
        );
        assert!(!v.footer.apply_enabled);
        assert_eq!(v.footer.reason, v.flow.steps[4].problem);
    }

    /// Q426 (FED-04, FLW-04's controls): each problem that needs a pick marks its step and shows its fix in the step's
    /// details (a missing skill: Import its folder…; a model not listed or a tool not connected: the model menu); an
    /// import clash shows Keep mine (the default) · Take theirs · Add as. Each pick clears its problem; Apply turns on
    /// after the last, and sends the picks.
    #[test]
    fn problems_that_need_a_pick_show_their_fix_on_the_step() {
        use otaq_core::flow::proposal::{AgentField as Field, Clash};
        let mut e = env(not_asked());
        let mut file = shop_web();
        file.agents[1].model = "claude:opus-9".into();
        file.agents[3].model = format!("{}:gpt-5.5", Tool::ALL[1].as_str());
        let mut d = draft(&file, false);
        d.proposal.source = Source::Document {
            file: Some("team.yaml".into()),
        };
        let missing = |about: About| ProposalProblem {
            about,
            message: "Security names the skill `security-review`, which isn't on this machine"
                .into(),
            fix: Some(Fix::ImportSkillFolder {
                skill: "security-review".into(),
            }),
        };
        let model = |agent: &str| ProposalProblem {
            about: About::Agent { name: agent.into() },
            message: format!(
                "{agent}'s model isn't in its tool's model list: pick a model from it"
            ),
            fix: Some(Fix::PickModel {
                agent: agent.into(),
            }),
        };
        d.proposal.problems = vec![
            missing(About::Stage {
                id: "security".into(),
                name: "Security".into(),
            }),
            missing(About::Agent {
                name: "Security reviewer".into(),
            }),
            model("Rust developer"),
            model("Security reviewer"),
        ];
        d.proposal.diff.agents = vec![AgentChange {
            name: "Validator".into(),
            change: AgentChangeKind::Changed,
            fields: vec![Field {
                field: "instructions".into(),
                from: String::new(),
                to: "Run the tests.".into(),
            }],
            used_in: Vec::new(),
            clash: Some(Clash {
                add_as: Some("Validator (team.yaml)".into()),
            }),
        }];
        let codex = Tool::ALL[1].display_name();
        let claude = Tool::ALL[0].display_name();
        let not_listed =
            format!("Rust developer: opus-9 isn\u{2019}t in {claude}\u{2019}s model list");
        let not_connected = format!("Security reviewer: {codex} isn\u{2019}t connected");

        // Each step with a problem is marked; Apply is off with the first.
        let v = render(&e, Opened::Project, &d, &file, &file, None, None);
        let problem = |v: &View, id: &str| {
            v.flow
                .steps
                .iter()
                .find(|s| s.id == id)
                .and_then(|s| s.problem.clone())
        };
        assert_eq!(problem(&v, "dev"), Some(not_listed.clone()));
        assert_eq!(
            problem(&v, "security").as_deref(),
            Some("security-review isn\u{2019}t on this machine")
        );
        assert_eq!(problem(&v, "validate"), None, "a clash keeps Apply on");
        assert!(v.flow.problems.is_empty() && v.flow.fixes.is_empty());
        assert!(!v.footer.apply_enabled);
        assert_eq!(
            v.footer.reason.as_deref(),
            Some("security-review isn\u{2019}t on this machine")
        );

        // Security: its missing skill once (the stage and its agent name it), then its agent's model.
        let fixes = |v: &View| v.flow.details.as_ref().unwrap().fixes.clone();
        let v = render(
            &e,
            Opened::Project,
            &d,
            &file,
            &file,
            Some("security"),
            None,
        );
        let security = fixes(&v);
        assert_eq!(
            security[0],
            FixRow {
                text: "security-review isn\u{2019}t on this machine".into(),
                fixed: false,
                control: FixControl::Folder {
                    skill: "security-review".into(),
                    label: "Import its folder\u{2026}",
                    picker_title: "The folder of security-review".into(),
                    picked: None,
                },
            }
        );
        assert_eq!(security[1].text, not_connected);
        assert_eq!(security.len(), 2);
        assert_eq!(v.flow.details.as_ref().unwrap().problem, None);

        // Dev: the model menu, the default model and the connected tools.
        let v = render(&e, Opened::Project, &d, &file, &file, Some("dev"), None);
        let [dev] = fixes(&v).try_into().unwrap();
        assert_eq!((dev.text.as_str(), dev.fixed), (not_listed.as_str(), false));
        let FixControl::Model { fix } = &dev.control else {
            panic!("the model menu: {dev:?}");
        };
        let tools: Vec<(&str, bool)> = fix
            .tools
            .iter()
            .map(|c| (c.label.as_str(), c.selected))
            .collect();
        assert_eq!(
            tools,
            [
                ("Default model", false),
                (claude, false),
                (Tool::ALL[3].display_name(), false)
            ]
        );
        assert_eq!(fix.picker, None);

        // Validate: the clash, Keep mine picked.
        let v = render(
            &e,
            Opened::Project,
            &d,
            &file,
            &file,
            Some("validate"),
            None,
        );
        let [clash] = fixes(&v).try_into().unwrap();
        assert_eq!(
            clash.text,
            "Validator is in the file and already here, with other instructions."
        );
        assert!(clash.fixed);
        let FixControl::Clash { agent, options } = &clash.control else {
            panic!("the clash's choices: {clash:?}");
        };
        assert_eq!(agent, "Validator");
        let options: Vec<(&str, &str, bool)> = options
            .iter()
            .map(|o| (o.id.as_str(), o.label.as_str(), o.selected))
            .collect();
        assert_eq!(
            options,
            [
                ("keep_mine", "Keep mine", true),
                ("take_theirs", "Take theirs", false),
                (
                    "add_as",
                    "Add as \u{201c}Validator (team.yaml)\u{201d}",
                    false
                )
            ]
        );
        assert_eq!(
            v.choices.clashes.get("Validator"),
            Some(&ClashPick::KeepMine)
        );

        // The folder and a tool picked: those problems are gone from their steps.
        e.choices
            .skill_folders
            .insert("security-review".into(), "/skills/security-review".into());
        e.choices
            .models
            .insert("Rust developer".into(), Tool::ALL[0].as_str().into());
        let v = render(
            &e,
            Opened::Project,
            &d,
            &file,
            &file,
            Some("security"),
            None,
        );
        assert_eq!(problem(&v, "dev"), None);
        assert_eq!(problem(&v, "security"), Some(not_connected.clone()));
        assert_eq!(v.footer.reason, Some(not_connected.clone()));
        let security = fixes(&v);
        assert!(security[0].fixed);
        assert!(matches!(
            &security[0].control,
            FixControl::Folder { picked: Some(p), .. } if p == "/skills/security-review"
        ));
        let v = render(&e, Opened::Project, &d, &file, &file, Some("dev"), None);
        let [dev] = fixes(&v).try_into().unwrap();
        let FixControl::Model { fix } = &dev.control else {
            panic!("the model menu: {dev:?}");
        };
        assert!(dev.fixed);
        assert!(fix.tools[1].selected);
        assert!(fix.picker.is_some(), "the tool's models follow");

        // The last one picked: Apply is on and sends every pick.
        e.choices
            .models
            .insert("Security reviewer".into(), DEFAULT.into());
        e.choices
            .clashes
            .insert("Validator".into(), ClashPick::AddAs);
        let v = render(&e, Opened::Project, &d, &file, &file, None, None);
        assert!(v.flow.steps.iter().all(|s| s.problem.is_none()));
        assert!(v.footer.apply_enabled);
        assert_eq!(v.footer.reason, None);
        let sent = v.choices.apply_choices();
        assert_eq!(sent.clashes.get("Validator"), Some(&ClashChoice::AddAs));
        assert_eq!(
            sent.skill_folders
                .get("security-review")
                .map(String::as_str),
            Some("/skills/security-review")
        );
        assert_eq!(sent.models.len(), 2);
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
        // Add after Grill: a new agent stage with the kind's default agent and no skill (Blank), selected.
        let (added, select) = edited(
            &file,
            &StageEdit::Add {
                after: Some("grill".into()),
            },
        )
        .unwrap();
        assert_eq!(select.as_deref(), Some("new-stage"));
        assert_eq!(ids(&added)[..3], ["grill", "new-stage", "dev"]);
        let new = added.flow.stage("new-stage").unwrap();
        assert_eq!(
            (new.agent.as_deref(), new.skill.as_deref()),
            (Some("Developer"), None)
        );
        let (again, select) = edited(&added, &StageEdit::Add { after: None }).unwrap();
        assert_eq!(select.as_deref(), Some("new-stage-2"));
        assert_eq!(again.flow.stages[0].name, "New stage 2");
        // The document reads back.
        assert!(otaq_core::flow::yaml::read(&otaq_core::flow::yaml::write(&again)).is_ok());
        // Move: never past the Delivery.
        let (moved, _) = edited(
            &file,
            &StageEdit::Move {
                stage: "grill".into(),
                to: 99,
            },
        )
        .unwrap();
        assert_eq!(
            ids(&moved),
            ["dev", "validate", "security", "review", "grill", "deliver"]
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
        assert!(s.is_human_check() && !s.has_agent() && s.rounds.is_none() && s.fail_to.is_none());
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
            (s.check_agents(), s.skill.as_deref(), s.rounds),
            (&["Validator".to_owned()][..], Some("validate"), Some(3))
        );
        assert!(otaq_core::flow::yaml::read(&otaq_core::flow::yaml::write(&back)).is_ok());
        // Rename, agent, an option.
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
        let (agent, _) = edited(
            &file,
            &StageEdit::Agent {
                stage: "dev".into(),
                agent: "Developer".into(),
            },
        )
        .unwrap();
        assert_eq!(
            agent.flow.stage("dev").unwrap().agent.as_deref(),
            Some("Developer")
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
    }

    #[test]
    fn the_gallery_builds() {
        assert_eq!(gallery().cells.len(), 8);
    }

    fn plan_details(v: &View) -> &Details {
        v.flow.details.as_ref().expect("Plan's details")
    }

    /// FED-05, FED-06 (`s27-05`): Plan picked in a new stage after Grill fills the name and kind; the skill is
    /// empty with both options; the step says it needs a skill; Apply is off with the reason; the panel offers to
    /// write it.
    #[test]
    fn plan_picked_has_an_empty_skill_with_both_options() {
        let e = env(not_asked());
        let shop = shop_web();
        let (file, d) = plan_draft(None);
        let d = Draft {
            proposal: d,
            ..draft(&file, false)
        };
        let v = render(&e, Opened::Project, &d, &file, &shop, Some("plan"), None);
        let details = plan_details(&v);
        let t = details.templates.as_ref().expect("a new stage's templates");
        assert_eq!(t.title, "New stage after Grill");
        assert_eq!(
            (t.label, t.plain_note),
            ("Start from", "name and kind; no skill")
        );
        let plain: Vec<(&str, bool)> = t.plain.iter().map(|c| (c.label, c.chosen)).collect();
        assert_eq!(
            plain,
            [
                ("Plan", true),
                ("Security check", false),
                ("Visual check", false),
                ("Docs", false),
                ("Blank", false)
            ]
        );
        assert_eq!(
            t.presets_label,
            format!("From {}\u{2019}s presets", otaq_core::naming::PRODUCT_NAME)
        );
        assert_eq!(t.presets_note, "complete, skill included");
        let presets: Vec<&str> = t.presets.iter().map(|c| c.label).collect();
        assert_eq!(presets, ["Grill", "Validate", "Review", "PR delivery"]);
        assert_eq!(details.kind, Some(EditorKind::Works));
        let agent = details.agent.as_ref().unwrap();
        assert_eq!(
            (agent.name.as_str(), agent.note.as_deref()),
            ("Developer", Some("default for An agent works"))
        );
        let skill = details.skill.as_ref().unwrap();
        assert_eq!(
            skill.state,
            SkillState::Empty {
                text: "No skill yet. An agent stage needs one: it\u{2019}s what the agent follows in Plan.".into(),
                pick: "Pick a skill\u{2026}",
                ask: "Ask the agent to write it",
                hint: "Pick: your own skills, searchable. Ask: starts Noyan on the right and uses tokens.",
                prefill: "Write a skill for the Plan stage: ".into(),
            }
        );
        // Pick a skill…: the user's imported skills, with their captions.
        let items: Vec<(&str, Option<&str>)> = skill
            .picker
            .items
            .iter()
            .map(|i| (i.id.as_str(), i.hint.as_deref()))
            .collect();
        assert_eq!(
            items,
            [(
                "security-review",
                Some("A security agent reviews the change for risky code.")
            )]
        );
        // The step: "needs a skill" in the agent's place.
        let step = &v.flow.steps[2];
        assert_eq!(step.name, "Plan");
        assert_eq!(step.problem.as_deref(), Some("needs a skill"));
        assert_eq!(step.who, None);
        assert_eq!(v.flow.summary, "8 steps \u{b7} 1 change, not applied");
        // Apply is off with the reason.
        assert!(!v.footer.apply_enabled);
        assert_eq!(v.footer.reason.as_deref(), Some("Plan needs a skill"));
        // The panel offers to write it; the chip does what the button does.
        assert_eq!(
            v.agent.hint,
            [
                "Plan needs a skill. Ask here to have one written (it reads this repo and your other skills, and uses \
              tokens), or pick one of yours in the stage."
            ]
        );
        let chips: Vec<(&str, &ChipAction)> = v
            .agent
            .chips
            .iter()
            .map(|c| (c.label.as_str(), &c.action))
            .collect();
        assert_eq!(
            chips,
            [
                (
                    "Write the skill",
                    &ChipAction::WriteSkill {
                        stage: "plan".into(),
                        prefill: "Write a skill for the Plan stage: ".into(),
                    }
                ),
                (
                    "What should Plan do?",
                    &ChipAction::Send {
                        message: "What should Plan do?".into()
                    }
                )
            ]
        );
        // Q441: the request is only prefilled, the caret after it; the user finishes it and sends.
        assert_eq!(
            write_skill_prefill(&file, "plan"),
            "Write a skill for the Plan stage: "
        );
        // A stage of the flow in use has no templates.
        let v = render(&e, Opened::Project, &d, &file, &shop, Some("dev"), None);
        assert!(plan_details(&v).templates.is_none());
    }

    /// FED-06 (`s27-06`): while the agent writes it the field says so with Stop, and Apply's reason says it too;
    /// once written, the skill is tinted with Open and Undo, the step is marked New, and Apply is on.
    #[test]
    fn the_skill_being_written_then_written() {
        let mut e = env(not_asked());
        let shop = shop_web();
        let (file, d) = plan_draft(None);
        let d = Draft {
            proposal: d,
            ..draft(&file, false)
        };
        e.writing = vec![SkillWriting {
            stage_id: "plan".into(),
            agent: Some("pa-1".into()),
        }];
        let v = render(&e, Opened::Project, &d, &file, &shop, Some("plan"), None);
        assert_eq!(
            plan_details(&v).skill.as_ref().unwrap().state,
            SkillState::Writing {
                text: "Being written by Noyan\u{2026}",
                stop: "Stop",
            }
        );
        assert_eq!(
            v.footer.reason.as_deref(),
            Some("Plan needs a skill \u{b7} being written")
        );
        assert!(!v.footer.apply_enabled);
        assert!(
            !v.agent.chips.iter().any(|c| c.label == "Write the skill"),
            "it is being written already"
        );

        let (file, d) = plan_draft(Some("plan-ticket"));
        let d = Draft {
            proposal: d,
            ..draft(&file, false)
        };
        e.writing.clear();
        e.written = vec![written_plan()];
        let v = render(&e, Opened::Project, &d, &file, &shop, Some("plan"), None);
        assert_eq!(
            plan_details(&v).skill.as_ref().unwrap().state,
            SkillState::Written {
                skill: "plan-ticket".into(),
                note: "Written by the agent",
                why: "Written by Checkout slowness".into(),
                open: "Open",
                undo: "Undo",
            }
        );
        let step = &v.flow.steps[2];
        assert_eq!(step.written.as_ref().map(|m| m.label), Some("New"));
        assert_eq!(step.problem, None);
        assert_eq!(step.who.as_deref(), Some("Developer"));
        assert!(v.footer.apply_enabled);
        assert_eq!(v.footer.reason, None);
        // Another skill picked over it: the field shows that one, untinted.
        let mut picked = file.clone();
        picked.flow.stages[1].skill = Some("security-review".into());
        let v = render(&e, Opened::Project, &d, &picked, &shop, Some("plan"), None);
        assert_eq!(
            plan_details(&v).skill.as_ref().unwrap().state,
            SkillState::Set {
                skill: "security-review".into()
            }
        );
    }

    /// FED-05: the plain templates fill the name and the kind (no skill); the presets' come complete, bringing an
    /// agent the user doesn't have; PR delivery becomes the flow's Delivery. A kind switch keeps an empty skill empty.
    /// Pick a skill… gives the stage's agent the skill.
    #[test]
    fn templates_and_a_picked_skill_change_the_document() {
        let (added, new) = edited(
            &shop_web(),
            &StageEdit::Add {
                after: Some("grill".into()),
            },
        )
        .unwrap();
        let new = new.unwrap();
        let template = |t: Template| {
            edited(
                &added,
                &StageEdit::Template {
                    stage: new.clone(),
                    template: t,
                },
            )
            .unwrap()
        };
        let (security, id) = template(Template::Security);
        assert_eq!(id.as_deref(), Some("security-check"));
        let s = security.flow.stage("security-check").unwrap();
        assert_eq!(
            (
                s.name.as_str(),
                s.kind,
                the_agent(s).map(String::as_str),
                s.skill.as_deref(),
                s.rounds
            ),
            (
                "Security check",
                StageKind::Check { by: CheckBy::Agent },
                Some("Validator"),
                None,
                Some(3)
            )
        );
        let (blank, id) = template(Template::Blank);
        let s = blank.flow.stage(id.as_deref().unwrap()).unwrap();
        assert_eq!((s.name.as_str(), s.skill.as_deref()), ("New stage", None));
        // From the presets: Grill complete (named Grill 2 beside shop-web's own), its agent kept.
        let (grill, id) = template(Template::Grill);
        let s = grill.flow.stage(id.as_deref().unwrap()).unwrap();
        assert_eq!(
            (s.name.as_str(), s.agent.as_deref(), s.skill.as_deref()),
            ("Grill 2", Some("Griller"), Some("grill"))
        );
        let (validate, id) = template(Template::Validate);
        let s = validate.flow.stage(id.as_deref().unwrap()).unwrap();
        assert_eq!((s.skill.as_deref(), s.rounds), (Some("validate"), Some(3)));
        // PR delivery: the flow's Delivery becomes Team PR's (its id kept), the new stage goes, Shipper comes along.
        let (pr, id) = template(Template::PrDelivery);
        assert_eq!(id.as_deref(), Some("deliver"));
        assert!(pr.flow.stage(&new).is_none());
        let d = pr.flow.stage("deliver").unwrap();
        assert_eq!(
            (
                d.agent.as_deref(),
                d.skill.as_deref(),
                d.delivery.map(|x| x.delivers)
            ),
            (Some("Shipper"), Some("open-pr"), Some(Delivers::Pr))
        );
        assert!(pr.agents.iter().any(|a| a.name == "Shipper"));
        assert!(otaq_core::flow::yaml::read(&otaq_core::flow::yaml::write(&pr)).is_ok());
        // A kind switch keeps Plan's empty skill empty.
        let (plan, _) = template(Template::Plan);
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
            (the_agent(s).map(String::as_str), s.skill.as_deref()),
            (Some("Validator"), None)
        );
        // The draft with no skill reads back (the checks say "Plan needs a skill").
        let again = otaq_core::flow::yaml::read(&otaq_core::flow::yaml::write(&plan)).unwrap();
        let problems = otaq_core::flow::check(&again.flow).unwrap_err();
        assert_eq!(problems[0].message, "Plan needs a skill");
        // Pick a skill…: Plan runs security-review, and its agent (Developer) gets it.
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
        let developer = picked
            .agents
            .iter()
            .find(|a| a.name == "Developer")
            .unwrap();
        assert_eq!(developer.skills, ["work-ticket", "security-review"]);
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

    /// FED-09 (`s26-08`, `s27-05`): each column ⋯ entry opens the editor over shop-web's flow in use. Edit Grill…
    /// selects Grill with its details; Add a stage after Grill puts a new stage right after it, selected, dashed, its
    /// details open, Apply on; Change the whole flow… opens on the legend with nothing to apply.
    #[test]
    fn each_column_entry_opens_the_editor_in_its_state() {
        let e = env(not_asked());
        let file = shop_web();
        let d = draft(&file, true);

        let edit = render(&e, Opened::Project, &d, &file, &file, Some("grill"), None);
        assert!(edit.flow.steps[1].selected && !edit.flow.steps[1].added);
        assert_eq!(
            edit.flow.details.as_ref().map(|d| d.title.as_str()),
            Some("Grill")
        );

        let (added, select) = edited(
            &file,
            &StageEdit::Add {
                after: Some("grill".into()),
            },
        )
        .unwrap();
        let select = select.expect("the new stage is selected");
        let d = draft(&added, false);
        let add = render(&e, Opened::Project, &d, &added, &file, Some(&select), None);
        let new = &add.flow.steps[2];
        assert_eq!(new.id, select);
        assert!(
            new.selected && new.added,
            "the new stage is dashed and selected"
        );
        assert!(add.flow.steps.iter().filter(|s| s.added).count() == 1);
        assert!(add.flow.details.is_some(), "its details open");
        assert!(add.flow.folded);

        let d = draft(&file, true);
        let whole = render(&e, Opened::Project, &d, &file, &file, None, None);
        assert!(whole.flow.details.is_none() && !whole.flow.folded);
        assert!(whole.flow.steps.iter().all(|s| !s.selected && !s.added));
        assert_eq!(whole.footer.apply, "No changes");
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

    fn add_check_agent(stage: &str, agent: &str) -> StageEdit {
        StageEdit::AddCheckAgent {
            stage: stage.into(),
            agent: agent.into(),
        }
    }

    fn fail_to(stage: &str, to: Option<&str>) -> StageEdit {
        StageEdit::FailTo {
            stage: stage.into(),
            to: to.map(str::to_owned),
        }
    }

    /// T-0529 (d0131, d0130, `s29-04`): every check says where its fails go under its kind; an agent check with two
    /// agents says "2 check agents" in the agent's place.
    #[test]
    fn the_steps_list_says_where_each_checks_fails_go() {
        let e = env(not_asked());
        let file = shop_with(&[
            add_check_agent("validate", "Security reviewer"),
            fail_to("review", Some("grill")),
        ]);
        let d = checked(&file);
        let v = render(&e, Opened::Project, &d, &file, &shop_web(), None, None);
        let rows: Vec<(&str, &str, Option<&str>, Option<&str>)> = v
            .flow
            .steps
            .iter()
            .map(|s| {
                (
                    s.name.as_str(),
                    s.kind.as_str(),
                    s.fails_to.as_deref(),
                    s.who.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("Ready for Dev", "Waits", None, None),
                ("Grill", "Agent works", None, Some("Griller")),
                ("Dev", "Agent works", None, Some("Rust developer")),
                (
                    "Validate",
                    "Agent checks",
                    Some("fails go to Dev"),
                    Some("2 check agents")
                ),
                (
                    "Security",
                    "Agent checks",
                    Some("fails go to Dev"),
                    Some("Security reviewer")
                ),
                (
                    "Review",
                    "You check",
                    Some("fails go to Grill"),
                    Some("You")
                ),
                ("Done", "Delivery \u{b7} PR", None, Some("PR")),
            ]
        );
        assert!(v.flow.steps.iter().all(|s| s.problem.is_none()));
        // The walking ticket names them too; the caption and "come back here" follow fail_to.
        assert_eq!(v.flow.tiles[3].ticket, "Checking");
        let grill = render(
            &e,
            Opened::Project,
            &d,
            &file,
            &shop_web(),
            Some("grill"),
            None,
        );
        let can = grill.flow.details.expect("Grill's details").can;
        assert!(
            can.iter().any(|c| c.option == "back"),
            "Review's fails go to Grill"
        );
    }

    /// d0131: Fails go to lists the Agent stages before the check, the nearest marked and picked by default; picking
    /// another sets `fail_to`, picking the nearest leaves it out; a stage that isn't one is refused.
    #[test]
    fn fails_go_to_offers_the_agent_stages_before_the_check() {
        let e = env(not_asked());
        let file = shop_web();
        let d = draft(&file, true);
        let v = render(&e, Opened::Project, &d, &file, &file, Some("review"), None);
        let details = v.flow.details.expect("Review's details");
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
                ("grill", "Grill", None, false, false),
                ("dev", "Dev", Some("nearest"), true, false),
            ]
        );
        assert_eq!(
            field.explanation,
            "A fail resumes that stage\u{2019}s session with the report or your comments; the card then walks every \
             stage after it again. To send one card somewhere else, drag it back."
        );
        assert_eq!(field.problem, None);
        // A check by you has no agents, no rounds and no rule; "A ticket can: skip it" stays.
        assert!(details.check_agents.is_none() && details.rounds.is_none());
        assert_eq!(details.rule, None);
        assert_eq!(
            details
                .can
                .iter()
                .map(|c| c.label.as_str())
                .collect::<Vec<_>>(),
            ["skip it"]
        );
        // An Agent stage has none.
        let v = render(&e, Opened::Project, &d, &file, &file, Some("dev"), None);
        assert!(v.flow.details.expect("Dev's details").fail_to.is_none());

        // Picking Grill sets it, and it shows; picking the nearest leaves it out again.
        let (to_grill, _) = edited(&file, &fail_to("review", Some("grill"))).unwrap();
        assert_eq!(
            to_grill.flow.stage("review").unwrap().fail_to.as_deref(),
            Some("grill")
        );
        let d = checked(&to_grill);
        let v = render(
            &e,
            Opened::Project,
            &d,
            &to_grill,
            &file,
            Some("review"),
            None,
        );
        let field = v.flow.details.unwrap().fail_to.unwrap();
        assert_eq!(field.value, "Grill");
        assert!(field.items[1].checked && !field.items[2].checked);
        assert!(v.footer.apply_enabled);
        assert_eq!(
            v.flow.change_line.as_deref(),
            Some(
                "This applies at once: tickets that haven\u{2019}t reached a changed step follow it; tickets in it \
                 or past it finish as they are."
            )
        );
        for nearest in [Some("dev"), None] {
            let (back, _) = edited(&to_grill, &fail_to("review", nearest)).unwrap();
            assert_eq!(back.flow.stage("review").unwrap().fail_to, None);
        }
        // Read back from the flow file by name.
        let yaml = otaq_core::flow::yaml::write(&to_grill);
        assert!(yaml.contains("fail_to: Grill"), "{yaml}");

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
            ("dev", "grill", "Dev isn\u{2019}t a check"),
        ] {
            assert_eq!(
                edited(&file, &fail_to(stage, Some(to))).unwrap_err(),
                why,
                "{stage} → {to}"
            );
        }
        // Removing Grill sends Review's fails to the nearest again.
        let (gone, _) = edited(
            &to_grill,
            &StageEdit::Remove {
                stage: "grill".into(),
            },
        )
        .unwrap();
        assert_eq!(gone.flow.stage("review").unwrap().fail_to, None);
    }

    /// d0130: an agent check lists its check agents (tool badge, name, model), adds one of the account's agents not
    /// listed, removes one; one Rounds limit for the stage; the rule text. The agent field is gone from it.
    #[test]
    fn an_agent_check_lists_its_check_agents() {
        let e = env(not_asked());
        let file = shop_web();
        let d = draft(&file, true);
        let details = |file: &FlowFile, d: &Draft| {
            render(
                &e,
                Opened::Project,
                d,
                file,
                &shop_web(),
                Some("validate"),
                None,
            )
            .flow
            .details
            .expect("Validate's details")
        };
        let one = details(&file, &d);
        assert!(
            one.agent.is_none(),
            "an agent check lists its agents instead"
        );
        let field = one.check_agents.expect("Check agents");
        assert_eq!(field.label, "Check agents");
        assert_eq!(
            field
                .rows
                .iter()
                .map(|r| (r.name.as_str(), r.remove.as_str()))
                .collect::<Vec<_>>(),
            [("Validator", "Remove Validator")]
        );
        assert_eq!(field.add, "Add a check agent");
        assert_eq!(field.note, "They run side by side, each read-only.");
        let offered: Vec<&str> = field.add_items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            offered,
            [
                "Developer",
                "Griller",
                "Rust developer",
                "Security reviewer"
            ]
        );
        let rounds = one.rounds.expect("Rounds");
        assert_eq!(
            (rounds.label, rounds.value.as_str(), rounds.note),
            ("Rounds", "3", "for the stage, whichever fails")
        );
        assert_eq!(
            rounds
                .items
                .iter()
                .map(|i| (i.id.as_str(), i.checked))
                .collect::<Vec<_>>(),
            [
                ("1", false),
                ("2", false),
                ("3", true),
                ("4", false),
                ("5", false)
            ]
        );
        assert_eq!(
            one.rule,
            Some(
                "Passes when every check agent passes. If one fails, the card goes back once all have given their \
                 verdict, with every report together; the next round runs them all again."
            )
        );
        assert_eq!(one.fail_to.map(|f| f.value), Some("Dev".to_owned()));
        assert!(one.skill.is_some());

        // Add Security reviewer: both listed, neither offered; twice is refused.
        let (two, _) = edited(&file, &add_check_agent("validate", "Security reviewer")).unwrap();
        assert_eq!(
            two.flow.stage("validate").unwrap().agents,
            ["Validator", "Security reviewer"]
        );
        let d = checked(&two);
        let field = details(&two, &d).check_agents.unwrap();
        assert_eq!(
            field
                .rows
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            ["Validator", "Security reviewer"]
        );
        assert!(
            !field
                .add_items
                .iter()
                .any(|i| i.id == "Validator" || i.id == "Security reviewer")
        );
        assert_eq!(
            edited(&two, &add_check_agent("validate", "security reviewer")).unwrap_err(),
            "security reviewer already checks Validate"
        );
        assert_eq!(
            edited(&file, &add_check_agent("dev", "Validator")).unwrap_err(),
            "Dev isn\u{2019}t an agent check"
        );
        let yaml = otaq_core::flow::yaml::write(&two);
        assert!(
            yaml.contains("agents: [Validator, Security reviewer]"),
            "{yaml}"
        );

        // Rounds.
        let (fewer, _) = edited(
            &two,
            &StageEdit::Rounds {
                stage: "validate".into(),
                rounds: 2,
            },
        )
        .unwrap();
        assert_eq!(fewer.flow.stage("validate").unwrap().rounds, Some(2));
        for (stage, rounds) in [("validate", 6), ("review", 2)] {
            assert!(
                edited(
                    &two,
                    &StageEdit::Rounds {
                        stage: stage.into(),
                        rounds
                    }
                )
                .is_err()
            );
        }

        // Remove Validator, then Security reviewer: the check has none, which shows and keeps Apply off.
        let remove = |file: &FlowFile, agent: &str| {
            edited(
                file,
                &StageEdit::RemoveCheckAgent {
                    stage: "validate".into(),
                    agent: agent.into(),
                },
            )
        };
        let (left_one, _) = remove(&two, "Validator").unwrap();
        assert_eq!(
            left_one.flow.stage("validate").unwrap().agents,
            ["Security reviewer"]
        );
        let (none, _) = remove(&left_one, "Security reviewer").unwrap();
        assert!(remove(&none, "Security reviewer").is_err());
        let d = checked(&none);
        let v = render(
            &e,
            Opened::Project,
            &d,
            &none,
            &file,
            Some("validate"),
            None,
        );
        let details = v.flow.details.unwrap();
        let message = "Validate is an agent check with no check agent: name one or more";
        assert_eq!(details.problem.as_deref(), Some(message));
        assert!(details.check_agents.unwrap().rows.is_empty());
        assert!(details.skill.is_some(), "its skill stays");
        assert_eq!(v.flow.steps[3].problem.as_deref(), Some(message));
        assert!(!v.footer.apply_enabled);
        assert_eq!(v.footer.reason.as_deref(), Some(message));
    }

    /// d0130, d0115 (`s29-04`): the change line for a check's new agents names them and when they run.
    #[test]
    fn the_change_line_for_new_check_agents() {
        let e = env(not_asked());
        let line = |file: &FlowFile| {
            let d = checked(file);
            render(&e, Opened::Project, &d, file, &shop_web(), None, None)
                .flow
                .change_line
        };
        let replaced = shop_with(&[
            add_check_agent("validate", "Security reviewer"),
            add_check_agent("validate", "Performance reviewer"),
            StageEdit::RemoveCheckAgent {
                stage: "validate".into(),
                agent: "Validator".into(),
            },
        ]);
        assert_eq!(
            line(&replaced).as_deref(),
            Some(
                "Validate now runs Security reviewer and Performance reviewer side by side. It applies at once: \
                 the next round of a ticket at Validate runs both."
            )
        );
        let one = shop_with(&[
            add_check_agent("validate", "Security reviewer"),
            StageEdit::RemoveCheckAgent {
                stage: "validate".into(),
                agent: "Validator".into(),
            },
        ]);
        assert_eq!(
            line(&one).as_deref(),
            Some(
                "Validate now runs Security reviewer. It applies at once: the next round of a ticket at Validate \
                 runs it."
            )
        );
        // With another change, its line follows.
        let more = shop_with(&[
            add_check_agent("validate", "Security reviewer"),
            fail_to("review", Some("grill")),
        ]);
        assert_eq!(
            line(&more).as_deref(),
            Some(
                "Validate now runs Validator and Security reviewer side by side. It applies at once: the next round \
                 of a ticket at Validate runs both. This applies at once: tickets that haven\u{2019}t reached a \
                 changed step follow it; tickets in it or past it finish as they are."
            )
        );
    }

    /// d0131: a check whose fails go to a stage that no longer fits (it moved after the check) shows the problem on
    /// its Fails go to, not twice, and keeps Apply off until another stage is picked.
    #[test]
    fn a_fail_to_problem_keeps_apply_off_until_fixed() {
        let e = env(not_asked());
        let file = shop_with(&[
            fail_to("review", Some("grill")),
            StageEdit::Move {
                stage: "grill".into(),
                to: 4,
            },
        ]);
        let d = checked(&file);
        let v = render(
            &e,
            Opened::Project,
            &d,
            &file,
            &shop_web(),
            Some("review"),
            None,
        );
        let message = "Review's fails go to Grill, which isn't an Agent stage before it";
        let details = v.flow.details.as_ref().unwrap();
        let field = details.fail_to.as_ref().unwrap();
        assert_eq!(field.value, "Grill");
        assert_eq!(field.problem.as_deref(), Some(message));
        assert!(
            field.items.iter().all(|i| !i.checked),
            "Grill isn't offered"
        );
        assert_eq!(details.problem, None, "shown once, on its field");
        assert_eq!(v.flow.steps[4].problem.as_deref(), Some(message));
        assert_eq!(
            v.flow.steps[4].fails_to.as_deref(),
            Some("fails go to Grill")
        );
        assert!(!v.footer.apply_enabled);
        assert_eq!(v.footer.reason.as_deref(), Some(message));

        let (fixed, _) = edited(&file, &fail_to("review", Some("dev"))).unwrap();
        let d = checked(&fixed);
        let v = render(
            &e,
            Opened::Project,
            &d,
            &fixed,
            &shop_web(),
            Some("review"),
            None,
        );
        assert_eq!(v.flow.details.unwrap().fail_to.unwrap().problem, None);
        assert!(v.footer.apply_enabled);
    }
}

//! The flow's types (STG-01, d0108): the stages a ticket walks between the implicit Backlog and Done. Ready for Dev
//! is the implicit wait before the first Agent stage; none of the three is a stage.
//!
//! A flow file holds `agents:` and `flow:`. M31 reads and writes both, but only the stages are used: an agent is
//! still a name, which M31 maps to today's Developer and Validator rows (d0086) and M32 resolves to an agent.

use serde::Deserialize;

/// The waiting column's names ([`Flow::waiting_name`]) and the stage name that keeps the default one.
pub const READY_FOR_DEV: &str = "Ready for Dev";
pub const READY: &str = "Ready";
const DEV_NAME: &str = "Dev";

/// A flow file: the agents it defines and its stages, in the YAML's order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FlowFile {
    pub agents: Vec<FlowAgent>,
    pub flow: Flow,
}

/// An agent a flow file defines (d0108: "instructions, skill folders, model"). Read and written as given; nothing
/// uses it until M32.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FlowAgent {
    pub name: String,
    /// `noyan` (d0108's role), when it has one.
    pub role: Option<String>,
    /// Empty when it has none.
    pub instructions: String,
    pub skills: Vec<String>,
    /// `default` (the account's default model, as shipped files say) or a model as M32 names it.
    pub model: String,
    /// Why the flow has it, naming its source (FLW-04, the flow editor); nothing else reads it.
    pub why: Option<String>,
}

/// The stages of a project, in order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Flow {
    pub stages: Vec<Stage>,
}

/// One stage. Which fields are set depends on the kind ([`crate::flow::yaml`] refuses the others):
/// `agent` and `skill` for an Agent stage, `agent` (and an optional `skill`) for a Delivery that can open a PR;
/// `agents`, `skill` and `rounds` for an agent check (d0130); `fail_to` for a check (d0131); `delivery` for the
/// Delivery stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    /// Permanent: kept as written, so a rename changes nothing (d0108). Otaq writes it; a file without one gets
    /// one from the name.
    pub id: String,
    pub name: String,
    pub kind: StageKind,
    pub column: Column,
    /// The agent of an Agent stage or a Delivery that can open a PR; never an agent check's ([`Self::agents`]).
    pub agent: Option<String>,
    /// An agent check's check agents, one or more (d0130), side by side, each running the stage's skill; empty on
    /// every other kind. Read them with [`Self::check_agents`].
    pub agents: Vec<String>,
    pub skill: Option<String>,
    /// What a ticket may pick instead of the usual agent: another agent, or skipping the stage (its choices,
    /// [`crate::board::stage_choices`]).
    pub options: Vec<StageOption>,
    /// An agent check's fails in a row before the card stops for the user (1–5), whichever check agent failed.
    pub rounds: Option<u32>,
    /// A check's "fails go to" stage, by id (d0131), so it follows a rename; `None` for the nearest Agent stage
    /// before the check. Resolve it with [`Flow::fail_to_of`].
    pub fail_to: Option<String>,
    pub delivery: Option<Delivery>,
    /// Why the flow has this stage, naming its source ("From 41 /security-review runs in shop-web"): the flow editor's
    /// why line (FLW-04). The engine and the checks never read it.
    pub why: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageKind {
    /// An agent works on the ticket with a skill.
    Agent,
    /// An agent or the user passes or fails the work.
    Check { by: CheckBy },
    /// Local merge or PR; always the last stage.
    Delivery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckBy {
    Agent,
    You,
}

/// Where a stage's cards show. Unrelated to the board's old `domain::Column`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Column {
    /// The stage has its own column.
    #[default]
    Own,
    /// Its cards stay in the column before it, tagged with the stage's name ("Validating" in Dev).
    WithPrevious,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StageOption {
    /// Another agent a ticket may use for this stage.
    Agent(String),
    /// The ticket may skip the stage.
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Delivery {
    pub mode: DeliveryMode,
    pub delivers: Delivers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeliveryMode {
    /// The card waits for the user's Local merge or Open PR click.
    Click,
    /// Local merge, or push and open the PR, with no click.
    Automatic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Delivers {
    /// A Local merge; runs no agent.
    Local,
    Pr,
    /// The user's choice at the click (click mode only).
    Either,
}

impl CheckBy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::You => "you",
        }
    }
}

impl Column {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Own => "own",
            Self::WithPrevious => "with-previous",
        }
    }
}

impl DeliveryMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Click => "click",
            Self::Automatic => "automatic",
        }
    }
}

impl Delivers {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Pr => "pr",
            Self::Either => "either",
        }
    }
}

impl StageOption {
    /// The word for skipping in `options:`.
    pub const SKIP: &str = "skip";

    pub fn as_str(&self) -> &str {
        match self {
            Self::Agent(name) => name,
            Self::Skip => Self::SKIP,
        }
    }
}

impl StageKind {
    /// The YAML's `kind:`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Check { .. } => "check",
            Self::Delivery => "delivery",
        }
    }
}

impl Stage {
    pub fn is_agent_stage(&self) -> bool {
        self.kind == StageKind::Agent
    }

    pub fn is_agent_check(&self) -> bool {
        self.kind == StageKind::Check { by: CheckBy::Agent }
    }

    pub fn is_human_check(&self) -> bool {
        self.kind == StageKind::Check { by: CheckBy::You }
    }

    pub fn is_delivery(&self) -> bool {
        self.kind == StageKind::Delivery
    }

    /// An Agent stage or an agent check with no skill yet (FED-06): the draft may hold one, an applied flow never
    /// does ([`super::check`]'s [`super::Rule::NeedsSkill`]).
    pub fn needs_skill(&self) -> bool {
        (self.is_agent_stage() || self.is_agent_check()) && self.skill.is_none()
    }

    /// An agent check's check agents (d0130), in the flow's order; empty for every other kind.
    pub fn check_agents(&self) -> &[String] {
        &self.agents
    }

    /// Every agent the stage itself names (not its options): an Agent stage's or a Delivery's agent, an agent check's
    /// check agents.
    pub fn named_agents(&self) -> impl Iterator<Item = &String> {
        self.agent.iter().chain(&self.agents)
    }

    /// The agents of the stage's options (another agent a ticket may pick).
    pub fn option_agents(&self) -> impl Iterator<Item = &String> {
        self.options.iter().filter_map(|o| match o {
            StageOption::Agent(name) => Some(name),
            StageOption::Skip => None,
        })
    }

    /// Every agent the stage names: [`Self::named_agents`], then its options' ([`Self::option_agents`]).
    pub fn every_agent(&self) -> impl Iterator<Item = &String> {
        self.named_agents().chain(self.option_agents())
    }

    /// The agents the stage itself names, as one line: "Developer", "Security reviewer + Performance reviewer" (an
    /// agent check's, d0130); empty when it names none.
    pub fn agents_line(&self) -> String {
        self.named_agents()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" + ")
    }

    /// The stage names an agent of its own: an Agent stage, a Delivery that can open a PR, an agent check with a check
    /// agent.
    pub fn has_agent(&self) -> bool {
        self.named_agents().next().is_some()
    }

    /// The flow lets a ticket skip this stage.
    pub fn can_skip(&self) -> bool {
        self.options.contains(&StageOption::Skip)
    }
}

impl Flow {
    pub fn stage(&self, id: &str) -> Option<&Stage> {
        self.stages.iter().find(|s| s.id == id)
    }

    pub fn position(&self, id: &str) -> Option<usize> {
        self.stages.iter().position(|s| s.id == id)
    }

    /// Where a Fail at the check `check` goes (d0131): its `fail_to` stage when that is an Agent stage before it, else
    /// the nearest Agent stage before it. `None` when it has none before it, which [`crate::flow::check`] refuses.
    /// Skips aren't looked at: the engine resolves those per ticket.
    pub fn fail_to_of(&self, check: &Stage) -> Option<&Stage> {
        let at = self.position(&check.id)?;
        let before = &self.stages[..at];
        check
            .fail_to
            .as_deref()
            .and_then(|id| before.iter().find(|s| s.id == id && s.is_agent_stage()))
            .or_else(|| before.iter().rev().find(|s| s.is_agent_stage()))
    }

    /// The first Agent stage, which Ready for Dev waits before.
    pub fn first_agent_stage(&self) -> Option<&Stage> {
        self.stages.iter().find(|s| s.is_agent_stage())
    }

    /// The waiting column's name (d0093, d0066, Q515): "Ready for Dev" while the first Agent stage is named Dev (the
    /// default flow), else "Ready".
    pub fn waiting_name(&self) -> &'static str {
        match self.first_agent_stage() {
            Some(stage) if stage.name != DEV_NAME => READY,
            _ => READY_FOR_DEV,
        }
    }

    /// The stage whose column the stage at `index` shows in: itself when `column: own`, else the nearest earlier
    /// stage with its own column (a chain folds into one owner). `None` when nothing before it owns a column, which
    /// [`crate::flow::check`] refuses.
    pub fn column_owner(&self, index: usize) -> Option<usize> {
        (0..=index)
            .rev()
            .find(|&i| self.stages.get(i).is_some_and(|s| s.column == Column::Own))
    }

    /// The agent check whose rounds a ticket at `stage` counts (d0063, "round 1 of 3"): that stage when it is one,
    /// else the next one after it (from Backlog or Ready for Dev, `None`: the first), else the last one before it.
    pub fn round_check(&self, stage: Option<&str>) -> Option<&Stage> {
        let at = stage.and_then(|id| self.position(id));
        if let Some(here) = at
            .and_then(|i| self.stages.get(i))
            .filter(|s| s.is_agent_check())
        {
            return Some(here);
        }
        let from = at.map_or(0, |i| i + 1);
        self.stages[from.min(self.stages.len())..]
            .iter()
            .find(|s| s.is_agent_check())
            .or_else(|| {
                self.stages[..from.min(self.stages.len())]
                    .iter()
                    .rev()
                    .find(|s| s.is_agent_check())
            })
    }

    /// The rounds of [`Self::round_check`] plus the `extra` rounds the user allowed; 3 plus `extra` without one.
    pub fn check_rounds(&self, stage: Option<&str>, extra: u32) -> u32 {
        self.round_check(stage)
            .map_or(super::check::DEFAULT_VALIDATION_ROUNDS + extra, |s| {
                super::engine::rounds_of(s, extra)
            })
    }
}

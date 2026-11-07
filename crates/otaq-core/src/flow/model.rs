//! The flow's types (STG-01, d0108): the stages a ticket walks between Backlog and Done, the fixed ends of every flow,
//! and how its work leaves the flow after the last one (the flow's [`Delivery`]). Starting a ticket puts it into the
//! first stage: until it can begin it waits at the top of that stage's column. Neither end is a stage, and neither is
//! the delivery: a flow's stages are only work and checks, each in its own column.
//!
//! A flow file holds `agents:`, `flow:` and `delivery:`. An agent is a name, resolved to one of the user's agents.

use serde::Deserialize;

/// A flow file: the agents it defines, its stages in the YAML's order, and its delivery.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FlowFile {
    pub agents: Vec<FlowAgent>,
    pub flow: Flow,
}

/// An agent a flow file defines (d0108): its instructions and model. An agent has no skills and no role: an old
/// file's `skills:` and `role:` still read, and are ignored and never written (T6).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FlowAgent {
    pub name: String,
    /// Empty when it has none.
    pub instructions: String,
    /// `default` (the account's default model, as shipped files say) or a model as M32 names it.
    pub model: String,
    /// Why the flow has it, naming its source (FLW-04, the flow editor); nothing else reads it.
    pub why: Option<String>,
}

/// The stages of a project, in order, and how the work is delivered after the last one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Flow {
    pub stages: Vec<Stage>,
    /// How a ticket's work leaves the flow once it passed the last stage: a setting of the flow, not a stage.
    pub delivery: Delivery,
}

/// One stage, in its own column on the board. Which fields are set depends on the kind ([`crate::flow::yaml`]
/// refuses the others): `agent` and `skill` for an Agent stage; `agent`, `skill` and `rounds` for an agent check;
/// `fail_to` for a check (d0131).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    /// Permanent: kept as written, so a rename changes nothing (d0108). Otaq writes it; a file without one gets
    /// one from the name.
    pub id: String,
    pub name: String,
    pub kind: StageKind,
    /// The agent that runs the stage: an Agent stage's or an agent check's (a check has one agent, like an Agent
    /// stage; two checks are two stages). A draft's agent check may have none yet.
    pub agent: Option<String>,
    pub skill: Option<String>,
    /// What a ticket may do besides running the stage as the flow says: only skipping it. A stage doesn't list the
    /// agents it allows: a ticket may pick any agent whose tool has the stage's skill
    /// ([`crate::board::stage_choices`]); the agent names older files list here are read and ignored.
    pub options: Vec<StageOption>,
    /// An agent check's fails in a row before the card stops for the user (1–5).
    pub rounds: Option<u32>,
    /// A check's "fails go to" stage, by id (d0131), so it follows a rename; `None` for the nearest Agent stage
    /// before the check. Resolve it with [`Flow::fail_to_of`].
    pub fail_to: Option<String>,
    /// What the stage settles and when it matters ("Settles how it's built: needed when the change touches several
    /// parts or its approach isn't obvious"). Every agent working a ticket, and Noyan, reads each stage's in `prime`,
    /// so a stage's agent can tell when it has nothing to add (it then finishes at once, with a short note). The
    /// engine and the checks never read it; a stage may have none. Written `description:`; an older file's `why:`
    /// reads as it.
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageKind {
    /// An agent works on the ticket with a skill.
    Agent,
    /// An agent or the user passes or fails the work.
    Check { by: CheckBy },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckBy {
    Agent,
    You,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StageOption {
    /// The ticket may skip the stage (the flow marks it skippable).
    Skip,
}

/// How a flow delivers the work once a ticket passed its last stage (`delivery:` in the flow file): a local merge, a
/// PR or the user's choice of either, on the user's click or with none. A ticket being delivered stays in the flow's
/// last stage ([`Flow::last_stage`]) as To deliver, PR open or Merged into.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Delivery {
    pub mode: DeliveryMode,
    pub delivers: Delivers,
    /// The delivery's agent: when a ticket reaches Review it writes the message the delivery uses (the squash
    /// commit's, and for a PR its branch, title and description), and it opens the PR. Required when the delivery can
    /// open a PR (`pr` or `either`); a local merge may leave it out, and then its message is Otaq's plain one.
    pub agent: Option<String>,
    /// The skill it opens the PR with; Otaq's `open-pr` when none is named.
    pub skill: Option<String>,
}

impl Default for Delivery {
    /// A local merge on the user's click: the delivery that needs nothing else.
    fn default() -> Self {
        Self {
            mode: DeliveryMode::Click,
            delivers: Delivers::Local,
            agent: None,
            skill: None,
        }
    }
}

impl Delivery {
    /// It can open a PR: it delivers `pr` or `either`.
    pub fn can_open_pr(&self) -> bool {
        self.delivers != Delivers::Local
    }

    /// It can merge locally: it delivers `local` or `either`.
    pub fn can_merge_locally(&self) -> bool {
        self.delivers != Delivers::Pr
    }

    /// It delivers with no click: its mode is automatic and it delivers `local` or `pr` (`either` needs the user's
    /// choice). Only the flow decides: skipped checks and checks the user moved the ticket past don't change it.
    pub fn is_automatic(&self) -> bool {
        self.mode == DeliveryMode::Automatic && self.delivers != Delivers::Either
    }

    /// What it delivers and when, in a few words: "Local merge or PR · click", "Local merge · automatic", "PR ·
    /// automatic".
    pub fn line(&self) -> String {
        let what = match self.delivers {
            Delivers::Local => "Local merge",
            Delivers::Pr => "PR",
            Delivers::Either => "Local merge or PR",
        };
        let when = match self.mode {
            DeliveryMode::Click => "click",
            DeliveryMode::Automatic => "automatic",
        };
        format!("{what} \u{b7} {when}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeliveryMode {
    /// The card waits for the user's click: Approve when the last stage is a check by you ("Approve & merge into
    /// main", "Approve & open PR"), else Local merge or Open PR.
    Click,
    /// Local merge, or push and open the PR, with no click.
    Automatic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Delivers {
    /// A Local merge; its agent, when named, only writes the squash message.
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

    /// An Agent stage or an agent check with no skill yet (FED-06): the draft may hold one, an applied flow never
    /// does ([`super::check`]'s [`super::Rule::NeedsSkill`]).
    pub fn needs_skill(&self) -> bool {
        (self.is_agent_stage() || self.is_agent_check()) && self.skill.is_none()
    }

    /// An agent check's agent; `None` for every other kind, and for a draft's check with none yet.
    pub fn check_agent(&self) -> Option<&str> {
        self.agent.as_deref().filter(|_| self.is_agent_check())
    }

    /// The agent the stage names: an Agent stage's or an agent check's.
    pub fn named_agents(&self) -> impl Iterator<Item = &String> {
        self.agent.iter()
    }

    /// Every agent the stage names: its own agent ([`Self::named_agents`]); a stage lists no others.
    pub fn every_agent(&self) -> impl Iterator<Item = &String> {
        self.named_agents()
    }

    /// The agent the stage itself names ("Developer"); empty when it names none.
    pub fn agents_line(&self) -> String {
        self.agent.clone().unwrap_or_default()
    }

    /// The stage names an agent of its own: an Agent stage, an agent check with its agent.
    pub fn has_agent(&self) -> bool {
        self.agent.is_some()
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

    /// The first Agent stage: a started ticket waits at the top of its column until it can begin.
    pub fn first_agent_stage(&self) -> Option<&Stage> {
        self.stages.iter().find(|s| s.is_agent_stage())
    }

    /// The flow's last stage: where a ticket stays while its work is delivered (To deliver, PR open, Merged into),
    /// since the delivery comes after it and isn't a stage. `None` only for a flow with no stages.
    pub fn last_stage(&self) -> Option<&Stage> {
        self.stages.last()
    }

    /// Whether the stage `id` is the flow's last stage.
    pub fn is_last(&self, id: &str) -> bool {
        self.last_stage().is_some_and(|s| s.id == id)
    }

    /// The check by you whose Approve delivers the work: the last stage, when it is a check by you and the delivery
    /// waits for a click ("Approve & merge into main", "Approve & open PR"). `None` otherwise.
    pub fn approve_stage(&self) -> Option<&Stage> {
        self.last_stage()
            .filter(|s| s.is_human_check() && self.delivery.mode == DeliveryMode::Click)
    }

    /// The name of the stage a started ticket goes into ("Dev"): the first Agent stage's, "the first stage" for a flow
    /// without one.
    pub fn first_stage_name(&self) -> &str {
        self.first_agent_stage()
            .map_or("the first stage", |s| s.name.as_str())
    }

    /// The agent check whose rounds a ticket at `stage` counts (d0063, "round 1 of 3"): that stage when it is one,
    /// else the next one after it (from Backlog or waiting to begin, `None`: the first), else the last one before it.
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

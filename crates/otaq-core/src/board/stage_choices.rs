//! A ticket's stage choices (AGM-08, AGM-11; d0087): a ticket keeps **one choice per stage** (table
//! `ticket_stage_choices`) with who set it. A stage doesn't list the agents it allows: for any stage an agent runs (an
//! Agent stage, an agent check) a ticket may pick **any agent whose tool has the stage's skill**
//! ([`Board::skill_tools`]); a stage running one of Otaq's own skills allows every agent. A stage the flow marks
//! skippable (`options: [skip]`, the only thing `options` holds) may be skipped. No choice = the stage's own agent
//! ("Product spec: Spec writer"), "required" on a skippable human check: you check it ("Spec review: required").
//!
//! **Who may set what** ([`judge`]): only the user, for any stage until it starts, and the user's choice wins. Noyan
//! is intake only: the Project agent sets only the choices the user names, with `--by-user` (d0087), which makes them
//! the user's, and any choice of its own is refused ([`NOYAN_USER_CHOICES_ONLY`]). Only Noyan passes on the user's
//! choices: a ticket's own agent (a helper too) that sends `--by-user` is refused ([`ONLY_NOYAN_PASSES_ON`]), and any
//! choice of its own is refused too ([`ONLY_THE_USER_PICKS`], [`ONLY_THE_USER_SKIPS`]): picking an agent for a later
//! stage would move work to a tool the user didn't choose. Which tickets each agent may change at all is the
//! policy's ([`crate::policy::Action::SetChoices`], CLI-06). A choice **locks** when its stage starts (the ticket has
//! a run of it, STG-05), except while that stage waits for its agent's tool (held before its session started): a
//! waiting stage hasn't started, so the waiting card's Change agent… is this same choice ([`StageOptions::held`]).
//! A word that isn't a choice of the stage is refused naming them; an agent whose tool lacks the stage's skill is
//! refused saying so.
//!
//! **What a choice does:** the running agent of a stage is its choice, else the stage's agent
//! ([`crate::agents::resolve::running_agent`]); a skipped stage is passed with a run of result `skipped`
//! ([`Board::ticket_skips`] feeds the flow engine, [`crate::flow::engine::At::skips`]); its Delivery still does what
//! the flow says. A stored choice for a stage the flow no longer has, or a skip of a stage no longer skippable, is
//! ignored; a chosen agent that is deleted takes its choices with it.
//!
//! **Nothing is skipped on its own:** a skippable human check runs unless the ticket skips it, even when every Agent
//! stage before it is skipped (the ticket waits there; the user skips the check too if they want). A set of choices
//! that leaves a human check the flow doesn't mark skippable with no Agent stage before it that runs is refused,
//! naming them ([`strands_check_refusal`]).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError, Refusal};
use crate::agent_tools::Tool;
use crate::agents::resolve;
use crate::agents::{Agent, SessionRole};
use crate::domain::{Actor, TicketKey};
use crate::flow::engine;
use crate::flow::{Flow, Stage, StageOption};
use crate::store::now_ms;

/// The word for skipping a stage (`--stage QA=skip`).
pub const SKIP: &str = StageOption::SKIP;

/// The usual choice of a skippable human check (`--stage QA=required`): the check happens, as the flow says.
pub const REQUIRED: &str = "required";

/// The refusal of a stage choice the Project agent makes on its own (T1): Noyan is intake only and passes on only
/// the choices the user names.
pub const NOYAN_USER_CHOICES_ONLY: &str = "Noyan sets only the stage choices the user names: add --by-user, or leave the stage's own agent";

/// The refusal of a ticket agent's `skip`: only the user skips a stage.
pub const ONLY_THE_USER_SKIPS: &str =
    "Only the user skips a stage: leave it, or ask the user to skip it";

/// The refusal of a ticket agent's pick of an agent (or of a stage's own one): only the user picks a stage's agent, so the
/// work never moves to a tool the user didn't choose.
pub const ONLY_THE_USER_PICKS: &str = "Only the user picks a stage's agent: leave the stage's own agent, or ask the user to change it";

/// The refusal of a ticket agent's `--by-user`: only Noyan passes on the user's choices.
pub const ONLY_NOYAN_PASSES_ON: &str =
    "Only Noyan passes on the user's choices: ask the user to set it";

/// One choice a request sets: `{"stage": "Dev", "choice": "Quick developer"}`. `stage` is a stage's name (ignoring
/// case) or id; `choice` an agent's name (ignoring case), [`SKIP`], or [`REQUIRED`] on a human check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoicePick {
    pub stage: String,
    pub choice: String,
    /// An agent's reason ("typo fix"); only an agent sends one, and only the user's choices are taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl ChoicePick {
    /// `Dev=Quick developer` (the CLI's `--stage` and the `-f` file's `- **Stage:**` line) with `reason`.
    pub fn parse(text: &str, reason: Option<&str>) -> Result<Self, String> {
        match text.split_once('=') {
            Some((stage, choice)) if !stage.trim().is_empty() && !choice.trim().is_empty() => {
                Ok(Self {
                    stage: stage.trim().to_owned(),
                    choice: choice.trim().to_owned(),
                    reason: reason
                        .map(str::trim)
                        .filter(|r| !r.is_empty())
                        .map(str::to_owned),
                })
            }
            _ => Err(format!(
                "give a stage choice as \"<stage>=<agent or {SKIP}>\", for example \"Dev=Quick developer\"; got \
                 \"{text}\""
            )),
        }
    }
}

/// What a stage runs for a ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Choice {
    /// The stage's own agent; "required" on a human check.
    Usual,
    /// Another agent, one whose tool has the stage's skill.
    Agent { id: String, name: String },
    /// The ticket skips the stage.
    Skip,
}

/// How a stage's usual choice reads: its agent ("Rust developer", "Spec writer" on a skippable Product spec, "Validator"
/// on an agent check), or "required" for a stage that runs no agent (a human check: you check it).
fn usual_word(stage: &Stage) -> String {
    match stage.has_agent() {
        true => stage.agents_line(),
        false => REQUIRED.to_owned(),
    }
}

impl Choice {
    /// As the ticket shows it: the agent's name, "skip", or the usual choice ("Rust developer", "required").
    pub fn label(&self, stage: &Stage) -> String {
        match self {
            Self::Usual => usual_word(stage),
            Self::Agent { name, .. } => name.clone(),
            Self::Skip => SKIP.to_owned(),
        }
    }

    fn stored(&self) -> (&'static str, Option<&str>) {
        match self {
            Self::Usual => ("usual", None),
            Self::Agent { id, .. } => ("agent", Some(id)),
            Self::Skip => ("skip", None),
        }
    }
}

/// Who set a choice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum SetBy {
    /// The user, or an agent's request in the user's words (`--by-user`, d0087).
    User,
    /// The Project agent, by its name ("Checkout slowness").
    ProjectAgent { name: String, reason: String },
    /// A ticket's own agent, by the agent it runs ("Architect"): only read from choices stored before only the user
    /// picked.
    Agent { name: String, reason: String },
}

impl SetBy {
    /// "you", "Noyan", "Architect".
    pub fn who(&self) -> String {
        match self {
            Self::User => "you".to_owned(),
            Self::ProjectAgent { .. } => "Noyan".to_owned(),
            Self::Agent { name, .. } => name.clone(),
        }
    }

    /// An agent's reason.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::User => None,
            Self::ProjectAgent { reason, .. } | Self::Agent { reason, .. } => Some(reason),
        }
    }

    pub fn is_user(&self) -> bool {
        *self == Self::User
    }

    fn stored(&self) -> (&'static str, Option<&str>) {
        match self {
            Self::User => ("user", None),
            Self::ProjectAgent { name, .. } => ("project_agent", Some(name)),
            Self::Agent { name, .. } => ("agent", Some(name)),
        }
    }

    fn read(set_by: &str, name: Option<String>, reason: Option<String>) -> Self {
        let (name, reason) = (name.unwrap_or_default(), reason.unwrap_or_default());
        match set_by {
            "project_agent" => Self::ProjectAgent { name, reason },
            "agent" => Self::Agent { name, reason },
            _ => Self::User,
        }
    }
}

/// Who asks to set a choice: the user, or an agent by the name its choices carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chooser {
    User,
    ProjectAgent {
        name: String,
    },
    /// A ticket's own agent, by the agent its session runs.
    TicketAgent {
        name: String,
    },
}

/// A ticket's choice for one stage, as stored (and still valid in its flow).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageChoice {
    pub stage_id: String,
    pub choice: Choice,
    pub set_by: SetBy,
    pub set_at: i64,
    /// What it read before its last change ("Quick developer"), from History; `None` when it was first set then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

/// One agent a stage offers, by name, and the agent that name stands for (`None` for a stage's own agent that isn't
/// one of the agents: a check error of the flow), with the tool it runs on now (its own, or the default model's;
/// `None` while it runs on the default model and none is set).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionAgent {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
}

/// A stage a ticket may choose for (one an agent runs, or one the flow marks skippable), and the ticket's choice for
/// it (AGM-09's pickers; T-0411 draws them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageOptions {
    pub stage_id: String,
    /// The stage's name ("Dev").
    pub stage: String,
    /// The stage's own agent ("Spec writer"); `None` for a stage that runs none (a human check). See
    /// [`Self::usual_label`] for how the usual choice reads.
    pub usual: Option<OptionAgent>,
    /// The other agents a ticket may pick: every agent whose tool has the stage's skill, in the agents' order; none
    /// for a stage that runs no agent.
    pub agents: Vec<OptionAgent>,
    /// The flow marks the stage skippable.
    pub can_skip: bool,
    /// The ticket's choice; `None` = the usual one, nobody chose.
    pub choice: Option<StageChoice>,
    /// The stage has started: its choice can't change any more.
    pub locked: bool,
    /// The stage waits for its agent's tool (its session is held before it started): its choice can change, and the
    /// waiting card's Change agent… changes it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub held: bool,
}

impl StageOptions {
    /// The ticket skips this stage.
    pub fn skipped(&self) -> bool {
        self.choice
            .as_ref()
            .is_some_and(|c| c.choice == Choice::Skip)
    }

    /// The usual choice as it reads: its agent ("Rust developer", "Spec writer"), or [`REQUIRED`] for a stage that runs no
    /// agent (a human check).
    pub fn usual_label(&self) -> String {
        match &self.usual {
            Some(agent) => agent.name.clone(),
            None => REQUIRED.to_owned(),
        }
    }

    /// The ticket's choice as it reads ("Quick developer", "skip"), the usual one when nobody chose.
    pub fn label(&self) -> String {
        match self.choice.as_ref().map(|c| &c.choice) {
            None | Some(Choice::Usual) => self.usual_label(),
            Some(Choice::Agent { name, .. }) => name.clone(),
            Some(Choice::Skip) => SKIP.to_owned(),
        }
    }

    /// Everything a ticket may choose here, in the words a choice uses: the usual one, the other agents, `skip`.
    pub fn words(&self) -> Vec<String> {
        let mut words = vec![self.usual_label()];
        words.extend(self.agents.iter().map(|a| a.name.clone()));
        if self.can_skip {
            words.push(SKIP.to_owned());
        }
        words
    }
}

/// The answer to `ticket.choices`: a ticket's (or, with no key, a new ticket's in a project) stages it may choose
/// for, in flow order: every stage an agent runs, and every stage the flow marks skippable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketChoices {
    pub key: Option<TicketKey>,
    pub stages: Vec<StageOptions>,
}

/// The mark of a stage a ticket passed by its choice: "Skipped Product spec · this ticket's choice" (AGM-08; the
/// card's wording, drawn by T-0411).
pub fn skipped_mark(stage: &str) -> String {
    format!("Skipped {stage} · this ticket's choice")
}

/// The refusal of a set of choices that leaves `check`, a human check the flow doesn't mark skippable, with no Agent
/// stage before it that runs: "Code review needs one of Product spec, Tech spec or Milestone plan to run; it can't be skipped on this flow".
pub fn strands_check_refusal(check: &str, before: &[&str]) -> String {
    let needs = match before {
        [one] => one.to_string(),
        [rest @ .., last] => format!("one of {} or {last}", rest.join(", ")),
        [] => "an Agent stage".to_owned(),
    };
    format!("{check} needs {needs} to run; it can't be skipped on this flow")
}

/// The ticket a choice is for, as [`judge`] needs it: one that exists, or one a batch is about to create.
#[derive(Debug, Clone, Copy)]
pub struct Target<'a> {
    /// "SHOP-5", or "the new ticket \"Fix README typo\"".
    pub label: &'a str,
    /// The stages that have started for it (it has a run of each).
    pub ran: &'a [String],
    /// The stage that waits for its agent's tool, held before its session started: not locked.
    pub held: Option<&'a str>,
}

impl Target<'_> {
    /// `stage` has started for the ticket and doesn't wait for its tool: its choice is locked.
    pub fn locks(&self, stage: &str) -> bool {
        self.ran.iter().any(|s| s == stage) && self.held != Some(stage)
    }
}

/// The stage `name` names in `flow`: its id, or its name ignoring case. An unknown one lists the stages a ticket may
/// choose for.
pub fn find_stage<'f>(flow: &'f Flow, project: &str, name: &str) -> Result<&'f Stage, BoardError> {
    let name = name.trim();
    let folded = name.to_lowercase();
    flow.stages
        .iter()
        .find(|s| s.id == name || s.name.to_lowercase() == folded)
        .ok_or_else(|| {
            let with: Vec<&str> = flow
                .stages
                .iter()
                .filter(|s| has_choices(s))
                .map(|s| s.name.as_str())
                .collect();
            let listed = if with.is_empty() {
                "none of its stages has choices".to_owned()
            } else {
                format!("stages with choices: {}", with.join(", "))
            };
            BoardError::NotFound(format!("{project}'s flow has no stage {name} ({listed})"))
        })
}

/// Whether a ticket may choose for `stage`: an agent runs it (an Agent stage, an agent check), or the flow marks it
/// skippable. A Delivery's agent and a human check that can't be skipped run as the flow says.
pub fn has_choices(stage: &Stage) -> bool {
    stage.is_agent_stage() || stage.is_agent_check() || stage.can_skip()
}

/// Whether an agent may be picked for `stage`: an agent runs it.
fn runs_agent(stage: &Stage) -> bool {
    stage.is_agent_stage() || stage.is_agent_check()
}

/// The refusal of a skip of a stage the flow doesn't mark skippable (#62: a flow adds `skip` to a stage's options).
pub fn not_skippable(stage: &str) -> String {
    format!("{stage} can't be skipped; add `{SKIP}` to its options in the flow to allow it.")
}

/// The refusal of an agent whose tool lacks the stage's skill: "Strong runs on Codex, which doesn't have the skill
/// `/team-validate` for Validate".
pub fn lacks_skill(agent: &str, tool: Tool, skill: &str, stage: &str) -> String {
    format!(
        "{agent} runs on {}, which doesn't have the skill `/{skill}` for {stage}",
        tool.display_name()
    )
}

/// The choice `word` means for `stage`, among what it offers (`options`): refused when the stage has no choices,
/// when `word` asks a skip of a stage that isn't skippable, or when it isn't one of its choices ("Quick dev isn't
/// one of Dev's choices: Rust developer, Quick developer"). An agent whose tool lacks the skill isn't one of them
/// ([`Board::check_choices`] says why).
pub fn choice_of(stage: &Stage, options: &StageOptions, word: &str) -> Result<Choice, BoardError> {
    let word = word.trim();
    let folded = word.to_lowercase();
    if folded == SKIP && !options.can_skip {
        return Err(BoardError::refused(
            Refusal::NotAnOption,
            not_skippable(&stage.name),
        ));
    }
    if !has_choices(stage) {
        return Err(BoardError::refused(
            Refusal::NotAnOption,
            format!(
                "{} has no choices: every ticket runs it as the flow says",
                stage.name
            ),
        ));
    }
    let usual = match &options.usual {
        Some(agent) => agent.name.to_lowercase() == folded,
        None => folded == REQUIRED,
    };
    if usual {
        return Ok(Choice::Usual);
    }
    if folded == SKIP {
        return Ok(Choice::Skip);
    }
    if let Some(agent) = options
        .agents
        .iter()
        .find(|a| a.name.to_lowercase() == folded)
        && let Some(id) = agent.agent_id.clone()
    {
        return Ok(Choice::Agent {
            id,
            name: agent.name.clone(),
        });
    }
    if !runs_agent(stage) {
        return Err(BoardError::refused(
            Refusal::NotAnOption,
            format!(
                "{} runs no agent: its choices are {REQUIRED} (you check it) and {SKIP}",
                stage.name
            ),
        ));
    }
    Err(BoardError::refused(
        Refusal::NotAnOption,
        format!(
            "{word} isn't one of {}'s choices: {}",
            stage.name,
            options.words().join(", ")
        ),
    ))
}

/// Judges one choice (AGM-08; d0087) of `choice` for `stage` of `target`. `by_user` makes the Project agent's
/// choice the user's.
///
/// - a stage that started is locked, for everyone, unless it waits for its tool ([`Target::locks`]);
/// - the user may set any other;
/// - the Project agent sets only the user's choices (`by_user`): its own are refused;
/// - only Noyan passes on the user's choices: a ticket agent's `by_user` is refused;
/// - a ticket agent's own choice is refused: only the user skips a stage, and only the user picks its agent.
pub fn judge(
    target: Target<'_>,
    stage: &Stage,
    choice: &Choice,
    chooser: &Chooser,
    by_user: bool,
) -> Result<(), BoardError> {
    if target.locks(&stage.id) {
        return Err(BoardError::refused(
            Refusal::ChoiceLocked,
            format!(
                "{} has started on {}: its choice is locked",
                stage.name, target.label
            ),
        ));
    }
    let refused = |text| Err(BoardError::refused(Refusal::NotAllowed, text));
    match chooser {
        Chooser::User => Ok(()),
        Chooser::ProjectAgent { .. } if by_user => Ok(()),
        Chooser::ProjectAgent { .. } => refused(NOYAN_USER_CHOICES_ONLY),
        Chooser::TicketAgent { .. } if by_user => refused(ONLY_NOYAN_PASSES_ON),
        Chooser::TicketAgent { .. } if *choice == Choice::Skip => refused(ONLY_THE_USER_SKIPS),
        Chooser::TicketAgent { .. } => refused(ONLY_THE_USER_PICKS),
    }
}

/// A checked choice, ready to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceWrite {
    pub stage_id: String,
    pub stage: String,
    pub choice: Choice,
    /// How the choice reads ("Quick developer", "skip", "required").
    pub label: String,
    pub set_by: SetBy,
}

type ChoiceRow = (
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    i64,
);

/// The agents and the default model [`Board::options_of`] reads once for a whole flow.
struct Roster {
    agents: Vec<Agent>,
    default: Option<crate::agents::ModelRef>,
}

impl Roster {
    fn option(&self, name: &str, agent: Option<&Agent>) -> OptionAgent {
        OptionAgent {
            name: agent.map_or_else(|| name.to_owned(), |a| a.name.clone()),
            agent_id: agent.map(|a| a.id.clone()),
            tool: agent.and_then(|a| self.tool_of(a)),
        }
    }

    /// The tool `agent` runs on now: its own, or the default model's.
    fn tool_of(&self, agent: &Agent) -> Option<Tool> {
        agent.model.on(self.default.as_ref()).map(|m| m.tool)
    }

    fn named(&self, name: &str) -> Option<&Agent> {
        let name = name.trim().to_lowercase();
        self.agents
            .iter()
            .find(|a| a.name.trim().to_lowercase() == name)
    }
}

impl Board {
    async fn roster(&self) -> Result<Roster, BoardError> {
        Ok(Roster {
            agents: self.store().agents().await?,
            default: self.store().default_model().await?,
        })
    }

    /// The tools that have `stage`'s skill for `ticket` (a key) of `project`, or the project's new tickets: every tool
    /// for a stage running one of Otaq's own skills.
    async fn stage_tools(&self, project: &str, ticket: Option<&str>, stage: &Stage) -> Vec<Tool> {
        match stage.skill.as_deref() {
            Some(skill) => self.skill_tools(project, ticket, skill).await,
            None => Tool::ALL.to_vec(),
        }
    }

    /// What `stage` offers to `ticket` (a key) of `project`: its own agent, and every other agent whose tool has the
    /// stage's skill (an agent whose tool isn't known yet is offered too). An agent has no role: the agent Noyan uses
    /// is offered like any other.
    async fn options_of(
        &self,
        project: &str,
        ticket: Option<&str>,
        stage: &Stage,
        roster: &Roster,
    ) -> StageOptions {
        let usual = stage
            .agent
            .as_deref()
            .filter(|_| runs_agent(stage))
            .map(|name| roster.option(name, roster.named(name)));
        let mut agents = Vec::new();
        if let Some(usual) = &usual {
            let tools = self.stage_tools(project, ticket, stage).await;
            agents = roster
                .agents
                .iter()
                .filter(|a| usual.agent_id.as_deref() != Some(a.id.as_str()))
                .filter(|a| roster.tool_of(a).is_none_or(|t| tools.contains(&t)))
                .map(|a| roster.option(&a.name, Some(a)))
                .collect();
        }
        StageOptions {
            stage_id: stage.id.clone(),
            stage: stage.name.clone(),
            usual,
            agents,
            can_skip: stage.can_skip(),
            choice: None,
            locked: false,
            held: false,
        }
    }

    /// The stages of `flow` a ticket may choose for, each with what it offers to `ticket` (a key) of `project`.
    async fn flow_options(
        &self,
        flow: &Flow,
        project: &str,
        ticket: Option<&str>,
    ) -> Result<Vec<StageOptions>, BoardError> {
        let roster = self.roster().await?;
        let mut stages = Vec::new();
        for stage in flow.stages.iter().filter(|s| has_choices(s)) {
            stages.push(self.options_of(project, ticket, stage, &roster).await);
        }
        Ok(stages)
    }

    /// The stages the ticket has run for real: a run that wasn't skipped (a drag back into a skipped stage runs it
    /// once).
    async fn stages_run(&self, ticket_id: &str) -> Result<Vec<String>, BoardError> {
        Ok(sqlx::query_scalar(
            "SELECT DISTINCT stage_id FROM stage_runs WHERE ticket_id = ? AND (result IS NULL OR result <> 'skipped')",
        )
        .bind(ticket_id)
        .fetch_all(self.store().pool())
        .await?)
    }

    /// The stages the ticket has started (it has a run of each, STG-05).
    async fn stages_ran(&self, ticket_id: &str) -> Result<Vec<String>, BoardError> {
        Ok(
            sqlx::query_scalar("SELECT DISTINCT stage_id FROM stage_runs WHERE ticket_id = ?")
                .bind(ticket_id)
                .fetch_all(self.store().pool())
                .await?,
        )
    }

    /// The stage of `flow` that waits for its agent's tool on `ticket`: the stage of a held session that hasn't
    /// started (a held Developer's Agent stage, a held validator's check). A waiting stage hasn't started, so its
    /// choice can still change.
    pub async fn held_stage(
        &self,
        ticket: &Ticket,
        flow: &Flow,
    ) -> Result<Option<String>, BoardError> {
        let holds = self.store().ticket_holds(&ticket.id).await?;
        if holds.is_empty() {
            return Ok(None);
        }
        let skips = self.ticket_skips(ticket, flow).await?;
        for hold in holds {
            if !crate::agents::gate::still_waits(ticket, hold.role) {
                continue;
            }
            let stage = match hold.role {
                SessionRole::Developer => resolve::agent_stage_of(flow, ticket, &skips),
                SessionRole::Validator => resolve::check_stage_of(flow, ticket, &skips),
                SessionRole::ProjectAgent => None,
            };
            if let Some(stage) = stage {
                return Ok(Some(stage.id.clone()));
            }
        }
        Ok(None)
    }

    /// The ticket's stored choices that still hold in `options` (the stage is still there, still runs an agent for an
    /// agent's choice, still skippable for a skip), by stage id.
    async fn stored_choices(
        &self,
        ticket_id: &str,
        options: &[StageOptions],
    ) -> Result<Vec<StageChoice>, BoardError> {
        let rows: Vec<ChoiceRow> = sqlx::query_as(
            "SELECT c.stage_id, c.choice, c.agent_id, c.set_by, c.by_name, c.reason, c.set_at
             FROM ticket_stage_choices c WHERE c.ticket_id = ?",
        )
        .bind(ticket_id)
        .fetch_all(self.store().pool())
        .await?;
        // Each stage's last change in History says what it read before (AGM-09's "Quick developer → Rust developer").
        let events: Vec<(String,)> = sqlx::query_as(
            "SELECT payload_json FROM events WHERE ticket_id = ? AND kind = ? ORDER BY at, rowid",
        )
        .bind(ticket_id)
        .bind(EventKind::StageChoiceSet.as_str())
        .fetch_all(self.store().pool())
        .await?;
        let mut last_from: std::collections::HashMap<String, Option<String>> =
            std::collections::HashMap::new();
        for (payload,) in events {
            let Ok(payload) = serde_json::from_str::<Value>(&payload) else {
                continue;
            };
            if payload.get("child").is_some() {
                continue;
            }
            if let Some(stage_id) = payload.get("stage_id").and_then(Value::as_str) {
                let from = payload
                    .get("from")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                last_from.insert(stage_id.to_owned(), from);
            }
        }
        let mut choices = Vec::new();
        for (stage_id, choice, agent_id, set_by, by_name, reason, set_at) in rows {
            let Some(offered) = options.iter().find(|o| o.stage_id == stage_id) else {
                continue;
            };
            let choice = match (choice.as_str(), agent_id) {
                ("skip", _) if offered.can_skip => Choice::Skip,
                ("usual", _) => Choice::Usual,
                // Kept while the agent is there (a deleted agent takes its choices with it), even when its tool no
                // longer has the stage's skill.
                ("agent", Some(id)) if offered.usual.is_some() => {
                    let name = match offered
                        .agents
                        .iter()
                        .find(|a| a.agent_id.as_deref() == Some(id.as_str()))
                    {
                        Some(agent) => agent.name.clone(),
                        None => match self.store().agent(&id).await? {
                            Some(agent) => agent.name,
                            None => continue,
                        },
                    };
                    Choice::Agent { id, name }
                }
                _ => continue,
            };
            choices.push(StageChoice {
                from: last_from.get(&stage_id).cloned().flatten(),
                stage_id,
                choice,
                set_by: SetBy::read(&set_by, by_name, reason),
                set_at,
            });
        }
        Ok(choices)
    }

    /// `ticket.choices` for an existing ticket: the stages it may choose for, its choices and locks.
    pub async fn ticket_choices(&self, key: &str) -> Result<TicketChoices, BoardError> {
        let ticket = self.ticket(key).await?;
        let project = ticket.key.project.to_string();
        let flow = self.effective_flow(&project).await?;
        let key = ticket.key.to_string();
        let mut stages = self.flow_options(&flow, &project, Some(&key)).await?;
        let stored = self.stored_choices(&ticket.id, &stages).await?;
        let ran = self.stages_ran(&ticket.id).await?;
        let held = self.held_stage(&ticket, &flow).await?;
        let target = Target {
            label: &key,
            ran: &ran,
            held: held.as_deref(),
        };
        for stage in &mut stages {
            stage.choice = stored
                .iter()
                .find(|c| c.stage_id == stage.stage_id)
                .cloned();
            stage.locked = target.locks(&stage.stage_id);
            stage.held = held.as_deref() == Some(stage.stage_id.as_str());
        }
        Ok(TicketChoices {
            key: Some(ticket.key),
            stages,
        })
    }

    /// `ticket.choices` for a ticket not created yet in `project` (the ⌘N form): the stages of its flow it may choose
    /// for, with the agents whose tools have each stage's skill for the project.
    pub async fn new_ticket_choices(&self, project: &str) -> Result<TicketChoices, BoardError> {
        let flow = self.effective_flow(project).await?;
        Ok(TicketChoices {
            key: None,
            stages: self.flow_options(&flow, project, None).await?,
        })
    }

    /// The stages `ticket` chose to skip (the flow engine's [`crate::flow::engine::At::skips`]), in flow order: only
    /// stages the flow marks skippable; none on a flow without `skip` options. Nothing is skipped on its own.
    pub async fn ticket_skips(
        &self,
        ticket: &Ticket,
        flow: &Flow,
    ) -> Result<Vec<String>, BoardError> {
        if !flow.stages.iter().any(Stage::can_skip) {
            return Ok(Vec::new());
        }
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT stage_id FROM ticket_stage_choices WHERE ticket_id = ? AND choice = 'skip'",
        )
        .bind(&ticket.id)
        .fetch_all(self.store().pool())
        .await?;
        Ok(flow
            .stages
            .iter()
            .filter(|s| s.can_skip() && rows.contains(&s.id))
            .map(|s| s.id.clone())
            .collect())
    }

    /// The agent `ticket` chose for `stage` instead of its own (AGM-05, AGM-08), if it chose one and the stage runs
    /// an agent.
    pub async fn chosen_agent(
        &self,
        ticket: &Ticket,
        stage: &Stage,
    ) -> Result<Option<Agent>, BoardError> {
        if !runs_agent(stage) {
            return Ok(None);
        }
        let id: Option<Option<String>> = sqlx::query_scalar(
            "SELECT agent_id FROM ticket_stage_choices WHERE ticket_id = ? AND stage_id = ? AND choice = 'agent'",
        )
        .bind(&ticket.id)
        .bind(&stage.id)
        .fetch_optional(self.store().pool())
        .await?;
        match id.flatten() {
            Some(id) => Ok(self.store().agent(&id).await?),
            None => Ok(None),
        }
    }

    /// Checks every pick for `ticket` (`None`: a ticket a batch is about to create in `project`, labelled `new`) and
    /// returns what to write; nothing is written, so a refused pick leaves the ticket (or the whole batch) as it was.
    pub async fn check_choices(
        &self,
        project: &str,
        ticket: Option<&Ticket>,
        new: &str,
        picks: &[ChoicePick],
        chooser: &Chooser,
        by_user: bool,
    ) -> Result<Vec<ChoiceWrite>, BoardError> {
        if picks.is_empty() {
            return Ok(Vec::new());
        }
        let flow = self.effective_flow(project).await?;
        let key = ticket.map(|t| t.key.to_string());
        let options = self.flow_options(&flow, project, key.as_deref()).await?;
        let (stored, ran, run, held, label) = match ticket {
            Some(t) => (
                self.stored_choices(&t.id, &options).await?,
                self.stages_ran(&t.id).await?,
                self.stages_run(&t.id).await?,
                self.held_stage(t, &flow).await?,
                t.key.to_string(),
            ),
            None => (Vec::new(), Vec::new(), Vec::new(), None, new.to_owned()),
        };
        let target = Target {
            label: &label,
            ran: &ran,
            held: held.as_deref(),
        };
        let mut writes: Vec<ChoiceWrite> = Vec::with_capacity(picks.len());
        for pick in picks {
            let stage = find_stage(&flow, project, &pick.stage)?;
            let offered = options
                .iter()
                .find(|o| o.stage_id == stage.id)
                .cloned()
                .unwrap_or_else(|| StageOptions {
                    stage_id: stage.id.clone(),
                    stage: stage.name.clone(),
                    usual: None,
                    agents: Vec::new(),
                    can_skip: false,
                    choice: None,
                    locked: false,
                    held: false,
                });
            let choice = match choice_of(stage, &offered, &pick.choice) {
                Ok(choice) => choice,
                Err(e) => return Err(self.why_not(stage, &offered, &pick.choice, e).await?),
            };
            judge(target, stage, &choice, chooser, by_user)?;
            let write = ChoiceWrite {
                stage_id: stage.id.clone(),
                stage: stage.name.clone(),
                label: choice.label(stage),
                choice,
                set_by: SetBy::User,
            };
            // A later pick for the same stage replaces an earlier one.
            writes.retain(|w| w.stage_id != write.stage_id);
            writes.push(write);
        }
        // Refused when the choices leave a check that can't be skipped with nothing before it (one that already did,
        // after a change of the flow, doesn't refuse an unrelated choice).
        let before: Vec<(String, Choice)> = stored
            .iter()
            .map(|c| (c.stage_id.clone(), c.choice.clone()))
            .collect();
        let mut after = before.clone();
        for write in &writes {
            after.retain(|(id, _)| *id != write.stage_id);
            after.push((write.stage_id.clone(), write.choice.clone()));
        }
        let already: Vec<&str> = stranded_checks(&flow, &before, &run)
            .iter()
            .map(|(check, _)| check.id.as_str())
            .collect();
        if let Some((check, needs)) = stranded_checks(&flow, &after, &run)
            .into_iter()
            .find(|(check, _)| !already.contains(&check.id.as_str()))
        {
            let names: Vec<&str> = needs.iter().map(|s| s.name.as_str()).collect();
            return Err(BoardError::refused(
                Refusal::StrandsCheck,
                strands_check_refusal(&check.name, &names),
            ));
        }
        Ok(writes)
    }

    /// Why `word` isn't a choice of `stage`: an agent whose tool lacks the stage's skill says so ([`lacks_skill`]);
    /// anything else is `refused` as [`choice_of`] worded it.
    async fn why_not(
        &self,
        stage: &Stage,
        offered: &StageOptions,
        word: &str,
        refused: BoardError,
    ) -> Result<BoardError, BoardError> {
        let (Some(skill), true) = (stage.skill.as_deref(), offered.usual.is_some()) else {
            return Ok(refused);
        };
        let roster = self.roster().await?;
        let lacking = roster
            .named(word)
            .and_then(|agent| roster.tool_of(agent).map(|tool| (agent, tool)));
        Ok(match lacking {
            Some((agent, tool)) => BoardError::refused(
                Refusal::NotAnOption,
                lacks_skill(&agent.name, tool, skill, &stage.name),
            ),
            None => refused,
        })
    }

    /// Writes checked choices on `ticket`, skipping those that already read so. Returns whether one changed.
    pub async fn write_choices(
        &self,
        ticket: &Ticket,
        writes: &[ChoiceWrite],
        actor: &Actor,
    ) -> Result<bool, BoardError> {
        if writes.is_empty() {
            return Ok(false);
        }
        let project = ticket.key.project.to_string();
        let flow = self.effective_flow(&project).await?;
        let options = self
            .flow_options(&flow, &project, Some(&ticket.key.to_string()))
            .await?;
        let stored = self.stored_choices(&ticket.id, &options).await?;
        let mut changed = false;
        let mut tx = self.store().begin_write().await?;
        for write in writes {
            let before = stored.iter().find(|c| c.stage_id == write.stage_id);
            if before.is_some_and(|c| c.choice == write.choice && c.set_by == write.set_by) {
                continue;
            }
            let (choice, agent_id) = write.choice.stored();
            let (set_by, by_name) = write.set_by.stored();
            sqlx::query(
                "INSERT INTO ticket_stage_choices (ticket_id, stage_id, choice, agent_id, set_by, by_name, reason,
                                                   set_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT (ticket_id, stage_id) DO UPDATE SET
                     choice = excluded.choice, agent_id = excluded.agent_id, set_by = excluded.set_by,
                     by_name = excluded.by_name, reason = excluded.reason, set_at = excluded.set_at",
            )
            .bind(&ticket.id)
            .bind(&write.stage_id)
            .bind(choice)
            .bind(agent_id)
            .bind(set_by)
            .bind(by_name)
            .bind(write.set_by.reason())
            .bind(now_ms())
            .execute(&mut *tx)
            .await?;
            let stage = flow
                .stage(&write.stage_id)
                .expect("checked against the flow");
            let from = before.map(|c| c.choice.label(stage));
            record_event(
                &mut tx,
                &ticket.id,
                EventKind::StageChoiceSet,
                actor,
                choice_event(write, from.as_deref()),
            )
            .await?;
            changed = true;
        }
        tx.commit().await?;
        Ok(changed)
    }
}

/// The human checks the flow doesn't mark skippable that have no Agent stage before them that runs on a ticket with
/// `choices` ([`engine::nothing_runs_before`]), each with those stages. `run`: the stages the ticket has run. A
/// skippable check isn't one: the ticket waits there with nothing before it, or the user skips it too.
fn stranded_checks<'f>(
    flow: &'f Flow,
    choices: &[(String, Choice)],
    run: &[String],
) -> Vec<(&'f Stage, Vec<&'f Stage>)> {
    let skips: Vec<String> = choices
        .iter()
        .filter(|(id, c)| *c == Choice::Skip && flow.stage(id).is_some_and(Stage::can_skip))
        .map(|(id, _)| id.clone())
        .collect();
    flow.stages
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_human_check() && !s.can_skip())
        .filter_map(|(i, s)| engine::nothing_runs_before(flow, i, &skips, run).map(|b| (s, b)))
        .collect()
}

/// The History payload of one choice ([`EventKind::StageChoiceSet`]).
fn choice_event(write: &ChoiceWrite, from: Option<&str>) -> Value {
    json!({
        "stage": write.stage,
        "stage_id": write.stage_id,
        "choice": write.label,
        "from": from,
        "set_by": write.set_by.stored().0,
        "by": write.set_by.who(),
        "reason": write.set_by.reason(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{CheckBy, StageKind};

    fn stage(id: &str, agent: Option<&str>, skippable: bool) -> Stage {
        Stage {
            id: id.into(),
            name: format!("{}{}", id[..1].to_uppercase(), &id[1..]),
            kind: match agent {
                Some(_) => StageKind::Agent,
                None => StageKind::Check { by: CheckBy::You },
            },
            column: Default::default(),
            agent: agent.map(Into::into),
            skill: None,
            options: skippable.then_some(StageOption::Skip).into_iter().collect(),
            rounds: None,
            fail_to: None,
            delivery: None,
            why: None,
        }
    }

    /// What `stage` offers with `others` (the agents whose tools have its skill) besides its own agent.
    fn offered(stage: &Stage, others: &[&str]) -> StageOptions {
        let agent = |name: &str| OptionAgent {
            name: name.into(),
            agent_id: Some(format!("id-{}", name.to_lowercase())),
            tool: None,
        };
        StageOptions {
            stage_id: stage.id.clone(),
            stage: stage.name.clone(),
            usual: stage.agent.as_deref().map(agent),
            agents: match stage.agent {
                Some(_) => others.iter().map(|n| agent(n)).collect(),
                None => Vec::new(),
            },
            can_skip: stage.can_skip(),
            choice: None,
            locked: false,
            held: false,
        }
    }

    fn refusal(result: Result<impl std::fmt::Debug, BoardError>) -> (Refusal, String) {
        let e = result.unwrap_err();
        (e.refusal().unwrap(), e.to_string())
    }

    #[test]
    fn a_pick_parses_as_stage_equals_choice() {
        assert_eq!(
            ChoicePick::parse(" Dev = Quick developer ", Some(" typo fix ")).unwrap(),
            ChoicePick {
                stage: "Dev".into(),
                choice: "Quick developer".into(),
                reason: Some("typo fix".into()),
            }
        );
        assert!(ChoicePick::parse("Dev", None).is_err());
        assert!(ChoicePick::parse("=skip", None).is_err());
    }

    /// A choice is the stage's own agent, another agent it offers (one whose tool has its skill), or `skip` on a
    /// stage the flow marks skippable; a human check that can't be skipped has none.
    #[test]
    fn a_choice_is_one_the_stage_offers() {
        let dev = stage("dev", Some("Rust developer"), false);
        let spec = stage("spec", Some("Spec writer"), true);
        let qa = stage("qa", None, true);
        let review = stage("review", None, false);
        let of = |s: &Stage, word: &str| choice_of(s, &offered(s, &["Quick developer"]), word);
        assert_eq!(
            of(&dev, "quick developer").unwrap(),
            Choice::Agent {
                id: "id-quick developer".into(),
                name: "Quick developer".into()
            }
        );
        assert_eq!(of(&dev, "Rust developer").unwrap(), Choice::Usual);
        assert_eq!(of(&spec, "skip").unwrap(), Choice::Skip);
        assert_eq!(
            of(&spec, "Quick developer").unwrap().label(&spec),
            "Quick developer"
        );
        assert_eq!(of(&qa, "required").unwrap(), Choice::Usual);
        assert_eq!(of(&qa, "Required").unwrap(), Choice::Usual);
        // "required" is a human check's word only; an Agent stage's usual choice is its agent.
        assert!(of(&spec, "required").is_err());
        assert_eq!(
            refusal(of(&dev, "skip")),
            (
                Refusal::NotAnOption,
                "Dev can't be skipped; add `skip` to its options in the flow to allow it.".into()
            )
        );
        assert_eq!(
            refusal(of(&dev, "Quick dev")),
            (
                Refusal::NotAnOption,
                "Quick dev isn't one of Dev's choices: Rust developer, Quick developer".into()
            )
        );
        assert_eq!(
            refusal(of(&qa, "Spec writer")),
            (
                Refusal::NotAnOption,
                "Qa runs no agent: its choices are required (you check it) and skip".into()
            )
        );
        assert_eq!(
            refusal(of(&review, "Quick developer")),
            (
                Refusal::NotAnOption,
                "Review has no choices: every ticket runs it as the flow says".into()
            )
        );
        assert_eq!(Choice::Usual.label(&qa), "required");
        // FLW-22 (Q290): a skippable Agent stage reads its agent.
        assert_eq!(Choice::Usual.label(&spec), "Spec writer");
        assert_eq!(of(&spec, "Spec writer").unwrap(), Choice::Usual);
        assert_eq!(Choice::Usual.label(&dev), "Rust developer");
        assert_eq!(
            lacks_skill("Strong", Tool::Codex, "team-validate", "Validate"),
            "Strong runs on Codex, which doesn't have the skill `/team-validate` for Validate"
        );
    }

    /// Who may set what (AGM-08): the user until the stage starts (a stage waiting for its tool hasn't); Noyan only
    /// the user's choices; a ticket's own agent nothing (only the user picks an agent or skips), and only Noyan
    /// passes on the user's choices (`--by-user`).
    #[test]
    fn the_choice_rule() {
        let dev = stage("dev", Some("Rust developer"), true);
        let none: [String; 0] = [];
        let dev_ran = ["spec".to_owned(), "dev".to_owned()];
        let fresh = Target {
            label: "SHOP-5",
            ran: &none,
            held: None,
        };
        let in_dev = Target {
            ran: &dev_ran,
            ..fresh
        };
        let held_in_dev = Target {
            held: Some("dev"),
            ..in_dev
        };
        let noyan = Chooser::ProjectAgent {
            name: "Noyan".into(),
        };
        let spec_writer = Chooser::TicketAgent {
            name: "Spec writer".into(),
        };
        let quick = Choice::Agent {
            id: "quick".into(),
            name: "Quick developer".into(),
        };
        let judged = |choice: &Choice, target, chooser: &Chooser, by_user| {
            judge(target, &dev, choice, chooser, by_user)
                .map_err(|e| (e.refusal().unwrap(), e.to_string()))
        };
        let refused = |code, text: &str| Err((code, text.to_owned()));
        let locked = "Dev has started on SHOP-5: its choice is locked";
        let rows = [
            (
                "user, fresh",
                judged(&quick, fresh, &Chooser::User, false),
                Ok(()),
            ),
            (
                "user, Dev started",
                judged(&quick, in_dev, &Chooser::User, false),
                refused(Refusal::ChoiceLocked, locked),
            ),
            (
                "user, Dev waits for its tool",
                judged(&quick, held_in_dev, &Chooser::User, false),
                Ok(()),
            ),
            (
                "Noyan's own choice",
                judged(&quick, fresh, &noyan, false),
                refused(Refusal::NotAllowed, NOYAN_USER_CHOICES_ONLY),
            ),
            (
                "Noyan's own skip",
                judged(&Choice::Skip, fresh, &noyan, false),
                refused(Refusal::NotAllowed, NOYAN_USER_CHOICES_ONLY),
            ),
            (
                "Noyan passes on the user's choice",
                judged(&quick, fresh, &noyan, true),
                Ok(()),
            ),
            (
                "Noyan passes on the user's skip",
                judged(&Choice::Skip, fresh, &noyan, true),
                Ok(()),
            ),
            (
                "ticket agent picks an agent for a later stage",
                judged(&quick, fresh, &spec_writer, false),
                refused(Refusal::NotAllowed, ONLY_THE_USER_PICKS),
            ),
            (
                "ticket agent sets a stage back to its own agent",
                judged(&Choice::Usual, fresh, &spec_writer, false),
                refused(Refusal::NotAllowed, ONLY_THE_USER_PICKS),
            ),
            (
                "ticket agent skips a later stage",
                judged(&Choice::Skip, fresh, &spec_writer, false),
                refused(Refusal::NotAllowed, ONLY_THE_USER_SKIPS),
            ),
            (
                "ticket agent picks in the user's words",
                judged(&quick, fresh, &spec_writer, true),
                refused(Refusal::NotAllowed, ONLY_NOYAN_PASSES_ON),
            ),
            (
                "an agent, Dev started",
                judged(&quick, in_dev, &spec_writer, false),
                refused(Refusal::ChoiceLocked, locked),
            ),
        ];
        for (name, got, expected) in rows {
            assert_eq!(got, expected, "{name}");
        }
    }

    #[test]
    fn the_skipped_mark() {
        assert_eq!(
            skipped_mark("Product spec"),
            "Skipped Product spec · this ticket's choice"
        );
    }
}

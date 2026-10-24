//! A ticket's stage choices (AGM-08, AGM-11; d0108's "Options", d0087): a stage of the flow may list `options`
//! (other agents, or `skip`), and a ticket keeps **one choice per such stage** (table `ticket_stage_choices`) with
//! who set it. No choice = the stage's usual agent ("Grill: Griller"), "required" on a stage that runs no agent ("Review: required";
//! FLW-22, Q290).
//!
//! **Who may set what** ([`judge`]): the user any stage until it starts, and the user's choice wins; an agent (the
//! Project agent, an epic lead, a ticket's own agent) only with a reason, never over the user's choice, and once the
//! ticket has started only a **later** stage's; before the ticket starts it may set any stage's. An agent's request
//! that carries `--by-user` (d0087) is the user's. Which tickets each agent may change at all is the policy's
//! ([`crate::policy::Action::SetChoices`], CLI-06, CLI-07): the Project agent its Backlog tickets, an epic lead its
//! epic's unstarted children, a ticket agent its own ticket. A choice **locks** when its stage starts (the ticket
//! has a run of it, STG-05). A choice must be one of the stage's options, else it is refused naming them; a stage
//! without options refuses any. Epics have no choices.
//!
//! **What a choice does:** the running agent of a stage is its choice, else the stage's agent
//! ([`crate::agents::resolve::running_agent`]); a skipped stage is passed with a run of result `skipped`
//! ([`Board::ticket_skips`] feeds the flow engine), and a ticket that skipped a human check reaches its Delivery in
//! click mode (the engine's [`crate::flow::engine::At::skips`]). A stored choice for a stage the flow no longer has,
//! or for an agent the stage no longer offers, is ignored.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError, Refusal};
use crate::agents::Agent;
use crate::agents::resolve;
use crate::domain::{Actor, Kind, Place, TicketKey};
use crate::flow::{Flow, Stage, StageOption};
use crate::store::now_ms;

/// The word for skipping a stage (`--stage QA=skip`).
pub const SKIP: &str = StageOption::SKIP;

/// The word for a skippable stage's usual choice (`--stage QA=required`): it runs.
pub const REQUIRED: &str = "required";

/// An agent's choice without a reason (AGM-08).
pub const SAY_WHY: &str = "Say why with --reason";

/// One choice a request sets: `{"stage": "Dev", "choice": "Quick developer", "reason": "typo fix"}`. `stage` is a
/// stage's name (ignoring case) or id; `choice` an agent's name (ignoring case), [`SKIP`] or [`REQUIRED`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoicePick {
    pub stage: String,
    pub choice: String,
    /// An agent's reason ("typo fix"); unused for the user's.
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
    /// The stage's own agent; "required" on a stage that runs no agent.
    Usual,
    /// Another agent of the stage's options.
    Agent { id: String, name: String },
    /// The ticket skips the stage.
    Skip,
}

/// How a stage's usual choice reads: its agent ("Rust developer", "Griller" on a skippable Grill), or "required" for a
/// stage that runs no agent (a human check; FLW-22, Q290).
fn usual_word(stage: &Stage) -> String {
    match &stage.agent {
        Some(agent) => agent.clone(),
        None => REQUIRED.to_owned(),
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
    /// An epic lead ("Wishlist lead").
    EpicLead { name: String, reason: String },
    /// A ticket's own agent, by the agent it runs ("Griller").
    Agent { name: String, reason: String },
}

impl SetBy {
    /// "you", "the Project agent", "Wishlist lead", "Griller".
    pub fn who(&self) -> String {
        match self {
            Self::User => "you".to_owned(),
            Self::ProjectAgent { .. } => "the Project agent".to_owned(),
            Self::EpicLead { name, .. } | Self::Agent { name, .. } => name.clone(),
        }
    }

    /// An agent's reason.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::User => None,
            Self::ProjectAgent { reason, .. }
            | Self::EpicLead { reason, .. }
            | Self::Agent { reason, .. } => Some(reason),
        }
    }

    pub fn is_user(&self) -> bool {
        *self == Self::User
    }

    fn stored(&self) -> (&'static str, Option<&str>) {
        match self {
            Self::User => ("user", None),
            Self::ProjectAgent { name, .. } => ("project_agent", Some(name)),
            Self::EpicLead { name, .. } => ("epic_lead", Some(name)),
            Self::Agent { name, .. } => ("agent", Some(name)),
        }
    }

    fn read(set_by: &str, name: Option<String>, reason: Option<String>) -> Self {
        let (name, reason) = (name.unwrap_or_default(), reason.unwrap_or_default());
        match set_by {
            "project_agent" => Self::ProjectAgent { name, reason },
            "epic_lead" => Self::EpicLead { name, reason },
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
    EpicLead {
        name: String,
    },
    /// A ticket's own agent, by the agent its session runs.
    TicketAgent {
        name: String,
    },
}

impl Chooser {
    /// Who the choice is recorded as: the user for the user and for `by_user` (d0087), else the agent with `reason`.
    fn set_by(&self, by_user: bool, reason: &str) -> SetBy {
        let reason = reason.trim().to_owned();
        match self {
            _ if by_user => SetBy::User,
            Self::User => SetBy::User,
            Self::ProjectAgent { name } => SetBy::ProjectAgent {
                name: name.clone(),
                reason,
            },
            Self::EpicLead { name } => SetBy::EpicLead {
                name: name.clone(),
                reason,
            },
            Self::TicketAgent { name } => SetBy::Agent {
                name: name.clone(),
                reason,
            },
        }
    }
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

/// One agent a stage offers, as the flow names it, and the agent that name stands for (`None` when it isn't one of
/// the agents: a check error of the flow).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionAgent {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// A stage with options, and the ticket's choice for it (AGM-09's pickers; T-0411 draws them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageOptions {
    pub stage_id: String,
    /// The stage's name ("Dev").
    pub stage: String,
    /// The stage's own agent ("Griller"); `None` for a stage that runs none (a human check). See
    /// [`Self::usual_label`] for how the usual choice reads.
    pub usual: Option<OptionAgent>,
    /// The other agents a ticket may pick, in the flow's order.
    pub agents: Vec<OptionAgent>,
    /// `skip` is one of the options.
    pub can_skip: bool,
    /// The ticket's choice; `None` = the usual one, nobody chose.
    pub choice: Option<StageChoice>,
    /// The stage has started: its choice can't change any more.
    pub locked: bool,
}

impl StageOptions {
    /// The ticket skips this stage.
    pub fn skipped(&self) -> bool {
        self.choice
            .as_ref()
            .is_some_and(|c| c.choice == Choice::Skip)
    }

    /// The usual choice as it reads: its agent ("Rust developer", "Griller"), or [`REQUIRED`] for a stage that runs no
    /// agent (FLW-22, Q290).
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

/// The answer to `ticket.choices`: a ticket's (or, with no key, a new ticket's in a project) stages with options, in
/// flow order; empty on the default flow and for an epic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketChoices {
    pub key: Option<TicketKey>,
    pub stages: Vec<StageOptions>,
}

/// The mark of a stage a ticket passed by its choice: "Skipped Grill · this ticket's choice" (AGM-08; the card's
/// wording, drawn by T-0411).
pub fn skipped_mark(stage: &str) -> String {
    format!("Skipped {stage} · this ticket's choice")
}

/// The ticket a choice is for, as [`judge`] needs it: one that exists, or one a batch is about to create.
#[derive(Debug, Clone, Copy)]
pub struct Target<'a> {
    /// "SHOP-5", or "the new ticket \"Fix README typo\"".
    pub label: &'a str,
    pub kind: Kind,
    pub started: bool,
    /// The first stage (an index into the flow) an agent may still choose for once the ticket has started: the one
    /// after the stage it is in, or the one it waits before.
    pub later_from: usize,
    /// The stages that have started for it (it has a run of each).
    pub ran: &'a [String],
}

/// The refusal of an agent's choice over the user's: "SHOP-5's Dev choice was set by you; agents can't change it".
pub fn set_by_you_refusal(label: &str, stage: &str) -> String {
    format!("{label}'s {stage} choice was set by you; agents can't change it")
}

/// The stage `name` names in `flow`: its id, or its name ignoring case. An unknown one lists the stages that have
/// options.
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
                .filter(|s| !s.options.is_empty())
                .map(|s| s.name.as_str())
                .collect();
            let listed = if with.is_empty() {
                "none of its stages has options".to_owned()
            } else {
                format!("stages with options: {}", with.join(", "))
            };
            BoardError::NotFound(format!("{project}'s flow has no stage {name} ({listed})"))
        })
}

/// What a stage offers, in the words a choice uses: its usual agent (or [`REQUIRED`]), its other agents, [`SKIP`].
fn option_words(stage: &Stage) -> Vec<String> {
    let mut words = vec![usual_word(stage)];
    words.extend(stage.options.iter().map(|o| o.as_str().to_owned()));
    words
}

/// The choice `word` means for `stage`, among its `options` (resolved): refused when the stage has no options or
/// `word` isn't one of them ("Quick dev isn't one of Dev's options: Rust developer, Quick developer"). A skip of a
/// stage without options says how the flow allows it (#62, d0108: a flow adds `skip` to a stage's options).
pub fn choice_of(stage: &Stage, options: &StageOptions, word: &str) -> Result<Choice, BoardError> {
    let word = word.trim();
    if stage.options.is_empty() {
        let text = if word.eq_ignore_ascii_case(SKIP) {
            format!(
                "{} has no options; add `{SKIP}` to its options in the flow to allow it.",
                stage.name
            )
        } else {
            format!(
                "{} has no options: every ticket runs it as the flow says",
                stage.name
            )
        };
        return Err(BoardError::refused(Refusal::NotAnOption, text));
    }
    let folded = word.to_lowercase();
    let usual = match &options.usual {
        Some(agent) => agent.name.to_lowercase() == folded || folded == REQUIRED,
        None => folded == REQUIRED,
    };
    if usual {
        return Ok(Choice::Usual);
    }
    if options.can_skip && folded == SKIP {
        return Ok(Choice::Skip);
    }
    if let Some(agent) = options
        .agents
        .iter()
        .find(|a| a.name.to_lowercase() == folded)
    {
        // A name the flow lists that isn't one of the agents can't run: the flow's check names it.
        let id = agent.agent_id.clone().ok_or_else(|| {
            BoardError::refused(
                Refusal::UnknownAgent,
                crate::flow::check::unknown_agent_message(stage, &agent.name),
            )
        })?;
        return Ok(Choice::Agent {
            id,
            name: agent.name.clone(),
        });
    }
    Err(BoardError::refused(
        Refusal::NotAnOption,
        format!(
            "{word} isn't one of {}'s options: {}",
            stage.name,
            option_words(stage).join(", ")
        ),
    ))
}

/// Judges one choice (AGM-08; d0108, d0087) for `target` at flow position `index`, over `current` (the choice it
/// has). `by_user` makes an agent's choice the user's.
///
/// - an epic has no choices;
/// - a stage that started is locked, for everyone;
/// - an agent's own choice needs a reason, never overrides the user's, and once the ticket has started is only for
///   a later stage.
pub fn judge(
    target: Target<'_>,
    stage: &Stage,
    index: usize,
    current: Option<&StageChoice>,
    chooser: &Chooser,
    by_user: bool,
    reason: Option<&str>,
) -> Result<(), BoardError> {
    if target.kind == Kind::Epic {
        return Err(BoardError::refused(
            Refusal::NotAnOption,
            format!("{} is an epic: epics have no stage choices", target.label),
        ));
    }
    if target.ran.contains(&stage.id) {
        return Err(BoardError::refused(
            Refusal::ChoiceLocked,
            format!(
                "{} has started on {}: its choice is locked",
                stage.name, target.label
            ),
        ));
    }
    if *chooser == Chooser::User || by_user {
        return Ok(());
    }
    if reason.is_none_or(|r| r.trim().is_empty()) {
        return Err(BoardError::refused(Refusal::EmptyField, SAY_WHY));
    }
    if current.is_some_and(|c| c.set_by.is_user()) {
        return Err(BoardError::refused(
            Refusal::NotAllowed,
            set_by_you_refusal(target.label, &stage.name),
        ));
    }
    if target.started && index < target.later_from {
        return Err(BoardError::refused(
            Refusal::NotAllowed,
            format!(
                "{} has started: an agent can only change the choice of a stage it hasn't reached, not {}",
                target.label, stage.name
            ),
        ));
    }
    Ok(())
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

/// The first stage an agent may still choose for on a started ticket at `place` (see [`Target::later_from`]).
fn later_from(flow: &Flow, place: &Place) -> usize {
    match place {
        Place::InStage(id) => flow.position(id).map_or(0, |i| i + 1),
        Place::Waiting(Some(id)) => flow.position(id).unwrap_or(0),
        Place::Done => flow.stages.len(),
        Place::Backlog | Place::Waiting(None) => 0,
    }
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

impl Board {
    /// What `stage` offers, resolved to the agents (a name that isn't one of them has no id).
    async fn options_of(&self, stage: &Stage) -> Result<StageOptions, BoardError> {
        let resolve = async |name: &str| -> Result<OptionAgent, BoardError> {
            Ok(OptionAgent {
                name: name.to_owned(),
                agent_id: resolve::named(self.store(), name).await?.map(|a| a.id),
            })
        };
        let usual = match &stage.agent {
            Some(name) => Some(resolve(name).await?),
            None => None,
        };
        let mut agents = Vec::new();
        for option in &stage.options {
            if let StageOption::Agent(name) = option {
                agents.push(resolve(name).await?);
            }
        }
        Ok(StageOptions {
            stage_id: stage.id.clone(),
            stage: stage.name.clone(),
            usual,
            agents,
            can_skip: stage.can_skip(),
            choice: None,
            locked: false,
        })
    }

    /// The stages of `flow` with options, each with what it offers.
    async fn flow_options(&self, flow: &Flow) -> Result<Vec<StageOptions>, BoardError> {
        let mut stages = Vec::new();
        for stage in flow.stages.iter().filter(|s| !s.options.is_empty()) {
            stages.push(self.options_of(stage).await?);
        }
        Ok(stages)
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

    /// The ticket's stored choices that still hold in `options` (the stage is still there and still offers what was
    /// chosen), by stage id.
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
                ("agent", Some(id)) => match offered
                    .agents
                    .iter()
                    .find(|a| a.agent_id.as_deref() == Some(id.as_str()))
                {
                    Some(agent) => Choice::Agent {
                        id,
                        name: agent.name.clone(),
                    },
                    None => continue,
                },
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

    /// `ticket.choices` for an existing ticket: its stages with options, its choices and locks. Empty for an epic.
    pub async fn ticket_choices(&self, key: &str) -> Result<TicketChoices, BoardError> {
        let ticket = self.ticket(key).await?;
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let stages = match ticket.kind {
            Kind::Epic => Vec::new(),
            Kind::Ticket => {
                let mut stages = self.flow_options(&flow).await?;
                let stored = self.stored_choices(&ticket.id, &stages).await?;
                let ran = self.stages_ran(&ticket.id).await?;
                for stage in &mut stages {
                    stage.choice = stored
                        .iter()
                        .find(|c| c.stage_id == stage.stage_id)
                        .cloned();
                    stage.locked = ran.contains(&stage.stage_id);
                }
                stages
            }
        };
        Ok(TicketChoices {
            key: Some(ticket.key),
            stages,
        })
    }

    /// `ticket.choices` for a ticket not created yet in `project` (the ⌘N form): its flow's stages with options.
    pub async fn new_ticket_choices(&self, project: &str) -> Result<TicketChoices, BoardError> {
        let flow = self.effective_flow(project).await?;
        Ok(TicketChoices {
            key: None,
            stages: self.flow_options(&flow).await?,
        })
    }

    /// The stages `ticket` skips by its choices (the flow engine's [`crate::flow::engine::At::skips`]); none for an
    /// epic and on a flow without `skip` options.
    pub async fn ticket_skips(
        &self,
        ticket: &Ticket,
        flow: &Flow,
    ) -> Result<Vec<String>, BoardError> {
        if ticket.is_epic() || !flow.stages.iter().any(Stage::can_skip) {
            return Ok(Vec::new());
        }
        let skips: Vec<String> = sqlx::query_scalar(
            "SELECT stage_id FROM ticket_stage_choices WHERE ticket_id = ? AND choice = 'skip'",
        )
        .bind(&ticket.id)
        .fetch_all(self.store().pool())
        .await?;
        Ok(skips
            .into_iter()
            .filter(|id| flow.stage(id).is_some_and(Stage::can_skip))
            .collect())
    }

    /// The agent `ticket` chose for `stage` instead of its usual one (AGM-05, AGM-08), if it chose one the stage
    /// still offers.
    pub async fn chosen_agent(
        &self,
        ticket: &Ticket,
        stage: &Stage,
    ) -> Result<Option<Agent>, BoardError> {
        if ticket.is_epic()
            || !stage
                .options
                .iter()
                .any(|o| matches!(o, StageOption::Agent(_)))
        {
            return Ok(None);
        }
        let id: Option<Option<String>> = sqlx::query_scalar(
            "SELECT agent_id FROM ticket_stage_choices WHERE ticket_id = ? AND stage_id = ? AND choice = 'agent'",
        )
        .bind(&ticket.id)
        .bind(&stage.id)
        .fetch_optional(self.store().pool())
        .await?;
        let Some(id) = id.flatten() else {
            return Ok(None);
        };
        for option in &stage.options {
            if let StageOption::Agent(name) = option
                && let Some(agent) = resolve::named(self.store(), name).await?
                && agent.id == id
            {
                return Ok(Some(agent));
            }
        }
        Ok(None)
    }

    /// Checks every pick for `ticket` (`None`: a ticket of `kind` a batch is about to create in `project`, labelled
    /// `label`) and returns what to write; nothing is written, so a refused pick leaves the ticket (or the whole
    /// batch) as it was.
    pub async fn check_choices(
        &self,
        project: &str,
        ticket: Option<&Ticket>,
        new: (Kind, &str),
        picks: &[ChoicePick],
        chooser: &Chooser,
        by_user: bool,
    ) -> Result<Vec<ChoiceWrite>, BoardError> {
        if picks.is_empty() {
            return Ok(Vec::new());
        }
        let flow = self.effective_flow(project).await?;
        let options = self.flow_options(&flow).await?;
        let (stored, ran, label) = match ticket {
            Some(t) if !t.is_epic() => (
                self.stored_choices(&t.id, &options).await?,
                self.stages_ran(&t.id).await?,
                t.key.to_string(),
            ),
            Some(t) => (Vec::new(), Vec::new(), t.key.to_string()),
            None => (Vec::new(), Vec::new(), new.1.to_owned()),
        };
        let target = Target {
            label: &label,
            kind: ticket.map_or(new.0, |t| t.kind),
            started: ticket.is_some_and(Ticket::has_started),
            later_from: ticket.map_or(0, |t| later_from(&flow, &t.place)),
            ran: &ran,
        };
        let mut writes: Vec<ChoiceWrite> = Vec::with_capacity(picks.len());
        for pick in picks {
            let stage = find_stage(&flow, project, &pick.stage)?;
            let index = flow.position(&stage.id).expect("a stage of the flow");
            if target.kind == Kind::Epic {
                judge(target, stage, index, None, chooser, by_user, None)?;
            }
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
                });
            let choice = choice_of(stage, &offered, &pick.choice)?;
            let current = stored.iter().find(|c| c.stage_id == stage.id);
            judge(
                target,
                stage,
                index,
                current,
                chooser,
                by_user,
                pick.reason.as_deref(),
            )?;
            let write = ChoiceWrite {
                stage_id: stage.id.clone(),
                stage: stage.name.clone(),
                label: choice.label(stage),
                choice,
                set_by: chooser.set_by(by_user, pick.reason.as_deref().unwrap_or_default()),
            };
            // A later pick for the same stage replaces an earlier one.
            writes.retain(|w| w.stage_id != write.stage_id);
            writes.push(write);
        }
        Ok(writes)
    }

    /// Writes checked choices on `ticket`, skipping those that already read so. Returns whether one changed. An epic
    /// lead's choice for a child is logged on the child and on the epic (`epic_id`).
    pub async fn write_choices(
        &self,
        ticket: &Ticket,
        writes: &[ChoiceWrite],
        actor: &Actor,
        epic_id: Option<&str>,
    ) -> Result<bool, BoardError> {
        if writes.is_empty() {
            return Ok(false);
        }
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let options = self.flow_options(&flow).await?;
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
                choice_event(write, from.as_deref(), None),
            )
            .await?;
            if let Some(epic_id) = epic_id.filter(|epic| *epic != ticket.id) {
                record_event(
                    &mut tx,
                    epic_id,
                    EventKind::StageChoiceSet,
                    actor,
                    choice_event(write, from.as_deref(), Some(&ticket.key)),
                )
                .await?;
            }
            changed = true;
        }
        tx.commit().await?;
        Ok(changed)
    }
}

/// The History payload of one choice ([`EventKind::StageChoiceSet`]).
fn choice_event(write: &ChoiceWrite, from: Option<&str>, child: Option<&TicketKey>) -> Value {
    let mut payload = json!({
        "stage": write.stage,
        "stage_id": write.stage_id,
        "choice": write.label,
        "from": from,
        "set_by": write.set_by.stored().0,
        "by": write.set_by.who(),
        "reason": write.set_by.reason(),
    });
    if let Some(child) = child {
        payload["child"] = json!(child);
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{CheckBy, StageKind};

    fn stage(id: &str, agent: Option<&str>, options: &[&str]) -> Stage {
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
            options: options
                .iter()
                .map(|o| match *o {
                    SKIP => StageOption::Skip,
                    name => StageOption::Agent(name.into()),
                })
                .collect(),
            rounds: None,
            delivery: None,
            why: None,
        }
    }

    fn offered(stage: &Stage) -> StageOptions {
        let agent = |name: &str| OptionAgent {
            name: name.into(),
            agent_id: Some(format!("id-{}", name.to_lowercase())),
        };
        StageOptions {
            stage_id: stage.id.clone(),
            stage: stage.name.clone(),
            usual: stage.agent.as_deref().map(agent),
            agents: stage
                .options
                .iter()
                .filter_map(|o| match o {
                    StageOption::Agent(name) => Some(agent(name)),
                    StageOption::Skip => None,
                })
                .collect(),
            can_skip: stage.can_skip(),
            choice: None,
            locked: false,
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

    /// A choice must be one of the stage's options (the usual agent, its other agents, skip); a stage without
    /// options refuses any.
    #[test]
    fn a_choice_is_one_of_the_options() {
        let dev = stage("dev", Some("Rust developer"), &["Quick developer"]);
        let grill = stage("grill", Some("Griller"), &[SKIP]);
        let qa = stage("qa", None, &[SKIP]);
        let security = stage("security", Some("Security reviewer"), &[]);
        let of = |s: &Stage, word: &str| choice_of(s, &offered(s), word);
        assert_eq!(
            of(&dev, "quick developer").unwrap(),
            Choice::Agent {
                id: "id-quick developer".into(),
                name: "Quick developer".into()
            }
        );
        assert_eq!(of(&dev, "Rust developer").unwrap(), Choice::Usual);
        assert_eq!(of(&grill, "skip").unwrap(), Choice::Skip);
        assert_eq!(of(&qa, "required").unwrap(), Choice::Usual);
        assert_eq!(
            refusal(of(&dev, "skip")),
            (
                Refusal::NotAnOption,
                "skip isn't one of Dev's options: Rust developer, Quick developer".into()
            )
        );
        assert_eq!(
            refusal(of(&qa, "Griller")),
            (
                Refusal::NotAnOption,
                "Griller isn't one of Qa's options: required, skip".into()
            )
        );
        assert_eq!(
            refusal(of(&security, "Quick developer")),
            (
                Refusal::NotAnOption,
                "Security has no options: every ticket runs it as the flow says".into()
            )
        );
        assert_eq!(
            refusal(of(&security, "skip")),
            (
                Refusal::NotAnOption,
                "Security has no options; add `skip` to its options in the flow to allow it."
                    .into()
            )
        );
        assert_eq!(Choice::Usual.label(&qa), "required");
        // FLW-22 (Q290): a skippable Agent stage reads its agent.
        assert_eq!(Choice::Usual.label(&grill), "Griller");
        assert_eq!(of(&grill, "Griller").unwrap(), Choice::Usual);
        assert_eq!(Choice::Usual.label(&dev), "Rust developer");
    }

    /// Who may set what (AGM-08): the user until the stage starts; an agent with a reason, never over the user,
    /// later stages only once the ticket started; `--by-user` is the user's.
    #[test]
    fn the_choice_rule() {
        let dev = stage("dev", Some("Rust developer"), &["Quick developer"]);
        let user_choice = StageChoice {
            stage_id: "dev".into(),
            choice: Choice::Usual,
            set_by: SetBy::User,
            set_at: 0,
            from: None,
        };
        let agent_choice = StageChoice {
            set_by: SetBy::ProjectAgent {
                name: "Planner".into(),
                reason: "typo fix".into(),
            },
            ..user_choice.clone()
        };
        let none: [String; 0] = [];
        let grill_ran = ["grill".to_owned()];
        let dev_ran = ["grill".to_owned(), "dev".to_owned()];
        let fresh = Target {
            label: "SHOP-5",
            kind: Kind::Ticket,
            started: false,
            later_from: 0,
            ran: &none,
        };
        // In Grill (index 0): Dev (index 1) is later.
        let in_grill = Target {
            started: true,
            later_from: 1,
            ran: &grill_ran,
            ..fresh
        };
        let in_dev = Target {
            started: true,
            later_from: 2,
            ran: &dev_ran,
            ..fresh
        };
        let planner = Chooser::ProjectAgent {
            name: "Planner".into(),
        };
        let griller = Chooser::TicketAgent {
            name: "Griller".into(),
        };
        let judged = |target, current, chooser: &Chooser, by_user, reason| {
            judge(target, &dev, 1, current, chooser, by_user, reason)
                .map_err(|e| (e.refusal().unwrap(), e.to_string()))
        };
        let refused = |code, text: &str| Err((code, text.to_owned()));
        let set_by_you = "SHOP-5's Dev choice was set by you; agents can't change it";
        let locked = "Dev has started on SHOP-5: its choice is locked";
        let rows = [
            (
                "user, fresh",
                judged(fresh, None, &Chooser::User, false, None),
                Ok(()),
            ),
            (
                "user over an agent's choice",
                judged(in_grill, Some(&agent_choice), &Chooser::User, false, None),
                Ok(()),
            ),
            (
                "user, Dev started",
                judged(in_dev, None, &Chooser::User, false, None),
                refused(Refusal::ChoiceLocked, locked),
            ),
            (
                "Project agent at creation, with a reason",
                judged(fresh, None, &planner, false, Some("typo fix")),
                Ok(()),
            ),
            (
                "agent, no reason",
                judged(fresh, None, &planner, false, Some(" ")),
                refused(Refusal::EmptyField, SAY_WHY),
            ),
            (
                "agent over the user's choice",
                judged(fresh, Some(&user_choice), &planner, false, Some("x")),
                refused(Refusal::NotAllowed, set_by_you),
            ),
            (
                "agent over the user's choice, in the user's words",
                judged(fresh, Some(&user_choice), &planner, true, None),
                Ok(()),
            ),
            (
                "Griller changes the later Dev",
                judged(
                    in_grill,
                    Some(&agent_choice),
                    &griller,
                    false,
                    Some("touches session handling"),
                ),
                Ok(()),
            ),
            (
                "an agent, Dev started",
                judged(in_dev, None, &griller, false, Some("x")),
                refused(Refusal::ChoiceLocked, locked),
            ),
        ];
        for (name, got, expected) in rows {
            assert_eq!(got, expected, "{name}");
        }
        // A started ticket waiting before Dev: an agent may still choose for it; not for a stage before.
        let waiting = Target {
            later_from: 1,
            ..in_grill
        };
        assert!(judge(waiting, &dev, 1, None, &griller, false, Some("x")).is_ok());
        let grill = stage("grill", Some("Griller"), &[SKIP]);
        let past = Target {
            ran: &none,
            ..in_dev
        };
        assert_eq!(
            refusal(judge(past, &grill, 0, None, &griller, false, Some("x"))).0,
            Refusal::NotAllowed
        );
        let epic = Target {
            kind: Kind::Epic,
            label: "SHOP-40",
            ..fresh
        };
        assert_eq!(
            refusal(judge(epic, &dev, 1, None, &Chooser::User, false, None)).1,
            "SHOP-40 is an epic: epics have no stage choices"
        );
    }

    #[test]
    fn the_skipped_mark() {
        assert_eq!(
            skipped_mark("Grill"),
            "Skipped Grill · this ticket's choice"
        );
    }
}

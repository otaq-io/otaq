//! `otaq prime`: the context a ticket's agent reads first (PLG-02, AGM-06; d0032, d0040, d0108).
//!
//! The ticket (title, What and why, base branch), then what its agent is told (AGM-06): **the agent's
//! instructions**, then **the project's notes**, then **the stage it works in and that stage's skill**, then the
//! **earlier stages** ([`Board::earlier_stages`]: each stage's latest run with its agent and result, oldest first),
//! then the ticket's **comments** in full, oldest first (CMT-07, d0109: the hand-over between stages; a report is
//! only named, never printed) and the **manual checks** listed for the user (d0125). After them the tickets it
//! depends on with their last comment from an Agent stage, pending questions, recorded decisions, the **stage
//! choices** the agent may still set ([`reader_choices`]; none on the default flow), and Otaq's own rules (`stage
//! next`, `stage done`, `--manual-check`). The output is concise Markdown for an agent ([`render`]); `--json` prints
//! the [`PrimeContext`] itself.

use serde::{Deserialize, Serialize};

use crate::agent_tools::AgentTool;
use crate::agent_tools::models::{ModelKind, ModelList, ModelLists};
use crate::agents::{Agent, AgentModel, ModelRef, SessionRole};
use crate::board::comments::FeedbackComment;
use crate::board::conflicts::{Conflict, ConflictReason};
use crate::board::manual_checks::ManualCheck;
use crate::board::stage_choices::StageOptions;
use crate::board::ticket_comments::{CommentAuthor, TicketComment};
use crate::board::tickets::{Base, Ticket};
use crate::board::{Board, BoardError};
use crate::domain::{Place, Status, TicketKey};
use crate::flow::engine::RunResult;
use crate::flow::{Flow, Stage, StageKind};
use crate::naming;

/// Everything `prime` prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimeContext {
    pub ticket: Ticket,
    /// The epic a child belongs to: its key and title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic: Option<(TicketKey, String)>,
    pub parents: Vec<Parent>,
    /// Questions from other agents waiting for this one (M10); empty until then.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_questions: Vec<String>,
    /// The decisions this ticket's agent recorded (DEC-02), so a resumed or sent-back agent sees them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<crate::decisions::Decision>,
    /// The epic lead's unread notes to this ticket (LEAD-10), at the top; its own agent's prime marks them read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unread_notes: Vec<crate::epic_lead::notes::LeadNote>,
    /// The commit skill of the stage's agent (d0050), named for the caller's tool by its copy in the worktree
    /// (AGM-07): `/otaq-commit`, or the agent's imported skill standing for it (`/otaq-commit-compact`). Empty when
    /// the agent has no commit skill: it commits with git.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commit_skill: String,
    /// The restack skill in the caller's tool's syntax: `/otaq-restack`.
    pub restack_skill: String,
    /// The branch conflicts with the one it merges into: the agent resolves that first, with the restack skill
    /// (a Delivery's conflict, or a reopened ticket's rebase; GIT-03, GIT-06, FLW-17).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<Conflict>,
    /// The agent's own instructions and the project's notes (AGM-06).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<Instructions>,
    /// The stage the session works in, its agent and its skill (AGM-06); `None` for an epic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<CurrentStage>,
    /// The ticket's earlier stages (AGM-06), oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub earlier: Vec<EarlierStage>,
    /// The ticket's comments (CMT-07), oldest first, deleted ones left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<TicketComment>,
    /// The manual checks listed for the user so far (d0125), oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_checks: Vec<ManualCheck>,
    /// The pull request being prepared or already open (REV-10, REV-14): for the Open PR and Push skills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<PrimePr>,
    /// The stage choices the reader may set (AGM-06, AGM-08); none on the default flow. Filled by the service.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<PrimeChoice>,
}

/// The "Pull request" section: what the skill proposes from, and what the user confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimePr {
    /// "GitHub", "Azure DevOps"; `None` when origin's host matches neither.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// origin, as Settings shows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    pub view: crate::pr::flow::PrFlowView,
}

/// A ticket this one depends on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parent {
    pub key: TicketKey,
    pub title: String,
    pub status: Status,
    /// Its last comment from an Agent stage (CMT-07, [`Board::last_agent_comment`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_comment: Option<TicketComment>,
}

/// What an agent is told to follow (AGM-06; d0108, d0040, d0068): its agent's own instructions, then the notes every
/// agent in the project gets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instructions {
    /// The agent's name ("Developer"), when the session runs one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The agent's own instructions; empty when it has none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub own: String,
    /// The project's name ("shop-web").
    pub project: String,
    /// "Notes every agent in this project gets"; empty when it has none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

impl Instructions {
    /// "## Agent instructions (Developer)" with the agent's own (`heading` names them), then "## Notes every agent in
    /// shop-web gets"; each only when it has text.
    pub fn render(&self, heading: &str) -> String {
        let mut out = String::new();
        if !self.own.trim().is_empty() {
            let from = self
                .agent
                .as_deref()
                .map(|a| format!(" ({a})"))
                .unwrap_or_default();
            out.push_str(&format!("\n## {heading}{from}\n\n{}\n", self.own.trim()));
        }
        if !self.notes.trim().is_empty() {
            out.push_str(&format!(
                "\n## Notes every agent in {} gets\n\n{}\n",
                self.project,
                self.notes.trim()
            ));
        }
        out
    }
}

/// The stage a ticket's session works in (AGM-06): "Dev, as Developer, with the skill `/otaq-work-ticket`".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentStage {
    /// The stage's name ("Dev").
    pub name: String,
    /// The agent that runs it for this ticket (its choice, else the stage's own; AGM-05).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The stage's skill, named for the caller's tool by its copy (`/otaq-work-ticket`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
}

/// A stage the ticket ran before (AGM-06), as its latest run ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EarlierStage {
    /// The stage's name ("Grill").
    pub stage: String,
    /// The agent that ran it; "you" for a human check; `None` when it ran none (a skipped stage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// An agent check's round.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u32>,
    pub result: RunResult,
}

impl EarlierStage {
    /// "Grill · Griller · finished", "Validate · Validator · round 2 · passed", "QA · skipped".
    pub fn label(&self) -> String {
        let result = match self.result {
            RunResult::Passed => "passed",
            RunResult::Failed => "failed",
            RunResult::Done => "finished",
            RunResult::Skipped => "skipped",
            RunResult::Cancelled => "stopped",
            RunResult::NoResult => "ended without a verdict",
        };
        let mut parts = vec![self.stage.clone()];
        parts.extend(self.agent.clone());
        parts.extend(self.round.map(|r| format!("round {r}")));
        parts.push(result.to_owned());
        parts.join(" \u{b7} ")
    }
}

/// A stage whose choice the reader may set (AGM-06, AGM-08), with what it offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimeChoice {
    /// The stage's name ("Dev").
    pub stage: String,
    /// What the ticket runs there now ("Rust developer", "skip"); `None` for a new ticket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now: Option<String>,
    /// The stage's own agent first, then the other agents it offers.
    pub options: Vec<PrimeOption>,
    /// `skip` is one of the options.
    pub can_skip: bool,
}

/// An agent a stage offers, as `prime` lists it: "Quick developer · default · Claude Code · haiku: Small fixes only."
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimeOption {
    pub name: String,
    /// The stage's own agent.
    pub usual: bool,
    /// The model it runs on, as Settings shows it ([`agent_model_text`]); empty for a name that isn't an agent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// The first line of its instructions; empty when it has none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub about: String,
}

impl PrimeOption {
    /// "Rust developer (the stage's own) · Claude Code · sonnet: Follow our error-handling rules."
    pub fn label(&self) -> String {
        let mut out = self.name.clone();
        if self.usual {
            out.push_str(" (the stage's own)");
        }
        if !self.model.is_empty() {
            out.push_str(&format!(" \u{b7} {}", self.model));
        }
        if !self.about.is_empty() {
            out.push_str(&format!(": {}", self.about));
        }
        out
    }
}

/// A model as Settings shows it (AGT-02, AUDIT-y C2), for the core's own texts: "tool default" (no model, or the
/// tool's default entry), "<Display> · always the newest" for a follow choice, else the text as saved. The view
/// models' `agents::model_label` says the same for the screens.
pub fn model_text(model: Option<&str>, list: Option<&ModelList>) -> String {
    let id = model.map(str::trim).unwrap_or_default();
    match list.and_then(|l| l.entry(id)) {
        _ if id.is_empty() => TOOL_DEFAULT.to_owned(),
        Some(e) if e.kind == ModelKind::ToolDefault => TOOL_DEFAULT.to_owned(),
        Some(e) if e.kind == ModelKind::Follow => {
            format!("{} \u{b7} always the newest", e.display_name)
        }
        None if id == "default" && list.is_none() => TOOL_DEFAULT.to_owned(),
        _ => id.to_owned(),
    }
}

/// The tool default beside a tool's name: "Claude Code · tool default".
const TOOL_DEFAULT: &str = "tool default";

/// An agent's model as `prime` names it (AGM-01): "Claude Code · Opus · always the newest · effort high", or
/// "default · Claude Code · haiku" for `default` (what the default model is today; just "default" while none is
/// set).
pub fn agent_model_text(agent: &Agent, default: Option<&ModelRef>, models: &ModelLists) -> String {
    let on = |m: &ModelRef| {
        let copy = models.copy(m.tool);
        format!(
            "{} \u{b7} {}",
            m.tool.display_name(),
            model_text(m.model.as_deref(), copy.as_ref().map(|c| &c.list))
        )
    };
    let mut text = match (&agent.model, default) {
        (AgentModel::Default, Some(m)) => format!("default \u{b7} {}", on(m)),
        (AgentModel::Default, None) => "default".to_owned(),
        (AgentModel::Tool { tool, model }, _) => on(&ModelRef {
            tool: *tool,
            model: model.clone(),
        }),
    };
    if let Some(effort) = &agent.effort {
        text.push_str(&format!(" \u{b7} effort {effort}"));
    }
    text
}

/// Who reads the stage choices, which says what it may do with them (AGM-08).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentReader {
    /// A ticket's agent: later stages of its own ticket, with a reason.
    TicketAgent,
    /// A Project agent: any stage of the tickets it creates.
    ProjectAgent,
    /// An epic lead: any stage of its never-started children.
    EpicLead,
}

/// Whose stage choices a prime lists ([`reader_choices`]).
#[derive(Debug, Clone, Copy)]
pub enum ChoicesFor<'a> {
    /// A ticket's own agent: the stages it hasn't reached, whose choice the user didn't set.
    Ticket(&'a Ticket),
    /// A Project agent or an epic lead in `project`: every stage with options, as a new ticket has them.
    NewTickets(&'a str),
}

/// The stage choices a prime lists (AGM-06, AGM-08), each option with its model and its instructions' first line;
/// none on the default flow (no stage has options) and for an epic.
pub async fn reader_choices(
    board: &Board,
    of: ChoicesFor<'_>,
    models: &ModelLists,
) -> Result<Vec<PrimeChoice>, BoardError> {
    let stages: Vec<StageOptions> = match of {
        ChoicesFor::NewTickets(project) => board.new_ticket_choices(project).await?.stages,
        ChoicesFor::Ticket(ticket) => {
            let stages = board.ticket_choices(&ticket.key.to_string()).await?.stages;
            if stages.is_empty() {
                return Ok(Vec::new());
            }
            let flow = board.effective_flow(ticket.key.project.as_str()).await?;
            let from = match ticket.has_started() {
                true => later_from(&flow, &ticket.place),
                false => 0,
            };
            stages
                .into_iter()
                .filter(|s| {
                    !s.locked
                        && flow.position(&s.stage_id).is_some_and(|i| i >= from)
                        && !s.choice.as_ref().is_some_and(|c| c.set_by.is_user())
                })
                .collect()
        }
    };
    if stages.is_empty() {
        return Ok(Vec::new());
    }
    let default = board.store().default_model().await?;
    let mut choices = Vec::with_capacity(stages.len());
    for stage in stages {
        let mut options = Vec::new();
        let offered = stage
            .usual
            .iter()
            .map(|a| (a, true))
            .chain(stage.agents.iter().map(|a| (a, false)));
        for (option, usual) in offered {
            let agent = match &option.agent_id {
                Some(id) => board.store().agent(id).await?,
                None => None,
            };
            options.push(PrimeOption {
                name: option.name.clone(),
                usual,
                model: agent
                    .as_ref()
                    .map(|a| agent_model_text(a, default.as_ref(), models))
                    .unwrap_or_default(),
                about: agent
                    .as_ref()
                    .and_then(|a| {
                        a.instructions
                            .lines()
                            .map(str::trim)
                            .find(|l| !l.is_empty())
                    })
                    .unwrap_or_default()
                    .to_owned(),
            });
        }
        choices.push(PrimeChoice {
            now: matches!(of, ChoicesFor::Ticket(_)).then(|| stage.label()),
            stage: stage.stage,
            options,
            can_skip: stage.can_skip,
        });
    }
    Ok(choices)
}

/// The first stage (an index into `flow`) a ticket's agent may still choose for once the ticket has started: the one
/// after the stage it is in, or the one it waits before (as `stage_choices` judges an agent's choice).
fn later_from(flow: &Flow, place: &Place) -> usize {
    match place {
        Place::InStage(id) => flow.position(id).map_or(0, |i| i + 1),
        Place::Waiting(Some(id)) => flow.position(id).unwrap_or(0),
        Place::Done => flow.stages.len(),
        Place::Backlog | Place::Waiting(None) => 0,
    }
}

/// The "## Stage choices" section (AGM-06, AGM-08): nothing when the reader may set none (the default flow), else
/// what `reader` may do, with an example from the first stage, and each stage's options. `key` is the reading
/// ticket's own.
pub fn render_choices(
    choices: &[PrimeChoice],
    reader: AgentReader,
    key: Option<&TicketKey>,
) -> String {
    let Some(first) = choices.first() else {
        return String::new();
    };
    let cmd = naming::COMMAND_NAME;
    let pick = first
        .options
        .iter()
        .find(|o| !o.usual)
        .map(|o| o.name.clone())
        .unwrap_or_else(|| crate::board::stage_choices::SKIP.to_owned());
    let example = format!("--stage \"{}={pick}\"", first.stage);
    let lead = match reader {
        AgentReader::TicketAgent => format!(
            "You may change these later stages' choices for this ticket when what you learned changes how hard their \
             work is, always with a reason: `{cmd} update {} {example} --reason \"…\"`. A choice the user set isn't \
             listed: it can't be changed.",
            key.map(ToString::to_string)
                .unwrap_or_else(|| "<KEY>".to_owned())
        ),
        AgentReader::ProjectAgent => format!(
            "A new ticket runs each stage's own agent. Pick another option for a ticket you create only when how hard \
             the work is says so, with a reason: `{cmd} create \"Title\" {example} --reason \"…\" --stdin`. When \
             unsure, leave the stage's own agent."
        ),
        AgentReader::EpicLead => format!(
            "Set them on never-started children only, when how hard the work is says so, always with a reason: \
             `{cmd} update <KEY> {example} --reason \"…\"`, or on `{cmd} create --parent`. Never change a choice \
             the user set."
        ),
    };
    let mut out = format!("\n## Stage choices\n\n{lead}\n\n");
    for c in choices {
        match &c.now {
            Some(now) => out.push_str(&format!("- {} (now: {now})\n", c.stage)),
            None => out.push_str(&format!("- {}\n", c.stage)),
        }
        for option in &c.options {
            out.push_str(&format!("  - {}\n", option.label()));
        }
        if c.can_skip {
            out.push_str("  - skip: the ticket passes this stage\n");
        }
    }
    out
}

impl Board {
    /// The prime context of a ticket, its skills named for an agent of `tool` (ADP-05): the caller's session's
    /// tool, or a new session's for someone running `otaq prime` by hand. The stage's skill and the commit skill are
    /// the stage's running agent's (AGM-05), resolved for it ([`crate::skills::Sources::runs`]: its imported skill
    /// standing for `commit`), by their copies' names in the worktree (AGM-07).
    pub async fn prime_context(
        &self,
        key: &str,
        tool: &dyn AgentTool,
    ) -> Result<PrimeContext, BoardError> {
        let (mut context, agent) = self
            .unnamed_prime_context(key, SessionRole::Developer)
            .await?;
        let sources = crate::skills::Sources {
            shipped: None,
            data: self.data(),
        };
        if let Some(stage) = &mut context.stage {
            stage.skill = stage
                .skill
                .take()
                .map(|skill| tool.skill_reference(&sources.runs(agent.as_ref(), &skill).own));
        }
        context.commit_skill = commit_skill(&sources, agent.as_ref())
            .map(|skill| tool.skill_reference(&skill))
            .unwrap_or_default();
        context.restack_skill = tool.skill_reference(crate::review::RESTACK_SKILL);
        Ok(context)
    }

    /// The prime context of `role`'s session (its stage: an Agent stage's, or an agent check's), with its skills by
    /// their own names, and the agent the stage runs for the ticket.
    async fn unnamed_prime_context(
        &self,
        key: &str,
        role: SessionRole,
    ) -> Result<(PrimeContext, Option<Agent>), BoardError> {
        let ticket = self.ticket(key).await?;
        let project = ticket.key.project.to_string();
        let tickets = self.tickets(&project).await?;
        let mut parents = Vec::new();
        for key in &ticket.depends_on {
            let Some(parent) = tickets.iter().find(|t| &t.key == key) else {
                continue;
            };
            parents.push(Parent {
                key: parent.key.clone(),
                title: parent.title.clone(),
                status: parent.status,
                last_comment: self.last_agent_comment(&parent.key.to_string()).await?,
            });
        }
        let epic = ticket.epic.as_ref().and_then(|e| {
            tickets
                .iter()
                .find(|t| &t.key == e)
                .map(|t| (t.key.clone(), t.title.clone()))
        });
        let flow = self.effective_flow(&project).await?;
        let stage: Option<Stage> = match ticket.is_epic() {
            true => None,
            false => {
                let skips = self.ticket_skips(&ticket, &flow).await?;
                match role {
                    SessionRole::Validator => {
                        crate::agents::resolve::check_stage_of(&flow, &ticket, &skips)
                    }
                    _ => crate::agents::resolve::agent_stage_of(&flow, &ticket, &skips),
                }
                .cloned()
            }
        };
        let agent = match &stage {
            Some(stage) => crate::agents::resolve::running_agent(self, &ticket, &stage.id)
                .await
                .ok(),
            None => None,
        };
        let instructions = self.instructions(&project, agent.as_ref()).await?;
        let earlier = self
            .earlier_stages(&ticket, &flow, stage.as_ref().map(|s| s.id.as_str()))
            .await?;
        let conflict = self.conflict(&ticket.id).await?;
        let pull_request = self.prime_pr(&ticket).await?;
        // Questions from linked tickets' agents waiting for this one's answer (MSG-02).
        let cmd = naming::COMMAND_NAME;
        let pending_questions = self
            .pending_questions(&ticket.id)
            .await?
            .into_iter()
            .map(|m| {
                format!(
                    "{} from {}: {} \u{2014} answer with `{cmd} answer {} --stdin`",
                    m.id,
                    m.asker(),
                    m.question,
                    m.id
                )
            })
            .collect();
        let decisions = crate::decisions::list_by_ticket_id(self, &ticket.id).await?;
        let unread_notes = crate::epic_lead::notes::unread(self, &ticket.id).await?;
        let comments = self.ticket_comments(key).await?.comments;
        let manual_checks = self.manual_checks(key).await?.items;
        let context = PrimeContext {
            unread_notes,
            conflict,
            ticket,
            epic,
            parents,
            pending_questions,
            decisions,
            commit_skill: String::new(),
            restack_skill: String::new(),
            instructions,
            stage: stage.map(|s| CurrentStage {
                name: s.name,
                agent: agent.as_ref().map(|a| a.name.clone()),
                skill: s.skill,
            }),
            earlier,
            comments,
            manual_checks,
            pull_request,
            choices: Vec::new(),
        };
        Ok((context, agent))
    }

    /// What an agent in `project` is told to follow (AGM-06): `agent`'s own instructions, then the project's notes;
    /// `None` when both are empty.
    pub async fn instructions(
        &self,
        project: &str,
        agent: Option<&Agent>,
    ) -> Result<Option<Instructions>, BoardError> {
        let own = agent
            .map(|a| a.instructions.trim().to_owned())
            .unwrap_or_default();
        let notes = self
            .project_settings(project)
            .await?
            .project_notes
            .trim()
            .to_owned();
        if own.is_empty() && notes.is_empty() {
            return Ok(None);
        }
        Ok(Some(Instructions {
            agent: agent.map(|a| a.name.clone()),
            own,
            project: self.project(project).await?.name,
            notes,
        }))
    }

    /// The ticket's earlier stages (AGM-06): each stage's latest run that ended (the latest round of an agent check),
    /// oldest first, leaving out `except` (the reader's own stage, whose runs are its own work).
    pub async fn earlier_stages(
        &self,
        ticket: &Ticket,
        flow: &Flow,
        except: Option<&str>,
    ) -> Result<Vec<EarlierStage>, BoardError> {
        let mut latest: Vec<crate::board::stage_runs::StageRun> = Vec::new();
        for run in self.stage_runs(&ticket.key.to_string()).await? {
            if run.result.is_none() || Some(run.stage.as_str()) == except {
                continue;
            }
            latest.retain(|r| r.stage != run.stage);
            latest.push(run);
        }
        Ok(latest
            .into_iter()
            .filter_map(|run| {
                Some(EarlierStage {
                    stage: flow
                        .stage(&run.stage)
                        .map_or_else(|| run.stage.clone(), |s| s.name.clone()),
                    agent: match run.kind.as_str() {
                        "you" => Some("you".to_owned()),
                        _ => run.agent,
                    },
                    round: run.round,
                    result: run.result?,
                })
            })
            .collect())
    }

    /// A ticket's last comment from an Agent stage's agent (CMT-07): what its dependents and its epic's lead read.
    /// A stage the flow no longer has counts as an Agent stage when the comment has no check round.
    pub async fn last_agent_comment(&self, key: &str) -> Result<Option<TicketComment>, BoardError> {
        let ticket = self.ticket(key).await?;
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        Ok(self
            .ticket_comments(key)
            .await?
            .comments
            .into_iter()
            .rev()
            .find(|c| match &c.author {
                CommentAuthor::Agent {
                    stage: Some(stage),
                    round,
                    ..
                } => flow
                    .stage(stage)
                    .map_or(round.is_none(), |s| s.kind == StageKind::Agent),
                _ => false,
            }))
    }

    /// The "Pull request" section, while a PR flow runs or once the ticket has a PR.
    async fn prime_pr(&self, ticket: &Ticket) -> Result<Option<PrimePr>, BoardError> {
        let flowing = self.pr_flow(&ticket.id).await?.is_some();
        if !flowing && ticket.flags.pr_number.is_none() {
            return Ok(None);
        }
        let repo = self.project(ticket.key.project.as_str()).await?.repo_path;
        let url = crate::pr::provider::origin_url(self.git()?, std::path::Path::new(&repo)).await;
        Ok(Some(PrimePr {
            provider: url
                .as_deref()
                .and_then(crate::pr::provider::Provider::detect)
                .map(|p| p.name().to_owned()),
            remote: url.as_deref().map(crate::pr::provider::display_remote),
            view: self.pr_flow_view(&ticket.key.to_string()).await?,
        }))
    }

    /// The summary of a ticket's latest `stage next` (into a human check or not, Q263).
    pub async fn review_summary(&self, ticket_id: &str) -> Result<Option<String>, BoardError> {
        let payload: Option<String> = sqlx::query_scalar(
            "SELECT payload_json FROM events WHERE ticket_id = ? AND kind IN (?, ?)
             ORDER BY at DESC, id DESC LIMIT 1",
        )
        .bind(ticket_id)
        .bind(crate::board::tickets::EventKind::ReviewSubmitted.as_str())
        .bind(crate::board::tickets::EventKind::StageFinished.as_str())
        .fetch_optional(self.store().pool())
        .await?;
        Ok(payload
            .and_then(|p| serde_json::from_str::<serde_json::Value>(&p).ok())
            .and_then(|p| p.get("summary")?.as_str().map(str::to_owned))
            .filter(|s| !s.trim().is_empty()))
    }
}

/// The commit skill `agent` runs (d0050), by its own name: `commit` (its imported skill of that name, else Otaq's),
/// when it is one of the agent's skills (so its copy is in the worktree); `None` for an agent with no commit skill.
/// With no agent resolved, Otaq's own. "What this agent reads" marks it the same way ([`crate::agent_reads`]).
pub(crate) fn commit_skill(
    sources: &crate::skills::Sources<'_>,
    agent: Option<&Agent>,
) -> Option<String> {
    let commit = crate::board::settings::WorkflowStep::Commit.own_skill();
    let Some(agent) = agent else {
        return Some(commit.to_owned());
    };
    let runs = sources.runs(Some(agent), commit).own;
    sources
        .of_agent(agent)
        .iter()
        .any(|s| s.own == runs)
        .then_some(runs)
}

/// What `otaq prime --validate` prints for a validator (VAL-10, d0063, AGM-06, CMT-07): the ticket, the check's
/// agent's instructions and the project's notes, the earlier stages, the ticket's comments (earlier rounds' findings
/// among them), the user's line comments after a Send back, the linked tickets and the diff against the ticket's base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatePrime {
    pub ticket: Ticket,
    /// The round this validator judges, and the project's rounds.
    pub round: u32,
    pub rounds: u32,
    /// The ticket's earlier stages (AGM-06), oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub earlier: Vec<EarlierStage>,
    /// The ticket's comments (CMT-07), oldest first, deleted ones left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<TicketComment>,
    /// The manual checks listed for the user so far (d0125), oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_checks: Vec<ManualCheck>,
    /// `git diff <base_sha> HEAD` in the worktree: the committed change.
    pub diff: String,
    /// After a Send back: the user's line comments, each to check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<LineReview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<Instructions>,
    /// Tickets this one depends on, with their last comments from an Agent stage.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<Parent>,
    /// Tickets that depend on this one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Linked>,
    /// The epic a child belongs to, with its description and the other children.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic: Option<EpicContext>,
}

/// The user's line comments of the latest Send back (`prime --validate`'s "The user's review").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineReview {
    pub round: u32,
    /// By file then line, oldest first on a line.
    pub comments: Vec<FeedbackComment>,
}

/// A linked ticket: key, title, status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Linked {
    pub key: TicketKey,
    pub title: String,
    pub status: Status,
}

/// The epic around an epic child.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpicContext {
    pub key: TicketKey,
    pub title: String,
    pub description: String,
    pub siblings: Vec<Linked>,
}

impl Board {
    /// The context of a ticket's validator (`otaq prime --validate`).
    pub async fn validate_prime(&self, key: &str) -> Result<ValidatePrime, BoardError> {
        let (base, _) = self
            .unnamed_prime_context(key, SessionRole::Validator)
            .await?;
        let ticket = base.ticket;
        let project = ticket.key.project.to_string();
        let tickets = self.tickets(&project).await?;
        let linked = |t: &Ticket| Linked {
            key: t.key.clone(),
            title: t.title.clone(),
            status: t.status,
        };
        let children = tickets
            .iter()
            .filter(|t| t.depends_on.contains(&ticket.key))
            .map(linked)
            .collect();
        let epic = ticket
            .epic
            .as_ref()
            .and_then(|e| tickets.iter().find(|t| &t.key == e))
            .map(|e| EpicContext {
                key: e.key.clone(),
                title: e.title.clone(),
                description: e.body.clone(),
                siblings: tickets
                    .iter()
                    .filter(|t| t.epic.as_ref() == Some(&e.key) && t.key != ticket.key)
                    .map(linked)
                    .collect(),
            });
        let rounds = self
            .effective_flow(&project)
            .await?
            .check_rounds(ticket.place.stage(), 0);
        let review = match &ticket.flags.sent_back {
            Some(sent) => Some(self.line_review(key, sent.round).await?),
            None => None,
        };
        Ok(ValidatePrime {
            round: ticket.validation_round + 1,
            rounds,
            earlier: base.earlier,
            comments: base.comments,
            manual_checks: base.manual_checks,
            diff: self.committed_diff(&ticket).await?,
            review,
            instructions: base.instructions,
            parents: base.parents,
            children,
            epic,
            ticket,
        })
    }

    /// The line comments the user sent back in `round`, by file then line (a thread on one line keeps its order).
    async fn line_review(&self, key: &str, round: u32) -> Result<LineReview, BoardError> {
        let mut comments: Vec<FeedbackComment> = self
            .comments(key, Some(round))
            .await?
            .comments
            .into_iter()
            .filter(|c| !c.is_pending())
            .map(|c| FeedbackComment {
                file: c.file,
                line: c.line,
                side: c.side,
                body: c.body,
            })
            .collect();
        comments.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        Ok(LineReview { round, comments })
    }

    /// `git diff <base_sha> HEAD` in the ticket's worktree; empty when it has none yet.
    async fn committed_diff(&self, ticket: &Ticket) -> Result<String, BoardError> {
        let Some(work) = &ticket.work else {
            return Ok(String::new());
        };
        let (Some(worktree), Some(base)) = (&work.worktree, &work.base_sha) else {
            return Ok(String::new());
        };
        Ok(self
            .git()?
            .run(std::path::Path::new(worktree), &["diff", base, "HEAD"])
            .await?)
    }
}

/// The Markdown a validator reads.
pub fn render_validate(ctx: &ValidatePrime) -> String {
    let t = &ctx.ticket;
    let cmd = naming::COMMAND_NAME;
    let mut out = format!("# Validate {} · {}\n\n", t.key, t.title);
    out.push_str(&format!(
        "You are the validator for {} ticket {}: a read-only judge of its agent's work (round {} of {}). Never \
         edit files, never commit and never fix anything.\n",
        naming::PRODUCT_NAME,
        t.key,
        ctx.round,
        ctx.rounds
    ));
    out.push_str("\n## What and why\n\n");
    out.push_str(if t.body.trim().is_empty() {
        "(no description)"
    } else {
        t.body.trim()
    });
    out.push('\n');
    if let Some(i) = &ctx.instructions {
        out.push_str(&i.render("Agent instructions"));
    }
    out.push_str(&render_earlier(&ctx.earlier));
    out.push_str(&render_comments(&ctx.ticket.key, &ctx.comments));
    out.push_str(&render_manual_checks(&ctx.manual_checks));
    if let Some(review) = ctx.review.as_ref().filter(|r| !r.comments.is_empty()) {
        out.push_str(&format!(
            "\n## The user's review (Send back, round {})\n\nCheck that each line comment was addressed.\n\n",
            review.round
        ));
        for c in &review.comments {
            out.push_str(&format!("- {}:{}: {}\n", c.file, c.line, c.body.trim()));
        }
    }
    if !ctx.parents.is_empty() || !ctx.children.is_empty() || ctx.epic.is_some() {
        out.push_str(
            "\n## Linked tickets\n\nA gap another of these covers is a note, not a failure.\n",
        );
        for p in &ctx.parents {
            out.push_str(&format!(
                "\n- depends on {} · {} ({})\n",
                p.key,
                p.title,
                p.status.label()
            ));
            if let Some(comment) = &p.last_comment {
                render_last_comment(&mut out, &p.key, comment);
            }
        }
        for c in &ctx.children {
            out.push_str(&format!(
                "- needed by {} · {} ({})\n",
                c.key,
                c.title,
                c.status.label()
            ));
        }
        if let Some(epic) = &ctx.epic {
            out.push_str(&format!("\nEpic {} · {}:\n", epic.key, epic.title));
            if !epic.description.trim().is_empty() {
                out.push_str(epic.description.trim());
                out.push('\n');
            }
            for s in &epic.siblings {
                out.push_str(&format!(
                    "- sibling {} · {} ({})\n",
                    s.key,
                    s.title,
                    s.status.label()
                ));
            }
        }
    }
    out.push_str(&format!(
        "\n## The diff\n\n```diff\n{}\n```\n\n## Your verdict\n\nKeep it short: one or two lines, one line per \
         finding with its file and line; notes start with \"Note:\". Finish with `{cmd} check pass --stdin` or \
         `{cmd} check fail --stdin` and the verdict on stdin. Save the full check run to a file in `${dir}` (never \
         in the worktree) and attach it with `--report-file`: `{cmd} check fail --stdin --report-file \
         \"${dir}/test-run.txt\"`. {manual}\n",
        ctx.diff.trim_end(),
        dir = naming::resource_dir_env_var(),
        manual = MANUAL_CHECK_RULE.replace("{cmd}", cmd).replace("{command}", "check pass|fail"),
    ));
    out
}

/// The "resolve this first" section: what conflicts, and the restack skill.
fn render_conflict(c: &Conflict, restack: &str) -> String {
    let lead = match c.reason {
        ConflictReason::Reopen => format!(
            "Reopened: the rebase onto `{}` conflicted. Resolve it first with the restack skill (`{restack}`).",
            c.target
        ),
        ConflictReason::Merge | ConflictReason::TestMerge => format!(
            "This branch conflicts with the latest `{}`. Rebase it onto `{}` and resolve the conflicts first, \
             with the restack skill (`{restack}`).",
            c.target, c.target
        ),
        ConflictReason::MergeCheck => format!(
            "The project's merge check failed on this branch rebased onto the latest `{}`; nothing was merged. \
             Rebase it onto `{}` with the restack skill (`{restack}`), fix what failed (the latest comment's \
             report has the output), then submit again.",
            c.target, c.target
        ),
    };
    let mut out = format!("\n## Resolve first\n\n{lead}\n");
    if !c.files.is_empty() {
        out.push_str("\nConflicting files:\n");
        for file in &c.files {
            out.push_str(&format!("- {file}\n"));
        }
    }
    out
}

/// The "Pull request" section (REV-10): provider, target, branch, the commits to propose, and what the user
/// confirmed.
fn render_pr(pr: &PrimePr) -> String {
    use crate::pr::flow::{FlowKind, FlowState};
    let v = &pr.view;
    let mut out = String::from("\n## Pull request\n\n");
    let remote = pr.remote.as_deref().unwrap_or("none");
    match &pr.provider {
        Some(provider) => out.push_str(&format!("- Provider: {provider} (origin {remote})\n")),
        None => out.push_str(&format!(
            "- Provider: not detected (origin {remote}): the built-in skills only handle GitHub and Azure DevOps\n"
        )),
    }
    out.push_str(&format!("- Target branch: `{}`\n", v.target));
    if v.branch_locked {
        out.push_str(&format!(
            "- Branch: `{}` (pushed; it can't be renamed)\n",
            v.branch
        ));
    } else {
        out.push_str(&format!(
            "- Branch: `{}` (temporary; propose its real name)\n",
            v.branch
        ));
    }
    if let Some(id) = &v.external_id {
        out.push_str(&format!("- External ID: {id}\n"));
    }
    if let (Some(n), Some(url)) = (v.pr_number, &v.pr_url) {
        out.push_str(&format!("- PR: #{n} {url}\n"));
    }
    // An automatic Delivery's flow (FLW-15): the skill keys on the word `automatic`.
    if v.flow.as_ref().is_some_and(|f| f.automatic) {
        out.push_str(&format!(
            "- Mode: automatic (no dialog): nobody reviews your proposal; {} confirms it at once\n",
            naming::PRODUCT_NAME
        ));
    }
    let kind = v.flow.as_ref().map(|f| f.kind);
    let pushed = v.commits.iter().filter(|c| c.pushed).count();
    if pushed > 0 {
        out.push_str("\nAlready on the PR (leave them as they are):\n");
        for c in v.commits.iter().filter(|c| c.pushed) {
            out.push_str(&format!("- {} {}\n", short(&c.sha), subject(&c.message)));
        }
    }
    let lead = match kind {
        Some(FlowKind::Push) => "New commits to propose messages for, oldest first:",
        _ => "Commits that go into the PR, oldest first (propose one message for each):",
    };
    out.push_str(&format!("\n{lead}\n"));
    for c in v.commits.iter().filter(|c| !c.pushed) {
        out.push_str(&format!("- {} {}\n", short(&c.sha), subject(&c.message)));
    }
    if let Some(flow) = &v.flow {
        match (flow.state, &flow.confirmed) {
            (FlowState::Pushing, Some(confirmed)) => {
                let who = match flow.automatic {
                    true => format!("{} confirmed it automatically.", naming::PRODUCT_NAME),
                    false => "The user confirmed.".to_owned(),
                };
                out.push_str(
                    &format!("\n**{who}** {} renamed the branch and reworded the commits. Push exactly this:\n\n", naming::PRODUCT_NAME),
                );
                out.push_str(&format!("- Branch: `{}`\n", confirmed.branch));
                if !confirmed.title.is_empty() {
                    out.push_str(&format!("- Title: {}\n", confirmed.title));
                }
                if !confirmed.description.is_empty() {
                    out.push_str("- Description:\n\n");
                    for line in confirmed.description.lines() {
                        out.push_str(&format!("  > {line}\n").replace("  > \n", "  >\n"));
                    }
                }
            }
            (FlowState::Proposed, _) => out.push_str(
                &format!("\nYour proposal is with the user. Wait: nothing is pushed until {} says they confirmed.\n", naming::PRODUCT_NAME),
            ),
            _ => out.push_str(&format!(
                "\nPropose with `{} pr propose --stdin`; don't push yet.\n",
                naming::COMMAND_NAME
            )),
        }
    }
    out
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

fn subject(message: &str) -> &str {
    message.lines().next().unwrap_or_default()
}

/// `> ` before each line of a quoted comment, indented by `indent`.
fn quote(out: &mut String, indent: &str, text: &str) {
    for line in text.trim().lines() {
        let line = line.trim_end();
        match line.is_empty() {
            true => out.push_str(&format!("{indent}>\n")),
            false => out.push_str(&format!("{indent}> {line}\n")),
        }
    }
}

/// The "## Earlier stages" section (AGM-06, CMT-07): one line per stage's latest run, oldest first (what they said
/// is in the comments); nothing when the ticket ran no other stage.
pub fn render_earlier(earlier: &[EarlierStage]) -> String {
    if earlier.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n## Earlier stages\n\n");
    for run in earlier {
        out.push_str(&format!("- {}\n", run.label()));
    }
    out
}

/// The heading of the user's reports in `prime` (RPT-01): the shipped skills tell agents to open them first.
pub const REPORTS_FROM_YOU: &str = "Reports from you \u{b7} read these before you start";

/// The "## Reports from you" section (RPT-01, d0109): each report the user attached, oldest first, with its name,
/// size and `otaq report KEY N`, and the first line of the comment it came with; nothing when the user attached none.
/// Never a report's text.
pub fn render_reports_from_you(key: &TicketKey, comments: &[TicketComment]) -> String {
    let mine: Vec<&TicketComment> = comments
        .iter()
        .filter(|c| c.report.is_some() && matches!(c.author, CommentAuthor::User { .. }))
        .collect();
    if mine.is_empty() {
        return String::new();
    }
    let mut out = format!("\n## {REPORTS_FROM_YOU}\n\n");
    for c in mine {
        let (Some(report), Some(name)) = (c.report, c.report_label()) else {
            continue;
        };
        let line = c
            .body
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or_default();
        out.push_str(&format!(
            "- {name} \u{b7} {}, {} line{} \u{b7} `{} report {key} {}`\n  With comment {} ({}): {line}\n",
            crate::board::reports::size_label(report.bytes),
            crate::board::reports::count_label(report.lines),
            if report.lines == 1 { "" } else { "s" },
            naming::COMMAND_NAME,
            c.n,
            c.n,
            c.heading(),
        ));
    }
    out
}

/// The heading of `prime`'s list of manual checks (d0125).
pub const MANUAL_CHECKS_HEADING: &str = "Manual checks for the user";

/// What every stage-ending agent is told about manual checks (d0125); `{cmd}` and `{command}` are filled in.
const MANUAL_CHECK_RULE: &str = "What you couldn't check yourself (a real browser, a device, a service you can't \
    reach), list for the user: `--manual-check \"<what to check>\"` on `{cmd} {command}`, one line per check. The \
    user tries them at review. Never leave a check to \"the PR\": there may be no PR.";

/// The "## Manual checks for the user" section (d0125): each item, ticked or not, with who listed it in which
/// stage; nothing when there are none.
pub fn render_manual_checks(items: &[ManualCheck]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "\n## {MANUAL_CHECKS_HEADING}\n\nWhat agents couldn't check themselves; the user tries them at review. \
         Ending your stage replaces your stage's unticked ones with what you list then: list again the ones that \
         still apply.\n\n"
    );
    for item in items {
        let (mark, ticked) = match item.ticked() {
            true => ("x", " \u{b7} ticked by the user"),
            false => (" ", ""),
        };
        out.push_str(&format!(
            "- [{mark}] {} \u{b7} {}{ticked}\n",
            item.text,
            item.source()
        ));
    }
    out
}

/// The "## Comments" section (CMT-07): every comment in full, oldest first, under its heading ("Validator · Validate
/// · round 2 · 3:05 pm"), with one line naming its report; nothing when the ticket has none. The user's reports are
/// listed first ([`render_reports_from_you`], RPT-01). Reports are never printed.
pub fn render_comments(key: &TicketKey, comments: &[TicketComment]) -> String {
    if comments.is_empty() {
        return String::new();
    }
    let mut out = render_reports_from_you(key, comments);
    out.push_str("\n## Comments\n");
    for c in comments {
        out.push_str(&format!("\n### {}\n\n{}\n", c.heading(), c.body.trim()));
        if let Some(line) = c.report_line(key) {
            out.push_str(&format!("\n{line}\n"));
        }
    }
    out
}

/// A parent's last comment from an Agent stage under its line in "Depends on": its heading, the text quoted and its
/// report's line.
fn render_last_comment(out: &mut String, key: &TicketKey, comment: &TicketComment) {
    out.push_str(&format!("  Last comment, {}:\n", comment.heading()));
    quote(out, "  ", &comment.body);
    if let Some(line) = comment.report_line(key) {
        out.push_str(&format!("  {line}\n"));
    }
}

/// The Markdown an agent reads.
pub fn render(ctx: &PrimeContext) -> String {
    let t = &ctx.ticket;
    let cmd = naming::COMMAND_NAME;
    let mut out = format!("# {} · {}\n\n", t.key, t.title);
    if !ctx.unread_notes.is_empty() {
        out.push_str(&format!(
            "## Notes from your epic's lead\n\n{}\n\nApply them before you go on.\n\n",
            crate::epic_lead::notes::unread_message(&ctx.unread_notes)
        ));
    }
    let base = match &t.base {
        Base::Branch(b) => format!("`{b}`"),
        Base::Epic(e) => format!("the epic's branch ({e})"),
    };
    let branch = t
        .work
        .as_ref()
        .and_then(|w| w.branch.clone())
        .unwrap_or_else(|| naming::branch(&t.key.to_string()));
    out.push_str(&format!(
        "You are the agent for {} ticket {}, working in its own worktree on branch `{branch}`. It merges into \
         {base} once the user approves it.\n",
        naming::PRODUCT_NAME,
        t.key
    ));
    if let Some((key, title)) = &ctx.epic {
        out.push_str(&format!("It is part of the epic {key} · {title}.\n"));
    }
    out.push_str(&format!("Priority: {}.\n", t.priority));
    if let Some(conflict) = &ctx.conflict {
        out.push_str(&render_conflict(conflict, &ctx.restack_skill));
    }

    out.push_str("\n## What and why\n\n");
    if t.body.trim().is_empty() {
        out.push_str("(no description)\n");
    } else {
        out.push_str(t.body.trim());
        out.push('\n');
    }

    // What the agent is told (AGM-06): its instructions, the project's notes, its stage, the earlier stages.
    if let Some(instructions) = &ctx.instructions {
        out.push_str(&instructions.render("Agent instructions"));
    }
    if let Some(stage) = &ctx.stage {
        out.push_str(&format!("\n## Your stage\n\n{}", stage.name));
        if let Some(agent) = &stage.agent {
            out.push_str(&format!(", as {agent}"));
        }
        if let Some(skill) = &stage.skill {
            out.push_str(&format!(", with the skill `{skill}`"));
        }
        out.push_str(".\n");
    }
    out.push_str(&render_earlier(&ctx.earlier));
    out.push_str(&render_comments(&t.key, &ctx.comments));
    out.push_str(&render_manual_checks(&ctx.manual_checks));

    if !ctx.parents.is_empty() {
        out.push_str("\n## Depends on\n\n");
        for p in &ctx.parents {
            out.push_str(&format!(
                "- {} · {} ({})\n",
                p.key,
                p.title,
                p.status.label()
            ));
            match &p.last_comment {
                Some(comment) => render_last_comment(&mut out, &p.key, comment),
                None => out.push_str("  (no comment yet)\n"),
            }
        }
    }

    if !ctx.pending_questions.is_empty() {
        out.push_str("\n## Pending questions\n\n");
        for question in &ctx.pending_questions {
            out.push_str(&format!("- {question}\n"));
        }
    }

    if !ctx.decisions.is_empty() {
        let lead = ctx
            .epic
            .as_ref()
            .map(|(_, title)| crate::project_agents::lead::lead_name(title));
        out.push_str(
            "\n## Decisions you recorded\n\nThe user sees these in the review; don't record them again.\n\n",
        );
        for decision in &ctx.decisions {
            out.push_str(&format!(
                "- {}",
                crate::decisions::render(decision, "  ", lead.as_deref())
            ));
        }
    }

    if let Some(pr) = &ctx.pull_request {
        out.push_str(&render_pr(pr));
    }

    out.push_str(&render_choices(
        &ctx.choices,
        AgentReader::TicketAgent,
        Some(&t.key),
    ));

    out.push_str(&format!("\n## {}'s rules\n\n", naming::PRODUCT_NAME));
    let commit = match ctx.commit_skill.as_str() {
        "" => "Commit everything with git (`git add -A`, `git commit`)".to_owned(),
        skill => format!("Commit everything with `{skill}`"),
    };
    let rules = [
        "Follow the Agent instructions and the project's notes above. They add to these rules; they can't skip \
         getting context or ending your stage."
            .to_owned(),
        format!(
            "{commit} before you end your stage. Never push and never open a PR: the user does that."
        ),
        format!(
            "When your stage's work is done, as the ticket's description describes, end it: `{cmd} stage next \
             --stdin` with a short summary of what you did on stdin; the next stage reads it. It is refused while \
             files are uncommitted."
        ),
        MANUAL_CHECK_RULE
            .replace("{cmd}", cmd)
            .replace("{command}", "stage next"),
        format!(
            "Only when there is nothing to deliver (no commits on the ticket's branch, for example it was already \
             fixed elsewhere), end the ticket instead: `{cmd} stage done --stdin` with the reason on stdin. It is \
             refused once the branch has commits."
        ),
        format!(
            "When you are stuck or a decision is the user's, ask: `{cmd} ask human --stdin` with the question on \
             stdin, then end your turn."
        ),
        "Long text (summaries, questions, ticket bodies) always goes through stdin (`--stdin`), never quoted \
         arguments."
            .to_owned(),
        format!(
            "Long output goes to a file in `${}`; attach it with `--report-file`.",
            naming::resource_dir_env_var()
        ),
        format!(
            "`{cmd} show <KEY>` reads any ticket; `{cmd} create \"Title\" --stdin` files a follow-up in Backlog."
        ),
    ];
    for (n, rule) in rules.iter().enumerate() {
        out.push_str(&format!("{}. {rule}\n", n + 1));
    }
    out
}

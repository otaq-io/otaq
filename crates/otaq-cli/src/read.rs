//! The read commands (SDD §11.2, d0032, d0039): `show` (alias `view`), `list`, `ready`, `blocked`,
//! `project list | show`. Agents may read anything (CLI-02).
//!
//! # `--json` shapes (stable; fields may be added, never renamed or removed)
//!
//! - `show`: `{"ticket": Ticket, "agents": [StageAgent], "choices": [Choice], "decisions": [Decision],
//!   "history": [HistoryEvent]?, "messages": [Message]?}`: `agents` are the agent each of the ticket's stages runs
//!   ([`StageAgentJson`], AGM-05); `choices` its stages with options and its choice for each ([`ChoiceJson`],
//!   AGM-11); `decisions` the ticket's recorded decisions (DEC-02, `otaq_core::decisions`), `history` and
//!   `messages` (questions between linked tickets) with `--long`.
//!   `Ticket` carries the Plan (`plan`, `plan_by`, `plan_at`; d0133).
//! - `show --piece N` (d0133): `{"key", "name", "number", "title", "waits_for", "text"}`: piece N of the ticket's
//!   Plan (`name` "40.3").
//! - `show --terminal` (SHL-10, d0087): `{"key", "session", "text"}`: the ticket's terminal as the user sees it,
//!   as plain text, and the session it came from.
//! - `list`: `[Ticket]` in key order.
//! - `ready` / `blocked`: `[{"key", "title", "status", "ready", "waiting_for": [{"key", "status",
//!   "pr_number"?}]?, "parent_closed"?, "reason"?}]`; `reason` is the card's waiting text (T-0077).
//! - `project list`: `[ProjectInfo]`; `project show`: `ProjectInfo`.
//!
//! `Ticket`, `HistoryEvent` and `ProjectInfo` are the protocol's types (`otaq_core::board`).

use otaq_core::agents::facts::TicketAgents;
use otaq_core::board::readiness::Readiness;
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::board::stage_choices::{SetBy, TicketChoices};
use otaq_core::board::tickets::{HistoryEvent, Ticket};
use otaq_core::client::Client;
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Place, Priority, Status, StatusSet, TicketKey};
use otaq_core::protocol::{
    ErrorBody, ErrorKind, KeyParams, ProjectParams, Reply, Request, TerminalParams,
    TicketAgentsParams, TicketChoicesParams,
};
use otaq_core::store::now_ms;
use serde::Serialize;
use serde_json::json;

use crate::cli::{GlobalArgs, ProjectCommand};
use crate::output::Output;
use crate::service::{self, client_error, internal};
use crate::target;

pub struct ShowArgs {
    pub key: Option<String>,
    pub long: bool,
}

pub struct ListArgs {
    pub status: Option<String>,
    pub priority: Option<Priority>,
}

/// Runs one read command on a fresh connection.
async fn with_client<F, T>(global: &GlobalArgs, run: F) -> Result<T, ErrorBody>
where
    F: AsyncFnOnce(&Client, &DataDir) -> Result<T, ErrorBody>,
{
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    let client = service::connect(&data, global).await?;
    run(&client, &data).await
}

pub fn show(args: ShowArgs, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    service::runtime()?.block_on(with_client(global, async |client, data| {
        let key = target::resolve_key(client, data, args.key, global).await?;
        let ticket = get_ticket(client, &key.to_string()).await?;
        let tickets = list_tickets(client, ticket.key.project.as_str()).await?;
        let history = if args.long {
            match request(
                client,
                Request::TicketHistory(KeyParams {
                    key: ticket.key.to_string(),
                }),
            )
            .await?
            {
                Reply::History(h) => Some(h),
                other => return Err(unexpected(&other)),
            }
        } else {
            None
        };
        let messages = if args.long {
            match request(
                client,
                Request::TicketMessages(KeyParams {
                    key: ticket.key.to_string(),
                }),
            )
            .await?
            {
                Reply::Messages(m) => Some(m),
                other => return Err(unexpected(&other)),
            }
        } else {
            None
        };
        let decisions = match request(
            client,
            Request::DecisionList(KeyParams {
                key: ticket.key.to_string(),
            }),
        )
        .await?
        {
            Reply::Decisions(d) => d,
            other => return Err(unexpected(&other)),
        };
        let agents = ticket_agents(client, &ticket.key.to_string()).await?;
        let choices = ticket_choices(client, &ticket.key.to_string()).await?;
        let mut human = show_text(&ticket, &tickets, Some(&agents), Some(&choices));
        if !decisions.is_empty() {
            human.push_str(&decisions_text(&decisions));
        }
        if let Some(messages) = messages.as_ref().filter(|m| !m.is_empty()) {
            human.push_str(&messages_text(messages));
        }
        if let Some(history) = &history {
            human.push_str(&history_text(history, now_ms()));
        }
        let mut value = json!({
            "ticket": ticket,
            "agents": agents_json(&agents),
            "choices": choices_json(&choices),
            "decisions": decisions,
        });
        if let Some(history) = history {
            value["history"] = json!(history);
        }
        if let Some(messages) = messages {
            value["messages"] = json!(messages);
        }
        out.print(human.trim_end(), &value, Some(&ticket.key.to_string()));
        Ok(())
    }))
}

/// `otaq show <KEY> --piece <N>` (d0133 audit answer 2): one piece of the ticket's Plan, found as a piece of its
/// ticket ("40.3 · Wishlist page"), with what it waits for and its text.
pub fn show_piece(
    key: Option<String>,
    number: u32,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    use otaq_core::domain::plan::{piece, piece_name};
    service::runtime()?.block_on(with_client(global, async |client, data| {
        let key = target::resolve_key(client, data, key, global).await?;
        let ticket = get_ticket(client, &key.to_string()).await?;
        let Some(p) = piece(&ticket.plan, number) else {
            return Err(ErrorBody::new(
                ErrorKind::NotFound,
                format!("{}'s plan has no piece {number}", ticket.key),
            ));
        };
        let name = piece_name(&ticket.key, p.number);
        let mut human = format!("{name} · {}\n", p.title);
        if !p.waits_for.is_empty() {
            let waits: Vec<String> = p
                .waits_for
                .iter()
                .map(|n| piece_name(&ticket.key, *n))
                .collect();
            human.push_str(&format!("Waits for: {}\n", waits.join(", ")));
        }
        if !p.text.is_empty() {
            human.push_str(&format!("\n{}\n", p.text));
        }
        let value = json!({
            "key": ticket.key,
            "name": name,
            "number": p.number,
            "title": p.title,
            "waits_for": p.waits_for,
            "text": p.text,
        });
        out.print(human.trim_end(), &value, Some(&name));
        Ok(())
    }))
}

/// `otaq show <KEY> --terminal` (SHL-10, CLI-09): the ticket's agent terminal (or with `--stage` the terminal of the
/// agent running that stage, FLW-20; with `--agent` one check agent's of several, d0130) as the user sees it, as plain
/// text. Read-only: the service decides who may
/// (CLI-03).
pub fn terminal(
    key: Option<String>,
    stage: Option<String>,
    agent: Option<String>,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    service::runtime()?.block_on(with_client(global, async |client, data| {
        let key = target::resolve_key(client, data, key, global).await?;
        let reply = request(
            client,
            Request::TicketTerminal(TerminalParams {
                key: key.to_string(),
                stage,
                agent,
            }),
        )
        .await?;
        let Reply::TicketTerminal(terminal) = reply else {
            return Err(unexpected(&reply));
        };
        let value =
            json!({ "key": terminal.key, "session": terminal.session, "text": terminal.text });
        out.print(&terminal.text, &value, Some(&terminal.key.to_string()));
        Ok(())
    }))
}

/// The ticket's recorded decisions (DEC-02): ID and question, then Built, the other options, Why and the outcome.
fn decisions_text(decisions: &[otaq_core::decisions::Decision]) -> String {
    let mut out = String::from("\n\nDecisions\n");
    for d in decisions {
        out.push_str(&format!("  {}", otaq_core::decisions::render(d, "    ")));
    }
    out
}

/// The questions between this ticket and its linked tickets (MSG-01), oldest first, each with its answer.
fn messages_text(messages: &[otaq_core::messages::Message]) -> String {
    let mut out = String::from("\n\nMessages\n");
    for m in messages {
        out.push_str(&format!(
            "  {} {} \u{2192} {}: {}\n",
            m.id,
            m.asker(),
            m.to,
            m.question
        ));
        match &m.answer {
            Some(answer) => out.push_str(&format!("    answered: {answer}\n")),
            None => out.push_str("    waiting for the answer\n"),
        }
    }
    out
}

pub fn list(args: ListArgs, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    service::runtime()?.block_on(with_client(global, async |client, data| {
        let project = target::require_project(client, data, global).await?;
        let tickets = list_tickets(client, project.as_str()).await?;
        let status = args
            .status
            .as_deref()
            .map(StatusFilter::parse)
            .transpose()?;
        let shown: Vec<Ticket> = tickets
            .into_iter()
            .filter(|t| status.as_ref().is_none_or(|s| s.matches(t)))
            .filter(|t| args.priority.is_none_or(|p| t.priority == p))
            .collect();
        let human = shown.iter().map(line).collect::<Vec<_>>().join("\n");
        let keys: Vec<String> = shown.iter().map(|t| t.key.to_string()).collect();
        out.print(
            &human,
            &shown,
            Some(keys.join("\n").as_str()).filter(|k| !k.is_empty()),
        );
        Ok(())
    }))
}

/// `ready` (`blocked == false`) or `blocked`.
pub fn readiness(blocked: bool, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    service::runtime()?.block_on(with_client(global, async |client, data| {
        let project = target::require_project(client, data, global).await?;
        let states = match request(
            client,
            Request::DepReadiness(ProjectParams {
                project: project.to_string(),
            }),
        )
        .await?
        {
            Reply::Readiness(states) => states,
            other => return Err(unexpected(&other)),
        };
        let shown: Vec<Readiness> = states.into_iter().filter(|s| s.ready != blocked).collect();
        let human = shown
            .iter()
            .map(readiness_line)
            .collect::<Vec<_>>()
            .join("\n");
        let keys: Vec<String> = shown.iter().map(|s| s.key.to_string()).collect();
        out.print(
            &human,
            &shown,
            Some(keys.join("\n").as_str()).filter(|k| !k.is_empty()),
        );
        Ok(())
    }))
}

pub fn project(
    command: ProjectCommand,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    service::runtime()?.block_on(with_client(global, async |client, data| {
        let projects = match request(client, Request::ProjectList).await? {
            Reply::Projects(p) => p,
            other => return Err(unexpected(&other)),
        };
        match command {
            ProjectCommand::List => {
                let human = projects
                    .iter()
                    .map(project_line)
                    .collect::<Vec<_>>()
                    .join("\n");
                let keys: Vec<&str> = projects.iter().map(|p| p.key.as_str()).collect();
                out.print(
                    &human,
                    &projects,
                    Some(keys.join("\n").as_str()).filter(|k| !k.is_empty()),
                );
            }
            ProjectCommand::Show { key } => {
                let key = match key {
                    Some(key) => key.trim().to_ascii_uppercase(),
                    None => target::require_project(client, data, global)
                        .await?
                        .to_string(),
                };
                let project = projects.into_iter().find(|p| p.key == key).ok_or_else(|| {
                    ErrorBody::new(ErrorKind::NotFound, format!("unknown project {key}"))
                })?;
                out.print(&project_text(&project), &project, Some(&project.key));
            }
        }
        Ok(())
    }))
}

async fn request(client: &Client, request: Request) -> Result<Reply, ErrorBody> {
    client.request(request).await.map_err(client_error)
}

/// The agents a ticket's stages run (`ticket.agents`).
pub async fn ticket_agents(client: &Client, key: &str) -> Result<TicketAgents, ErrorBody> {
    match request(
        client,
        Request::TicketAgents(TicketAgentsParams {
            key: Some(key.to_owned()),
            project: None,
        }),
    )
    .await?
    {
        Reply::TicketAgents(a) => Ok(*a),
        other => Err(unexpected(&other)),
    }
}

/// One stage's agent in `--json` (AGM-05): `{"stage": "Dev", "agent": "Developer"}`. `agent` is missing when the stage names one that isn't there.
#[derive(Debug, Serialize)]
pub struct StageAgentJson {
    pub stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// The stages' agents as `--json` carries them.
pub fn agents_json(agents: &TicketAgents) -> Vec<StageAgentJson> {
    agents
        .stages
        .iter()
        .map(|s| StageAgentJson {
            stage: s.stage.clone(),
            agent: s.agent.as_ref().map(|a| a.name.clone()),
        })
        .collect()
}

/// The ticket's stages with options and its choices (`ticket.choices`, AGM-08).
pub async fn ticket_choices(client: &Client, key: &str) -> Result<TicketChoices, ErrorBody> {
    match request(
        client,
        Request::TicketChoices(TicketChoicesParams {
            key: Some(key.to_owned()),
            project: None,
        }),
    )
    .await?
    {
        Reply::TicketChoices(c) => Ok(*c),
        other => Err(unexpected(&other)),
    }
}

/// One stage's choice in `--json` (AGM-11): `{"stage": "Dev", "choice": "Quick developer", "set_by":
/// "project_agent", "by": "Noyan", "reason": "typo fix", "locked": false, "options": ["Rust developer",
/// "Quick developer"]}`. Every stage with options is listed; `set_by`, `by` and `reason` are missing when nobody
/// chose (the usual one runs).
#[derive(Debug, Serialize)]
pub struct ChoiceJson {
    pub stage: String,
    pub choice: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub locked: bool,
    pub options: Vec<String>,
}

/// The stages' choices as `--json` carries them.
pub fn choices_json(choices: &TicketChoices) -> Vec<ChoiceJson> {
    choices
        .stages
        .iter()
        .map(|stage| {
            let set_by = stage.choice.as_ref().map(|c| &c.set_by);
            ChoiceJson {
                stage: stage.stage.clone(),
                choice: stage.label(),
                set_by: set_by.map(|s| match s {
                    SetBy::User => "user",
                    SetBy::ProjectAgent { .. } => "project_agent",
                    SetBy::Agent { .. } => "agent",
                }),
                by: set_by.map(SetBy::who),
                reason: set_by.and_then(SetBy::reason).map(str::to_owned),
                locked: stage.locked,
                options: stage.words(),
            }
        })
        .collect()
}

/// The choices someone made, one line each (AGM-11): "Choices:     Dev: Quick developer · set by Noyan · typo
/// fix". Nothing when nobody chose.
pub fn choices_text(choices: &TicketChoices) -> String {
    let mut text = String::new();
    for stage in choices.stages.iter().filter(|s| s.choice.is_some()) {
        let label = if text.is_empty() { "Choices:" } else { "" };
        let set_by = stage.choice.as_ref().map(|c| &c.set_by);
        let mut line = format!("{}: {}", stage.stage, stage.label());
        if let Some(set_by) = set_by {
            line.push_str(&format!(" · set by {}", set_by.who()));
            if let Some(reason) = set_by.reason().filter(|r| !r.is_empty()) {
                line.push_str(&format!(" · {reason}"));
            }
        }
        text.push_str(&format!("{label:<12} {line}\n"));
    }
    text
}

pub async fn get_ticket(client: &Client, key: &str) -> Result<Ticket, ErrorBody> {
    match request(
        client,
        Request::TicketGet(KeyParams {
            key: key.to_owned(),
        }),
    )
    .await?
    {
        Reply::Ticket(t) => Ok(*t),
        other => Err(unexpected(&other)),
    }
}

pub async fn list_tickets(client: &Client, project: &str) -> Result<Vec<Ticket>, ErrorBody> {
    match request(
        client,
        Request::TicketList(ProjectParams {
            project: project.to_owned(),
        }),
    )
    .await?
    {
        Reply::Tickets(t) => Ok(t),
        other => Err(unexpected(&other)),
    }
}

pub fn unexpected(reply: &Reply) -> ErrorBody {
    internal(format!(
        "unexpected answer from the background service: {reply:?}"
    ))
}

/// The board's columns as the default flow draws them, by the name `--status` and `otaq show` use (`dev`,
/// "Ready for Review"); a ticket's column comes from its place (STG-08): Backlog, Ready for Dev and Done as
/// themselves, a stage by the kind of statuses it allows (an agent check's are Dev's), and a card waiting before a
/// later Agent stage in that stage's column, Dev.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Backlog,
    ReadyForDev,
    Dev,
    ReadyForReview,
    Approved,
    Done,
}

impl Column {
    const ALL: [Self; 6] = [
        Self::Backlog,
        Self::ReadyForDev,
        Self::Dev,
        Self::ReadyForReview,
        Self::Approved,
        Self::Done,
    ];

    fn of(ticket: &Ticket) -> Self {
        match &ticket.place {
            Place::Backlog => Self::Backlog,
            Place::Waiting(None) => Self::ReadyForDev,
            Place::Waiting(Some(_)) => Self::Dev,
            Place::Done => Self::Done,
            Place::InStage(_) => match ticket.status.set() {
                StatusSet::HumanCheck => Self::ReadyForReview,
                StatusSet::Delivery => Self::Approved,
                _ => Self::Dev,
            },
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::ReadyForDev => "ready_for_dev",
            Self::Dev => "dev",
            Self::ReadyForReview => "ready_for_review",
            Self::Approved => "approved",
            Self::Done => "done",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Backlog => "Backlog",
            Self::ReadyForDev => "Ready for Dev",
            Self::Dev => "Dev",
            Self::ReadyForReview => "Ready for Review",
            Self::Approved => "Approved",
            Self::Done => "Done",
        }
    }
}

/// `--status`: a status (`working`, `asks_you`, "Ready for Review") or a column (`dev`, `done`).
enum StatusFilter {
    Status(Status),
    Column(Column),
}

impl StatusFilter {
    fn parse(text: &str) -> Result<Self, ErrorBody> {
        let normal = text.trim().to_ascii_lowercase().replace([' ', '-'], "_");
        if let Ok(status) = normal.parse::<Status>() {
            return Ok(Self::Status(status));
        }
        if let Some(column) = Column::ALL.into_iter().find(|c| c.id() == normal) {
            return Ok(Self::Column(column));
        }
        Err(ErrorBody::new(
            ErrorKind::Usage,
            format!(
                "unknown status {text:?}; use a status such as working or a column such as dev"
            ),
        ))
    }

    fn matches(&self, ticket: &Ticket) -> bool {
        match self {
            Self::Status(s) => ticket.status == *s,
            Self::Column(c) => Column::of(ticket) == *c,
        }
    }
}

/// `SHOP-18  Working               Split user full name`
pub fn line(ticket: &Ticket) -> String {
    format!(
        "{:<10} P{} {:<22} {}",
        ticket.key.to_string(),
        ticket.priority.number(),
        ticket.status.label(),
        ticket.title
    )
}

fn status_of(key: &TicketKey, tickets: &[Ticket]) -> String {
    tickets
        .iter()
        .find(|t| &t.key == key)
        .map_or_else(|| "?".to_owned(), |t| t.status.label().to_owned())
}

/// The full human view of one ticket.
pub fn show_text(
    ticket: &Ticket,
    tickets: &[Ticket],
    agents: Option<&TicketAgents>,
    choices: Option<&TicketChoices>,
) -> String {
    let mut text = format!("{} · {}\n", ticket.key, ticket.title);
    let (status, column) = (ticket.status.label(), Column::of(ticket).label());
    // A status named like its column ("Ready for Dev") says it once.
    let column = if status == column {
        String::new()
    } else {
        format!(" ({column})")
    };
    text.push_str(&format!("Status:      {status}{column}\n"));
    if let Some(signal) = ticket.signal.as_ref().and_then(|s| s.text.as_deref()) {
        text.push_str(&format!("Waiting on:  {signal}\n"));
    }
    text.push_str(&format!("Priority:    {}\n", ticket.priority));
    text.push_str(&format!("Base:        {}\n", ticket.base.branch()));
    if !ticket.depends_on.is_empty() {
        let parents: Vec<String> = ticket
            .depends_on
            .iter()
            .map(|p| format!("{p} ({})", status_of(p, tickets)))
            .collect();
        text.push_str(&format!("Depends on:  {}\n", parents.join(", ")));
    }
    if let Some(id) = &ticket.external_id {
        text.push_str(&format!("External ID: {id}\n"));
    }
    if let Some(branch) = ticket.work.as_ref().and_then(|w| w.branch.as_deref()) {
        text.push_str(&format!("Branch:      {branch}\n"));
    }
    let origin = if ticket.created_by.is_agent() {
        " (created by agent)"
    } else {
        ""
    };
    text.push_str(&format!("Created by:  {}{origin}\n", ticket.created_by));
    for stage in agents.map(agents_json).unwrap_or_default() {
        let label = format!("{}:", stage.stage);
        let agent = stage.agent.as_deref().unwrap_or("not one of your agents");
        text.push_str(&format!("{label:<12} {agent}\n"));
    }
    if let Some(choices) = choices {
        text.push_str(&choices_text(choices));
    }
    if !ticket.body.trim().is_empty() {
        text.push_str(&format!("\nWhat and why\n{}\n", ticket.body.trim_end()));
    }
    if !ticket.plan.trim().is_empty() {
        let by = ticket
            .plan_by
            .as_ref()
            .map(|by| format!(" (written by {})", by.label()))
            .unwrap_or_default();
        text.push_str(&format!("\nPlan{by}\n{}\n", ticket.plan.trim_end()));
    }
    text
}

fn history_text(history: &[HistoryEvent], now: i64) -> String {
    let mut text = String::from("\nHistory (newest first)\n");
    for event in history {
        text.push_str(&format!(
            "{:<20} {:<16} {}\n",
            event.kind.as_str(),
            event.actor.to_string(),
            ago(now - event.at)
        ));
    }
    text
}

/// "just now", "4 min ago", "3 h ago", "2 d ago".
pub fn ago(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    match s {
        0..60 => "just now".to_owned(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86_400 => format!("{} h ago", s / 3600),
        _ => format!("{} d ago", s / 86_400),
    }
}

fn readiness_line(state: &Readiness) -> String {
    let base = format!(
        "{:<10} {:<22} {}",
        state.key.to_string(),
        state.status.label(),
        state.title
    );
    match &state.reason {
        Some(reason) => format!("{base} · {reason}"),
        None => base,
    }
}

fn project_line(project: &ProjectInfo) -> String {
    format!(
        "{:<6} {:<20} {:<12} {}",
        project.key, project.name, project.base_branch, project.repo_path
    )
}

fn project_text(project: &ProjectInfo) -> String {
    format!(
        "{} · {}\nRepository:  {}\nBase branch: {}\nAgents:      {} working, {} need you",
        project.key,
        project.name,
        project.repo_path,
        project.base_branch,
        project.agents_working,
        project.needs_you
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages() {
        assert_eq!(ago(10_000), "just now");
        assert_eq!(ago(4 * 60_000), "4 min ago");
        assert_eq!(ago(3 * 3_600_000), "3 h ago");
        assert_eq!(ago(50 * 3_600_000), "2 d ago");
    }

    #[test]
    fn status_filters() {
        assert!(matches!(
            StatusFilter::parse("working"),
            Ok(StatusFilter::Status(Status::Working))
        ));
        assert!(matches!(
            StatusFilter::parse("Ready for Review"),
            Ok(StatusFilter::Status(Status::WaitingForYou))
        ));
        assert!(matches!(
            StatusFilter::parse("dev"),
            Ok(StatusFilter::Column(Column::Dev))
        ));
        assert!(matches!(
            StatusFilter::parse("ready_for_dev"),
            Ok(StatusFilter::Column(Column::ReadyForDev))
        ));
        assert!(StatusFilter::parse("paused").is_err());
    }
}

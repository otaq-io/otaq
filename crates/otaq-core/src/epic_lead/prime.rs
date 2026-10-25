//! `otaq prime --epic` (LEAD-03; d0083, d0084, d0040): what an epic's lead reads first, and again at each wake-up.
//! The epic (description, External ID, branch and base, place, queue mode); every child in start order with its
//! status, priority, dependencies, whether it started, who added it, its last stage's summary, its recorded decisions
//! and its worktree and branch; the children's same-file overlaps; the open questions (waiting for the lead, and the
//! user's); the lead's unread notes and its suggestions in every state; the children whose first stop waits for the
//! lead's note (T-0285); the stage choices it may set on never-started children (AGM-06, AGM-08; none on the default
//! flow); the lead's one rule; and the Epic lead role's agent's instructions, then the project's notes (AGM-06).
//! Concise Markdown ([`render`]);
//! `--json` prints the [`EpicPrime`] itself.

use serde::{Deserialize, Serialize};

use crate::board::epic_flight::{EpicPlace, EpicQueueMode, been_to_dev, epic_place};
use crate::board::order::start_order;
use crate::board::questions::{HumanQuestion, no_notes};
use crate::board::tickets::{Base, Ticket};
use crate::board::{Board, BoardError, Refusal};
use crate::decisions::{self, Decision};
use crate::domain::{Actor, Kind, Priority, Status, TicketKey};
use crate::epic_lead::notes::{self, LeadNote};
use crate::epic_lead::overlap::{self, Overlap};
use crate::epic_lead::questions::{self, LeadQuestion};
use crate::epic_lead::stuck::{self, StuckState};
use crate::epic_lead::suggest::{self, Suggestion, SuggestionState};
use crate::naming;
use crate::prime::{AgentReader, Instructions, PrimeChoice, render_choices};
use crate::project_agents::lead::lead_name;

/// Everything `prime --epic` prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpicPrime {
    pub epic: PrimeEpic,
    /// The lead's name: "Wishlist lead".
    pub lead: String,
    /// Every child, in start order: the ones that started by when they started, then the rest as they will start.
    pub children: Vec<PrimeChild>,
    /// Running children that changed the same files so far (LEAD-09).
    pub overlaps: Vec<Overlap>,
    /// The children's questions waiting for the lead (LEAD-06), oldest first.
    pub lead_questions: Vec<LeadQuestion>,
    /// The children's questions the user hasn't answered yet: passed on, handed back, or asked while no lead was
    /// live. Oldest first.
    pub user_questions: Vec<UserQuestion>,
    /// The lead's notes no child has read yet (LEAD-10), oldest first.
    pub unread_notes: Vec<LeadNote>,
    /// The lead's suggestions in every state, dismissed ones too, so it doesn't repeat them (LEAD-08), oldest first.
    pub suggestions: Vec<Suggestion>,
    /// Children whose first stop waits for the lead's one note (LEAD-07), in start order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stuck: Vec<StuckChild>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<Instructions>,
    /// The stage choices it may set on never-started children (AGM-08); none on the default flow. Filled by the
    /// service.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<PrimeChoice>,
    /// "You may change only never-started children of SHOP-40; name a written source for every answer, note and
    /// mark."
    pub rules: String,
}

/// The epic itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimeEpic {
    pub key: TicketKey,
    pub title: String,
    /// The epic's description: its plan.
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// The epic branch the children merge into (`otaq/SHOP-40`).
    pub branch: String,
    /// The branch exists: the first child started (EPC-02).
    pub branch_created: bool,
    /// The branch the epic branch starts from and merges into (`main`).
    pub base: String,
    pub place: EpicPlace,
    pub queue_mode: EpicQueueMode,
}

/// One child of the epic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimeChild {
    pub key: TicketKey,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<TicketKey>,
    /// It has been to Dev. A never-started child is the only kind the lead may change (CLI-07).
    pub started: bool,
    /// Who added it: "you" (the user), "Wishlist lead", "SHOP-40.2" (a ticket's agent) or "Noyan".
    pub added_by: String,
    /// Its last comment from an Agent stage (CMT-07, [`Board::last_agent_comment`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_comment: Option<crate::board::ticket_comments::TicketComment>,
    /// Its recorded decisions (DEC-02), with the lead's mark and the user's outcome.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<Decision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// A child's question that waits for the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserQuestion {
    pub child: TicketKey,
    pub question: HumanQuestion,
}

/// A child whose first stop waits for the lead's note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StuckChild {
    pub child: TicketKey,
    /// "Validation failed 3 times", "Keeps conflicting with `otaq/SHOP-40`", "Idle".
    pub reason: String,
    /// The last validation report, the merge target, or the agent's last line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The lead's one rule: "You may change only never-started children of SHOP-40; name a written source for every
/// answer, note and mark."
pub fn rules_line(epic: &TicketKey) -> String {
    format!(
        "You may change only never-started children of {epic}; name a written source for every answer, note and mark."
    )
}

/// Who added a child, as the lead reads it.
fn added_by(actor: &Actor, lead_agent: Option<&str>, lead: &str) -> String {
    match actor {
        Actor::User(_) => "you".to_owned(),
        Actor::Agent(key) => key.to_string(),
        Actor::ProjectAgent(id) if Some(id.as_str()) == lead_agent => lead.to_owned(),
        Actor::ProjectAgent(_) => "Noyan".to_owned(),
    }
}

/// The context of the epic `epic`'s lead (by key; an old key resolves). Refused for a ticket that isn't an epic.
pub async fn context(board: &Board, epic: &str) -> Result<EpicPrime, BoardError> {
    let epic = board.ticket(epic).await?;
    if epic.kind != Kind::Epic {
        return Err(BoardError::refused(
            Refusal::NotAnEpic,
            format!("{} isn't an epic", epic.key),
        ));
    }
    let project = epic.key.project.to_string();
    let tickets = board.tickets(&project).await?;
    let lead = lead_name(&epic.title);
    let lead_agent: Option<String> =
        sqlx::query_scalar("SELECT id FROM project_agents WHERE epic_id = ?")
            .bind(&epic.id)
            .fetch_optional(board.store().pool())
            .await?;

    let mut children: Vec<&Ticket> = tickets
        .iter()
        .filter(|t| t.epic.as_ref() == Some(&epic.key))
        .collect();
    // Started first, by when they started; then the rest in the order they will start (SCH-01).
    children.sort_by_cached_key(|t| {
        let started = been_to_dev(t);
        (
            !started,
            started.then(|| t.started_at.unwrap_or(t.created_at)),
            (!started).then(|| start_order(t, &tickets)),
        )
    });
    let mut decided = decisions::list_for_epic(board, &epic.key.to_string()).await?;
    let lead_questions = questions::waiting(board, &epic.id).await?;
    let mut prime_children = Vec::with_capacity(children.len());
    let mut user_questions = Vec::new();
    let mut stuck = Vec::new();
    for child in children {
        let started = been_to_dev(child);
        let work = child.work.as_ref().filter(|_| started);
        let decisions = decided
            .iter()
            .position(|g| g.key == child.key)
            .map(|i| decided.remove(i).decisions)
            .unwrap_or_default();
        if let Some(question) = board.open_question(&child.id).await?
            && !lead_questions.iter().any(|q| q.id == question.id)
        {
            user_questions.push(UserQuestion {
                child: child.key.clone(),
                question,
            });
        }
        if let Some(held) = stuck::find(board, &child.id).await?
            && held.state == StuckState::Held
        {
            stuck.push(StuckChild {
                child: child.key.clone(),
                reason: held.first.reason(),
                detail: held.first.detail.clone(),
            });
        }
        prime_children.push(PrimeChild {
            key: child.key.clone(),
            title: child.title.clone(),
            status: child.status,
            priority: child.priority,
            depends_on: child.depends_on.clone(),
            started,
            added_by: added_by(&child.created_by, lead_agent.as_deref(), &lead),
            last_comment: board.last_agent_comment(&child.key.to_string()).await?,
            decisions,
            worktree: work.and_then(|w| w.worktree.clone()),
            branch: work.and_then(|w| w.branch.clone()),
        });
    }
    user_questions.sort_by_key(|q| q.question.asked_at);

    let overlaps = overlap::overlaps(board, &epic.key.to_string()).await?;
    let unread_notes = notes::list_for_epic(board, &epic.key.to_string())
        .await?
        .into_iter()
        .filter(|n| n.read_at.is_none())
        .collect();
    let suggestions = suggest::list_for_epic(board, &epic.key.to_string()).await?;
    // The Epic lead role's agent's instructions, then the project's notes (AGM-06).
    let agent = board
        .store()
        .role_agent(crate::agents::Role::EpicLead)
        .await?;
    let instructions = board.instructions(&project, agent.as_ref()).await?;
    let branch = epic.work.as_ref().and_then(|w| w.branch.clone());
    let base = match &epic.base {
        Base::Branch(b) => b.clone(),
        Base::Epic(e) => e.to_string(),
    };
    Ok(EpicPrime {
        epic: PrimeEpic {
            key: epic.key.clone(),
            title: epic.title.clone(),
            description: epic.body.clone(),
            external_id: epic.external_id.clone(),
            branch_created: branch.is_some(),
            branch: branch.unwrap_or_else(|| naming::branch(&epic.key.to_string())),
            base,
            place: epic_place(&epic, &tickets),
            queue_mode: epic.epic_queue_mode,
        },
        rules: rules_line(&epic.key),
        lead,
        children: prime_children,
        overlaps,
        lead_questions,
        user_questions,
        unread_notes,
        suggestions,
        stuck,
        instructions,
        choices: Vec::new(),
    })
}

fn place_text(place: EpicPlace) -> &'static str {
    match place {
        EpicPlace::Card => "a card in Backlog",
        EpicPlace::Track => {
            "in flight: its children are cards on the board (its lane with Lanes: Epic)"
        }
        EpicPlace::Delivery => "its delivery card (Ready for Review or later)",
    }
}

fn queue_text(mode: EpicQueueMode) -> &'static str {
    match mode {
        EpicQueueMode::None => "not queued",
        EpicQueueMode::Whole => "the whole epic is queued: new children join the queue",
        EpicQueueMode::Single => "one child was queued: new children wait until queued",
    }
}

/// `> ` before each line of a quoted block.
fn quote(out: &mut String, indent: &str, text: &str) {
    for line in text.trim().lines() {
        let line = line.trim_end();
        match line.is_empty() {
            true => out.push_str(&format!("{indent}>\n")),
            false => out.push_str(&format!("{indent}> {line}\n")),
        }
    }
}

/// " Options: 1. A (recommended) 2. B" on the question's line; when an option has a note (ATT-17), one option per
/// line with its note whole under it, so the lead weighs them.
fn options_text(options: &[String], notes: &[Option<String>], recommended: Option<u32>) -> String {
    let label = |n: usize, o: &str| match recommended == Some(n as u32 + 1) {
        true => format!("{}. {o} (recommended)", n + 1),
        false => format!("{}. {o}", n + 1),
    };
    if no_notes(notes) {
        let listed: Vec<String> = options
            .iter()
            .enumerate()
            .map(|(n, o)| label(n, o))
            .collect();
        return format!(" Options: {}", listed.join(" "));
    }
    let listed: Vec<String> = options
        .iter()
        .enumerate()
        .map(|(n, o)| match notes.get(n).cloned().flatten() {
            Some(note) => format!(
                "\n  {}\n     {}",
                label(n, o),
                note.trim().replace('\n', "\n     ")
            ),
            None => format!("\n  {}", label(n, o)),
        })
        .collect();
    format!(" Options:{}", listed.concat())
}

/// The Markdown the lead reads.
pub fn render(ctx: &EpicPrime) -> String {
    let cmd = naming::COMMAND_NAME;
    let e = &ctx.epic;
    let mut out = format!("# Epic lead · {} · {}\n\n", e.key, e.title);
    out.push_str(&format!(
        "You are {}, the {} lead of epic {}. You plan and direct its children through `{cmd}`; you read and run \
         code in their worktrees, but never edit files and never commit.\n",
        ctx.lead,
        naming::PRODUCT_NAME,
        e.key
    ));

    out.push_str("\n## The epic\n\n");
    if let Some(id) = &e.external_id {
        out.push_str(&format!("- External ID: {id}\n"));
    }
    let created = match e.branch_created {
        true => "",
        false => " (created when the first child starts)",
    };
    out.push_str(&format!(
        "- Branch: `{}` from `{}`{created}\n",
        e.branch, e.base
    ));
    out.push_str(&format!("- Place: {}\n", place_text(e.place)));
    out.push_str(&format!("- Queue: {}\n", queue_text(e.queue_mode)));
    out.push_str("\n### Description\n\n");
    match e.description.trim() {
        "" => out.push_str("(no description)\n"),
        text => {
            out.push_str(text);
            out.push('\n');
        }
    }

    out.push_str("\n## Children, in start order\n\n");
    if ctx.children.is_empty() {
        out.push_str("(no children yet)\n");
    }
    for c in &ctx.children {
        let started = match c.started {
            true => "started",
            false => "never started",
        };
        out.push_str(&format!(
            "- {} · {} ({} · {}) · {started} · added by {}\n",
            c.key,
            c.title,
            c.status.label(),
            c.priority,
            c.added_by
        ));
        if !c.depends_on.is_empty() {
            let deps: Vec<String> = c.depends_on.iter().map(ToString::to_string).collect();
            out.push_str(&format!("  depends on {}\n", deps.join(", ")));
        }
        if let Some(worktree) = &c.worktree {
            let branch = c.branch.as_deref().unwrap_or("?");
            out.push_str(&format!("  worktree {worktree} · branch `{branch}`\n"));
        }
        if let Some(comment) = &c.last_comment {
            out.push_str(&format!("  Last comment, {}:\n", comment.heading()));
            quote(&mut out, "  ", &comment.body);
            if let Some(line) = comment.report_line(&c.key) {
                out.push_str(&format!("  {line}\n"));
            }
        }
        if !c.decisions.is_empty() {
            out.push_str("  Recorded decisions:\n");
            for d in &c.decisions {
                out.push_str(&format!(
                    "  - {}",
                    decisions::render(d, "    ", Some(&ctx.lead))
                ));
            }
        }
    }

    out.push_str("\n## Same file\n\n");
    if ctx.overlaps.is_empty() {
        out.push_str("(no running children changed the same file)\n");
    }
    for o in &ctx.overlaps {
        out.push_str(&format!("- {} and {}: {}\n", o.a, o.b, o.files.join(", ")));
    }

    out.push_str("\n## Open questions\n\n");
    if ctx.lead_questions.is_empty() && ctx.user_questions.is_empty() {
        out.push_str("(none)\n");
    }
    if !ctx.lead_questions.is_empty() {
        out.push_str(&format!(
            "Waiting for you: answer with `{cmd} answer <ID> --stdin`, or pass it on with `{cmd} ask human \
             --pass-on <ID> --stdin`.\n\n"
        ));
    }
    for q in &ctx.lead_questions {
        out.push_str(&format!("- {} from {}: {}", q.id, q.child, q.question));
        if !q.options.is_empty() {
            out.push_str(&options_text(&q.options, &q.notes, q.recommended));
        }
        out.push('\n');
    }
    if !ctx.user_questions.is_empty() {
        if !ctx.lead_questions.is_empty() {
            out.push('\n');
        }
        out.push_str("Waiting for the user:\n\n");
    }
    for q in &ctx.user_questions {
        out.push_str(&format!(
            "- {} from {}: {}",
            q.question.id, q.child, q.question.text
        ));
        if !q.question.options.is_empty() {
            out.push_str(&options_text(
                &q.question.options,
                &q.question.notes,
                q.question.recommended,
            ));
        }
        if let Some(via) = &q.question.via_lead {
            // Under the listed options when they carry notes, else on the question's line.
            let gap = if no_notes(&q.question.notes) {
                " "
            } else {
                "\n  "
            };
            out.push_str(&format!("{gap}(passed on by {}: {})", via.lead, via.reason));
        }
        out.push('\n');
    }

    out.push_str("\n## Your notes not read yet\n\n");
    if ctx.unread_notes.is_empty() {
        out.push_str("(none)\n");
    }
    for n in &ctx.unread_notes {
        out.push_str(&format!("- to {}: {}\n", n.child, n.text.trim()));
    }

    out.push_str("\n## Your suggestions\n\n");
    if ctx.suggestions.is_empty() {
        out.push_str("(none)\n");
    }
    for s in &ctx.suggestions {
        let state = match s.state {
            SuggestionState::Open => "waiting for the user".to_owned(),
            SuggestionState::Confirmed => format!("{} by the user", s.kind.done()),
            SuggestionState::Dismissed => {
                "dismissed by the user; don't suggest it again".to_owned()
            }
        };
        out.push_str(&format!(
            "- {} {} {} · {state}: {}\n",
            s.kind.gerund(),
            s.child,
            s.child_title,
            s.reason.trim()
        ));
    }

    if !ctx.stuck.is_empty() {
        out.push_str(&format!(
            "\n## Stuck children\n\nEach waits for one note from you (`{cmd} note <KEY> --stdin`), or goes to the \
             user.\n\n"
        ));
    }
    for s in &ctx.stuck {
        out.push_str(&format!("- {} stopped: {}\n", s.child, s.reason));
        if let Some(detail) = &s.detail {
            quote(&mut out, "  ", detail);
        }
    }

    out.push_str(&render_choices(&ctx.choices, AgentReader::EpicLead, None));

    out.push_str(&format!(
        "\n## {}'s rules\n\n{}\n",
        naming::PRODUCT_NAME,
        ctx.rules
    ));

    if let Some(instructions) = &ctx.instructions {
        out.push_str(&instructions.render("Agent instructions"));
    }
    out
}

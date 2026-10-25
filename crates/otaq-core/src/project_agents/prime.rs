//! `otaq prime --project` (PAG-03, CLI-06; d0068): what a Project agent reads first. Every ticket of the
//! project with its status, priority, dependencies and epic; for each started ticket its worktree and branch (it
//! may read and run code there); the **stage choices** it may set on the tickets it creates (AGM-06, AGM-08; none on
//! the default flow); the rules of what it may change; and what it is told to follow: the Project agent role's
//! agent's **instructions**, then the project's notes (AGM-06; d0068, d0108). Concise Markdown ([`render`]); `--json`
//! prints the [`ProjectPrime`] itself.

use serde::{Deserialize, Serialize};

use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError};
use crate::domain::{Kind, Priority, Status, StatusSet, TicketKey};
use crate::naming;
use crate::policy::BACKLOG_ONLY;
use crate::prime::{AgentReader, Instructions, PrimeChoice, render_choices};

/// Everything `prime --project` prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPrime {
    /// The project's key and name.
    pub project: String,
    pub name: String,
    /// The user's checkout, where the Project agent runs.
    pub repo_path: String,
    /// Every ticket and epic, by key.
    pub tickets: Vec<PrimeTicket>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<Instructions>,
    /// The stage choices it may set on the tickets it creates (AGM-08); none on the default flow. Filled by the
    /// service.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<PrimeChoice>,
}

/// One ticket as `prime --project` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimeTicket {
    pub key: TicketKey,
    pub title: String,
    pub epic: bool,
    pub status: Status,
    pub priority: Priority,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<TicketKey>,
    /// The epic it belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<TicketKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// In Backlog and never started: the Project agent may edit it.
    pub editable: bool,
    /// Started tickets: where their code is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

impl PrimeTicket {
    fn of(t: &Ticket) -> Self {
        let work = t.work.as_ref().filter(|_| t.has_started());
        Self {
            key: t.key.clone(),
            title: t.title.clone(),
            epic: t.kind == Kind::Epic,
            status: t.status,
            priority: t.priority,
            depends_on: t.depends_on.clone(),
            parent: t.epic.clone(),
            external_id: t.external_id.clone(),
            editable: t.board_group() == StatusSet::Backlog && !t.has_started(),
            worktree: work.and_then(|w| w.worktree.clone()),
            branch: work.and_then(|w| w.branch.clone()),
        }
    }
}

impl Board {
    /// The prime context of a project's Project agents.
    pub async fn project_prime(&self, project: &str) -> Result<ProjectPrime, BoardError> {
        let p = self.project(project).await?;
        let tickets = self.tickets(&p.key).await?;
        // The Project agent role's agent's instructions (AGM-02: the Project agent instructions moved there), then
        // the project's notes (AGM-06).
        let agent = self
            .store()
            .role_agent(crate::agents::Role::ProjectAgent)
            .await?;
        let instructions = self.instructions(&p.key, agent.as_ref()).await?;
        Ok(ProjectPrime {
            project: p.key,
            name: p.name,
            repo_path: p.repo_path,
            tickets: tickets.iter().map(PrimeTicket::of).collect(),
            instructions,
            choices: Vec::new(),
        })
    }
}

/// The Markdown a Project agent reads.
pub fn render(ctx: &ProjectPrime) -> String {
    let cmd = naming::COMMAND_NAME;
    let mut out = format!("# Noyan · {} ({})\n\n", ctx.name, ctx.project);
    out.push_str(&format!(
        "You are Noyan, the planning agent of this {} project, in {}, the user's own checkout. You investigate \
         and file tickets; agents build them later in their own worktrees.\n",
        naming::PRODUCT_NAME,
        ctx.repo_path
    ));

    out.push_str("\n## Tickets\n\n");
    if ctx.tickets.is_empty() {
        out.push_str("(no tickets yet)\n");
    }
    for t in &ctx.tickets {
        let kind = if t.epic { "epic · " } else { "" };
        out.push_str(&format!(
            "- {} · {} ({kind}{} · {})",
            t.key,
            t.title,
            t.status.label(),
            t.priority
        ));
        if let Some(parent) = &t.parent {
            out.push_str(&format!(" · in epic {parent}"));
        }
        if !t.depends_on.is_empty() {
            let deps: Vec<String> = t.depends_on.iter().map(ToString::to_string).collect();
            out.push_str(&format!(" · depends on {}", deps.join(", ")));
        }
        if let Some(id) = &t.external_id {
            out.push_str(&format!(" · external ID {id}"));
        }
        out.push('\n');
        if let Some(worktree) = &t.worktree {
            let branch = t.branch.as_deref().unwrap_or("?");
            out.push_str(&format!("  worktree {worktree} · branch `{branch}`\n"));
        }
    }

    out.push_str(&render_choices(
        &ctx.choices,
        AgentReader::ProjectAgent,
        None,
    ));

    out.push_str(&format!("\n## {}'s rules\n\n", naming::PRODUCT_NAME));
    let rules = [
        "Follow Noyan’s instructions and the project's notes below. They add to these rules.".to_owned(),
        format!(
            "Right after you read the user's first request, name this conversation: \
             `{cmd} agent name \"<3–4 words>\"`. Rename it only when the user asks (`--rename`)."
        ),
        "Read and run code in this checkout and in the ticket worktrees above, but never edit files and never \
         commit: code changes go through tickets."
            .to_owned(),
        format!(
            "Create tickets with `{cmd} create \"Title\" --stdin` (long text on stdin), several at once with \
             `{cmd} create -f <file>`; epics with `--type epic`, children with `--parent`, priority with `-p`."
        ),
        format!(
            "Edit, link (`{cmd} dep add`) and prioritise only tickets still in Backlog; anything else is refused \
             (\"{BACKLOG_ONLY}\"). If the CLI refuses, tell the user what you would need."
        ),
        format!(
            "Ask a ticket's agent with `{cmd} ask <KEY> --stdin`; ask the user with `{cmd} ask human --stdin` \
             (with `--option` when there are clear alternatives), then end your turn."
        ),
    ];
    for (n, rule) in rules.iter().enumerate() {
        out.push_str(&format!("{}. {rule}\n", n + 1));
    }

    if let Some(instructions) = &ctx.instructions {
        out.push_str(&instructions.render("Noyan’s instructions"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket(key: &str, title: &str, status: Status) -> PrimeTicket {
        PrimeTicket {
            key: key.parse().unwrap(),
            title: title.into(),
            epic: false,
            status,
            priority: Priority::default(),
            depends_on: Vec::new(),
            parent: None,
            external_id: None,
            editable: status == Status::Backlog,
            worktree: None,
            branch: None,
        }
    }

    /// The whole text for a small board (T-0200, the prime snapshot).
    #[test]
    fn snapshot() {
        let mut started = ticket("SHOP-19", "Price table", Status::Working);
        started.worktree = Some("/data/wt/shop-web/SHOP-19".into());
        started.branch = Some("feature/price-table".into());
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.epic = true;
        epic.external_id = Some("GH-212".into());
        let mut child = ticket("SHOP-40.1", "Wishlist API", Status::Backlog);
        child.parent = Some("SHOP-40".parse().unwrap());
        child.depends_on = vec!["SHOP-19".parse().unwrap()];
        let ctx = ProjectPrime {
            project: "SHOP".into(),
            name: "shop-web".into(),
            repo_path: "/src/shop-web".into(),
            tickets: vec![started, epic, child],
            instructions: Some(Instructions {
                agent: Some("Noyan".into()),
                own: "Use my /import-github skill.".into(),
                project: "shop-web".into(),
                notes: "Ask before touching the payment code.".into(),
            }),
            choices: Vec::new(),
        };
        let cmd = naming::COMMAND_NAME;
        let product = naming::PRODUCT_NAME;
        let expected = format!(
            "# Noyan · shop-web (SHOP)

You are Noyan, the planning agent of this {product} project, in /src/shop-web, the user's own checkout. You \
investigate and file tickets; agents build them later in their own worktrees.

## Tickets

- SHOP-19 · Price table (Working · P3 · Normal)
  worktree /data/wt/shop-web/SHOP-19 · branch `feature/price-table`
- SHOP-40 · Wishlist (epic · Backlog · P3 · Normal) · external ID GH-212
- SHOP-40.1 · Wishlist API (Backlog · P3 · Normal) · in epic SHOP-40 · depends on SHOP-19

## {product}'s rules

1. Follow Noyan’s instructions and the project's notes below. They add to these rules.
2. Right after you read the user's first request, name this conversation: `{cmd} agent name \"<3–4 words>\"`. \
Rename it only when the user asks (`--rename`).
3. Read and run code in this checkout and in the ticket worktrees above, but never edit files and never commit: \
code changes go through tickets.
4. Create tickets with `{cmd} create \"Title\" --stdin` (long text on stdin), several at once with `{cmd} create -f \
<file>`; epics with `--type epic`, children with `--parent`, priority with `-p`.
5. Edit, link (`{cmd} dep add`) and prioritise only tickets still in Backlog; anything else is refused (\"Noyan can \
only change Backlog tickets\"). If the CLI refuses, tell the user what you would need.
6. Ask a ticket's agent with `{cmd} ask <KEY> --stdin`; ask the user with `{cmd} ask human --stdin` (with \
`--option` when there are clear alternatives), then end your turn.

## Noyan’s instructions (Noyan)

Use my /import-github skill.

## Notes every agent in shop-web gets

Ask before touching the payment code.
"
        );
        assert_eq!(render(&ctx), expected);
    }
}

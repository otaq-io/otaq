//! The agent's PR commands (REV-10, GIT-07; d0050): `otaq pr propose --stdin`, `otaq pr opened <url>`
//! and `otaq branch rename <name>`. The service checks who calls (an agent only for its own ticket, CLI-02).

use std::path::Path;

use serde_json::json;

use super::proposal::{self, Proposal};
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::Actor;
use crate::store::now_ms;

impl Board {
    /// `otaq pr propose --stdin`: checks the proposal's shape and stores it on the ticket's PR flow.
    pub async fn pr_propose(
        &self,
        key: &str,
        text: &str,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.ticket(key).await?;
        let proposal = proposal::parse(text)
            .map_err(|message| BoardError::refused(Refusal::BadProposal, message))?;
        let cmd = crate::naming::COMMAND_NAME;
        let flow = match self.pr_flow(&ticket.id).await? {
            None => {
                return Err(BoardError::refused(
                    Refusal::MoveRefused,
                    format!(
                        "{} has no Open PR in progress: the user starts it with Approve & open PR",
                        ticket.key
                    ),
                ));
            }
            Some(flow) if flow.state == super::flow::FlowState::Pushing => {
                return Err(BoardError::refused(
                    Refusal::MoveRefused,
                    format!(
                        "The user already confirmed {}'s PR: run `{cmd} prime` and push it",
                        ticket.key
                    ),
                ));
            }
            Some(flow) => flow,
        };
        let proposal = self.check_proposal(&ticket, flow.kind, &proposal).await?;
        self.store_proposal(&ticket, &proposal, actor).await?;
        self.ticket_by_id(&ticket.id).await
    }

    /// Stores a checked proposal: the flow is now "proposed".
    pub(crate) async fn store_proposal(
        &self,
        ticket: &Ticket,
        proposal: &Proposal,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let mut tx = self.store().begin_write().await?;
        sqlx::query(
            "UPDATE pr_flows SET state = 'proposed', proposal_json = ?, reported = NULL, updated_at = ?
             WHERE ticket_id = ?",
        )
        .bind(serde_json::to_string(proposal).expect("proposal serializes"))
        .bind(now_ms())
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::PrProposed,
            actor,
            json!({ "branch": proposal.branch, "title": proposal.title, "commits": proposal.commits.len() }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// `otaq pr opened <url>`: records the PR's URL and number; the branch name is locked from now on.
    pub async fn pr_opened(
        &self,
        key: &str,
        url: &str,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.ticket(key).await?;
        let url = url.trim();
        let number = proposal::pr_number(url).ok_or_else(|| {
            BoardError::refused(
                Refusal::BadPrUrl,
                format!(
                    "{url} isn't a pull request URL; report the PR's own page, like https://github.com/acme/shop-web/pull/481 or …/_git/shop-web/pullrequest/481"
                ),
            )
        })?;
        let mut tx = self.store().begin_write().await?;
        sqlx::query(
            "UPDATE tickets SET pr_url = ?, pr_number = ?, branch_pushed = TRUE, updated_at = ? WHERE id = ?",
        )
        .bind(url)
        .bind(i64::from(number))
        .bind(now_ms())
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::PrOpened,
            actor,
            json!({ "url": url, "number": number }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, url, "PR reported");
        self.ticket_by_id(&ticket.id).await
    }

    /// `otaq branch rename <name>` (GIT-07): renames the ticket's branch until its first push.
    pub async fn rename_branch(
        &self,
        key: &str,
        name: &str,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.ticket(key).await?;
        let name = name.trim();
        let Some(row) = self.store().ticket(&ticket.id).await? else {
            return Err(BoardError::NotFound(key.to_owned()));
        };
        let Some(current) = row.branch_name.clone() else {
            return Err(BoardError::refused(
                Refusal::NoBranch,
                format!(
                    "{} has no branch yet: it starts on one when it goes to Dev",
                    ticket.key
                ),
            ));
        };
        if row.branch_pushed {
            return Err(BoardError::refused(
                Refusal::BranchLocked,
                branch_locked_message(&ticket.key.to_string(), &current),
            ));
        }
        if let Some(problem) = proposal::branch_name_problem(name) {
            return Err(BoardError::refused(Refusal::BadBranchName, problem));
        }
        if name == current {
            return Ok(ticket);
        }
        let repo = self.project(ticket.key.project.as_str()).await?.repo_path;
        let git = self.git()?;
        if git
            .resolve(Path::new(&repo), &format!("refs/heads/{name}"))
            .await?
            .is_some()
        {
            return Err(BoardError::refused(
                Refusal::BranchTaken,
                format!("A branch named {name} already exists"),
            ));
        }
        git.run(Path::new(&repo), &["branch", "-m", &current, name])
            .await?;
        let mut tx = self.store().begin_write().await?;
        sqlx::query("UPDATE tickets SET branch_name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(now_ms())
            .bind(&ticket.id)
            .execute(&mut *tx)
            .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::BranchRenamed,
            actor,
            json!({ "from": current, "to": name }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, from = %current, to = name, "branch renamed");
        self.ticket_by_id(&ticket.id).await
    }
}

/// "SHOP-18's branch was pushed as `bugfix/SHOP-18-login-redirect` and can't be renamed" (GIT-07).
pub fn branch_locked_message(key: &str, branch: &str) -> String {
    format!("{key}'s branch was pushed as `{branch}` and can't be renamed")
}

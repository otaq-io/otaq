//! The Open PR / Push to PR flow of a delivery unit (REV-10, REV-14; d0050, d0041, d0059), from the user's click
//! until the skill reports the PR or the user cancels: `pr_flows` (migration 0018).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::proposal::{self, Proposal};
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::{Actor, Status, TicketKey, stored_enum};
use crate::git::reword::CommitInfo;
use crate::store::now_ms;

stored_enum! {
    pub enum FlowKind ("PR flow kind") {
        /// Open PR: the first push, the branch gets its real name.
        Open = "open",
        /// Push to PR #N: new commits to the PR's locked branch.
        Push = "push",
    }
}

stored_enum! {
    pub enum FlowState ("PR flow state") {
        /// The skill runs; no proposal yet ("Open PR skill is preparing…").
        Preparing = "preparing",
        /// The dialog shows the proposal.
        Proposed = "proposed",
        /// The user confirmed: the edits are applied and the skill was told to push.
        Pushing = "pushing",
    }
}

/// A ticket's PR flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrFlow {
    pub kind: FlowKind,
    pub state: FlowState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<Proposal>,
    /// What the user confirmed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed: Option<Proposal>,
    /// What the skill reported instead of a PR (a failed push, a missing login).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported: Option<String>,
    pub started_at: i64,
    /// An automatic Delivery's (FLW-15): no dialog, Otaq confirms the proposal as it is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub automatic: bool,
}

type FlowRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    bool,
);

impl Board {
    /// The ticket's PR flow, if one is under way.
    pub async fn pr_flow(&self, ticket_id: &str) -> Result<Option<PrFlow>, BoardError> {
        let row: Option<FlowRow> = sqlx::query_as(
            "SELECT kind, state, proposal_json, confirmed_json, reported, started_at, automatic FROM pr_flows
             WHERE ticket_id = ?",
        )
        .bind(ticket_id)
        .fetch_optional(self.store().pool())
        .await?;
        let Some((kind, state, proposal, confirmed, reported, started_at, automatic)) = row else {
            return Ok(None);
        };
        let parse = |json: Option<String>| json.and_then(|j| serde_json::from_str(&j).ok());
        Ok(Some(PrFlow {
            kind: kind.parse().map_err(unknown)?,
            state: state.parse().map_err(unknown)?,
            proposal: parse(proposal),
            confirmed: parse(confirmed),
            reported,
            started_at,
            automatic,
        }))
    }
}

fn unknown(e: crate::domain::UnknownValue) -> BoardError {
    BoardError::NotFound(e.to_string())
}

/// One commit of the PR as the dialog shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowCommit {
    pub sha: String,
    /// The current message (the proposal's, while one is shown, comes separately).
    pub message: String,
    /// Already on the PR's remote branch: read-only (Push to PR, REV-14).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pushed: bool,
}

/// What the Open PR / Push to PR dialog shows (`ticket.pr_flow`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrFlowView {
    pub key: TicketKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<PrFlow>,
    /// The branch the PR targets: the ticket's or epic's base branch.
    pub target: String,
    /// The ticket's branch now: `otaq/SHOP-18`, or the pushed name.
    pub branch: String,
    /// The branch was pushed: its name can't change (GIT-07).
    pub branch_locked: bool,
    /// Every commit of the PR, oldest first.
    pub commits: Vec<FlowCommit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// The skill that proposes: the project's Open PR skill, or its Push skill for a ticket with a PR.
    #[serde(default)]
    pub skill: String,
}

/// The answer to a live branch-name check (REV-10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum BranchCheck {
    Ok,
    /// Not a valid ref name, or taken: the reason, for the dialog.
    Problem {
        message: String,
    },
}

/// The ticket's PR commits, and how many of the first ones are already on the remote branch.
pub(crate) struct PrCommits {
    pub repo: PathBuf,
    pub branch: String,
    pub head: String,
    pub commits: Vec<CommitInfo>,
    pub pushed: usize,
}

impl PrCommits {
    /// The commits the proposal names and the user may reword: all of them for Open PR, the unpushed ones for
    /// Push to PR.
    pub fn editable(&self) -> &[CommitInfo] {
        &self.commits[self.pushed..]
    }
}

/// The message of a delivery unit with no commits to propose.
fn no_commits(key: &TicketKey) -> BoardError {
    BoardError::refused(
        Refusal::BadProposal,
        format!("{key} has no commits to push"),
    )
}

impl Board {
    /// The PR's commits: `base_sha..branch`; for a ticket with a PR, the ones already on `origin/<branch>` are
    /// marked pushed (after a fetch of that branch).
    pub(crate) async fn pr_commits(&self, ticket: &Ticket) -> Result<PrCommits, BoardError> {
        let row = self
            .store()
            .ticket(&ticket.id)
            .await?
            .ok_or_else(|| BoardError::NotFound(ticket.key.to_string()))?;
        let (Some(branch), Some(base_sha)) = (row.branch_name, row.base_sha) else {
            return Err(BoardError::refused(
                Refusal::NoBranch,
                format!("{} has no branch to push", ticket.key),
            ));
        };
        let repo = PathBuf::from(self.project(ticket.key.project.as_str()).await?.repo_path);
        let git = self.git()?;
        let head = git
            .resolve(&repo, &format!("refs/heads/{branch}"))
            .await?
            .ok_or_else(|| {
                BoardError::refused(
                    Refusal::NoBranch,
                    format!("{}'s branch {branch} doesn't exist any more", ticket.key),
                )
            })?;
        let commits = git.commits_between(&repo, &base_sha, &head).await?;
        let mut pushed = 0;
        if row.pr_number.is_some() {
            let tracking = format!("refs/remotes/origin/{branch}");
            let refspec = format!("+refs/heads/{branch}:{tracking}");
            if let Err(e) = git
                .run(&repo, &["fetch", "--quiet", "origin", &refspec])
                .await
            {
                tracing::warn!(key = %ticket.key, error = %e, "can't fetch the PR's branch");
            }
            if let Some(remote) = git.resolve(&repo, &tracking).await? {
                let new = git.commits_between(&repo, &remote, &head).await?;
                pushed = commits.len().saturating_sub(new.len());
            }
        }
        Ok(PrCommits {
            repo,
            branch,
            head,
            commits,
            pushed,
        })
    }

    /// What the Open PR / Push to PR dialog shows.
    pub async fn pr_flow_view(&self, key: &str) -> Result<PrFlowView, BoardError> {
        let ticket = self.ticket(key).await?;
        let commits = self.pr_commits(&ticket).await?;
        let row = self
            .store()
            .ticket(&ticket.id)
            .await?
            .ok_or_else(|| BoardError::NotFound(key.to_owned()))?;
        let target = self.merge_target(&ticket).await?.target;
        let step = match row.pr_number {
            Some(_) => crate::board::settings::WorkflowStep::Push,
            None => crate::board::settings::WorkflowStep::OpenPr,
        };
        let skill = self
            .delivery_skill(ticket.key.project.as_str(), step)
            .await?;
        Ok(PrFlowView {
            skill,
            flow: self.pr_flow(&ticket.id).await?,
            target,
            branch: commits.branch.clone(),
            branch_locked: row.branch_pushed,
            commits: commits
                .commits
                .iter()
                .enumerate()
                .map(|(n, c)| FlowCommit {
                    sha: c.sha.clone(),
                    message: c.message.clone(),
                    pushed: n < commits.pushed,
                })
                .collect(),
            pr_number: row.pr_number.and_then(|n| u32::try_from(n).ok()),
            pr_url: row.pr_url,
            external_id: ticket.external_id.clone(),
            key: ticket.key,
        })
    }

    /// Whether `name` can be the PR's branch: a valid ref name, not taken locally (by another branch) and not on
    /// the remote (`git ls-remote`). Used live by the dialog and again on confirm.
    pub async fn check_branch_name(
        &self,
        key: &str,
        name: &str,
    ) -> Result<BranchCheck, BoardError> {
        let ticket = self.ticket(key).await?;
        let name = name.trim();
        // The dialog's hint ends with the fix (d0093).
        if name.contains(' ') {
            return Ok(BranchCheck::Problem {
                message: "Use hyphens instead of spaces".to_owned(),
            });
        }
        if let Some(message) = proposal::branch_name_problem(name) {
            return Ok(BranchCheck::Problem { message });
        }
        let current = ticket.work.as_ref().and_then(|w| w.branch.clone());
        if current.as_deref() == Some(name) {
            return Ok(BranchCheck::Ok);
        }
        let repo = PathBuf::from(self.project(ticket.key.project.as_str()).await?.repo_path);
        let git = self.git()?;
        if git
            .resolve(&repo, &format!("refs/heads/{name}"))
            .await?
            .is_some()
        {
            return Ok(BranchCheck::Problem {
                message: "A local branch with this name already exists. Choose another name."
                    .to_owned(),
            });
        }
        let heads = format!("refs/heads/{name}");
        match git
            .run(&repo, &["ls-remote", "--heads", "origin", &heads])
            .await
        {
            Ok(out) if !out.trim().is_empty() => Ok(BranchCheck::Problem {
                message: "A branch with this name already exists on origin. Choose another name."
                    .to_owned(),
            }),
            Ok(_) => Ok(BranchCheck::Ok),
            Err(e) => Ok(BranchCheck::Problem {
                message: format!("Can\u{2019}t check origin for {name}: {e}"),
            }),
        }
    }

    /// Starts the flow on the user's click, or `automatic` for an automatic Delivery (FLW-15): "preparing" until the
    /// skill proposes. A flow already under way starts over.
    pub(crate) async fn start_pr_flow(
        &self,
        ticket: &Ticket,
        kind: FlowKind,
        automatic: bool,
    ) -> Result<(), BoardError> {
        let now = now_ms();
        sqlx::query(
            "INSERT INTO pr_flows (ticket_id, kind, state, started_at, updated_at, automatic)
             VALUES (?, ?, 'preparing', ?, ?, ?)
             ON CONFLICT (ticket_id) DO UPDATE SET kind = excluded.kind, state = 'preparing', proposal_json = NULL,
                 confirmed_json = NULL, reported = NULL, started_at = excluded.started_at,
                 updated_at = excluded.updated_at, automatic = excluded.automatic",
        )
        .bind(&ticket.id)
        .bind(kind.as_str())
        .bind(now)
        .bind(now)
        .bind(automatic)
        .execute(self.store().pool())
        .await?;
        Ok(())
    }

    /// What the skill reported instead of a PR (`otaq ask human` during the flow): shown in the dialog with
    /// Retry and Cancel.
    pub async fn report_pr_problem(&self, ticket_id: &str, text: &str) -> Result<(), BoardError> {
        sqlx::query("UPDATE pr_flows SET reported = ?, updated_at = ? WHERE ticket_id = ?")
            .bind(text)
            .bind(now_ms())
            .bind(ticket_id)
            .execute(self.store().pool())
            .await?;
        Ok(())
    }

    /// Ends the flow (Cancel, or the PR was reported): the proposal is discarded.
    pub(crate) async fn end_pr_flow(&self, ticket_id: &str) -> Result<(), BoardError> {
        sqlx::query("DELETE FROM pr_flows WHERE ticket_id = ?")
            .bind(ticket_id)
            .execute(self.store().pool())
            .await?;
        Ok(())
    }

    /// Checks a proposal against the ticket's commits and its flow: the same commits, in order (short IDs
    /// resolved to full ones); Push to PR keeps the locked branch; Open PR needs a title. Returns it with full IDs.
    pub(crate) async fn check_proposal(
        &self,
        ticket: &Ticket,
        kind: FlowKind,
        proposal: &Proposal,
    ) -> Result<Proposal, BoardError> {
        let bad = |message: String| BoardError::refused(Refusal::BadProposal, message);
        let commits = self.pr_commits(ticket).await?;
        let editable = commits.editable();
        if editable.is_empty() {
            return Err(no_commits(&ticket.key));
        }
        if kind == FlowKind::Push && proposal.branch != commits.branch {
            return Err(BoardError::refused(
                Refusal::BranchLocked,
                super::commands::branch_locked_message(&ticket.key.to_string(), &commits.branch),
            ));
        }
        if kind == FlowKind::Open && proposal.title.trim().is_empty() {
            return Err(bad("the proposal has no PR title".to_owned()));
        }
        let listed: Vec<String> = editable.iter().map(|c| short(&c.sha).to_owned()).collect();
        if proposal.commits.len() != editable.len() {
            return Err(bad(format!(
                "the proposal has {} commit messages, but {} has {} commits to push: {}",
                proposal.commits.len(),
                ticket.key,
                editable.len(),
                listed.join(", ")
            )));
        }
        let mut checked = proposal.clone();
        for (n, (proposed, actual)) in checked.commits.iter_mut().zip(editable).enumerate() {
            if !actual.sha.starts_with(&proposed.sha) {
                return Err(bad(format!(
                    "commit {} of the proposal is {}, but the commits to push are, in order: {}",
                    n + 1,
                    proposed.sha,
                    listed.join(", ")
                )));
            }
            proposed.sha = actual.sha.clone();
        }
        Ok(checked)
    }

    /// The user's confirmed edits, applied by Otaq itself (REV-10): the commits are reworded (trees
    /// unchanged, checked) and the branch renamed; then the name is locked and the flow is "pushing". Any failure
    /// leaves the branch as it was.
    pub(crate) async fn apply_confirmed(
        &self,
        ticket: &Ticket,
        kind: FlowKind,
        confirmed: &Proposal,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let commits = self.pr_commits(ticket).await?;
        let git = self.git()?;
        let repo = commits.repo.as_path();
        let parent = match commits.pushed {
            0 => self
                .store()
                .ticket(&ticket.id)
                .await?
                .and_then(|r| r.base_sha)
                .ok_or_else(|| no_commits(&ticket.key))?,
            n => commits.commits[n - 1].sha.clone(),
        };
        let pairs: Vec<(CommitInfo, String)> = commits
            .editable()
            .iter()
            .cloned()
            .zip(confirmed.commits.iter().map(|c| c.message.clone()))
            .collect();
        let tip = git.reword(repo, &parent, &pairs).await?;
        // No file changes: every new commit has its old commit's tree.
        let new = git.commits_between(repo, &parent, &tip).await?;
        for (old, new) in pairs.iter().map(|(c, _)| c).zip(&new) {
            if git.tree_of(repo, &old.sha).await? != git.tree_of(repo, &new.sha).await? {
                return Err(BoardError::refused(
                    Refusal::BadProposal,
                    "rewording changed a commit's files; nothing was changed",
                ));
            }
        }
        let branch_ref = format!("refs/heads/{}", commits.branch);
        if tip != commits.head {
            git.run(repo, &["update-ref", &branch_ref, &tip, &commits.head])
                .await?;
        }
        let renamed = kind == FlowKind::Open && confirmed.branch != commits.branch;
        if renamed
            && let Err(e) = git
                .run(repo, &["branch", "-m", &commits.branch, &confirmed.branch])
                .await
        {
            // Put the old commits back.
            if tip != commits.head {
                let _ = git
                    .run(repo, &["update-ref", &branch_ref, &commits.head, &tip])
                    .await;
            }
            return Err(e.into());
        }
        // The reviewed head follows the reworded commits (no new content, GIT-05).
        self.record_otaq_rebase(&ticket.id, &commits.head, &tip)
            .await?;
        let mut tx = self.store().begin_write().await?;
        sqlx::query(
            "UPDATE tickets SET branch_name = ?, branch_pushed = TRUE, updated_at = ? WHERE id = ?",
        )
        .bind(&confirmed.branch)
        .bind(now_ms())
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE pr_flows SET state = 'pushing', confirmed_json = ?, updated_at = ? WHERE ticket_id = ?",
        )
        .bind(serde_json::to_string(confirmed).expect("proposal serializes"))
        .bind(now_ms())
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        if renamed {
            record_event(
                &mut tx,
                &ticket.id,
                EventKind::BranchRenamed,
                actor,
                json!({ "from": commits.branch, "to": confirmed.branch }),
            )
            .await?;
        }
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::PrConfirmed,
            actor,
            json!({ "kind": kind, "branch": confirmed.branch, "title": confirmed.title }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, branch = %confirmed.branch, "PR edits applied");
        Ok(())
    }
}

impl Board {
    /// **PR rework** (REV-14, d0041): Approved · PR open → Ready for Review. The PR stays (number, URL, locked
    /// branch); the review starts again from the branch's head, so the user can Send back as usual.
    pub(crate) async fn pr_rework(
        &self,
        ticket: &Ticket,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let row = self
            .store()
            .ticket(&ticket.id)
            .await?
            .ok_or_else(|| BoardError::NotFound(ticket.key.to_string()))?;
        let repo = PathBuf::from(self.project(ticket.key.project.as_str()).await?.repo_path);
        let head = match &row.branch_name {
            Some(branch) => {
                self.git()?
                    .resolve(&repo, &format!("refs/heads/{branch}"))
                    .await?
            }
            None => None,
        };
        let to = Status::WaitingForYou;
        // Back from the open PR to the human check before the Delivery (STG-06).
        let moved = self
            .moved(ticket, ticket.status, crate::flow::engine::Move::PrRework)
            .await?;
        let now = now_ms();
        let mut tx = self.store().begin_write().await?;
        moved.record(&mut tx, &ticket.id, None).await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, review_head_sha = COALESCE(?, review_head_sha),
                                review_entered_at = ?, pr_rework = TRUE, updated_by = ?, updated_at = ?, ",
            crate::store::tickets::set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(moved.place.stage_id())
        .bind(to.as_str())
        .bind(head)
        .bind(now)
        .bind(actor.to_string())
        .bind(now)
        .bind(to.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::Moved,
            actor,
            json!({ "from": ticket.status, "to": to, "rework": true }),
        )
        .await?;
        tx.commit().await?;
        self.ticket_by_id(&ticket.id).await
    }
}

impl Board {
    /// **Send back with PR comments** (FLW-16, d0108): Delivery · PR open → the nearest earlier Agent stage, Working
    /// (its live session gets the skill) or Queued before it with the skill still to run. The PR stays (number,
    /// URL, locked branch); the optional note is the user's comment (`hand_over`, CMT-05), posted first, and the agent
    /// reads the PR's unresolved comments with `otaq review feedback`. Every agent check's counter restarts at 0, as
    /// after a human Fail. Returns what the agent is told ("SHOP-18 was sent back to Dev with PR comments and a
    /// comment. Run `otaq prime`, then `otaq review feedback`."), already stored for a queued ticket.
    pub(crate) async fn record_send_back_pr(
        &self,
        ticket: &Ticket,
        hand_over: Option<&crate::board::hand_over::HandOver>,
        to: Status,
        resume_skill: Option<&str>,
        actor: &Actor,
    ) -> Result<String, BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let moved = self
            .moved(ticket, ticket.status, crate::flow::engine::Move::PrComments)
            .await?;
        let stage = moved.place.stage().map(str::to_owned);
        let name = stage
            .as_deref()
            .and_then(|id| flow.stage(id))
            .or_else(|| flow.first_agent_stage())
            .map_or_else(|| "Dev".to_owned(), |s| s.name.clone());
        let stage = match to.is_queued() {
            true => stage.and_then(|id| crate::board::send_back::waits_before(&flow, &id)),
            false => stage,
        };
        let round = self.current_round(&ticket.id).await?;
        let note = hand_over.map_or("", |h| h.body.as_str());
        let now = now_ms();
        let mut tx = self.store().begin_write().await?;
        let posted = match hand_over {
            Some(h) => Some(h.post(&mut tx, &ticket.id, actor).await?),
            None => None,
        };
        moved
            .record(&mut tx, &ticket.id, posted.as_ref().map(|p| p.id.as_str()))
            .await?;
        sqlx::query(
            "INSERT INTO reviews (id, ticket_id, round, action, at) VALUES (?, ?, ?, 'send_back', ?)",
        )
        .bind(crate::store::new_id())
        .bind(&ticket.id)
        .bind(i64::from(round))
        .bind(now)
        .execute(&mut *tx)
        .await?;
        crate::board::send_back::return_to_dev(
            &mut tx,
            ticket,
            to,
            stage.as_deref(),
            resume_skill,
            actor,
        )
        .await?;
        let told = crate::board::hand_over::sent_back_text(&crate::board::hand_over::SentBack {
            key: &ticket.key.to_string(),
            stage: &name,
            line_comments: 0,
            pr_comments: true,
            comment: posted.is_some(),
            by_you: false,
            from: None,
        });
        if resume_skill.is_some() {
            sqlx::query("UPDATE tickets SET resume_prompt = ? WHERE id = ?")
                .bind(&told)
                .bind(&ticket.id)
                .execute(&mut *tx)
                .await?;
        }
        if posted.is_some() {
            crate::board::stage_runs::mark_handed_over(&mut tx, &ticket.id).await?;
        }
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::SentBackWithPrComments,
            actor,
            json!({ "pr": ticket.flags.pr_number, "note": note, "to": to }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, %to, "sent back with PR comments");
        Ok(told)
    }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

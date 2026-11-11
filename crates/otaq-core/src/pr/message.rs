//! The delivery's message, written while the ticket waits in Review: when a ticket reaches Review on a flow whose
//! `delivery:` names an agent (click mode), that agent writes the message the delivery will use, so Approve's
//! confirmation opens at once and nobody waits for it. For a Local merge it is the squash commit's message (the title
//! line, then the description); for a PR, its title, description and branch. The agent sends it with `otaq pr message
//! --stdin`; until then the confirmation starts from Otaq's plain message and says the agent is still writing.
//!
//! It is asked for again only when the ticket arrives with new commits (a send back that changed the work): the same
//! head keeps the message it has. `delivery_messages` (migration 0099).
//!
//! The user's own edit of that message in Approve & merge or Approve & open PR is kept apart, on the ticket, until the
//! ticket lands or the user chooses Use the proposed message: reopening the dialog after a refusal, a closed dialog or
//! an app restart shows the edit, and a Send back keeps it. `edited_messages` (migration 0117).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError, Refusal};
use crate::domain::{Status, stored_enum};
use crate::naming;
use crate::store::now_ms;

stored_enum! {
    pub enum MessageState ("delivery message state") {
        /// The agent was asked and hasn't sent it yet.
        Writing = "writing",
        /// The agent sent it.
        Ready = "ready",
    }
}

/// A ticket's delivery message, as its agent wrote it (or is writing it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryMessage {
    pub state: MessageState,
    /// The delivery's agent, as the flow names it ("Strong").
    pub agent: String,
    /// The branch commit it was asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    /// "SHOP-18 Retry the order request once on timeout"; empty while writing.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// The PR's branch name, when the flow can open a PR and the agent proposed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

impl DeliveryMessage {
    pub fn is_ready(&self) -> bool {
        self.state == MessageState::Ready
    }

    /// The squash commit's message: the title line, a blank line, the description.
    pub fn squash(&self) -> String {
        match self.description.trim() {
            "" => self.title.trim().to_owned(),
            body => format!("{}\n\n{body}", self.title.trim()),
        }
    }
}

/// What the agent sends on stdin: `{"title": …, "description": …, "branch": …}` (`branch` only for a flow that can
/// open a PR).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageText {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// Reads the agent's message and checks its shape: a one-line title, and a valid branch name when one is given.
pub fn parse(text: &str) -> Result<MessageText, String> {
    let mut message: MessageText = serde_json::from_str(text.trim()).map_err(|e| {
        format!(
            "the message isn't the JSON object `{} pr message` takes ({e}); expected {{\"title\", \"description\", \"branch\"}}",
            naming::COMMAND_NAME
        )
    })?;
    message.title = message.title.trim().to_owned();
    message.description = message.description.trim().to_owned();
    message.branch = message
        .branch
        .map(|b| b.trim().to_owned())
        .filter(|b| !b.is_empty());
    if message.title.is_empty() {
        return Err("the message has no title".to_owned());
    }
    if message.title.contains('\n') {
        return Err("the title is one line: put the rest in the description".to_owned());
    }
    if let Some(problem) = message
        .branch
        .as_deref()
        .and_then(super::proposal::branch_name_problem)
    {
        return Err(problem);
    }
    Ok(message)
}

/// What the delivery's agent is told when the ticket reaches Review. One line, typed with Enter.
pub fn prompt(key: &str, target: &str, can_open_pr: bool, can_merge: bool) -> String {
    let cmd = naming::COMMAND_NAME;
    let product = naming::PRODUCT_NAME;
    let uses = match (can_merge, can_open_pr) {
        (true, true) => format!(
            "the squash commit if it is merged into {target}, or the pull request if one is opened"
        ),
        (false, true) => "the pull request".to_owned(),
        _ => format!("the squash commit that merges it into {target}"),
    };
    let branch = match can_open_pr {
        true => format!(
            ", \"branch\": the PR's branch name (follow the user's rules; else `{}-<a few words>`)",
            naming::branch(key)
        ),
        false => String::new(),
    };
    format!(
        "{key} is waiting in Review. You are its delivery's agent: write the message its delivery uses, for {uses}, so \
         it is ready when the user approves. Run `{cmd} prime` and read the commits against the base branch, and \
         follow the user's Agent instructions. The message must pass the project's own commit message rules, so find \
         them first: {MESSAGE_RULES_PLACES}, and the style of recent commits on {target}. Then send one JSON object \
         with `{cmd} pr message --stdin`: \"title\": one line that follows those rules and keeps {key} where they \
         allow it (\"feat: retry the order request once on timeout ({key})\"); when nothing sets a format, it starts \
         with {key} (\"{key} Retry the order request once on timeout\"), \"description\": what changed and why, and \
         how it was checked, in Markdown{branch}. Change no files and no commits, push nothing, and end your turn: \
         {product} shows it to the user, who may edit it."
    )
}

/// Where an agent finds a project's commit message rules before it writes a message: the merge message above, and
/// the commit, open-pr and push skills, which name the same places. The hooks folder is the one git really uses, so a
/// husky-style `core.hooksPath` is found from a ticket's worktree too.
pub const MESSAGE_RULES_PLACES: &str = "a CONTRIBUTING file or similar docs, a commitlint or similar config \
     (`commitlint.config.*`, `.commitlintrc*`, a `commitlint` key in `package.json`), the commit-msg hook in the \
     folder `git rev-parse --git-path hooks` prints (it follows `core.hooksPath`, as husky sets it)";

/// What the agent's `otaq pr message` prints.
pub fn stored_text(key: &str) -> String {
    format!(
        "{key}'s message is with the user. End your turn: nothing is merged or pushed until they approve."
    )
}

/// The user's edit of a ticket's delivery message, as kept on the ticket: each field the user changed from the
/// proposal; `None` (or no entry) shows the proposal there.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditedMessage {
    /// Approve & merge's squash message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge: Option<String>,
    /// Approve & open PR's title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The per-commit messages a Push to PR rewords, by commit sha.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub commits: BTreeMap<String, String>,
}

impl EditedMessage {
    pub fn is_empty(&self) -> bool {
        self.merge.is_none()
            && self.title.is_none()
            && self.description.is_none()
            && self.commits.is_empty()
    }
}

/// One dialog's edit as the dialog keeps it: it replaces that dialog's part of the kept edit and leaves the other's.
/// An empty field is "the proposal" (Use the proposed message sends them all empty).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "dialog", rename_all = "snake_case")]
pub enum MessageEdit {
    /// Approve & merge: the squash message, `None` when it is the proposal.
    Merge {
        #[serde(default)]
        message: Option<String>,
    },
    /// Approve & open PR / Push to PR: the fields that differ from the proposal.
    Pr {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        commits: BTreeMap<String, String>,
    },
}

/// Drops a ticket's kept edit, bound to its id: when it lands (merged, finished, its PR opened or pushed), in the same
/// transaction as the move.
pub(crate) const FORGET_EDITED: &str = "DELETE FROM edited_messages WHERE ticket_id = ?";

type EditedRow = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

type Row = (
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
);

impl Board {
    /// The ticket's delivery message, if one was asked for.
    pub async fn delivery_message(
        &self,
        ticket_id: &str,
    ) -> Result<Option<DeliveryMessage>, BoardError> {
        let row: Option<Row> = sqlx::query_as(
            "SELECT state, agent, head, title, description, branch FROM delivery_messages WHERE ticket_id = ?",
        )
        .bind(ticket_id)
        .fetch_optional(self.store().pool())
        .await?;
        let Some((state, agent, head, title, description, branch)) = row else {
            return Ok(None);
        };
        Ok(Some(DeliveryMessage {
            state: state
                .parse()
                .map_err(|e: crate::domain::UnknownValue| BoardError::NotFound(e.to_string()))?,
            agent,
            head,
            title,
            description,
            branch,
        }))
    }

    /// The agent was asked for the message of `head`: "writing" until it sends it; an earlier message is dropped.
    pub(crate) async fn start_delivery_message(
        &self,
        ticket_id: &str,
        agent: &str,
        head: Option<&str>,
    ) -> Result<(), BoardError> {
        sqlx::query(
            "INSERT INTO delivery_messages (ticket_id, state, agent, head, title, description, branch, updated_at)
             VALUES (?, 'writing', ?, ?, '', '', NULL, ?)
             ON CONFLICT (ticket_id) DO UPDATE SET state = 'writing', agent = excluded.agent, head = excluded.head,
                 title = '', description = '', branch = NULL, updated_at = excluded.updated_at",
        )
        .bind(ticket_id)
        .bind(agent)
        .bind(head)
        .bind(now_ms())
        .execute(self.store().pool())
        .await?;
        Ok(())
    }

    /// The message couldn't be asked for (the agent can't start now): the confirmation starts from the plain one.
    pub(crate) async fn forget_delivery_message(&self, ticket_id: &str) -> Result<(), BoardError> {
        sqlx::query("DELETE FROM delivery_messages WHERE ticket_id = ?")
            .bind(ticket_id)
            .execute(self.store().pool())
            .await?;
        Ok(())
    }

    /// The session asked for the message ended before the agent sent it: the confirmation starts from the plain
    /// message, and nothing says the agent is still writing. A message it sent stays. Whether one was dropped.
    pub(crate) async fn drop_unwritten_message(&self, ticket_id: &str) -> Result<bool, BoardError> {
        let done =
            sqlx::query("DELETE FROM delivery_messages WHERE ticket_id = ? AND state = 'writing'")
                .bind(ticket_id)
                .execute(self.store().pool())
                .await?;
        Ok(done.rows_affected() > 0)
    }

    /// `otaq pr message --stdin`: the agent's message, stored for Approve's confirmation. Refused once the ticket no
    /// longer waits to be delivered.
    pub async fn store_delivery_message(
        &self,
        key: &str,
        text: &str,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.ticket(key).await?;
        if !matches!(ticket.status, Status::WaitingForYou | Status::ToDeliver) {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; its delivery's message is written while it waits in Review",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        let message = parse(text).map_err(|e| BoardError::refused(Refusal::BadProposal, e))?;
        let agent = match self.delivery_message(&ticket.id).await? {
            Some(m) => m.agent,
            None => self
                .effective_flow(ticket.key.project.as_str())
                .await?
                .delivery
                .agent
                .unwrap_or_default(),
        };
        let head = self.branch_head(&ticket).await;
        sqlx::query(
            "INSERT INTO delivery_messages (ticket_id, state, agent, head, title, description, branch, updated_at)
             VALUES (?, 'ready', ?, ?, ?, ?, ?, ?)
             ON CONFLICT (ticket_id) DO UPDATE SET state = 'ready', title = excluded.title,
                 description = excluded.description, branch = excluded.branch,
                 head = COALESCE(delivery_messages.head, excluded.head), updated_at = excluded.updated_at",
        )
        .bind(&ticket.id)
        .bind(agent)
        .bind(head)
        .bind(&message.title)
        .bind(&message.description)
        .bind(&message.branch)
        .bind(now_ms())
        .execute(self.store().pool())
        .await?;
        tracing::info!(key = %ticket.key, "delivery message stored");
        self.ticket(key).await
    }

    /// The user's kept edit of the ticket's delivery message; empty when there is none.
    pub async fn edited_message(&self, ticket_id: &str) -> Result<EditedMessage, BoardError> {
        let row: Option<EditedRow> = sqlx::query_as(
            "SELECT merge_message, pr_title, pr_description, pr_commits FROM edited_messages WHERE ticket_id = ?",
        )
        .bind(ticket_id)
        .fetch_optional(self.store().pool())
        .await?;
        let Some((merge, title, description, commits)) = row else {
            return Ok(EditedMessage::default());
        };
        Ok(EditedMessage {
            merge,
            title,
            description,
            // Commit messages that can't be read show the proposal there rather than failing the dialog.
            commits: commits
                .and_then(|c| serde_json::from_str(&c).ok())
                .unwrap_or_default(),
        })
    }

    /// Keeps the user's edit of one dialog's message on the ticket (empty, it puts back the proposal there). It stays
    /// until the ticket lands; a Send back keeps it.
    pub async fn keep_edited_message(
        &self,
        key: &str,
        edit: MessageEdit,
    ) -> Result<EditedMessage, BoardError> {
        let ticket = self.ticket(key).await?;
        let mut kept = self.edited_message(&ticket.id).await?;
        match edit {
            MessageEdit::Merge { message } => kept.merge = message,
            MessageEdit::Pr {
                title,
                description,
                commits,
            } => {
                kept.title = title;
                kept.description = description;
                kept.commits = commits;
            }
        }
        if kept.is_empty() {
            self.forget_edited_message(&ticket.id).await?;
            return Ok(kept);
        }
        let commits = (!kept.commits.is_empty())
            .then(|| serde_json::to_string(&kept.commits).ok())
            .flatten();
        sqlx::query(
            "INSERT INTO edited_messages (ticket_id, merge_message, pr_title, pr_description, pr_commits, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (ticket_id) DO UPDATE SET merge_message = excluded.merge_message,
                 pr_title = excluded.pr_title, pr_description = excluded.pr_description,
                 pr_commits = excluded.pr_commits, updated_at = excluded.updated_at",
        )
        .bind(&ticket.id)
        .bind(&kept.merge)
        .bind(&kept.title)
        .bind(&kept.description)
        .bind(commits)
        .bind(now_ms())
        .execute(self.store().pool())
        .await?;
        Ok(kept)
    }

    /// Use the proposed message everywhere: the kept edit goes.
    pub(crate) async fn forget_edited_message(&self, ticket_id: &str) -> Result<(), BoardError> {
        sqlx::query(FORGET_EDITED)
            .bind(ticket_id)
            .execute(self.store().pool())
            .await?;
        Ok(())
    }

    /// The commit the ticket's branch points at now; `None` when it can't be read.
    pub(crate) async fn branch_head(&self, ticket: &Ticket) -> Option<String> {
        let branch = ticket.work.as_ref()?.branch.clone()?;
        let repo = self
            .project(ticket.key.project.as_str())
            .await
            .ok()?
            .repo_path;
        self.git()
            .ok()?
            .resolve(std::path::Path::new(&repo), &format!("refs/heads/{branch}"))
            .await
            .ok()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_needs_a_one_line_title_and_a_valid_branch() {
        let ok = parse(
            r#"{"title": " SHOP-18 Retry once ", "description": "Retries.\n", "branch": ""}"#,
        )
        .unwrap();
        assert_eq!(ok.title, "SHOP-18 Retry once");
        assert_eq!(ok.description, "Retries.");
        assert_eq!(ok.branch, None, "an empty branch is no branch");
        assert_eq!(
            parse(r#"{"title": "", "description": "x"}"#).unwrap_err(),
            "the message has no title"
        );
        assert!(
            parse(r#"{"title": "a\nb"}"#)
                .unwrap_err()
                .contains("one line")
        );
        assert!(
            parse(r#"{"title": "a", "branch": "has space"}"#)
                .unwrap_err()
                .contains("isn't a valid branch name")
        );
        assert!(parse("not json").unwrap_err().contains("pr message"));
    }

    #[test]
    fn the_squash_message_is_the_title_then_the_description() {
        let m = DeliveryMessage {
            state: MessageState::Ready,
            agent: "Strong".into(),
            head: None,
            title: "SHOP-18 Retry once".into(),
            description: "Retries POST /orders.".into(),
            branch: None,
        };
        assert_eq!(m.squash(), "SHOP-18 Retry once\n\nRetries POST /orders.");
        let bare = DeliveryMessage {
            description: String::new(),
            ..m
        };
        assert_eq!(bare.squash(), "SHOP-18 Retry once");
    }

    /// The agent is told what the message is for, how to send it, and to change nothing.
    #[test]
    fn the_prompt_names_the_command_and_what_the_message_is_for() {
        let either = prompt("SHOP-18", "main", true, true);
        assert!(
            either.contains(&format!("`{} pr message --stdin`", naming::COMMAND_NAME)),
            "{either}"
        );
        assert!(either.contains("merged into main, or the pull request"));
        assert!(either.contains("\"branch\""));
        assert!(!either.contains('\n'), "one line, typed with Enter");
        let local = prompt("SHOP-18", "main", false, true);
        assert!(local.contains("the squash commit that merges it into main"));
        assert!(
            !local.contains("\"branch\""),
            "a local merge has no PR branch"
        );
        assert!(local.contains("Change no files and no commits"));
    }

    /// The agent looks for the project's message rules before it writes, follows them keeping the key where they
    /// allow, and leads with the key only when nothing sets a format.
    #[test]
    fn the_prompt_follows_the_projects_message_rules_and_else_leads_with_the_key() {
        let text = prompt("HOOK-1", "main", false, true);
        for needle in [
            "CONTRIBUTING",
            "commitlint",
            "`git rev-parse --git-path hooks`",
            "`core.hooksPath`",
            "Agent instructions",
            "recent commits on main",
            "keeps HOOK-1 where they allow it",
            "\"feat: retry the order request once on timeout (HOOK-1)\"",
            "when nothing sets a format, it starts with HOOK-1 (\"HOOK-1 Retry the order request once on timeout\")",
        ] {
            assert!(
                text.contains(needle),
                "the prompt must say {needle:?}: {text}"
            );
        }
        let rules = text.find("find them first").unwrap();
        let send = text.find("pr message --stdin").unwrap();
        assert!(
            rules < send,
            "the rules are found before the message is sent"
        );
    }
}

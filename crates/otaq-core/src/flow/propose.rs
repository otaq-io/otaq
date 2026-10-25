//! `otaq flow show`, `otaq flow propose` and the answer to a proposal (FLW-02, FLW-03; d0108 "The CLI proposes a
//! whole document", second gap review rule 10).
//!
//! **The document** ([`Board::flow_document`]): the project's applied stages and the agents they name, as stored now,
//! written in the format `propose` reads, so `flow show | flow propose --stdin` is "the same as your current flow".
//!
//! **A proposal from the CLI** ([`Board::propose`]): a document (`-f`, `--stdin`) or another project's stages
//! (`--from`), by a Project agent or the user's own terminal. It goes through T-0413's checks; a document the same as
//! the flow as it will be is refused with [`SAME`] (nothing to ask the user), and so is one with a problem the editor
//! can't fix (a Project agent's with any problem: it can act on them). Otherwise it lands in the project's draft, the
//! agent's changes marked (FED-02, [`super::draft`]), or during Flow setup becomes that tool's entry (ASK-03), and the
//! command returns at once.
//!
//! **The answer** ([`Answer`]): the user's Apply, Cancel or Ask for more changes reaches the proposing Project agent's
//! session as its next message ([`deliver_answer`]), typed once the agent is idle and its input line is empty (d0048),
//! whatever triggered it (the flow editor, a request). A proposal from the user's terminal gets nothing.

use serde::{Deserialize, Serialize};

use super::model::FlowFile;
use super::proposal::{
    ApplyChoices, Proposal, ProposedBy, Row, SAME, Source, flow_agent_of, match_ids,
};
use super::yaml;
use crate::agents::ModelCatalog;
use crate::board::{Board, BoardError, Refusal};
use crate::naming;
use crate::session::deliveries::{Deliveries, DeliveryKind};
use crate::store::now_ms;

/// Who may propose (FLW-03, user decision 2026-10-20): a ticket's agent, a check and an epic lead are refused.
pub const ONLY_A_PROJECT_AGENT_OR_YOU: &str = "Only Noyan or you can propose a flow";

/// What `flow.propose` proposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum Proposed {
    /// A whole flow document (`-f <file>`, `--stdin`); `file` its name, for an import clash's "Add as".
    Document {
        yaml: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<String>,
    },
    /// Another project's stages (`--from <project>`, by key or name); agents are shared.
    Project { project: String },
}

/// `flow.propose`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposeParams {
    pub project: String,
    pub source: Proposed,
}

/// `flow.ask_for_changes`'s parameters: the flow editor's Ask for more changes on the open proposal `proposal`, with the
/// user's line of text for the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskForChangesParams {
    pub project: String,
    pub proposal: u32,
    pub text: String,
}

/// `flow show`: the project's applied flow as a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowDocument {
    /// The project's key.
    pub project: String,
    /// The applied version.
    pub version: u32,
    /// The document as `propose` reads it.
    pub yaml: String,
    /// The same document as JSON (`flow show --json`).
    pub document: serde_json::Value,
}

/// The user's answer to a Project agent's proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Apply; `waiting` is what still waits for a quiet board ("QA after Security"), if anything.
    Applied {
        waiting: Option<String>,
    },
    Cancelled,
    /// Ask for more changes, with the user's text.
    MoreChanges(String),
}

/// The message the proposing agent gets: "The user applied Proposal 3.", "The user cancelled Proposal 3.", "The user
/// asks for more changes to Proposal 3: Keep Review for every ticket".
pub fn answer_text(proposal: u32, answer: &Answer) -> String {
    match answer {
        Answer::Applied { waiting: None } => format!("The user applied Proposal {proposal}."),
        Answer::Applied {
            waiting: Some(waiting),
        } => format!(
            "The user applied Proposal {proposal}. This part waits until no ticket is working: {waiting}."
        ),
        Answer::Cancelled => format!("The user cancelled Proposal {proposal}."),
        Answer::MoreChanges(text) => {
            format!(
                "The user asks for more changes to Proposal {proposal}: {}",
                text.trim()
            )
        }
    }
}

/// What `propose` prints once the proposal is open: the agent ends its turn and waits.
pub fn waiting_text(proposal: &Proposal) -> String {
    match &proposal.proposed_by {
        ProposedBy::ProjectAgent { .. } => format!(
            "Proposal {} is waiting for the user. End your turn: their answer comes back here.",
            proposal.id
        ),
        ProposedBy::User if proposal.problems.is_empty() => {
            format!(
                "Proposal {} is waiting for you in {}.",
                proposal.id,
                naming::PRODUCT_NAME
            )
        }
        ProposedBy::User => format!(
            "Proposal {} is waiting for you in {}: fix {} there before you apply it.",
            proposal.id,
            naming::PRODUCT_NAME,
            match proposal.problems.len() {
                1 => "1 problem".to_owned(),
                n => format!("{n} problems"),
            }
        ),
    }
}

/// Types `answer` into the session that proposed `proposal` when a Project agent did; nothing for the user's own.
/// A failed delivery is logged, never the answer's error: the user's click already took effect.
pub async fn deliver_answer(
    deliveries: &Deliveries,
    proposed_by: &ProposedBy,
    proposal: u32,
    answer: &Answer,
) {
    let ProposedBy::ProjectAgent { session, .. } = proposed_by else {
        return;
    };
    let text = answer_text(proposal, answer);
    // An answer to the agent's own request, typed as the next message like an agent's question or answer (d0048).
    if let Err(e) = deliveries
        .queue_delivery(session, DeliveryKind::AgentMessage, &text)
        .await
    {
        tracing::warn!(%session, error = %e, "couldn't deliver the answer to a flow proposal");
    }
}

impl Board {
    /// `flow show`: the project's applied stages, and the stored agents they name (their `agent` and options'), in
    /// stage order.
    pub async fn flow_document(&self, project: &str) -> Result<FlowDocument, BoardError> {
        let key = self.project(project).await?.key;
        let version = self.current_flow(&key).await?;
        let applied = self.current_flow_file(&key).await?;
        let stored = self.store().agents().await?;
        let mut file = FlowFile {
            agents: Vec::new(),
            flow: applied.flow,
        };
        for stage in &file.flow.stages {
            for name in stage.every_agent() {
                if file
                    .agents
                    .iter()
                    .any(|a| a.name.eq_ignore_ascii_case(name))
                {
                    continue;
                }
                if let Some(agent) = stored.iter().find(|a| a.name.eq_ignore_ascii_case(name)) {
                    file.agents.push(flow_agent_of(agent));
                }
            }
        }
        let text = yaml::write(&file);
        let document = serde_saphyr::from_str(&text).map_err(|e| {
            BoardError::NotFound(format!("{key}'s flow doesn't read as a document: {e}"))
        })?;
        Ok(FlowDocument {
            project: key,
            version: version.version,
            yaml: text,
            document,
        })
    }

    /// `flow propose` (FLW-03): checks the proposal against the flow as it will be, then opens it (replacing the open
    /// one). Refused with [`SAME`] when nothing differs, and with its problems, one per line, when any can't be fixed
    /// in the flow editor (any at all for a Project agent).
    pub async fn propose(
        &self,
        project: &str,
        proposed: &Proposed,
        by: &ProposedBy,
        models: &dyn ModelCatalog,
    ) -> Result<Proposal, BoardError> {
        let refuse = |message: String| BoardError::refused(Refusal::BadFlow, message);
        let key = self.project(project).await?.key;
        let (file, written, source) = match proposed {
            Proposed::Document { yaml: text, file } => {
                let (read, written) = yaml::read_with_written_ids(text)
                    .map_err(|e| refuse(format!("The flow doesn't read: {e}")))?;
                (read, written, Source::Document { file: file.clone() })
            }
            Proposed::Project { project: other } => {
                let other = self.project_named(other).await?;
                if other.key == key {
                    return Err(refuse(format!(
                        "{} is the project you propose for: name another one",
                        other.name
                    )));
                }
                let file = FlowFile {
                    agents: Vec::new(),
                    flow: self.effective_flow(&other.key).await?,
                };
                // Another project's ids mean nothing here: its stages match this flow's by name, then position.
                let written = vec![false; file.flow.stages.len()];
                let source = Source::Project {
                    key: other.key,
                    name: other.name,
                };
                (file, written, source)
            }
        };
        // Looked at before it is stored, so a refusal leaves the open proposal as it was.
        let (base, _) = self.flow_as_it_will_be(&key).await?;
        let mut matched = file.clone();
        match_ids(&base.flow, &mut matched.flow, &written);
        let row = Row {
            id: 0,
            project: key.clone(),
            yaml: yaml::write(&matched),
            source: source.clone(),
            proposed_by: by.clone(),
            draft: super::draft::DraftMemory::default(),
            created_at: now_ms(),
        };
        let view = self.view(&row, models, &ApplyChoices::default()).await?;
        let agent = matches!(by, ProposedBy::ProjectAgent { .. });
        if view.problems.iter().any(|p| agent || p.fix.is_none()) {
            let lines: Vec<&str> = view.problems.iter().map(|p| p.message.as_str()).collect();
            return Err(refuse(lines.join("\n")));
        }
        if view.diff.same {
            return Err(refuse(SAME.to_owned()));
        }
        // Outside Flow setup it lands in the draft (FED-02); during it each tool's is its own entry (ASK-03, FED-03).
        if self.project(&key).await?.flow_setup_since.is_none() {
            return self
                .propose_into_draft(&key, matched, &source, by, models)
                .await;
        }
        self.open_read_proposal(
            &key,
            file,
            &written,
            &source,
            by,
            &super::draft::DraftMemory::default(),
            models,
        )
        .await
    }

    /// Who proposed `project`'s open proposal `id`; `None` when it isn't open (any more).
    pub async fn proposed_by(
        &self,
        project: &str,
        id: u32,
    ) -> Result<Option<ProposedBy>, BoardError> {
        Ok(self
            .proposal_row_by_id(project, id)
            .await?
            .map(|r| r.proposed_by))
    }

    /// **Ask for more changes** (`flow.ask_for_changes`): closes the open proposal `id` of a Project agent; its text
    /// goes to the agent, which proposes again. Who proposed it.
    pub async fn ask_for_changes(
        &self,
        project: &str,
        id: u32,
        text: &str,
    ) -> Result<ProposedBy, BoardError> {
        let refuse = |message: String| BoardError::refused(Refusal::BadFlow, message);
        if text.trim().is_empty() {
            return Err(refuse("Write what to change".to_owned()));
        }
        let by = self
            .proposed_by(project, id)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("Proposal {id} isn't open any more")))?;
        if by == ProposedBy::User {
            return Err(refuse(format!(
                "Proposal {id} isn't from Noyan: nobody to ask"
            )));
        }
        self.drop_proposal(id).await?;
        Ok(by)
    }
}

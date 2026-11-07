//! Skills: Otaq never copies, imports or renames a skill (T2).
//!
//! - **The user's skills stay where they are.** The agent tool finds them the way it always does: personal skills,
//!   project skills committed to the repository (which git puts in every worktree) and, for Claude Code, the main
//!   checkout's when the worktree has none. Otaq writes no skill folder into a worktree or a checkout.
//! - **Otaq's own skills come from its plugin.** The skills in `<plugin>/skills/` ([`SHIPPED_FOLDER`]) reach every
//!   session through Otaq's per-tool plugin, the same one that carries its hooks, so every one of them is in every
//!   session from its start and a hand-off can always type a skill by name.
//! - **A session names a skill in its tool's form** ([`crate::agent_tools::AgentTool::skill_reference`]): Otaq's own
//!   as `/otaq:<name>` (Claude Code, Copilot), `$otaq:<name>` (Codex) or `<name>` (OpenCode); the user's as
//!   `/<name>`, `$<name>` or `<name>`. A name that matches one of Otaq's shipped skills always means Otaq's.
//! - **A stage's `skill:` is the only place a skill is picked**: a stage that names one runs it by that name; a stage
//!   that names none runs Otaq's own skill for it.
//!
//! [`copy`] holds what finds and removes the copies an older Otaq wrote (folders holding its marker), which the warm
//! build still leaves out.

pub mod copy;
pub mod folder;
pub mod found;
pub mod testing;

pub use copy::{marked_copies, remove_copy};

use crate::board::Board;

/// The folder of Otaq's shipped skills inside its plugin folder ([`crate::agent_tools::LaunchContext::plugin_dir`]):
/// `<plugin>/skills/<name>/`.
pub const SHIPPED_FOLDER: &str = "skills";

impl Board {
    /// The skill the PR path runs for `step` (Open PR, Push), by its own name: for Open PR the skill the flow's
    /// Delivery stage names (`skill: my-pr`, Q296), else Otaq's (`open-pr`, `push`). The Open PR flow's lines and
    /// REV-11's "use your own Open PR skill" name it (WSK-08).
    pub async fn delivery_skill(
        &self,
        project: &str,
        step: crate::board::settings::WorkflowStep,
    ) -> Result<String, crate::board::BoardError> {
        self.delivery_skill_name(project, step).await
    }

    /// The skill name the PR path runs for `step`: for Open PR the one the flow's Delivery stage names (Q296), else
    /// the step's own (`open-pr`, `push`).
    pub async fn delivery_skill_name(
        &self,
        project: &str,
        step: crate::board::settings::WorkflowStep,
    ) -> Result<String, crate::board::BoardError> {
        let named = match step {
            crate::board::settings::WorkflowStep::OpenPr => self
                .effective_flow(project)
                .await?
                .stages
                .iter()
                .find(|s| s.is_delivery())
                .and_then(|s| s.skill.clone()),
            _ => None,
        };
        Ok(named.unwrap_or_else(|| step.own_skill().to_owned()))
    }
}

//! Skills: Otaq never copies, imports or renames a skill (T2).
//!
//! - **The user's skills stay where they are.** The agent tool finds them the way it always does: personal skills,
//!   project skills committed to the repository (which git puts in every worktree) and, for Claude Code, the main
//!   checkout's when the worktree has none. Otaq writes no skill folder into a worktree or a checkout.
//! - **Otaq's own skills come from its plugin.** The skills in `<plugin>/skills/` ([`SHIPPED_FOLDER`]) reach every
//!   session through Otaq's per-tool plugin, the same one that carries its hooks, so every one of them is in every
//!   session from its start and a hand-off can always type a skill by name.
//! - **Otaq's own skills are named in their namespace** (decisions 063, 074): a workflow skill as `otaq:<name>`
//!   (`otaq:validate`), a generic one as `tuq:<name>` (`tuq:product-spec`), in flows and everywhere Otaq names one
//!   ([`own_name`], [`own_folder`]). A bare name (`validate`) is always the user's own skill, even when it matches
//!   one of Otaq's. Flows stored before the rename are rewritten once ([`namespaced`], decision 075).
//! - **A session names a skill in its tool's form** ([`crate::agent_tools::AgentTool::skill_reference`]): Otaq's own
//!   as `/otaq:<name>` (Claude Code, Copilot), `$otaq:<name>` (Codex) or `<name>` (OpenCode); the user's as
//!   `/<name>`, `$<name>` or `<name>`. Until tuq's skills run on the user's tuq, a `tuq:<name>` runs the copy still in
//!   Otaq's plugin, so it is typed as Otaq's (`/otaq:product-spec`).
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

/// The namespace of Otaq's workflow skills: `otaq:validate` (decision 063), its plugin's name.
pub const OTAQ_NAMESPACE: &str = crate::naming::PLUGIN_NAMESPACE;

/// The namespace of the generic skills, tuq's: `tuq:product-spec` (decision 063).
pub const TUQ_NAMESPACE: &str = "tuq";

/// The shipped skills ([`crate::agents::shipped::SKILLS`]) that are tuq's, named `tuq:<name>`; every other one is a
/// workflow skill, named `otaq:<name>`. Until tuq's skills run on the user's tuq, their copies are still in Otaq's
/// plugin.
pub const TUQ_SKILLS: &[&str] = &["milestone-plan", "product-spec", "tech-spec"];

/// The namespace a shipped skill's folder (`validate`, `product-spec`) is named in: `otaq` or `tuq`.
fn namespace_of(folder: &str) -> &'static str {
    if TUQ_SKILLS.contains(&folder) {
        TUQ_NAMESPACE
    } else {
        OTAQ_NAMESPACE
    }
}

/// The name of the shipped skill in the plugin's folder `folder`, in its namespace: `otaq:validate`,
/// `tuq:product-spec`.
pub fn own_name(folder: &str) -> String {
    format!("{}:{folder}", namespace_of(folder))
}

/// The plugin folder of Otaq's own skill named `skill` (`otaq:validate` → `validate`, `tuq:product-spec` →
/// `product-spec`); `None` for any other name, a bare one included: that is the user's skill (decision 063).
pub fn own_folder(skill: &str) -> Option<&'static str> {
    let (namespace, name) = skill.split_once(':')?;
    crate::agents::shipped::SKILLS
        .iter()
        .copied()
        .find(|folder| *folder == name && namespace_of(folder) == namespace)
}

/// Whether `skill` names one of Otaq's own skills: `otaq:<name>` or `tuq:<name>` for a skill Otaq ships.
pub fn is_own(skill: &str) -> bool {
    own_folder(skill).is_some()
}

/// The rename of decision 075: a bare name of one of Otaq's shipped skills (`validate`, `product-spec`) in its
/// namespace (`otaq:validate`, `tuq:product-spec`); `None` for any other name, which stays as it is.
pub fn namespaced(bare: &str) -> Option<String> {
    crate::agents::shipped::SKILLS
        .contains(&bare)
        .then(|| own_name(bare))
}

impl Board {
    /// The skill the PR path runs for `step` (Open PR, Push), by its own name: for Open PR the skill the flow's
    /// delivery names (`skill: my-pr`, Q296), else Otaq's (`otaq:open-pr`, `otaq:push`). The Open PR flow's lines and
    /// REV-11's "use your own Open PR skill" name it (WSK-08).
    pub async fn delivery_skill(
        &self,
        project: &str,
        step: crate::board::settings::WorkflowStep,
    ) -> Result<String, crate::board::BoardError> {
        self.delivery_skill_name(project, step).await
    }

    /// The skill name the PR path runs for `step`: for Open PR the one the flow's delivery names (Q296), else
    /// the step's own (`otaq:open-pr`, `otaq:push`).
    pub async fn delivery_skill_name(
        &self,
        project: &str,
        step: crate::board::settings::WorkflowStep,
    ) -> Result<String, crate::board::BoardError> {
        let named = match step {
            crate::board::settings::WorkflowStep::OpenPr => {
                self.effective_flow(project).await?.delivery.skill
            }
            _ => None,
        };
        Ok(named.unwrap_or_else(|| step.own_skill().to_owned()))
    }
}

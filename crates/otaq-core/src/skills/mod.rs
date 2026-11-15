//! Skills: Otaq never copies, imports or renames a skill (T2).
//!
//! - **The user's skills stay where they are.** The agent tool finds them the way it always does: personal skills,
//!   project skills committed to the repository (which git puts in every worktree) and, for Claude Code, the main
//!   checkout's when the worktree has none. Otaq writes no skill folder into a worktree or a checkout.
//! - **Otaq's own skills come from its plugin.** The skills in `<plugin>/skills/` ([`crate::runtime::shipped_skills`];
//!   the repository's is the `otaq-skills` submodule, which holds them in its own `skills/`) reach every
//!   session through Otaq's per-tool plugin, the same one that carries its hooks, so every one of them is in every
//!   session from its start and a hand-off can always type a skill by name.
//! - **Otaq's own skills are named in their namespace** (decisions 063, 074): a workflow skill as `otaq:<name>`
//!   (`otaq:validate`), in flows and everywhere Otaq names one ([`own_name`], [`own_folder`]). A generic one is tuq's,
//!   `tuq:<name>` (`tuq:product-spec`): it runs the user's tuq, which Otaq installs when a stage needs it
//!   ([`crate::tuq`], decisions 100, 102, 103). A bare name (`validate`) is always the user's own skill, even when it
//!   matches one of Otaq's. Flows stored before the rename are rewritten once ([`namespaced`], decision 075).
//! - **A session names a skill in its tool's form** ([`crate::agent_tools::AgentTool::skill_reference`]): Otaq's own
//!   as `/otaq:<name>` (Claude Code), `$otaq:<name>` (Codex), and on Copilot and OpenCode from the prefixed copy
//!   ([`prefixed`], decisions 072, 092) as `/otaq:otaq-<name>` and `otaq-<name>`; tuq's as `/tuq:<name>`,
//!   `$tuq:<name>`, `/tuq:tuq-<name>` and `tuq-<name>` (decision 105); the user's as `/<name>`, `$<name>` or
//!   `<name>`.
//! - **A stage's `skill:` is the only place a skill is picked**: a stage that names one runs it by that name; a stage
//!   that names none runs Otaq's own skill for it.
//! - **An Agent stage whose skill isn't Otaq's runs two** ([`task_skill`], decisions 081, 098): Otaq's workflow
//!   skill `otaq:work-ticket` first, which says how the work meets Otaq (asking through the ticket, committing, ending
//!   the stage), then, once that first turn ends, the stage's own **task skill** (`tuq:product-spec`, the user's
//!   `team-dev`), which says what the stage produces ([`crate::start::sequence`]). A task skill knows nothing of Otaq,
//!   and a skill only a person may start can't be called by another skill (Claude Code, Copilot), so Otaq types it.
//!
//! [`copy`] holds what finds and removes the copies an older Otaq wrote (folders holding its marker), which the warm
//! build still leaves out.

pub mod copy;
pub mod folder;
pub mod found;
pub mod prefixed;
pub mod testing;

pub use copy::{marked_copies, remove_copy};

use crate::board::Board;

/// The namespace of Otaq's workflow skills: `otaq:validate` (decision 063), its plugin's name.
pub const OTAQ_NAMESPACE: &str = crate::naming::PLUGIN_NAMESPACE;

/// The namespace of the generic skills, tuq's: `tuq:product-spec` (decision 063). They are the user's tuq, not Otaq's
/// plugin's ([`crate::tuq`]).
pub const TUQ_NAMESPACE: &str = crate::tuq::NAMESPACE;

/// The skills Otaq shipped before they became tuq's alone (decision 102), which the rename of decision 075 names
/// `tuq:<name>`: a stored flow's bare `product-spec` is `tuq:product-spec`.
pub const RENAMED_TO_TUQ: &[&str] = &["milestone-plan", "product-spec", "tech-spec"];

/// The task skill of an Agent stage whose `skill:` is `stage_skill` (decision 081): any skill that isn't one of Otaq's
/// own (`tuq:product-spec`, the user's `team-dev`), which Otaq types once `otaq:work-ticket`'s first turn ended. `None`
/// for a stage that names none or names one of Otaq's own: that skill alone runs, as the stage's first prompt.
pub fn task_skill(stage_skill: Option<&str>) -> Option<&str> {
    stage_skill
        .map(str::trim)
        .filter(|name| !name.is_empty() && !is_own(name))
}

/// The name of the shipped skill in the plugin's folder `folder`, in Otaq's namespace: `otaq:validate`.
pub fn own_name(folder: &str) -> String {
    format!("{}:{folder}", OTAQ_NAMESPACE)
}

/// The plugin folder of Otaq's own workflow skill named `skill` (`otaq:validate` → `validate`); `None` for any other
/// name: a bare one is the user's skill (decision 063), a `tuq:` one is the user's tuq's ([`crate::tuq`]).
pub fn own_folder(skill: &str) -> Option<&'static str> {
    let name = skill.strip_prefix(OTAQ_NAMESPACE)?.strip_prefix(':')?;
    crate::agents::shipped::SKILLS
        .iter()
        .copied()
        .find(|folder| *folder == name)
}

/// Whether `skill` names one of Otaq's own workflow skills, which come with the app: `otaq:<name>` for a skill Otaq
/// ships. Every tool has them.
pub fn is_own(skill: &str) -> bool {
    own_folder(skill).is_some()
}

/// The skill's own name when `skill` is one of Otaq's (`otaq:validate` → `validate`) or tuq's (`tuq:product-spec` →
/// `product-spec`, decision 104: tuq counts as Otaq's), for the words Otaq shows about it; `None` for any other name.
pub fn otaqs_or_tuqs(skill: &str) -> Option<&str> {
    own_folder(skill).or_else(|| crate::tuq::skill_of(skill))
}

/// The rename of decision 075: a bare name of one of Otaq's shipped skills (`validate`) in its namespace
/// (`otaq:validate`), and of the ones that became tuq's (`product-spec`) in tuq's (`tuq:product-spec`); `None` for
/// any other name, which stays as it is.
pub fn namespaced(bare: &str) -> Option<String> {
    if crate::agents::shipped::SKILLS.contains(&bare) {
        Some(own_name(bare))
    } else if RENAMED_TO_TUQ.contains(&bare) {
        Some(format!("{TUQ_NAMESPACE}:{bare}"))
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    use super::task_skill;

    /// Decision 081: a stage naming tuq's or the user's skill runs it as its task skill, after `otaq:work-ticket`; a
    /// stage naming one of Otaq's own, or none, runs that one alone.
    #[test]
    fn a_stage_has_a_task_skill_only_when_its_skill_isnt_otaqs() {
        assert_eq!(
            task_skill(Some("tuq:product-spec")),
            Some("tuq:product-spec")
        );
        assert_eq!(task_skill(Some(" team-dev ")), Some("team-dev"));
        // A bare name is the user's, even one matching Otaq's (decision 063).
        assert_eq!(task_skill(Some("work-ticket")), Some("work-ticket"));
        assert_eq!(task_skill(Some(crate::own_skill!("work-ticket"))), None);
        assert_eq!(task_skill(Some(crate::own_skill!("validate"))), None);
        assert_eq!(task_skill(Some("  ")), None);
        assert_eq!(task_skill(None), None);
    }
}

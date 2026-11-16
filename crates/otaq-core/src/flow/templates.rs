//! The ready stages Edit flow offers between two columns: data shipped with the app (`flows/stage-templates.yaml`),
//! not code. Each template is a stage in the flow file's own format, with its description, model and skill filled in,
//! and one line for the palette. Blank stage comes last.

use std::sync::LazyLock;

use super::model::Stage;
use super::yaml::{self, StageTemplate};

/// The shipped templates file.
pub const TEMPLATES_YAML: &str = include_str!("../../flows/stage-templates.yaml");

static TEMPLATES: LazyLock<Vec<StageTemplate>> = LazyLock::new(|| {
    yaml::read_templates(TEMPLATES_YAML).expect("the shipped stage templates read (tested)")
});

/// Every template, in the palette's order.
pub fn templates() -> &'static [StageTemplate] {
    &TEMPLATES
}

/// A template by its stage's id (`security-review`).
pub fn template(id: &str) -> Option<&'static StageTemplate> {
    templates().iter().find(|t| t.stage.id == id)
}

impl StageTemplate {
    /// The id the palette sends back: its stage's.
    pub fn id(&self) -> &str {
        &self.stage.id
    }

    /// The stage it adds, before it is given a name and id the flow doesn't have yet.
    pub fn stage(&self) -> &Stage {
        &self.stage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::model::{CheckBy, StageKind};

    #[test]
    fn the_shipped_templates_read_in_the_palettes_order() {
        let titles: Vec<&str> = templates().iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Security review",
                "Update the docs",
                "Design review",
                "Product spec",
                "Tech spec",
                "Blank stage",
            ]
        );
        for t in templates() {
            assert!(!t.line.is_empty(), "{} has a line", t.title);
            // Every stage an agent works or checks says who does it.
            if t.stage.runs_agent() {
                assert_eq!(t.stage.model(), Some("default"), "{}", t.title);
            }
        }
    }

    #[test]
    fn each_ready_stage_comes_with_its_description_model_and_skill() {
        let security = template("security-review").unwrap();
        assert_eq!(security.stage.kind, StageKind::Check { by: CheckBy::Agent });
        assert_eq!(security.stage.model(), Some("default"));
        assert_eq!(security.stage.skill.as_deref(), Some("security-check"));
        assert!(security.stage.description.is_some());
        // A check by you has no model and no skill.
        let design = template("design-review").unwrap();
        assert_eq!(design.stage.kind, StageKind::Check { by: CheckBy::You });
        assert!(design.stage.who.model.is_none() && design.stage.skill.is_none());
        // Blank stage: a name and a kind, nothing else.
        let blank = templates().last().unwrap();
        assert_eq!(blank.stage.name, "New stage");
        assert!(blank.stage.skill.is_none() && blank.stage.description.is_none());
    }

    /// A template's skill is never a tool's built-in command, which a flow refuses as a skill.
    #[test]
    fn no_template_names_a_built_in_command() {
        for t in templates() {
            if let Some(skill) = &t.stage.skill {
                assert!(
                    !crate::agent_tools::claude::BUILT_IN_COMMANDS.contains(&skill.as_str()),
                    "{skill}"
                );
            }
        }
    }

    #[test]
    fn a_template_naming_another_stage_is_refused() {
        let text = "templates:\n  - line: x\n    stage:\n      name: Check\n      kind: check\n      by: you\n      fail_to: Dev\n";
        assert!(yaml::read_templates(text).is_err());
    }
}

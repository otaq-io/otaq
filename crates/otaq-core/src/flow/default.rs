//! The default flow (STG-03, FLW-01, d0108, d0004): Dev → Validate → Review, each in its own column, delivered on
//! Review's Approve by a local merge or a PR (`delivers: either`, `mode: click`). Otaq ships it as a file, embedded
//! at build time; every project gets it as its first flow version ([`super::store`]).

use std::sync::LazyLock;

use super::model::{Flow, FlowFile};
use super::yaml;
use crate::naming;

/// The default flow's stage ids.
pub const DEV: &str = "dev";
pub const VALIDATE: &str = "validate";
pub const REVIEW: &str = "review";

static DEFAULT: LazyLock<FlowFile> = LazyLock::new(|| {
    yaml::read(naming::DEFAULT_FLOW_YAML).expect("the shipped default flow reads (tested)")
});

/// The shipped default flow file, as read.
pub fn default_file() -> &'static FlowFile {
    &DEFAULT
}

/// The shipped default flow: Dev → Validate → Review, then delivered.
pub fn default_flow() -> Flow {
    DEFAULT.flow.clone()
}

/// Tests only: changes to the default flow the suites put a project on with
/// [`crate::board::Board::edit_flow_for_tests`].
#[doc(hidden)]
pub mod testing {
    use super::super::model::{Delivers, DeliveryMode, FlowFile, StageOption};
    use super::{REVIEW, VALIDATE};

    /// No Validate stage: a submit goes straight to Review.
    pub fn without_validate(file: &mut FlowFile) {
        file.flow.stages.retain(|s| s.id != VALIDATE);
    }

    /// Validate marked skippable (`options: [skip]`): Skip this check and Move on anyway are offered on it.
    pub fn skippable_validate(file: &mut FlowFile) {
        for stage in file.flow.stages.iter_mut().filter(|s| s.id == VALIDATE) {
            if !stage.options.contains(&StageOption::Skip) {
                stage.options.push(StageOption::Skip);
            }
        }
    }

    /// Validate with `rounds` rounds.
    pub fn with_rounds(rounds: u32) -> impl FnOnce(&mut FlowFile) {
        move |file| {
            for stage in file.flow.stages.iter_mut().filter(|s| s.id == VALIDATE) {
                stage.rounds = Some(rounds);
            }
        }
    }

    /// Dev → Validate, then a local merge with no click (`mode: automatic`, `delivers: local`) right after
    /// Validate's pass: what Auto-merge's "All tickets" was (d0108). No Review.
    pub fn automatic_local_delivery(file: &mut FlowFile) {
        file.flow.stages.retain(|s| s.id != REVIEW);
        let delivery = &mut file.flow.delivery;
        delivery.mode = DeliveryMode::Automatic;
        delivery.delivers = Delivers::Local;
        // A Local merge runs no agent.
        delivery.agent = None;
        delivery.skill = None;
    }

    /// The flow delivers by `delivers`, in its mode (d0121's tests: a PR flow still needs the remote).
    pub fn delivering(delivers: Delivers) -> impl FnOnce(&mut FlowFile) {
        move |file| {
            let delivery = &mut file.flow.delivery;
            delivery.delivers = delivers;
            if delivers == Delivers::Local {
                delivery.agent = None;
                delivery.skill = None;
            }
        }
    }

    /// Dev → Validate, then a PR opened with no click and no dialog (`mode: automatic`, `delivers: pr`) by the
    /// Developer, Dev's agent, so it resumes Dev's session.
    pub fn automatic_pr_delivery(file: &mut FlowFile) {
        file.flow.stages.retain(|s| s.id != REVIEW);
        let delivery = &mut file.flow.delivery;
        delivery.mode = DeliveryMode::Automatic;
        delivery.delivers = Delivers::Pr;
    }
}

//! The default flow (STG-03, FLW-01, d0108, d0004): Dev → Validate → Review → Done, Validate in its own column and
//! Done in Review's (its Approved section). Otaq ships it as a file, embedded at build time; every project gets it as
//! its first flow version ([`super::store`]).

use std::sync::LazyLock;

use super::model::{Flow, FlowFile};
use super::yaml;
use crate::naming;

/// The default flow's stage ids.
pub const DEV: &str = "dev";
pub const VALIDATE: &str = "validate";
pub const REVIEW: &str = "review";
pub const DELIVER: &str = "deliver";

static DEFAULT: LazyLock<FlowFile> = LazyLock::new(|| {
    yaml::read(naming::DEFAULT_FLOW_YAML).expect("the shipped default flow reads (tested)")
});

/// The shipped default flow file, as read.
pub fn default_file() -> &'static FlowFile {
    &DEFAULT
}

/// The shipped default flow: Dev → Validate → Review → Done.
pub fn default_flow() -> Flow {
    DEFAULT.flow.clone()
}

/// Tests only: changes to the default flow the suites put a project on with
/// [`crate::board::Board::edit_flow_for_tests`].
#[doc(hidden)]
pub mod testing {
    use super::super::model::{Delivers, Delivery, DeliveryMode, FlowFile};
    use super::{DELIVER, REVIEW, VALIDATE};

    /// No Validate stage: a submit goes straight to Review.
    pub fn without_validate(file: &mut FlowFile) {
        file.flow.stages.retain(|s| s.id != VALIDATE);
    }

    /// Validate with `rounds` rounds.
    pub fn with_rounds(rounds: u32) -> impl FnOnce(&mut FlowFile) {
        move |file| {
            for stage in file.flow.stages.iter_mut().filter(|s| s.id == VALIDATE) {
                stage.rounds = Some(rounds);
            }
        }
    }

    /// Dev → Validate → Done, the Delivery merging locally with no click (`mode: automatic`, `delivers: local`) right
    /// after Validate's pass: what Auto-merge's "All tickets" was (d0108). No Review: an epic stops at Review epic.
    pub fn automatic_local_delivery(file: &mut FlowFile) {
        file.flow.stages.retain(|s| s.id != REVIEW);
        for stage in file.flow.stages.iter_mut().filter(|s| s.id == DELIVER) {
            // A Local merge runs no agent.
            stage.agent = None;
            stage.skill = None;
            stage.delivery = Some(Delivery {
                mode: DeliveryMode::Automatic,
                delivers: Delivers::Local,
            });
        }
    }

    /// The Delivery delivers by `delivers`, in its mode (d0121's tests: a PR flow still needs the remote).
    pub fn delivering(delivers: Delivers) -> impl FnOnce(&mut FlowFile) {
        move |file| {
            for stage in file.flow.stages.iter_mut().filter(|s| s.id == DELIVER) {
                if let Some(delivery) = stage.delivery.as_mut() {
                    delivery.delivers = delivers;
                }
            }
        }
    }

    /// Dev → Validate → Done, the Delivery opening a PR with no click and no dialog (`mode: automatic`, `delivers:
    /// pr`) on the Developer, Dev's agent, so it resumes Dev's session.
    pub fn automatic_pr_delivery(file: &mut FlowFile) {
        file.flow.stages.retain(|s| s.id != REVIEW);
        for stage in file.flow.stages.iter_mut().filter(|s| s.id == DELIVER) {
            stage.delivery = Some(Delivery {
                mode: DeliveryMode::Automatic,
                delivers: Delivers::Pr,
            });
        }
    }
}

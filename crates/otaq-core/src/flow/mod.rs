//! A project's flow (d0108, SDD §7.6): the stages a ticket walks between Backlog and Done. `model` holds the types,
//! `yaml` reads and writes flow files (STG-01), `check` holds the rules every flow must follow (STG-02), `default`
//! the shipped default flow (STG-03), `store` each project's applied flow and the ones before it (FLW-01), `engine`
//! the moves between stages (STG-06), `proposal` a proposed flow checked and compared (FLW-03, FLW-04), `apply` what
//! applies when and the wait for a change's span or a quiet board (FLW-05, FLW-06, FED-08), `draft` the flow being
//! edited before Apply (FED-02), `apply_now` Apply now on a waiting change (APN-02…APN-04),
//! `presets` the shipped preset files (FLW-08), `propose` `otaq flow show` and `propose` and the answer to a proposal
//! (FLW-02, FLW-03), `write_skill` a stage's skill the Project agent writes for the draft (FED-06).

pub mod apply;
pub mod apply_now;
pub mod check;
pub mod default;
pub mod draft;
pub mod engine;
pub mod model;
pub mod presets;
pub mod proposal;
pub mod propose;
pub mod store;
pub mod write_skill;
pub mod yaml;

pub use check::{Problem, Rule, check, check_agents, check_skills};
pub use default::{default_file, default_flow};
pub use model::{
    CheckBy, Column, Delivers, Delivery, DeliveryMode, Flow, FlowAgent, FlowFile, Stage, StageKind,
    StageOption,
};
pub use store::{AppliedBy, FlowVersion, NewFlowVersion};

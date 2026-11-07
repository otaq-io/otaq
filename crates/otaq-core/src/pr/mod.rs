//! Pull requests (REV-09…REV-14; d0017, d0041, d0050, d0059): provider detection, the skill's proposal and the
//! agent's PR commands. Otaq never watches PRs: nothing here polls or calls GitHub or Azure DevOps on its own.

pub mod commands;
pub mod comments;
pub mod flow;
pub mod message;
pub mod proposal;
pub mod provider;
pub mod service;
pub mod verify;

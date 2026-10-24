//! The integration tests of `otaq-cli` as one test program (decision 0056, SDD §23.2): one module per file, so the
//! crate is linked once rather than once per file. Run them with `cargo nextest run -p otaq-cli`; one file with
//! `cargo nextest run -p otaq-cli -E 'test(/^<file>::/)'`.

mod agent;
mod ask;
mod ask_each_tool;
mod attach;
mod cli;
mod client_start;
mod comment_path;
mod comments;
mod copilot_hooks;
mod daemon;
mod decision;
mod deps;
mod epic_children;
mod epic_pr;
mod epic_prime;
mod flow;
mod hook;
mod install_local;
mod lead;
mod limit;
mod m24_example;
mod m38_example;
mod notify_check;
mod onboarding;
mod open_pr;
mod open_questions;
mod opencode_hook;
mod pr;
mod prime;
mod project_agent_attention;
mod project_agents;
mod read;
mod skills;
mod stage_choices;
mod stage_path;
mod statusline;
mod terminals;
mod validate;
mod write;

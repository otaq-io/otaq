//! A permission prompt on the sheet's Overview: what the agent waits to be allowed to do, above Answer in terminal.
//! Otaq doesn't answer permission prompts itself (they are answered in the tool's terminal), so the Overview shows
//! the tool's own prompt (the command when it is one) and says where the answer goes.

use otaq_core::agent_tools::Tool;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::signals::HELPER_PROMPT_PREFIX;
use serde::Serialize;
use ts_rs::TS;

/// The tools' prefix for a shell command ("Run command: npm install").
const RUN_COMMAND: &str = "Run command: ";

/// "Codex is waiting for your answer in its terminal" and its prompt: the command (`$ npm install`) or the tool's
/// words ("Claude needs your permission to use Bash").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetPermissionPrompt")]
pub struct PermissionPrompt {
    pub heading: String,
    /// The command, or the tool's prompt as it said it.
    pub text: String,
    /// `text` is a shell command: drawn as code after a `$`.
    pub command: bool,
}

/// The permission prompt of a ticket whose agent (or checker) waits at one, as its latest session's `tool` asked it;
/// `None` for any other ticket, or when the tool gave no prompt text.
pub fn of(ticket: &Ticket, tool: Option<Tool>) -> Option<PermissionPrompt> {
    if !matches!(ticket.status, Status::NeedsYou | Status::CheckerNeedsYou) {
        return None;
    }
    let raw = ticket.signal.as_ref()?.text.as_deref()?.trim();
    let (helper, prompt) = match raw.strip_prefix(HELPER_PROMPT_PREFIX) {
        Some(rest) => (true, rest.trim()),
        None => (false, raw),
    };
    if prompt.is_empty() {
        return None;
    }
    let (text, command) = match prompt.strip_prefix(RUN_COMMAND).map(str::trim) {
        Some(c) if !c.is_empty() => (c, true),
        _ => (prompt, false),
    };
    let who = match (tool, helper) {
        (Some(tool), false) => tool.display_name().to_owned(),
        (Some(tool), true) => format!("A helper agent of {}", tool.display_name()),
        (None, false) => "The agent".to_owned(),
        (None, true) => "A helper agent".to_owned(),
    };
    Some(PermissionPrompt {
        heading: format!("{who} is waiting for your answer in its terminal"),
        text: text.to_owned(),
        command,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::board::tickets::AgentSignal;

    fn at_prompt(status: Status, text: Option<&str>) -> Ticket {
        let mut t = ticket("SHOP-7", "Check total for an empty list", status);
        t.signal = Some(AgentSignal {
            text: text.map(str::to_owned),
            ..AgentSignal::default()
        });
        t
    }

    #[test]
    fn a_command_shows_as_a_command_with_where_to_answer() {
        let t = at_prompt(Status::NeedsYou, Some("Run command: npm install"));
        assert_eq!(
            of(&t, Some(Tool::Codex)),
            Some(PermissionPrompt {
                heading: "Codex is waiting for your answer in its terminal".into(),
                text: "npm install".into(),
                command: true,
            })
        );
    }

    #[test]
    fn another_prompt_shows_the_tools_words() {
        let t = at_prompt(
            Status::NeedsYou,
            Some("Claude needs your permission to use Bash"),
        );
        let p = of(&t, Some(Tool::Claude)).unwrap();
        assert_eq!(p.text, "Claude needs your permission to use Bash");
        assert!(!p.command);
        assert_eq!(
            p.heading,
            "Claude Code is waiting for your answer in its terminal"
        );
    }

    #[test]
    fn a_helpers_prompt_says_a_helper_asks() {
        let t = at_prompt(Status::NeedsYou, Some("helper agent asks: Run command: ls"));
        let p = of(&t, None).unwrap();
        assert_eq!(
            p.heading,
            "A helper agent is waiting for your answer in its terminal"
        );
        assert_eq!(p.text, "ls");
        assert!(p.command);
    }

    #[test]
    fn the_checkers_prompt_shows_too() {
        let t = at_prompt(Status::CheckerNeedsYou, Some("Run command: cargo test"));
        assert_eq!(of(&t, None).unwrap().text, "cargo test");
    }

    #[test]
    fn nothing_without_a_prompt_or_outside_a_permission_wait() {
        assert_eq!(of(&at_prompt(Status::NeedsYou, None), None), None);
        assert_eq!(of(&at_prompt(Status::NeedsYou, Some("  ")), None), None);
        assert_eq!(
            of(&at_prompt(Status::AsksYou, Some("Run command: ls")), None),
            None
        );
        assert_eq!(
            of(&at_prompt(Status::Working, Some("Run command: ls")), None),
            None
        );
    }
}

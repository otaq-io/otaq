//! The run-command dialog (RUN-02; *13 · Run & Shell → 1 · Empty … 5 · Saved*): the first Run in a project asks
//! "What command runs shop-web?" (with an optional Open URL); **Ask the agent** is a click that has the Project
//! agent's named agent suggest one with a short why (d0086); the user confirms or edits it; Save stores it in the
//! project's settings (never in the repo, d0023) and starts the Run. Nothing is saved without Save.

use otaq_core::domain::TicketKey;
use otaq_core::terminals::run::RunInfo;
use otaq_core::terminals::suggest::Suggestion;

/// Where the dialog is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// 1 · Empty: no command yet.
    Empty,
    /// 2 · The agent thinking: what it looked at so far ("Read(package.json)").
    Thinking { lines: Vec<String> },
    /// 3 · Suggestion: the fields hold the agent's answer; the why is shown.
    Suggested { why: String },
    /// 4 · Editing: the user typed.
    Editing,
    /// 5 · Saved: the Run starts.
    Saved { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCommandModel {
    /// The project's key and name.
    pub project: String,
    pub project_name: String,
    /// The named agent Ask the agent asks: the project's Project agent ("Strong").
    pub agent: String,
    /// The ticket whose Run starts after Save, with its Run port.
    pub ticket: Option<(TicketKey, Option<u16>)>,
    pub phase: Phase,
    /// The agent's failure (or a failed save), shown under the fields; the fields stay editable.
    pub error: Option<String>,
    /// An agent tool is connected: Ask the agent is hidden only without one (RUN-02, d0086).
    pub tool_connected: bool,
}

impl RunCommandModel {
    /// The dialog for a project, opened from a ticket's Run (or from project settings without one). `agent` is the
    /// named agent the project's Project agent role resolves to.
    pub fn new(
        project: &str,
        project_name: &str,
        agent: &str,
        run: Option<&RunInfo>,
        current_command: &str,
    ) -> Self {
        Self {
            project: project.to_owned(),
            project_name: project_name.to_owned(),
            agent: agent.to_owned(),
            ticket: run.map(|r| (r.key.clone(), r.port)),
            phase: if current_command.trim().is_empty() {
                Phase::Empty
            } else {
                Phase::Editing
            },
            error: None,
            tool_connected: true,
        }
    }

    /// The dialog with no agent tool connected: no Ask the agent.
    pub fn with_tool_connected(mut self, connected: bool) -> Self {
        self.tool_connected = connected;
        self
    }

    pub fn title(&self) -> String {
        format!("What command runs {}?", self.project_name)
    }

    pub fn thinking(&self) -> bool {
        matches!(self.phase, Phase::Thinking { .. })
    }

    /// Ask the agent is offered while there is no command to keep (empty field, or after a failed suggestion).
    pub fn offers_ask(&self, command: &str) -> bool {
        if !self.tool_connected {
            return false;
        }
        match self.phase {
            Phase::Empty => true,
            Phase::Editing => command.trim().is_empty(),
            Phase::Thinking { .. } | Phase::Suggested { .. } | Phase::Saved { .. } => false,
        }
    }

    /// Ask the agent was clicked: `false` when one is already running.
    pub fn ask(&mut self) -> bool {
        if !self.tool_connected || self.thinking() || matches!(self.phase, Phase::Saved { .. }) {
            return false;
        }
        self.phase = Phase::Thinking { lines: Vec::new() };
        self.error = None;
        true
    }

    /// The agent looked at something.
    pub fn progress(&mut self, line: &str) {
        if let Phase::Thinking { lines } = &mut self.phase {
            lines.push(line.to_owned());
        }
    }

    /// The agent answered: the fields take the suggestion (returned: command and Open URL).
    pub fn suggested(&mut self, suggestion: &Suggestion) -> Option<(String, String)> {
        if !self.thinking() {
            return None;
        }
        self.phase = Phase::Suggested {
            why: suggestion.why.clone(),
        };
        Some((suggestion.command.clone(), suggestion.open_url.clone()))
    }

    /// The agent gave no usable answer: an error, and the fields stay as they were.
    pub fn failed(&mut self, message: &str, command: &str) {
        if !self.thinking() {
            return;
        }
        self.phase = if command.trim().is_empty() {
            Phase::Empty
        } else {
            Phase::Editing
        };
        self.error = Some(message.to_owned());
    }

    /// Cancel while the agent thinks: the process stops and the dialog stays.
    pub fn cancel_ask(&mut self, command: &str) {
        if self.thinking() {
            self.phase = if command.trim().is_empty() {
                Phase::Empty
            } else {
                Phase::Editing
            };
        }
    }

    /// The user typed in a field.
    pub fn edited(&mut self) {
        if matches!(self.phase, Phase::Suggested { .. } | Phase::Empty) {
            self.phase = Phase::Editing;
        }
    }

    /// Save (& start) is enabled with a command, and not while the agent thinks.
    pub fn can_save(&self, command: &str) -> bool {
        !command.trim().is_empty() && !self.thinking() && !matches!(self.phase, Phase::Saved { .. })
    }

    pub fn save_label(&self) -> &'static str {
        if self.ticket.is_some() {
            "Save & start"
        } else {
            "Save"
        }
    }

    /// Saved: "Saved for shop-web. Running SHOP-19 on :4020… Change it later in project settings."
    pub fn saved(&mut self) {
        let running = match &self.ticket {
            Some((key, Some(port))) => format!(" Running {key} on :{port}…"),
            Some((key, None)) => format!(" Running {key}…"),
            None => String::new(),
        };
        self.phase = Phase::Saved {
            message: format!(
                "Saved for {}.{running} Change it later in project settings.",
                self.project_name
            ),
        };
        self.error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::terminals::run::RunState;

    fn run() -> RunInfo {
        RunInfo {
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            key: "SHOP-19".parse().unwrap(),
            title: "Price table".into(),
            state: RunState::NotStarted,
            port: Some(4020),
            command: String::new(),
            open_url: None,
            session: None,
            started_at: None,
            ended_at: None,
            exit_code: None,
            stopped_by: None,
            started_by: None,
            command_set_by: None,
        }
    }

    fn suggestion() -> Suggestion {
        Suggestion {
            command: "pnpm dev --port $PORT".into(),
            open_url: "http://localhost:$PORT".into(),
            why: "package.json has \"dev\": \"next dev\".".into(),
        }
    }

    #[test]
    fn the_five_states() {
        // 1 · Empty
        let mut m = RunCommandModel::new("SHOP", "shop-web", "Strong", Some(&run()), "");
        assert_eq!(m.title(), "What command runs shop-web?");
        assert_eq!(m.phase, Phase::Empty);
        assert!(m.offers_ask(""));
        assert!(!m.can_save(""));
        assert_eq!(m.save_label(), "Save & start");

        // 2 · The agent thinking
        assert!(m.ask());
        assert!(!m.ask(), "one at a time");
        m.progress("Read(package.json)");
        m.progress("Read(next.config.mjs)");
        assert_eq!(
            m.phase,
            Phase::Thinking {
                lines: vec!["Read(package.json)".into(), "Read(next.config.mjs)".into()]
            }
        );
        assert!(!m.can_save("anything"), "not while the agent thinks");
        assert!(!m.offers_ask(""));

        // 3 · Suggestion
        let filled = m.suggested(&suggestion()).unwrap();
        assert_eq!(
            filled,
            (
                "pnpm dev --port $PORT".into(),
                "http://localhost:$PORT".into()
            )
        );
        assert!(matches!(&m.phase, Phase::Suggested { why } if why.starts_with("package.json")));
        assert!(m.can_save("pnpm dev --port $PORT"));

        // 4 · Editing
        m.edited();
        assert_eq!(m.phase, Phase::Editing);
        assert!(m.can_save("pnpm dev --port $PORT --turbo"));
        assert!(!m.can_save("   "));

        // 5 · Saved
        m.saved();
        assert_eq!(
            m.phase,
            Phase::Saved {
                message: "Saved for shop-web. Running SHOP-19 on :4020… Change it later in project settings."
                    .into()
            }
        );
        assert!(!m.can_save("x"), "saved once");
    }

    #[test]
    fn a_failed_suggestion_leaves_the_fields_editable() {
        let mut m = RunCommandModel::new("SHOP", "shop-web", "Strong", Some(&run()), "");
        m.ask();
        m.failed("The agent couldn't suggest a command.", "");
        assert_eq!(m.phase, Phase::Empty);
        assert_eq!(
            m.error.as_deref(),
            Some("The agent couldn't suggest a command.")
        );
        assert!(m.offers_ask(""));
        // A late answer after a failure changes nothing.
        assert_eq!(m.suggested(&suggestion()), None);
        // Asking again clears the error.
        m.ask();
        assert_eq!(m.error, None);
    }

    #[test]
    fn cancel_stops_thinking_and_keeps_the_dialog() {
        let mut m = RunCommandModel::new("SHOP", "shop-web", "Strong", None, "cargo run");
        assert_eq!(m.phase, Phase::Editing, "a command exists already");
        assert_eq!(m.save_label(), "Save");
        assert!(!m.offers_ask("cargo run"));
        m.phase = Phase::Thinking { lines: vec![] };
        m.cancel_ask("cargo run");
        assert_eq!(m.phase, Phase::Editing);
        m.saved();
        assert_eq!(
            m.phase,
            Phase::Saved {
                message: "Saved for shop-web. Change it later in project settings.".into()
            }
        );
    }

    /// RUN-02 (d0086): "Ask the agent" is hidden only when no agent tool is connected.
    #[test]
    fn ask_the_agent_is_hidden_only_with_no_tool() {
        let m = RunCommandModel::new("SHOP", "shop-web", "Strong", None, "");
        assert!(m.offers_ask(""));
        let mut none = m.with_tool_connected(false);
        assert!(!none.offers_ask(""));
        assert!(!none.ask(), "nothing to ask");
        assert_eq!(none.phase, Phase::Empty);
        let v = crate::view_models::run_page::command_view(&none, "", None);
        assert!(v.ask.is_none());
    }
}

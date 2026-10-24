//! What the app shell (header, rail) asks its owner to do, and how it shows paths.

/// What the shell asks its owner to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellRequest {
    NewTicket,
    OpenSettings,
    AddProject,
    /// Run the tools check again (the onboarding's tools step).
    CheckTools,
    /// A project's settings, at its Agent instructions (d0040).
    ProjectInstructions(String),
    /// The current project's Project terminal (Terminals, ⌃`; RUN-06): open it, or focus it when it is open.
    ProjectTerminal,
    /// "What command runs shop-web?" (RUN-02), for a ticket's Run.
    RunCommand {
        project: String,
        run: Option<Box<otaq_core::terminals::run::RunInfo>>,
    },
    /// A needs-you item: switch to its project if needed and open its sheet on this tab (UI-06).
    OpenTicket {
        project: String,
        key: otaq_core::domain::TicketKey,
        tab: crate::view_models::sheet::Content,
    },
    /// Resume all: confirm, then resume the interrupted sessions.
    ResumeAll,
    /// Resume after the usage limit reset (USG-06): "continue" to the waiting agents.
    ResumeAfterLimit,
    /// An answer stopped at unsent text in the terminal (SES-07): ask first.
    Unsent(crate::view_models::terminal_strip::UnsentPrompt),
    /// The Cleanup window (the rail's broom, Settings → Data, the board filter's "cleanup"; REC-04).
    OpenCleanup,
}

/// A path with the home folder shown as `~` (the user profile folder on Windows).
pub fn tilde(path: &str) -> String {
    match std::env::home_dir().map(|h| h.display().to_string()) {
        Some(home) if !home.is_empty() && path.starts_with(&home) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_becomes_a_tilde() {
        let home = std::env::home_dir()
            .map(|h| h.display().to_string())
            .unwrap_or_default();
        if !home.is_empty() {
            assert_eq!(tilde(&format!("{home}/code/shop-web")), "~/code/shop-web");
        }
        assert_eq!(tilde("/opt/repo"), "/opt/repo");
    }
}

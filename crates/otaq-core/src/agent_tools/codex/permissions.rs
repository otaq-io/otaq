//! Codex's permission warning (SET-02, d0011; T-0351): whether the user's own approval settings stop agents on
//! prompts. Read-only; Otaq never changes them.
//!
//! Codex asks before a command leaves its sandbox unless the user's `config.toml` says otherwise:
//! `approval_policy` (`untrusted`, `on-request`, `never`, or a `granular` table; Codex's default asks) at the top
//! level or in the profile `profile = "<name>"` selects, and `approvals_reviewer = "auto_review"` (the
//! `--approve-for-me` route: a reviewing agent answers instead of the user). Values from Codex CLI 0.157.1's
//! `--help` (`-a/--ask-for-approval`) and its app-server schema (`AskForApproval`, `ApprovalsReviewer`).
//! Otaq launches with no approval flag (d0011), so these settings are what its agents get.

use super::config_toml::{self, Approvals};
use super::{CodexContext, DISPLAY_NAME};

pub(super) fn warning(context: &CodexContext) -> Option<String> {
    let home = context.codex_home.as_deref()?;
    let approvals = match config_toml::read(&config_toml::path(home)) {
        Ok(Some(doc)) => config_toml::approvals(&doc),
        Ok(None) => Approvals::default(),
        // Can't be told.
        Err(_) => return None,
    };
    asks(&approvals).then(|| format!("Your {DISPLAY_NAME} settings ask for permission prompts"))
}

/// Whether new sessions stop to ask the user: every policy but `never`, unless a reviewing agent answers.
pub(super) fn asks(approvals: &Approvals) -> bool {
    let reviewed = matches!(
        approvals.reviewer.as_deref(),
        Some("auto_review" | "guardian_subagent")
    );
    approvals.policy.as_deref() != Some("never") && !reviewed
}

#[cfg(test)]
mod tests {
    use super::super::config_toml::tests::USER_CONFIG;
    use super::*;

    fn approvals(policy: Option<&str>, reviewer: Option<&str>) -> Approvals {
        Approvals {
            policy: policy.map(str::to_owned),
            reviewer: reviewer.map(str::to_owned),
        }
    }

    #[test]
    fn only_never_or_a_reviewing_agent_stops_no_agent() {
        assert!(asks(&approvals(None, None)), "Codex's default asks");
        assert!(asks(&approvals(Some("on-request"), None)));
        assert!(asks(&approvals(Some("untrusted"), None)));
        assert!(asks(&approvals(Some("granular"), None)));
        assert!(asks(&approvals(Some("on-request"), Some("user"))));
        assert!(!asks(&approvals(Some("never"), None)));
        assert!(!asks(&approvals(Some("on-request"), Some("auto_review"))));
    }

    fn context(home: &std::path::Path) -> CodexContext {
        CodexContext {
            tool: crate::agent_tools::test_support::context(home, None),
            codex_home: Some(home.to_path_buf()),
            held_back: false,
        }
    }

    #[test]
    fn the_warning_reads_the_users_config() {
        let home = tempfile::tempdir().unwrap();
        let file = config_toml::path(home.path());
        let warning = Some("Your Codex settings ask for permission prompts".to_owned());
        assert_eq!(super::warning(&context(home.path())), warning, "no config");
        std::fs::write(&file, USER_CONFIG).unwrap();
        assert_eq!(super::warning(&context(home.path())), warning);
        std::fs::write(&file, format!("profile = \"fast\"\n{USER_CONFIG}")).unwrap();
        assert_eq!(
            super::warning(&context(home.path())),
            None,
            "the profile says never"
        );
        std::fs::write(&file, "approval_policy = \n").unwrap();
        assert_eq!(super::warning(&context(home.path())), None, "can't be told");
        let none = CodexContext {
            codex_home: None,
            ..context(home.path())
        };
        assert_eq!(super::warning(&none), None);
    }
}

//! Detect (ADP-10): `codex` found on PATH and its version (T-0347); signed in from `codex login status` (T-0351).
//!
//! `codex login status` makes no network call and prints one line (on stderr, Codex CLI 0.157.1): "Logged in using
//! ChatGPT", "Logged in using an API key - sk-…" (the key masked), "Logged in using access token" and the like, or
//! "Not logged in" (exit 1). The masked key is never kept.

use std::process::Stdio;
use std::time::Duration;

use super::{COMMAND_NAME, CodexContext};
use crate::agent_tools::{Detection, version_output};

/// How long `codex login status` may take.
const STATUS_TIMEOUT: Duration = Duration::from_secs(15);

/// Installed when `codex --version` names a version; signed in as [`signed_in`] says.
pub(super) async fn detect(context: &CodexContext) -> Detection {
    let Some(output) = version_output(COMMAND_NAME, &context.tool.path).await else {
        return Detection::default();
    };
    let Some(version) = output.as_deref().and_then(parse_version) else {
        return Detection::default();
    };
    let (signed_in, account) = signed_in(context).await;
    Detection {
        installed: true,
        signed_in,
        version: Some(version),
        update_needed: false,
        account,
        sign_in_offered: false,
    }
}

/// `codex-cli 0.157.1` → `0.157.1`.
pub(super) fn parse_version(output: &str) -> Option<String> {
    let version = output
        .lines()
        .next()?
        .trim()
        .strip_prefix("codex-cli ")?
        .trim();
    version
        .starts_with(|c: char| c.is_ascii_digit())
        .then(|| version.to_owned())
}

/// Whether Codex is signed in, and how (the account line: "signed in with ChatGPT"), from `codex login status`
/// with Codex's home as the service sees it. `None` when it can't be told (it failed or printed something else).
pub(super) async fn signed_in(context: &CodexContext) -> (Option<bool>, Option<String>) {
    let Some(program) = crate::tools::find_on_path(COMMAND_NAME, &context.tool.path) else {
        return (None, None);
    };
    let Ok(launch) = crate::tools::cmd_shim::command(&program, &context.tool.path) else {
        return (None, None);
    };
    let mut command = tokio::process::Command::from(launch);
    command
        .args(["login", "status"])
        .env("PATH", &context.tool.path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(home) = &context.codex_home {
        command.env("CODEX_HOME", home);
    }
    let Ok(Ok(output)) = tokio::time::timeout(STATUS_TIMEOUT, command.output()).await else {
        return (None, None);
    };
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    tracing::debug!(output = %text.trim(), "codex login status");
    parse_login_status(&text)
}

/// "Logged in using ChatGPT" → signed in, "signed in with ChatGPT"; "Logged in using an API key - sk-…" → "signed
/// in with an API key"; "Not logged in" → not signed in; anything else → not known.
pub(super) fn parse_login_status(output: &str) -> (Option<bool>, Option<String>) {
    for line in output.lines().map(str::trim) {
        if line.eq_ignore_ascii_case("Not logged in") {
            return (Some(false), None);
        }
        if let Some(how) = line.strip_prefix("Logged in using ") {
            let how = how.split(" - ").next().unwrap_or(how).trim();
            let account = (!how.is_empty()).then(|| format!("signed in with {how}"));
            return (Some(true), account);
        }
    }
    (None, None)
}

/// **Sign in…**: `codex login` (d0086).
pub(super) fn login_command() -> Option<Vec<String>> {
    Some(vec![COMMAND_NAME.to_owned(), "login".to_owned()])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Logged in using ChatGPT" is the CLI probe's (2026-10-17, signed in); "Not logged in" was seen signed out in
    /// a temporary `CODEX_HOME` (0.157.1, on stderr, exit 1); the API-key line is the binary's text.
    #[test]
    fn login_status_outputs() {
        assert_eq!(
            parse_login_status("Logged in using ChatGPT\n"),
            (Some(true), Some("signed in with ChatGPT".into()))
        );
        assert_eq!(
            parse_login_status("\nLogged in using an API key - sk-proj-***ABCD\n"),
            (Some(true), Some("signed in with an API key".into()))
        );
        assert_eq!(
            parse_login_status("Logged in using access token"),
            (Some(true), Some("signed in with access token".into()))
        );
        assert_eq!(parse_login_status("\nNot logged in\n"), (Some(false), None));
        assert_eq!(parse_login_status("codex-cli 0.157.1\n"), (None, None));
        assert_eq!(parse_login_status(""), (None, None));
    }

    /// Detect runs `codex login status` on the context's PATH (a stand-in that prints what Codex prints).
    #[tokio::test]
    async fn detect_reads_the_login_status() {
        let bin = tempfile::tempdir().unwrap();
        let context = CodexContext {
            tool: crate::agent_tools::test_support::context(bin.path(), None),
            codex_home: Some(bin.path().join("codex-home")),
        };
        let stub = |status: &str| {
            crate::process::testing::stub_command(
                bin.path(),
                COMMAND_NAME,
                &format!(
                    r#"case "$1" in --version) echo "codex-cli 0.157.1";; login) [ -n "$CODEX_HOME" ] && {status};; esac"#
                ),
            )
        };
        stub("echo 'Logged in using ChatGPT'");
        let found = detect(&context).await;
        assert_eq!(found.signed_in, Some(true));
        assert_eq!(found.account.as_deref(), Some("signed in with ChatGPT"));
        stub("{ echo 'Not logged in' >&2; exit 1; }");
        let found = detect(&context).await;
        assert!(found.installed);
        assert_eq!((found.signed_in, found.account), (Some(false), None));
    }
}

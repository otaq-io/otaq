//! Whether Claude Code is signed in, from its own answer: `claude auth status --json` (Claude Code 2.1.x).
//!
//! It prints one JSON object and makes no model call: `"loggedIn": true` (exit 0) with how it is signed in, or
//! `"loggedIn": false` and `"authMethod": "none"` (exit 1) when it isn't, which is also what an expired login gives
//! (seen in the Windows release test: the Tools step said signed in, the first ticket then printed "Login expired").
//! Only `loggedIn` is read; the account fields (email, organisation) are never kept. Anything else (the command
//! failed, took too long, printed no such object, an older Claude Code without `auth status`) is no answer: the tool
//! shows as "couldn't check", never as signed in.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;

/// How long `claude auth status` may take (as `--version`).
const STATUS_TIMEOUT: Duration = Duration::from_secs(15);

/// The arguments of the status command.
pub(super) const STATUS_ARGS: [&str; 3] = ["auth", "status", "--json"];

/// The arguments of **Sign in…**'s command, which signs in from the terminal (a browser page, or a code to paste).
pub(super) const LOGIN_ARGS: [&str; 2] = ["auth", "login"];

/// Runs `program auth status --json` on `path` and reads its answer; `None` when there is none to read.
pub(super) async fn signed_in(program: &Path, path: &OsStr) -> Option<bool> {
    let launch = crate::tools::cmd_shim::command(program, path).ok()?;
    let mut command = tokio::process::Command::from(launch);
    command
        .args(STATUS_ARGS)
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let output = match tokio::time::timeout(STATUS_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "can't run claude auth status");
            return None;
        }
        Err(_) => {
            tracing::warn!(
                "claude auth status gave no answer within {}s",
                STATUS_TIMEOUT.as_secs()
            );
            return None;
        }
    };
    let answer = parse_status(&String::from_utf8_lossy(&output.stdout));
    if answer.is_none() {
        tracing::warn!(code = ?output.status.code(), "claude auth status printed no sign-in answer");
    }
    answer
}

/// `loggedIn` from the status command's JSON; `None` when the output isn't an object with a boolean `loggedIn`.
pub(super) fn parse_status(output: &str) -> Option<bool> {
    let value: Value = serde_json::from_str(output.trim()).ok()?;
    value.get("loggedIn")?.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Signed out as Claude Code 2.1.322 prints it (an empty `CLAUDE_CONFIG_DIR`), and signed in with the account
    /// fields replaced.
    #[test]
    fn status_outputs() {
        let signed_out = r#"{
  "loggedIn": false,
  "authMethod": "none",
  "apiProvider": "firstParty",
  "analyticsDisabled": false,
  "projectsDirectory": "/tmp/cc/projects",
  "configDirectory": "/tmp/cc",
  "loginExpiresAt": null
}
"#;
        assert_eq!(parse_status(signed_out), Some(false));
        let signed_in = r#"{"loggedIn": true, "authMethod": "claude.ai", "apiProvider": "firstParty",
            "email": "me@example.com", "orgName": "Example", "subscriptionType": "max"}"#;
        assert_eq!(parse_status(signed_in), Some(true));
        // No answer: an old Claude Code's error, its version line, nothing, a JSON without the field.
        for output in [
            "error: unknown command 'auth'\n",
            "2.1.4 (Claude Code)\n",
            "",
            r#"{"authMethod": "claude.ai"}"#,
            r#"{"loggedIn": "yes"}"#,
        ] {
            assert_eq!(parse_status(output), None, "{output}");
        }
    }
}

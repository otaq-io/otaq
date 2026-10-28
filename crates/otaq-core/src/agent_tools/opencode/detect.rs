//! Detect (ADP-10): `opencode` found on PATH and its version (T-0347); signed in from `opencode auth list` and Sign
//! in… (T-0355).

use std::time::Duration;

use serde_json::Value;

use super::{COMMAND_NAME, OpenCodeContext};
use crate::agent_tools::{Detection, version_output};

/// Installed when `opencode --version` names a version; signed in as [`signed_in`] says.
pub(super) async fn detect(context: &OpenCodeContext) -> Detection {
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
        sign_in_offered: account.as_deref() == Some(FREE_MODELS_ONLY),
        account,
    }
}

/// `v2.0.17` → `2.0.17`.
pub(super) fn parse_version(output: &str) -> Option<String> {
    let first = output.lines().next()?.trim();
    let version = first.strip_prefix('v').unwrap_or(first);
    (version.starts_with(|c: char| c.is_ascii_digit()) && !version.contains(char::is_whitespace))
        .then(|| version.to_owned())
}

/// How long `opencode auth list` may take (as `--version`).
const AUTH_TIMEOUT: Duration = Duration::from_secs(15);

/// The account line when no provider is signed in: OpenCode still runs its free Zen models (Q-K).
pub(super) const FREE_MODELS_ONLY: &str = "free models only";

/// `opencode auth list --standalone --format json`: the providers it can use, signed in or from an environment
/// variable. `--standalone` so the check never starts OpenCode's shared background service (without it the command
/// leaves `opencode serve --service` running).
fn auth_list_args() -> [&'static str; 5] {
    ["auth", "list", "--standalone", "--format", "json"]
}

/// Whether OpenCode is signed in to a provider, and the account line, from `opencode auth list` (ADP-10). With no
/// provider it still runs the free models, so it counts as Connected ("free models only", Q-K): `None`, never
/// `Some(false)`. `(None, None)` when the list can't be read.
pub(super) async fn signed_in(context: &OpenCodeContext) -> (Option<bool>, Option<String>) {
    let Some(program) = crate::tools::find_on_path(COMMAND_NAME, &context.tool.path) else {
        return (None, None);
    };
    let Ok(launch) = crate::tools::cmd_shim::command(&program, &context.tool.path) else {
        return (None, None);
    };
    let mut run = tokio::process::Command::from(launch);
    run.args(auth_list_args())
        .env("PATH", &context.tool.path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    match tokio::time::timeout(AUTH_TIMEOUT, run.output()).await {
        Ok(Ok(output)) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            tracing::debug!(output = %text, "opencode auth list");
            account(&text)
        }
        _ => (None, None),
    }
}

/// The signed-in state from `opencode auth list --format json`: `[]` → free models only; a provider →
/// "signed in to Anthropic"; several → "signed in to Anthropic and OpenAI".
pub(super) fn account(output: &str) -> (Option<bool>, Option<String>) {
    let Ok(Value::Array(providers)) = serde_json::from_str::<Value>(output.trim()) else {
        return (None, None);
    };
    let names: Vec<&str> = providers
        .iter()
        .filter_map(|p| {
            p.get("name")
                .or_else(|| p.get("id"))
                .and_then(Value::as_str)
        })
        .collect();
    match names.split_last() {
        None => (None, Some(FREE_MODELS_ONLY.to_owned())),
        Some((only, [])) => (Some(true), Some(format!("signed in to {only}"))),
        Some((last, rest)) => (
            Some(true),
            Some(format!("signed in to {} and {last}", rest.join(", "))),
        ),
    }
}

/// **Sign in…**: `opencode auth login --standalone`, which asks for the provider and its key or sign-in in the
/// terminal (`--standalone`: no background service left running).
pub(super) fn login_command(_context: &OpenCodeContext) -> Option<Vec<String>> {
    Some(
        [COMMAND_NAME, "auth", "login", "--standalone"]
            .map(str::to_owned)
            .to_vec(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/opencode/auth/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    /// Captured with OpenCode 2.0.17 in a scratch home: none; a stored key and an environment variable.
    #[test]
    fn auth_list_outputs() {
        assert_eq!(
            account(&fixture("list-none")),
            (None, Some("free models only".into())),
            "Connected all the same (Q-K)"
        );
        assert_eq!(
            account(&fixture("list-two")),
            (Some(true), Some("signed in to Anthropic and OpenAI".into()))
        );
        assert_eq!(
            account(r#"[{"id":"anthropic","name":"Anthropic","connections":[]}]"#),
            (Some(true), Some("signed in to Anthropic".into()))
        );
        assert_eq!(
            account(r#"[{"name":"A"},{"name":"B"},{"name":"C"}]"#)
                .1
                .as_deref(),
            Some("signed in to A, B and C")
        );
        // The plain text form, or anything else: unknown.
        assert_eq!(account("No authenticated integrations\n"), (None, None));
        assert_eq!(
            auth_list_args(),
            ["auth", "list", "--standalone", "--format", "json"]
        );
    }

    #[test]
    fn sign_in() {
        let bin = tempfile::tempdir().unwrap();
        let context =
            OpenCodeContext::new(crate::agent_tools::test_support::context(bin.path(), None));
        assert_eq!(
            login_command(&context).unwrap(),
            ["opencode", "auth", "login", "--standalone"]
        );
    }
}

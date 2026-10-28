//! Detect (ADP-10): `copilot` found on PATH, its version, and "update needed" before [`OLDEST`] (Q-G; T-0347);
//! signed in from its `config.json` or a token variable, and Sign in… (T-0353).
//!
//! Signed in, without a model call: Copilot keeps its token in the OS keychain and the accounts it signed in to in
//! `config.json` (`loggedInUsers`, `lastLoggedInUser`; `logged_in_users`, `last_logged_in_user` as 0.0.x wrote
//! them, which 1.0.98 still reads). A token variable takes precedence over them (`copilot help environment`,
//! 1.0.98: `COPILOT_GITHUB_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`, in that order). An account whose keychain entry is
//! gone still reads as signed in; its first launch then asks to sign in.

use std::ffi::OsString;
use std::path::Path;

use serde_json::{Map, Value};

use super::{COMMAND_NAME, CopilotContext, trust};
use crate::agent_tools::{Detection, version_output};

/// The variables Copilot takes a token from, in its order.
const TOKEN_VARIABLES: [&str; 3] = ["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"];

/// `config.json`'s accounts: the last one signed in to, then every one, each in 1.0.98's and 0.0.x's spelling.
const LAST_USER: [&str; 2] = ["lastLoggedInUser", "last_logged_in_user"];
const USERS: [&str; 2] = ["loggedInUsers", "logged_in_users"];

/// The oldest Copilot Otaq runs: older ones lack the flags and hooks the adapter uses (the Homebrew 0.0.404).
pub(super) const OLDEST: [u32; 3] = [1, 0, 98];

/// Installed when `copilot --version` names a version; older than [`OLDEST`] → update needed (never launched, and
/// not asked whether it is signed in).
pub(super) async fn detect(context: &CopilotContext) -> Detection {
    let Some(output) = version_output(COMMAND_NAME, &context.tool.path).await else {
        return Detection::default();
    };
    let Some(version) = output.as_deref().and_then(parse_version) else {
        return Detection::default();
    };
    if update_needed(&version) {
        return Detection {
            installed: true,
            version: Some(version),
            update_needed: true,
            ..Detection::default()
        };
    }
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

/// `GitHub Copilot CLI 0.0.404.` (then a second line) → `0.0.404`.
pub(super) fn parse_version(output: &str) -> Option<String> {
    let first = output.lines().next()?.trim();
    let version = first.split_whitespace().last()?.trim_end_matches('.');
    numbers(version).map(|_| version.to_owned())
}

/// Whether `version` is older than [`OLDEST`].
pub(super) fn update_needed(version: &str) -> bool {
    numbers(version).is_none_or(|found| found < OLDEST)
}

/// `1.0.98` → `[1, 0, 98]`; a pre-release suffix (`1.0.98-1`) is ignored.
fn numbers(version: &str) -> Option<[u32; 3]> {
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u32>().ok());
    let found = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(found)
}

/// Whether Copilot is signed in, and as whom ("signed in as octo-me"). `None` without Copilot's home folder (tests,
/// parts of the service built on their own): nothing is read.
pub(super) async fn signed_in(context: &CopilotContext) -> (Option<bool>, Option<String>) {
    let Some(home) = context.copilot_home.as_deref() else {
        return (None, None);
    };
    signed_in_from(home, |name| std::env::var_os(name))
}

/// [`signed_in`] from the `config.json` in `home` and the variables `var` answers. An account in the file names the
/// login; else a token variable that is set ("signed in with GH_TOKEN"); else not signed in. A file that can't be
/// read or parsed counts as no account.
pub(super) fn signed_in_from(
    home: &Path,
    var: impl Fn(&str) -> Option<OsString>,
) -> (Option<bool>, Option<String>) {
    let config = std::fs::read_to_string(trust::config_file(home))
        .ok()
        .and_then(|text| trust::parse(&text))
        .map(|(_, config)| config);
    if let Some(login) = config.as_ref().and_then(signed_in_login) {
        return (Some(true), Some(format!("signed in as {login}")));
    }
    let token = TOKEN_VARIABLES
        .into_iter()
        .find(|name| var(name).is_some_and(|value| !value.is_empty()));
    match token {
        Some(name) => (Some(true), Some(format!("signed in with {name}"))),
        None => (Some(false), None),
    }
}

/// The login of the last account signed in to, else of the first account listed.
fn signed_in_login(config: &Map<String, Value>) -> Option<String> {
    let login = |user: &Value| {
        user.get("login")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
    };
    let field = |names: [&str; 2]| names.into_iter().find_map(|name| config.get(name));
    field(LAST_USER).and_then(login).or_else(|| {
        field(USERS)
            .and_then(Value::as_array)
            .and_then(|users| users.iter().find_map(login))
    })
}

/// **Sign in…**: `copilot login` ("Authenticate with Copilot" in `copilot --help`, 1.0.98; its `--acp`
/// `initialize` names the same command as its terminal sign-in).
pub(super) fn login_command(_context: &CopilotContext) -> Option<Vec<String>> {
    Some(vec![COMMAND_NAME.to_owned(), "login".to_owned()])
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    #[cfg(unix)]
    use crate::agent_tools::AgentTool;
    #[cfg(unix)]
    use crate::agent_tools::copilot::Copilot;
    #[cfg(unix)]
    use crate::agent_tools::test_support::{context, stand_in};

    fn fixture(name: &str) -> String {
        let path: PathBuf = [
            env!("CARGO_MANIFEST_DIR"),
            "tests",
            "fixtures",
            "copilot",
            "config",
            name,
        ]
        .iter()
        .collect();
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn no_variables(_: &str) -> Option<OsString> {
        None
    }

    /// The accounts in `config.json`, in 1.0.98's spelling and 0.0.x's, name the login.
    #[test]
    fn signed_in_from_the_config_file() {
        for file in ["config.json", "config-0.0.x.json"] {
            let home = tempfile::tempdir().unwrap();
            std::fs::write(home.path().join("config.json"), fixture(file)).unwrap();
            assert_eq!(
                signed_in_from(home.path(), no_variables),
                (Some(true), Some("signed in as octo-me".into())),
                "{file}"
            );
        }
        // Only the list.
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("config.json"),
            r#"{"loggedInUsers": [{"host": "https://github.com", "login": "octo-work"}]}"#,
        )
        .unwrap();
        assert_eq!(
            signed_in_from(home.path(), no_variables).1.as_deref(),
            Some("signed in as octo-work")
        );
    }

    /// No account (a new home, an empty list, no file, a broken file) and no token: not signed in. A token variable
    /// counts, in Copilot's order; an empty one doesn't.
    #[test]
    fn not_signed_in_without_an_account_or_a_token() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            signed_in_from(home.path(), no_variables),
            (Some(false), None)
        );
        for text in [
            "// User settings belong in settings.json.\n// This file is managed automatically.\n{\n  \"firstLaunchAt\": \"2026-10-17T21:40:12.066Z\"\n}\n",
            r#"{"loggedInUsers": [], "trustedFolders": ["/r"]}"#,
            "{ not json",
        ] {
            std::fs::write(home.path().join("config.json"), text).unwrap();
            assert_eq!(
                signed_in_from(home.path(), no_variables),
                (Some(false), None),
                "{text}"
            );
        }
        let both = |name: &str| {
            matches!(name, "GH_TOKEN" | "GITHUB_TOKEN").then(|| OsString::from("ghp_x"))
        };
        assert_eq!(
            signed_in_from(home.path(), both),
            (Some(true), Some("signed in with GH_TOKEN".into()))
        );
        let empty = |name: &str| (name == "COPILOT_GITHUB_TOKEN").then(OsString::new);
        assert_eq!(signed_in_from(home.path(), empty), (Some(false), None));
    }

    /// Detect reads the home folder's file once the version runs; Sign in… is `copilot login`.
    // Unix only: the stand-in tool is an `sh` script started by name, which Windows can't run (Q598).
    #[cfg(unix)]
    #[tokio::test]
    async fn detect_says_who_is_signed_in_and_how_to_sign_in() {
        let bin = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        stand_in(bin.path(), "copilot", "GitHub Copilot CLI 1.0.98.\n");
        std::fs::write(home.path().join("config.json"), fixture("config.json")).unwrap();
        let adapter = Copilot::with_context(CopilotContext {
            tool: context(bin.path(), None),
            copilot_home: Some(home.path().to_path_buf()),
        });
        let found = adapter.detect().await;
        assert_eq!(found.signed_in, Some(true));
        assert_eq!(found.account.as_deref(), Some("signed in as octo-me"));
        assert_eq!(
            adapter.login_command(),
            Some(vec!["copilot".to_owned(), "login".to_owned()])
        );

        // No home folder known: not asked.
        let none = Copilot::new(context(bin.path(), None));
        assert_eq!(none.detect().await.signed_in, None);
    }
}

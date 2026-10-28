//! The Codex skeleton (T-0347): names, input quirks, skills, detect on a stand-in program. Each lane adds its tests
//! beside its own file.

use std::path::Path;
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::agent_tools::test_support::context;
#[cfg(unix)]
use crate::agent_tools::test_support::stand_in;

fn codex(bin: &Path, home: Option<&Path>) -> Codex {
    Codex::new(context(bin, home))
}

#[test]
fn names() {
    let bin = tempfile::tempdir().unwrap();
    let adapter = codex(bin.path(), None);
    assert_eq!(adapter.tool(), Tool::Codex);
    assert_eq!(adapter.display_name(), "Codex");
    assert_eq!(adapter.short_name(), "Codex");
    assert_eq!(adapter.command_name(), "codex");
    assert_eq!(adapter.resume_hint("019a1b2c"), "codex resume 019a1b2c");
    assert_eq!(
        adapter.login_command(),
        Some(vec!["codex".to_owned(), "login".to_owned()])
    );
    assert_eq!(
        adapter.install_command(Os::Mac),
        "npm install -g @openai/codex"
    );
}

/// ADP-06: Enter 100 ms after the text (its paste detection); only the current line of a draft clears.
#[test]
fn input_quirks() {
    let bin = tempfile::tempdir().unwrap();
    let adapter = codex(bin.path(), None);
    let quirks = adapter.input();
    assert_eq!(quirks.clear_line, b"\x05\x15");
    assert_eq!(quirks.enter_delay, Duration::from_millis(100));
    assert!(!quirks.multi_line_clearable);
    assert!(!adapter.typed_key_answers_prompt());
    assert!(!adapter.turn_end_reports_background());
    assert!(!adapter.swaps_refused_model());
}

/// AGM-07: a skill by its copy's name (Codex mentions a skill by `$<name>`), the same for a shipped and an imported one; no plugin namespace.
#[test]
fn skills_by_their_copys_name() {
    let bin = tempfile::tempdir().unwrap();
    let adapter = codex(bin.path(), None);
    assert_eq!(
        adapter.skill_reference("commit"),
        format!("${}-commit", crate::naming::COMMAND_NAME)
    );
    assert_eq!(
        adapter.skill_reference("team-validate"),
        format!("${}-team-validate", crate::naming::COMMAND_NAME)
    );
}

/// Detect runs the program on the context's PATH (here a stand-in that prints what `codex --version` prints).
// Unix only: the stand-in tool is an `sh` script started by name, which Windows can't run (Q598).
#[cfg(unix)]
#[tokio::test]
async fn detect_reads_the_version_from_the_path() {
    let bin = tempfile::tempdir().unwrap();
    let adapter = codex(bin.path(), None);
    assert_eq!(adapter.detect().await, Detection::default());

    stand_in(bin.path(), "codex", "codex-cli 0.157.1\n");
    let found = adapter.detect().await;
    assert!(found.installed);
    assert_eq!(found.version.as_deref(), Some("0.157.1"));
    assert!(!found.update_needed);
    assert_eq!(
        found.signed_in, None,
        "its login status says nothing it knows"
    );

    stand_in(bin.path(), "codex", "something else\n");
    assert_eq!(adapter.detect().await, Detection::default());
}

/// A saved Codex payload, `tests/fixtures/codex/<name>`.
pub(super) fn fixture(name: &str) -> serde_json::Value {
    serde_json::from_str(&session_lines(&format!("../{name}"))).unwrap()
}

/// A saved session file, `tests/fixtures/codex/session_file/<name>`.
pub(super) fn session_lines(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/codex/session_file/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Hooks reach the adapter's mapping ([`hooks`]); one it doesn't know is unrecognised.
#[test]
fn an_unknown_hook_is_unrecognised() {
    let bin = tempfile::tempdir().unwrap();
    let adapter = codex(bin.path(), None);
    assert_eq!(
        adapter.map_hook(
            "SubagentStart",
            &json!({"session_id": "s"}),
            SignalContext::default()
        ),
        [ToolEvent::Unrecognised {
            event: "SubagentStart".into()
        }]
    );
    assert_eq!(adapter.windows_source(), WindowsSource::ToolFiles);
    assert_eq!(adapter.watched_file("s", Path::new("/r")), None, "no home");
}

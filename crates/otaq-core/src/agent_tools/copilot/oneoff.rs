//! "Ask the agent" (RUN-02; d0086, d0088; T-0353): a one-off
//!
//! ```text
//! copilot -p "<prompt>" [--model <model> | --model auto --auto-tier <tier>] --available-tools view glob grep rg
//!         --deny-tool shell --deny-tool write --no-ask-user --output-format json --no-auto-update
//! ```
//!
//! in the project's folder. `-p` answers once and exits. `--available-tools` leaves the model only Copilot's reading
//! tools (`view`, `glob`, `grep`, `rg`: its own "read" and "search" kinds), which ask no permission inside the
//! folder; `--deny-tool shell` and `--deny-tool write` refuse commands and edits even so (a denial wins over every
//! allow); `--no-ask-user` leaves out its question tool, since nobody answers. `--output-format json` prints its
//! session events as JSON lines. `--no-auto-update`: Copilot doesn't update itself ([`super::NO_AUTO_UPDATE`]).
//! Never an approval or bypass flag (d0011): no `--allow-all-tools`, `--allow-all`, `--yolo` or `COPILOT_ALLOW_ALL`. Checked with Copilot 1.0.98 on 2026-10-18 against a local stand-in model
//! (`COPILOT_PROVIDER_BASE_URL`, scratch `COPILOT_HOME`): the model was offered `view`, `grep` and `glob` only.
//!
//! Copilot keeps the question's session in its `session-state` (there is no option to keep none); nothing resumes
//! it.

use serde_json::Value;

use super::{COMMAND_NAME, CopilotContext, NO_AUTO_UPDATE, launch};
use crate::agent_tools::{LaunchError, OneOffCommand, OneOffLine, OneOffRequest, OneOffTool};

/// Copilot's reading tools, the only ones the question sees.
const READING_TOOLS: [&str; 4] = ["view", "glob", "grep", "rg"];

/// What the question may never do, whatever the user's settings allow.
const DENIED: [&str; 2] = ["shell", "write"];

pub(super) fn command(
    _context: &CopilotContext,
    request: &OneOffRequest,
) -> Result<OneOffCommand, LaunchError> {
    let mut args = vec!["-p".to_owned(), request.prompt.clone()];
    args.extend(launch::model_flags(request.model.as_deref(), false));
    args.push("--available-tools".to_owned());
    args.extend(READING_TOOLS.map(str::to_owned));
    for denied in DENIED {
        args.extend(["--deny-tool".to_owned(), denied.to_owned()]);
    }
    args.extend(["--no-ask-user", "--output-format", "json", NO_AUTO_UPDATE].map(str::to_owned));
    Ok(OneOffCommand {
        program: COMMAND_NAME.to_owned(),
        args,
        cwd: request.cwd.clone(),
        env: Vec::new(),
    })
}

/// A tool it starts becomes progress (`view(package.json)`); each message with text is an answer (the last one
/// counts); `session.error` is the error.
pub(super) fn read_line(line: &str) -> Option<OneOffLine> {
    tracing::debug!(line, "a line of the one-off question's output");
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    let data = value.get("data");
    let field = |name: &str| data?.get(name).and_then(Value::as_str);
    match value.get("type")?.as_str()? {
        "tool.execution_start" => {
            let arguments = data?.get("arguments");
            let target = ["path", "pattern", "command"]
                .into_iter()
                .find_map(|key| arguments?.get(key)?.as_str())
                .map(str::to_owned);
            Some(OneOffLine::Tools(vec![OneOffTool {
                name: field("toolName")?.to_owned(),
                target,
            }]))
        }
        "assistant.message" => {
            let text = field("content")?.trim();
            (!text.is_empty()).then(|| OneOffLine::Answer {
                text: text.to_owned(),
                is_error: false,
            })
        }
        "session.error" => Some(OneOffLine::Answer {
            text: field("message").unwrap_or("").to_owned(),
            is_error: true,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::agent_tools::AgentTool;
    use crate::agent_tools::copilot::Copilot;
    use crate::agent_tools::test_support::context;

    fn request(model: Option<&str>) -> OneOffRequest {
        OneOffRequest {
            cwd: "/Users/me/shop-web".into(),
            prompt: "Suggest a run command".into(),
            model: model.map(str::to_owned),
        }
    }

    fn adapter() -> Copilot {
        Copilot::new(context(std::path::Path::new("/nonexistent"), None))
    }

    fn fixture(name: &str) -> String {
        let path: PathBuf = [
            env!("CARGO_MANIFEST_DIR"),
            "tests",
            "fixtures",
            "copilot",
            "oneoff",
            name,
        ]
        .iter()
        .collect();
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// The exact arguments: reading tools only, shell and writes denied, the model as launched; never an approval
    /// or bypass flag.
    #[test]
    fn the_question_reads_only_on_the_model_as_launched() {
        let tail = [
            "--available-tools",
            "view",
            "glob",
            "grep",
            "rg",
            "--deny-tool",
            "shell",
            "--deny-tool",
            "write",
            "--no-ask-user",
            "--output-format",
            "json",
            "--no-auto-update",
        ];
        let plain = adapter().one_off(&request(None)).unwrap();
        assert_eq!(plain.program, "copilot");
        assert_eq!(plain.cwd, std::path::Path::new("/Users/me/shop-web"));
        assert_eq!(plain.args[..2], ["-p", "Suggest a run command"]);
        assert_eq!(plain.args[2..], tail);

        let exact = adapter().one_off(&request(Some("gpt-5.5"))).unwrap();
        assert_eq!(exact.args[2..4], ["--model", "gpt-5.5"]);
        assert_eq!(exact.args[4..], tail);

        let auto = adapter()
            .one_off(&request(Some("auto@intelligence")))
            .unwrap();
        assert_eq!(
            auto.args[2..6],
            ["--model", "auto", "--auto-tier", "intelligence"]
        );
        for args in [&plain.args, &exact.args, &auto.args] {
            for flag in [
                "--allow-all-tools",
                "--allow-all",
                "--yolo",
                "--allow-tool",
                "-i",
            ] {
                assert!(!args.iter().any(|a| a == flag), "{flag}");
            }
        }
    }

    /// The captured run (a stand-in model reading `package.json`): the tool is progress, the final message the
    /// answer; every other line is nothing.
    #[test]
    fn the_tool_is_progress_and_the_message_the_answer() {
        let adapter = adapter();
        let read: Vec<OneOffLine> = fixture("answer.jsonl")
            .lines()
            .filter_map(|line| adapter.one_off_line(line))
            .collect();
        assert_eq!(
            read,
            [
                OneOffLine::Tools(vec![OneOffTool {
                    name: "view".into(),
                    target: Some("/Users/me/shop-web/package.json".into())
                }]),
                OneOffLine::Answer {
                    text: "Run `npm run dev -- --port $PORT`.".into(),
                    is_error: false
                },
            ]
        );
        assert_eq!(adapter.one_off_line("not json"), None);
        let grep = r#"{"type":"tool.execution_start","data":{"toolCallId":"c","toolName":"glob","arguments":{"pattern":"**/*.csproj"}}}"#;
        assert_eq!(
            adapter.one_off_line(grep),
            Some(OneOffLine::Tools(vec![OneOffTool {
                name: "glob".into(),
                target: Some("**/*.csproj".into())
            }]))
        );
    }

    /// The captured failure (its model unreachable): `session.error` is the error, with Copilot's message.
    #[test]
    fn a_session_error_is_the_error() {
        let adapter = adapter();
        let read: Vec<OneOffLine> = fixture("no_provider.jsonl")
            .lines()
            .filter_map(|line| adapter.one_off_line(line))
            .collect();
        assert_eq!(
            read,
            [OneOffLine::Answer {
                text: "Could not connect to local model provider at http://127.0.0.1:9/v1.\n  Check that the service is running and the port is correct."
                    .into(),
                is_error: true
            }]
        );
    }
}

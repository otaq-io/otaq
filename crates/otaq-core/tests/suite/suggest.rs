//! Ask the agent (T-0135, T-0342, RUN-02): only on a click, the Project agent's named agent answers a one-off
//! read-only question in the project's folder and suggests a run command; progress and the outcome arrive as events;
//! Cancel stops it; nothing is saved. An ordinary shell command that prints a fixed stream-json answer stands in for
//! the tool's program.

use std::path::PathBuf;
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::AgentModel;
use otaq_core::board::Refusal;
use otaq_core::protocol::Event;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::terminals::suggest::{SuggestUpdate, Suggestion};

/// A stand-in for the tool that prints `lines` (stream-json) and exits with `code`.
fn stand_in(lines: &[&str], code: i32) -> Vec<String> {
    let mut script = String::new();
    for line in lines {
        script.push_str(&format!("printf '%s\\n' '{line}'; "));
    }
    script.push_str(&format!("echo 'stand-in done' >&2; exit {code}"));
    vec!["sh".into(), "-c".into(), script, "stand-in".into()]
}

/// The service with `program` standing in for the tool. No tool is found on PATH, so no starter entry is made.
fn with_program(program: Vec<String>) -> impl FnOnce(ServiceConfig) -> ServiceConfig {
    move |mut c| {
        c.launch = Some(LaunchContext {
            bin_dir: PathBuf::from("/nonexistent"),
            plugin_dir: PathBuf::from("/nonexistent/plugin"),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program,
                config_dir: None,
            },
        });
        c.tools.path = Default::default();
        c
    }
}

async fn updates(
    receiver: &mut tokio::sync::broadcast::Receiver<Event>,
    project: &str,
) -> Vec<SuggestUpdate> {
    let mut all = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(Event::RunSuggestion { project: p, update }) = receiver.recv().await
                && p == project
            {
                let last = !matches!(update, SuggestUpdate::Progress { .. });
                all.push(update);
                if last {
                    return;
                }
            }
        }
    })
    .await
    .expect("the suggestion ends");
    all
}

#[tokio::test(flavor = "multi_thread")]
async fn a_suggestion_with_progress() {
    let tool = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"package.json"}}]}}"#;
    let result = r#"{"type":"result","is_error":false,"result":"{\"command\": \"pnpm dev --port $PORT\", \"open_url\": \"http://localhost:$PORT\", \"why\": \"package.json has dev.\"}"}"#;
    let w = TestService::start_with(with_program(stand_in(&[tool, result], 0))).await;
    w.add_project("shop-web").await;
    let mut listener = w.events.listen();
    w.suggester.start("SHOP").await.unwrap();
    let got = updates(&mut listener.receiver, "SHOP").await;
    assert_eq!(
        got,
        [
            SuggestUpdate::Progress {
                line: "Read(package.json)".into()
            },
            SuggestUpdate::Done {
                suggestion: Suggestion {
                    command: "pnpm dev --port $PORT".into(),
                    open_url: "http://localhost:$PORT".into(),
                    why: "package.json has dev.".into(),
                }
            }
        ]
    );
    // Nothing was saved: the project still has no run command.
    let settings = w.board.project_settings("SHOP").await.unwrap();
    assert!(settings.run_command.is_empty());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn no_answer_is_a_failure() {
    let w = TestService::start_with(with_program(stand_in(&[], 3))).await;
    w.add_project("shop-web").await;
    let mut listener = w.events.listen();
    w.suggester.start("SHOP").await.unwrap();
    let got = updates(&mut listener.receiver, "SHOP").await;
    assert_eq!(
        got,
        [SuggestUpdate::Failed {
            message: "The agent stopped without a suggestion (exit 3): stand-in done.".into()
        }]
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_stops_it_and_without_a_tool_it_is_refused() {
    let dir = tempfile::TempDir::new().unwrap();
    let pid_file = dir.path().join("suggest.pid");
    let script = format!("echo $$ > '{}'; exec sleep 30", pid_file.display());
    let w = TestService::start_with(with_program(vec![
        "sh".into(),
        "-c".into(),
        script,
        "stand-in".into(),
    ]))
    .await;
    w.add_project("shop-web").await;
    let mut listener = w.events.listen();
    w.suggester.start("SHOP").await.unwrap();
    let pid: i32 = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&pid_file)
                && let Ok(pid) = text.trim().parse()
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(w.suggester.cancel("SHOP"));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !u32::try_from(pid).is_ok_and(otaq_core::process::alive),
        "the process ended"
    );
    assert!(
        listener.receiver.try_recv().is_err(),
        "a cancelled suggestion says nothing"
    );
    assert!(!w.suggester.cancel("SHOP"), "nothing left to cancel");
    w.stop().await;

    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let err = w.suggester.start("SHOP").await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NotAvailableYet));
    w.stop().await;
}

/// The arguments the question ran with, one per line, as a stand-in wrote them.
async fn ran_with(file: &std::path::Path) -> Vec<String> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(file)
                && text.ends_with("--end--\n")
            {
                return text.lines().map(str::to_owned).collect::<Vec<_>>();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the stand-in ran")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_question_runs_on_the_project_agent_roles_agent() {
    let dir = tempfile::TempDir::new().unwrap();
    let args_file = dir.path().join("args");
    let script = format!(
        "for a in \"$@\"; do printf '%s\\n' \"$a\" | head -n 1 >> '{0}'; done; echo --end-- >> '{0}'",
        args_file.display()
    );
    let w = TestService::start_with(with_program(vec![
        "sh".into(),
        "-c".into(),
        script,
        "stand-in".into(),
    ]))
    .await;
    w.add_project("shop-web").await;
    let args = |model: Option<&str>| {
        let mut args: Vec<String> = [
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--tools",
            "Read,Glob,Grep",
        ]
        .map(str::to_owned)
        .to_vec();
        if let Some(model) = model {
            args.extend(["--model".to_owned(), model.to_owned()]);
        }
        // The prompt's first line, then the stand-in's end mark.
        args.push("You are helping a developer set up one setting in a tool that runs their project for them. Look at the repository in".to_owned());
        args.push("--end--".to_owned());
        args
    };

    // Noyan on `default` and no default model yet: the new-session tool with its default model, no model option.
    w.suggester.start("SHOP").await.unwrap();
    assert_eq!(ran_with(&args_file).await, args(None));

    // The Project agent role's agent on `opus`: `--model opus` (AGM-05).
    w.set_agent_model(
        "Noyan",
        AgentModel::Tool {
            tool: Tool::ALL[0],
            model: Some("opus".into()),
        },
    )
    .await;
    std::fs::remove_file(&args_file).unwrap();
    w.suggester.start("SHOP").await.unwrap();
    assert_eq!(ran_with(&args_file).await, args(Some("opus")));
    w.stop().await;
}

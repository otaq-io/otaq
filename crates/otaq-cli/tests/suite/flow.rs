//! `otaq flow show`, `presets` and `propose` (T-0414; FLW-02, FLW-03; d0108): the shown flow proposes back as "the
//! same as your current flow"; a broken file prints its problems and exits 1; `--from` proposes another project's
//! stages; only a Project agent and the user may propose; the user's Apply, Cancel and Ask for more changes reach the
//! proposing Project agent's session as its next message. `claude` stands in as an ordinary `sh` started the way a
//! Project agent is; the repositories are temporary. No test starts claude.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::tickets::NewTicketInput;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Actor, Kind};
use otaq_core::flow::apply::ApplyParams;
use otaq_core::flow::default::testing::without_validate;
use otaq_core::flow::presets::presets;
use otaq_core::flow::proposal::{ApplyChoices, SAME};
use otaq_core::flow::propose::{AskForChangesParams, ONLY_A_PROJECT_AGENT_OR_YOU};
use otaq_core::flow::{FlowFile, check, yaml};
use otaq_core::git::testing::TempRepos;
use otaq_core::naming;
use otaq_core::protocol::{ProjectParams, Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::SessionKind;
use serde_json::Value;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

/// pocket-api and pocket-notes, both on the default flow; `claude` is an interactive
/// `sh` in the data folder.
async fn world() -> TestService {
    let home = tempfile::TempDir::new().unwrap();
    let data = home.path().join("h");
    let script = format!(
        "export {}='{}'; export PS1=''; exec sh",
        DataDir::env_var(),
        data.display()
    );
    let w = TestService::start_in(home, TempRepos::new(), |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("pocket-api").await;
    w.add_project("pocket-notes").await;
    w
}

async fn key_of(w: &TestService, name: &str) -> String {
    w.board
        .store()
        .projects()
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.name == name)
        .unwrap()
        .key
}

/// `otaq <args>` from the user's own terminal, in `dir`, with `stdin`.
fn user(w: &TestService, dir: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    use std::io::Write;
    let mut child = Command::new(CB)
        .args(args)
        .env(DataDir::env_var(), w.data.root())
        .env_remove(naming::ticket_env_var())
        .env_remove(naming::role_env_var())
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(stdin.unwrap_or("").as_bytes()).unwrap();
    drop(input);
    child.wait_with_output().unwrap()
}

async fn repo_of(w: &TestService, key: &str) -> PathBuf {
    PathBuf::from(w.board.project(key).await.unwrap().repo_path)
}

/// The project's flow as `flow show` prints it, without Validate.
async fn without_validate_text(w: &TestService, key: &str) -> String {
    let shown = w.board.flow_document(key).await.unwrap();
    let mut file: FlowFile = yaml::read(&shown.yaml).unwrap();
    without_validate(&mut file);
    yaml::write(&file)
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn show_presets_and_propose_from_the_users_terminal() {
    let w = world().await;
    let api = key_of(&w, "pocket-api").await;
    let repo = repo_of(&w, &api).await;

    // `show` prints the applied flow and its agents; proposing it back is the same flow.
    let shown = user(&w, &repo, &["flow", "show"], None);
    assert!(shown.status.success(), "{shown:?}");
    let shown = text(&shown.stdout);
    let file = yaml::read(&shown).unwrap();
    let stages: Vec<&str> = file.flow.stages.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(stages, ["Dev", "Validate", "Review", "Done"]);
    let agents: Vec<&str> = file.agents.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(agents, ["Developer", "Validator"]);
    let same = user(&w, &repo, &["flow", "propose", "--stdin"], Some(&shown));
    assert_eq!(same.status.code(), Some(1), "{same:?}");
    assert_eq!(text(&same.stderr).trim(), format!("error: {SAME}"));
    assert!(
        w.board
            .proposal(&api, &otaq_core::agents::FixedModels::sample())
            .await
            .unwrap()
            .is_none()
    );
    // --json: the document as JSON too.
    let json = user(&w, &repo, &["flow", "show", "--json"], None);
    let json: Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(json["project"], api);
    assert_eq!(json["document"]["flow"][0]["name"], "Dev");
    assert_eq!(
        json["document"]["flow"][1]["agents"],
        serde_json::json!(["Validator"])
    );

    // A check that names where its fails go, and an agent check with two agents (d0131, d0130): shown as set.
    let notes = key_of(&w, "pocket-notes").await;
    let mut file = otaq_core::flow::default_file().clone();
    for stage in &mut file.flow.stages {
        if stage.id == "validate" {
            stage.agents = vec!["Validator".into(), "Developer".into()];
        }
        if stage.id == "review" {
            stage.fail_to = Some("dev".into());
        }
    }
    w.board.use_flow_for_tests(&notes, &file).await;
    let notes_repo = repo_of(&w, &notes).await;
    let shown_notes = user(&w, &notes_repo, &["flow", "show"], None);
    let shown_notes = text(&shown_notes.stdout);
    assert!(
        shown_notes.contains("    agents: [Validator, Developer]\n")
            && shown_notes.contains("    by: you\n    column: own\n    fail_to: Dev\n"),
        "{shown_notes}"
    );

    // `presets`: the four shipped files, one line each.
    let listed = user(&w, &repo, &["flow", "presets"], None);
    assert!(listed.status.success(), "{listed:?}");
    let lines: Vec<String> = text(&listed.stdout).lines().map(str::to_owned).collect();
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(lines.len(), presets().len());
    for (line, preset) in lines.iter().zip(presets()) {
        assert_eq!(
            *line,
            format!(
                "{} \u{b7} {} \u{b7} {}",
                preset.file, preset.name, preset.line
            )
        );
    }
    assert!(
        lines[0].starts_with("otaq-default.yaml \u{b7} "),
        "{lines:?}"
    );
    // --json: each preset's file too, with its agent checks' agents (d0130).
    let json = user(&w, &repo, &["flow", "presets", "--json"], None);
    let json: Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(json[0]["yaml"], naming::DEFAULT_FLOW_YAML);
    assert!(
        json[0]["yaml"]
            .as_str()
            .unwrap()
            .contains("    agents: [Validator]\n"),
        "{json}"
    );

    // A broken file: its problems, exit 1, nothing opens.
    let broken = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../otaq-core/tests/flows/broken/delivery-not-last.yaml");
    let problems = check(
        &yaml::read(&std::fs::read_to_string(&broken).unwrap())
            .unwrap()
            .flow,
    )
    .unwrap_err();
    let refused = user(
        &w,
        &repo,
        &["flow", "propose", "-f", broken.to_str().unwrap()],
        None,
    );
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    for problem in &problems {
        assert!(
            text(&refused.stderr).contains(&problem.message),
            "{problem:?} in {refused:?}"
        );
    }
    // A file that doesn't read.
    let unread = user(
        &w,
        &repo,
        &["flow", "propose", "--stdin"],
        Some("flow: [nope"),
    );
    assert_eq!(unread.status.code(), Some(1), "{unread:?}");
    assert!(
        text(&unread.stderr).contains("The flow doesn't read"),
        "{unread:?}"
    );

    // `--from pocket-notes`: its stages, after pocket-api dropped Validate.
    w.board.edit_flow_for_tests(&api, without_validate).await;
    let from = user(
        &w,
        &repo,
        &["flow", "propose", "--from", "pocket-notes", "--json"],
        None,
    );
    assert!(from.status.success(), "{from:?}");
    let proposal: Value = serde_json::from_slice(&from.stdout).unwrap();
    assert_eq!(proposal["source"]["kind"], "project");
    assert_eq!(proposal["source"]["name"], "pocket-notes");
    let proposed = yaml::read(proposal["yaml"].as_str().unwrap()).unwrap();
    let stages: Vec<&str> = proposed
        .flow
        .stages
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(stages, ["Dev", "Validate", "Review", "Done"]);
    assert!(
        proposal["diff"]["stages"]
            .as_array()
            .is_some_and(|s| !s.is_empty())
    );
    // From the user's terminal, nothing waits for an answer.
    let human = user(
        &w,
        &repo,
        &["flow", "propose", "--from", "pocket-notes"],
        None,
    );
    assert!(human.status.success(), "{human:?}");
    assert!(
        text(&human.stdout)
            .trim()
            .ends_with("is waiting for you in Otaq."),
        "{human:?}"
    );
    w.stop().await;
}

/// A ticket's agent, its check and an epic lead are refused; a Project agent and the user pass.
#[tokio::test(flavor = "multi_thread")]
async fn only_a_project_agent_or_the_user_may_propose() {
    let w = world().await;
    let api = key_of(&w, "pocket-api").await;
    let repo = repo_of(&w, &api).await;
    let proposal = without_validate_text(&w, &api).await;
    let propose = format!("'{CB}' flow propose --stdin --json");
    let refused = |ran: otaq_core::service::testing::Ran| {
        assert_ne!(ran.status, 0, "{ran:?}");
        let e: Value = serde_json::from_str(&ran.stdout).unwrap();
        assert_eq!(e["error"]["message"], ONLY_A_PROJECT_AGENT_OR_YOU, "{e}");
    };

    let ticket = w.create(&api, "Rate limit").await;
    refused(
        w.agent_shell(&ticket)
            .await
            .run_with_stdin(&propose, &proposal)
            .await,
    );
    refused(
        w.shell(&ticket, SessionKind::Validator)
            .await
            .run_with_stdin(&propose, &proposal)
            .await,
    );
    let epic = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: api.clone(),
                title: "Billing".into(),
                kind: Kind::Epic,
                ..Default::default()
            },
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    let lead = w
        .project_agents
        .leads()
        .open(&epic.key.to_string())
        .await
        .unwrap();
    refused(
        w.shell_in(&lead.agent.session.as_ref().unwrap().id)
            .run_with_stdin(&propose, &proposal)
            .await,
    );

    let opened = w
        .project_agents
        .open(&api, "Change the flow")
        .await
        .unwrap();
    let agent = w.shell_in(&opened.session.as_ref().unwrap().id);
    let ran = agent
        .run_with_stdin(&format!("'{CB}' flow propose --stdin"), &proposal)
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let open = w
        .board
        .proposal(&api, &otaq_core::agents::FixedModels::sample())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        ran.stdout.trim(),
        format!(
            "Proposal {} is waiting for the user. End your turn: their answer comes back here.",
            open.id
        )
    );

    let user_ran = user(&w, &repo, &["flow", "propose", "--stdin"], Some(&proposal));
    assert!(user_ran.status.success(), "{user_ran:?}");
    w.stop().await;
}

/// The Project agent's proposal, answered by `answer` through the service as the app sends it; the text that reaches
/// its session.
async fn answered(
    w: &TestService,
    agent: &AgentShell,
    api: &str,
    answer: impl FnOnce(u32) -> Request,
) {
    let proposal = without_validate_text(w, api).await;
    let ran = agent
        .run_with_stdin(&format!("'{CB}' flow propose --stdin --silent"), &proposal)
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let id: u32 = ran.stdout.trim().parse().unwrap();
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let reply = client.request(answer(id)).await.unwrap();
    assert!(!matches!(reply, Reply::FlowProposal(_)), "{reply:?}");
}

async fn reaches(w: &TestService, agent: &AgentShell, needle: &str) {
    // Typed once the agent is idle with an empty input line; Deliver now types it here.
    w.deliveries.deliver_now(&agent.session).await.unwrap();
    w.wait_for_output(&agent.session, needle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_cancel_and_ask_for_more_changes_reach_the_project_agent() {
    let w = world().await;
    let api = key_of(&w, "pocket-api").await;
    let opened = w
        .project_agents
        .open(&api, "Change the flow")
        .await
        .unwrap();
    let agent = w.shell_in(&opened.session.as_ref().unwrap().id);

    answered(&w, &agent, &api, |id| {
        Request::FlowAskForChanges(AskForChangesParams {
            project: api.clone(),
            proposal: id,
            text: "Keep Validate for API tickets".into(),
        })
    })
    .await;
    reaches(
        &w,
        &agent,
        "The user asks for more changes to Proposal 1: Keep Validate for API tickets",
    )
    .await;

    answered(&w, &agent, &api, |_| {
        Request::FlowCancelProposal(ProjectParams {
            project: api.clone(),
        })
    })
    .await;
    reaches(&w, &agent, "The user cancelled Proposal 2.").await;

    answered(&w, &agent, &api, |id| {
        Request::FlowApply(ApplyParams {
            project: api.clone(),
            proposal: id,
            choices: ApplyChoices::default(),
        })
    })
    .await;
    // Dropping Validate is a shape change; nothing is working, so it applies at once.
    reaches(&w, &agent, "The user applied Proposal 3.").await;
    let stages: Vec<String> = w
        .board
        .effective_flow(&api)
        .await
        .unwrap()
        .stages
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(stages, ["Dev", "Review", "Done"]);
    // Each answer went once; nothing else waits for the agent.
    assert!(
        w.deliveries
            .pending(&agent.session)
            .await
            .unwrap()
            .is_empty()
    );
    w.stop().await;
}

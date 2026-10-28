//! Agents told whether the theme is light or dark (T-0483, d0117 point 4): every session gets `COLORFGBG` from the
//! theme in use when it starts, and a program's colour questions (OSC 10 / 11) get exactly one answer: the
//! service's, with the theme's terminal colours, while no terminal that answers shows the session; the terminal's
//! own otherwise. Sessions are ordinary `sh` commands; no agent tool starts.

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::board::settings::{EnvVar, Theme, ThemeSetting};
use otaq_core::service::testing::{TestService, path_without_build_folders};
use otaq_core::session::{SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize};
use otaq_core::store::projects::Project;

const WAIT: Duration = Duration::from_secs(30);

async fn use_theme(w: &TestService, theme: Theme) {
    let mut settings = w.board.global_settings().await.unwrap();
    settings.theme = ThemeSetting::Picked(theme);
    w.board.set_global_settings(&settings).await.unwrap();
}

/// A program and its arguments.
type Command = (String, Vec<String>);

/// `script` run with `sh -c`.
fn sh(script: String) -> Command {
    ("sh".into(), vec!["-c".into(), script])
}

/// Starts `command` in a session of `kind` for `ticket`, with `env` from the spec.
async fn spawn(
    w: &TestService,
    project: &Project,
    kind: SessionKind,
    ticket: Option<SessionTicket>,
    (program, args): Command,
    env: Vec<(String, String)>,
) -> String {
    w.sessions
        .spawn(SpawnSpec {
            kind,
            start: Start::User,
            project_id: project.id.clone(),
            project_key: project.key.clone(),
            ticket,
            program,
            args,
            cwd: PathBuf::from(&project.repo_path),
            env: [("PATH".to_owned(), path_without_build_folders())]
                .into_iter()
                .chain(env)
                .collect(),
            size: TerminalSize::default(),
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            theme: None,
        })
        .await
        .unwrap()
        .id
}

/// A file's text once `done` is in it.
async fn read_when_done(path: &Path) -> String {
    tokio::time::timeout(WAIT, async {
        loop {
            if let Ok(text) = std::fs::read_to_string(path)
                && let Some(text) = text.strip_suffix("done\n")
            {
                return text.to_owned();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{} was never written", path.display()))
}

/// The session's `COLORFGBG`, as the program saw it, once the session ended: the next agent session of the ticket
/// would be refused while it runs (on Windows a session ends half a second after its program, `DRAIN_WAIT`).
async fn colorfgbg(
    w: &TestService,
    project: &Project,
    kind: SessionKind,
    ticket: Option<SessionTicket>,
    file: &Path,
) -> String {
    let script = format!(
        "printf '%s\\n' \"$COLORFGBG\" done > '{0}.tmp'; mv '{0}.tmp' '{0}'",
        file.display()
    );
    let spec_var = vec![("COLORFGBG".to_owned(), "7;1".to_owned())];
    let id = spawn(w, project, kind, ticket, sh(script), spec_var).await;
    let seen = read_when_done(file).await.trim().to_owned();
    w.sessions.wait_ended(&id).await;
    seen
}

/// The order rule of `session/env.rs`: a program gets the project's rows, then Otaq's own variables on top, which
/// win. `COLORFGBG` is Otaq's own and set after the ticket's and the launch's variables, so a project row and a
/// variable of the launch with that name both lose. Agents, Shells and Run alike; a theme change reaches the next
/// session.
#[tokio::test]
async fn colorfgbg_is_otaqs_own_so_project_and_ticket_variables_lose() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Theme").await;
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.env_vars = vec![EnvVar::new("COLORFGBG", "1;2")];
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    for (theme, expected) in [(Theme::OtaqLight, "0;15"), (Theme::OtaqDark, "15;0")] {
        use_theme(&w, theme).await;
        for kind in [SessionKind::Agent, SessionKind::Shell, SessionKind::Run] {
            let file = tmp.path().join(format!("{}-{kind}", theme.as_str()));
            let ticket = Some(SessionTicket {
                id: ticket.id.clone(),
                key: ticket.key.clone(),
            });
            let seen = colorfgbg(&w, &project, kind, ticket, &file).await;
            assert_eq!(seen, expected, "{} {kind}", theme.as_str());
        }
    }
    // A theme of the other kind than Otaq's own: its kind is what counts.
    use_theme(&w, Theme::SolarizedLight).await;
    let file = tmp.path().join("solarized");
    assert_eq!(
        colorfgbg(&w, &project, SessionKind::Project, None, &file).await,
        "0;15"
    );
    w.stop().await;
}

/// An agent's session is told the theme its launch read (the tool's own setting came from the same read), even
/// when the setting changed in between; a session without one reads the theme in use.
#[tokio::test]
async fn an_agent_session_keeps_the_theme_its_launch_read() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    use_theme(&w, Theme::OtaqDark).await;
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("launched");
    w.sessions
        .spawn(SpawnSpec {
            kind: SessionKind::Agent,
            start: Start::User,
            project_id: project.id.clone(),
            project_key: project.key.clone(),
            ticket: None,
            program: "sh".into(),
            args: vec![
                "-c".into(),
                format!(
                    "printf '%s\\n' \"$COLORFGBG\" done > '{0}.tmp'; mv '{0}.tmp' '{0}'",
                    file.display()
                ),
            ],
            cwd: PathBuf::from(&project.repo_path),
            env: vec![("PATH".to_owned(), path_without_build_folders())],
            size: TerminalSize::default(),
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            theme: Some(Theme::Daylight),
        })
        .await
        .unwrap();
    assert_eq!(read_when_done(&file).await.trim(), "0;15");
    w.stop().await;
}

/// A program asking for the background (`ESC ] 11 ; ? BEL`) and then reading what came back within a second,
/// written to `file` as hex, and the variables it needs. It waits for `gate` first, when given.
#[cfg(unix)]
fn ask_background(file: &Path, gate: Option<&Path>) -> (Command, Vec<(String, String)>) {
    let wait = gate.map_or_else(String::new, |gate| {
        format!("while [ ! -e '{}' ]; do sleep 0.02; done; ", gate.display())
    });
    let script = format!(
        "stty -icanon -echo min 0 time 10; {wait}printf 'asking\\n'; printf '\\033]11;?\\007'; sleep 1; \
         answer=$(dd bs=256 count=1 2>/dev/null | od -An -tx1 | tr -d ' \\n'); \
         printf '%s\\n' \"$answer\" done > '{0}.tmp'; mv '{0}.tmp' '{0}'",
        file.display()
    );
    (sh(script), Vec::new())
}

/// [`ask_background`] on Windows: this test binary, started again as [`colour_asker`].
#[cfg(windows)]
fn ask_background(file: &Path, gate: Option<&Path>) -> (Command, Vec<(String, String)>) {
    use otaq_core::service::testing::{ask_file_var, ask_gate_var, colour_asker_args};
    let program = std::env::current_exe().expect("the test binary");
    let env = [(ask_file_var(), Some(file)), (ask_gate_var(), gate)]
        .into_iter()
        .filter_map(|(name, path)| Some((name, path?.display().to_string())))
        .collect();
    (
        (
            program.display().to_string(),
            colour_asker_args("theme_kind::colour_asker"),
        ),
        env,
    )
}

/// The program [`ask_background`] starts on Windows (Q601); does nothing when run with the other ignored tests.
#[cfg(windows)]
#[test]
#[ignore = "the colour tests' stand-in program on Windows"]
fn colour_asker() {
    otaq_core::service::testing::colour_asker();
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// No terminal shows the session: the service answers once, with the theme's background, ended with the
/// question's BEL.
#[tokio::test]
async fn with_no_terminal_the_service_answers_a_colour_question_once() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    use_theme(&w, Theme::OtaqLight).await;
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("answer");
    let (command, env) = ask_background(&file, None);
    spawn(&w, &project, SessionKind::Project, None, command, env).await;
    assert_eq!(
        read_when_done(&file).await.trim(),
        hex(b"\x1b]11;rgb:fafa/fafa/f8f8\x07"),
        "Otaq Light's term-bg, once"
    );
    w.stop().await;
}

/// A terminal that answers is attached: the service stays silent, so the program gets the terminal's answer
/// alone. A read-only one (it doesn't answer) leaves the answer to the service, once.
#[tokio::test]
async fn an_attached_terminal_answers_alone_and_a_read_only_one_leaves_it_to_the_service() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    use_theme(&w, Theme::OtaqDark).await;
    let tmp = tempfile::tempdir().unwrap();
    for answers in [true, false] {
        let file = tmp.path().join(format!("answer-{answers}"));
        let gate = tmp.path().join(format!("gate-{answers}"));
        let (command, env) = ask_background(&file, Some(&gate));
        let id = spawn(&w, &project, SessionKind::Project, None, command, env).await;
        let (mut attached, held) = w.sessions.attach_answering(&id, answers).unwrap();
        assert_eq!(held.is_some(), answers);
        std::fs::write(&gate, "").unwrap();
        // The terminal sees the question and answers it as xterm.js does (with ST).
        let mut seen = attached.scrollback.clone();
        tokio::time::timeout(WAIT, async {
            while !seen.windows(7).any(|w| w == b"\x1b]11;?\x07") {
                seen.extend_from_slice(&attached.output.recv().await.unwrap().data);
            }
        })
        .await
        .expect("the question");
        let page = b"\x1b]11;rgb:0101/0202/0303\x1b\\";
        if answers {
            w.sessions.write(&id, page).unwrap();
        }
        let expected = if answers {
            hex(page)
        } else {
            hex(b"\x1b]11;rgb:0a0a/0b0b/0d0d\x07")
        };
        assert_eq!(
            read_when_done(&file).await.trim(),
            expected,
            "answers: {answers}"
        );
        drop(held);
    }
    w.stop().await;
}

/// Once the last answering terminal detaches, the service answers again.
#[tokio::test]
async fn a_detached_terminal_hands_the_answers_back_to_the_service() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    use_theme(&w, Theme::OtaqDark).await;
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("answer");
    let gate = tmp.path().join("gate");
    let (command, env) = ask_background(&file, Some(&gate));
    let id = spawn(&w, &project, SessionKind::Project, None, command, env).await;
    let (_, held) = w.sessions.attach_answering(&id, true).unwrap();
    drop(held);
    std::fs::write(&gate, "").unwrap();
    assert_eq!(
        read_when_done(&file).await.trim(),
        hex(b"\x1b]11;rgb:0a0a/0b0b/0d0d\x07")
    );
    w.stop().await;
}

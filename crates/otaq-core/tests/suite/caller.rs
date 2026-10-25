//! Caller identity by process ancestry (T-0064, CLI-01, CLI-04): ordinary nested shells inside PTY sessions stand
//! in for `claude` and the commands it runs. macOS and Linux use `sh` (and `nc` for a raw request); Windows uses
//! `cmd` and PowerShell, whose `$PID` is the Windows process ID (T-0156).

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::caller::{self, Caller};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::Actor;
use otaq_core::git::testing::TempRepos;
use otaq_core::naming;
use otaq_core::service::testing::TestService;
use otaq_core::service::{Service, ServiceConfig, Stopper};
use otaq_core::session::{SessionKind, SessionTicket, Sessions, SpawnSpec, Start, TerminalSize};

struct World {
    home: tempfile::TempDir,
    _repos: TempRepos,
    stopper: Stopper,
    task: tokio::task::JoinHandle<()>,
    sessions: Sessions,
    board: otaq_core::board::Board,
    project_id: String,
    repo: std::path::PathBuf,
    tickets: Vec<Ticket>,
}

impl World {
    async fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let data = DataDir::new(home.path().join("home"));
        let service = Service::bind(ServiceConfig::new(data)).await.unwrap();
        let (stopper, board, sessions) = (service.stopper(), service.board(), service.sessions());
        let task = tokio::spawn(service.serve());
        let repos = TempRepos::new();
        let repo = repos.repo("shop-web", 1);
        let project = board
            .add_project(&AddProject {
                path: repo.display().to_string(),
                ..Default::default()
            })
            .await
            .unwrap();
        let me = Actor::User("me".into());
        let mut tickets = Vec::new();
        for title in ["Split user full name", "Add wishlist"] {
            let input = NewTicketInput {
                project: "SHOP".into(),
                title: title.into(),
                ..Default::default()
            };
            tickets.push(board.create_ticket(&input, &me).await.unwrap());
        }
        Self {
            home,
            _repos: repos,
            stopper,
            task,
            sessions,
            board,
            project_id: project.id,
            repo,
            tickets,
        }
    }

    fn socket(&self) -> std::path::PathBuf {
        DataDir::new(self.home.path().join("home")).socket_path()
    }

    /// Runs `script` with `sh -c` as a session of `kind` for SHOP-1, with `$OTAQ_TICKET` set like an agent's.
    #[cfg(unix)]
    async fn run(&self, kind: SessionKind, script: &str) -> String {
        self.spawn(kind, "sh", vec!["-c".into(), script.into()])
            .await
    }

    /// Runs the PowerShell `script` (saved as a file) as a session of `kind` for SHOP-1, with `$OTAQ_TICKET`
    /// set like an agent's.
    #[cfg(windows)]
    async fn run_powershell(&self, kind: SessionKind, name: &str, script: &str) -> String {
        let file = self.home.path().join(format!("{name}.ps1"));
        std::fs::write(&file, script).unwrap();
        self.spawn(kind, "powershell.exe", powershell_file(&file))
            .await
    }

    async fn spawn(&self, kind: SessionKind, program: &str, args: Vec<String>) -> String {
        let ticket = &self.tickets[0];
        self.sessions
            .spawn(SpawnSpec {
                kind,
                start: Start::User,
                project_id: self.project_id.clone(),
                project_key: "SHOP".into(),
                ticket: Some(SessionTicket {
                    id: ticket.id.clone(),
                    key: ticket.key.clone(),
                }),
                program: program.into(),
                args,
                cwd: self.repo.clone(),
                env: vec![(naming::ticket_env_var(), ticket.key.to_string())],
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

    async fn stop(self) {
        self.stopper.stop();
        self.task.await.unwrap();
    }
}

/// `powershell -File <file>`, without profile or prompts.
#[cfg(windows)]
fn powershell_file(file: &Path) -> Vec<String> {
    [
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain([file.display().to_string()])
    .collect()
}

/// Waits for a file to be written and returns its trimmed contents.
async fn read_when_written(path: &Path) -> String {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(path)
                && text.ends_with('\n')
            {
                return text.trim().to_owned();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{} never written", path.display()))
}

#[tokio::test(flavor = "multi_thread")]
async fn nested_shells_inside_an_agent_session_are_that_agent() {
    let w = World::new().await;
    let out = w.home.path().join("pid");
    // A shell inside a shell inside the session, with the ticket variable removed on the way.
    #[cfg(unix)]
    let session = {
        let script = format!(
            "env -u {var} sh -c 'sh -c \"echo \\$\\$ > {out}; exec sleep 30\"'",
            var = naming::ticket_env_var(),
            out = out.display()
        );
        w.run(SessionKind::Agent, &script).await
    };
    // Windows: PowerShell (the session) → cmd → PowerShell, which writes its process ID.
    #[cfg(windows)]
    let session = {
        let inner = w.home.path().join("inner.ps1");
        std::fs::write(
            &inner,
            format!(
                "Set-Content -Path '{}' -Value $PID\nStart-Sleep 30\n",
                out.display()
            ),
        )
        .unwrap();
        let script = format!(
            "Remove-Item Env:\\{var} -ErrorAction SilentlyContinue\n& cmd.exe /d /c \"powershell {args}\"\n",
            var = naming::ticket_env_var(),
            args = powershell_file(&inner).join(" ")
        );
        w.run_powershell(SessionKind::Agent, "outer", &script).await
    };
    let pid: u32 = read_when_written(&out).await.parse().unwrap();
    match caller::resolve(Some(pid), &w.sessions) {
        Caller::Agent {
            ticket,
            ticket_id,
            session: s,
        } => {
            assert_eq!(ticket.to_string(), "SHOP-1");
            assert_eq!(ticket_id, w.tickets[0].id);
            assert_eq!(s, session);
        }
        other => panic!("the nested shell must resolve to SHOP-1's agent, not {other:?}"),
    }
    // This test process is outside every session: the user.
    assert_eq!(
        caller::resolve(Some(std::process::id()), &w.sessions),
        Caller::User
    );
    assert_eq!(caller::resolve(None, &w.sessions), Caller::User);
    // Once the session ends, its processes are no longer an agent.
    w.sessions.stop(&session).await.unwrap();
    assert_eq!(caller::resolve(Some(pid), &w.sessions), Caller::User);
    w.stop().await;
}

/// T-0184 (CLI-05, d0063): a process inside a validator session is that ticket's validator, not its agent.
#[tokio::test(flavor = "multi_thread")]
async fn processes_inside_a_validator_session_are_the_validator() {
    let w = World::new().await;
    let out = w.home.path().join("pid-validator");
    #[cfg(unix)]
    let session = {
        let script = format!("sh -c 'echo $$ > {}; exec sleep 30'", out.display());
        w.run(SessionKind::Validator, &script).await
    };
    #[cfg(windows)]
    let session = {
        let script = format!(
            "Set-Content -Path '{}' -Value $PID\nStart-Sleep 30\n",
            out.display()
        );
        w.run_powershell(SessionKind::Validator, "validator", &script)
            .await
    };
    let pid: u32 = read_when_written(&out).await.parse().unwrap();
    match caller::resolve(Some(pid), &w.sessions) {
        Caller::Validator {
            ticket,
            ticket_id,
            session: s,
        } => {
            assert_eq!(ticket.to_string(), "SHOP-1");
            assert_eq!(ticket_id, w.tickets[0].id);
            assert_eq!(s, session);
        }
        other => panic!("expected SHOP-1's validator, got {other:?}"),
    }
    w.stop().await;
}

/// T-0206 (CLI-01, d0068): a process inside a Project agent's session is that Project agent. The session is opened
/// as the app opens one, with an ordinary command standing in for `claude` (`sh`; on Windows PowerShell, whose `$PID`
/// is the Windows process ID).
#[tokio::test(flavor = "multi_thread")]
async fn processes_inside_a_project_agent_session_are_that_project_agent() {
    let scratch = tempfile::tempdir().unwrap();
    let out = scratch.path().join("pid-noyan");
    #[cfg(unix)]
    let claude = vec![
        "sh".to_owned(),
        "-c".to_owned(),
        format!("sh -c 'echo $$ > {}; exec sleep 30'", out.display()),
        "sh".to_owned(),
    ];
    #[cfg(windows)]
    let claude = {
        let file = scratch.path().join("noyan.ps1");
        std::fs::write(
            &file,
            format!(
                "Set-Content -Path '{}' -Value $PID\nStart-Sleep 30\n",
                out.display()
            ),
        )
        .unwrap();
        std::iter::once("powershell.exe".to_owned())
            .chain(powershell_file(&file))
            .collect()
    };
    let launch = LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: claude,
            config_dir: None,
        },
    };
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(launch),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    let agent = w
        .project_agents
        .open("SHOP", "Look around the checkout code")
        .await
        .unwrap();
    let session = agent.session.as_ref().expect("a session").id.clone();
    let pid: u32 = read_when_written(&out).await.parse().unwrap();
    match caller::resolve(Some(pid), &w.sessions) {
        Caller::ProjectAgent {
            project,
            agent: id,
            session: s,
        } => {
            assert_eq!(project, agent.project);
            assert_eq!(id, agent.id);
            assert_eq!(s, session);
        }
        other => panic!("expected the Project agent, got {other:?}"),
    }
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn shell_project_and_run_sessions_are_the_user() {
    let w = World::new().await;
    for kind in [SessionKind::Shell, SessionKind::Project, SessionKind::Run] {
        let out = w.home.path().join(format!("pid-{kind}"));
        #[cfg(unix)]
        let session = {
            let script = format!("sh -c 'echo $$ > {}; exec sleep 30'", out.display());
            w.run(kind, &script).await
        };
        #[cfg(windows)]
        let session = {
            let script = format!(
                "Set-Content -Path '{}' -Value $PID\nStart-Sleep 30\n",
                out.display()
            );
            w.run_powershell(kind, &format!("user-{kind}"), &script)
                .await
        };
        let pid: u32 = read_when_written(&out).await.parse().unwrap();
        assert_eq!(
            caller::resolve(Some(pid), &w.sessions),
            Caller::User,
            "{kind}"
        );
        w.sessions.stop(&session).await.unwrap();
    }
    w.stop().await;
}

/// A request sent from inside the agent session (through `nc`, an ordinary command; on Windows PowerShell's
/// pipe client) is recorded as the agent, even when it claims to be the user and the ticket variable is unset.
#[tokio::test(flavor = "multi_thread")]
async fn requests_from_an_agent_session_act_as_the_agent() {
    let w = World::new().await;
    let out = w.home.path().join("reply");
    let request = r#"{"protocol_version":1,"id":1,"caller":"nevzat","request":{"method":"ticket.update","params":{"key":"SHOP-1","title":"Split the full name"}}}"#;
    #[cfg(unix)]
    {
        let script = format!(
            "printf '%s\\n' '{request}' | env -u {var} nc -U -w 2 '{sock}' > '{out}.tmp'; mv '{out}.tmp' '{out}'; echo >> '{out}'",
            var = naming::ticket_env_var(),
            sock = w.socket().display(),
            out = out.display()
        );
        w.run(SessionKind::Agent, &script).await;
    }
    #[cfg(windows)]
    {
        let pipe = otaq_core::ipc::pipe_name(&w.socket());
        let script = format!(
            "Remove-Item Env:\\{var} -ErrorAction SilentlyContinue
$pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', '{name}', [System.IO.Pipes.PipeDirection]::InOut)
$pipe.Connect(5000)
$writer = New-Object System.IO.StreamWriter($pipe)
$writer.AutoFlush = $true
$reader = New-Object System.IO.StreamReader($pipe)
$writer.WriteLine('{request}')
Set-Content -Path '{out}.tmp' -Value $reader.ReadLine()
Move-Item '{out}.tmp' '{out}'
",
            var = naming::ticket_env_var(),
            name = pipe.trim_start_matches(r"\\.\pipe\"),
            out = out.display()
        );
        w.run_powershell(SessionKind::Agent, "request", &script)
            .await;
    }
    let reply = read_when_written(&out).await;
    assert!(reply.contains("\"reply\""), "{reply}");
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.title, "Split the full name");
    assert_eq!(
        ticket.updated_by,
        Some(Actor::Agent("SHOP-1".parse().unwrap()))
    );
    w.stop().await;
}

/// A client can't claim another process's PID: the service compares it with the socket's peer PID.
#[tokio::test(flavor = "multi_thread")]
async fn a_claimed_pid_must_be_the_peer() {
    use otaq_core::protocol::{
        Body, ErrorKind, FrameReader, Message, Request, decode, write_message,
    };
    use tokio::io::BufReader;

    let w = World::new().await;
    let stream = otaq_core::ipc::connect(&w.socket()).await.unwrap();
    let (read, mut write) = otaq_core::ipc::split(stream);
    let mut reader = FrameReader::new(BufReader::new(read));

    let honest = Message::request(1, Request::Ping);
    write_message(&mut write, &honest).await.unwrap();
    let frame = reader.next_frame().await.unwrap().unwrap();
    assert!(matches!(decode(frame).unwrap().body, Body::Reply(_)));

    let lying = Message {
        pid: Some(1),
        ..Message::request(2, Request::Ping)
    };
    write_message(&mut write, &lying).await.unwrap();
    let frame = reader.next_frame().await.unwrap().unwrap();
    let Body::Error(error) = decode(frame).unwrap().body else {
        panic!("a foreign PID must be refused")
    };
    assert_eq!(error.kind, ErrorKind::BadRequest);
    drop(write);
    w.stop().await;
}

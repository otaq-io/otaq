//! Turn ends no tool reports (ADP-12, ADP-08; d0086, d0042).
//!
//! - **Background work.** Codex, Copilot and OpenCode end a turn without saying what it left running
//!   ([`crate::agent_tools::AgentTool::turn_end_reports_background`] is false). At such a turn end Otaq looks at the
//!   agent's process tree ([`crate::process::tree`]): what still runs there and wasn't there when the turn started
//!   (the tool's own helpers, Q-M) is background work, and the card shows Working in background naming the command
//!   ("background: `npm test`"). Otaq looks again every [`QuietTiming::recheck`] (the card changes only when what
//!   runs does); once nothing is left the turn ends
//!   as any other (Idle, or as a submit left it), unless a new turn started first.
//! - **A key at the tool's own prompt.** Copilot sends nothing when its prompt is cancelled with Esc
//!   ([`crate::agent_tools::AgentTool::typed_key_answers_prompt`]). A key the user types into such a terminal while
//!   it shows Needs you counts as answering (a ticket's agent, a validator or a Project agent): it shows
//!   Working, and if the tool sends nothing within [`QuietTiming::answer_wait`] and nothing runs under it, the turn
//!   counts as ended. Only the user's own keys count (the terminal's input), never Otaq's deliveries.
//!
//! Claude Code reports both itself: none of this runs for it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use super::Signals;
use super::background::{BackgroundTask, BackgroundWork};
use super::watch::Watched;
use crate::agent_tools::{Tool, ToolEvent};
use crate::caller::Caller;
use crate::domain::Status;
use crate::process::TreeProcess;

/// How long Otaq waits before it looks at an agent's processes again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuietTiming {
    /// Between two looks at a quiet turn's background work.
    pub recheck: Duration,
    /// After the user's key at the tool's own prompt, before the turn counts as ended.
    pub answer_wait: Duration,
}

impl Default for QuietTiming {
    fn default() -> Self {
        Self {
            recheck: Duration::from_secs(3),
            answer_wait: Duration::from_secs(5),
        }
    }
}

/// The longest command a card names, in characters.
const COMMAND_CHARS: usize = 40;

/// One session's processes and pending looks.
#[derive(Debug, Default)]
pub(super) struct Quiet {
    /// The processes in the agent's tree when its current turn started (the tool's own helpers).
    before: Option<HashSet<u32>>,
    /// The programs of the agent's direct children then: a later process of the same program is the tool's own too.
    helper_programs: HashSet<PathBuf>,
    /// Those direct children's IDs (for [`adopt_execs`]).
    helper_pids: HashSet<u32>,
    /// Moves with each status event: a pending recheck from before it is dropped.
    turn: u64,
    /// Moves with every hook and status event: a pending answer wait from before it is dropped.
    answer: u64,
    /// An answer wait is pending.
    answering: bool,
    /// The tool's last status event was a prompt for the user (needs you or a question of its own): what a validator,
    /// Project agent shows as Needs you.
    prompting: bool,
}

impl Signals {
    /// Changes the waits (tests).
    pub fn set_quiet_timing(&self, timing: QuietTiming) {
        *self.inner.quiet_timing.lock().expect("quiet timing") = timing;
    }

    fn quiet_timing(&self) -> QuietTiming {
        *self.inner.quiet_timing.lock().expect("quiet timing")
    }

    /// Whether `tool`'s turn ends need Otaq's own look at the processes.
    fn checks_processes(&self, tool: Tool) -> bool {
        !self.inner.adapters.get(tool).turn_end_reports_background()
    }

    /// A hook of `session` arrived: a pending answer wait is over (the tool did send something).
    pub(super) fn hook_came(&self, session: &str) {
        if let Some(quiet) = self.inner.quiet.lock().expect("quiet").get_mut(session) {
            quiet.answer += 1;
            quiet.answering = false;
        }
    }

    /// One of `tool`'s events for `session`, before it is applied: a status event drops the pending looks, and a new
    /// turn records the processes already running (the tool's helpers, Q-M). A session seen for the first time
    /// records them too, so a first turn the tool didn't announce still has them. The hook process that sent the
    /// event (`reporter`) and the processes that started it (the tool's shell or launcher for its hooks) still run
    /// while the event is applied: they are recorded too, never the turn's work.
    pub(super) fn quiet_event(
        &self,
        tool: Tool,
        session: &str,
        event: &ToolEvent,
        reporter: Option<u32>,
    ) {
        if !self.checks_processes(tool) {
            return;
        }
        let prompting = matches!(
            event,
            ToolEvent::NeedsYou { .. } | ToolEvent::AsksYou { .. }
        );
        let status = matches!(
            event,
            ToolEvent::NeedsYou { .. }
                | ToolEvent::AsksYou { .. }
                | ToolEvent::TurnEnded { .. }
                | ToolEvent::NewTurn { .. }
                | ToolEvent::Failed { .. }
                | ToolEvent::LimitReached { .. }
        );
        let new_turn = matches!(event, ToolEvent::NewTurn { .. });
        let unknown = self
            .inner
            .quiet
            .lock()
            .expect("quiet")
            .get(session)
            .is_none_or(|q| q.before.is_none());
        let snapshot = (new_turn || unknown)
            .then(|| self.root_of(session))
            .flatten()
            .map(|root| (root, self.inner.sessions.process_tree(session)));
        let hook = reporter.map(crate::process::ancestry);
        let mut all = self.inner.quiet.lock().expect("quiet");
        let quiet = all.entry(session.to_owned()).or_default();
        if status {
            quiet.turn += 1;
            quiet.answer += 1;
            quiet.answering = false;
            quiet.prompting = prompting;
        }
        if let Some((root, processes)) = snapshot {
            let pids: HashSet<u32> = processes.iter().map(|p| p.pid).collect();
            // The agent's direct children, not the hook that sent this event. On Windows also a process whose
            // starter ended: Git Bash's `exec` starts the program anew and ends the process that called it.
            let helpers: Vec<&TreeProcess> = processes
                .iter()
                .filter(|p| p.parent == root || (cfg!(windows) && !pids.contains(&p.parent)))
                .filter(|p| hook.as_ref().is_none_or(|chain| !chain.contains(&p.pid)))
                .collect();
            quiet.helper_programs = helpers.iter().filter_map(|p| p.program.clone()).collect();
            quiet.helper_pids = helpers.iter().map(|p| p.pid).collect();
            quiet.before = Some(pids);
        }
        if let (Some(chain), Some(before)) = (hook, quiet.before.as_mut()) {
            before.extend(chain);
        }
    }

    /// A turn end without background work from a tool that doesn't report it: the event with what Otaq finds still
    /// running, if anything (and a recheck is set); `None` to apply the event as it is.
    pub(super) fn left_running(
        &self,
        caller: &Caller,
        tool: Tool,
        session: &str,
        event: &ToolEvent,
    ) -> Option<ToolEvent> {
        let ToolEvent::TurnEnded {
            background,
            last_message,
        } = event
        else {
            return None;
        };
        // A validator's turn ends are its validation's.
        if !background.is_empty()
            || matches!(caller, Caller::Validator { .. })
            || !self.checks_processes(tool)
        {
            return None;
        }
        let work = self.background_of(session);
        if work.is_empty() {
            return None;
        }
        tracing::debug!(session, work = ?work.tasks_text(), "the turn ended with processes still running");
        let turn = self
            .inner
            .quiet
            .lock()
            .expect("quiet")
            .get(session)
            .map_or(0, |q| q.turn);
        let recheck = ToolEvent::TurnEnded {
            background: BackgroundWork::default(),
            last_message: last_message.clone(),
        };
        let signals = self.clone();
        let (caller, session, seen) = (caller.clone(), session.to_owned(), work.clone());
        let wait = self.quiet_timing().recheck;
        // Applied again only when what runs changed, so the card's line follows it and what follows a turn end
        // (deliveries, a validation's check) isn't repeated every few seconds.
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(wait).await;
                if signals.turn_moved(&session, turn)
                    || !signals
                        .inner
                        .sessions
                        .info(&session)
                        .is_ok_and(|i| i.is_running())
                {
                    return;
                }
                if signals.background_of(&session) != seen {
                    break;
                }
            }
            signals.apply_later(caller, tool, recheck).await;
        });
        Some(ToolEvent::TurnEnded {
            background: work,
            last_message: last_message.clone(),
        })
    }

    /// The user typed into `session`'s terminal (only the terminal's own input, never Otaq's deliveries). On a tool
    /// whose prompt a key answers without a signal (Copilot's Esc), a key while the card shows Needs you makes it
    /// Working; if the tool then sends nothing within [`QuietTiming::answer_wait`] and nothing runs under it, the turn
    /// counts as ended. Every key during the wait starts it again.
    pub async fn typed(&self, session: &str) -> Result<(), crate::board::BoardError> {
        let Ok(info) = self.inner.sessions.info(session) else {
            return Ok(());
        };
        let Some(tool) = info.tool else {
            return Ok(());
        };
        if !info.is_running() || !self.inner.adapters.get(tool).typed_key_answers_prompt() {
            return Ok(());
        }
        let Some(caller) = super::watch::caller_of(&self.inner.sessions, session) else {
            return Ok(());
        };
        let (prompting, answering) = self
            .inner
            .quiet
            .lock()
            .expect("quiet")
            .get(session)
            .map_or((false, false), |q| (q.prompting, q.answering));
        match &caller {
            // A ticket's agent: its card says whether it waits.
            Caller::Agent { ticket_id, .. } => {
                let ticket = self.inner.board.ticket_by_id(ticket_id).await?;
                if ticket.status != Status::NeedsYou && !answering {
                    return Ok(());
                }
                if ticket.status == Status::NeedsYou {
                    self.answered(&ticket.id).await?;
                }
            }
            // A validator or Project agent (0086: any Copilot terminal showing Needs you): its wait came from
            // the tool's prompt event; the key moves it on as the tool's own new turn would.
            Caller::Validator { .. } | Caller::Helper { .. } | Caller::ProjectAgent { .. } => {
                if !prompting && !answering {
                    return Ok(());
                }
                if prompting {
                    self.apply_later(caller.clone(), tool, ToolEvent::NewTurn { prompt: None })
                        .await;
                }
            }
            Caller::User => return Ok(()),
        }
        let answer = {
            let mut all = self.inner.quiet.lock().expect("quiet");
            let quiet = all.entry(session.to_owned()).or_default();
            quiet.answer += 1;
            quiet.answering = true;
            quiet.prompting = false;
            quiet.answer
        };
        let signals = self.clone();
        let session = session.to_owned();
        let timing = self.quiet_timing();
        tokio::spawn(async move {
            tokio::time::sleep(timing.answer_wait).await;
            loop {
                if signals.answer_moved(&session, answer) {
                    return;
                }
                if signals.background_of(&session).is_empty() {
                    break;
                }
                tokio::time::sleep(timing.recheck).await;
            }
            if let Some(quiet) = signals.inner.quiet.lock().expect("quiet").get_mut(&session) {
                quiet.answering = false;
            }
            tracing::info!(
                session,
                "no signal after the user's key at the tool's prompt; its turn counts as ended"
            );
            let ended = ToolEvent::TurnEnded {
                background: BackgroundWork::default(),
                last_message: None,
            };
            signals.apply_later(caller, tool, ended).await;
        });
        Ok(())
    }

    fn turn_moved(&self, session: &str, turn: u64) -> bool {
        self.inner
            .quiet
            .lock()
            .expect("quiet")
            .get(session)
            .is_none_or(|q| q.turn != turn)
    }

    fn answer_moved(&self, session: &str, answer: u64) -> bool {
        self.inner
            .quiet
            .lock()
            .expect("quiet")
            .get(session)
            .is_none_or(|q| q.answer != answer || !q.answering)
    }

    /// Applies an event Otaq made itself the way a hook's are, with what follows a hook (the service's watcher
    /// path); without a watcher (no service), only the signals.
    async fn apply_later(&self, caller: Caller, tool: Tool, event: ToolEvent) {
        if !self
            .inner
            .sessions
            .info(super::session_of(&caller).unwrap_or_default())
            .is_ok_and(|i| i.is_running())
        {
            return;
        }
        match self.inner.watcher.get() {
            Some(watcher) => {
                watcher
                    .apply(Watched {
                        caller,
                        tool,
                        events: vec![event],
                    })
                    .await;
            }
            None => {
                if let Err(e) = self.apply_events(&caller, tool, &[event]).await {
                    tracing::warn!(error = %e, "can't apply the quiet turn's end");
                }
            }
        }
    }

    /// The session's root process: its program in the PTY.
    fn root_of(&self, session: &str) -> Option<u32> {
        let sessions = &self.inner.sessions;
        sessions
            .agent_roots()
            .into_iter()
            .find(|r| r.session == session)
            .map(|r| r.pid)
            .or_else(|| {
                sessions
                    .project_agent_roots()
                    .into_iter()
                    .find(|r| r.session == session)
                    .map(|r| r.pid)
            })
    }

    /// What runs in the session's process tree that wasn't there when its turn started, one task per command the
    /// turn started (a command's own children are part of it).
    fn background_of(&self, session: &str) -> BackgroundWork {
        if self.root_of(session).is_none() {
            return BackgroundWork::default();
        }
        let processes = self.inner.sessions.process_tree(session);
        let (before, helpers) = {
            let mut all = self.inner.quiet.lock().expect("quiet");
            match all.get_mut(session) {
                Some(q) => {
                    if cfg!(windows) {
                        adopt_execs(&processes, q);
                    }
                    (
                        q.before.clone().unwrap_or_default(),
                        q.helper_programs.clone(),
                    )
                }
                None => (HashSet::new(), HashSet::new()),
            }
        };
        BackgroundWork {
            tasks: started_in_turn(&processes, &before, &helpers)
                .into_iter()
                .map(|p| BackgroundTask {
                    kind: "shell".to_owned(),
                    description: Some(command_text(p)),
                    helper_agent: false,
                })
                .collect(),
            wakeups: Vec::new(),
        }
    }
}

/// Windows: a process that runs a program anew in place of one that ran before the turn (Git Bash's `exec` starts it
/// as a new process, a child of the old one, and ends the old one) is that process, not the turn's work: it is
/// recorded as running before the turn, and as one of the tool's helpers (its program too) when the old one was. A
/// turn that started while a helper was still being started (Git Bash's `&` forks first and execs a moment later)
/// would otherwise count it once it runs, or (seen under the forked `sh`) not take its program as the tool's.
/// Recorded in `quiet`, so a later process that reuses the old one's ID changes nothing.
fn adopt_execs(processes: &[TreeProcess], quiet: &mut Quiet) {
    let Some(before) = quiet.before.as_mut() else {
        return;
    };
    let running: HashSet<u32> = processes.iter().map(|p| p.pid).collect();
    // Also one the turn's start saw already, under its forked `sh`: it is the helper, not a child of one.
    let execs: Vec<&TreeProcess> = processes
        .iter()
        .filter(|p| before.contains(&p.parent) && !running.contains(&p.parent))
        .collect();
    for p in execs {
        before.insert(p.pid);
        if quiet.helper_pids.contains(&p.parent) {
            quiet.helper_pids.insert(p.pid);
            quiet.helper_programs.extend(p.program.clone());
        }
    }
}

/// The processes the turn started, without their own children: not running before the turn, not one of the
/// tool's own programs (a process of a program the agent ran as a direct child before the turn), and not a Windows
/// console host (`conhost.exe`, which Windows starts for a console program run without a console: it is that
/// program's, never work of its own).
fn started_in_turn<'a>(
    processes: &'a [TreeProcess],
    before: &HashSet<u32>,
    helpers: &HashSet<PathBuf>,
) -> Vec<&'a TreeProcess> {
    let new: HashMap<u32, &TreeProcess> = processes
        .iter()
        .filter(|p| !before.contains(&p.pid))
        .filter(|p| !console_host(p))
        .filter(|p| {
            p.program
                .as_ref()
                .is_none_or(|program| !helpers.contains(program))
        })
        .map(|p| (p.pid, p))
        .collect();
    let mut tops: Vec<&TreeProcess> = new
        .values()
        .filter(|p| !new.contains_key(&p.parent))
        .copied()
        .collect();
    tops.sort_by_key(|p| p.pid);
    tops
}

fn console_host(process: &TreeProcess) -> bool {
    process
        .program
        .as_ref()
        .and_then(|p| p.to_str()?.rsplit(['/', '\\']).next())
        .is_some_and(|n| n.eq_ignore_ascii_case("conhost.exe"))
}

/// The command as a card names it: "sleep 30", "npm test" (a shell's `-c` text rather than the shell), at most
/// [`COMMAND_CHARS`] characters.
pub fn command_text(process: &TreeProcess) -> String {
    // Windows: `C:\…\sleep.exe` is named `sleep`.
    let program = |arg: &str| {
        let name = arg.rsplit(['/', '\\']).next().unwrap_or(arg);
        let name = match name.len().checked_sub(4) {
            Some(at) if name.is_char_boundary(at) && name[at..].eq_ignore_ascii_case(".exe") => {
                &name[..at]
            }
            _ => name,
        };
        name.trim_start_matches('-').to_owned()
    };
    let text = match process.args.split_first() {
        Some((first, rest)) => {
            let name = program(first);
            let shell = matches!(name.as_str(), "sh" | "bash" | "zsh" | "dash" | "fish");
            let script = rest
                .iter()
                .position(|a| a.starts_with('-') && !a.starts_with("--") && a.contains('c'))
                .and_then(|i| rest.get(i + 1));
            match script {
                Some(script) if shell => script.split_whitespace().collect::<Vec<_>>().join(" "),
                _ => std::iter::once(name)
                    .chain(rest.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(" "),
            }
        }
        None => process
            .program
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(
                || format!("process {}", process.pid),
                |n| n.to_string_lossy().into_owned(),
            ),
    };
    shorten(text.trim())
}

fn shorten(text: &str) -> String {
    if text.chars().count() <= COMMAND_CHARS {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(COMMAND_CHARS - 1).collect();
    short.push('…');
    short
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, parent: u32, program: &str, args: &[&str]) -> TreeProcess {
        TreeProcess {
            pid,
            parent,
            program: Some(PathBuf::from(program)),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
        }
    }

    #[test]
    fn a_card_names_the_command() {
        assert_eq!(
            command_text(&process(9, 1, "/bin/sleep", &["/bin/sleep", "30"])),
            "sleep 30"
        );
        // Copilot runs a command as `bash --norc --noprofile -c "…"`: the command is named.
        assert_eq!(
            command_text(&process(
                9,
                1,
                "/bin/bash",
                &["/bin/bash", "--norc", "--noprofile", "-c", "npm  test"]
            )),
            "npm test"
        );
        assert_eq!(
            command_text(&process(9, 1, "/bin/zsh", &["-zsh", "-lc", "make check"])),
            "make check"
        );
        let long = command_text(&process(
            9,
            1,
            "/usr/bin/env",
            &[
                "env",
                "a-very-long-argument-that-goes-on-and-on",
                "and-more",
            ],
        ));
        assert_eq!(long.chars().count(), COMMAND_CHARS);
        assert!(long.ends_with('…'));
        let nameless = TreeProcess {
            pid: 7,
            parent: 1,
            program: None,
            args: vec![],
        };
        assert_eq!(command_text(&nameless), "process 7");
        // Windows: the program's path and `.exe` aren't named.
        assert_eq!(
            command_text(&process(
                9,
                1,
                r"C:\Program Files\Git\usr\bin\sleep.exe",
                &[r"C:\Program Files\Git\usr\bin\sleep.EXE", "30"]
            )),
            "sleep 30"
        );
    }

    /// Q-M: what ran before the turn and the tool's own programs are left out; a command's children are part of it.
    #[test]
    fn only_what_the_turn_started_counts() {
        let processes = [
            // The tool's helper, there before the turn, and a later one of the same program.
            process(
                10,
                1,
                "/opt/codex/codex-code-mode-host",
                &["codex-code-mode-host"],
            ),
            process(
                30,
                1,
                "/opt/codex/codex-code-mode-host",
                &["codex-code-mode-host"],
            ),
            // A grandchild that was there before (OpenCode's server's MCP child).
            process(11, 10, "/usr/bin/node", &["node", "mcp.js"]),
            // The turn's command and its child.
            process(20, 1, "/bin/sh", &["/bin/sh", "-c", "npm test"]),
            process(21, 20, "/usr/bin/node", &["node", "jest"]),
        ];
        let before = HashSet::from([10, 11]);
        let helpers = HashSet::from([PathBuf::from("/opt/codex/codex-code-mode-host")]);
        let started: Vec<u32> = started_in_turn(&processes, &before, &helpers)
            .iter()
            .map(|p| p.pid)
            .collect();
        assert_eq!(started, [20]);
        assert!(started_in_turn(&processes[..3], &before, &helpers).is_empty());
        // A console host Windows started for the command is the command's, not work of its own.
        let hosted = [
            process(20, 1, "/bin/sh", &["/bin/sh", "-c", "npm test"]),
            process(22, 1, r"C:\Windows\System32\conhost.exe", &["conhost.exe"]),
        ];
        let started: Vec<u32> = started_in_turn(&hosted, &before, &helpers)
            .iter()
            .map(|p| p.pid)
            .collect();
        assert_eq!(started, [20]);
    }

    /// Windows: a helper the turn's start caught before or during Git Bash's `exec` (its forked `sh`, alone or with
    /// the program under it) is the program it became, and a later process of that program is the tool's own; a
    /// process whose starter still runs, or didn't run before the turn, is the turn's.
    #[test]
    fn an_exec_of_a_process_from_before_the_turn_is_from_before() {
        let sh = r"C:\Program Files\Git\usr\bin\sh.exe";
        let tail = r"C:\Program Files\Git\usr\bin\tail.exe";
        let sleep = r"C:\Program Files\Git\usr\bin\sleep.exe";
        let node = r"C:\Program Files\nodejs\node.exe";
        let mut quiet = Quiet {
            before: Some(HashSet::from([8, 11, 13, 14])),
            helper_programs: HashSet::from([PathBuf::from(sh)]),
            helper_pids: HashSet::from([8, 13]),
            ..Quiet::default()
        };
        let processes = [
            // The forked `sh` (8, ended) became `tail` (9).
            process(9, 8, tail, &[tail, "-f", "/dev/null"]),
            // The turn's start saw both: the forked `sh` (13, ended) and the `node` it became (14).
            process(14, 13, node, &[node, "server.js"]),
            // A process from before that still runs, and its new child.
            process(11, 1, sleep, &[sleep, "300"]),
            process(12, 11, sleep, &[sleep, "30"]),
            // The turn's: another `tail`, and `sleep` whose forked `sh` (20) ended.
            process(21, 1, tail, &[tail, "-f", "/dev/null"]),
            process(22, 20, sleep, &[sleep, "30"]),
            process(23, 1, node, &[node, "server.js"]),
        ];
        adopt_execs(&processes, &mut quiet);
        assert_eq!(quiet.before, Some(HashSet::from([8, 9, 11, 13, 14])));
        assert_eq!(quiet.helper_pids, HashSet::from([8, 9, 13, 14]));
        let started: Vec<u32> = started_in_turn(
            &processes,
            quiet.before.as_ref().unwrap(),
            &quiet.helper_programs,
        )
        .iter()
        .map(|p| p.pid)
        .collect();
        assert_eq!(started, [12, 22]);
    }
}

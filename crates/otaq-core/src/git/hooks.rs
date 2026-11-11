//! What one of Otaq's own git steps is doing: the step it is on, the user's hook that runs right now and since when,
//! and, when a hook refuses, which hook it was and its whole output.
//!
//! git itself says which hook it runs: its trace2 events (`GIT_TRACE2_EVENT`) have a `child_start` with the hook's
//! name and a `child_exit` with its exit code. Otaq points them at a temporary file for the commands that can run a
//! hook ([`runs_hooks`], every command once a [`GitWatch`] is attached), reads it while git runs and once git is
//! done. The hooks are the user's own, run by the user's own git with the user's environment: Otaq never runs a hook
//! itself, never copies one and never passes `--no-verify`. The only change a hook can notice is that the git
//! commands it runs also write their events to that file, never to the hook's own output.
//!
//! A hook's output is what git printed (git sends a hook's output to its standard error) from the moment the hook
//! before it ended to the moment it ended itself, by the time each piece arrived; the last hook, the one that refuses,
//! gets everything up to git's end, so its output is whole, with anything git printed after it ("The pre-rebase hook
//! refused to rebase."). Paths under the user's home are shortened (`~/…`, `~\…`) so the user's name never shows.
//!
//! Nothing here stops a hook or sets it a time limit: dropping the future of a git command ends that git process,
//! and a hook it started may outlive it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

/// A step of one of Otaq's own git operations: a start, a reopened ticket's update, Local merge, a helper's merge,
/// the PR flow's rewording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitStep {
    /// A start makes the ticket's worktree (`git worktree add`: post-checkout).
    AddWorktree,
    /// Uncommitted files are kept in Otaq's temporary checkpoint commit. It runs no hook (the one place Otaq's
    /// steps skip them: nothing of it lands), so no hook refusal names this step.
    SaveUncommitted,
    /// A branch is brought up to date by merging the branch it goes into into it (prepare-commit-msg, commit-msg,
    /// pre-merge-commit, post-merge; never a rebase).
    UpdateBranch,
    /// The uncommitted files kept by the checkpoint are put back.
    RestoreUncommitted,
    /// Local merge fetches the target branch and brings it up to date with its remote.
    UpdateTarget,
    /// Local merge makes or resets its scratch worktree (post-checkout).
    PrepareScratch,
    /// The branch is squash-merged (`git merge --squash`: post-merge).
    Squash,
    /// The squash commit is made with the confirmed message (pre-commit, prepare-commit-msg, commit-msg,
    /// post-commit).
    Commit,
    /// The project's Merge check runs on the merged result.
    MergeCheck,
    /// The target branch moves to the new commit, in the user's own folder when it is checked out there
    /// (post-merge).
    MoveTarget,
    /// The PR flow writes the commit messages the user confirmed.
    Reword,
}

impl GitStep {
    /// Whether the user can stop the step while it runs. Moving the target is the one that can't: by then the merge
    /// has landed, and the post-merge hook it runs in the user's own folder has nothing to undo.
    pub fn stoppable(self) -> bool {
        !matches!(self, Self::MoveTarget)
    }

    /// Whether its hooks run in the user's own folder rather than in Otaq's worktrees: moving the target fast-forwards
    /// the user's checkout, where their post-merge hook runs.
    pub fn runs_in_users_folder(self) -> bool {
        matches!(self, Self::MoveTarget)
    }
}

/// What a watched git operation is doing now ([`GitWatch::activity`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GitActivity {
    /// The step it is on, once it has said one.
    pub step: Option<GitStep>,
    /// The steps it was on before, in order: a step that comes up twice (Local merge prepares its scratch worktree
    /// before and after bringing the branch up to date) is placed by what came before it.
    pub earlier: Vec<GitStep>,
    /// The hook running right now.
    pub hook: Option<RunningHook>,
    /// The hooks that ran so far, passed or failed, oldest first; each is added when its git command ends.
    pub hooks: Vec<HookRun>,
}

/// A hook that runs right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunningHook {
    /// `pre-commit`.
    pub name: String,
    pub step: Option<GitStep>,
    /// Where git runs it: the worktree, the scratch worktree or the user's own folder.
    pub folder: PathBuf,
    /// When it started, in milliseconds since the Unix epoch.
    pub since_ms: i64,
}

/// A hook that ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HookRun {
    /// `commit-msg`.
    pub name: String,
    pub step: Option<GitStep>,
    pub folder: PathBuf,
    pub started_ms: i64,
    pub ended_ms: i64,
    /// Its exit code; `None` when git didn't say.
    pub exit: Option<i32>,
    /// Everything it printed, paths under the home shortened.
    pub output: String,
}

impl HookRun {
    pub fn passed(&self) -> bool {
        self.exit == Some(0)
    }

    /// It failed because it tried to read from the terminal: see [`needs_terminal`].
    pub fn needs_terminal(&self) -> bool {
        !self.passed() && needs_terminal(&self.output)
    }
}

/// A hook's output says it failed because it tried to read from the terminal (`exec < /dev/tty`, a "commit anyway?
/// [y/N]" read from `/dev/tty`). Otaq runs git with no terminal, so opening `/dev/tty` fails, and every shell and
/// platform says so in its own words on a line naming `/dev/tty`:
/// - bash (Linux, and Git for Windows' sh): "pre-commit: line 3: /dev/tty: No such device or address";
/// - dash: "pre-commit: 3: cannot open /dev/tty: No such device or address"; busybox ash: "can't open '/dev/tty'";
/// - macOS, where the same error reads "Device not configured": bash and sh "/dev/tty: Device not configured", zsh
///   "device not configured: /dev/tty", ksh "/dev/tty: cannot open [Device not configured]";
/// - Python "[Errno 6] No such device or address: '/dev/tty'", Node "ENXIO: no such device or address, open
///   '/dev/tty'".
pub fn needs_terminal(output: &str) -> bool {
    const FAILURES: [&str; 5] = [
        "no such device or address",
        "device not configured",
        "enxio",
        "cannot open",
        "can't open",
    ];
    output.lines().any(|line| {
        let line = line.to_lowercase();
        line.contains("/dev/tty") && FAILURES.iter().any(|f| line.contains(f))
    })
}

/// A git command a hook refused: [`GitError::HookRefused`](super::GitError::HookRefused).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HookRefusal {
    /// The hook that refused, with its whole output.
    pub hook: HookRun,
    /// The git command it refused: `commit --quiet --allow-empty -F …`.
    pub args: String,
    /// git's exit code.
    pub code: Option<i32>,
    /// Everything git printed, the hook's output included, paths under the home shortened.
    pub stderr: String,
}

impl HookRefusal {
    /// `pre-commit`.
    pub fn name(&self) -> &str {
        &self.hook.name
    }

    pub fn step(&self) -> Option<GitStep> {
        self.hook.step
    }

    /// The hook's whole output, paths under the home shortened.
    pub fn output(&self) -> &str {
        &self.hook.output
    }
}

/// What a ticket keeps of a hook's refusal of one of Otaq's own git steps, for its card, its sheet and its History:
/// which hook, the step, what it stopped and the hook's whole output. It outlives the worktree the hook ran in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefusedHook {
    /// `post-checkout`.
    pub name: String,
    /// The step it refused, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<GitStep>,
    /// What it stopped, as the card says it: "the start", "the update from main".
    pub what: String,
    /// The hook's whole output, paths under the home shortened; git's own output when the hook printed nothing.
    pub output: String,
}

impl RefusedHook {
    /// Kept from `refusal`; `step` when git's events didn't say one (nothing watched the command).
    pub fn of(refusal: &HookRefusal, step: GitStep, what: impl Into<String>) -> Self {
        let output = match refusal.output().trim() {
            "" => refusal.stderr.trim(),
            output => output,
        };
        Self {
            name: refusal.name().to_owned(),
            step: Some(refusal.step().unwrap_or(step)),
            what: what.into(),
            output: shorten_home(output),
        }
    }

    /// It refused because it tried to read from the terminal: see [`needs_terminal`].
    pub fn needs_terminal(&self) -> bool {
        needs_terminal(&self.output)
    }

    /// "post-checkout hook refused the start".
    pub fn line(&self) -> String {
        format!("{} hook refused {}", self.name, self.what)
    }
}

/// Follows one git operation: give it to [`Git::watched`](super::Git::watched) and every command that `Git` runs
/// reports here. Cloning shares it.
#[derive(Debug, Clone)]
pub struct GitWatch(Arc<watch::Sender<GitActivity>>);

impl Default for GitWatch {
    fn default() -> Self {
        Self::new()
    }
}

impl GitWatch {
    pub fn new() -> Self {
        Self(Arc::new(watch::channel(GitActivity::default()).0))
    }

    /// What it is doing now.
    pub fn activity(&self) -> GitActivity {
        self.0.borrow().clone()
    }

    /// Wakes on every change: a new step, a hook that starts or ends.
    pub fn subscribe(&self) -> watch::Receiver<GitActivity> {
        self.0.subscribe()
    }

    pub(super) fn step(&self) -> Option<GitStep> {
        self.0.borrow().step
    }

    pub(super) fn set_step(&self, step: GitStep) {
        self.0.send_if_modified(|a| {
            if a.step == Some(step) {
                return false;
            }
            if let Some(was) = a.step.replace(step) {
                a.earlier.push(was);
            }
            true
        });
    }

    pub(super) fn set_running(&self, hook: Option<RunningHook>) {
        self.0.send_if_modified(|a| {
            let changed = a.hook != hook;
            a.hook = hook;
            changed
        });
    }

    pub(super) fn add_runs(&self, runs: &[HookRun]) {
        self.0.send_modify(|a| {
            a.hook = None;
            a.hooks.extend_from_slice(runs);
        });
    }
}

/// Whether `git <args>` can run one of the hooks a project usually has, so its events are read even when nothing
/// watches it: a refusal then still names the hook.
pub(super) fn runs_hooks(args: &[&str]) -> bool {
    matches!(
        args.first().copied(),
        Some(
            "commit"
                | "merge"
                | "rebase"
                | "checkout"
                | "switch"
                | "worktree"
                | "am"
                | "cherry-pick"
                | "revert"
                | "pull"
                | "push"
                | "clone"
                | "hook"
        )
    )
}

/// The current time in microseconds since the Unix epoch, the clock git's events use.
pub(super) fn now_us() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX))
}

/// The events of one git command, read from the file git writes them to.
pub(super) struct HookTrace {
    _dir: tempfile::TempDir,
    file: PathBuf,
    /// How far the file was read.
    read: u64,
    /// The end of the file that isn't a whole line yet.
    partial: Vec<u8>,
    /// Hooks started and not ended yet (normally one).
    open: Vec<Open>,
    /// Hooks that ended, in the order they did.
    done: Vec<Ended>,
}

struct Open {
    sid: String,
    child_id: i64,
    name: String,
    start_us: i64,
}

struct Ended {
    name: String,
    start_us: i64,
    end_us: i64,
    code: Option<i32>,
}

/// The fields of a trace2 event Otaq reads.
#[derive(Deserialize)]
struct Event {
    event: String,
    #[serde(default)]
    sid: String,
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    child_id: Option<i64>,
    #[serde(default)]
    child_class: Option<String>,
    #[serde(default)]
    hook_name: Option<String>,
    #[serde(default)]
    code: Option<i32>,
}

impl HookTrace {
    pub(super) fn new() -> std::io::Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix(&format!("{}-git-", crate::naming::COMMAND_NAME))
            .tempdir()?;
        let file = dir.path().join("events.json");
        Ok(Self {
            _dir: dir,
            file,
            read: 0,
            partial: Vec::new(),
            open: Vec::new(),
            done: Vec::new(),
        })
    }

    /// The variables that make git write its events to the file: with times (an event-brief setting of the user's
    /// would drop them), and no deeper than the top regions.
    pub(super) fn env(&self) -> [(&'static str, String); 3] {
        [
            ("GIT_TRACE2_EVENT", self.file.display().to_string()),
            ("GIT_TRACE2_EVENT_BRIEF", "false".to_owned()),
            ("GIT_TRACE2_EVENT_NESTING", "1".to_owned()),
        ]
    }

    /// Reads what git wrote since the last read.
    pub(super) fn read(&mut self) {
        use std::io::{Read, Seek, SeekFrom};
        let Ok(mut file) = std::fs::File::open(&self.file) else {
            return;
        };
        if file.seek(SeekFrom::Start(self.read)).is_err() {
            return;
        }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return;
        }
        self.read += bytes.len() as u64;
        self.partial.extend_from_slice(&bytes);
        let Some(last) = self.partial.iter().rposition(|b| *b == b'\n') else {
            return;
        };
        let rest = self.partial.split_off(last + 1);
        let whole = std::mem::replace(&mut self.partial, rest);
        for line in whole.split(|b| *b == b'\n') {
            if let Ok(event) = serde_json::from_slice::<Event>(line) {
                self.take(event);
            }
        }
    }

    fn take(&mut self, event: Event) {
        let time = event.time.as_deref().and_then(parse_time_us);
        match event.event.as_str() {
            "child_start" if event.child_class.as_deref() == Some("hook") => {
                // A git command a running hook started runs its own hooks: they are part of that hook.
                let inside_a_hook = self
                    .open
                    .iter()
                    .any(|o| event.sid.starts_with(&format!("{}/", o.sid)));
                if inside_a_hook {
                    return;
                }
                let (Some(name), Some(child_id)) = (event.hook_name, event.child_id) else {
                    return;
                };
                self.open.push(Open {
                    sid: event.sid,
                    child_id,
                    name,
                    start_us: time.unwrap_or_else(now_us),
                });
            }
            "child_exit" => {
                let Some(at) = self
                    .open
                    .iter()
                    .position(|o| o.sid == event.sid && Some(o.child_id) == event.child_id)
                else {
                    return;
                };
                let open = self.open.remove(at);
                self.done.push(Ended {
                    name: open.name,
                    start_us: open.start_us,
                    end_us: time.unwrap_or_else(now_us),
                    code: event.code,
                });
            }
            _ => {}
        }
    }

    /// The hook running now, if any: the one started last.
    pub(super) fn running(&self) -> Option<(&str, i64)> {
        self.open
            .last()
            .map(|o| (o.name.as_str(), o.start_us / 1000))
    }

    /// The hooks that ran, each with its output from `chunks` (what git printed, with the time each piece arrived).
    pub(super) fn runs(
        &self,
        chunks: &[(i64, Vec<u8>)],
        step: Option<GitStep>,
        folder: &Path,
    ) -> Vec<HookRun> {
        let mut hooks: Vec<&Ended> = self.done.iter().collect();
        hooks.sort_by_key(|h| h.start_us);
        (0..hooks.len())
            .map(|i| {
                let hook = hooks[i];
                // From the end of the hook before to its own end (the last one: to git's end), so the output of the
                // last hook, the one that refuses, is never cut short by the moment it took to read it.
                let from = if i == 0 {
                    i64::MIN
                } else {
                    hooks[i - 1].end_us
                };
                let to = if i + 1 == hooks.len() {
                    i64::MAX
                } else {
                    hook.end_us
                };
                let bytes: Vec<u8> = chunks
                    .iter()
                    .filter(|(at, _)| *at >= from && *at < to)
                    .flat_map(|(_, b)| b.iter().copied())
                    .collect();
                HookRun {
                    name: hook.name.clone(),
                    step,
                    folder: folder.to_path_buf(),
                    started_ms: hook.start_us / 1000,
                    ended_ms: hook.end_us / 1000,
                    exit: hook.code,
                    output: shorten_home(String::from_utf8_lossy(&bytes).trim_end()),
                }
            })
            .collect()
    }

    /// The hook that refused a git command that failed: the last hook to run, when it failed. A failed hook that
    /// something else ran after didn't stop git there.
    pub(super) fn refusing(runs: &[HookRun]) -> Option<&HookRun> {
        runs.last().filter(|run| !run.passed())
    }
}

/// `2026-11-11T09:48:10.245027Z` in microseconds since the Unix epoch.
fn parse_time_us(time: &str) -> Option<i64> {
    let time = time.strip_suffix('Z')?;
    let (date, clock) = time.split_once('T')?;
    let mut date = date.splitn(3, '-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (clock, fraction) = clock.split_once('.').unwrap_or((clock, "0"));
    let mut clock = clock.splitn(3, ':').map(str::parse::<i64>);
    let (hour, minute, second) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );
    let digits: String = fraction.chars().take(6).collect();
    let micros = format!("{digits:0<6}").parse::<i64>().ok()?;
    // Days since the epoch of a proleptic Gregorian date (Howard Hinnant's days_from_civil).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + hour) * 60 + minute) * 60_000_000 + second * 1_000_000 + micros)
}

/// `text` with the user's home folder shortened to `~`, in every form git and its hooks print it: `/Users/ana/…`,
/// and on Windows `C:\Users\ana\…`, `C:/Users/ana/…` and `/c/Users/ana/…`. A longer folder name that only starts
/// the same (`/Users/anabel`) is left alone.
pub fn shorten_home(text: &str) -> String {
    match std::env::home_dir() {
        Some(home) => shorten_home_with(text, &home.display().to_string()),
        None => text.to_owned(),
    }
}

fn shorten_home_with(text: &str, home: &str) -> String {
    let home = home.trim_end_matches(['/', '\\']);
    if home.is_empty() {
        return text.to_owned();
    }
    let mut forms = vec![home.to_owned()];
    let bytes = home.as_bytes();
    if bytes.len() > 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        let rest = home[2..].replace('\\', "/");
        let drive = home[..1].to_ascii_lowercase();
        let upper = home[..1].to_ascii_uppercase();
        forms.extend([
            format!("{upper}:{}", &home[2..]),
            format!("{drive}:{}", &home[2..]),
            format!("{upper}:{rest}"),
            format!("{drive}:{rest}"),
            format!("/{drive}{rest}"),
        ]);
    }
    forms.sort_by_key(|f| std::cmp::Reverse(f.len()));
    forms.dedup();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'scan: while !rest.is_empty() {
        for form in &forms {
            if let Some(after) = rest.strip_prefix(form.as_str())
                && after
                    .chars()
                    .next()
                    .is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '.' | '-' | '_')))
                && !out.chars().last().is_some_and(|c| c.is_alphanumeric())
            {
                out.push('~');
                rest = after;
                continue 'scan;
            }
        }
        let mut chars = rest.chars();
        out.push(chars.next().expect("not empty"));
        rest = chars.as_str();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_folder_is_shortened_in_every_form() {
        assert_eq!(
            shorten_home_with(
                "/Users/ana/shop/.git/hooks/pre-commit: lint failed",
                "/Users/ana"
            ),
            "~/shop/.git/hooks/pre-commit: lint failed"
        );
        assert_eq!(
            shorten_home_with("see /Users/anabel/x and /Users/ana", "/Users/ana"),
            "see /Users/anabel/x and ~"
        );
        let windows = r"C:\Users\ana";
        assert_eq!(
            shorten_home_with(r"C:\Users\ana\hooksproj\hooks\pre-commit failed", windows),
            r"~\hooksproj\hooks\pre-commit failed"
        );
        assert_eq!(
            shorten_home_with(
                "C:/Users/ana/repo/.git/hooks/pre-commit: line 3: /dev/tty: No such device or address",
                windows
            ),
            "~/repo/.git/hooks/pre-commit: line 3: /dev/tty: No such device or address"
        );
        assert_eq!(
            shorten_home_with("/c/Users/ana/repo and c:/Users/ana", windows),
            "~/repo and ~"
        );
        assert_eq!(
            shorten_home_with("nothing here", "/Users/ana"),
            "nothing here"
        );
    }

    #[test]
    fn event_times_read_as_microseconds() {
        assert_eq!(
            parse_time_us("1970-01-01T00:00:01.000002Z"),
            Some(1_000_002)
        );
        assert_eq!(
            parse_time_us("2026-11-11T09:48:10.245027Z"),
            Some(1_794_390_490_245_027)
        );
    }

    #[test]
    fn a_hook_inside_a_running_hook_is_part_of_it() {
        let mut trace = HookTrace::new().unwrap();
        let line = |json: &str| serde_json::from_str::<Event>(json).unwrap();
        trace.take(line(r#"{"event":"child_start","sid":"a","time":"1970-01-01T00:00:01Z","child_id":0,"child_class":"hook","hook_name":"pre-commit"}"#));
        trace.take(line(r#"{"event":"child_start","sid":"a/b","time":"1970-01-01T00:00:02Z","child_id":0,"child_class":"hook","hook_name":"post-checkout"}"#));
        assert_eq!(trace.running(), Some(("pre-commit", 1000)));
        trace.take(line(r#"{"event":"child_exit","sid":"a/b","time":"1970-01-01T00:00:03Z","child_id":0,"code":0}"#));
        trace.take(line(r#"{"event":"child_exit","sid":"a","time":"1970-01-01T00:00:04Z","child_id":0,"code":1}"#));
        let chunks = vec![
            (500_000, b"before\n".to_vec()),
            (1_500_000, b"lint failed\n".to_vec()),
        ];
        let runs = trace.runs(&chunks, None, Path::new("/r"));
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].name, "pre-commit");
        assert_eq!(runs[0].exit, Some(1));
        assert_eq!(runs[0].output, "before\nlint failed");
        assert_eq!(
            HookTrace::refusing(&runs).map(|r| r.name.as_str()),
            Some("pre-commit")
        );
    }

    #[test]
    fn a_hook_that_needs_a_terminal_is_told_by_what_each_shell_prints() {
        // Each as the shell or platform prints it with no terminal: Git for Windows' sh as the release test and the
        // Windows VM showed it, macOS's shells as they print it, Linux's bash, dash and busybox, Python and Node.
        for output in [
            "C:/Users/ana/shop/.git/hooks/pre-commit: line 3: /dev/tty: No such device or address",
            "/usr/bin/dash: 1: cannot open /dev/tty: No such device or address",
            ".git/hooks/pre-push: line 2: /dev/tty: Device not configured",
            ".git/hooks/pre-commit:2: device not configured: /dev/tty",
            ".git/hooks/pre-commit[2]: /dev/tty: cannot open [Device not configured]",
            "sh: can't open '/dev/tty': No such device or address",
            "lint: 3 problems\nOSError: [Errno 6] Device not configured: '/dev/tty'",
            "Error: ENXIO: no such device or address, open '/dev/tty'",
        ] {
            assert!(needs_terminal(output), "{output}");
        }
        for output in [
            "",
            "lint: TODO found in src/calc.js:3",
            "commit anyway? [y/N] (no answer: refusing)",
            "reading the answer from /dev/tty",
            "No such device or address",
        ] {
            assert!(!needs_terminal(output), "{output}");
        }
        let run = |exit, output: &str| HookRun {
            name: "pre-commit".into(),
            step: None,
            folder: PathBuf::new(),
            started_ms: 0,
            ended_ms: 0,
            exit,
            output: output.into(),
        };
        let tty = ".git/hooks/pre-commit: line 2: /dev/tty: No such device or address";
        assert!(run(Some(1), tty).needs_terminal());
        assert!(!run(Some(0), tty).needs_terminal(), "it passed after all");
    }
}

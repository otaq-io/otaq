//! Activity, the header's button and its popover (SHL-03, SCH-03, RUN-04, USG-03; d0087, d0086, d0020, d0016, d0077;
//! *02 · App shell → Row · Header states (0086 · V)*, *Row · Header parts: Activity, rail (0086 · V)*, *Row · Activity
//! popover states (0087)*). It replaces the header's tokens today, plan windows, agents chip and processes chip (USG-05).
//!
//! The button reads "● 6 running · ▷ 3" (agents doing a turn across all projects; ▷ the running processes, hidden at
//! 0), or "● 3 / 5 agents" with agent limits on. It says something about usage only when a window has
//! [`WARN_AT_LEFT`] % or less left ("18% left") or a limit is active ("Claude Code limit"), always amber
//! ([`WARNING_TOKEN`]) and never turquoise, which means needs you (d0077); the limit keeps its own banner (d0020).
//! With one tool connected no tool is named in the button ("5h 18% left", "5h limit"). Percentages are what's
//! **left** (100 − used). The popover (⇧⌘U) lists the agents running, what's left per tool and window with reset
//! times, today's tokens, the processes and Open Terminals, under the agent-limit stepper when limits are on. No
//! dollar figure anywhere (d0086).

use otaq_core::agent_tools::LimitWindow;
use otaq_core::board::settings::AgentLimits;
use otaq_core::scheduler::SchedulerStatus;
use otaq_core::terminals::run::RunInfo;
use otaq_core::usage::activity::{Activity, ToolActivity, ToolLimits};
use otaq_core::usage::limit::LimitKind;
use serde::Serialize;
use ts_rs::TS;

use super::card::tokens_label;
use super::clock::reset_label;
use super::limits::{self, AgentsChip};
use super::processes;
use super::run_page::{self, ProcessesView};
use super::tool_avatar::{self, AvatarTool, ToolAvatarView};
use crate::key_labels::KeyLabels;
use crate::keymap::{KeyAction, Platform};

/// A window with this share left or less (in %) shows in the button and turns its bar amber.
pub const WARN_AT_LEFT: f64 = 20.0;

/// The usage warning's colour: amber (`st-paused`), never turquoise (`st-attn` is needs you only, d0077).
pub const WARNING_TOKEN: &str = "st-paused";

/// The popover's line while no agent runs, naming the waiting column ([`Inputs::waiting`]): "Nothing running. Tickets
/// in Ready for Dev start when a slot and their parents allow."
pub fn nothing_running(waiting: &str) -> String {
    format!("Nothing running. Tickets in {waiting} start when a slot and their parents allow.")
}

/// A tool without limit windows now (an API-key login).
pub const NO_PLAN: &str = "No plan limit · tokens only";

/// A tool that doesn't report limits.
pub const NOT_REPORTED: &str = "Limits not reported";

/// A window that is used up.
pub const LIMIT_REACHED: &str = "limit reached";

/// The 7-day window's length, in minutes.
const WEEK: u32 = 7 * 24 * 60;

/// One connected tool's activity and the tool it stands for (tests and the gallery give it other tools).
pub type ToolEntry<'a> = (AvatarTool, &'a ToolActivity);

/// The connected tools' parts of the activity summary, in its order. With one tool connected (M21), Claude Code's.
pub fn connected(activity: &Activity) -> Vec<ToolEntry<'_>> {
    connected_among(activity, &tool_avatar::connected())
}

/// The parts of the activity summary for the tools in `connected`, in the summary's order.
pub fn connected_among<'a>(activity: &'a Activity, connected: &[AvatarTool]) -> Vec<ToolEntry<'a>> {
    activity
        .tools
        .iter()
        .filter_map(|t| {
            let tool = AvatarTool::from(t.tool);
            connected.contains(&tool).then_some((tool, t))
        })
        .collect()
}

/// What Activity is drawn from.
#[derive(Debug, Clone)]
pub struct Inputs<'a> {
    /// The connected tools ([`connected`]).
    pub tools: Vec<ToolEntry<'a>>,
    /// Today's tokens across every tool.
    pub tokens_today: u64,
    pub limits: &'a AgentLimits,
    pub scheduler: Option<&'a SchedulerStatus>,
    pub runs: &'a [RunInfo],
    pub now: i64,
    /// The waiting column's name across the projects: "Ready for Dev", or "Ready" once a project's flow doesn't start
    /// at Dev (d0093, Q515).
    pub waiting: &'a str,
}

impl Inputs<'_> {
    fn running(&self) -> u32 {
        self.tools.iter().map(|(_, t)| t.running).sum()
    }

    /// Tools are named in the button only when two or more are connected (d0087).
    fn names_tools(&self) -> bool {
        self.tools.len() > 1
    }
}

/// A limit window as Activity words it.
#[derive(Debug, Clone)]
struct Window {
    /// "5h", "week", "month".
    label: String,
    /// What's left, 0–100.
    left: f64,
    /// Used up, or the window the active limit is on.
    reached: bool,
    /// The window the tool's active limit is on.
    limit: bool,
    /// The reset time still ahead.
    resets_at: Option<i64>,
}

impl Window {
    /// "18% left", or "limit reached".
    fn left_text(&self) -> String {
        if self.reached {
            LIMIT_REACHED.to_owned()
        } else {
            left_percent(self.left)
        }
    }

    fn warns(&self) -> bool {
        self.reached || self.left <= WARN_AT_LEFT
    }

    /// How close to its limit: used-up windows first.
    fn closeness(&self) -> f64 {
        if self.reached { -1. } else { self.left }
    }
}

/// "18% left", rounded to a whole percent (82.4 % used: "18% left").
pub fn left_percent(left: f64) -> String {
    format!("{}% left", left.clamp(0., 100.).round() as u32)
}

/// A window's name: "5h" (300 minutes), "week", "month", else its length ("90 min", "2d"), or the tool's key for a
/// window without a fixed length.
pub fn window_label(window: &LimitWindow) -> String {
    match window.length_minutes {
        Some(WEEK) => "week".to_owned(),
        Some(m) if m < 24 * 60 && m % 60 == 0 => format!("{}h", m / 60),
        Some(m) if m < 24 * 60 => format!("{m} min"),
        Some(m) if (28 * 24 * 60..=31 * 24 * 60).contains(&m) => "month".to_owned(),
        Some(m) => format!("{}d", m / (24 * 60)),
        None => window.key.clone(),
    }
}

/// The index of the window the tool's active limit is on: the session limit on its shortest window, the weekly one
/// on its week-long (or longer) window; another limit on none.
fn limited_window(windows: &[LimitWindow], kind: LimitKind) -> Option<usize> {
    match kind {
        LimitKind::Session => (!windows.is_empty()).then_some(0),
        LimitKind::Weekly => windows
            .iter()
            .rposition(|w| w.length_minutes.is_some_and(|m| m >= WEEK)),
        LimitKind::Other => None,
    }
}

/// A tool's windows at `now`. A window whose reset time has passed has reset: all of it is left until the tool
/// reports again.
fn windows(tool: &ToolActivity, now: i64) -> Vec<Window> {
    let ToolLimits::Windows(windows) = &tool.limits else {
        return Vec::new();
    };
    let limited = tool
        .limit
        .as_ref()
        .and_then(|l| limited_window(windows, l.kind));
    windows
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let is_limit = limited == Some(i);
            let reset = w.resets_at.filter(|&at| at <= now).is_some();
            let left = if reset {
                100.
            } else {
                (100. - w.used_percent).clamp(0., 100.)
            };
            let limit_reset = tool.limit.as_ref().and_then(|l| l.resets_at);
            let resets_at = if is_limit {
                limit_reset.or(w.resets_at)
            } else {
                w.resets_at
            };
            Window {
                label: window_label(w),
                left,
                reached: is_limit || (!reset && left.round() == 0.),
                limit: is_limit,
                resets_at: resets_at.filter(|&at| at > now),
            }
        })
        .collect()
}

/// The amber part of the button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityWarning")]
pub struct Warning {
    /// "18% left", "5h 18% left", "Claude Code limit", "5h limit".
    pub text: String,
    /// Always [`WARNING_TOKEN`].
    pub token: &'static str,
    /// A limit is active: the whole button takes the amber edge (*Activity button · states → A tool hit its limit*).
    pub limit: bool,
}

/// The button's hover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityHover")]
pub struct Hover {
    /// "6 agents running across all projects".
    pub title: String,
    /// "pocket-notes 2 · notes-api 1 · 2 processes", then the window closest to its limit ("Claude Code · 5h: 18%
    /// left · resets 3:45 pm"). Lines without anything to say are left out.
    pub lines: Vec<String>,
}

/// The header's Activity button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityButton")]
pub struct Button {
    /// "● 6 running", or "● 3 / 5 agents" with agent limits on.
    pub agents: AgentsChip,
    /// "3" after ▷: the running processes; `None` (hidden) at 0.
    pub processes: Option<String>,
    /// The usage warning; `None` while every window has more than [`WARN_AT_LEFT`] % left.
    pub warning: Option<Warning>,
    pub hover: Hover,
    /// "Activity" and the user's key for it (⇧⌘U), which opens the popover as a click does.
    pub label: &'static str,
    pub keys: Option<String>,
}

pub fn button(inputs: &Inputs<'_>, keys: &KeyLabels) -> Button {
    let running = inputs.running();
    Button {
        agents: limits::agents_chip(inputs.limits, inputs.scheduler, running),
        processes: processes::count(inputs.runs).map(|n| n.to_string()),
        warning: warning(inputs),
        hover: hover(inputs, running),
        label: "Activity",
        keys: keys.label(KeyAction::OpenActivity),
    }
}

fn warning(inputs: &Inputs<'_>) -> Option<Warning> {
    let names = inputs.names_tools();
    let warn = |text, limit| {
        Some(Warning {
            text,
            token: WARNING_TOKEN,
            limit,
        })
    };
    // A limit first: the tool's name, or with one tool the window it is on.
    if let Some((tool, data)) = inputs.tools.iter().find(|(_, t)| t.limit.is_some()) {
        if names {
            return warn(format!("{} limit", tool.name()), true);
        }
        let text = windows(data, inputs.now)
            .into_iter()
            .find(|w| w.limit)
            .map_or_else(
                || "Usage limit".to_owned(),
                |w| format!("{} limit", w.label),
            );
        return warn(text, true);
    }
    let (_, closest) = closest(inputs)?;
    if !closest.warns() {
        return None;
    }
    if names {
        warn(closest.left_text(), false)
    } else {
        warn(format!("{} {}", closest.label, closest.left_text()), false)
    }
}

/// The window closest to its limit across the connected tools, with its tool.
fn closest(inputs: &Inputs<'_>) -> Option<(AvatarTool, Window)> {
    inputs
        .tools
        .iter()
        .flat_map(|(tool, t)| windows(t, inputs.now).into_iter().map(|w| (*tool, w)))
        .min_by(|a, b| a.1.closeness().total_cmp(&b.1.closeness()))
}

fn plural(n: u32, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn hover(inputs: &Inputs<'_>, running: u32) -> Hover {
    let title = format!(
        "{} running across all projects",
        plural(running, "agent", "agents")
    );
    // Projects with a running agent, oldest first, every tool together.
    let mut projects: Vec<(&str, u32)> = Vec::new();
    for p in inputs.tools.iter().flat_map(|(_, t)| &t.by_project) {
        match projects.iter_mut().find(|(name, _)| *name == p.name) {
            Some((_, n)) => *n += p.running,
            None => projects.push((&p.name, p.running)),
        }
    }
    let mut parts: Vec<String> = projects
        .into_iter()
        .map(|(name, n)| format!("{name} {n}"))
        .collect();
    if let Some(n) = processes::count(inputs.runs) {
        parts.push(plural(n, "process", "processes"));
    }
    let mut lines = Vec::new();
    if !parts.is_empty() {
        lines.push(parts.join(" · "));
    }
    if let Some((tool, w)) = closest(inputs) {
        lines.push(format!(
            "{} · {}: {}{}",
            tool.name(),
            w.label,
            w.left_text(),
            resets(w.resets_at, inputs.now)
                .map(|r| format!(" · {r}"))
                .unwrap_or_default()
        ));
    }
    Hover { title, lines }
}

/// "resets 3:45 pm".
fn resets(at: Option<i64>, now: i64) -> Option<String> {
    at.map(|at| format!("resets {}", reset_label(at, now)))
}

/// AGENTS RUNNING: the count, and one row per tool only when more than one is connected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityAgents")]
pub struct Agents {
    pub title: &'static str,
    pub count: String,
    pub rows: Vec<ToolCount>,
    /// [`nothing_running`] at 0.
    pub empty: Option<String>,
}

/// One tool's running agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityToolCount")]
pub struct ToolCount {
    pub name: &'static str,
    /// Hidden while only one tool is connected (T-0321).
    pub avatar: Option<ToolAvatarView>,
    pub count: String,
}

/// One limit window's row: "5h · 18% left · resets 3:45 pm" with a bar of what's left.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityWindow")]
pub struct WindowRow {
    /// "5h", "week".
    pub label: String,
    /// "18% left", or "limit reached".
    pub left: String,
    /// "resets 3:45 pm", "resets Mon", "resets Nov 1"; `None` when unknown or passed.
    pub reset: Option<String>,
    /// The bar's filled share, what's left, 0–100.
    pub fill: u32,
    /// [`WARN_AT_LEFT`] % or less left, or reached: amber ([`WARNING_TOKEN`]).
    pub amber: bool,
}

/// One tool's block under USAGE · WHAT'S LEFT. Its name heads it, also with one tool (d0087).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityToolUsage")]
pub struct ToolUsage {
    /// "Claude Code".
    pub name: &'static str,
    /// Hidden while only one tool is connected (T-0321).
    pub avatar: Option<ToolAvatarView>,
    /// "1.21M today".
    pub tokens: String,
    pub windows: Vec<WindowRow>,
    /// "credits: 120", "40 / 200 AI credits" (used of the allowance), [`NO_PLAN`] or [`NOT_REPORTED`].
    pub notes: Vec<String>,
}

/// USAGE · WHAT'S LEFT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityUsage")]
pub struct Usage {
    pub title: &'static str,
    /// "Today 1.93M tokens".
    pub today: String,
    pub tools: Vec<ToolUsage>,
}

/// PROCESSES: today's rows with Open, Stop and Stop all (RUN-04, unchanged).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityProcesses")]
pub struct Processes {
    pub title: &'static str,
    pub count: String,
    pub list: ProcessesView,
}

/// The popover's last line: "Open Terminals ⌃`".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityLink")]
pub struct Link {
    pub label: &'static str,
    pub keys: Option<String>,
}

/// The Activity popover, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityPopover")]
pub struct Popover {
    /// The agent-limit stepper, first, while limits are on (`shell.step_limit` presses it).
    pub limits: Option<limits::Popover>,
    pub agents: Agents,
    pub usage: Usage,
    /// `None` while nothing runs.
    pub processes: Option<Processes>,
    pub open_terminals: Link,
}

fn credits(window: &LimitWindow) -> Option<String> {
    let c = window.credits?;
    let n = |v: f64| v.max(0.).round() as u64;
    Some(match c.total {
        Some(total) => format!("{} / {} AI credits", n(c.used), n(total)),
        None => format!("credits: {}", n(c.used)),
    })
}

pub fn popover(inputs: &Inputs<'_>, keys: &KeyLabels) -> Popover {
    let shown = tool_avatar::shown_with(&inputs.tools.iter().map(|(t, _)| *t).collect::<Vec<_>>());
    let avatar = |tool: AvatarTool| shown.then(|| ToolAvatarView::of(tool, None));
    let running = inputs.running();
    let agents = Agents {
        title: "AGENTS RUNNING",
        count: running.to_string(),
        rows: if inputs.names_tools() && running > 0 {
            inputs
                .tools
                .iter()
                .map(|(tool, t)| ToolCount {
                    name: tool.name(),
                    avatar: avatar(*tool),
                    count: t.running.to_string(),
                })
                .collect()
        } else {
            Vec::new()
        },
        empty: (running == 0).then(|| nothing_running(inputs.waiting)),
    };
    let tools = inputs
        .tools
        .iter()
        .map(|(tool, t)| {
            let rows = windows(t, inputs.now)
                .into_iter()
                .map(|w| WindowRow {
                    left: w.left_text(),
                    reset: resets(w.resets_at, inputs.now),
                    fill: if w.reached { 0 } else { w.left.round() as u32 },
                    amber: w.warns(),
                    label: w.label,
                })
                .collect();
            let notes = match &t.limits {
                ToolLimits::Windows(ws) => ws.iter().filter_map(credits).collect(),
                ToolLimits::NoPlanWindows => vec![NO_PLAN.to_owned()],
                ToolLimits::NotReported => vec![NOT_REPORTED.to_owned()],
            };
            ToolUsage {
                name: tool.name(),
                avatar: avatar(*tool),
                tokens: format!("{} today", tokens_label(t.tokens_today)),
                windows: rows,
                notes,
            }
        })
        .collect();
    let processes = processes::count(inputs.runs).map(|n| Processes {
        title: "PROCESSES",
        count: n.to_string(),
        list: run_page::processes_view(inputs.runs, inputs.now),
    });
    Popover {
        limits: inputs
            .limits
            .enabled
            .then(|| limits::popover(inputs.limits)),
        agents,
        usage: Usage {
            title: "USAGE · WHAT'S LEFT",
            today: format!("Today {} tokens", tokens_label(inputs.tokens_today)),
            tools,
        },
        processes,
        open_terminals: Link {
            label: "Open Terminals",
            keys: keys.label(KeyAction::Terminals),
        },
    }
}

/// The gallery's Activity states, from [`fixtures`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ActivityGallery")]
pub struct Gallery {
    /// *Activity button · states*: calm, 18 % left (two tools, then one), at a limit (two tools, then one), limits
    /// on, nothing running.
    pub buttons: Vec<(String, Button)>,
    /// *Activity popover states*: two windows, a tool at its limit, an API-key user, limits not reported, limits
    /// on, every tool, one tool connected · nothing running.
    pub popovers: Vec<(String, Popover)>,
}

/// The gallery, on the Mac's keys.
pub fn gallery() -> Gallery {
    let f = fixtures::Fixtures::new();
    let button_of = |caption: &str, s: &fixtures::State| {
        (
            caption.to_owned(),
            button(&s.inputs(), &KeyLabels::base(Platform::Mac)),
        )
    };
    let popover_of = |caption: &str, s: &fixtures::State| {
        (
            caption.to_owned(),
            popover(&s.inputs(), &KeyLabels::base(Platform::Mac)),
        )
    };
    Gallery {
        buttons: vec![
            button_of("Calm (every tool above 20 % left)", &f.calm),
            button_of("A tool has 20 % or less left", &f.low_two),
            button_of("Only one tool connected", &f.low_one),
            button_of("A tool hit its limit", &f.limit_two),
            button_of("One tool at its limit", &f.limit_one),
            button_of("Agent limits on", &f.limits_on),
            button_of("Nothing running", &f.idle),
        ],
        popovers: vec![
            popover_of("Two windows", &f.low_one),
            popover_of("A tool at its limit", &f.limit_two),
            popover_of("API-key user", &f.api_key),
            popover_of("Limits not reported", &f.not_reported),
            popover_of("Agent limits on", &f.limits_on),
            popover_of("Every tool", &f.four),
            popover_of("One tool connected · nothing running", &f.idle),
        ],
    }
}

/// Sample data for the gallery and the tests: pocket-notes and notes-api (the M21 example), a fixed clock, Claude
/// Code's 5h and week windows, and other tools standing in for M23's.
pub mod fixtures {
    use otaq_core::agent_tools::{Credits, LimitWindow, Tool};
    use otaq_core::board::settings::AgentLimits;
    use otaq_core::terminals::run::{RunInfo, RunState};
    use otaq_core::usage::activity::{ProjectRunning, ToolActivity, ToolLimits};
    use otaq_core::usage::limit::LimitKind;
    use otaq_core::usage::plan::ActiveLimit;

    use super::{Inputs, ToolEntry};
    use crate::view_models::tool_avatar::AvatarTool;

    /// A fixed moment (2026-10-08, around noon UTC).
    pub const NOW: i64 = 1_791_547_200_000;
    const MINUTE: i64 = 60_000;
    const DAY: i64 = 86_400_000;

    pub fn window(
        key: &str,
        minutes: Option<u32>,
        used: f64,
        resets_in: Option<i64>,
    ) -> LimitWindow {
        LimitWindow {
            key: key.into(),
            length_minutes: minutes,
            used_percent: used,
            resets_at: resets_in.map(|ms| NOW + ms),
            credits: None,
        }
    }

    /// Claude Code's 5h and week windows, `five` and `week` used.
    pub fn claude_windows(five: f64, week: f64) -> ToolLimits {
        ToolLimits::Windows(vec![
            window("five_hour", Some(300), five, Some(80 * MINUTE)),
            window("seven_day", Some(7 * 24 * 60), week, Some(3 * DAY)),
        ])
    }

    pub fn tool(running: &[(&str, u32)], tokens: u64, limits: ToolLimits) -> ToolActivity {
        ToolActivity {
            tool: Tool::Claude,
            running: running.iter().map(|(_, n)| n).sum(),
            by_project: running
                .iter()
                .map(|(name, n)| ProjectRunning {
                    key: name.to_uppercase(),
                    name: (*name).to_owned(),
                    running: *n,
                })
                .collect(),
            tokens_today: tokens,
            limits,
            limit: None,
        }
    }

    pub fn limited(mut t: ToolActivity, kind: LimitKind) -> ToolActivity {
        t.limit = Some(ActiveLimit {
            tool: Tool::Claude,
            kind,
            text: "You've hit your limit".into(),
            hit_at: NOW - 10 * MINUTE,
            resets_at: Some(NOW + 80 * MINUTE),
            reset_source: None,
            reset_seen_at: None,
        });
        t
    }

    pub fn run(key: &str, port: u16) -> RunInfo {
        RunInfo {
            project: "NOTE".into(),
            project_name: "pocket-notes".into(),
            key: key.parse().unwrap(),
            title: format!("{key} title"),
            state: RunState::Running,
            port: Some(port),
            command: "npm run dev".into(),
            open_url: Some("http://localhost:$PORT".into()),
            session: None,
            started_at: Some(NOW - 12 * MINUTE),
            ended_at: None,
            exit_code: None,
            stopped_by: None,
        }
    }

    /// One state: its tools, limits and runs.
    pub struct State {
        pub tools: Vec<(AvatarTool, ToolActivity)>,
        pub limits: AgentLimits,
        pub runs: Vec<RunInfo>,
        entries: Vec<(AvatarTool, usize)>,
    }

    impl State {
        pub fn new(
            tools: Vec<(AvatarTool, ToolActivity)>,
            limits: AgentLimits,
            runs: Vec<RunInfo>,
        ) -> Self {
            let entries = tools
                .iter()
                .enumerate()
                .map(|(i, (t, _))| (*t, i))
                .collect();
            Self {
                tools,
                limits,
                runs,
                entries,
            }
        }

        pub fn entries(&self) -> Vec<ToolEntry<'_>> {
            self.entries
                .iter()
                .map(|(t, i)| (*t, &self.tools[*i].1))
                .collect()
        }

        pub fn inputs(&self) -> Inputs<'_> {
            Inputs {
                tools: self.entries(),
                tokens_today: self.tools.iter().map(|(_, t)| t.tokens_today).sum(),
                limits: &self.limits,
                scheduler: None,
                runs: &self.runs,
                now: NOW,
                waiting: otaq_core::flow::model::READY_FOR_DEV,
            }
        }
    }

    /// The gallery's states.
    pub struct Fixtures {
        pub calm: State,
        pub low_two: State,
        pub low_one: State,
        pub limit_two: State,
        pub limit_one: State,
        pub limits_on: State,
        pub api_key: State,
        pub not_reported: State,
        pub idle: State,
        /// Every tool: Claude Code, Codex with its week, Copilot's month with credits, OpenCode not reported.
        pub four: State,
    }

    impl Fixtures {
        pub fn new() -> Self {
            let off = AgentLimits::default();
            let on = AgentLimits {
                enabled: true,
                ..AgentLimits::default()
            };
            let runs = || vec![run("NOTE-5", 4020), run("NOTE-6", 4031)];
            let cc = |five, week| {
                tool(
                    &[("pocket-notes", 2), ("notes-api", 1)],
                    1_210_000,
                    claude_windows(five, week),
                )
            };
            let codex = || {
                let mut five = window("primary", Some(300), 35., Some(4 * 60 * MINUTE));
                five.credits = Some(Credits {
                    used: 120.,
                    total: None,
                });
                tool(
                    &[("pocket-notes", 2)],
                    520_000,
                    ToolLimits::Windows(vec![
                        five,
                        window("secondary", Some(7 * 24 * 60), 12., Some(5 * DAY)),
                    ]),
                )
            };
            let opencode = || tool(&[], 90_000, ToolLimits::NotReported);
            let copilot = || {
                let mut month = window("chat", Some(30 * 24 * 60), 20., Some(24 * DAY));
                month.credits = Some(Credits {
                    used: 40.,
                    total: Some(200.),
                });
                tool(
                    &[("pocket-notes", 1)],
                    110_000,
                    ToolLimits::Windows(vec![month]),
                )
            };
            let pair =
                |cc: ToolActivity| vec![(AvatarTool::ClaudeCode, cc), (AvatarTool::Codex, codex())];
            Self {
                calm: State::new(pair(cc(40., 30.)), off, runs()),
                low_two: State::new(pair(cc(82., 40.)), off, runs()),
                low_one: State::new(vec![(AvatarTool::ClaudeCode, cc(82., 40.))], off, runs()),
                limit_two: State::new(
                    pair(limited(cc(100., 48.), LimitKind::Session)),
                    off,
                    runs(),
                ),
                limit_one: State::new(
                    vec![(
                        AvatarTool::ClaudeCode,
                        limited(cc(100., 48.), LimitKind::Session),
                    )],
                    off,
                    runs(),
                ),
                limits_on: State::new(vec![(AvatarTool::ClaudeCode, cc(40., 30.))], on, runs()),
                api_key: State::new(
                    vec![
                        (
                            AvatarTool::ClaudeCode,
                            tool(&[("pocket-notes", 3)], 1_210_000, ToolLimits::NoPlanWindows),
                        ),
                        (AvatarTool::Codex, codex()),
                    ],
                    off,
                    runs(),
                ),
                not_reported: State::new(
                    vec![
                        (AvatarTool::ClaudeCode, cc(40., 30.)),
                        (AvatarTool::OpenCode, opencode()),
                    ],
                    off,
                    runs(),
                ),
                four: State::new(
                    vec![
                        (AvatarTool::ClaudeCode, cc(40., 30.)),
                        (AvatarTool::Codex, codex()),
                        (AvatarTool::Copilot, copilot()),
                        (AvatarTool::OpenCode, opencode()),
                    ],
                    off,
                    runs(),
                ),
                idle: State::new(
                    vec![(
                        AvatarTool::ClaudeCode,
                        tool(
                            &[],
                            0,
                            ToolLimits::Windows(vec![
                                window("five_hour", Some(300), 64., Some(-5 * MINUTE)),
                                window("seven_day", Some(7 * 24 * 60), 26., Some(3 * DAY)),
                            ]),
                        ),
                    )],
                    off,
                    Vec::new(),
                ),
            }
        }
    }

    impl Default for Fixtures {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::agent_tools::Credits;
    use otaq_core::scheduler::SchedulerStatus;

    use super::fixtures::{self, Fixtures, NOW, State};
    use super::*;
    use crate::view_models::clock::clock_12h;

    fn local_clock(at: i64) -> String {
        let t = otaq_core::local_time::local(at).unwrap();
        clock_12h(t.hour, t.minute)
    }

    fn mac_button(s: &State) -> Button {
        button(&s.inputs(), &KeyLabels::base(Platform::Mac))
    }

    fn mac_popover(s: &State) -> Popover {
        popover(&s.inputs(), &KeyLabels::base(Platform::Mac))
    }

    #[test]
    fn calm_button_counts_agents_and_processes_and_says_nothing_about_usage() {
        let f = Fixtures::new();
        let b = mac_button(&f.calm);
        assert_eq!((b.agents.count.as_str(), b.agents.label), ("5", "running"));
        assert!(b.agents.active && !b.agents.control);
        assert_eq!(b.processes.as_deref(), Some("2"));
        assert_eq!(b.warning, None);
        assert_eq!(b.label, "Activity");
        assert_eq!(b.keys.as_deref(), Some("⇧⌘U"));
    }

    /// KEY-08: Activity's tooltip and the popover's Open Terminals show the user's keys: Activity → ⌘J, Terminals'
    /// ⌃` removed (its `g` `t` shows), and with letters off too, none.
    #[test]
    fn the_button_and_the_popover_show_the_users_keys() {
        use crate::key_labels::fixtures as keys;
        let f = Fixtures::new();
        let j = keys::mac(&keys::activity_on_cmd_j());
        assert_eq!(button(&f.calm.inputs(), &j).keys.as_deref(), Some("⌘J"));
        let removed = keys::removed("open_activity", None, 0);
        assert_eq!(
            button(&f.calm.inputs(), &removed).keys.as_deref(),
            Some("g a")
        );
        let none = keys::with(false, &[("open_activity", None, 0, None)]);
        assert_eq!(button(&f.calm.inputs(), &none).keys, None);
        let terminals = keys::removed("terminals", None, 0);
        assert_eq!(
            popover(&f.calm.inputs(), &terminals)
                .open_terminals
                .keys
                .as_deref(),
            Some("g t")
        );
        let neither = keys::with(false, &[("terminals", None, 0, None)]);
        assert_eq!(
            popover(&f.calm.inputs(), &neither).open_terminals.keys,
            None
        );
    }

    #[test]
    fn processes_are_hidden_at_zero() {
        let f = Fixtures::new();
        let b = mac_button(&f.idle);
        assert_eq!(b.processes, None);
        assert_eq!(b.agents.count, "0");
        assert!(!b.agents.active);
    }

    #[test]
    fn at_eighteen_percent_left_the_button_warns_in_amber() {
        let f = Fixtures::new();
        // One tool: the window is named instead of the tool.
        let one = mac_button(&f.low_one).warning.unwrap();
        assert_eq!(one.text, "5h 18% left");
        assert_eq!(one.token, "st-paused");
        // Two tools (a test-only second one): neither is named.
        let two = mac_button(&f.low_two).warning.unwrap();
        assert_eq!(two.text, "18% left");
        assert_eq!(two.token, WARNING_TOKEN);
        assert_ne!(two.token, "st-attn", "never turquoise");
        assert!(!one.limit && !two.limit, "no amber edge before a limit");
    }

    #[test]
    fn the_threshold_is_twenty_percent_left() {
        let at = |used: f64| {
            let s = State::new(
                vec![(
                    AvatarTool::ClaudeCode,
                    fixtures::tool(&[], 0, fixtures::claude_windows(used, 0.)),
                )],
                AgentLimits::default(),
                Vec::new(),
            );
            mac_button(&s).warning.map(|w| w.text)
        };
        assert_eq!(WARN_AT_LEFT, 20.);
        assert_eq!(at(80.), Some("5h 20% left".to_owned()));
        assert_eq!(at(79.9), None);
        assert_eq!(left_percent(17.6), "18% left");
        assert_eq!(left_percent(-3.), "0% left");
    }

    #[test]
    fn at_a_limit_the_button_names_the_tool_or_with_one_tool_the_window() {
        let f = Fixtures::new();
        let two = mac_button(&f.limit_two).warning.unwrap();
        assert_eq!(two.text, "Claude Code limit");
        assert_eq!(two.token, "st-paused", "amber, never turquoise");
        let one = mac_button(&f.limit_one).warning.unwrap();
        assert_eq!(one.text, "5h limit");
        assert_eq!(one.token, "st-paused");
        assert!(one.limit && two.limit, "the amber edge at a limit");
        // The weekly limit is on the week window.
        let mut weekly = f.limit_one.tools[0].1.clone();
        weekly.limit.as_mut().unwrap().kind = LimitKind::Weekly;
        let s = State::new(
            vec![(AvatarTool::ClaudeCode, weekly)],
            AgentLimits::default(),
            Vec::new(),
        );
        assert_eq!(mac_button(&s).warning.unwrap().text, "week limit");
    }

    #[test]
    fn with_limits_on_the_button_reads_slots_out_of_the_max() {
        let f = Fixtures::new();
        assert_eq!(mac_button(&f.limits_on).agents.count, "3/5");
        let status = SchedulerStatus {
            holding: 4,
            ..SchedulerStatus::default()
        };
        let mut inputs = f.limits_on.inputs();
        inputs.scheduler = Some(&status);
        let b = button(&inputs, &KeyLabels::base(Platform::Mac));
        assert_eq!((b.agents.count.as_str(), b.agents.label), ("4/5", "agents"));
        assert!(b.agents.control);
    }

    #[test]
    fn the_hover_splits_by_project_and_names_the_closest_window() {
        let f = Fixtures::new();
        let h = mac_button(&f.low_one).hover;
        assert_eq!(h.title, "3 agents running across all projects");
        assert_eq!(
            h.lines,
            [
                "pocket-notes 2 · notes-api 1 · 2 processes".to_owned(),
                format!(
                    "Claude Code · 5h: 18% left · resets {}",
                    local_clock(NOW + 80 * 60_000)
                ),
            ]
        );
        // Across tools, projects add up; the closest window wins whichever tool it is.
        let h = mac_button(&f.low_two).hover;
        assert_eq!(h.title, "5 agents running across all projects");
        assert_eq!(h.lines[0], "pocket-notes 4 · notes-api 1 · 2 processes");
        assert!(h.lines[1].starts_with("Claude Code · 5h: 18% left"));
        // At a limit: the window says so.
        let h = mac_button(&f.limit_one).hover;
        assert!(h.lines[1].starts_with("Claude Code · 5h: limit reached · resets "));
        // Nothing running, no processes: only the title and the window.
        let h = mac_button(&f.idle).hover;
        assert_eq!(h.title, "0 agents running across all projects");
        assert_eq!(h.lines.len(), 1);
        assert!(
            h.lines[0].starts_with("Claude Code · week: 74% left"),
            "{:?}",
            h.lines
        );
    }

    #[test]
    fn the_popover_lists_two_windows_with_what_is_left() {
        let f = Fixtures::new();
        let p = mac_popover(&f.low_one);
        assert_eq!(p.limits, None, "limits off: no stepper");
        assert_eq!(p.agents.title, "AGENTS RUNNING");
        assert_eq!(p.agents.count, "3");
        assert!(p.agents.rows.is_empty(), "one tool: no per-tool rows");
        assert_eq!(p.agents.empty, None);
        assert_eq!(p.usage.title, "USAGE · WHAT'S LEFT");
        assert_eq!(p.usage.today, "Today 1.21M tokens");
        let cc = &p.usage.tools[0];
        assert_eq!(
            cc.name, "Claude Code",
            "the name heads the block with one tool"
        );
        assert_eq!(cc.avatar, None, "avatars hidden with one tool");
        assert_eq!(cc.tokens, "1.21M today");
        assert_eq!(
            cc.windows[0],
            WindowRow {
                label: "5h".into(),
                left: "18% left".into(),
                reset: Some(format!("resets {}", local_clock(NOW + 80 * 60_000))),
                fill: 18,
                amber: true,
            }
        );
        let week = &cc.windows[1];
        assert_eq!(
            (week.label.as_str(), week.left.as_str()),
            ("week", "60% left")
        );
        assert!(!week.amber);
        let t = otaq_core::local_time::local(NOW + 3 * 86_400_000).unwrap();
        assert_eq!(
            week.reset.as_deref(),
            Some(
                format!(
                    "resets {}",
                    crate::view_models::clock::WEEKDAYS[t.weekday as usize]
                )
                .as_str()
            )
        );
        assert!(cc.notes.is_empty());
        let processes = p.processes.unwrap();
        assert_eq!(
            (processes.title, processes.count.as_str()),
            ("PROCESSES", "2")
        );
        assert_eq!(processes.list.rows[0].open.as_deref(), Some("Open :4020"));
        assert_eq!(processes.list.stop_all, "Stop all");
        assert_eq!(p.open_terminals.label, "Open Terminals");
        assert_eq!(p.open_terminals.keys.as_deref(), Some("⌃`"));
    }

    #[test]
    fn the_popover_at_a_limit_says_limit_reached() {
        let f = Fixtures::new();
        let p = mac_popover(&f.limit_two);
        assert_eq!(p.agents.count, "5");
        let rows: Vec<_> = p
            .agents
            .rows
            .iter()
            .map(|r| (r.name, r.count.as_str()))
            .collect();
        assert_eq!(rows, [("Claude Code", "3"), ("Codex", "2")]);
        assert!(
            p.agents.rows[0].avatar.is_some(),
            "two tools: avatars shown"
        );
        let five = &p.usage.tools[0].windows[0];
        assert_eq!(five.left, LIMIT_REACHED);
        assert!(five.amber);
        assert_eq!(five.fill, 0);
        assert_eq!(
            five.reset,
            Some(format!("resets {}", local_clock(NOW + 80 * 60_000)))
        );
        assert_eq!(p.usage.tools[0].windows[1].left, "52% left");
        assert_eq!(p.usage.tools[1].notes, ["credits: 120"]);
        assert_eq!(p.usage.today, "Today 1.73M tokens");
    }

    #[test]
    fn an_api_key_user_has_tokens_only() {
        let f = Fixtures::new();
        let p = mac_popover(&f.api_key);
        let cc = &p.usage.tools[0];
        assert!(cc.windows.is_empty());
        assert_eq!(cc.notes, [NO_PLAN]);
        assert_eq!(cc.notes[0], "No plan limit · tokens only");
        assert_eq!(cc.tokens, "1.21M today");
        assert_eq!(mac_button(&f.api_key).warning, None);
    }

    #[test]
    fn a_tool_that_does_not_report_limits_says_so() {
        let f = Fixtures::new();
        let p = mac_popover(&f.not_reported);
        let oc = &p.usage.tools[1];
        assert_eq!(oc.name, "OpenCode");
        assert_eq!(oc.notes, ["Limits not reported"]);
        assert_eq!(oc.tokens, "90k today");
        assert!(oc.windows.is_empty());
        assert_eq!(p.agents.rows[1].count, "0");
    }

    #[test]
    fn every_tool_with_its_own_windows() {
        let f = Fixtures::new();
        let p = mac_popover(&f.four);
        let names: Vec<&str> = p.usage.tools.iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            ["Claude Code", "Codex", "GitHub Copilot", "OpenCode"]
        );
        // Running agents per tool, today's tokens per tool.
        let running: Vec<(&str, &str)> = p
            .agents
            .rows
            .iter()
            .map(|r| (r.name, r.count.as_str()))
            .collect();
        assert_eq!(
            running,
            [
                ("Claude Code", "3"),
                ("Codex", "2"),
                ("GitHub Copilot", "1"),
                ("OpenCode", "0")
            ]
        );
        let tokens: Vec<&str> = p.usage.tools.iter().map(|t| t.tokens.as_str()).collect();
        assert_eq!(
            tokens,
            ["1.21M today", "520k today", "110k today", "90k today"]
        );
        // Codex: its week.
        let codex = &p.usage.tools[1];
        assert_eq!(
            (
                codex.windows[1].label.as_str(),
                codex.windows[1].left.as_str()
            ),
            ("week", "88% left")
        );
        assert_eq!(codex.notes, ["credits: 120"]);
        // Copilot: one month window with its credits.
        let copilot = &p.usage.tools[2];
        assert_eq!(copilot.windows.len(), 1);
        let month = &copilot.windows[0];
        assert_eq!(
            (month.label.as_str(), month.left.as_str()),
            ("month", "80% left")
        );
        assert_eq!(
            month.reset,
            Some(format!(
                "resets {}",
                crate::view_models::clock::reset_label(NOW + 24 * 86_400_000, NOW)
            ))
        );
        assert_eq!(copilot.notes, ["40 / 200 AI credits"]);
        // OpenCode: tokens, limits not reported.
        let oc = &p.usage.tools[3];
        assert_eq!(oc.notes, [NOT_REPORTED]);
        assert!(oc.windows.is_empty());
        assert_eq!(p.usage.today, "Today 1.93M tokens");
        // At Codex's limit the button names it.
        let mut limited = Fixtures::new().four;
        limited.tools[1].1.limit =
            fixtures::limited(limited.tools[1].1.clone(), LimitKind::Weekly).limit;
        assert_eq!(mac_button(&limited).warning.unwrap().text, "Codex limit");
    }

    #[test]
    fn with_limits_on_the_stepper_comes_first() {
        let f = Fixtures::new();
        let p = mac_popover(&f.limits_on);
        let stepper = p.limits.unwrap();
        assert_eq!(stepper.title, "Agent limits");
        assert_eq!(stepper.max_agents.value, 5);
        assert_eq!(stepper.back_pressure.value, 3);
        // The stepper is the first field on the wire, so it is drawn first.
        let json = serde_json::to_string(&mac_popover(&f.limits_on)).unwrap();
        assert!(
            json.starts_with(r#"{"limits":{"title":"Agent limits""#),
            "{json}"
        );
    }

    #[test]
    fn nothing_running_says_what_starts_next() {
        let f = Fixtures::new();
        let p = mac_popover(&f.idle);
        assert_eq!(p.agents.count, "0");
        assert!(p.agents.rows.is_empty());
        assert_eq!(
            p.agents.empty.as_deref(),
            Some(
                "Nothing running. Tickets in Ready for Dev start when a slot and their parents allow."
            )
        );
        assert_eq!(
            nothing_running("Ready"),
            "Nothing running. Tickets in Ready start when a slot and their parents allow."
        );
        assert_eq!(p.usage.today, "Today 0 tokens");
        let cc = &p.usage.tools[0];
        assert_eq!(cc.tokens, "0 today");
        // The 5h window's reset has passed: all of it is left, with no reset time.
        assert_eq!(cc.windows[0].left, "100% left");
        assert_eq!(cc.windows[0].reset, None);
        assert_eq!(cc.windows[1].left, "74% left");
        assert_eq!(p.processes, None);
    }

    #[test]
    fn windows_are_named_by_their_length() {
        let w = |m| fixtures::window("k", m, 0., None);
        assert_eq!(window_label(&w(Some(300))), "5h");
        assert_eq!(window_label(&w(Some(7 * 24 * 60))), "week");
        assert_eq!(window_label(&w(Some(30 * 24 * 60))), "month");
        assert_eq!(window_label(&w(Some(90))), "90 min");
        assert_eq!(window_label(&w(Some(2 * 24 * 60))), "2d");
        assert_eq!(window_label(&w(None)), "k");
    }

    #[test]
    fn credits_with_an_allowance_say_what_is_left() {
        let mut w = fixtures::window("month", None, 20., None);
        w.credits = Some(Credits {
            used: 40.,
            total: Some(200.),
        });
        assert_eq!(credits(&w).as_deref(), Some("40 / 200 AI credits"));
    }

    #[test]
    fn no_dollar_figure_anywhere() {
        let g = gallery();
        let json = serde_json::to_string(&g).unwrap();
        assert!(!json.contains('$'), "{json}");
        assert_eq!(g.buttons.len(), 7);
        assert_eq!(g.popovers.len(), 7);
    }

    #[test]
    fn only_connected_tools_are_listed() {
        let activity = Activity {
            day: "2026-10-08".into(),
            tools: vec![fixtures::tool(
                &[("pocket-notes", 1)],
                5,
                ToolLimits::NoPlanWindows,
            )],
            tokens_today: 5,
        };
        let tools = connected(&activity);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].0, AvatarTool::ClaudeCode);
    }
}

//! OS notifications (ATT-11; d0016, d0042, d0090): only for items that **block an agent** (the needs-you family and
//! Interrupted), never for Ready for Review, a ticket whose terminal has focus or an agent stopped by the plan
//! limit (both already left out of the needs-you items), and **once per item**, not per event. A global switch
//! turns them off (on by default), one checkbox per [`Kind`] leaves a kind out, and both apply at the next
//! derivation.
//!
//! An item is the ticket in that status since that moment: the same question still waiting after the user looked
//! at its terminal and left is not notified again; a new question later is. More than three new items of one
//! project at once become one notification for the project (frame *11 → OS notifications*: "grouped per project
//! after 3").
//!
//! From M12 (d0063, d0064; frame *11 → OS notifications → macOS · M12*): a validator's permission prompt,
//! "Validation failed N times", "Validation ended without a result" and the Auto-merge stops notify too, as turquoise
//! items; failed rounds that go back to the agent and auto-merges raise no item, so they never notify.
//!
//! From M25 (d0090): a notification says what happened (title), where (subtitle: the project and the ticket title,
//! agent name) and what the user is asked (body); it is threaded per project, carries its [`Target`] (what a
//! click opens) and plays the default sound unless Settings turns it off. [`pace`] holds new ones for 2 minutes after
//! a pop-up.
//!
//! [`should_notify`] and [`plan`] are the pure decision; [`Notifier`] posts. Inside `Otaq.app` macOS posts through
//! its notification API under the bundle's ID ([`macos`]), and a click reaches the service's delegate, which opens
//! the target ([`Opener::open`]). Outside the bundle the macOS notifier runs `osascript`, whose notifications belong
//! to Script Editor (a click can't open a ticket that way, Q22). On Linux `notify-send` posts through the desktop's
//! notification service and reports a click (T-0159, Q34). On Windows a toast is shown under Windows PowerShell's
//! identity; Windows hands its click only to a registered app, so there a click doesn't reach Otaq.

#[cfg(target_os = "macos")]
pub mod macos;
pub mod pace;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::agent_tools::Tool;
use crate::board::needs_you::{NeedsYouGroup, NeedsYouItem};
use crate::board::settings::{GlobalSettings, NotificationKinds};
use crate::data_dir::DataDir;
use crate::domain::{Status, TicketKey, validation};
use crate::naming;
use crate::naming::PRODUCT_NAME;

/// More new items than this in one project at once are grouped into one notification.
pub const GROUP_AFTER: usize = 3;

/// What a notification is about: one Settings checkbox each (d0090).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Permission prompts of agents, validators and Project agents.
    Permission,
    /// Questions of agents and Project agents.
    Question,
    /// An agent ended its turn waiting for the user (Idle).
    TurnEnded,
    /// A session was interrupted.
    Interrupted,
    /// Validation failed N times, or ended without a result.
    ValidationDecision,
    AutoMergeStopped,
    /// An agent tool isn't available.
    ToolUnavailable,
}

impl Kind {
    pub const ALL: [Self; 7] = [
        Self::Permission,
        Self::Question,
        Self::TurnEnded,
        Self::Interrupted,
        Self::ValidationDecision,
        Self::AutoMergeStopped,
        Self::ToolUnavailable,
    ];

    /// The kind of a ticket's item in `status`.
    pub fn of_status(status: Status) -> Self {
        match status {
            Status::NeedsYou | Status::CheckerNeedsYou => Self::Permission,
            Status::AsksYou => Self::Question,
            Status::Idle => Self::TurnEnded,
            Status::CheckFailed | Status::CheckNoResult => Self::ValidationDecision,
            Status::DeliveryStopped => Self::AutoMergeStopped,
            _ => Self::Interrupted,
        }
    }
}

/// What a click on a notification opens (d0090 "What a click opens"). An item answered before the click opens the
/// same place.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    /// The ticket's sheet on the Session tab, its terminal focused.
    Ticket { key: TicketKey },
    /// The ticket's validator terminal, focused.
    Validator { key: TicketKey },
    /// The ticket's Files tab, to review it (the status icon's "your check" line, d0139 TRY-06).
    Review { key: TicketKey },
    /// The ticket's sheet on the tab it opens on, nothing started or resumed (the status icon's Interrupted and Start
    /// failed lines, d0139 TRY-06).
    Sheet { key: TicketKey },
    /// The Project agent in the agent panel, focused.
    ProjectAgent { project: String, agent: String },
    /// Needs you, the strip (grouped notifications and summaries).
    NeedsYou,
    /// The board (a tool that isn't available: its limit banner explains it), of `project` when one is named.
    Board {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
}

/// The value of [`open_env_var`] that opens Needs you.
pub const OPEN_NEEDS_YOU: &str = "needs_you";
const OPEN_VALIDATOR: &str = "validator:";
const OPEN_REVIEW: &str = "review:";
const OPEN_SHEET: &str = "sheet:";
const OPEN_AGENT: &str = "agent:";
const OPEN_BOARD: &str = "board";

impl Target {
    /// The target as one string, for [`open_env_var`] and a notification's `userInfo`: `SHOP-22` (a ticket, as
    /// before M25), `validator:SHOP-22`, `review:SHOP-22`, `sheet:SHOP-22`, `agent:SHOP:pa7`, `needs_you`, `board` or
    /// `board:SHOP`.
    pub fn to_open_value(&self) -> String {
        match self {
            Self::Ticket { key } => key.to_string(),
            Self::Validator { key } => format!("{OPEN_VALIDATOR}{key}"),
            Self::Review { key } => format!("{OPEN_REVIEW}{key}"),
            Self::Sheet { key } => format!("{OPEN_SHEET}{key}"),
            Self::ProjectAgent { project, agent } => format!("{OPEN_AGENT}{project}:{agent}"),
            Self::NeedsYou => OPEN_NEEDS_YOU.to_owned(),
            Self::Board { project: None } => OPEN_BOARD.to_owned(),
            Self::Board {
                project: Some(project),
            } => format!("{OPEN_BOARD}:{project}"),
        }
    }

    /// Reads [`Self::to_open_value`]'s string back; anything else opens nothing.
    pub fn parse_open_value(value: &str) -> Option<Self> {
        let value = value.trim();
        if value == OPEN_NEEDS_YOU {
            return Some(Self::NeedsYou);
        }
        if value == OPEN_BOARD {
            return Some(Self::Board { project: None });
        }
        if let Some(project) = value
            .strip_prefix(OPEN_BOARD)
            .and_then(|r| r.strip_prefix(':'))
        {
            return (!project.is_empty()).then(|| Self::Board {
                project: Some(project.to_owned()),
            });
        }
        if let Some(key) = value.strip_prefix(OPEN_VALIDATOR) {
            return key.parse().ok().map(|key| Self::Validator { key });
        }
        if let Some(key) = value.strip_prefix(OPEN_REVIEW) {
            return key.parse().ok().map(|key| Self::Review { key });
        }
        if let Some(key) = value.strip_prefix(OPEN_SHEET) {
            return key.parse().ok().map(|key| Self::Sheet { key });
        }
        if let Some(rest) = value.strip_prefix(OPEN_AGENT) {
            let (project, agent) = rest.split_once(':')?;
            return (!project.is_empty() && !agent.is_empty()).then(|| Self::ProjectAgent {
                project: project.to_owned(),
                agent: agent.to_owned(),
            });
        }
        value.parse().ok().map(|key| Self::Ticket { key })
    }
}

/// One OS notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsNotification {
    /// What happened: "SHOP-22 needs permission".
    pub title: String,
    /// Where: "shop-web · Fix flaky login redirect"; empty for a summary or a tool that isn't available.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subtitle: String,
    /// What the user is asked, whole (the OS cuts): "Allow `rm -rf .next`?".
    pub body: String,
    /// The Settings checkbox it belongs to; `None` for one that stands for several items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    /// What a click opens.
    pub target: Target,
    /// The project key it is grouped under in the notification list; `None` across projects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    /// The OS default sound (Settings → "Play a sound").
    pub sound: bool,
}

/// What a notification says beyond its item: the question, prompt, last line or stop message (for "Validation
/// failed N times", the first finding) and N.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemText {
    pub text: Option<String>,
    /// "Validation failed N times": the rounds that failed.
    pub failed_times: u32,
}

impl From<&str> for ItemText {
    fn from(text: &str) -> Self {
        Self {
            text: Some(text.to_owned()),
            failed_times: 0,
        }
    }
}

/// An item, as far as "once per item" is concerned.
pub type ItemId = (String, Status, i64);

pub fn item_id(item: &NeedsYouItem) -> ItemId {
    (item.id.clone(), item.status, item.since)
}

/// The item as one string, to tell whether it is still waiting ([`Held::presence`]).
pub fn presence_of(item: &NeedsYouItem) -> String {
    format!("ticket:{}:{:?}:{}", item.id, item.status, item.since)
}

/// A waiting Project agent as one string ([`Held::presence`]).
pub fn agent_presence(agent: &crate::project_agents::AgentNeedsYou) -> String {
    format!("agent:{}:{:?}:{}", agent.agent, agent.status, agent.since)
}

/// A tool that isn't available, since `since`, as one string ([`Held::presence`]).
pub fn tool_presence(tool: Tool, since: i64) -> String {
    format!("tool:{tool:?}:{since}")
}

/// What the Settings say about notifications, read at every derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choices {
    /// The main switch.
    pub enabled: bool,
    pub kinds: NotificationKinds,
    pub sound: bool,
}

impl Choices {
    pub fn of(settings: &GlobalSettings) -> Self {
        Self {
            enabled: settings.notifications,
            kinds: settings.notification_kinds,
            sound: settings.notification_sound,
        }
    }

    /// Whether a notification of `kind` goes out.
    pub fn allows(&self, kind: Kind) -> bool {
        self.enabled && self.kinds.allows(kind)
    }
}

impl Default for Choices {
    fn default() -> Self {
        Self::of(&GlobalSettings::default())
    }
}

/// A notification for one item, before [`group`] and the quiet period ([`pace`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub notification: OsNotification,
    pub item: Held,
}

/// What a summary and the 3-at-once rule need of an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    /// The item as one string: an item no longer among the waiting ones was answered ([`presence_of`]).
    pub presence: String,
    /// The project's key and name; `None` for a tool that isn't available.
    pub project: Option<(String, String)>,
    /// How a summary names it: the ticket key, the agent's name, the tool's name.
    pub label: String,
    /// A ticket's item: more than [`GROUP_AFTER`] of one project at once are grouped.
    pub ticket: bool,
}

/// Whether an item blocks an agent: turquoise, or Interrupted (d0016).
pub fn should_notify(item: &NeedsYouItem) -> bool {
    // An agent's question to a finished ticket blocks no agent (the ask is non-blocking): no notification.
    item.message.is_none()
        && (item.group == NeedsYouGroup::Family || item.status == Status::Interrupted)
}

/// The notifications for `items` (the current needs-you items), one per new item, given the items notified before.
/// Every eligible item is remembered in `seen`, notified or not, so turning the switch or its kind on later doesn't
/// bring old items up.
pub fn plan(
    items: &[NeedsYouItem],
    texts: &HashMap<String, ItemText>,
    seen: &mut HashSet<ItemId>,
    choices: &Choices,
) -> Vec<Planned> {
    items
        .iter()
        .filter(|i| should_notify(i))
        .filter(|i| seen.insert(item_id(i)))
        .filter(|i| choices.allows(Kind::of_status(i.status)))
        .map(|i| Planned {
            notification: one(i, texts.get(&i.id).cloned().unwrap_or_default(), choices),
            item: Held {
                presence: presence_of(i),
                project: Some((i.project.clone(), i.project_name.clone())),
                label: i.key.to_string(),
                ticket: true,
            },
        })
        .collect()
}

/// d0090's texts for a ticket's item: `text` is its question, prompt or last line.
fn one(item: &NeedsYouItem, facts: ItemText, choices: &Choices) -> OsNotification {
    let text = facts
        .text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let key = &item.key;
    let decide = format!("Decide in {}.", naming::PRODUCT_NAME);
    let (title, body, target) = match item.status {
        Status::AsksYou => (
            format!("{key} asks a question"),
            text.unwrap_or_default().to_owned(),
            Target::Ticket { key: key.clone() },
        ),
        Status::NeedsYou => (
            format!("{key} needs permission"),
            text.unwrap_or_default().to_owned(),
            Target::Ticket { key: key.clone() },
        ),
        Status::Idle => (
            format!("{key} ended its turn"),
            text.unwrap_or_default().to_owned(),
            Target::Ticket { key: key.clone() },
        ),
        // "SHOP-33 validator permission" · "Allow Bash(pnpm test)?"; a click opens the validator's terminal.
        Status::CheckerNeedsYou => (
            format!("{key} validator permission"),
            text.unwrap_or_default().to_owned(),
            Target::Validator { key: key.clone() },
        ),
        // "SHOP-32 · Validation failed 3 times" · "<finding>. Decide in Otaq."
        Status::CheckFailed => (
            format!(
                "{key} \u{b7} {}",
                validation::failed_times(facts.failed_times.max(1))
            ),
            match text {
                Some(finding) => format!("{} {decide}", sentence(finding)),
                None => decide,
            },
            Target::Ticket { key: key.clone() },
        ),
        Status::CheckNoResult => (
            format!(
                "{key} \u{b7} {}",
                text.filter(|t| *t == validation::DISCARDED)
                    .unwrap_or(validation::NO_RESULT)
            ),
            decide,
            Target::Ticket { key: key.clone() },
        ),
        Status::DeliveryStopped => (
            format!("{key} \u{b7} Delivery stopped"),
            text.map(|t| t.replace('`', "")).unwrap_or_default(),
            Target::Ticket { key: key.clone() },
        ),
        _ => (
            format!("{key} session interrupted"),
            format!("Resume from {}.", naming::PRODUCT_NAME),
            Target::Ticket { key: key.clone() },
        ),
    };
    OsNotification {
        title,
        subtitle: format!("{} \u{b7} {}", item.project_name, item.title),
        body,
        kind: Some(Kind::of_status(item.status)),
        target,
        thread: Some(item.project.clone()),
        sound: choices.sound,
    }
}

/// `text` ending as a sentence: a full stop added unless it already ends with one (or `?`, `!`).
fn sentence(text: &str) -> String {
    if text.ends_with(['.', '?', '!']) {
        text.to_owned()
    } else {
        format!("{text}.")
    }
}

/// The notifications for waiting Project agents (PAG-07): "Noyan asks a question" / "needs permission" ·
/// "shop-web · <agent name>", the question or prompt as the body. Once per wait, like tickets' items; focused agents
/// raise no item, so they never notify.
pub fn plan_agents(
    items: &[crate::project_agents::AgentNeedsYou],
    seen: &mut HashSet<ItemId>,
    choices: &Choices,
) -> Vec<Planned> {
    items
        .iter()
        .filter(|i| seen.insert((format!("agent:{}", i.agent), i.status, i.since)))
        .filter_map(|i| {
            let (what, kind, text) = match &i.question {
                Some(q) => ("asks a question", Kind::Question, q.text.trim().to_owned()),
                None => (
                    "needs permission",
                    Kind::Permission,
                    i.reason
                        .strip_prefix("permission: ")
                        .unwrap_or(&i.reason)
                        .to_owned(),
                ),
            };
            if !choices.allows(kind) {
                return None;
            }
            let (title, place, target) = (
                format!("Noyan {what}"),
                i.name.clone(),
                Target::ProjectAgent {
                    project: i.project.clone(),
                    agent: i.agent.clone(),
                },
            );
            Some(Planned {
                notification: OsNotification {
                    title,
                    subtitle: format!("{} \u{b7} {place}", i.project_name),
                    body: text,
                    kind: Some(kind),
                    target,
                    thread: Some(i.project.clone()),
                    sound: choices.sound,
                },
                item: Held {
                    presence: agent_presence(i),
                    project: Some((i.project.clone(), i.project_name.clone())),
                    label: i.name.clone(),
                    ticket: false,
                },
            })
        })
        .collect()
}

/// What is posted for a batch of new items at once: more than [`GROUP_AFTER`] tickets' items of one project become
/// one notification for the project ("4 agents need you" · "shop-web" · the keys); everything else is posted as
/// itself. In the order the items came.
pub fn group(batch: Vec<Planned>) -> Vec<OsNotification> {
    let mut buckets: Vec<(Option<String>, Vec<Planned>)> = Vec::new();
    for planned in batch {
        let project = planned
            .item
            .project
            .as_ref()
            .filter(|_| planned.item.ticket)
            .map(|(key, _)| key.clone());
        match buckets
            .iter_mut()
            .find(|(p, _)| p.is_some() && *p == project)
        {
            Some((_, list)) => list.push(planned),
            None => buckets.push((project, vec![planned])),
        }
    }
    buckets
        .into_iter()
        .flat_map(|(project, list)| match project {
            Some(project) if list.len() > GROUP_AFTER => vec![grouped(&project, &list)],
            _ => list.into_iter().map(|p| p.notification).collect(),
        })
        .collect()
}

fn grouped(project: &str, list: &[Planned]) -> OsNotification {
    let name = list[0]
        .item
        .project
        .as_ref()
        .map_or(project, |(_, name)| name.as_str());
    let keys: Vec<&str> = list.iter().map(|p| p.item.label.as_str()).collect();
    OsNotification {
        title: format!("{} agents need you", list.len()),
        subtitle: name.to_owned(),
        body: keys.join(", "),
        kind: None,
        target: Target::NeedsYou,
        thread: Some(project.to_owned()),
        sound: list[0].notification.sound,
    }
}

/// The notification for the items held in a quiet period (d0090): "<N> more agents need you", the keys per project
/// as the body ("shop-web: SHOP-12, SHOP-33 · billing-api: BILL-6"; a Project agent by its name, a tool that
/// isn't available by the tool's name, last), opening Needs you, in no project's thread.
pub fn summary(held: &[Held], sound: bool) -> OsNotification {
    let mut projects: Vec<(&str, Vec<&str>)> = Vec::new();
    let mut others: Vec<&str> = Vec::new();
    for item in held {
        match &item.project {
            Some((_, name)) => match projects.iter_mut().find(|(p, _)| p == name) {
                Some((_, labels)) => labels.push(&item.label),
                None => projects.push((name, vec![&item.label])),
            },
            None => others.push(&item.label),
        }
    }
    let parts: Vec<String> = projects
        .into_iter()
        .map(|(name, labels)| format!("{name}: {}", labels.join(", ")))
        .chain(others.into_iter().map(str::to_owned))
        .collect();
    OsNotification {
        title: format!("{} more agents need you", held.len()),
        subtitle: String::new(),
        body: parts.join(" \u{b7} "),
        kind: None,
        target: Target::NeedsYou,
        thread: None,
        sound,
    }
}

/// The one notification for a tool that isn't available while tickets wait on it or started on its fallback (USG-10,
/// d0086, AGM-03): "Codex isn't available" / "5 tickets waiting", "3 tickets on the fallback", or both. Posted when it
/// first happens, not per ticket; a click opens the board.
pub fn tool_unavailable(tool: Tool, waiting: u32, on_fallback: u32) -> OsNotification {
    let tickets = |n: u32| match n {
        1 => "1 ticket".to_owned(),
        n => format!("{n} tickets"),
    };
    let mut parts = Vec::new();
    if waiting > 0 {
        parts.push(format!("{} waiting", tickets(waiting)));
    }
    if on_fallback > 0 {
        parts.push(format!("{} on the fallback", tickets(on_fallback)));
    }
    OsNotification {
        title: format!("{} isn\u{2019}t available", tool.display_name()),
        subtitle: String::new(),
        body: parts.join(" \u{b7} "),
        kind: Some(Kind::ToolUnavailable),
        target: Target::Board { project: None },
        thread: None,
        sound: true,
    }
}

/// The first-hide note (TRY-02, d0139; page 31 item 8): the window has gone and Otaq keeps running in the menu bar /
/// tray. `os` is `std::env::consts::OS`; `agents` the agent sessions working now. Windows gets the pin hint; macOS
/// and Linux name the tent icon. A click opens Otaq on the board.
pub fn first_hide(os: &str, agents: u32) -> OsNotification {
    let (title, body) = if os == "windows" {
        (
            format!("{PRODUCT_NAME} is still running"),
            format!(
                "Its icon is in the notification area. Windows may put new icons under ^ next to the clock: drag \
                 {PRODUCT_NAME}\u{2019}s icon onto the taskbar to keep it in view."
            ),
        )
    } else {
        let place = if os == "macos" { "menu bar" } else { "tray" };
        let working = match agents {
            0 => String::new(),
            1 => "1 agent keeps working. ".to_owned(),
            n => format!("{n} agents keep working. "),
        };
        (
            format!("{PRODUCT_NAME} is still running in the {place}"),
            format!(
                "{working}Click the tent icon to open {PRODUCT_NAME}, or to stop agents and quit."
            ),
        )
    };
    OsNotification {
        title,
        subtitle: String::new(),
        body,
        kind: None,
        target: Target::Board { project: None },
        thread: None,
        sound: false,
    }
}

/// What macOS allows Otaq (d0090): read from the notification API inside the bundle; outside it there is nothing to
/// read (`osascript`); the other platforms aren't read yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    NotAsked,
    Allowed,
    TurnedOff,
    OutsideBundle,
    Unknown,
}

/// What a click on a notification does. Called at most once, from any thread.
pub type OnClick = Box<dyn FnOnce() + Send>;

/// What a click on any of the notifier's notifications opens, for a notifier whose clicks arrive on their own
/// ([`Notifier::on_clicks`]).
pub type OpenTarget = std::sync::Arc<dyn Fn(Target) + Send + Sync>;

/// Called, from any thread, when a read of the OS's permission found it changed (T-0372: the Settings line and the
/// banners follow).
pub type PermissionChanged = std::sync::Arc<dyn Fn() + Send + Sync>;

/// Posts OS notifications.
pub trait Notifier: Send + Sync {
    fn post(&self, notification: &OsNotification, on_click: OnClick);

    /// What the OS allows, as last read; never waits for the OS ([`Self::refresh_permission`] reads it again).
    fn permission(&self) -> PermissionState {
        PermissionState::Unknown
    }

    /// Reads the OS's permission again, off the caller's thread.
    fn refresh_permission(&self) {}

    /// What to call when a read finds the permission changed.
    fn on_permission(&self, _changed: PermissionChanged) {}

    /// For a notifier whose clicks arrive by themselves (macOS's delegate, also for notifications posted by an
    /// earlier run): what a click opens. Others call each notification's [`OnClick`].
    fn on_clicks(&self, _open: OpenTarget) {}
}

/// Posts nothing (tests).
pub struct NoNotifier;

impl Notifier for NoNotifier {
    fn post(&self, _notification: &OsNotification, _on_click: OnClick) {}
}

/// The longest a notification is watched for a click; after that a click does nothing.
const CLICK_WATCH: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// macOS outside the bundle: `osascript`'s `display notification`, which works without an app bundle. The texts go
/// in as arguments, never into the script. A click shows the notification's sender (Script Editor), not the ticket
/// (Q22).
pub struct OsaScriptNotifier;

/// `osascript`'s arguments for `n`: the script, then the title, the body and the subtitle as `argv`.
pub fn osascript_args(n: &OsNotification) -> Vec<String> {
    let sound = if n.sound {
        " sound name \"default\""
    } else {
        ""
    };
    [
        "-e",
        "on run argv",
        "-e",
        &format!(
            "display notification (item 2 of argv) with title (item 1 of argv) subtitle (item 3 of argv){sound}"
        ),
        "-e",
        "end run",
        &n.title,
        &n.body,
        &n.subtitle,
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

impl Notifier for OsaScriptNotifier {
    fn post(&self, n: &OsNotification, _on_click: OnClick) {
        let result = std::process::Command::new("osascript")
            .args(osascript_args(n))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if let Err(e) = result {
            tracing::warn!(error = %e, "can't post a notification");
        }
    }

    fn permission(&self) -> PermissionState {
        PermissionState::OutsideBundle
    }
}

/// Linux's body: the notification's body with the subtitle as its last line (d0090; Linux has no subtitle).
pub fn linux_body(n: &OsNotification) -> String {
    match (n.body.is_empty(), n.subtitle.is_empty()) {
        (_, true) => n.body.clone(),
        (true, false) => n.subtitle.clone(),
        (false, false) => format!("{}\n{}", n.body, n.subtitle),
    }
}

/// Linux: `notify-send` (libnotify) through the desktop's notification service (freedesktop). It waits for the
/// notification to close and prints `default` when it was clicked; nothing is installed or configured.
pub struct NotifySendNotifier;

impl Notifier for NotifySendNotifier {
    fn post(&self, n: &OsNotification, on_click: OnClick) {
        let mut command = std::process::Command::new("notify-send");
        command
            .args([
                format!("--app-name={}", naming::PRODUCT_NAME),
                "--action=default=Open".to_owned(),
                "--wait".to_owned(),
                "--".to_owned(),
                n.title.clone(),
                linux_body(n),
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        watch_for_click(command, |out| out.trim() == "default", on_click);
    }
}

/// Windows: a toast through the Windows notification platform, attributed to Windows PowerShell's app identity,
/// which ships with every Windows (no identity of Otaq's own is registered, Q34). Windows hands a toast's
/// click only to an app registered for its identity, so a click can't reach Otaq: like macOS (Q22), the toast
/// tells, the app opens the ticket. It therefore carries no "Open terminal" button.
pub struct ToastNotifier;

/// The AppUserModelID of Windows PowerShell, registered by Windows itself.
pub const POWERSHELL_APP_ID: &str =
    r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";

/// The toast's XML (frame *11 → OS notifications → Windows 11*, d0090): the title, the body, and the subtitle as the
/// attribution line.
pub fn toast_xml(n: &OsNotification) -> String {
    fn escape(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }
    let attribution = if n.subtitle.is_empty() {
        String::new()
    } else {
        format!(
            "<text placement=\"attribution\">{}</text>",
            escape(&n.subtitle)
        )
    };
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text>{attribution}</binding>\
         </visual></toast>",
        escape(&n.title),
        escape(&n.body)
    )
}

impl Notifier for ToastNotifier {
    fn post(&self, n: &OsNotification, _on_click: OnClick) {
        let xml = toast_xml(n);
        #[cfg(windows)]
        toast::post(xml);
        #[cfg(not(windows))]
        let _ = xml;
    }
}

#[cfg(windows)]
mod toast {
    use std::sync::OnceLock;
    use std::sync::mpsc::{Sender, channel};

    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
    use windows::core::HSTRING;

    use super::POWERSHELL_APP_ID;

    /// Hands the toast to the one notification thread, started on the first toast. Every toast goes through that
    /// thread, which initialises the Windows Runtime once and never ends: with a thread per toast, the second toast
    /// of a service crashed it (an access violation), as the runtime objects the first thread set up went away with
    /// that thread.
    pub fn post(xml: String) {
        static THREAD: OnceLock<Option<Sender<String>>> = OnceLock::new();
        let thread = THREAD.get_or_init(|| {
            let (sender, toasts) = channel::<String>();
            let spawned = std::thread::Builder::new()
                .name("notifications".into())
                .spawn(move || {
                    // SAFETY: initialises the Windows Runtime for this thread, which lives as long as the process.
                    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
                    for xml in toasts {
                        if let Err(e) = show(&xml) {
                            tracing::warn!(error = %e, "can't post a notification");
                        }
                    }
                });
            match spawned {
                Ok(_) => Some(sender),
                Err(e) => {
                    tracing::warn!(error = %e, "can't post a notification");
                    None
                }
            }
        });
        if let Some(sender) = thread {
            let _ = sender.send(xml);
        }
    }

    fn show(xml: &str) -> windows::core::Result<()> {
        let document = XmlDocument::new()?;
        document.LoadXml(&HSTRING::from(xml))?;
        let toast = ToastNotification::CreateToastNotification(&document)?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(POWERSHELL_APP_ID))?
            .Show(&toast)
    }
}

/// Starts `command` and, on a thread of its own, waits up to [`CLICK_WATCH`] for it to end; calls `on_click` when
/// its output says the notification was clicked.
fn watch_for_click(
    mut command: std::process::Command,
    clicked: fn(&str) -> bool,
    on_click: OnClick,
) {
    let child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            tracing::warn!(program = ?command.get_program(), error = %e, "can't post a notification");
            return;
        }
    };
    let spawned = std::thread::Builder::new()
        .name("notification".into())
        .spawn(move || {
            let mut child = child;
            let deadline = std::time::Instant::now() + CLICK_WATCH;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(std::time::Duration::from_millis(250));
                    }
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return;
                    }
                }
            }
            let mut out = String::new();
            if let Some(mut stdout) = child.stdout.take() {
                let _ = std::io::Read::read_to_string(&mut stdout, &mut out);
            }
            if clicked(&out) {
                on_click();
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "can't watch a notification");
    }
}

/// The notifier of this platform: macOS's notification API inside `Otaq.app` (`osascript` outside it), `notify-send`
/// on Linux, a PowerShell toast on Windows.
pub fn platform_notifier() -> std::sync::Arc<dyn Notifier> {
    #[cfg(target_os = "macos")]
    {
        match super::notification_center::in_bundle() {
            Some(in_bundle) => std::sync::Arc::new(macos::NativeNotifier::new(in_bundle)),
            None => std::sync::Arc::new(OsaScriptNotifier),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        if cfg!(windows) {
            std::sync::Arc::new(ToastNotifier)
        } else {
            std::sync::Arc::new(NotifySendNotifier)
        }
    }
}

/// What a click on a notification opens. An app connected to the service (one that subscribed to events) is
/// told to come forward on the target; with none, the app next to the service's binary is started on it (the
/// target in the [`open_env_var`] variable): through LaunchServices inside `Otaq.app`, so it comes forward as a proper
/// app, else as a plain program.
#[derive(Clone)]
pub struct Opener {
    events: crate::service::EventBus,
    subscribers: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    app: Option<std::path::PathBuf>,
}

/// Counts a subscribed client while it lives.
pub struct Subscribed(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl Drop for Subscribed {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Posting notifications, opening what a click asks for, and the time the quiet period counts in, together.
#[derive(Clone)]
pub struct Notifications {
    pub notifier: std::sync::Arc<dyn Notifier>,
    pub opener: Opener,
    pub clock: std::sync::Arc<dyn pace::Clock>,
}

/// The variable that tells a starting app what to open: `OTAQ_OPEN` = [`Target::to_open_value`].
pub fn open_env_var() -> String {
    naming::env_var("OPEN")
}

/// The command that starts the app at `app` on `target` when no app is connected: `open -a <Otaq.app> --env
/// OTAQ_OPEN=<target>` when `app` is a bundle's program (macOS then brings it forward as an app), else `app` itself
/// with the variable set. An app started through LaunchServices doesn't inherit the service's environment, so
/// `data_home` (the service's own `OTAQ_HOME`, when set) is passed the same way: the app then reaches this service's
/// data folder, not the default one. It is the only variable the app reads to find its service.
pub fn launch_command(
    app: &std::path::Path,
    target: &Target,
    data_home: Option<&std::ffi::OsStr>,
) -> std::process::Command {
    let value = target.to_open_value();
    match super::bundle::bundle_of(app) {
        Some(bundle) if cfg!(target_os = "macos") => {
            let mut command = std::process::Command::new("open");
            command
                .arg("-a")
                .arg(&bundle.root)
                .arg("--env")
                .arg(format!("{}={value}", open_env_var()));
            if let Some(home) = data_home.filter(|h| !h.is_empty()) {
                let mut pair = std::ffi::OsString::from(format!("{}=", DataDir::env_var()));
                pair.push(home);
                command.arg("--env").arg(pair);
            }
            command
        }
        _ => {
            let mut command = std::process::Command::new(app);
            command.env(open_env_var(), value);
            command
        }
    }
}

impl Opener {
    pub fn new(events: crate::service::EventBus, app: Option<std::path::PathBuf>) -> Self {
        Self {
            events,
            subscribers: Default::default(),
            app,
        }
    }

    /// A client subscribed to events; it counts until the guard is dropped.
    pub fn subscribed(&self) -> Subscribed {
        self.subscribers
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Subscribed(self.subscribers.clone())
    }

    pub fn open(&self, target: Target) {
        if self.subscribers.load(std::sync::atomic::Ordering::SeqCst) > 0 {
            self.events
                .publish(crate::protocol::Event::OpenFromNotification { target });
            return;
        }
        let Some(app) = &self.app else {
            return;
        };
        let data_home = std::env::var_os(DataDir::env_var());
        let mut command = launch_command(app, &target, data_home.as_deref());
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(
                crate::process::DETACHED_PROCESS | crate::process::CREATE_NEW_PROCESS_GROUP,
            );
        }
        let through_launch_services = command.get_program() == "open";
        match command.spawn() {
            // `open` ends once LaunchServices has the app; reap it so it leaves no zombie.
            Ok(mut child) if through_launch_services => {
                let _ = std::thread::Builder::new()
                    .name("open app".into())
                    .spawn(move || child.wait());
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(app = %app.display(), error = %e, "can't open the app from a notification");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> Choices {
        Choices::default()
    }

    fn item(key: &str, status: Status, group: NeedsYouGroup, since: i64) -> NeedsYouItem {
        NeedsYouItem {
            group,
            project: key.split('-').next().unwrap().into(),
            project_name: if key.starts_with("BILL") {
                "billing-api".into()
            } else {
                "shop-web".into()
            },
            id: key.into(),
            key: key.parse().unwrap(),
            title: match key {
                "SHOP-22" => "Fix flaky login redirect".into(),
                "SHOP-25" => "Upgrade to Next.js 15".into(),
                _ => "Split user full name".into(),
            },
            status,
            interrupted_from: None,
            question: None,
            reason: String::new(),
            since,
            age_ms: 0,
            blocked: 0,
            message: None,
            change_agent: None,
            waiting_in: None,
            piece: None,
            helpers_to_resume: Vec::new(),
        }
    }

    fn texts() -> HashMap<String, ItemText> {
        HashMap::from([
            ("SHOP-22".into(), "Allow `rm -rf .next`?".into()),
            (
                "SHOP-23".into(),
                "Should fullName be split everywhere, or only in the profile form?".into(),
            ),
        ])
    }

    fn shown(out: &[OsNotification]) -> Vec<(&str, &str, &str)> {
        out.iter()
            .map(|n| (n.title.as_str(), n.subtitle.as_str(), n.body.as_str()))
            .collect()
    }

    fn notifications(planned: Vec<Planned>) -> Vec<OsNotification> {
        planned.into_iter().map(|p| p.notification).collect()
    }

    fn agent_wait(agent: &str, status: Status, since: i64) -> crate::project_agents::AgentNeedsYou {
        crate::project_agents::AgentNeedsYou {
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            agent: agent.into(),
            name: "Checkout slowness".into(),
            status,
            reason: "permission: Allow pnpm test?".into(),
            question: (status == Status::AsksYou).then(|| crate::board::questions::HumanQuestion {
                id: "q".into(),
                text: "Profile in production too?".into(),
                options: Vec::new(),
                recommended: None,
                notes: Vec::new(),
                asked_at: since,
            }),
            since,
        }
    }

    /// T-0201 (PAG-07, ATT-11), T-0371 (d0090): a waiting Project agent notifies once per wait: "Noyan needs
    /// permission" · "shop-web · <agent name>" · the prompt; a click opens it in the agent panel. The switch off posts
    /// nothing.
    #[test]
    fn project_agents_notify_once_per_wait() {
        let mut seen = HashSet::new();
        let items = [
            agent_wait("a1", Status::NeedsYou, 5),
            agent_wait("a2", Status::AsksYou, 6),
        ];
        let out = notifications(plan_agents(&items, &mut seen, &on()));
        assert_eq!(
            shown(&out),
            [
                (
                    "Noyan needs permission",
                    "shop-web · Checkout slowness",
                    "Allow pnpm test?"
                ),
                (
                    "Noyan asks a question",
                    "shop-web · Checkout slowness",
                    "Profile in production too?"
                ),
            ]
        );
        assert_eq!(
            out[0].target,
            Target::ProjectAgent {
                project: "SHOP".into(),
                agent: "a1".into()
            }
        );
        assert_eq!(out[0].kind, Some(Kind::Permission));
        assert_eq!(out[1].kind, Some(Kind::Question));
        assert_eq!(out[0].thread.as_deref(), Some("SHOP"));
        assert!(plan_agents(&items, &mut seen, &on()).is_empty(), "once");
        // A new wait later notifies again; with the switch off it is only remembered.
        let off = Choices {
            enabled: false,
            ..on()
        };
        let later = [agent_wait("a1", Status::NeedsYou, 9)];
        assert!(plan_agents(&later, &mut seen, &off).is_empty());
        assert!(plan_agents(&later, &mut seen, &on()).is_empty());
        assert_eq!(
            plan_agents(&[agent_wait("a1", Status::NeedsYou, 12)], &mut seen, &on()).len(),
            1
        );
    }

    #[test]
    fn only_blocking_items_notify() {
        use NeedsYouGroup::*;
        assert!(should_notify(&item("SHOP-22", Status::NeedsYou, Family, 1)));
        assert!(should_notify(&item("SHOP-23", Status::AsksYou, Family, 1)));
        assert!(should_notify(&item("SHOP-19", Status::Idle, Family, 1)));
        assert!(should_notify(&item(
            "SHOP-25",
            Status::Interrupted,
            Stopped,
            1
        )));
        assert!(!should_notify(&item(
            "SHOP-28",
            Status::StartInterrupted,
            Stopped,
            1
        )));
        assert!(!should_notify(&item(
            "SHOP-30",
            Status::ParentClosed,
            Stopped,
            1
        )));
        assert!(!should_notify(&item(
            "SHOP-18",
            Status::WaitingForYou,
            Review,
            1
        )));
    }

    /// T-0371 (d0090): every kind's title (what happened), subtitle (project · ticket title) and body (the
    /// question, prompt or finding), with its kind, its target and the project's thread; once per item.
    #[test]
    fn every_kind_says_what_where_and_what_you_are_asked() {
        use NeedsYouGroup::*;
        let titled = |key: &str, title: &str, status: Status, group| NeedsYouItem {
            title: title.into(),
            ..item(key, status, group, 1)
        };
        let items = vec![
            item("SHOP-22", Status::NeedsYou, Family, 10),
            titled("SHOP-12", "Dark mode", Status::AsksYou, Family),
            titled("SHOP-19", "Price table", Status::Idle, Family),
            titled(
                "SHOP-33",
                "Shipping estimate",
                Status::CheckerNeedsYou,
                Family,
            ),
            titled(
                "SHOP-32",
                "Address autocomplete",
                Status::CheckFailed,
                Family,
            ),
            titled("SHOP-34", "Tax display", Status::CheckNoResult, Family),
            titled(
                "SHOP-44",
                "Wishlist share link",
                Status::DeliveryStopped,
                Family,
            ),
            item("SHOP-25", Status::Interrupted, Stopped, 30),
            item("SHOP-18", Status::WaitingForYou, Review, 40),
        ];
        assert!(items[..8].iter().all(should_notify));
        let texts = HashMap::from([
            (
                "SHOP-22".to_owned(),
                ItemText::from("Allow `rm -rf .next`?"),
            ),
            (
                "SHOP-12".to_owned(),
                ItemText::from("Should dark mode follow the system setting?"),
            ),
            (
                "SHOP-19".to_owned(),
                ItemText::from("The price table now reads from the API."),
            ),
            (
                "SHOP-33".to_owned(),
                ItemText::from("Allow Bash(pnpm test)?"),
            ),
            (
                "SHOP-32".to_owned(),
                ItemText {
                    text: Some(
                        "The description asks for keyboard navigation; arrow keys do nothing"
                            .into(),
                    ),
                    failed_times: 3,
                },
            ),
            (
                "SHOP-44".to_owned(),
                ItemText::from("Keeps conflicting with `wishlist`"),
            ),
        ]);
        let mut seen = HashSet::new();
        let out = notifications(plan(&items, &texts, &mut seen, &on()));
        let decide = format!("Decide in {}.", naming::PRODUCT_NAME);
        let failed = format!(
            "The description asks for keyboard navigation; arrow keys do nothing. {decide}"
        );
        let resume = format!("Resume from {}.", naming::PRODUCT_NAME);
        assert_eq!(
            shown(&out),
            [
                (
                    "SHOP-22 needs permission",
                    "shop-web · Fix flaky login redirect",
                    "Allow `rm -rf .next`?"
                ),
                (
                    "SHOP-12 asks a question",
                    "shop-web · Dark mode",
                    "Should dark mode follow the system setting?"
                ),
                (
                    "SHOP-19 ended its turn",
                    "shop-web · Price table",
                    "The price table now reads from the API."
                ),
                (
                    "SHOP-33 validator permission",
                    "shop-web · Shipping estimate",
                    "Allow Bash(pnpm test)?"
                ),
                (
                    "SHOP-32 · Validation failed 3 times",
                    "shop-web · Address autocomplete",
                    failed.as_str()
                ),
                (
                    "SHOP-34 · Validation ended without a result",
                    "shop-web · Tax display",
                    decide.as_str()
                ),
                (
                    "SHOP-44 · Delivery stopped",
                    "shop-web · Wishlist share link",
                    "Keeps conflicting with wishlist"
                ),
                (
                    "SHOP-25 session interrupted",
                    "shop-web · Upgrade to Next.js 15",
                    resume.as_str()
                ),
            ]
        );
        let kinds: Vec<Option<Kind>> = out.iter().map(|n| n.kind).collect();
        assert_eq!(
            kinds,
            [
                Some(Kind::Permission),
                Some(Kind::Question),
                Some(Kind::TurnEnded),
                Some(Kind::Permission),
                Some(Kind::ValidationDecision),
                Some(Kind::ValidationDecision),
                Some(Kind::AutoMergeStopped),
                Some(Kind::Interrupted),
            ]
        );
        for n in &out {
            assert_eq!(n.thread.as_deref(), Some("SHOP"), "{}", n.title);
            assert!(n.sound);
        }
        assert_eq!(
            out[3].target,
            Target::Validator {
                key: "SHOP-33".parse().unwrap()
            },
            "a validator's prompt opens the validator's terminal"
        );
        assert!(
            out.iter()
                .enumerate()
                .filter(|(i, _)| *i != 3)
                .all(|(_, n)| matches!(n.target, Target::Ticket { .. }))
        );
        // Nothing again for the same items (another event, a focus round trip).
        assert!(plan(&items, &texts, &mut seen, &on()).is_empty());
        // A new question from the same ticket is a new item.
        let again = vec![item("SHOP-23", Status::AsksYou, Family, 99)];
        assert_eq!(plan(&again, &texts, &mut seen, &on()).len(), 1);
        // A finding that already ends a sentence isn't given a second full stop.
        let texts = HashMap::from([(
            "SHOP-32".to_owned(),
            ItemText {
                text: Some("Arrow keys do nothing.".into()),
                failed_times: 2,
            },
        )]);
        let again = [titled(
            "SHOP-32",
            "Address autocomplete",
            Status::CheckFailed,
            Family,
        )];
        let out = notifications(plan(&again, &texts, &mut HashSet::new(), &on()));
        assert_eq!(out[0].title, "SHOP-32 · Validation failed 2 times");
        assert_eq!(out[0].body, format!("Arrow keys do nothing. {decide}"));
        // M12: failed rounds that go back to the agent and auto-merges raise no item, so they never notify.
        for quiet in [
            Status::Working,
            Status::Checking,
            Status::MergeConflictFixing,
            Status::Merged,
        ] {
            assert!(
                crate::board::needs_you::group_of(quiet).is_none(),
                "{quiet:?} raises no item, so no notification"
            );
        }
    }

    /// T-0371 (d0090): long texts go to the OS whole; macOS, Windows and Linux cut them as they draw.
    #[test]
    fn long_texts_are_kept_whole() {
        let command = format!(
            "Allow `{}`?",
            "pnpm exec playwright test --grep login ".repeat(20)
        );
        let title =
            "Make the login redirect keep the query string and the hash on every route ".repeat(5);
        let long = NeedsYouItem {
            title: title.clone(),
            ..item("SHOP-22", Status::NeedsYou, NeedsYouGroup::Family, 1)
        };
        let texts = HashMap::from([("SHOP-22".to_owned(), ItemText::from(command.as_str()))]);
        let out = notifications(plan(&[long], &texts, &mut HashSet::new(), &on()));
        assert_eq!(out[0].body, command.trim());
        assert_eq!(out[0].subtitle, format!("shop-web · {title}"));
    }

    /// T-0371 (d0090): an unticked kind posts nothing but is still remembered, so ticking it later doesn't bring
    /// it up; the other kinds still go out. The main switch off posts nothing of any kind.
    #[test]
    fn a_kind_turned_off_is_left_out_and_stays_out() {
        use NeedsYouGroup::*;
        let mut no_turn_ends = on();
        no_turn_ends.kinds.turn_ended = false;
        let items = vec![
            item("SHOP-19", Status::Idle, Family, 1),
            item("SHOP-22", Status::NeedsYou, Family, 1),
        ];
        let mut seen = HashSet::new();
        let out = notifications(plan(&items, &texts(), &mut seen, &no_turn_ends));
        assert_eq!(shown(&out)[0].0, "SHOP-22 needs permission");
        assert_eq!(out.len(), 1);
        assert!(plan(&items, &texts(), &mut seen, &on()).is_empty());

        let mut no_questions = on();
        no_questions.kinds.question = false;
        let asks = [agent_wait("a2", Status::AsksYou, 6)];
        assert!(plan_agents(&asks, &mut seen, &no_questions).is_empty());
        assert!(plan_agents(&asks, &mut seen, &on()).is_empty());

        let off = Choices {
            enabled: false,
            ..on()
        };
        let later = vec![item("SHOP-22", Status::NeedsYou, Family, 50)];
        assert!(plan(&later, &texts(), &mut seen, &off).is_empty());
        assert!(plan(&later, &texts(), &mut seen, &on()).is_empty());
        assert!(!off.allows(Kind::Permission));
        assert!(!no_turn_ends.allows(Kind::TurnEnded) && no_turn_ends.allows(Kind::Question));
    }

    /// The sound follows Settings → "Play a sound".
    #[test]
    fn the_sound_follows_the_setting() {
        let quiet = Choices {
            sound: false,
            ..on()
        };
        let items = [item("SHOP-22", Status::NeedsYou, NeedsYouGroup::Family, 1)];
        let out = notifications(plan(&items, &texts(), &mut HashSet::new(), &quiet));
        assert!(!out[0].sound);
        let out = notifications(plan(&items, &texts(), &mut HashSet::new(), &on()));
        assert!(out[0].sound);
    }

    #[test]
    fn more_than_three_at_once_are_grouped_per_project() {
        let items: Vec<NeedsYouItem> = (1..=4)
            .map(|n| {
                item(
                    &format!("SHOP-{n}"),
                    Status::Interrupted,
                    NeedsYouGroup::Stopped,
                    5,
                )
            })
            .chain([item("BILL-6", Status::NeedsYou, NeedsYouGroup::Family, 5)])
            .collect();
        let out = group(plan(&items, &HashMap::new(), &mut HashSet::new(), &on()));
        assert_eq!(
            shown(&out),
            [
                (
                    "4 agents need you",
                    "shop-web",
                    "SHOP-1, SHOP-2, SHOP-3, SHOP-4"
                ),
                (
                    "BILL-6 needs permission",
                    "billing-api · Split user full name",
                    ""
                ),
            ]
        );
        assert_eq!(out[0].target, Target::NeedsYou);
        assert_eq!(out[0].thread.as_deref(), Some("SHOP"));
        assert_eq!(out[0].kind, None);
        assert_eq!(out[1].thread.as_deref(), Some("BILL"));
        let three = &items[..3];
        assert_eq!(
            group(plan(three, &HashMap::new(), &mut HashSet::new(), &on())).len(),
            3
        );
        // Project agents aren't part of the rule.
        let agents: Vec<_> = (1..=4)
            .map(|n| agent_wait(&format!("a{n}"), Status::NeedsYou, n))
            .collect();
        let planned = plan_agents(&agents, &mut HashSet::new(), &on());
        assert_eq!(group(planned).len(), 4);
    }

    /// T-0371 (d0090): the quiet period's summary names the keys per project, opens Needs you and sits in no
    /// project's thread.
    #[test]
    fn the_summary_names_the_keys_per_project() {
        let held = |label: &str, project: Option<(&str, &str)>| Held {
            presence: label.into(),
            project: project.map(|(k, n)| (k.into(), n.into())),
            label: label.into(),
            ticket: true,
        };
        let shop = Some(("SHOP", "shop-web"));
        let bill = Some(("BILL", "billing-api"));
        let n = summary(
            &[
                held("SHOP-12", shop),
                held("BILL-6", bill),
                held("SHOP-33", shop),
                held("Codex", None),
            ],
            true,
        );
        assert_eq!(n.title, "4 more agents need you");
        assert_eq!(
            n.body,
            "shop-web: SHOP-12, SHOP-33 · billing-api: BILL-6 · Codex"
        );
        assert_eq!(n.subtitle, "");
        assert_eq!(n.target, Target::NeedsYou);
        assert_eq!(n.thread, None);
    }

    /// A tool that isn't available opens the board, in no project's thread.
    #[test]
    fn a_tool_that_is_not_available_opens_the_board() {
        let n = tool_unavailable(Tool::Codex, 5, 0);
        assert_eq!(n.title, "Codex isn\u{2019}t available");
        assert_eq!(n.body, "5 tickets waiting");
        assert_eq!(n.kind, Some(Kind::ToolUnavailable));
        assert_eq!(n.target, Target::Board { project: None });
        assert_eq!(n.thread, None);
    }

    /// T-0371 (d0090): every target travels as one string (`OTAQ_OPEN`, `userInfo`); the values from before M25 (a
    /// key, `needs_you`) read as before.
    #[test]
    fn every_target_travels_as_one_string() {
        let targets = [
            Target::Ticket {
                key: "SHOP-22".parse().unwrap(),
            },
            Target::Ticket {
                key: "SHOP-40".parse().unwrap(),
            },
            Target::Validator {
                key: "SHOP-33".parse().unwrap(),
            },
            Target::ProjectAgent {
                project: "SHOP".into(),
                agent: "pa7".into(),
            },
            Target::NeedsYou,
            Target::Review {
                key: "WEB-7".parse().unwrap(),
            },
            Target::Sheet {
                key: "SHOP-30".parse().unwrap(),
            },
            Target::Board { project: None },
            Target::Board {
                project: Some("SHOP".into()),
            },
        ];
        let values: Vec<String> = targets.iter().map(Target::to_open_value).collect();
        assert_eq!(
            values,
            [
                "SHOP-22",
                "SHOP-40",
                "validator:SHOP-33",
                "agent:SHOP:pa7",
                "needs_you",
                "review:WEB-7",
                "sheet:SHOP-30",
                "board",
                "board:SHOP"
            ]
        );
        for (target, value) in targets.iter().zip(&values) {
            assert_eq!(Target::parse_open_value(value).as_ref(), Some(target));
        }
        assert_eq!(
            Target::parse_open_value(OPEN_NEEDS_YOU),
            Some(Target::NeedsYou)
        );
        for bad in [
            "not a key",
            "validator:",
            "review:",
            "sheet:nope",
            "agent:SHOP",
            "agent::pa7",
            "board:",
            "lead:SHOP-40",
        ] {
            assert_eq!(Target::parse_open_value(bad), None, "{bad}");
        }
        // The event carries it as JSON.
        assert_eq!(
            serde_json::to_value(&targets[3]).unwrap(),
            serde_json::json!({"kind": "project_agent", "project": "SHOP", "agent": "pa7"})
        );
    }

    #[test]
    fn the_toast_carries_the_texts_escaped_with_the_subtitle_as_attribution() {
        let mut n = notifications(plan(
            &[item("SHOP-22", Status::NeedsYou, NeedsYouGroup::Family, 1)],
            &HashMap::from([(
                "SHOP-22".to_owned(),
                ItemText::from("Allow `rm -rf .next`? <a & b>"),
            )]),
            &mut HashSet::new(),
            &on(),
        ))
        .remove(0);
        let xml = toast_xml(&n);
        assert!(
            xml.contains("<text>SHOP-22 needs permission</text>"),
            "{xml}"
        );
        assert!(
            xml.contains("<text>Allow `rm -rf .next`? &lt;a &amp; b&gt;</text>"),
            "{xml}"
        );
        assert!(
            xml.contains(
                "<text placement=\"attribution\">shop-web · Fix flaky login redirect</text>"
            ),
            "{xml}"
        );
        assert!(
            !xml.contains("<action"),
            "no button that couldn't act: {xml}"
        );
        n.subtitle.clear();
        assert!(!toast_xml(&n).contains("attribution"));
    }

    /// d0090: Linux has no subtitle; it is the body's last line.
    #[test]
    fn linux_appends_the_subtitle_to_the_body() {
        let mut n = tool_unavailable(Tool::Codex, 1, 0);
        assert_eq!(linux_body(&n), "1 ticket waiting");
        n.subtitle = "shop-web · Fix flaky login redirect".into();
        assert_eq!(
            linux_body(&n),
            "1 ticket waiting\nshop-web · Fix flaky login redirect"
        );
        n.body.clear();
        assert_eq!(linux_body(&n), "shop-web · Fix flaky login redirect");
    }

    /// Outside the bundle `osascript` gets the subtitle and the sound; the texts stay arguments, never script.
    #[test]
    fn osascript_takes_the_texts_as_arguments() {
        let mut n = notifications(plan(
            &[item("SHOP-22", Status::NeedsYou, NeedsYouGroup::Family, 1)],
            &HashMap::from([("SHOP-22".to_owned(), ItemText::from("Allow \"x\"? end run"))]),
            &mut HashSet::new(),
            &on(),
        ))
        .remove(0);
        let args = osascript_args(&n);
        assert_eq!(
            args[3],
            "display notification (item 2 of argv) with title (item 1 of argv) subtitle (item 3 of argv) sound name \"default\""
        );
        assert_eq!(
            &args[6..],
            [
                "SHOP-22 needs permission",
                "Allow \"x\"? end run",
                "shop-web · Fix flaky login redirect"
            ]
        );
        n.sound = false;
        assert!(!osascript_args(&n)[3].contains("sound"));
        assert_eq!(
            OsaScriptNotifier.permission(),
            PermissionState::OutsideBundle
        );
        assert_eq!(NoNotifier.permission(), PermissionState::Unknown);
    }

    /// With no app connected a click starts the app on its target: through LaunchServices (`open -a`) when the app
    /// is a bundle's program on macOS, else the program itself; either way with `OTAQ_OPEN` set.
    #[test]
    fn a_click_starts_the_app_on_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join(naming::app_binary_name());
        std::fs::write(&plain, "").unwrap();
        let target = Target::Validator {
            key: "SHOP-33".parse().unwrap(),
        };
        let command = launch_command(&plain, &target, Some(std::ffi::OsStr::new("/tmp/m25")));
        assert_eq!(command.get_program(), plain.as_os_str());
        let env: Vec<_> = command.get_envs().collect();
        assert_eq!(
            env,
            [(
                std::ffi::OsStr::new(&open_env_var()),
                Some(std::ffi::OsStr::new("validator:SHOP-33"))
            )]
        );

        let contents = dir
            .path()
            .join(naming::bundle_folder_name())
            .join("Contents");
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();
        std::fs::write(
            contents.join("Info.plist"),
            format!(
                "<plist><dict><key>CFBundleIdentifier</key><string>{}</string></dict></plist>",
                naming::BUNDLE_ID
            ),
        )
        .unwrap();
        let in_bundle = contents.join("MacOS").join(naming::app_binary_name());
        std::fs::write(&in_bundle, "").unwrap();
        let args_of = |command: &std::process::Command| -> Vec<String> {
            command
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };
        let command = launch_command(&in_bundle, &target, None);
        if cfg!(target_os = "macos") {
            assert_eq!(command.get_program(), "open");
            let args = args_of(&command);
            let root = dir
                .path()
                .canonicalize()
                .unwrap()
                .join(naming::bundle_folder_name());
            assert_eq!(
                args,
                [
                    "-a".to_owned(),
                    root.display().to_string(),
                    "--env".to_owned(),
                    format!("{}=validator:SHOP-33", open_env_var())
                ]
            );
            // The service's data folder goes along: LaunchServices doesn't pass the service's environment on.
            let with_home = args_of(&launch_command(
                &in_bundle,
                &target,
                Some(std::ffi::OsStr::new("/tmp/m25")),
            ));
            assert_eq!(
                with_home[4..],
                [
                    "--env".to_owned(),
                    format!("{}=/tmp/m25", DataDir::env_var())
                ]
            );
            assert_eq!(with_home[..4], args[..]);
            let empty = launch_command(&in_bundle, &target, Some(std::ffi::OsStr::new("")));
            assert_eq!(args_of(&empty), args, "an empty variable isn't passed");
        } else {
            assert_eq!(command.get_program(), in_bundle.as_os_str());
        }
    }

    /// T-0566 (TRY-02, page 31 item 8): the first-hide note's words per platform and agent count.
    #[test]
    fn the_first_hide_note() {
        let name = PRODUCT_NAME;
        let click = format!("Click the tent icon to open {name}, or to stop agents and quit.");
        let mac = first_hide("macos", 2);
        assert_eq!(
            mac.title,
            format!("{name} is still running in the menu bar")
        );
        assert_eq!(mac.body, format!("2 agents keep working. {click}"));
        assert_eq!(mac.target, Target::Board { project: None });
        assert_eq!(
            first_hide("macos", 1).body,
            format!("1 agent keeps working. {click}")
        );
        assert_eq!(first_hide("macos", 0).body, click);
        assert_eq!(
            first_hide("linux", 0).title,
            format!("{name} is still running in the tray")
        );
        let windows = first_hide("windows", 3);
        assert_eq!(windows.title, format!("{name} is still running"));
        assert!(
            windows
                .body
                .starts_with("Its icon is in the notification area."),
            "{}",
            windows.body
        );
    }
}

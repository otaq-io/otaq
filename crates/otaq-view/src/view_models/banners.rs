//! The banner area's rules (P6, d0016; *11 · Banners → Stacking · usage limit + reconnecting*): one banner per
//! situation, never one per agent; at most two rows; ordered error › restart required › usage limit › folder trust
//! failed › permission warning › folder trust note › service update ready › reconnecting; reconnecting folds into
//! another banner as a suffix instead of taking a row of its own. The two folder-trust banners (PRJ-08, d0085; *11 →
//! 4c*): the warning for the projects whose trust couldn't be recorded sits above the permission warning, the
//! one-time note below it. The two notification banners (ATT-13, ATT-14, d0090): "macOS is blocking Otaq's
//! notifications" (amber) right below the folder trust warning, above the permission warning; Otaq's calm "Allow
//! notifications?" ask right below the folder trust note. The whole order: error › restart required › usage limit ›
//! folder trust failed › notifications blocked › permission warning › flow change waiting › folder trust note ›
//! notifications ask › service update ready › reconnecting. The flow change waiting (FLW-06, d0108; *11 → A flow change
//! waiting*) is the project on screen's: amber, under the other warnings. The paused banner (PAU-08, d0112; `s24-07`)
//! is the project on screen's too, amber and first of all.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use otaq_core::agent_tools::Tool;

use crate::icons::IconName;
use crate::view_models::agent_tools_page::and_list;
use crate::view_models::limit_banner::LineView;

/// Where the permission-mode warning's Read the docs goes (allowlists and auto mode).
pub const PERMISSION_DOCS_URL: &str = "https://code.claude.com/docs/en/permission-modes";

/// The situations a banner can stand for. The order of the variants is the stacking order (first = top).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "BannerKind")]
pub enum SituationKind {
    /// Amber, first: the open project is paused (PAU-08, d0112); it can't be closed while paused, and Resume is the
    /// title bar's.
    Paused,
    /// Red: the background service has ended (reboot or crash).
    ServiceStopped,
    /// Red: the service came back and its sessions were interrupted (REC-06).
    SessionsInterrupted,
    RestartRequired,
    UsageLimit,
    /// Amber: Claude Code's trust couldn't be recorded for some projects (PRJ-08).
    TrustFailed,
    /// Amber: macOS blocked a notification that mattered (ATT-14, d0090).
    NotificationsBlocked,
    PermissionWarning,
    /// Amber: the project on screen has a flow change waiting for a quiet board (FLW-06); no ticket of it starts.
    FlowWaiting,
    /// Calm: the one-time note that Otaq recorded Claude Code's trust for projects added earlier (PRJ-08).
    TrustNote,
    /// Calm: Otaq's own "Allow notifications?" before macOS's question (ATT-13, d0090).
    NotificationsAsk,
    ServiceUpdateReady,
    Reconnecting,
}

/// A situation with what its banner shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Situation {
    /// The open project is paused ([`crate::view_models::pause::banner`]): "shop-web is paused." and what that
    /// left, or "All projects are paused.", "… still paused." after a restart.
    Paused {
        title: String,
        text: String,
    },
    ServiceStopped,
    /// The service couldn't be reached or started at all: the banner says why (Q40), with Start service to try
    /// again. The same situation as [`Situation::ServiceStopped`], so never a second banner.
    ServiceCouldNotStart {
        reason: String,
    },
    /// After a reboot, a crash (`unclean`) or a service restart: `count` sessions were interrupted; `at` is when
    /// the service started again ("02:14"); `agents` Project agents ended with it (d0068, T-0244), and `leads` epic
    /// leads (d0083, T-0289).
    SessionsInterrupted {
        count: u32,
        unclean: bool,
        at: String,
        agents: u32,
        leads: u32,
    },
    RestartRequired {
        working: u32,
        waiting: u32,
    },
    /// The usage limits and the tools that aren't available (USG-06, USG-10; [`crate::view_models::limit_banner`]):
    /// the first line's title, text and Resume (what a page that draws one line shows), and every tool's line.
    UsageLimit {
        title: String,
        text: String,
        resume: Option<crate::view_models::limit_banner::ResumeButton>,
        lines: Vec<LineView>,
    },
    /// The projects whose folder trust couldn't be recorded, by key. One banner for all of them (P6).
    TrustFailed {
        projects: Vec<TrustFailure>,
    },
    PermissionWarning,
    /// The tools Otaq recorded folder trust for (d0085; Claude Code when the service didn't say).
    TrustNote {
        tools: Vec<Tool>,
    },
    /// macOS has Otaq's notifications turned off and one would have gone out (ATT-14).
    NotificationsBlocked,
    /// The first agent started while macOS hasn't been asked (ATT-13).
    NotificationsAsk,
    /// The project on screen's waiting flow change (FLW-06): what it changes ("QA after Security · Review can be
    /// skipped") and how many tickets it waits for. Apply now opens the window that lists them (APN-01).
    FlowWaiting {
        summary: String,
        count: usize,
    },
    ServiceUpdateReady {
        working: u32,
        waiting: u32,
    },
    Reconnecting,
}

/// A project whose folder trust couldn't be recorded: its key and why (a sentence without its final period).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustFailure {
    pub project: String,
    pub reason: String,
}

impl Situation {
    pub fn kind(&self) -> SituationKind {
        match self {
            Self::Paused { .. } => SituationKind::Paused,
            Self::ServiceStopped | Self::ServiceCouldNotStart { .. } => {
                SituationKind::ServiceStopped
            }
            Self::SessionsInterrupted { .. } => SituationKind::SessionsInterrupted,
            Self::RestartRequired { .. } => SituationKind::RestartRequired,
            Self::UsageLimit { .. } => SituationKind::UsageLimit,
            Self::TrustFailed { .. } => SituationKind::TrustFailed,
            Self::PermissionWarning => SituationKind::PermissionWarning,
            Self::TrustNote { .. } => SituationKind::TrustNote,
            Self::NotificationsBlocked => SituationKind::NotificationsBlocked,
            Self::NotificationsAsk => SituationKind::NotificationsAsk,
            Self::FlowWaiting { .. } => SituationKind::FlowWaiting,
            Self::ServiceUpdateReady { .. } => SituationKind::ServiceUpdateReady,
            Self::Reconnecting => SituationKind::Reconnecting,
        }
    }
}

impl SituationKind {
    /// Whether the user can close it. Reconnecting goes away by itself; stopped, restart required, the usage limit
    /// and the folder trust warning stay until the situation ends; the notifications ask goes with Allow or Not now.
    pub fn dismissable(self) -> bool {
        matches!(
            self,
            Self::PermissionWarning
                | Self::NotificationsBlocked
                | Self::TrustNote
                | Self::ServiceUpdateReady
                | Self::SessionsInterrupted
        )
    }
}

/// "3 agents working, 2 waiting".
pub fn agent_counts(working: u32, waiting: u32) -> String {
    let agents = if working == 1 { "agent" } else { "agents" };
    format!("{working} {agents} working, {waiting} waiting")
}

/// What one banner row shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BannerRow {
    pub kind: SituationKind,
    pub title: String,
    pub text: String,
    /// The Restart now button (update ready, restart required), Start service (stopped), Resume all or Apply now (a
    /// flow change waiting).
    pub action: Option<String>,
    /// A second button: "Review & resume" (opens the after-restart summary sheet).
    pub secondary: Option<&'static str>,
    pub dismissable: bool,
    /// The usage limit's Resume: its label, whether it can be pressed, and whether it is highlighted.
    pub resume: Option<crate::view_models::limit_banner::ResumeButton>,
    pub tooltip: Option<BannerTooltip>,
    /// The usage limit's lines, one per tool.
    pub lines: Vec<LineView>,
}

/// A banner's tooltip: a bold line, then one line each (the reasons of several failed projects).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct BannerTooltip {
    pub title: String,
    pub lines: Vec<String>,
}

/// The banner's colour (*01 · Components → Banners*): turquoise for the usage limit, amber for warnings, the accent for
/// calm news, red when something broke and waits, grey while reconnecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BannerTone {
    Attention,
    Warning,
    Info,
    Error,
    Neutral,
}

/// Which of a banner's buttons was pressed, as the page sends it back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BannerButtonId {
    /// Restart now, Start service, Resume all (after interrupted sessions), Read the docs, Allow (the notifications
    /// ask), Open System Settings (notifications blocked) or Apply now (a flow change waiting: its window, APN-01).
    Action,
    /// Review & resume, the notifications ask's Not now, or Cancel change (a flow change waiting).
    Secondary,
    /// The usage limit's Resume.
    Resume,
}

/// One button of a banner as drawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct BannerButton {
    pub id: BannerButtonId,
    pub label: String,
    pub icon: IconName,
    pub enabled: bool,
    /// Highlighted (the usage limit's Resume after the reset).
    pub primary: bool,
}

/// One banner row as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "Banner")]
pub struct BannerView {
    pub kind: SituationKind,
    pub tone: BannerTone,
    pub icon: IconName,
    pub title: String,
    pub text: String,
    pub buttons: Vec<BannerButton>,
    pub dismissable: bool,
    /// Shown on hovering the text.
    pub tooltip: Option<BannerTooltip>,
    /// The usage limit's lines, one per tool (USG-10); empty for every other banner. The page draws these in place of
    /// the title, text and buttons when there are any.
    pub lines: Vec<LineView>,
}

impl BannerRow {
    /// The row as drawn: tone and icon by situation; while the service starts or restarts (`busy`) its button says
    /// so and nothing can be pressed twice.
    pub fn view(&self, busy: bool) -> BannerView {
        let (tone, icon) = match self.kind {
            SituationKind::Paused => (BannerTone::Warning, IconName::Pause),
            SituationKind::ServiceStopped => (BannerTone::Error, IconName::ServerOff),
            SituationKind::SessionsInterrupted => (BannerTone::Error, IconName::ZapOff),
            SituationKind::RestartRequired => (BannerTone::Warning, IconName::ServerCog),
            // Neutral, never turquoise (d0077); its tools that aren't available are amber in their own lines.
            SituationKind::UsageLimit => (BannerTone::Neutral, IconName::Gauge),
            SituationKind::TrustFailed => (BannerTone::Warning, IconName::ShieldX),
            SituationKind::PermissionWarning => (BannerTone::Warning, IconName::ShieldAlert),
            SituationKind::TrustNote => (BannerTone::Info, IconName::ShieldCheck),
            SituationKind::NotificationsBlocked => (BannerTone::Warning, IconName::TriangleAlert),
            SituationKind::NotificationsAsk => (BannerTone::Info, IconName::Bell),
            SituationKind::FlowWaiting => (BannerTone::Warning, IconName::Hourglass),
            SituationKind::ServiceUpdateReady => (BannerTone::Info, IconName::RefreshCw),
            SituationKind::Reconnecting => (BannerTone::Neutral, IconName::RefreshCw),
        };
        let mut buttons = Vec::new();
        if let Some(label) = self.action.as_deref() {
            let restarts = matches!(
                self.kind,
                SituationKind::ServiceStopped
                    | SituationKind::RestartRequired
                    | SituationKind::ServiceUpdateReady
            );
            let (label, icon) = match (self.kind, busy && restarts) {
                (SituationKind::ServiceStopped, true) => ("Starting…", IconName::LoaderCircle),
                (_, true) => ("Restarting…", IconName::LoaderCircle),
                (SituationKind::ServiceStopped | SituationKind::SessionsInterrupted, false) => {
                    (label, IconName::Play)
                }
                (SituationKind::PermissionWarning | SituationKind::NotificationsBlocked, false) => {
                    (label, IconName::ExternalLink)
                }
                (SituationKind::NotificationsAsk, false) => (label, IconName::Bell),
                (SituationKind::FlowWaiting, false) => (label, IconName::Play),
                (_, false) => (label, IconName::RotateCw),
            };
            buttons.push(BannerButton {
                id: BannerButtonId::Action,
                label: label.to_owned(),
                icon,
                enabled: !(busy && restarts),
                primary: false,
            });
        }
        if let Some(label) = self.secondary {
            // The ask's Not now closes it, and Cancel change drops the change; Review & resume opens the summary.
            let icon = match self.kind {
                SituationKind::NotificationsAsk | SituationKind::FlowWaiting => IconName::X,
                _ => IconName::ArrowRight,
            };
            buttons.push(BannerButton {
                id: BannerButtonId::Secondary,
                label: label.to_owned(),
                icon,
                enabled: true,
                primary: false,
            });
        }
        if let Some(resume) = &self.resume {
            buttons.push(BannerButton {
                id: BannerButtonId::Resume,
                label: resume.label.clone(),
                icon: IconName::Play,
                enabled: resume.enabled && !busy,
                primary: resume.primary,
            });
        }
        BannerView {
            kind: self.kind,
            tone,
            icon,
            title: self.title.clone(),
            text: self.text.clone(),
            buttons,
            dismissable: self.dismissable,
            tooltip: self.tooltip.clone(),
            lines: self.lines.clone(),
        }
    }
}

const RECONNECTING_SUFFIX: &str = "  ·  ↻ reconnecting to the background service…";

/// Up to this many banner rows are shown at once.
pub const MAX_ROWS: usize = 2;

#[derive(Debug, Default, Clone)]
pub struct BannerModel {
    active: BTreeMap<SituationKind, Situation>,
    dismissed: BTreeSet<SituationKind>,
}

impl BannerModel {
    /// Raises (or updates) a situation. Raising the same situation again replaces it, never adds a banner.
    pub fn raise(&mut self, situation: Situation) {
        self.active.insert(situation.kind(), situation);
    }

    /// The situation has ended; a later raise shows it again (also after a dismissal).
    pub fn clear(&mut self, kind: SituationKind) {
        self.active.remove(&kind);
        self.dismissed.remove(&kind);
    }

    /// The user closed the banner. Ignored for banners that can't be closed.
    pub fn dismiss(&mut self, kind: SituationKind) {
        if kind.dismissable() {
            self.dismissed.insert(kind);
        }
    }

    pub fn is_active(&self, kind: SituationKind) -> bool {
        self.active.contains_key(&kind)
    }

    /// The folder trust banners from the service's notices state (PRJ-08): the note while it is pending, the warning
    /// while any project's trust couldn't be recorded. A note the user closed stays closed until the service says
    /// it is no longer pending.
    pub fn set_trust(&mut self, note: bool, tools: Vec<Tool>, failed: Vec<TrustFailure>) {
        if note {
            self.raise(Situation::TrustNote { tools });
        } else {
            self.clear(SituationKind::TrustNote);
        }
        if failed.is_empty() {
            self.clear(SituationKind::TrustFailed);
        } else {
            self.raise(Situation::TrustFailed { projects: failed });
        }
    }

    /// The notification banners from `notifications.state` (ATT-13, ATT-14): the ask while pending, the blocked
    /// banner while the service shows it (the service keeps its closing).
    pub fn set_notifications(&mut self, ask: bool, blocked: bool) {
        if ask {
            self.raise(Situation::NotificationsAsk);
        } else {
            self.clear(SituationKind::NotificationsAsk);
        }
        if blocked {
            self.raise(Situation::NotificationsBlocked);
        } else {
            self.clear(SituationKind::NotificationsBlocked);
        }
    }

    /// With "Restart required", only Restart works: everything else that talks to the service is disabled.
    pub fn only_restart_allowed(&self) -> bool {
        self.is_active(SituationKind::RestartRequired)
    }

    /// The rows to draw, top first.
    pub fn rows(&self) -> Vec<BannerRow> {
        let shown: Vec<&Situation> = self
            .active
            .values()
            .filter(|s| !self.dismissed.contains(&s.kind()))
            .collect();
        let reconnecting = shown
            .iter()
            .any(|s| s.kind() == SituationKind::Reconnecting);
        let others: Vec<&Situation> = shown
            .iter()
            .copied()
            .filter(|s| s.kind() != SituationKind::Reconnecting)
            .collect();
        if others.is_empty() {
            return if reconnecting {
                vec![row(&Situation::Reconnecting)]
            } else {
                Vec::new()
            };
        }
        let mut rows: Vec<BannerRow> = others.into_iter().take(MAX_ROWS).map(row).collect();
        if reconnecting {
            rows[0].text.push_str(RECONNECTING_SUFFIX);
        }
        rows
    }
}

fn row(situation: &Situation) -> BannerRow {
    let kind = situation.kind();
    let mut secondary = None;
    let mut resume = None;
    let mut tooltip = None;
    let mut lines = Vec::new();
    let (title, text, action) = match situation {
        // `s24-07`, `s24-08`: no button; Resume is the title bar's.
        Situation::Paused { title, text } => (title.clone(), text.clone(), None),
        Situation::ServiceStopped => (
            "Background service stopped".to_owned(),
            "Its agent sessions ended with it. Nothing restarts on its own.".to_owned(),
            Some("Start service"),
        ),
        Situation::ServiceCouldNotStart { reason } => (
            "Background service couldn’t start".to_owned(),
            reason.clone(),
            Some("Start service"),
        ),
        // *16 → Opening after a reboot*; *11 → After the service restart*.
        Situation::SessionsInterrupted {
            count,
            unclean,
            at,
            agents,
            leads,
        } => {
            let sessions = if *count == 1 { "session was" } else { "sessions were" };
            // "8 sessions were interrupted · 2 conversations with Noyan ended" (*16 → Opening after a reboot*), "4 sessions were
            // interrupted · 1 epic lead ended" (*16 → Epic lead after a restart*).
            let ended: String = crate::view_models::after_restart::ended(*agents as usize, *leads as usize)
                .iter()
                .map(|e| format!(" \u{b7} {e}"))
                .collect();
            if *unclean {
                secondary = Some("Review & resume");
                (
                    format!("{count} {sessions} interrupted{ended}"),
                    format!(
                        "The background service started again at {at} after a reboot or crash. Nothing restarts on its own."
                    ),
                    Some("Resume all"),
                )
            } else {
                secondary = Some("Review & resume");
                (
                    "Service restarted".to_owned(),
                    format!(
                        "{count} {sessions} interrupted by the restart{ended} · waiting tickets keep their questions"
                    ),
                    Some("Resume all"),
                )
            }
        }
        Situation::RestartRequired { working, waiting } => (
            "Restart required".to_owned(),
            format!(
                "This app can’t talk to the running service (older version) · {}",
                agent_counts(*working, *waiting)
            ),
            Some("Restart now"),
        ),
        Situation::UsageLimit {
            title,
            text,
            resume: button,
            lines: drawn,
        } => {
            resume = button.clone();
            lines = drawn.clone();
            (title.clone(), text.clone(), None)
        }
        // "Claude Code doesn’t trust NOTE’s folder: <reason>. Its agents will wait on Claude Code’s trust screen.";
        // with several, the keys in the text and each reason in the tooltip.
        Situation::TrustFailed { projects } => match projects.as_slice() {
            [one] => (
                format!(
                    "Claude Code doesn\u{2019}t trust {}\u{2019}s folder:",
                    one.project
                ),
                format!("{}. {TRUST_SCREEN_ONE}", one.reason),
                None,
            ),
            several => {
                let title = format!(
                    "Claude Code doesn\u{2019}t trust {} projects\u{2019} folders",
                    several.len()
                );
                let keys: Vec<&str> = several.iter().map(|p| p.project.as_str()).collect();
                tooltip = Some(BannerTooltip {
                    title: title.clone(),
                    lines: several
                        .iter()
                        .map(|p| format!("{}: {}.", p.project, p.reason))
                        .collect(),
                });
                (
                    format!("{title}:"),
                    format!("{}. {TRUST_SCREEN_SEVERAL}", keys.join(", ")),
                    None,
                )
            }
        },
        // The note is one sentence: no title.
        Situation::TrustNote { tools } => (String::new(), trust_note(tools), None),
        // One sentence each: no title (ATT-13, ATT-14; d0090, *CHANGES-2026-10-18-aa* §3, §5).
        Situation::NotificationsAsk => {
            secondary = Some(NOT_NOW);
            (String::new(), notifications_ask(), Some(ALLOW))
        }
        Situation::NotificationsBlocked => (
            String::new(),
            notifications_blocked(),
            Some(OPEN_SYSTEM_SETTINGS),
        ),
        Situation::PermissionWarning => (
            "Permission prompts are on".to_owned(),
            "With 5–10 agents, permission prompts will stop agents often. See allowlists · auto mode in the Claude Code docs".to_owned(),
            Some("Read the docs"),
        ),
        // *11 → A flow change waiting*: Apply now (its window, APN-01), then Cancel change (no question). Show the N
        // tickets went in M35 (d0108 changelog 2026-10-21): the window lists the same tickets.
        Situation::FlowWaiting { summary, count } => {
            secondary = Some(CANCEL_CHANGE);
            (
                FLOW_CHANGE_WAITING.to_owned(),
                flow_waiting_text(summary, *count),
                (*count > 0).then_some(APPLY_NOW),
            )
        }
        Situation::ServiceUpdateReady { working, waiting } => (
            "Service update ready".to_owned(),
            format!(
                "Restart now ({}) · or later, when it suits you",
                agent_counts(*working, *waiting)
            ),
            Some("Restart now"),
        ),
        Situation::Reconnecting => (
            "Reconnecting…".to_owned(),
            "Lost contact with the background service. Agents keep running; the board will catch up."
                .to_owned(),
            None,
        ),
    };
    BannerRow {
        kind,
        title,
        text,
        action: action.map(str::to_owned),
        secondary,
        dismissable: kind.dismissable(),
        resume,
        tooltip,
        lines,
    }
}

/// The flow change banner's title (FLW-06).
pub const FLOW_CHANGE_WAITING: &str = "Flow change waiting";
/// Drops the waiting change, with no question (FLW-06): the banner's and Settings → Flow's.
pub const CANCEL_CHANGE: &str = "Cancel change";

/// "8 tickets", "1 ticket".
pub fn tickets_count(count: usize) -> String {
    if count == 1 {
        "1 ticket".to_owned()
    } else {
        format!("{count} tickets")
    }
}

/// The flow change banner's text (FLW-06): "QA after Security · 8 tickets still working. It applies when they’re
/// through; until then no new ticket starts."
pub fn flow_waiting_text(summary: &str, count: usize) -> String {
    let tail = format!(
        "{} still working. It applies when they\u{2019}re through; until then no new ticket starts.",
        tickets_count(count)
    );
    if summary.is_empty() {
        tail
    } else {
        format!("{summary} \u{b7} {tail}")
    }
}

/// Opens the Apply now window (APN-01); not offered while the change waits for no ticket (it applies at the
/// scheduler's next pass).
pub const APPLY_NOW: &str = crate::view_models::apply_now::APPLY_NOW;

/// Otaq's ask before macOS's own question (ATT-13, d0090).
pub fn notifications_ask() -> String {
    format!(
        "{} tells you when an agent needs permission or asks a question. Allow notifications?",
        otaq_core::naming::PRODUCT_NAME
    )
}
pub const ALLOW: &str = "Allow";
pub const NOT_NOW: &str = "Not now";

/// The banner when macOS blocked a notification that mattered (ATT-14, d0090).
pub fn notifications_blocked() -> String {
    format!(
        "macOS is blocking {}\u{2019}s notifications, so you won\u{2019}t hear when an agent needs you.",
        otaq_core::naming::PRODUCT_NAME
    )
}

/// Opens Otaq's entry in System Settings → Notifications (the blocked banner, the Settings status line).
pub const OPEN_SYSTEM_SETTINGS: &str = "Open System Settings";

/// The one-time folder trust note (PRJ-08, d0085), naming the tools whose trust Otaq recorded ("Claude Code and
/// Codex"); Claude Code when none is named.
pub fn trust_note(tools: &[Tool]) -> String {
    let names: Vec<&str> = match tools {
        [] => vec![Tool::ALL[0].display_name()],
        tools => tools.iter().map(|t| t.display_name()).collect(),
    };
    format!(
        "{} let {} work in your projects\u{2019} folders, as adding a project now does.",
        otaq_core::naming::PRODUCT_NAME,
        and_list(&names)
    )
}

/// What a failed trust means, after the reason: for one project, and for several (Q74).
pub const TRUST_SCREEN_ONE: &str = "Its agents will wait on Claude Code\u{2019}s trust screen.";
pub const TRUST_SCREEN_SEVERAL: &str =
    "Their agents will wait on Claude Code\u{2019}s trust screen.";

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(model: &BannerModel) -> Vec<SituationKind> {
        model.rows().iter().map(|r| r.kind).collect()
    }

    #[test]
    fn the_same_situation_twice_is_one_banner() {
        let mut m = BannerModel::default();
        m.raise(Situation::ServiceUpdateReady {
            working: 1,
            waiting: 0,
        });
        m.raise(Situation::ServiceUpdateReady {
            working: 3,
            waiting: 2,
        });
        let rows = m.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].text,
            "Restart now (3 agents working, 2 waiting) · or later, when it suits you"
        );
    }

    /// FLW-06, APN-01 (*11 → A flow change waiting*): the banner's text and count, Apply now and Cancel change (no
    /// Show the N tickets any more); under the permission warning; it can't be closed.
    #[test]
    fn a_flow_change_waiting() {
        let mut m = BannerModel::default();
        m.raise(Situation::FlowWaiting {
            summary: "QA after Security".into(),
            count: 8,
        });
        m.raise(Situation::PermissionWarning);
        assert_eq!(
            kinds(&m),
            [SituationKind::PermissionWarning, SituationKind::FlowWaiting]
        );
        let view = m.rows()[1].view(false);
        assert_eq!(view.tone, BannerTone::Warning);
        assert_eq!(view.title, "Flow change waiting");
        assert_eq!(
            view.text,
            "QA after Security \u{b7} 8 tickets still working. It applies when they\u{2019}re through; until then no \
             new ticket starts."
        );
        let buttons: Vec<(BannerButtonId, &str)> = view
            .buttons
            .iter()
            .map(|b| (b.id, b.label.as_str()))
            .collect();
        assert_eq!(
            buttons,
            [
                (BannerButtonId::Action, "Apply now"),
                (BannerButtonId::Secondary, "Cancel change")
            ]
        );
        assert_eq!(view.buttons[0].icon, IconName::Play);
        assert!(!view.dismissable);
        m.dismiss(SituationKind::FlowWaiting);
        assert_eq!(m.rows().len(), 2, "stays until it applies or is cancelled");

        // Two merged changes and one ticket; none left (it applies at the next pass): no Apply now.
        m.raise(Situation::FlowWaiting {
            summary: "QA after Security \u{b7} Review can be skipped".into(),
            count: 1,
        });
        let view = m.rows()[1].view(false);
        assert!(view.text.starts_with(
            "QA after Security \u{b7} Review can be skipped \u{b7} 1 ticket still working."
        ));
        assert_eq!(view.buttons[0].label, "Apply now");
        m.raise(Situation::FlowWaiting {
            summary: "QA after Security".into(),
            count: 0,
        });
        let view = m.rows()[1].view(false);
        assert_eq!(view.buttons.len(), 1);
        assert_eq!(view.buttons[0].label, "Cancel change");
        m.clear(SituationKind::FlowWaiting);
        assert_eq!(kinds(&m), [SituationKind::PermissionWarning]);
    }

    #[test]
    fn stacking_order_and_at_most_two_rows() {
        let mut m = BannerModel::default();
        m.raise(Situation::ServiceUpdateReady {
            working: 0,
            waiting: 0,
        });
        m.raise(Situation::PermissionWarning);
        m.raise(Situation::UsageLimit {
            title: "Session limit reached".into(),
            text: "Resets at 3:00 pm".into(),
            resume: None,
            lines: Vec::new(),
        });
        assert_eq!(
            kinds(&m),
            [SituationKind::UsageLimit, SituationKind::PermissionWarning]
        );
        m.raise(Situation::RestartRequired {
            working: 0,
            waiting: 0,
        });
        m.raise(Situation::ServiceStopped);
        assert_eq!(
            kinds(&m),
            [
                SituationKind::ServiceStopped,
                SituationKind::RestartRequired
            ]
        );
    }

    #[test]
    fn reconnecting_alone_has_its_own_row() {
        let mut m = BannerModel::default();
        m.raise(Situation::Reconnecting);
        let rows = m.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Reconnecting…");
        assert!(!rows[0].dismissable);
        m.clear(SituationKind::Reconnecting);
        assert!(m.rows().is_empty());
    }

    #[test]
    fn reconnecting_folds_into_another_banner() {
        let mut m = BannerModel::default();
        m.raise(Situation::UsageLimit {
            title: "Session limit reached".into(),
            text: "Resets at 3:00 pm · 8 agents stopped".into(),
            resume: None,
            lines: Vec::new(),
        });
        m.raise(Situation::Reconnecting);
        let rows = m.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].text,
            "Resets at 3:00 pm · 8 agents stopped  ·  ↻ reconnecting to the background service…"
        );
    }

    #[test]
    fn restart_required_disables_other_actions() {
        let mut m = BannerModel::default();
        assert!(!m.only_restart_allowed());
        m.raise(Situation::RestartRequired {
            working: 3,
            waiting: 2,
        });
        assert!(m.only_restart_allowed());
        let rows = m.rows();
        assert_eq!(rows[0].action.as_deref(), Some("Restart now"));
        assert!(
            !rows[0].dismissable,
            "only Restart works; it can't be closed"
        );
        m.dismiss(SituationKind::RestartRequired);
        assert_eq!(m.rows().len(), 1, "dismiss is ignored");
    }

    #[test]
    fn dismissed_banners_stay_away_until_raised_after_clearing() {
        let mut m = BannerModel::default();
        m.raise(Situation::ServiceUpdateReady {
            working: 0,
            waiting: 0,
        });
        m.dismiss(SituationKind::ServiceUpdateReady);
        assert!(m.rows().is_empty());
        m.raise(Situation::ServiceUpdateReady {
            working: 1,
            waiting: 0,
        });
        assert!(m.rows().is_empty(), "same situation, still dismissed");
        m.clear(SituationKind::ServiceUpdateReady);
        m.raise(Situation::ServiceUpdateReady {
            working: 1,
            waiting: 0,
        });
        assert_eq!(m.rows().len(), 1);
    }

    #[test]
    fn interrupted_sessions_after_a_reboot_or_a_restart() {
        let mut m = BannerModel::default();
        m.raise(Situation::SessionsInterrupted {
            count: 4,
            unclean: true,
            at: "02:14".into(),
            agents: 0,
            leads: 0,
        });
        let row = &m.rows()[0];
        assert_eq!(row.title, "4 sessions were interrupted");
        m.raise(Situation::SessionsInterrupted {
            count: 8,
            unclean: true,
            at: "02:14".into(),
            agents: 2,
            leads: 0,
        });
        assert_eq!(
            m.rows()[0].title,
            "8 sessions were interrupted \u{b7} 2 conversations with Noyan ended"
        );
        m.raise(Situation::SessionsInterrupted {
            count: 4,
            unclean: true,
            at: "02:14".into(),
            agents: 0,
            leads: 1,
        });
        assert_eq!(
            m.rows()[0].title,
            "4 sessions were interrupted \u{b7} 1 epic lead ended"
        );
        m.raise(Situation::SessionsInterrupted {
            count: 8,
            unclean: true,
            at: "02:14".into(),
            agents: 2,
            leads: 0,
        });
        let row = &m.rows()[0];
        assert_eq!(
            row.text,
            "The background service started again at 02:14 after a reboot or crash. Nothing restarts on its own."
        );
        assert_eq!(
            (row.action.as_deref(), row.secondary, row.dismissable),
            (Some("Resume all"), Some("Review & resume"), true)
        );
        m.raise(Situation::SessionsInterrupted {
            count: 1,
            unclean: false,
            at: "09:00".into(),
            agents: 0,
            leads: 0,
        });
        let row = &m.rows()[0];
        assert_eq!(row.title, "Service restarted");
        assert_eq!(
            row.text,
            "1 session was interrupted by the restart · waiting tickets keep their questions"
        );
        m.dismiss(SituationKind::SessionsInterrupted);
        assert!(m.rows().is_empty());
    }

    #[test]
    fn a_start_error_is_the_stopped_banner_with_its_reason() {
        let mut m = BannerModel::default();
        m.raise(Situation::ServiceStopped);
        m.raise(Situation::ServiceCouldNotStart {
            reason: "the data folder's path is too long".into(),
        });
        let rows = m.rows();
        assert_eq!(rows.len(), 1, "one situation, one banner");
        assert_eq!(rows[0].title, "Background service couldn’t start");
        assert_eq!(rows[0].text, "the data folder's path is too long");
        let view = rows[0].view(false);
        assert_eq!(
            (view.tone, view.icon),
            (BannerTone::Error, IconName::ServerOff)
        );
        assert_eq!(view.buttons[0].label, "Start service");
        assert!(!view.dismissable);
    }

    #[test]
    fn the_drawn_banner_names_its_buttons() {
        let mut m = BannerModel::default();
        m.raise(Situation::ServiceUpdateReady {
            working: 3,
            waiting: 2,
        });
        let view = m.rows()[0].view(false);
        assert_eq!(
            (view.tone, view.icon),
            (BannerTone::Info, IconName::RefreshCw)
        );
        assert_eq!(
            view.buttons,
            [BannerButton {
                id: BannerButtonId::Action,
                label: "Restart now".into(),
                icon: IconName::RotateCw,
                enabled: true,
                primary: false,
            }]
        );
        assert!(view.dismissable);
        let busy = m.rows()[0].view(true);
        assert_eq!(busy.buttons[0].label, "Restarting…");
        assert_eq!(busy.buttons[0].icon, IconName::LoaderCircle);
        assert!(!busy.buttons[0].enabled, "never pressed twice");

        let stopped = row(&Situation::ServiceStopped).view(true);
        assert_eq!(stopped.buttons[0].label, "Starting…");

        let interrupted = row(&Situation::SessionsInterrupted {
            count: 4,
            unclean: true,
            at: "02:14".into(),
            agents: 0,
            leads: 0,
        })
        .view(false);
        let labels: Vec<(&str, IconName)> = interrupted
            .buttons
            .iter()
            .map(|b| (b.label.as_str(), b.icon))
            .collect();
        assert_eq!(
            labels,
            [
                ("Resume all", IconName::Play),
                ("Review & resume", IconName::ArrowRight)
            ]
        );
        let warning = row(&Situation::PermissionWarning).view(true);
        assert_eq!(
            (
                warning.tone,
                warning.buttons[0].icon,
                warning.buttons[0].enabled
            ),
            (BannerTone::Warning, IconName::ExternalLink, true),
            "a restart doesn't hold Read the docs"
        );
    }

    #[test]
    fn the_usage_limit_draws_its_resume() {
        let limit = |enabled, primary| Situation::UsageLimit {
            title: "Usage limit reset".into(),
            text: "Your plan limit reset".into(),
            resume: Some(crate::view_models::limit_banner::ResumeButton {
                label: "Resume (8)".into(),
                enabled,
                primary,
            }),
            lines: Vec::new(),
        };
        let view = row(&limit(true, true)).view(false);
        assert_eq!(view.tone, BannerTone::Neutral, "never turquoise");
        assert_eq!(
            view.buttons,
            [BannerButton {
                id: BannerButtonId::Resume,
                label: "Resume (8)".into(),
                icon: IconName::Play,
                enabled: true,
                primary: true,
            }]
        );
        assert!(!row(&limit(false, false)).view(false).buttons[0].enabled);
        assert!(!row(&limit(true, true)).view(true).buttons[0].enabled);
        assert!(!view.dismissable);
    }

    fn failure(project: &str, reason: &str) -> TrustFailure {
        TrustFailure {
            project: project.into(),
            reason: reason.into(),
        }
    }

    #[test]
    fn the_folder_trust_note_names_the_tools() {
        let product = otaq_core::naming::PRODUCT_NAME;
        assert_eq!(
            trust_note(&[Tool::Codex]),
            format!(
                "{product} let Codex work in your projects’ folders, as adding a project now does."
            )
        );
        assert_eq!(
            trust_note(&[Tool::Claude, Tool::Codex, Tool::Copilot]),
            format!(
                "{product} let Claude Code, Codex and GitHub Copilot work in your projects’ folders, as adding a \
                 project now does."
            )
        );
        let mut m = BannerModel::default();
        m.set_trust(true, vec![Tool::Claude, Tool::Codex], Vec::new());
        assert!(m.rows()[0].text.contains("Claude Code and Codex work"));
    }

    #[test]
    fn the_folder_trust_note_is_calm_and_closes() {
        let mut m = BannerModel::default();
        m.set_trust(true, Vec::new(), Vec::new());
        let view = m.rows()[0].view(false);
        assert_eq!(view.kind, SituationKind::TrustNote);
        assert_eq!(view.title, "");
        assert_eq!(
            view.text,
            format!(
                "{} let Claude Code work in your projects’ folders, as adding a project now does.",
                otaq_core::naming::PRODUCT_NAME
            )
        );
        assert_eq!(
            (view.tone, view.icon),
            (BannerTone::Info, IconName::ShieldCheck)
        );
        assert!(view.buttons.is_empty());
        assert!(view.dismissable, "one dismiss (×)");
        assert_eq!(view.tooltip, None);
        // Closed: hidden at once, and still hidden while the service hasn't caught up.
        m.dismiss(SituationKind::TrustNote);
        assert!(m.rows().is_empty());
        m.set_trust(true, Vec::new(), Vec::new());
        assert!(m.rows().is_empty());
        m.set_trust(false, Vec::new(), Vec::new());
        assert!(m.rows().is_empty());
    }

    #[test]
    fn the_folder_trust_warning_for_one_project() {
        let mut m = BannerModel::default();
        m.set_trust(
            false,
            Vec::new(),
            vec![failure(
                "NOTE",
                "Claude Code’s config file couldn’t be written",
            )],
        );
        let view = m.rows()[0].view(false);
        assert_eq!(view.kind, SituationKind::TrustFailed);
        assert_eq!(
            format!("{} {}", view.title, view.text),
            "Claude Code doesn’t trust NOTE’s folder: Claude Code’s config file couldn’t be written. Its agents will \
             wait on Claude Code’s trust screen."
        );
        assert_eq!(
            (view.tone, view.icon),
            (BannerTone::Warning, IconName::ShieldX)
        );
        assert!(view.buttons.is_empty(), "no button");
        assert!(!view.dismissable, "no dismiss");
        assert_eq!(view.tooltip, None);
        m.dismiss(SituationKind::TrustFailed);
        assert_eq!(m.rows().len(), 1, "it can't be closed");
        // A later startup recorded the trust.
        m.set_trust(false, Vec::new(), Vec::new());
        assert!(m.rows().is_empty());
    }

    #[test]
    fn the_folder_trust_warning_for_two_projects() {
        let mut m = BannerModel::default();
        m.set_trust(
            false,
            Vec::new(),
            vec![
                failure("NOTE", "Claude Code’s config file couldn’t be written"),
                failure("SHOP", "Claude Code never keeps trust for your home folder"),
            ],
        );
        let rows = m.rows();
        assert_eq!(rows.len(), 1, "one banner for every failed project");
        let view = rows[0].view(false);
        assert_eq!(
            format!("{} {}", view.title, view.text),
            "Claude Code doesn’t trust 2 projects’ folders: NOTE, SHOP. Their agents will wait on Claude Code’s trust \
             screen."
        );
        assert_eq!(
            view.tooltip,
            Some(BannerTooltip {
                title: "Claude Code doesn’t trust 2 projects’ folders".into(),
                lines: vec![
                    "NOTE: Claude Code’s config file couldn’t be written.".into(),
                    "SHOP: Claude Code never keeps trust for your home folder.".into(),
                ],
            })
        );
    }

    #[test]
    fn the_folder_trust_banners_stack_around_the_permission_warning() {
        let limit = || Situation::UsageLimit {
            title: "Session limit reached".into(),
            text: "Resets at 3:00 pm".into(),
            resume: None,
            lines: Vec::new(),
        };
        let mut m = BannerModel::default();
        m.raise(Situation::PermissionWarning);
        m.set_trust(true, Vec::new(), vec![failure("NOTE", "why")]);
        assert_eq!(
            kinds(&m),
            [SituationKind::TrustFailed, SituationKind::PermissionWarning],
            "the warning above the permission warning; the note waits for a free row"
        );
        m.set_trust(true, Vec::new(), Vec::new());
        assert_eq!(
            kinds(&m),
            [SituationKind::PermissionWarning, SituationKind::TrustNote],
            "the note below the permission warning"
        );
        m.set_trust(true, Vec::new(), vec![failure("NOTE", "why")]);
        m.raise(limit());
        assert_eq!(
            kinds(&m),
            [SituationKind::UsageLimit, SituationKind::TrustFailed],
            "the usage limit stays on top"
        );
        m.clear(SituationKind::PermissionWarning);
        m.clear(SituationKind::UsageLimit);
        m.set_trust(true, Vec::new(), Vec::new());
        m.raise(Situation::Reconnecting);
        let rows = m.rows();
        assert_eq!(rows.len(), 1, "reconnecting folds into the note");
        assert_eq!(rows[0].kind, SituationKind::TrustNote);
        assert!(rows[0].text.ends_with(RECONNECTING_SUFFIX));
        m.raise(limit());
        m.set_trust(false, Vec::new(), vec![failure("NOTE", "why")]);
        let rows = m.rows();
        assert_eq!(
            rows.iter().map(|r| r.kind).collect::<Vec<_>>(),
            [SituationKind::UsageLimit, SituationKind::TrustFailed]
        );
        assert!(rows[0].text.ends_with(RECONNECTING_SUFFIX));
        assert!(!rows[1].text.ends_with(RECONNECTING_SUFFIX));
    }

    /// T-0372 (ATT-13): Otaq's calm ask with Allow and Not now, no close button.
    #[test]
    fn the_notifications_ask_is_calm_with_allow_and_not_now() {
        let mut m = BannerModel::default();
        m.set_notifications(true, false);
        let rows = m.rows();
        assert_eq!(rows.len(), 1);
        let view = rows[0].view(false);
        assert_eq!(view.kind, SituationKind::NotificationsAsk);
        assert_eq!(view.title, "");
        assert_eq!(
            view.text,
            format!(
                "{} tells you when an agent needs permission or asks a question. Allow notifications?",
                otaq_core::naming::PRODUCT_NAME
            )
        );
        assert_eq!((view.tone, view.icon), (BannerTone::Info, IconName::Bell));
        assert_eq!(
            view.buttons
                .iter()
                .map(|b| (b.id, b.label.as_str(), b.icon, b.primary))
                .collect::<Vec<_>>(),
            [
                (BannerButtonId::Action, "Allow", IconName::Bell, false),
                (BannerButtonId::Secondary, "Not now", IconName::X, false),
            ]
        );
        assert!(!view.dismissable, "Not now closes it");
        m.dismiss(SituationKind::NotificationsAsk);
        assert_eq!(m.rows().len(), 1);
        // Answered: the service says it isn't pending any more.
        m.set_notifications(false, false);
        assert!(m.rows().is_empty());
    }

    /// T-0372 (ATT-14): the amber "macOS is blocking" banner with Open System Settings and a close button.
    #[test]
    fn the_notifications_blocked_banner_is_amber_and_closes() {
        let mut m = BannerModel::default();
        m.set_notifications(false, true);
        let view = m.rows()[0].view(false);
        assert_eq!(view.kind, SituationKind::NotificationsBlocked);
        assert_eq!(view.title, "");
        assert_eq!(
            view.text,
            format!(
                "macOS is blocking {}’s notifications, so you won’t hear when an agent needs you.",
                otaq_core::naming::PRODUCT_NAME
            )
        );
        assert_eq!(
            (view.tone, view.icon),
            (BannerTone::Warning, IconName::TriangleAlert)
        );
        assert_eq!(
            view.buttons
                .iter()
                .map(|b| (b.id, b.label.as_str(), b.icon))
                .collect::<Vec<_>>(),
            [(
                BannerButtonId::Action,
                "Open System Settings",
                IconName::ExternalLink
            )]
        );
        assert!(view.dismissable);
        m.dismiss(SituationKind::NotificationsBlocked);
        assert!(m.rows().is_empty());
        // The service keeps the closing; a banner it raises again (after allowed, then off) shows.
        m.set_notifications(false, false);
        m.set_notifications(false, true);
        assert_eq!(kinds(&m), [SituationKind::NotificationsBlocked]);
    }

    /// T-0372: blocked right below the folder trust warning, above the permission warning; the ask right below the
    /// folder trust note; the usage limit stays on top and reconnecting folds into the top row.
    #[test]
    fn the_notification_banners_stack_with_the_others() {
        let limit = || Situation::UsageLimit {
            title: "Session limit reached".into(),
            text: "Resets at 3:00 pm".into(),
            resume: None,
            lines: Vec::new(),
        };
        let mut m = BannerModel::default();
        m.set_notifications(true, true);
        assert_eq!(
            kinds(&m),
            [
                SituationKind::NotificationsBlocked,
                SituationKind::NotificationsAsk
            ]
        );
        m.raise(Situation::PermissionWarning);
        assert_eq!(
            kinds(&m),
            [
                SituationKind::NotificationsBlocked,
                SituationKind::PermissionWarning
            ],
            "blocked above the permission warning"
        );
        m.set_trust(false, Vec::new(), vec![failure("NOTE", "why")]);
        assert_eq!(
            kinds(&m),
            [
                SituationKind::TrustFailed,
                SituationKind::NotificationsBlocked
            ],
            "below the folder trust warning"
        );
        m.raise(limit());
        assert_eq!(
            kinds(&m),
            [SituationKind::UsageLimit, SituationKind::TrustFailed],
            "the usage limit stays on top"
        );
        m.clear(SituationKind::UsageLimit);
        m.clear(SituationKind::PermissionWarning);
        m.set_trust(true, Vec::new(), Vec::new());
        m.set_notifications(true, false);
        assert_eq!(
            kinds(&m),
            [SituationKind::TrustNote, SituationKind::NotificationsAsk],
            "the ask below the folder trust note"
        );
        m.raise(Situation::Reconnecting);
        let rows = m.rows();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].text.ends_with(RECONNECTING_SUFFIX));
        assert!(!rows[1].text.ends_with(RECONNECTING_SUFFIX));
        m.set_trust(false, Vec::new(), Vec::new());
        let rows = m.rows();
        assert_eq!(rows.len(), 1, "reconnecting folds into the ask");
        assert_eq!(rows[0].kind, SituationKind::NotificationsAsk);
        assert!(rows[0].text.ends_with(RECONNECTING_SUFFIX));
    }

    #[test]
    fn counts_read_naturally() {
        assert_eq!(agent_counts(1, 0), "1 agent working, 0 waiting");
        assert_eq!(agent_counts(3, 2), "3 agents working, 2 waiting");
    }
}

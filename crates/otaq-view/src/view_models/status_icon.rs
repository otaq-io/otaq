//! The status icon in the menu bar / tray (d0139, TRY-01, TRY-04, TRY-05): its look, its accessible name and its
//! native menu, from the app's link to the service and the service's lists. T-0566 has the plain and not connected
//! looks with Open Otaq and a stand-in Quit Otaq ([`StatusIconView`]); T-0567 the [`summary`] across all projects
//! (the count, the lists, Pause / Resume all); T-0568 draws the summary as page 31 does.
//!
//! **Not connected** follows the link with a grace period ([`NOT_CONNECTED_AFTER`]): a link down for less than that
//! (a service restart, a dropped connection the client retries at once) keeps the icon as it was, so it doesn't
//! flicker; a link still down after it shows the not-connected look until it is connected again.

use std::time::{Duration, Instant};

use otaq_core::agents::pause::PauseCounts;
use otaq_core::attention::needs_you::NeedsYouList;
use otaq_core::board::needs_you::{NeedsYouGroup, NeedsYouItem, RunningItem};
use otaq_core::domain::Status;
use otaq_core::naming::PRODUCT_NAME;

use crate::view_models::connection::LinkState;
use crate::view_models::needs_you_strip::{self, Item, Target};
use crate::view_models::session_tab;

/// How long the link may be down (reconnecting, stopped, failed) before the icon says not connected. The client
/// retries from 100 ms, doubling to 2 s, so a service that comes back reconnects well within it.
pub const NOT_CONNECTED_AFTER: Duration = Duration::from_secs(3);

/// The icon's look (page 31 item 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconLook {
    /// Otaq is running.
    Plain,
    /// Something blocks an agent: the count is drawn.
    NeedsYou,
    /// Every project is paused, however each got paused.
    AllPaused,
    /// Every project is paused and a paused ticket still asks: the count is drawn too.
    AllPausedNeedsYou,
    /// The app can't reach its background service: no count, no pause; wins over the others.
    NotConnected,
}

/// What a menu item does when picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconAction {
    /// Shows and focuses the window.
    OpenOtaq,
    /// Quits the app only, agents keep working (stand-in until T-0569's Stop agents and quit; T-0568 keeps it as the
    /// not-connected menu's Quit Otaq, audit AS q2).
    Quit,
    /// Pauses every project (0112's All projects scope, no question).
    PauseAll,
    /// Lifts what an All projects pause set (d0112 2026-10-27).
    ResumeAll,
    /// Stops every agent and the service, pauses all projects, then quits (T-0569).
    StopAgentsAndQuit,
}

impl IconAction {
    pub const ALL: [IconAction; 5] = [
        IconAction::OpenOtaq,
        IconAction::Quit,
        IconAction::PauseAll,
        IconAction::ResumeAll,
        IconAction::StopAgentsAndQuit,
    ];

    /// The native menu item's id.
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenOtaq => "open",
            Self::Quit => "quit",
            Self::PauseAll => "pause_all",
            Self::ResumeAll => "resume_all",
            Self::StopAgentsAndQuit => "stop_and_quit",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }
}

/// One row of the native menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuRow {
    /// A disabled line that says something ("Otaq lost its connection").
    Line(String),
    Action {
        action: IconAction,
        label: String,
    },
    Separator,
}

/// Everything the door shows for the icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusIconView {
    pub look: IconLook,
    /// The tooltip and accessible name (page 31 "Icon names").
    pub name: String,
    pub menu: Vec<MenuRow>,
}

/// "Open Otaq" (page 31).
pub fn open_label() -> String {
    format!("Open {PRODUCT_NAME}")
}

/// "Quit Otaq" (page 31's not-connected menu, audit AS q2).
pub fn quit_label() -> String {
    format!("Quit {PRODUCT_NAME}")
}

/// The not-connected menu's line, "Otaq lost its connection".
pub fn lost_connection() -> String {
    format!("{PRODUCT_NAME} lost its connection")
}

/// The icon's name per look and count (page 31 "Icon names"): "Otaq", "Otaq · 2 need you", "Otaq · all projects
/// paused", "Otaq · all projects paused · 1 needs you", "Otaq · not connected to its background service".
pub fn icon_name(look: IconLook, count: u32) -> String {
    let need = if count == 1 { "needs" } else { "need" };
    match look {
        IconLook::Plain => PRODUCT_NAME.to_owned(),
        IconLook::NeedsYou => format!("{PRODUCT_NAME} \u{b7} {count} {need} you"),
        IconLook::AllPaused => format!("{PRODUCT_NAME} \u{b7} all projects paused"),
        IconLook::AllPausedNeedsYou => {
            format!("{PRODUCT_NAME} \u{b7} all projects paused \u{b7} {count} {need} you")
        }
        IconLook::NotConnected => {
            format!("{PRODUCT_NAME} \u{b7} not connected to its background service")
        }
    }
}

impl StatusIconView {
    pub fn of(look: IconLook) -> Self {
        let open = MenuRow::Action {
            action: IconAction::OpenOtaq,
            label: open_label(),
        };
        let quit = MenuRow::Action {
            action: IconAction::Quit,
            label: quit_label(),
        };
        let menu = match look {
            IconLook::NotConnected => vec![
                MenuRow::Line(lost_connection()),
                open,
                MenuRow::Separator,
                quit,
            ],
            // The summary's looks are drawn from [`summary`] (T-0568); until then the plain menu.
            _ => vec![open, MenuRow::Separator, quit],
        };
        Self {
            look,
            name: icon_name(look, 0),
            menu,
        }
    }
}

/// The link part of the icon's state (its [`IconLook::NotConnected`] with the grace period), fed the link's states as
/// they come.
#[derive(Debug, Default)]
pub struct IconLink {
    /// Since when the link has been down; `None` while connected or connecting.
    down_since: Option<Instant>,
}

impl IconLink {
    /// The link is now `state`.
    pub fn on_link(&mut self, state: &LinkState, now: Instant) {
        match state {
            LinkState::Reconnecting { .. } | LinkState::Stopped | LinkState::Failed(_) => {
                self.down_since.get_or_insert(now);
            }
            // Connecting is the app's own start: nothing is known yet, so the icon stays plain.
            LinkState::Connected(_) | LinkState::Connecting => self.down_since = None,
        }
    }

    pub fn connection_look(&self, now: Instant) -> IconLook {
        match self.down_since {
            Some(since) if now.duration_since(since) >= NOT_CONNECTED_AFTER => {
                IconLook::NotConnected
            }
            _ => IconLook::Plain,
        }
    }

    pub fn view(&self, now: Instant) -> StatusIconView {
        StatusIconView::of(self.connection_look(now))
    }

    /// When the look changes by itself (the grace period ends), if the link stays as it is.
    pub fn changes_at(&self, now: Instant) -> Option<Instant> {
        let at = self.down_since? + NOT_CONNECTED_AFTER;
        (at > now).then_some(at)
    }
}

// The summary across all projects (T-0567; TRY-04, TRY-05; d0139 gap review G4, G6, R6, audit AS q1, q2; d0114
// refined 2026-10-27; page 31 `s31-01`, `s31-04`):
// - The count is what blocks an agent right now: the needs-you family (questions, permission prompts, Idle, a check
//   that needs a decision; also a paused or interrupted ticket that kept its question, d0045) and the Noyan
//   conversations waiting for an answer. Your checks and Interrupted / Start failed tickets never count. Never capped.
// - The lists, in order: Needs you (0016's order), Your checks and stopped tickets (listed, not counted), Running (one
//   line per ticket an agent works on, with its stage). A ticket shows once, in the first list it belongs to. A paused
//   project's Interrupted tickets wait for its Resume (as the strip leaves them out). Projects paused on their own
//   are named under Paused.
// - The cap: at most [`CAP`] ticket lines; needs-you lines come first, so they are cut last; past the cap "and 31 more
//   · Open Otaq". The headings' counts stay true ("Running · 36").
// - The actions: Open Otaq; Pause all while any project isn't paused; Resume all only while an All projects pause
//   exists (audit AS q1); Stop agents and quit. Not connected: Pause all disabled, a plain Quit Otaq (audit AS q2).

/// The most ticket lines the menu lists (page 31: "The long list stops at 10 ticket lines").
pub const CAP: usize = 10;

pub const NEEDS_YOU: &str = "Needs you";
pub const CHECKS_AND_STOPPED: &str = "Your checks and stopped tickets";
pub const RUNNING: &str = "Running";
pub const PAUSED: &str = "Paused";
pub const ALL_PAUSED_NOTHING_RUNNING: &str = "All projects paused \u{b7} nothing running";
pub const NOTHING: &str = "Nothing needs you \u{b7} nothing running";
pub const PAUSE_ALL: &str = "Pause all projects";
pub const RESUME_ALL: &str = "Resume all projects";
pub const STOP_AND_QUIT: &str = "Stop agents and quit";

/// One line of the menu, above its actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// A disabled heading: "Needs you · 2", "Your checks and stopped tickets", "Running · 2", "Paused".
    Heading(String),
    /// A ticket's or a Noyan conversation's line ("SHOP-22 · allow a command"); picking it opens `target`.
    /// `status` is the ticket's, for its coloured dot (the board's colours); `None` for a Noyan conversation.
    Item {
        text: String,
        status: Option<Status>,
        target: Target,
    },
    /// A disabled line: "web-app · paused on its own", "Nothing needs you · nothing running", "Otaq lost its
    /// connection".
    Note(String),
    /// "and 31 more · Open Otaq": picking it opens the window.
    More(String),
}

/// An action of the menu, below the lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub action: IconAction,
    pub label: String,
    pub enabled: bool,
}

/// The icon and its menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub look: IconLook,
    /// What blocks an agent now, never capped (0 while not connected).
    pub count: u32,
    /// The icon's accessible name: "Otaq · 2 need you".
    pub name: String,
    pub lines: Vec<Line>,
    pub actions: Vec<Action>,
}

/// The summary from the service's lists. `connection` is the link's look ([`IconLink::connection_look`]: plain, or
/// not connected past the grace period); `list` and `pause` are the last fetched (`None` before the first answer).
pub fn summary(
    connection: IconLook,
    list: Option<&NeedsYouList>,
    pause: Option<&PauseCounts>,
) -> Summary {
    if connection == IconLook::NotConnected {
        return Summary {
            look: IconLook::NotConnected,
            count: 0,
            name: icon_name(IconLook::NotConnected, 0),
            lines: vec![Line::Note(lost_connection())],
            actions: vec![
                action(IconAction::OpenOtaq, open_label(), true),
                action(IconAction::PauseAll, PAUSE_ALL.to_owned(), false),
                action(IconAction::Quit, quit_label(), true),
            ],
        };
    }
    let empty = NeedsYouList::default();
    let list = list.unwrap_or(&empty);
    let no_counts = PauseCounts::default();
    let pause = pause.unwrap_or(&no_counts);
    let paused = |project: &str| {
        pause
            .projects
            .iter()
            .any(|p| p.project == project && p.paused_since.is_some())
    };
    let all_paused =
        !pause.projects.is_empty() && pause.projects.iter().all(|p| p.paused_since.is_some());

    let mut lists = Lists::default();
    for item in needs_you_strip::items(list) {
        match &item {
            Item::Agent(a) => {
                let chip = needs_you_strip::agent_chip(a, item.target());
                let text = match a.status {
                    Status::AsksYou => format!("Noyan \u{b7} {} asks", a.name),
                    _ => format!("Noyan \u{b7} {} \u{b7} {}", a.name, chip.reason),
                };
                lists.needs_you.push(Line::Item {
                    text,
                    status: None,
                    target: chip.target,
                });
            }
            Item::Ticket(t) => {
                if !lists.first_time(t.key.to_string()) {
                    continue;
                }
                let line = Line::Item {
                    text: format!("{} \u{b7} {}", t.key, ticket_reason(t, item.target())),
                    status: Some(t.status),
                    target: item.target(),
                };
                if t.group == NeedsYouGroup::Family {
                    lists.needs_you.push(line);
                } else if t.status == Status::Interrupted && paused(&t.project) {
                    // It waits for its project's Resume, as the strip leaves it out (PAU-03).
                } else {
                    lists.checks.push(line);
                }
            }
        }
    }
    for r in &list.running {
        if lists.first_time(r.key.to_string()) {
            lists.running.push(running_line(r));
        }
    }

    let count = u32::try_from(lists.needs_you.len()).unwrap_or(u32::MAX);
    let look = match (all_paused, count > 0) {
        (true, true) => IconLook::AllPausedNeedsYou,
        (true, false) => IconLook::AllPaused,
        (false, true) => IconLook::NeedsYou,
        (false, false) => IconLook::Plain,
    };
    let name = icon_name(look, count);

    let mut lines = lists.capped(count);
    let own: Vec<Line> = pause
        .paused_on_their_own()
        .map(|p| Line::Note(format!("{} \u{b7} paused on its own", p.name)))
        .collect();
    if !own.is_empty() {
        lines.push(Line::Heading(PAUSED.to_owned()));
        lines.extend(own);
    }
    if lines.is_empty() {
        lines.push(Line::Note(
            if all_paused {
                ALL_PAUSED_NOTHING_RUNNING
            } else {
                NOTHING
            }
            .to_owned(),
        ));
    }

    let mut actions = vec![action(IconAction::OpenOtaq, open_label(), true)];
    if pause.projects.iter().any(|p| p.paused_since.is_none()) {
        actions.push(action(IconAction::PauseAll, PAUSE_ALL.to_owned(), true));
    }
    if pause.all_pause_exists() {
        actions.push(action(IconAction::ResumeAll, RESUME_ALL.to_owned(), true));
    }
    actions.push(action(
        IconAction::StopAgentsAndQuit,
        STOP_AND_QUIT.to_owned(),
        true,
    ));
    Summary {
        look,
        count,
        name,
        lines,
        actions,
    }
}

fn action(action: IconAction, label: String, enabled: bool) -> Action {
    Action {
        action,
        label,
        enabled,
    }
}

/// The three lists before the cap, and the tickets already listed.
#[derive(Default)]
struct Lists {
    needs_you: Vec<Line>,
    checks: Vec<Line>,
    running: Vec<Line>,
    seen: std::collections::HashSet<String>,
}

impl Lists {
    /// Whether the ticket `key` isn't listed yet (a ticket shows once, in the first list it belongs to).
    fn first_time(&mut self, key: String) -> bool {
        self.seen.insert(key)
    }

    /// The lists with their headings, at most [`CAP`] item lines in this order, then "and N more · Open Otaq". A
    /// heading shows while any of its list's lines does, with its full count.
    fn capped(self, count: u32) -> Vec<Line> {
        let total = self.needs_you.len() + self.checks.len() + self.running.len();
        let mut left = CAP;
        let mut lines = Vec::new();
        let running = self.running.len();
        for (heading, items) in [
            (Some(format!("{NEEDS_YOU} \u{b7} {count}")), self.needs_you),
            (Some(CHECKS_AND_STOPPED.to_owned()), self.checks),
            (Some(format!("{RUNNING} \u{b7} {running}")), self.running),
        ] {
            if items.is_empty() || left == 0 {
                continue;
            }
            lines.extend(heading.map(Line::Heading));
            let shown = items.len().min(left);
            left -= shown;
            lines.extend(items.into_iter().take(shown));
        }
        let hidden = total - (CAP - left);
        if hidden > 0 {
            lines.push(Line::More(format!(
                "and {hidden} more \u{b7} {}",
                open_label()
            )));
        }
        lines
    }
}

/// A needs-you or stopped ticket's words after its key: the strip's ("allow a command", "a question", "your
/// check"), and for a stopped ticket its state ("Interrupted", "Start failed").
fn ticket_reason(t: &NeedsYouItem, target: Target) -> String {
    match t.status {
        Status::Interrupted if t.group == NeedsYouGroup::Stopped => {
            return "Interrupted".to_owned();
        }
        Status::StartInterrupted => return "Start failed".to_owned(),
        Status::ResumeFailed => return "Resume failed".to_owned(),
        Status::ParentClosed => return "Parent closed".to_owned(),
        _ => {}
    }
    needs_you_strip::ticket_chip(t, target).map_or_else(|| t.reason.clone(), |c| c.reason)
}

/// "SHOP-18 · Dev · working", "WEB-9 · Validate · checking"; it opens the ticket on its working side.
fn running_line(r: &RunningItem) -> Line {
    let doing = if r.status == Status::Checking {
        "checking"
    } else {
        "working"
    };
    let text = match &r.stage {
        Some(stage) => format!("{} \u{b7} {stage} \u{b7} {doing}", r.key),
        None => format!("{} \u{b7} {doing}", r.key),
    };
    Line::Item {
        text,
        status: Some(r.status),
        target: Target::Ticket {
            project: r.project.clone(),
            key: r.key.to_string(),
            tab: session_tab::lands_on(r.status),
            piece: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected() -> LinkState {
        LinkState::Connected(otaq_core::client::ServiceInfo {
            status: Default::default(),
            compatibility: otaq_core::client::Compatibility::Same,
        })
    }

    #[test]
    fn a_short_drop_keeps_the_icon_plain() {
        let start = Instant::now();
        let mut m = IconLink::default();
        m.on_link(&LinkState::Connecting, start);
        assert_eq!(m.connection_look(start), IconLook::Plain);
        m.on_link(&connected(), start);
        m.on_link(&LinkState::Reconnecting { attempt: 1 }, start);
        assert_eq!(m.changes_at(start), Some(start + NOT_CONNECTED_AFTER));
        // Later attempts don't restart the grace period.
        let soon = start + Duration::from_secs(1);
        m.on_link(&LinkState::Reconnecting { attempt: 4 }, soon);
        assert_eq!(m.connection_look(soon), IconLook::Plain);
        m.on_link(&connected(), soon);
        assert_eq!(
            m.connection_look(start + Duration::from_secs(10)),
            IconLook::Plain
        );
        assert_eq!(m.changes_at(soon), None);
    }

    #[test]
    fn a_link_down_past_the_grace_period_is_not_connected() {
        let start = Instant::now();
        for down in [
            LinkState::Reconnecting { attempt: 2 },
            LinkState::Stopped,
            LinkState::Failed("can't start the service".into()),
        ] {
            let mut m = IconLink::default();
            m.on_link(&connected(), start);
            m.on_link(&down, start);
            let past = start + NOT_CONNECTED_AFTER;
            assert_eq!(m.connection_look(past), IconLook::NotConnected, "{down:?}");
            assert_eq!(m.changes_at(past), None);
            m.on_link(&connected(), past);
            assert_eq!(
                m.connection_look(past),
                IconLook::Plain,
                "back when it returns"
            );
        }
    }

    #[test]
    fn the_menus_and_names() {
        let plain = StatusIconView::of(IconLook::Plain);
        assert_eq!(plain.name, PRODUCT_NAME);
        assert_eq!(
            plain.menu,
            [
                MenuRow::Action {
                    action: IconAction::OpenOtaq,
                    label: format!("Open {PRODUCT_NAME}")
                },
                MenuRow::Separator,
                MenuRow::Action {
                    action: IconAction::Quit,
                    label: format!("Quit {PRODUCT_NAME}")
                },
            ]
        );
        let lost = StatusIconView::of(IconLook::NotConnected);
        assert_eq!(
            lost.name,
            format!("{PRODUCT_NAME} · not connected to its background service")
        );
        assert_eq!(
            lost.menu[0],
            MenuRow::Line(format!("{PRODUCT_NAME} lost its connection"))
        );
        assert_eq!(lost.menu[1], plain.menu[0]);
        for action in IconAction::ALL {
            assert_eq!(IconAction::from_id(action.id()), Some(action));
        }
        assert_eq!(IconAction::from_id("nope"), None);
    }
}

#[cfg(test)]
mod summary_tests {
    use otaq_core::agents::pause::{PauseKind, ProjectPause};
    use otaq_core::attention::needs_you::NeedsYouCounts;
    use otaq_core::board::needs_you::WaitingIn;
    use otaq_core::project_agents::AgentNeedsYou;

    use super::*;
    use crate::view_models::needs_you_strip::samples::ticket;
    use crate::view_models::sheet::Content;

    fn running(key: &str, stage: &str, status: Status) -> RunningItem {
        let project = key.split('-').next().unwrap_or_default().to_owned();
        RunningItem {
            project_name: project.to_lowercase(),
            project,
            id: key.into(),
            key: key.parse().unwrap(),
            title: key.into(),
            status,
            stage: Some(stage.into()),
        }
    }

    fn noyan(name: &str, status: Status) -> AgentNeedsYou {
        AgentNeedsYou {
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            agent: "pa1".into(),
            name: name.into(),
            status,
            reason: "asks: which pages are slow?".into(),
            question: None,
            since: 0,
        }
    }

    fn review(key: &str) -> NeedsYouItem {
        let mut t = ticket(key, "Landing", Status::WaitingForYou, "your check");
        t.waiting_in = Some(WaitingIn {
            stage: "review".into(),
            name: "Review".into(),
            rank: 10,
            delivers: None,
        });
        t
    }

    fn list(
        items: Vec<NeedsYouItem>,
        agents: Vec<AgentNeedsYou>,
        running: Vec<RunningItem>,
    ) -> NeedsYouList {
        NeedsYouList {
            counts: NeedsYouCounts::default(),
            items,
            project_agents: agents,
            running,
        }
    }

    /// Each project by key with how it is paused: `None` runs.
    fn pause(projects: &[(&str, &str, Option<PauseKind>)]) -> PauseCounts {
        PauseCounts {
            working: 0,
            paused: 0,
            projects: projects
                .iter()
                .map(|(key, name, kind)| ProjectPause {
                    project: (*key).into(),
                    name: (*name).into(),
                    paused_since: kind.map(|_| 1),
                    paused_by: *kind,
                    working: 0,
                    paused: 0,
                })
                .collect(),
        }
    }

    fn running_projects() -> PauseCounts {
        pause(&[("SHOP", "shop-web", None), ("WEB", "web-app", None)])
    }

    fn texts(s: &Summary) -> Vec<String> {
        s.lines
            .iter()
            .map(|l| match l {
                Line::Heading(t) => format!("# {t}"),
                Line::Item { text, .. } => text.clone(),
                Line::Note(t) => format!("({t})"),
                Line::More(t) => format!("> {t}"),
            })
            .collect()
    }

    /// `@` stands for the product's name (d0028).
    fn o(text: &str) -> String {
        text.replace('@', PRODUCT_NAME)
    }

    fn actions(s: &Summary) -> Vec<(String, bool)> {
        s.actions
            .iter()
            .map(|a| (a.label.clone(), a.enabled))
            .collect()
    }

    /// The story (M49): SHOP-22 asks to run a command, WEB-7 waits for your check, SHOP-18 and WEB-9 work.
    fn story() -> NeedsYouList {
        list(
            vec![
                ticket(
                    "SHOP-22",
                    "Fix flaky login redirect",
                    Status::NeedsYou,
                    "permission: …",
                ),
                review("WEB-7"),
            ],
            Vec::new(),
            vec![
                running("SHOP-18", "Dev", Status::Working),
                running("WEB-9", "Validate", Status::Checking),
            ],
        )
    }

    #[test]
    fn the_story_counts_one_and_lists_in_order() {
        let s = summary(IconLook::Plain, Some(&story()), Some(&running_projects()));
        assert_eq!((s.look, s.count), (IconLook::NeedsYou, 1));
        assert_eq!(s.name, o("@ \u{b7} 1 needs you"));
        assert_eq!(
            texts(&s),
            [
                "# Needs you \u{b7} 1",
                "SHOP-22 \u{b7} allow a command",
                "# Your checks and stopped tickets",
                "WEB-7 \u{b7} your check",
                "# Running \u{b7} 2",
                "SHOP-18 \u{b7} Dev \u{b7} working",
                "WEB-9 \u{b7} Validate \u{b7} checking",
            ]
        );
        assert_eq!(
            actions(&s),
            [
                (o("Open @"), true),
                ("Pause all projects".to_owned(), true),
                ("Stop agents and quit".to_owned(), true)
            ]
        );
        // Picking SHOP-22 opens its terminal; WEB-7 its review (d0139 G5).
        let targets: Vec<&Target> = s
            .lines
            .iter()
            .filter_map(|l| match l {
                Line::Item { target, .. } => Some(target),
                _ => None,
            })
            .collect();
        assert!(
            matches!(targets[0], Target::Ticket { key, tab: Content::Agent, .. } if key == "SHOP-22")
        );
        assert!(
            matches!(targets[1], Target::Ticket { key, tab: Content::Files, .. } if key == "WEB-7")
        );
        assert!(
            matches!(targets[3], Target::Ticket { key, tab: Content::Check, .. } if key == "WEB-9")
        );
    }

    /// d0139 G4: Interrupted and Review never raise the count; a Noyan conversation waiting for an answer does; a
    /// ticket shows once, in the first list it belongs to.
    #[test]
    fn only_what_blocks_an_agent_counts() {
        let quiet = list(
            vec![
                ticket("SHOP-30", "Search", Status::Interrupted, "session ended"),
                ticket("SHOP-31", "Cart", Status::StartInterrupted, "start failed"),
                review("WEB-7"),
            ],
            Vec::new(),
            vec![running("WEB-7", "Review", Status::Working)],
        );
        let s = summary(IconLook::Plain, Some(&quiet), Some(&running_projects()));
        assert_eq!(
            (s.look, s.count, s.name.clone()),
            (IconLook::Plain, 0, o("@"))
        );
        assert_eq!(
            texts(&s),
            [
                "# Your checks and stopped tickets",
                "SHOP-30 \u{b7} Interrupted",
                "SHOP-31 \u{b7} Start failed",
                "WEB-7 \u{b7} your check",
            ]
        );

        let asks = list(
            Vec::new(),
            vec![noyan("Checkout slowness", Status::AsksYou)],
            Vec::new(),
        );
        let s = summary(IconLook::Plain, Some(&asks), Some(&running_projects()));
        assert_eq!((s.look, s.count), (IconLook::NeedsYou, 1));
        assert_eq!(texts(&s)[1], "Noyan \u{b7} Checkout slowness asks");
        assert!(
            matches!(&s.lines[1], Line::Item { target: Target::Agent { agent, .. }, status: None, .. } if agent == "pa1")
        );
    }

    /// d0139 G6, audit AS q1 (page 31 "Some projects paused"): WEB paused on its own, SHOP working → Pause all, a
    /// Paused heading with web-app, no Resume all; after Pause all → Resume all; every project paused one by one →
    /// the all-paused look, no Resume all.
    #[test]
    fn pause_all_and_resume_all_follow_how_projects_were_paused() {
        let nothing = NeedsYouList::default();
        let some = pause(&[
            ("SHOP", "shop-web", None),
            ("WEB", "web-app", Some(PauseKind::Own)),
        ]);
        let s = summary(IconLook::Plain, Some(&nothing), Some(&some));
        assert_eq!(s.look, IconLook::Plain);
        assert_eq!(
            texts(&s),
            ["# Paused", "(web-app \u{b7} paused on its own)"]
        );
        assert_eq!(
            actions(&s),
            [
                (o("Open @"), true),
                ("Pause all projects".to_owned(), true),
                ("Stop agents and quit".to_owned(), true)
            ]
        );

        let after = pause(&[
            ("SHOP", "shop-web", Some(PauseKind::All)),
            ("WEB", "web-app", Some(PauseKind::Own)),
        ]);
        let s = summary(IconLook::Plain, Some(&nothing), Some(&after));
        assert_eq!(
            (s.look, s.name.clone()),
            (IconLook::AllPaused, o("@ \u{b7} all projects paused"))
        );
        assert_eq!(
            actions(&s),
            [
                (o("Open @"), true),
                ("Resume all projects".to_owned(), true),
                ("Stop agents and quit".to_owned(), true)
            ]
        );

        let one_by_one = pause(&[
            ("SHOP", "shop-web", Some(PauseKind::Own)),
            ("WEB", "web-app", Some(PauseKind::Own)),
        ]);
        let s = summary(IconLook::Plain, Some(&nothing), Some(&one_by_one));
        assert_eq!(s.look, IconLook::AllPaused);
        assert!(!s.actions.iter().any(|a| a.action == IconAction::ResumeAll));
        assert!(!s.actions.iter().any(|a| a.action == IconAction::PauseAll));

        // Every project paused by All, nothing listed: page 31's one line.
        let all = pause(&[
            ("SHOP", "shop-web", Some(PauseKind::All)),
            ("WEB", "web-app", Some(PauseKind::All)),
        ]);
        let s = summary(IconLook::Plain, Some(&nothing), Some(&all));
        assert_eq!(texts(&s), ["(All projects paused \u{b7} nothing running)"]);
        // A paused ticket still asks (it kept its question): counted, with the all-paused look; a paused project's
        // plain Interrupted tickets wait for its Resume and aren't listed.
        let mut kept = ticket("SHOP-12", "Dark mode", Status::Interrupted, "a question");
        kept.group = NeedsYouGroup::Family;
        kept.interrupted_from = Some(Status::AsksYou);
        let paused_list = list(
            vec![
                kept,
                ticket("SHOP-33", "Shipping", Status::Interrupted, "paused"),
            ],
            Vec::new(),
            Vec::new(),
        );
        let s = summary(IconLook::Plain, Some(&paused_list), Some(&all));
        assert_eq!((s.look, s.count), (IconLook::AllPausedNeedsYou, 1));
        assert_eq!(s.name, o("@ \u{b7} all projects paused \u{b7} 1 needs you"));
        assert_eq!(
            texts(&s),
            ["# Needs you \u{b7} 1", "SHOP-12 \u{b7} a question"]
        );

        // No projects: plain, and nothing to pause.
        let s = summary(
            IconLook::Plain,
            Some(&nothing),
            Some(&PauseCounts::default()),
        );
        assert_eq!(s.look, IconLook::Plain);
        assert_eq!(texts(&s), ["(Nothing needs you \u{b7} nothing running)"]);
        assert_eq!(
            actions(&s),
            [
                (o("Open @"), true),
                ("Stop agents and quit".to_owned(), true)
            ]
        );
    }

    /// R6 (page 31 "A long list (capped)"): 40 items → 10 ticket lines, needs-you first, "and 30 more · Open Otaq";
    /// the count and the headings stay true.
    #[test]
    fn a_long_list_is_capped_and_the_count_is_not() {
        let asks: Vec<NeedsYouItem> = (1..=4)
            .map(|n| ticket(&format!("API-{n}"), "x", Status::AsksYou, "asks"))
            .collect();
        let runs: Vec<RunningItem> = (1..=36)
            .map(|n| running(&format!("SHOP-{n}"), "Dev", Status::Working))
            .collect();
        let s = summary(
            IconLook::Plain,
            Some(&list(asks, Vec::new(), runs)),
            Some(&running_projects()),
        );
        assert_eq!(s.count, 4);
        let t = texts(&s);
        assert_eq!(t[0], "# Needs you \u{b7} 4");
        assert_eq!(t[5], "# Running \u{b7} 36");
        let items = s
            .lines
            .iter()
            .filter(|l| matches!(l, Line::Item { .. }))
            .count();
        assert_eq!(items, CAP);
        assert_eq!(*t.last().unwrap(), o("> and 30 more \u{b7} Open @"));

        // Needs-you lines are cut last: 12 of them fill the cap, the rest is "more", the count stays 12.
        let many: Vec<NeedsYouItem> = (1..=12)
            .map(|n| ticket(&format!("API-{n}"), "x", Status::NeedsYou, "permission"))
            .collect();
        let s = summary(
            IconLook::Plain,
            Some(&list(
                many,
                Vec::new(),
                vec![running("SHOP-1", "Dev", Status::Working)],
            )),
            Some(&running_projects()),
        );
        assert_eq!((s.count, s.name.clone()), (12, o("@ \u{b7} 12 need you")));
        let t = texts(&s);
        assert_eq!(t.len(), 1 + CAP + 1);
        assert!(!t.iter().any(|l| l.starts_with("# Running")));
        assert_eq!(*t.last().unwrap(), o("> and 3 more \u{b7} Open @"));
    }

    /// d0139 G7, audit AS q2: not connected wins; one disabled line, Pause all disabled, a plain Quit Otaq.
    #[test]
    fn not_connected_wins_and_quits_plainly() {
        let s = summary(
            IconLook::NotConnected,
            Some(&story()),
            Some(&running_projects()),
        );
        assert_eq!((s.look, s.count), (IconLook::NotConnected, 0));
        assert_eq!(
            s.name,
            o("@ \u{b7} not connected to its background service")
        );
        assert_eq!(texts(&s), [o("(@ lost its connection)")]);
        assert_eq!(
            actions(&s),
            [
                (o("Open @"), true),
                ("Pause all projects".to_owned(), false),
                (o("Quit @"), true)
            ]
        );
        // Before the first answer: plain, nothing listed.
        let s = summary(IconLook::Plain, None, None);
        assert_eq!(s.look, IconLook::Plain);
    }
}

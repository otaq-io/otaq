//! The command palette (KEY-06, d0097; *02 · Command palette*, `s02-19` … `s02-23`): one field and one list ranked by
//! how well each row matches, whatever it is. An action shows its icon and the user's keys, a ticket its key in mono
//! and its column, a project "Switch to …" with its ⌘ number. An action that can't run now stays in the list, greyed,
//! with the reason, and can't be picked. The current project's tickets come before the other projects' (below "Other
//! projects"); `>` first lists actions only; an empty field lists the recent items of every project, newest first.

use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::store::recents::{RecentItem, RecentUse};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::keymap::{Binding, KeyAction, KeyPlace};
use crate::view_models::flow_columns::Columns;
use crate::view_models::pause;
use crate::view_models::sheet::{self, SheetTab};
use crate::view_models::sheet_actions::{self, Action as SheetAction};
use crate::view_models::{close, send_back};

pub const PLACEHOLDER: &str = "Search tickets, projects and actions…";
/// The field's hint: `>` and what it does.
pub const ACTIONS_KEY: &str = ">";
pub const ACTIONS_HINT: &str = "actions only";
/// The hint in `>` mode.
pub const ACTIONS_ONLY: &str = "Actions only";
/// Section labels are stored in sentence case and uppercased by styling (d0093).
pub const RECENT: &str = "Recent";
pub const OTHER_PROJECTS: &str = "Other projects";
pub const NOTHING_HINT: &str = "Search a ticket by its key (SHOP-18) or title, a project by its name, or type > to list every action.";

/// Why an action can't run now (the 0093 glossary's words). Only "no ticket in review" is drawn (`s02-22`).
pub const NO_TICKET_IN_REVIEW: &str = "no ticket in review";
pub const NO_TICKET_OPEN: &str = "no ticket open";
pub const ALREADY_FINISHED: &str = "already finished";
pub const AN_EPIC: &str = "an epic closes with Close epic…";
pub const NO_VALIDATION_TO_RERUN: &str = "no validation to re-run";
pub const NO_DIFF: &str = "no diff yet";
pub const NOT_IN_AN_EPIC: &str = "not in an epic";
pub const NO_TERMINAL: &str = "no terminal focused";

/// The rows a palette lists at most while typing.
const MOST: usize = 50;

/// What the palette knows about the ticket open in the sheet: enough to say which actions run now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenTicket {
    /// Approve ▾ and Open PR take it (Ready for Review).
    pub in_review: bool,
    /// Send back takes it.
    pub send_back: bool,
    /// Close ticket: `None` when it can be closed, else why not.
    pub close: Option<&'static str>,
    /// The sheet shows Retry validation (a validation that ended without a result).
    pub retry_validation: bool,
    /// The sheet has a Files tab.
    pub diff: bool,
    /// An epic or one of its children.
    pub in_epic: bool,
}

impl OpenTicket {
    pub fn of(ticket: &Ticket) -> Self {
        let finished = matches!(ticket.status, Status::Merged | Status::ClosedNotMerged);
        Self {
            in_review: ticket.status == Status::WaitingForYou,
            send_back: send_back::available(ticket),
            close: if close::can_close(ticket) {
                None
            } else if finished {
                Some(ALREADY_FINISHED)
            } else {
                Some(AN_EPIC)
            },
            retry_validation: sheet_actions::buttons(ticket)
                .iter()
                .any(|(a, _)| *a == SheetAction::RetryValidation),
            diff: sheet::tabs(ticket).contains(&SheetTab::Files),
            in_epic: ticket.is_epic() || ticket.epic.is_some(),
        }
    }
}

/// Where the palette was opened from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Context {
    /// The ticket open in the sheet, if any.
    pub open: Option<OpenTicket>,
    /// A terminal had the keyboard (Find in terminal scrollback runs there).
    pub terminal: bool,
    /// The project Files view is shown (Go to file searches it, FIL-11).
    pub files_view: bool,
}

/// Why `action` can't run in `context`; `None` when it can.
pub fn unavailable(action: KeyAction, context: &Context) -> Option<&'static str> {
    use KeyAction as A;
    let open = context.open.as_ref();
    let needs_open = || open.is_none().then_some(NO_TICKET_OPEN);
    match action {
        A::Approve | A::OpenPr => {
            (!open.is_some_and(|o| o.in_review)).then_some(NO_TICKET_IN_REVIEW)
        }
        A::SendBack => (!open.is_some_and(|o| o.send_back)).then_some(NO_TICKET_IN_REVIEW),
        A::CloseTicket => open.map_or(Some(NO_TICKET_OPEN), |o| o.close),
        A::RerunValidation => needs_open().or_else(|| {
            (!open.is_some_and(|o| o.retry_validation)).then_some(NO_VALIDATION_TO_RERUN)
        }),
        A::OpenDiff => needs_open().or_else(|| (!open.is_some_and(|o| o.diff)).then_some(NO_DIFF)),
        // The open ticket's Files tab, else the Files view's search (FIL-11).
        A::GoToFile if context.files_view && open.is_none() => None,
        A::GoToFile => needs_open().or_else(|| (!open.is_some_and(|o| o.diff)).then_some(NO_DIFF)),
        A::FocusEpic => {
            needs_open().or_else(|| (!open.is_some_and(|o| o.in_epic)).then_some(NOT_IN_AN_EPIC))
        }
        A::FindInTerminal => (!context.terminal).then_some(NO_TERMINAL),
        A::StartDev
        | A::Park
        | A::NextTicket
        | A::PreviousTicket
        | A::CloseSheet
        | A::ExpandSheet
        | A::NextTab
        | A::PreviousTab
        | A::SwitchTab(_) => needs_open(),
        A::CommandPalette
        | A::SwitchProject
        | A::Project(_)
        | A::NextNeedsYouItem
        | A::FilterBoard
        | A::ShowShortcuts
        | A::ShowBoard
        | A::ShowList
        | A::Pause
        | A::OpenActivity
        | A::Settings
        | A::FocusAgentPanel
        | A::NewProjectAgent
        | A::NewTicket
        | A::OpenSheet
        | A::Terminals
        | A::ShowFiles => None,
    }
}

/// Whether the palette lists `action`. Not the palette itself; not Project 1–9 (each project is a row of its own,
/// with its key); not the numbered ticket tabs; not Open the ticket (↵ on a card: a ticket row opens its sheet); not
/// Pause / Resume agents (its scopes are rows of their own, [`Item::Pause`]).
pub fn listed(action: KeyAction) -> bool {
    !matches!(
        action,
        KeyAction::CommandPalette
            | KeyAction::Project(_)
            | KeyAction::SwitchTab(_)
            | KeyAction::OpenSheet
            | KeyAction::Pause
    )
}

/// Whether running `action` goes into the recent items: what the palette lists, except moving between tickets and
/// tabs and closing the sheet, which would push everything else out.
pub fn recorded(action: KeyAction) -> bool {
    listed(action)
        && !matches!(
            action,
            KeyAction::NextTicket
                | KeyAction::PreviousTicket
                | KeyAction::NextTab
                | KeyAction::PreviousTab
                | KeyAction::CloseSheet
        )
}

/// The action's name in the palette: [`KeyAction::title`], with each half of a pair on its own (Q223), and
/// "Terminals" as drawn.
pub fn title(action: KeyAction) -> &'static str {
    match action {
        KeyAction::NextTicket => "Next ticket",
        KeyAction::PreviousTicket => "Previous ticket",
        KeyAction::OpenSheet => "Open the ticket",
        KeyAction::CloseSheet => "Close the ticket",
        KeyAction::NextTab => "Next panel tab",
        KeyAction::PreviousTab => "Previous panel tab",
        // As `s02-20` draws it.
        KeyAction::Terminals => "Terminals",
        other => other.title(),
    }
}

/// The action's icon at the row's start (`s02-20`; the ones not drawn there are the closest of the bundled set).
pub fn icon(action: KeyAction) -> IconName {
    use IconName as I;
    use KeyAction as A;
    match action {
        A::CommandPalette | A::FilterBoard => I::Search,
        A::SwitchProject | A::Project(_) => I::FolderGit2,
        A::NextNeedsYouItem => I::Hand,
        A::NextTicket => I::ArrowDown,
        A::PreviousTicket => I::ArrowUp,
        A::FocusEpic => I::Layers,
        A::ShowShortcuts => I::Keyboard,
        A::ShowBoard => I::Columns3,
        A::ShowList => I::List,
        A::Pause => I::Pause,
        A::OpenActivity => I::Activity,
        A::Settings => I::Settings,
        A::FocusAgentPanel => I::Bot,
        A::NewProjectAgent => I::SquarePlus,
        A::NewTicket => I::Plus,
        A::OpenSheet | A::ExpandSheet => I::Maximize2,
        A::CloseSheet => I::X,
        A::SwitchTab(_) | A::NextTab => I::ChevronRight,
        A::PreviousTab => I::ChevronLeft,
        A::Approve => I::Check,
        A::SendBack => I::CornerUpLeft,
        A::StartDev => I::Play,
        A::Park => I::CirclePause,
        A::CloseTicket => I::CircleX,
        A::OpenPr => I::GitPullRequest,
        A::RerunValidation => I::RotateCcw,
        A::OpenDiff => I::FileDiff,
        A::Terminals => I::SquareTerminal,
        A::FindInTerminal => I::ScanSearch,
        A::ShowFiles => I::FolderOpen,
        A::GoToFile => I::File,
    }
}

/// One row's content. `PaletteItem` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "PaletteItem")]
pub enum Item {
    /// "Approve ▾", its keys (⌘↵; "⌘N" "c" for New ticket, drawn with "or" between), and why it can't run now
    /// ("· no ticket in review"), greyed with its keys dimmed.
    Action {
        action: KeyAction,
        icon: IconName,
        title: &'static str,
        keys: Vec<String>,
        reason: Option<String>,
    },
    /// "SHOP-22  Fix flaky login redirect … Dev · Needs you"; another project's names it ("· billing-api").
    Ticket {
        key: String,
        title: String,
        /// The ticket's project key: picking it switches there first when it isn't the current one.
        project: String,
        other_project: Option<String>,
        place: String,
    },
    /// "Switch to billing-api" with its ⌘ number.
    Project {
        project: String,
        title: String,
        keys: Vec<String>,
    },
    /// "Pause this project", "Resume all projects" (PAU-08, d0112): runs at once, no confirmation. `project` is the
    /// open project's key for This project.
    Pause {
        scope: pause::Scope,
        resume: bool,
        project: Option<String>,
        icon: IconName,
        title: &'static str,
    },
    /// "Other projects": the tickets after it are other projects'.
    Divider { label: &'static str },
}

/// A row: its content, and whether ↵ and the highlight take it (not a greyed action, not the divider).
/// `PaletteRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PaletteRow")]
pub struct Row {
    pub item: Item,
    pub pickable: bool,
}

/// "Nothing matches “zzq”" and where else to look. `PaletteNothing` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PaletteNothing")]
pub struct Nothing {
    pub icon: IconName,
    pub title: String,
    pub hint: &'static str,
}

/// The footer (`s02-20`): the keys inside the palette, and both ways to open it. `PaletteFooter` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PaletteFooter")]
pub struct Footer {
    pub move_keys: [&'static str; 2],
    pub move_label: &'static str,
    pub run_key: &'static str,
    /// "run", and "open" while a ticket is highlighted.
    pub run: &'static str,
    pub open: &'static str,
    pub close_key: &'static str,
    pub close: &'static str,
    /// The user's keys of the palette: "⌘K · ⇧⌘P".
    pub opens: String,
}

/// The palette for one query. `PaletteView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PaletteView")]
pub struct View {
    pub placeholder: &'static str,
    pub hint_key: &'static str,
    pub hint: &'static str,
    /// "Actions only" while the query starts with `>` (then the field shows the `>` and the hint goes).
    pub actions_only: Option<&'static str>,
    /// "Recent" over the recent items of an empty field.
    pub section: Option<&'static str>,
    pub rows: Vec<Row>,
    pub nothing: Option<Nothing>,
    pub footer: Footer,
}

/// What the palette lists from.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    /// The user's keys on this platform ([`crate::keymap::user_keys`]).
    pub bindings: &'a [Binding],
    /// The single-letter switch: off, letters and sequences aren't shown (they don't run).
    pub letters: bool,
    /// The project on screen.
    pub project: Option<&'a str>,
    /// Every project, in the rail's order (their ⌘ numbers).
    pub projects: &'a [ProjectInfo],
    /// Every project's tickets.
    pub tickets: &'a [Ticket],
    pub recent: &'a [RecentItem],
    pub context: Context,
}

/// The user's keys of `action` as Kbd texts, outside terminals, in slot order.
pub fn keys_of(action: KeyAction, inputs: &Inputs) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for b in inputs.bindings.iter().filter(|b| {
        b.action == action
            && b.place != KeyPlace::InsideTerminals
            && (inputs.letters || !b.key.is_letter())
    }) {
        if !keys.contains(&b.text) {
            keys.push(b.text.clone());
        }
    }
    keys
}

/// The Pause and Resume rows for the open project and for all projects ([`pause::palette`]), with their titles.
fn pause_rows(inputs: &Inputs) -> Vec<(&'static str, Row)> {
    pause::palette(inputs.project, inputs.projects)
        .into_iter()
        .map(|(scope, resume, title)| {
            let row = Row {
                pickable: true,
                item: Item::Pause {
                    scope,
                    resume,
                    project: (scope == pause::Scope::Project)
                        .then(|| inputs.project.map(str::to_owned))
                        .flatten(),
                    icon: if resume {
                        IconName::Play
                    } else {
                        IconName::Pause
                    },
                    title,
                },
            };
            (title, row)
        })
        .collect()
}

fn action_row(action: KeyAction, inputs: &Inputs) -> Row {
    let reason = unavailable(action, &inputs.context);
    Row {
        pickable: reason.is_none(),
        item: Item::Action {
            action,
            icon: icon(action),
            title: title(action),
            keys: keys_of(action, inputs),
            reason: reason.map(|r| format!("\u{b7} {r}")),
        },
    }
}

/// "Review", "Dev · Needs you": the board column it is in, and Needs you when it waits on the user.
pub fn place(ticket: &Ticket) -> String {
    // The default flow's column names; every project's flow has the same columns until M33.
    let columns = Columns::of(&otaq_core::flow::default_file().flow);
    let column = columns.column_of(ticket).title.as_str();
    if ticket.status.is_needs_you_family() {
        format!("{column} \u{b7} Needs you")
    } else {
        column.to_owned()
    }
}

fn ticket_row(ticket: &Ticket, inputs: &Inputs) -> Row {
    let project = ticket.key.project.to_string();
    let other_project = (inputs.project != Some(project.as_str())).then(|| {
        let name = inputs
            .projects
            .iter()
            .find(|p| p.key == project)
            .map_or(project.as_str(), |p| p.name.as_str());
        format!("\u{b7} {name}")
    });
    Row {
        pickable: true,
        item: Item::Ticket {
            key: ticket.key.to_string(),
            title: ticket.title.clone(),
            project,
            other_project,
            place: place(ticket),
        },
    }
}

fn project_row(index: usize, project: &ProjectInfo, inputs: &Inputs) -> Row {
    let keys = u8::try_from(index + 1)
        .ok()
        .filter(|n| *n <= 9)
        .map(|n| keys_of(KeyAction::Project(n), inputs))
        .unwrap_or_default();
    Row {
        pickable: true,
        item: Item::Project {
            project: project.key.clone(),
            title: format!("Switch to {}", project.name),
            keys,
        },
    }
}

/// How well `query` (lowercase, not empty) matches `text`; `None` when it doesn't. The whole text beats its start,
/// the start beats a word's start, that beats anywhere inside, and that beats the letters in order close together.
pub fn score(query: &str, text: &str) -> Option<u32> {
    let text = text.to_lowercase();
    if text == query {
        return Some(1000);
    }
    if text.starts_with(query) {
        return Some(800);
    }
    if let Some(at) = text.find(query) {
        let word_start = text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| !c.is_alphanumeric());
        return Some(if word_start { 600 } else { 400 });
    }
    // The letters in order, within twice the query's length.
    let wanted: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).collect();
    let chars: Vec<char> = text.chars().collect();
    let first = chars.iter().position(|c| Some(c) == wanted.first())?;
    let mut at = first;
    for want in &wanted[1..] {
        at += 1 + chars[at + 1..].iter().position(|c| c == want)?;
    }
    let span = at + 1 - first;
    (span <= wanted.len() * 2).then(|| 300 - u32::try_from(span).unwrap_or(300).min(299))
}

/// A ticket matches by its key or its title; its key wins a tie (`NOTE-2` before a title that mentions it).
fn ticket_score(query: &str, ticket: &Ticket) -> Option<u32> {
    let key = score(query, &ticket.key.to_string()).map(|s| s + 50);
    key.max(score(query, &ticket.title))
}

fn project_score(query: &str, project: &ProjectInfo) -> Option<u32> {
    score(query, &project.name)
        .map(|s| s + 10)
        .max(score(query, &project.key))
        .max(score(query, &format!("Switch to {}", project.name)))
}

/// The palette's actions in the map's order.
fn actions() -> impl Iterator<Item = KeyAction> {
    KeyAction::all().into_iter().filter(|a| listed(*a))
}

/// The recent items that still exist, newest first, as rows: an action the app no longer has, a ticket or a project
/// that is gone, and a switch to the project already shown are left out.
fn recent_rows(inputs: &Inputs) -> Vec<Row> {
    inputs
        .recent
        .iter()
        .filter_map(|item| match &item.used {
            RecentUse::Action { action } => serde_json::from_value::<KeyAction>(action.clone())
                .ok()
                .filter(|a| listed(*a))
                .map(|a| action_row(a, inputs)),
            RecentUse::Ticket { key } => inputs
                .tickets
                .iter()
                .find(|t| t.key.to_string() == *key)
                .map(|t| ticket_row(t, inputs)),
            RecentUse::Project { project } => inputs
                .projects
                .iter()
                .enumerate()
                .find(|(_, p)| p.key == *project && inputs.project != Some(p.key.as_str()))
                .map(|(i, p)| project_row(i, p, inputs)),
        })
        .collect()
}

/// The palette for `query`.
pub fn view(query: &str, inputs: &Inputs) -> View {
    let actions_mode = query.trim_start().strip_prefix(ACTIONS_KEY);
    let needle = actions_mode.unwrap_or(query).trim().to_lowercase();
    let mut section = None;
    let rows: Vec<Row> = if actions_mode.is_some() {
        let mut ranked: Vec<(u32, Row)> = actions()
            .filter_map(|a| {
                let s = if needle.is_empty() {
                    Some(0)
                } else {
                    score(&needle, title(a))
                };
                s.map(|s| (s, action_row(a, inputs)))
            })
            .collect();
        for (title, row) in pause_rows(inputs) {
            let s = if needle.is_empty() {
                Some(0)
            } else {
                score(&needle, title)
            };
            ranked.extend(s.map(|s| (s, row)));
        }
        ranked.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
        ranked.into_iter().map(|(_, r)| r).collect()
    } else if needle.is_empty() {
        let recent = recent_rows(inputs);
        if recent.is_empty() {
            actions()
                .map(|a| action_row(a, inputs))
                .chain(pause_rows(inputs).into_iter().map(|(_, r)| r))
                .collect()
        } else {
            section = Some(RECENT);
            recent
        }
    } else {
        let mut here: Vec<(u32, Row)> = Vec::new();
        let mut other: Vec<(u32, Row)> = Vec::new();
        for t in inputs.tickets {
            if let Some(s) = ticket_score(&needle, t) {
                let row = ticket_row(t, inputs);
                if inputs.project == Some(t.key.project.to_string().as_str()) {
                    here.push((s, row));
                } else {
                    other.push((s, row));
                }
            }
        }
        for a in actions() {
            if let Some(s) = score(&needle, title(a)) {
                here.push((s, action_row(a, inputs)));
            }
        }
        for (title, row) in pause_rows(inputs) {
            if let Some(s) = score(&needle, title) {
                here.push((s, row));
            }
        }
        for (i, p) in inputs.projects.iter().enumerate() {
            if inputs.project == Some(p.key.as_str()) {
                continue;
            }
            if let Some(s) = project_score(&needle, p) {
                here.push((s, project_row(i, p, inputs)));
            }
        }
        // Stable: equal matches keep tickets, then actions, then projects, each in its own order.
        here.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
        other.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
        let mut rows: Vec<Row> = here.into_iter().map(|(_, r)| r).take(MOST).collect();
        if !other.is_empty() {
            rows.push(Row {
                item: Item::Divider {
                    label: OTHER_PROJECTS,
                },
                pickable: false,
            });
            rows.extend(other.into_iter().map(|(_, r)| r).take(MOST));
        }
        rows
    };
    let typed = actions_mode.map_or(query.trim(), str::trim);
    let nothing = (rows.is_empty() && !typed.is_empty()).then(|| Nothing {
        icon: IconName::SearchX,
        title: format!("Nothing matches \u{201c}{typed}\u{201d}"),
        hint: NOTHING_HINT,
    });
    View {
        placeholder: PLACEHOLDER,
        hint_key: ACTIONS_KEY,
        hint: ACTIONS_HINT,
        actions_only: actions_mode.map(|_| ACTIONS_ONLY),
        section,
        rows,
        nothing,
        footer: Footer {
            move_keys: ["↑", "↓"],
            move_label: "move",
            run_key: "↵",
            run: "run",
            open: "open",
            close_key: "Esc",
            close: "close",
            opens: keys_of(KeyAction::CommandPalette, inputs).join(" \u{b7} "),
        },
    }
}

/// One palette state for the gallery (`s02-20` … `s02-23`): the artboard's caption, the field's text and the list.
/// `PaletteGalleryFrame` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PaletteGalleryFrame")]
pub struct GalleryFrame {
    pub caption: &'static str,
    pub query: &'static str,
    pub view: View,
}

/// The artboards' states with their shop-web and billing-api samples: just opened, typing, `>`, no results, a ticket
/// from another project, an action that can't run, and Windows / Linux.
pub fn gallery() -> Vec<GalleryFrame> {
    use crate::keymap::{CustomKeys, Platform, user_keys};
    use crate::view_models::card_fixtures::ticket;

    let project = |key: &str, name: &str| ProjectInfo {
        key: key.into(),
        name: name.into(),
        repo_path: format!("/code/{name}"),
        base_branch: "main".into(),
        agents_working: 0,
        needs_you: 0,
        flow_setup_since: None,
        paused_since: None,
    };
    let projects = [project("SHOP", "shop-web"), project("BILL", "billing-api")];
    let tickets = [
        ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou),
        ticket("SHOP-18", "Checkout API", Status::WaitingForYou),
        ticket("SHOP-31", "Hotfix: checkout crash", Status::Backlog),
        ticket("SHOP-35", "Slow checkout query", Status::Queued),
        ticket("SHOP-20", "Checkout page", Status::Queued),
        ticket("SHOP-49", "Export orders as CSV", Status::Queued),
        ticket(
            "SHOP-40.4",
            "Wishlist share link: approve copy",
            Status::Working,
        ),
        ticket("BILL-4", "Invoice PDF export", Status::Working),
        ticket("BILL-9", "Export usage to S3", Status::Backlog),
    ];
    let action = |a: KeyAction| RecentUse::Action {
        action: serde_json::to_value(a).unwrap_or_default(),
    };
    let recent: Vec<RecentItem> = [
        action(KeyAction::NextNeedsYouItem),
        RecentUse::Ticket {
            key: "SHOP-22".into(),
        },
        action(KeyAction::StartDev),
        RecentUse::Ticket {
            key: "SHOP-18".into(),
        },
        RecentUse::Project {
            project: "BILL".into(),
        },
        action(KeyAction::OpenPr),
        action(KeyAction::Terminals),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, used)| RecentItem {
        used,
        at: 100 - i64::try_from(i).unwrap_or(0),
    })
    .collect();
    let mac = user_keys(Platform::Mac, &CustomKeys::default());
    let win = user_keys(Platform::WinLinux, &CustomKeys::default());
    let review = Context {
        open: Some(OpenTicket::of(&tickets[1])),
        terminal: false,
        files_view: false,
    };
    let frame = |caption, query, bindings: &[Binding], context| GalleryFrame {
        caption,
        query,
        view: view(
            query,
            &Inputs {
                bindings,
                letters: true,
                project: Some("SHOP"),
                projects: &projects,
                tickets: &tickets,
                recent: &recent,
                context,
            },
        ),
    };
    vec![
        frame("Just opened: recent actions and tickets", "", &mac, review),
        frame("Typing: one ranked list", "check", &mac, review),
        frame("> first: actions only", ">", &mac, review),
        frame("No results", "zzq", &mac, review),
        frame("A ticket from another project", "export", &mac, review),
        frame(
            "An action that can’t run now",
            "app",
            &mac,
            Context::default(),
        ),
        frame("Windows and Linux", "", &win, review),
    ]
}

#[cfg(test)]
mod tests {
    use otaq_core::store::recents::{RecentItem, RecentUse};
    use serde_json::json;

    use super::*;
    use crate::keymap::{self, CustomKeys, Platform};
    use crate::view_models::card_fixtures::ticket;

    fn project(key: &str, name: &str) -> ProjectInfo {
        ProjectInfo {
            key: key.into(),
            name: name.into(),
            repo_path: format!("/tmp/m29/{name}"),
            base_branch: "main".into(),
            agents_working: 0,
            needs_you: 0,
            flow_setup_since: None,
            paused_since: None,
        }
    }

    /// The example of `tickets/m29-tickets.md`: pocket-notes (NOTE) and pocket-api (API).
    fn example() -> (Vec<ProjectInfo>, Vec<Ticket>, Vec<Binding>) {
        let projects = vec![
            project("NOTE", "pocket-notes"),
            project("API", "pocket-api"),
        ];
        let tickets = vec![
            ticket("NOTE-1", "Sort notes by date", Status::WaitingForYou),
            ticket("NOTE-2", "Export as CSV", Status::Queued),
            ticket("NOTE-3", "Undo for the NOTE-2 export", Status::Backlog),
            ticket("API-1", "Rate limit the export", Status::Working),
        ];
        (
            projects,
            tickets,
            keymap::user_keys(Platform::Mac, &CustomKeys::default()),
        )
    }

    fn inputs<'a>(
        projects: &'a [ProjectInfo],
        tickets: &'a [Ticket],
        bindings: &'a [Binding],
        recent: &'a [RecentItem],
    ) -> Inputs<'a> {
        Inputs {
            bindings,
            letters: true,
            project: Some("NOTE"),
            projects,
            tickets,
            recent,
            context: Context::default(),
        }
    }

    /// Each row in a word: "action Park", "ticket API-1 · pocket-api", "divider", "project Switch to pocket-api".
    fn words(view: &View) -> Vec<String> {
        view.rows
            .iter()
            .map(|r| match &r.item {
                Item::Action { title, reason, .. } => match reason {
                    Some(reason) => format!("action {title} {reason}"),
                    None => format!("action {title}"),
                },
                Item::Ticket {
                    key, other_project, ..
                } => match other_project {
                    Some(other) => format!("ticket {key} {other}"),
                    None => format!("ticket {key}"),
                },
                Item::Project { title, .. } => format!("project {title}"),
                Item::Pause { title, .. } => format!("pause {title}"),
                Item::Divider { label } => format!("divider {label}"),
            })
            .collect()
    }

    #[test]
    fn each_kind_has_its_row() {
        let (projects, tickets, bindings) = example();
        let i = inputs(&projects, &tickets, &bindings, &[]);
        let v = view("export", &i);
        assert_eq!(
            v.rows[0].item,
            Item::Ticket {
                key: "NOTE-2".into(),
                title: "Export as CSV".into(),
                project: "NOTE".into(),
                other_project: None,
                place: "Ready for Dev".into(),
            }
        );
        let v = view("pocket", &i);
        assert_eq!(
            words(&v),
            ["project Switch to pocket-api"],
            "the project on screen isn't offered"
        );
        assert_eq!(
            v.rows[0].item,
            Item::Project {
                project: "API".into(),
                title: "Switch to pocket-api".into(),
                keys: vec!["⌘2".into()],
            }
        );
        let v = view("new ticket", &i);
        assert_eq!(
            v.rows[0].item,
            Item::Action {
                action: KeyAction::NewTicket,
                icon: IconName::Plus,
                title: "New ticket",
                keys: vec!["⌘N".into(), "c".into()],
                reason: None,
            }
        );
        assert!(v.rows[0].pickable);
        // Letters off: only the modifier key.
        let off = Inputs {
            letters: false,
            ..i
        };
        let Item::Action { keys, .. } = &view("new ticket", &off).rows[0].item else {
            panic!()
        };
        assert_eq!(keys, &["⌘N"]);
        assert_eq!(v.placeholder, "Search tickets, projects and actions…");
        assert_eq!((v.hint_key, v.hint), (">", "actions only"));
        assert_eq!(v.footer.opens, "⌘K \u{b7} ⇧⌘P");
        assert_eq!(
            (v.footer.run, v.footer.open, v.footer.close),
            ("run", "open", "close")
        );
    }

    #[test]
    fn a_ticket_row_says_its_column_and_needs_you() {
        assert_eq!(place(&ticket("API-1", "t", Status::Working)), "Dev");
        assert_eq!(
            place(&ticket("API-1", "t", Status::NeedsYou)),
            "Dev \u{b7} Needs you"
        );
        assert_eq!(
            place(&ticket("API-1", "t", Status::WaitingForYou)),
            "Review"
        );
    }

    #[test]
    fn the_key_ranks_first() {
        let (projects, tickets, bindings) = example();
        let i = inputs(&projects, &tickets, &bindings, &[]);
        let v = view("NOTE-2", &i);
        assert_eq!(words(&v)[..2], ["ticket NOTE-2", "ticket NOTE-3"]);
        assert!(score("note-2", "NOTE-2") > score("note-2", "Undo for the NOTE-2 export"));
        assert!(score("exp", "Export as CSV") > score("exp", "Rate limit the export"));
        assert!(
            score("rte", "Rate limit the export").is_some(),
            "letters in order"
        );
        assert_eq!(score("zzq", "Rate limit the export"), None);
    }

    #[test]
    fn other_projects_tickets_come_below_the_divider() {
        let (projects, tickets, bindings) = example();
        let i = inputs(&projects, &tickets, &bindings, &[]);
        assert_eq!(
            words(&view("export", &i)),
            [
                "ticket NOTE-2",
                "ticket NOTE-3",
                "divider Other projects",
                "ticket API-1 \u{b7} pocket-api"
            ]
        );
        let v = view("export", &i);
        assert!(!v.rows[2].pickable);
        let Item::Ticket { project, .. } = &v.rows[3].item else {
            panic!()
        };
        assert_eq!(project, "API");
    }

    #[test]
    fn greater_than_lists_actions_only_with_the_keyless_ones() {
        let (projects, tickets, bindings) = example();
        let i = inputs(&projects, &tickets, &bindings, &[]);
        let v = view(">", &i);
        assert_eq!(v.actions_only, Some("Actions only"));
        assert!(
            v.rows
                .iter()
                .all(|r| matches!(r.item, Item::Action { .. } | Item::Pause { .. }))
        );
        let all = words(&v);
        for keyless in [
            "Park",
            "Close ticket",
            "Open PR",
            "Re-run validation",
            "Open diff",
        ] {
            assert!(
                all.iter()
                    .any(|w| w.starts_with(&format!("action {keyless}"))),
                "{keyless} in {all:?}"
            );
        }
        for not in [
            "Command palette",
            "Project 1–9",
            "Switch panel tab",
            "Open the ticket",
        ] {
            assert!(
                !all.iter().any(|w| w.starts_with(&format!("action {not}"))),
                "{not}"
            );
        }
        // Each half of a pair on its own (Q223).
        assert!(all.contains(&"action Next ticket \u{b7} no ticket open".to_owned()));
        assert!(all.contains(&"action Previous ticket \u{b7} no ticket open".to_owned()));
        let v = view(">park", &i);
        assert_eq!(words(&v)[0], "action Park \u{b7} no ticket open");
        let Item::Action { keys, .. } = &v.rows[0].item else {
            panic!()
        };
        assert!(keys.is_empty());
        assert_eq!(words(&view("> export", &i)), Vec::<String>::new());
    }

    #[test]
    fn an_action_that_cant_run_is_greyed_with_its_reason() {
        let (projects, tickets, bindings) = example();
        let i = inputs(&projects, &tickets, &bindings, &[]);
        let v = view("app", &i);
        assert_eq!(words(&v)[0], "action Approve ▾ \u{b7} no ticket in review");
        assert!(!v.rows[0].pickable);
        let Item::Action { keys, .. } = &v.rows[0].item else {
            panic!()
        };
        assert_eq!(keys, &["⌘↵"]);
        // NOTE-1 open, in Review: it runs.
        let open = Inputs {
            context: Context {
                open: Some(OpenTicket::of(&tickets[0])),
                terminal: false,
                files_view: false,
            },
            ..i
        };
        let v = view("app", &open);
        assert_eq!(words(&v)[0], "action Approve ▾");
        assert!(v.rows[0].pickable);
        // NOTE-2 open, in Ready for Dev: not in review.
        let ready = Inputs {
            context: Context {
                open: Some(OpenTicket::of(&tickets[1])),
                terminal: false,
                files_view: false,
            },
            ..i
        };
        assert_eq!(
            words(&view("send back", &ready))[0],
            "action Send back \u{b7} no ticket in review"
        );
        assert_eq!(words(&view(">park", &ready))[0], "action Park");
        assert_eq!(words(&view(">open diff", &ready))[0], "action Open diff");
        assert_eq!(
            words(&view(">focus the epic", &ready))[0],
            "action Focus the epic of the selected card \u{b7} not in an epic"
        );
        assert_eq!(
            words(&view(">find in terminal", &ready))[0],
            "action Find in terminal scrollback \u{b7} no terminal focused"
        );
        // FIL-11: Files is listed; Go to file runs in the Files view with no ticket open, else needs one.
        assert_eq!(words(&view(">files", &i))[0], "action Files");
        assert_eq!(
            words(&view(">go to file", &i))[0],
            "action Go to file \u{b7} no ticket open"
        );
        let files = Inputs {
            context: Context {
                files_view: true,
                ..Context::default()
            },
            ..i
        };
        assert_eq!(words(&view(">go to file", &files))[0], "action Go to file");
    }

    #[test]
    fn the_open_tickets_facts() {
        let review = OpenTicket::of(&ticket("NOTE-1", "t", Status::WaitingForYou));
        assert!(review.in_review && review.send_back && review.diff);
        assert_eq!(review.close, None);
        let merged = OpenTicket::of(&ticket("NOTE-1", "t", Status::Merged));
        assert_eq!(merged.close, Some(ALREADY_FINISHED));
        let failed = OpenTicket::of(&ticket("NOTE-1", "t", Status::CheckNoResult));
        assert!(failed.retry_validation, "{failed:?}");
    }

    #[test]
    fn recents_come_from_every_project_newest_first() {
        let (projects, tickets, bindings) = example();
        let at = |used, at| RecentItem { used, at };
        let recent = [
            at(
                RecentUse::Ticket {
                    key: "API-1".into(),
                },
                5,
            ),
            at(
                RecentUse::Action {
                    action: json!({"action": "park"}),
                },
                4,
            ),
            at(
                RecentUse::Ticket {
                    key: "NOTE-2".into(),
                },
                3,
            ),
            at(
                RecentUse::Project {
                    project: "API".into(),
                },
                2,
            ),
            // Gone, or nothing to switch to: left out.
            at(
                RecentUse::Ticket {
                    key: "NOTE-99".into(),
                },
                1,
            ),
            at(
                RecentUse::Action {
                    action: json!({"action": "no_such_action"}),
                },
                1,
            ),
            at(
                RecentUse::Project {
                    project: "NOTE".into(),
                },
                0,
            ),
        ];
        let i = inputs(&projects, &tickets, &bindings, &recent);
        let v = view("", &i);
        assert_eq!(v.section, Some("Recent"));
        assert_eq!(
            words(&v),
            [
                "ticket API-1 \u{b7} pocket-api",
                "action Park \u{b7} no ticket open",
                "ticket NOTE-2",
                "project Switch to pocket-api"
            ]
        );
        // Nothing used yet: every action, without the label.
        let fresh = view("", &inputs(&projects, &tickets, &bindings, &[]));
        assert_eq!(fresh.section, None);
        assert!(fresh.rows.len() > 20);
    }

    #[test]
    fn nothing_matches() {
        let (projects, tickets, bindings) = example();
        let i = inputs(&projects, &tickets, &bindings, &[]);
        let v = view("zzq", &i);
        assert!(v.rows.is_empty());
        assert_eq!(
            v.nothing,
            Some(Nothing {
                icon: IconName::SearchX,
                title: "Nothing matches \u{201c}zzq\u{201d}".into(),
                hint: "Search a ticket by its key (SHOP-18) or title, a project by its name, or type > to list every action.",
            })
        );
        assert_eq!(
            view(">zzq", &i).nothing.unwrap().title,
            "Nothing matches \u{201c}zzq\u{201d}"
        );
        assert_eq!(view("", &i).nothing, None);
    }

    #[test]
    fn what_goes_into_the_recent_items() {
        assert!(recorded(KeyAction::Park));
        assert!(recorded(KeyAction::NextNeedsYouItem));
        for not in [
            KeyAction::CommandPalette,
            KeyAction::Project(2),
            KeyAction::NextTicket,
            KeyAction::CloseSheet,
            KeyAction::SwitchTab(1),
        ] {
            assert!(!recorded(not), "{not:?}");
        }
    }
}

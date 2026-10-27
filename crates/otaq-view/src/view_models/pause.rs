//! **Pause in the app** (PAU-07, PAU-08; d0112, d0097, d0093; *24 → Row · Pause and resume* `s24-07`, *Row · Resume
//! asks for the scope too, and a restart* `s24-08`): the title bar's **Pause** next to Activity, **Resume** while the
//! open project is paused, each opening its scope menu ("Pause agents" / "Resume agents", uppercased by styling): **This
//! project** first (focused, Return) and **All projects**, with their counts. **No confirmation**: picking a scope
//! pauses at once, since nothing is lost. ⇧⌘. opens the same menu (0097's Stop all row, reused).
//!
//! While the open project is paused: the amber banner first in the stack, which can't be closed ("shop-web is
//! paused." / "All projects are paused."), Activity reads "5 paused", the rail's tile counts its paused agents in
//! amber, and every card the pause stopped reads "Paused" in amber, its timer held ([`crate::view_models::card`]).
//! After a restart the banner says it is **still** paused and that a pause never lifts on its own, with no toast
//! (PAU-07, Q417); the reboot summary leaves paused projects out ([`crate::view_models::after_restart::outside_pause`]).
//!
//! The counts are the service's (`agents.pause_counts`): `working` is what Pause stops, `paused` what Resume resumes.
//!
//! **Resume all projects lifts only what an All projects pause set** (d0112 2026-10-27; *31 · item 7*): a project
//! paused on its own stays paused, so the Resume menu's All projects row counts only the others and names it ("7 paused
//! in 2 projects · web-app stays paused", with the line why), and without an All projects pause there is no All
//! projects row to resume.

use otaq_core::agents::pause::{PauseCounts, PauseKind, ProjectPause, StopRecord};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::view_models::agent_tools_page::and_list;
use crate::view_models::banners::Situation;
use crate::view_models::limits::AgentsChip;

pub const PAUSE: &str = "Pause";
pub const RESUME: &str = "Resume";
/// The menus' headings, stored in sentence case and uppercased by styling (d0093).
pub const PAUSE_AGENTS: &str = "Pause agents";
pub const RESUME_AGENTS: &str = "Resume agents";
pub const THIS_PROJECT: &str = "This project";
pub const ALL_PROJECTS: &str = "All projects";
/// The Pause menu's line (audit 2026-10-23 item 1: Pause stops agents at once).
pub const PAUSE_NOTE: &str =
    "Agents stop now; Resume continues where each one was. Nothing is lost.";
/// What a pause leaves, in the banner after "5 agents stopped; " (`s24-07`).
const LEAVES: &str =
    "Resume continues where each one was. Queued tickets wait; questions for you stay open.";
/// The restart's first sentence, in the banner (PAU-07, `s24-08`): "Otaq restarted; a pause never lifts
/// on its own."
pub fn restarted_line() -> String {
    format!(
        "{} restarted; a pause never lifts on its own.",
        otaq_core::naming::PRODUCT_NAME
    )
}

/// Which projects a Pause or Resume covers: the open one, or every project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "PauseScopeKind")]
pub enum Scope {
    /// "This project": the open project.
    Project,
    /// "All projects".
    All,
}

/// The title bar's button: "Pause", or "Resume" (the accent, `s24-08`) while the open project is paused. A click
/// opens the scope menu. `PauseButton` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PauseButton")]
pub struct Button {
    /// "Pause" or "Resume": the button's name; Pause draws its icon only.
    pub label: &'static str,
    pub icon: IconName,
    /// Resume: the accent button with its label and a chevron.
    pub resumes: bool,
    /// The tooltip's name, "Pause / Resume agents", and the user's key for it (⇧⌘.).
    pub tooltip: &'static str,
    pub keys: Option<String>,
}

/// One row of the menu: "This project · shop-web · 5 working ⇧⌘." or "All projects · 7 working in 2 projects".
/// `PauseScopeRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PauseScopeRow")]
pub struct ScopeRow {
    pub scope: Scope,
    pub icon: IconName,
    pub label: &'static str,
    pub detail: String,
    /// The menu's key on the first row (⇧⌘.), as drawn.
    pub keys: Option<String>,
}

/// The scope menu under the button (`s24-07`, `s24-08`): This project first and focused (Return picks it), then All
/// projects, then the line. `PauseMenu` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PauseMenu")]
pub struct Menu {
    /// "Pause agents" or "Resume agents".
    pub title: &'static str,
    /// Picking a row resumes (else pauses).
    pub resumes: bool,
    pub rows: Vec<ScopeRow>,
    /// "Agents stop now; …" (Pause); "Resume this project only and billing-api stays paused, with its own banner."
    /// (Resume, while another project is paused too).
    pub note: Option<String>,
}

/// What the title bar's Pause part draws for the open project. `PauseView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PauseView")]
pub struct View {
    /// The open project's key, which This project pauses; `None` before the first project.
    pub project: Option<String>,
    pub button: Button,
    pub menu: Menu,
}

/// "1 agent", "5 agents".
fn agents(n: u32) -> String {
    if n == 1 {
        "1 agent".to_owned()
    } else {
        format!("{n} agents")
    }
}

/// "in 1 project", "in 2 projects".
fn in_projects(n: usize) -> String {
    if n == 1 {
        "in 1 project".to_owned()
    } else {
        format!("in {n} projects")
    }
}

fn paused(p: &ProjectPause) -> bool {
    p.paused_since.is_some()
}

/// Paused on its own: Resume all projects leaves it paused.
fn on_its_own(p: &ProjectPause) -> bool {
    paused(p) && p.paused_by == Some(PauseKind::Own)
}

/// Paused by an All projects pause: Resume all projects lifts it.
fn by_all(p: &ProjectPause) -> bool {
    paused(p) && !on_its_own(p)
}

/// Page 31 item 7's line under Resume's All projects: "web-app was paused on its own: Resume all leaves it paused,
/// with its own banner and Resume." `stay` names the projects paused on their own.
fn stays_line(stay: &[&str]) -> String {
    match stay {
        [one] => format!(
            "{one} was paused on its own: Resume all leaves it paused, with its own banner and Resume."
        ),
        more => format!(
            "{} were paused on their own: Resume all leaves them paused, each with its own banner and Resume.",
            and_list(more)
        ),
    }
}

/// The open project's counts, if it is one of `counts`.
fn find<'a>(counts: &'a PauseCounts, shown: Option<&str>) -> Option<&'a ProjectPause> {
    let shown = shown?;
    counts.projects.iter().find(|p| p.project == shown)
}

/// The Pause or Resume part of the title bar for the open project `shown`; `counts` are every project's
/// (`agents.pause_counts` with All projects).
pub fn view(shown: Option<&str>, counts: &PauseCounts, keys: &KeyLabels) -> View {
    let project = find(counts, shown);
    let resumes = match project {
        Some(p) => paused(p),
        // No project open: Resume only once every project is paused and an All projects pause has something to lift.
        None => {
            !counts.projects.is_empty()
                && counts.projects.iter().all(paused)
                && counts.projects.iter().any(by_all)
        }
    };
    let key = keys.label(KeyAction::Pause);
    View {
        project: project.map(|p| p.project.clone()),
        button: Button {
            label: if resumes { RESUME } else { PAUSE },
            icon: if resumes {
                IconName::Play
            } else {
                IconName::Pause
            },
            resumes,
            tooltip: KeyAction::Pause.title(),
            keys: key.clone(),
        },
        menu: menu(project, counts, resumes, key),
    }
}

fn menu(
    project: Option<&ProjectPause>,
    counts: &PauseCounts,
    resumes: bool,
    key: Option<String>,
) -> Menu {
    let mut rows = Vec::new();
    if let Some(p) = project {
        let n = if resumes { p.paused } else { p.working };
        rows.push(ScopeRow {
            scope: Scope::Project,
            icon: IconName::Folder,
            label: THIS_PROJECT,
            detail: format!("{} \u{b7} {n} {}", p.name, word(resumes)),
            keys: key,
        });
    }
    // Resume all resumes what an All projects pause stopped (a project paused on its own stays paused, d0112
    // 2026-10-27); Pause all stops what works anywhere.
    let stay: Vec<&str> = counts
        .projects
        .iter()
        .filter(|p| on_its_own(p))
        .map(|p| p.name.as_str())
        .collect();
    let (n, places) = if resumes {
        let of: Vec<&ProjectPause> = counts.projects.iter().filter(|p| by_all(p)).collect();
        (
            of.iter().map(|p| p.paused).sum::<u32>(),
            of.iter().filter(|p| p.paused > 0).count(),
        )
    } else {
        (
            counts.projects.iter().map(|p| p.working).sum::<u32>(),
            counts.projects.iter().filter(|p| p.working > 0).count(),
        )
    };
    // Without an All projects pause, Resume all would lift nothing: no row (as the icon's menu, d0139 audit AS q1).
    if !resumes || counts.projects.iter().any(by_all) {
        let mut detail = format!("{n} {} {}", word(resumes), in_projects(places));
        if resumes && !stay.is_empty() {
            let verb = if stay.len() == 1 { "stays" } else { "stay" };
            detail.push_str(&format!(" \u{b7} {} {verb} paused", and_list(&stay)));
        }
        rows.push(ScopeRow {
            scope: Scope::All,
            icon: IconName::Layers,
            label: ALL_PROJECTS,
            detail,
            keys: None,
        });
    }
    let note = if resumes && !stay.is_empty() && counts.projects.iter().any(by_all) {
        Some(stays_line(&stay))
    } else if resumes {
        let others: Vec<&str> = counts
            .projects
            .iter()
            .filter(|o| {
                paused(o) && Some(o.project.as_str()) != project.map(|p| p.project.as_str())
            })
            .map(|o| o.name.as_str())
            .collect();
        project.filter(|_| !others.is_empty()).map(|_| {
            let stay = if others.len() == 1 { "stays" } else { "stay" };
            format!(
                "Resume this project only and {} {stay} paused, with its own banner.",
                and_list(&others)
            )
        })
    } else {
        Some(PAUSE_NOTE.to_owned())
    };
    Menu {
        title: if resumes { RESUME_AGENTS } else { PAUSE_AGENTS },
        resumes,
        rows,
        note,
    }
}

fn word(resumes: bool) -> &'static str {
    if resumes { "paused" } else { "working" }
}

/// The paused banner after a Stop agents and quit (TRY-07, page 31 item 6), while every project is still paused:
/// "All projects are paused." · "Stop agents and quit stopped 7 agents in 2 projects yesterday at 18:02. Resume
/// continues where each one was; queued tickets wait until then." Else [`banner`]'s.
pub fn banner_after_stop(
    shown: Option<&str>,
    counts: &PauseCounts,
    service_started_at: i64,
    stopped: Option<&StopRecord>,
    now_ms: i64,
) -> Option<Situation> {
    let every = !counts.projects.is_empty() && counts.projects.iter().all(paused);
    match stopped {
        Some(stop) if every && find(counts, shown).is_some() => Some(Situation::Paused {
            title: "All projects are paused.".to_owned(),
            text: format!(
                "Stop agents and quit stopped {} {} {}. Resume continues where each one was; queued tickets wait \
                 until then.",
                agents(stop.agents),
                in_projects(usize::try_from(stop.projects).unwrap_or(usize::MAX)),
                stopped_when(stop.at, now_ms)
            ),
        }),
        _ => banner(shown, counts, service_started_at),
    }
}

/// When a stop happened: "today at 18:02", "yesterday at 18:02", "on 9 Oct at 18:02".
fn stopped_when(at_ms: i64, now_ms: i64) -> String {
    let at = crate::view_models::clock::local_hhmm(at_ms);
    match crate::view_models::project_agent_page::ended_when(at_ms, now_ms).as_str() {
        "yesterday" => format!("yesterday at {at}"),
        day if day == at => format!("today at {at}"),
        day => format!("on {day} at {at}"),
    }
}

/// The paused banner of the open project `shown` (PAU-08; `s24-07`, `s24-08`): amber, first in the stack, no close,
/// no button (Resume is the title bar's). "shop-web is paused." with what it stopped and which projects keep working,
/// or "All projects are paused." while every project is; after a restart (`service_started_at` came after the pause)
/// "… is still paused." and that a pause never lifts on its own. `None` while the open project runs.
pub fn banner(
    shown: Option<&str>,
    counts: &PauseCounts,
    service_started_at: i64,
) -> Option<Situation> {
    let project = find(counts, shown).filter(|p| paused(p))?;
    let since = project.paused_since.unwrap_or_default();
    let restarted = since < service_started_at;
    let every = counts.projects.len() > 1 && counts.projects.iter().all(paused);
    let (who, stopped) = if every {
        let n: u32 = counts.projects.iter().map(|p| p.paused).sum();
        let places = counts.projects.iter().filter(|p| p.paused > 0).count();
        (
            "All projects are".to_owned(),
            format!("{} {}", agents(n), in_projects(places)),
        )
    } else {
        (format!("{} is", project.name), agents(project.paused))
    };
    let (title, text) = if restarted {
        (
            format!("{who} still paused."),
            format!(
                "{} {stopped} wait where each one was; Resume when you\u{2019}re ready.",
                restarted_line()
            ),
        )
    } else {
        let working: Vec<&str> = counts
            .projects
            .iter()
            .filter(|p| !paused(p) && p.working > 0)
            .map(|p| p.name.as_str())
            .collect();
        let keeps = match working.as_slice() {
            [] => String::new(),
            [one] => format!(" {one} keeps working."),
            more => format!(" {} keep working.", and_list(more)),
        };
        (
            format!("{who} paused."),
            format!("{stopped} stopped; {LEAVES}{keeps}"),
        )
    };
    Some(Situation::Paused { title, text })
}

/// Activity while the open project is paused (PAU-08, `s24-07`): "5 paused", its dot amber. `None` while it runs.
pub fn activity(shown: Option<&str>, counts: &PauseCounts) -> Option<AgentsChip> {
    let project = find(counts, shown).filter(|p| paused(p))?;
    Some(AgentsChip {
        count: project.paused.to_string(),
        label: "paused",
        control: false,
        active: false,
        paused: true,
    })
}

/// The palette's Pause and Resume rows (PAU-08): "Pause this project" or "Resume this project" for the open one,
/// "Pause all projects" while any project runs, "Resume all projects" while an All projects pause holds any (one
/// paused on its own stays paused, d0112 2026-10-27). Each runs at once.
pub fn palette(
    shown: Option<&str>,
    projects: &[otaq_core::board::snapshot::ProjectInfo],
) -> Vec<(Scope, bool, &'static str)> {
    let mut rows = Vec::new();
    if let Some(p) = shown.and_then(|s| projects.iter().find(|p| p.key == s)) {
        rows.push(match p.paused_since {
            Some(_) => (Scope::Project, true, "Resume this project"),
            None => (Scope::Project, false, "Pause this project"),
        });
    }
    if projects.iter().any(|p| p.paused_since.is_none()) {
        rows.push((Scope::All, false, "Pause all projects"));
    }
    if projects
        .iter()
        .any(|p| p.paused_since.is_some() && p.paused_by != Some(PauseKind::Own))
    {
        rows.push((Scope::All, true, "Resume all projects"));
    }
    rows
}

/// The Needs you strip's items without what a pause stopped (`s24-07`: "Needs you · 4", no Interrupted group): a
/// paused project's Interrupted tickets wait for its Resume and read "Paused" on their cards; one that waited for the
/// user keeps its question.
pub fn strip_items(
    items: Vec<crate::view_models::needs_you_strip::Item>,
    projects: &[otaq_core::board::snapshot::ProjectInfo],
) -> Vec<crate::view_models::needs_you_strip::Item> {
    use crate::view_models::needs_you_strip::Item;
    use otaq_core::domain::Status;
    let paused = |key: &str| {
        projects
            .iter()
            .any(|p| p.key == key && p.paused_since.is_some())
    };
    items
        .into_iter()
        .filter(|item| match item {
            Item::Ticket(t) => {
                !(t.status == Status::Interrupted
                    && !t.interrupted_from.is_some_and(|s| s.is_needs_you_family())
                    && paused(&t.project))
            }
            Item::Agent(_) => true,
        })
        .collect()
}

/// The gallery's states (`s24-07`, `s24-08`): Pause on shop-web with billing-api working, Resume on shop-web with
/// billing-api paused too, Resume with only shop-web paused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PauseGallery")]
pub struct Gallery {
    pub states: Vec<GalleryState>,
    /// The paused banner: shop-web paused, every project paused, shop-web still paused after a restart.
    pub banners: Vec<crate::view_models::banners::BannerView>,
}

/// One state of the gallery: its name and what the title bar draws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PauseGalleryState")]
pub struct GalleryState {
    pub name: &'static str,
    pub view: View,
}

/// The example's projects: shop-web with 5 agents, billing-api with 2; `paused` names the paused ones.
pub fn example(paused: &[&str]) -> PauseCounts {
    let project = |key: &str, name: &str, working: u32| {
        let is_paused = paused.contains(&key);
        ProjectPause {
            project: key.to_owned(),
            name: name.to_owned(),
            paused_since: is_paused.then_some(1),
            paused_by: is_paused.then_some(PauseKind::All),
            working: if is_paused { 0 } else { working },
            paused: if is_paused { working } else { 0 },
        }
    };
    PauseCounts {
        working: 0,
        paused: 0,
        projects: vec![
            project("BILL", "billing-api", 2),
            project("SHOP", "shop-web", 5),
        ],
    }
}

/// Page 31 item 7: billing-api paused on its own, then Pause all: Resume all leaves billing-api paused.
pub fn example_paused_on_its_own() -> PauseCounts {
    let mut counts = example(&["SHOP", "BILL"]);
    for p in &mut counts.projects {
        if p.project == "BILL" {
            p.paused_by = Some(PauseKind::Own);
        }
    }
    counts
}

pub fn gallery(keys: &KeyLabels) -> Gallery {
    let state = |name, paused: &[&str]| GalleryState {
        name,
        view: view(Some("SHOP"), &example(paused), keys),
    };
    let banner = |paused: &[&str], started_at| {
        let mut model = crate::view_models::banners::BannerModel::default();
        if let Some(situation) = banner(Some("SHOP"), &example(paused), started_at) {
            model.raise(situation);
        }
        model.rows().into_iter().map(|r| r.view(false))
    };
    Gallery {
        states: vec![
            state("Pause · shop-web working", &[]),
            state("Resume · every project paused", &["SHOP", "BILL"]),
            state("Resume · shop-web paused", &["SHOP"]),
            GalleryState {
                name: "Resume · billing-api paused on its own",
                view: view(Some("SHOP"), &example_paused_on_its_own(), keys),
            },
        ],
        banners: banner(&["SHOP"], 0)
            .chain(banner(&["SHOP", "BILL"], 0))
            .chain(banner(&["SHOP"], 2))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Platform;

    fn keys() -> KeyLabels {
        KeyLabels::base(Platform::Mac)
    }

    fn counts(projects: &[(&str, &str, Option<i64>, u32, u32)]) -> PauseCounts {
        PauseCounts {
            working: 0,
            paused: 0,
            projects: projects
                .iter()
                .map(|(key, name, since, working, paused)| ProjectPause {
                    project: (*key).to_owned(),
                    name: (*name).to_owned(),
                    paused_since: *since,
                    paused_by: since.map(|_| PauseKind::All),
                    working: *working,
                    paused: *paused,
                })
                .collect(),
        }
    }

    /// The example: shop-web 5 working, billing-api 2, otaq idle.
    fn running() -> PauseCounts {
        counts(&[
            ("BILL", "billing-api", None, 2, 0),
            ("DOCS", "docs-site", None, 0, 0),
            ("SHOP", "shop-web", None, 5, 0),
        ])
    }

    fn rows(menu: &Menu) -> Vec<(Scope, &str, &str, Option<&str>)> {
        menu.rows
            .iter()
            .map(|r| (r.scope, r.label, r.detail.as_str(), r.keys.as_deref()))
            .collect()
    }

    /// `s24-07`: Pause opens "PAUSE AGENTS": This project first with ⇧⌘., All projects with every project's count, and
    /// the line that Pause stops at once.
    #[test]
    fn the_pause_menu_counts_this_project_and_all_projects() {
        let v = view(Some("SHOP"), &running(), &keys());
        assert_eq!(v.project.as_deref(), Some("SHOP"));
        assert_eq!(
            (v.button.label, v.button.icon, v.button.resumes),
            ("Pause", IconName::Pause, false)
        );
        assert_eq!(v.button.tooltip, "Pause / Resume agents");
        assert_eq!(v.button.keys.as_deref(), Some("\u{21e7}\u{2318}."));
        assert_eq!(v.menu.title, "Pause agents");
        assert!(!v.menu.resumes);
        assert_eq!(
            rows(&v.menu),
            [
                (
                    Scope::Project,
                    "This project",
                    "shop-web \u{b7} 5 working",
                    Some("\u{21e7}\u{2318}.")
                ),
                (Scope::All, "All projects", "7 working in 2 projects", None),
            ]
        );
        assert_eq!(
            v.menu.note.as_deref(),
            Some("Agents stop now; Resume continues where each one was. Nothing is lost.")
        );
    }

    /// `s24-08`: while the open project is paused the button is Resume, and its menu counts what is paused; with
    /// billing-api paused too, the line says it stays paused with its own banner.
    #[test]
    fn the_resume_menu_counts_what_is_paused() {
        let all = counts(&[
            ("BILL", "billing-api", Some(10), 0, 2),
            ("DOCS", "docs-site", Some(10), 0, 0),
            ("SHOP", "shop-web", Some(10), 0, 5),
        ]);
        let v = view(Some("SHOP"), &all, &keys());
        assert_eq!(
            (v.button.label, v.button.icon, v.button.resumes),
            ("Resume", IconName::Play, true)
        );
        assert_eq!(v.menu.title, "Resume agents");
        assert_eq!(
            rows(&v.menu),
            [
                (
                    Scope::Project,
                    "This project",
                    "shop-web \u{b7} 5 paused",
                    Some("\u{21e7}\u{2318}.")
                ),
                (Scope::All, "All projects", "7 paused in 2 projects", None),
            ]
        );
        assert_eq!(
            v.menu.note.as_deref(),
            Some(
                "Resume this project only and billing-api and docs-site stay paused, with its own banner."
            )
        );

        let one = counts(&[
            ("BILL", "billing-api", None, 2, 0),
            ("SHOP", "shop-web", Some(10), 0, 1),
        ]);
        let v = view(Some("SHOP"), &one, &keys());
        assert_eq!(
            rows(&v.menu)[1],
            (Scope::All, "All projects", "1 paused in 1 project", None)
        );
        assert_eq!(v.menu.note, None, "nothing else stays paused");
        // On billing-api, which runs, the button is Pause and All projects counts what works.
        let v = view(Some("BILL"), &one, &keys());
        assert_eq!(v.button.label, "Pause");
        assert_eq!(rows(&v.menu)[1].2, "2 working in 1 project");
    }

    #[test]
    fn without_a_project_only_all_projects_is_offered() {
        let v = view(None, &running(), &keys());
        assert_eq!(v.project, None);
        assert_eq!(
            rows(&v.menu),
            [(Scope::All, "All projects", "7 working in 2 projects", None)]
        );
    }

    fn texts(s: Option<Situation>) -> (String, String) {
        match s {
            Some(Situation::Paused { title, text }) => (title, text),
            other => panic!("not the paused banner: {other:?}"),
        }
    }

    /// PAU-08 (`s24-07`, `s24-08`): the banner's two texts, the project's and every project's.
    #[test]
    fn the_banner_says_what_is_paused_and_what_keeps_working() {
        let one = counts(&[
            ("BILL", "billing-api", None, 2, 0),
            ("DOCS", "docs-site", None, 0, 0),
            ("SHOP", "shop-web", Some(100), 0, 5),
        ]);
        assert_eq!(
            texts(banner(Some("SHOP"), &one, 50)),
            (
                "shop-web is paused.".to_owned(),
                "5 agents stopped; Resume continues where each one was. Queued tickets wait; questions for you stay \
                 open. billing-api keeps working."
                    .to_owned()
            )
        );
        assert_eq!(banner(Some("BILL"), &one, 50), None, "billing-api runs");
        assert_eq!(banner(None, &one, 50), None);

        let all = counts(&[
            ("BILL", "billing-api", Some(100), 0, 2),
            ("DOCS", "docs-site", Some(100), 0, 0),
            ("SHOP", "shop-web", Some(100), 0, 5),
        ]);
        assert_eq!(
            texts(banner(Some("SHOP"), &all, 50)),
            (
                "All projects are paused.".to_owned(),
                "7 agents in 2 projects stopped; Resume continues where each one was. Queued tickets wait; \
                 questions for you stay open."
                    .to_owned()
            )
        );
        // Nothing else works: no "keeps working"; one agent is one.
        let alone = counts(&[
            ("BILL", "billing-api", None, 0, 0),
            ("SHOP", "shop-web", Some(100), 0, 1),
        ]);
        assert_eq!(
            texts(banner(Some("SHOP"), &alone, 50)).1,
            "1 agent stopped; Resume continues where each one was. Queued tickets wait; questions for you stay open."
        );
    }

    /// PAU-07 (`s24-08`): a pause from before the service started says it is still paused after the restart.
    #[test]
    fn after_a_restart_the_banner_says_it_is_still_paused() {
        let one = counts(&[
            ("BILL", "billing-api", None, 2, 0),
            ("SHOP", "shop-web", Some(100), 0, 5),
        ]);
        assert_eq!(
            texts(banner(Some("SHOP"), &one, 200)),
            (
                "shop-web is still paused.".to_owned(),
                format!(
                    "{} 5 agents wait where each one was; Resume when you\u{2019}re ready.",
                    restarted_line()
                )
            )
        );
        let all = counts(&[
            ("BILL", "billing-api", Some(100), 0, 2),
            ("SHOP", "shop-web", Some(100), 0, 5),
        ]);
        assert_eq!(
            texts(banner(Some("BILL"), &all, 200)).0,
            "All projects are still paused."
        );
    }

    /// TRY-07 (page 31 item 6): after Stop agents and quit, only the pause banner, worded for the stop; once a
    /// project runs again, the usual banner.
    #[test]
    fn after_a_stop_the_banner_says_what_it_stopped() {
        let all = counts(&[
            ("BILL", "billing-api", Some(100), 0, 2),
            ("SHOP", "shop-web", Some(100), 0, 5),
        ]);
        let day = 86_400_000;
        let at = 1_790_000_000_000;
        let stop = StopRecord {
            at,
            agents: 7,
            projects: 2,
        };
        let (title, text) = texts(banner_after_stop(
            Some("SHOP"),
            &all,
            200,
            Some(&stop),
            at + day,
        ));
        assert_eq!(title, "All projects are paused.");
        assert_eq!(
            text,
            format!(
                "Stop agents and quit stopped 7 agents in 2 projects yesterday at {}. Resume continues where each \
                 one was; queued tickets wait until then.",
                crate::view_models::clock::local_hhmm(at)
            )
        );
        let (_, today) = texts(banner_after_stop(
            Some("SHOP"),
            &all,
            200,
            Some(&stop),
            at + 60_000,
        ));
        assert!(today.contains(" today at "), "{today}");
        let (_, long_ago) = texts(banner_after_stop(
            Some("SHOP"),
            &all,
            200,
            Some(&stop),
            at + 9 * day,
        ));
        assert!(long_ago.contains(" on "), "{long_ago}");
        // billing-api resumed on its own: the usual banner.
        let one = counts(&[
            ("BILL", "billing-api", None, 2, 0),
            ("SHOP", "shop-web", Some(100), 0, 5),
        ]);
        assert_eq!(
            texts(banner_after_stop(Some("SHOP"), &one, 200, Some(&stop), at)).0,
            "shop-web is still paused."
        );
        assert_eq!(banner_after_stop(None, &all, 200, Some(&stop), at), None);
    }

    #[test]
    fn activity_reads_paused_while_the_open_project_is() {
        let one = counts(&[
            ("BILL", "billing-api", None, 2, 0),
            ("SHOP", "shop-web", Some(100), 0, 5),
        ]);
        let chip = activity(Some("SHOP"), &one).unwrap();
        assert_eq!(
            (chip.count.as_str(), chip.label, chip.paused),
            ("5", "paused", true)
        );
        assert_eq!(activity(Some("BILL"), &one), None);
    }

    #[test]
    fn the_palette_offers_this_project_and_all_projects() {
        use otaq_core::board::snapshot::ProjectInfo;
        let info = |key: &str, paused: bool| ProjectInfo {
            key: key.to_owned(),
            name: key.to_lowercase(),
            repo_path: String::new(),
            base_branch: "main".to_owned(),
            agents_working: 0,
            needs_you: 0,
            flow_setup_since: None,
            paused_since: paused.then_some(1),
            paused_by: paused.then_some(PauseKind::All),
        };
        let titles = |shown, projects: &[ProjectInfo]| -> Vec<&'static str> {
            palette(shown, projects)
                .into_iter()
                .map(|(_, _, t)| t)
                .collect()
        };
        let both = [info("BILL", false), info("SHOP", false)];
        assert_eq!(
            titles(Some("SHOP"), &both),
            ["Pause this project", "Pause all projects"]
        );
        let one = [info("BILL", false), info("SHOP", true)];
        assert_eq!(
            titles(Some("SHOP"), &one),
            [
                "Resume this project",
                "Pause all projects",
                "Resume all projects"
            ]
        );
        assert_eq!(
            palette(Some("SHOP"), &one)[0],
            (Scope::Project, true, "Resume this project")
        );
        assert_eq!(titles(None, &[info("SHOP", true)]), ["Resume all projects"]);

        // d0112 2026-10-27: a project paused on its own is resumed on its own; Resume all only lifts an All pause.
        let own = |key: &str| ProjectInfo {
            paused_by: Some(PauseKind::Own),
            ..info(key, true)
        };
        assert_eq!(
            titles(Some("SHOP"), &[info("BILL", false), own("SHOP")]),
            ["Resume this project", "Pause all projects"]
        );
        assert_eq!(
            titles(Some("SHOP"), &[info("BILL", true), own("SHOP")]),
            ["Resume this project", "Resume all projects"]
        );
    }

    /// Paused on its own (`own`) or by All projects (`all`), as the service says.
    fn paused_by(mut counts: PauseCounts, own: &[&str]) -> PauseCounts {
        for p in &mut counts.projects {
            if own.contains(&p.project.as_str()) {
                p.paused_by = Some(PauseKind::Own);
            }
        }
        counts
    }

    /// Page 31 item 7 (d0112 2026-10-27): web-app paused on its own, then Pause all: Resume's All projects counts what
    /// it lifts and names web-app, which stays paused, with the line why.
    #[test]
    fn resume_all_names_a_project_paused_on_its_own() {
        let all = paused_by(
            counts(&[
                ("BILL", "billing-api", Some(10), 0, 2),
                ("SHOP", "shop-web", Some(10), 0, 5),
                ("WEB", "web-app", Some(5), 0, 3),
            ]),
            &["WEB"],
        );
        let v = view(Some("SHOP"), &all, &keys());
        assert_eq!(
            rows(&v.menu)[1],
            (
                Scope::All,
                "All projects",
                "7 paused in 2 projects \u{b7} web-app stays paused",
                None
            )
        );
        assert_eq!(
            v.menu.note.as_deref(),
            Some(
                "web-app was paused on its own: Resume all leaves it paused, with its own banner and Resume."
            )
        );
        // On web-app itself the same row; This project resumes it.
        let v = view(Some("WEB"), &all, &keys());
        assert_eq!(rows(&v.menu)[0].2, "web-app \u{b7} 3 paused");
        assert_eq!(
            rows(&v.menu)[1].2,
            "7 paused in 2 projects \u{b7} web-app stays paused"
        );

        // Two on their own.
        let two = paused_by(all.clone(), &["WEB", "BILL"]);
        let v = view(Some("SHOP"), &two, &keys());
        assert_eq!(
            rows(&v.menu)[1].2,
            "5 paused in 1 project \u{b7} billing-api and web-app stay paused"
        );
        assert_eq!(
            v.menu.note.as_deref(),
            Some(
                "billing-api and web-app were paused on their own: Resume all leaves them paused, each with its own \
                 banner and Resume."
            )
        );
    }

    /// d0139 audit AS q1, in the window too: with only projects paused on their own, Resume all would lift nothing, so
    /// the menu has This project only.
    #[test]
    fn without_an_all_projects_pause_resume_has_this_project_only() {
        let alone = paused_by(
            counts(&[
                ("SHOP", "shop-web", None, 5, 0),
                ("WEB", "web-app", Some(5), 0, 3),
            ]),
            &["WEB"],
        );
        let v = view(Some("WEB"), &alone, &keys());
        assert!(v.button.resumes);
        assert_eq!(
            rows(&v.menu),
            [(
                Scope::Project,
                "This project",
                "web-app \u{b7} 3 paused",
                Some("\u{21e7}\u{2318}.")
            )]
        );
        assert_eq!(v.menu.note, None);
        // Every project on its own, none open: nothing for Resume all, so the button is Pause.
        let both = paused_by(
            counts(&[
                ("SHOP", "shop-web", Some(5), 0, 5),
                ("WEB", "web-app", Some(5), 0, 3),
            ]),
            &["SHOP", "WEB"],
        );
        assert!(!view(None, &both, &keys()).button.resumes);
    }

    /// `s24-07`: the strip of a paused project leaves out what the pause stopped; a question it stopped stays, and
    /// a running project's Interrupted tickets stay.
    #[test]
    fn the_strip_leaves_out_what_the_pause_stopped() {
        use crate::view_models::needs_you_strip::samples::{item, ticket};
        use otaq_core::board::snapshot::ProjectInfo;
        use otaq_core::domain::Status;
        let info = |key: &str, paused: bool| ProjectInfo {
            key: key.to_owned(),
            name: key.to_lowercase(),
            repo_path: String::new(),
            base_branch: "main".to_owned(),
            agents_working: 0,
            needs_you: 0,
            flow_setup_since: None,
            paused_since: paused.then_some(1),
            paused_by: None,
        };
        let mut asked = ticket(
            "SHOP-12",
            "Add dark mode toggle",
            Status::Interrupted,
            "a question",
        );
        asked.interrupted_from = Some(Status::AsksYou);
        let items = vec![
            item(ticket(
                "SHOP-33",
                "Shipping estimate",
                Status::Interrupted,
                "interrupted",
            )),
            item(asked),
            item(ticket(
                "SHOP-22",
                "Fix flaky login redirect",
                Status::NeedsYou,
                "allow a command",
            )),
            item(ticket(
                "BILL-4",
                "Invoice template",
                Status::Interrupted,
                "interrupted",
            )),
        ];
        let kept: Vec<String> = strip_items(items, &[info("SHOP", true), info("BILL", false)])
            .iter()
            .map(|i| match i {
                crate::view_models::needs_you_strip::Item::Ticket(t) => t.key.to_string(),
                crate::view_models::needs_you_strip::Item::Agent(a) => a.project.clone(),
            })
            .collect();
        assert_eq!(kept, ["SHOP-12", "SHOP-22", "BILL-4"]);
    }

    #[test]
    fn the_gallery_draws_pause_and_resume() {
        let g = gallery(&keys());
        let labels: Vec<&str> = g.states.iter().map(|s| s.view.button.label).collect();
        assert_eq!(labels, ["Pause", "Resume", "Resume", "Resume"]);
        assert_eq!(
            g.states[3].view.menu.rows[1].detail,
            "5 paused in 1 project \u{b7} billing-api stays paused"
        );
        let titles: Vec<&str> = g.banners.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "shop-web is paused.",
                "All projects are paused.",
                "shop-web is still paused."
            ]
        );
    }
}

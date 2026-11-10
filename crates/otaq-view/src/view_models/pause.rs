//! **Pause in the app** (PAU-07, PAU-08; d0112, d0093): the title bar's **Pause**, shown only while agents run, opens
//! its scope menu: **Pause shop-web** first (focused, Return) and **Pause all projects**, each saying what it stops,
//! then the line that Resume continues each one where it was. **No confirmation**: picking a scope pauses at once,
//! since nothing is lost. ⇧⌘. opens the same menu while the button shows.
//!
//! While the open project is paused: the amber banner first in the stack, which can't be closed, carries **Resume**
//! ("shop-web is paused" / "All projects are paused"); every card the pause stopped reads "Paused" in amber, its timer
//! held ([`crate::view_models::card`]), and the rail's tile counts its paused agents in amber. After a restart the
//! banner says it is **still** paused and that a pause never lifts on its own, with no toast (PAU-07, Q417); the reboot
//! summary leaves paused projects out ([`crate::view_models::after_restart::outside_pause`]).
//!
//! The counts are the service's (`agents.pause_counts`): `working` is what Pause stops, `paused` what Resume resumes.
//! **Resume all projects lifts only what an All projects pause set** (d0112 2026-10-27): a project paused on its own
//! stays paused, so the banner's Resume on such a project resumes it alone.

use otaq_core::agents::pause::{PauseCounts, PauseKind, ProjectPause, StopRecord};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::view_models::agent_tools_page::and_list;
use crate::view_models::banners::Situation;

pub const PAUSE: &str = "Pause";
pub const RESUME: &str = "Resume";
/// The menu's name for screen readers.
pub const PAUSE_AGENTS: &str = "Pause agents";
pub const PAUSE_ALL_PROJECTS: &str = "Pause all projects";
/// The Pause menu's line: Pause stops agents at once, and nothing is lost.
pub const PAUSE_NOTE: &str = "Resume continues each one where it was.";
/// What a pause leaves, in the banner after "5 agents stopped where they were; ".
const LEAVES: &str = "queued tickets wait.";
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
    /// The open project: "Pause shop-web".
    Project,
    /// "Pause all projects".
    All,
}

/// The title bar's button: "Pause" with its icon. A click opens the scope menu. `PauseButton` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PauseButton")]
pub struct Button {
    pub label: &'static str,
    pub icon: IconName,
    /// The tooltip's name, "Pause / Resume agents", and the user's key for it (⇧⌘.).
    pub tooltip: &'static str,
    pub keys: Option<String>,
}

/// One row of the menu: "Pause shop-web · 4 agents stop where they are; queued tickets wait. ⇧⌘." or "Pause all
/// projects · 5 agents in 2 projects.". `PauseScopeRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PauseScopeRow")]
pub struct ScopeRow {
    pub scope: Scope,
    pub icon: IconName,
    pub label: String,
    pub detail: String,
    /// The menu's key on the first row (⇧⌘.).
    pub keys: Option<String>,
}

/// The scope menu under the button: the open project first and focused (Return picks it), then all projects, then
/// the line. `PauseMenu` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PauseMenu")]
pub struct Menu {
    /// "Pause agents": the menu's name for screen readers.
    pub title: &'static str,
    pub rows: Vec<ScopeRow>,
    /// "Resume continues each one where it was."
    pub note: Option<String>,
}

/// What the title bar's Pause part draws for the open project. `PauseView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PauseView")]
pub struct View {
    /// Whether Pause is in the title bar: only while agents run in some project.
    pub shown: bool,
    /// The open project's key, which its row pauses; `None` before the first project.
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

/// The open project's counts, if it is one of `counts`.
fn find<'a>(counts: &'a PauseCounts, shown: Option<&str>) -> Option<&'a ProjectPause> {
    let shown = shown?;
    counts.projects.iter().find(|p| p.project == shown)
}

/// The Pause part of the title bar for the open project `shown`; `counts` are every project's (`agents.pause_counts`
/// with All projects). Pause shows only while agents run; Resume is the paused banner's.
pub fn view(shown: Option<&str>, counts: &PauseCounts, keys: &KeyLabels) -> View {
    let project = find(counts, shown);
    let key = keys.label(KeyAction::Pause);
    View {
        shown: counts.projects.iter().any(|p| p.working > 0),
        project: project.map(|p| p.project.clone()),
        button: Button {
            label: PAUSE,
            icon: IconName::Pause,
            tooltip: KeyAction::Pause.title(),
            keys: key.clone(),
        },
        menu: menu(project, counts, key),
    }
}

/// "4 agents stop where they are; queued tickets wait."
fn stops(n: u32) -> String {
    match n {
        0 => "Queued tickets wait.".to_owned(),
        1 => "1 agent stops where it is; queued tickets wait.".to_owned(),
        n => format!("{n} agents stop where they are; queued tickets wait."),
    }
}

fn menu(project: Option<&ProjectPause>, counts: &PauseCounts, key: Option<String>) -> Menu {
    let mut rows = Vec::new();
    // A paused project has nothing to pause: the banner offers its Resume.
    if let Some(p) = project.filter(|p| !paused(p)) {
        rows.push(ScopeRow {
            scope: Scope::Project,
            icon: IconName::Pause,
            label: format!("{PAUSE} {}", p.name),
            detail: stops(p.working),
            keys: key,
        });
    }
    let n: u32 = counts.projects.iter().map(|p| p.working).sum();
    let places = counts.projects.iter().filter(|p| p.working > 0).count();
    rows.push(ScopeRow {
        scope: Scope::All,
        icon: IconName::Layers,
        label: PAUSE_ALL_PROJECTS.to_owned(),
        detail: format!("{} {}.", agents(n), in_projects(places)),
        keys: None,
    });
    Menu {
        title: PAUSE_AGENTS,
        rows,
        note: Some(PAUSE_NOTE.to_owned()),
    }
}

/// How the paused banner's Resume resumes the open project `shown`: every project when an All projects pause holds
/// them all (Resume all lifts only what it set), else the open project alone. `None` while it isn't paused.
pub fn banner_resume(shown: Option<&str>, counts: &PauseCounts) -> Option<Scope> {
    let project = find(counts, shown).filter(|p| paused(p))?;
    let every = counts.projects.len() > 1 && counts.projects.iter().all(paused);
    let by_all = |p: &ProjectPause| p.paused_by != Some(PauseKind::Own);
    Some(
        if every && by_all(project) && counts.projects.iter().all(by_all) {
            Scope::All
        } else {
            Scope::Project
        },
    )
}

/// The paused banner after a Stop agents and quit (TRY-07, page 31 item 6), while every project is still paused:
/// "All projects are paused" · "Quit and stop agents stopped 7 agents in 2 projects yesterday at 18:02. Resume
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
            title: "All projects are paused".to_owned(),
            text: format!(
                "Quit and stop agents stopped {} {} {}. Resume continues where each one was; queued tickets wait \
                 until then.",
                agents(stop.agents),
                in_projects(usize::try_from(stop.projects).unwrap_or(usize::MAX)),
                stopped_when(stop.at, now_ms)
            ),
            state: format!("stopped@{}", stop.at),
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

/// The paused banner of the open project `shown` (PAU-08): amber, first in the stack, no close, with Resume. "shop-web
/// is paused" with what it stopped and which projects keep working, or "All projects are paused" while every project
/// is; after a restart (`service_started_at` came after the pause) "… is still paused" and that a pause never lifts
/// on its own. `None` while the open project runs.
pub fn banner(
    shown: Option<&str>,
    counts: &PauseCounts,
    service_started_at: i64,
) -> Option<Situation> {
    let project = find(counts, shown).filter(|p| paused(p))?;
    let since = project.paused_since.unwrap_or_default();
    let restarted = since < service_started_at;
    let every = counts.projects.len() > 1 && counts.projects.iter().all(paused);
    let (who, stopped, n) = if every {
        let n: u32 = counts.projects.iter().map(|p| p.paused).sum();
        let places = counts.projects.iter().filter(|p| p.paused > 0).count();
        (
            "All projects are".to_owned(),
            format!("{} {}", agents(n), in_projects(places)),
            n,
        )
    } else {
        (
            format!("{} is", project.name),
            agents(project.paused),
            project.paused,
        )
    };
    let were = if n == 1 { "it was" } else { "they were" };
    let (title, text) = if restarted {
        (
            format!("{who} still paused"),
            format!("{} {stopped} wait where each one was.", restarted_line()),
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
            format!("{who} paused"),
            format!("{stopped} stopped where {were}; {LEAVES}{keeps}"),
        )
    };
    // This pause: the project and when it began (a restart words it anew, but it is the same pause).
    let state = format!("{}@{since}", project.project);
    Some(Situation::Paused { title, text, state })
}

/// The palette's Pause and Resume rows (PAU-08): "Resume this project" for the open one while it is paused, "Pause
/// this project" while an agent works there, "Pause all projects" while an agent works in any project that runs, "Resume all projects" while an All projects pause holds any (one
/// paused on its own stays paused, d0112 2026-10-27). Each runs at once.
pub fn palette(
    shown: Option<&str>,
    projects: &[otaq_core::board::snapshot::ProjectInfo],
) -> Vec<(Scope, bool, &'static str)> {
    let mut rows = Vec::new();
    // Pause is offered only while something runs there, as the title bar's Pause.
    let runs = |p: &otaq_core::board::snapshot::ProjectInfo| {
        p.paused_since.is_none() && p.agents_working > 0
    };
    if let Some(p) = shown.and_then(|s| projects.iter().find(|p| p.key == s)) {
        if p.paused_since.is_some() {
            rows.push((Scope::Project, true, "Resume this project"));
        } else if runs(p) {
            rows.push((Scope::Project, false, "Pause this project"));
        }
    }
    if projects.iter().any(runs) {
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
            state("Pause · shop-web paused, billing-api working", &["SHOP"]),
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
            .map(|r| {
                (
                    r.scope,
                    r.label.as_str(),
                    r.detail.as_str(),
                    r.keys.as_deref(),
                )
            })
            .collect()
    }

    /// Pause asks for the scope: the open project first with ⇧⌘., then all projects with every project's count, and
    /// the line that Resume continues each one where it was.
    #[test]
    fn the_pause_menu_asks_this_project_or_all_of_them() {
        let v = view(Some("SHOP"), &running(), &keys());
        assert!(v.shown);
        assert_eq!(v.project.as_deref(), Some("SHOP"));
        assert_eq!((v.button.label, v.button.icon), ("Pause", IconName::Pause));
        assert_eq!(v.button.tooltip, "Pause / Resume agents");
        assert_eq!(v.button.keys.as_deref(), Some("\u{21e7}\u{2318}."));
        assert_eq!(
            rows(&v.menu),
            [
                (
                    Scope::Project,
                    "Pause shop-web",
                    "5 agents stop where they are; queued tickets wait.",
                    Some("\u{21e7}\u{2318}.")
                ),
                (
                    Scope::All,
                    "Pause all projects",
                    "7 agents in 2 projects.",
                    None
                ),
            ]
        );
        assert_eq!(
            v.menu.note.as_deref(),
            Some("Resume continues each one where it was.")
        );
        let one = counts(&[("SHOP", "shop-web", None, 1, 0)]);
        assert_eq!(
            rows(&view(Some("SHOP"), &one, &keys()).menu)[0].2,
            "1 agent stops where it is; queued tickets wait."
        );
    }

    /// With nothing running the title bar has no Pause; a paused project's Resume is its banner's, so while another
    /// project runs the menu offers only all projects.
    #[test]
    fn pause_shows_only_while_agents_run() {
        let idle = counts(&[
            ("BILL", "billing-api", None, 0, 0),
            ("SHOP", "shop-web", None, 0, 0),
        ]);
        assert!(!view(Some("SHOP"), &idle, &keys()).shown);
        let paused = counts(&[
            ("BILL", "billing-api", None, 0, 0),
            ("SHOP", "shop-web", Some(10), 0, 5),
        ]);
        assert!(!view(Some("SHOP"), &paused, &keys()).shown);
        let other = counts(&[
            ("BILL", "billing-api", None, 2, 0),
            ("SHOP", "shop-web", Some(10), 0, 5),
        ]);
        let v = view(Some("SHOP"), &other, &keys());
        assert!(v.shown);
        assert_eq!(
            rows(&v.menu),
            [(
                Scope::All,
                "Pause all projects",
                "2 agents in 1 project.",
                None
            )]
        );
    }

    #[test]
    fn without_a_project_only_all_projects_is_offered() {
        let v = view(None, &running(), &keys());
        assert_eq!(v.project, None);
        assert_eq!(
            rows(&v.menu),
            [(
                Scope::All,
                "Pause all projects",
                "7 agents in 2 projects.",
                None
            )]
        );
    }

    /// The banner's Resume lifts every project only when an All projects pause holds them all; else the open one.
    #[test]
    fn the_banner_resumes_the_project_or_every_project() {
        let all = counts(&[
            ("BILL", "billing-api", Some(10), 0, 2),
            ("SHOP", "shop-web", Some(10), 0, 5),
        ]);
        assert_eq!(banner_resume(Some("SHOP"), &all), Some(Scope::All));
        let one = counts(&[
            ("BILL", "billing-api", None, 2, 0),
            ("SHOP", "shop-web", Some(10), 0, 5),
        ]);
        assert_eq!(banner_resume(Some("SHOP"), &one), Some(Scope::Project));
        assert_eq!(banner_resume(Some("BILL"), &one), None, "billing-api runs");
        let mut own = all.clone();
        own.projects[0].paused_by = Some(PauseKind::Own);
        assert_eq!(banner_resume(Some("SHOP"), &own), Some(Scope::Project));
    }

    fn texts(s: Option<Situation>) -> (String, String) {
        match s {
            Some(Situation::Paused { title, text, .. }) => (title, text),
            other => panic!("not the paused banner: {other:?}"),
        }
    }

    /// PAU-08: the banner's two texts, the project's and every project's.
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
                "shop-web is paused".to_owned(),
                "5 agents stopped where they were; queued tickets wait. billing-api keeps working."
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
                "All projects are paused".to_owned(),
                "7 agents in 2 projects stopped where they were; queued tickets wait.".to_owned()
            )
        );
        // Nothing else works: no "keeps working"; one agent is one.
        let alone = counts(&[
            ("BILL", "billing-api", None, 0, 0),
            ("SHOP", "shop-web", Some(100), 0, 1),
        ]);
        assert_eq!(
            texts(banner(Some("SHOP"), &alone, 50)).1,
            "1 agent stopped where it was; queued tickets wait."
        );
    }

    /// PAU-07: a pause from before the service started says it is still paused after the restart.
    #[test]
    fn after_a_restart_the_banner_says_it_is_still_paused() {
        let one = counts(&[
            ("BILL", "billing-api", None, 2, 0),
            ("SHOP", "shop-web", Some(100), 0, 5),
        ]);
        assert_eq!(
            texts(banner(Some("SHOP"), &one, 200)),
            (
                "shop-web is still paused".to_owned(),
                format!("{} 5 agents wait where each one was.", restarted_line())
            )
        );
        let all = counts(&[
            ("BILL", "billing-api", Some(100), 0, 2),
            ("SHOP", "shop-web", Some(100), 0, 5),
        ]);
        assert_eq!(
            texts(banner(Some("BILL"), &all, 200)).0,
            "All projects are still paused"
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
        assert_eq!(title, "All projects are paused");
        assert_eq!(
            text,
            format!(
                "Quit and stop agents stopped 7 agents in 2 projects yesterday at {}. Resume continues where each \
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
            "shop-web is still paused"
        );
        assert_eq!(banner_after_stop(None, &all, 200, Some(&stop), at), None);
    }

    #[test]
    fn the_palette_offers_this_project_and_all_projects() {
        use otaq_core::board::snapshot::ProjectInfo;
        let info = |key: &str, paused: bool| ProjectInfo {
            key: key.to_owned(),
            name: key.to_lowercase(),
            repo_path: String::new(),
            base_branch: "main".to_owned(),
            agents_working: u32::from(!paused),
            needs_you: 0,
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

        // Nothing at work: no Pause anywhere (a kept session of a ticket waiting for review isn't at work).
        let idle = |key: &str| ProjectInfo {
            agents_working: 0,
            ..info(key, false)
        };
        assert!(titles(Some("SHOP"), &[idle("BILL"), idle("SHOP")]).is_empty());
        assert_eq!(
            titles(Some("SHOP"), &[info("BILL", false), idle("SHOP")]),
            ["Pause all projects"]
        );
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
    fn the_gallery_draws_pause_and_the_paused_banner() {
        let g = gallery(&keys());
        let shown: Vec<bool> = g.states.iter().map(|s| s.view.shown).collect();
        assert_eq!(shown, [true, true]);
        assert_eq!(g.states[1].view.menu.rows.len(), 1, "only all projects");
        let titles: Vec<&str> = g.banners.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "shop-web is paused",
                "All projects are paused",
                "shop-web is still paused"
            ]
        );
        assert_eq!(g.banners[0].buttons[0].label, "Resume");
    }
}

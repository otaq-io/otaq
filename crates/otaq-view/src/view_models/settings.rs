//! Settings (SET-01, PRJ-03, PRJ-04; d0003, d0023, d0040, d0055; *15 · Settings*): one window with a section
//! nav; the global page (Data, Appearance, Tools) and one page per project (name, key, repository, base branch,
//! the notes every agent in the project gets, setup and run commands, Open URL, env files, Remove project). Everything is stored in the app's database, never in a repository (d0023). Values the service
//! flags (a missing env file, a URL that isn't one) show in red with its reason; the nav marks the project.
//! Sections for later features are added by their tickets (limits T-0082 is in, workflow skills T-0139, usage
//! T-0126, notifications T-0117, PR provider T-0141, "let finished tickets answer" T-0153).

use otaq_core::board::settings::{
    GlobalSettings, ProjectSettings, SettingProblem, TERMINAL_FONT_SIZES,
};
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::data_dir::DataDir;

use crate::icons::IconName;
use crate::shell::tilde;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Global,
    Project(String),
}

/// The global page's sections, in nav order, with their nav icon (*15 · Settings → Nav*).
pub const GLOBAL_SECTIONS: [(&str, IconName); 6] = [
    ("Agent limits", IconName::Gauge),
    ("Usage", IconName::Coins),
    ("Notifications", IconName::Bell),
    ("Data", IconName::Database),
    ("Appearance", IconName::Palette),
    ("Tools", IconName::Wrench),
];

/// Every project's nav icon.
pub const PROJECT_ICON: IconName = IconName::FolderGit2;

/// One row of the section nav.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavItem {
    pub label: String,
    pub icon: IconName,
    pub page: Page,
    /// A global row's section on the global page.
    pub section: Option<usize>,
    /// The open page, or on the global page the section clicked or scrolled to: accent-soft fill, accent icon,
    /// semibold primary text. Only one row is on.
    pub on: bool,
    /// A flagged value: the red alert icon after the label.
    pub alert: bool,
}

/// One project's page data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPage {
    pub info: ProjectInfo,
    pub settings: ProjectSettings,
    pub problems: Vec<SettingProblem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsModel {
    pub global: GlobalSettings,
    pub global_problems: Vec<SettingProblem>,
    pub projects: Vec<ProjectPage>,
}

impl SettingsModel {
    pub fn project(&self, key: &str) -> Option<&ProjectPage> {
        self.projects.iter().find(|p| p.info.key == key)
    }

    /// Replaces (or adds) a project's page after loading or saving it.
    pub fn set_project(&mut self, page: ProjectPage) {
        match self
            .projects
            .iter_mut()
            .find(|p| p.info.key == page.info.key)
        {
            Some(slot) => *slot = page,
            None => self.projects.push(page),
        }
    }

    /// Whether the nav marks a project (it has a flagged value).
    pub fn needs_attention(&self, key: &str) -> bool {
        self.project(key).is_some_and(|p| !p.problems.is_empty())
    }

    /// The nav: the global sections (all on the one global page; `section` is the one on), then every project.
    pub fn nav_items(&self, page: &Page, section: usize, projects: &[ProjectInfo]) -> Vec<NavItem> {
        let globals = GLOBAL_SECTIONS
            .iter()
            .enumerate()
            .map(|(i, (label, icon))| NavItem {
                label: (*label).to_owned(),
                icon: *icon,
                page: Page::Global,
                section: Some(i),
                on: *page == Page::Global && i == section,
                alert: false,
            });
        let projects = projects.iter().map(|p| {
            let target = Page::Project(p.key.clone());
            NavItem {
                label: p.name.clone(),
                icon: PROJECT_ICON,
                on: *page == target,
                page: target,
                section: None,
                alert: self.needs_attention(&p.key),
            }
        });
        globals.chain(projects).collect()
    }
}

/// The global section in view: the last one starting at or above the top visible row (`starts` are each
/// section's first row). At the very bottom the last section is in view, though it may never reach the top.
pub fn section_in_view(starts: &[usize], top_row: usize, at_bottom: bool) -> usize {
    if at_bottom {
        return starts.len().saturating_sub(1);
    }
    starts.iter().rposition(|s| *s <= top_row).unwrap_or(0)
}

/// The message for a field, if the service flagged it.
pub fn problem_for(problems: &[SettingProblem], field: &str) -> Option<String> {
    problems
        .iter()
        .find(|p| p.field == field)
        .map(|p| p.message.clone())
}

/// The message for one env file, if it was flagged.
pub fn env_file_problem(problems: &[SettingProblem], file: &str) -> Option<String> {
    problems
        .iter()
        .filter(|p| p.field == "env_files")
        .find(|p| {
            p.message.starts_with(&format!("{file} "))
                || p.message
                    .starts_with(&format!("Can\u{2019}t find {file} in "))
        })
        .map(|p| p.message.clone())
}

/// The database file and logs folder as shown in Data (d0055), with `~` for the home folder. The folder ends in the
/// platform's own separator (`logs\` on Windows, not `logs/`).
pub fn data_paths(data: &DataDir) -> (String, String) {
    (
        tilde(&data.database_path().display().to_string()),
        tilde(&format!(
            "{}{}",
            data.logs_dir().display(),
            std::path::MAIN_SEPARATOR
        )),
    )
}

/// Reads the terminal font size field.
pub fn parse_font_size(text: &str) -> Result<u32, String> {
    let range = TERMINAL_FONT_SIZES;
    match text.trim().trim_end_matches("px").trim().parse::<u32>() {
        Ok(n) if range.contains(&n) => Ok(n),
        _ => Err(format!(
            "Use a size from {} to {}",
            range.start(),
            range.end()
        )),
    }
}

/// The Remove project confirmation (PRJ-04): what goes, what is never touched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveText {
    pub title: String,
    pub deleted: Vec<String>,
    pub kept: Vec<String>,
    /// "1 agent is running. Removing stops it first." (agents arrive in M3).
    pub agents: Option<String>,
}

pub fn remove_text(info: &ProjectInfo, tickets: usize) -> RemoveText {
    let product = otaq_core::naming::PRODUCT_NAME;
    RemoveText {
        title: format!("Remove {} from {product}?", info.name),
        deleted: vec![
            format!(
                "{tickets} ticket{}, their runs and comments",
                if tickets == 1 { "" } else { "s" }
            ),
            "Project settings (commands, env files)".into(),
            format!("{product} worktrees and branches — listed in Cleanup first"),
        ],
        kept: vec![
            format!("The repository at {}", tilde(&info.repo_path)),
            "Your own branches, commits and remotes".into(),
            "Open pull requests".into(),
        ],
        agents: (info.agents_working > 0).then(|| {
            format!(
                "{} agent{} running. Removing stops {} first.",
                info.agents_working,
                if info.agents_working == 1 {
                    " is"
                } else {
                    "s are"
                },
                if info.agents_working == 1 {
                    "it"
                } else {
                    "them"
                }
            )
        }),
    }
}

/// How the PR provider row looks (REV-11; *15 → Project · configured + invalid value, PR provider · not detected*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRow {
    /// The text under the row's label.
    pub description: String,
    /// "GitHub (detected from origin) · gh: logged in"; for a remote that isn't detected, the lead "Not detected:".
    pub status: String,
    /// "use your own Open PR skill" (a link to the Open PR field) when nothing was detected.
    pub link: Option<&'static str>,
    pub tone: ProviderTone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTone {
    /// Detected and logged in.
    Ready,
    /// Detected, but its CLI is missing or logged out.
    Attention,
    /// Not detected.
    NotDetected,
}

/// The read-only PR provider row of a project page; `None` while it loads.
pub fn provider_row(provider: Option<&otaq_core::pr::provider::PrProvider>) -> ProviderRow {
    const DESCRIPTION: &str = "Detected from the origin remote, used by your Open PR skill. Not a setting: change the remote, or use your own Open PR skill.";
    let Some(p) = provider else {
        return ProviderRow {
            description: DESCRIPTION.to_owned(),
            status: "Checking…".to_owned(),
            link: None,
            tone: ProviderTone::Attention,
        };
    };
    match p.provider {
        Some(_) => ProviderRow {
            description: DESCRIPTION.to_owned(),
            status: p.summary(),
            link: None,
            tone: if p.cli_found && p.logged_in == Some(true) {
                ProviderTone::Ready
            } else {
                ProviderTone::Attention
            },
        },
        None => ProviderRow {
            description: match &p.remote {
                Some(remote) => {
                    format!("origin is {remote} \u{b7} no gh or az provider matches.")
                }
                None => "This repository has no origin remote \u{b7} no gh or az provider matches."
                    .to_owned(),
            },
            status: "Not detected:".to_owned(),
            link: Some("use your own Open PR skill"),
            tone: ProviderTone::NotDetected,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::naming::PRODUCT_NAME;
    use otaq_core::pr::provider::{PrProvider, Provider};

    #[test]
    fn provider_rows() {
        let mut p = PrProvider {
            remote: Some("dev.azure.com/acme/billing/_git/billing-api".into()),
            provider: Some(Provider::AzureDevOps),
            logged_in: Some(true),
            cli_found: true,
            open_pr_skill: "/my-pr".into(),
        };
        let row = provider_row(Some(&p));
        assert_eq!(
            row.status,
            "Azure DevOps (detected from origin) \u{b7} az: signed in"
        );
        assert_eq!(row.tone, ProviderTone::Ready);
        assert!(
            row.description
                .starts_with("Detected from the origin remote")
        );
        p.logged_in = Some(false);
        assert_eq!(provider_row(Some(&p)).tone, ProviderTone::Attention);
        p.provider = None;
        p.remote = Some("git.acme.internal/web/handbook".into());
        let row = provider_row(Some(&p));
        assert_eq!(row.status, "Not detected:");
        assert_eq!(row.link, Some("use your own Open PR skill"));
        assert_eq!(
            row.description,
            "origin is git.acme.internal/web/handbook \u{b7} no gh or az provider matches."
        );
        assert_eq!(provider_row(None).status, "Checking…");
    }

    fn info(working: u32) -> ProjectInfo {
        ProjectInfo {
            key: "BILL".into(),
            name: "billing-api".into(),
            repo_path: "/code/billing-api".into(),
            base_branch: "develop".into(),
            agents_working: working,
            needs_you: 0,
            flow_setup_since: None,
            paused_since: None,
            paused_by: None,
        }
    }

    #[test]
    fn flagged_values_show_per_field_and_mark_the_nav() {
        let problems = vec![
            SettingProblem {
                field: "env_files".into(),
                message: "Can’t find .env.local in ~/code/shop-web. Add it to the repo root, or remove it from this list.".into(),
            },
            SettingProblem {
                field: "open_url".into(),
                message: "localhost isn't a URL".into(),
            },
        ];
        assert_eq!(
            env_file_problem(&problems, ".env.local").as_deref(),
            Some(
                "Can’t find .env.local in ~/code/shop-web. Add it to the repo root, or remove it from this list."
            )
        );
        assert_eq!(env_file_problem(&problems, "appsettings.json"), None);
        assert_eq!(
            problem_for(&problems, "open_url").as_deref(),
            Some("localhost isn't a URL")
        );
        let mut model = SettingsModel::default();
        model.set_project(ProjectPage {
            info: info(0),
            settings: ProjectSettings::default(),
            problems,
        });
        assert!(model.needs_attention("BILL"));
        model.set_project(ProjectPage {
            info: info(0),
            settings: ProjectSettings::default(),
            problems: Vec::new(),
        });
        assert_eq!(model.projects.len(), 1, "replaced, not added");
        assert!(!model.needs_attention("BILL"));
    }

    #[test]
    fn every_nav_item_has_its_icon() {
        let mut model = SettingsModel::default();
        model.set_project(ProjectPage {
            info: info(0),
            settings: ProjectSettings::default(),
            problems: vec![SettingProblem {
                field: "open_url".into(),
                message: "localhost isn't a URL".into(),
            }],
        });
        let items = model.nav_items(&Page::Project("BILL".into()), 0, &[info(0)]);
        let icons: Vec<_> = items.iter().map(|i| i.icon.lucide_name()).collect();
        assert_eq!(
            icons,
            [
                "gauge",
                "coins",
                "bell",
                "database",
                "palette",
                "wrench",
                "folder-git-2",
            ]
        );
        let bill = items.last().unwrap();
        assert_eq!(bill.label, "billing-api");
        assert!(bill.on && bill.alert);
        assert!(items[..6].iter().all(|i| !i.on && i.page == Page::Global));
        let items = model.nav_items(&Page::Global, 1, &[info(0)]);
        let on: Vec<_> = items
            .iter()
            .filter(|i| i.on)
            .map(|i| i.label.as_str())
            .collect();
        assert_eq!(
            on,
            ["Usage"],
            "one row is on: the section clicked or in view"
        );
        assert_eq!(items[1].section, Some(1));
        assert_eq!(items[6].section, None);
    }

    #[test]
    fn the_section_in_view_follows_the_scroll() {
        // Rows: limits 0..=4, instructions 5..=6, skills 7..=8, …
        let starts = [0, 5, 7, 9, 12, 14, 17, 20];
        assert_eq!(section_in_view(&starts, 0, false), 0);
        assert_eq!(section_in_view(&starts, 4, false), 0);
        assert_eq!(section_in_view(&starts, 5, false), 1);
        assert_eq!(section_in_view(&starts, 13, false), 4);
        assert_eq!(section_in_view(&starts, 18, false), 6);
        assert_eq!(
            section_in_view(&starts, 18, true),
            7,
            "the bottom shows the last one"
        );
        assert_eq!(section_in_view(&[], 3, false), 0);
    }

    #[test]
    fn load_and_save_helpers() {
        assert_eq!(parse_font_size("15 px"), Ok(15));
        assert!(parse_font_size("99").is_err());
        assert!(parse_font_size("big").is_err());
        let data = DataDir::new("/tmp/home/.x");
        let (db, logs) = data_paths(&data);
        assert!(db.ends_with(".db"));
        assert!(logs.ends_with(&format!("logs{}", std::path::MAIN_SEPARATOR)));
    }

    #[test]
    fn remove_confirmation_says_what_goes_and_what_stays() {
        let t = remove_text(&info(0), 2);
        assert_eq!(t.title, format!("Remove billing-api from {PRODUCT_NAME}?"));
        assert_eq!(t.deleted[0], "2 tickets, their runs and comments");
        assert_eq!(t.kept[0], "The repository at /code/billing-api");
        assert_eq!(t.agents, None);
        assert_eq!(
            remove_text(&info(1), 1).agents.as_deref(),
            Some("1 agent is running. Removing stops it first.")
        );
    }
}

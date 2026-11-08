//! The project's Settings page (`project_settings.view`): everything about this repository on one page in the
//! project's main area, opened from the view switch's Settings. A section list on the left (it follows the scroll
//! and jumps on a click) and one card per section, in this order: Flow (the strip, Export, Edit flow; drawn from
//! `settings.flow`, [`super::flow_settings`]), Repository (name, base branch, the flow's delivery, the PR provider,
//! the merge check), Run (the apps), Workspace (setup command, the variables the user sets, files to copy, ticket
//! resources, finished tickets, warm build), Notes for agents, Remove project. App settings (you and your machine)
//! are elsewhere, under the rail's gear.
//!
//! The rows are [`super::settings_page`]'s, and an edit goes through [`super::settings_page::edit_project`], except
//! Deliver as: the delivery is the flow's own setting, so changing it here writes a new flow version ([`deliver_as`]),
//! as the flow editor does. The PR provider is a fact of the repository, not of the flow: one line, detected from
//! the remote ([`provider`]).

use otaq_core::board::settings::{ProjectSettings, SettingProblem};
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::flow::model::{Delivers, Delivery, DeliveryMode};
use otaq_core::naming::PRODUCT_NAME;
use otaq_core::pr::provider::PrProvider;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::shell::tilde;
use crate::view_models::settings::problem_for;
use crate::view_models::settings_page::{
    ButtonAction, Choice, Control, Page, ProjectInputs, ProjectSection, Row, Section, app_rows,
    env_vars_row, merge_check_row, multiline, row, text, ticket_resource_rows, warm_builds_row,
};

/// One entry of the section list on the left.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ProjectSettingsNavItem")]
pub struct NavItem {
    pub id: ProjectSection,
    pub label: String,
}

/// The whole page (`project_settings.view`): the project's name and folder over the section list, the list, the
/// line under it, and the cards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ProjectSettingsView")]
pub struct View {
    pub key: String,
    pub name: String,
    /// "~/code/shop-web".
    pub path: String,
    pub nav: Vec<NavItem>,
    /// "App-wide settings (agents, notifications, appearance) are under the gear on the rail."
    pub app_settings: String,
    /// The cards, as a project page of rows (the Flow card's rows come from `settings.flow`).
    pub page: Page,
}

/// The section list's words, in the page's order.
pub fn nav_label(id: ProjectSection) -> &'static str {
    match id {
        ProjectSection::Flow => "Flow",
        ProjectSection::Repository => "Repository",
        ProjectSection::Run => "Run",
        ProjectSection::Workspace => "Workspace",
        ProjectSection::ProjectNotes => "Notes for agents",
        ProjectSection::RemoveProject => "Remove project",
    }
}

pub const APP_SETTINGS_LINE: &str =
    "App-wide settings (agents, notifications, appearance) are under the gear on the rail.";

/// The page from the project's settings, its flow's delivery and the rest of [`ProjectInputs`].
pub fn view(input: &ProjectInputs) -> View {
    let page = page(input);
    let nav = match &page {
        Page::Project { sections, .. } => sections
            .iter()
            .map(|s| NavItem {
                id: s.id,
                label: nav_label(s.id).to_owned(),
            })
            .collect(),
        Page::Global { .. } => Vec::new(),
    };
    View {
        key: input.info.key.clone(),
        name: input.info.name.clone(),
        path: tilde(&input.info.repo_path),
        nav,
        app_settings: APP_SETTINGS_LINE.to_owned(),
        page,
    }
}

fn card(id: ProjectSection, intro: &str, rows: Vec<Row>) -> Section<ProjectSection> {
    Section {
        id,
        title: Some(nav_label(id).to_owned()),
        intro: (!intro.is_empty()).then(|| intro.to_owned()),
        rows,
        note: None,
    }
}

/// The cards, in the page's order.
pub fn page(input: &ProjectInputs) -> Page {
    let (info, s, p) = (input.info, input.settings, input.problems);
    let flow = card(
        ProjectSection::Flow,
        "The stages every ticket goes through, and who works in each. Backlog and Done are fixed.",
        Vec::new(),
    );
    let repository = card(
        ProjectSection::Repository,
        "Where agents start, how work leaves, and what runs before a merge.",
        vec![
            row(
                "name",
                "Name",
                "",
                text(&info.name, "Project name", false, None),
            ),
            row(
                "base_branch",
                "Base branch",
                "Agents start here; work merges here.",
                Control::Branch {
                    value: info.base_branch.clone(),
                    search: crate::view_models::menu::MenuSearch::branches(),
                },
            ),
            delivery_row(input.delivery, &info.base_branch),
            row(
                "",
                PROVIDER_LABEL,
                "Providers are set up once per computer, in Settings → Git & pull requests.",
                Control::Provider,
            ),
            merge_check_row(s, p),
        ],
    );
    let run = card(
        ProjectSection::Run,
        "How a ticket\u{2019}s copy of your app starts, for you and for the checkers.",
        app_rows(s, p, input.tool_connected, input.project_agent),
    );
    let workspace = card(
        ProjectSection::Workspace,
        "What each ticket\u{2019}s own copy of the repo gets before its agent starts.",
        workspace_rows(info, s, p, input),
    );
    let notes = card(
        ProjectSection::ProjectNotes,
        &format!("Every agent working on {} reads these first.", info.name),
        vec![row(
            "project_notes",
            "Notes",
            "Added to every agent\u{2019}s instructions here, whatever stage it runs.",
            multiline(
                &s.project_notes,
                "Notes for every agent here, e.g. \u{201c}Run the tests with make test.\u{201d}",
            ),
        )],
    );
    let remove = card(
        ProjectSection::RemoveProject,
        "",
        vec![row(
            "",
            "Remove project",
            format!(
                "Removes {} from {PRODUCT_NAME}: its tickets, copies and branches made by {PRODUCT_NAME}. Your \
                 repository and your own branches aren\u{2019}t touched.",
                info.name
            ),
            Control::Button {
                label: format!("Remove {}\u{2026}", info.name),
                icon: IconName::Trash2,
                destructive: true,
                action: ButtonAction::RemoveProject,
            },
        )],
    );
    Page::Project {
        key: info.key.clone(),
        title: info.name.clone(),
        intro: String::new(),
        sections: vec![flow, repository, run, workspace, notes, remove],
    }
}

/// Workspace: the setup command, the variables the user sets (none are read from env files), the files to copy
/// (named, never read or shown), the ticket resources, finished tickets and the warm build.
fn workspace_rows(
    info: &ProjectInfo,
    s: &ProjectSettings,
    p: &[SettingProblem],
    input: &ProjectInputs,
) -> Vec<Row> {
    let mut rows = vec![
        row(
            "setup_command",
            "Setup command",
            "Runs once in each new copy, before the agent starts.",
            text(
                &s.setup_command,
                "none \u{2014} e.g. pnpm install",
                true,
                problem_for(p, "setup_command"),
            ),
        ),
        env_vars_row(s, p),
        files_to_copy_row(s, p),
    ];
    rows.extend(ticket_resource_rows(s, p));
    rows.push(row(
        "let_finished_tickets_answer",
        "Finished tickets",
        format!(
            "Let finished tickets answer questions about their work: when an agent asks a Done ticket something, \
             {PRODUCT_NAME} resumes its session answer-only (it changes nothing). Off: those questions come to you."
        ),
        Control::Toggle {
            on: s.let_finished_tickets_answer,
        },
    ));
    rows.push(warm_builds_row(info, s, input.warm));
    rows
}

/// "Files to copy": `.env.local` and the like, copied into each copy by name. Otaq never reads them or shows what's
/// in them.
fn files_to_copy_row(s: &ProjectSettings, p: &[SettingProblem]) -> Row {
    use crate::view_models::settings::env_file_problem;
    use crate::view_models::settings_page::EnvFileRow;
    row(
        "env_files",
        "Files to copy",
        format!(
            "Copied from your folder into each copy, like .env.local. Never committed; {PRODUCT_NAME} never reads \
             or shows what\u{2019}s in them."
        ),
        Control::EnvFiles {
            rows: s
                .env_files
                .iter()
                .map(|f| EnvFileRow {
                    value: f.clone(),
                    problem: env_file_problem(p, f),
                })
                .collect(),
            add_placeholder: "Add file".into(),
            remove_label: "Remove file".into(),
        },
    )
}

/// Deliver as's fields.
pub const DELIVERS_FIELD: &str = "delivery.delivers";
pub const MODE_FIELD: &str = "delivery.mode";
pub const DELIVERY_LABEL: &str = "Delivery";
pub const DELIVERY_NOTE: &str =
    "This is the flow\u{2019}s setting: changing it here changes the flow, and the flow file.";
pub const EITHER_NOTE: &str = "With Either, you choose at Approve.";
pub const PROVIDER_LABEL: &str = "Pull requests through";

/// The Delivery row: the flow's `delivery:` shown as the flow's own (Merge into <base> · Pull request · Either; On
/// Approve · Automatically). Either is chosen at your click, so with Either the modes stay on On Approve, switched off,
/// and a line says why. A flow that doesn't read
/// shows why instead.
pub fn delivery_row(delivery: Option<&Delivery>, base: &str) -> Row {
    let description = "How work leaves the flow.";
    let Some(d) = delivery else {
        return row(
            "",
            DELIVERY_LABEL,
            description,
            Control::Note {
                text: "The flow doesn\u{2019}t read: open Edit flow to fix it.".to_owned(),
            },
        );
    };
    let choice = |id: &str, label: &str| Choice {
        id: id.to_owned(),
        label: label.to_owned(),
    };
    let either = d.delivers == Delivers::Either;
    row(
        "delivery",
        DELIVERY_LABEL,
        description,
        Control::Delivery {
            delivers: vec![
                choice("local", &format!("Merge into {base}")),
                choice("pr", "Pull request"),
                choice("either", "Either"),
            ],
            delivers_value: delivers_id(d.delivers).to_owned(),
            modes: vec![
                choice("click", "On Approve"),
                choice("automatic", "Automatically"),
            ],
            mode_value: d.mode.as_str().to_owned(),
            modes_off: either,
            either: either.then(|| EITHER_NOTE.to_owned()),
            note: DELIVERY_NOTE.to_owned(),
        },
    )
}

fn delivers_id(d: Delivers) -> &'static str {
    match d {
        Delivers::Local => "local",
        Delivers::Pr => "pr",
        Delivers::Either => "either",
    }
}

/// The flow's delivery with Deliver as's edit `field` = `value` (`delivery.delivers`: local / pr / either;
/// `delivery.mode`: click / automatic). A local merge opens no PR, so it drops the PR's skill but keeps the agent, which
/// writes its squash message (and opens the PR again on a later switch back); Either is chosen at your click, so
/// picking it sets On Approve, and Automatically isn't offered with it.
pub fn deliver_as(delivery: &Delivery, field: &str, value: &str) -> Result<Delivery, String> {
    let mut out = delivery.clone();
    match field {
        DELIVERS_FIELD => {
            out.delivers = match value {
                "local" => Delivers::Local,
                "pr" => Delivers::Pr,
                "either" => Delivers::Either,
                other => return Err(format!("not a delivery: {other}")),
            };
            if out.delivers == Delivers::Local {
                out.skill = None;
            }
            if out.delivers == Delivers::Either {
                out.mode = DeliveryMode::Click;
            }
        }
        MODE_FIELD => {
            out.mode = match value {
                "click" => DeliveryMode::Click,
                "automatic" => DeliveryMode::Automatic,
                other => return Err(format!("not a delivery mode: {other}")),
            };
            if out.mode == DeliveryMode::Automatic && out.delivers == Delivers::Either {
                return Err(
                    "Either is chosen at your click: pick Merge or Pull request to deliver automatically".to_owned(),
                );
            }
        }
        other => return Err(format!("unknown setting: {other}")),
    }
    Ok(out)
}

/// Whether `field` is one of Deliver as's: the flow's, not the project's settings.
pub fn is_delivery_field(field: &str) -> bool {
    field == DELIVERS_FIELD || field == MODE_FIELD
}

/// How the provider line looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ProviderLook")]
pub enum ProviderLook {
    Ready,
    Attention,
    NotDetected,
}

/// The Repository card's PR provider line (`settings.provider`): "GitHub · acme/shop-web", with how its tool
/// stands on this computer and "Set up…" when it isn't ready here. Detected from the remote: a fact of the
/// repository, not a setting and not the flow's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SettingsProvider")]
pub struct ProviderView {
    /// "GitHub · acme/shop-web"; "None detected" when no provider matches the remote.
    pub line: String,
    /// "gh: signed in", "gh isn't installed", "origin is git.acme.internal/web/handbook".
    pub status: String,
    pub icon: IconName,
    pub look: ProviderLook,
    /// "Set up…" when the provider's tool isn't ready on this computer: it opens app Settings where it is set up.
    pub set_up: Option<String>,
}

pub const SET_UP: &str = "Set up\u{2026}";

/// The provider line from what was detected; `None` while it is being checked.
pub fn provider(detected: Option<&PrProvider>) -> ProviderView {
    let Some(p) = detected else {
        return ProviderView {
            line: "Checking\u{2026}".to_owned(),
            status: String::new(),
            icon: IconName::CircleDashed,
            look: ProviderLook::Attention,
            set_up: None,
        };
    };
    let Some(provider) = p.provider else {
        return ProviderView {
            line: "None detected".to_owned(),
            status: match &p.remote {
                Some(remote) => format!("origin is {remote}: no GitHub or Azure DevOps repository"),
                None => "This repository has no origin remote".to_owned(),
            },
            icon: IconName::CircleQuestionMark,
            look: ProviderLook::NotDetected,
            set_up: None,
        };
    };
    let repo = p.remote.as_deref().map(repository_path).unwrap_or_default();
    let line = if repo.is_empty() {
        provider.name().to_owned()
    } else {
        format!("{} \u{b7} {repo}", provider.name())
    };
    let cli = provider.tool().command();
    let (status, ready) = match (p.cli_found, p.logged_in) {
        (false, _) => (format!("{cli} isn\u{2019}t installed"), false),
        (true, Some(true)) => (format!("{cli}: signed in"), true),
        (true, Some(false)) => (format!("{cli}: not signed in"), false),
        (true, None) => (format!("{cli}: sign-in unknown"), false),
    };
    ProviderView {
        line,
        status,
        icon: if ready {
            IconName::CircleCheck
        } else {
            IconName::TriangleAlert
        },
        look: if ready {
            ProviderLook::Ready
        } else {
            ProviderLook::Attention
        },
        set_up: (!ready).then(|| SET_UP.to_owned()),
    }
}

/// "acme/shop-web" from "github.com/acme/shop-web": the remote without its host.
fn repository_path(remote: &str) -> String {
    remote
        .split_once('/')
        .map_or(remote, |(_, path)| path)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::board::settings::GlobalSettings;
    use otaq_core::pr::provider::Provider;

    fn info() -> ProjectInfo {
        ProjectInfo {
            key: "SHOP".into(),
            name: "shop-web".into(),
            repo_path: "/code/shop-web".into(),
            base_branch: "main".into(),
            agents_working: 0,
            needs_you: 0,
            flow_setup_since: None,
            paused_since: None,
            paused_by: None,
        }
    }

    fn page_of(delivery: Option<&Delivery>) -> View {
        let settings = ProjectSettings::default();
        let global = GlobalSettings::default();
        view(&ProjectInputs {
            info: &info(),
            settings: &settings,
            problems: &[],
            global: &global,
            warm: None,
            project_agent: "Noyan",
            tool_connected: true,
            delivery,
        })
    }

    fn sections(v: &View) -> &[Section<ProjectSection>] {
        match &v.page {
            Page::Project { sections, .. } => sections,
            Page::Global { .. } => panic!("a project page"),
        }
    }

    #[test]
    fn the_page_has_its_cards_in_order_with_the_section_list() {
        let v = page_of(Some(&Delivery::default()));
        let labels: Vec<&str> = v.nav.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Flow",
                "Repository",
                "Run",
                "Workspace",
                "Notes for agents",
                "Remove project"
            ]
        );
        assert_eq!(v.path, "/code/shop-web");
        let repository: Vec<&str> = sections(&v)[1]
            .rows
            .iter()
            .map(|r| r.label.as_str())
            .collect();
        assert_eq!(
            repository,
            [
                "Name",
                "Base branch",
                "Delivery",
                "Pull requests through",
                "Merge check \u{b7} optional"
            ]
        );
        let workspace: Vec<&str> = sections(&v)[3]
            .rows
            .iter()
            .map(|r| r.field.as_str())
            .collect();
        assert_eq!(workspace[..3], ["setup_command", "env_vars", "env_files"]);
        assert!(workspace.contains(&"resource_create"));
        assert_eq!(
            workspace[workspace.len() - 2..],
            ["let_finished_tickets_answer", "warm_builds"]
        );
        assert_eq!(sections(&v)[1].title.as_deref(), Some("Repository"));
    }

    #[test]
    fn files_to_copy_are_named_and_never_read() {
        let v = page_of(None);
        let files = sections(&v)[3]
            .rows
            .iter()
            .find(|r| r.field == "env_files")
            .unwrap();
        assert_eq!(files.label, "Files to copy");
        assert!(files.description.contains("never reads or shows"));
    }

    #[test]
    fn the_delivery_row_shows_the_flows_setting() {
        let pr = Delivery {
            delivers: Delivers::Pr,
            mode: DeliveryMode::Automatic,
            agent: Some("Shipper".into()),
            skill: Some("open-pr".into()),
        };
        let row = delivery_row(Some(&pr), "develop");
        let Control::Delivery {
            delivers,
            delivers_value,
            modes,
            mode_value,
            modes_off,
            either,
            note,
        } = row.control
        else {
            panic!("{row:?}")
        };
        let labels: Vec<&str> = delivers.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Merge into develop", "Pull request", "Either"]);
        assert_eq!(
            (delivers_value.as_str(), mode_value.as_str()),
            ("pr", "automatic")
        );
        assert_eq!(modes.len(), 2);
        assert!(!modes_off);
        assert_eq!(either, None);
        assert_eq!(note, DELIVERY_NOTE);
        // Either is chosen at your click: the modes show On Approve, switched off, and the line says why.
        let either_row = delivery_row(
            Some(&Delivery {
                delivers: Delivers::Either,
                ..Delivery::default()
            }),
            "main",
        );
        let Control::Delivery {
            modes,
            mode_value,
            modes_off,
            either,
            ..
        } = either_row.control
        else {
            panic!()
        };
        let labels: Vec<&str> = modes.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["On Approve", "Automatically"]);
        assert_eq!(mode_value, "click");
        assert!(modes_off);
        assert_eq!(either.as_deref(), Some(EITHER_NOTE));
        // A flow that doesn't read.
        assert!(matches!(
            delivery_row(None, "main").control,
            Control::Note { .. }
        ));
    }

    #[test]
    fn deliver_as_changes_the_flows_delivery() {
        let pr = Delivery {
            delivers: Delivers::Pr,
            mode: DeliveryMode::Automatic,
            agent: Some("Shipper".into()),
            skill: Some("open-pr".into()),
        };
        // A local merge opens no PR: no skill, but its agent stays to write the squash message.
        let local = deliver_as(&pr, DELIVERS_FIELD, "local").unwrap();
        assert_eq!(
            (
                local.delivers,
                local.mode,
                local.agent.as_deref(),
                local.skill.as_deref()
            ),
            (
                Delivers::Local,
                DeliveryMode::Automatic,
                Some("Shipper"),
                None
            )
        );
        // So switching back to Either or a PR still has the agent that opens it.
        let back = deliver_as(&local, DELIVERS_FIELD, "either").unwrap();
        assert_eq!(back.agent.as_deref(), Some("Shipper"));
        // Either is chosen at your click.
        let either = deliver_as(&pr, DELIVERS_FIELD, "either").unwrap();
        assert_eq!(
            (either.delivers, either.mode),
            (Delivers::Either, DeliveryMode::Click)
        );
        assert_eq!(either.agent.as_deref(), Some("Shipper"));
        assert!(deliver_as(&either, MODE_FIELD, "automatic").is_err());
        let click = deliver_as(&pr, MODE_FIELD, "click").unwrap();
        assert_eq!(
            (click.delivers, click.mode),
            (Delivers::Pr, DeliveryMode::Click)
        );
        assert!(deliver_as(&pr, DELIVERS_FIELD, "carrier pigeon").is_err());
        assert!(is_delivery_field(MODE_FIELD) && !is_delivery_field("merge_check"));
    }

    #[test]
    fn the_provider_line() {
        let mut p = PrProvider {
            remote: Some("github.com/acme/shop-web".into()),
            provider: Some(Provider::GitHub),
            logged_in: Some(true),
            cli_found: true,
            open_pr_skill: "open-pr".into(),
        };
        let v = provider(Some(&p));
        assert_eq!(v.line, "GitHub \u{b7} acme/shop-web");
        assert_eq!(v.status, "gh: signed in");
        assert_eq!((v.look, v.set_up), (ProviderLook::Ready, None));
        // Not ready on this computer: Set up….
        p.logged_in = Some(false);
        let v = provider(Some(&p));
        assert_eq!(v.status, "gh: not signed in");
        assert_eq!(v.set_up.as_deref(), Some(SET_UP));
        p.cli_found = false;
        assert_eq!(provider(Some(&p)).status, "gh isn\u{2019}t installed");
        // No provider matches the remote.
        p.provider = None;
        p.remote = Some("git.acme.internal/web/handbook".into());
        let v = provider(Some(&p));
        assert_eq!(v.line, "None detected");
        assert_eq!(v.look, ProviderLook::NotDetected);
        assert_eq!(v.set_up, None);
        assert_eq!(provider(None).line, "Checking\u{2026}");
    }
}

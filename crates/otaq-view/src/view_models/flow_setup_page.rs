//! A later project's **Flow setup page** (T-0436; ASK-04, d0108 "Each connected tool's agent looks for itself, when
//! the user asks"; FED-09, d0115): a project added after another one's flow is settled shows this page instead of its
//! board until its flow is settled (Use this flow or Skip, use the default). Its body is the flow editor from Otaq
//! default, as onboarding's Flow step (`flow_editor.rs`), so this model only says when the page shows and what the
//! title bar and the sidebar call it:
//!
//! - **Title bar:** "billing-api · Flow setup" with "2 agents at work" (blue, while an agent works and nothing is
//!   proposed yet) or "2 proposals" (green, once one is).
//! - **Sidebar:** the project's tile is marked "Flow setup".

use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::project_agents::first_flow::{FlowSetup, FlowSetupRow};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::theme::Palette;
use crate::view_models::status::PillView;

/// The mark of a project in Flow setup, in the title bar and the sidebar.
pub const FLOW_SETUP: &str = "Flow setup";

/// `onboarding.flow_setup_page {project}`: whether the shown project's main area is its Flow setup page instead of its board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowSetupPageView")]
pub struct Page {
    /// The project asked about (an answer for another one is not drawn).
    pub project: String,
    /// The project is in Flow setup: the page shows, not the board.
    pub shows: bool,
}

/// The page for `project` among `projects`: shown while that project is in Flow setup.
pub fn page(projects: &[ProjectInfo], project: &str) -> Page {
    Page {
        project: project.to_owned(),
        shows: in_flow_setup(projects, project),
    }
}

/// Whether `project` is in Flow setup (a project not in the list is not).
pub fn in_flow_setup(projects: &[ProjectInfo], project: &str) -> bool {
    projects
        .iter()
        .any(|p| p.key == project && p.flow_setup_since.is_some())
}

/// The title bar's part for a project in Flow setup: "Flow setup" after the name, then the status pill, if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "HeaderFlowSetup")]
pub struct Header {
    /// "Flow setup".
    pub label: &'static str,
    /// "2 agents at work" or "2 proposals"; `None` while nothing was asked.
    pub status: Option<PillView>,
}

/// The title bar's part for `setup` (`None`: the flow is settled).
pub fn header(setup: &FlowSetup) -> Option<Header> {
    setup.since?;
    let working = setup
        .tools
        .iter()
        .filter(|t| matches!(t.row, FlowSetupRow::Working { .. }))
        .count();
    Some(Header {
        label: FLOW_SETUP,
        status: status(working, setup.proposals.len()),
    })
}

/// The status pill: the proposals once there is one (the page compares them), else the agents at work, else none.
pub fn status(working: usize, proposals: usize) -> Option<PillView> {
    let counted = |n: usize, one: &str, many: &str| {
        if n == 1 {
            format!("1 {one}")
        } else {
            format!("{n} {many}")
        }
    };
    if proposals > 0 {
        Some(PillView {
            palette: Palette::Approved,
            icon: IconName::Hand,
            label: counted(proposals, "proposal", "proposals"),
        })
    } else if working > 0 {
        Some(PillView {
            palette: Palette::Working,
            icon: IconName::LoaderCircle,
            label: counted(working, "agent at work", "agents at work"),
        })
    } else {
        None
    }
}

/// The sidebar's mark for `project`: "Flow setup" while it is in Flow setup.
pub fn rail_mark(project: &ProjectInfo) -> Option<String> {
    project.flow_setup_since.map(|_| FLOW_SETUP.to_owned())
}

/// shop-web's Flow setup with Claude Code and OpenCode connected (`s14-14`), for the galleries and the tests.
pub mod fixtures {
    use otaq_core::agent_tools::Tool;
    use otaq_core::agent_tools::connected::AgentToolState;
    use otaq_core::project_agents::first_flow::{FlowSetup, FlowSetupRow, FlowSetupTool};

    use crate::view_models::tool_problems::Tools;
    use crate::view_models::tool_problems::fixtures::{connected, status};

    /// 2026-10-21 12:33 UTC.
    pub const NOW: i64 = 1_792_585_980_000;
    /// The flow agents' ids.
    pub const CC: &str = "a-cc";
    pub const OC: &str = "a-oc";
    pub const CLAUDE: Tool = Tool::ALL[0];
    pub const OPENCODE: Tool = Tool::ALL[3];

    /// Claude Code and OpenCode connected, at `NOW`.
    pub fn tools() -> Tools {
        let mut tools = Tools::from_statuses(vec![
            connected(CLAUDE),
            status(Tool::ALL[1], AgentToolState::NotInstalled),
            status(Tool::ALL[2], AgentToolState::NotInstalled),
            connected(OPENCODE),
        ]);
        tools.now = NOW;
        tools
    }

    pub fn working(agent: &str, minutes: i64) -> FlowSetupRow {
        FlowSetupRow::Working {
            agent: agent.into(),
            since: NOW - minutes * 60_000 - 5_000,
        }
    }

    /// shop-web's Flow setup with Claude Code's and OpenCode's rows.
    pub fn setup(claude: FlowSetupRow, opencode: FlowSetupRow) -> FlowSetup {
        FlowSetup {
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            since: Some(NOW - 10 * 60_000),
            tools: vec![
                FlowSetupTool {
                    tool: CLAUDE,
                    row: claude,
                },
                FlowSetupTool {
                    tool: OPENCODE,
                    row: opencode,
                },
            ],
            proposals: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::project_agents::first_flow::FlowSetupRow;

    use super::fixtures::{CC, NOW, OC, setup, working};
    use super::*;

    fn project(key: &str, name: &str, flow_setup: bool) -> ProjectInfo {
        ProjectInfo {
            key: key.into(),
            name: name.into(),
            repo_path: format!("/code/{name}"),
            base_branch: "main".into(),
            agents_working: 0,
            needs_you: 0,
            flow_setup_since: flow_setup.then_some(NOW),
            paused_since: None,
            paused_by: None,
        }
    }

    #[test]
    fn the_page_shows_only_while_the_project_is_in_flow_setup() {
        let projects = [
            project("SHOP", "shop-web", false),
            project("BILL", "billing-api", true),
        ];
        let bill = page(&projects, "BILL");
        assert!(bill.shows);
        assert_eq!(bill.project, "BILL");
        assert!(
            !page(&projects, "SHOP").shows,
            "a settled project shows its board"
        );
        assert!(!page(&projects, "GONE").shows);
    }

    #[test]
    fn title_bar_with_two_agents_at_work_s14_18() {
        let header = header(&setup(working(CC, 1), working(OC, 1))).unwrap();
        assert_eq!(header.label, "Flow setup");
        let pill = header.status.unwrap();
        assert_eq!(pill.label, "2 agents at work");
        assert_eq!(pill.palette, Palette::Working);
        assert_eq!(pill.icon, IconName::LoaderCircle);
    }

    #[test]
    fn title_bar_with_one_agent_and_nothing_asked() {
        let one = header(&setup(working(CC, 1), FlowSetupRow::NotAsked)).unwrap();
        assert_eq!(one.status.unwrap().label, "1 agent at work");
        let none = header(&setup(FlowSetupRow::NotAsked, FlowSetupRow::NotAsked)).unwrap();
        assert_eq!(none.label, "Flow setup");
        assert_eq!(none.status, None, "nothing asked: no pill");
    }

    #[test]
    fn title_bar_with_two_proposals_s14_18() {
        let pill = status(0, 2).unwrap();
        assert_eq!(pill.label, "2 proposals");
        assert_eq!(pill.palette, Palette::Approved);
        assert_eq!(pill.icon, IconName::Hand);
        assert_eq!(
            status(1, 1).unwrap().label,
            "1 proposal",
            "a proposal outranks an agent still working"
        );
    }

    #[test]
    fn a_settled_project_has_no_title_bar_part() {
        let mut settled = setup(working(CC, 1), working(OC, 1));
        settled.since = None;
        assert_eq!(header(&settled), None);
    }

    #[test]
    fn the_sidebar_marks_a_project_in_flow_setup() {
        assert_eq!(
            rail_mark(&project("BILL", "billing-api", true)).as_deref(),
            Some("Flow setup")
        );
        assert_eq!(rail_mark(&project("SHOP", "shop-web", false)), None);
    }
}

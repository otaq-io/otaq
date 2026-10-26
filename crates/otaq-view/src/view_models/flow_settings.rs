//! Settings → Flow, a project's section (FLW-07…FLW-11, d0108; *15 → Row · The project's Flow section*, *Row · Use a
//! preset*): who changed the flow and when, the read-only stage table, the waiting change with Cancel change, and the
//! actions. **Change flow…** opens the flow editor (FED-09, d0115); every other action but Export puts its flow in the
//! editor's draft (`flow.propose_from`) and opens the editor on it. The Use a preset… dialog and the Copy from another
//! project… list are here too, and the toasts of a waiting change applied or cancelled.

use otaq_core::agents::{Agent, AgentList};
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::flow::apply::WaitingChange;
use otaq_core::flow::check::DEFAULT_VALIDATION_ROUNDS;
use otaq_core::flow::model::{
    CheckBy, Column, Delivers, DeliveryMode, Flow, FlowFile, Stage, StageKind, StageOption,
};
use otaq_core::flow::presets::{Preset, presets};
use otaq_core::flow::store::{AppliedBy, FlowVersion};
use otaq_core::naming::{DEFAULT_FLOW_FILE, PRODUCT_NAME};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::agents::{AgentTools, list_model};
use crate::view_models::banners::{CANCEL_CHANGE, tickets_count};
use crate::view_models::menu::MenuItem;
use crate::view_models::project_agent_page::ended_when;
use crate::view_models::toasts::{NewToast, ToastLevel};

/// One row of the stage table (*Row · The project's Flow section*): the stage and its kind ("Check · agent · 3 rounds ·
/// shown in Dev"), who runs it and on which model, and what a ticket may pick instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowSettingsStage")]
pub struct StageRow {
    pub icon: IconName,
    pub name: String,
    pub kind: String,
    /// "Rust developer", "You"; "—" for a stage nobody runs (a Delivery that only merges).
    pub agent: String,
    /// "Claude Code · sonnet"; none for You and for a name no agent has.
    pub model: Option<String>,
    /// "Quick developer", "skip"; "—" with none.
    pub options: String,
}

/// The waiting change's line, with Cancel change (FLW-06): "A flow change is waiting: QA after Security. 8 tickets are
/// still working, and no new ticket starts until they’re done."
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowSettingsWaiting")]
pub struct WaitingLine {
    pub text: String,
    pub cancel_label: String,
}

/// A button of the section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowSettingsButton")]
pub struct Button {
    pub label: String,
    pub icon: Option<IconName>,
    pub disabled: bool,
    /// Why it can't be pressed ("No other project to copy from").
    pub hint: Option<String>,
}

/// The ⋯ menu's items, by id ([`Section::menu`]).
pub const IMPORT: &str = "import";
pub const EXPORT: &str = "export";
pub const BACK: &str = "back";
pub const RESET: &str = "reset";

/// Settings → Flow as drawn (`settings.flow`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FlowSettings")]
pub struct Section {
    pub project: String,
    pub title: String,
    /// "Changed by Noyan · yesterday".
    pub changed: String,
    pub intro: String,
    pub waiting: Option<WaitingLine>,
    /// STAGE · AGENT · MODEL · OPTIONS A TICKET CAN PICK.
    pub columns: Vec<String>,
    /// "Backlog → Ready for Dev → … → Done: always first and last".
    pub ends: String,
    pub rows: Vec<StageRow>,
    /// The applied flow doesn't read (never stored so; a guard): shown in place of the table.
    pub problem: Option<String>,
    /// Change flow…: opens the flow editor on the project's flow.
    pub change: Button,
    pub preset: Button,
    pub copy: Button,
    /// The ⋯ button's name, and its menu: Import flow… · Export flow… · Back to the previous flow · Reset to the
    /// default flow.
    pub menu_label: String,
    pub menu: Vec<MenuItem>,
    /// The save dialog's title and the file name it offers (Export flow…), and the open dialog's title (Import
    /// flow…).
    pub export_title: String,
    pub export_file: String,
    pub import_title: String,
}

/// What the section is built from: the project, its current flow version and how many it has, the waiting change,
/// the agents and the tools (for "Claude Code · sonnet"), the other projects, and the time now.
pub struct Inputs<'a> {
    pub project: &'a ProjectInfo,
    pub current: &'a FlowVersion,
    /// `flow.versions`' length: Back to the previous flow needs a version before the current one.
    pub versions: usize,
    pub waiting: Option<&'a WaitingChange>,
    pub agents: &'a AgentList,
    pub tools: &'a AgentTools,
    pub other_projects: usize,
    pub now_ms: i64,
}

pub const TITLE: &str = "Flow";
pub const INTRO: &str = "The project\u{2019}s columns and who works in each. Change it in the flow editor; nothing reaches the board until you apply.";
pub const ENDS: &str =
    "Backlog \u{2192} Ready for Dev \u{2192} \u{2026} \u{2192} Done: always first and last";
pub const CHANGE_FLOW: &str = "Change flow\u{2026}";
pub const USE_A_PRESET: &str = "Use a preset\u{2026}";
pub const COPY_FROM_ANOTHER_PROJECT: &str = "Copy from another project\u{2026}";
pub const NO_OTHER_PROJECT: &str = "No other project to copy from";
pub const NO_PREVIOUS_FLOW: &str = "This is the first flow the project had";

/// The section from what the service gave.
pub fn section(input: &Inputs) -> Section {
    let file = input.current.file();
    let (rows, problem) = match &file {
        Ok(file) => (rows(file, input.agents, input.tools), None),
        Err(e) => (
            Vec::new(),
            Some(format!("The flow doesn\u{2019}t read: {e}")),
        ),
    };
    let button = |label: &str, icon: Option<IconName>| Button {
        label: label.to_owned(),
        icon,
        disabled: false,
        hint: None,
    };
    let no_previous = input.versions < 2;
    Section {
        project: input.project.key.clone(),
        title: TITLE.to_owned(),
        changed: changed(input.current, input.now_ms),
        intro: INTRO.to_owned(),
        waiting: input.waiting.map(waiting_line),
        columns: ["Stage", "Agent \u{b7} model", "Options a ticket can pick"]
            .map(str::to_owned)
            .to_vec(),
        ends: ENDS.to_owned(),
        rows,
        problem,
        change: button(CHANGE_FLOW, Some(IconName::Workflow)),
        preset: button(USE_A_PRESET, None),
        copy: Button {
            disabled: input.other_projects == 0,
            hint: (input.other_projects == 0).then(|| NO_OTHER_PROJECT.to_owned()),
            ..button(COPY_FROM_ANOTHER_PROJECT, None)
        },
        menu_label: "More flow actions".to_owned(),
        menu: vec![
            MenuItem::new(IMPORT, "Import flow\u{2026}").icon(IconName::FolderInput),
            MenuItem::new(EXPORT, "Export flow\u{2026}").icon(IconName::Upload),
            MenuItem {
                separated: true,
                disabled: no_previous,
                hint: no_previous.then(|| NO_PREVIOUS_FLOW.to_owned()),
                ..MenuItem::new(BACK, "Back to the previous flow").icon(IconName::History)
            },
            MenuItem::new(RESET, "Reset to the default flow").icon(IconName::RotateCcw),
        ],
        export_title: format!("Export {}\u{2019}s flow", input.project.name),
        export_file: export_file(&input.project.name),
        import_title: "Import a flow".to_owned(),
    }
}

/// The file Export flow… offers: "shop-web-flow.yaml".
pub fn export_file(project: &str) -> String {
    let slug: String = project
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "flow.yaml".to_owned()
    } else {
        format!("{slug}-flow.yaml")
    }
}

/// "Changed by Noyan · yesterday", "Changed by you · 11:42", "Otaq’s default flow · 9 Oct" (the flow
/// the project got when it was added).
pub fn changed(current: &FlowVersion, now_ms: i64) -> String {
    let when = ended_when(current.applied_at, now_ms);
    let who = match current.applied_by {
        AppliedBy::Shipped => format!("{PRODUCT_NAME}\u{2019}s default flow"),
        AppliedBy::User => "Changed by you".to_owned(),
        AppliedBy::ProjectAgent => "Changed by Noyan".to_owned(),
    };
    if when.is_empty() {
        who
    } else {
        format!("{who} \u{b7} {when}")
    }
}

/// The waiting change's line in the section (FLW-06; *A change waiting for a quiet board*).
pub fn waiting_line(waiting: &WaitingChange) -> WaitingLine {
    let count = waiting.waiting_for.len();
    let working = if count == 1 {
        "1 ticket is still working".to_owned()
    } else {
        format!("{count} tickets are still working")
    };
    let summary = waiting.summary();
    let what = if summary.is_empty() {
        "A flow change is waiting.".to_owned()
    } else {
        format!("A flow change is waiting: {summary}.")
    };
    WaitingLine {
        text: format!("{what} {working}, and no new ticket starts until they\u{2019}re done."),
        cancel_label: CANCEL_CHANGE.to_owned(),
    }
}

/// The table's rows, one per stage in order.
pub fn rows(file: &FlowFile, agents: &AgentList, tools: &AgentTools) -> Vec<StageRow> {
    let flow = &file.flow;
    flow.stages
        .iter()
        .enumerate()
        .map(|(i, stage)| {
            // An agent check's agents on one line, and their models in the same order (d0130).
            let model_of = |name: &str| {
                find(agents, name).map(|a| list_model(a, tools, agents.default_model.as_ref()))
            };
            let (agent, model) = match (stage.kind, stage.named_agents().next()) {
                (StageKind::Check { by: CheckBy::You }, _) => ("You".to_owned(), None),
                (_, Some(_)) if stage.check_agents().len() > 1 => (
                    stage.agents_line(),
                    stage
                        .check_agents()
                        .iter()
                        .map(|n| model_of(n))
                        .collect::<Option<Vec<_>>>()
                        .map(|m| m.join(" + ")),
                ),
                (_, Some(name)) => (stage.agents_line(), model_of(name)),
                (_, None) => (NONE.to_owned(), None),
            };
            StageRow {
                icon: stage_icon(stage),
                name: stage.name.clone(),
                kind: kind_line(flow, i),
                agent,
                model,
                options: options(stage),
            }
        })
        .collect()
}

const NONE: &str = "\u{2014}";

fn find<'a>(agents: &'a AgentList, name: &str) -> Option<&'a Agent> {
    agents
        .agents
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(name))
}

/// A stage's icon, as its column's (`board.rs`'s `column_look` on a quiet board): a bot, a shield, a person, a merge
/// or a PR.
pub fn stage_icon(stage: &Stage) -> IconName {
    match stage.kind {
        StageKind::Agent => IconName::Bot,
        StageKind::Check { by: CheckBy::Agent } => IconName::ShieldCheck,
        StageKind::Check { by: CheckBy::You } => IconName::UserCheck,
        StageKind::Delivery => match stage.delivery.map(|d| d.delivers) {
            Some(Delivers::Pr) => IconName::GitPullRequest,
            _ => IconName::GitMerge,
        },
    }
}

/// "Agent", "Check · agent · 3 rounds · shown in Dev", "Check · you", "Delivery · PR · automatic", "Delivery · click ·
/// shown in Review".
pub fn kind_line(flow: &Flow, index: usize) -> String {
    let stage = &flow.stages[index];
    let mut parts: Vec<String> = match stage.kind {
        StageKind::Agent => vec!["Agent".into()],
        StageKind::Check { by } => {
            let mut parts = vec!["Check".to_owned(), by.as_str().to_owned()];
            if by == CheckBy::Agent {
                let rounds = stage.rounds.unwrap_or(DEFAULT_VALIDATION_ROUNDS);
                parts.push(if rounds == 1 {
                    "1 round".to_owned()
                } else {
                    format!("{rounds} rounds")
                });
            }
            parts
        }
        StageKind::Delivery => {
            let mut parts = vec!["Delivery".to_owned()];
            if let Some(d) = stage.delivery {
                match d.delivers {
                    Delivers::Local => parts.push("Local merge".into()),
                    Delivers::Pr => parts.push("PR".into()),
                    Delivers::Either => {}
                }
                parts.push(
                    match d.mode {
                        DeliveryMode::Click => "click",
                        DeliveryMode::Automatic => "automatic",
                    }
                    .into(),
                );
            }
            parts
        }
    };
    if stage.column == Column::WithPrevious
        && let Some(owner) = flow
            .column_owner(index)
            .and_then(|i| flow.stages.get(i))
            .filter(|o| o.id != stage.id)
    {
        parts.push(format!("shown in {}", owner.name));
    }
    parts.join(" \u{b7} ")
}

/// "Quick developer", "skip", "Quick developer, skip"; "—" with none.
pub fn options(stage: &Stage) -> String {
    if stage.options.is_empty() {
        return NONE.to_owned();
    }
    stage
        .options
        .iter()
        .map(StageOption::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// One column of a preset's small diagram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PresetColumn")]
pub struct PresetColumn {
    pub icon: IconName,
    pub label: String,
}

/// One preset of Use a preset… (*Row · Use a preset*): its name, how it delivers, one line, its columns and its file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PresetCard")]
pub struct PresetCard {
    pub name: String,
    /// "Done · click", "Done · Local merge, automatic".
    pub delivers: String,
    pub line: String,
    pub columns: Vec<PresetColumn>,
    /// "spec-first.yaml": what Pick sends (`flow.propose_from` with `preset`).
    pub file: String,
}

/// Use a preset… (FLW-08): the four presets in a dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PresetDialog")]
pub struct PresetDialog {
    pub project: String,
    pub title: String,
    pub presets: Vec<PresetCard>,
    pub cancel: String,
}

pub fn preset_dialog(project: &ProjectInfo) -> PresetDialog {
    PresetDialog {
        project: project.key.clone(),
        title: format!("Use a preset for {}", project.name),
        presets: presets().iter().map(preset_card).collect(),
        cancel: "Cancel".to_owned(),
    }
}

/// A preset's card: the diagram is Ready for Dev, each stage with its own column but the Delivery, and Done
/// ([`Preset::columns`]), with the stages' icons.
pub fn preset_card(preset: &Preset) -> PresetCard {
    let file = preset.read();
    let stages = &file.flow.stages;
    let delivery = stages.iter().find(|s| s.is_delivery());
    let mut columns = vec![PresetColumn {
        icon: IconName::Hourglass,
        label: "Ready for Dev".to_owned(),
    }];
    columns.extend(
        stages
            .iter()
            .filter(|s| s.column == Column::Own && !s.is_delivery())
            .map(|s| PresetColumn {
                icon: stage_icon(s),
                label: s.name.clone(),
            }),
    );
    columns.push(PresetColumn {
        icon: delivery.map_or(IconName::GitMerge, stage_icon),
        label: "Done".to_owned(),
    });
    PresetCard {
        name: preset.name.clone(),
        delivers: preset.delivers(),
        line: preset.line.to_owned(),
        columns,
        file: preset.file.to_owned(),
    }
}

/// Reset to the default flow proposes this preset file (FLW-09).
pub const RESET_FILE: &str = DEFAULT_FLOW_FILE;

/// One project Copy from another project… can copy from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowCopySource")]
pub struct CopySource {
    pub key: String,
    pub name: String,
}

/// Copy from another project… (FLW-09): the other projects, by name. No frame draws the list: a menu under the
/// button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FlowCopyMenu")]
pub struct CopyMenu {
    pub title: String,
    pub projects: Vec<CopySource>,
}

pub fn copy_menu(project: &str, projects: &[ProjectInfo]) -> CopyMenu {
    CopyMenu {
        title: "Copy the flow of".to_owned(),
        projects: projects
            .iter()
            .filter(|p| p.key != project)
            .map(|p| CopySource {
                key: p.key.clone(),
                name: p.name.clone(),
            })
            .collect(),
    }
}

/// The toast when a waiting change applies (FLW-06; *Applied, or cancelled*): "shop-web’s flow changed" · "QA after
/// Security is on the board. New tickets start again."
pub fn applied_toast(project: &str, summary: &str) -> NewToast {
    let text = if summary.is_empty() {
        "New tickets start again.".to_owned()
    } else {
        format!("{summary} is on the board. New tickets start again.")
    };
    NewToast::new(
        ToastLevel::Success,
        format!("{project}\u{2019}s flow changed"),
        Some(text),
    )
}

/// The toast's title when Cancel change dropped the waiting change.
pub const FLOW_CHANGE_CANCELLED: &str = "Flow change cancelled";

/// "Flow change cancelled" · "shop-web keeps its flow. New tickets start again."
pub fn cancelled_toast(project: &str) -> NewToast {
    NewToast::new(
        ToastLevel::Success,
        FLOW_CHANGE_CANCELLED,
        Some(format!(
            "{project} keeps its flow. New tickets start again."
        )),
    )
}

/// "8 tickets", for the banner and the section.
pub fn count(waiting: &WaitingChange) -> String {
    tickets_count(waiting.waiting_for.len())
}

#[cfg(test)]
mod tests {
    use otaq_core::agent_tools::Tool;
    use otaq_core::agents::{AgentModel, ModelRef};
    use otaq_core::flow::yaml;

    use super::*;
    use crate::view_models::agents::fixtures;

    fn test_file() -> FlowFile {
        let core = format!("{}-core", otaq_core::naming::COMMAND_NAME);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(core)
            .join("tests/flows/every-kind.yaml");
        yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn info(key: &str, name: &str) -> ProjectInfo {
        ProjectInfo {
            key: key.into(),
            name: name.into(),
            repo_path: String::new(),
            base_branch: "main".into(),
            agents_working: 0,
            needs_you: 0,
            flow_setup_since: None,
            paused_since: None,
            paused_by: None,
        }
    }

    fn project() -> ProjectInfo {
        info("SHOP", "shop-web")
    }

    fn version(yaml: &str, applied_by: AppliedBy, at: i64) -> FlowVersion {
        FlowVersion {
            project: "SHOP".into(),
            version: 2,
            yaml: yaml.to_owned(),
            applied_by,
            agent_name: Some("Noyan".into()),
            preset: None,
            applied_at: at,
            current: true,
        }
    }

    /// The agents of `s15-60`: Rust developer on sonnet, Validator on Codex, Security reviewer on opus.
    fn agents() -> AgentList {
        let on = |tool: Tool, model: &str, name: &str| {
            let mut agent = fixtures::agent(&name.to_lowercase(), name, None);
            agent.model = AgentModel::Tool {
                tool,
                model: Some(model.to_owned()),
            };
            agent
        };
        AgentList {
            agents: vec![
                on(Tool::Claude, "sonnet", "Spec writer"),
                on(Tool::Claude, "sonnet", "Rust developer"),
                on(Tool::Codex, "gpt-5.5", "Validator"),
                on(Tool::Claude, "opus", "Security reviewer"),
                on(Tool::Claude, "haiku", "Shipper"),
            ],
            default_model: Some(ModelRef {
                tool: Tool::Claude,
                model: Some("sonnet".into()),
            }),
            fallbacks: Vec::new(),
        }
    }

    fn tools() -> AgentTools {
        fixtures::tools(&[Tool::Claude, Tool::Codex])
    }

    fn waiting(changes: &[&str], tickets: usize) -> WaitingChange {
        WaitingChange {
            project: "SHOP".into(),
            yaml: String::new(),
            applied_by: AppliedBy::User,
            agent_name: None,
            preset: None,
            changes: changes.iter().map(|c| (*c).to_owned()).collect(),
            since: 0,
            wait: otaq_core::flow::apply::Wait::WholeBoard,
            hold: otaq_core::flow::apply::Hold::READY_FOR_DEV,
            waiting_for: (1..=tickets)
                .map(|n| format!("SHOP-{n}").parse().unwrap())
                .collect(),
        }
    }

    const NOW: i64 = 1_792_000_000_000;

    /// `s15-60`'s table for the test flow: each stage with its kind, its agent and model, its options.
    #[test]
    fn the_table_of_the_test_flow() {
        let text = yaml::write(&test_file());
        let current = version(&text, AppliedBy::ProjectAgent, NOW - 86_400_000);
        let (agents, tools, project) = (agents(), tools(), project());
        let s = section(&Inputs {
            project: &project,
            current: &current,
            versions: 2,
            waiting: None,
            agents: &agents,
            tools: &tools,
            other_projects: 1,
            now_ms: NOW,
        });
        assert_eq!(s.title, "Flow");
        assert_eq!(s.changed, "Changed by Noyan \u{b7} yesterday");
        assert_eq!(
            s.columns,
            ["Stage", "Agent \u{b7} model", "Options a ticket can pick"]
        );
        assert_eq!(
            s.ends,
            "Backlog \u{2192} Ready for Dev \u{2192} \u{2026} \u{2192} Done: always first and last"
        );
        let table: Vec<(&str, &str, &str, Option<&str>, &str)> = s
            .rows
            .iter()
            .map(|r| {
                (
                    r.name.as_str(),
                    r.kind.as_str(),
                    r.agent.as_str(),
                    r.model.as_deref(),
                    r.options.as_str(),
                )
            })
            .collect();
        assert_eq!(
            table,
            [
                (
                    "Product spec",
                    "Agent",
                    "Spec writer",
                    Some("Claude Code \u{b7} sonnet"),
                    "skip"
                ),
                (
                    "Dev",
                    "Agent",
                    "Rust developer",
                    Some("Claude Code \u{b7} sonnet"),
                    "Quick developer"
                ),
                (
                    "Validate",
                    "Check \u{b7} agent \u{b7} 3 rounds \u{b7} shown in Dev",
                    "Validator",
                    Some("Codex \u{b7} gpt-5.5"),
                    "\u{2014}"
                ),
                (
                    "Security",
                    "Check \u{b7} agent \u{b7} 2 rounds",
                    "Security reviewer + Validator",
                    Some("Claude Code \u{b7} opus + Codex \u{b7} gpt-5.5"),
                    "\u{2014}"
                ),
                ("QA", "Check \u{b7} you", "You", None, "skip"),
                (
                    "Done",
                    "Delivery \u{b7} PR \u{b7} automatic",
                    "Shipper",
                    Some("Claude Code \u{b7} haiku"),
                    "\u{2014}"
                ),
            ]
        );
        let icons: Vec<IconName> = s.rows.iter().map(|r| r.icon).collect();
        assert_eq!(
            icons,
            [
                IconName::Bot,
                IconName::Bot,
                IconName::ShieldCheck,
                IconName::ShieldCheck,
                IconName::UserCheck,
                IconName::GitPullRequest
            ]
        );
        assert_eq!(s.waiting, None);
        assert_eq!(s.change.label, "Change flow\u{2026}");
        assert_eq!(s.change.icon, Some(IconName::Workflow));
        assert_eq!(s.preset.label, "Use a preset\u{2026}");
        assert!(!s.copy.disabled);
        let menu: Vec<(&str, &str, bool)> = s
            .menu
            .iter()
            .map(|m| (m.id.as_str(), m.label.as_str(), m.disabled))
            .collect();
        assert_eq!(
            menu,
            [
                ("import", "Import flow\u{2026}", false),
                ("export", "Export flow\u{2026}", false),
                ("back", "Back to the previous flow", false),
                ("reset", "Reset to the default flow", false),
            ]
        );
        assert_eq!(s.export_file, "shop-web-flow.yaml");
    }

    /// The default flow on a new project: Otaq's own, nothing before it to go back to, and no project to copy from.
    #[test]
    fn a_new_project_on_the_default_flow() {
        let current = FlowVersion {
            applied_by: AppliedBy::Shipped,
            agent_name: None,
            ..version(
                otaq_core::naming::DEFAULT_FLOW_YAML,
                AppliedBy::Shipped,
                NOW - 60_000,
            )
        };
        let agents = AgentList {
            agents: Vec::new(),
            default_model: None,
            fallbacks: Vec::new(),
        };
        let (tools, project) = (tools(), project());
        let s = section(&Inputs {
            project: &project,
            current: &current,
            versions: 1,
            waiting: None,
            agents: &agents,
            tools: &tools,
            other_projects: 0,
            now_ms: NOW,
        });
        assert!(
            s.changed
                .starts_with(&format!("{PRODUCT_NAME}\u{2019}s default flow \u{b7} "))
        );
        let kinds: Vec<&str> = s.rows.iter().map(|r| r.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "Agent",
                "Check \u{b7} agent \u{b7} 3 rounds",
                "Check \u{b7} you",
                "Delivery \u{b7} click \u{b7} shown in Review"
            ]
        );
        assert_eq!(s.rows[0].model, None, "an agent the list doesn't have");
        assert!(s.copy.disabled);
        assert_eq!(s.copy.hint.as_deref(), Some(NO_OTHER_PROJECT));
        assert!(s.menu.iter().find(|m| m.id == BACK).unwrap().disabled);
    }

    /// The waiting change's line and Cancel change (*A change waiting for a quiet board*).
    #[test]
    fn the_waiting_line() {
        let line = waiting_line(&waiting(&["QA after Security"], 8));
        assert_eq!(
            line.text,
            "A flow change is waiting: QA after Security. 8 tickets are still working, and no new ticket starts \
             until they\u{2019}re done."
        );
        assert_eq!(line.cancel_label, "Cancel change");
        let both = waiting_line(&waiting(&["QA after Security", "Review can be skipped"], 1));
        assert_eq!(
            both.text,
            "A flow change is waiting: QA after Security \u{b7} Review can be skipped. 1 ticket is still working, \
             and no new ticket starts until they\u{2019}re done."
        );
    }

    /// Use a preset… (*Row · Use a preset*): the four presets with their diagram, line and file.
    #[test]
    fn the_preset_dialog() {
        let d = preset_dialog(&project());
        assert_eq!(d.title, "Use a preset for shop-web");
        let cards: Vec<(&str, &str, &str, &str)> = d
            .presets
            .iter()
            .map(|p| {
                (
                    p.name.as_str(),
                    p.delivers.as_str(),
                    p.line.as_str(),
                    p.file.as_str(),
                )
            })
            .collect();
        let default = format!("{PRODUCT_NAME} default");
        assert_eq!(
            cards,
            [
                (
                    default.as_str(),
                    "Done \u{b7} click",
                    "Today\u{2019}s board",
                    DEFAULT_FLOW_FILE
                ),
                (
                    "Solo autopilot",
                    "Done \u{b7} Local merge, automatic",
                    "Solo projects that trust the checks",
                    "solo-autopilot.yaml"
                ),
                (
                    "Spec first",
                    "Done \u{b7} click",
                    "Plan on the board, then build",
                    "spec-first.yaml"
                ),
                (
                    "Team PR",
                    "Done \u{b7} PR, automatic",
                    "Teams that review on GitHub or GitLab: the PR is the review",
                    "team-pr.yaml"
                ),
            ]
        );
        let spec_first: Vec<(&str, IconName)> = d.presets[2]
            .columns
            .iter()
            .map(|c| (c.label.as_str(), c.icon))
            .collect();
        assert_eq!(
            spec_first,
            [
                ("Ready for Dev", IconName::Hourglass),
                ("Product spec", IconName::Bot),
                ("Tech spec", IconName::Bot),
                ("Milestone plan", IconName::Bot),
                ("Spec review", IconName::UserCheck),
                ("Dev", IconName::Bot),
                ("Validate", IconName::ShieldCheck),
                ("Code review", IconName::UserCheck),
                ("Done", IconName::GitMerge),
            ]
        );
        let team: Vec<&str> = d.presets[3]
            .columns
            .iter()
            .map(|c| c.label.as_str())
            .collect();
        assert_eq!(team, ["Ready for Dev", "Dev", "Validate", "Done"]);
        assert_eq!(
            d.presets[3].columns.last().unwrap().icon,
            IconName::GitPullRequest
        );
        assert_eq!(d.cancel, "Cancel");
    }

    #[test]
    fn copy_lists_the_other_projects() {
        let other = info("BILL", "billing-api");
        let menu = copy_menu("SHOP", &[project(), other]);
        assert_eq!(
            menu.projects,
            [CopySource {
                key: "BILL".into(),
                name: "billing-api".into()
            }]
        );
    }

    /// *Applied, or cancelled*: the two toasts.
    #[test]
    fn the_applied_and_cancelled_toasts() {
        let applied = applied_toast("shop-web", "QA after Security");
        assert_eq!(applied.title, "shop-web\u{2019}s flow changed");
        assert_eq!(
            applied.text.as_deref(),
            Some("QA after Security is on the board. New tickets start again.")
        );
        assert_eq!(push(applied).icon, None, "the success check");
        let cancelled = cancelled_toast("shop-web");
        assert_eq!(cancelled.title, "Flow change cancelled");
        assert_eq!(
            cancelled.text.as_deref(),
            Some("shop-web keeps its flow. New tickets start again.")
        );
        let drawn = push(cancelled);
        assert_eq!(drawn.icon, Some(IconName::Undo2));
        assert_eq!(
            drawn.icon_tone,
            Some(crate::view_models::toasts::ToastIconTone::Secondary)
        );
    }

    fn push(toast: NewToast) -> crate::view_models::toasts::ToastView {
        let mut model = crate::view_models::toasts::ToastModel::default();
        model.push_toast(toast, std::time::Instant::now());
        model.items()[0].view()
    }

    #[test]
    fn the_export_file_name() {
        assert_eq!(export_file("shop-web"), "shop-web-flow.yaml");
        assert_eq!(export_file("Pocket API"), "pocket-api-flow.yaml");
        assert_eq!(export_file("…"), "flow.yaml");
    }
}

//! One agent's page on Settings → Agents (AGM-01, AGM-04; d0108, d0078, d0092; `s15-58` / `s15-59` "One agent",
//! "Deleting an agent that's in use"): its name, its model (the model picker of `s15-30`, with the tool's effort level
//! where the tool has one), its instructions (the Markdown editor), where it is used, and its amber mark (AGT-05). An
//! agent has no role: each stage names its agent, and the agent Noyan uses is one setting on the Agents page. An agent
//! has no skill list: a stage's `skill:` is the only place a skill is picked (T7). A saved agent's edits are saved one change at a time and apply to the next
//! session that starts; New agent and Duplicate fill a draft that **Add agent** saves.
//!
//! The page holds the agent's fields as a [`Draft`]; every change goes through [`apply`], so the rules (a new tool
//! clears the model and the effort) live here.

use otaq_core::agent_tools::Tool;
use otaq_core::agents::{Agent, AgentFields, AgentList, AgentModel, InUse};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::agents::{
    AgentTools, MENU_LABEL, MarkView, NO_DEFAULT, NOTHING, agent_menu, came_from, mark_view,
    short_model, tool_model, used_in,
};
use super::menu::MenuItem;
use super::model_picker::{self, Picker};
use super::tool_avatar::AvatarTool;

/// An agent's fields as the page holds them: a saved agent's, or New agent's and Duplicate's draft.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "AgentDraft")]
pub struct Draft {
    pub name: String,
    pub instructions: String,
    /// The tool, as stored (`claude`); `None`: the default model.
    pub tool: Option<String>,
    /// From the tool's list; `None`: the tool's default.
    pub model: Option<String>,
    pub effort: Option<String>,
}

impl Draft {
    /// A saved agent's fields.
    pub fn of(agent: &Agent) -> Self {
        Self::from_fields(&AgentFields {
            name: agent.name.clone(),
            instructions: agent.instructions.clone(),
            model: agent.model.clone(),
            effort: agent.effort.clone(),
        })
    }

    /// Duplicate's draft (`agents.duplicate`).
    pub fn from_fields(fields: &AgentFields) -> Self {
        let (tool, model) = match &fields.model {
            AgentModel::Default => (None, None),
            AgentModel::Tool { tool, model } => (Some(tool.as_str().to_owned()), model.clone()),
        };
        Self {
            name: fields.name.clone(),
            instructions: fields.instructions.clone(),
            tool,
            model,
            effort: fields.effort.clone(),
        }
    }

    pub fn tool(&self) -> Option<Tool> {
        self.tool.as_deref().and_then(|t| t.parse().ok())
    }

    /// What `agents.add` and `agents.edit` take.
    pub fn fields(&self) -> AgentFields {
        AgentFields {
            name: self.name.clone(),
            instructions: self.instructions.clone(),
            model: match self.tool() {
                None => AgentModel::Default,
                Some(tool) => AgentModel::Tool {
                    tool,
                    model: self.model.clone(),
                },
            },
            effort: self.effort.clone(),
        }
    }
}

/// One change on the page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "field", content = "value", rename_all = "snake_case")]
#[ts(rename = "AgentChange")]
pub enum Change {
    Name(String),
    Instructions(String),
    /// The tool as stored, or `None` for the default model.
    Tool(Option<String>),
    /// From the tool's list; `None`: the tool's default.
    Model(Option<String>),
    /// One of the tool's levels; `None`: the tool's default.
    Effort(Option<String>),
}

/// `draft` with `change`: another tool starts on its default model with no effort (its list and its levels differ).
pub fn apply(mut draft: Draft, change: Change) -> Draft {
    match change {
        Change::Name(name) => draft.name = name,
        Change::Instructions(text) => draft.instructions = text,
        Change::Tool(tool) => {
            if tool != draft.tool {
                draft.tool = tool;
                draft.model = None;
                draft.effort = None;
            }
        }
        Change::Model(model) => draft.model = model,
        Change::Effort(effort) => draft.effort = effort.filter(|e| !e.is_empty()),
    }
    draft
}

/// Which field a refused save is written under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Model,
    Page,
}

/// The field a refusal code belongs to (`name_taken`, `model_not_listed`…).
pub fn field_of(refusal: Option<&str>) -> Field {
    match refusal {
        Some("empty_field" | "bad_name" | "name_taken") => Field::Name,
        Some("model_not_listed" | "bad_effort") => Field::Model,
        _ => Field::Page,
    }
}

/// A refused change: under which field, and the service's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub field: Field,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentNameField")]
pub struct NameField {
    pub label: String,
    pub value: String,
    pub problem: Option<String>,
}

/// One choice of a small select (the tool, the effort, the agent Noyan uses).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentOption")]
pub struct Choice {
    /// What the page sends back; empty for "none" (the default model, the tool's effort).
    pub id: String,
    pub label: String,
    pub selected: bool,
}

pub(crate) fn choice(id: &str, label: &str, selected: bool) -> Choice {
    Choice {
        id: id.to_owned(),
        label: label.to_owned(),
        selected,
    }
}

/// The tool's own effort level (AGM-01): only for a tool that has one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentEffortField")]
pub struct EffortField {
    pub label: String,
    pub value: String,
    pub options: Vec<Choice>,
}

/// Model: the tool (or the default model), the model picker for the tool, the effort.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentModelField")]
pub struct ModelField {
    pub label: String,
    pub tool_label: String,
    /// The tool's closed value: "Claude Code", "Default model".
    pub tool_value: String,
    pub tools: Vec<Choice>,
    /// On the default model, what it is today: "Claude Code · sonnet".
    pub default_now: Option<String>,
    /// The tool's model picker (`s15-30`); none on the default model.
    pub picker: Option<Picker>,
    pub effort: Option<EffortField>,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentInstructionsField")]
pub struct InstructionsField {
    pub label: String,
    pub value: String,
    pub placeholder: String,
    /// The toolbar's source switch (d0078 D9).
    pub markdown_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentUsedIn")]
pub struct UsedIn {
    pub label: String,
    pub items: Vec<String>,
    /// "—" when nothing uses it.
    pub empty: Option<String>,
}

/// New agent's and Duplicate's buttons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentAddButtons")]
pub struct AddButtons {
    pub add_label: String,
    pub cancel_label: String,
}

/// The whole page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "AgentPage")]
pub struct Page {
    /// The saved agent; `None` for a draft.
    pub id: Option<String>,
    /// Its name as saved; "New agent" for a draft.
    pub title: String,
    pub came_from: Option<String>,
    pub back_label: String,
    /// "What this agent reads" on a saved agent (AGR-01).
    pub reads_label: Option<String>,
    pub menu_label: String,
    /// Duplicate, Delete… on a saved agent.
    pub menu: Vec<MenuItem>,
    pub draft: Draft,
    pub name: NameField,
    pub model: ModelField,
    pub instructions: InstructionsField,
    pub used_in: Option<UsedIn>,
    pub mark: Option<MarkView>,
    /// "Edits apply to the next session that starts." on a saved agent.
    pub note: Option<String>,
    pub add: Option<AddButtons>,
    /// A refusal that belongs to no field.
    pub problem: Option<String>,
}

/// What the page is built from.
pub struct Inputs<'a> {
    pub list: &'a AgentList,
    pub tools: &'a AgentTools,
    /// The saved agent shown; `None` for a draft.
    pub id: Option<&'a str>,
    pub draft: &'a Draft,
    pub uses: Option<&'a InUse>,
    pub refused: Option<&'a Refused>,
}

/// The tool's name.
fn tool_name(tool: Tool) -> &'static str {
    AvatarTool::from(tool).name()
}

pub const DEFAULT_MODEL: &str = "Default model";
pub const TOOL_DEFAULT_EFFORT: &str = "Tool default";
pub const NEW_AGENT: &str = "New agent";

/// A refusal as a sentence: "Noyan uses Strong. Pick another agent for Noyan first."
pub fn sentence(text: String) -> String {
    if text.ends_with('.') {
        text
    } else {
        format!("{text}.")
    }
}

fn model_field(input: &Inputs, agent: Option<&Agent>, problem: Option<String>) -> ModelField {
    let (tools, draft) = (input.tools, input.draft);
    let tool = draft.tool();
    let mut on: Vec<Tool> = Tool::ALL
        .iter()
        .copied()
        .filter(|t| tools.is_connected(*t) || Some(*t) == tool)
        .collect();
    on.dedup();
    let mut options = vec![choice("", DEFAULT_MODEL, tool.is_none())];
    options.extend(
        on.iter()
            .map(|t| choice(t.as_str(), tool_name(*t), Some(*t) == tool)),
    );
    let picker = tool.map(|tool| {
        // The mark is for this very model: the hint says the account can't use it.
        let refused = agent
            .and_then(|a| a.mark.as_ref())
            .is_some_and(|m| m.model.is_some() && m.model == draft.model);
        let mut picker = model_picker::picker(
            tool,
            tools.state(tool),
            draft.model.as_deref(),
            refused,
            agent.map(|a| a.name.as_str()),
        );
        // The field shows the model the short way (`s15-58`: "opus").
        if picker.value.no_longer_listed.is_none() {
            picker.value.label = short_model(draft.model.as_deref(), tools.list(tool));
            picker.value.detail = None;
            picker.value.mono = true;
        }
        picker
    });
    let effort = tool
        .map(|t| t.effort_levels())
        .filter(|levels| !levels.is_empty())
        .map(|levels| {
            let picked = draft.effort.as_deref();
            let mut options = vec![choice("", TOOL_DEFAULT_EFFORT, picked.is_none())];
            options.extend(
                levels
                    .iter()
                    .map(|level| choice(level, level, picked == Some(*level))),
            );
            EffortField {
                label: "Effort".to_owned(),
                value: picked.unwrap_or(TOOL_DEFAULT_EFFORT).to_owned(),
                options,
            }
        });
    ModelField {
        label: "Model".to_owned(),
        tool_label: "Tool".to_owned(),
        tool_value: tool.map_or(DEFAULT_MODEL, tool_name).to_owned(),
        tools: options,
        default_now: tool.is_none().then(|| match &input.list.default_model {
            Some(model) => tool_model(model, tools),
            None => NO_DEFAULT.to_owned(),
        }),
        picker,
        effort,
        problem,
    }
}

/// The page.
pub fn page(input: &Inputs) -> Page {
    let agent = input
        .id
        .and_then(|id| input.list.agents.iter().find(|a| a.id == id));
    let draft = input.draft;
    let problem = |field: Field| {
        input
            .refused
            .filter(|r| r.field == field)
            .map(|r| r.message.clone())
    };
    let saved = agent.is_some();
    Page {
        id: agent.map(|a| a.id.clone()),
        title: agent.map_or(NEW_AGENT, |a| a.name.as_str()).to_owned(),
        came_from: agent.map(came_from),
        back_label: "Back to Agents".to_owned(),
        reads_label: saved.then(|| super::agent_reads::TITLE.to_owned()),
        menu_label: MENU_LABEL.to_owned(),
        menu: if saved { agent_menu() } else { Vec::new() },
        draft: draft.clone(),
        name: NameField {
            label: "Name".to_owned(),
            value: draft.name.clone(),
            problem: problem(Field::Name),
        },
        model: model_field(input, agent, problem(Field::Model)),
        instructions: InstructionsField {
            label: "Instructions".to_owned(),
            value: draft.instructions.clone(),
            placeholder: "What this agent does and how, in Markdown".to_owned(),
            markdown_label: "Markdown".to_owned(),
        },
        used_in: agent.map(|_| {
            let items = input.uses.map(used_in).unwrap_or_default();
            UsedIn {
                label: "Used in".to_owned(),
                empty: items.is_empty().then(|| NOTHING.to_owned()),
                items,
            }
        }),
        mark: agent.and_then(|a| a.mark.as_ref()).map(mark_view),
        note: saved.then(|| "Edits apply to the next session that starts.".to_owned()),
        add: (!saved).then(|| AddButtons {
            add_label: "Add agent".to_owned(),
            cancel_label: "Cancel".to_owned(),
        }),
        problem: problem(Field::Page),
    }
}

/// Delete…'s dialog (AGM-01; `s15-58` "Deleting an agent that's in use"): refused with where it is used (a stage, or
/// Noyan), or the plain question when nothing uses it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "AgentDeleteDialog")]
pub enum DeleteDialog {
    Refused {
        title: String,
        text: String,
        ok_label: String,
    },
    Confirm {
        id: String,
        title: String,
        text: String,
        cancel_label: String,
        delete_label: String,
    },
}

/// The dialog for the agent `id`, from `agents.in_use`.
pub fn delete_dialog(id: &str, uses: &InUse) -> DeleteDialog {
    let name = &uses.agent;
    match uses.refusal() {
        Some(why) => DeleteDialog::Refused {
            title: format!("Can\u{2019}t delete {name}"),
            text: sentence(why).replace('\'', "\u{2019}"),
            ok_label: "OK".to_owned(),
        },
        None => DeleteDialog::Confirm {
            id: id.to_owned(),
            title: format!("Delete {name}?"),
            text: format!(
                "Nothing uses {name}. Sessions that ran on it keep its name in their history."
            ),
            cancel_label: "Cancel".to_owned(),
            delete_label: "Delete".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::agents::fixtures::{claude_list, example, on, tools};
    use otaq_core::agents::in_use::StageUse;

    fn page_of(list: &AgentList, tools: &AgentTools, id: &str, uses: &[InUse]) -> Page {
        let agent = list.agents.iter().find(|a| a.id == id).unwrap();
        page(&Inputs {
            list,
            tools,
            id: Some(id),
            draft: &Draft::of(agent),
            uses: uses.iter().find(|u| u.agent == agent.name),
            refused: None,
        })
    }

    /// AGM-01's refusals as `s15-58` draws them, and the plain question for an agent nothing uses.
    #[test]
    fn delete_is_refused_for_a_stages_agent_and_the_agent_noyan_uses() {
        let (_, uses) = example();
        let of = |name: &str| uses.iter().find(|u| u.agent == name).unwrap();
        let refused = |d: DeleteDialog| match d {
            DeleteDialog::Refused { title, text, .. } => (title, text),
            other => panic!("refused: {other:?}"),
        };
        assert_eq!(
            refused(delete_dialog("validator", of("Validator"))),
            (
                "Can\u{2019}t delete Validator".to_owned(),
                "Validator is used in pocket-notes \u{b7} Validate. Change that stage first."
                    .to_owned()
            )
        );
        assert_eq!(
            refused(delete_dialog("noyan", of("Noyan"))),
            (
                "Can\u{2019}t delete Noyan".to_owned(),
                "Noyan uses Noyan. Pick another agent for Noyan first.".to_owned()
            )
        );
        let spec_writer = InUse {
            agent: "Spec writer".into(),
            noyan: false,
            stages: vec![StageUse {
                project: Some("shop-web".into()),
                stage: "Product spec".into(),
            }],
        };
        assert_eq!(
            refused(delete_dialog("g", &spec_writer)).1,
            "Spec writer is used in shop-web \u{b7} Product spec. Change that stage first."
        );
        assert_eq!(
            delete_dialog("strong", of("Strong")),
            DeleteDialog::Confirm {
                id: "strong".into(),
                title: "Delete Strong?".into(),
                text:
                    "Nothing uses Strong. Sessions that ran on it keep its name in their history."
                        .into(),
                cancel_label: "Cancel".into(),
                delete_label: "Delete".into(),
            }
        );
    }

    /// AGM-01: Effort shows only for a tool that has a documented level (Claude Code, Codex, Copilot), never for
    /// OpenCode or the default model.
    #[test]
    fn effort_shows_only_where_the_tool_has_one() {
        let (mut list, uses) = example();
        let all = tools(Tool::ALL);
        list.agents[3].effort = Some("high".into());
        let strong = page_of(&list, &all, "strong", &uses);
        let effort = strong.model.effort.expect("Claude Code has levels");
        assert_eq!(effort.label, "Effort");
        assert_eq!(effort.value, "high");
        let levels: Vec<&str> = effort.options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(
            levels,
            ["Tool default", "low", "medium", "high", "xhigh", "max"]
        );
        assert_eq!(strong.model.tool_value, "Claude Code");
        assert_eq!(
            strong.model.picker.as_ref().map(|p| p.value.label.as_str()),
            Some("opus")
        );
        list.agents[4] = on(Tool::OpenCode, list.agents[4].clone());
        let light = page_of(&list, &all, "light", &uses);
        assert!(light.model.effort.is_none(), "OpenCode has none");
        list.agents[0].model = AgentModel::Default;
        let noyan = page_of(&list, &all, "noyan", &uses);
        assert!(noyan.model.effort.is_none() && noyan.model.picker.is_none());
        assert_eq!(noyan.model.tool_value, "Default model");
        assert_eq!(
            noyan.model.default_now.as_deref(),
            Some("Claude Code \u{b7} haiku")
        );
    }

    /// One agent's page (`s15-58` "One agent"): the fields in order, used in, and the note. An agent has no role:
    /// no agent's page shows one, Noyan's agent included. An agent has no skill list (T7): the page has no skills
    /// field, so its view carries none.
    #[test]
    fn one_agents_page_shows_no_role_and_its_used_in() {
        let (list, uses) = example();
        let tools = AgentTools::one(Some(claude_list()));
        let developer = page_of(&list, &tools, "developer", &uses);
        assert_eq!(developer.title, "Developer");
        assert_eq!(
            developer.came_from,
            Some(format!(
                "Added by the {} default preset",
                otaq_core::naming::PRODUCT_NAME
            ))
        );
        assert_eq!(developer.name.label, "Name");
        assert_eq!(
            developer.instructions.value,
            "Run `just test` before committing"
        );
        let view = serde_json::to_value(&developer).unwrap();
        assert!(view.get("skills").is_none(), "no skills field");
        assert!(
            view["draft"].get("skills").is_none(),
            "no skills in the draft"
        );
        assert_eq!(
            developer.used_in.as_ref().unwrap().items,
            [
                "pocket-api \u{b7} Dev",
                "pocket-api \u{b7} Done",
                "pocket-notes \u{b7} Dev",
                "pocket-notes \u{b7} Done"
            ]
        );
        assert_eq!(
            developer.note.as_deref(),
            Some("Edits apply to the next session that starts.")
        );
        // Noyan's own agent's page shows no role either: Noyan is only where it is used.
        let noyan = page_of(&list, &tools, "noyan", &uses);
        let view = serde_json::to_value(&noyan).unwrap();
        assert!(view.get("role").is_none() && view.get("role_tag").is_none());
        assert_eq!(
            noyan.used_in.as_ref().unwrap().items,
            ["every project \u{b7} Noyan"]
        );
        let strong = page_of(&list, &tools, "strong", &[]);
        assert_eq!(strong.used_in.unwrap().empty.as_deref(), Some("\u{2014}"));
    }

    /// New agent and Duplicate fill a draft: no used in, Add agent; a new tool starts on its default model
    /// with no effort; the draft goes to `agents.add` as it reads.
    #[test]
    fn a_draft_changes_and_saves_as_it_reads() {
        let (list, _) = example();
        let tools = AgentTools::one(Some(claude_list()));
        let draft = apply(
            apply(
                apply(Draft::default(), Change::Name("Quick".into())),
                Change::Tool(Some("claude".into())),
            ),
            Change::Model(Some("haiku".into())),
        );
        let draft = apply(draft, Change::Effort(Some("low".into())));
        let new = page(&Inputs {
            list: &list,
            tools: &tools,
            id: None,
            draft: &draft,
            uses: None,
            refused: None,
        });
        assert_eq!(new.title, "New agent");
        assert!(new.used_in.is_none() && new.menu.is_empty());
        assert_eq!(new.add.as_ref().unwrap().add_label, "Add agent");
        let fields = draft.fields();
        assert_eq!(
            fields.model,
            AgentModel::Tool {
                tool: Tool::Claude,
                model: Some("haiku".into())
            }
        );
        assert_eq!(fields.effort.as_deref(), Some("low"));
        // Back to the default model: no model, no effort.
        let back = apply(draft, Change::Tool(None));
        assert_eq!((back.model, back.effort), (None, None));
        assert_eq!(back.tool, None);
        let copy = Draft::of(&list.agents[2]);
        // A refused name is written under Name.
        let refused = Refused {
            field: field_of(Some("name_taken")),
            message: "Another agent is already called Light".into(),
        };
        let page = page(&Inputs {
            list: &list,
            tools: &tools,
            id: None,
            draft: &copy,
            uses: None,
            refused: Some(&refused),
        });
        assert_eq!(
            page.name.problem.as_deref(),
            Some("Another agent is already called Light")
        );
    }
}

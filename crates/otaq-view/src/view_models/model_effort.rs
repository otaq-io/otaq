//! The model menu and the Effort control: how Otaq picks who does a piece of work (UX-65, UX-68, UX-75; tuq 014;
//! canvas "Agents inside the flow", A-2…A-4). One behaviour wherever a model is picked: a stage's Who does it in Edit
//! flow, Done's model, Settings → Noyan's "Noyan starts on" and the Noyan panel's header all build their field with
//! [`view`] and draw it with the page's `ModelEffortField` (`components/ModelEffort.tsx`).
//!
//! **Model** is a menu grouped by tool, only the tools set up on this Mac (Connected, tuq 014): each tool's group
//! starts with `default`, "your <tool> account's model" (UX-68, stored as the tool alone: `codex`), then the models of
//! the tool's own list ([`super::model_picker`]'s, stored `tool:model`: `codex:gpt-5.5`). On top, **As in your other
//! flows**: the model-and-effort pairs the flows already use ([`Use`]), each naming where; one click sets both. Under
//! the groups, the tools that aren't set up, with Set up… (Settings → Tools).
//!
//! **Effort** is the model's tool's levels, as the tool names them, after "Default" (no effort: the tool's own); it is
//! hidden for a tool with none (OpenCode). A new model whose tool doesn't offer the effort takes the nearest level it
//! does ([`otaq_core::agents::nearest_effort`], UX-75).
//!
//! **Not set up** (A-4): a model whose tool isn't set up stays as it is, marked: "<Tool> isn't set up on this Mac.
//! Tickets wait at <stage> until you set it up or pick another model", with Set up <Tool> and Pick another model.
//!
//! **Marked** (AGT-05, UX-65; plan piece 1 "Marks"): a model the tool refused, no longer lists, or swapped carries the
//! amber mark of what it came from ("Codex couldn't run gpt-5.5 on SHOP-3"), with Pick another model ([`marked`]).
//! Only a saved model change or a later first turn that gets through clears it (`otaq_core::agents::mark`).

use otaq_core::agent_tools::{ModelKind, Tool};
use otaq_core::agents::{AgentModel, ModelRef};
use serde::Serialize;
use ts_rs::TS;

use super::agents::{and_list, is_tool_default, short_model};
use super::tool_avatar::ToolAvatarView;
use super::tool_problems::{Tools, tool_word};
use crate::machine::this_machine;

/// The field's label.
pub const MODEL_LABEL: &str = "Model";
pub const EFFORT_LABEL: &str = "Effort";
/// The menu's title.
pub const MENU_TITLE: &str = "Pick a model\u{2026}";
pub const PAIRS_HEADING: &str = "AS IN YOUR OTHER FLOWS \u{b7} sets model and effort";
/// Each tool's first entry (UX-68): its account's own model.
pub const DEFAULT: &str = "default";
/// The Effort control's first entry: no effort is passed, the tool uses its own.
pub const DEFAULT_EFFORT: &str = "Default";
pub const SET_UP: &str = "Set up\u{2026}";
pub const PICK_ANOTHER: &str = "Pick another model";
pub const MENU_FOOT: &str =
    "A model name without a version always means that tool\u{2019}s newest.";
pub const EFFORT_HINT: &str =
    "How hard the model thinks. Higher is slower and uses more of your plan.";
/// A stage's Model hint (A-2).
pub fn stage_hint() -> String {
    format!(
        "Any model of a tool on {}. Every ticket at this stage uses it; it changes only here.",
        this_machine()
    )
}
/// The most pairs the menu offers on top.
pub const MAX_PAIRS: usize = 6;
/// The Noyan panel header's menu (A-6, UX-70): its title, its heading, the default's note, the way to the whole field
/// and its foot.
pub const CONVERSATION_TITLE: &str = "For this conversation";
pub const CONVERSATION_HEADING: &str = "FOR THIS CONVERSATION";
pub const THE_DEFAULT: &str = "the default, from Settings \u{2192} Noyan";
pub const ANOTHER: &str = "Another model or effort\u{2026}";
pub const CONVERSATION_FOOT: &str = "The next conversation starts on the default again.";
/// The Noyan panel's Model hint, under the whole field ("Another model or effort…").
pub const CONVERSATION_HINT: &str = "For this conversation only. The next conversation starts on the default from Settings \u{2192} Noyan.";

/// One place a model and effort are used, for the menu's pairs: a stage of a project's flow ("Dev" in "shop-web"),
/// Done, or Noyan (no project).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    /// As stored: a tool or `tool:model` (`default` is read as the default model's tool).
    pub model: String,
    pub effort: Option<String>,
    /// "Dev", "Done", "Noyan".
    pub what: String,
    /// The project's name; `None` for Noyan.
    pub project: Option<String>,
}

/// Every stage an agent works or checks, and the delivery when it names a model, of `flow` in `project`.
pub fn uses_in(project: &str, flow: &otaq_core::flow::model::Flow) -> Vec<Use> {
    let of = |what: &str, who: &otaq_core::flow::model::WhoDoesIt| {
        who.model.as_ref().map(|model| Use {
            model: model.clone(),
            effort: who.effort.clone(),
            what: what.to_owned(),
            project: Some(project.to_owned()),
        })
    };
    flow.stages
        .iter()
        .filter(|s| s.runs_agent())
        .filter_map(|s| of(&s.name, &s.who))
        .chain(of("Done", &flow.delivery.who))
        .collect()
}

/// The field's closed value: the tool's avatar and "Codex · gpt-5.5". `ModelEffortValue` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelEffortValue")]
pub struct Value {
    pub avatar: Option<ToolAvatarView>,
    /// "Codex · gpt-5.5", "Claude Code · default"; "Pick a model…" with none.
    pub text: String,
    /// Amber after the text: the tool's list no longer has the model ("no longer listed").
    pub no_longer_listed: Option<String>,
}

/// One pair on top of the menu: "CX gpt-5.5 · medium", "Validate in shop-web and billing-api". `ModelEffortPair` on
/// the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelEffortPair")]
pub struct Pair {
    /// Unique in the menu.
    pub id: String,
    /// What it sets, as stored.
    pub model: String,
    pub effort: Option<String>,
    pub avatar: ToolAvatarView,
    /// "opus · high", "default · medium".
    pub label: String,
    /// "Dev in shop-web and billing-api".
    pub places: String,
    pub selected: bool,
}

/// One model of a tool's group. `ModelEffortItem` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelEffortItem")]
pub struct Item {
    /// What picking it stores: the tool (`codex`, its `default`) or `tool:model`.
    pub model: String,
    /// "default", "opus", "gpt-5.5", "newest GPT Sol".
    pub label: String,
    /// "your Codex account’s model", "now claude-opus-5-5".
    pub detail: Option<String>,
    /// An exact model id, in the mono font.
    pub mono: bool,
    pub selected: bool,
}

/// A tool's group of the menu: "CLAUDE CODE", `default` first. `ModelEffortGroup` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelEffortGroup")]
pub struct Group {
    #[ts(type = "string")]
    pub tool: Tool,
    pub heading: String,
    pub avatar: ToolAvatarView,
    pub items: Vec<Item>,
    /// "Loading Codex's models…" while its list isn't read yet: only `default` to pick.
    pub status: Option<String>,
}

/// The open menu. `ModelEffortMenu` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelEffortMenu")]
pub struct Menu {
    pub title: &'static str,
    /// [`PAIRS_HEADING`], with pairs only.
    pub pairs_heading: Option<&'static str>,
    pub pairs: Vec<Pair>,
    pub groups: Vec<Group>,
    /// "Copilot and OpenCode aren’t set up", with [`Self::set_up`] after it; `None` when every tool is.
    pub missing: Option<String>,
    pub set_up: &'static str,
    pub foot: &'static str,
    /// "Another model or effort…": a short menu of pairs only (the Noyan panel's header, A-6), whose last line opens
    /// the whole field; `None` for the whole menu.
    pub another: Option<&'static str>,
}

/// A model whose tool isn't set up on this Mac (A-4). `ModelEffortNotSetUp` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelEffortNotSetUp")]
pub struct NotSetUp {
    #[ts(type = "string")]
    pub tool: Tool,
    /// "Codex isn’t set up on this Mac. Tickets wait at Validate until you set it up or pick another model."
    pub text: String,
    /// "Set up Codex": Settings → Tools.
    pub set_up: String,
    /// "Pick another model": opens the menu.
    pub pick: &'static str,
}

/// The amber mark on a model (AGT-05): what the tool did with it, and the tool's own words. `ModelEffortMarked` on the
/// page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelEffortMarked")]
pub struct Marked {
    /// "Codex couldn't run gpt-5.5 on SHOP-3", "Codex no longer lists gpt-5.4", "Copilot ran auto instead of
    /// gpt-5.5 on SHOP-29".
    pub text: String,
    /// The tool's own message, in quotes; `None` when it gave none.
    pub message: Option<String>,
    /// "Pick another model": opens the menu.
    pub pick: &'static str,
}

/// The field's [`Marked`] line for `mark`.
pub fn marked(mark: &otaq_core::agents::Mark) -> Marked {
    Marked {
        text: mark.text.clone(),
        message: mark
            .message
            .as_deref()
            .map(|m| format!("\u{201c}{m}\u{201d}")),
        pick: PICK_ANOTHER,
    }
}

/// One level of the Effort control. `ModelEffortLevel` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelEffortLevel")]
pub struct Level {
    /// The level as stored (`medium`); empty for [`DEFAULT_EFFORT`].
    pub id: String,
    /// "Medium", as the tool names it.
    pub label: String,
    pub selected: bool,
}

/// The Effort control: the model's tool's levels. `ModelEffortEffort` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelEffortEffort")]
pub struct Effort {
    pub label: &'static str,
    pub levels: Vec<Level>,
    pub hint: &'static str,
}

/// The whole field: Model (its value and menu) and Effort. `ModelEffortView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelEffortView")]
pub struct View {
    pub model_label: &'static str,
    pub value: Value,
    pub menu: Menu,
    /// Under the field; the [`NotSetUp`] line takes its place.
    pub hint: String,
    pub not_set_up: Option<NotSetUp>,
    /// The model's amber mark (AGT-05), with Pick another model; under the field in the hint's place, unless
    /// [`Self::not_set_up`] is there. Set by the caller that knows what the model came from.
    pub marked: Option<Marked>,
    /// `None` for a tool with no effort levels, and while the tool isn't known.
    pub effort: Option<Effort>,
}

/// Where the field is: its hint, and what waits while the tool isn't set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place<'a> {
    pub hint: &'a str,
    /// "Validate": "Tickets wait at Validate until you set it up or pick another model". `None`: nothing waits named.
    pub waits_at: Option<&'a str>,
}

/// The tools set up on this Mac, in [`Tool::ALL`]'s order.
fn set_up(tools: &Tools) -> Vec<Tool> {
    Tool::ALL
        .iter()
        .copied()
        .filter(|t| tools.connected.contains(t))
        .collect()
}

/// The tool and model `model` runs on: its own, the default model's for `default`. `None` when it doesn't read.
pub fn runs_on(model: &str, tools: &Tools) -> Option<ModelRef> {
    AgentModel::parse(model)
        .ok()?
        .on(tools.default_model.as_ref())
}

/// The model as a tile or a pair shows it, without its tool: "default", "opus", "gpt-5.5".
pub fn short_label(model: &str, tools: &Tools) -> String {
    match AgentModel::parse(model) {
        Ok(AgentModel::Default) => DEFAULT.to_owned(),
        Ok(AgentModel::Tool { tool, model }) => {
            let list = tools.list(tool);
            if is_tool_default(model.as_deref(), list) {
                DEFAULT.to_owned()
            } else {
                short_model(model.as_deref(), list)
            }
        }
        Err(_) => model.to_owned(),
    }
}

/// A level as the tool names it, first letter up: "Medium", "XHigh".
pub fn level_label(level: &str) -> String {
    match level {
        "xhigh" => "XHigh".to_owned(),
        _ => {
            let mut chars = level.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

/// "opus · high", "gpt-5.5": a model and its effort the short way, as a stage's tile and a pair show them.
pub fn short_line(model: &str, effort: Option<&str>, tools: &Tools) -> String {
    let label = short_label(model, tools);
    match effort {
        Some(effort) => format!("{label} \u{b7} {effort}"),
        None => label,
    }
}

/// A tile's who line (A-1): the tool's avatar and "gpt-5.5 · medium"; with the amber "isn't set up" text when its tool
/// isn't (A-4). `None` with no model.
pub fn tile(
    model: Option<&str>,
    effort: Option<&str>,
    tools: &Tools,
) -> Option<(Option<ToolAvatarView>, String, Option<String>)> {
    let model = model?;
    let on = runs_on(model, tools);
    let avatar = on
        .as_ref()
        .map(|on| ToolAvatarView::of(on.tool, on.model.as_deref()));
    let off = on
        .filter(|on| !tools.connected.contains(&on.tool))
        .map(|on| not_set_up_text(on.tool, None));
    Some((avatar, short_line(model, effort, tools), off))
}

/// "Codex isn’t set up on this Mac. Tickets wait at Validate until you set it up or pick another model."
pub fn not_set_up_text(tool: Tool, waits_at: Option<&str>) -> String {
    let word = tool_word(tool);
    match waits_at {
        Some(at) => format!(
            "{word} isn\u{2019}t set up on {}. Tickets wait at {at} until you set it up or pick another model.",
            this_machine()
        ),
        None => format!(
            "{word} isn\u{2019}t set up on {}. Set it up or pick another model.",
            this_machine()
        ),
    }
}

/// "Dev in shop-web and billing-api, Done in shop-web", "Noyan".
fn places(uses: &[&Use]) -> String {
    let mut by: Vec<(&str, Vec<String>)> = Vec::new();
    for u in uses {
        let at = match by.iter_mut().find(|(what, _)| *what == u.what) {
            Some((_, at)) => at,
            None => {
                by.push((&u.what, Vec::new()));
                &mut by.last_mut().expect("just pushed").1
            }
        };
        if let Some(project) = &u.project
            && !at.contains(project)
        {
            at.push(project.clone());
        }
    }
    by.into_iter()
        .map(|(what, at)| match at.is_empty() {
            true => what.to_owned(),
            false => format!("{what} in {}", and_list(&at)),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A pair's model as stored and its effort.
type PairKey = (String, Option<String>);

/// `model` as a pair stores it: `default` as the default model's tool, so it groups with the tool's own default.
fn stored(model: &str, tools: &Tools) -> Option<String> {
    match AgentModel::parse(model) {
        Ok(AgentModel::Default) => tools
            .default_model
            .as_ref()
            .map(|d| d.tool.as_str().to_owned()),
        Ok(parsed) => Some(parsed.text()),
        Err(_) => None,
    }
}

/// The pairs on top (tuq 014: only tools set up), most used first, at most [`MAX_PAIRS`]. `default` reads as the
/// default model's tool, so it groups with the tool's own default.
fn pairs(uses: &[Use], tools: &Tools, model: Option<&str>, now: Option<&str>) -> Vec<Pair> {
    let stored = |m: &str| stored(m, tools);
    let mut groups: Vec<(PairKey, Vec<&Use>)> = Vec::new();
    for u in uses {
        let Some(m) = stored(&u.model) else { continue };
        let Some(on) = runs_on(&m, tools) else {
            continue;
        };
        if !tools.connected.contains(&on.tool) {
            continue;
        }
        // An effort the tool doesn't have can't be set with it.
        let effort = u
            .effort
            .clone()
            .filter(|e| on.tool.effort_levels().contains(&e.as_str()));
        let key = (m, effort);
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, at)) => at.push(u),
            None => groups.push((key, vec![u])),
        }
    }
    // Most used first; the first seen first among as many.
    groups.sort_by_key(|(_, at)| std::cmp::Reverse(at.len()));
    let current = model.and_then(stored);
    groups
        .into_iter()
        .take(MAX_PAIRS)
        .filter_map(|((model, effort), at)| {
            let on = runs_on(&model, tools)?;
            Some(Pair {
                id: format!("pair:{model}:{}", effort.as_deref().unwrap_or_default()),
                selected: current.as_deref() == Some(model.as_str()) && effort.as_deref() == now,
                avatar: ToolAvatarView::of(on.tool, on.model.as_deref()),
                label: short_line(&model, effort.as_deref(), tools),
                places: places(&at),
                model,
                effort,
            })
        })
        .collect()
}

/// One tool's group: `default`, then its list's models (the picker's newest and exact ones), `picked` selected.
fn group(tool: Tool, tools: &Tools, picked: Option<&ModelRef>) -> Group {
    let list = tools.list(tool);
    let mine = picked.filter(|p| p.tool == tool);
    let default_picked = mine.is_some_and(|p| is_tool_default(p.model.as_deref(), list));
    let mut items = vec![Item {
        model: tool.as_str().to_owned(),
        label: DEFAULT.to_owned(),
        detail: Some(format!("your {} account\u{2019}s model", tool_word(tool))),
        mono: false,
        selected: default_picked,
    }];
    if let Some(list) = list {
        for e in list
            .entries
            .iter()
            .filter(|e| matches!(e.kind, ModelKind::Follow | ModelKind::Exact))
        {
            items.push(Item {
                model: format!("{}:{}", tool.as_str(), e.id),
                label: short_model(Some(&e.id), Some(list)),
                detail: match e.kind {
                    ModelKind::Follow => e
                        .resolves_to
                        .as_ref()
                        .map(|id| format!("now {id}"))
                        .or_else(|| e.description.clone().filter(|d| !d.trim().is_empty())),
                    _ => None,
                },
                mono: e.kind == ModelKind::Exact,
                selected: !default_picked
                    && mine.and_then(|p| p.model.as_deref()) == Some(e.id.as_str()),
            });
        }
    }
    Group {
        tool,
        heading: tool.display_name().to_uppercase(),
        avatar: ToolAvatarView::of(tool, None),
        items,
        status: list
            .is_none()
            .then(|| super::model_picker::loading_text(tool)),
    }
}

/// The menu over the tools set up on this Mac, `model` selected, with `uses` as its pairs.
pub fn menu(model: Option<&str>, effort: Option<&str>, uses: &[Use], tools: &Tools) -> Menu {
    let picked = model.and_then(|m| runs_on(m, tools));
    let pairs = pairs(uses, tools, model, effort);
    let missing: Vec<String> = Tool::ALL
        .iter()
        .filter(|t| !tools.connected.contains(t))
        .map(|t| tool_word(*t).to_owned())
        .collect();
    Menu {
        title: MENU_TITLE,
        pairs_heading: (!pairs.is_empty()).then_some(PAIRS_HEADING),
        pairs,
        groups: set_up(tools)
            .into_iter()
            .map(|tool| group(tool, tools, picked.as_ref()))
            .collect(),
        missing: match missing.len() {
            0 => None,
            1 => Some(format!("{} isn\u{2019}t set up", missing[0])),
            _ => Some(format!("{} aren\u{2019}t set up", and_list(&missing))),
        },
        set_up: SET_UP,
        foot: MENU_FOOT,
        another: None,
    }
}

/// The Effort control for `tool` with `effort` picked; `None` for a tool with no levels.
pub fn effort_control(tool: Tool, effort: Option<&str>) -> Option<Effort> {
    let levels = tool.effort_levels();
    if levels.is_empty() {
        return None;
    }
    let picked = effort.filter(|e| levels.contains(e));
    Some(Effort {
        label: EFFORT_LABEL,
        levels: std::iter::once(Level {
            id: String::new(),
            label: DEFAULT_EFFORT.to_owned(),
            selected: picked.is_none(),
        })
        .chain(levels.iter().map(|l| Level {
            id: (*l).to_owned(),
            label: level_label(l),
            selected: picked == Some(*l),
        }))
        .collect(),
        hint: EFFORT_HINT,
    })
}

/// The field for `model` and `effort` as stored (`None`: no model yet), in `place`.
pub fn view(
    model: Option<&str>,
    effort: Option<&str>,
    uses: &[Use],
    tools: &Tools,
    place: Place,
) -> View {
    let on = model.and_then(|m| runs_on(m, tools));
    let value = match (model, &on) {
        (Some(m), Some(on)) => {
            let list = tools.list(on.tool);
            let gone = on
                .model
                .as_deref()
                .filter(|id| !is_tool_default(Some(id), list))
                .filter(|id| list.is_some_and(|l| l.entry(id).is_none()))
                .filter(|id| otaq_core::agent_tools::follow::FollowChoice::parse(id).is_none());
            Value {
                avatar: Some(ToolAvatarView::of(on.tool, on.model.as_deref())),
                text: format!("{} \u{b7} {}", tool_word(on.tool), short_label(m, tools)),
                no_longer_listed: gone.map(|_| super::model_picker::NO_LONGER_LISTED.to_owned()),
            }
        }
        (Some(m), None) => Value {
            avatar: None,
            text: m.to_owned(),
            no_longer_listed: None,
        },
        (None, _) => Value {
            avatar: None,
            text: MENU_TITLE.to_owned(),
            no_longer_listed: None,
        },
    };
    let not_set_up = on
        .as_ref()
        .filter(|on| !tools.connected.contains(&on.tool))
        .map(|on| NotSetUp {
            tool: on.tool,
            text: not_set_up_text(on.tool, place.waits_at),
            set_up: format!("Set up {}", tool_word(on.tool)),
            pick: PICK_ANOTHER,
        });
    View {
        model_label: MODEL_LABEL,
        value,
        menu: menu(model, effort, uses, tools),
        hint: place.hint.to_owned(),
        not_set_up,
        marked: None,
        effort: on.and_then(|on| effort_control(on.tool, effort)),
    }
}

/// One pair for `model` and `effort` naming `places`, selected when it is `now`; `None` when the model doesn't read.
fn pair(
    model: &str,
    effort: Option<&str>,
    places: &str,
    now: (Option<&str>, Option<&str>),
    tools: &Tools,
) -> Option<Pair> {
    let model = stored(model, tools)?;
    let on = runs_on(&model, tools)?;
    let levels = on.tool.effort_levels();
    let effort = effort.filter(|e| levels.contains(e));
    let current = now.0.and_then(|m| stored(m, tools));
    let now_effort = now.1.filter(|e| levels.contains(e));
    Some(Pair {
        id: format!("pair:{model}:{}", effort.unwrap_or_default()),
        selected: current.as_deref() == Some(model.as_str()) && effort == now_effort,
        avatar: ToolAvatarView::of(on.tool, on.model.as_deref()),
        label: short_line(&model, effort, tools),
        places: places.to_owned(),
        effort: effort.map(str::to_owned),
        model,
    })
}

/// The Noyan panel header's field (A-6, UX-70) for a conversation not started yet, on `model` and `effort`: its
/// short menu "For this conversation" has the default from Settings → Noyan (`default`) first, then the pairs the
/// flows use (`uses`) and the conversation's own pick when it is none of them, then [`ANOTHER`], which opens the whole
/// field (`menu.groups`, Effort) for this conversation; the foot says the next conversation starts on the default
/// again.
pub fn for_conversation(
    model: &str,
    effort: Option<&str>,
    default: (&str, Option<&str>),
    uses: &[Use],
    tools: &Tools,
) -> View {
    let mut v = view(
        Some(model),
        effort,
        uses,
        tools,
        Place {
            hint: CONVERSATION_HINT,
            waits_at: None,
        },
    );
    let now = (Some(model), effort);
    let mut pairs: Vec<Pair> = pair(default.0, default.1, THE_DEFAULT, now, tools)
        .into_iter()
        .collect();
    for p in std::mem::take(&mut v.menu.pairs) {
        if !pairs.iter().any(|q| q.id == p.id) {
            pairs.push(p);
        }
    }
    if !pairs.iter().any(|p| p.selected)
        && let Some(own) = pair(model, effort, "", now, tools)
    {
        let at = pairs.len().min(1);
        pairs.insert(at, own);
    }
    v.menu.title = CONVERSATION_TITLE;
    v.menu.pairs_heading = Some(CONVERSATION_HEADING);
    v.menu.pairs = pairs;
    v.menu.foot = CONVERSATION_FOOT;
    v.menu.another = Some(ANOTHER);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::agents::fixtures;

    fn tools(connected: &[Tool]) -> Tools {
        Tools {
            connected: connected.to_vec(),
            lists: fixtures::all_lists(),
            ..Tools::default()
        }
    }

    fn at(what: &str, project: &str, model: &str, effort: Option<&str>) -> Use {
        Use {
            model: model.to_owned(),
            effort: effort.map(str::to_owned),
            what: what.to_owned(),
            project: Some(project.to_owned()),
        }
    }

    static HINT: std::sync::LazyLock<String> = std::sync::LazyLock::new(stage_hint);

    /// A stage's field, at Validate.
    fn stage() -> Place<'static> {
        Place {
            hint: &HINT,
            waits_at: Some("Validate"),
        }
    }

    /// A-3: grouped by tool, only tools set up, each starting with `default`; the ones not set up named under them.
    #[test]
    fn the_menu_groups_the_set_up_tools_each_starting_with_default() {
        let t = tools(&[Tool::Claude, Tool::Codex]);
        let v = view(Some("codex:gpt-5.5"), Some("medium"), &[], &t, stage());
        let heads: Vec<&str> = v.menu.groups.iter().map(|g| g.heading.as_str()).collect();
        assert_eq!(heads, ["CLAUDE CODE", "CODEX"]);
        let claude = &v.menu.groups[0];
        assert_eq!(claude.items[0].label, "default");
        assert_eq!(claude.items[0].model, "claude");
        assert_eq!(
            claude.items[0].detail.as_deref(),
            Some("your Claude Code account\u{2019}s model")
        );
        let labels: Vec<&str> = claude.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["default", "opus", "sonnet", "haiku", "fable"]);
        assert_eq!(claude.items[1].model, "claude:opus");
        let codex = &v.menu.groups[1];
        let picked: Vec<&str> = codex
            .items
            .iter()
            .filter(|i| i.selected)
            .map(|i| i.model.as_str())
            .collect();
        assert_eq!(picked, ["codex:gpt-5.5"]);
        assert!(codex.items.iter().any(|i| i.label == "gpt-5.5" && i.mono));
        assert_eq!(
            v.menu.missing.as_deref(),
            Some("Copilot and OpenCode aren\u{2019}t set up")
        );
        assert_eq!(v.value.text, "Codex \u{b7} gpt-5.5");
        assert_eq!(v.not_set_up, None);
        assert_eq!(v.hint, stage_hint());
    }

    /// The tool alone is its default; `default` is the default model's tool's.
    #[test]
    fn a_tool_alone_and_default_select_the_tools_default() {
        let mut t = tools(&[Tool::Claude, Tool::Codex]);
        let v = view(Some("codex"), None, &[], &t, stage());
        assert!(v.menu.groups[1].items[0].selected);
        assert_eq!(v.value.text, "Codex \u{b7} default");
        t.default_model = Some(ModelRef {
            tool: Tool::Claude,
            model: None,
        });
        let v = view(Some("default"), Some("high"), &[], &t, stage());
        assert!(v.menu.groups[0].items[0].selected);
        assert_eq!(v.value.text, "Claude Code \u{b7} default");
        assert_eq!(
            tile(Some("default"), Some("high"), &t).unwrap().1,
            "default \u{b7} high"
        );
    }

    /// The Effort control is the tool's levels after Default; hidden for OpenCode, which has none.
    #[test]
    fn effort_is_the_tools_levels_and_hidden_for_a_tool_with_none() {
        let t = tools(&[Tool::Codex, Tool::OpenCode]);
        let v = view(Some("codex:gpt-5.5"), Some("medium"), &[], &t, stage());
        let effort = v.effort.expect("Codex has levels");
        let labels: Vec<&str> = effort.levels.iter().map(|l| l.label.as_str()).collect();
        assert_eq!(
            labels,
            ["Default", "Minimal", "Low", "Medium", "High", "XHigh"]
        );
        let picked: Vec<&str> = effort
            .levels
            .iter()
            .filter(|l| l.selected)
            .map(|l| l.id.as_str())
            .collect();
        assert_eq!(picked, ["medium"]);
        let v = view(Some("opencode"), None, &[], &t, stage());
        assert_eq!(v.effort, None);
    }

    /// The pairs on top: what the flows use, each naming where, most used first; only tools set up (tuq 014).
    #[test]
    fn the_pairs_name_where_each_is_used() {
        let t = tools(&[Tool::Claude, Tool::Codex]);
        let uses = [
            at("Dev", "shop-web", "claude:opus", Some("high")),
            at("Validate", "shop-web", "codex:gpt-5.5", Some("medium")),
            at("Dev", "billing-api", "claude:opus", Some("high")),
            at("Validate", "billing-api", "codex:gpt-5.5", Some("medium")),
            at("Review", "billing-api", "claude:opus", Some("high")),
            at("Docs", "billing-api", "copilot:gpt-5.5", None),
        ];
        let v = view(Some("codex:gpt-5.5"), Some("medium"), &uses, &t, stage());
        assert_eq!(v.menu.pairs_heading, Some(PAIRS_HEADING));
        let pairs: Vec<(&str, &str, bool)> = v
            .menu
            .pairs
            .iter()
            .map(|p| (p.label.as_str(), p.places.as_str(), p.selected))
            .collect();
        assert_eq!(
            pairs,
            [
                (
                    "opus \u{b7} high",
                    "Dev in shop-web and billing-api, Review in billing-api",
                    false
                ),
                (
                    "gpt-5.5 \u{b7} medium",
                    "Validate in shop-web and billing-api",
                    true
                ),
            ]
        );
        assert_eq!(v.menu.pairs[0].model, "claude:opus");
        assert_eq!(v.menu.pairs[0].effort.as_deref(), Some("high"));
        // No pairs, no heading.
        assert_eq!(view(None, None, &[], &t, stage()).menu.pairs_heading, None);
    }

    /// A-4: a model whose tool isn't set up stays, marked, with Set up <Tool> and Pick another model.
    #[test]
    fn a_tool_that_isnt_set_up_is_marked() {
        let t = tools(&[Tool::Claude]);
        let v = view(Some("codex:gpt-5.5"), Some("medium"), &[], &t, stage());
        let off = v.not_set_up.expect("Codex isn't set up");
        assert_eq!(
            off.text,
            format!(
                "Codex isn\u{2019}t set up on {}. Tickets wait at Validate until you set it up or pick another \
                 model.",
                this_machine()
            )
        );
        assert_eq!(off.set_up, "Set up Codex");
        assert_eq!(off.pick, "Pick another model");
        assert_eq!(v.value.text, "Codex \u{b7} gpt-5.5");
        let (_, line, mark) = tile(Some("codex:gpt-5.5"), Some("medium"), &t).unwrap();
        assert_eq!(line, "gpt-5.5 \u{b7} medium");
        assert!(mark.is_some());
        assert!(v.menu.groups.iter().all(|g| g.tool != Tool::Codex));
    }

    /// A tool whose list isn't read yet offers only its `default`, saying it loads.
    #[test]
    fn a_tool_whose_list_isnt_read_offers_its_default() {
        let t = Tools {
            connected: vec![Tool::Codex],
            ..Tools::default()
        };
        let v = view(Some("codex:gpt-5.5"), None, &[], &t, stage());
        let g = &v.menu.groups[0];
        assert_eq!(g.items.len(), 1);
        assert_eq!(g.status.as_deref(), Some("Loading Codex's models\u{2026}"));
        assert_eq!(v.value.no_longer_listed, None);
    }
}

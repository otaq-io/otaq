//! The model picker of one agent's page and of the Agents page's model dropdowns (T-0410; before it the Add and Edit
//! agent dialogs, T-0340, T-0361; AGT-02, d0086; *15 → Add agent · model
//! picker open* `k8WyHN` / `L1mzD`, *Edit agent · Codex (M23)* `ObF25` / `l5T21`, *Model list states (M23)* `GTuGI` /
//! `GUoan`, *Old GPT · no longer listed* `OCna6` / `aPdlu`), built from Otaq's copy of the agent's tool's own list
//! (T-0335's `ModelList`). Only what the list has can be picked and nothing can be typed (d0086): "Tool default"
//! first, then **ALWAYS THE NEWEST** (Claude Code's aliases, Copilot's "Auto · efficiency" tiers, Codex's and
//! OpenCode's "<Family> · always the newest", each with what it resolves to today where it resolves to one model),
//! then **EXACT MODELS**. While a first read runs it says "Loading Codex's models…"; when the read failed, "Couldn't
//! load Copilot's models · Retry" (`agents.models_retry`), and the saved model stays. A saved model the list no
//! longer has is shown as saved, marked "no longer listed" (amber).

use otaq_core::agent_tools::{ModelKind, Tool};
use otaq_core::naming::PRODUCT_NAME;
use serde::Serialize;
use ts_rs::TS;

use super::agents::{ListState, TOOL_DEFAULT_LABEL, is_tool_default, model_label, tool_word};

/// One choice. `value` is what the dialog sends as the agent's model: `None` for the tool default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelPickerItem")]
pub struct Item {
    pub value: Option<String>,
    /// "Tool default", "Opus · always the newest", "Auto · balance", `claude-opus-5-5`.
    pub label: String,
    /// The muted part on the right: "Otaq passes no model", "now claude-opus-5-5", an Auto tier's description.
    pub detail: Option<String>,
    /// An exact model id is drawn in the mono font.
    pub mono: bool,
    pub selected: bool,
}

/// A group under its small heading ("ALWAYS THE NEWEST"); the tool default's group has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelPickerGroup")]
pub struct Group {
    pub heading: Option<String>,
    pub items: Vec<Item>,
    /// The muted line when the group has no items: "Claude Code's list has only its aliases".
    pub empty: Option<String>,
}

/// What the closed field shows: the picked model and what it runs today.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelPickerValue")]
pub struct Value {
    pub label: String,
    pub detail: Option<String>,
    pub mono: bool,
    /// Amber beside a saved model the list doesn't have any more: "no longer listed" (`OCna6`).
    pub no_longer_listed: Option<String>,
}

/// The list isn't there to pick from (`GTuGI`): loading, or it couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelPickerStatus")]
pub struct Status {
    /// Loading: a spinner before the text; else the amber "couldn't load" line.
    pub loading: bool,
    /// "Loading Codex's models…", "Couldn't load Copilot's models".
    pub text: String,
    /// "Retry" after a "·" (`agents.models_retry` for [`Self::retry_tool`], then the dialog again).
    pub retry_label: Option<String>,
    /// The tool to read again, as stored (`copilot`).
    #[ts(type = "string | null")]
    pub retry_tool: Option<Tool>,
    /// The muted line under it: "The saved model stays selected while the list loads.", "Verifier keeps gpt-5.5
    /// as saved. …".
    pub note: Option<String>,
}

/// The whole picker: the field's value, the open list's groups in order, and the hint under the field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModelPickerView")]
pub struct Picker {
    pub value: Value,
    pub groups: Vec<Group>,
    /// Loading or couldn't load; `None` once the list is there.
    pub status: Option<Status>,
    pub hint: String,
}

pub const NEWEST_HEADING: &str = "ALWAYS THE NEWEST";
pub const EXACT_HEADING: &str = "EXACT MODELS";
pub const NO_LONGER_LISTED: &str = "no longer listed";
pub const RETRY_LABEL: &str = "Retry";

/// "now claude-opus-5-5".
fn now(resolves_to: Option<&str>) -> Option<String> {
    resolves_to.map(|id| format!("now {id}"))
}

/// What passing nothing means, beside "Tool default".
pub fn passes_nothing() -> String {
    format!("{PRODUCT_NAME} passes no model")
}

/// "Loading Codex's models…".
pub fn loading_text(tool: Tool) -> String {
    format!("Loading {}'s models\u{2026}", tool_word(tool))
}

/// "Couldn't load Copilot's models".
pub fn failed_text(tool: Tool) -> String {
    format!(
        "Couldn\u{2019}t load {}\u{2019}s models. Check your connection, then try again.",
        tool_word(tool)
    )
}

/// A follow entry's detail: what it runs now, or (one that resolves per task, Copilot's Auto) the tool's own
/// description, else that the tool picks.
fn follow_detail(tool: Tool, e: &otaq_core::agent_tools::models::ModelEntry) -> Option<String> {
    match e.resolves_to.as_deref() {
        Some(id) => now(Some(id)),
        None => Some(
            e.description
                .clone()
                .filter(|d| !d.trim().is_empty())
                .unwrap_or_else(|| format!("{} picks the model for each task", tool_word(tool))),
        ),
    }
}

/// The picker for `tool`'s list in `state`, with `picked` (the dialog's current model, `None` = tool default).
/// `refused`: the entry is marked for this very model (the hint then says the account can't use it, `WFHlh`).
/// `keeps`: the entry's name in Edit agent, for "Verifier keeps gpt-5.5 as saved".
pub fn picker(
    tool: Tool,
    state: ListState<'_>,
    picked: Option<&str>,
    refused: bool,
    keeps: Option<&str>,
) -> Picker {
    let list = state.list();
    let word = tool_word(tool);
    let entries = list.map(|l| l.entries.as_slice()).unwrap_or_default();
    let picked_default = is_tool_default(picked, list);
    let selected = |id: &str| !picked_default && picked == Some(id);
    let item = |e: &otaq_core::agent_tools::models::ModelEntry| Item {
        value: Some(e.id.clone()),
        label: model_label(Some(&e.id), list),
        detail: match e.kind {
            ModelKind::Follow => follow_detail(tool, e),
            _ => None,
        },
        mono: e.kind == ModelKind::Exact,
        selected: selected(&e.id),
    };
    let of = |kind| entries.iter().filter(move |e| e.kind == kind);
    let default = Group {
        heading: None,
        items: vec![Item {
            value: None,
            label: TOOL_DEFAULT_LABEL.to_owned(),
            detail: Some(passes_nothing()),
            mono: false,
            selected: picked_default,
        }],
        empty: None,
    };
    let newest = Group {
        heading: Some(NEWEST_HEADING.to_owned()),
        items: of(ModelKind::Follow).map(item).collect(),
        empty: None,
    };
    let exact: Vec<Item> = of(ModelKind::Exact).map(item).collect();
    let exact = Group {
        heading: Some(EXACT_HEADING.to_owned()),
        empty: exact
            .is_empty()
            .then(|| format!("{word}'s list has only its aliases")),
        items: exact,
    };
    let mut no_longer_listed = None;
    let value = if picked_default {
        Value {
            label: TOOL_DEFAULT_LABEL.to_owned(),
            detail: Some(passes_nothing()),
            mono: false,
            no_longer_listed: None,
        }
    } else {
        let id = picked.unwrap_or_default();
        let entry = list.and_then(|l| l.entry(id));
        // Only a list that was read can say a model is gone.
        no_longer_listed = (list.is_some() && entry.is_none()).then(|| NO_LONGER_LISTED.to_owned());
        Value {
            label: model_label(Some(id), list),
            detail: entry
                .filter(|e| e.kind == ModelKind::Follow)
                .and_then(|e| now(e.resolves_to.as_deref())),
            // A saved model the copy no longer has is shown as saved: its id.
            mono: entry.is_none_or(|e| e.kind == ModelKind::Exact),
            no_longer_listed: no_longer_listed.clone(),
        }
    };
    let status = match state {
        ListState::Loading => Some(Status {
            loading: true,
            text: loading_text(tool),
            retry_label: None,
            retry_tool: None,
            note: Some("The saved model stays selected while the list loads.".to_owned()),
        }),
        ListState::Failed => Some(Status {
            loading: false,
            text: failed_text(tool),
            retry_label: Some(RETRY_LABEL.to_owned()),
            retry_tool: Some(tool),
            note: keeps.map(|name| {
                format!(
                    "{name} keeps {} as saved. Tickets still start on it; the check before each launch uses the last copy of the list.",
                    model_label(picked, None)
                )
            }),
        }),
        ListState::Listed(_) | ListState::Unread => None,
    };
    let hint = if no_longer_listed.is_some() {
        format!(
            "{word}'s list doesn't have it any more. Pick another model; it isn't removed for you."
        )
    } else if refused {
        format!("From {word}'s own list. The list offered it, but this account can't use it.")
    } else {
        format!(
            "From {word}'s own list. A follow choice is resolved when each session starts, and the session keeps the exact model it ran on."
        )
    };
    Picker {
        value,
        // Nothing to pick from but the tool default while the list isn't there.
        groups: if status.is_some() {
            vec![default]
        } else {
            vec![default, newest, exact]
        },
        status,
        hint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::agents::{ListState, fixtures};

    /// A group's heading and its items' label and detail.
    type Shape = (Option<String>, Vec<(String, Option<String>)>);

    fn labels(p: &Picker) -> Vec<Shape> {
        p.groups
            .iter()
            .map(|g| {
                (
                    g.heading.clone(),
                    g.items
                        .iter()
                        .map(|i| (i.label.clone(), i.detail.clone()))
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn the_groups_come_in_order_with_what_each_follow_choice_runs_today() {
        let list = fixtures::claude_list();
        let p = picker(
            Tool::Claude,
            ListState::Listed(&list),
            Some("fable"),
            false,
            None,
        );
        let s = |t: &str| t.to_owned();
        assert_eq!(
            labels(&p),
            vec![
                (
                    None,
                    vec![(
                        s("Tool default"),
                        Some(format!(
                            "{} passes no model",
                            otaq_core::naming::PRODUCT_NAME
                        ))
                    )]
                ),
                (
                    Some(s("ALWAYS THE NEWEST")),
                    vec![
                        (
                            s("Opus · always the newest"),
                            Some(s("now claude-opus-5-5"))
                        ),
                        (
                            s("Sonnet · always the newest"),
                            Some(s("now claude-sonnet-5-5"))
                        ),
                        (
                            s("Haiku · always the newest"),
                            Some(s("now claude-haiku-5-5"))
                        ),
                        (
                            s("Fable · always the newest"),
                            Some(s("now claude-fable-5-1"))
                        ),
                    ]
                ),
                (Some(s("EXACT MODELS")), vec![]),
            ]
        );
        assert_eq!(
            p.groups[2].empty.as_deref(),
            Some("Claude Code's list has only its aliases")
        );
        assert_eq!(
            p.hint,
            "From Claude Code's own list. A follow choice is resolved when each session starts, and the session keeps the exact model it ran on."
        );
        let picked: Vec<&str> = p
            .groups
            .iter()
            .flat_map(|g| &g.items)
            .filter(|i| i.selected)
            .map(|i| i.label.as_str())
            .collect();
        assert_eq!(picked, ["Fable · always the newest"]);
        assert_eq!(p.value.label, "Fable · always the newest");
        assert_eq!(p.value.detail.as_deref(), Some("now claude-fable-5-1"));
        assert_eq!(p.groups[1].items[0].value.as_deref(), Some("opus"));
    }

    #[test]
    fn the_tool_default_is_picked_for_no_model_and_for_the_tools_own_default() {
        let list = fixtures::claude_list();
        for picked in [None, Some("default")] {
            let p = picker(Tool::Claude, ListState::Listed(&list), picked, false, None);
            assert!(p.groups[0].items[0].selected);
            assert_eq!(
                p.groups[0].items[0].value, None,
                "the tool default passes nothing"
            );
            assert_eq!(p.value.label, "Tool default");
            assert_eq!(
                p.value.detail.as_deref(),
                Some(format!("{} passes no model", otaq_core::naming::PRODUCT_NAME).as_str())
            );
        }
    }

    #[test]
    fn exact_models_list_in_mono_and_a_saved_model_the_list_lost_shows_as_saved() {
        let mut list = fixtures::claude_list();
        list.entries.push(fixtures::entry(
            "claude-opus-5-5",
            "Opus 5.5",
            ModelKind::Exact,
            None,
        ));
        let p = picker(
            Tool::Claude,
            ListState::Listed(&list),
            Some("claude-opus-5-5"),
            false,
            None,
        );
        assert_eq!(p.groups[2].empty, None);
        let exact = &p.groups[2].items[0];
        assert_eq!(
            (exact.label.as_str(), exact.mono, exact.selected),
            ("claude-opus-5-5", true, true)
        );

        let p = picker(
            Tool::Claude,
            ListState::Listed(&list),
            Some("claude-opus-4-0"),
            false,
            None,
        );
        assert_eq!(p.value.label, "claude-opus-4-0");
        assert!(p.value.mono);
        assert!(p.groups.iter().flat_map(|g| &g.items).all(|i| !i.selected));
        // M23: the list that was read doesn't have it: "no longer listed" (amber), and it isn't removed.
        assert_eq!(
            p.value.no_longer_listed.as_deref(),
            Some("no longer listed")
        );
        assert_eq!(
            p.hint,
            "Claude Code's list doesn't have it any more. Pick another model; it isn't removed for you."
        );
        // Before any read nothing can say it's gone.
        let p = picker(
            Tool::Claude,
            ListState::Unread,
            Some("claude-opus-4-0"),
            false,
            None,
        );
        assert_eq!(p.value.no_longer_listed, None);
        assert_eq!(p.status, None);
    }

    /// Codex (`ObF25`): Tool default, "GPT Sol · always the newest" with "now gpt-6-sol", then the exact models.
    #[test]
    fn codex_offers_its_families_then_its_exact_models() {
        let list = fixtures::codex_list();
        let p = picker(
            Tool::Codex,
            ListState::Listed(&list),
            Some("gpt-5.5"),
            false,
            None,
        );
        let s = |t: &str| t.to_owned();
        assert_eq!(
            labels(&p),
            vec![
                (None, vec![(s("Tool default"), Some(passes_nothing()))]),
                (
                    Some(s("ALWAYS THE NEWEST")),
                    vec![(s("GPT Sol · always the newest"), Some(s("now gpt-6-sol")))]
                ),
                (
                    Some(s("EXACT MODELS")),
                    ["gpt-6-sol", "gpt-5.5", "gpt-5.5-pro", "gpt-5.5-mini"]
                        .map(|id| (s(id), None))
                        .to_vec()
                ),
            ]
        );
        assert_eq!(
            p.groups[1].items[0].value.as_deref(),
            Some("newest:gpt-sol")
        );
        assert_eq!((p.value.label.as_str(), p.value.mono), ("gpt-5.5", true));
        assert!(p.hint.starts_with("From Codex's own list."));
        // Codex's own default entry is the tool default.
        let p = picker(
            Tool::Codex,
            ListState::Listed(&list),
            Some("default"),
            false,
            None,
        );
        assert!(p.groups[0].items[0].selected);
    }

    /// Copilot's Auto tiers (Q-A, Q145): just "Auto · balance", Copilot's description beside it, no "always the
    /// newest" and no "now …".
    #[test]
    fn copilots_auto_tiers_have_no_now_part() {
        let list = fixtures::copilot_list();
        let p = picker(
            Tool::Copilot,
            ListState::Listed(&list),
            Some("auto@balance"),
            false,
            None,
        );
        let s = |t: &str| t.to_owned();
        assert_eq!(
            labels(&p)[1],
            (
                Some(s("ALWAYS THE NEWEST")),
                vec![
                    (
                        s("Auto · balance"),
                        Some(s("A balance of speed and quality"))
                    ),
                    (
                        s("Auto · intelligence"),
                        Some(s("The most capable model for the task"))
                    ),
                ]
            )
        );
        assert_eq!(p.value.label, "Auto · balance");
        assert_eq!(p.value.detail, None);
        assert!(!p.value.mono);
        assert!(p.hint.starts_with("From Copilot's own list."));
        // Without a description, the tool picks.
        let mut bare = list.clone();
        bare.entries[0].description = None;
        let p = picker(Tool::Copilot, ListState::Listed(&bare), None, false, None);
        assert_eq!(
            p.groups[1].items[0].detail.as_deref(),
            Some("Copilot picks the model for each task")
        );
        // An agent saved on Auto · efficiency, which Copilot no longer runs (decision 117): shown as saved, "no
        // longer listed", and not offered.
        let p = picker(
            Tool::Copilot,
            ListState::Listed(&list),
            Some("auto@efficiency"),
            false,
            None,
        );
        assert_eq!(p.value.label, "auto@efficiency");
        assert_eq!(p.value.no_longer_listed.as_deref(), Some(NO_LONGER_LISTED));
        assert!(
            p.groups
                .iter()
                .flat_map(|g| &g.items)
                .all(|i| !i.label.contains("efficiency"))
        );
    }

    /// OpenCode: its family ("GLM Flash") and `provider/id` exact models.
    #[test]
    fn opencode_offers_its_families_and_provider_models() {
        let list = fixtures::opencode_list();
        let p = picker(Tool::OpenCode, ListState::Listed(&list), None, false, None);
        assert_eq!(p.groups[1].items[0].label, "GLM Flash · always the newest");
        assert_eq!(
            p.groups[1].items[0].detail.as_deref(),
            Some("now opencode/glm-5-flash")
        );
        let exact: Vec<&str> = p.groups[2].items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(exact, ["opencode/glm-5-flash", "google/gemini-3-pro"]);
    }

    /// `GTuGI`: loading keeps the saved model with a note; couldn't load offers Retry and says the entry keeps it.
    #[test]
    fn loading_and_couldnt_load_keep_the_saved_model() {
        let p = picker(
            Tool::Codex,
            ListState::Loading,
            Some("gpt-5.5"),
            false,
            Some("Standard"),
        );
        let status = p.status.as_ref().expect("loading");
        assert!(status.loading);
        assert_eq!(status.text, "Loading Codex's models…");
        assert_eq!(status.retry_label, None);
        assert_eq!(
            status.note.as_deref(),
            Some("The saved model stays selected while the list loads.")
        );
        assert_eq!(p.value.label, "gpt-5.5");
        assert_eq!(p.value.no_longer_listed, None);
        assert_eq!(p.groups.len(), 1, "only the tool default to pick");

        let p = picker(
            Tool::Copilot,
            ListState::Failed,
            Some("gpt-5.5"),
            false,
            Some("Verifier"),
        );
        let status = p.status.expect("couldn't load");
        assert!(!status.loading);
        assert_eq!(
            status.text,
            "Couldn’t load Copilot’s models. Check your connection, then try again."
        );
        assert_eq!(status.retry_label.as_deref(), Some("Retry"));
        assert_eq!(status.retry_tool, Some(Tool::Copilot));
        assert_eq!(
            status.note.as_deref(),
            Some(
                "Verifier keeps gpt-5.5 as saved. Tickets still start on it; the check before each launch uses the last copy of the list."
            )
        );
        assert_eq!(p.value.label, "gpt-5.5");
        // Add agent has no saved entry to name.
        let p = picker(Tool::Copilot, ListState::Failed, None, false, None);
        assert_eq!(p.status.and_then(|s| s.note), None);
    }

    #[test]
    fn a_refused_model_says_the_account_cant_use_it() {
        let list = fixtures::claude_list();
        let p = picker(
            Tool::Claude,
            ListState::Listed(&list),
            Some("fable"),
            true,
            None,
        );
        assert_eq!(
            p.hint,
            "From Claude Code's own list. The list offered it, but this account can't use it."
        );
    }
}

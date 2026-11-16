//! Settings → Noyan (UX-70, refined 2026-11-16; canvas A-8): app settings' own Noyan section, right after Tools (Noyan
//! isn't a tool). Its page holds only **Noyan starts on**: the model and effort every new Noyan conversation starts
//! on, the shared model field of [`super::model_effort`] with the flows' model-and-effort pairs on top of its menu.
//! The Noyan panel's header switches it for one conversation only. Settings → Tools' Where they're used still names
//! Noyan under the tool it starts on ([`super::agent_tools_page::used`]).

use otaq_core::agents::{AgentModel, NoyanStartsOn};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::agent_tools_page::{Flows, NOYAN};
use super::model_effort;

/// Settings → Noyan: "Noyan starts on" (UX-70). `NoyanSettingsPage` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "NoyanSettingsPage")]
pub struct Page {
    /// "Noyan".
    pub title: String,
    /// "Noyan starts on".
    pub label: String,
    /// The shared Model and Effort field; its hint is [`NOYAN_HINT`].
    pub picker: model_effort::View,
}

/// What Noyan starts on's field says under it (UX-70).
pub const NOYAN_HINT: &str =
    "For new conversations. Switch for one conversation in the Noyan panel.";

/// The page: Noyan starts on, the shared field with the flows' model-and-effort pairs on top of its menu. No model
/// stored yet reads as `default` (the account's default model's tool), as the conversation starts on it.
pub fn page(f: &Flows) -> Page {
    let model = f.noyan.model.as_deref().unwrap_or(model_effort::DEFAULT);
    Page {
        title: NOYAN.to_owned(),
        label: format!("{NOYAN} starts on"),
        picker: model_effort::view(
            Some(model),
            f.noyan.effort.as_deref(),
            f.uses,
            f.tools,
            model_effort::Place {
                hint: NOYAN_HINT,
                waits_at: None,
            },
        ),
    }
}

/// One change to Noyan starts on, as the field sends it: a model (its effort stays, or moves to the nearest level the
/// new tool offers, UX-75), a pair of "As in your other flows" (both), or an effort (`None`: the tool's own).
/// `NoyanSettingsEdit` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "edit", rename_all = "snake_case")]
#[ts(rename = "NoyanSettingsEdit")]
pub enum Edit {
    Model {
        model: String,
    },
    Pair {
        model: String,
        #[serde(default)]
        #[ts(optional)]
        effort: Option<String>,
    },
    Effort {
        #[serde(default)]
        #[ts(optional)]
        effort: Option<String>,
    },
}

/// Noyan starts on after `edit`, to store (`noyan.set_starts_on`; the service checks it again). A model that doesn't
/// read, or a pair's effort its tool doesn't have, is refused with why.
pub fn edit(on: &NoyanStartsOn, edit: &Edit) -> Result<NoyanStartsOn, String> {
    match edit {
        Edit::Model { model } => {
            let parsed = AgentModel::parse(model)?;
            // `default`'s tool is known only from the account, so its effort stays (as a stage's does).
            let effort = match (parsed.tool(), on.effort.as_deref()) {
                (Some(tool), Some(effort)) => {
                    otaq_core::agents::nearest_effort(tool, effort).map(str::to_owned)
                }
                (_, effort) => effort.map(str::to_owned),
            };
            Ok(NoyanStartsOn {
                model: Some(parsed.text()),
                effort,
            })
        }
        Edit::Pair { model, effort } => {
            let parsed = AgentModel::parse(model)?;
            if let Some(why) = effort
                .as_deref()
                .and_then(|e| otaq_core::agents::effort_refusal(&parsed, e))
            {
                return Err(why);
            }
            Ok(NoyanStartsOn {
                model: Some(parsed.text()),
                effort: effort.clone(),
            })
        }
        Edit::Effort { effort } => Ok(NoyanStartsOn {
            model: on.model.clone(),
            effort: effort.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::agent_tools::Tool;

    use super::super::model_effort::Use;
    use super::super::tool_problems::Tools;
    use super::*;

    /// UX-70: Settings → Noyan holds Noyan starts on, the shared field, with the flows' pairs on top of its menu (the
    /// gallery's page: Claude Code · opus · high with the sample flows).
    #[test]
    fn the_noyan_page_has_noyan_starts_on() {
        let p = super::super::agent_tools_page::gallery().noyan;
        assert_eq!(p.title, "Noyan");
        assert_eq!(p.label, "Noyan starts on");
        assert_eq!(p.picker.value.text, "Claude Code · opus");
        assert_eq!(
            p.picker.hint,
            "For new conversations. Switch for one conversation in the Noyan panel."
        );
        let effort = p.picker.effort.expect("Claude Code has effort levels");
        assert!(effort.levels.iter().any(|l| l.id == "high" && l.selected));
        // The menu's pairs are the flows' (Noyan isn't one of its own pairs).
        assert!(!p.picker.menu.pairs.is_empty());
        assert!(
            p.picker
                .menu
                .pairs
                .iter()
                .all(|p| !p.places.contains("Noyan"))
        );
    }

    /// Nothing stored reads as `default`, the default model's tool.
    #[test]
    fn nothing_stored_reads_as_default() {
        let tools = Tools {
            connected: vec![Tool::Claude],
            lists: crate::view_models::agents::fixtures::all_lists(),
            default_model: Some(otaq_core::agents::ModelRef {
                tool: Tool::Claude,
                model: None,
            }),
            ..Tools::default()
        };
        let uses: Vec<Use> = Vec::new();
        let on = NoyanStartsOn::default();
        let f = Flows {
            noyan: &on,
            uses: &uses,
            tools: &tools,
        };
        assert_eq!(page(&f).picker.value.text, "Claude Code · default");
    }

    /// Noyan starts on's edits: a model keeps the effort or moves it to the nearest level its tool has (UX-75), a pair
    /// sets both, an effort only the effort; a model that doesn't read is refused.
    #[test]
    fn noyan_starts_on_edits() {
        let on = NoyanStartsOn {
            model: Some("codex:gpt-5.5".into()),
            effort: Some("xhigh".into()),
        };
        let model = |m: &str| Edit::Model { model: m.into() };
        assert_eq!(
            edit(&on, &model("claude:opus")).unwrap(),
            NoyanStartsOn {
                model: Some("claude:opus".into()),
                effort: Some(
                    otaq_core::agents::nearest_effort(Tool::Claude, "xhigh")
                        .unwrap()
                        .to_owned()
                ),
            }
        );
        assert_eq!(
            edit(&on, &model("opencode")).unwrap().effort,
            None,
            "OpenCode has no effort"
        );
        assert_eq!(
            edit(&on, &model("default")).unwrap().effort.as_deref(),
            Some("xhigh"),
            "default's tool isn't known here: the effort stays"
        );
        assert_eq!(
            edit(
                &on,
                &Edit::Pair {
                    model: "claude:sonnet".into(),
                    effort: Some("medium".into()),
                }
            )
            .unwrap(),
            NoyanStartsOn {
                model: Some("claude:sonnet".into()),
                effort: Some("medium".into()),
            }
        );
        assert_eq!(
            edit(&on, &Edit::Effort { effort: None }).unwrap(),
            NoyanStartsOn {
                model: Some("codex:gpt-5.5".into()),
                effort: None,
            }
        );
        assert!(edit(&on, &model("nope:x")).is_err());
        assert!(
            edit(
                &on,
                &Edit::Pair {
                    model: "opencode".into(),
                    effort: Some("high".into()),
                }
            )
            .is_err()
        );
        // As the page sends them.
        let e: Edit =
            serde_json::from_value(serde_json::json!({ "edit": "effort", "effort": "low" }))
                .unwrap();
        assert_eq!(
            e,
            Edit::Effort {
                effort: Some("low".into())
            }
        );
    }
}

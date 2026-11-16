//! What Otaq ships about who does the work (AGM-02, UX-68): the names of Otaq's own skills, and the account's default
//! model, which a shipped flow's `model: default` becomes when the flow is applied ([`default_tool`]).

use super::{AgentModel, ModelRef};
use crate::agent_tools::Tool;
use crate::flow::FlowFile;
use crate::store::{Store, StoreError};

/// The skills Otaq ships, by their folders in its plugin (`plugin/skills/<name>/`), its workflow skills. A flow and
/// Otaq name each in its namespace ([`crate::skills::own_name`]: `otaq:validate`); a bare name is the user's skill,
/// and tuq's (`tuq:product-spec`) are the user's tuq's ([`crate::tuq`]).
pub const SKILLS: &[&str] = &[
    "address-review",
    "answer-question",
    "check-ticket",
    "commit",
    "flow",
    "noyan",
    "open-pr",
    "push",
    "restack",
    "validate",
    "work-ticket",
];

/// Sets the account's default model to the first connected tool with its default model when none is set (a fresh
/// install: AGM-02's "else the first starter entry's tool with no model"). `connected` in [`Tool::ALL`]'s order.
/// Whether it set one.
pub async fn ensure_default_model(store: &Store, connected: &[Tool]) -> Result<bool, StoreError> {
    let Some(&tool) = connected.first() else {
        return Ok(false);
    };
    if store.default_model().await?.is_some() {
        return Ok(false);
    }
    store
        .set_default_model(&ModelRef { tool, model: None })
        .await?;
    tracing::info!(%tool, "the account's default model is the first connected tool's");
    Ok(true)
}

/// `file` with each `model: default` (Otaq's shipped flows, older files) made the tool of the account's default model
/// `default` (UX-68: `claude`, that tool's own default), as a flow is applied. Left as it is while no default model is
/// set: it then resolves when a session starts. Whether it changed anything.
pub fn default_tool(file: &mut FlowFile, default: Option<&ModelRef>) -> bool {
    let Some(default) = default else {
        return false;
    };
    let tool = AgentModel::Tool {
        tool: default.tool,
        model: None,
    }
    .text();
    let mut changed = false;
    let whos = file
        .flow
        .stages
        .iter_mut()
        .map(|s| &mut s.who)
        .chain(std::iter::once(&mut file.flow.delivery.who));
    for who in whos {
        if who.model.as_deref().map(str::trim) == Some("default") {
            who.model = Some(tool.clone());
            // An effort the tool doesn't have goes (UX-75: its default is the tool's).
            if who
                .effort
                .as_deref()
                .is_some_and(|e| !default.tool.effort_levels().contains(&e))
            {
                who.effort = None;
            }
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_model_becomes_the_default_models_tool() {
        let mut file = crate::flow::default_file().clone();
        assert!(!default_tool(&mut file, None));
        let default = ModelRef {
            tool: Tool::Claude,
            model: Some("opus".into()),
        };
        assert!(default_tool(&mut file, Some(&default)));
        for stage in file.flow.stages.iter().filter(|s| s.runs_agent()) {
            assert_eq!(stage.who.model.as_deref(), Some("claude"));
        }
        assert_eq!(file.flow.delivery.who.model.as_deref(), Some("claude"));
        assert!(!default_tool(&mut file, Some(&default)));
    }
}

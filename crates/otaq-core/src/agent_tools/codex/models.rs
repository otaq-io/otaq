//! Codex's model list (AGT-02; T-0351): `codex app-server`'s `model/list` (its documented protocol, not the `debug`
//! subcommand), hidden models left out.
//!
//! `model/list { includeHidden: false, cursor }` answers `{data: [Model], nextCursor}`; each `Model` has `id` (what
//! `-m` takes), `displayName`, `description`, `hidden` and `isDefault` (the one Codex runs with no `-m` and no
//! `model` in `config.toml`). Signed in, the list is the account's (fetched, then cached in `models_cache.json`);
//! signed out or offline, Codex answers from its cache or the list bundled with the binary, so it still answers
//! (the models report, row 2). Checked against Codex CLI 0.157.1 (`tests/fixtures/codex/app_server/`).
//!
//! The list becomes "Tool default" (passed as nothing; it runs what `isDefault` names today, unless the user's
//! `config.toml` pins a `model`), then every listed model as an exact one, in Codex's order. Codex has no aliases;
//! its "always the newest" choices are families from models.dev ([`super::Codex::family_source`]).

use std::time::Duration;

use serde_json::{Value, json};

use super::app_server::{AppServer, neutral_cwd};
use super::{CodexContext, DISPLAY_NAME};
use crate::agent_tools::models::{ModelEntry, ModelKind};
use crate::agent_tools::{ModelList, ModelListError};

/// The longest the whole list may take (it answers in about a second signed in, 3 s offline with no cache).
pub const LIST_TIMEOUT: Duration = Duration::from_secs(10);

/// The tool default's id in Otaq's copy (never passed to Codex).
pub const TOOL_DEFAULT: &str = "default";

/// The most pages read (Codex's list is one or two pages).
const MAX_PAGES: usize = 20;

pub(super) async fn read_list(context: &CodexContext) -> Result<ModelList, ModelListError> {
    let cwd = neutral_cwd(context);
    let models = tokio::time::timeout(LIST_TIMEOUT, async {
        let mut server = AppServer::start(context, &cwd).await?;
        let pages = read_pages(&mut server).await;
        server.stop().await;
        pages
    })
    .await
    .unwrap_or_else(|_| Err(format!("no answer within {}s", LIST_TIMEOUT.as_secs())))
    .map_err(failed)?;
    parse_models(&models)
}

/// Every page's models, following `nextCursor`.
async fn read_pages(server: &mut AppServer) -> Result<Vec<Value>, String> {
    let mut models = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let answer = server
            .request("model/list", list_params(cursor.as_deref()))
            .await?;
        let (page, next) = page(&answer)?;
        models.extend(page);
        match next {
            Some(next) => cursor = Some(next),
            None => return Ok(models),
        }
    }
    Err(format!("more than {MAX_PAGES} pages"))
}

/// `model/list`'s parameters: hidden models left out.
pub(super) fn list_params(cursor: Option<&str>) -> Value {
    match cursor {
        Some(cursor) => json!({ "includeHidden": false, "cursor": cursor }),
        None => json!({ "includeHidden": false }),
    }
}

/// One answer's models and its `nextCursor`.
pub(super) fn page(answer: &Value) -> Result<(Vec<Value>, Option<String>), String> {
    let data = answer
        .get("data")
        .and_then(Value::as_array)
        .ok_or("its answer has no data")?;
    let next = answer
        .get("nextCursor")
        .and_then(Value::as_str)
        .filter(|c| !c.is_empty())
        .map(str::to_owned);
    Ok((data.clone(), next))
}

fn failed(reason: impl Into<String>) -> ModelListError {
    ModelListError {
        message: format!("{DISPLAY_NAME} didn't list its models"),
        reason: reason.into(),
    }
}

/// The entries of `models` (every page's, in order): the tool default first, resolving to the `isDefault` model,
/// then each model that isn't hidden as an exact one.
pub(super) fn parse_models(models: &[Value]) -> Result<ModelList, ModelListError> {
    let text = |model: &Value, field: &str| {
        model
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let shown: Vec<&Value> = models
        .iter()
        .filter(|m| m.get("hidden").and_then(Value::as_bool) != Some(true))
        .filter(|m| text(m, "id").is_some())
        .collect();
    if shown.is_empty() {
        return Err(failed("its answer lists no models"));
    }
    let default = shown
        .iter()
        .find(|m| m.get("isDefault").and_then(Value::as_bool) == Some(true))
        .and_then(|m| text(m, "id"));
    let mut entries = vec![ModelEntry {
        id: TOOL_DEFAULT.to_owned(),
        display_name: "Tool default".to_owned(),
        description: None,
        resolves_to: default,
        kind: ModelKind::ToolDefault,
        family: None,
        released_at: None,
    }];
    let mut seen = std::collections::HashSet::new();
    for model in shown {
        let Some(id) = text(model, "id") else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        entries.push(ModelEntry {
            display_name: text(model, "displayName").unwrap_or_else(|| id.clone()),
            description: text(model, "description"),
            resolves_to: None,
            kind: ModelKind::Exact,
            family: None,
            released_at: None,
            id,
        });
    }
    Ok(ModelList { entries })
}

#[cfg(test)]
mod tests {
    use super::super::app_server::tests::{saved, stand_in};
    use super::*;

    fn result(name: &str) -> Value {
        saved(name)["result"].clone()
    }

    /// The saved three pages (Codex 0.157.1, signed out: its bundled list) give the tool default (resolving to
    /// `gpt-6-astra`, the one marked `isDefault`) and seven exact models in Codex's order.
    #[test]
    fn the_saved_pages_give_the_default_and_every_model() {
        let mut models = Vec::new();
        let mut cursors = Vec::new();
        for name in [
            "model_list-page1.json",
            "model_list-page2.json",
            "model_list-page3.json",
        ] {
            let (page, next) = page(&result(name)).unwrap();
            models.extend(page);
            cursors.push(next);
        }
        assert_eq!(cursors, [Some("3".into()), Some("6".into()), None]);
        let list = parse_models(&models).unwrap();
        let rows: Vec<_> = list
            .entries
            .iter()
            .map(|e| (e.id.as_str(), e.display_name.as_str(), e.kind))
            .collect();
        assert_eq!(
            rows,
            [
                ("default", "Tool default", ModelKind::ToolDefault),
                ("gpt-6-astra", "GPT-6-Astra", ModelKind::Exact),
                ("gpt-6-sol", "GPT-6-Sol", ModelKind::Exact),
                ("gpt-6-luna", "GPT-6-Luna", ModelKind::Exact),
                ("gpt-5.6-sol", "GPT-5.6-Sol", ModelKind::Exact),
                ("gpt-5.6-terra", "GPT-5.6-Terra", ModelKind::Exact),
                ("gpt-5.6-luna", "GPT-5.6-Luna", ModelKind::Exact),
                ("gpt-5.5", "GPT-5.5", ModelKind::Exact),
            ]
        );
        let default = list.tool_default().unwrap();
        assert_eq!(default.resolves_to.as_deref(), Some("gpt-6-astra"));
        assert_eq!(default.passed(), None);
        assert_eq!(list.entry("gpt-6-sol").unwrap().passed(), Some("gpt-6-sol"));
        assert_eq!(
            list.entry("gpt-6-sol").unwrap().description.as_deref(),
            Some("Workhorse model for coding and everyday work.")
        );
    }

    #[test]
    fn hidden_models_are_left_out_and_an_empty_list_is_an_error() {
        let models = [
            json!({"id": "gpt-6-sol", "displayName": "GPT-6-Sol", "hidden": false, "isDefault": false}),
            json!({"id": "gpt-reserve", "displayName": "GPT Reserve", "hidden": true, "isDefault": true}),
        ];
        let list = parse_models(&models).unwrap();
        assert!(!list.contains("gpt-reserve"));
        assert_eq!(
            list.tool_default().unwrap().resolves_to,
            None,
            "the default is hidden"
        );
        let err = parse_models(&models[1..]).unwrap_err();
        assert_eq!(err.message, "Codex didn't list its models");
        assert!(page(&json!({"nope": 1})).is_err());
    }

    #[test]
    fn the_request_never_asks_for_hidden_models() {
        assert_eq!(list_params(None), json!({"includeHidden": false}));
        assert_eq!(
            list_params(Some("3")),
            json!({"includeHidden": false, "cursor": "3"})
        );
    }

    /// The whole read through a stand-in server: every page followed, the server ended.
    #[tokio::test]
    async fn the_list_is_read_page_by_page() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        stand_in(
            dir.path(),
            &log,
            &[
                (r#""cursor":"3""#, result("model_list-page2.json")),
                (r#""cursor":"6""#, result("model_list-page3.json")),
                (r#""method":"model/list""#, result("model_list-page1.json")),
            ],
            "",
        );
        let context = CodexContext {
            tool: crate::agent_tools::test_support::context(dir.path(), None),
            codex_home: Some(dir.path().to_path_buf()),
            held_back: false,
        };
        let list = read_list(&context).await.unwrap();
        assert_eq!(list.entries.len(), 8);
        let log = std::fs::read_to_string(&log).unwrap();
        assert_eq!(log.matches(r#""method":"model/list""#).count(), 3, "{log}");
        assert!(!log.contains(r#""includeHidden":true"#));
    }

    #[tokio::test]
    async fn no_codex_on_path_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let context = CodexContext {
            tool: crate::agent_tools::test_support::context(dir.path(), None),
            codex_home: None,
            held_back: false,
        };
        let err = read_list(&context).await.unwrap_err();
        assert_eq!(err.message, "Codex didn't list its models");
    }
}

//! A plugin's settings for one project (the otaq-marketplace README, "Ticket sources"): each field is the project's value,
//! else the global default, else the manifest's default, read field by field so one bad value never resets the
//! others.

use serde_json::{Map, Value};

use super::manifest::{Manifest, SettingKind, SettingScope};

/// The values a command runs with.
pub fn effective(
    manifest: &Manifest,
    global: &Map<String, Value>,
    project: &Map<String, Value>,
) -> Map<String, Value> {
    let mut out = Map::new();
    for spec in &manifest.settings {
        let set =
            |values: &Map<String, Value>| values.get(&spec.key).filter(|v| present(v)).cloned();
        let global = (spec.scope == SettingScope::Any)
            .then(|| set(global))
            .flatten();
        if let Some(value) = set(project).or(global).or_else(|| spec.default.clone()) {
            out.insert(spec.key.clone(), value);
        }
    }
    out
}

fn present(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(s) => !s.trim().is_empty(),
        Value::Object(o) => !o.is_empty(),
        _ => true,
    }
}

/// What is wrong with `values` (effective ones: every required field filled, every value of its format), as
/// `(key, message)`; empty when a command can run.
pub fn problems(manifest: &Manifest, values: &Map<String, Value>) -> Vec<(String, String)> {
    let mut problems = Vec::new();
    for spec in &manifest.settings {
        let value = values.get(&spec.key);
        match (spec.kind, value) {
            (_, None) if spec.required => {
                problems.push((spec.key.clone(), format!("{} is required", spec.label)));
            }
            (SettingKind::String, Some(Value::String(s))) if !spec.format.accepts(s.trim()) => {
                problems.push((
                    spec.key.clone(),
                    format!("{}: {}", spec.label, spec.format.describe()),
                ));
            }
            (SettingKind::String, Some(v)) if !v.is_string() => {
                problems.push((spec.key.clone(), format!("{} is text", spec.label)));
            }
            (SettingKind::Map, Some(v))
                if !v
                    .as_object()
                    .is_some_and(|o| o.values().all(Value::is_string)) =>
            {
                problems.push((
                    spec.key.clone(),
                    format!("{} maps names to words", spec.label),
                ));
            }
            _ => {}
        }
    }
    problems
}

/// Only the manifest's keys, strings trimmed: what is stored from a settings form.
pub fn clean(manifest: &Manifest, values: &Map<String, Value>) -> Map<String, Value> {
    values
        .iter()
        .filter(|(k, v)| manifest.setting(k).is_some() && present(v))
        .map(|(k, v)| {
            let v = match v {
                Value::String(s) => Value::String(s.trim().to_owned()),
                other => other.clone(),
            };
            (k.clone(), v)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::plugins::manifest::{read, tests::github};

    fn map(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn project_then_global_then_default_field_by_field() {
        let m = read(&github()).unwrap();
        let global = map(json!({"query": "is:open label:triage", "repo": "global/ignored"}));
        let project = map(json!({"repo": "acme/shop", "query": " "}));
        let values = effective(&m, &global, &project);
        assert_eq!(values["repo"], "acme/shop");
        assert_eq!(
            values["query"], "is:open label:triage",
            "an empty project value uses the global one"
        );
        assert_eq!(
            values["priority_labels"],
            json!({"P1": "urgent"}),
            "the manifest's default"
        );
        let values = effective(&m, &global, &Map::new());
        assert!(values.get("repo").is_none(), "repo is project-only");
        assert!(problems(&m, &values).iter().any(|(k, _)| k == "repo"));
    }

    #[test]
    fn values_must_fit_their_format() {
        let m = read(&github()).unwrap();
        let values = map(json!({"repo": "--upload-pack=x"}));
        assert_eq!(problems(&m, &values)[0].0, "repo");
        assert!(problems(&m, &map(json!({"repo": "acme/shop"}))).is_empty());
        let cleaned = clean(
            &m,
            &map(json!({"repo": " acme/shop ", "other": "x", "query": ""})),
        );
        assert_eq!(cleaned, map(json!({"repo": "acme/shop"})));
    }
}

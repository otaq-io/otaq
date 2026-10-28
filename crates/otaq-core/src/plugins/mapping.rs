//! A CLI's JSON output as issues, through the command's `map:` (the otaq-marketplace README, "Writing a plugin").

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::manifest::{Field, Manifest, Mapping, SettingKind, Transform, TypeCategory, Via};
use super::markdown;
use super::path::JsonPath;
use super::template::{Template, TemplateScope, Values, scalar};
use crate::domain::Priority;

/// One tracker issue, in the shape every plugin returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// What `fetch` takes: `123`, `PAY-12`.
    pub id: String,
    /// What the ticket's External ID shows: `#123`, `PAY-12`.
    pub key: String,
    pub url: String,
    pub title: String,
    /// Markdown; a search may leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub type_category: TypeCategory,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub state_category: StateCategory,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<IssueRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueRef {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateCategory {
    #[default]
    Open,
    Done,
}

impl Issue {
    /// A work item that isn't finished: what import lists and auto-pull takes.
    pub fn importable(&self) -> bool {
        self.type_category == TypeCategory::Work && self.state_category == StateCategory::Open
    }
}

/// The issues of `search`'s output.
pub fn search_results(
    manifest: &Manifest,
    output: &Value,
    settings: &Map<String, Value>,
) -> Result<Vec<Issue>, String> {
    let map = &manifest.commands.search.map;
    let items = map.items.as_deref().unwrap_or("$");
    let path = JsonPath::parse(items)?;
    let found = path.all(output);
    let list: Vec<&Value> = match found.as_slice() {
        [one] if one.is_array() => one.as_array().into_iter().flatten().collect(),
        _ => found,
    };
    list.into_iter()
        .map(|item| issue(manifest, map, item, settings))
        .collect()
}

/// The one issue of `fetch`'s output (the first, when it is a list).
pub fn fetched(
    manifest: &Manifest,
    output: &Value,
    settings: &Map<String, Value>,
) -> Result<Issue, String> {
    let map = &manifest.commands.fetch.map;
    let path = JsonPath::parse(map.item.as_deref().unwrap_or("$"))?;
    let item = path
        .first(output)
        .map(|v| v.as_array().and_then(|a| a.first()).unwrap_or(v))
        .ok_or("the output has no issue")?;
    issue(manifest, map, item, settings)
}

fn issue(
    manifest: &Manifest,
    map: &Mapping,
    item: &Value,
    settings: &Map<String, Value>,
) -> Result<Issue, String> {
    let read = |field: &Option<Field>| {
        field
            .as_ref()
            .map(|f| text(f, item, settings))
            .transpose()
            .map(Option::flatten)
    };
    let id = text(&map.id, item, settings)?.ok_or("an issue has no id")?;
    let key = read(&map.key)?.unwrap_or_else(|| id.clone());
    let title = text(&map.title, item, settings)?
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| format!("{key} has no title"))?;
    let kind = read(&map.kind)?;
    let state = read(&map.state)?;
    let state_category = match read(&map.state_category)? {
        Some(c) => category_word(&c),
        None => match &state {
            Some(s)
                if manifest
                    .states
                    .done
                    .iter()
                    .any(|d| d.eq_ignore_ascii_case(s)) =>
            {
                StateCategory::Done
            }
            _ => StateCategory::Open,
        },
    };
    let url = match read(&map.url)? {
        Some(url) => url,
        None => issue_url(manifest, settings, &id, &key)?,
    };
    let parent = read(&map.parent)?.map(|key| IssueRef {
        key,
        url: read(&map.parent_url).ok().flatten(),
    });
    let priority = read(&map.priority)?.and_then(|p| priority_word(&p));
    Ok(Issue {
        type_category: manifest.types.category(kind.as_deref()),
        id,
        key,
        url,
        title: title.trim().to_owned(),
        body: read(&map.body)?,
        kind,
        state,
        state_category,
        parent,
        priority,
        updated_at: read(&map.updated_at)?,
    })
}

fn issue_url(
    manifest: &Manifest,
    settings: &Map<String, Value>,
    id: &str,
    key: &str,
) -> Result<String, String> {
    let template = manifest
        .issue_url
        .as_deref()
        .ok_or_else(|| format!("{key} has no url, and the plugin has no issue_url"))?;
    let keys: Vec<&str> = manifest.settings.iter().map(|s| s.key.as_str()).collect();
    let raw: Vec<&str> = manifest
        .settings
        .iter()
        .filter(|s| s.format == super::manifest::Format::Url)
        .map(|s| s.key.as_str())
        .collect();
    Template::parse(template, TemplateScope::IssueUrl, &keys)?.render_url(
        &Values {
            settings: Some(settings),
            id: Some(id),
            key: Some(key),
            ..Default::default()
        },
        &raw,
    )
}

/// A field's text: `None` when the output has no value there.
fn text(
    field: &Field,
    item: &Value,
    settings: &Map<String, Value>,
) -> Result<Option<String>, String> {
    match field {
        Field::Path(p) if p.contains('{') => {
            let template = Template::parse(p, TemplateScope::Item, &[])?;
            Ok(template
                .render(&Values {
                    item: Some(item),
                    ..Default::default()
                })
                .ok())
        }
        Field::Path(p) => Ok(JsonPath::parse(p)?.first(item).and_then(scalar)),
        Field::Rule(rule) => {
            let values = JsonPath::parse(&rule.from)?.all(item);
            if let Some(transform) = rule.transform {
                let Some(value) = values.first() else {
                    return Ok(None);
                };
                let markdown = match transform {
                    Transform::AdfToMarkdown => markdown::adf_to_markdown(value),
                    Transform::HtmlToMarkdown => {
                        markdown::html_to_markdown(value.as_str().unwrap_or_default())
                    }
                };
                return Ok(Some(markdown));
            }
            let words = values.into_iter().filter_map(scalar);
            Ok(match &rule.via {
                None => words.into_iter().next(),
                Some(via) => {
                    let table = lookup_table(via, settings);
                    words
                        .filter_map(|w| {
                            table
                                .iter()
                                .find(|(k, _)| k.eq_ignore_ascii_case(&w))
                                .map(|(_, v)| v.clone())
                        })
                        .next()
                }
            })
        }
    }
}

fn lookup_table(via: &Via, settings: &Map<String, Value>) -> Vec<(String, String)> {
    match via {
        Via::Map(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        Via::Setting(name) => {
            let key = name.strip_prefix("settings.").unwrap_or(name);
            settings
                .get(key)
                .and_then(Value::as_object)
                .map(|o| {
                    o.iter()
                        .filter_map(|(k, v)| Some((k.clone(), scalar(v)?)))
                        .collect()
                })
                .unwrap_or_default()
        }
    }
}

fn category_word(word: &str) -> StateCategory {
    match word.trim().to_ascii_lowercase().as_str() {
        "done" | "closed" | "complete" | "completed" | "resolved" => StateCategory::Done,
        _ => StateCategory::Open,
    }
}

/// `urgent`, `high`, `normal`, `low` (and Jira's Highest, Medium, Lowest), or 1–4.
pub fn priority_word(word: &str) -> Option<Priority> {
    let word = word.trim().to_ascii_lowercase();
    match word.as_str() {
        "urgent" | "highest" | "critical" | "blocker" => Some(Priority::Urgent),
        "high" => Some(Priority::High),
        "normal" | "medium" => Some(Priority::Normal),
        "low" | "lowest" | "minor" | "trivial" => Some(Priority::Low),
        n => n.parse::<i64>().ok().and_then(Priority::from_number),
    }
}

/// The settings a map-kind setting may hold, for the form.
pub fn is_map(manifest: &Manifest, key: &str) -> bool {
    manifest
        .setting(key)
        .is_some_and(|s| s.kind == SettingKind::Map)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::plugins::manifest::{read, tests::github};

    fn settings() -> Map<String, Value> {
        json!({"repo": "acme/shop", "priority_labels": {"P1": "urgent", "P2": "high"}})
            .as_object()
            .unwrap()
            .clone()
    }

    #[test]
    fn github_search_output_becomes_issues() {
        let m = read(&github()).unwrap();
        let output = json!([
            {"number": 12, "title": "Fix login", "url": "https://github.com/acme/shop/issues/12", "state": "OPEN",
             "labels": [{"name": "bug"}, {"name": "p2"}]},
            {"number": 13, "title": "Old", "state": "CLOSED", "labels": []}
        ]);
        let issues = search_results(&m, &output, &settings()).unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].key, "#12");
        assert_eq!(issues[0].id, "12");
        assert_eq!(
            issues[0].priority,
            Some(Priority::High),
            "via the setting, ignoring case"
        );
        assert!(issues[0].importable());
        assert_eq!(
            issues[1].url, "https://github.com/acme/shop/issues/13",
            "from issue_url"
        );
        assert_eq!(issues[1].state_category, StateCategory::Done);
        assert!(!issues[1].importable());
    }

    #[test]
    fn fetch_reads_one_issue() {
        let m = read(&github()).unwrap();
        let output = json!({"number": 12, "title": " Fix ", "body": "Body", "url": "u", "state": "OPEN", "labels": []});
        let issue = fetched(&m, &output, &settings()).unwrap();
        assert_eq!(issue.title, "Fix");
        assert_eq!(issue.body.as_deref(), Some("Body"));
        assert_eq!(issue.priority, None);
        let no_title = json!({"number": 1, "title": ""});
        assert!(
            fetched(&m, &no_title, &settings())
                .unwrap_err()
                .contains("no title")
        );
    }

    #[test]
    fn priority_words() {
        assert_eq!(priority_word("Highest"), Some(Priority::Urgent));
        assert_eq!(priority_word("Medium"), Some(Priority::Normal));
        assert_eq!(priority_word("2"), Some(Priority::High));
        assert_eq!(priority_word("9"), None);
    }
}

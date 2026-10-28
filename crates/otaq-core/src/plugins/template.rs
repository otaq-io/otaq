//! `{placeholder}` templates: a command's arguments, `issue_url`, and a mapping field such as `"#{number}"`.
//!
//! A placeholder fills part of **one** argument and never splits it: Otaq runs the program directly, with no shell,
//! so a value with spaces or quotes stays one argument. A filled value may not start an argument with `-` (it
//! would read as an option), and no value may hold a line break.

use serde_json::Value;

use super::path::JsonPath;

/// Which placeholders a template may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateScope {
    /// `{settings.<key>}`, `{query}`, `{limit}`.
    Search,
    /// `{settings.<key>}`, `{id}`.
    Fetch,
    /// `{settings.<key>}`, `{id}`, `{key}`.
    IssueUrl,
    /// A path into the issue's JSON: `{number}`, `{fields.key}`.
    Item,
}

#[derive(Debug, Clone, PartialEq)]
enum Part {
    Text(String),
    Setting(String),
    Var(&'static str),
    Path(JsonPath),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    parts: Vec<Part>,
}

/// The values a template is filled with.
#[derive(Debug, Default)]
pub struct Values<'a> {
    pub settings: Option<&'a serde_json::Map<String, Value>>,
    pub query: Option<&'a str>,
    pub limit: Option<u32>,
    pub id: Option<&'a str>,
    pub key: Option<&'a str>,
    pub item: Option<&'a Value>,
}

impl Template {
    pub fn parse(text: &str, scope: TemplateScope, settings: &[&str]) -> Result<Self, String> {
        let mut parts = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            if open > 0 {
                parts.push(Part::Text(rest[..open].to_owned()));
            }
            let after = &rest[open + 1..];
            let close = after
                .find('}')
                .ok_or_else(|| format!("\"{text}\" has a {{ with no }}"))?;
            parts.push(placeholder(&after[..close], scope, settings)?);
            rest = &after[close + 1..];
        }
        if rest.contains('}') {
            return Err(format!("\"{text}\" has a }} with no {{"));
        }
        if !rest.is_empty() {
            parts.push(Part::Text(rest.to_owned()));
        }
        Ok(Self { parts })
    }

    pub fn has_placeholders(&self) -> bool {
        self.parts.iter().any(|p| !matches!(p, Part::Text(_)))
    }

    /// The filled argument. A missing value is an error naming it.
    pub fn render(&self, values: &Values) -> Result<String, String> {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Text(t) => out.push_str(t),
                other => {
                    let value = lookup(other, values)?;
                    if value.chars().any(|c| c == '\n' || c == '\r' || c == '\0') {
                        return Err(format!("{} has a line break", name(other)));
                    }
                    if out.is_empty() && value.starts_with('-') {
                        return Err(format!("{} can't start with -", name(other)));
                    }
                    out.push_str(&value);
                }
            }
        }
        Ok(out)
    }

    /// The filled address: values are percent-encoded (`/` kept, so `owner/name` stays a path), except settings of the
    /// `url` format (passed in `raw`).
    pub fn render_url(&self, values: &Values, raw: &[&str]) -> Result<String, String> {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Text(t) => out.push_str(t),
                Part::Setting(key) if raw.contains(&key.as_str()) => {
                    out.push_str(lookup(part, values)?.trim_end_matches('/'));
                }
                other => out.push_str(&encode(&lookup(other, values)?)),
            }
        }
        Ok(out)
    }
}

fn placeholder(inner: &str, scope: TemplateScope, settings: &[&str]) -> Result<Part, String> {
    let inner = inner.trim();
    if scope == TemplateScope::Item {
        return JsonPath::parse(inner).map(Part::Path);
    }
    if let Some(key) = inner.strip_prefix("settings.") {
        return if settings.contains(&key) {
            Ok(Part::Setting(key.to_owned()))
        } else {
            Err(format!("{{{inner}}} names no setting"))
        };
    }
    let allowed: &[&'static str] = match scope {
        TemplateScope::Search => &["query", "limit"],
        TemplateScope::Fetch => &["id"],
        TemplateScope::IssueUrl => &["id", "key"],
        TemplateScope::Item => &[],
    };
    allowed
        .iter()
        .find(|v| **v == inner)
        .map(|v| Part::Var(v))
        .ok_or_else(|| format!("{{{inner}}} isn't a placeholder here"))
}

fn lookup(part: &Part, values: &Values) -> Result<String, String> {
    let found = match part {
        Part::Text(t) => Some(t.clone()),
        Part::Setting(key) => values.settings.and_then(|s| s.get(key)).and_then(scalar),
        Part::Var("query") => values.query.map(str::to_owned),
        Part::Var("limit") => values.limit.map(|l| l.to_string()),
        Part::Var("id") => values.id.map(str::to_owned),
        Part::Var("key") => values.key.map(str::to_owned),
        Part::Var(_) => None,
        Part::Path(path) => values.item.and_then(|i| path.first(i)).and_then(scalar),
    };
    found
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{} has no value", name(part)))
}

fn name(part: &Part) -> String {
    match part {
        Part::Text(t) => t.clone(),
        Part::Setting(key) => format!("The setting {key}"),
        Part::Var(v) => format!("{{{v}}}"),
        Part::Path(p) => format!("{{{p}}}"),
    }
}

/// A string, number or boolean as text.
pub fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn settings() -> serde_json::Map<String, Value> {
        json!({"repo": "acme/shop", "org": "https://dev.azure.com/acme/", "flag": "--evil", "project": "My App"})
            .as_object()
            .unwrap()
            .clone()
    }

    #[test]
    fn a_value_stays_one_argument() {
        let s = settings();
        let t = Template::parse("{query}", TemplateScope::Search, &[]).unwrap();
        let values = Values {
            query: Some("is:open label:\"good first\" ; rm -rf /"),
            ..Default::default()
        };
        assert_eq!(
            t.render(&values).unwrap(),
            "is:open label:\"good first\" ; rm -rf /"
        );
        let t = Template::parse("{settings.repo}", TemplateScope::Search, &["repo"]).unwrap();
        let values = Values {
            settings: Some(&s),
            ..Default::default()
        };
        assert_eq!(t.render(&values).unwrap(), "acme/shop");
    }

    #[test]
    fn options_and_line_breaks_are_refused() {
        let s = settings();
        let t = Template::parse("{settings.flag}", TemplateScope::Search, &["flag"]).unwrap();
        let values = Values {
            settings: Some(&s),
            ..Default::default()
        };
        assert!(
            t.render(&values)
                .unwrap_err()
                .contains("can't start with -")
        );
        let t =
            Template::parse("--repo={settings.flag}", TemplateScope::Search, &["flag"]).unwrap();
        assert_eq!(
            t.render(&values).unwrap(),
            "--repo=--evil",
            "not at the start"
        );
        let t = Template::parse("{query}", TemplateScope::Search, &[]).unwrap();
        let values = Values {
            query: Some("a\nb"),
            ..Default::default()
        };
        assert!(t.render(&values).is_err());
        assert!(
            t.render(&Values::default())
                .unwrap_err()
                .contains("no value")
        );
    }

    #[test]
    fn unknown_placeholders_are_refused() {
        assert!(Template::parse("{id}", TemplateScope::Search, &[]).is_err());
        assert!(Template::parse("{settings.nope}", TemplateScope::Fetch, &["repo"]).is_err());
        assert!(Template::parse("{query", TemplateScope::Search, &[]).is_err());
        assert!(Template::parse("query}", TemplateScope::Search, &[]).is_err());
    }

    #[test]
    fn urls_encode_values_but_not_url_settings() {
        let s = settings();
        let t = Template::parse(
            "{settings.org}/{settings.project}/_workitems/edit/{id}",
            TemplateScope::IssueUrl,
            &["org", "project"],
        )
        .unwrap();
        let values = Values {
            settings: Some(&s),
            id: Some("12"),
            ..Default::default()
        };
        assert_eq!(
            t.render_url(&values, &["org"]).unwrap(),
            "https://dev.azure.com/acme/My%20App/_workitems/edit/12"
        );
    }

    #[test]
    fn item_templates_read_the_issue() {
        let t = Template::parse("#{number}", TemplateScope::Item, &[]).unwrap();
        let item = json!({"number": 7});
        let values = Values {
            item: Some(&item),
            ..Default::default()
        };
        assert_eq!(t.render(&values).unwrap(), "#7");
    }
}

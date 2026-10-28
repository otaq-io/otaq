//! Paths into a CLI's JSON output: `$` (the whole value), `fields.summary`, `labels[0]`, `labels[].name` (each
//! element), and `fields["System.Title"]` for a key with dots. No filters, no wildcards: a mapping stays readable.

use std::fmt;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Key(String),
    Index(usize),
    Each,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonPath {
    text: String,
    steps: Vec<Step>,
}

impl JsonPath {
    pub fn parse(text: &str) -> Result<Self, String> {
        let bad = || format!("\"{text}\" isn't a path");
        let trimmed = text.trim();
        let mut rest = match trimmed.strip_prefix('$') {
            Some(after) => after.strip_prefix('.').unwrap_or(after),
            None => trimmed,
        };
        let mut steps = Vec::new();
        let mut first = true;
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix('[') {
                let close = after.find(']').ok_or_else(bad)?;
                let inner = &after[..close];
                if inner.is_empty() {
                    steps.push(Step::Each);
                } else if let Some(quoted) = quoted(inner) {
                    steps.push(Step::Key(quoted.to_owned()));
                } else {
                    steps.push(Step::Index(inner.parse().map_err(|_| bad())?));
                }
                rest = &after[close + 1..];
            } else {
                let after = if first {
                    rest
                } else {
                    rest.strip_prefix('.').ok_or_else(bad)?
                };
                let end = after.find(['.', '[']).unwrap_or(after.len());
                let key = &after[..end];
                if key.is_empty() || key.contains(['"', '\'', ']', '{', '}', ' ']) {
                    return Err(bad());
                }
                steps.push(Step::Key(key.to_owned()));
                rest = &after[end..];
            }
            first = false;
        }
        Ok(Self {
            text: trimmed.to_owned(),
            steps,
        })
    }

    /// Every value the path reaches (several through `[]`).
    pub fn all<'v>(&self, value: &'v Value) -> Vec<&'v Value> {
        let mut current = vec![value];
        for step in &self.steps {
            let mut next = Vec::new();
            for v in current {
                match step {
                    Step::Key(k) => next.extend(v.get(k)),
                    Step::Index(n) => next.extend(v.get(n)),
                    Step::Each => next.extend(v.as_array().into_iter().flatten()),
                }
            }
            current = next;
        }
        current.retain(|v| !v.is_null());
        current
    }

    /// The first value the path reaches.
    pub fn first<'v>(&self, value: &'v Value) -> Option<&'v Value> {
        self.all(value).into_iter().next()
    }
}

fn quoted(inner: &str) -> Option<&str> {
    let inner = inner.trim();
    inner
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| inner.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
}

impl fmt::Display for JsonPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_keys_indexes_and_lists() {
        let v = json!({
            "fields": {"System.Title": "Fix", "summary": "S", "labels": [{"name": "P1"}, {"name": "bug"}]},
            "list": [1, 2]
        });
        let get = |p: &str| JsonPath::parse(p).unwrap().first(&v).cloned();
        assert_eq!(get("fields.summary"), Some(json!("S")));
        assert_eq!(get("fields[\"System.Title\"]"), Some(json!("Fix")));
        assert_eq!(get("$.fields['System.Title']"), Some(json!("Fix")));
        assert_eq!(get("list[1]"), Some(json!(2)));
        assert_eq!(get("$"), Some(v.clone()));
        assert_eq!(get("fields.missing"), None);
        let names = JsonPath::parse("fields.labels[].name").unwrap();
        assert_eq!(names.all(&v), [&json!("P1"), &json!("bug")]);
    }

    #[test]
    fn refuses_what_isnt_a_path() {
        for bad in ["a..b", "a[", "a[x]", "a b", "{a}", "a.\"b\""] {
            assert!(JsonPath::parse(bad).is_err(), "{bad}");
        }
    }
}

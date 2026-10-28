//! The built-in transforms (the otaq-marketplace README, "How a plugin works"): Jira's document format (ADF)
//! and Azure DevOps' HTML, as Markdown for a ticket's body. Small on purpose: headings, paragraphs, emphasis, code,
//! links, lists, quotes and rules; anything else keeps its text.

use serde_json::Value;

/// Atlassian Document Format (a JSON tree) as Markdown. A plain string passes through.
pub fn adf_to_markdown(doc: &Value) -> String {
    if let Some(text) = doc.as_str() {
        return text.trim().to_owned();
    }
    let mut out = String::new();
    blocks(doc, &mut out, 0);
    tidy(&out)
}

fn children(node: &Value) -> impl Iterator<Item = &Value> {
    node.get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn blocks(node: &Value, out: &mut String, depth: usize) {
    for child in children(node) {
        block(child, out, depth);
    }
}

fn block(node: &Value, out: &mut String, depth: usize) {
    let kind = node.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "paragraph" => {
            out.push_str(&inline(node));
            out.push_str("\n\n");
        }
        "heading" => {
            let level = node
                .pointer("/attrs/level")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 6) as usize;
            out.push_str(&"#".repeat(level));
            out.push(' ');
            out.push_str(&inline(node));
            out.push_str("\n\n");
        }
        "bulletList" | "orderedList" => {
            let ordered = kind == "orderedList";
            for (n, item) in children(node).enumerate() {
                let marker = if ordered {
                    format!("{}. ", n + 1)
                } else {
                    "- ".to_owned()
                };
                let mut inner = String::new();
                blocks(item, &mut inner, depth + 1);
                list_item(out, depth, &marker, &inner);
            }
            if depth == 0 {
                out.push('\n');
            }
        }
        "taskList" => {
            for item in children(node) {
                let done = item.pointer("/attrs/state").and_then(Value::as_str) == Some("DONE");
                let marker = if done { "- [x] " } else { "- [ ] " };
                list_item(out, depth, marker, &inline(item));
            }
            if depth == 0 {
                out.push('\n');
            }
        }
        "codeBlock" => {
            let language = node
                .pointer("/attrs/language")
                .and_then(Value::as_str)
                .unwrap_or("");
            out.push_str(&format!("```{language}\n{}\n```\n\n", plain(node)));
        }
        "blockquote" | "panel" => {
            let mut inner = String::new();
            blocks(node, &mut inner, 0);
            for line in tidy(&inner).lines() {
                out.push_str(if line.is_empty() { ">" } else { "> " });
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }
        "rule" => out.push_str("---\n\n"),
        "mediaSingle" | "mediaGroup" => out.push_str("_(attachment)_\n\n"),
        "table" => {
            for row in children(node) {
                let cells: Vec<String> = children(row)
                    .map(|cell| {
                        let mut inner = String::new();
                        blocks(cell, &mut inner, 0);
                        tidy(&inner).replace('\n', " ")
                    })
                    .collect();
                out.push_str(&format!("| {} |\n", cells.join(" | ")));
            }
            out.push('\n');
        }
        _ if node.get("content").is_some() => blocks(node, out, depth),
        _ => {
            let text = inline_node(node);
            if !text.is_empty() {
                out.push_str(&text);
                out.push_str("\n\n");
            }
        }
    }
}

fn list_item(out: &mut String, depth: usize, marker: &str, inner: &str) {
    let indent = "  ".repeat(depth);
    let inner = tidy(inner);
    let mut lines = inner.lines();
    out.push_str(&format!("{indent}{marker}{}\n", lines.next().unwrap_or("")));
    for line in lines.filter(|l| !l.is_empty()) {
        let continued = if line.starts_with("  ") || line.starts_with("- ") {
            line.to_owned()
        } else {
            format!("  {line}")
        };
        out.push_str(&format!("{indent}{continued}\n"));
    }
}

fn inline(node: &Value) -> String {
    children(node).map(inline_node).collect()
}

fn inline_node(node: &Value) -> String {
    match node.get("type").and_then(Value::as_str).unwrap_or("") {
        "text" => {
            let mut text = node
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            for mark in node
                .get("marks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                text = match mark.get("type").and_then(Value::as_str) {
                    Some("strong") => format!("**{text}**"),
                    Some("em") => format!("_{text}_"),
                    Some("code") => format!("`{text}`"),
                    Some("strike") => format!("~~{text}~~"),
                    Some("link") => match mark.pointer("/attrs/href").and_then(Value::as_str) {
                        Some(href) => format!("[{text}]({href})"),
                        None => text,
                    },
                    _ => text,
                };
            }
            text
        }
        "hardBreak" => "\n".to_owned(),
        "mention" => node
            .pointer("/attrs/text")
            .and_then(Value::as_str)
            .unwrap_or("@someone")
            .to_owned(),
        "emoji" => node
            .pointer("/attrs/text")
            .or_else(|| node.pointer("/attrs/shortName"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        "inlineCard" => node
            .pointer("/attrs/url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        _ => inline(node),
    }
}

/// A node's text with no marks (a code block's).
fn plain(node: &Value) -> String {
    children(node)
        .map(|c| match c.get("text").and_then(Value::as_str) {
            Some(t) => t.to_owned(),
            None => plain(c),
        })
        .collect()
}

/// HTML (Azure DevOps' descriptions) as Markdown.
pub fn html_to_markdown(html: &str) -> String {
    let mut out = String::new();
    let mut lists: Vec<(bool, usize)> = Vec::new();
    let mut link: Option<String> = None;
    let mut pre = false;
    let mut rest = html;
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            text(&mut out, rest, pre);
            break;
        };
        text(&mut out, &rest[..open], pre);
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else {
            text(&mut out, &rest[open..], pre);
            break;
        };
        let tag = &after[..close];
        rest = &after[close + 1..];
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match (name.as_str(), closing) {
            ("p" | "div", _) => paragraph(&mut out),
            ("br", _) => out.push('\n'),
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                paragraph(&mut out);
                let level = name[1..].parse::<usize>().unwrap_or(1);
                out.push_str(&"#".repeat(level));
                out.push(' ');
            }
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", true) => paragraph(&mut out),
            ("strong" | "b", _) => out.push_str("**"),
            ("em" | "i", _) => out.push('_'),
            ("code", _) if !pre => out.push('`'),
            ("pre", false) => {
                paragraph(&mut out);
                out.push_str("```\n");
                pre = true;
            }
            ("pre", true) => {
                out.push_str("\n```");
                pre = false;
                paragraph(&mut out);
            }
            ("ul" | "ol", false) => {
                if lists.is_empty() {
                    paragraph(&mut out);
                }
                lists.push((name == "ol", 0));
            }
            ("ul" | "ol", true) => {
                lists.pop();
                if lists.is_empty() {
                    paragraph(&mut out);
                }
            }
            ("li", false) => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                let depth = lists.len().saturating_sub(1);
                out.push_str(&"  ".repeat(depth));
                match lists.last_mut() {
                    Some((true, n)) => {
                        *n += 1;
                        out.push_str(&format!("{n}. "));
                    }
                    _ => out.push_str("- "),
                }
            }
            ("a", false) => {
                link = attribute(tag, "href");
                out.push('[');
            }
            ("a", true) => match link.take() {
                Some(href) => out.push_str(&format!("]({href})")),
                None => out.push(']'),
            },
            ("img", _) => out.push_str("_(image)_"),
            ("hr", _) => {
                paragraph(&mut out);
                out.push_str("---");
                paragraph(&mut out);
            }
            ("blockquote", false) => {
                paragraph(&mut out);
                out.push_str("> ");
            }
            ("tr", true) => out.push('\n'),
            ("td" | "th", false) => out.push_str("| "),
            ("td" | "th", true) => out.push(' '),
            _ => {}
        }
    }
    tidy(&out)
}

fn paragraph(out: &mut String) {
    let trimmed = out.trim_end_matches([' ', '\n']).len();
    out.truncate(trimmed);
    if !out.is_empty() {
        out.push_str("\n\n");
    }
}

fn text(out: &mut String, raw: &str, pre: bool) {
    let decoded = entities(raw);
    if pre {
        out.push_str(&decoded);
        return;
    }
    let mut last_space = out.ends_with([' ', '\n']) || out.is_empty();
    for c in decoded.chars() {
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(c);
            last_space = false;
        }
    }
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let at = tag.find(&format!("{name}="))?;
    let rest = &tag[at + name.len() + 1..];
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &rest[1..];
    let end = rest.find(quote)?;
    Some(entities(&rest[..end]))
}

fn entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        let entity = after.find(';').filter(|n| *n <= 8).map(|n| &after[..=n]);
        let decoded = entity.and_then(|e| match e {
            "&amp;" => Some("&".to_owned()),
            "&lt;" => Some("<".to_owned()),
            "&gt;" => Some(">".to_owned()),
            "&quot;" => Some("\"".to_owned()),
            "&#39;" | "&apos;" => Some("'".to_owned()),
            "&nbsp;" => Some(" ".to_owned()),
            _ => e
                .strip_prefix("&#")
                .and_then(|n| n.strip_suffix(';'))
                .and_then(|n| match n.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => n.parse().ok(),
                })
                .and_then(char::from_u32)
                .map(String::from),
        });
        match (entity, decoded) {
            (Some(e), Some(d)) => {
                out.push_str(&d);
                rest = &after[e.len()..];
            }
            _ => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Trailing spaces off, at most one empty line in a row, no empty lines at the ends.
fn tidy(text: &str) -> String {
    let mut out = String::new();
    let mut empty = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            empty += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if empty > 0 { "\n\n" } else { "\n" });
        }
        out.push_str(line);
        empty = 0;
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn adf_becomes_markdown() {
        let doc = json!({"type": "doc", "version": 1, "content": [
            {"type": "heading", "attrs": {"level": 2}, "content": [{"type": "text", "text": "Steps"}]},
            {"type": "paragraph", "content": [
                {"type": "text", "text": "Log in "},
                {"type": "text", "text": "fails", "marks": [{"type": "strong"}]},
                {"type": "text", "text": " on "},
                {"type": "text", "text": "Safari", "marks": [{"type": "link", "attrs": {"href": "https://x.test"}}]}
            ]},
            {"type": "bulletList", "content": [
                {"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "one"}]}]},
                {"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "two"}]}]}
            ]},
            {"type": "codeBlock", "attrs": {"language": "rust"}, "content": [{"type": "text", "text": "fn main() {}"}]}
        ]});
        assert_eq!(
            adf_to_markdown(&doc),
            "## Steps\n\nLog in **fails** on [Safari](https://x.test)\n\n- one\n- two\n\n```rust\nfn main() {}\n```"
        );
        assert_eq!(adf_to_markdown(&json!("plain")), "plain");
    }

    #[test]
    fn html_becomes_markdown() {
        let html = "<div>Users <b>can't</b> log&nbsp;in.</div><div><br></div>\
                    <ul><li>Open <a href=\"https://x.test/a?b=1&amp;c=2\">the page</a></li><li>Click</li></ul>\
                    <h2>Notes</h2><p>5 &lt; 6</p><pre>let a = 1;\n  b</pre>";
        assert_eq!(
            html_to_markdown(html),
            "Users **can't** log in.\n\n- Open [the page](https://x.test/a?b=1&c=2)\n- Click\n\n## Notes\n\n5 < 6\n\n\
             ```\nlet a = 1;\n  b\n```"
        );
        assert_eq!(
            html_to_markdown("<ol><li>a</li><li>b</li></ol>"),
            "1. a\n2. b"
        );
        assert_eq!(
            html_to_markdown("plain &#x41; &unknown;"),
            "plain A &unknown;"
        );
    }
}

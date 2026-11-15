//! A comment's words without its Markdown marks, for one-line previews (tuq 110): History's "commented" line, a
//! failed check's card line, the Overview's attachment rows. A full comment (the thread, `prime`, `otaq comments`) is
//! never stripped.
//!
//! - `**Done**`, `*x*`, `_x_`, `~~x~~` → the words inside. A lone `*` or `_` that isn't a paired mark stays
//!   (`before-*`, `src/*.rs`, `snake_case`).
//! - `` `x` `` → `x`, the text inside written as is.
//! - `[text](url)` → `text`; `![alt](src)` → `alt` (its `src` without alt text); `<https://…>` → the URL.
//! - A leading `#`, `>`, `-`, `*`, `+`, `1.`, `1)` and a task box (`- [ ]`, `- [x]`) is dropped; so are code fence
//!   lines, rules (`---`) and a table's `|---|` row. The lines inside a code fence are kept as written.
//! - `\*` → `*`. HTML is never interpreted: a tag stays as text.
//! - Lines are joined with one space; blank lines go.
//!
//! Callers cut the result to their length afterwards, so no half-mark is left at the cut.

/// `markdown`'s words on one line, without its marks.
pub fn plain(markdown: &str) -> String {
    plain_lines(markdown).join(" ")
}

/// `markdown`'s non-empty lines without their marks, for a preview of its first line (a failed check's verdict, an
/// attachment's row). Code fences are followed across lines, so a line inside one is kept as written.
pub fn plain_lines(markdown: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for line in markdown.lines() {
        let trimmed = line.trim();
        if let Some((mark, len)) = fence_mark(trimmed) {
            match fence {
                None => {
                    fence = Some((mark, len));
                    continue;
                }
                Some((open, open_len)) if open == mark && len >= open_len => {
                    fence = None;
                    continue;
                }
                Some(_) => {}
            }
        }
        if fence.is_some() {
            if !trimmed.is_empty() {
                out.push(trimmed.to_owned());
            }
            continue;
        }
        if is_rule(trimmed) || is_table_rule(trimmed) {
            continue;
        }
        let words = inline(&block_marks_off(trimmed).chars().collect::<Vec<_>>());
        let words = words.trim();
        if !words.is_empty() {
            out.push(words.to_owned());
        }
    }
    out
}

/// A code fence's mark and length (```` ``` ````, `~~~`, with or without an info string).
fn fence_mark(line: &str) -> Option<(char, usize)> {
    let mark = line.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let len = line.chars().take_while(|c| *c == mark).count();
    // A backtick fence's info string has no backtick (else it's an inline code span).
    (len >= 3 && !(mark == '`' && line[len..].contains('`'))).then_some((mark, len))
}

/// `---`, `***`, `___`, `===` (a setext underline or a rule), spaces allowed.
fn is_rule(line: &str) -> bool {
    let mut chars = line.chars().filter(|c| !c.is_whitespace());
    let Some(first) = chars.next() else {
        return false;
    };
    matches!(first, '-' | '*' | '_' | '=')
        && line.chars().filter(|c| *c == first).count() >= 3
        && chars.all(|c| c == first)
}

/// A table's `| --- | :---: |` row.
fn is_table_rule(line: &str) -> bool {
    line.contains('|')
        && line.contains('-')
        && line
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// `line` without its leading quote, heading, list and task marks, nested ones too (`> - [ ] x` → `x`).
fn block_marks_off(line: &str) -> &str {
    let mut rest = line;
    loop {
        let before = rest;
        if let Some(r) = rest.strip_prefix('>') {
            rest = r.trim_start();
        } else if let Some(r) = heading_off(rest) {
            rest = r;
        } else if let Some(r) = bullet_off(rest) {
            rest = task_off(r).unwrap_or(r);
        } else if let Some(r) = number_off(rest) {
            rest = task_off(r).unwrap_or(r);
        }
        if rest.len() == before.len() {
            return rest;
        }
    }
}

/// Spaces or the line's end after a mark.
fn after_mark(rest: &str) -> Option<&str> {
    (rest.is_empty() || rest.starts_with([' ', '\t'])).then(|| rest.trim_start())
}

fn heading_off(line: &str) -> Option<&str> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = after_mark(&line[hashes..])?;
    // An ATX heading's closing `#`s.
    let closed = rest.trim_end_matches('#');
    Some(
        if closed.len() < rest.len() && (closed.is_empty() || closed.ends_with(' ')) {
            closed.trim_end()
        } else {
            rest
        },
    )
}

fn bullet_off(line: &str) -> Option<&str> {
    let rest = line.strip_prefix(['-', '*', '+'])?;
    after_mark(rest)
}

fn number_off(line: &str) -> Option<&str> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if !(1..=9).contains(&digits) {
        return None;
    }
    let rest = line[digits..].strip_prefix(['.', ')'])?;
    after_mark(rest)
}

fn task_off(item: &str) -> Option<&str> {
    let rest = item
        .strip_prefix("[ ]")
        .or_else(|| item.strip_prefix("[x]"))
        .or_else(|| item.strip_prefix("[X]"))?;
    after_mark(rest)
}

/// The inline marks off one line.
fn inline(c: &[char]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        match ch {
            '\\' if c.get(i + 1).is_some_and(char::is_ascii_punctuation) => {
                out.push(c[i + 1]);
                i += 2;
            }
            '`' => {
                let n = run(c, i);
                match code_end(c, i, n) {
                    Some(end) => {
                        out.extend(code_text(&c[i + n..end]));
                        i = end + n;
                    }
                    None => {
                        out.extend(&c[i..i + n]);
                        i += n;
                    }
                }
            }
            '!' if c.get(i + 1) == Some(&'[') => match link(c, i + 1) {
                Some((text, dest, end)) => {
                    let alt = inline(text);
                    match alt.trim() {
                        "" => out.push_str(&destination(dest)),
                        alt => out.push_str(alt),
                    }
                    i = end;
                }
                None => {
                    out.push('!');
                    i += 1;
                }
            },
            '[' => match link(c, i) {
                Some((text, dest, end)) => {
                    let words = inline(text);
                    match words.trim() {
                        "" => out.push_str(&destination(dest)),
                        _ => out.push_str(&words),
                    }
                    i = end;
                }
                None => {
                    out.push('[');
                    i += 1;
                }
            },
            '<' => match autolink(c, i) {
                Some(end) => {
                    out.extend(&c[i + 1..end]);
                    i = end + 1;
                }
                None => {
                    out.push('<');
                    i += 1;
                }
            },
            '*' | '_' | '~' => {
                let n = run(c, i);
                match emphasis_end(c, i, n) {
                    Some(end) => {
                        out.push_str(&inline(&c[i + n..end]));
                        i = end + n;
                    }
                    None => {
                        out.extend(&c[i..i + n]);
                        i += n;
                    }
                }
            }
            _ => {
                out.push(ch);
                i += 1;
            }
        }
    }
    out
}

/// How many of `c[at]` follow one another from `at`.
fn run(c: &[char], at: usize) -> usize {
    c[at..].iter().take_while(|x| **x == c[at]).count()
}

/// Where the code span opened by `n` backticks at `at` closes: the next run of exactly `n`.
fn code_end(c: &[char], at: usize, n: usize) -> Option<usize> {
    let mut j = at + n;
    while j < c.len() {
        if c[j] == '`' {
            let m = run(c, j);
            if m == n {
                return Some(j);
            }
            j += m;
        } else {
            j += 1;
        }
    }
    None
}

/// A code span's text as written, without the one space each side that lets it start or end with a backtick.
fn code_text(inner: &[char]) -> &[char] {
    match inner {
        [' ', middle @ .., ' '] if middle.iter().any(|c| *c != ' ') => middle,
        _ => inner,
    }
}

/// Where the emphasis opened by the run of `n` marks at `at` closes, if it is a paired mark: it opens at a word's start
/// before a non-space and closes on a run of the same length after a non-space, at a word's end.
fn emphasis_end(c: &[char], at: usize, n: usize) -> Option<usize> {
    let mark = c[at];
    if n > 3 || (mark == '~' && n != 2) {
        return None;
    }
    let opens_word = at == 0
        || c[at - 1].is_whitespace()
        || matches!(
            c[at - 1],
            '(' | '[' | '{' | '"' | '\'' | '\u{201c}' | '\u{2018}'
        );
    let starts_text = c.get(at + n).is_some_and(|x| !x.is_whitespace());
    if !opens_word || !starts_text {
        return None;
    }
    let mut j = at + n;
    while j < c.len() {
        match c[j] {
            '`' => {
                let m = run(c, j);
                j = code_end(c, j, m).map_or(j + m, |end| end + m);
            }
            x if x == mark => {
                let m = run(c, j);
                let after_text = !c[j - 1].is_whitespace();
                let ends_word = c.get(j + m).is_none_or(|x| !x.is_alphanumeric());
                if m == n && after_text && ends_word && j > at + n {
                    return Some(j);
                }
                j += m;
            }
            _ => j += 1,
        }
    }
    None
}

/// `[text](destination)` from the `[` at `at`: the text, the destination and the index after the `)`.
fn link(c: &[char], at: usize) -> Option<(&[char], &[char], usize)> {
    let close = matching(c, at, '[', ']')?;
    if c.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = matching(c, close + 1, '(', ')')?;
    Some((&c[at + 1..close], &c[close + 2..end], end + 1))
}

/// The index of the `close` that matches the `open` at `at`, nested pairs and escapes skipped.
fn matching(c: &[char], at: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    let mut j = at;
    while j < c.len() {
        match c[j] {
            '\\' => j += 1,
            x if x == open => depth += 1,
            x if x == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// A link's destination without its title or angle brackets: `<a b> "title"` → `a b`.
fn destination(dest: &[char]) -> String {
    let dest: String = dest.iter().collect();
    let dest = dest.trim();
    if let Some(inner) = dest.strip_prefix('<')
        && let Some(end) = inner.find('>')
    {
        return inner[..end].to_owned();
    }
    dest.split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// The `>` closing an autolink (`<https://example.com>`) opened at `at`: a scheme, `:` and no space or `<`.
fn autolink(c: &[char], at: usize) -> Option<usize> {
    let end = at + 1 + c[at + 1..].iter().position(|x| *x == '>')?;
    let inner = &c[at + 1..end];
    let colon = inner.iter().position(|x| *x == ':')?;
    let scheme = &inner[..colon];
    let scheme_ok = (2..=32).contains(&scheme.len())
        && scheme[0].is_ascii_alphabetic()
        && scheme
            .iter()
            .all(|x| x.is_ascii_alphanumeric() || matches!(x, '+' | '.' | '-'));
    let rest_ok = inner[colon + 1..]
        .iter()
        .all(|x| !x.is_whitespace() && *x != '<');
    (scheme_ok && rest_ok).then_some(end)
}

#[cfg(test)]
mod tests {
    use super::{plain, plain_lines};

    #[test]
    fn the_plans_examples() {
        assert_eq!(plain("**Done**"), "Done");
        assert_eq!(plain("`x`"), "x");
        assert_eq!(plain("[text](https://example.com)"), "text");
        assert_eq!(plain("# Heading"), "Heading");
        assert_eq!(plain("- item"), "item");
        assert_eq!(plain("1. first"), "first");
        assert_eq!(plain("> quoted"), "quoted");
        assert_eq!(plain("- [ ] to do"), "to do");
        assert_eq!(plain("- [x] done"), "done");
    }

    #[test]
    fn lines_are_joined_with_spaces() {
        assert_eq!(
            plain(
                "## Summary\n\n- **Reused** the CSV writer\n- Kept `export.rs` as is\n\n1. First\n2) Second"
            ),
            "Summary Reused the CSV writer Kept export.rs as is First Second"
        );
        assert_eq!(plain("one  \ntwo\r\n\n\nthree"), "one two three");
        assert_eq!(plain(""), "");
        assert_eq!(plain("\n  \n"), "");
    }

    #[test]
    fn lines_one_by_one_follow_fences() {
        assert_eq!(
            plain_lines("\n```rust\nlet x = **y**;\n```\n## **Next** line\n"),
            ["let x = **y**;", "Next line"]
        );
        assert!(plain_lines("\n- [ ]\n---\n").is_empty());
    }

    #[test]
    fn emphasis_of_every_kind() {
        assert_eq!(plain("*italic* and _also_"), "italic and also");
        assert_eq!(plain("***both*** and __strong__"), "both and strong");
        assert_eq!(plain("~~gone~~ now"), "gone now");
        assert_eq!(plain("**Fail:** the limit"), "Fail: the limit");
        assert_eq!(plain("(**a**, \"*b*\")"), "(a, \"b\")");
        assert_eq!(plain("_**nested**_"), "nested");
        assert_eq!(plain("**`code` inside**"), "code inside");
    }

    #[test]
    fn lone_marks_inside_words_stay() {
        assert_eq!(
            plain("snake_case_name and my_var"),
            "snake_case_name and my_var"
        );
        assert_eq!(
            plain("tests named before-* run first"),
            "tests named before-* run first"
        );
        assert_eq!(plain("src/*.rs and tests/*.rs"), "src/*.rs and tests/*.rs");
        assert_eq!(plain("2 * 3 * 4"), "2 * 3 * 4");
        assert_eq!(plain("a ** b"), "a ** b");
        assert_eq!(plain("**unclosed"), "**unclosed");
        assert_eq!(plain("~5 min, ~10 min"), "~5 min, ~10 min");
        assert_eq!(
            plain("_private and __dunder__init"),
            "_private and __dunder__init"
        );
    }

    #[test]
    fn code_is_kept_as_written() {
        assert_eq!(plain("Run `**not bold**` now"), "Run **not bold** now");
        assert_eq!(plain("``a `tick` b``"), "a `tick` b");
        assert_eq!(plain("`` `x` ``"), "`x`");
        assert_eq!(
            plain("glob `src/**/*.sql` and `snake_case`"),
            "glob src/**/*.sql and snake_case"
        );
        assert_eq!(plain("an `unclosed span"), "an `unclosed span");
        assert_eq!(
            plain("Fixed:\n```rust\nlet x = *y; // **kept**\n```\nDone."),
            "Fixed: let x = *y; // **kept** Done."
        );
        assert_eq!(plain("~~~\n# not a heading\n~~~"), "# not a heading");
    }

    #[test]
    fn links_images_and_autolinks() {
        assert_eq!(
            plain("See [the **PR**](https://x.y/1 \"title\")."),
            "See the PR."
        );
        assert_eq!(plain("[](https://x.y/1)"), "https://x.y/1");
        assert_eq!(plain("![a screenshot](shot.png) here"), "a screenshot here");
        assert_eq!(plain("![](shot.png)"), "shot.png");
        assert_eq!(plain("[a [nested] one](u(1))"), "a [nested] one");
        assert_eq!(
            plain("go to <https://example.com/a>"),
            "go to https://example.com/a"
        );
        assert_eq!(plain("[just brackets] and [x]"), "[just brackets] and [x]");
    }

    #[test]
    fn html_and_placeholders_stay_text() {
        assert_eq!(
            plain("Observable<number | null>"),
            "Observable<number | null>"
        );
        assert_eq!(
            plain("NN-<page>-<step>.png and <why>"),
            "NN-<page>-<step>.png and <why>"
        );
        assert_eq!(plain("<div>hi</div>"), "<div>hi</div>");
    }

    #[test]
    fn block_marks_nest_and_rules_go() {
        assert_eq!(plain("> - [ ] quoted task"), "quoted task");
        assert_eq!(plain(">> deep"), "deep");
        assert_eq!(plain("### Closed ###"), "Closed");
        assert_eq!(plain("#hashtag stays"), "#hashtag stays");
        assert_eq!(plain("* star item\n+ plus item"), "star item plus item");
        assert_eq!(plain("above\n---\nbelow\n***"), "above below");
        assert_eq!(plain("Title\n====="), "Title");
        assert_eq!(
            plain("| a | b |\n|---|:-:|\n| 1 | 2 |"),
            "| a | b | | 1 | 2 |"
        );
        assert_eq!(plain("2026. A year"), "A year");
        assert_eq!(plain("10,000 rows"), "10,000 rows");
        assert_eq!(plain("-5 degrees"), "-5 degrees");
    }

    #[test]
    fn escapes_drop_their_backslash() {
        assert_eq!(plain(r"\*not italic\* and \_x\_"), "*not italic* and _x_");
        assert_eq!(plain(r"C:\Users\me"), r"C:\Users\me");
    }
}

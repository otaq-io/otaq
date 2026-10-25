//! The skills only teach commands and flags the CLI has (T-0140, T-0539): every `otaq <command> [<subcommand>]
//! [--flag]…` written in `plugin/skills/**/*.md` is found in clap's command tree, the command and each flag on it, so a
//! renamed command or flag fails the build.

use std::path::{Path, PathBuf};

use clap::CommandFactory;
use otaq_core::naming;

use crate::cli::Cli;

/// Every Markdown file under `dir`, in a stable order.
fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(markdown_files(&path));
        } else if path.extension().is_some_and(|e| e == "md") {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// Each `otaq …` command written in `text`, as the words a shell would split it into. The name must start a word
/// (not `OTAQ_…`, `/otaq:…` or a path); the command ends at a closing backtick, `)`, `|`, `;`, `&`, a redirect, a
/// heredoc (`<<`) or the line's end, unless the line ends with `\`. `<KEY>` placeholders are one word.
fn commands(text: &str) -> Vec<Vec<String>> {
    let lead = format!("{} ", naming::COMMAND_NAME);
    let mut found = Vec::new();
    for (at, _) in text.match_indices(&lead) {
        let before = text[..at].chars().last();
        if before.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '/' | ':' | '-' | '.')) {
            continue;
        }
        found.push(words(&text[at + lead.len()..]));
    }
    found
}

/// The words of one command at the start of `rest`, up to where it ends (see [`commands`]).
fn words(rest: &str) -> Vec<String> {
    let (mut words, mut word, mut quote, mut started) = (Vec::new(), String::new(), None, false);
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), '\n') => break,
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => (quote, started) = (Some(c), true),
            (None, '\\') if chars.peek() == Some(&'\n') => {
                chars.next();
            }
            (None, '<') if chars.peek() == Some(&'<') => break,
            (None, '<') => {
                word.push('<');
                for c in chars.by_ref() {
                    word.push(c);
                    if c == '>' {
                        break;
                    }
                }
                started = true;
            }
            (None, '`' | ')' | '|' | ';' | '&' | '>' | '\n') => break,
            (None, c) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                }
                started = false;
            }
            (None, c) => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// A word that names a command: lowercase letters and dashes.
fn is_command_word(word: &str) -> bool {
    word.starts_with(|c: char| c.is_ascii_lowercase())
        && word.chars().all(|c| c.is_ascii_lowercase() || c == '-')
}

/// What's wrong with `words` (the words after `otaq`) against `root`, or `None` when the command and every flag exist.
/// Words that don't name a command at all ("the otaq CLI") are not a command.
fn check(root: &clap::Command, words: &[String]) -> Option<String> {
    let first = words.first()?;
    if !is_command_word(first) {
        return None;
    }
    let mut cmd = root;
    let mut path = Vec::new();
    let mut rest = words.iter();
    for word in rest.by_ref() {
        match cmd.find_subcommand(word) {
            Some(sub) => {
                cmd = sub;
                path.push(word.as_str());
            }
            // A command that takes no arguments of its own needs one of its subcommands here.
            None if is_command_word(word)
                && cmd.has_subcommands()
                && cmd.get_positionals().next().is_none() =>
            {
                return Some(format!("`{word}` isn't a command of `{}`", cmd.get_name()));
            }
            None => break,
        }
        if !cmd.has_subcommands() {
            break;
        }
    }
    for word in words.iter() {
        let known = if let Some(long) = word.strip_prefix("--") {
            let long = long.split('=').next().unwrap_or_default();
            if !long.starts_with(|c: char| c.is_ascii_alphabetic()) {
                continue;
            }
            cmd.get_arguments().any(|a| {
                a.get_long() == Some(long)
                    || a.get_all_aliases().is_some_and(|all| all.contains(&long))
            })
        } else if let Some(short) = word.strip_prefix('-')
            && let [c] = short.chars().collect::<Vec<_>>()[..]
            && c.is_ascii_alphabetic()
        {
            cmd.get_arguments().any(|a| a.get_short() == Some(c))
        } else {
            continue;
        };
        if !known {
            return Some(format!(
                "`{} {}` has no `{word}`",
                naming::COMMAND_NAME,
                path.join(" ")
            ));
        }
    }
    None
}

/// The CLI's command tree, global flags carried down to every subcommand.
fn tree() -> clap::Command {
    let mut root = Cli::command();
    root.build();
    root
}

#[test]
fn every_command_and_flag_a_skill_names_exists() {
    let skills = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/skills");
    let root = tree();
    let (mut checked, mut problems) = (0, Vec::new());
    for file in markdown_files(&skills) {
        let text = std::fs::read_to_string(&file).unwrap();
        for words in commands(&text) {
            if let Some(problem) = check(&root, &words) {
                problems.push(format!("{}: {problem} ({words:?})", file.display()));
            }
            checked += 1;
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(checked > 100, "only {checked} commands found");
}

#[test]
fn commands_are_split_where_a_shell_would_end_them() {
    let cmd = naming::COMMAND_NAME;
    let var = naming::env_var("X");
    let text = format!(
        "Run `{cmd} prime`, then `{cmd} helper start 3 --title \"Wishlist page\" --stdin <<'NOTE'` and\n\
         grep -n x \"$({cmd} report SHOP-19 1 --path)\" | head; {cmd} show <KEY> --stage <name> > out.txt\n\
         {cmd} stage next --stdin \\\n  --manual-check \"It's fine\" <<'C'\n\
         Not /{cmd}:commit, ${var} or the {cmd} CLI."
    );
    let s = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        commands(&text),
        vec![
            s(&["prime"]),
            s(&[
                "helper",
                "start",
                "3",
                "--title",
                "Wishlist page",
                "--stdin"
            ]),
            s(&["report", "SHOP-19", "1", "--path"]),
            s(&["show", "<KEY>", "--stage", "<name>"]),
            s(&["stage", "next", "--stdin", "--manual-check", "It's fine"]),
            s(&["CLI."]),
        ]
    );
}

#[test]
fn a_renamed_command_or_flag_is_found() {
    let root = tree();
    let s = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
    assert_eq!(check(&root, &s(&["helper", "done", "--stdin"])), None);
    assert_eq!(
        check(&root, &s(&["create", "-f", "tickets.md", "-P", "SHOP"])),
        None
    );
    assert_eq!(check(&root, &s(&["ask", "human", "--option", "Yes"])), None);
    assert_eq!(check(&root, &s(&["CLI."])), None);
    let cmd = naming::COMMAND_NAME;
    for (words, problem) in [
        (s(&["notes"]), format!("`notes` isn't a command of `{cmd}`")),
        (
            s(&["helper", "finish"]),
            "`finish` isn't a command of `helper`".to_owned(),
        ),
        (
            s(&["helper", "done", "--manual"]),
            format!("`{cmd} helper done` has no `--manual`"),
        ),
        (
            s(&["create", "--type", "epic"]),
            format!("`{cmd} create` has no `--type`"),
        ),
        (
            s(&["delete", "SHOP-4", "-x"]),
            format!("`{cmd} delete` has no `-x`"),
        ),
    ] {
        assert_eq!(check(&root, &words), Some(problem), "{words:?}");
    }
}

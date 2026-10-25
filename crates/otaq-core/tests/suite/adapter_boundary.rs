//! Decision 0088 (ADP-01): each agent tool is known only by its adapter, `otaq-core/src/agent_tools/<tool>/`. This
//! test reads the source of the core, the CLI and the view models and fails with the file and line wherever one of
//! [`PATTERNS`] appears outside the adapters' folders ([`ADAPTERS`]) and outside the named [`EXCEPTIONS`]. Claude
//! Code's patterns came with M20, Codex's, Copilot's and OpenCode's with M23.
//!
//! What is read: code only. Comment lines (`//`, `///`, `//!`) are skipped, because the interface's docs name each
//! tool's flags and files on purpose (`agent_tools/mod.rs`) and a caller's comment may say what a tool does; the
//! code is what acts on a tool. Unit-test modules (`#[cfg(test)] mod … { … }`) are skipped too: like `tests/`,
//! they feed real-shaped Claude data and stored values (`"tool": "claude"`) to the code under test.

use std::fs;
use std::path::{Path, PathBuf};

/// What only the Claude Code adapter may say, and what each one is.
const PATTERNS: &[(&str, &str)] = &[
    (
        "claude_session_id",
        "the old session column (now the tool session id)",
    ),
    (
        "runs_claude",
        "the old session-kind check (now the session's tool)",
    ),
    ("\"claude\"", "`claude` as a program or stored name"),
    ("CLAUDE_CONFIG_DIR", "Claude Code's config folder variable"),
    (".claude.json", "Claude Code's config file"),
    ("--session-id", "Claude Code's launch flag"),
    ("notification_type", "a Claude Code hook payload field"),
    ("rate_limits", "Claude Code's status-line field"),
    (
        "claude exited",
        "a tool named in a History row; word it through `command_name` (Q77)",
    ),
    (
        "cache_creation_input_tokens",
        "a Claude Code transcript field",
    ),
    (
        "Tool::Claude",
        "branching on the tool instead of asking its adapter",
    ),
    (
        "agent_tools::claude",
        "the Claude Code adapter's own module, reached past the interface",
    ),
    // Codex (ADP-07).
    ("\"codex\"", "`codex` as a program or stored name"),
    ("\"codex ", "a Codex command line"),
    (
        "\".codex",
        "Codex's home folder (`~/.codex`) or a project's `.codex`",
    ),
    (
        "CODEX_",
        "Codex's environment variables (`CODEX_HOME`, `CODEX_THREAD_ID`)",
    ),
    ("config.toml", "Codex's config file"),
    (
        "codex exited",
        "a tool named in a History row; word it through `command_name` (Q77)",
    ),
    (
        "Tool::Codex",
        "branching on the tool instead of asking its adapter",
    ),
    (
        "agent_tools::codex",
        "the Codex adapter's own module, reached past the interface",
    ),
    // GitHub Copilot (ADP-08).
    ("\"copilot\"", "`copilot` as a program or stored name"),
    ("\"copilot ", "a Copilot command line"),
    ("\".copilot", "Copilot's home folder (`~/.copilot`)"),
    (
        "COPILOT_",
        "Copilot's environment variables (`COPILOT_HOME`)",
    ),
    ("events.jsonl", "Copilot's session events file"),
    (
        "copilot exited",
        "a tool named in a History row; word it through `command_name` (Q77)",
    ),
    ("--allow-tool", "Copilot's launch flag"),
    (
        "Tool::Copilot",
        "branching on the tool instead of asking its adapter",
    ),
    (
        "agent_tools::copilot",
        "the Copilot adapter's own module, reached past the interface",
    ),
    // OpenCode (ADP-09).
    ("\"opencode\"", "`opencode` as a program or stored name"),
    ("\"opencode ", "an OpenCode command line"),
    (
        "OPENCODE_",
        "OpenCode's environment variables (`OPENCODE_CONFIG_CONTENT`)",
    ),
    ("--standalone", "OpenCode's launch flag"),
    (
        "opencode exited",
        "a tool named in a History row; word it through `command_name` (Q77)",
    ),
    (
        "Tool::OpenCode",
        "branching on the tool instead of asking its adapter",
    ),
    (
        "agent_tools::opencode",
        "the OpenCode adapter's own module, reached past the interface",
    ),
];

/// The adapters: everything under these folders may name its tool (and an adapter may share a flag's spelling with
/// another tool's, such as Copilot's `--session-id`).
const ADAPTERS: &[&str] = &[
    "otaq-core/src/agent_tools/claude/",
    "otaq-core/src/agent_tools/codex/",
    "otaq-core/src/agent_tools/copilot/",
    "otaq-core/src/agent_tools/opencode/",
];

/// A file (or folder, ending in `/`) under `crates/` that may use one pattern, and why.
struct Exception {
    path: &'static str,
    pattern: &'static str,
    reason: &'static str,
}

const EXCEPTIONS: &[Exception] = &[
    Exception {
        path: "otaq-core/src/agent_tools/mod.rs",
        pattern: "\"claude\"",
        reason: "the `Tool` enum's stored name for Claude Code",
    },
    Exception {
        path: "otaq-core/src/agent_tools/mod.rs",
        pattern: "Tool::Claude",
        reason: "the registry: one arm per tool, picking its adapter",
    },
    Exception {
        path: "otaq-core/src/agent_tools/mod.rs",
        pattern: "\"codex\"",
        reason: "the `Tool` enum's stored name for Codex",
    },
    Exception {
        path: "otaq-core/src/agent_tools/mod.rs",
        pattern: "\"copilot\"",
        reason: "the `Tool` enum's stored name for Copilot",
    },
    Exception {
        path: "otaq-core/src/agent_tools/mod.rs",
        pattern: "\"opencode\"",
        reason: "the `Tool` enum's stored name for OpenCode",
    },
    Exception {
        path: "otaq-core/src/agent_tools/mod.rs",
        pattern: "Tool::Codex",
        reason: "the registry: one arm per tool, picking its adapter",
    },
    Exception {
        path: "otaq-core/src/agent_tools/mod.rs",
        pattern: "Tool::Copilot",
        reason: "the registry: one arm per tool, picking its adapter",
    },
    Exception {
        path: "otaq-core/src/agent_tools/mod.rs",
        pattern: "Tool::OpenCode",
        reason: "the registry: one arm per tool, picking its adapter",
    },
    Exception {
        path: "otaq-core/src/tools.rs",
        pattern: "\"claude\"",
        reason: "the checklist's `claude` variant, no row since M23: Claude Code's adapter detects through it (SET-02)",
    },
    Exception {
        path: "otaq-core/src/tools.rs",
        pattern: "Tool::Claude",
        reason: "the checklist's `claude` variant, no row since M23: Claude Code's adapter detects through it (SET-02)",
    },
    Exception {
        path: "otaq-core/src/tools.rs",
        pattern: "CLAUDE_CONFIG_DIR",
        reason: "the checklist's permission-mode warning reads Claude Code's settings (d0011, d0016)",
    },
    Exception {
        path: "otaq-view/src/view_models/onboarding.rs",
        pattern: "Tool::Claude",
        reason: "the Tools checklist's rows in onboarding (SET-02, `tools::Tool`, not the agent tool)",
    },
    Exception {
        path: "otaq-core/src/service/mod.rs",
        pattern: "agent_tools::claude",
        reason: "the service builds the registry: the Claude Code adapter and its config file (d0088)",
    },
    Exception {
        path: "otaq-cli/src/statusline.rs",
        pattern: "agent_tools::claude",
        reason: "Claude Code's status line (USG-04) is `otaq statusline`, which calls into the adapter",
    },
    Exception {
        path: "otaq-cli/src/statusline.rs",
        pattern: "Tool::Claude",
        reason: "Claude Code's status line reports Claude Code's limit windows",
    },
    Exception {
        path: "otaq-view/src/view_models/tool_avatar.rs",
        pattern: "Tool::Claude",
        reason: "the tool avatar (d0087): the stored tool to its letters and colour token, one arm per tool",
    },
    Exception {
        path: "otaq-view/src/view_models/tool_avatar.rs",
        pattern: "Tool::Codex",
        reason: "the tool avatar (d0087): the stored tool to its letters and colour token, one arm per tool",
    },
    Exception {
        path: "otaq-view/src/view_models/tool_avatar.rs",
        pattern: "Tool::Copilot",
        reason: "the tool avatar (d0087): the stored tool to its letters and colour token, one arm per tool",
    },
    Exception {
        path: "otaq-view/src/view_models/tool_avatar.rs",
        pattern: "Tool::OpenCode",
        reason: "the tool avatar (d0087): the stored tool to its letters and colour token, one arm per tool",
    },
    // Sample data outside `#[cfg(test)]`: Claude Code is the tool the samples ran on.
    Exception {
        path: "otaq-core/src/service/testing.rs",
        pattern: "Tool::Claude",
        reason: "the test service (for `tests/`) records a session as an earlier Claude Code start would",
    },
    Exception {
        path: "otaq-view/src/view_models/activity.rs",
        pattern: "Tool::Claude",
        reason: "`fixtures`, Activity's states for tests and the design gallery",
    },
    Exception {
        path: "otaq-view/src/view_models/activity.rs",
        pattern: "Tool::Codex",
        reason: "`fixtures`, Activity's states for tests and the design gallery (`AvatarTool::Codex`)",
    },
    Exception {
        path: "otaq-view/src/view_models/activity.rs",
        pattern: "Tool::OpenCode",
        reason: "`fixtures`, Activity's states for tests and the design gallery (`AvatarTool::OpenCode`)",
    },
    Exception {
        path: "otaq-view/src/view_models/activity.rs",
        pattern: "Tool::Copilot",
        reason: "`fixtures`, Activity's states for tests and the design gallery (`AvatarTool::Copilot`, T-0364)",
    },
    Exception {
        path: "otaq-view/src/view_models/limit_banner.rs",
        pattern: "Tool::Claude",
        reason: "`fixtures` and `gallery`, the banner's sample lines per tool for tests and the design gallery (T-0364)",
    },
    Exception {
        path: "otaq-view/src/view_models/limit_banner.rs",
        pattern: "Tool::Codex",
        reason: "`fixtures` and `gallery`, the banner's sample lines per tool for tests and the design gallery (T-0364)",
    },
    Exception {
        path: "otaq-view/src/view_models/card_fixtures.rs",
        pattern: "Tool::Claude",
        reason: "`tool_variants`, the M23 cards' sample tools for tests and the design gallery (T-0362)",
    },
    Exception {
        path: "otaq-view/src/view_models/card_fixtures.rs",
        pattern: "Tool::Codex",
        reason: "`tool_variants`, the M23 cards' sample tools for tests and the design gallery (T-0362)",
    },
    Exception {
        path: "otaq-view/src/view_models/card_fixtures.rs",
        pattern: "Tool::Copilot",
        reason: "`tool_variants`, the M23 cards' sample tools for tests and the design gallery (T-0362)",
    },
    Exception {
        path: "otaq-view/src/view_models/agents.rs",
        pattern: "Tool::Claude",
        reason: "`fixtures`, Settings → Agents' sample agents and model lists on every tool for tests (T-0361, T-0405)",
    },
    Exception {
        path: "otaq-view/src/view_models/agents.rs",
        pattern: "Tool::Codex",
        reason: "`fixtures`, Settings → Agents' sample agents and model lists on every tool for tests (T-0361, T-0405)",
    },
    Exception {
        path: "otaq-view/src/view_models/agents.rs",
        pattern: "Tool::Copilot",
        reason: "`fixtures`, Settings → Agents' sample agents and model lists on every tool for tests (T-0361, T-0405)",
    },
    Exception {
        path: "otaq-view/src/view_models/agents.rs",
        pattern: "Tool::OpenCode",
        reason: "`fixtures`, Settings → Agents' sample agents and model lists on every tool for tests (T-0361, T-0405)",
    },
    Exception {
        path: "otaq-view/src/view_models/overview.rs",
        pattern: "Tool::Claude",
        reason: "`fixtures`, SHOP-22's comments and reports for tests and the design gallery (T-0465)",
    },
    Exception {
        path: "otaq-view/src/view_models/overview.rs",
        pattern: "Tool::Codex",
        reason: "`fixtures`, SHOP-22's comments and reports for tests and the design gallery (T-0465)",
    },
    Exception {
        path: "otaq-view/src/view_models/onboarding_page.rs",
        pattern: "agent_tools::claude",
        reason: "the design gallery's trust banners show Claude Code's own failure texts (PRJ-06)",
    },
    Exception {
        path: "otaq-view/src/view_models/agent_tools_page.rs",
        pattern: "Tool::Claude",
        reason: "`docs_url`: only Claude Code's docs explain allowlists and auto mode for the permission warning's link (SET-02, T-0360)",
    },
];

/// Folders under `crates/` that are read.
const SCANNED: &[&str] = &["otaq-core/src", "otaq-cli/src", "otaq-view/src"];

fn crates_folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates folder")
        .to_path_buf()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("readable folder") {
        let path = entry.expect("folder entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The code lines of a Rust source, numbered from 1: comment lines and unit-test modules left out. A test module
/// is `#[cfg(test)]` followed by `mod … {`, up to the `}` at the same indent (rustfmt's layout, which
/// `check.sh` enforces).
fn code_lines(src: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut lines = src.lines().enumerate().peekable();
    while let Some((i, line)) = lines.next() {
        let trimmed = line.trim_start();
        if trimmed == "#[cfg(test)]"
            && let Some((_, next)) = lines.peek()
            && next.trim_start().starts_with("mod ")
            && next.trim_end().ends_with('{')
        {
            let indent = &next[..next.len() - next.trim_start().len()];
            let close = format!("{indent}}}");
            lines.next();
            for (_, inner) in lines.by_ref() {
                if inner.trim_end() == close {
                    break;
                }
            }
            continue;
        }
        if trimmed.starts_with("//") {
            continue;
        }
        out.push((i + 1, line));
    }
    out
}

/// The index of the exception that lets `path` use `pattern`.
fn excepted(path: &str, pattern: &str) -> Option<usize> {
    EXCEPTIONS.iter().position(|e| {
        e.pattern == pattern
            && (e.path == path || (e.path.ends_with('/') && path.starts_with(e.path)))
    })
}

/// Every pattern use outside the adapter and its exceptions (file, line, pattern, the line), and the indexes of
/// the exceptions that were used.
fn scan() -> (Vec<String>, Vec<usize>) {
    let crates = crates_folder();
    let mut files = Vec::new();
    for dir in SCANNED {
        rust_files(&crates.join(dir), &mut files);
    }
    assert!(
        files.len() > 50,
        "expected the crates' source files, found {files:?}"
    );
    let mut offenders = Vec::new();
    let mut used = Vec::new();
    for file in &files {
        let rel = file
            .strip_prefix(&crates)
            .expect("under crates/")
            .to_string_lossy()
            .replace('\\', "/");
        if ADAPTERS.iter().any(|adapter| rel.starts_with(adapter)) {
            continue;
        }
        let src = fs::read_to_string(file).expect("readable source");
        for (line, text) in code_lines(&src) {
            for (pattern, what) in PATTERNS {
                if !text.contains(pattern) {
                    continue;
                }
                match excepted(&rel, pattern) {
                    Some(i) => used.push(i),
                    None => offenders.push(format!(
                        "crates/{rel}:{line}: `{pattern}` ({what}): {}",
                        text.trim()
                    )),
                }
            }
        }
    }
    (offenders, used)
}

#[test]
fn each_tool_is_known_only_by_its_adapter() {
    let (offenders, _) = scan();
    assert!(
        offenders.is_empty(),
        "a tool's specifics outside its adapter ({ADAPTERS:?}): ask the session's adapter \
         (`agent_tools::AgentTool`) instead, or add a named exception with its reason (d0088):\n{}",
        offenders.join("\n")
    );
}

#[test]
fn every_exception_is_still_needed() {
    let (_, used) = scan();
    let crates = crates_folder();
    for (i, e) in EXCEPTIONS.iter().enumerate() {
        assert!(
            crates.join(e.path).exists(),
            "the exception's file is gone: {}",
            e.path
        );
        assert!(
            PATTERNS.iter().any(|(p, _)| *p == e.pattern),
            "the exception names no pattern: {} {}",
            e.path,
            e.pattern
        );
        assert!(
            used.contains(&i),
            "no longer needed, remove it: {} `{}` ({})",
            e.path,
            e.pattern,
            e.reason
        );
    }
}

#[test]
fn the_scanner_skips_comments_and_test_modules() {
    let src = "/// `claude --session-id`\n\
               fn a() { run(\"claude\") }\n\
               \x20   // rate_limits\n\
               #[cfg(test)]\n\
               mod tests {\n\
               \x20   fn b() { \"claude\" }\n\
               \x20   mod inner {\n\
               \x20   }\n\
               }\n\
               fn c() {}\n";
    let lines: Vec<(usize, &str)> = code_lines(src);
    assert_eq!(
        lines,
        [(2, "fn a() { run(\"claude\") }"), (10, "fn c() {}")]
    );
}

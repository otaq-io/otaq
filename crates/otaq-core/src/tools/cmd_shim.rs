//! npm's launchers on Windows (d0088 point 2). npm installs a tool as `codex.cmd`, a batch file that runs
//! `node <the tool's script> %*`; starting the `.cmd` would pass every argument, the first message too, through
//! `cmd.exe`, where `&`, `|`, `%`, `"` and `^` can cut it short or run part of it. Otaq reads the launcher instead
//! and starts what it runs, the arguments untouched; a `.cmd` it can't read doesn't start ([`UnreadableLauncher`]).
//!
//! Read (saved real ones in `tests/fixtures/npm-shims`):
//! - npm 7 and later (`cmd-shim` 4–8): `IF EXIST "%dp0%\node.exe"` sets `_prog` to it, else to `node`, then
//!   `… & "%_prog%"  "%dp0%\node_modules\@openai\codex\bin\codex.js" %*`;
//! - the older two-branch form (npm 6, Yarn 1, Node's own `corepack.cmd`): `IF EXIST "%~dp0\node.exe" (` the
//!   line with it `) ELSE (` the line with `node` `)`;
//! - pnpm's (`@zkochan/cmd-shim`): the same, with `NODE_PATH` set first;
//! - any one line `"<program>" <arguments> %*` (a script run with its interpreter, a test's stand-in).
//!
//! Anything else is refused, Node's own hand-written `npm.cmd` included. Bun's launchers are `.exe` files and run
//! as they are. The reader is plain text work, so its tests run on every OS; only Windows uses it.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// What starts for a program: the program itself, or the program a launcher runs with the arguments it puts
/// first (`node.exe` and the tool's script).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    /// The launcher's arguments, before the caller's.
    pub leading: Vec<OsString>,
    /// The folders a pnpm launcher puts first on `NODE_PATH`.
    pub node_path: Option<OsString>,
}

impl Launch {
    fn plain(program: PathBuf) -> Self {
        Self {
            program,
            leading: Vec::new(),
            node_path: None,
        }
    }

    /// `NODE_PATH` for the program when the launcher sets it: its folders, then `existing` (what the program would
    /// get otherwise).
    pub fn node_path_with(&self, existing: Option<&OsStr>) -> Option<OsString> {
        let mut value = self.node_path.clone()?;
        if let Some(existing) = existing.filter(|e| !e.is_empty()) {
            value.push(";");
            value.push(existing);
        }
        Some(value)
    }

    /// A command that starts it, with the launcher's arguments; the caller adds its own.
    pub fn command(&self) -> std::process::Command {
        let mut command = std::process::Command::new(&self.program);
        command.args(&self.leading);
        if let Some(node_path) = self.node_path_with(std::env::var_os("NODE_PATH").as_deref()) {
            command.env("NODE_PATH", node_path);
        }
        command
    }
}

/// A `.cmd` or `.bat` Otaq can't read as an npm launcher (the user's own script, most likely): it isn't started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableLauncher {
    pub file: PathBuf,
}

impl UnreadableLauncher {
    /// As a start problem says it, with the fix: "Codex is started through your own script file
    /// (C:\tools\codex.cmd), which Otaq can't run safely. Install Codex the normal way, or start it without the
    /// script."
    pub fn message(&self, tool: Option<&str>) -> String {
        let (name, again) = match tool {
            Some(tool) => (tool, tool),
            None => ("The tool", "it"),
        };
        format!(
            "{name} is started through your own script file ({}), which Otaq can\u{2019}t run safely. Install \
             {again} the normal way, or start it without the script.",
            self.file.display()
        )
    }
}

impl fmt::Display for UnreadableLauncher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message(None))
    }
}

impl std::error::Error for UnreadableLauncher {}

/// What starts for `program` with `path` as its PATH. On Windows a bare name is found on `path` with `PATHEXT`
/// ([`super::find_on_path`]; a name not found stays as it is, and fails to start as before), and a `.cmd` or `.bat`
/// is read as an npm launcher. Elsewhere, and for any other file, the program itself.
pub fn launch(program: &OsStr, path: &OsStr) -> Result<Launch, UnreadableLauncher> {
    let mut program = PathBuf::from(program);
    if !cfg!(windows) {
        return Ok(Launch::plain(program));
    }
    let bare = !program.as_os_str().to_string_lossy().contains(['/', '\\']);
    if bare && let Some(found) = super::find_on_path(&program.to_string_lossy(), path) {
        program = found;
    }
    if !is_batch(&program) {
        return Ok(Launch::plain(program));
    }
    std::fs::read(&program)
        .ok()
        .and_then(|bytes| read(&String::from_utf8_lossy(&bytes), &program, path))
        .ok_or(UnreadableLauncher { file: program })
}

/// [`launch`] as a command, for a program that isn't started in a terminal.
pub fn command(
    program: impl AsRef<OsStr>,
    path: &OsStr,
) -> Result<std::process::Command, UnreadableLauncher> {
    launch(program.as_ref(), path).map(|launch| launch.command())
}

/// Whether Windows would run `program` with `cmd.exe`.
pub fn is_batch(program: &Path) -> bool {
    program
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"))
}

/// The launcher `text`, read from the file `shim`: what it starts, with `path` as the PATH it would search for a
/// bare `node`. `None` when it isn't a launcher read here, or what it starts isn't there.
pub fn read(text: &str, shim: &Path, path: &OsStr) -> Option<Launch> {
    let dir = shim.parent()?;
    let mut reader = Reader {
        dir,
        prog: None,
        node_path: None,
        blocks: Vec::new(),
        run: None,
    };
    for line in text.lines() {
        reader.line(line)?;
    }
    if !reader.blocks.is_empty() {
        return None;
    }
    let (program, leading) = reader.run?;
    let program = resolve(&program, path)?;
    Some(Launch {
        program,
        leading,
        node_path: reader.node_path,
    })
}

/// The prefix npm 7's launcher puts before the program, so Ctrl-C doesn't ask "Terminate batch job?".
const NPM_PREFIX: &str = "endlocal & goto #_undefined_# 2>nul || title %comspec% & ";

/// The lines every launcher of npm 7 and later starts with, and the `PATHEXT` line that keeps `node` from finding
/// `node.js`: nothing to do for a direct start.
const IGNORED: &[&str] = &[
    "echo off",
    "goto start",
    ":find_dp0",
    "set dp0=%~dp0",
    "exit /b",
    ":start",
    "setlocal",
    "call :find_dp0",
    "set pathext=%pathext:;.js;=;%",
];

struct Reader<'a> {
    dir: &'a Path,
    /// `_prog`, npm 7's program.
    prog: Option<String>,
    node_path: Option<OsString>,
    /// The `IF` blocks open: whether the block's own lines run.
    blocks: Vec<Block>,
    /// The line that starts the program: the program and its arguments before `%*`.
    run: Option<(String, Vec<OsString>)>,
}

#[derive(Clone, Copy)]
struct Block {
    outer: bool,
    condition: bool,
    in_else: bool,
}

impl Block {
    fn runs(self) -> bool {
        self.outer && (self.condition != self.in_else)
    }
}

impl Reader<'_> {
    fn runs(&self) -> bool {
        self.blocks.last().is_none_or(|b| b.runs())
    }

    /// One line; `None` when it isn't one read here.
    fn line(&mut self, line: &str) -> Option<()> {
        let line = line.trim();
        let line = line.strip_prefix('@').unwrap_or(line).trim();
        let lower = line.to_ascii_lowercase();
        if line.is_empty() || IGNORED.contains(&lower.as_str()) {
            return Some(());
        }
        let runs = self.runs();
        // Keywords are matched in lower case; lower-casing ASCII keeps every byte where it was in `line`.
        if lower.starts_with("if exist ") {
            let target = line["if exist ".len()..].strip_suffix('(')?.trim();
            let target = self.word(unquote(target)?)?;
            let condition = Path::new(&target).is_file();
            self.blocks.push(Block {
                outer: runs,
                condition,
                in_else: false,
            });
        } else if lower == "if not defined node_path (" {
            // Both branches set the same folders; [`Launch::node_path_with`] adds what was there.
            self.blocks.push(Block {
                outer: runs,
                condition: true,
                in_else: false,
            });
        } else if lower == ") else (" {
            let block = self.blocks.last_mut()?;
            if block.in_else {
                return None;
            }
            block.in_else = true;
        } else if lower == ")" {
            self.blocks.pop()?;
        } else if let Some(value) = set_value(line, "_prog") {
            let value = self.word(value)?;
            if runs {
                self.prog = Some(value);
            }
        } else if let Some(value) = set_value(line, "node_path") {
            let value = value
                .strip_suffix(";%NODE_PATH%")
                .or_else(|| value.strip_suffix(";%node_path%"))
                .unwrap_or(value);
            let value = self.word(value)?;
            if runs {
                self.node_path = Some(value.into());
            }
        } else {
            // The line that starts the program; any other line isn't read.
            let command = line.strip_suffix("%*")?;
            let command = match lower.strip_prefix(NPM_PREFIX) {
                Some(_) => &command[NPM_PREFIX.len()..],
                None => command,
            };
            let words = split(command)?;
            let (program, args) = words.split_first()?;
            let program = self.word(program)?;
            let leading = args
                .iter()
                .map(|a| self.word(a).map(OsString::from))
                .collect::<Option<Vec<_>>>()?;
            if runs {
                if self.run.is_some() {
                    return None;
                }
                self.run = Some((program, leading));
            }
        }
        Some(())
    }

    /// A word of a line with the launcher's own variables in place: `%dp0%` and `%~dp0` (the launcher's folder)
    /// at its start, or the whole of it `%_prog%`. `None` when another `%` is left.
    fn word(&self, word: &str) -> Option<String> {
        let lower = word.to_ascii_lowercase();
        if lower == "%_prog%" {
            return self.prog.clone();
        }
        let rest = ["%dp0%", "%~dp0"]
            .iter()
            .find_map(|var| lower.starts_with(var).then(|| &word[var.len()..]));
        match rest {
            Some(rest) if !rest.contains('%') => Some(
                join_relative(self.dir, rest)
                    .into_os_string()
                    .into_string()
                    .ok()?,
            ),
            Some(_) => None,
            None if word.contains('%') => None,
            None => Some(word.to_owned()),
        }
    }
}

/// The value of `SET "<name>=<value>"` (any case), `None` for another line.
fn set_value<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let head = format!("set \"{name}=");
    let lower = line.to_ascii_lowercase();
    lower.strip_prefix(&head)?;
    line[head.len()..].strip_suffix('"')
}

/// A word in double quotes, or one without any.
fn unquote(word: &str) -> Option<&str> {
    match word.strip_prefix('"') {
        Some(rest) => rest.strip_suffix('"').filter(|w| !w.contains('"')),
        None => (!word.contains('"')).then_some(word),
    }
}

/// The words of a command line, as `cmd.exe` would pass them: split at spaces outside double quotes, the quotes
/// removed. `None` when it has a character `cmd.exe` would act on outside quotes (`&`, `|`, `<`, `>`, `^`, `(`,
/// `)`), or an unclosed quote.
fn split(command: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quoted = false;
    for c in command.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                in_word = true;
            }
            ' ' | '\t' if !quoted => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '&' | '|' | '<' | '>' | '^' | '(' | ')' if !quoted => return None,
            c => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if quoted {
        return None;
    }
    if in_word {
        words.push(word);
    }
    Some(words)
}

/// `rest` (`\node_modules\…\codex.js`, `..\pkg\bin\x.js`) under `dir`, either slash a separator, `..` taken away.
fn join_relative(dir: &Path, rest: &str) -> PathBuf {
    let mut path = dir.to_path_buf();
    for part in rest
        .split(['\\', '/'])
        .filter(|p| !p.is_empty() && *p != ".")
    {
        if part == ".." {
            if matches!(path.components().next_back(), Some(Component::Normal(_))) {
                path.pop();
            }
        } else {
            path.push(part);
        }
    }
    path
}

/// The program a launcher's line names: a path that is there and isn't another batch file, or a bare name
/// (`node`) found on `path` as an `.exe` or `.com` (the launcher takes `.JS` out of `PATHEXT` for the same
/// reason: `node.js` beside a tool's files isn't Node).
fn resolve(program: &str, path: &OsStr) -> Option<PathBuf> {
    if program.contains(['/', '\\']) {
        let program = PathBuf::from(program);
        return (program.is_file() && !is_batch(&program)).then_some(program);
    }
    if Path::new(program).extension().is_some() {
        return super::find_on_path(program, path).filter(|p| !is_batch(p));
    }
    ["exe", "com"]
        .iter()
        .find_map(|ext| super::find_on_path(&format!("{program}.{ext}"), path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path: PathBuf = [
            env!("CARGO_MANIFEST_DIR"),
            "tests",
            "fixtures",
            "npm-shims",
            name,
        ]
        .iter()
        .collect();
        std::fs::read_to_string(path).unwrap()
    }

    /// A file at `path` that a PATH lookup takes (the executable bit, for the tests on macOS and Linux).
    fn program_file(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    const CODEX_JS: &[&str] = &["node_modules", "@openai", "codex", "bin", "codex.js"];

    /// `parts` under `base`, with the OS's own separator (as the launcher's paths are read).
    fn under(base: &Path, parts: &[&str]) -> PathBuf {
        parts
            .iter()
            .fold(base.to_path_buf(), |path, part| path.join(part))
    }

    /// An npm prefix folder with `codex.cmd`'s script, and a folder with `node.exe` for the PATH.
    struct Install {
        _root: tempfile::TempDir,
        npm: PathBuf,
        path: OsString,
        node: PathBuf,
    }

    fn install() -> Install {
        let root = tempfile::tempdir().unwrap();
        let npm = root.path().join("npm");
        program_file(&under(&npm, CODEX_JS));
        let node = root.path().join("nodejs").join("node.exe");
        program_file(&node);
        let path = std::env::join_paths([node.parent().unwrap()]).unwrap();
        Install {
            _root: root,
            npm,
            path,
            node,
        }
    }

    fn script(i: &Install) -> OsString {
        under(&i.npm, CODEX_JS).into_os_string()
    }

    #[test]
    fn npm_s_launcher_runs_node_from_the_path() {
        let i = install();
        let launch = read(&fixture("codex.cmd"), &i.npm.join("codex.cmd"), &i.path).unwrap();
        assert_eq!(launch.program, i.node);
        assert_eq!(launch.leading, vec![script(&i)]);
        assert_eq!(launch.node_path, None);
    }

    #[test]
    fn npm_s_launcher_runs_the_node_beside_it_first() {
        let i = install();
        let beside = i.npm.join("node.exe");
        program_file(&beside);
        let launch = read(&fixture("codex.cmd"), &i.npm.join("codex.cmd"), &i.path).unwrap();
        assert_eq!(launch.program, beside);
        assert_eq!(launch.leading, vec![script(&i)]);
    }

    #[test]
    fn no_node_anywhere_is_unreadable() {
        let i = install();
        let empty = OsString::new();
        assert_eq!(
            read(&fixture("codex.cmd"), &i.npm.join("codex.cmd"), &empty),
            None
        );
    }

    /// Node's own `corepack.cmd`: the two-branch form, with `node.exe` beside it as Node's installer puts it.
    #[test]
    fn the_two_branch_form() {
        let root = tempfile::tempdir().unwrap();
        let nodejs = root.path().join("nodejs");
        let corepack = under(
            &nodejs,
            &["node_modules", "corepack", "dist", "corepack.js"],
        );
        program_file(&corepack);
        let shim = nodejs.join("corepack.cmd");
        let empty = OsString::new();
        assert_eq!(read(&fixture("corepack.cmd"), &shim, &empty), None);

        let elsewhere = root.path().join("bin").join("node.exe");
        program_file(&elsewhere);
        let path = std::env::join_paths([elsewhere.parent().unwrap()]).unwrap();
        let launch = read(&fixture("corepack.cmd"), &shim, &path).unwrap();
        assert_eq!(launch.program, elsewhere);
        assert_eq!(launch.leading, vec![corepack.clone().into_os_string()]);

        let beside = nodejs.join("node.exe");
        program_file(&beside);
        let launch = read(&fixture("corepack.cmd"), &shim, &path).unwrap();
        assert_eq!(launch.program, beside);
        assert_eq!(launch.leading, vec![corepack.into_os_string()]);
    }

    /// pnpm's form (`@zkochan/cmd-shim`'s template): `NODE_PATH` first, the script one folder up.
    #[test]
    fn pnpm_s_form_sets_node_path() {
        let i = install();
        let launch = read(
            &fixture("pnpm-codex.cmd"),
            &i.npm.join("bin").join("codex.cmd"),
            &i.path,
        )
        .unwrap();
        assert_eq!(launch.program, i.node);
        assert_eq!(launch.leading, vec![script(&i)]);
        assert_eq!(
            launch.node_path_with(None),
            Some(OsString::from(r"C:\pnpm\global\5\node_modules"))
        );
        assert_eq!(
            launch.node_path_with(Some(OsStr::new(r"D:\mine"))),
            Some(OsString::from(r"C:\pnpm\global\5\node_modules;D:\mine"))
        );
    }

    /// One line, a program by its full path (a test's stand-in, [`crate::process::testing::stub_command`]).
    #[test]
    fn one_line_with_a_full_path() {
        let root = tempfile::tempdir().unwrap();
        let sh = root.path().join("Git").join("bin").join("sh.exe");
        program_file(&sh);
        let shim = root.path().join("tool.cmd");
        let text = format!("@\"{}\" \"%~dp0tool.sh\" %*\r\n", sh.display());
        let launch = read(&text, &shim, &OsString::new()).unwrap();
        assert_eq!(launch.program, sh);
        assert_eq!(
            launch.leading,
            vec![root.path().join("tool.sh").into_os_string()]
        );
    }

    /// Node's own `npm.cmd` (a loop over another program's output) and other batch files aren't npm launchers.
    #[test]
    fn other_batch_files_are_unreadable() {
        let i = install();
        let shim = i.npm.join("x.cmd");
        assert_eq!(read(&fixture("npm.cmd"), &shim, &i.path), None);
        for text in [
            "",
            "@echo hello\r\n",
            "node \"%dp0%\\x.js\" %* & del x\r\n",
            "node \"%dp0%\\x.js\" %1\r\n",
            "@SET FOO=bar\r\nnode \"%dp0%\\x.js\" %*\r\n",
            "node \"%dp0%\\x.js\" %*\r\nnode \"%dp0%\\y.js\" %*\r\n",
            "node \"%USERPROFILE%\\x.js\" %*\r\n",
            "IF EXIST \"%dp0%\\node.exe\" (\r\n  node \"%dp0%\\x.js\" %*\r\n",
            "\"%dp0%\\other.cmd\" %*\r\n",
        ] {
            assert_eq!(read(text, &shim, &i.path), None, "{text:?}");
        }
    }

    #[test]
    fn words_split_as_cmd_passes_them() {
        assert_eq!(
            split(r#""%_prog%"  "%dp0%\a b\x.js" --flag "#),
            Some(vec![
                "%_prog%".into(),
                r"%dp0%\a b\x.js".into(),
                "--flag".into()
            ])
        );
        assert_eq!(split(r#""a&b" c"#), Some(vec!["a&b".into(), "c".into()]));
        assert_eq!(split("a | b"), None);
        assert_eq!(split(r#""open"#), None);
    }

    #[test]
    fn relative_parts_are_joined() {
        let dir = Path::new("/p/bin");
        assert_eq!(
            join_relative(dir, r"\..\x\y.js"),
            PathBuf::from("/p/x/y.js")
        );
        assert_eq!(
            join_relative(dir, "tool.sh"),
            PathBuf::from("/p/bin/tool.sh")
        );
    }

    #[test]
    fn the_start_problem_names_the_tool_and_the_file_and_says_what_to_do() {
        let e = UnreadableLauncher {
            file: r"C:\tools\codex.cmd".into(),
        };
        assert_eq!(
            e.message(Some("Codex")),
            "Codex is started through your own script file (C:\\tools\\codex.cmd), which Otaq can\u{2019}t run \
             safely. Install Codex the normal way, or start it without the script."
        );
        assert_eq!(
            e.message(None),
            "The tool is started through your own script file (C:\\tools\\codex.cmd), which Otaq can\u{2019}t run \
             safely. Install it the normal way, or start it without the script."
        );
    }

    #[test]
    fn elsewhere_a_program_starts_as_it_is() {
        if !cfg!(windows) {
            let launch = launch(OsStr::new("codex.cmd"), OsStr::new("/nowhere")).unwrap();
            assert_eq!(launch, Launch::plain("codex.cmd".into()));
        }
    }
}

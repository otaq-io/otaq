//! How the user's commands and shells run on each platform (PLT-02).
//!
//! - A **one-line command** the user typed (the setup command, the Run command) runs with `/bin/sh -c` on macOS
//!   and Linux. On Windows it runs with `cmd /d /s /c "<command>"`, as `npm` runs scripts there. That line goes
//!   to an outer `cmd` through an environment variable (`%OTAQ_COMMAND%`), which `cmd` expands before it
//!   parses: the command keeps its quotes and operators exactly as typed, whatever quoting the program starting
//!   `cmd` applies to its arguments, and the inner `cmd` expands its variables (`%PORT%`) as usual.
//! - The **interactive shell** of the Shell tab and the Project terminal is the user's login shell (`$SHELL -l`,
//!   else `/bin/sh`) on macOS and Linux; on Windows PowerShell (`pwsh` when installed, else Windows PowerShell).
//! - The **status line** is the user's Claude Code `statusLine` command, which Claude Code runs with Git Bash on
//!   Windows; Otaq runs it the same way ([`status_line`]).
//! - An **agent's commands** run in whatever shell its tool uses: Claude Code uses Git Bash on Windows, Codex
//!   PowerShell. The skills' stdin examples are bash heredocs (`<<'EOF'`), which PowerShell can't parse, so
//!   `otaq prime` adds [`powershell_stdin_rule`] when the shell it was started from is PowerShell
//!   ([`caller_shell`]): the shell actually running the commands, not a guess from the tool's name.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::naming;

/// A program, its arguments and the variables it needs on top of the inherited environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// The variable that carries a command to `cmd.exe` on Windows: `OTAQ_COMMAND`.
pub fn command_env_var() -> String {
    naming::env_var("COMMAND")
}

/// The variable that carries the line Run prints first on Windows: `OTAQ_COMMAND_SHOWN`.
pub fn shown_env_var() -> String {
    naming::env_var("COMMAND_SHOWN")
}

/// The inner `cmd` line for a Windows command (see the module comment).
fn windows_command(command: &str) -> String {
    format!("cmd /d /s /c \"{command}\"")
}

/// Runs `command` as the platform's shell would.
pub fn one_line(command: &str) -> ShellCommand {
    if cfg!(windows) {
        ShellCommand {
            program: "cmd.exe".to_owned(),
            args: vec!["/d".into(), "/c".into(), format!("%{}%", command_env_var())],
            env: vec![(command_env_var(), windows_command(command))],
        }
    } else {
        ShellCommand {
            program: "/bin/sh".to_owned(),
            args: vec!["-c".into(), command.to_owned()],
            env: vec![],
        }
    }
}

/// Like [`one_line`], but first prints `$ <shown>` on its own line (Run's first line, "$ pnpm dev --port 4020").
///
/// On Windows the shown line is echoed with delayed expansion (`!…!`), which `cmd` applies after parsing, so
/// characters like `&` or `>` in it are printed, not run; the command itself runs in the inner `cmd`, without
/// delayed expansion.
pub fn one_line_shown(command: &str, shown: &str) -> ShellCommand {
    if cfg!(windows) {
        ShellCommand {
            program: "cmd.exe".to_owned(),
            args: vec![
                "/d".into(),
                "/v:on".into(),
                "/c".into(),
                format!("echo(!{}!&%{}%", shown_env_var(), command_env_var()),
            ],
            env: vec![
                (command_env_var(), windows_command(command)),
                (shown_env_var(), format!("$ {shown}")),
            ],
        }
    } else {
        let script = r#"printf '%s\n' "$1"; exec /bin/sh -c "$2""#;
        ShellCommand {
            program: "/bin/sh".to_owned(),
            args: vec![
                "-c".into(),
                script.into(),
                naming::COMMAND_NAME.into(),
                format!("$ {shown}"),
                command.into(),
            ],
            env: vec![],
        }
    }
}

/// The user's interactive shell and its arguments, from the given `$SHELL` value (macOS and Linux) and `PATH`.
pub fn interactive(shell_var: Option<&str>, path: &OsStr) -> (String, Vec<String>) {
    if cfg!(windows) {
        let program = crate::tools::find_on_path("pwsh", path)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "powershell.exe".to_owned());
        (program, vec!["-NoLogo".to_owned()])
    } else {
        let shell = shell_var
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("/bin/sh")
            .to_owned();
        (shell, vec!["-l".to_owned()])
    }
}

/// How the user's status-line command runs: `/bin/sh -c` on macOS and Linux, Git Bash on Windows (as Claude
/// Code runs it), `cmd.exe` if Git Bash can't be found.
pub fn status_line(command: &str, path: &OsStr) -> ShellCommand {
    if cfg!(windows)
        && let Some(bash) = git_bash(
            std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH").as_deref(),
            path,
        )
    {
        return ShellCommand {
            program: bash.display().to_string(),
            args: vec!["-c".into(), command.to_owned()],
            env: vec![],
        };
    }
    one_line(command)
}

/// Git Bash, as Claude Code finds it on Windows: `CLAUDE_CODE_GIT_BASH_PATH`, else `bin\bash.exe` of the Git
/// installation that `git` on PATH belongs to (`…\Git\cmd\git.exe` → `…\Git\bin\bash.exe`).
pub fn git_bash(configured: Option<&OsStr>, path: &OsStr) -> Option<PathBuf> {
    if let Some(configured) = configured.filter(|c| !c.is_empty()) {
        return Some(PathBuf::from(configured));
    }
    let git = crate::tools::find_on_path("git", path)?;
    let root = git.parent()?.parent()?;
    [
        root.join("bin").join("bash.exe"),
        root.join("usr").join("bin").join("bash.exe"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// `PATH` with `first` in front, joined with the platform's separator (`:`, `;` on Windows).
pub fn prepend_path(first: Option<&Path>, path: &OsStr) -> OsString {
    let mut parts: Vec<PathBuf> = first.map(Path::to_path_buf).into_iter().collect();
    if !path.is_empty() {
        parts.extend(std::env::split_paths(path));
    }
    std::env::join_paths(parts).unwrap_or_else(|_| path.to_owned())
}

/// The shell an agent's commands run in, as far as it matters for passing text on stdin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallerShell {
    /// Windows PowerShell or PowerShell 7 (`powershell.exe`, `pwsh`).
    PowerShell,
    /// A POSIX shell (`bash`, `sh`, `zsh`…), `cmd`, or one Otaq doesn't know.
    Other,
}

/// The shell a command was started from: the first known shell among `ancestors` (the command's parent first), by
/// the program's file name. Anything in between that isn't a shell (an `otaq` shim, `node`) is passed over; with no
/// known shell it is [`CallerShell::Other`].
pub fn caller_shell(ancestors: &[PathBuf]) -> CallerShell {
    for program in ancestors {
        let program = program.to_string_lossy().to_ascii_lowercase();
        let name = program.rsplit(['/', '\\']).next().unwrap_or_default();
        let stem = name.strip_suffix(".exe").unwrap_or(name);
        match stem {
            "powershell" | "pwsh" => return CallerShell::PowerShell,
            "bash" | "sh" | "zsh" | "dash" | "fish" | "cmd" => return CallerShell::Other,
            _ => {}
        }
    }
    CallerShell::Other
}

/// What `otaq prime` adds when the agent's commands run in PowerShell: the skills' heredoc examples fail there
/// ("Missing file specification after redirection operator").
pub fn powershell_stdin_rule() -> String {
    "\
## Your shell is PowerShell

Your commands run in PowerShell, not bash, so the skill's `<<'…'` examples (heredocs) fail here. Pass text on stdin \
with a here-string piped into the command: `@'` ends its line, the text follows, and `'@` starts its own line. End a \
continued line with a backtick (`` ` ``), never `\\`.

```powershell
@'
Added the price table with sorting by price and name.
`npm test` at a1b2c3d: 48 passed; log: `${dir}/test-run.txt`
'@ | {cmd} stage next --stdin `
  --manual-check \"On a phone-width window, open /prices: the header stays put\"
```

Inside `@' … '@` nothing is expanded; in a command you run, write a variable as `$env:{dir}`.
"
    .replace("{cmd}", naming::COMMAND_NAME)
    .replace("{dir}", &naming::resource_dir_env_var())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn programs(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn the_first_known_shell_up_the_chain_decides() {
        let ps = CallerShell::PowerShell;
        assert_eq!(
            caller_shell(&programs(&[
                r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
            ])),
            ps
        );
        assert_eq!(
            caller_shell(&programs(&[r"C:\Program Files\PowerShell\7\PWSH.EXE"])),
            ps
        );
        assert_eq!(caller_shell(&programs(&["/usr/local/bin/pwsh"])), ps);
        // A shim or runtime between the command and its shell is passed over.
        assert_eq!(
            caller_shell(&programs(&[r"C:\tools\node.exe", r"C:\x\pwsh.exe"])),
            ps
        );
        // Claude Code on Windows: Git Bash, even when PowerShell is further up.
        assert_eq!(
            caller_shell(&programs(&[
                r"C:\Program Files\Git\usr\bin\bash.exe",
                r"C:\x\powershell.exe"
            ])),
            CallerShell::Other
        );
        assert_eq!(caller_shell(&programs(&["/bin/zsh"])), CallerShell::Other);
        assert_eq!(
            caller_shell(&programs(&[r"C:\Windows\System32\cmd.exe"])),
            CallerShell::Other
        );
        assert_eq!(caller_shell(&[]), CallerShell::Other);
    }

    #[test]
    fn the_powershell_rule_shows_a_here_string_into_the_command() {
        let rule = powershell_stdin_rule();
        assert!(rule.starts_with("## Your shell is PowerShell\n"), "{rule}");
        assert!(rule.contains("@'\nAdded the price table"), "{rule}");
        let cmd = naming::COMMAND_NAME;
        assert!(
            rule.contains(&format!("\n'@ | {cmd} stage next --stdin `\n")),
            "{rule}"
        );
        let dir = naming::resource_dir_env_var();
        assert!(rule.contains(&format!("`$env:{dir}`")), "{rule}");
        assert!(!rule.contains("{cmd}") && !rule.contains("{dir}"), "{rule}");
    }

    #[test]
    fn path_gets_the_folder_first() {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let rest = OsString::from(format!("/usr/bin{sep}/bin"));
        assert_eq!(
            prepend_path(Some(Path::new("/opt/cb/bin")), &rest),
            OsString::from(format!("/opt/cb/bin{sep}/usr/bin{sep}/bin"))
        );
        assert_eq!(prepend_path(None, &rest), rest);
        assert_eq!(
            prepend_path(Some(Path::new("/opt/cb/bin")), OsStr::new("")),
            OsString::from("/opt/cb/bin"),
            "no trailing separator"
        );
    }

    #[test]
    fn one_line_commands_keep_the_text_as_typed() {
        let c = one_line(r#"echo "a b" && echo c"#);
        if cfg!(windows) {
            assert_eq!(c.program, "cmd.exe");
            assert_eq!(c.args, ["/d", "/c", &format!("%{}%", command_env_var())]);
            assert_eq!(
                c.env,
                [(
                    command_env_var(),
                    r#"cmd /d /s /c "echo "a b" && echo c""#.to_owned()
                )]
            );
        } else {
            assert_eq!(c.program, "/bin/sh");
            assert_eq!(c.args, ["-c", r#"echo "a b" && echo c"#]);
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_login_shell_or_sh() {
        let path = OsStr::new("/usr/bin");
        assert_eq!(
            interactive(Some("/bin/zsh"), path),
            ("/bin/zsh".into(), vec!["-l".into()])
        );
        assert_eq!(interactive(Some(" "), path).0, "/bin/sh");
        assert_eq!(interactive(None, path).0, "/bin/sh");
    }

    /// The command really runs, with quotes and operators intact.
    #[test]
    fn one_line_runs_through_the_platform_shell() {
        let c = one_line(r#"echo "a  b"&& echo c"#);
        let out = std::process::Command::new(&c.program)
            .args(&c.args)
            .envs(c.env.iter().map(|(k, v)| (k, v)))
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        if cfg!(windows) {
            assert_eq!(text, "\"a  b\"\nc\n");
        } else {
            assert_eq!(text, "a  b\nc\n");
        }
    }

    /// The command's own variables expand as the shell expands them (`%PORT%`, `$PORT`).
    #[test]
    fn variables_in_the_command_expand() {
        let command = if cfg!(windows) {
            "echo port=%CB_SHELL_TEST_PORT%!"
        } else {
            "echo port=$CB_SHELL_TEST_PORT!"
        };
        let c = one_line_shown(command, "x");
        let out = std::process::Command::new(&c.program)
            .args(&c.args)
            .envs(c.env.iter().map(|(k, v)| (k, v)))
            .env("CB_SHELL_TEST_PORT", "4020")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        assert_eq!(text, "$ x\nport=4020!\n");
    }

    #[test]
    fn the_shown_line_comes_first_and_is_not_run() {
        let c = one_line_shown("echo ran", "echo ran > out & x");
        let out = std::process::Command::new(&c.program)
            .args(&c.args)
            .envs(c.env.iter().map(|(k, v)| (k, v)))
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        assert_eq!(text, "$ echo ran > out & x\nran\n");
    }
}

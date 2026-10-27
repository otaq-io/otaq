//! The `otaq` command for the user's own terminal (d0140, UPD-07): `<data folder>/bin/otaq` kept on every start, and
//! About's **Use otaq in your own terminal** row with **Add to my shell's PATH** (page 32 item 5). The rules are in
//! `otaq_core::shell_path`; this reads the environment and words the row. Both block (the login shell runs, files
//! are written): the page's requests run them on a blocking thread.

use std::path::Path;

use otaq_core::client::CommandLauncher;
use otaq_core::data_dir::DataDir;
use otaq_core::shell_path::{self, AddResult, BinKind, BinRefresh, PathStatus, ShellEnv};
use otaq_view::view_models::about::{PathInputs, PathRow, PathState, PathTarget, path_row};

/// Points `bin/otaq` at this app's own `otaq` (the one next to it). Called once per start, by the first copy of the
/// app. Its call in `main` runs it only where the app may start its service ([`crate::install_place::Gate::runs`], T-0579).
pub fn refresh_bin(data: &DataDir) {
    let own = match CommandLauncher::next_to_current_exe() {
        Ok(launcher) => launcher.program,
        Err(e) => {
            eprintln!("bin: no command next to the app: {e}");
            return;
        }
    };
    if !own.is_file() {
        eprintln!("bin: no command next to the app: {}", own.display());
        return;
    }
    // A copy in use (Windows) is replaced at the next start.
    match shell_path::refresh_bin(data, &own, BinKind::for_this_os()) {
        Ok(BinRefresh::Unchanged | BinRefresh::Updated | BinRefresh::InUse) => {}
        Err(e) => eprintln!("bin: can't update {}: {e}", data.bin_dir().display()),
    }
}

/// Where the button writes, read from this process's environment.
fn target(env: Option<&ShellEnv>) -> PathTarget {
    if cfg!(windows) {
        return PathTarget::Windows;
    }
    match env.and_then(|env| Some((env, env.shell_kind()?))) {
        Some((env, shell)) => PathTarget::Files(
            env.shell_files(shell)
                .iter()
                .map(|f| shown(f, &env.home))
                .collect(),
        ),
        None => PathTarget::OtherShell,
    }
}

/// `~/.zshrc` for a path under the home folder, else the path as it is.
fn shown(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if !rest.as_os_str().is_empty() => {
            format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display())
        }
        _ => path.display().to_string(),
    }
}

fn inputs(data: &DataDir, env: Option<&ShellEnv>, state: PathState) -> PathInputs {
    let bin = data.bin_dir();
    PathInputs {
        target: target(env),
        state,
        folder: match env {
            Some(env) => shown(&bin, &env.home),
            None => bin.display().to_string(),
        },
    }
}

/// The row as it is now: on the PATH, the line written, or the offer. Runs the login shell (up to 5 s).
pub fn row(data: &DataDir) -> PathRow {
    row_with(
        data,
        ShellEnv::from_process().as_ref(),
        otaq_core::tools::login_path,
    )
}

/// [`row`] for `env`, `login_path` giving the login shell's PATH.
fn row_with(
    data: &DataDir,
    env: Option<&ShellEnv>,
    login_path: impl Fn(&Path) -> Option<std::ffi::OsString>,
) -> PathRow {
    let bin = data.bin_dir();
    let status = if cfg!(windows) {
        windows_status(&bin)
    } else {
        let login = env.and_then(|env| login_path(env.shell.as_deref()?));
        let written = env.is_some_and(|env| {
            env.shell_kind().is_some_and(|shell| {
                shell_path::line_written(&env.shell_files(shell), &bin, &env.home)
            })
        });
        shell_path::status(login.as_deref(), &bin, written)
    };
    let state = match status {
        PathStatus::Offer => PathState::Offer,
        PathStatus::Added => PathState::Added,
        PathStatus::AlreadyThere => PathState::AlreadyThere,
    };
    path_row(&inputs(data, env, state))
}

#[cfg(windows)]
fn windows_status(bin: &Path) -> PathStatus {
    use otaq_core::shell_path::UserPath;
    match shell_path::RegistryUserPath.read() {
        Ok(Some(path)) if shell_path::windows_has(&path, &bin.to_string_lossy()) => {
            PathStatus::AlreadyThere
        }
        _ => PathStatus::Offer,
    }
}

#[cfg(not(windows))]
fn windows_status(_bin: &Path) -> PathStatus {
    PathStatus::Offer
}

/// **Add to my shell's PATH**: writes the line (or Windows' PATH) and gives the row's new state.
pub fn add(data: &DataDir) -> PathRow {
    match add_with(data, ShellEnv::from_process().as_ref()) {
        Some(row) => row,
        None => row(data),
    }
}

/// [`add`] for `env`; `None` for a shell the button isn't offered for.
fn add_with(data: &DataDir, env: Option<&ShellEnv>) -> Option<PathRow> {
    let bin = data.bin_dir();
    let result = if cfg!(windows) {
        windows_add(&bin)
    } else {
        let (env, shell) = env.and_then(|env| Some((env, env.shell_kind()?)))?;
        shell_path::add_to_shell(env, shell, &bin)
    };
    let state = match result {
        AddResult::Added { .. } => PathState::Added,
        AddResult::AlreadyThere => PathState::AlreadyThere,
        AddResult::Failed {
            file,
            read_only,
            why,
            line,
        } => PathState::Failed {
            file: file.map(|f| match env {
                Some(env) => shown(&f, &env.home),
                None => f.display().to_string(),
            }),
            read_only,
            why,
            line,
        },
    };
    Some(path_row(&inputs(data, env, state)))
}

#[cfg(windows)]
fn windows_add(bin: &Path) -> AddResult {
    shell_path::windows_add(&shell_path::RegistryUserPath, bin)
}

#[cfg(not(windows))]
fn windows_add(_bin: &Path) -> AddResult {
    AddResult::AlreadyThere
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_under_home_are_shown_with_a_tilde() {
        let home = Path::new("/Users/me");
        assert_eq!(shown(&home.join(".zshrc"), home), "~/.zshrc");
        assert_eq!(shown(Path::new("/tmp/x/bin"), home), "/tmp/x/bin");
    }

    #[cfg(not(windows))]
    #[test]
    fn the_target_follows_the_login_shell() {
        let env = ShellEnv {
            home: "/Users/me".into(),
            shell: Some("/bin/zsh".into()),
            zdotdir: None,
            xdg_config_home: None,
            macos: true,
        };
        assert_eq!(
            target(Some(&env)),
            PathTarget::Files(vec!["~/.zshrc".into()])
        );
        let tcsh = ShellEnv {
            shell: Some("/bin/tcsh".into()),
            ..env
        };
        assert_eq!(target(Some(&tcsh)), PathTarget::OtherShell);
        assert_eq!(target(None), PathTarget::OtherShell);
    }

    /// The click, against a temporary home: the offer, Added, then (in a new terminal's PATH) already there; a
    /// read-only `.zshrc` gives the line to paste.
    #[cfg(unix)]
    #[test]
    fn the_row_through_a_click() {
        use otaq_view::view_models::about::PathTone;
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_owned();
        let data = DataDir::new(home.join(otaq_core::naming::data_dir_name()));
        let env = ShellEnv {
            home: home.clone(),
            shell: Some("/bin/zsh".into()),
            zdotdir: None,
            xdg_config_home: None,
            macos: cfg!(target_os = "macos"),
        };
        let no_bin = |_: &Path| Some(std::ffi::OsString::from("/usr/bin:/bin"));
        let offer = row_with(&data, Some(&env), no_bin);
        assert_eq!(offer.tone, PathTone::Offer);
        assert!(offer.button.is_some());

        let added = add_with(&data, Some(&env)).unwrap();
        assert_eq!(added.tone, PathTone::Done);
        assert!(added.result.unwrap().starts_with("Added · "));
        assert!(added.detail.unwrap().contains("~/.zshrc"));
        // Until a new terminal's PATH has it, About still says Added; a second click adds nothing.
        assert!(
            row_with(&data, Some(&env), no_bin)
                .result
                .unwrap()
                .starts_with("Added · ")
        );
        assert!(
            add_with(&data, Some(&env))
                .unwrap()
                .result
                .unwrap()
                .ends_with(" works in your terminal")
        );
        let with_bin = |_: &Path| {
            Some(
                std::env::join_paths([std::path::PathBuf::from("/usr/bin"), data.bin_dir()])
                    .unwrap(),
            )
        };
        let already = row_with(&data, Some(&env), with_bin);
        assert!(already.result.unwrap().ends_with(" works in your terminal"));
        assert_eq!(
            already.detail.unwrap(),
            format!(
                "~/{}/bin is already on your PATH.",
                otaq_core::naming::data_dir_name()
            )
        );
        let zshrc = home.join(".zshrc");
        assert_eq!(
            std::fs::read_to_string(&zshrc)
                .unwrap()
                .matches("# added by")
                .count(),
            1
        );

        // A read-only .zshrc.
        std::fs::write(&zshrc, "export A=1\n").unwrap();
        std::fs::set_permissions(&zshrc, std::fs::Permissions::from_mode(0o444)).unwrap();
        let failed = add_with(&data, Some(&env)).unwrap();
        assert_eq!(failed.tone, PathTone::Failed);
        assert!(failed.result.unwrap().ends_with(" write to ~/.zshrc"));
        assert_eq!(
            failed.detail.as_deref(),
            Some("The file is read-only. Paste this line at its end, then open a new terminal:")
        );
        assert_eq!(
            failed.paste.unwrap(),
            format!(
                "export PATH=\"$HOME/{}/bin:$PATH\" # added by {}",
                otaq_core::naming::data_dir_name(),
                otaq_core::naming::PRODUCT_NAME
            )
        );
    }
}

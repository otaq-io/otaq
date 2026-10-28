//! The `otaq` command for the user's own terminal (d0140, UPD-07, UPD-09; page 32 item 5).
//!
//! **`<data folder>/bin/otaq`** ([`refresh_bin`]): the app points it at its own `otaq` on every start. On macOS it's a
//! link into the bundle; on Windows (`otaq.exe`) and Linux a copy, because Windows links need admin rights and an
//! AppImage's insides move on every start. A copy in use is left and replaced at the next start. The copy of Otaq
//! opened last wins. Beside a copy goes the plugin folder (`bin/plugin`, replaced on every start): a service started
//! through `bin/otaq` (`otaq attach SHOP-12` with the app closed) makes its runtime copy from `bin/`, so it runs from
//! that copy and its agents get the plugin, even after an AppImage's mount is gone (d0140 gap review C1/G1, T-0574's
//! question QB). On Windows Otaq's ConPTY (`conpty.dll`, `OpenConsole.exe`, [`crate::session::conpty`]) goes there
//! too, for the same reason. A link (macOS) needs none: it resolves into the bundle.
//!
//! **Add to my shell's PATH** ([`add_to_shell`], [`windows_add`]): only on the click. One line, marked
//! `# added by Otaq`, at the end of the login shell's file: zsh `$ZDOTDIR/.zshrc` (`~/.zshrc` without `ZDOTDIR`);
//! bash `~/.bashrc`, and on macOS also `~/.bash_profile` when it exists and doesn't source `.bashrc` (Terminal opens
//! login shells, which read only `.bash_profile`); fish `$XDG_CONFIG_HOME/fish/conf.d/otaq.fish` (`~/.config`
//! without it), a file of its own. The line is the one page 32 shows to paste, so a pasted line is found and
//! removed the same way. Writing again never adds a second line. On Windows the folder is added to the user's own
//! `Path` in the registry (`HKCU\Environment`, kept `REG_EXPAND_SZ`), no PowerShell profile.
//!
//! **Removal** ([`remove_from_shells`], [`windows_remove`], for `otaq uninstall`): only lines marked and naming this
//! data folder's `bin`, in every file a line may be in whatever the shell is now; the rest of each file is left
//! byte for byte.

use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::data_dir::DataDir;
use crate::naming;

/// The command's file name in `bin/`: `otaq`, `otaq.exe` on Windows.
pub fn command_file_name() -> String {
    format!("{}{}", naming::COMMAND_NAME, std::env::consts::EXE_SUFFIX)
}

/// How `bin/otaq` points at the app's own `otaq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinKind {
    /// A symbolic link (macOS, into the bundle).
    Link,
    /// A copy of the file (Windows and Linux).
    Copy,
}

impl BinKind {
    pub fn for_this_os() -> Self {
        if cfg!(target_os = "macos") {
            Self::Link
        } else {
            Self::Copy
        }
    }
}

/// The plugin folder beside a copied `bin/otaq`: `bin/plugin`, the first place
/// [`crate::agent_tools::plugin_dir_next_to`] looks.
const BIN_PLUGIN_FOLDER: &str = "plugin";

/// What [`refresh_bin`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinRefresh {
    /// It already pointed at (or held) the app's own `otaq`.
    Unchanged,
    /// Made or replaced.
    Updated,
    /// A copy in use (an `otaq attach` still open on Windows): left as it is, replaced at the next start.
    InUse,
}

/// Makes `<data folder>/bin/otaq` point at `own` (the app's own `otaq`), creating `bin/` when missing. The new link
/// or copy is made under a temporary name and renamed over the old one, so nothing ever runs a half-written file. A
/// copy also gets `plugin` (the app's plugin folder) as `bin/plugin`, swapped in whole, and the ConPTY files beside
/// `own` when it has them; the returned state is the command's.
pub fn refresh_bin(
    data: &DataDir,
    own: &Path,
    kind: BinKind,
    plugin: Option<&Path>,
) -> io::Result<BinRefresh> {
    let bin = data.bin_dir();
    DataDir::ensure_dir(&bin)?;
    if kind == BinKind::Copy
        && let Some(plugin) = plugin
    {
        crate::runtime::replace_dir(plugin, &bin.join(BIN_PLUGIN_FOLDER))?;
    }
    if kind == BinKind::Copy
        && let Some(own_dir) = own.parent()
    {
        refresh_conpty(own_dir, &bin)?;
    }
    let target = bin.join(command_file_name());
    let temp = bin.join(format!(
        ".{}.new-{}",
        command_file_name(),
        std::process::id()
    ));
    match kind {
        BinKind::Link => {
            if std::fs::read_link(&target).is_ok_and(|to| to == own) {
                return Ok(BinRefresh::Unchanged);
            }
            let _ = std::fs::remove_file(&temp);
            symlink(own, &temp)?;
        }
        BinKind::Copy => {
            if same_file_contents(own, &target) {
                return Ok(BinRefresh::Unchanged);
            }
            let _ = std::fs::remove_file(&temp);
            if let Err(e) = std::fs::copy(own, &temp) {
                let _ = std::fs::remove_file(&temp);
                return Err(e);
            }
        }
    }
    match std::fs::rename(&temp, &target) {
        Ok(()) => Ok(BinRefresh::Updated),
        Err(e) => {
            let _ = std::fs::remove_file(&temp);
            // Windows refuses to replace a program that is running.
            if kind == BinKind::Copy
                && e.kind() == io::ErrorKind::PermissionDenied
                && target.exists()
            {
                Ok(BinRefresh::InUse)
            } else {
                Err(e)
            }
        }
    }
}

/// Copies Otaq's ConPTY files from `own_dir` into `bin` when they are there and differ, each under a temporary name
/// and renamed into place. One in use (a service started from `bin/` loaded it) is left, replaced at the next start.
fn refresh_conpty(own_dir: &Path, bin: &Path) -> io::Result<()> {
    use crate::session::conpty::{HOST, LIBRARY};
    for name in [LIBRARY, HOST] {
        let (from, to) = (own_dir.join(name), bin.join(name));
        if !from.is_file() || same_file_contents(&from, &to) {
            continue;
        }
        let temp = bin.join(format!(".{name}.new-{}", std::process::id()));
        let _ = std::fs::remove_file(&temp);
        let copied = std::fs::copy(&from, &temp).and_then(|_| std::fs::rename(&temp, &to));
        if let Err(e) = copied {
            let _ = std::fs::remove_file(&temp);
            if !(e.kind() == io::ErrorKind::PermissionDenied && to.exists()) {
                return Err(e);
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn symlink(original: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(not(unix))]
fn symlink(original: &Path, link: &Path) -> io::Result<()> {
    std::fs::copy(original, link).map(|_| ())
}

/// `b` is a plain file with the same bytes as `a` (not a link).
fn same_file_contents(a: &Path, b: &Path) -> bool {
    let (Ok(meta_a), Ok(meta_b)) = (std::fs::metadata(a), std::fs::symlink_metadata(b)) else {
        return false;
    };
    if !meta_b.is_file() || meta_a.len() != meta_b.len() {
        return false;
    }
    match (std::fs::read(a), std::fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// The login shells whose file the button writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Zsh,
    Bash,
    Fish,
}

impl ShellKind {
    /// From the login shell's program (`/bin/zsh`, `/opt/homebrew/bin/fish`); `None` for any other shell.
    pub fn from_program(program: &Path) -> Option<Self> {
        match program.file_name()?.to_str()? {
            "zsh" => Some(Self::Zsh),
            "bash" => Some(Self::Bash),
            "fish" => Some(Self::Fish),
            _ => None,
        }
    }
}

/// Where the shell files are, read from the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellEnv {
    pub home: PathBuf,
    /// The login shell: `$SHELL`, which login and launchd set from the account's entry.
    pub shell: Option<PathBuf>,
    /// `$ZDOTDIR`: zsh reads `.zshrc` there instead of the home folder.
    pub zdotdir: Option<PathBuf>,
    /// `$XDG_CONFIG_HOME`: fish's config folder is `fish/` in it.
    pub xdg_config_home: Option<PathBuf>,
    /// macOS: Terminal opens login shells, so bash's `.bash_profile` counts.
    pub macos: bool,
}

impl ShellEnv {
    /// This process's environment; `None` without a home folder.
    pub fn from_process() -> Option<Self> {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Some(Self {
            home: std::env::home_dir()?,
            shell: var("SHELL"),
            zdotdir: var("ZDOTDIR"),
            xdg_config_home: var("XDG_CONFIG_HOME"),
            macos: cfg!(target_os = "macos"),
        })
    }

    /// The login shell, when it is one the button writes for.
    pub fn shell_kind(&self) -> Option<ShellKind> {
        ShellKind::from_program(self.shell.as_deref()?)
    }

    fn zshrc(&self) -> PathBuf {
        self.zdotdir.as_ref().unwrap_or(&self.home).join(".zshrc")
    }

    fn fish_file(&self) -> PathBuf {
        self.xdg_config_home
            .clone()
            .unwrap_or_else(|| self.home.join(".config"))
            .join("fish")
            .join("conf.d")
            .join(format!("{}.fish", naming::COMMAND_NAME))
    }

    /// The files the button writes for `shell`, the one the row names first.
    pub fn shell_files(&self, shell: ShellKind) -> Vec<PathBuf> {
        match shell {
            ShellKind::Zsh => vec![self.zshrc()],
            ShellKind::Bash => {
                let mut files = vec![self.home.join(".bashrc")];
                let profile = self.home.join(".bash_profile");
                if self.macos
                    && std::fs::read_to_string(&profile).is_ok_and(|text| !sources_bashrc(&text))
                {
                    files.push(profile);
                }
                files
            }
            ShellKind::Fish => vec![self.fish_file()],
        }
    }

    /// Every file a line may be in, whatever the login shell is now (the uninstall looks in all of them).
    pub fn all_shell_files(&self) -> Vec<PathBuf> {
        let mut files = vec![self.home.join(".zshrc")];
        if self.zshrc() != files[0] {
            files.push(self.zshrc());
        }
        files.push(self.home.join(".bashrc"));
        files.push(self.home.join(".bash_profile"));
        files.push(self.fish_file());
        files
    }
}

/// Whether a `.bash_profile` reads `.bashrc`: a line that isn't a comment, names `.bashrc` and runs `source` or `.`.
fn sources_bashrc(profile: &str) -> bool {
    profile.lines().any(|line| {
        let line = line.trim();
        !line.starts_with('#')
            && line.contains(".bashrc")
            && line
                .split(|c: char| c.is_whitespace() || c == ';' || c == '&' || c == '|')
                .any(|word| word == "source" || word == ".")
    })
}

/// The mark at the end of the line: `# added by Otaq`.
pub fn marker() -> String {
    format!("# added by {}", naming::PRODUCT_NAME)
}

/// The folder as the line names it: `$HOME/.otaq/bin` under the home folder (escaped for `"…"`), else the full path.
fn quoted_dir(bin: &Path, home: &Path, shell: ShellKind) -> String {
    let escape = |text: &str| {
        let mut out = String::new();
        for c in text.chars() {
            if matches!(c, '\\' | '"' | '$') || (c == '`' && shell != ShellKind::Fish) {
                out.push('\\');
            }
            out.push(c);
        }
        out
    };
    match bin.strip_prefix(home) {
        Ok(rest) if !rest.as_os_str().is_empty() => {
            format!("$HOME/{}", escape(&rest.to_string_lossy()))
        }
        _ => escape(&bin.to_string_lossy()),
    }
}

/// The line the button writes, and the one page 32 shows to paste: `export PATH="$HOME/.otaq/bin:$PATH" # added by
/// Otaq`; for fish `set -gx PATH "$HOME/.otaq/bin" $PATH # added by Otaq`.
pub fn path_line(shell: ShellKind, bin: &Path, home: &Path) -> String {
    let dir = quoted_dir(bin, home, shell);
    match shell {
        ShellKind::Zsh | ShellKind::Bash => format!("export PATH=\"{dir}:$PATH\" {}", marker()),
        ShellKind::Fish => format!("set -gx PATH \"{dir}\" $PATH {}", marker()),
    }
}

/// A marked line naming this `bin`, in any shell's syntax.
fn is_our_line(line: &str, bin: &Path, home: &Path) -> bool {
    let line = line.trim_end();
    line.ends_with(&marker())
        && [ShellKind::Zsh, ShellKind::Fish]
            .iter()
            .map(|shell| quoted_dir(bin, home, *shell))
            .chain([bin.to_string_lossy().into_owned()])
            .any(|dir| line.contains(&dir))
}

/// The file's text has a marked line for `bin`.
pub fn has_line(text: &str, bin: &Path, home: &Path) -> bool {
    text.lines().any(|line| is_our_line(line, bin, home))
}

/// `text` with `line` appended on a line of its own.
pub fn with_line(text: &str, line: &str) -> String {
    let mut out = text.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(line);
    out.push('\n');
    out
}

/// `text` without its marked lines for `bin`, everything else byte for byte; `None` when it has none.
pub fn without_lines(text: &str, bin: &Path, home: &Path) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut removed = false;
    for line in text.split_inclusive('\n') {
        if is_our_line(line, bin, home) {
            removed = true;
        } else {
            out.push_str(line);
        }
    }
    removed.then_some(out)
}

/// Whether `bin` is one of `path`'s folders.
pub fn on_path(path: &OsStr, bin: &Path) -> bool {
    let canonical = crate::paths::canonical(bin).ok();
    std::env::split_paths(path).any(|dir| {
        dir == bin || (canonical.is_some() && crate::paths::canonical(&dir).ok() == canonical)
    })
}

/// The row's state before a click (page 32 item 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathStatus {
    /// Not on the PATH: the offer with **Add to my shell's PATH**.
    Offer,
    /// The line is in the shell file, but the login shell's PATH doesn't have it yet (or couldn't be read).
    Added,
    /// The login shell's PATH (or Windows' user PATH) already has the folder.
    AlreadyThere,
}

/// The state from the login shell's PATH (`None` when it couldn't be read) and whether a shell file has the line.
pub fn status(login_path: Option<&OsStr>, bin: &Path, line_written: bool) -> PathStatus {
    if login_path.is_some_and(|path| on_path(path, bin)) {
        PathStatus::AlreadyThere
    } else if line_written {
        PathStatus::Added
    } else {
        PathStatus::Offer
    }
}

/// Whether any of `files` has the marked line for `bin`.
pub fn line_written(files: &[PathBuf], bin: &Path, home: &Path) -> bool {
    files
        .iter()
        .any(|file| std::fs::read_to_string(file).is_ok_and(|text| has_line(&text, bin, home)))
}

/// What a click did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddResult {
    /// The line was appended to these files (the others already had it).
    Added { files: Vec<PathBuf> },
    /// Every file already had it, or the folder is already on the user's PATH.
    AlreadyThere,
    /// `file` couldn't be written: the line to paste (on Windows, the folder to add).
    Failed {
        file: Option<PathBuf>,
        /// Refused for lack of rights: "The file is read-only".
        read_only: bool,
        why: String,
        line: String,
    },
}

/// **Add to my shell's PATH** for `shell`: appends the line to each of its files that doesn't have it yet, creating a
/// missing file (and fish's `conf.d/`). Appending writes into the file itself, so a `.zshrc` that is a link (a
/// dotfiles folder) stays a link.
pub fn add_to_shell(env: &ShellEnv, shell: ShellKind, bin: &Path) -> AddResult {
    let line = path_line(shell, bin, &env.home);
    let mut written = Vec::new();
    for file in env.shell_files(shell) {
        let failed = |e: io::Error| AddResult::Failed {
            file: Some(file.clone()),
            read_only: e.kind() == io::ErrorKind::PermissionDenied,
            why: e.to_string(),
            line: line.clone(),
        };
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return failed(e),
        };
        if has_line(&text, bin, &env.home) {
            continue;
        }
        if shell == ShellKind::Fish
            && let Some(parent) = file.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return failed(e);
        }
        let appended = with_line(&text, &line);
        let result = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .and_then(|mut f| f.write_all(&appended.as_bytes()[text.len()..]));
        if let Err(e) = result {
            return failed(e);
        }
        tracing::info!(file = %file.display(), "shell path: line added");
        written.push(file);
    }
    if written.is_empty() {
        AddResult::AlreadyThere
    } else {
        AddResult::Added { files: written }
    }
}

/// One file the uninstall changed, or couldn't.
#[derive(Debug)]
pub struct Removed {
    pub file: PathBuf,
    pub result: io::Result<()>,
}

/// Removes this data folder's marked lines from every shell file that has one (`otaq uninstall`, UPD-09). Fish's
/// own file is deleted once nothing else is in it.
pub fn remove_from_shells(env: &ShellEnv, bin: &Path) -> Vec<Removed> {
    let mut done = Vec::new();
    for file in env.all_shell_files() {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Some(rest) = without_lines(&text, bin, &env.home) else {
            continue;
        };
        let result = if file == env.fish_file() && rest.trim().is_empty() {
            std::fs::remove_file(&file)
        } else {
            std::fs::write(&file, rest)
        };
        done.push(Removed { file, result });
    }
    done
}

// Windows: the user's own `Path` (`HKCU\Environment`).

/// One entry of a `Path` is `dir`: case-insensitive, a trailing `\` ignored.
fn same_windows_dir(entry: &str, dir: &str) -> bool {
    let clean = |s: &str| s.trim().trim_end_matches(['\\', '/']).to_lowercase();
    !entry.trim().is_empty() && clean(entry) == clean(dir)
}

/// Whether the `Path` value has `dir`.
pub fn windows_has(path: &str, dir: &str) -> bool {
    path.split(';').any(|entry| same_windows_dir(entry, dir))
}

/// The `Path` value with `dir` added at the end; `None` when it's already there. Other entries are kept as they are,
/// `%USERPROFILE%`-style ones unexpanded. A value ending in `;` keeps ending in one, so removing `dir` again gives
/// back the same value.
pub fn windows_with(path: &str, dir: &str) -> Option<String> {
    if windows_has(path, dir) {
        return None;
    }
    Some(if path.is_empty() {
        dir.to_owned()
    } else if path.ends_with(';') {
        format!("{path}{dir};")
    } else {
        format!("{path};{dir}")
    })
}

/// The `Path` value without `dir`; `None` when it isn't there. Everything else is kept as it is.
pub fn windows_without(path: &str, dir: &str) -> Option<String> {
    windows_has(path, dir).then(|| {
        path.split(';')
            .filter(|entry| !same_windows_dir(entry, dir))
            .collect::<Vec<_>>()
            .join(";")
    })
}

/// The user's own `Path`: the registry on Windows, a stand-in in tests.
pub trait UserPath {
    /// The value, unexpanded; `None` when the user has none.
    fn read(&self) -> io::Result<Option<String>>;
    /// Writes it (as `REG_EXPAND_SZ`) and tells running programs the environment changed.
    fn write(&self, value: &str) -> io::Result<()>;
}

/// **Add to my shell's PATH** on Windows: `bin` added to the user's own `Path`, for this account only.
pub fn windows_add(store: &dyn UserPath, bin: &Path) -> AddResult {
    let dir = bin.to_string_lossy().into_owned();
    let failed = |e: io::Error| AddResult::Failed {
        file: None,
        read_only: e.kind() == io::ErrorKind::PermissionDenied,
        why: e.to_string(),
        line: dir.clone(),
    };
    let current = match store.read() {
        Ok(current) => current.unwrap_or_default(),
        Err(e) => return failed(e),
    };
    match windows_with(&current, &dir) {
        None => AddResult::AlreadyThere,
        Some(value) => match store.write(&value) {
            Ok(()) => AddResult::Added { files: Vec::new() },
            Err(e) => failed(e),
        },
    }
}

/// Removes `bin` from the user's own `Path`; `Ok(true)` when it was there.
pub fn windows_remove(store: &dyn UserPath, bin: &Path) -> io::Result<bool> {
    let current = store.read()?.unwrap_or_default();
    match windows_without(&current, &bin.to_string_lossy()) {
        None => Ok(false),
        Some(value) => store.write(&value).map(|()| true),
    }
}

/// The user's own `Path` in the registry, `HKCU\Environment\Path`.
#[cfg(windows)]
pub struct RegistryUserPath;

#[cfg(windows)]
impl UserPath for RegistryUserPath {
    fn read(&self) -> io::Result<Option<String>> {
        use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
        use windows_sys::Win32::System::Registry::{
            HKEY_CURRENT_USER, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegGetValueW,
        };
        let (key, value) = (wide("Environment"), wide("Path"));
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        let mut bytes: u32 = 0;
        // SAFETY: both names are NUL-terminated; a null buffer asks only for the size.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                flags,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut bytes,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let mut buffer = vec![0u16; (bytes as usize).div_ceil(2) + 1];
        let mut bytes = (buffer.len() * 2) as u32;
        // SAFETY: `buffer` holds `bytes` bytes.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                flags,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Ok(Some(String::from_utf16_lossy(&buffer[..len])))
    }

    fn write(&self, path: &str) -> io::Result<()> {
        use windows_sys::Win32::Foundation::ERROR_SUCCESS;
        use windows_sys::Win32::System::Registry::{
            HKEY_CURRENT_USER, REG_EXPAND_SZ, RegSetKeyValueW,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
        };
        let (key, value, data) = (wide("Environment"), wide("Path"), wide(path));
        // SAFETY: the names and the data are NUL-terminated; the size counts the data's bytes with the NUL.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                REG_EXPAND_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        // New terminals opened from Explorer and the Start menu read the new PATH.
        let area = wide("Environment");
        let mut result: usize = 0;
        // SAFETY: `area` is NUL-terminated and outlives the call, which waits at most a second per window.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                area.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                1000,
                &mut result,
            )
        };
        Ok(())
    }
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Home {
        _tmp: tempfile::TempDir,
        env: ShellEnv,
        bin: PathBuf,
    }

    fn home(shell: &str, macos: bool) -> Home {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let bin = home.join(naming::data_dir_name()).join("bin");
        Home {
            env: ShellEnv {
                home,
                shell: Some(PathBuf::from(format!("/bin/{shell}"))),
                zdotdir: None,
                xdg_config_home: None,
                macos,
            },
            bin,
            _tmp: tmp,
        }
    }

    fn line_count(text: &str) -> usize {
        text.matches(&marker()).count()
    }

    // Unix only: the shell file's line is written on macOS and Linux; Windows adds the folder to the registry's Path.
    #[cfg(unix)]
    #[test]
    fn the_line_names_the_folder_under_home() {
        let home = Path::new("/Users/me");
        let bin = home.join(naming::data_dir_name()).join("bin");
        let shown = format!("$HOME/{}/bin", naming::data_dir_name());
        assert_eq!(
            path_line(ShellKind::Zsh, &bin, home),
            format!("export PATH=\"{shown}:$PATH\" {}", marker())
        );
        assert_eq!(
            path_line(ShellKind::Fish, &bin, home),
            format!("set -gx PATH \"{shown}\" $PATH {}", marker())
        );
        // A data folder elsewhere (OTAQ_HOME) is named in full, quoted.
        assert_eq!(
            path_line(ShellKind::Bash, Path::new("/tmp/a \"b\"/bin"), home),
            format!("export PATH=\"/tmp/a \\\"b\\\"/bin:$PATH\" {}", marker())
        );
        assert!(marker().ends_with(naming::PRODUCT_NAME));
    }

    #[test]
    fn zsh_one_line_however_often_and_removal_leaves_the_rest() {
        let h = home("zsh", true);
        let zshrc = h.env.home.join(".zshrc");
        let before = "alias ll='ls -l'\nexport EDITOR=vim"; // no newline at the end
        std::fs::write(&zshrc, before).unwrap();
        assert_eq!(
            add_to_shell(&h.env, ShellKind::Zsh, &h.bin),
            AddResult::Added {
                files: vec![zshrc.clone()]
            }
        );
        assert_eq!(
            add_to_shell(&h.env, ShellKind::Zsh, &h.bin),
            AddResult::AlreadyThere
        );
        let text = std::fs::read_to_string(&zshrc).unwrap();
        assert_eq!(line_count(&text), 1, "{text}");
        assert!(text.starts_with(before) && text.ends_with(&format!("{}\n", marker())));
        assert!(line_written(
            &h.env.shell_files(ShellKind::Zsh),
            &h.bin,
            &h.env.home
        ));

        // Something the user adds after it stays.
        std::fs::write(&zshrc, format!("{text}eval \"$(starship init zsh)\"\n")).unwrap();
        let removed = remove_from_shells(&h.env, &h.bin);
        assert_eq!(removed.len(), 1);
        assert!(removed[0].result.is_ok());
        assert_eq!(
            std::fs::read_to_string(&zshrc).unwrap(),
            format!("{before}\neval \"$(starship init zsh)\"\n")
        );
    }

    #[test]
    fn zsh_follows_zdotdir_and_a_missing_file_is_made() {
        let mut h = home("zsh", false);
        let dot = h.env.home.join("zdot");
        std::fs::create_dir_all(&dot).unwrap();
        h.env.zdotdir = Some(dot.clone());
        assert_eq!(h.env.shell_files(ShellKind::Zsh), vec![dot.join(".zshrc")]);
        add_to_shell(&h.env, ShellKind::Zsh, &h.bin);
        assert_eq!(
            std::fs::read_to_string(dot.join(".zshrc")).unwrap(),
            format!("{}\n", path_line(ShellKind::Zsh, &h.bin, &h.env.home))
        );
        assert!(!h.env.home.join(".zshrc").exists());
        assert!(h.env.all_shell_files().contains(&dot.join(".zshrc")));
        remove_from_shells(&h.env, &h.bin);
        assert_eq!(std::fs::read_to_string(dot.join(".zshrc")).unwrap(), "");
    }

    #[test]
    fn bash_on_macos_also_writes_a_bash_profile_that_skips_bashrc() {
        let h = home("bash", true);
        let (rc, profile) = (h.env.home.join(".bashrc"), h.env.home.join(".bash_profile"));
        // No .bash_profile: only .bashrc.
        assert_eq!(h.env.shell_files(ShellKind::Bash), vec![rc.clone()]);
        // One that sources .bashrc: still only .bashrc.
        std::fs::write(&profile, "if [ -f ~/.bashrc ]; then . ~/.bashrc; fi\n").unwrap();
        assert_eq!(h.env.shell_files(ShellKind::Bash), vec![rc.clone()]);
        std::fs::write(&profile, "[ -r ~/.bashrc ] && source ~/.bashrc\n").unwrap();
        assert_eq!(h.env.shell_files(ShellKind::Bash), vec![rc.clone()]);
        // One that doesn't (a comment naming it doesn't count): both.
        std::fs::write(&profile, "# see ~/.bashrc\nexport A=1\n").unwrap();
        assert_eq!(
            h.env.shell_files(ShellKind::Bash),
            vec![rc.clone(), profile.clone()]
        );
        assert_eq!(
            add_to_shell(&h.env, ShellKind::Bash, &h.bin),
            AddResult::Added {
                files: vec![rc.clone(), profile.clone()]
            }
        );
        assert_eq!(
            add_to_shell(&h.env, ShellKind::Bash, &h.bin),
            AddResult::AlreadyThere
        );
        assert_eq!(remove_from_shells(&h.env, &h.bin).len(), 2);
        assert_eq!(std::fs::read_to_string(&rc).unwrap(), "");
        assert_eq!(
            std::fs::read_to_string(&profile).unwrap(),
            "# see ~/.bashrc\nexport A=1\n"
        );
        // On Linux only .bashrc.
        let linux = ShellEnv {
            macos: false,
            ..h.env.clone()
        };
        assert_eq!(linux.shell_files(ShellKind::Bash), vec![rc]);
    }

    #[test]
    fn fish_gets_a_file_of_its_own_in_conf_d() {
        let mut h = home("fish", false);
        assert_eq!(h.env.shell_kind(), Some(ShellKind::Fish));
        let config = h.env.home.join("cfg");
        h.env.xdg_config_home = Some(config.clone());
        let file = config
            .join("fish")
            .join("conf.d")
            .join(format!("{}.fish", naming::COMMAND_NAME));
        assert_eq!(h.env.shell_files(ShellKind::Fish), vec![file.clone()]);
        add_to_shell(&h.env, ShellKind::Fish, &h.bin);
        add_to_shell(&h.env, ShellKind::Fish, &h.bin);
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(
            text,
            format!("{}\n", path_line(ShellKind::Fish, &h.bin, &h.env.home))
        );
        assert!(text.starts_with("set -gx PATH \"$HOME/"));
        remove_from_shells(&h.env, &h.bin);
        assert!(!file.exists(), "nothing else in it: deleted");
    }

    // Unix only: the shell file's line is written on macOS and Linux; Windows adds the folder to the registry's Path.
    #[cfg(unix)]
    #[test]
    fn another_data_folders_line_and_pasted_lines() {
        let h = home("zsh", true);
        let zshrc = h.env.home.join(".zshrc");
        let other = Path::new("/tmp/elsewhere/bin");
        // The other folder's line, and this one pasted by hand with two spaces before the mark (page 32's copy).
        let pasted = format!(
            "export PATH=\"$HOME/{}/bin:$PATH\"  {}",
            naming::data_dir_name(),
            marker()
        );
        let text = format!(
            "{}\n{pasted}\n",
            path_line(ShellKind::Zsh, other, &h.env.home)
        );
        std::fs::write(&zshrc, &text).unwrap();
        assert!(has_line(&text, &h.bin, &h.env.home));
        assert_eq!(
            add_to_shell(&h.env, ShellKind::Zsh, &h.bin),
            AddResult::AlreadyThere
        );
        remove_from_shells(&h.env, &h.bin);
        assert_eq!(
            std::fs::read_to_string(&zshrc).unwrap(),
            format!("{}\n", path_line(ShellKind::Zsh, other, &h.env.home)),
            "only this data folder's line goes"
        );
        assert_eq!(without_lines("a\nb\n", &h.bin, &h.env.home), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_file_gives_the_line_to_paste() {
        use std::os::unix::fs::PermissionsExt;
        let h = home("zsh", true);
        let zshrc = h.env.home.join(".zshrc");
        std::fs::write(&zshrc, "export A=1\n").unwrap();
        std::fs::set_permissions(&zshrc, std::fs::Permissions::from_mode(0o444)).unwrap();
        match add_to_shell(&h.env, ShellKind::Zsh, &h.bin) {
            AddResult::Failed {
                file,
                line,
                read_only,
                ..
            } => {
                assert_eq!(file, Some(zshrc.clone()));
                assert!(read_only);
                assert_eq!(line, path_line(ShellKind::Zsh, &h.bin, &h.env.home));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), "export A=1\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_zshrc_stays_a_link() {
        let h = home("zsh", true);
        let real = h.env.home.join("dotfiles-zshrc");
        std::fs::write(&real, "export A=1\n").unwrap();
        std::os::unix::fs::symlink(&real, h.env.home.join(".zshrc")).unwrap();
        add_to_shell(&h.env, ShellKind::Zsh, &h.bin);
        assert!(
            h.env
                .home
                .join(".zshrc")
                .symlink_metadata()
                .unwrap()
                .is_symlink()
        );
        assert_eq!(line_count(&std::fs::read_to_string(&real).unwrap()), 1);
        remove_from_shells(&h.env, &h.bin);
        assert!(
            h.env
                .home
                .join(".zshrc")
                .symlink_metadata()
                .unwrap()
                .is_symlink()
        );
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "export A=1\n");
    }

    #[test]
    fn the_login_shells_path_decides_the_state() {
        let bin = PathBuf::from("/h/d/bin");
        let path = std::env::join_paths(["/usr/bin", "/h/d/bin/"]).unwrap();
        assert_eq!(status(Some(&path), &bin, false), PathStatus::AlreadyThere);
        let without = std::env::join_paths(["/usr/bin"]).unwrap();
        assert_eq!(status(Some(&without), &bin, false), PathStatus::Offer);
        assert_eq!(status(Some(&without), &bin, true), PathStatus::Added);
        assert_eq!(status(None, &bin, true), PathStatus::Added);
        assert_eq!(status(None, &bin, false), PathStatus::Offer);
        assert_eq!(
            ShellKind::from_program(Path::new("/bin/zsh")),
            Some(ShellKind::Zsh)
        );
        assert_eq!(ShellKind::from_program(Path::new("/bin/tcsh")), None);
    }

    #[test]
    fn windows_path_edits_keep_every_other_entry() {
        let dir = r"C:\Users\me\.data\bin";
        let path = r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\tools;";
        let added = windows_with(path, dir).unwrap();
        assert_eq!(added, format!("{path}{dir};"));
        assert_eq!(windows_with(&added, dir), None);
        assert_eq!(
            windows_with(&added, &format!("{}\\", dir.to_uppercase())),
            None
        );
        assert_eq!(windows_without(&added, dir).as_deref(), Some(path));
        assert_eq!(windows_with("", dir).as_deref(), Some(dir));
        assert_eq!(windows_with("C:\\a", dir), Some(format!("C:\\a;{dir}")));
        // In the middle: removed, the rest (an empty entry too) as it was.
        assert_eq!(
            windows_without(&format!("C:\\a;{dir};;C:\\b"), dir).as_deref(),
            Some("C:\\a;;C:\\b")
        );
        assert_eq!(windows_without("C:\\a", dir), None);
    }

    struct FakePath(RefCell<Option<String>>, bool);
    impl UserPath for FakePath {
        fn read(&self) -> io::Result<Option<String>> {
            Ok(self.0.borrow().clone())
        }
        fn write(&self, value: &str) -> io::Result<()> {
            if self.1 {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            *self.0.borrow_mut() = Some(value.to_owned());
            Ok(())
        }
    }

    #[test]
    fn windows_add_and_remove_through_the_seam() {
        let bin = Path::new(r"C:\Users\me\.data\bin");
        let store = FakePath(RefCell::new(None), false);
        assert_eq!(
            windows_add(&store, bin),
            AddResult::Added { files: Vec::new() }
        );
        assert_eq!(windows_add(&store, bin), AddResult::AlreadyThere);
        assert_eq!(store.0.borrow().as_deref(), Some(bin.to_str().unwrap()));
        assert!(windows_remove(&store, bin).unwrap());
        assert!(!windows_remove(&store, bin).unwrap());
        assert_eq!(store.0.borrow().as_deref(), Some(""));
        let refused = FakePath(RefCell::new(Some("C:\\a".into())), true);
        assert!(matches!(
            windows_add(&refused, bin),
            AddResult::Failed { file: None, .. }
        ));
    }

    #[test]
    fn bin_copy_is_made_kept_and_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path().join("data"));
        let own = tmp.path().join("own");
        std::fs::write(&own, b"build 1").unwrap();
        let target = data.bin_dir().join(command_file_name());
        assert_eq!(
            refresh_bin(&data, &own, BinKind::Copy, None).unwrap(),
            BinRefresh::Updated
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"build 1");
        assert_eq!(
            refresh_bin(&data, &own, BinKind::Copy, None).unwrap(),
            BinRefresh::Unchanged
        );
        // Another copy of the app opened last wins.
        let other = tmp.path().join("other");
        std::fs::write(&other, b"build 2").unwrap();
        assert_eq!(
            refresh_bin(&data, &other, BinKind::Copy, None).unwrap(),
            BinRefresh::Updated
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"build 2");
        // Only the command in bin/, no temporary file left.
        assert_eq!(std::fs::read_dir(data.bin_dir()).unwrap().count(), 1);
    }

    /// A copy gets the app's plugin folder beside it (QB), replaced whole by the next start; a link gets none.
    #[test]
    fn bin_copy_gets_the_plugin_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path().join("data"));
        let own = tmp.path().join("own");
        std::fs::write(&own, b"build 1").unwrap();
        let plugin = tmp.path().join("app-plugin");
        let manifest = plugin.join("claude").join(".claude-plugin");
        std::fs::create_dir_all(&manifest).unwrap();
        std::fs::write(manifest.join("plugin.json"), "{}").unwrap();
        refresh_bin(&data, &own, BinKind::Copy, Some(&plugin)).unwrap();
        let copied = data.bin_dir().join(BIN_PLUGIN_FOLDER);
        let copied_manifest = copied.join("claude/.claude-plugin/plugin.json");
        assert_eq!(std::fs::read_to_string(&copied_manifest).unwrap(), "{}");
        assert_eq!(
            crate::agent_tools::plugin_dir_next_to(&data.bin_dir()),
            Some(copied.clone())
        );

        // The next build's plugin, with the command unchanged: replaced whole, nothing temporary left.
        std::fs::write(manifest.join("plugin.json"), "{\"v\":2}").unwrap();
        std::fs::write(copied.join("stale"), "").unwrap();
        assert_eq!(
            refresh_bin(&data, &own, BinKind::Copy, Some(&plugin)).unwrap(),
            BinRefresh::Unchanged
        );
        assert_eq!(
            std::fs::read_to_string(&copied_manifest).unwrap(),
            "{\"v\":2}"
        );
        assert!(!copied.join("stale").exists());
        let mut names: Vec<String> = std::fs::read_dir(data.bin_dir())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, [command_file_name(), BIN_PLUGIN_FOLDER.to_owned()]);
    }

    /// A copy gets the ConPTY files that are beside the app's `otaq` (Windows), replaced when they change; without
    /// them beside it, none.
    #[test]
    fn bin_copy_gets_the_conpty_files_beside_it() {
        use crate::session::conpty::{HOST, LIBRARY};
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path().join("data"));
        let app = tmp.path().join("app");
        std::fs::create_dir_all(&app).unwrap();
        let own = app.join(command_file_name());
        std::fs::write(&own, b"build 1").unwrap();
        refresh_bin(&data, &own, BinKind::Copy, None).unwrap();
        assert!(!data.bin_dir().join(LIBRARY).exists());

        std::fs::write(app.join(LIBRARY), b"dll 1").unwrap();
        std::fs::write(app.join(HOST), b"host 1").unwrap();
        refresh_bin(&data, &own, BinKind::Copy, None).unwrap();
        assert_eq!(
            std::fs::read(data.bin_dir().join(LIBRARY)).unwrap(),
            b"dll 1"
        );
        assert_eq!(std::fs::read(data.bin_dir().join(HOST)).unwrap(), b"host 1");

        std::fs::write(app.join(LIBRARY), b"dll 2").unwrap();
        assert_eq!(
            refresh_bin(&data, &own, BinKind::Copy, None).unwrap(),
            BinRefresh::Unchanged
        );
        assert_eq!(
            std::fs::read(data.bin_dir().join(LIBRARY)).unwrap(),
            b"dll 2"
        );
        let mut names: Vec<String> = std::fs::read_dir(data.bin_dir())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let mut want = vec![command_file_name(), LIBRARY.to_owned(), HOST.to_owned()];
        want.sort();
        assert_eq!(names, want);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_gets_no_plugin_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path().join("data"));
        let own = tmp.path().join("own");
        std::fs::write(&own, b"x").unwrap();
        let plugin = tmp.path().join("app-plugin");
        std::fs::create_dir_all(&plugin).unwrap();
        refresh_bin(&data, &own, BinKind::Link, Some(&plugin)).unwrap();
        assert!(!data.bin_dir().join(BIN_PLUGIN_FOLDER).exists());
    }

    #[cfg(unix)]
    #[test]
    fn bin_link_points_into_the_app_and_follows_the_last_opened() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path().join("data"));
        let own = tmp.path().join("own");
        std::fs::write(&own, b"x").unwrap();
        let target = data.bin_dir().join(command_file_name());
        assert_eq!(
            refresh_bin(&data, &own, BinKind::Link, None).unwrap(),
            BinRefresh::Updated
        );
        assert_eq!(std::fs::read_link(&target).unwrap(), own);
        assert_eq!(
            refresh_bin(&data, &own, BinKind::Link, None).unwrap(),
            BinRefresh::Unchanged
        );
        // A copy left there (or a broken link) is replaced.
        let other = tmp.path().join("other");
        assert_eq!(
            refresh_bin(&data, &other, BinKind::Link, None).unwrap(),
            BinRefresh::Updated
        );
        assert_eq!(std::fs::read_link(&target).unwrap(), other);
        assert_eq!(std::fs::read_dir(data.bin_dir()).unwrap().count(), 1);
    }

    /// A new terminal (an interactive login zsh, with nothing but a temporary HOME) finds the command once the line
    /// is added, and not after the uninstall's removal. Skipped where zsh isn't installed.
    #[cfg(unix)]
    #[test]
    fn a_new_zsh_finds_the_command() {
        let zsh = Path::new("/bin/zsh");
        if !zsh.exists() {
            return;
        }
        let h = home("zsh", cfg!(target_os = "macos"));
        let data = DataDir::new(h.env.home.join(naming::data_dir_name()));
        let own = h.env.home.join("own");
        std::fs::write(&own, "#!/bin/sh\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&own, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        refresh_bin(&data, &own, BinKind::for_this_os(), None).unwrap();
        let found = || {
            let out = std::process::Command::new(zsh)
                .args([
                    "-l",
                    "-i",
                    "-c",
                    &format!("command -v {}", naming::COMMAND_NAME),
                ])
                .env_clear()
                .env("HOME", &h.env.home)
                .env("PATH", "/usr/bin:/bin")
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_owned()
        };
        assert_eq!(found(), "");
        add_to_shell(&h.env, ShellKind::Zsh, &data.bin_dir());
        assert_eq!(
            found(),
            data.bin_dir()
                .join(command_file_name())
                .display()
                .to_string()
        );
        remove_from_shells(&h.env, &data.bin_dir());
        assert_eq!(found(), "");
    }
}

//! A start from a notification's link: on Windows a click on a toast opens its `otaq://open/…` link
//! ([`otaq_core::attention::notify::open_link`]), which the installer registers to start this app with the link as
//! its argument. The start then opens the link's target as a start with `OTAQ_OPEN` does: the copy that runs comes
//! forward on it, or this one opens on it.

use std::ffi::OsString;
use std::path::Path;

use otaq_core::attention::notify::{open_env_var, parse_open_link};
use otaq_core::data_dir::DataDir;

/// The variables a start with `args` sets before anything reads them: `OTAQ_OPEN` from the first argument that is an
/// open link, and `OTAQ_HOME` when the link names a data folder that `holds_data` (an Otaq data folder already
/// there). A link from anywhere else can't start the app on a folder of its choosing.
pub fn variables(args: &[OsString], holds_data: impl Fn(&Path) -> bool) -> Vec<(String, OsString)> {
    let Some(link) = args
        .iter()
        .filter_map(|arg| arg.to_str())
        .find_map(parse_open_link)
    else {
        return Vec::new();
    };
    let mut vars = vec![(open_env_var(), OsString::from(link.open))];
    if let Some(home) = link.home.filter(|home| holds_data(home)) {
        vars.push((DataDir::env_var(), home.into_os_string()));
    }
    vars
}

/// Whether `home` is an Otaq data folder: its database is there.
pub fn holds_data(home: &Path) -> bool {
    DataDir::new(home).database_path().is_file()
}

/// Sets the start's [`variables`]. A start from a click may bring a window forward, and Windows lets only the
/// program the user clicked do that: it passes that on, so the copy that runs can come forward when this start hands
/// over to it.
pub fn apply() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let vars = variables(&args, holds_data);
    if vars.is_empty() {
        return;
    }
    for (name, value) in vars {
        // SAFETY: called first thing in `main`, when the app has no other thread that could read the environment.
        unsafe { std::env::set_var(name, value) };
    }
    #[cfg(windows)]
    // SAFETY: a plain call with no pointers.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(
            windows_sys::Win32::UI::WindowsAndMessaging::ASFW_ANY,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    /// A start's argument: a link of Otaq's scheme, `rest` following the scheme.
    fn link(rest: &str) -> String {
        format!("{}{rest}", otaq_core::attention::notify::OPEN_LINK_SCHEME)
    }

    /// A click on a toast starts the app with its link: the app opens the ticket, on the service's own data folder
    /// when the link names one that is an Otaq data folder.
    #[test]
    fn a_start_from_a_toasts_link_opens_its_ticket() {
        let vars = variables(
            &args(&[&link("://open/SHOP-22?home=C%3A%5Cdata")]),
            |home| home == Path::new(r"C:\data"),
        );
        assert_eq!(
            vars,
            vec![
                (open_env_var(), OsString::from("SHOP-22")),
                (DataDir::env_var(), OsString::from(r"C:\data")),
            ]
        );
        let elsewhere = variables(&args(&[&link("://open/SHOP-22?home=C%3A%5Cother")]), |_| {
            false
        });
        assert_eq!(elsewhere, vec![(open_env_var(), OsString::from("SHOP-22"))]);
    }

    /// The installer registers the scheme the toasts' links use, to start the installed app with the link, and the
    /// uninstaller removes it.
    #[test]
    fn the_installer_registers_the_links_scheme() {
        let hooks = include_str!("../bundle/windows/hooks.nsh");
        let key = format!(
            r#""Software\Classes\{}""#,
            otaq_core::attention::notify::OPEN_LINK_SCHEME
        );
        let command = format!(
            r#"'"$INSTDIR\{}-app.exe" "%1"'"#,
            otaq_core::naming::COMMAND_NAME
        );
        assert!(hooks.contains(&format!(r#"WriteRegStr HKCU {key} "URL Protocol" """#)));
        assert!(hooks.contains(&command), "{command}");
        assert!(hooks.contains(&format!("DeleteRegKey HKCU {key}")));
    }

    #[test]
    fn a_plain_start_sets_nothing() {
        assert!(variables(&args(&[]), |_| true).is_empty());
        assert!(variables(&args(&["--notify-check"]), |_| true).is_empty());
        assert!(variables(&args(&[&link("://open/not%20a%20target")]), |_| true).is_empty());
    }
}

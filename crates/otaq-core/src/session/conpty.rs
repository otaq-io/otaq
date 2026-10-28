//! Windows: the ConPTY that sessions run in (Q601, d0117 point 4).
//!
//! Windows' own ConPTY (conhost, 10.0.26100) swallows a program's colour questions (OSC 10 / 11): neither the service
//! ([`super::colors`]) nor the app's terminal ever sees one, and an answer written to the session is dropped on its
//! way in. Every tool then falls back to its default colours after its own timeout. Microsoft's newer ConPTY, the one
//! Windows Terminal ships (`conpty.dll` with its `OpenConsole.exe`, MIT), passes both through.
//!
//! portable-pty loads a `conpty.dll` by name when one can be found, and Windows' own otherwise. A module already
//! loaded is what a load by name finds, so Otaq loads its own copy by its full path before the first session: from
//! the program's folder (the runtime copy, the installed app), or in a debug build from `OTAQ_DEV_CONPTY_DIR` (tests,
//! `scripts/check.ps1`). Without a copy, sessions run in Windows' own ConPTY, as before.

use std::path::PathBuf;

use crate::naming::PRODUCT_NAME;

/// The library portable-pty looks for.
pub const LIBRARY: &str = "conpty.dll";
/// The console host the library starts, beside it.
pub const HOST: &str = "OpenConsole.exe";

/// The debug build's folder holding the two files ([`crate::runtime`] copies them beside `otaq`).
pub fn dir_var() -> String {
    crate::naming::env_var("DEV_CONPTY_DIR")
}

/// The folders looked in, in order.
fn candidates() -> Vec<PathBuf> {
    let own = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from));
    let seam = cfg!(debug_assertions)
        .then(|| std::env::var_os(dir_var()).map(PathBuf::from))
        .flatten();
    own.into_iter().chain(seam).collect()
}

/// Loads Otaq's copy, once, before portable-pty's first load. Which one runs sessions is logged.
pub fn preload() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let found = candidates()
            .into_iter()
            .find(|dir| dir.join(LIBRARY).is_file() && dir.join(HOST).is_file());
        let Some(dir) = found else {
            tracing::info!("sessions run in Windows' own ConPTY: no {LIBRARY} beside the program");
            return;
        };
        let path = dir.join(LIBRARY);
        if load(&path) {
            tracing::info!(library = %path.display(), "sessions run in {PRODUCT_NAME}'s ConPTY");
        } else {
            tracing::warn!(library = %path.display(), error = %std::io::Error::last_os_error(),
                "can't load {PRODUCT_NAME}'s ConPTY; sessions run in Windows' own");
        }
    });
}

#[cfg(windows)]
fn load(path: &std::path::Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // Never freed: portable-pty's load by name gets this module for the life of the process.
    !unsafe { windows_sys::Win32::System::LibraryLoader::LoadLibraryW(wide.as_ptr()) }.is_null()
}

#[cfg(not(windows))]
fn load(_: &std::path::Path) -> bool {
    false
}

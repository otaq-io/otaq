//! How the texts name the computer Otaq runs on: "this Mac" on macOS, "this computer" elsewhere.

/// The machine's name in "isn't set up on this Mac".
pub fn this_machine() -> &'static str {
    if cfg!(target_os = "macos") {
        "this Mac"
    } else {
        "this computer"
    }
}

//! A stand-in npm install on Windows (d0088 point 2): `codex.cmd` as npm wrote it (`tests/fixtures/npm-shims`), with
//! npm's extensionless `sh` script beside it, and a script in `node_modules` that writes the arguments it got as
//! JSON to the file named by `ARGV_OUT`; Node is the one on the test's PATH.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// A first message `cmd.exe` would act on: `&` and `|` end a command, `%` expands, `"` and `^` quote and escape,
/// `<` redirects.
pub const MESSAGE: &str = "Fix the A&B page \u{2014} 100% \"done\" ^ | <x>";

/// More text `cmd.exe` would change: a variable, a backslash before a quote, a trailing backslash.
pub const MORE: &str = r#"%PATH% !x! a\"b c\"#;

/// The npm prefix folder (`codex.cmd` in it) under `dir`.
pub fn install(dir: &Path) -> PathBuf {
    let npm = dir.join("npm");
    let bin = npm
        .join("node_modules")
        .join("@openai")
        .join("codex")
        .join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(
        bin.join("codex.js"),
        "require('fs').writeFileSync(process.env.ARGV_OUT, JSON.stringify(process.argv.slice(2)));\n",
    )
    .unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/npm-shims");
    std::fs::copy(fixtures.join("codex.cmd"), npm.join("codex.cmd")).unwrap();
    std::fs::write(npm.join("codex"), "#!/bin/sh\nexec node codex.js \"$@\"\n").unwrap();
    npm
}

/// A PATH with `npm` first, then the folder of the `node` this test run has.
pub fn path(npm: &Path) -> OsString {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let node = otaq_core::tools::find_on_path("node", &inherited)
        .expect("Node on the PATH (tests need Node)");
    std::env::join_paths([npm, node.parent().unwrap()]).unwrap()
}

/// The arguments the script wrote to `file`.
pub fn arguments(file: &Path) -> Vec<String> {
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

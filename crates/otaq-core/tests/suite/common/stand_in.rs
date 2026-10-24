//! The stand-in agent tool (`tests/fixtures/stand-in/agent-tool.sh`, T-0487, d0118): it reads its skill folders only
//! at start, runs a skill it loaded (`RUNS otaq-restack`) and answers any other with `Unknown command: /otaq-x`, so
//! a hand-off typed before the tool could load its skill shows. See the fixture's README.

use std::path::{Path, PathBuf};

use otaq_core::agent_tools::claude::{ClaudeLaunch, find_plugin_dir};
use otaq_core::agent_tools::{LaunchContext, Tool};

/// The script.
pub fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stand-in/agent-tool.sh")
}

/// The four tools' names in `bin`, each a link to [`script`]; returns `bin/claude`.
pub fn install(bin: &Path) -> PathBuf {
    std::fs::create_dir_all(bin).unwrap();
    for tool in [Tool::Claude, Tool::Codex, Tool::Copilot, Tool::OpenCode] {
        let link = bin.join(tool.command_name());
        if !link.exists() {
            std::os::unix::fs::symlink(script(), &link).unwrap();
        }
    }
    bin.join(Tool::Claude.command_name())
}

/// A plugin folder as the installed app has it, under `root`: Claude Code's and Copilot's hooks plugins (their
/// manifests) and the repository's shipped skills.
pub fn plugin(root: &Path) -> PathBuf {
    let plugin = root.join("plugin");
    for tool in ["claude", "copilot"] {
        let manifest = plugin.join(tool).join(".claude-plugin");
        std::fs::create_dir_all(&manifest).unwrap();
        std::fs::write(manifest.join("plugin.json"), "{}").unwrap();
    }
    let repo = find_plugin_dir(Path::new("/nonexistent")).expect("the repository's plugin");
    copy_dir(&repo.join("skills"), &plugin.join("skills"));
    plugin
}

/// The service's launch: the four tools are the stand-in in `bin` ([`install`]), the plugin is [`plugin`]'s.
pub fn launch(bin: &Path, root: &Path) -> LaunchContext {
    let claude = install(bin);
    LaunchContext {
        bin_dir: bin.to_owned(),
        plugin_dir: plugin(root),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: vec![claude.display().to_string()],
            config_dir: None,
        },
    }
}

/// Copies the folder `from` into `to`, files and modes.
pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}

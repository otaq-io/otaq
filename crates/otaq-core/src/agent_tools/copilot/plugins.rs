//! The skills of the plugins installed in GitHub Copilot, read only by the skill finder (S6).
//!
//! `copilot plugin install` keeps a plugin in Copilot's home (`$COPILOT_HOME`, else `~/.copilot`), under
//! `installed-plugins/<marketplace>/<plugin>/` (`_direct/<source>/` for one installed from a repository or a folder).
//! Its skills are its `skills/` folder, run as `/<plugin>:<skill>`; its name is its manifest's `name` (`plugin.json`,
//! `.github/plugin/plugin.json` or `.claude-plugin/plugin.json`), else its folder's.

use std::path::Path;

use crate::agent_tools::{PluginSkills, plugin_manifest_name};

/// Where a plugin's manifest may be, in its folder.
const MANIFESTS: &[&[&str]] = &[
    &["plugin.json"],
    &[".github", "plugin", "plugin.json"],
    &[".claude-plugin", "plugin.json"],
];

/// The skills folders of the plugins installed in Copilot, whose home is `home`.
pub fn plugin_skills(home: &Path) -> Vec<PluginSkills> {
    let mut found = Vec::new();
    for source in folders(&home.join("installed-plugins")) {
        for plugin in folders(&source) {
            let Some(folder_name) = plugin.file_name().map(|n| n.to_string_lossy().into_owned())
            else {
                continue;
            };
            found.push(PluginSkills {
                plugin: plugin_manifest_name(&plugin, MANIFESTS).unwrap_or(folder_name),
                folder: plugin.join("skills"),
            });
        }
    }
    found
}

/// The folders in `dir`, by name.
fn folders(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut found: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_plugins_by_manifest_name_else_folder() {
        let dir = tempfile::TempDir::new().unwrap();
        let installed = dir.path().join("installed-plugins");
        std::fs::create_dir_all(installed.join("team/tuq/skills")).unwrap();
        let direct = installed.join("_direct/owner--web");
        std::fs::create_dir_all(direct.join(".github/plugin")).unwrap();
        std::fs::write(
            direct.join(".github/plugin/plugin.json"),
            r#"{"name": "web"}"#,
        )
        .unwrap();
        let found = plugin_skills(dir.path());
        assert_eq!(
            found,
            [
                PluginSkills {
                    plugin: "web".into(),
                    folder: direct.join("skills"),
                },
                PluginSkills {
                    plugin: "tuq".into(),
                    folder: installed.join("team/tuq/skills"),
                },
            ]
        );
    }
}

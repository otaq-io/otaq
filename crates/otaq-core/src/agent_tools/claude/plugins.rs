//! The skills of the plugins installed in Claude Code, read only by the skill finder (S6).
//!
//! Claude Code lists its installed plugins in `<its folder>/plugins/installed_plugins.json` (`$CLAUDE_CONFIG_DIR`,
//! else `~/.claude`): `{"version": 2, "plugins": {"<plugin>@<marketplace>": [{"scope", "projectPath",
//! "installPath", …}]}}` (an older file has one object per plugin instead of a list). A plugin's skills are the
//! `skills/` folder of its `installPath`, run as `/<plugin>:<skill>`. One installed for a project (`project`,
//! `local`) counts only in that project's checkout; one the user's `settings.json` turns off (`enabledPlugins`
//! `false`) doesn't count.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::agent_tools::PluginSkills;

/// The skills folders of the plugins installed in Claude Code, whose own folder is `config`, for `checkout`.
pub fn plugin_skills(config: &Path, checkout: Option<&Path>) -> Vec<PluginSkills> {
    let Some(installed) = read_json(&config.join("plugins").join("installed_plugins.json")) else {
        return Vec::new();
    };
    let off: HashSet<String> = read_json(&config.join("settings.json"))
        .and_then(|settings| {
            settings["enabledPlugins"].as_object().map(|enabled| {
                enabled
                    .iter()
                    .filter(|(_, on)| on.as_bool() == Some(false))
                    .map(|(id, _)| id.clone())
                    .collect()
            })
        })
        .unwrap_or_default();
    let Some(plugins) = installed["plugins"].as_object() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (id, entries) in plugins {
        if off.contains(id) {
            continue;
        }
        let plugin = id.split('@').next().unwrap_or(id);
        if plugin.is_empty() {
            continue;
        }
        let entries: Vec<&Value> = match entries {
            Value::Array(list) => list.iter().collect(),
            other => vec![other],
        };
        let here = entries.into_iter().find(|entry| applies(entry, checkout));
        if let Some(path) = here.and_then(|entry| entry["installPath"].as_str()) {
            found.push(PluginSkills {
                plugin: plugin.to_owned(),
                folder: PathBuf::from(path).join("skills"),
            });
        }
    }
    found
}

/// Whether an install counts in `checkout`: one for the user (or with no scope) everywhere, one for a project only
/// in that project's checkout.
fn applies(entry: &Value, checkout: Option<&Path>) -> bool {
    match entry["scope"].as_str() {
        Some("project" | "local") => {
            let (Some(project), Some(checkout)) = (entry["projectPath"].as_str(), checkout) else {
                return false;
            };
            same_folder(Path::new(project), checkout)
        }
        _ => true,
    }
}

fn same_folder(a: &Path, b: &Path) -> bool {
    a == b
        || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

fn read_json(file: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(file).ok()?;
    serde_json::from_str(&text)
        .inspect_err(|e| tracing::debug!(file = %file.display(), error = %e, "a Claude Code file doesn't read"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn user_and_project_installs_and_turned_off_ones() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = dir.path().join(".claude");
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let json = serde_json::json!({
            "version": 2,
            "plugins": {
                "tuq@team": [{"scope": "user", "installPath": dir.path().join("c/tuq").display().to_string()}],
                "web@team": [{"scope": "project", "projectPath": repo.display().to_string(),
                              "installPath": dir.path().join("c/web").display().to_string()}],
                "other@team": [{"scope": "local", "projectPath": "/elsewhere",
                                "installPath": dir.path().join("c/other").display().to_string()}],
                "off@team": [{"scope": "user", "installPath": dir.path().join("c/off").display().to_string()}],
                "old@team": {"installPath": dir.path().join("c/old").display().to_string()}
            }
        });
        write(
            &config.join("plugins/installed_plugins.json"),
            &json.to_string(),
        );
        write(
            &config.join("settings.json"),
            r#"{"enabledPlugins": {"off@team": false, "tuq@team": true}}"#,
        );
        let names = |checkout: Option<&Path>| -> Vec<String> {
            let mut names: Vec<String> = plugin_skills(&config, checkout)
                .into_iter()
                .map(|p| p.plugin)
                .collect();
            names.sort();
            names
        };
        assert_eq!(names(Some(&repo)), ["old", "tuq", "web"]);
        assert_eq!(names(None), ["old", "tuq"]);
        let tuq = plugin_skills(&config, None)
            .into_iter()
            .find(|p| p.plugin == "tuq")
            .unwrap();
        assert_eq!(tuq.folder, dir.path().join("c/tuq/skills"));
        assert!(plugin_skills(&dir.path().join("nothing"), None).is_empty());
    }
}

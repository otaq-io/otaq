//! The skills of the plugins installed in Codex, read only by the skill finder (S6).
//!
//! `codex plugin add <plugin>@<marketplace>` writes `[plugins."<plugin>@<marketplace>"]` (with `enabled = true`) to
//! `config.toml` in Codex's home (`$CODEX_HOME`, else `~/.codex`) and runs the plugin from its installed copy,
//! `plugins/cache/<marketplace>/<plugin>/<version>/` there, whose root `skills/` folder holds its skills (see
//! [`super::setup`]). A plugin set `enabled = false` doesn't count; of several version folders, the one written last
//! is the one in use.

use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item};

use crate::agent_tools::PluginSkills;

/// The skills folders of the plugins installed in Codex, whose home is `home`.
pub fn plugin_skills(home: &Path) -> Vec<PluginSkills> {
    let Ok(text) = std::fs::read_to_string(home.join("config.toml")) else {
        return Vec::new();
    };
    let doc: DocumentMut = match text.parse() {
        Ok(doc) => doc,
        Err(e) => {
            tracing::debug!(error = %e, "Codex's config.toml doesn't read: no plugin skills");
            return Vec::new();
        }
    };
    let Some(plugins) = doc.get("plugins").and_then(Item::as_table_like) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (id, plugin) in plugins.iter() {
        if plugin.get("enabled").and_then(Item::as_bool) == Some(false) {
            continue;
        }
        let Some((name, marketplace)) = id.split_once('@') else {
            continue;
        };
        let root = home
            .join("plugins")
            .join("cache")
            .join(marketplace)
            .join(name);
        if let Some(copy) = newest_folder(&root) {
            found.push(PluginSkills {
                plugin: name.to_owned(),
                folder: copy.join("skills"),
            });
        }
    }
    found
}

/// The folder in `root` written last.
fn newest_folder(root: &Path) -> Option<PathBuf> {
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .max_by_key(|entry| {
            entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        })
        .map(|entry| entry.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_and_enabled_plugins_count() {
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path();
        std::fs::write(
            home.join("config.toml"),
            "model = \"x\"\n\n[plugins.\"tuq@team\"]\nenabled = true\n\n[plugins.\"off@team\"]\nenabled = false\n\n[plugins.\"gone@team\"]\nenabled = true\n",
        )
        .unwrap();
        for name in ["tuq", "off"] {
            std::fs::create_dir_all(home.join(format!("plugins/cache/team/{name}/1.0.0/skills")))
                .unwrap();
        }
        let found = plugin_skills(home);
        assert_eq!(
            found,
            [PluginSkills {
                plugin: "tuq".into(),
                folder: home.join("plugins/cache/team/tuq/1.0.0/skills"),
            }]
        );
        assert!(plugin_skills(&home.join("nothing")).is_empty());
    }
}

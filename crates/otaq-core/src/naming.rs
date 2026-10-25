//! The product's names, in one place (d0028, d0076).
//!
//! The product name spreads into the command, the plugin namespace, skill copies (`otaq-work-ticket`), branch names and
//! environment variables (`OTAQ_TICKET`). Every crate builds these strings from the constants below, so a
//! forced rename later means changing only this file. A test (`tests/suite/naming_literals.rs`) fails if another source
//! file spells one of them out.

/// The product name shown to people.
pub const PRODUCT_NAME: &str = "Otaq";

/// The command-line binary.
pub const COMMAND_NAME: &str = "otaq";

/// The plugin namespace: the name of Otaq's hooks plugin for each tool (`otaq@otaq-local` in Codex).
pub const PLUGIN_NAMESPACE: &str = "otaq";

/// The palette key of the bundled Otaq themes, Otaq Dark and Otaq Light, as the design has it and as
/// d0069's settings stored both (d0070 stores `otaq-dark` and `otaq-light`).
pub const THEME_KEY: &str = "otaq";

/// The stored names of Otaq Dark and Otaq Light (d0070).
pub const THEME_DARK: &str = "otaq-dark";
pub const THEME_LIGHT: &str = "otaq-light";

/// The prefix of every environment variable Otaq reads or sets.
pub const ENV_PREFIX: &str = "OTAQ_";

/// An environment variable name: `env_var("HOME")` → `OTAQ_HOME`.
pub fn env_var(suffix: &str) -> String {
    format!("{ENV_PREFIX}{suffix}")
}

/// The variable that tells an agent session which ticket it works on: `OTAQ_TICKET`.
pub fn ticket_env_var() -> String {
    env_var("TICKET")
}

/// The variable that says what an agent session is for when it isn't the ticket's agent: `OTAQ_ROLE`
/// (`validator`, `project_agent`). Hooks and the CLI read it (d0063, d0068).
pub fn role_env_var() -> String {
    env_var("ROLE")
}

/// [`role_env_var`]'s value in a validator session.
pub const ROLE_VALIDATOR: &str = "validator";
/// [`role_env_var`]'s value in a Project agent session.
pub const ROLE_PROJECT_AGENT: &str = "project_agent";
/// The ticket resource's database name, set where a project has ticket resources (d0087): `OTAQ_DB_NAME`.
pub fn db_name_env_var() -> String {
    env_var("DB_NAME")
}

/// The project's template database name (d0087): `OTAQ_DB_TEMPLATE`.
pub fn db_template_env_var() -> String {
    env_var("DB_TEMPLATE")
}

/// The ticket's own resource folder in the data folder (d0087): `OTAQ_RESOURCE_DIR`.
pub fn resource_dir_env_var() -> String {
    env_var("RESOURCE_DIR")
}

/// The data folder's folder of ticket resource folders: `res` (`res/<repo>/<KEY>`, d0087).
pub fn resource_dir_name() -> &'static str {
    "res"
}

/// The first part of every database name Otaq computes: `otaq` (`otaq_shop_shop_18`, d0087).
pub fn db_prefix() -> &'static str {
    COMMAND_NAME
}

/// The longest name a skill may have (the skill format's limit).
pub const SKILL_NAME_MAX: usize = 64;

/// The name of Otaq's copy of a skill (AGM-07, d0098): its folder in `.agents/skills/` and `.claude/skills/` and its
/// SKILL.md's `name:`, the same for shipped and imported skills on every tool. `otaq-` and the skill's own name,
/// lower case, every character but `a-z`, `0-9` and `-` made a `-` (no two in a row, none at the end), cut to
/// [`SKILL_NAME_MAX`] characters: `copy_name("commit-compact")` → `otaq-commit-compact`,
/// `copy_name("frontend-design:frontend-design")` → `otaq-frontend-design-frontend-design`.
pub fn copy_name(skill: &str) -> String {
    let mut name = format!("{COMMAND_NAME}-");
    for c in skill.to_lowercase().chars() {
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() {
            c
        } else {
            '-'
        };
        if !(c == '-' && name.ends_with('-')) {
            name.push(c);
        }
    }
    name.truncate(SKILL_NAME_MAX);
    name.trim_end_matches('-').to_owned()
}

/// The prefix of branches Otaq creates: `otaq/`.
pub fn branch_prefix() -> String {
    format!("{COMMAND_NAME}/")
}

/// The data folder's name in the user's home folder: `.otaq` (d0055).
pub fn data_dir_name() -> String {
    format!(".{COMMAND_NAME}")
}

/// The desktop app's binary, installed next to the command: `otaq-app`.
pub fn app_binary_name() -> String {
    format!("{COMMAND_NAME}-app{}", std::env::consts::EXE_SUFFIX)
}

/// The macOS bundle's ID (d0072), under which macOS shows its notifications (d0090).
pub const BUNDLE_ID: &str = "io.otaq.desktop";

/// The macOS bundle's folder name: `Otaq.app` (d0072).
pub fn bundle_folder_name() -> String {
    format!("{PRODUCT_NAME}.app")
}

/// The default flow's file (STG-03, d0108), shipped embedded; its contents are [`DEFAULT_FLOW_YAML`].
pub const DEFAULT_FLOW_FILE: &str = "otaq-default.yaml";
/// The contents of [`DEFAULT_FLOW_FILE`], read by [`crate::flow::default`].
pub const DEFAULT_FLOW_YAML: &str = include_str!("../flows/otaq-default.yaml");

/// Otaq's agents file (AGM-02, d0108), shipped embedded; its contents are [`AGENTS_YAML`].
pub const AGENTS_FILE: &str = "otaq-agents.yaml";
/// The contents of [`AGENTS_FILE`], read by [`crate::agents::shipped`].
pub const AGENTS_YAML: &str = include_str!("../flows/otaq-agents.yaml");

/// A branch Otaq names: `branch("SHOP-40")` → `otaq/SHOP-40`.
pub fn branch(name: &str) -> String {
    format!("{}{name}", branch_prefix())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants() {
        assert_eq!(PRODUCT_NAME, "Otaq");
        assert_eq!(COMMAND_NAME, "otaq");
        assert_eq!(PLUGIN_NAMESPACE, "otaq");
        assert_eq!(ENV_PREFIX, "OTAQ_");
    }

    #[test]
    fn env_var_adds_the_prefix() {
        assert_eq!(env_var("HOME"), "OTAQ_HOME");
    }

    #[test]
    fn ticket_env_var_name() {
        assert_eq!(ticket_env_var(), "OTAQ_TICKET");
        assert_eq!(role_env_var(), "OTAQ_ROLE");
    }

    #[test]
    fn copy_names() {
        assert_eq!(copy_name("commit-compact"), "otaq-commit-compact");
        assert_eq!(copy_name("work-ticket"), "otaq-work-ticket");
        assert_eq!(
            copy_name("frontend-design:frontend-design"),
            "otaq-frontend-design-frontend-design"
        );
        assert_eq!(copy_name("My PR  skill!"), "otaq-my-pr-skill");
        let long = copy_name(&"a".repeat(70));
        assert_eq!(long.len(), SKILL_NAME_MAX);
        assert_eq!(long, format!("otaq-{}", "a".repeat(59)));
        assert_eq!(
            copy_name(&format!("{}-b", "a".repeat(58))),
            format!("otaq-{}", "a".repeat(58))
        );
    }

    #[test]
    fn branch_names() {
        assert_eq!(branch_prefix(), "otaq/");
        assert_eq!(branch("SHOP-40"), "otaq/SHOP-40");
    }

    #[test]
    fn app_binary() {
        assert!(app_binary_name().starts_with("otaq-app"));
    }

    #[test]
    fn resource_names() {
        assert_eq!(db_name_env_var(), "OTAQ_DB_NAME");
        assert_eq!(db_template_env_var(), "OTAQ_DB_TEMPLATE");
        assert_eq!(resource_dir_env_var(), "OTAQ_RESOURCE_DIR");
        assert_eq!(resource_dir_name(), "res");
        assert_eq!(db_prefix(), "otaq");
    }

    #[test]
    fn data_dir_name_is_hidden() {
        assert_eq!(data_dir_name(), ".otaq");
    }
}

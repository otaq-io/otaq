//! A plugin's `plugin.yaml` (the otaq-marketplace README, "Writing a plugin"): read, then checked before anything runs.
//!
//! Protocol 1 is declarative only: a command is an argv template (`run:`) and a mapping of the CLI's JSON output
//! (`map:`). `script:` is reserved for a later protocol and refused here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::path::JsonPath;
use super::template::{Template, TemplateScope};
use crate::naming::{COMMAND_NAME, PRODUCT_NAME};

/// The plugin format this Otaq reads.
pub const PROTOCOL: u32 = 1;

/// The setting holding the saved search: the import dialog starts with it, and auto-pull runs it.
pub const PULL_QUERY: &str = "query";

/// The longest a plugin's id may be.
const MAX_ID_LEN: usize = 40;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub protocol: u32,
    /// The Otaq versions it works with, a semver range: `">=0.1.0, <2.0.0"`.
    pub otaq: String,
    #[serde(default)]
    pub description: String,
    /// An SPDX id ([`super::open_source`]).
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub maintainers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// The extension points it fills; protocol 1 knows `ticket_source` only.
    pub provides: Vec<String>,
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub requires: Vec<Requirement>,
    #[serde(default)]
    pub settings: Vec<SettingSpec>,
    #[serde(default)]
    pub types: Types,
    #[serde(default)]
    pub states: States,
    /// What the import dialog's search box says: `JQL`, `GitHub search`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_label: Option<String>,
    /// The issue's web page, when the CLI's output has none: `https://github.com/{settings.repo}/issues/{id}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_url: Option<String>,
    pub commands: Commands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Import,
    Refresh,
    AutoPull,
}

/// A CLI the plugin runs. Otaq checks it is installed (and logged in) before every command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Requirement {
    /// The program, as found on `PATH`: `gh`, `acli`, `az`.
    pub id: String,
    pub name: String,
    /// Runs when installed: exit 0 means it is.
    pub check: Vec<String>,
    /// Exit 0 means logged in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_check: Option<Vec<String>>,
    /// What **Log in** runs in Otaq's terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<Vec<String>>,
    #[serde(default)]
    pub install: InstallHints,
    /// A part the CLI needs added (`az`'s `azure-devops`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<Extension>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallHints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<String>,
}

impl InstallHints {
    /// The hint for this OS.
    pub fn here(&self) -> Option<&str> {
        let hint = if cfg!(windows) {
            &self.windows
        } else if cfg!(target_os = "macos") {
            &self.macos
        } else {
            &self.linux
        };
        hint.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extension {
    pub name: String,
    /// Exit 0 means it is added.
    pub check: Vec<String>,
    /// What **Install** runs in Otaq's terminal.
    pub install: Vec<String>,
}

/// One field of the plugin's settings form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingSpec {
    pub key: String,
    pub label: String,
    #[serde(rename = "type", default)]
    pub kind: SettingKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// What a string value may look like ([`Format`]).
    #[serde(default)]
    pub format: Format,
    /// Where the value is set: a project only, or also as a global default.
    #[serde(default)]
    pub scope: SettingScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    #[default]
    String,
    /// Names to values: a priority mapping.
    Map,
}

/// The shapes a string setting may have. Every value is one argument and never starts with `-`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// Any one line.
    #[default]
    Text,
    /// Letters, digits, `-`, `_` and `.`: a project key.
    Slug,
    /// `owner/name`.
    OwnerRepo,
    /// A host name: `acme.atlassian.net`.
    Host,
    /// An `https://` address: `https://dev.azure.com/acme`.
    Url,
}

impl Format {
    /// Whether `value` fits.
    pub fn accepts(self, value: &str) -> bool {
        let slug = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if value.starts_with('-') || value.chars().any(char::is_control) {
            return false;
        }
        match self {
            Self::Text => true,
            Self::Slug => slug(value),
            Self::OwnerRepo => value
                .split_once('/')
                .is_some_and(|(owner, name)| slug(owner) && slug(name)),
            Self::Host => slug(value) && value.contains('.'),
            Self::Url => value
                .strip_prefix("https://")
                .is_some_and(|rest| !rest.is_empty() && !rest.contains(char::is_whitespace)),
        }
    }

    /// What the form says when a value doesn't fit.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Text => "one line, not starting with -",
            Self::Slug => "letters, digits, -, _ and .",
            Self::OwnerRepo => "owner/name",
            Self::Host => "a host name, like acme.atlassian.net",
            Self::Url => "an https:// address",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingScope {
    /// A global default, which a project may override.
    #[default]
    Any,
    /// Only a project sets it (a repository).
    Project,
}

/// Which tracker types are work items. Matched ignoring case.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Types {
    /// The category of a type not listed (and of an issue with no type).
    #[serde(default)]
    pub default: TypeCategory,
    #[serde(default)]
    pub containers: Vec<String>,
    #[serde(default)]
    pub work: Vec<String>,
}

impl Types {
    pub fn category(&self, kind: Option<&str>) -> TypeCategory {
        let listed = |names: &[String]| {
            kind.is_some_and(|k| names.iter().any(|n| n.eq_ignore_ascii_case(k)))
        };
        if listed(&self.containers) {
            TypeCategory::Container
        } else if listed(&self.work) {
            TypeCategory::Work
        } else {
            self.default
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeCategory {
    #[default]
    Work,
    Container,
}

/// Which tracker states count as finished, when the mapping gives no `state_category`. Matched ignoring case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct States {
    #[serde(default)]
    pub done: Vec<String>,
}

impl Default for States {
    fn default() -> Self {
        Self {
            done: ["closed", "done", "resolved", "removed"]
                .map(str::to_owned)
                .to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Commands {
    pub search: Command,
    pub fetch: Command,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Command {
    #[serde(default)]
    pub run: Vec<String>,
    #[serde(default)]
    pub map: Mapping,
    /// Reserved for a later protocol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<Value>,
}

/// How a CLI's JSON output becomes issues.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    /// The list of issues (`search`); `$` for the whole output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<String>,
    /// The one issue (`fetch`); `$` for the whole output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    pub id: Field,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<Field>,
    pub title: Field,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Field>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<Field>,
    /// `open` or `done`; without it, [`States`] decides from `state`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_category: Option<Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_url: Option<Field>,
    /// `urgent`, `high`, `normal`, `low` or 1–4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Field>,
}

impl Default for Field {
    fn default() -> Self {
        Self::Path(String::new())
    }
}

/// One field of the mapping: a path (`fields.summary`), a template over the item (`"#{number}"`), or a path with a
/// transform or a lookup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Field {
    Path(String),
    Rule(FieldRule),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldRule {
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    /// A lookup: an inline map, or `settings.<key>` naming a map setting. The first value found wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<Via>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Via {
    Setting(String),
    Map(BTreeMap<String, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transform {
    /// Jira's document format.
    AdfToMarkdown,
    /// Azure DevOps' descriptions.
    HtmlToMarkdown,
}

/// Why a manifest can't be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError(pub String);

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ManifestError {}

fn bad(message: impl Into<String>) -> ManifestError {
    ManifestError(message.into())
}

/// Reads `plugin.yaml` and checks it ([`Manifest::check`]).
pub fn read(text: &str) -> Result<Manifest, ManifestError> {
    let manifest: Manifest =
        serde_saphyr::from_str(text).map_err(|e| bad(format!("plugin.yaml can't be read: {e}")))?;
    manifest.check()?;
    Ok(manifest)
}

/// Whether `id` can name a plugin: lowercase letters, digits and `-`, starting with a letter.
pub fn valid_id(id: &str) -> bool {
    id.len() <= MAX_ID_LEN
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl Manifest {
    /// Every rule a plugin must keep before Otaq loads it. The open-source rules are [`super::open_source`]'s.
    pub fn check(&self) -> Result<(), ManifestError> {
        if !valid_id(&self.id) {
            return Err(bad(format!(
                "\"{}\" isn't a plugin id: lowercase letters, digits and -, starting with a letter",
                self.id
            )));
        }
        if self.name.trim().is_empty() {
            return Err(bad("The plugin has no name"));
        }
        self.semver()?;
        semver::VersionReq::parse(&self.otaq).map_err(|e| {
            bad(format!(
                "{COMMAND_NAME}: \"{}\" isn't a version range: {e}",
                self.otaq
            ))
        })?;
        if self.protocol == 0 || self.protocol > PROTOCOL {
            return Err(bad(format!(
                "{} needs plugin protocol {}; this {PRODUCT_NAME} reads protocol {PROTOCOL}",
                self.name, self.protocol
            )));
        }
        if !self.provides.iter().any(|p| p == "ticket_source") {
            return Err(bad(format!(
                "provides: this {PRODUCT_NAME} runs ticket_source plugins only"
            )));
        }
        if !self.capabilities.contains(&Capability::Import) {
            return Err(bad("A ticket source has the import capability"));
        }
        let mut keys: Vec<&str> = Vec::new();
        for setting in &self.settings {
            if !valid_key(&setting.key) || keys.contains(&setting.key.as_str()) {
                return Err(bad(format!(
                    "settings: \"{}\" is a bad or repeated key",
                    setting.key
                )));
            }
            keys.push(&setting.key);
        }
        for requirement in &self.requires {
            if requirement.id.is_empty() || requirement.check.is_empty() {
                return Err(bad(format!(
                    "requires: \"{}\" needs an id and a check",
                    requirement.id
                )));
            }
        }
        for (name, command, scope) in [
            ("search", &self.commands.search, TemplateScope::Search),
            ("fetch", &self.commands.fetch, TemplateScope::Fetch),
        ] {
            self.check_command(name, command, scope)?;
        }
        if self.has(Capability::AutoPull) && self.setting(PULL_QUERY).is_none() {
            return Err(bad(format!(
                "auto_pull needs a \"{PULL_QUERY}\" setting: the search it runs"
            )));
        }
        if self.commands.search.map.items.is_none() {
            return Err(bad("commands.search.map needs items"));
        }
        if let Some(url) = &self.issue_url {
            Template::parse(url, TemplateScope::IssueUrl, &keys)
                .map_err(|e| bad(format!("issue_url: {e}")))?;
        }
        Ok(())
    }

    fn check_command(
        &self,
        name: &str,
        command: &Command,
        scope: TemplateScope,
    ) -> Result<(), ManifestError> {
        if command.script.is_some() {
            return Err(bad(format!(
                "commands.{name}: script commands come in a later plugin protocol; use run and map"
            )));
        }
        let Some(program) = command.run.first() else {
            return Err(bad(format!("commands.{name} has no run")));
        };
        if program.contains('{') {
            return Err(bad(format!(
                "commands.{name}.run: the program can't be a placeholder"
            )));
        }
        if !self.requires.iter().any(|r| &r.id == program) {
            return Err(bad(format!(
                "commands.{name} runs {program}, which isn't in requires"
            )));
        }
        let keys: Vec<&str> = self.settings.iter().map(|s| s.key.as_str()).collect();
        for arg in &command.run {
            Template::parse(arg, scope, &keys)
                .map_err(|e| bad(format!("commands.{name}.run: {e}")))?;
        }
        let map = &command.map;
        for path in [&map.items, &map.item].into_iter().flatten() {
            JsonPath::parse(path).map_err(|e| bad(format!("commands.{name}.map: {e}")))?;
        }
        for field in map.fields() {
            let from = match field {
                Field::Path(p) => p,
                Field::Rule(rule) => {
                    if let Some(Via::Setting(s)) = &rule.via {
                        let key = s.strip_prefix("settings.").unwrap_or(s);
                        let map_setting = self
                            .settings
                            .iter()
                            .any(|spec| spec.key == key && spec.kind == SettingKind::Map);
                        if !map_setting {
                            return Err(bad(format!(
                                "commands.{name}.map: via {s} names no map setting"
                            )));
                        }
                    }
                    &rule.from
                }
            };
            if from.contains('{') {
                Template::parse(from, TemplateScope::Item, &keys)
                    .map_err(|e| bad(format!("commands.{name}.map: {e}")))?;
            } else {
                JsonPath::parse(from).map_err(|e| bad(format!("commands.{name}.map: {e}")))?;
            }
        }
        Ok(())
    }

    pub fn semver(&self) -> Result<semver::Version, ManifestError> {
        semver::Version::parse(&self.version)
            .map_err(|e| bad(format!("version: \"{}\" isn't semver: {e}", self.version)))
    }

    /// Whether this Otaq is in the plugin's `otaq` range.
    pub fn fits(&self, otaq: &semver::Version) -> bool {
        fits(&self.otaq, self.protocol, otaq)
    }

    pub fn has(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub fn setting(&self, key: &str) -> Option<&SettingSpec> {
        self.settings.iter().find(|s| s.key == key)
    }

    /// Every command line the plugin can run, as the consent screen lists them.
    pub fn command_lines(&self) -> Vec<Vec<String>> {
        let mut lines = vec![
            self.commands.search.run.clone(),
            self.commands.fetch.run.clone(),
        ];
        for requirement in &self.requires {
            lines.push(requirement.check.clone());
            lines.extend(requirement.login_check.clone());
            lines.extend(requirement.login.clone());
            if let Some(extension) = &requirement.extension {
                lines.push(extension.check.clone());
                lines.push(extension.install.clone());
            }
        }
        lines
    }
}

/// Whether a plugin with this `otaq` range and protocol runs on `otaq`.
pub fn fits(range: &str, protocol: u32, otaq: &semver::Version) -> bool {
    protocol >= 1
        && protocol <= PROTOCOL
        && semver::VersionReq::parse(range).is_ok_and(|req| req.matches(otaq))
}

/// This Otaq's version.
pub fn otaq_version() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is semver")
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.starts_with(|c: char| c.is_ascii_lowercase())
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl Mapping {
    fn fields(&self) -> impl Iterator<Item = &Field> {
        [Some(&self.id), Some(&self.title)]
            .into_iter()
            .chain([
                self.key.as_ref(),
                self.url.as_ref(),
                self.body.as_ref(),
                self.kind.as_ref(),
                self.state.as_ref(),
                self.state_category.as_ref(),
                self.parent.as_ref(),
                self.parent_url.as_ref(),
                self.priority.as_ref(),
                self.updated_at.as_ref(),
            ])
            .flatten()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The test manifest; its Otaq-range key is spelled from the naming module.
    const GITHUB_TEMPLATE: &str = r##"
id: github-issues
name: GitHub Issues
version: 1.0.0
protocol: 1
RANGE_KEY: ">=0.1.0, <2.0.0"
license: MIT
provides: [ticket_source]
capabilities: [import, refresh, auto_pull]
requires:
  - id: gh
    name: GitHub CLI
    check: [gh, --version]
    login_check: [gh, auth, status]
    login: [gh, auth, login]
    install: { macos: brew install gh }
settings:
  - { key: repo, label: Repository, required: true, format: owner_repo, scope: project }
  - { key: query, label: Auto-pull search, default: "is:open" }
  - { key: priority_labels, label: Priority labels, type: map, default: { P1: urgent } }
issue_url: "https://github.com/{settings.repo}/issues/{id}"
commands:
  search:
    run: [gh, issue, list, -R, "{settings.repo}", --search, "{query}", --limit, "{limit}", --json, "number,title,url,state,labels"]
    map:
      items: "$"
      id: number
      key: "#{number}"
      url: url
      title: title
      state: state
      priority: { from: "labels[].name", via: settings.priority_labels }
  fetch:
    run: [gh, issue, view, "{id}", -R, "{settings.repo}", --json, "number,title,body,url,state,labels"]
    map:
      item: "$"
      id: number
      key: "#{number}"
      url: url
      title: title
      body: body
      state: state
      priority: { from: "labels[].name", via: settings.priority_labels }
"##;

    pub(crate) fn github() -> String {
        GITHUB_TEMPLATE.replace("RANGE_KEY", crate::naming::COMMAND_NAME)
    }

    #[test]
    fn reads_a_declarative_plugin() {
        let m = read(&github()).unwrap();
        assert_eq!(m.id, "github-issues");
        assert!(m.has(Capability::AutoPull));
        assert!(m.fits(&semver::Version::new(0, 1, 0)));
        assert!(!m.fits(&semver::Version::new(2, 0, 0)));
        assert_eq!(m.setting("repo").unwrap().format, Format::OwnerRepo);
        assert!(matches!(
            m.commands.search.map.priority,
            Some(Field::Rule(_))
        ));
        assert_eq!(m.command_lines()[0][0], "gh");
    }

    fn refused(edit: impl Fn(&str) -> String) -> String {
        read(&edit(&github())).unwrap_err().0
    }

    #[test]
    fn refuses_what_protocol_1_doesnt_allow() {
        assert!(refused(|t| t.replace("id: github-issues", "id: GitHub")).contains("plugin id"));
        assert!(refused(|t| t.replace("protocol: 1", "protocol: 2")).contains("protocol 2"));
        assert!(refused(|t| t.replace("version: 1.0.0", "version: one")).contains("semver"));
        assert!(
            refused(|t| t.replace("[gh, issue, list", "[curl, issue, list")).contains("requires")
        );
        assert!(refused(|t| t.replace("\"{query}\"", "\"{nope}\"")).contains("nope"));
        assert!(
            refused(|t| t.replace("[gh, issue, view", "[\"{id}\", issue, view"))
                .contains("program")
        );
        assert!(
            refused(|t| t.replace(
                "settings.priority_labels }\n  fetch",
                "settings.repo }\n  fetch"
            ))
            .contains("map setting")
        );
        let script = github().replace(
            "  fetch:\n    run:",
            "  fetch:\n    script: { program: node }\n    run:",
        );
        assert!(
            read(&script)
                .unwrap_err()
                .0
                .contains("later plugin protocol")
        );
        assert!(
            refused(|t| t.replace("provides: [ticket_source]", "provides: [notify]"))
                .contains("ticket_source")
        );
    }

    #[test]
    fn formats_keep_values_to_one_safe_argument() {
        assert!(Format::OwnerRepo.accepts("acme/shop"));
        assert!(!Format::OwnerRepo.accepts("acme"));
        assert!(!Format::Text.accepts("--upload-pack=x"));
        assert!(!Format::Text.accepts("a\nb"));
        assert!(Format::Host.accepts("acme.atlassian.net"));
        assert!(!Format::Host.accepts("acme"));
        assert!(Format::Url.accepts("https://dev.azure.com/acme"));
        assert!(!Format::Url.accepts("http://dev.azure.com/acme"));
        assert!(Format::Slug.accepts("PAY"));
    }

    #[test]
    fn types_and_states_ignore_case() {
        let types = Types {
            default: TypeCategory::Work,
            containers: vec!["Epic".into(), "Feature".into()],
            work: vec![],
        };
        assert_eq!(types.category(Some("epic")), TypeCategory::Container);
        assert_eq!(types.category(Some("Story")), TypeCategory::Work);
        assert_eq!(types.category(None), TypeCategory::Work);
    }
}

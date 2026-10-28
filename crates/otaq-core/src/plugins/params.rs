//! The `plugins.*` requests' parameters (the otaq-marketplace README); the requests are in [`crate::protocol::Request`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::registry::Pick;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginIdParams {
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceAddParams {
    pub url: String,
}

/// A marketplace plugin's consent screen; no `version` for the newest compatible one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentParams {
    pub marketplace: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// An install or update of what the consent screen showed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallParams {
    pub marketplace: String,
    pub id: String,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalFolderParams {
    pub folder: String,
    /// The consent screen's digest; absent for the consent screen itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DefaultsParams {
    pub id: String,
    #[serde(default)]
    pub values: Map<String, Value>,
}

/// **Log in** (or **Add extension**) for one of a plugin's CLIs, in Otaq's terminal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginTerminalParams {
    pub id: String,
    pub requirement: String,
    #[serde(default)]
    pub extension: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectPluginSetParams {
    pub project: String,
    pub plugin: String,
    pub enabled: bool,
    #[serde(default)]
    pub values: Map<String, Value>,
    #[serde(default)]
    pub auto_pull: bool,
    #[serde(default = "fifteen")]
    pub pull_minutes: i64,
}

fn fifteen() -> i64 {
    15
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPluginParams {
    pub project: String,
    pub plugin: String,
}

/// A search; no `query` for the saved one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchParams {
    pub project: String,
    pub plugin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportParams {
    pub project: String,
    pub plugin: String,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveParams {
    pub key: String,
    /// `{"title": "tracker", "body": "mine"}`.
    #[serde(default)]
    pub picks: BTreeMap<String, Pick>,
}

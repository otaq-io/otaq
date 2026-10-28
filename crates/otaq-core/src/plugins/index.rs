//! A marketplace's `plugins/index.json` (the otaq-marketplace README, "Layout"): written by the
//! marketplace's CI, listing every version of every plugin with its release asset and SHA256.
//!
//! Fetched only when useful (the otaq-marketplace README, "Installing and updating"): when Settings → Plugins opens and the
//! copy is over an hour old, and once a day while a marketplace plugin is installed. Nothing about the user is sent.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::manifest;

/// The official marketplace.
pub const OFFICIAL_URL: &str = crate::naming::MARKETPLACE_URL;
pub const OFFICIAL_LABEL: &str = crate::naming::MARKETPLACE_LABEL;
/// How old a copy may be when the Plugins page opens.
pub const PAGE_MAX_AGE: Duration = Duration::from_secs(60 * 60);
/// How old a copy may be for the daily check.
pub const DAILY: Duration = Duration::from_secs(24 * 60 * 60);
const READ_TIMEOUT: Duration = Duration::from_secs(20);
/// The largest release asset Otaq downloads.
const MAX_DOWNLOAD: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub schema: u32,
    pub kind: String,
    #[serde(default)]
    pub plugins: Vec<IndexPlugin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexPlugin {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub maintainers: Vec<String>,
    pub versions: Vec<IndexVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexVersion {
    pub version: String,
    pub protocol: u32,
    pub otaq: String,
    /// The release asset (a [`super::bundle::Bundle`]).
    pub url: String,
    pub sha256: String,
    /// `plugins/jira-v1.2.0`: the source at that tag, and the diff link between two.
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl Index {
    pub fn read(text: &str) -> Result<Self, String> {
        let index: Self = serde_json::from_str(text)
            .map_err(|e| format!("plugins/index.json can't be read: {e}"))?;
        if index.schema != 1 || index.kind != "plugins" {
            return Err(format!(
                "plugins/index.json is schema {} of {}; this {} reads schema 1 of plugins",
                index.schema,
                index.kind,
                crate::naming::PRODUCT_NAME
            ));
        }
        Ok(index)
    }

    pub fn plugin(&self, id: &str) -> Option<&IndexPlugin> {
        self.plugins.iter().find(|p| p.id == id)
    }
}

impl IndexPlugin {
    /// The newest version that runs on `otaq`; incompatible ones are hidden.
    pub fn newest_for(&self, otaq: &semver::Version) -> Option<&IndexVersion> {
        self.versions
            .iter()
            .filter(|v| manifest::fits(&v.otaq, v.protocol, otaq))
            .filter_map(|v| Some((semver::Version::parse(&v.version).ok()?, v)))
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, v)| v)
    }

    pub fn version(&self, version: &str) -> Option<&IndexVersion> {
        self.versions.iter().find(|v| v.version == version)
    }
}

/// `https://github.com/<owner>/<repo>` from what the user typed (also `owner/repo` and `.git` endings).
pub fn normalize_url(input: &str) -> Result<String, String> {
    let bad =
        || format!("\"{input}\" isn't a GitHub repository: use https://github.com/owner/repo");
    let trimmed = input.trim().trim_end_matches('/');
    let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let path = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("github.com/"))
        .unwrap_or(trimmed);
    let (owner, repo) = path.split_once('/').ok_or_else(bad)?;
    let part = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if !part(owner) || !part(repo) || repo.contains('/') {
        return Err(bad());
    }
    Ok(format!("https://github.com/{owner}/{repo}"))
}

/// Where a marketplace's index is read: its default branch's `plugins/index.json`.
pub fn index_url(marketplace: &str) -> String {
    match marketplace.strip_prefix("https://github.com/") {
        Some(path) => format!("https://raw.githubusercontent.com/{path}/HEAD/plugins/index.json"),
        // A test server, or any other host serving the same layout.
        None => format!("{}/plugins/index.json", marketplace.trim_end_matches('/')),
    }
}

/// The diff between two releases, for an update's consent screen.
pub fn compare_url(marketplace: &str, from_tag: &str, to_tag: &str) -> Option<String> {
    let base = marketplace.strip_prefix("https://github.com/")?;
    (!from_tag.is_empty() && !to_tag.is_empty())
        .then(|| format!("https://github.com/{base}/compare/{from_tag}...{to_tag}"))
}

/// A release's source, for the consent screen.
pub fn source_url(marketplace: &str, id: &str, tag: &str) -> Option<String> {
    let base = marketplace.strip_prefix("https://github.com/")?;
    let at = if tag.is_empty() { "HEAD" } else { tag };
    Some(format!("https://github.com/{base}/tree/{at}/plugins/{id}"))
}

/// What a fetch got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    New { text: String, etag: Option<String> },
    NotModified,
}

/// Reads a marketplace's index and downloads releases.
#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
}

impl Default for Http {
    fn default() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(format!(
                "{}/{}",
                crate::naming::COMMAND_NAME,
                env!("CARGO_PKG_VERSION")
            ))
            .timeout(READ_TIMEOUT)
            .build()
            .unwrap_or_default();
        Self { client }
    }
}

impl Http {
    pub async fn index(&self, marketplace: &str, etag: Option<&str>) -> Result<Fetched, String> {
        let mut request = self.client.get(index_url(marketplace));
        if let Some(etag) = etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let response = request.send().await.map_err(|e| e.to_string())?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(Fetched::NotModified);
        }
        if !status.is_success() {
            return Err(format!("the marketplace answered {status}"));
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let text = response.text().await.map_err(|e| e.to_string())?;
        Index::read(&text)?;
        Ok(Fetched::New { text, etag })
    }

    pub async fn download(&self, url: &str) -> Result<Vec<u8>, String> {
        if !url.starts_with("https://") && !url.starts_with("http://127.0.0.1") {
            return Err(format!("{url} isn't an https:// address"));
        }
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("the download answered {}", response.status()));
        }
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() > MAX_DOWNLOAD {
            return Err("the download is over 10 MB".to_owned());
        }
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX_TEMPLATE: &str = r#"{"schema": 1, "kind": "plugins", "plugins": [
        {"id": "jira", "name": "Jira", "versions": [
            {"version": "1.0.0", "protocol": 1, "RANGE_KEY": ">=0.1.0", "url": "https://x/1", "sha256": "a"},
            {"version": "1.2.0", "protocol": 1, "RANGE_KEY": ">=0.1.0, <0.2.0", "url": "https://x/2", "sha256": "b"},
            {"version": "2.0.0", "protocol": 1, "RANGE_KEY": ">=9.0.0", "url": "https://x/3", "sha256": "c"},
            {"version": "3.0.0", "protocol": 7, "RANGE_KEY": ">=0.1.0", "url": "https://x/4", "sha256": "d"}
        ]}
    ]}"#;

    #[test]
    fn the_newest_compatible_version_wins() {
        let text = INDEX_TEMPLATE.replace("RANGE_KEY", crate::naming::COMMAND_NAME);
        let index = Index::read(&text).unwrap();
        let jira = index.plugin("jira").unwrap();
        assert_eq!(
            jira.newest_for(&semver::Version::new(0, 1, 5))
                .unwrap()
                .version,
            "1.2.0"
        );
        assert_eq!(
            jira.newest_for(&semver::Version::new(0, 3, 0))
                .unwrap()
                .version,
            "1.0.0"
        );
        assert!(Index::read(r#"{"schema": 2, "kind": "plugins"}"#).is_err());
        assert!(Index::read(r#"{"schema": 1, "kind": "skills"}"#).is_err());
    }

    #[test]
    fn urls() {
        assert_eq!(
            normalize_url("acme/tracker-plugins").unwrap(),
            "https://github.com/acme/tracker-plugins"
        );
        assert_eq!(
            normalize_url("https://github.com/acme/x.git/").unwrap(),
            "https://github.com/acme/x"
        );
        assert!(normalize_url("https://gitlab.com/a/b").is_err());
        assert!(normalize_url("a/b/c").is_err());
        assert_eq!(
            index_url("https://github.com/acme/tracker-plugins"),
            "https://raw.githubusercontent.com/acme/tracker-plugins/HEAD/plugins/index.json"
        );
        assert_eq!(
            compare_url(
                "https://github.com/acme/tracker-plugins",
                "plugins/jira-v1.0.0",
                "plugins/jira-v1.2.0"
            )
            .unwrap(),
            "https://github.com/acme/tracker-plugins/compare/plugins/jira-v1.0.0...plugins/jira-v1.2.0"
        );
    }
}

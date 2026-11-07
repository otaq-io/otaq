//! PR provider detection (REV-11, REV-12; d0050, d0017). There is no provider setting: the provider follows the
//! `origin` remote's URL. `github.com` → GitHub (`gh`); `dev.azure.com` / `*.visualstudio.com` → Azure DevOps
//! (`az`). Any other remote is "not detected", and Open PR then works only with the user's own skill. Login status
//! comes from the tools check (`gh auth status`, `az account show`); Otaq stores no token.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::board::settings::WorkflowStep;
use crate::git::Git;
use crate::tools::{self, CheckEnv, Tool};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    GitHub,
    AzureDevOps,
}

impl Provider {
    /// "GitHub", "Azure DevOps".
    pub fn name(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::AzureDevOps => "Azure DevOps",
        }
    }

    /// The CLI the built-in skills use.
    pub fn tool(self) -> Tool {
        match self {
            Self::GitHub => Tool::Gh,
            Self::AzureDevOps => Tool::Az,
        }
    }

    /// The provider of a remote URL, in HTTPS or SSH form; `None` for any other host.
    pub fn detect(url: &str) -> Option<Self> {
        let host = remote_host(url)?.to_ascii_lowercase();
        if host == "github.com" || host.ends_with(".github.com") {
            Some(Self::GitHub)
        } else if host == "dev.azure.com"
            || host.ends_with(".dev.azure.com")
            || host == "visualstudio.com"
            || host.ends_with(".visualstudio.com")
        {
            Some(Self::AzureDevOps)
        } else {
            None
        }
    }
}

/// The host of a remote URL: `https://user@github.com/a/b.git`, `ssh://git@host:22/a/b`, `git@github.com:a/b.git`.
pub fn remote_host(url: &str) -> Option<&str> {
    let url = url.trim();
    let (rest, scheme) = match url.split_once("://") {
        Some((_, rest)) => (rest, true),
        None => (url, false),
    };
    // scp-like form (git@host:path) needs a colon before any slash.
    if !scheme
        && !rest
            .split('/')
            .next()
            .is_some_and(|first| first.contains(':'))
    {
        return None;
    }
    let authority = rest.split('/').next()?;
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host_port.split(':').next()?;
    (!host.is_empty()).then_some(host)
}

/// A remote URL as Settings shows it: without the scheme, user and `.git` — `git.acme.internal/web/handbook`.
pub fn display_remote(url: &str) -> String {
    let url = url.trim();
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = match rest.split_once('@') {
        Some((user, after)) if !user.contains('/') => after,
        _ => rest,
    };
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    rest.replacen(':', "/", 1).trim_end_matches('/').to_owned()
}

/// What Settings and Approve ▾ show about a project's PR provider (`project.pr_provider`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrProvider {
    /// The `origin` remote as Settings shows it; `None` without an `origin`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<Provider>,
    /// Whether the provider's CLI is logged in; `None` when not detected or the CLI is missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logged_in: Option<bool>,
    /// Whether the provider's CLI is on the PATH.
    #[serde(default)]
    pub cli_found: bool,
    /// The Open PR skill the project uses.
    pub open_pr_skill: String,
}

impl PrProvider {
    /// Whether the project uses Otaq's own Open PR skill (`open-pr`).
    pub fn built_in_open_pr(&self) -> bool {
        self.open_pr_skill == WorkflowStep::OpenPr.own_skill()
    }

    /// Why Open PR can't be offered: the built-in skill needs a detected provider.
    pub fn open_pr_blocked(&self) -> Option<&'static str> {
        (self.provider.is_none() && self.built_in_open_pr()).then_some("PR provider not detected")
    }

    /// "GitHub (detected from origin) · gh: logged in", or "Not detected: use your own Open PR skill".
    pub fn summary(&self) -> String {
        match self.provider {
            None => "Not detected: use your own Open PR skill".to_owned(),
            Some(p) => {
                let cli = p.tool().command();
                let login = match (self.cli_found, self.logged_in) {
                    (false, _) => format!("{cli}: not installed"),
                    (true, Some(true)) => format!("{cli}: signed in"),
                    (true, Some(false)) => format!("{cli}: not signed in"),
                    (true, None) => format!("{cli}: login unknown"),
                };
                format!("{} (detected from origin) \u{b7} {login}", p.name())
            }
        }
    }
}

/// The `origin` remote's URL, if the repository has one.
pub async fn origin_url(git: &Git, repo: &Path) -> Option<String> {
    git.run(repo, &["remote", "get-url", "origin"])
        .await
        .ok()
        .map(|u| u.trim().to_owned())
        .filter(|u| !u.is_empty())
}

/// Detects the provider of `repo` and checks its CLI's login (only reads: `remote get-url`, `auth status`).
pub async fn detect(git: &Git, repo: &Path, open_pr_skill: String, env: &CheckEnv) -> PrProvider {
    let mut found = detect_from_remote(git, repo, open_pr_skill).await;
    if let Some(p) = found.provider {
        let status = tools::check_one(p.tool(), env).await;
        found.cli_found = status.found;
        found.logged_in = status.logged_in;
    }
    found
}

/// Like [`detect`], from the remote URL only: runs no provider CLI (the login stays unknown).
pub async fn detect_from_remote(git: &Git, repo: &Path, open_pr_skill: String) -> PrProvider {
    let url = origin_url(git, repo).await;
    PrProvider {
        remote: url.as_deref().map(display_remote),
        provider: url.as_deref().and_then(Provider::detect),
        logged_in: None,
        cli_found: false,
        open_pr_skill,
    }
}

impl crate::board::Board {
    /// A project's PR provider and its CLI's login (REV-11, REV-12).
    pub async fn pr_provider(
        &self,
        project: &str,
        env: &CheckEnv,
    ) -> Result<PrProvider, crate::board::BoardError> {
        let repo = self.project(project).await?.repo_path;
        let skill = self.delivery_skill(project, WorkflowStep::OpenPr).await?;
        Ok(detect(self.git()?, Path::new(&repo), skill, env).await)
    }

    /// The project's PR provider from its remote URL alone (no CLI runs).
    pub async fn pr_provider_from_remote(
        &self,
        project: &str,
    ) -> Result<PrProvider, crate::board::BoardError> {
        let repo = self.project(project).await?.repo_path;
        let skill = self.delivery_skill(project, WorkflowStep::OpenPr).await?;
        Ok(detect_from_remote(self.git()?, Path::new(&repo), skill).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_github_and_azure_devops_in_https_and_ssh_forms() {
        use Provider::*;
        for (url, want) in [
            ("https://github.com/acme/shop-web.git", Some(GitHub)),
            ("https://nevzat@github.com/acme/shop-web", Some(GitHub)),
            ("git@github.com:acme/shop-web.git", Some(GitHub)),
            ("ssh://git@github.com/acme/shop-web.git", Some(GitHub)),
            (
                "ssh://git@ssh.github.com:443/acme/shop-web.git",
                Some(GitHub),
            ),
            (
                "https://dev.azure.com/acme/shop/_git/billing-api",
                Some(AzureDevOps),
            ),
            (
                "https://acme@dev.azure.com/acme/shop/_git/billing-api",
                Some(AzureDevOps),
            ),
            (
                "git@ssh.dev.azure.com:v3/acme/shop/billing-api",
                Some(AzureDevOps),
            ),
            (
                "https://acme.visualstudio.com/shop/_git/billing-api",
                Some(AzureDevOps),
            ),
            (
                "acme@vs-ssh.visualstudio.com:v3/acme/shop/billing-api",
                Some(AzureDevOps),
            ),
            ("https://git.acme.internal/web/handbook.git", None),
            ("git@git.acme.internal:web/handbook.git", None),
            ("https://github.com.evil.example/acme/x", None),
            ("https://notgithub.com/acme/x", None),
            ("/srv/git/shop-web.git", None),
            ("../shop-web.git", None),
            ("", None),
        ] {
            assert_eq!(Provider::detect(url), want, "{url}");
        }
    }

    #[test]
    fn display_drops_scheme_user_and_dot_git() {
        assert_eq!(
            display_remote("https://git.acme.internal/web/handbook.git"),
            "git.acme.internal/web/handbook"
        );
        assert_eq!(
            display_remote("git@github.com:acme/shop-web.git"),
            "github.com/acme/shop-web"
        );
        assert_eq!(
            display_remote("https://acme@dev.azure.com/acme/shop/_git/billing-api"),
            "dev.azure.com/acme/shop/_git/billing-api"
        );
    }

    #[test]
    fn summary_and_blocker() {
        let mut p = PrProvider {
            remote: Some("github.com/acme/shop-web".into()),
            provider: Some(Provider::GitHub),
            logged_in: Some(true),
            cli_found: true,
            open_pr_skill: WorkflowStep::OpenPr.own_skill().to_owned(),
        };
        assert_eq!(
            p.summary(),
            "GitHub (detected from origin) \u{b7} gh: signed in"
        );
        assert_eq!(p.open_pr_blocked(), None);
        p.logged_in = Some(false);
        assert_eq!(
            p.summary(),
            "GitHub (detected from origin) \u{b7} gh: not signed in"
        );
        p.provider = None;
        assert_eq!(p.summary(), "Not detected: use your own Open PR skill");
        assert_eq!(p.open_pr_blocked(), Some("PR provider not detected"));
        // An Open PR skill the Delivery stage names, shown by its name, is the user's own (REV-11, WSK-08).
        p.open_pr_skill = "my-pr".into();
        assert!(!p.built_in_open_pr());
        assert_eq!(
            p.open_pr_blocked(),
            None,
            "a custom skill handles any remote"
        );
    }
}

//! "Is the CLI installed and logged in?" for each of a plugin's `requires` (the otaq-marketplace README, "Ticket sources").
//! Only ever reads: the manifest's `check`, `login_check` and extension `check` command lines.

use serde::{Deserialize, Serialize};

use super::manifest::Manifest;
use super::runner::{CHECK_TIMEOUT, Runner};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequirementStatus {
    pub id: String,
    pub name: String,
    pub installed: bool,
    /// `None` when the plugin has no login check (or the CLI isn't installed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logged_in: Option<bool>,
    /// `None` when the CLI needs no extension (or isn't installed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_added: Option<bool>,
    /// How to install it on this OS, when it isn't.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,
    /// What **Log in** runs, when not logged in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<Vec<String>>,
    /// What **Add extension** runs, when it is missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_install: Option<Vec<String>>,
}

impl RequirementStatus {
    /// Ready for commands.
    pub fn ready(&self) -> bool {
        self.installed && self.logged_in != Some(false) && self.extension_added != Some(false)
    }
}

pub async fn check(manifest: &Manifest, runner: &Runner) -> Vec<RequirementStatus> {
    let mut out = Vec::new();
    for requirement in &manifest.requires {
        let ok = |argv: Vec<String>| async move {
            runner
                .run(&argv, CHECK_TIMEOUT)
                .await
                .is_ok_and(|o| o.success)
        };
        let installed = ok(requirement.check.clone()).await;
        let mut status = RequirementStatus {
            id: requirement.id.clone(),
            name: requirement.name.clone(),
            installed,
            logged_in: None,
            extension_added: None,
            install_hint: None,
            login: None,
            extension_install: None,
        };
        if !installed {
            status.install_hint = requirement.install.here().map(str::to_owned);
            out.push(status);
            continue;
        }
        if let Some(extension) = &requirement.extension {
            let added = ok(extension.check.clone()).await;
            status.extension_added = Some(added);
            if !added {
                status.extension_install = Some(extension.install.clone());
            }
        }
        if let Some(login_check) = &requirement.login_check {
            let logged_in = ok(login_check.clone()).await;
            status.logged_in = Some(logged_in);
            if !logged_in {
                status.login = requirement.login.clone();
            }
        }
        out.push(status);
    }
    out
}

//! Adding a project (UI-08, PRJ-01…PRJ-03; *14 · First run & onboarding → 3 · Add project*, *Not a git repo*,
//! *Already added*, *Adding a second project later*). The service checks the folder (`project.probe`): a git
//! repository, not added yet; it suggests the key and detects the base branch. The key validates live (format,
//! not used by another project) and can't be changed later; the base branch can be changed before adding.
//! Nothing is written into the repository (d0023). Adding the project records the folder trust of each connected
//! tool that keeps one (PRJ-06, PRJ-07, d0085, d0086): the form names those tools, and when a tool's trust fails the
//! project is still added and the form says why, per tool, before it closes.

use otaq_core::board::Refusal;
use otaq_core::board::projects::FolderProbe;
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::domain::ProjectKey;
use otaq_core::naming;

use otaq_core::agent_tools::Tool;

use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::shell::tilde;
use crate::view_models::agent_tools_page::and_list;

/// An error under the folder picker: a title and what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderError {
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddProjectModel {
    /// The folder the user picked, as picked.
    pub folder: Option<String>,
    pub probe: Option<FolderProbe>,
    pub folder_error: Option<FolderError>,
    pub probing: bool,
    pub key: String,
    /// The user typed in the key field (the suggestion no longer replaces it).
    pub key_edited: bool,
    pub base_branch: String,
    /// An error from `project.add` for the key (e.g. taken meanwhile).
    pub key_server_error: Option<String>,
    /// Any other error from `project.add`.
    pub add_error: Option<String>,
    pub adding: bool,
    /// The connected tools that keep folder trust, in d0086's order: the line names them.
    pub trust_tools: Vec<Tool>,
    /// The project was added, but these tools' trust couldn't be recorded for its folder: why (a sentence without
    /// its final period). The form waits for Continue.
    pub trust_failed: Vec<(Tool, String)>,
}

impl AddProjectModel {
    pub fn pick(&mut self, folder: String) {
        self.folder = Some(folder);
        self.probe = None;
        self.folder_error = None;
        self.add_error = None;
        self.probing = true;
    }

    /// The probe's answer: the detected values, or the refusal (code and message) for the folder.
    pub fn probed(
        &mut self,
        result: Result<FolderProbe, (Option<String>, String)>,
        projects: &[ProjectInfo],
        keys: &KeyLabels,
    ) {
        self.probing = false;
        match result {
            Ok(probe) => {
                if !self.key_edited {
                    self.key = probe.suggested_key.clone();
                }
                self.base_branch = probe.base_branch.clone();
                self.probe = Some(probe);
            }
            Err((code, message)) => {
                let folder = self.folder.clone().unwrap_or_default();
                self.folder_error = Some(folder_error(
                    code.as_deref(),
                    &message,
                    &folder,
                    projects,
                    keys,
                ));
            }
        }
    }

    pub fn set_key(&mut self, key: &str) {
        self.key = key.trim().to_uppercase();
        self.key_edited = true;
        self.key_server_error = None;
    }

    pub fn set_base(&mut self, base: &str) {
        self.base_branch = base.trim().to_owned();
    }

    /// The key field's message, live: format, then uniqueness, then the service's answer.
    pub fn key_error(&self, projects: &[ProjectInfo]) -> Option<String> {
        if self.key.is_empty() {
            return Some("A project key is needed".into());
        }
        if ProjectKey::new(&self.key).is_err() {
            return Some(
                "Use uppercase letters and digits, starting with a letter (up to 10)".into(),
            );
        }
        if let Some(other) = projects.iter().find(|p| p.key == self.key) {
            return Some(format!("{} is already used by {}", self.key, other.name));
        }
        self.key_server_error.clone()
    }

    /// "Used in ticket IDs like SHOP-18. Can’t be changed later."
    pub fn key_hint(&self) -> String {
        let key = if self.key.is_empty() {
            "SHOP"
        } else {
            &self.key
        };
        format!("Used in ticket IDs like {key}-18. Can’t be changed later.")
    }

    pub fn can_add(&self, projects: &[ProjectInfo]) -> bool {
        self.probe.is_some()
            && self.folder_error.is_none()
            && self.key_error(projects).is_none()
            && !self.base_branch.is_empty()
            && !self.adding
            && self.trust_failed.is_empty()
    }

    /// `project.add` answered that the project was added but some tools' folder trust wasn't recorded (PRJ-07).
    pub fn added_untrusted(&mut self, failed: Vec<(Tool, String)>) {
        self.adding = false;
        self.trust_failed = failed;
    }

    /// The line under the folder: which tools adding the project lets work in it (PRJ-07, d0086). One tool keeps
    /// today's wording; none (no connected tool keeps folder trust) leaves the line out.
    pub fn trust_line(&self) -> Option<String> {
        let names: Vec<&str> = self.trust_tools.iter().map(|t| t.display_name()).collect();
        match names.as_slice() {
            [] => None,
            [one] => Some(format!(
                "Adding a project also lets {one} work in this folder (its hooks and settings will run)."
            )),
            several => Some(format!(
                "Adding a project also lets {} work in this folder (their hooks and settings will run).",
                and_list(several)
            )),
        }
    }

    /// Why a tool's trust wasn't recorded, one sentence per tool.
    pub fn trust_errors(&self) -> Vec<String> {
        self.trust_failed
            .iter()
            .map(|(tool, reason)| {
                let name = tool.display_name();
                format!(
                    "{name}\u{2019}s trust couldn\u{2019}t be recorded: {reason}. Its agents will wait on {name}\u{2019}s trust screen."
                )
            })
            .collect()
    }

    /// The answer of `project.add` when it failed: a key problem goes under the key, the rest under the form.
    pub fn add_failed(&mut self, code: Option<&str>, message: String) {
        self.adding = false;
        match code {
            Some(c) if c == Refusal::KeyTaken.as_str() || c == Refusal::BadKey.as_str() => {
                self.key_server_error = Some(message)
            }
            _ => self.add_error = Some(message),
        }
    }
}

/// The design's wording for a folder that can't be added.
fn folder_error(
    code: Option<&str>,
    message: &str,
    folder: &str,
    projects: &[ProjectInfo],
    keys: &KeyLabels,
) -> FolderError {
    let shown = tilde(folder);
    if code == Some(Refusal::NotAGitRepo.as_str()) {
        return FolderError {
            title: "Not a git repository".into(),
            detail: format!(
                "{shown} has no .git folder. Pick the repo’s root folder, or run git init there first."
            ),
        };
    }
    if code == Some(Refusal::AlreadyAdded.as_str()) {
        let found = projects.iter().enumerate().find(|(_, p)| {
            folder.starts_with(&p.repo_path) || message.starts_with(&format!("{} ", p.name))
        });
        if let Some((i, p)) = found {
            let switch = (i < 9)
                .then(|| keys.label(KeyAction::Project(i as u8 + 1)))
                .flatten()
                .map_or_else(String::new, |k| format!("Switch to it with {k}, or pick "));
            let detail = if switch.is_empty() {
                "Pick a different folder.".to_owned()
            } else {
                format!("{switch}a different folder.")
            };
            return FolderError {
                title: format!("{} is already in {}", p.name, naming::PRODUCT_NAME),
                detail,
            };
        }
    }
    FolderError {
        title: "Can’t add this folder".into(),
        detail: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_labels::fixtures;

    fn projects() -> Vec<ProjectInfo> {
        vec![
            ProjectInfo {
                key: "SHOP".into(),
                name: "shop-web".into(),
                repo_path: "/code/shop-web".into(),
                base_branch: "main".into(),
                agents_working: 0,
                needs_you: 0,
                flow_setup_since: None,
                paused_since: None,
                paused_by: None,
            },
            ProjectInfo {
                key: "BILL".into(),
                name: "billing-api".into(),
                repo_path: "/code/billing-api".into(),
                base_branch: "main".into(),
                agents_working: 0,
                needs_you: 0,
                flow_setup_since: None,
                paused_since: None,
                paused_by: None,
            },
        ]
    }

    fn probe(key: &str) -> FolderProbe {
        FolderProbe {
            repo_path: "/code/new-app".into(),
            name: "new-app".into(),
            suggested_key: key.into(),
            base_branch: "develop".into(),
            branches: Default::default(),
        }
    }

    #[test]
    fn valid_folder_prefills_key_and_base() {
        let mut m = AddProjectModel::default();
        assert!(!m.can_add(&projects()));
        m.pick("/code/new-app".into());
        assert!(m.probing);
        m.probed(Ok(probe("NEW")), &projects(), &fixtures::base());
        assert_eq!((m.key.as_str(), m.base_branch.as_str()), ("NEW", "develop"));
        assert_eq!(m.key_error(&projects()), None);
        assert!(m.can_add(&projects()));
        assert_eq!(
            m.key_hint(),
            "Used in ticket IDs like NEW-18. Can’t be changed later."
        );
        m.set_base("release/2.3");
        assert_eq!(m.base_branch, "release/2.3");
        m.set_base("  ");
        assert!(!m.can_add(&projects()), "the base branch is needed");
    }

    #[test]
    fn key_validates_live() {
        let mut m = AddProjectModel::default();
        m.pick("/code/new-app".into());
        m.probed(Ok(probe("NEW")), &projects(), &fixtures::base());
        m.set_key("shop");
        assert_eq!(m.key, "SHOP", "typed lowercase becomes uppercase");
        assert_eq!(
            m.key_error(&projects()).as_deref(),
            Some("SHOP is already used by shop-web")
        );
        m.set_key("1AB");
        assert!(
            m.key_error(&projects())
                .unwrap()
                .starts_with("Use uppercase")
        );
        m.set_key("");
        assert!(m.key_error(&projects()).is_some());
        m.set_key("APP");
        assert!(m.can_add(&projects()));
        // A later probe keeps what the user typed.
        m.probed(Ok(probe("NEW")), &projects(), &fixtures::base());
        assert_eq!(m.key, "APP");
        m.add_failed(Some("key_taken"), "APP is already used by app".into());
        assert_eq!(
            m.key_error(&projects()).as_deref(),
            Some("APP is already used by app")
        );
    }

    #[test]
    fn not_a_git_repo_and_already_added() {
        let mut m = AddProjectModel::default();
        m.pick("/Users/me/Downloads/designs".into());
        m.probed(
            Err((
                Some("not_a_git_repo".into()),
                "… isn't a git repository".into(),
            )),
            &projects(),
            &fixtures::base(),
        );
        let e = m.folder_error.clone().unwrap();
        assert_eq!(e.title, "Not a git repository");
        assert!(e.detail.ends_with(
            "has no .git folder. Pick the repo’s root folder, or run git init there first."
        ));
        assert!(!m.can_add(&projects()));

        m.pick("/code/billing-api".into());
        m.probed(
            Err((
                Some("already_added".into()),
                "billing-api is already added as BILL".into(),
            )),
            &projects(),
            &fixtures::base(),
        );
        let e = m.folder_error.clone().unwrap();
        assert_eq!(
            e.title,
            format!("billing-api is already in {}", naming::PRODUCT_NAME)
        );
        assert_eq!(
            e.detail,
            "Switch to it with ⌘2, or pick a different folder."
        );
    }

    /// KEY-08: "Switch to it with …" names the user's key for that project, and is left out when it has none.
    #[test]
    fn already_added_names_the_users_project_key() {
        let detail = |keys: &KeyLabels| {
            let mut m = AddProjectModel::default();
            m.pick("/code/billing-api".into());
            m.probed(
                Err((
                    Some("already_added".into()),
                    "billing-api is already added as BILL".into(),
                )),
                &projects(),
                keys,
            );
            m.folder_error.unwrap().detail
        };
        assert_eq!(
            detail(&fixtures::replaced("project", Some(2), 0, "⌥2")),
            "Switch to it with ⌥2, or pick a different folder."
        );
        assert_eq!(
            detail(&fixtures::removed("project", Some(2), 0)),
            "Pick a different folder."
        );
        assert_eq!(
            detail(&fixtures::letters_off()),
            "Switch to it with ⌘2, or pick a different folder."
        );
    }
}

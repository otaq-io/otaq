//! The name and email git signs commits with, when git has none (a fresh Mac often hasn't): every commit an agent
//! makes and every merge would otherwise fail with git's "Author identity unknown". The same small form shows on
//! the Tools step's git row, on Settings → Git & pull requests, and in the merge dialog when a merge finds none.
//! **Save** writes them into the user's own git settings (`git config --global`, `tools.git_identity`), only on that
//! click.

use otaq_core::git::Identity;
use serde::Serialize;
use ts_rs::TS;

/// The form's words and what git has so far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "GitIdentityForm")]
pub struct Form {
    /// "git doesn't know your name and email yet".
    pub title: String,
    /// Why it matters, in one sentence.
    pub text: String,
    pub name_label: String,
    /// What git has already (one of the two may be set); empty otherwise.
    pub name: String,
    pub name_placeholder: String,
    pub email_label: String,
    pub email: String,
    pub email_placeholder: String,
    /// Where Save writes them.
    pub note: String,
    pub save: String,
}

/// The state line of git's row while they are missing.
pub const STATE: &str = "Needs your name and email";

pub fn form(has: &Identity) -> Form {
    Form {
        title: "git doesn\u{2019}t know your name and email yet".to_owned(),
        text: "Every commit carries its author\u{2019}s name and email: the agents\u{2019} commits and your merges. \
               Without them git refuses to commit."
            .to_owned(),
        name_label: "Name".to_owned(),
        name: has.name.clone().unwrap_or_default(),
        name_placeholder: "Ada Lovelace".to_owned(),
        email_label: "Email".to_owned(),
        email: has.email.clone().unwrap_or_default(),
        email_placeholder: "ada@example.com".to_owned(),
        note: "Saved in your git settings for every repository (git config --global).".to_owned(),
        save: "Save".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_starts_from_what_git_has() {
        let f = form(&Identity {
            name: Some("Ada".into()),
            email: None,
        });
        assert_eq!((f.name.as_str(), f.email.as_str()), ("Ada", ""));
        assert_eq!(f.title, "git doesn\u{2019}t know your name and email yet");
        assert!(f.note.contains("git config --global"), "{}", f.note);
    }
}

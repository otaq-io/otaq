//! A project's git hook refused Approve & merge (the user's click or the flow's automatic delivery), or one of the
//! commit messages Approve & open PR was to write: how the ticket shows it. The card's Needs you flag and line come from [`super::card::hook_refusal`]; this module has the sheet's
//! callout (which hook, the step, the hook's whole output in the same box as a refused start's, and the way on) and
//! the toast that only points to the ticket. History's row is the refused start's ([`super::history`]).

use otaq_core::git::{GitStep, RefusedHook};

use super::sheet::{HookOutput, hook_output};

/// The callout on the ticket's sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRefusalView {
    /// "Your commit-msg hook refused the merge".
    pub heading: String,
    /// "It refused the squash commit." when git said which step.
    pub step: Option<String>,
    /// The hook's whole output, with the slot for a sentence above it ([`HookOutput::note`]).
    pub hook: HookOutput,
    /// The commit message it refused, when it checked one of the PR's.
    pub message: Option<RefusedMessage>,
    /// "Nothing was merged … Edit the message so the hook accepts it, or fix what it found, then merge again."
    pub way_on: String,
}

/// The commit message a hook refused, above its output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedMessage {
    /// "The message it refused".
    pub label: String,
    pub text: String,
}

/// The sheet's callout for a refused merge or PR. `base` is the branch the ticket merges into ("main").
pub fn view(refused: &RefusedHook, base: &str) -> HookRefusalView {
    let pr = refused.step == Some(GitStep::Reword);
    HookRefusalView {
        heading: format!("Your {}", refused.line()),
        step: refused.step.map(|s| step_sentence(s, base)),
        hook: hook_output(refused),
        message: refused.message.as_ref().map(|text| RefusedMessage {
            label: "The message it refused".to_owned(),
            text: text.clone(),
        }),
        way_on: match pr {
            true => "Nothing was rewritten or pushed: the branch and the worktree are as they were. Edit the \
                     message so the hook accepts it, then open the PR again; your edits are kept."
                .to_owned(),
            false => format!(
                "Nothing was merged: {base}, the branch and the worktree are as they were. Edit the message so the \
                 hook accepts it, or fix what it found, then merge again."
            ),
        },
    }
}

/// "It refused the squash commit.": the step of Otaq's git work the hook ran in.
pub fn step_sentence(step: GitStep, base: &str) -> String {
    let what = match step {
        GitStep::AddWorktree => "the making of the ticket\u{2019}s worktree".to_owned(),
        GitStep::SaveUncommitted => "the save of the uncommitted files".to_owned(),
        GitStep::UpdateBranch => format!("the merge of {base} into the branch"),
        GitStep::RestoreUncommitted => "putting the uncommitted files back".to_owned(),
        GitStep::UpdateTarget => format!("bringing {base} up to date"),
        GitStep::PrepareScratch => "the scratch worktree\u{2019}s checkout".to_owned(),
        GitStep::Squash => "the squash of the branch".to_owned(),
        GitStep::Commit => "the squash commit".to_owned(),
        GitStep::MergeCheck => "the merge check".to_owned(),
        GitStep::MoveTarget => format!("moving {base} to the new commit"),
        GitStep::Reword => "a commit message of the PR".to_owned(),
    };
    format!("It refused {what}.")
}

/// The toast when the user's click was refused: it only points to the ticket, where the refusal is (a click on it
/// opens the ticket). `line` is "commit-msg hook refused the merge". Title and text.
pub fn toast(key: &str, line: &str) -> (String, String) {
    (
        format!("{key}: your {line}"),
        "Its output and the way on are on the ticket.".to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(step: Option<GitStep>) -> RefusedHook {
        RefusedHook {
            name: "commit-msg".into(),
            step,
            what: "the merge".into(),
            output: "commitlint: type must be one of [feat, fix]\n".into(),
            message: None,
        }
    }

    #[test]
    fn the_sheet_names_the_hook_the_step_and_the_way_on() {
        let v = view(&refused(Some(GitStep::Commit)), "main");
        assert_eq!(v.heading, "Your commit-msg hook refused the merge");
        assert_eq!(v.step.as_deref(), Some("It refused the squash commit."));
        assert_eq!(
            v.hook.output,
            "commitlint: type must be one of [feat, fix]\n"
        );
        assert_eq!(v.hook.label, "Output of your commit-msg hook");
        assert_eq!(v.hook.note, None);
        assert!(
            v.way_on.starts_with("Nothing was merged: main,"),
            "{}",
            v.way_on
        );
        assert!(v.way_on.contains("Edit the message"), "{}", v.way_on);
        assert_eq!(view(&refused(None), "main").step, None);
        assert_eq!(
            step_sentence(GitStep::UpdateBranch, "develop"),
            "It refused the merge of develop into the branch."
        );
    }

    /// A PR's commit message the commit-msg hook refused: the message itself, the hook's output, and the PR dialog
    /// as the way on.
    #[test]
    fn a_refused_pr_message_shows_the_message() {
        let pr = RefusedHook {
            step: Some(GitStep::Reword),
            what: "the PR".into(),
            ..refused(None)
        }
        .with_message("update stuff");
        let v = view(&pr, "main");
        assert_eq!(v.heading, "Your commit-msg hook refused the PR");
        assert_eq!(
            v.step.as_deref(),
            Some("It refused a commit message of the PR.")
        );
        let message = v.message.expect("the refused message");
        assert_eq!(message.text, "update stuff");
        assert_eq!(message.label, "The message it refused");
        assert!(
            v.way_on.starts_with("Nothing was rewritten or pushed"),
            "{}",
            v.way_on
        );
        assert!(v.way_on.contains("open the PR again"), "{}", v.way_on);
        assert_eq!(view(&refused(None), "main").message, None);
    }

    #[test]
    fn the_toast_points_to_the_ticket() {
        let (title, text) = toast("HOOK-1", &refused(None).line());
        assert_eq!(title, "HOOK-1: your commit-msg hook refused the merge");
        assert_eq!(text, "Its output and the way on are on the ticket.");
    }
}

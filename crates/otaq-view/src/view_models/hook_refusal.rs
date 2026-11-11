//! A project's git hook refused Approve & merge (the user's click or the flow's automatic delivery): how the ticket
//! shows it. The card's Needs you flag and line come from [`super::card::hook_refusal`]; this module has the sheet's
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
    /// "Nothing was merged … Edit the message so the hook accepts it, or fix what it found, then merge again."
    pub way_on: String,
}

/// The sheet's callout for a refused merge. `base` is the branch the ticket merges into ("main").
pub fn view(refused: &RefusedHook, base: &str) -> HookRefusalView {
    HookRefusalView {
        heading: format!("Your {}", refused.line()),
        step: refused.step.map(|s| step_sentence(s, base)),
        hook: hook_output(refused),
        way_on: format!(
            "Nothing was merged: {base}, the branch and the worktree are as they were. Edit the message so the hook \
             accepts it, or fix what it found, then merge again."
        ),
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
        GitStep::Reword => "the new commit messages".to_owned(),
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

    #[test]
    fn a_hook_that_asks_in_the_terminal_is_named_as_such_above_its_output() {
        let refused = RefusedHook {
            name: "pre-commit".into(),
            step: Some(GitStep::Commit),
            what: "the merge".into(),
            output: "~/shop/.git/hooks/pre-commit: line 3: /dev/tty: No such device or address"
                .into(),
        };
        let v = view(&refused, "main");
        assert_eq!(
            v.hook.note,
            Some(format!(
                "Your pre-commit hook asks a question in the terminal; {} can\u{2019}t answer it. Make the hook \
                 skip its question when there\u{2019}s no terminal, or do this step by hand.",
                otaq_core::naming::PRODUCT_NAME
            ))
        );
        assert_eq!(v.hook.output, refused.output, "the output stays whole");
    }

    #[test]
    fn the_toast_points_to_the_ticket() {
        let (title, text) = toast("HOOK-1", &refused(None).line());
        assert_eq!(title, "HOOK-1: your commit-msg hook refused the merge");
        assert_eq!(text, "Its output and the way on are on the ticket.");
    }
}

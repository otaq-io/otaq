//! Commit messages follow the project's own rules: the delivery's agent (writing the merge message) and agents
//! committing in their worktrees look for the project's message rules before writing, and keep the ticket key where
//! the rules allow. Only what Otaq controls is checked here, on real hooks in temporary repositories: the place the
//! prompt and the skills send the agent to find the hooks is the folder git really runs them from (also from a
//! ticket's worktree, and with a husky-style `core.hooksPath`), and the messages they show as examples pass a
//! `feat:`/`fix:` commit-msg hook on the first try, while the key-first default is what a project with no rules gets.

use std::path::{Path, PathBuf};
use std::process::Command;

use otaq_core::git::testing::{TempRepos, commit, git};
use otaq_core::pr::message::{MESSAGE_RULES_PLACES, prompt};

/// A Conventional Commits rule, as the release test's project had it.
const FEAT_FIX_HOOK: &str = "#!/bin/sh\n\
    if ! head -n 1 \"$1\" | grep -Eq '^(feat|fix)(\\(.+\\))?: '; then\n\
    \x20 echo 'commit-msg: the message must start with \"feat: \" or \"fix: \"' >&2\n\
    \x20 exit 1\n\
    fi\n";

fn write_hook(path: &Path, script: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, script).unwrap();
    // Git for Windows runs hooks with its own sh; only macOS and Linux need the executable bit.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// Commits a change with `message` the way an agent does (the user's hooks run); whether git took it, and its output.
fn try_commit(worktree: &Path, message: &str) -> (bool, String) {
    std::fs::write(worktree.join("calc.js"), format!("// {message}\n")).unwrap();
    git(worktree, &["add", "calc.js"]);
    let out = Command::new("git")
        .args(["commit", "--quiet", "-m", message])
        .current_dir(worktree)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), said)
}

/// A ticket's worktree of `repo`, as Otaq makes it (a linked worktree on its own branch).
fn ticket_worktree(repos: &TempRepos, repo: &Path, key: &str) -> PathBuf {
    let path = repos.root().join(format!("wt-{key}"));
    git(
        repo,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            &format!("otaq/{key}"),
            path.to_str().unwrap(),
        ],
    );
    path
}

/// The hooks folder the agent is told to read, as git prints it from the worktree.
fn hooks_folder(worktree: &Path) -> PathBuf {
    let printed = git(worktree, &["rev-parse", "--git-path", "hooks"]);
    worktree.join(printed)
}

/// The example title the prompt gives for a project with rules: `feat: … (KEY)`.
fn rules_example(text: &str, key: &str) -> String {
    let end = format!("({key})");
    let start = text.find("\"feat: ").expect("a feat: example") + 1;
    let len = text[start..].find(&end).expect("the key in the example") + end.len();
    text[start..start + len].to_owned()
}

/// The example title the prompt gives for a project with no rules: the key first.
fn plain_example(text: &str, key: &str) -> String {
    let marker = format!("it starts with {key} (\"");
    let start = text.find(&marker).expect("a key-first example") + marker.len();
    let len = text[start..].find('"').unwrap();
    text[start..start + len].to_owned()
}

/// Husky-style: the rule lives in a committed `.githooks/commit-msg` named by `core.hooksPath`. From the ticket's
/// worktree the agent finds that hook where the prompt says, and the message written as the prompt shows passes it
/// the first time; the key-first message is the one the hook refuses, which is why the rules are read first.
#[test]
fn with_a_hooks_path_rule_the_prompts_message_passes_the_hook_first_time() {
    let repos = TempRepos::new();
    let repo = repos.repo("hookproj", 1);
    write_hook(&repo.join(".githooks/commit-msg"), FEAT_FIX_HOOK);
    std::fs::write(
        repo.join("commitlint.config.js"),
        "module.exports = { extends: ['@commitlint/config-conventional'] };\n",
    )
    .unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "chore: add the hooks"]);
    git(&repo, &["config", "core.hooksPath", ".githooks"]);
    let worktree = ticket_worktree(&repos, &repo, "HOOK-1");

    let text = prompt("HOOK-1", "main", false, true);
    assert!(text.contains(MESSAGE_RULES_PLACES));
    let hooks = hooks_folder(&worktree);
    assert!(
        hooks.join("commit-msg").is_file(),
        "`git rev-parse --git-path hooks` names the hooksPath folder from the worktree: {}",
        hooks.display()
    );

    let (ok, said) = try_commit(&worktree, &plain_example(&text, "HOOK-1"));
    assert!(!ok, "the key-first message is refused by a feat:/fix: rule");
    assert!(said.contains("must start with"), "{said}");
    let (ok, said) = try_commit(&worktree, &rules_example(&text, "HOOK-1"));
    assert!(ok, "the rule-following message passes first time: {said}");
    // The commit skill's example passes the same hook.
    let (ok, said) = try_commit(&worktree, "feat: add the price table with sorting (HOOK-1)");
    assert!(ok, "{said}");
}

/// A hook in `.git/hooks` (no hooksPath) is found from a linked worktree too: git prints the shared hooks folder.
#[test]
fn a_hook_in_the_repositorys_own_hooks_folder_is_found_from_the_worktree() {
    let repos = TempRepos::new();
    let repo = repos.repo("plainhooks", 1);
    write_hook(&repo.join(".git/hooks/commit-msg"), FEAT_FIX_HOOK);
    let worktree = ticket_worktree(&repos, &repo, "HOOK-2");
    let hooks = hooks_folder(&worktree);
    assert!(
        hooks.join("commit-msg").is_file(),
        "{} has the repository's commit-msg hook",
        hooks.display()
    );
    let text = prompt("HOOK-2", "main", true, true);
    let (ok, said) = try_commit(&worktree, &rules_example(&text, "HOOK-2"));
    assert!(ok, "{said}");
}

/// A project with no rules: the default message, key first, is what the prompt asks for, and it commits.
#[test]
fn with_no_rules_the_key_still_leads() {
    let repos = TempRepos::new();
    let repo = repos.repo("norules", 1);
    commit(&repo, "README.md", "a project\n");
    let worktree = ticket_worktree(&repos, &repo, "SHOP-18");
    let text = prompt("SHOP-18", "main", false, true);
    let plain = plain_example(&text, "SHOP-18");
    assert!(plain.starts_with("SHOP-18 "), "{plain}");
    let (ok, said) = try_commit(&worktree, &plain);
    assert!(ok, "{said}");
}

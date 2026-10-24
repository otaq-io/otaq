//! Keeping the base current (T-0047): fetch, fast-forward when only behind, hold when diverged, never touch a
//! dirty checkout. A local bare repository plays the remote; nothing goes over the network.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use otaq_core::git::testing::{TempRepos, commit, git};
use otaq_core::git::{BaseSync, Git};

fn runner() -> Git {
    Git::find().expect("git on PATH")
}

/// Every file outside `.git` with its bytes, to prove a working tree wasn't touched.
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                out.insert(path.strip_prefix(root).unwrap().to_path_buf(), bytes);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

struct Remote {
    repos: TempRepos,
    /// The user's clone, with `develop` checked out and tracking `origin/develop`.
    clone: PathBuf,
    /// Another clone that pushes new commits to the remote.
    other: PathBuf,
}

fn setup() -> Remote {
    let repos = TempRepos::new();
    let clone = repos.clone_with_origin("billing-api", "develop", 2);
    let other = repos.root().join("billing-api-seed");
    Remote {
        repos,
        clone,
        other,
    }
}

impl Remote {
    /// Somebody else pushes a commit to `origin/develop`.
    fn push_upstream(&self, file: &str) {
        commit(&self.other, file, "theirs\n");
        git(&self.other, &["push", "--quiet", "origin", "develop"]);
    }

    fn head(&self, reference: &str) -> String {
        git(&self.clone, &["rev-parse", reference])
    }
}

#[tokio::test]
async fn up_to_date() {
    let r = setup();
    let before = r.head("develop");
    let result = runner().sync_base(&r.clone, "develop").await.unwrap();
    assert_eq!(result, BaseSync::UpToDate);
    assert_eq!(r.head("develop"), before);
}

#[tokio::test]
async fn ahead_only_is_up_to_date_and_never_pushed() {
    let r = setup();
    commit(&r.clone, "mine.txt", "mine\n");
    let result = runner().sync_base(&r.clone, "develop").await.unwrap();
    assert_eq!(result, BaseSync::UpToDate);
    assert_ne!(
        r.head("develop"),
        r.head("origin/develop"),
        "nothing pushed"
    );
}

#[tokio::test]
async fn behind_fast_forwards_the_clean_checkout() {
    let r = setup();
    r.push_upstream("new.txt");
    let result = runner().sync_base(&r.clone, "develop").await.unwrap();
    assert_eq!(result, BaseSync::FastForwarded);
    assert_eq!(r.head("develop"), r.head("origin/develop"));
    assert_eq!(
        std::fs::read_to_string(r.clone.join("new.txt")).unwrap(),
        "theirs\n",
        "the checked-out files moved with the branch"
    );
    assert!(git(&r.clone, &["status", "--porcelain"]).is_empty());
}

#[tokio::test]
async fn behind_and_not_checked_out_moves_only_the_ref() {
    let r = setup();
    git(&r.clone, &["checkout", "--quiet", "-b", "feature"]);
    let files = tree(&r.clone);
    r.push_upstream("new.txt");
    let result = runner().sync_base(&r.clone, "develop").await.unwrap();
    assert_eq!(result, BaseSync::FastForwarded);
    assert_eq!(r.head("develop"), r.head("origin/develop"));
    assert_eq!(tree(&r.clone), files, "the feature checkout is untouched");
}

#[tokio::test]
async fn behind_with_a_dirty_checkout_is_left_alone() {
    let r = setup();
    std::fs::write(r.clone.join("file1.txt"), "my unsaved work\n").unwrap();
    let files = tree(&r.clone);
    let before = r.head("develop");
    r.push_upstream("new.txt");

    let result = runner().sync_base(&r.clone, "develop").await.unwrap();
    let BaseSync::CheckoutDirty { message } = result else {
        panic!("{result:?}")
    };
    assert!(message.contains("uncommitted changes"), "{message}");
    assert_eq!(r.head("develop"), before, "the ref is left alone");
    assert_eq!(tree(&r.clone), files, "byte-identical working tree");
    assert_eq!(git(&r.clone, &["stash", "list"]), "", "never stashes");
}

#[tokio::test]
async fn diverged_holds_with_the_message() {
    let r = setup();
    commit(&r.clone, "mine.txt", "mine\n");
    std::fs::write(r.clone.join("file2.txt"), "dirty too\n").unwrap();
    let files = tree(&r.clone);
    let before = r.head("develop");
    r.push_upstream("theirs.txt");

    let result = runner().sync_base(&r.clone, "develop").await.unwrap();
    assert_eq!(
        result,
        BaseSync::Diverged {
            message: "`develop` and `origin/develop` have diverged. Push or pull, then retry"
                .into()
        }
    );
    assert_eq!(r.head("develop"), before, "nothing merged or rebased");
    assert_eq!(tree(&r.clone), files);
}

#[tokio::test]
async fn a_branch_only_on_the_remote() {
    let r = setup();
    git(&r.other, &["checkout", "--quiet", "-b", "release"]);
    git(&r.other, &["push", "--quiet", "origin", "release"]);
    let result = runner().sync_base(&r.clone, "release").await.unwrap();
    assert_eq!(result, BaseSync::OnlyOnRemote);
    assert!(!r.head("origin/release").is_empty(), "fetched");
}

#[tokio::test]
async fn no_remote_skips_fetching() {
    let repos = TempRepos::new();
    let repo = repos.repo("shop-web", 1);
    let result = runner().sync_base(&repo, "main").await.unwrap();
    assert_eq!(result, BaseSync::NoRemote);
}

#[tokio::test]
async fn a_failed_fetch_is_its_own_outcome() {
    let r = setup();
    let before = r.head("develop");
    let gone = r.repos.root().join("billing-api-origin.git");
    std::fs::remove_dir_all(gone).unwrap();
    let result = runner().sync_base(&r.clone, "develop").await.unwrap();
    let BaseSync::FetchFailed(failure) = result else {
        panic!("expected a failed fetch, got {result:?}");
    };
    assert_eq!(failure.remote, "origin");
    assert!(
        failure
            .reason()
            .starts_with("git fetch --quiet origin failed (exit "),
        "{}",
        failure.reason()
    );
    assert_eq!(r.head("develop"), before, "nothing moved");
}

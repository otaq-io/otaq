//! M24's example end to end (T-0480; FIL-01…FIL-11; d0087, d0056, d0079): the project Files view's requests as the
//! app sends them, through the service, against the example's repositories.
//! - shop-web (SHOP, base `main`, Dev then Security): SHOP-19 (in Security) and SHOP-29 (in Dev) both edit
//!   `applyDiscount` in `src/lib/pricing.ts` (lines 47–58 and 41–52), so the file reads with both owners' lines and
//!   the note, SHOP-19 named first (furthest along); SHOP-22 waits on a permission with
//!   `src/middleware.ts` uncommitted; SHOP-36 on `release/2.3` changes `src/lib/vat.ts`, a file only there; the
//!   Wishlist epic (SHOP-40) has 40.1 merged and 40.2 writing `WishlistButton.tsx`; SHOP-12 has no worktree yet.
//! - SHOP-29's stand-in agent commits: the next tree and read show the commit.
//! - billing-api (BILL, base `develop`) with BILL-7 changing one file: everything reads develop.
//!
//! No agent tool runs; every folder is temporary (the stand-in's `HOME` too), and the stand-in runs no `otaq`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::files::EntryKind;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::project_files::{
    FileOwner, ProjectChanges, ProjectEntry, ProjectFile, ProjectSources, ProjectTree,
};
use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::client::{ClientError, request_once};
use otaq_core::domain::{Actor, Kind, Status};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::git::diff::{FileStatus, LineKind};
use otaq_core::git::testing::{commit, git};
use otaq_core::naming;
use otaq_core::protocol::{
    ErrorBody, MoveParams, ProjectFilesDiffParams, ProjectFilesPathParams, ProjectParams, Reply,
    Request,
};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, path_without_build_folders};

const NOTE: &str = "SHOP-19 and SHOP-29 both edit applyDiscount (lines 41\u{2013}58). Whichever merges second will \
                    rebase on the other.";

/// The stand-in `claude`. Every start is logged to `state/starts` (`<ticket> <arguments>`). A ticket's agent waits
/// for `state/go-<KEY>`, then adds `src/lib/tier.ts`, commits everything in its worktree and marks
/// `state/committed-<KEY>`; then it waits. Nothing typed into it is ever run.
fn stand_in(state: &Path) -> String {
    let state = state.display();
    let ticket_var = naming::ticket_env_var();
    format!(
        r#"
export HOME='{state}/home'
STATE='{state}'
KEY="${{{ticket_var}-}}"
printf '%s %s\n' "${{KEY:-project}}" "$*" >> "$STATE/starts"
[ -n "$KEY" ] || exec sleep 300
while [ ! -e "$STATE/go-$KEY" ]; do sleep 0.05; done
rm "$STATE/go-$KEY"
printf 'export const tier = 1\n' > src/lib/tier.ts
git add -A
git -c user.name=t -c user.email=t@t commit -qm "$KEY: tier" 2>> "$STATE/errors"
touch "$STATE/committed-$KEY"
exec sleep 300
"#
    )
}

/// shop-web's flow: Dev, Validate, Security (a check), QA, Done.
fn shop_flow() -> FlowFile {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../otaq-core/tests/flows/every-kind.yaml");
    let mut file = yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    file.flow.stages.retain(|s| s.id != "grill");
    file
}

fn launch(script: String) -> impl FnOnce(ServiceConfig) -> ServiceConfig {
    move |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: path_without_build_folders().into(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    }
}

/// `pricing.ts` on main: `applyDiscount` from line 30 to 66 (bodies indented, so git names the function each change
/// is in).
fn pricing_base() -> Vec<String> {
    let mut lines: Vec<String> = (1..30).map(|n| format!("// note {n}")).collect();
    lines.push("export function applyDiscount(total: Money, coupon?: Coupon): Money {".into());
    lines.extend((31..66).map(|n| format!("  const step{n} = {n}")));
    lines.push("}".into());
    lines
}

/// The base's lines with `first..=last` (1-based) rewritten; `0, 0` rewrites none.
fn rewrite(base: &[String], first: usize, last: usize) -> String {
    base.iter()
        .enumerate()
        .map(|(i, line)| match (first..=last).contains(&(i + 1)) {
            true => format!("{line} * 2\n"),
            false => format!("{line}\n"),
        })
        .collect()
}

struct World {
    w: TestService,
    state: tempfile::TempDir,
}

impl World {
    fn path(&self, file: &str) -> PathBuf {
        self.state.path().join(file)
    }

    fn shop(&self) -> PathBuf {
        self.w.repos.root().join("shop-web")
    }

    async fn create(&self, input: NewTicketInput) -> Ticket {
        self.w
            .board
            .create_ticket(&input, &Actor::User("me".into()))
            .await
            .unwrap()
    }

    /// Fills `project` with closed tickets until the next one gets `number`.
    async fn skip_to(&self, project: &str, number: u32) {
        loop {
            let t = self.w.create(project, "filler").await;
            self.w.force_status(&t, Status::ClosedNotMerged).await;
            if t.key.number + 1 >= number {
                break;
            }
        }
    }

    async fn ticket(&self, key: &str) -> Ticket {
        self.w.board.ticket(key).await.unwrap()
    }

    /// Waits (a minute at most) until `done` holds; else fails with the stand-in's starts and errors.
    async fn until(&self, what: &str, mut done: impl AsyncFnMut() -> bool) {
        let reached = tokio::time::timeout(Duration::from_secs(60), async {
            while !done().await {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        assert!(
            reached.is_ok(),
            "never: {what}\n---- starts\n{}\n---- errors\n{}",
            std::fs::read_to_string(self.path("starts")).unwrap_or_default(),
            std::fs::read_to_string(self.path("errors")).unwrap_or_default()
        );
    }

    /// `request` as the app sends it.
    async fn ask(&self, request: Request) -> Result<Reply, ErrorBody> {
        match request_once(&self.w.data.socket_path(), request).await {
            Ok(reply) => Ok(reply),
            Err(ClientError::Service(body)) => Err(body),
            Err(e) => panic!("{e}"),
        }
    }

    async fn tree(&self, project: &str, path: &str) -> ProjectTree {
        let request = Request::FilesProjectTree(ProjectFilesPathParams {
            project: project.into(),
            path: path.into(),
            base_only: false,
            worktree: None,
        });
        match self.ask(request).await.unwrap() {
            Reply::ProjectTree(tree) => *tree,
            other => panic!("{other:?}"),
        }
    }

    async fn read(&self, project: &str, path: &str) -> ProjectFile {
        let request = Request::FilesProjectRead(ProjectFilesPathParams {
            project: project.into(),
            path: path.into(),
            base_only: false,
            worktree: None,
        });
        match self.ask(request).await.unwrap() {
            Reply::ProjectFile(file) => *file,
            other => panic!("{other:?}"),
        }
    }

    async fn changes(&self, project: &str) -> ProjectChanges {
        let request = Request::FilesProjectChanges(ProjectParams {
            project: project.into(),
        });
        match self.ask(request).await.unwrap() {
            Reply::ProjectChanges(changes) => *changes,
            other => panic!("{other:?}"),
        }
    }

    async fn sources(&self, project: &str) -> ProjectSources {
        let request = Request::FilesProjectSources(ProjectParams {
            project: project.into(),
        });
        match self.ask(request).await.unwrap() {
            Reply::ProjectSources(sources) => *sources,
            other => panic!("{other:?}"),
        }
    }

    async fn stop(self) {
        for s in self
            .w
            .sessions
            .in_memory()
            .into_iter()
            .filter(|s| s.is_running())
        {
            let _ = self.w.sessions.stop(&s.id).await;
        }
        self.w.stop().await;
    }
}

fn entry<'a>(tree: &'a ProjectTree, name: &str) -> &'a ProjectEntry {
    tree.entries
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("{name} in {:?}", tree.entries))
}

/// Each owner as (key, file status, uncommitted, other base, its base).
fn owners(owners: &[FileOwner]) -> Vec<(String, FileStatus, bool, bool, &str)> {
    owners
        .iter()
        .map(|o| {
            (
                o.key.to_string(),
                o.file.status,
                o.uncommitted,
                o.other_base,
                o.base_branch.as_str(),
            )
        })
        .collect()
}

fn ranges(file: &ProjectFile, key: &str) -> Vec<(u32, u32)> {
    file.lines
        .iter()
        .find(|l| l.key.to_string() == key)
        .unwrap_or_else(|| panic!("{key}'s lines in {:?}", file.lines))
        .ranges
        .iter()
        .map(|r| (r.start, r.lines))
        .collect()
}

/// FIL-01…FIL-11: the example's shop-web and billing-api through `files.project_*`.
#[tokio::test(flavor = "multi_thread")]
async fn the_files_view_reads_the_example() {
    let state = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(state.path().join("home")).unwrap();
    let w = TestService::start_with(launch(stand_in(state.path()))).await;
    let world = World { w, state };
    world.w.add_project("shop-web").await;
    world.w.board.use_flow_for_tests("SHOP", &shop_flow()).await;
    let repo = world.shop();

    // main: pricing.ts and middleware.ts; release/2.3 also has vat.ts.
    let base = pricing_base();
    std::fs::create_dir_all(repo.join("src/lib")).unwrap();
    commit(&repo, "src/lib/pricing.ts", &rewrite(&base, 0, 0));
    commit(&repo, "src/middleware.ts", "export const auth = true\n");
    git(&repo, &["checkout", "--quiet", "-b", "release/2.3"]);
    commit(&repo, "src/lib/vat.ts", "export const vat = 0.2\n");
    git(&repo, &["checkout", "--quiet", "main"]);

    // SHOP-12: no worktree yet.
    world.skip_to("SHOP", 12).await;
    let dark = world.w.create("SHOP", "Add dark mode toggle").await;
    assert_eq!(dark.key.to_string(), "SHOP-12");
    // SHOP-19 (in its Security check): lines 47–58 of applyDiscount, committed.
    world.skip_to("SHOP", 19).await;
    let expiry = world.w.create("SHOP", "Coupon expiry").await;
    assert_eq!(expiry.key.to_string(), "SHOP-19");
    let expiry_wt = world.w.start_worktree(&expiry).await;
    commit(&expiry_wt, "src/lib/pricing.ts", &rewrite(&base, 47, 58));
    world
        .w
        .force_stage(&expiry, "security", Status::Checking)
        .await;
    // SHOP-22 waits on a permission: middleware.ts changed, uncommitted.
    world.skip_to("SHOP", 22).await;
    let login = world.w.create("SHOP", "Login rate limit").await;
    assert_eq!(login.key.to_string(), "SHOP-22");
    let login_wt = world.w.start_worktree(&login).await;
    std::fs::write(
        login_wt.join("src/middleware.ts"),
        "export const auth = true\nexport const limit = 5\n",
    )
    .unwrap();
    world.w.force_status(&login, Status::NeedsYou).await;
    // SHOP-29: dragged onto Ready for Dev, its stand-in Dev agent works on lines 41–52, uncommitted.
    world.skip_to("SHOP", 29).await;
    let tier = world.w.create("SHOP", "Tier discount").await;
    assert_eq!(tier.key.to_string(), "SHOP-29");
    let reply = world
        .ask(Request::TicketMove(MoveParams {
            key: "SHOP-29".into(),
            target: DropTarget::ReadyForDev,
            confirmed: true,
        }))
        .await
        .unwrap();
    assert!(matches!(reply, Reply::Ticket(_)), "{reply:?}");
    world
        .until("SHOP-29's agent works in its worktree", async || {
            let t = world.ticket("SHOP-29").await;
            t.status == Status::Working
                && t.work
                    .as_ref()
                    .and_then(|w| w.worktree.as_deref())
                    .is_some_and(|wt| Path::new(wt).is_dir())
        })
        .await;
    world
        .until("SHOP-29's stand-in starts", async || {
            std::fs::read_to_string(world.path("starts"))
                .unwrap_or_default()
                .lines()
                .any(|l| l.starts_with("SHOP-29 "))
        })
        .await;
    let tier_wt = PathBuf::from(
        world
            .ticket("SHOP-29")
            .await
            .work
            .unwrap()
            .worktree
            .unwrap(),
    );
    std::fs::write(tier_wt.join("src/lib/pricing.ts"), rewrite(&base, 41, 52)).unwrap();
    // SHOP-36 on release/2.3 changes vat.ts.
    world.skip_to("SHOP", 36).await;
    let vat = world
        .create(NewTicketInput {
            project: "SHOP".into(),
            title: "VAT rounding hotfix".into(),
            base_branch: Some("release/2.3".into()),
            ..Default::default()
        })
        .await;
    assert_eq!(vat.key.to_string(), "SHOP-36");
    let vat_wt = world.w.start_worktree(&vat).await;
    commit(&vat_wt, "src/lib/vat.ts", "export const vat = 0.21\n");
    world.w.force_status(&vat, Status::Working).await;
    // The Wishlist epic: 40.1 merged into the epic's branch, 40.2 writing WishlistButton.tsx on top.
    world.skip_to("SHOP", 40).await;
    let epic = world
        .create(NewTicketInput {
            project: "SHOP".into(),
            title: "Wishlist".into(),
            kind: Kind::Epic,
            ..Default::default()
        })
        .await;
    assert_eq!(epic.key.to_string(), "SHOP-40");
    let child = |title: &str| NewTicketInput {
        project: "SHOP".into(),
        title: title.into(),
        epic: Some("SHOP-40".into()),
        ..Default::default()
    };
    let store = world.create(child("Wishlist store")).await;
    let store_wt = world.w.start_worktree(&store).await;
    commit(
        &store_wt,
        "src/lib/wishlist.ts",
        "export const items = []\n",
    );
    let epic_branch = naming::branch("SHOP-40");
    let merged = git(&store_wt, &["rev-parse", "HEAD"]);
    git(
        &repo,
        &["update-ref", &format!("refs/heads/{epic_branch}"), &merged],
    );
    git(
        &repo,
        &["worktree", "remove", "--force", store_wt.to_str().unwrap()],
    );
    world.w.force_status(&store, Status::Merged).await;
    let button = world.create(child("Wishlist button")).await;
    let button_wt = world.w.start_worktree(&button).await;
    std::fs::create_dir_all(button_wt.join("src/components")).unwrap();
    commit(
        &button_wt,
        "src/components/WishlistButton.tsx",
        "export function WishlistButton() {}\n",
    );
    world.w.force_status(&button, Status::Working).await;
    let checkout = git(&repo, &["status", "--porcelain"]);

    // The top: `src` once, with every changed file below it; the counts.
    let top = world.tree("SHOP", "").await;
    assert_eq!(top.source.base_branch, "main");
    assert_eq!(top.source.base_sha, git(&repo, &["rev-parse", "main"]));
    let srcs: Vec<_> = top.entries.iter().filter(|e| e.name == "src").collect();
    assert_eq!(srcs.len(), 1);
    assert_eq!(
        (srcs[0].kind, srcs[0].changed_below),
        (EntryKind::Folder, 4)
    );
    let counts = top.counts.unwrap();
    assert_eq!(
        (
            counts.tickets,
            counts.changed,
            counts.needs_you,
            counts.needs_you_tickets,
            counts.same_file
        ),
        (5, 4, 1, 1, 1)
    );

    // src/lib: pricing.ts with both owners, vat.ts once, "not on main"; 40.1's merged file isn't there.
    let lib = world.tree("SHOP", "src/lib").await;
    let mut pricing_owners = owners(&entry(&lib, "pricing.ts").owners);
    pricing_owners.sort();
    assert_eq!(
        pricing_owners,
        [
            ("SHOP-19".into(), FileStatus::Modified, false, false, "main"),
            ("SHOP-29".into(), FileStatus::Modified, true, false, "main"),
        ]
    );
    let vats: Vec<_> = lib.entries.iter().filter(|e| e.name == "vat.ts").collect();
    assert_eq!(vats.len(), 1);
    assert!(vats[0].not_on_base);
    assert_eq!(
        owners(&vats[0].owners),
        [(
            "SHOP-36".into(),
            FileStatus::Modified,
            false,
            true,
            "release/2.3"
        )]
    );
    assert!(lib.entries.iter().all(|e| e.name != "wishlist.ts"));
    assert!(lib.entries.iter().all(|e| e.name != "tier.ts"));

    // pricing.ts: both owners' lines on main's numbering, and the note.
    let pricing = world.read("SHOP", "src/lib/pricing.ts").await;
    assert!(pricing.on_base);
    assert_eq!(pricing.text, rewrite(&base, 0, 0));
    assert_eq!(ranges(&pricing, "SHOP-19"), [(47, 12)]);
    assert_eq!(ranges(&pricing, "SHOP-29"), [(41, 12)]);
    let [note] = pricing.same_file.as_slice() else {
        panic!("one note: {:?}", pricing.same_file);
    };
    assert_eq!(note.note, NOTE);
    assert_eq!(note.base_branch, "main");
    // One owner's diff: SHOP-19's lines.
    let reply = world
        .ask(Request::FilesProjectDiff(ProjectFilesDiffParams {
            project: "SHOP".into(),
            path: "src/lib/pricing.ts".into(),
            ticket: "SHOP-19".into(),
        }))
        .await
        .unwrap();
    let Reply::FileDiff(diff) = reply else {
        panic!("{reply:?}")
    };
    assert_eq!((diff.file.added, diff.file.removed), (12, 12));
    assert!(diff.hunks.iter().all(|h| h.lines.iter().all(|l| {
        l.kind == LineKind::Context || l.text.contains("step4") || l.text.contains("step5")
    })));

    // middleware.ts: SHOP-22's uncommitted change.
    let middleware = world.read("SHOP", "src/middleware.ts").await;
    assert_eq!(
        owners(&middleware.owners),
        [("SHOP-22".into(), FileStatus::Modified, true, false, "main")]
    );
    assert!(middleware.owners[0].needs_you);
    assert!(middleware.same_file.is_empty());

    // vat.ts reads as SHOP-36's diff.
    let vat_file = world.read("SHOP", "src/lib/vat.ts").await;
    assert!(vat_file.not_on_base && !vat_file.on_base);
    assert_eq!(vat_file.diff_of.as_ref(), Some(&vat.key));
    let vat_diff = vat_file.diff.unwrap();
    let changed: Vec<_> = vat_diff.hunks[0]
        .lines
        .iter()
        .map(|l| (l.kind, l.text.as_str()))
        .collect();
    assert_eq!(
        changed,
        [
            (LineKind::Removed, "export const vat = 0.2"),
            (LineKind::Added, "export const vat = 0.21"),
        ]
    );

    // WishlistButton.tsx: the running child is its owner, tagged with the epic's branch.
    let button_file = world
        .read("SHOP", "src/components/WishlistButton.tsx")
        .await;
    assert!(button_file.not_on_base);
    assert_eq!(
        owners(&button_file.owners),
        [(
            button.key.to_string(),
            FileStatus::Added,
            false,
            true,
            epic_branch.as_str()
        )]
    );

    // All changes by ticket: SHOP-22 (needs you) first, then flow order, other bases last.
    let changes = world.changes("SHOP").await;
    let groups: Vec<_> = changes
        .groups
        .iter()
        .map(|g| {
            (
                g.ticket.key.to_string(),
                g.ticket.needs_you,
                g.files
                    .iter()
                    .map(|e| e.file.path.as_str())
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    assert_eq!(
        groups,
        [
            ("SHOP-22".to_owned(), true, vec!["src/middleware.ts"]),
            ("SHOP-29".to_owned(), false, vec!["src/lib/pricing.ts"]),
            ("SHOP-19".to_owned(), false, vec!["src/lib/pricing.ts"]),
            ("SHOP-36".to_owned(), false, vec!["src/lib/vat.ts"]),
            (
                button.key.to_string(),
                false,
                vec!["src/components/WishlistButton.tsx"]
            ),
        ]
    );
    assert_eq!(changes.counts.same_file, 1);

    // The picker: the epic with 40.1 merged and 40.2 on top; tickets with a worktree; SHOP-12 without, not an owner.
    let sources = world.sources("SHOP").await;
    let epics: Vec<_> = sources
        .epics
        .iter()
        .map(|e| {
            (
                e.key.to_string(),
                e.branch.as_str(),
                e.merged.clone(),
                e.on_top.clone(),
            )
        })
        .collect();
    assert_eq!(
        epics,
        [(
            "SHOP-40".to_owned(),
            epic_branch.as_str(),
            vec![store.key.clone()],
            vec![button.key.clone()]
        )]
    );
    let tickets: Vec<_> = sources
        .tickets
        .iter()
        .map(|t| (t.key.to_string(), t.other_base, t.files))
        .collect();
    assert_eq!(
        tickets,
        [
            ("SHOP-22".to_owned(), false, 1),
            ("SHOP-29".to_owned(), false, 1),
            ("SHOP-19".to_owned(), false, 1),
            ("SHOP-36".to_owned(), true, 1),
            (button.key.to_string(), true, 1),
        ]
    );
    let none: Vec<_> = sources.no_worktree.iter().map(|t| t.key.clone()).collect();
    assert_eq!(none, std::slice::from_ref(&dark.key));

    // SHOP-29's agent commits: the next tree and read show it.
    std::fs::write(world.path("go-SHOP-29"), "").unwrap();
    world
        .until("SHOP-29's agent commits", async || {
            world.path("committed-SHOP-29").exists()
        })
        .await;
    let lib = world.tree("SHOP", "src/lib").await;
    assert_eq!(
        owners(&entry(&lib, "tier.ts").owners),
        [("SHOP-29".into(), FileStatus::Added, false, false, "main")]
    );
    let pricing = world.read("SHOP", "src/lib/pricing.ts").await;
    let committed = pricing.owners.iter().find(|o| o.key == tier.key).unwrap();
    assert!(!committed.uncommitted);
    assert_eq!(ranges(&pricing, "SHOP-29"), [(41, 12)]);
    assert_eq!(pricing.same_file[0].note, NOTE);

    // Read-only: the user's checkout is as it was.
    assert_eq!(git(&repo, &["status", "--porcelain"]), checkout);
    assert_eq!(git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]), "main");

    // billing-api on develop: BILL-7 changes one file; every label says develop.
    let bill_repo = world.w.repos.repo("billing-api", 1);
    git(&bill_repo, &["checkout", "--quiet", "-b", "develop"]);
    commit(&bill_repo, "invoice.rs", "fn invoice() {}\n");
    git(&bill_repo, &["checkout", "--quiet", "main"]);
    world
        .w
        .board
        .add_project(&AddProject {
            path: bill_repo.display().to_string(),
            base_branch: Some("develop".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    world.skip_to("BILL", 7).await;
    let invoice = world.w.create("BILL", "Invoice totals").await;
    assert_eq!(invoice.key.to_string(), "BILL-7");
    let invoice_wt = world.w.start_worktree(&invoice).await;
    commit(&invoice_wt, "invoice.rs", "fn invoice() { total() }\n");
    world.w.force_status(&invoice, Status::Working).await;
    let bill = world.tree("BILL", "").await;
    assert_eq!(bill.source.base_branch, "develop");
    assert_eq!(
        bill.source.base_sha,
        git(&bill_repo, &["rev-parse", "develop"])
    );
    assert_eq!(
        owners(&entry(&bill, "invoice.rs").owners),
        [(
            "BILL-7".into(),
            FileStatus::Modified,
            false,
            false,
            "develop"
        )]
    );
    assert_eq!(bill.counts.unwrap().tickets, 1);
    assert_eq!(world.sources("BILL").await.source.base_branch, "develop");
    assert_eq!(
        world.read("BILL", "invoice.rs").await.text,
        "fn invoice() {}\n"
    );
    // shop-web's view never shows billing-api's tickets.
    assert_eq!(world.tree("SHOP", "").await.counts.unwrap().tickets, 5);

    world.stop().await;
}

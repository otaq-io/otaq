# Plan · git-hooks

- **Ticket:** none
- **Date:** 2026-11-11
- **Decisions:** 026, 027, 028, 029, 030, 031, 032, 033, 034, 035, 036, 038, 039
- **Tech spec:** none (the decisions above; each issue's evidence is in ideas/ux-rethink/build/release-test/issues-hooks/, the exploration in ../RESULTS-hooks.md)
- **Orchestrator prompt:** tuq/prompts/git-hooks-orchestrator.md

<!-- tuq:plan:start -->

Otaq works with a project's git hooks: the 11 issues of the Windows hooks exploration
(ideas/ux-rethink/build/release-test/issues-hooks/, NN-….md, evidence in ../RESULTS-hooks.md and ../shots-hooks/)
are fixed before the release, following decisions 026–036, 038 and 039 (037 was dropped). It is built in 16 pieces,
one behaviour each. Piece 1 is the groundwork most others need: Otaq's own git steps know which hook is running or
refused, with its whole output. The ways the user sees that (a refusal on the ticket, the running hook and Stop, a
terminal hook, post-merge, the PR messages' check) wait for it. Everything else runs side by side: never rebasing,
the message rules, the kept message, the push rules and check, the clean failed start. The temporary save waits
for the no-rebase change, which decides where it is still needed. Built on the release-fixes branch and merged into
it locally. The agent in charge follows tuq/prompts/git-hooks-orchestrator.md.

## Pieces

1. Otaq's git steps know which hook runs or refused, with its whole output — waits for nothing
   Groundwork for 2, 3, 5, 11, 14, 15 and 16, in otaq-core only: no screen. Today every git call goes through
   `Git::run` (crates/otaq-core/src/git/mod.rs): stdin /dev/null, no timeout, and a failure becomes
   `git <args> failed (exit N): <stderr>`, which callers cut to git's last line. Make each of Otaq's own git steps
   (start's `worktree add` in crates/otaq-core/src/start/worktree.rs, the reopened ticket's update and checkpoint in
   git/checkpoint.rs and start/sequence.rs, Local merge in git/merge.rs, PR rewording in git/reword.rs and
   pr/flow.rs, helpers/merge.rs) able to say: which step it is on, which hook is running right now and since when,
   and on a refusal which hook refused and its **full** output. git doesn't name the hook for every refusal
   (commit-msg and pre-commit both end in "git commit failed"); finding out reliably is this piece's job. The output
   has paths shortened (`~/…`, `~\…`) so the user's name never shows (issue 04: the toast showed the full Windows
   hook path). Issues 04 and 07; decisions 026, 027.
   Check: unit tests on a temporary repo with failing pre-commit, commit-msg, pre-rebase and post-checkout hooks:
   each refusal names the right hook and carries the hook's whole output with the home path shortened; a slow hook is
   reported as running, by name, while it runs.
2. A refused Approve & merge shows on the ticket — waits for 1
   Decision 026, issue 04. When a hook refuses any step of Approve & merge (crates/otaq-core/src/git/merge.rs via
   review/mod.rs and board/merge.rs), whether the Merging dialog is open or was closed: the card gets a Needs you
   flag naming the hook and the step ("pre-commit hook refused the merge"); the ticket's sheet shows which hook, its
   full output (shortened paths, from piece 1) and the way on (edit the message and retry); History keeps the
   refusal with its output. The toast, if any, only points to the ticket. Today the refusal is a toast ("Approve &
   merge for HOOK-1 didn't go through · git commit failed: …") and a WARN in the log, nothing on the ticket. The
   automatic delivery already has a turquoise "delivery stopped" state with Retry merge (review/delivery.rs,
   `delivery_stopped`): use one way for both. One behaviour across core, view models (card.rs, approve.rs, the sheet)
   and the screens, on purpose.
   Check: with a commit-msg hook that refuses, Approve & merge, close the dialog: the card shows Needs you, the sheet
   shows the hook's whole message and Retry, History lists it; main, branch and worktree are unchanged.
3. A refused start shows on the ticket — waits for 1
   Decision 026, issue 04 (and 01's wrong-hook text). When a hook refuses a ticket's start (post-checkout on
   `worktree add`, or the reopened ticket's update from main and its save of uncommitted files; start/worktree.rs,
   start/sequence.rs), the card's start-failed state names the hook and step, the sheet shows its full output and
   Retry start, and History keeps it. Today the error said "Your **pre-commit** hook rejected Otaq's temporary
   checkpoint" when the commit-msg hook did, and dropped the hook's message. One behaviour across core, view models
   and screens, on purpose.
   Check: a failing post-checkout hook: the card says the post-checkout hook refused the start, the sheet shows its
   output, History lists it.
4. Otaq's merges and agents' commits follow the project's message rules — waits for nothing
   Decision 028, issue 01. Before writing the merge message (the delivery agent's message for Approve & merge,
   `otaq pr message`, crates/otaq-core/src/pr/message.rs and the prompt it is given) and before committing
   (plugin/skills/commit/SKILL.md step 4 says to start each message with the ticket key), the agent looks for the
   project's message rules: CONTRIBUTING and similar docs, a commitlint-style config, the repository's hooks
   (`.git/hooks`, `core.hooksPath` such as husky's `.githooks`), the Agent instructions, recent commits on the
   target. It follows what it finds (`feat: add subtraction (HOOK-1)`), keeping the key where the rule allows; with
   nothing found, the key leads as today. Also the open-pr and push skills' per-commit messages. No template
   setting. The plain message Otaq uses when no writer ran stays as it is.
   Check: a test project with a commitlint config and a `feat:`/`fix:` commit-msg hook: the proposed merge message
   and an agent's commit pass the hook on the first try; in a project with no rules the key still leads.
5. The real step and the running hook show while a git step runs — waits for 1
   Decision 027, issue 07. While Approve & merge or a start runs, the card and (while open) the Merging dialog show
   the step git is really on, not step 1 "Fetch main and bring it up to date" for the whole run
   (crates/otaq-view/src/view_models/approve.rs `merging_steps`, crates/otaq-desktop/ui/src/screens/review/Dialogs.tsx
   "merging", card.rs/card_line.rs "Merging…", "Starting…"). While a hook runs they say which one and for how long:
   "running your pre-commit hook · 40 s". The steps follow piece 8's no-rebase order once it is merged (no "Rebase
   onto main" step). One behaviour across core, view models and screens, on purpose.
   Check: with a 40 s pre-commit, Approve & merge: the dialog and the card move through the steps and show "running
   your pre-commit hook" with a growing time; with the dialog closed the card still shows it.
6. Stop on a running git step, with no time limit — waits for 5
   Decision 027, issue 07. While Approve & merge or a start runs, Stop is offered on the ticket and in the Merging
   dialog. A stopped step leaves main, the branch and the worktree as they were (as a refusal does) and the ticket
   stays where it was. Otaq never stops a hook itself: no timeout. The post-merge hook has no Stop (piece 15).
   The Merge check already has a Stop (d0120, `stop_merge_check` in crates/otaq-desktop/ui/src/screens/sheet/
   actionRoutes.ts, `CheckRun::Stopped` in git/merge.rs): extend that to every step. A hanging hook today keeps
   "Merging…" up forever (`Git::run` has only kill-on-drop).
   Check: a pre-commit hook that sleeps forever: Approve & merge, Stop from the card: the ticket is back in Review,
   main and the branch unchanged, no git process left; the same for a hanging post-checkout during a start.
7. An edited merge or PR message is kept until the ticket lands — waits for nothing
   Decision 031, issue 05. The user's edit to the merge message in Approve & merge, or to the PR title and
   description (and the per-commit messages) in Approve & open PR, is kept on the ticket: after a refusal, a closed
   dialog and an app restart, reopening shows the edit. It goes when the ticket lands or when the user picks a new
   **Use the proposed message**, which puts back the agent's proposal. A Send back keeps it.
   crates/otaq-view/src/view_models/approve.rs, pr_dialog.rs, ui/src/screens/review/Dialogs.tsx, and where the
   proposal is stored (pr/message.rs). One behaviour across store, view models and dialogs, on purpose.
   Check: edit the message to `feat: …`, let a hook refuse, reopen: the edit is there; restart the app: still there;
   Use the proposed message brings the proposal back.
8. Otaq brings a branch up to date by merging main in, never by rebasing — waits for nothing
   Decision 029, issue 02. Everywhere a branch is brought up to date, main is merged into it; nothing rebases: Local
   merge's free rebase (git/merge.rs step 3, then squash as today), the reopened ticket's update
   (start/sequence.rs `rebase_reopened`, git/checkpoint.rs `rebase_with_checkpoint`), a helper's branch onto the
   ticket's branch (helpers/merge.rs, and its "Rebase onto it (`git rebase …`)" hand-back sentence), the restack
   skill (plugin/skills/restack/SKILL.md tells the agent to `git rebase`), and conflict reasons
   (board/conflicts.rs). A conflict still merges nothing and goes to the agent as today. Approve & merge still lands
   one squashed commit with the one message. No setting. The merge commit made on the branch runs the project's
   hooks: when one refuses it, it is a refusal like any other (026). Review's "changed since you reviewed it" and
   the reviewed head (`record_otaq_rebase`, GIT-05) must still be right after a merge of main into the branch.
   Check: a pre-rebase hook that always refuses: with main moved on, Approve & merge lands; a reopened ticket starts;
   a conflict goes to the agent, whose restack merges main in; the hook log shows pre-rebase never ran.
9. Otaq's temporary save of unfinished files skips the hooks — waits for 8
   Decision 030, issue 01. Where Otaq still saves a worktree's uncommitted files for a moment around an update
   (today the `wip: paused by Otaq` checkpoint in git/checkpoint.rs, used by the reopened ticket's start; after
   piece 8, find which updates still need it), the project's hooks don't run on that save. It is undone right after
   and the files come back as they were. The only exception to "hooks always run": every commit that lands still
   runs them, and the module's comments say so. Today a commit-msg rule refuses it every time, so a reopened ticket
   with uncommitted files can never start.
   Check: a commit-msg hook that refuses everything but `feat:`, and a lint pre-commit that refuses a TODO: reopen a
   ticket whose worktree has uncommitted files with a TODO: it starts, and the files are back uncommitted.
10. A failed start leaves nothing half-made; Retry start starts over — waits for nothing
   Decision 035, issue 08. When a new ticket's first start fails at any step (a post-checkout hook's refusal among
   them), no half-made worktree stays registered, so Retry start does the whole start again and every start hook
   runs again. Today git has already made the worktree, Retry start reuses it, skips `worktree add` and the hook
   never runs again (crates/otaq-core/src/start/worktree.rs, `worktree add --quiet` and the already-registered
   path). A reopened ticket's worktree holds earlier work and is never removed by a failed start.
   Check: a post-checkout that fails once and then passes: the start fails, Retry start runs the hook again (hook
   log) and the ticket starts.
11. A hook's changes to the landing commit are said — waits for 1
   Decision 032, issue 06. When a hook (a formatter) changes the files of the commit Approve & merge makes, it lands
   as the hook left it, and the merged message and History say which hook changed how many files ("Your pre-commit
   hook changed 2 files"), with **Show changes**: the difference between the reviewed tree and what landed. The Merge
   check doesn't rerun for it. Nothing extra when no hook changed anything. git/merge.rs (compare the squash commit's
   tree with the branch's), the merged toast and History. One behaviour across core and the screens, on purpose.
   Check: a pre-commit that adds a header line to each staged .js: Approve & merge says "Your pre-commit hook changed
   2 files", Show changes shows those 2 lines; with a hook that changes nothing, no such line.
12. Agents push when the user asks, or when delivering — waits for nothing
   Decision 033, issue 03. The skills say "Don't push unless the user asks" instead of "Never push": a request in the
   ticket, a comment or Tell now is enough, with no "override" words. plugin/skills/work-ticket/SKILL.md ("Never
   push, never open a PR, never merge"), commit (step 6 "Never push."), address-review, restack, and the push and
   open-pr skills. When an agent pushes, it waits for the push to finish however long the pre-push hook takes,
   reports the real result, never bypasses a refusal (`--no-verify`) and asks the user how to go on. Keep the skills'
   other "never" rules (never open a PR or merge on their own).
   Check: a Codex agent asked in a comment to push does so with no override words; with a 50 s pre-push it reports
   success only after the push finished; with a failing pre-push it asks and doesn't bypass.
13. Otaq checks that a delivery push landed — waits for nothing
   Decision 034, issue 03. When the delivery agent of Approve & open PR or Push to PR reports the push or PR done
   (`otaq pr opened`, the push skill's end), Otaq checks the remote has the ticket's branch at the commit it should.
   If not, the ticket shows Needs you: "The branch isn't on the remote as expected", with what the remote has.
   crates/otaq-core/src/pr/verify.rs already fetches the branch and compares trees after a push, but returns nothing
   when the branch isn't on origin: that case now is the mismatch. After a refused push the reworded messages stay on
   the local branch and the user's edits are kept, so trying again only pushes again. The agent still pushes.
   Check: a fake delivery that reports `otaq pr opened` without pushing: the ticket shows Needs you; a real push
   shows nothing extra.
14. A hook that asks in the terminal is named as such — waits for 2, 3
   Decision 036, issue 09. When a hook fails because it needs a terminal (`exec < /dev/tty`, "/dev/tty: No such
   device or address"), the refusal on the ticket (pieces 2, 3) says above its output: "Your pre-commit hook asks a
   question in the terminal; Otaq can't answer it. Make the hook skip its question when there's no terminal, or do
   this step by hand." (with the real hook name). Otaq doesn't give hooks a terminal. Agents hitting the same failure
   say the same when they ask the user (the commit, push and open-pr skills).
   Check: an interactive pre-commit: Approve & merge's refusal on the ticket carries that sentence with
   "pre-commit".
15. The post-merge hook in the user's folder is shown, its output kept — waits for 5
   Decision 038, issue 11. Local merge's last step moves main in the user's own repository folder (`merge --ff-only`
   in git/merge.rs), which runs their post-merge hook there. While it runs, the card and the Merging dialog show
   "running your post-merge hook in ~/shop-web · 20 s" (piece 5's way). No Stop: the merge has landed. Its output,
   passed or failed, is a line in the ticket's History; a failure doesn't change the ticket (merged, no Needs you).
   Nothing extra without a post-merge hook.
   Check: a 20 s post-merge that prints a line: the dialog shows it running in the shortened folder, History keeps
   the printed line; a failing one leaves the ticket merged with the failure in History.
16. Reworded PR messages go through the commit-msg hook first — waits for 2, 7
   Decision 039, issue 01. In Approve & open PR, before Otaq rewrites the branch's commit messages
   (git/reword.rs uses `git commit-tree`, which runs no hook; pr/flow.rs), each confirmed message goes through the
   project's commit-msg hook. A refusal rewrites and pushes nothing, shows on the ticket like piece 2's (the message
   and the hook's output), and the user's edits stay (piece 7) to fix and retry. All passing: rewrite as today and
   the delivery agent pushes. No commit-msg hook: as today.
   Check: a `feat:`/`fix:` commit-msg hook: confirm a PR with one message "update stuff": nothing is rewritten, the
   ticket shows the hook's refusal for that message; fix it to `fix: …` and the PR goes on.

<!-- tuq:plan:end -->

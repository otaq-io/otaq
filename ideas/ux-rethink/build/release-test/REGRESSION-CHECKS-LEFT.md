# Checks left for the regression test (after release round 6)

Logged 2026-11-12, when the maintainer stopped re-testing round 6 to move on to question-box. Each item was not tried
in the real app on the release build, or only by a test or a piece agent's own build. Cover them in the next regression
test. Sources: `RESULTS-retest-7.md` (and its follow-ups), `issues-round6/README.md`, the round 6 pieces' hand-backs.

## Claude Code (Windows)
- Push to PR, Retry push and a push-skill prompt reading Needs you in Review (round 6 pieces 1, 2). Blocked in re-test 7
  by the test data: hooksproj's `local-pr` skill is only in `.agents/skills/`, where Claude Code doesn't look (NEW-13);
  put it where Claude Code finds it (`.claude/skills/`) first.
- A writer prompt open at Approve & merge into a conflict: Needs you in Dev, restack after the answer, merged (round 6
  piece 6 on Claude Code). Claude Code runs in auto mode by default: switch the session to manual (Shift+Tab) to get a
  prompt.
- The written message reaching the Approve and Open PR dialogs (needs round 6 piece 7, the 1 KB loss).
- Resume from a "signed out" hold after signing in, and a login expiring mid-session → Sign in… (round 5 piece 7).

## NEW-12 and NEW-10, once fixed (stopped pieces 7 and 8; see issues-round6/README.md)
- NEW-12: a long message (the writer's request, 3 KB on one line and on several) typed into Claude Code on Windows
  arrives whole, and the written message fills the Approve dialog.
- NEW-10 (decision 048): Ctrl+V pastes text into Claude Code and Codex (the unsent-text footer shows), an image still
  pastes in Claude Code, PowerShell pastes once.

## Codex (Windows), covered only by tests so far
- Retry push in the app after a failed pre-push, and an automatic PR whose skill hits a prompt (round 6 piece 2).
- The marked card's full wording in Review (the sheet and PR dialog covered it).
- A failed merge check (not a conflict) sending the ticket to Dev with the writer's prompt open (decision 010's refined
  case).
- A conflict hand-back while the agent limit is full (Queued): the writer prompt stays calm until the next signal
  (known issue; confirm it's still only that).
- Answer sessions: an OS notification for the answering ticket's prompt (round 6 piece 3); a dirty answer worktree kept
  for Cleanup, and a ticket reopened while its answer session runs (round 6 piece 4).
- Ctrl-clicking a link open while a selection shows (round 6 piece 5).

## Other OSes and providers
- macOS and Linux: no round 6 re-test ran there. At least the core path on each, with Codex's `$otaq:push` /
  `$otaq:restack` (round 6 piece 1's space) and Claude Code's long-message typing (piece 7).
- The real GitHub (gh) and Azure DevOps (az) PR paths; only the local bare remote was tried.

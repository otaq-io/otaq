# Re-test 8 · fix check 2 (NEW-b1, b3, d1–d5, c1, base-sha) on Windows · results (Windows 11 ARM64 VM, build fae1fd78)

Date: 2026-11-12, from 17:58 VM time. Build: `Otaq_0.1.0_arm64-setup-fae1fd78.exe` (release-fixes fae1fd78, local on
the Mac). Data: the normal release-test data (`otaq-reltest\home`, `launch.cmd` via `otaqtest`). Screenshots
`shots-retest-8/fix2-*.jpg`.

Privacy: no screenshot shows the Windows user name (terminal paths read `~\otaq-reltest\…`), a login, email or
token. The clipboard was used twice (Noyan requests in Edit flow's box, not an agent terminal): each time Set-Clipboard
+ Get-Clipboard in the same exec, then checked on screen before sending; nothing foreign arrived. No code changed,
nothing committed in the repo on the Mac, nothing pushed anywhere. Agents: Claude Code only (Architect) for the ticket
and Noyan; the hooksproj merge-message writer (Codex Developer) started by itself when HOOK-13 reached Review.

## Summary
PASS 8 · PARTLY 1 · KNOWN/unchanged 1 · new problems 1 (no blocker) · notes 5. Every fix holds in the real app.

| Check | Result |
|---|---|
| Install fae1fd78 (`/S`), only the test app + its service stopped, About | **PASS** · "Version 0.1.0 · build fae1fd78" |
| NEW-b1 · only agent waiting (agent limit off): Running leaves, Pause stays where it was; a click there opens Pause | **PASS** (the usage chip now slides into Running's place; palette Pause rows / Running view: see notes) |
| NEW-b3 · several-answers box: 1–4 tick/untick, no tab switch, ticks kept | **PASS** with focus in the box (after a click on a row); with focus elsewhere digits still switch tabs, as the single-answer box does; a tab click and back resets the ticks (NOTE-f1) |
| NEW-d1 · Edit flow's box at the 55 % cap: question keeps its lines, options scroll | **PASS** (bold question + ~4 lines of context; options in their own scroll with Something else below; Send in view) |
| NEW-d2 · the same question above Noyan's terminal | **PASS** (question + context at the top, options scroll, "1–4 pick", Answer in terminal / Send answer below the options, none over an option) |
| NEW-c1 (re-look) · long question on a ticket's Overview, 908-px window | **unchanged (NEW-c1 still open)**: Send ~36 CSS px below the fold until the sheet scrolls |
| NEW-d3 · Ask for changes keeps the shown proposal while Noyan redrafts; Stop leaves the last proposal | **PASS** |
| NEW-d4 · drafting row, Needs you header, proposal bar, Ask for changes name the agent | **PASS**: "Architect · Claude Code is drafting…", "Needs you · Architect · Claude Code asks", "Architect · Claude Code's suggestion for shop", "Ask for changes to Architect · Claude Code's proposal…"; after Stop "Claude Code's suggestion" (as the commit says). Permission line NOT TESTED (Claude in auto mode never prompted) |
| NEW-d5 · Cleanup deletes a finished ticket's worktree → `otaq prime --project` no longer lists it | **PASS** (SHOP-33, SHOP-46); SHOP-5's line, deleted by the old build, stays (NOTE-f2) |
| base-sha · branch rebased by hand onto newer main: Changes shows only the ticket's changes | **PASS** (HOOK-10: 1 file, +3) |
| base-sha · main merged into the branch by hand: Changes, footer, commit count, Open PR's commits | **PASS** (HOOK-13: 1 file +1; "2 commits" = own commit + the merge commit; Open PR lists those two, not main's) |
| The other Otaq (`~\otaq-qbox`) untouched | **PASS** (PIDs 8380/18580 alive, no input sent to it) |

## Running log
- 17:58 Install: stopped only the test app (PID 17992, `%LOCALAPPDATA%\Otaq\otaq-app.exe`) and `taskkill /T` its
  service (PID 19780, `otaq-reltest\home\runtime\0cc7efc5…`; HOOK-10's idle Claude session and other children went with
  it). qbox PIDs 8380/18580 left. Setup `/S` exit 0: `otaq-app.exe` 17:57 and `otaq.exe` 17:54 (both new). Started via
  `schtasks /run /tn otaqtest`: app PID 13936, service PID 22660 from runtime `fae1fd782a68…`. No restart dialog this
  time; a toast "HOOK-4 · Delivery stopped · The branch isn't on the remote as expected" came at start (an old state).
- About: **"Version 0.1.0 · build fae1fd78 · Built from source"**. `fix2-about-fae1fd78.jpg`
- 18:00 base-sha, rebase: HOOK-10 (Review, PR #3; 3 own commits on f4da09c, main 3 ahead at 69ae147) before: Changes
  "1 file", README +3. Rebased `otaq/HOOK-10` by hand in its worktree onto main (`git rebase --no-verify main`; the
  hooksproj test pre-rebase hook refuses otherwise; README conflict resolved keeping both lines) → 819ff8d on 69ae147.
  Reopened the sheet: **Changes 1 file · "1 file · +3 against main · the PR will look like this"**, only README's three
  HOOK-10 lines green, main's HOOK-11 lines as context, no `.claude` deletion (main's 69ae147). `fix2-base-hook10-rebased-changes-1-file.jpg`
- 18:02 HOOK-13 "R8f2-base-merge" created by CLI (Dev=Architect, Validate=Architect; it asks "go on?" first) and
  started from its card (Start). A Windows Firewall prompt for the new runtime's `otaq` came: Cancel.
- NEW-b1: while Architect worked, title bar "Codex · 16% left | 1 running | Pause" (Pause at x≈1160/1280).
  `fix2-b1-running-chip-with-pause-while-working.jpg`. Agent asked its question (the only agent, waiting): Running left,
  **Pause stayed at the same x**; the usage chip moved right into Running's place. `fix2-b1-agent-asks-running-gone-pause-same-place.jpg`,
  `fix2-b1-titlebar-zoom-pause-same-place.jpg`. A click where Pause was opened the Pause menu ("Pause hooksproj · 1
  agent stops where it is…", "Pause all projects · 1 agent in 1 project") `fix2-b1-click-where-pause-was-opens-pause.jpg`.
  A click where "1 running" had been now lands on the usage chip and opens the Running popover reading **"AGENTS
  RUNNING 0 · Nothing running. A started ticket begins once an agent is free and the tickets it waits for are done."**
  `fix2-b1-click-where-running-was-opens-usage-nothing-running.jpg`. Palette (Ctrl+K) "pause": **"Nothing matches
  pause"** `fix2-b1-palette-no-pause-rows.jpg`. Ctrl+Shift+U: opens the same popover (from the usage chip), "Nothing running".
- NEW-b3: `otaq ask human --multiple` (r8c-q-multi, recommended 1,3) asked on HOOK-13 by CLI with OTAQ_TICKET.
  Overview: checkboxes 1 and 3 ticked, "Pick any · 1–4 tick · ↵ sends". With focus on the page (sheet just opened, or
  after a click on the question's text) "2" still **switches to Session** (`fix2-b3-digit-without-focus-in-box-switches-to-session.jpg`);
  the single-answer box does the same (checked later: "2" after a scroll → Session). After a click on a row (focus in
  the box): **1 unticked Unit tests, 1 ticked it again, 2 unticked Lint, 3 unticked Price rounding, 4 ticked Something
  else and opened its text field; no tab switch, no reset.** `fix2-b3-digit-1-unticks-unit-tests.jpg`,
  `fix2-b3-digit-2-unticks-lint-no-tab-switch.jpg`, `fix2-b3-digit-4-ticks-something-else.jpg`. Clicking Session and
  back to Overview resets the ticks to the recommended ones (1, 3) and drops Something else. `fix2-b3-tab-click-back-resets-ticks.jpg`
- NEW-c1 (re-look): r8c-q-long asked on HOOK-13. Overview: text capped and scrolls (own scrollbar), options with notes
  below; Something else's row is cut at the sheet's bottom and **Send is below the fold**; one notch of the sheet's
  wheel brings it into view (~24 screenshot px ≈ 36 CSS px below). Same as part c. `fix2-c1-long-question-overview-send-below-fold.jpg`,
  `fix2-c1-long-question-scrolled-send-in-view.jpg`. (The wheel over the options did not scroll the sheet.)
- Answered "Something else: Go on" with ↵: the agent got **"Answer to your question: Go on"** once (transcript), but
  a toast also came: **"The answer to HOOK-13 wasn't sent · HOOK-13 isn't waiting for an answer"** → NEW-f1.
  Architect appended the line (f725be3), Validate (Architect) passed in round 1, HOOK-13 → Review "Ready for you"; the
  merge-message writer (Developer, Codex) started at Review.
- 18:06 base-sha, merge: commit b8c0a26 on hooksproj main (docs/r8f2-main.md, test setup), then `git merge
  --no-verify --no-edit main` in HOOK-13's worktree (merge commit 12a438d). Reopened: **Changes 1 file · "1 file · +1
  against main · Validate passed · round 1"**, footer "1 file · +1", docs/r8f2-main.md not listed.
  `fix2-base-hook13-main-merged-in-changes-1-file.jpg`. Approve menu: "Done, don't merge · Close the ticket and keep
  its **2 commits** out of main" (f725be3 + the merge commit; main's b8c0a26 not counted) `fix2-base-hook13-menu-2-commits.jpg`.
  **Approve & open PR** dialog: "Commit messages (2 commits)": "fix: HOOK-13: append fix check 2 line to README…" and
  "Merge branch 'main' into otaq/HOOK-13"; main's commit not listed. `fix2-base-hook13-open-pr-2-commits-no-main-commit.jpg`.
  Cancelled (nothing pushed). Its first commit row runs the subject into the trailer: "…to READMECo-Authored-By: Claude" (NOTE-f3).
- 18:08 NEW-d5: prime --project (shop) before listed worktrees for SHOP-5 (deleted in part d by the old build),
  SHOP-33 (dirty leftover) and SHOP-46 (empty folder from part e). Settings → Data → Cleanup listed SHOP-46 ("no ticket
  uses this worktree") and SHOP-33 ("Closed · not merged with uncommitted changes · README.md"); deleted both ("Cleaned
  up 2 items"). `fix2-d5-cleanup-shop33-shop46-listed.jpg`, `fix2-d5-cleanup-done-2-items.jpg`. After: prime lists
  **SHOP-33 and SHOP-46 without a worktree line**; folders gone; `git worktree list` only main + scratch. SHOP-5's stale
  line (deleted by the old build) is still there (NOTE-f2).
- 18:10 Noyan uses → Architect (Settings → Agents). shop → Edit flow; box "Noyan · Architect · Claude Code". Pasted
  (checked on screen) a request to run `..\r8c-ask-long.ps1` first, then propose a Docs stage after Dev; ↵.
- NEW-d4: drafting row **"Architect · Claude Code is drafting a change to shop's flow… Show Stop"** (the conversation
  was later named "Add Docs stage"). `fix2-d4-drafting-row-names-architect.jpg`
- NEW-d1: the long question (3 options with notes): box at ~55 % (columns shrink above), header **"Needs you · Architect
  · Claude Code asks"**, the bold question and four lines of context visible in their own scroll, the options in a
  second scroll (Something else reached by the wheel), "1–4 pick · ↵ sends", Answer in terminal, Send answer in view.
  `fix2-d1-edit-flow-long-question-55pct.jpg`, `fix2-d1-d4-edit-flow-box-zoom-question-lines-options-scroll.jpg`
- NEW-d2: Answer in terminal opened Noyan's panel ("Waiting for you"): the band starts with the bold question and
  three lines of context (scrolls), then options with notes in their own scroll, then "1–4 pick · ↵ sends", Answer in
  terminal, Send answer below the options; the terminal under it. `fix2-d2-panel-and-box-long-question.jpg`,
  `fix2-d2-noyan-panel-band-question-shows-buttons-below-options.jpg`. Sent option 3 with the box's Send answer
  (click): Claude got "Answer to your question: Both in one line (option 3)."; no "wasn't sent" toast this time.
- Proposal 48: bar **"Architect · Claude Code's suggestion for shop · 1 change, marked"**, Docs "Added", Why these
  changes, box **"Ask for changes to Architect · Claude Code's proposal…"**. `fix2-d4-proposal-bar-and-ask-for-changes-name-architect.jpg`
- NEW-d3: "Also make Validate 2 rounds." ↵ → the drafting row came back while **Proposal 48 stayed on the board**
  (Docs Added, bar, Discard / Save flow). `fix2-d3-redrafting-keeps-proposal-48.jpg` → 2 changes (`fix2-d3-redraft-2-changes.jpg`).
  Claude's turns were too quick to Stop (5–9 s); a request that made Claude ask a question ("can't sleep in the
  foreground") showed the proposal kept under the question box (`fix2-d3-question-while-redrafting-proposal-stays.jpg`).
  Sent its answer and pressed **Stop** while it redrafted: the conversation ended (RECENTLY ENDED "Add Docs stage
  18:15") and **the last proposal stayed** ("Claude Code's suggestion for shop · 1 change · Changed Validate 2
  rounds"). `fix2-d3-after-stop-last-proposal-stays.jpg`. Discard → plain editor; Cancel; panel closed.
- Put back: Noyan uses → Noyan. `otaq flow show -P SHOP` identical to the copy taken before (`r8f2-shop-flow-before.txt`
  = `r8d-shop-flow-before.txt`).

## New problems
- **NEW-f1 · Sending a Something-else answer with ↵ on a ticket's Overview delivers it and also shows "The answer to
  HOOK-13 wasn't sent · HOOK-13 isn't waiting for an answer".** HOOK-13, single-answer question, option 4 + "Go on", one
  ↵: Claude's transcript has the answer once and it went on working, but the error toast came at the same time (looks
  like a second send after the first closed the question). The Send answer button (click) in Edit flow's box gave no
  toast. Not re-tried (it would need a new question). **Blocker: no** (the answer arrives; the toast is wrong and
  alarming). `fix2-f1-answer-delivered-and-wasnt-sent-toast.jpg` (Overview shows "Answer to your question: Go on", pill Working, and the toast).

## Notes
- NOTE-f1 · NEW-b3: digits tick only while focus is inside the box (after a click on a row/checkbox); with the sheet
  just opened, or after a click on the question's text, "2" switches to Session — same as the single-answer box. A
  user who reads "1–4 tick" and presses 2 without clicking first still leaves Overview. Leaving Overview (tab click) and
  coming back resets the ticks to the recommended ones.
- NOTE-f2 · NEW-d5: records left by the old build stay: SHOP-5 (deleted by Cleanup in part d) is still listed in
  `otaq prime --project` with its gone worktree. Only deletions made by fae1fd78 clear the record.
- NOTE-f3 · Open PR dialog's commit row joins the subject and the trailer: "fix: HOOK-13: append fix check 2 line to
  READMECo-Authored-By: Claude…" (newline dropped).
- NOTE-f4 · NEW-b1 neighbours: with the only agent waiting, a click where "1 running" was now lands on the usage chip,
  whose popover reads "AGENTS RUNNING 0 · Nothing running. A started ticket begins once an agent is free…" — wrong
  words for an agent waiting on the user. The palette has no Pause rows in that state ("Nothing matches pause");
  Ctrl+Shift+U opens that same popover. Later "1 running" stayed while the only other agent was HOOK-13's Codex
  merge-message writer, which the Open PR dialog said "wants to run a command" (a prompt) — so a Review writer at a
  prompt counts as running while a Dev agent waiting for an answer does not (not dug further).
- NOTE-f5 · NEW-d4 wording: the name carries the tool, "Architect · Claude Code's suggestion / proposal"; reads oddly
  (the possessive lands on "Claude Code").
- A Windows Firewall prompt for `otaq` (new runtime folder) came at the first agent start: Cancel.

## Not tested
- NEW-d4's permission line ("Architect wants your permission: …"): Claude Code runs Noyan in auto mode and never
  prompted; switching it to manual mid-test was not tried.
- Push to PR's commit list after the HOOK-10 rebase (PR already open; a push would need a force-push to the bare remote).

## Setup changes and state left
| Change | State now |
|---|---|
| Installed app 4147a741 → **fae1fd78** (`%LOCALAPPDATA%\Otaq`, `otaq-app.exe` and `otaq.exe` both new) | **running on the normal release-test data** via `otaqtest` / `launch.cmd`: app PID 13936, service PID 22660 (runtime `fae1fd782a68…`), hooksproj board on screen |
| HOOK-10's idle Claude Code session | ended by the reinstall (as expected) |
| HOOK-10 branch | **rebased by hand** onto hooksproj main 69ae147 → 819ff8d (local only; origin/otaq/HOOK-10 still c9c5a4f, PR #3 open, so the branch and its PR have diverged) |
| hooksproj main | new test-setup commit **b8c0a26** "test setup main moves on" (docs/r8f2-main.md), not reverted |
| HOOK-13 "R8f2-base-merge" (Dev and Validate = Architect) | Review · **Ready for you**; branch otaq/HOOK-13 = f725be3 + merge of main (12a438d); its Claude session live and idle; the Codex merge-message writer started at Review and wanted to run a command (not answered); "1 running" in the title bar |
| Cleanup | deleted the shop worktrees SHOP-33 (dirty README, Closed · not merged) and SHOP-46 (empty folder); branches kept |
| Noyan uses | Architect for the test, **put back to Noyan** |
| shop flow | four proposals (48) discarded, editor cancelled; `otaq flow show -P SHOP` unchanged (`otaq-reltest\r8f2-shop-flow-before.txt` / `-after.txt`) |
| Noyan conversation "Add Docs stage" (Architect) | ended by Stop, in RECENTLY ENDED |
| Questions asked by CLI on HOOK-13 (several-answers, long) | replaced / answered; none open |
| Files `otaq-reltest\r8f2-shop-flow-before.txt`, `r8f2-shop-flow-after.txt` | left (no secrets) |
| Clipboard | the last Noyan request text (no secrets) |
| The other Otaq (`~\otaq-qbox`) | untouched |

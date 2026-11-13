# Re-test 8 · part a · build/install + phase A (First run) on Windows · results (Windows 11 ARM64 VM, build release-fixes 0cc7efc5)

Date: 2026-11-12, 14:37–15:07 VM time. Plan: `PLAN.md` phase A, checked against DECISIONS.md UX-14…18, 20, 35, 36,
61 (first part), 62 (question box), A11–A17 and the earlier `RESULTS-A.md` (ef84b175). Screenshots:
`shots-retest-8/a-*.jpg` (JPEG). The GitHub login on the Tools step is blacked out (`a-tools-step-no-warnings.jpg`); no
other screenshot shows a Windows user name, login, email, token or sign-in URL (paths show as `~\…`). No Otaq code
changed, nothing committed in the repo, nothing pushed anywhere, nothing signed in or out.

Data: a fresh first-run data folder `otaq-reltest\home-r8a` (plus a short second one, `home-r8a2`, for the two checks
the first run couldn't reach), empty git config `otaq-reltest\gitconfig-r8a`, new repo `otaq-reltest\shop-r8a`. Agent
tool: **Claude Code v2.1.322** (signed in by the maintainer; default **auto mode**), the main path. Every agent stage
(Developer and Checker, Otaq default) ran on Claude Code.

## Summary
Phase A: **PASS 24 · FAIL 0 · KNOWN 4 · NOT TESTED 3 · NOTE 5.** Every point of phase A's line holds on Claude Code.
Three of RESULTS-A's four FAILs are gone: the Tools step no longer shows permission warnings (UX-36), Claude Code's
signed-in state is real (`claude auth status` loggedIn), and Windows toasts are titled "Otaq". The fourth (a login
failure reading only "Idle") can't be reached with a signed-in tool. Two new small problems, neither a blocker.

| Check | Result |
|---|---|
| Setup: incremental bundle → 0cc7efc5, build, install `/S`, About | **PASS** · About "Version 0.1.0 · build 0cc7efc5 · Built from source" |
| Welcome: model in three lines + mini board, one button (UX-16, A12) | **PASS** |
| Tools: four agent tools + git only, no permission warnings (UX-16, UX-36, A12) | **PASS** (RESULTS-A's UX-36 FAIL fixed) |
| Tools: Install / Sign in… in a terminal (UX-37) | **NOT TESTED** (every tool installed and signed in) |
| Project: folder, trust line, Name/Base branch/Ticket key prefilled, no Flow choice (UX-16, A13) | **PASS** (Name still plain text, NOTE) |
| Board on Otaq default, headers say who acts; no box, strip or Pause yet (UX-16, UX-20, UX-61) | **PASS**; Dev reads "An agent does the work" **KNOWN**; Done a "Done 0 →" link **KNOWN** |
| "Your first ticket" card: prefilled, editable, honest line, Create & start / Not now (UX-17, A14) | **PASS** |
| Not now → empty Backlog with a plain New ticket (UX-17) | **PASS** (second first run) |
| Git name/email asked at the first start: Save and start (identity rule) | **PASS** |
| Create & start: starts, "1 running" + Pause appear (UX-15, UX-19) | **PASS** |
| Notifications asked on the first Create & start (UX-18, A15) | **NOT TESTED** (macOS-only by design; Windows has no permission prompt, as in RESULTS-A) |
| First agent question: turquoise card, strip, hint with Ctrl+J (UX-14) | **PASS** |
| The question on Overview drawn per UX-62 (bold question, context, options with notes, Recommended) | **PASS**; the hint under it says "1–4 pick" for 3 options (NEW-a1) |
| First permission prompt: card, strip, one-time hint (UX-36, UX-14) | **PASS**; card "claude needs your permission" without the command **KNOWN (R7 NEW-9)** |
| First checker fail: "Fixing · sent back by Validate", one-time hint (UX-14) | **PASS**; the hint went away unanswered when round 2 passed fast (NOTE) |
| First ticket in Review: "Ready for you · passed Validate", magenta edge, strip; "Your turn: review" hint (UX-14, A11) | **PASS** (the hint: on the second first run; on the first the sheet was open, as built) |
| Review: Overview first, Approve & merge menu (UX-13, UX-56) | **PASS** |
| Merge dialog with the prepared message; writer still writing on Claude Code | **PASS**; writer never fills in **KNOWN (R7 NEW-12)** |
| Merge: toast "SHOP-1 merged into main · Squashed as …", sheet closes, Done (A17) | **PASS** (5f85373, author Release Tester) |
| "That was your flow" with Edit flow, after the first approve (UX-61, A16) | **PASS** |
| "New work starts here" over the box, which appears now (UX-20, A17) | **PASS** |
| Usage-data banner after that, one hint at a time (UX-35) | **PASS** |
| Window close while an agent works: one-time note, tray menu (UX-18, Windows) | **PASS**; toast now titled "Otaq" (RESULTS-A FAIL fixed) |
| Done order newest first (board-02) | **KNOWN** (oldest first, gap-analysis #49) |

## Checks

### Setup · PASS
- `%USERPROFILE%\otaq-build\retest-hooks` was at e8ddb1b5. Bundle `r8-e8ddb1b5-0cc7efc5.bundle` (40.8 MB, sent with
  vmpal_send_files, kept as `otaq-build\r8.bundle`) verified, fetched, fast-forward e8ddb1b5 → **0cc7efc5**.
- `retest-hooks-build.ps1` (+ `noupd.json`), log `otaq-build\retest8-build.log`: BUILD-EXIT=0 (Rust 2 min 10 s),
  `Otaq_0.1.0_arm64-setup.exe` 19.47 MiB, kept as `otaq-build\Otaq_0.1.0_arm64-setup-0cc7efc5.exe`.
- The e8ddb1b5 app (`otaq-reltest\home`) and its service were stopped (`taskkill /T` on the service; this ended
  HOOK-5's idle Codex session and NOTE-20's Claude Code `/otaq:milestone-plan` session). Setup `/S` exit 0
  (`otaq-app.exe` 14:46, `otaq.exe` 14:44). Started with `launch.cmd` through the `otaqtest` task: **About "Version
  0.1.0 · build 0cc7efc5 · Built from source"**. `a-setup-about-0cc7efc5.jpg`
- Found running and left alone: a second Otaq from `~\otaq-qbox\app` (a question-box piece's own build and home, four
  idle Claude Code `/otaq:flow` sessions, its own tray icon). Its window sat behind the test app the whole time.

### Setup of the first run
- `otaq-reltest\home-r8a` (new, created by the app), `otaq-reltest\gitconfig-r8a` (empty file), `launch-r8a.cmd` (a copy
  of `launch.cmd` with those two paths). The `otaqtest` task pointed at it for the run, then back at `launch.cmd`.
- `otaq-reltest\shop-r8a`: `git init -b main`, one commit "Setup Pocket shop" (author Setup) with the files of shop's
  first commit 143cfc3 (README with "libary", "informaton" and the broken link `docs/missing.md`; `src\price.js`;
  `package.json`; `test.js`), with test.js's lost `'$5'` put back (RESULTS-A's setup note) so `node test.js` prints ok,
  as SETUP.md describes. No remote.

### Welcome · PASS (UX-16, A12)
- Step indicator "1 Welcome · 2 Tools · 3 Project"; "Coding agents that follow your process"; "Your board's columns are
  your process. A ticket moves through them: agents do the work, other agents check it, and you decide what gets
  merged."; mini board Backlog (SHOP-1 Not started, "You write a ticket") → Dev (SHOP-1 turquoise "Asks you", "An agent
  writes the code") → Validate ("A checker tests it, and sends it back if it fails") → Review ("You review, then
  approve") → Done ("Merged, or sent as a pull request"); turquoise-dot line; "Next: connect an agent tool and add a
  project. About 2 minutes."; one button **Get started**. Same as RESULTS-A. `a-welcome.jpg`

### Tools · PASS (UX-16, UX-36, A12)
- "Connect an agent tool … Otaq never installs or signs in without your click." AGENT TOOLS "4 ready": Claude Code
  v2.1.322 · signed in, Codex v0.166.0 · signed in with ChatGPT, GitHub Copilot v1.0.109 · signed in as (login), OpenCode
  v2.2.5. ALSO NEEDED: git 2.56.0.windows.4 "required" · "Needs your name and email" with the inline amber form (Name /
  Email / Save, "Saved in your git settings for every repository (git config --global)"). No gh / az rows. Check again,
  Continue (not blocked by the empty identity). **No permission warnings** on any row: RESULTS-A's UX-36 FAIL is fixed.
  `a-tools-step-no-warnings.jpg`
- Claude Code's "signed in" is true this time (`claude auth status`: loggedIn true), and its session worked.
- With an identity saved (second run) the git row reads just "2.56.0.windows.4" with a check.

### Project · PASS (UX-16, A13)
- "Add a project · A project is a git repository on your machine…". Choose folder… → Windows picker → `~\otaq-reltest\
  shop-r8a` (green outline), "Claude Code, Codex and Copilot will run their hooks and settings in this folder.", Name
  "shop-r8a", Base branch "main" (a dropdown), Ticket key "SHOP" ("prefilled from the repo name"; "SHOP-1, SHOP-2, …
  Can't be changed later."), "Run command and environment can wait…", Add project. No flow choice. `a-project-step.jpg`
- NOTE (as in RESULTS-A) · Name is plain text in the grey box; a click doesn't edit it (A13 draws a field).

### Board and the first-ticket card · PASS (UX-16, UX-17, UX-20, UX-61, A14)
- Backlog "Not started" → Dev "An agent does the work" → Validate "A checker tests it" → Review "You review and
  approve", "Done 0 →" under Review, Edit flow at the end. No box for new work, no strip, no Pause.
- "Your first ticket" card with the accent outline: "Tidy the README" / "Fix typos and broken links. Change nothing
  else." (editable fields), "Small, clear tasks work best.", "Takes about 5 minutes and uses your Claude Code plan.",
  **Create & start** / **Not now**. `a-board-first-ticket-card.jpg`
- KNOWN · Dev's header "An agent does the work" (canvas "An agent writes the code"; questions.md piece 15).
- KNOWN · Done is a "Done N →" link, not an end column (questions-triage).
- **Not now** (second first run, `home-r8a2`): the card goes, leaving an empty Backlog ("Write a ticket here…", Add
  ticket) and the toolbar's New ticket; still no box. New ticket opened the full form. `a-r8a2-not-now-empty-backlog.jpg`

### First start · PASS
- Create & start with the empty git config → "git doesn't know your name and email yet · Every commit carries its
  author's name and email… Name / Email … Saved in your git settings for every repository (git config --global)."
  Cancel / **Save and start**. Release Tester / tester@example.com → saved (`gitconfig-r8a` holds them), SHOP-1 to Dev
  "Working · Developer", **"1 running"** and **Pause** in the title bar. `a-git-identity-ask-at-first-start.jpg`
- Windows Firewall asked about "otaq" (Cancel; expected).
- NOT TESTED · UX-18 / A15 notifications ask: nothing asked on Windows (Settings → notifications on). As built, a
  macOS-only check (RESULTS-A).
- Claude Code in auto mode did Dev and Validate in about 1 min, without a prompt or a question (Session tab:
  "auto mode on (shift+tab to cycle)").

### First question · PASS (UX-14, UX-62)
- SHOP-2 "Show prices with two decimals", which tells the agent to ask with `otaq ask human` first. The card turned
  turquoise "Asks you · should test.js be updated too? test.js currently asserts formatPrice(5) === '$5'. Once…" with
  **Answer**, strip "Needs you · 1 · SHOP-2 a question", a dot on the rail tile, and the one-time hint **"An agent needs
  you · When an agent asks something, its card turns turquoise and shows up here. Ctrl+J jumps to the next one, in any
  project." Got it**. `a-first-question-hint-card-strip.jpg`
- Answer → Overview, first on the page: **bold "Should test.js be updated too?"**, the context in regular weight
  below, options "Yes, update test.js" (★ Recommended, its note), "No, leave test.js" (note), "Something else: type
  your own answer", numbers 1–3, Answer in terminal, **Send answer**. `a-question-box-overview.jpg`. Sent option 2;
  the agent went on with it ("test.js left unchanged, as the user answered (option 2…)").
- Small: the hint under the options reads **"1–4 pick · ↵ sends"** with three options → NEW-a1.

### First permission prompt · PASS (UX-36, UX-14)
- Shift+Tab in SHOP-2's session → "manual mode on". At Claude Code's "Edit file src\price.js · Do you want to make this
  edit?" the card read "Asks you · claude needs your permission" with **Open**, strip "Needs you · 1 · SHOP-2 allow a
  command", and the one-time hint **"The agent asks for permission · Its tool is set to ask before some actions, so the
  agent waits for you. With several agents at once, allowlists or auto mode in the tool's own settings let them work
  on. Otaq won't change your settings." Got it**. `a-permission-prompt-hint.jpg`
- KNOWN (R7 NEW-9) · the card doesn't name the action ("claude needs your permission").
- Answered in the terminal ("1", then "2 · Yes, and switch to auto mode" for the next Bash prompt).

### First checker fail · PASS (UX-14)
- SHOP-3 "Note the re-test in README" asked the checker to fail round 1 (to reach this hint on purpose). The card went
  back to Dev **"Fixing · sent back by Validate"** and the hint **"Sent back by the checker · Validate tested the change
  and found a problem, so the ticket went back to Dev with the checker's notes. You only review work that passes." Got
  it** showed beside it. `a-checker-sent-back-hint.jpg`. Round 2 passed; the Overview read "Check passed · on the second
  try · the first try's findings are in History" and WHAT THE AGENT DID showed the **round-2** handoff (RESULTS-A's
  stale-summary note doesn't happen here).
- NOTE · Round 2 passed within ~10 s, and the hint went away with the moment before I clicked Got it; it isn't marked
  seen (`hints.seen.checker_sent_back` absent), so it will come again at the next checker fail. Harmless.
- A user's Send back (SHOP-2, "Changed my mind: update test.js…") did not show this hint (correct).

### First ticket in Review · PASS (UX-14, A11)
- SHOP-1: card "Ready for you · passed Validate", magenta edge, **Review** button, strip "Your review · 1 · SHOP-1
  ready", Next Ctrl+J; Running and Pause gone.
- "Your turn: review" hint: on the first run I had SHOP-1's sheet open (switching its checker to manual mode) at the
  moment it reached Review, so it was answered by seeing (`hints.seen.first_review` set then), as built. On the second
  first run (TWO-1, sheet closed) it showed: **"Your turn: review · Open the ticket to see what changed and what the
  checker found. Approve merges it or opens a PR, as your flow delivers; Send back returns it to the agent with your
  notes." Open TWO-1**, which opened the ticket. `a-r8a2-your-turn-review-hint.jpg`

### Review and merge · PASS (UX-13, UX-56, A17)
- Overview first: header "Ready for you", "Dev and Validate by Claude Code · 1 min · started by you", stepper, WHAT THE
  AGENT DID, CHECKER "Check passed · on the first try" + Full report, a recorded **Decision** (removed the broken link
  vs write docs), review bar "Send back · to Dev…" · "1 file +1 −4" · **Approve & merge into main** with its menu
  (Approve & open PR disabled "PR provider not detected", "Your flow delivers either way…", Merge into another ticket…,
  Done, nothing to merge (disabled with reason), Done, don't merge). `a-review-overview.jpg`, `a-approve-menu.jpg`
- Merge dialog: "Squashed into one commit on main…", "No merge check · set one in project settings", the prepared
  message "SHOP-1: Tidy the README / README tidied: …", "Developer is still writing the message…", the worktree note,
  Cancel / Merge. `a-merge-dialog-still-writing.jpg`
- KNOWN (R7 NEW-12) · on Claude Code the writer's message never came for SHOP-1 (still "writing" after 65 s); SHOP-2 and
  SHOP-3 were merged at once with the prepared message ("still writing" shown), so not waited for.
- Merge → toast **"SHOP-1 merged into main · Squashed as 5f85373"**, the sheet closed, SHOP-1 in Done "Merged into
  main". shop-r8a main: `5f85373 Release Tester <tester@example.com> | SHOP-1: Tidy the README`, README fixed, folder
  fast-forwarded and clean, `node test.js` ok. Identity not asked again at merge (already saved; correct).

### After the first approve · PASS (UX-61, UX-20, UX-35, A16, A17)
- **"That was your flow · SHOP-1 went Backlog → Dev → Validate → Review → Done. These columns are this project's process:
  add a check, drop one, or pick other agents with Edit flow." Got it / Edit flow** at Edit flow, and the box for new
  work appeared at the same moment. `a-merged-toast-that-was-your-flow.jpg`
- Got it → **"New work starts here · Type a ticket and press ↵ to create it as written. Or Ask Noyan to shape it: Noyan
  takes a look and writes it up, or files separate tickets when you asked for several things. You start them." Got it**
  over the box. `a-new-work-starts-here.jpg`
- Got it → the quiet banner **"Help improve Otaq? Share anonymous usage data: which screens and actions are used. Never
  code, tickets or names. You can change it in Settings → Data." No thanks / Share**. No thanks → gone,
  `usage_data` "off". `a-usage-banner.jpg`. One hint at a time throughout.
- The box: Ctrl+↵ created and started SHOP-3 (UX-22/48).
- NOTE · On the second run, **Done, don't merge** on the first ticket ("counts as merged") also brought "That was your
  flow" and the box. UX-61 says "after the first approve"; reading "Done, don't merge" as that is defensible, so a note.
  `a-r8a2-done-dont-merge-flow-hint.jpg`
- KNOWN · Done lists SHOP-1, SHOP-2, SHOP-3 oldest first (gap-analysis #49).

### Closing the window · PASS (UX-18, Windows)
- Closing the window while SHOP-2's agent waited: the window hid, the app kept running, one-time toast **titled
  "Otaq"** (Otaq's icon): "Otaq is still running · Its icon is in the notification area. Windows may put new icons
  under ^ next to the clock: drag Otaq's icon onto the taskbar to keep it in view." RESULTS-A's "Windows PowerShell"
  title FAIL is fixed. `a-close-window-still-running-toast.jpg`
- Tray icon menu: "NEEDS YOU · 1 · SHOP-2 · allow a command", Open Otaq Ctrl+O, Pause all, Quit Otaq Ctrl+Q.
  `a-tray-menu-needs-you.jpg`. Its SHOP-2 item opened the window on SHOP-2's Session tab at the prompt ("Typing goes to
  the agent · you're here, so no alert").
- As in RESULTS-A, the Windows wording doesn't say agents keep working (UX-18's line is the macOS one).

## New problems
- **NEW-a1 · The answer box's hint says "1–4 pick" whatever the number of options.** SHOP-2's question had three rows
  (two options + Something else, numbered 1–3) and read "1–4 pick · ↵ sends". `answer_options.rs` `options_hint`
  returns a fixed "1–4 pick" (and "1–4 tick" for several answers). Expected the real range (1–3). Blocker: no
  (wording). `a-question-box-overview.jpg`
- **NEW-a2 · Ctrl+Q in the window didn't quit** (second run's board, after No thanks; nothing happened). Seen once, the
  focus may have been in the board; the tray's Quit Otaq wasn't tried. Blocker: no (quit is phase D; the tray menu
  shows Quit Otaq Ctrl+Q).

## Notes
- NOTE-a1 · Name on the Project step is plain text (A13 draws a field) — as in RESULTS-A.
- NOTE-a2 · The checker-fail hint can go unanswered when the next round passes in seconds; it then shows again later.
- NOTE-a3 · "Done, don't merge" on the first ticket counts as the first approve for the after-approve hints.
- NOTE-a4 · No Windows toast was seen for the question or the permission prompt: the window had focus each time (the
  service is told the window's focus), so none was expected; a toast with the window hidden wasn't tried.
- NOTE-a5 · Claude Code (auto mode) runs the first ticket through Dev and Validate in about a minute without a single
  prompt or question, so a first-run user on Claude Code sees none of the question / permission / checker hints on
  the first ticket; they come later.

## Not tested
- UX-37 Install / Sign in… under a tool's row (all four tools installed and signed in).
- UX-18 / A15 notifications ask and the second ask at the first question (macOS only, as built).
- RESULTS-A's FAIL "a login failure reads only Idle" (Claude Code is signed in; not forced).

## Setup changes and state left
| Change | State now |
|---|---|
| `otaq-build\retest-hooks` e8ddb1b5 → **0cc7efc5**; `r8.bundle`, `retest8-build.log`, `Otaq_0.1.0_arm64-setup-0cc7efc5.exe` in `otaq-build` | left |
| Installed app e8ddb1b5 → **0cc7efc5** (`%LOCALAPPDATA%\Otaq`) | installed; **running on the normal release-test data** (`otaq-reltest\home`, `launch.cmd` via `otaqtest`, started 15:06; About build 0cc7efc5) |
| Normal data: HOOK-5's idle Codex session and NOTE-20's Claude Code session ended by the reinstall | not restarted by me; tickets unchanged otherwise |
| `otaq-reltest\home-r8a`, `home-r8a2`, `gitconfig-r8a` (Release Tester / tester@example.com), `launch-r8a.cmd`, `launch-r8a2.cmd` | left in place; no app runs on them |
| `otaq-reltest\shop-r8a`: main 62abc9d (SHOP-1 5f85373, SHOP-2 880970c, SHOP-3 62abc9d merged); branches otaq/SHOP-1…3 and otaq/TWO-1 (Done, don't merge) | left, clean, `node test.js` ok |
| Scheduled task `otaqtest` | back to `launch.cmd` |
| The question-box piece's own Otaq (`~\otaq-qbox`) | found running, left running (window behind the test app) |
| Clipboard | holds TWO-1's title text (no secrets) |

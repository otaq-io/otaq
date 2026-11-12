# Re-test 5 · release-issues round 4 on Windows · results (Windows 11 ARM64 VM, build release-fixes 528d738a)

Date: 2026-11-11, 11:42–12:49 VM time. Screenshots: `shots-retest-5/` next to this file (JPEG; Windows paths with the
user name in Codex/Noyan prompts and the Needs you line, the GitHub login on Settings → Agents, and Claude Code's
sign-in URL are blacked out. Paths shortened to `~\otaq-reltest\…` by the tools are left).

## Summary
Checks: **PASS 10 · PARTLY 1 · FAIL 1** of 12 (pieces 24–33, the Add a project dialog, the regression walk), and the
uninstall check PASS.

- PASS · **24** · "Ask the agent to write it…" on "Update the docs 2" wrote the skill onto Update the docs 2 (Noyan ran
  `otaq flow write-skill update-the-docs-2 --done`); "Update the docs" kept its own skill.
- PASS · **25** · Sign in… on NOTE-16's sheet (Claude Code stopped · login expired) opened a "Claude Code sign in"
  dialog showing the login's terminal (URL, "Paste code here if prompted >", then "Login successful."). Resume carried on.
- **FAIL · 26** · The restack request waited behind the unsent text (15 s+, text untouched), but **the first Backspace
  I pressed in that line let it go**: Otaq typed `$otaq:restack …` straight after the remaining text and sent both as
  one message.
- PASS · **27** · Park dialog "conversation · resumes on Product spec"; a ticket closed from Code review ticks only the
  stages it passed, and one closed after Product spec ticks none.
- **PARTLY · 28** · Helpers: Needs you on the row as the prompt is drawn and "working" ≤0.7 s after "y" (6 of 6). Checker:
  the first prompt of a turn reads "Needs you · checker" within 1 s and clears ≤0.7 s after "y" (5 of 5), **but a second
  prompt Codex had queued right behind the first one never read Needs you** (3 of 3: 4 s, 9 s and 30 s+ still
  "Checking"), as in re-test 3.
- PASS · **29** · The agent in charge reads "idle" in Validate and in Code review. The card behind an open sheet followed
  the pill while the agent worked (see NOTE-2 for the focused case).
- PASS · **30** · Restart dialog closed after "Resume 1"; the strip chip's hover caps a 1,400-character command with
  "…"; Edit flow shows no "Removing waits…" note on an empty stage (the with-tickets case wasn't set up).
- PASS · **31** · Settings → Git & pull requests was filled in the first screenshot after the click (<0.5 s); Settings →
  Agents correct.
- PASS · **32** · Later-turn `otaq dep add` → "CHANGED 1 · SHOP-34 · now waits for SHOP-33 · Undo", CREATED rows keep
  their titles; Undo → "Undone · SHOP-34 no longer waits for SHOP-33".
- PASS · **33** · An open question (asked by SHOP-32's Developer) was closed by Send back (`answered_via=closed`); after
  the Dev agent's turn end the ticket read Idle, not Asks you.
- PASS · **Add a project** (995b16ef) · the Flow title has space under it.
- PASS · **Regression** · SHOP-32 Approve & merge (fc46f70); running chip "1 running"; Settings opens, all four tools
  Connected.
- PASS · **Uninstall (decision 024)** · `HKCU\Software\Classes\otaq` present after install, gone after the silent
  uninstall, back after reinstalling.

New problems: NEW-1 (piece 26: one Backspace releases the restack request onto pasted unsent text), NEW-2 (piece 28: a
queued second checker prompt never reads Needs you), NEW-3 (a Dev prompt during "Merge check failed · fixing" shows no
Needs you), NEW-4 (a "1 message waiting" chip that never delivers, and an "Unsent text" footer over an empty line), and
notes.

## Setup
- Build: branch release-fixes at **528d738a** ("Merge piece 26: a test that the restack request waits behind unsent
  text"). An incremental bundle `5e8e9fc7..release-fixes` (93 KB) was fetched into `%USERPROFILE%\otaq-build\retest3\repo`
  (fast-forward 5e8e9fc7 → 528d738a) and built with the same `build.ps1` (shared `CARGO_TARGET_DIR`, `npm.cmd ci` +
  `npm.cmd run bundle -- --bundles nsis --config noupd.json`; PowerShell needed `-ExecutionPolicy Bypass`):
  BUILD-EXIT=0, `Otaq_0.1.0_arm64-setup.exe` 18.95 MiB, kept as `retest3\Otaq_0.1.0_arm64-setup-528d738a.exe`, log
  `retest3\build-retest5.log`.
- Waiting for the hooks agent: `%USERPROFILE%\otaq-reltest\hooksproj` was still there at 11:50:01 and gone at 11:50:48
  (with `hooksproj-remote.git`). I installed only after that.
- Install: the 5e8e9fc7 app and service (started 10:29, no agent process running) were stopped with taskkill, setup
  `/S` exit 0 (`otaq-app.exe` 11:49, `otaq.exe` 11:47), started with `launch.cmd` through the `otaqtest` task. About:
  "Version 0.1.0 · build 528d738a · Built from source". `setup-about-528d738a.jpg`
- Data: the release test's shop (Dev → Validate → Review, Codex Developer/Checker, merge check `node test.js`) and notes
  (Spec first; Product/Tech spec and Milestone plan on Claude Code, Dev and Validate on Codex). Noyan on Codex.
- Typing: text on the clipboard (`Set-Clipboard`) and Ctrl+V; keys like "y" pressed. Codex prompts were answered "y"
  (and Esc once, see 28), never "p", so no Codex rules were added.
- Claude Code was Connected at 11:51 (`c31-agents-pane-at-start.jpg`) but signed out at 11:53: NOTE-16's Milestone
  planner session showed "Login expired · Please run /login" and `claude auth status` said `loggedIn: false`
  (`.credentials.json` rewritten 11:53). I didn't sign anything out; the hooks agent was done by then, so it looks like
  the login expired. I used it for check 25 and signed back in through Otaq's own Sign in… (the account was already
  signed in in Edge; one click on Authorize). Claude Code was Connected again at the end.

## Checks

### PASS · 24 · A skill asked for on a new stage goes to that stage
- shop → Edit flow → + between Dev and Validate → Update the docs; + between it and Validate → Update the docs (again),
  which Otaq named **"Update the docs 2"**. Draft "2 changes · added Update the docs, added Update the docs 2". Both had
  `update-docs` (missing). So the first one's skill is easy to tell apart, I picked Otaq's `commit` for "Update the
  docs". `c24-two-stages-added.jpg`
- On "Update the docs 2" → "Ask the agent to write it." → the box read "Write a skill for the Update the docs 2 stage:"
  (I added a one-line description) → Send. Noyan (Codex) opened ("Write docs skill"); the stage's Skill read "Being
  written by Noyan… Stop". `c24-asked-for-update-the-docs-2.jpg`
- Two permission prompts showed the one-line "Needs you · Write docs skill wants your permission: Run command: … Answer"
  line (`c24-noyan-prompt-needs-you-line.jpg`). It wrote `.agents\skills\update-readme\SKILL.md`, then ran **`otaq flow
  write-skill update-the-docs-2 --done`** ("update-readme is in Update the docs 2's skill fi…").
- Result: "Update the docs 2" → Skill "update-readme · Written by the agent · Open · Undo", card "update-readme is
  written but not committed yet". "Update the docs" → Skill still "commit · Otaq's own". `c24-skill-on-second-stage.jpg`,
  `c24-first-stage-keeps-commit.jpg`
- Cancel in Edit flow: flow back to Dev → Validate → Review, `.agents` gone, `git status` clean. The Noyan conversation
  was ended (End agent).
- Noyan took 7 min 22 s, about 3 min of it "Working" with no output after writing the file (Codex's own time).

### PASS · 25 · Sign in… on a ticket's sheet shows its terminal
- NOTE-16 (notes, Product spec and Tech spec skipped, Milestone plan on Claude Code) stopped at once: "Stopped · login
  expired"; the strip read "NOTE-16 stopped · login expired". `c25-note16-session-login-expired.jpg`
- The sheet's callout first read "Claude Code stopped · login expired · Sign in to Claude Code, then Resume…" with
  **Resume and Answer in terminal only** (no Sign in…). `c25-sheet-no-sign-in-before-recheck.jpg` After I opened
  Settings → Agents (Claude Code "Found · not signed in · Sign in…", `c25-settings-claude-not-signed-in.jpg`) and went
  back, the callout had **Sign in…**. `c25-sheet-sign-in-button.jpg` (see NOTE-1)
- Sign in… → a "Claude Code sign in" dialog over the sheet, showing the login's own output: "Opening browser to sign
  in… If the browser didn't open, visit: <URL> · Paste code here if prompted >". Edge opened Claude's authorise page.
  `c25-sign-in-terminal-dialog.jpg`
- After Authorize the dialog showed "Login successful." `c25-sign-in-login-successful.jpg`. Esc closed it; Resume →
  NOTE-16 went on (Working). `c25-resumed-working.jpg`. Not the "Nothing open" Terminals drawer of re-test 2.

### FAIL · 26 · A request Otaq types never wipes unsent text
- SHOP-32 (shop) in Review, its Dev Codex session idle. A setup commit on main (87d39a7, `setup-move-r5.txt`) made
  main move; the merge check was set to `..\..\..\mc.cmd` with `otaq-reltest\failcheck.flag` present.
- I pasted "unsent draft line left by the tester retest5" into the Dev session's input line and didn't send it.
  `c26-unsent-text-before-approve.jpg`
- Approve & merge (dialog "Runs ..\..\..\mc.cmd first when main has moved") → Merge → "Merge check failed on main" with
  the MERGE CHECK FAILED output; SHOP-32 back to Dev, "Merge check failed · fixing". `c26-approve-dialog-runs-mc.jpg`,
  `c26-merge-check-failed.jpg`
- **It waited**: 15 s later the line still read "› unsent draft line left by the tester retest5", nothing typed, the
  header "2 messages waiting · Deliver now". `c26-restack-waits-text-intact-15s.jpg`
- **Then it went too early**: I clicked in the terminal and sent Backspace 50 times (more than enough to empty the line).
  1 s later the transcript showed **"› unsent draft line left by the tester retest$otaq:restack SHOP-32's merge check
  failed on the result rebased onto 'main'; the comment has its output. Run 'otaq prim…"** sent as one message: only the
  "5" had been deleted, and the request was typed after the rest of my text and submitted with it. Codex then worked on
  the restack. `c26-FAIL-restack-typed-after-one-backspace.jpg`, `c26-FAIL-sent-as-one-message.jpg`
- Reading: Otaq decided the line was empty after the first key. The piece's test types "half-typed" byte by byte and
  then 10 deletes; here the text came in as one paste (Ctrl+V in the app's terminal). My guess is the input tracker
  counts the paste as less than its length, so one delete makes it "empty" and the screen check is skipped. Not
  verified in the code.
- The failcheck.flag was removed and the merge check put back to `node test.js`. Codex then rebased (up to date),
  failed to run `..\..\..\mc.cmd` from the worktree and moved on; the checker (round 2) passed after a comment.

### PASS · 27 · A ticket's stage names are its own
- NOTE-17 (notes; Product spec on Claude Code's Spec writer, "reply READY and wait") idle in Product spec, dragged to
  Backlog: "Park NOTE-17? … conversation · **resumes on Product spec**". Parked. `c27-park-dialog-resumes-on-product-spec.jpg`
- NOTE-16 closed from Code review: the path shows Product spec and Tech spec as skipped, Milestone plan, Spec review,
  Dev and Validate ticked, **Code review not ticked**. `c27-closed-from-code-review-dots.jpg`
- NOTE-17 closed from Backlog (parked from Product spec, which it never finished): **no stage ticked**.
  `c27-closed-from-product-spec-none-ticked.jpg`
- A held ticket's column: not tested (no held ticket came up without signing out).

### PARTLY · 28 · A Codex prompt on screen reads Needs you at once
- **Helpers** (NOTE-16; Milestone plan wrote a 2-piece Plan; Spec review approved; Dev on Codex started helpers 16.1
  docs/one.txt and 16.2 docs/two.txt, both Codex):
  - Both rows read "needs you · a command" with their prompts on screen. `c28-helpers-both-need-you.jpg`
  - "y" on 16.1 → "working" at 0.5 s; "y" on 16.2 → "working" at 0.7 s. `c28-helper-16-1-working-after-y.jpg`,
    `c28-helper-16-2-working-0-7s-after-y.jpg`
  - Watching 16.2 at 2 s steps: one frame "Working · Running hook" with the row "working", the next frame the prompt
    drawn and the row already "needs you · a command". `c28-helper-16-2-working-running-hook.jpg`,
    `c28-helper-16-2-prompt-drawn-row-needs-you-same-frame.jpg`
  - 16.1's second prompt: "needs you" when I opened it. 6 helper answers in all, each cleared ≤0.7 s.
- **Checker, first prompt of a turn** (5 times on SHOP-32 and NOTE-16): the pill and card read "Needs you · checker"
  within 1 s of the prompt being drawn, and "Checking · round 1 of 3" 0.7 s after "y". `c28-checker-prompt-drawn-0s.jpg`,
  `c28-checker-needs-you-1s.jpg`, `c28-checker-checking-0-7s-after-y.jpg`, `c28-note16-checker-needs-you-1s-card-and-pill.jpg`
- **Checker, a second prompt Codex queued behind the first** (Codex shows two "Running …" lines at once, then asks for
  them one after the other): **never read Needs you** while drawn. See NEW-2.
  - SHOP-32 round 1: after "y" on the first, the second prompt was on screen with the pill "Checking · round 1 of 3",
    the card "Checking" and no Needs you in the strip, still so after 30 s. `c28-checker-2nd-prompt-drawn-pill-checking.jpg`,
    `c28-checker-2nd-prompt-30s-still-checking.jpg`, `c28-checker-2nd-prompt-card-checking-strip-0.jpg`
  - NOTE-16: while the first prompt (`Get-Content package.json; …`) was on screen the card already named the *next*
    command ("Asks you · run command: git show --stat …"). After "y" the `git show` prompt was drawn and the pill and card
    read "Checking" at 1 s and at 9 s. `c28-note16-checker-card-names-other-command.jpg`,
    `c28-note16-2nd-prompt-1s-checking.jpg`, `c28-note16-2nd-prompt-9s-still-checking.jpg`
  - SHOP-32 round 2: first prompt (`$env:OTAQ_RESOURCE_DIR`, with `git diff --` queued) Needs you; after "y" the
    `git diff --` prompt was on screen with "Checking · round 1 of 3" at 4 s. `c28-shop32-round2-two-commands-first-needs-you.jpg`,
    `c28-shop32-round2-second-prompt-checking.jpg`
- Esc once: on SHOP-32's checker prompt for `otaq check fail` (it wanted to fail over my setup file); the check ended
  "without a result"; I added a comment and pressed Retry validation.

### PASS · 29 · An idle agent in charge and the card read their real state
- NOTE-16 in Validate: Session list "IN CHARGE Developer · Codex · **idle**" (helpers "merged"), while the checker
  asked. `c29-validate-in-charge-idle.jpg`
- NOTE-16 in Code review: "Developer · Codex · **idle**". `c29-code-review-in-charge-idle.jpg`. After close: "ended".
- Card behind an open sheet while SHOP-33's Codex worked through prompts: card and pill read "Working" together, and both
  moved together when unfocused (card "Asks you", pill "Needs you").
  `c29-card-and-pill-working-same.jpg`, `c29-unfocused-card-asks-you-pill-needs-you.jpg`
- With the terminal focused ("Typing goes to the agent · you're here, so no alert") and a prompt on screen, the card
  read "Working" while the pill read "Needs you" for 12 s+, until I clicked out of the terminal. See NOTE-2.
  `c29-focused-card-working-pill-needs-you.jpg`

### PASS · 30 · Small dialog and panel fixes
- Restart dialog: app and service killed with SHOP-33 working; on launch "1 agent was stopped by a restart · SHOP-33
  Retest5 dep A · was working in Dev" (ticked), Resume 1 → **the dialog closed** and SHOP-33 read Working.
  `c30-restart-dialog-1-stopped.jpg`, `c30-restart-dialog-closed-after-resume.jpg`
- Needs you strip chip hover: a short command shows whole (`c30-strip-hover-short-command-whole.jpg`); Noyan's ~300
  character `@' … '@ | otaq create …` shows whole (`c30-strip-hover-mid-command-whole.jpg`); a 1,400-character
  `Write-Output "Line 1 … Line 12 …"` shows about 600 characters ending "…test…" in a box about 250 × 120 px.
  `c30-strip-hover-long-command-capped.jpg`
- Edit flow, Validate selected with no ticket in it: only "Remove stage" at the panel's foot, **no "Removing waits until
  no ticket is in it"**. `c30-empty-stage-no-remove-note.jpg`. The with-tickets case wasn't set up (no ticket was in a
  stage while I had Edit flow open).

### PASS · 31 · Settings → Git & pull requests fills in quickly
- First Settings open about 1 min after the app started: gear → Agents → Git & pull requests: the screenshot right after
  the click already showed the pane filled in: "git 2.56.0.windows.4 · C:\Program Files\Git\cmd\git.exe · Needed by
  every project", "GitHub · gh isn't installed", "Azure DevOps · az isn't installed", with winget commands and Install.
  `c31-git-click-0s.jpg`, `c31-git-filled-0-5s.jpg`
- Re-check: rows stayed and were the same 0.5 s later. `c31-git-recheck-0-5s.jpg`
- Settings → Agents: Claude Code v2.1.322, Codex v0.166.0, GitHub Copilot v1.0.109, OpenCode v2.2.5, all Connected (at
  11:51 and again at 12:47). `c31-agents-pane-at-start.jpg`, `reg-settings-agents-all-connected.jpg`

### PASS · 32 · Noyan's later-turn change and an undone row
- Turn 1 ("write two tickets… don't make either wait"): Noyan ran `otaq create` twice: "CREATED 2 · IN BACKLOG · SHOP-33
  Retest5 dep A · SHOP-34 Retest5 dep B". `c32-turn1-created-2.jpg`
- Turn 2 ("make SHOP-34 wait for SHOP-33"): `otaq dep add SHOP-34 SHOP-33` → a new **"CHANGED 1 · SHOP-34 · now waits for
  SHOP-33 · Undo"** row; the CREATED rows still read "SHOP-33 Retest5 dep A" and "SHOP-34 Retest5 dep B"; SHOP-34's card
  "Waits for SHOP-33 to merge". `c32-turn2-changed-row-undo.jpg`
- Undo → the row reads **"Undone · SHOP-34 no longer waits for SHOP-33"** (tooltip "Retest5 dep B"), no Undo; the card's
  wait is gone. `c32-undone-row.jpg`

### PASS · 33 · Sending a ticket back closes its open question
- SHOP-32 in Review ("Ready for you", its Developer idle, delivery message written). Through "Tell Developer now" I asked
  the Developer to ask me a question: it ran `otaq ask human` ("Retest5 question: should the README line end with a full
  stop?"); the store had it open (`human_questions.answered_at` NULL). `c33-dev-agent-asked-question-in-review.jpg`
- In Review the ticket kept "Ready for you" and the sheet showed no question. `c33-review-sheet-question-not-shown.jpg`
  (So "Asks you" wasn't visible before the send back; the check is on the stored question and what comes after.)
- Send back · to Dev with a comment ("the earlier question is dropped; reply WAITING"). `c33-send-back-dialog.jpg`
- At once: the question row had `answered_at` set, `answered_via = closed`, `answer` NULL. Card Working.
  `c33-sent-back-working.jpg`
- The Dev agent replied WAITING, got Otaq's turn-end reminder, said it was waiting, and stopped: card, pill and strip
  read **Idle** ("SHOP-32 idle"), not Asks you, and the old question didn't come back. `c33-turn-end-idle-not-asks-you.jpg`

### PASS · Add a project dialog (995b16ef)
- "+" on the rail → Add a project: "Flow" has a clear gap (about 8 px) above "Otaq default". Cancelled.
  `add-project-dialog.jpg`, `add-project-dialog-zoom-flow-title.jpg`

### PASS · Short regression
- SHOP-32 Dev → Validate (passed) → Review → Approve & merge into main: the dialog showed the plain message and
  "Developer wants to run a command to write the message… Answer in terminal"; Merge → "Merged", the merge check `node
  test.js` passed, main **fc46f70** "SHOP-32: Retest5 send back". `reg-approve-merge-dialog.jpg`, `reg-SHOP-32-merged.jpg`
- Running chip "1 running": "AGENTS RUNNING 1 · SHOP-33 Retest5 dep A · Developer · 7s", Claude Code 0, Codex 1, GitHub
  Copilot 0, OpenCode 0. `reg-running-chip-1.jpg`
- Settings opens (Agents, Git & pull requests, About).

### PASS · Uninstall check (decision 024)
- Before: `reg query HKCU\Software\Classes\otaq` → `(Default) URL:Otaq`, `URL Protocol`, `DefaultIcon`
  `"…\Otaq\otaq-app.exe",0`, `shell\open\command` `"…\Otaq\otaq-app.exe" "%1"`.
- App and service stopped; `%LOCALAPPDATA%\Otaq\uninstall.exe /S` → exit 0; `otaq-app.exe` gone. Then
  `HKCU\Software\Classes\otaq` → **not found**; `HKCU\…\Uninstall\Otaq` not found; the release-test data folder kept.
- Reinstalled `Otaq_0.1.0_arm64-setup-528d738a.exe /S` → exit 0, the key back (`shell\open\command` as before), started
  with launch.cmd: About "build 528d738a".

## New findings
- **NEW-1 (piece 26)** · A pasted unsent line holds a restack request only until the first delete: after one Backspace
  Otaq typed the request onto the rest of the text and Codex got "…tester retest$otaq:restack …" as one message.
  `c26-FAIL-restack-typed-after-one-backspace.jpg`
- **NEW-2 (piece 28)** · A checker's second prompt that Codex queued right behind the first never reads Needs you (pill
  and card "Checking", strip empty) for as long as it's on screen; the first one's card may name it early. 3 of 3 times.
  Helpers didn't show it in 6 answers. `c28-checker-2nd-prompt-30s-still-checking.jpg`,
  `c28-note16-2nd-prompt-9s-still-checking.jpg`
- **NEW-3** · SHOP-32's Dev agent at a Codex prompt (`git rebase main`, `git diff …`) while the ticket was "Merge check
  failed · fixing": the pill and card kept "Merge check failed · fixing" and the strip had no Needs you. I only found the
  prompts by opening the session. `new-fixing-prompt-pill-no-needs-you.jpg`, `new-fixing-prompt-card-no-needs-you.jpg`
- **NEW-4** · After the send back, SHOP-32's Dev session header read "1 message waiting · Deliver now" and the footer
  "Unsent text in the input line · Otaq's note waits until it's empty" while Codex's line was empty. The waiting row in
  `pending_deliveries` belonged to the round-1 checker's ended session ("The user added comment 2. Run `otaq prime`…"),
  and the count stayed through Review until the merge. (The unsent-text flag may come from a "y" I pressed after a
  prompt had closed.) `new-unsent-text-claimed-line-empty.jpg`
- NOTE-1 · A ticket stopped for "login expired" shows Sign in… on its sheet only after the tools are checked again
  (opening Settings → Agents); before that the callout says "Sign in to Claude Code, then Resume" with no Sign in….
  `c25-sheet-no-sign-in-before-recheck.jpg`
- NOTE-2 · With the session's terminal focused ("you're here, so no alert"), a Codex prompt left the card "Working"
  while the sheet's pill read "Needs you". Probably by design (no alert while you're there), but the card says Working
  for an agent that is waiting. `c29-focused-card-working-pill-needs-you.jpg`
- NOTE-3 · Claude Code's login expired by itself at 11:53 (see Setup).
- NOTE-4 · A Codex Developer told the merge check failed on a flag file ran `& ..\..\..\mc.cmd` from its worktree
  (not found there), then handed back saying the flag must be removed. Expected for this test setup.

## Setup changes (what was put back)
| Change | Why | State now |
|---|---|---|
| Installed app 5e8e9fc7 → 528d738a, then uninstalled and reinstalled 528d738a | the re-test, decision 024 | installed **528d738a**, running on the release-test data (launch.cmd), no agent running |
| `retest3\repo` fast-forwarded to 528d738a; `retest5.bundle`, `build-retest5.log`, `Otaq_0.1.0_arm64-setup-528d738a.exe` added | build | left, like before |
| shop: setup commit 87d39a7 "Test setup: main moves (retest 5)" (adds `setup-move-r5.txt`) | make the merge check run (26) | **not reverted**: SHOP-32 merged on top; main fc46f70, clean |
| shop merge check `..\..\..\mc.cmd` + `otaq-reltest\failcheck.flag` | 26 | back to `node test.js`; flag removed |
| shop Edit flow draft (Update the docs, Update the docs 2, skill `update-readme`) | 24, 30 | Cancelled: flow Dev → Validate → Review, `.agents` gone, `git status` clean |
| Tickets SHOP-32, SHOP-33, SHOP-34, NOTE-16, NOTE-17 | checks | SHOP-32 merged (fc46f70, README "Tested in retest 5."); SHOP-33, SHOP-34, NOTE-16, NOTE-17 closed without merging (branches kept). SHOP-33's worktree `home\wt\shop\SHOP-33` is kept by Close because it has an uncommitted README change (in Cleanup) |
| Noyan conversations "Write docs skill", "Retest5 README Lines" | 24, 30, 32 | ended (Recently ended) |
| Claude Code sign-in | expired by itself at 11:53 | signed back in through Otaq's Sign in…; Connected |
| Codex permission answers | "y" (one Esc), never "p" | no Codex rules added |
| Scheduled task `otaqtest` | start the app on screen | left (it existed before) |

## State left in the VM
The installed app (**528d738a**) is running on screen with its service on the release-test data folder (`otaq-app.exe`
and `otaq.exe` only; no codex, claude, copilot or opencode process). shop: Backlog, Dev, Validate and Review empty, Done
30, main fc46f70, clean, flow Dev → Validate → Review, merge check `node test.js`. notes: everything Done or closed, main
84406d5, flow unchanged. Noyan: no conversation open. hooksproj was already gone before I installed. Sign-ins: all four
Connected.

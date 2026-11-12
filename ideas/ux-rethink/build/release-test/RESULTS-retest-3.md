# Re-test 3 · release-issues follow-ups on Windows · results (Windows 11 ARM64 VM, build release-fixes 08caba65)

Date: 2026-11-11, 01:35–02:20 VM time (then the layout check on 4cc5ce7b). Screenshots: `shots-retest-3/` next to this file (JPEG; Windows paths with
the user name in Codex prompts and the board's Needs you line are blacked out where they show; the GitHub login on
Settings → Agents is blacked out. Paths shortened to `~\otaq-reltest\…` by the tools are left).

## Summary
Checks: **PASS 4 · PARTLY 1** (piece 23: a key alone clears the answered prompt in a helper and a checker session, but
in 3 of the 12 answers I timed the Needs you stayed up while Codex ran the approved command; see the piece). Regression
walk: PASS. **Edit flow layout (4cc5ce7b): PASS 3 of 3.**

- PASS · **Piece 7** · 1 running everywhere: chip, its list, rail, tray, Quit; still 1 after Keep them running and reopening.
- PASS · **Piece 8** · Codex unparked from Dev and from Product spec, and Claude Code unparked from Dev: each got "You
  were parked and are now back. Carry on…" and worked; the card read Working only while it worked, then Idle.
- PASS · **Piece 17** · Needs you replaces "drafting a flow"; "y" brings the line back within 1 s; the next prompt shows
  Needs you naming the new command.
- PARTLY · **Piece 23** · Helper (6 answers) and checker (6 answers, two tickets) at Codex prompts: "y" alone cleared
  Needs you within 0.5–2 s in 9 of 12 answers; in 3 it stayed up for 4–5 s until the next prompt (see the piece; one
  case is clearly a second command Codex had already asked for). Answer sessions: not tested.
- PASS · **Edit flow layout (4cc5ce7b)** · header cards fill their columns; the panel is about 400 px; Remove stage
  sits at the panel's foot, tall or short, and the columns scroll sideways.
- PASS · **Regression** · SHOP-29 Dev → Validate → Review → Approve & merge (e471839); Settings opens (all four tools
  Connected); restart dialog after a kill with a Claude Code agent working: "2 agents were stopped by a restart", Resume 1.

New problems: NEW-1 (Needs you line for a long command fills a third of the window), NEW-2 (a Codex prompt drawn on
screen reaches Needs you late, up to ~30 s), NEW-3 (the Otaq reminder typed into Codex's input line while it works,
seen again), and smaller notes.

## Setup
- Build: branch release-fixes at **08caba65** ("Merge piece 23: a key alone answers a Codex prompt in every agent
  session"), brought in as a git bundle, cloned to `%USERPROFILE%\otaq-build\retest3\repo`, built with the shared
  `CARGO_TARGET_DIR` (`%USERPROFILE%\otaq-build\target`): `npm.cmd ci` + `npm.cmd run bundle -- --bundles nsis --config
  noupd.json` → BUILD-EXIT=0, `Otaq_0.1.0_arm64-setup.exe` (18.93 MiB). The running ac5e6200 app and service were
  stopped (no agent was running; an unsaved Edit flow draft on shop, "moved Validate", was Cancelled first), the setup
  installed with `/S` (exit 0; `otaq-app.exe` 01:43, `otaq.exe` 01:41) and started with the release test's `launch.cmd`
  (OTAQ_HOME=`%USERPROFILE%\otaq-reltest\home`) through the `otaqtest` scheduled task. About: "Version 0.1.0 · build
  08caba65 · Built from source". Windows Firewall asked about `otaq` once (Cancel). `setup-about.jpg`,
  `setup-agents-connected.jpg`
- Data: the release test's shop (Otaq default: Dev → Validate (Checker, 3 rounds) → Review, Codex Developer/Checker)
  and notes (Spec first: Product spec, Tech spec, Milestone plan on Claude Code; Spec review; Dev and Validate on Codex;
  Code review). Noyan on Codex. Claude Code, Codex, Copilot and OpenCode signed in and left so. Every Codex command asks
  permission; answered "y" in the app's terminal (never "p", so no Codex rules were added).
- Typing: the VM keyboard is UK, so text was put on the clipboard with `Set-Clipboard` (vmpal_exec) and pasted with
  Ctrl+V; keys like "y" were pressed directly.

## Checks

### PASS · Piece 7 (round 2) · the running chip, tray and Quit count the same
- Set up as re-test 2: SHOP-30 "Retest3 claude working" on Claude Code (Dev with Spec writer instead of Developer; told
  to run `sleep 100` eight times; Claude Code refuses a foreground sleep, so it ran each in the background and the card
  read "Working · in the background"), and SHOP-29 "Retest3 codex idle" on Codex, idle at its prompt after replying
  READY (card "Idle · Answer", strip "Needs you · 1 · SHOP-29 idle"). `p07-claude-agent-background-sleep.jpg`
- Before quitting: chip **"1 running"**, its list "AGENTS RUNNING 1 · SHOP-30 Retest3 claude working · Spec writer"
  with Claude Code 1, Codex 0; rail: shop's running mark 1 (and its Needs you badge 1); tray "NEEDS YOU · 1 · SHOP-29 ·
  idle · **1 agent running** · shop 1"; Quit Otaq → "**1 agent is working.** Quitting keeps it running in the
  background…". `p07-before-chip-list-1.jpg`, `p07-before-tray-1-running.jpg`, `p07-quit-question-1-working.jpg`
- Keep them running, reopened with launch.cmd: chip "1 running", list SHOP-30 only, rail 1, tray "1 agent running ·
  shop 1" with SHOP-29 under Needs you, Quit again "1 agent is working" (Cancelled). `p07-reopened-chip-list-1.jpg`,
  `p07-reopened-tray-1-running.jpg`, `p07-reopened-quit-1-working.jpg`
- "Checker · needs you" for a working ticket (re-test 2's NOTE-12): not seen. No checker ran during this check; later,
  while SHOP-29's and NOTE-14's checkers ran, the chip listed nothing wrong that I saw (I did not open the chip at those
  moments, so this is not a full answer).

### PASS · Piece 8 (round 2, decision 020) · a ticket dragged back from Parked carries on
- Codex from Dev: SHOP-29 (Codex idle at its prompt after its restart was resumed) parked (Move… → Backlog → Park) and
  dragged back to Dev. Card "Working" 3 s after the drop, **"Idle · Answer" 11 s later**; Session: "› You were parked
  and are now back. Carry on where you left off; run 'otaq prime' if you need the ticket again." → "READY" → Otaq's
  turn-end reminder → "Waiting for your next message…", "Worked for 9s"; pill Idle. (Its ticket said "reply READY and
  wait", so carrying on meant answering again and stopping.) `p08-codex-dev-unparked-working-3s.jpg`,
  `p08-codex-dev-idle-after-11s.jpg`, `p08-codex-dev-carry-on-session.jpg`
- Codex from another agent stage: NOTE-15 "Retest3 codex spec park" (notes, Product spec run by Developer on Codex)
  answered READY and went Idle; parked from Product spec (dialog said "conversation · resumes on Dev", the note from
  re-test 2 is still there), dragged back to Product spec: "Working · writing the decisions" for about 10 s, then Idle;
  Session shows the carry-on message as the first prompt and "Worked for 19s". `p08-codex-spec-park-dialog-says-dev.jpg`,
  `p08-codex-spec-unparked-working.jpg`, `p08-codex-spec-carry-on-idle.jpg`
- Claude Code: SHOP-30 parked while working (background sleep 2 of 8) and dragged back to Dev (no dialog on the way
  back): card "Working"; Session "› You were parked and are now back. Carry on where you left off…" → "Sleep 2 was cut
  off again when the session was parked… I'm starting sleep 2 again" and it went on through sleep 8. When it finished
  (DONE, then waiting on purpose) the card read **Idle · Answer** and the pill Idle. `p08-claude-park-dialog.jpg`,
  `p08-claude-unparked-working.jpg`, `p08-claude-unparked-carry-on.jpg`, `p08-claude-idle-after-it-stopped.jpg`
- Also seen: after an app-and-service kill, Resume sent "You were interrupted by a restart. Carry on where you left off"
  to SHOP-30 and SHOP-29 (piece 1's path), and both carried on.

### PASS · Piece 17 (round 2) · "drafting a flow" comes back after "y"
- shop → Edit flow → + between Dev and Validate → Update the docs (Developer on Codex, skill update-docs missing) → Ask
  the agent to write it… → Send. Noyan (Codex) opened in a new conversation; the board line "New conversation is
  drafting a flow for shop… Show". `p17-drafting-line.jpg`
- First permission prompt (`Get-Content …\skills\work-ticket\SKILL.md`): the line became "Needs you · Docs stage skill
  wants your permission: Run command: Get-Content … [Answer]", Noyan's header "Waiting for you".
  `p17-needs-you-replaces-line.jpg`
- "y" alone: **1 s later** "Docs stage skill is drafting a flow for shop… Show" was back and the header said "Working";
  still so 3 s and 7 s later while Codex worked. `p17-line-back-1s-after-y.jpg`
- Next prompt (writing the SKILL.md): Needs you again, naming the new command (the whole `@' --- name: update-docs …
  '@ | Set-Content …` text, see NEW-1), header "Waiting for you". `p17-next-prompt-needs-you-new-command.jpg`
- "y": the line came back; Noyan finished (Idle), the stage card read "update-docs is written but not committed yet".
  Cancel in Edit flow: shop's flow unchanged and `.agents\skills\update-docs` gone (`git status` clean).
  `p17-noyan-idle-lines-gone.jpg`

### PARTLY · Piece 23 · a key alone answers a Codex prompt in a helper and a checker session
- Helper: NOTE-14 "Retest3 helpers" (notes; Product spec and Tech spec skipped; Milestone plan on Claude Code wrote a
  2-piece Plan; Spec review approved; Dev on Codex). The agent in charge started helpers 14.1 (docs/one.txt) and 14.2
  (docs/two.txt), both Codex. Sheet → Session lists "IN CHARGE Developer" and "HELPERS 14.1 / 14.2" with their state;
  the strip said "NOTE-14 14.1 needs you". `p23-helpers-started.jpg`, `p23-helper-14-1-prompt-needs-you.jpg`
- Six helper answers ("y" in the helper's terminal), the helper row read at 0.5–2 s:
  - 14.1, 1st prompt → "working" at 1.5 s. `p23-helper-14-1-working-1s-after-y.jpg`
  - 14.2, 1st prompt → "working" at 1.5 s.
  - 14.2, 2nd prompt (`git status --short; git diff --no-index …`) → **still "needs you · a command" at 1.5 s**, while the
    terminal showed the command ran and "Working (24s)"; the next prompt (the commit) came about 4 s later.
    `p23-helper-14-2-still-needs-you-after-y.jpg`
  - 14.2, 3rd prompt (the commit) → **still "needs you · a command" at 0.7 s and 2.2 s**, Codex "Working (29s)"; next
    prompt about 4 s later. `p23-helper-14-2-still-needs-you-after-y-2.jpg`
  - 14.1, 2nd prompt (create + commit) → "working" at 1.5 s. `p23-helper-14-1-working-1-5s-after-y.jpg`
  - 14.1 and 14.2, `otaq helper done` prompts → "being reviewed" within 1 s.
- Checker: SHOP-29 in Validate (Checker, Codex). (Its first checker run read the ticket's test text "reply READY and
  wait" and ended "Check ended without a result"; I rewrote the description and pressed Retry validation.)
  - 1st prompt: pill "Needs you · checker"; "y" → **still "Needs you · checker" at 0.7 s and 2.2 s** while Codex ran it
    and worked; the next prompt came about 5 s later. `p23-checker-prompt-needs-you.jpg`,
    `p23-checker-still-needs-you-2s-after-y.jpg`
  - 2nd prompt (`node test.js …`): "y" → **"Checking · round 1 of 3" at 0.7 s**. `p23-checker-checking-0-7s-after-y.jpg`
  - 3rd prompt (`otaq check pass`): it never showed Needs you before I answered (2 s after it was drawn); "y" → passed,
    SHOP-29 "Ready for you" in Review.
- Checker on NOTE-14 (Validate, Codex): the card said "Asks you · run command: git status --short; git diff --check;
  gi…" while the terminal showed a *different* prompt (`Get-Content docs/one.txt; Get-Content docs/two.txt`). After "y"
  on that one, exactly the card's command was the next prompt, so Otaq had the second request before Codex drew it
  (Codex asks for two commands in a row). The card went "Checking the whole ticket" 2 s after the "y" while the second
  prompt was on screen. On the next "y" the card went "Checking the whole ticket" at once but the sheet's pill kept
  "Needs you · checker" while Codex worked (and the idle Developer in charge was listed "needs you" all through
  Validate). `p23-checker-card-names-next-command.jpg`, `p23-checker-after-y-named-command-shows.jpg`,
  `p23-checker-card-clears-pill-stays.jpg`
- Reading: the "y" path works for both kinds (most answers cleared within 0.7–1.5 s). The answers where Needs you
  stayed are at least partly a second request Codex had already made (the NOTE-14 case shows it); for the two 14.2 cases
  and SHOP-29's first checker prompt I can't tell, because the row names only "a command". Either way the user sees
  Needs you for a prompt that isn't on screen yet, and the card and the sheet's pill disagree for a few seconds.
- Answer sessions: not tested (didn't bring one to a Codex permission prompt). Claude Code still needing Enter: not
  tested.

### PASS · Short regression
- A ticket through Review to Approve & merge: SHOP-29 (Dev → Validate passed → Review "Ready for you") → Approve &
  merge into main: the dialog offered the plain message and "Developer wants to run a command to write the message…
  [Answer in terminal]" (the delivery's Codex at a prompt); Merge → "Merged", main e471839 "SHOP-29: Retest3 codex
  idle". `reg-SHOP-29-ready-for-review.jpg`, `reg-approve-merge-dialog.jpg`, `reg-SHOP-29-merged.jpg`
- Settings opens: Agents (Claude Code, Codex, GitHub Copilot, OpenCode Connected), Git & pull requests (git found; gh,
  az not installed; the pane was empty for about 2 s the first time, as in re-test 2), About (build 08caba65).
  `setup-agents-connected.jpg`, `reg-settings-git-empty-2s.jpg`, `reg-settings-git-after-6s.jpg`
- The restart dialog after a restart with an agent running: app and service killed with SHOP-30 working (Claude Code)
  and SHOP-29 idle (Codex) → 1.5 s after launch "**2 agents were stopped by a restart** · SHOP-30 Retest3 claude working
  · was working in Dev" (ticked), "SHOP-29 still waits for your answer; answering resumes it.", "Resume 1". Resume 1 →
  the row said "Resumed", SHOP-30 "Working · in the background" and its Session got "You were interrupted by a restart.
  Carry on…". (The dialog stayed open after Resume, showing "Resumed" and a greyed "Resume 0", until Not now.)
  `reg-restart-dialog.jpg`, `reg-restart-resumed.jpg`

## New findings
- NEW-1 (piece 17) · The Needs you line on the board for Noyan's prompt prints the whole command, which for a
  `@' … '@ | Set-Content` SKILL.md write is the full skill text: the line grew to about a third of the window height and
  pushed the board down until answered. `p17-next-prompt-needs-you-new-command.jpg`
- NEW-2 (piece 23 / 19) · A Codex prompt drawn on screen often reaches Needs you late: a helper's `git status …` prompt
  read "working" for 4–14 s, `otaq helper done` prompts read "working" for 3 s and more, NOTE-14's checker prompt read
  "Checking" for about 30 s, NOTE-14's Dev prompts read "Working". `p23-helper-next-prompt-row-says-working.jpg`,
  `new-checker-prompt-drawn-still-checking.jpg`
- NEW-3 · Otaq's reminder "Otaq: you ended your turn but your stage is still open…" was typed into Codex's input line
  while Codex was already "Working (2s)" after I pressed Esc on its prompt (re-test 2 saw the same after a Resume); it
  was then sent as a message. `new-reminder-typed-while-working-again.jpg`
- NOTE · NOTE-14's Developer in charge, idle at its prompt after Dev ended, was listed "needs you" during Validate and
  "working" in Code review (it was idle both times). `p23-checker-card-clears-pill-stays.jpg`
- NOTE · With a ticket's sheet open, the card behind it can lag: NOTE-15's card read "Working · writing the decisions"
  while the sheet's pill read Idle (≥3 s); after closing the sheet the card read Idle.
  `new-card-behind-sheet-working-pill-idle.jpg`
- NOTE · The Park dialog for a ticket parked from Product spec says "conversation · resumes on Dev" (as re-test 2 saw
  for Update the docs); dragged back it resumed in Product spec. `p08-codex-spec-park-dialog-says-dev.jpg`
- NOTE · A closed ticket's stage dots: NOTE-15, closed from Product spec, shows Product spec, Tech spec, Milestone plan
  and Dev ticked and Spec review / Validate grey. `new-closed-ticket-stage-dots.jpg`
- NOTE · A test ticket whose description says "reply READY and wait" also makes its checker do that: SHOP-29's first
  Validate ended "Check ended without a result · Retry validation". Expected, but the card's "Validate gave no result ·
  decide" was clear. `new-checker-ended-without-result.jpg`
- NOTE · Claude Code (v2.1.322) blocks a foreground `sleep N` ("Blocked: standalone sleep…"), so a "keep busy" test
  agent runs in the background and reads "Working · in the background"; it counted as running in chip, tray and Quit.

## Setup changes (what was put back)
| Change | Why | State now |
|---|---|---|
| Installed app ac5e6200 → 08caba65 → 4cc5ce7b | the re-test, then the layout check | installed **4cc5ce7b**, running on the release-test data (launch.cmd), no agent running |
| Unsaved Edit flow draft on shop ("moved Validate") found open at the start | in the way | Cancelled; shop's flow unchanged (Dev → Validate → Review) |
| shop: new stage "Update the docs" + skill `update-docs` written by Noyan (piece 17) | piece 17 | Cancelled in Edit flow: stage and `.agents\skills\update-docs` gone, `git status` clean |
| Tickets SHOP-29, SHOP-30, NOTE-14, NOTE-15 | checks | SHOP-29 merged (shop main e471839, adds "Tested in retest 3." to README.md); SHOP-30, NOTE-14, NOTE-15 closed without merging (branches kept, as Close does) |
| Noyan conversation "Docs stage skill" | piece 17 | ended (End agent); in Recently ended |
| `%USERPROFILE%\otaq-build\retest3\` (bundle, repo, logs, setup exe) | build | left, like retest2 |
| Codex permission answers | all prompts answered "y" (this time only), never "p" | no Codex rules added |
| Sign-ins | none changed | Claude Code, Codex, Copilot, OpenCode Connected |
| Scheduled task `otaqtest` | start the app on screen | left (it existed before) |
| Edit flow on shop and notes during the layout check | opened, stages selected | Cancelled both; no draft, flows unchanged |

## Edit flow layout (4cc5ce7b)
Build: `git fetch` of a second bundle into `retest3\repo` (fast-forward 08caba65 → 4cc5ce7b, 3 files: FlowBoard.tsx,
flow_board.css, flow_editor.css), the same `build.ps1` (cargo reused, BUILD-EXIT=0), setup `/S` (exit 0, `otaq-app.exe`
02:16), started with launch.cmd. About: "Version 0.1.0 · build 4cc5ce7b · Built from source". `layout-about-4cc5ce7b.jpg`.
Window 1920 px wide at 100 % (screenshots are scaled to 1280). Before, on 08caba65, for comparison:
`layout-before-08caba65.jpg` (narrow headers inside wide columns, panel ~265 screenshot px).
- PASS · 1 · Header cards fill their column: shop → Edit flow, Validate selected: Backlog, Dev, Validate and Review
  headers span their column edge to edge; the Done header is as wide as the Done ticket cards under it (all other
  columns were empty by then). The same on notes' eight stages and Done column.
  `layout-headers-full-width-panel-400.jpg`, `layout-notes-8-stages-scrollbar.jpg`
- PASS · 2 · The stage panel is about 400 px: 265 screenshot px of 1280 = ~397 px of the 1920 px window, the same for
  Validate, Review and notes' Dev (not squeezed when notes' eight columns don't fit). The columns scroll sideways: notes
  shows a horizontal scrollbar under the columns and scrolling right brings Milestone plan … Done into view beside the
  panel. `layout-notes-8-stages-scrollbar.jpg`, `layout-notes-scrolled-sideways.jpg`
- PASS · 3 · Remove stage at the panel's foot: Validate (taller than the window) shows "Removing waits until no ticket is
  in it" on the left and "Remove stage" on the right at the bottom edge, and they stay there when the panel's content is
  scrolled (Rounds and "Tickets may skip this stage" move up under them). Review (short panel, ending at "Tickets may skip
  this stage" half-way down) keeps the note and the button at the bottom edge, not after the content.
  `layout-tall-panel-scrolled-remove-at-foot.jpg`, `layout-short-panel-remove-at-foot.jpg`
- Note: the note "Removing waits until no ticket is in it" shows also for stages with no ticket in them (Validate,
  Review, notes' Dev were all empty). Windows' "Activate Windows" watermark sits over the panel's bottom right in these
  shots; it is the VM's, not Otaq's.

## State left in the VM
The installed app (**4cc5ce7b**) is running on screen with its service on the release-test data folder; no agent is
running (no codex/claude process). shop: everything Done or closed, main e471839, clean, flow Dev → Validate → Review.
notes: everything Done or closed, main 84406d5, clean, flow Spec first unchanged. Noyan: no conversation open. Sign-ins
unchanged: Claude Code, Codex, Copilot, OpenCode Connected.

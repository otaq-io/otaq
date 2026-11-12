# Re-test 4 · three release-fixes follow-ups on Windows · results (Windows 11 ARM64 VM, build release-fixes 5e8e9fc7)

Date: 2026-11-11, 10:23–10:47 VM time. Screenshots: `shots-retest-4/` next to this file (JPEG; Windows paths with the
user name in Noyan's terminal, the Needs you line and its tooltip are blacked out; the GitHub login on Settings → Agents
is blacked out. Paths shortened to `~\otaq-reltest\…` by the tools are left).

## Summary
Checks: **PASS 4 of 4.**

- PASS · **1 · Turn-end reminder (8c0037fb)** · Esc at a Codex permission prompt (2 times) and Esc during a running
  turn (once): each time Codex first finished stopping ("Working … · Running hook" → "Conversation interrupted", input
  line empty), and only then was the reminder typed: once, as a message, never in the input line. A plain turn end
  (Codex answers READY and stops) got the reminder once too. Codex's session file has exactly 4 reminders, one user
  message each, which matches Otaq's 4 "reminder queued" log lines. Two more Esc presses in turns that the reminder
  itself had started got no reminder and went Idle, as reminder.rs intends.
- PASS · **2 · Needs you line (4c441400)** · Noyan's SKILL.md write in Edit flow: "Needs you · Docs stage skill wants
  your permission: Run command: @' … Answer" on one line. The board didn't move, and the tooltip shows the command
  capped with "…". "y" brought "drafting a flow" back within 1 s. A ticket card at a multi-line here-string command reads
  "Asks you · run command: @' …" on one line, and the sheet shows the whole command.
- PASS · **3 · Git & pull requests (34d7cc7a)** · right after a fresh start the pane drew at once with git, GitHub and
  Azure DevOps all "Checking…", then filled in within about 3 s. Closed and reopened, it showed the last results at once
  while Re-check spun.
- PASS · **4 · Quick regression** · "y" at a Codex prompt → Working within 0.7 s. The chip showed "1 running" (only
  SHOP-31, Codex 1). Settings → Agents showed all four tools Connected.

New problems: none. See the notes for small things.

## Setup
- Build: branch release-fixes at **5e8e9fc7** ("Merge: a Needs you line naming a long command stays one line"). An
  incremental git bundle (4cc5ce7b..5e8e9fc7, 23 KB) was fetched into `%USERPROFILE%\otaq-build\retest3\repo` (fast-forward
  to 5e8e9fc7), then built with the same `build.ps1` (shared `CARGO_TARGET_DIR`; `npm.cmd ci` + `npm.cmd run bundle --
  --bundles nsis --config noupd.json`): BUILD-EXIT=0, `Otaq_0.1.0_arm64-setup.exe` (18.95 MiB). A copy of it is kept as
  `retest3\Otaq_0.1.0_arm64-setup-5e8e9fc7.exe`, and the log is `retest3\build-retest4.log`.
- Install: no Otaq app, service or agent was running when I started (the VM had been restarted since re-test 3, so the
  4cc5ce7b app wasn't running). Setup `/S` exit 0 (`otaq-app.exe` 10:29, `otaq.exe` 10:27). Started with the release test's
  `launch.cmd` (OTAQ_HOME=`%USERPROFILE%\otaq-reltest\home`) through the `otaqtest` scheduled task. About: "Version 0.1.0 ·
  build 5e8e9fc7 · Built from source". Windows Firewall asked about `otaq` once (Cancel). `setup-about-5e8e9fc7.jpg`
- Data: the release test's shop (Dev → Validate → Review, Developer/Checker on Codex), with Noyan on Codex. All four tools
  stayed signed in. Every Codex prompt was answered "y" or Esc, never "p", so no Codex rules were added.
- Typing: text was put on the clipboard with `Set-Clipboard` and pasted with Ctrl+V (UK keyboard). The first paste in
  the new-ticket box pasted an older clipboard text; I cleared it and pasted again before creating the ticket.
- The maintainer was watching the VM. Once, Edit flow opened and then closed again on shop while I was between steps
  (not my clicks). I waited and carried on; it didn't affect a check.

## Checks

### PASS · 1 · The turn-end reminder waits until Codex has stopped
Test ticket SHOP-31 "Retest4 codex reminder" (shop, Dev, Developer on Codex) was told to run five commands one at a
time, then edit README.md and commit. Codex ran `node test.js` and `git status --short` without asking and asked about
`git log --oneline -3`. Each new turn that wasn't the reminder's was started with the sheet's "Tell Developer now".
Shots were taken 0.3–0.7 s apart after each Esc, with the sheet's Session tab open.

- **Run 1 · Esc at the `git log` prompt** (turn started by the ticket). At 0.7 s the pill read Working and Codex still
  showed "Working (19s · esc to interrupt) · Running hook", with nothing typed. At 1.4 s Codex had stopped ("Conversation
  interrupted"). The Otaq reminder was then in the transcript as one message, Codex was "Working (0s)", and the input
  line was empty ("Ask Codex to do anything"). Codex went on to retry the command. `c1-run1-prompt-needs-you.jpg`,
  `c1-run1-0-7s-codex-still-stopping.jpg`, `c1-run1-1-4s-reminder-once-input-empty.jpg`, `c1-run1-6s-codex-answers.jpg`
- **Run 2 · Esc at the next prompt** (the turn the reminder started). Codex stopped, and there was no reminder at 0.7 s,
  2.6 s or 20 s: the pill went Idle at about 2.6 s and the strip read "SHOP-31 idle". This matches
  `signals/reminder.rs`: "If the turn the reminder started ends the same way, the ticket shows Idle as before."
  `c1-run2-reminder-turn-interrupted-no-second-reminder.jpg`, `c1-run2-idle-after-20s.jpg`
- **Run 3 · Esc at a prompt** (a turn started by Tell Developer now: "The user added comment 1. Run 'otaq prime'…").
  At 0.7 s Codex was still in "Working (8s) · Running hook". At 1.1 s it showed "Conversation interrupted" with an empty
  input line and **no reminder yet**. At 2.6 s the reminder had been sent once and Codex was "Working (0s)".
  `c1-run3-prompt-after-tell-now.jpg`, `c1-run3-0-7s-running-hook.jpg`,
  `c1-run3-1-1s-stopped-empty-line-no-reminder-yet.jpg`, `c1-run3-2-6s-reminder-once.jpg`
- **Run 4 · Esc during a running turn** (Tell Developer now, then Esc while Codex was "Working (1s)", before any
  prompt). At 0.3 s it still showed "Working (5s)". At 0.7 s it showed "Conversation interrupted", an empty line and no
  reminder. At 1.1 s the reminder had been sent once and Codex was "Working (0s)". 5 s later the reminder's turn was at
  a prompt (Needs you). `c1-run4-turn-working-before-esc.jpg`, `c1-run4-0-3s-still-working.jpg`,
  `c1-run4-0-7s-stopped-empty-line-no-reminder-yet.jpg`, `c1-run4-1-1s-reminder-typed-once.jpg`,
  `c1-run4-5s-reminder-turn-at-prompt.jpg`
- Esc in the reminder's own turns (after run 3, after run 4 and after the plain turn end): Idle, no reminder, the same
  as run 2. `c1-reminder-turn-esc-idle-no-second.jpg`
- **Plain turn end**: Tell Developer now "reply with just the word READY … and stop". Codex ran `otaq prime`, answered
  "READY" and stopped. The reminder followed once, as a message, with the input line empty and the pill Working.
  `c1-plain-turn-working.jpg`, `c1-plain-turn-ready-then-reminder-once.jpg`
- **Files**: Codex's session file (`~\.codex\sessions\2026\11\11\rollout-2026-11-11T10-31-47-….jsonl`) holds the
  reminder text 4 times. Each is one `response_item` plus its `event_msg` (07:32:18, 07:35:15, 07:36:42, 07:38:07 UTC) and
  there are no second copies. Otaq's log (`home\logs\otaq.2026-11-11.log`) has 4 "stage still open at turn end: reminder
  queued" lines, each about 0.8 s before its copy in Codex's file. In re-tests 2 and 3, one queued reminder showed up twice
  in Codex's file.
- The card and pill timing after Esc: in every run the pill went from Needs you to Working within 0.7 s (Codex was
  still stopping then), and to Idle about 2.6 s after Esc when no reminder was due. Nothing showed Needs you longer than
  about 0.7 s after Esc.

### PASS · 2 · A Needs you line naming a long command stays one line
- shop → Edit flow → + between Dev and Validate → Update the docs (Developer on Codex, skill update-docs missing) → "Ask
  the agent to write it" → Send. Noyan (Codex) opened in a new conversation in the left panel. The line read "Docs stage
  skill is drafting a flow for shop… Show". `c2-drafting-line.jpg`
- First prompt (two `Get-Content …\SKILL.md` commands, about 230 characters): "Needs you · Docs stage skill wants your
  permission: Run command: Get-Content C:\Users\…\codex\pl… Answer" on one line, cut with "…" before Answer, with the
  board in place. Hovering showed the whole command in a tooltip. "y" in Noyan's terminal brought back "Docs stage skill
  is drafting a flow for shop… Show" within 1 s, with Noyan Working. `c2-needs-you-one-line-get-content.jpg`,
  `c2-tooltip-get-content.jpg`, `c2-y-drafting-back-1s.jpg`
- **The SKILL.md write** (`@' --- name: update-docs … '@ | Set-Content …`, about 95 lines): "Needs you · Docs stage skill
  wants your permission: Run command: @' … Answer". That's one line, with Noyan's header "Waiting for you"; the board and
  the "This applies at once" note stayed where they were (compare re-test 3's
  `p17-next-prompt-needs-you-new-command.jpg`). The tooltip showed the line with the command's text (line breaks shown
  as spaces) in a box about 250 × 115 px, ending "…instructions and…", so it is capped. "y" brought the drafting line
  back within 1 s. `c2-needs-you-set-content-one-line.jpg`, `c2-tooltip-set-content-capped.jpg`,
  `c2-y-drafting-back-after-write.jpg`
- A third, short prompt (`Get-Content .agents\skills\update-docs\SKILL.md; otaq flow write-skill … --done`) fitted
  whole on the line. After "y", Noyan finished (Idle) and the stage card read "update-docs is written but not committed
  yet".
- Cancel in Edit flow: shop's flow is unchanged (Dev → Validate → Review), `.agents\skills\update-docs` is gone, and
  `git status` is clean. The Noyan conversation was ended with End agent (it's in Recently ended).
  `c2-noyan-ended-flow-cancelled.jpg`
- **Ticket card**: SHOP-31 was told (Tell Developer now) to write a file with a multi-line `@' … '@ | Set-Content
  docs/retest4.txt` command. The card read "Asks you · run command: @' …" on one line, and the strip read "SHOP-31 allow
  a command". The sheet's "Developer wants to run a command" box shows all 7 lines. I didn't answer this prompt; the
  ticket was closed instead, so the file was never written. `c2-card-here-string-first-line.jpg`,
  `c2-card-sheet-shows-whole-command.jpg`

### PASS · 3 · Settings → Git & pull requests draws at once
- About 30 s after the 5e8e9fc7 app started (the first time Settings was opened in this run): gear → Agents → Git &
  pull requests. At about 0.5 s and 1.5 s the pane showed the GIT section with "git … Checking…", PULL REQUEST PROVIDERS
  with "GitHub uses gh … Checking…" and "Azure DevOps uses az … Checking…", the Re-check button, and the "Otaq uses them
  to push…" note. It was never an empty pane. `git-checking-0-5s-after-open.jpg`, `git-checking-1-5s.jpg`
- By about 3 s it had filled in: "git 2.56.0.windows.4 · C:\Program Files\Git\cmd\git.exe · Needed by every project"
  (ticked), "GitHub · gh isn't installed" with `winget install GitHub.cli` and Install, and "Azure DevOps · az isn't
  installed" with `winget install Microsoft.AzureCLI` and Install. That's the same as re-test 3. `git-filled-3s.jpg`
- Settings closed and opened again, then Git & pull requests: the filled-in rows were there in the first screenshot
  (under 0.5 s), while Re-check showed its spinner (checking again). `git-reopened-last-results-at-once.jpg`
- I didn't restart the app separately for this check. It was the first Settings open after the install's start, which
  is the cold case.

### PASS · 4 · Quick regression
- SHOP-31 at a Codex prompt (`git log --oneline -3`, Needs you) → "y" → the pill read Working at 0.7 s and Codex ran
  it. `reg-prompt-needs-you.jpg`, `reg-y-working-0-7s.jpg`
- The running chip: "1 running", and its list showed "AGENTS RUNNING 1 · SHOP-31 Retest4 codex reminder · Developer"
  with Claude Code 0, Codex 1, GitHub Copilot 0 and OpenCode 0. When SHOP-31 was at a prompt or Idle the chip was hidden
  and the strip read "Needs you · 1". `reg-chip-1-running.jpg`
- Settings → Agents: Claude Code (v2.1.322), Codex (v0.166.0), GitHub Copilot (v1.0.109) and OpenCode (v2.2.5) all
  Connected. `agents-pane-at-start.jpg`, `reg-agents-opencode-connected.jpg`

## Notes (no new problems)
- NOTE · In the plain turn-end case, Codex's status line under the reminder read "Working (10s …)" right after the
  reminder was typed, not "(0s)" as in the Esc runs. The reminder still arrived once as its own message after "READY",
  with an empty input line, and the session file shows one copy. This is probably just how Codex counts the elapsed time.
  `c1-plain-turn-ready-then-reminder-once.jpg`
- NOTE · Clicking the chip and then pressing Escape to close its list didn't interrupt Codex (the terminal had lost
  focus), which is good.
- NOTE · With a short command, the Needs you line on the ticket card and the Edit flow line show it whole, as before.
- Not tested: a Codex Needs you on a checker's or helper's card with a long command (no checker or helper ran).

## Setup changes (what was put back)
| Change | Why | State now |
|---|---|---|
| Installed app 4cc5ce7b → 5e8e9fc7 | the re-test | installed **5e8e9fc7**, running on the release-test data (launch.cmd), no agent running |
| `retest3\repo` fast-forwarded to 5e8e9fc7; `retest3\retest4.bundle`, `build-retest4.log`, `Otaq_0.1.0_arm64-setup-5e8e9fc7.exe` added | build | left, like before |
| Ticket SHOP-31 "Retest4 codex reminder" (4 comments, "Tell Developer now") | check 1, 2 (card), 4 | closed without merging; its worktree was removed (nothing was committed or written; `git worktree list` shows only main and Otaq's scratch). Branch kept, as Close does |
| shop: new stage "Update the docs" + skill `update-docs` written by Noyan | check 2 | Cancelled in Edit flow: stage and `.agents\skills\update-docs` gone, `git status` clean, main e471839 |
| Noyan conversation "Docs stage skill" | check 2 | ended (End agent); in Recently ended |
| Codex permission answers | "y" or Esc only, never "p" | no Codex rules added |
| Sign-ins | none changed | Claude Code, Codex, Copilot, OpenCode Connected |
| Scheduled task `otaqtest` | start the app on screen | left (it existed before) |

## State left in the VM
The installed app (**5e8e9fc7**) is running on screen with its service on the release-test data folder
(`otaq-app.exe` and `otaq.exe` only; no codex, claude, copilot or opencode process). shop: board with Backlog, Dev,
Validate and Review empty, Done 27 (SHOP-31 closed), main e471839, clean, flow Dev → Validate → Review. notes:
untouched. Noyan: no conversation open. Sign-ins unchanged.

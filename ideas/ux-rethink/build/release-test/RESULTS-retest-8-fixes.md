# Re-test 8 · fix check (NEW-a1, NEW-a2) on Windows · results (Windows 11 ARM64 VM, build 4147a741)

Date: 2026-11-12, 15:39–15:53 VM time. Build: `Otaq_0.1.0_arm64-setup-4147a741.exe`. 4147a741 is a test snapshot
("r8 test snapshot: NEW-a1/a2 fixes"); `git diff 826d4f7f 4147a741 -- crates` is empty, so its code under `crates/`
is identical to the pushed `release-fixes` commits **37e54088** (NEW-a1) and **826d4f7f** (NEW-a2). Data: the normal
release-test data (`otaq-reltest\home`, `launch.cmd` via `otaqtest`), project shop (Codex agents). Phase B (part b)
was paused for this. Screenshots `shots-retest-8/fix-*.jpg`; the one shot whose terminal showed a full path with the
Windows user name is cropped above that line (`fix-a2-ctrl-q-in-session-terminal-asks.jpg`); no other shot shows a
user name, login, email or token. Codex prompts answered "y" only. No code changed, nothing committed or pushed.

## Summary
PASS 10 · FAIL 0 · NOT TESTED 1 · NOTE 4. Both fixes hold. No new blocker.

| Check | Result |
|---|---|
| Install 4147a741 (`/S`), only the test app + its service stopped, About | **PASS** · About "Version 0.1.0 · build 4147a741 · Built from source" |
| NEW-a1 · Overview, 2 options + Other… → "1–3 pick · ↵ sends" | **PASS** |
| NEW-a1 · Overview, 3 options + Other… → "1–4 pick · ↵ sends" | **PASS** |
| NEW-a1 · Overview, `--multiple` 3 options → "Pick any · 1–4 tick · ↵ sends"; 2 options → "Pick any · 1–3 tick" | **PASS** |
| NEW-a1 · Needs you strip popover (Noyan's question, 2 options) → "1–3 pick · ↵ sends"; same above Noyan's terminal | **PASS** |
| NEW-a1 · send-back question keeps "1–2 pick" | **NOT TESTED** in the UI (see below); code unchanged there |
| NEW-a2 · Ctrl+Q on the board with a ticket agent working → "Quit Otaq?" dialog (same as the tray's) | **PASS** |
| NEW-a2 · Ctrl+Q with focus in the Session tab's terminal, agent working → the dialog | **PASS** |
| NEW-a2 · Cancel in that dialog keeps the app; Keep them running quits and the agent goes on | **PASS** |
| NEW-a2 · Ctrl+Q with nothing working: Settings dialog, merge dialog, Session terminal, Terminal drawer → quits at once | **PASS** |
| NEW-a2 · Settings → Keyboard: recording Ctrl+Q refused "Ctrl+Q quits Otaq. Press another key.", key unchanged | **PASS** |
| The other Otaq (`~\otaq-qbox`) untouched throughout | **PASS** (its app PID 8380 and service PID 18580, started 13:44, alive at the end) |
| After the reinstall: SHOP-42's Codex session and the shop Noyan conversation | **NOTE** (see below) |

## Checks

### Install · PASS
- Before: the test app `%LOCALAPPDATA%\Otaq\otaq-app.exe` (PID 2288) and its service from `otaq-reltest\home\runtime`
  (PID 5308), plus the qbox app (`~\otaq-qbox\app`, PID 8380) and its service (PID 18580). Killed only 2288 and
  `taskkill /T` 5308 (its Codex children went with it). Setup `/S` exit 0: `otaq-app.exe` 15:37 (new), `otaq.exe`
  still 14:44 (unchanged by the fix, so the runtime folder and service build id stay `0cc7efc54e9b…`). Started with
  `schtasks /run /tn otaqtest` (task runs `launch.cmd`). No firewall prompt came (the runtime folder didn't change).
- About: **"Version 0.1.0 · build 4147a741 · Built from source"**. `fix-about-4147a741.jpg`

### After the reinstall (point 4) · NOTE
- At start: dialog **"2 agents were stopped by a restart · Each can continue its same conversation, in its own copy.
  Resuming uses tokens."** with one checkbox row "Noyan · Whole dollar README · conversation" and the line **"SHOP-42
  still waits for your answer; answering resumes it. They wait in Needs you if you close this."** Not now / Resume 1.
  `fix-restart-2-agents-stopped-dialog.jpg`
- Not now: SHOP-42's card **"Needs you: session ended by restart · answering resumes it"** with an Answer button;
  strip chip "SHOP-42 allow a command"; its sheet pill **"Interrupted"** with a **Resume** button on Overview (the
  permission prompt itself isn't shown there). `fix-shop42-interrupted-resume.jpg`
- The Noyan conversation "Whole dollar README 15:40" sits under **RECENTLY ENDED** in Noyan's panel with Resume. It was
  not resumed (a new conversation was used for the strip check).
- Later restarts (after each Ctrl+Q) showed no such dialog: the service kept running and the sessions stayed live.

### NEW-a1 · the answer box's hint · PASS
Asked with `otaq ask human --stdin` from a PowerShell with `OTAQ_TICKET` set (caller: the user).
- SHOP-42 (Dev): 2 options (Alpha + note, Beta, recommended 1) → three rows 1–3 and **"1–3 pick · ↵ sends"**.
  `fix-a1-overview-2-options-1-3-pick.jpg`, `fix-a1-overview-2-options-zoom.jpg`
- 3 options (recommended 2) → four rows and **"1–4 pick · ↵ sends"**. `fix-a1-overview-3-options-1-4-pick.jpg`
- `--multiple`, 3 options, recommended 1,3 → checkboxes, 1 and 3 ticked, **"Pick any · 1–4 tick · ↵ sends"**.
  `fix-a1-overview-multiple-pick-any-1-4-tick.jpg`; 2 options → **"Pick any · 1–3 tick · ↵ sends"**.
  `fix-a1-overview-multiple-2-options-1-3-tick.jpg`
- Strip popover: a new Noyan conversation (Codex) was asked to run `'R8 fix check: Noyan two options?' | otaq ask human
  --stdin --option Yes --option No` (one Codex permission prompt, "y"). Above Noyan's terminal **"1–3 pick ↵ sends"**
  (`fix-a1-noyan-panel-2-options-1-3-pick.jpg`); the strip chip "Agent · a question" opened the popover "Noyan · Test
  step, no work asks" with Yes 1 / No 2 / Something else 3 and **"1–3 pick · ↵ sends"**, Answer in terminal, Send
  answer, "Open in the agent panel". `fix-a1-strip-popover-noyan-1-3-pick.jpg`. Answered Yes from the popover.
- Send-back · NOT TESTED in the UI: a send-back needs a ticket past an earlier stage. Asked on SHOP-42 in Review with
  `--send-back Dev`: accepted and stored (open in `human_questions`), but shown nowhere — the same as **NEW-b2**
  (questions asked in Review don't show). The code's send-back hint is unchanged ("1–2 pick", `question_bar` in
  `answer_options.rs`).

### NEW-a2 · Ctrl+Q · PASS
- **Agent working, board**: SHOP-42's Checker running ("1 running", card "Checking"), focus on the board after closing
  the sheet: Ctrl+Q → native **"Quit Otaq? · 1 agent is working. Quitting keeps it running in the background; Quit and
  stop agents stops it where it is (Resume later)." Keep them running / Quit and stop agents / Cancel**.
  `fix-a2-ctrl-q-on-board-asks.jpg`. Keep them running → the window quit; the Checker went on (after restart the card
  had moved on to its next prompt).
- **Agent working, Session terminal**: focus in SHOP-42's Session tab terminal while it was "Checking · round 1 of 3":
  Ctrl+Q → the same dialog. `fix-a2-ctrl-q-in-session-terminal-asks.jpg`. **Cancel** → app stays, nothing changed.
  `fix-a2-cancel-keeps-app.jpg`
- **Nothing working** (quits at once, no question, as the tray's Quit Otaq): in Settings (Keyboard page, a dialog);
  in the Approve & merge dialog; with focus in the Session terminal while the Codex agent sat at a permission prompt;
  with focus in the Terminal drawer's shell. Each time only the test app's window went; its service stayed.
- The tray icon's **Quit Otaq** in the same "agent at a prompt" state also quit at once without asking, so Ctrl+Q
  matches it (Quit's question counts ticket agents at work only: `agent_counts` → `works_now`).
- **Keyboard**: Settings → Keyboard → click "Start / move to Dev" (Ctrl+D) → "Press the new keys…" → Ctrl+Q: red line
  **"Ctrl+Q quits Otaq. Press another key."**, the app didn't quit; Esc → Ctrl+D unchanged.
  `fix-keyboard-record-ctrl-q-refused.jpg`
- The qbox Otaq: its window came to the front after each test-app quit; no key was sent while it had focus; its PIDs
  8380/18580 (started 13:44) were alive at the end.

## Notes
- NOTE-f1 · A question asked (CLI, as the user) on a ticket whose session had ended by restart (SHOP-42 "Interrupted")
  or on a Backlog ticket (SHOP-41) is stored but not shown; SHOP-42's appeared once Resume restarted its session.
  Only the CLI-as-user path does this (an agent asks from a live session), so a note.
- NOTE-f2 · "With a live session it asks first" doesn't hold literally: Ctrl+Q (and the tray's Quit Otaq) ask only
  while a ticket agent is working. An agent waiting at a prompt, a Noyan conversation that is Working (tried once:
  quit at once while Noyan showed "Working"), and a drawer shell don't count. They live in the service, which keeps
  running after the quit, so nothing is lost. As designed (`quit_question`), consistent with the tray.
- NOTE-f3 · NEW-b1 seen again: the Running chip goes while the agent waits at a prompt.
- NOTE-f4 · No firewall prompt on this install: `otaq.exe` didn't change, so its runtime folder didn't either.

## New problems
None.

## Setup changes and state left
| Change | State now |
|---|---|
| Installed app 0cc7efc5 → **4147a741** (`%LOCALAPPDATA%\Otaq`; `otaq.exe` unchanged) | **running on the normal release-test data** via `otaqtest` / `launch.cmd` (started 15:51); service PID 192 (started 15:40) |
| shop SHOP-42 "Say prices are in whole dollars in README" | resumed, Developer added the README line (commit 7fec26f on otaq/SHOP-42), Checker passed → **Review · Ready for you**, not merged. The merge-message writer waits at a permission prompt (seen in the Approve & merge dialog; not answered) |
| shop SHOP-41 | unchanged (Backlog, waits for SHOP-42) |
| Test questions asked by CLI (SHOP-41 "two options?", SHOP-42 several, the send-back) | SHOP-42's earlier ones were replaced by the next; the two left open (SHOP-41's, SHOP-42's send-back) closed by hand in `otaq.db` (`human_questions.answered_via='closed'`, node:sqlite); no open questions left |
| Noyan: old "Whole dollar README" | ended by the reinstall, in RECENTLY ENDED, not resumed |
| Noyan: new conversation "Test step…" (shop, Codex) | asked one test question, answered Yes; idle, live |
| Terminal drawer | Terminal 1 (shop) open, drawer closed |
| Clipboard | the Noyan test message (no secrets) |
| The other Otaq (`~\otaq-qbox`) | untouched, running |

# Re-test 7 · release round 6 on Windows · results (Windows 11 ARM64 VM, build release-fixes f080c8f5)

Date: 2026-11-12, 02:53–03:47 VM time. Plan `tuq/plans/release-round6.md` (five pieces, decisions 044–046; 010, 040,
041, 042 kept), issues in `issues-round6/`. Screenshots: `shots-retest-7/` next to this file (JPEG). The Windows user
name is blacked out wherever a Codex prompt or a shell printed a full path (`c2-push-*`, `c2-sendback-*`, `c5-*`,
`core-push-to-pr-done-*`, `new-after-cancel-*`, `new-attach-*`, `obs5b-question-item-opens-session-tab-shop37`), the
GitHub login on Settings → Agents, and the Claude sign-in URL (`r5p7-sign-in-*`). Paths shortened to `~\otaq-reltest\…`
by the tools are left. No email or token was on screen. No Otaq code was changed, nothing committed in the repo or
pushed to anything real (the only remote is the local bare `hooksproj-remote.git`). Nothing was signed in or out.

## Summary
Checks: **PASS 4 · PARTLY 2 · FAIL 0 · not tried 1** of 7 (pieces 1–5, round 5 piece 7, check 1's Claude Code side),
plus the core-path walk (PASS). Every Check of round 6 passes on what it lists, except `otaq attach` (piece 3). Four new
problems, one of them on the core path (NEW-4).

| Check | Result |
|---|---|
| 1 Push to PR, Retry push and a conflict restack start the skill's turn on Codex; a multi-line message is sent whole | **PASS** (Codex, by clicking the buttons): `$otaq:push` submitted 2 of 2, Retry submitted 1 of 1, a bare `$otaq:restack` submitted 1 of 1, a multi-line answer went in as one message. The restack first waited behind a leftover writer prompt (NEW-4) |
| 2 Push to PR's prompt reads Needs you, stays in Review; "y" → Ready for you; writer's prompt only in Approve; Send back over a prompt names it | **PASS** on all listed points (Preparing and Pushing; Send back and Push to PR over a prompt → "A permission prompt is waiting…" · Keep it and cancel; Approve & merge over an open writer prompt merges). Gaps beside it: after Cancel on the PR dialog the still-running skill turn's prompts read nowhere (NEW-3); a here-string command is named "$ @' …" (NEW-5) |
| 3 Answer session's prompt: Needs you on the answering ticket, a click opens it, "y" answers, answer reaches the asker; `otaq attach`; Cleanup | **PARTLY**: everything in the app PASS; **`otaq attach` FAIL**: Windows Terminal's replies to colour queries are typed into the Codex session; in the answer session the Esc in them declined the prompt and the session ended (NEW-1) |
| 4 Killing an answer session puts the question in Needs you on the asker; Resume in History → Messages restarts it | **PASS** (kill, restart after reinstall, and a turn that ended unanswered). But a resumed answer session answered the old question's id and was refused (NEW-2) |
| 5 Codex Shift+drag survives, Ctrl+C copies and keeps the draft; Ctrl+Shift+C; PowerShell copy/interrupt; link hover | **PASS** (link: underline with Ctrl held, as designed). `ping -t` in Otaq's PowerShell isn't stopped by Ctrl+C (2 of 2), the known issue's Start-Sleep case (NEW-6) |
| Round 5 piece 7 · login expired → Sign in… | **PARTLY**: Claude Code is signed out in the VM; a ticket on a Claude Code stage is held "Claude Code isn't available · signed out" with **Sign in…** at once, and Sign in… opens `claude`'s login. A login that expires mid-session, and Resume after sign-in, not tried |
| Check 1's Claude Code side (restack waits behind pasted text, then goes alone) | **not tried**: Claude Code isn't signed in in the VM (`claude auth status`: loggedIn false; Settings: "Found · not signed in"); I didn't sign in |
| Core-path walk: start, Needs you, review, Approve & merge with a hook, Push to PR with pre-push/commit-msg, Edit flow, Settings | **PASS** (one stall on the way: NEW-4) |

Point 5 of the brief: (a) the asking Codex ticket reads **"Idle"** (Needs you "SHOP-37 idle", Answer button), never
"asked SHOP-35 · waiting" — not a core-path blocker. (b) confirmed: the question's Needs you item opens the asker's
**Session tab**; the Overview doesn't mention the question; Answer / Resume are only in History → Messages — not a
core-path blocker. Details under "Point 5".

New problems: NEW-1 `otaq attach` types terminal replies into the session (and Ctrl+] doesn't detach); NEW-2 a resumed
answer session answers the previous question's id; NEW-3 Cancel on the PR dialog leaves the skill turn running unseen;
**NEW-4 (blocker) a writer prompt open at Approve & merge, when the merge hits a conflict, stays calm in Dev and the
restack waits behind it with nothing on the board**; NEW-5 a here-string command is named "@' …"; NEW-6 Ctrl+C doesn't
stop `ping -t` in Otaq's PowerShell; NEW-7 a user's answer is typed as "SHOP-35 answered your question". Notes NOTE-1…3.

## Setup
- Build: branch release-fixes at **f080c8f5** ("Merge piece 4: a question whose answer session ended unanswered goes to
  Needs you"). An incremental bundle `d0a165b5..release-fixes` (80 KB, sent with vmpal_send_files) was fetched into
  `%USERPROFILE%\otaq-build\retest-hooks` (fast-forward d0a165b5 → f080c8f5, 77 files) and built with the existing
  `retest-hooks-build.ps1` (shared `CARGO_TARGET_DIR`, `npm.cmd ci` + `npm.cmd run bundle -- --bundles nsis --config
  noupd.json`): BUILD-EXIT=0, `Otaq_0.1.0_arm64-setup.exe` 19.4 MB, kept as
  `otaq-build\Otaq_0.1.0_arm64-setup-f080c8f5.exe`, log `otaq-build\retest7-build.log`.
- Install: the running d0a165b5 app and its service were stopped (`taskkill /T` on the service also ended HOOK-5's Dev
  session and the four Noyan conversations from re-test 6; no piece-agent Codex processes were left), setup `/S`
  exit 0, started with `launch.cmd` through the `otaqtest` task on the release-test data (`otaq-reltest\home`).
  Firewall prompt for the new runtime folder: Cancel. About: "Version 0.1.0 · build f080c8f5 · Built from source".
  `setup-about-f080c8f5.jpg`
- Data as in re-test 6: shop (Dev → Validate → Review, Codex, merge check `node test.js`), notes (Spec first, Claude Code
  spec stages), hooksproj **HOOK** (hooks in `.git/hooks` logging to `retest-hooks-lib\hooks-log.txt`; flow PR skill
  `local-pr`, origin = local bare repo). For Retry push, pre-push got a temporary line that fails while
  `retest-hooks-lib\prepush-fail.flag` exists (restored afterwards).
- New tickets (all `otaq create`): SHOP-37 "Retest7 asker" (depends on SHOP-35), HOOK-6 "Retest7 hooks merge", NOTE-20
  "Retest7 sign-in check". The question in 4 was asked with `OTAQ_TICKET=SHOP-37 otaq ask SHOP-35 --stdin`.
- Typing went through `Set-Clipboard` + Ctrl+V or key presses, each paste checked; clipboard sentinels before each copy
  check. Codex prompts answered "y" (Esc twice: a Codex misreading in 1, and the sign-in popup), never "p".
- Settings → Agents: Codex v0.166.0, GitHub Copilot v1.0.109, OpenCode v2.2.5 Connected; **Claude Code v2.1.322 "Found ·
  not signed in"** (it was Connected in re-test 6; I didn't change it). `reg-settings-agents-claude-not-signed-in.jpg`

## Checks

### PASS · 1 · A skill Otaq types on its own into Codex is sent
- **Push to PR** (HOOK-5, Review, PR #2, "1 new commit since the PR was opened"; its Dev session was restarted by the
  reinstall): the dialog "Push skill is preparing…", and the session shows **`› $otaq:push` submitted**; Codex worked
  at once (it read the restack skill, since HOOK-5 conflicted with main). A second Push to PR later: submitted again, the
  agent went straight to `otaq pr propose`. `c1-push-to-pr-preparing.jpg`, `new-cancelled-push-prompt-unseen-ready-for-you.jpg`
  (the `› $otaq:push` line), `c2-second-push-prompt-named-as-heredoc-start.jpg`
- **Retry push**: with the fail flag on, the push was refused by pre-push ("pre-push: integration FAILED (retest7
  flag)"), the agent asked `otaq ask human`, and the dialog read **"The skill stopped: HOOK-5 wasn't pushed … Retry"**.
  Flag removed → Retry → the skill's turn started at once (agent at `git push origin otaq/HOOK-5-add-modulo`), pre-push
  ran its 50 s, "integration ok", `cfbe503..69aa0f8`, `otaq pr opened …/pull/2`, ticket "PR open".
  `c1-prepush-failed-skill-stopped-retry.jpg`, `c1-retry-push-starts-turn-prompt.jpg`, `core-pushing-prepush-50s.jpg`,
  `core-push-to-pr-done-prepush-ok-pr-open.jpg`
- **Conflict restack**: a commit on shop's main (623cc1f, README line) against SHOP-37's README line; Approve & merge →
  "Merging main in stopped — conflict · README.md", SHOP-37 back to Dev "Conflicts with main · fixing", header "1 message
  waiting · Deliver now". The message waited (the writer's prompt was still open, see NEW-4); once that turn ended, a
  **bare `$otaq:restack` was typed and submitted**, Codex merged main, resolved README, committed f679353 and ran `otaq
  stage next`. `c1-conflict-restack-waits-behind-writer-prompt.jpg`, `c1-conflict-restack-submitted-codex-works.jpg`
- **Multi-line message**: answering SHOP-37's question in History → Messages typed "SHOP-35 answered your question
  01a1cd16… ("Retest 7 question: …"):", a blank line and the answer, as **one submitted message** (Codex "Working").
  `c1-multiline-answer-typed-whole-submitted.jpg`
- Claude Code side: not tried (see Summary).

### PASS · 2 · A prompt during a delivery skill turn reads Needs you while the ticket stays in Review
- **Preparing** (first Push to PR): Codex stopped at `Get-Content …\skills\restack\SKILL.md`. Pill **Needs you**, strip
  "Needs you · 2 · HOOK-5 allow a co…", card "…command: Get-Content …", ticket in Review; the PR dialog: **"The agent is
  waiting for your permission to run a command"** with the command and **Answer in terminal** (it opens a "HOOK-5 · Push
  to PR" terminal popup). `c2-push-skill-prompt-dialog-waits-pill-strip.jpg`
- **"y" → Ready for you**: on the second Push to PR the agent stopped at `otaq pr propose`; "y" → pill **Ready for you**,
  strip without HOOK-5, and the proposal (2 commits) in the dialog. `c2-after-y-ready-for-you-proposal.jpg`
- **Pushing**: "Applied your edits · 2 commits reworded · Pushing to origin…" then "The agent is waiting for your
  permission · `$ git push origin otaq/HOOK-5-add-modulo`" with Answer in terminal; pill Needs you, card names the
  command. Same for its `otaq ask human` prompt. `c2-pushing-prompt-named-git-push.jpg`, `c2-pushing-ask-human-prompt.jpg`
- **Push to PR over an open prompt**: "A permission prompt is waiting in HOOK-5's terminal · The agent wants to run
  Get-Content … Answer it in its terminal, then try again; nothing was sent." · only **Keep it and cancel**.
  `c2-push-over-prompt-names-it-keep-and-cancel.jpg`
- **Send back over an open prompt**: the Send back dialog opens as usual; on **Send back to Dev** the same "A permission
  prompt is waiting…" dialog with only Keep it and cancel; nothing was sent. `c2-sendback-over-prompt-names-it-keep-and-cancel.jpg`
- **Writer's prompt (decision 010)**: SHOP-37 in Review, the message writer at `git log --oneline main..HEAD; …`:
  session header "Waiting for you", pill "Ready for you", strip "Your review · SHOP-37 ready", no Needs you. The Approve
  dialog shows "Developer wants to run a command to write the message. Merge now with this one, edit it, or wait… ·
  Answer in terminal". `c2-writer-prompt-review-calm-header-waiting.jpg`, `c2-approve-dialog-shows-writer-prompt.jpg`
- **Approve & merge over an open prompt**: twice over the writer's open prompt — the first ran into the conflict (1,
  NEW-4), the second merged SHOP-37 (2cb82cc) and ended the session ("stopped"). `c2-approve-dialog-writer-prompt-2.jpg`,
  `c2-approve-merge-over-writer-prompt-merged.jpg`
- Beside the check: Cancel on the PR dialog while the agent waited → NEW-3; the dialog naming "$ @' …" → NEW-5.

### PARTLY · 3 · An answer session can be seen and reached
- Answer session from SHOP-36's question (see 4): SHOP-35's Done card **"answering SHOP-36"**, then at Codex's
  `git show` prompt **"Needs you · answering SHOP-36"** with the Needs you edge, still in Done; strip "Needs you · 1 ·
  SHOP-35 allow a command"; the asker SHOP-36 had no Needs you for it. `c3-done-card-needs-you-answering-strip.jpg`
- **A click on the strip item opens SHOP-35's Session tab** on the answer session, header "Waiting for you", footer
  "Typing goes to the agent · you're here, so no alert" (041). `c3-strip-click-opens-answer-session-prompt.jpg`
- **Cleanup** (Settings → Data → Review…) while it waited: lists only `wt\shop\SHOP-5` and `wt\shop\SHOP-33`, not SHOP-35.
  `c3-cleanup-doesnt-list-answering-shop35.jpg`
- **"y" answers it**: "y" on `git show 181e24e …`, then "y" on `… | otaq answer 01a1cba2… --stdin`; the session ended,
  and SHOP-36's Messages read "SHOP-35 → SHOP-36 · 'git show 181e24e -- README.md' confirms I added: 'Tested in retest
  6.' · **Answered by resuming SHOP-35's session · 16s**". `c3-answer-session-prompt-focused-no-alert.jpg`,
  `c3-answer-reached-asker-messages.jpg`
- **`otaq attach SHOP-35` — FAIL**: started in a Windows Terminal window (scheduled task, `OTAQ_HOME` set) while the answer
  session was at its first prompt. It attached, and at once the input line read `]10;rgb:cccc/cccc/cccc\]11;rgb:0c0c/
  0c0c/0c0c\` — Windows Terminal's answers to the colour queries in the replayed output, typed into Codex. The Esc in them
  declined the prompt ("You canceled the request to run git show …", "Conversation interrupted"), and the window printed
  "[SHOP-35's session ended: stopped]" (the session row: `answer`, ended 52 s after start, `stopped`); the question went
  back to Needs you. `c3-FAIL-attach-osc-replies-cancel-prompt-session-stopped.jpg` → NEW-1

### PASS · 4 · A question whose answer session ended unanswered goes to Needs you
- **After a restart**: re-test 6's killed answer session (SHOP-36 → SHOP-35) came up right after the reinstall as
  "Needs you · SHOP-36 a question"; History → Messages: "Sent to you · SHOP-35's answer session ended without answering"
  with **Answer** and **Resume SHOP-35**. Resume → "Delivered · SHOP-35 is finished, resumed answer-only", one running.
  `c4-restart-stale-question-needs-you-strip.jpg`, `c4-restart-history-messages-ended-without-answering.jpg`,
  `c4-resume-shop35-delivered-answering.jpg`
- **A turn that ended unanswered** (the attach case in 3): back to Needs you at once; Resume worked again.
  `c3-resumed-again-needs-you.jpg`
- **Kill**: SHOP-37 (Codex, Dev) asked SHOP-35 a new question; the answer session stopped at a prompt
  (`c4-answer-session-at-prompt-before-kill.jpg`). I killed only that Codex (`taskkill /T` on its node process, the
  `kind=answer` session's pty): within 5 s the strip read **"Needs you · 2 · SHOP-37 idle · SHOP-37 a question"**.
  `c4-killed-question-needs-you-on-asker.jpg`
- **Resume SHOP-35** in SHOP-37's History → Messages restarted it ("Delivered · SHOP-35 is finished, resumed
  answer-only"; SHOP-35 "answering SHOP-37", then "Needs you · answering SHOP-37"). `c4-history-messages-answer-resume.jpg`,
  `c4-resume-restarts-answer-session.jpg`
- It never restarted by itself. SHOP-35's re-made worktree is gone at the end (it was clean).
- But the resumed session didn't answer the new question → NEW-2. I answered it myself (Answer box in Messages).
  `c4-answer-it-yourself-box.jpg`

### PASS · 5 · In a Codex terminal on Windows, a selection survives until Ctrl+C
- SHOP-37's live Codex session, draft "my draft r7" in the line, clipboard SENTINEL-R7-1. Shift+drag over "Waiting for
  your instruction in this session", then the mouse moved away (to the sheet header) and back over the terminal: **the
  highlight stayed**. Ctrl+C → clipboard "Waiting for your instruction in this session", **draft kept**, no interrupt.
  `c5-shift-selection-survives-mouse-moves.jpg`, `c5-ctrl-c-copied-draft-kept.jpg`
- Shift+drag "Worked for 1m 39s • 03:07", moves away and back, **Ctrl+Shift+C** → clipboard "Worked for 1m 39s • 03:07"
  (was SENTINEL-R7-2), draft kept. `c5-ctrl-shift-c-selection-zoom.jpg`
- PowerShell (the ticket's Terminal tab): drag "hello-r7-shell" → Ctrl+C → clipboard "hello-r7-shell" (was SENTINEL-R7-3),
  no ^C. Ctrl+C with nothing selected at the prompt: "abc^C", new prompt. `c5-shell-selection-before-ctrl-c-zoom.jpg`,
  `c5-shell-ctrl-c-at-prompt-zoom.jpg`. A running `ping -t 127.0.0.1` was **not stopped** by Ctrl+C (2 of 2; ended with
  taskkill) → NEW-6, `c5-shell-ping-running-after-ctrl-c.jpg`
- **Link**: Codex printed `https://example.com/retest7` (dashed underline at rest, Codex's OSC 8 link). A plain hover
  changes nothing (by design, `links.ts`: "Holding the key underlines the link"); hover with **Ctrl held → solid
  underline and "Ctrl-click to open"**. The shell's plain URL likewise shows nothing on a plain hover.
  `c5-codex-link-at-rest.jpg`, `c5-codex-link-ctrl-hover-underline.jpg`

### PARTLY · Round 5 piece 7 · Sign in… when a tool's login is gone
- Claude Code is signed out in the VM (not by me). The notes board shows a banner "Claude Code isn't available · signed
  out · 1 ticket waiting" once NOTE-20 was started; its card "Can't start · Spec writer's model can't run: Claude Code
  isn't…"; the sheet: **Held** · "Claude Code isn't available · signed out · Nothing started and nothing is locked. Spec
  writer starts once Claude Code is available again." · **Open agent… · Sign in… · Change agent…**.
  `r5p7-claude-signed-out-banner-card.jpg`, `r5p7-held-signed-out-offers-sign-in.jpg`
- **Sign in…** opens a "Claude Code sign in" terminal ("Opening browser to sign in… If the browser didn't open, visit:
  <url> · Paste code here if prompted >") and the browser. I closed both (Esc; a `claude` process stayed until I ended
  it, NOTE-2). `r5p7-sign-in-opens-claude-login-url-redacted.jpg`
- Not tried: a login that expires while an agent runs, and the ticket carrying on after a sign-in. NOTE-20 was moved back
  to Backlog.

## Point 5 of the brief
- **(a) The asking ticket while its question waits.** SHOP-37 (Codex, Dev, idle at its line when it asked) read
  **"Idle"** on the card, with an **Answer** button and the strip item "SHOP-37 idle"; never "asked SHOP-35 · waiting"
  (decision 045), not after the ask, not while SHOP-35 answered or waited at its prompt. SHOP-35's card did read
  "answering SHOP-37". `obs5a-asker-reads-idle-not-asked-waiting.jpg`. **Not a core-path blocker** (questions between
  tickets are off the core path; the ticket still works), but it calls the user to an agent that is waiting on another
  ticket, and its Answer button answers the idle agent, not the question. Proposed known issue.
- **(b) Where the question's Needs you item leads.** Clicking "SHOP-36 a question" / "SHOP-37 a question" opens the
  asker's **Session tab** (SHOP-36: "No session"); the Overview says nothing about the question; Answer and Resume
  SHOP-35 are only in **History → Messages**. `obs5b-question-item-opens-asker-session-tab.jpg`,
  `obs5b-question-item-opens-session-tab-shop37.jpg`, `obs5b-overview-no-question-shown.jpg`. **Not a core-path
  blocker**; it makes decision 046's "answer it yourself / Resume" hard to find. Proposed known issue.

## Core-path walk — PASS
- **Start**: SHOP-37 by its Start button, HOOK-6 by dragging to Dev; both Codex, both at once **Needs you** at the first
  prompt ("Asks you · run command: git rev-parse --git-path hooks", strip "allow a command"). A click on the strip opens
  the session focused. `core-shop37-started-prompt-needs-you.jpg`, `core-hook6-drag-started-needs-you.jpg`
- **Review → Approve & merge with a hook** (hooksproj): HOOK-6 committed `fix: add retest 7 line (HOOK-6)` through
  pre-commit/commit-msg; Move → Review (skip Validate); Approve & merge with the first line set to `fix: add retest 7
  line (HOOK-6)` (the default "HOOK-6: Retest7 hooks merge" would fail the feat/fix commit-msg hook, as in re-test 6) →
  Merged, main `eeef153`; hook log 03:43:01–02 in the scratch worktree: post-checkout, prepare-commit-msg, commit-msg.
  `core-hook6-merge-dialog-fix-message.jpg`, `core-hook6-merged-with-hooks.jpg`. shop: SHOP-37 merged (2cb82cc) after
  its restack.
- **Push to PR on the local bare remote with pre-push and commit-msg**: see 1 and 2 — the merge commit (03:23:20) and two
  rewords (03:26:58) ran commit-msg; pre-push failed on purpose (03:27:25), then passed after Retry (03:28:36, 50 s);
  remote `otaq/HOOK-5-add-modulo` = 69aa0f8 "Merge branch 'main' into otaq/HOOK-5-add-modulo".
- **Edit flow** (shop): opens, stage details panel (Validate: Checker, validate, fails to Dev), Cancel restores.
  `reg-edit-flow-stage-details.jpg`
- **Settings**: Agents (above), Data → Cleanup (3), About. 
- The one stall on the way is NEW-4.

## New problems
- **NEW-1 (piece 3) · `otaq attach` types the terminal's replies into the agent's session, and Ctrl+] doesn't detach.**
  From Windows Terminal (PowerShell profile, UK keyboard), `otaq attach SHOP-35` and `otaq attach HOOK-5` both left
  `]10;rgb:cccc/cccc/cccc\]11;rgb:0c0c/0c0c/0c0c\` in the Codex input line (2 of 2): the terminal answers the OSC 10/11
  colour queries in the replayed output and `attach` forwards the answers as input (`crates/otaq-cli/src/attach.rs`
  filters nothing but the detach key). In the answer session the reply's Esc declined the open prompt and the answer
  session ended (question → Needs you); in HOOK-5's idle session the junk stayed in the line, in the app too, until I
  deleted it (Otaq's notes would wait behind it as "unsent text"). Ctrl+] (0x1d) didn't detach (2 tries); closing the
  window ended the attach and the session kept running. **Blocker: no** — `attach` is the CLI way in; the app's Session
  tab reaches every session (piece 3 passed there). But it breaks what it attaches to; fix before people use it.
  `c3-FAIL-attach-osc-replies-cancel-prompt-session-stopped.jpg`, `new-attach-normal-session-osc-replies-in-line-no-detach.jpg`,
  `new-attach-junk-left-in-hook5-line-in-app.jpg`
- **NEW-2 (pieces 3/4) · A resumed answer session answers the previous question's id.** SHOP-35 had already answered
  SHOP-36's question (01a1cba2…). For SHOP-37's new question (01a1cd16…) the answer session resumes the same Codex
  conversation and is started with a bare `$otaq:answer-question` (the Codex rollout shows no id or text in the turn);
  both times (the first ask and the Resume) Codex skipped `otaq prime` and ran `… | otaq answer 01a1cba2… --stdin` with
  the old answer; Otaq refused it ("Question 01a1cba2… was answered already"), the turn ended, and the question went to
  Needs you. So a second question to the same finished ticket is never answered by its agent (2 of 2). **Blocker: no**
  (off the core path; decision 046's Needs you and "answer it yourself" catch it). Likely fix: name the question in the
  typed request (`$otaq:answer-question <id>`), as other skills get their ticket.
  `new-resumed-answer-session-answers-old-question-id.jpg`, `new-resumed-answer-session-prompt-old-id.jpg`,
  `new-old-id-refused-question-back-in-needs-you.jpg`
- **NEW-3 (piece 2) · Cancel on the PR dialog leaves the skill turn running, unseen.** With the push skill at a prompt,
  Cancel closed the dialog; the ticket went back to "Ready for you", the strip dropped HOOK-5, the running chip went —
  but Codex still sat at the prompt (header "Waiting for you"). Answered in the session, the cancelled turn carried on
  for 7 min of prompts nobody was shown: it merged main into the branch, resolved src/calc.js, committed the merge, then
  `otaq pr propose` was refused "HOOK-5 has no Open PR in progress". (The Send back dialog at that point opened normally
  and caught the prompt only on confirm.) **Blocker: no** — the user cancelled, and Push to PR without Cancel works; but
  Cancel should stop or interrupt the turn, or the prompt should keep reading Needs you. Same family as re-test 6's
  "Enter after Cancel runs a skill whose flow is gone". `new-after-cancel-prompt-open-ticket-ready-for-you.jpg`,
  `new-cancelled-push-prompt-unseen-ready-for-you.jpg`, `new-cancelled-push-turn-continues-refused.jpg`,
  `new-after-cancel-sendback-plain-dialog-over-prompt.jpg`
- **NEW-4 (piece 2 / decision 010) · A writer prompt open at Approve & merge stays calm after a conflict sends the
  ticket to Dev, and the restack waits behind it.** SHOP-37 in Review, its Codex message writer at a prompt (calm, as 010
  says); Approve & merge (the dialog offers "Merge now with this one") → main had moved with a conflicting README line →
  SHOP-37 back to Dev "Conflicts with main · fixing", restack queued ("1 message waiting · Deliver now"). The writer's
  prompt was still open, and it **never read Needs you**: card "Conflicts with main · fixing", strip "Needs you · 0
  here", Overview only "Conflicts with main … Open agent session", for 1 min+ until I opened the session; only the header
  said "Waiting for you". After "y", the **next** prompts did read Needs you; when the writer's turn ended the restack was
  typed. So the 010 calm sticks to a prompt drawn in Review after the ticket leaves Review. **Blocker: yes** — core path
  (review → merge) on Codex, which asks before every command, so the writer is usually at a prompt when you approve; a
  moved, conflicting main is common; the result is a ticket that reads "fixing" while nothing happens and nothing calls
  the user. Fix direction: drop the writer's calm (and end the writer turn, or show its prompt as Needs you) when the
  ticket leaves Review. `c1-conflict-restack-waits-behind-writer-prompt.jpg`, `new-fixing-card-no-needs-you-writer-prompt-open.jpg`,
  `new-fixing-overview-no-prompt-shown.jpg`, `new-fixing-session-writer-prompt-restack-waiting.jpg`,
  `new-next-prompt-reads-needs-you.jpg`
- **NEW-5 (piece 2) · A here-string command is named by its first line.** The PR dialog and card named `otaq pr
  propose`'s prompt "$ @' …" / "command: @' …" (Codex wraps stdin in a PowerShell here-string); the user can't tell
  what is asked without opening the terminal. **Blocker: no.** `c2-second-push-prompt-named-as-heredoc-start.jpg`
- **NEW-6 · Ctrl+C doesn't stop a running `ping -t` in Otaq's PowerShell terminal** (2 of 2; at the prompt "^C" works,
  as in re-test 6). Same as the known issue's Start-Sleep case, now seen in Otaq's own terminal. **Blocker: no** (piece
  5 doesn't touch keys; known-issue family). `c5-shell-ping-running-after-ctrl-c.jpg`, `c5-shell-ctrl-c-doesnt-stop-ping.jpg`
- **NEW-7 · An answer you give is typed as the other ticket's.** After I answered SHOP-37's question in History →
  Messages, Otaq typed "SHOP-35 answered your question 01a1cd16…"; Codex then took it for a question to answer and
  started `answer-question` work (I declined with Esc). **Blocker: no.** `c1-multiline-answer-typed-whole-submitted.jpg`
- NOTE-1 · HOOK-5's Overview still reads "Conflicts with main · 1 file (src/calc.js) · HOOK-3 changed src/calc.js" after
  main was merged into its branch and pushed (69aa0f8). Maybe stale; not checked further.
- NOTE-2 · Closing the "Claude Code sign in" popup with Esc left a `claude` process (the login) running.
- NOTE-3 · Claude Code is no longer signed in in the VM (Connected in re-test 6); round 5 piece 7's expiry case and
  check 1's Claude Code side need a signed-in Claude Code (or a Mac run).

## Proposed known-issue additions (for issues-round6/README.md; not edited)
- NEW-1 `otaq attach` from Windows Terminal types the colour-query replies into the session; Ctrl+] doesn't detach.
- NEW-2 A second question to the same finished ticket: the resumed answer session answers the old id.
- NEW-3 Cancel on the PR dialog leaves the skill turn running, its prompts unseen.
- NEW-5 A here-string command is named "@' …".
- NEW-6 Ctrl+C doesn't stop `ping -t` in Otaq's PowerShell (merge with the Start-Sleep line).
- NEW-7 A user's answer is typed as "SHOP-35 answered your question".
- Point 5(a): the asking ticket reads "Idle" (and Needs you) instead of "asked … · waiting".
- Point 5(b): the question's Needs you item opens the Session tab; Answer/Resume only in History → Messages.
- **Not a known issue: NEW-4** — a release blocker under the bar (core path, Codex).

## Setup changes (what was put back)
| Change | Why | State now |
|---|---|---|
| Installed app d0a165b5 → **f080c8f5** | the re-test | installed f080c8f5, running on the release-test data (launch.cmd via `otaqtest`) |
| `retest-hooks` fast-forwarded to f080c8f5; `retest7.bundle`, `retest7-build.log`, `Otaq_0.1.0_arm64-setup-f080c8f5.exe` in `otaq-build` | build | left |
| hooksproj `.git/hooks/pre-push`: fail line while `prepush-fail.flag` exists | Retry push (1) | restored from the original; flag removed |
| hooks log | 1, core path | header line "### 03:17 retest 7 (f080c8f5) starts…" added |
| shop: setup commit 623cc1f "Test setup: README conflict (retest 7)" | conflict restack (1) | not reverted; SHOP-37 merged on top (2cb82cc); main clean |
| Scheduled task `otaqattach7` + `attach7.cmd` | `otaq attach` (3) | deleted |
| HOOK-5's Codex line: attach junk | NEW-1 | deleted with Backspace |
| Tickets | checks | SHOP-37 merged (asked SHOP-35; question answered by me). HOOK-5 Review "PR #2 open", branch pushed 69aa0f8. HOOK-6 merged (eeef153). NOTE-20 back in Backlog. SHOP-36's old question answered by SHOP-35's resumed session. HOOK-4 unchanged (Needs you · branch isn't on the remote) |
| Re-test 6's four Noyan conversations and HOOK-5's old session | ended by stopping the service for the install | — |
| Claude sign-in popup | r5 piece 7 | closed; browser and the `claude` login process ended; nothing signed in |
| Codex answers | "y", Esc twice, never "p" | no Codex rules added |

## State left in the VM
The installed app (**f080c8f5**) is running on screen with its service on `otaq-reltest\home`. One Codex process runs:
HOOK-5's Dev session (live, idle, PR open). shop: Backlog, Dev, Validate, Review empty, main 2cb82cc clean. notes:
NOTE-18 in Dev (start failed), NOTE-19 and NOTE-20 in Backlog. hooksproj: main eeef153, HOOK-4 and HOOK-5 in Review.
Codex, Copilot, OpenCode Connected; Claude Code "Found · not signed in" (as found).

## Follow-up · build e8ddb1b5 (round 6 piece 6)

Date: 2026-11-12, 10:15–10:41 VM time. Re-test of NEW-4 on the release build of piece 6 (decision 010 refined
2026-11-12), plus a short regression. Screenshots `shots-retest-7/r7b-*.jpg` (no Windows user name, login, email,
token or sign-in URL on any of them). No Otaq code changed, nothing committed in the repo, nothing pushed anywhere.

| Item | Result |
|---|---|
| Setup: build, install, About | **PASS**: About "Version 0.1.0 · build e8ddb1b5 · Built from source" |
| NEW-4: writer prompt open at Approve & merge, conflict → Dev reads Needs you; "y" → fixing; restack typed after the writer's turn; Codex resolves; back to Review; merge | **PASS** (all points, watched through to the merge) |
| Regression: writer prompt in Review shows only in the Approve dialog | **PASS** (twice) |
| Regression: Push to PR skill prompt reads Needs you while in Review (check 2) | **PASS**; beside it a new stall after a cancelled Push to PR (NEW-8, not a blocker) |

### Setup
- Bundle `f080c8f5..release-fixes` (16 KB) fetched into `otaq-build\retest-hooks` (fast-forward to e8ddb1b5), built
  with `retest-hooks-build.ps1`: BUILD-EXIT=0, setup kept as `otaq-build\Otaq_0.1.0_arm64-setup-e8ddb1b5.exe`, log
  `otaq-build\retest7b-build.log`. The f080c8f5 app and its service stopped (`taskkill /T`; this ended HOOK-5's idle
  Codex session), setup `/S` exit 0, started with `launch.cmd` via the `otaqtest` task on `otaq-reltest\home`.
  Firewall prompt: Cancel. `r7b-setup-about-e8ddb1b5.jpg`
- New ticket **SHOP-38** "Retest7b writer conflict" (`otaq create`, Codex, Dev → Validate → Review): append
  "Tested in retest 7b (SHOP-38)." to README.md. Every Codex prompt answered "y" (Esc once, see NEW-8), never "p".

### PASS · NEW-4 on e8ddb1b5
- SHOP-38 went Dev → Validate (passed) → Review. In Review the message writer (in SHOP-38's own Dev session) stopped
  at `git rev-parse --git-path hooks`: pill **Ready for you**, strip "Your review · 1 · SHOP-38 ready", no Needs you.
  `r7b-review-writer-prompt-calm-ready-for-you.jpg`
- I committed a conflicting README line on shop's main (908d9dd "Test setup: README conflict (retest 7b)"), then
  **Approve & merge** with the writer's prompt still open. The dialog showed "Developer wants to run a command to
  write the message. Merge now with this one, edit it, or wait… · Answer in terminal". `r7b-approve-dialog-shows-writer-prompt.jpg`
- **Merge** → "Merging main in stopped — conflict · README.md · SHOP-38 went back to its agent…"; at once SHOP-38 was
  **In Dev** with pill **Needs you** and strip **"Needs you · 1 · SHOP-38 allow a command"**; header "1 message waiting
  · Deliver now". `r7b-new4-conflict-dialog-dev-needs-you.jpg`
- Board: SHOP-38's card in Dev with the Needs you edge, **"Asks you · run command: git rev-parse --git-path hooks"**
  and Open. `r7b-new4-card-strip-needs-you-names-writer-command.jpg`. Overview: "Conflicts with main · 1 file
  (README.md) · 908d9dd changed README.md · SHOP-38 returns to Review when it's fixed" **and** "Developer wants to run a
  command · $ git rev-parse --git-path hooks · Answer in terminal". `r7b-new4-overview-conflict-and-writer-prompt.jpg`
- "y" → the writer's next prompt (`git diff --stat main...HEAD`) read Needs you on the card ("Asks you · run command:
  git diff --stat main...HEAD"). `r7b-new4-next-writer-prompt-needs-you.jpg`. While the writer ran a command between
  prompts the pill read **"Conflicts with main · fixing"** (Live, "1 message waiting").
  `r7b-new4-after-y-fixing-restack-waiting.jpg`. Its later prompts (`git diff main...HEAD -- README.md`,
  `git log -1 --format=%B HEAD`, the here-string `… | otaq pr message --stdin`) each read Needs you again.
  (With the terminal focused the strip shows "0 here" while the card and pill say Needs you — decision 041's focus
  rule, the same as at the first Dev prompt.)
- When the writer's turn ended, **`$otaq:restack` was typed and submitted by itself** ("1 message waiting" gone).
  `r7b-new4-writer-turn-ends-restack-typed.jpg`. Codex: `git merge main` → read README → kept both lines → merge
  commit 553bb71 → `otaq stage next` ("Merged main and resolved README.md…"). `r7b-new4-codex-resolved-conflict-stage-next.jpg`
- Validate passed again ("on the first try"), SHOP-38 back in **Review**, the writer again at a prompt and calm.
  `r7b-new4-back-in-review-writer-prompt-calm.jpg`
- **Approve & merge** (again over the writer's open prompt, "Merge now with this one") → **Merged**: main 460021c
  "SHOP-38: Retest7b writer conflict" on 908d9dd; README ends with both lines; the worktree is gone; no Codex process
  left. `r7b-new4-merged.jpg`
- Whole walk 10:23–10:33 (about 10 min, Codex prompts answered as they came).

### PASS · Regression · writer prompt in Review shows only in the Approve dialog
- Both times SHOP-38 sat in Review with the writer at a prompt: card "Ready for you · passed Validate" (magenta edge),
  strip "Your review · SHOP-38 ready", no Needs you; the Approve dialog named it with Answer in terminal.
  `r7b-reg-review-card-calm-writer-prompt-open.jpg`, `r7b-review-writer-prompt-calm-ready-for-you.jpg`,
  `r7b-approve-dialog-shows-writer-prompt.jpg`

### PASS · Regression · Push to PR's skill prompt reads Needs you in Review
- HOOK-5 (Review, PR #2) had no Push to PR offered (no new commits). I committed one line on its branch
  (0e9976c "fix: note retest 7b push check (HOOK-5)", through the hooks), sent it back with PR comments ("No code change
  needed…"), Codex ran `otaq stage next`; the Validate checker wanted to fail it for the old HOOK-3 changes, so I
  declined that one prompt (Esc) and used Move → Review (skip Validate). The footer then offered **Push to PR #2** ("1 new
  commit since the PR was opened").
- **Push to PR #2** → "Push skill is preparing…", `$otaq:push` submitted; at the agent's `otaq pr propose` prompt: pill
  **Needs you**, strip "Needs you · 2 · HOOK-5 allow a co…", card "…command: @' …", **still in Review**, and the dialog
  "The agent is waiting for your permission to run a command · $ @' … · Answer in terminal" (the here-string name is
  NEW-5, unchanged). `r7b-reg-push-skill-prompt-needs-you-in-review.jpg`
- Answer in terminal → "HOOK-5 · Push to PR" popup → "y": pill **Ready for you**, strip without HOOK-5, Codex "Proposed the
  message for unpushed commit 0e9976c. Otaq is waiting for your confirmation". But the PR dialog was gone with the
  popup (I pressed Esc once to close the popup; whether that Esc also cancelled the PR dialog, or the dialog closed
  by itself, I couldn't tell), so the proposal was never shown → NEW-8.

### New problem
- **NEW-8 · After a cancelled Push to PR, the next Push to PR waits "Push skill is preparing…" forever.** After the
  dialog above closed with the proposal pending, **Push to PR #2** again (2 of 2): `$otaq:push` submitted, Codex ran
  `otaq prime` and ended its turn with "Otaq still shows commit 0e9976c awaiting confirmation. I haven't pushed; please
  confirm the proposal in Otaq to continue" — but the dialog stayed at **"Push skill is preparing… · push · reading the
  commits"**, New commits (0), Push disabled, for 1 min+ with the agent idle and no hint; only Cancel. (`otaq prime` for
  HOOK-5 from a shell lists 0e9976c under "New commits to propose messages for… Propose with `otaq pr propose --stdin`",
  so Codex likely went by its own memory of the cancelled turn.) The dialog doesn't notice that the skill's turn ended
  without a proposal (re-test 7 saw "The skill stopped… Retry" when the agent asked; here it never says). **Blocker:
  no** — it needs a Cancel (or a lost dialog) mid-proposal first; the plain Push to PR path passed in re-test 7. Same
  family as NEW-3. Possible extra: one Esc in the "Answer in terminal" popup may close the PR dialog too (not
  confirmed). `r7b-new8-push-after-cancel-preparing-forever.jpg`

### Not tried
- The Claude Code side of the writer-prompt case (Claude Code still "Found · not signed in" in the VM).
- A failed **Merge check** sending the ticket to Dev with the writer's prompt open (decision 010 names it beside the
  conflict); only the conflict was tried.
- Finishing HOOK-5's push (I stopped at NEW-8 to save Codex quota: Codex reports "monthly limit: 24% left").

### Setup changes and state left
| Change | State now |
|---|---|
| Installed app f080c8f5 → **e8ddb1b5** | installed, running on `otaq-reltest\home` (launch.cmd via `otaqtest`; no new scheduled task made) |
| `retest-hooks` → e8ddb1b5; `retest7b.bundle`, `retest7b-build.log`, `Otaq_0.1.0_arm64-setup-e8ddb1b5.exe` in `otaq-build` | left |
| shop: setup commit 908d9dd; SHOP-38 merged (460021c) | main clean at 460021c |
| HOOK-5: my commit 0e9976c on its branch (not pushed); sent back, Validate skipped, back in Review | Review "Ready for you · skipped Validate", Push to PR offered, Codex session idle; remote still 69aa0f8 |
| Clipboard | holds my last note text (no secrets) |

At the end: e8ddb1b5 running with its service; one Codex process (HOOK-5's idle session); HOOK-4 unchanged (Needs you ·
branch isn't on the remote); notes unchanged.

## Follow-up · Claude Code on Windows (build e8ddb1b5)

Date: 2026-11-12, 11:25–11:51 VM time (with a short pause for the maintainer at 11:43). Claude Code v2.1.322 signed in
by the maintainer. Its default here is **auto mode**, which shows no permission prompts, so where a prompt was needed
I switched that one session to manual mode with Shift+Tab. Screenshots `shots-retest-7/r7c-*.jpg` (no Windows user
name, login, email, token or sign-in URL on them; the Copilot login is cropped off the Settings shot). No Otaq code
changed, nothing committed in the repo, nothing pushed anywhere. Claude Code agent used: **Architect** (Claude Code,
model default), picked per ticket or put on hooksproj's flow for the time of the test.

| Check | Result |
|---|---|
| 1 Settings → Agents Claude Code Connected; `claude auth status` logged in | **PASS** (`v2.1.322 · signed in`, Connected; `loggedIn: true`) |
| 2 Round 5 piece 7: NOTE-20 starts once signed in; "signed out" callout gone | **PARTLY**: NOTE-20 (back in Backlog since re-test 7) started at once, with no banner or callout, and ran Product spec → Tech spec → Milestone plan to Spec review in about 1 min. Resume *from the held state* not tried (the hold was gone; I can't sign out) |
| 3 Round 5 check 1 on Claude Code: pasted text + one Backspace, conflict restack waits, then typed alone | **PASS** (paste with Ctrl+Shift+V; Ctrl+V pastes nothing, NEW-10) |
| 4 Round 6 piece 1 on Claude Code: Push to PR / conflict restack start the skill's turn; a multi-line message is sent whole | **PARTLY / FAIL**: conflict restack **PASS** (`/otaq:restack` alone, submitted, worked); a 413-character two-line answer arrived whole (**PASS**); but the 1,296-character message-writer request arrived **without its first 1,024 characters** (2 of 2), **FAIL** → NEW-12. Push to PR not reachable (NEW-13) |
| 5 Round 6 piece 2 on Claude Code: Push to PR skill prompt → Needs you in Review | **not tried**: no PR could be opened on Claude Code in this test data (NEW-13) |
| 6 Round 6 piece 6 on Claude Code: writer prompt open at Approve & merge into a conflict | **not tried**: no writer prompt could be had. On Claude Code the writer ran no command at all (it got a fragment of its request, NEW-12); HOOK-7's writer ran on Codex (NEW-11) |
| 7 Core path on Claude Code | **PASS** for start → Needs you at a prompt → review → Approve & merge with the hooks (HOOK-7, and HOOK-8/HOOK-9 too). Card text: NEW-9. Push to PR on the bare remote **not tried** (NEW-13) |

### Details
- **Check 1**: `r7c-settings-agents-claude-code-connected.jpg`.
- **Check 2**: `r7c-note20-start-signed-in-working.jpg`, `r7c-note20-claude-code-ran-to-spec-review.jpg`. NOTE-20 is left
  in Spec review.
- **Core path, HOOK-7** ("Retest7c Claude Code merge", `otaq create --stage Dev=Architect`): dragged to Dev; session
  switched to manual; Claude's first `git status …` prompt → pill **Needs you**, strip "HOOK-7 allow a command", card
  "Asks you · claude needs your permission" (NEW-9); a click on the strip opened the session focused; "1" answered;
  commit `fix: add retest 7c line (HOOK-7)` passed pre-commit/commit-msg. The Codex checker sat at "Working" for 2+ min
  showing nothing, so I used Move → Review (skip Validate), here and for HOOK-8/9. Approve & merge with the first line
  changed to `fix: …` (commit-msg hook) → **Merged 03a594b**; hook log 11:40:02–03: post-checkout, post-merge,
  pre-commit, prepare-commit-msg, commit-msg, post-commit. `r7c-core-hook7-claude-prompt-session-needs-you.jpg`,
  `r7c-core-hook7-card-asks-you-not-naming-command.jpg`, `r7c-core-hook7-merge-dialog-fix-message.jpg`,
  `r7c-core-hook7-merged-with-hooks.jpg`
- **Check 3** (HOOK-7 in Review, its Claude session idle): clipboard "unsent draft r7c please wait" (checked; the
  shared Mac clipboard overwrote it twice, so a loop held it while pasting). **Ctrl+V pasted nothing** (3 tries,
  NEW-10); **Ctrl+Shift+V** pasted it as one chunk; one Backspace → "unsent draft r7c please wai". Setup commit on main
  d3af1ad (README last line) → Approve & merge → "Merging main in stopped — conflict · README.md", HOOK-7 in Dev
  "Conflicts with main · fixing", header **"1 message waiting · Deliver now"**. After 15 s the line still held only my
  text. Emptied with Backspace → **`/otaq:restack` alone, submitted** (transcript: the user turn is exactly
  `/otaq:restack`); Claude's `git merge main …` prompt then read **Needs you** on the card and strip while "fixing"
  (round 5 piece 3 on Claude Code: PASS); resolved as merge 2ad292f, `otaq stage next`.
  `r7c-c3-pasted-ctrl-shift-v-one-backspace.jpg`, `r7c-c3-conflict-restack-1-message-waiting.jpg`,
  `r7c-c3-15s-later-restack-not-typed-after-text.jpg`, `r7c-c3-c4-line-emptied-restack-typed-alone-submitted.jpg`,
  `r7c-c4-restack-turn-merge-prompt.jpg`, `r7c-fixing-prompt-reads-needs-you-card-strip.jpg`
- **Multi-line message** (check 4): HOOK-8's agent asked `otaq ask human`; my two-line, 413-character answer (pasted in
  the Overview's answer box) arrived in Claude's transcript **whole**, both lines, ANS-START … ANS-END; Claude ran
  `otaq stage next`. A "Tell Architect now" comment goes as a short notice ("The user added comment 2. Run `otaq
  prime`"), which arrived whole.
- Claude Code's grey suggestion in an empty line ("push it") was never taken for unsent text.
- **Open PR** on HOOK-8 (after putting hooksproj's delivery agent on Architect): toast "Open PR for HOOK-8 didn't go
  through · Claude Code doesn't have the skill '/local-pr'"; the ticket stayed in Review. `r7c-open-pr-claude-code-lacks-local-pr-skill.jpg`
- HOOK-8 and HOOK-9 were merged with the plain message (first line set to `fix: …`): 898aff8, f980652.

### New problems
- **NEW-9 · A Claude Code permission prompt's card doesn't name the command.** The card reads "Asks you · claude needs
  your permission" (strip "allow a command"), while a Codex prompt's card names it ("run command: git status --short;
  …", `r7c-codex-checker-card-names-command-for-comparison.jpg`). Decision 040 says the card names the command on
  screen. **Blocker: no**: Needs you shows and leads to the prompt; only the name is missing.
- **NEW-10 · Ctrl+V doesn't paste text into a Claude Code session on Windows** (3 of 3; Ctrl+Shift+V works). Claude Code
  takes Ctrl+V as its image paste; Codex pasted with Ctrl+V in re-test 5. **Blocker: no** (typing and Ctrl+Shift+V
  work), but Ctrl+V is the habit on Windows.
- **NEW-11 · A ticket that picks another Dev agent still gets the flow's delivery agent as its message writer.** HOOK-7
  ran Dev on Architect (Claude Code), but in Review Otaq started a separate Codex "Developer" delivery session to write
  the message (`delivery.agent: Developer`). Neither the Repository card nor the Done stage shows or changes that agent;
  only `otaq flow propose` did. **Blocker: no** (the merge works and the writer is optional), but a user who chose
  Claude Code gets Codex prompts in the Approve dialog.
- **NEW-12 · A long message typed into Claude Code on Windows loses its first 1,024 characters.** The message-writer
  request (`pr/message.rs` `prompt`, one line of 1,296 characters) reached Claude as a 272-character user turn starting
  `escription": what changed and why…` (HOOK-8 and HOOK-9, **2 of 2**, the same cut each time; it is exactly the first
  1,024 bytes that went missing). Claude then guessed: it printed a JSON reply instead of running `otaq pr message`, so
  the Approve dialog stayed at "Architect is still writing the message" and only the plain message could be used.
  Shorter messages arrived whole (413 characters, `/otaq:restack`, notices). The cause is likely chunked input on
  Windows (ConPTY), or Claude Code's handling of a large burst. **Blocker: yes, release-critical, though merging still
  works**: on Claude Code on Windows every review loses its written message (Codex got the same request whole in
  re-test 7b), and an agent acting on the tail of an instruction could do the wrong thing. Any automatic message over
  1 KB is affected. Merge still works with the plain message, so start → review → merge isn't stopped. The writer's
  title and description feed Open PR, which I couldn't try (NEW-13). Check the same on macOS/Linux.
  `r7c-writer-request-arrives-as-escription-hook8.jpg`, `r7c-writer-request-head-lost-hook9-json-reply.jpg`,
  `r7c-writer-json-printed-dialog-still-writing.jpg`
- **NEW-13 · The test project's PR skill can't be used by Claude Code.** hooksproj's `local-pr` lives in
  `.agents/skills/` (where Codex finds it). With Claude Code as the delivery agent, Open PR refuses at once: "Claude
  Code doesn't have the skill '/local-pr'". This is test data more than a bug, and Otaq says it clearly. But the flow
  accepted a delivery agent whose tool can't see the delivery skill (through `otaq flow propose`) without a warning.
  **Blocker: no** (clear message; a project set up for Claude Code would have the skill in `.claude/skills/`). It
  blocked checks 4 (Push to PR), 5 and the PR half of 7.
- NOTE-4 · Claude Code's default here is auto mode: no permission prompts at all, so Needs you at a prompt only shows in
  manual mode. NOTE-5 · An idle Claude session in Validate showed the session header "Waiting for you" with no prompt
  open (the card and pill were right).

### Not tried
- Push to PR, Retry push and a push-skill prompt on Claude Code (checks 4, 5, PR half of 7): blocked by NEW-13.
- A writer prompt on Claude Code (check 6): the writer never ran a command (NEW-12).
- Resume of a ticket held "signed out" (check 2): the hold was gone, and I can't sign out.

### Setup changes and state left
| Change | State now |
|---|---|
| hooksproj flow: Dev agent and delivery agent Developer → Architect (proposal 36) | **put back** to Developer (proposal 37, saved; `otaq flow show` lists Dev/delivery Developer) |
| hooksproj main: setup commit d3af1ad; HOOK-7 03a594b, HOOK-9 f980652, HOOK-8 898aff8 merged | main clean at 898aff8 |
| New tickets HOOK-7, HOOK-8, HOOK-9 | all Merged, worktrees gone, sessions ended |
| NOTE-20 started | in **Spec review**, its Claude Code session idle (not moved back) |
| `%TEMP%\r7c-flow*.yaml` | deleted |
| VM clipboard | my test text or the Mac's |
Running at the end: the e8ddb1b5 app and service, NOTE-20's idle Claude session, HOOK-5's idle Codex session. No
scheduled task made. Claude Code stays signed in; nothing was signed in or out.

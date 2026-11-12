# Re-test 8 · part c · phase C (Ticket) on Windows · results (Windows 11 ARM64 VM, build 4147a741)

Date: 2026-11-12, 16:00–16:25 VM time. Build **4147a741** (installed; its code = release-fixes 826d4f7f). Data: the
normal release-test data (`otaq-reltest\home`, `launch.cmd` via `otaqtest`), projects notes (Spec first) and shop.
Plan: `PLAN.md` phase C, checked against DECISIONS.md (UX-4, 13, 24, 31, 32, 42–45, 54–58, 60, 62, 64), ASSUMPTIONS
A18–A24, A43, A46, `RESULTS-C.md` (ef84b175) and re-tests 7 / 8a / 8b / fixes. Screenshots: `shots-retest-8/c-*.jpg`
(no Windows user name, login, email or token on any of them; paths show as `~\…`). No Otaq code changed, nothing
committed in the repo, nothing pushed anywhere, nothing signed in or out. Codex prompts answered "y" only.

Agents: notes tickets ran Dev and Validate on **Architect = Claude Code** (v2.1.322, signed in, auto mode; Shift+Tab to
manual for one prompt); notes' message writer is the flow's delivery agent **Developer = Codex** (R7 NEW-11). shop's
SHOP-42 (Codex Developer/Checker) was used for the sheet tabs, Send back over a prompt and Approve & merge over the
writer's open prompt. Codex was at 20 % of its month at the start, so Codex runs were kept to the writers.

## Summary
Phase C: **PASS 26 · PARTLY 2 · FAIL 0 · KNOWN 3 (+ NEW-b5, gap #32 inline) · NOT TESTED 6 · NOTE 8 · new problems 3
(none a blocker).** Six of RESULTS-C's ten FAILs are fixed: the send-back reason is the agent's comment, "failed 1
time" (singular), "Send it on / Move on anyway" works on a non-skippable check, the prepared message arrives (Codex)
and no writer is left running after the merge, the conflict banner says what moved / nothing merged / "2 of 3", and
WHAT THE AGENT DID shows the latest round. Still as before: the ⋯ menu has no "Merge into another ticket…" (NOTE; A46
doesn't list it) and a Claude Code prompt isn't named on Overview or the card (KNOWN, R7 NEW-9; Codex prompts are
named). Not re-tried: Codex's told comment ("unsent text") and the merge-check pill.

| Check | Result |
|---|---|
| Sheet header: crumb, title, pill, Move…, ⋯, full window, close; meta line; stepper with skip marks (A19) | **PASS** |
| Tabs Overview · Session · Changes "1 file" · History; Review opens on Overview; review bar on every tab (UX-4, A20) | **PASS** (SHOP-42) |
| Session switch Agent · Validate · Terminal; "Checker · Codex · tool default"; "Ended · passed", read-only (A23) | **PASS** |
| Changes: copy card, file list, summary + report, diff; line comment → "1 line comment being written · Send back with comments" | **PASS** (comment not posted) |
| History: filters, newest first, plain rows, session rows; copy put the full session id on the clipboard (UX-45, A24) | **PASS** |
| UX-62 long question on Overview: bold question line, context with Markdown (bold, code, bullets, numbered list, code block scrolling sideways), ~60 chars wide, question + context scroll as one text | **PASS** |
| UX-62 cap: text capped (~270 px) since Send would fall below, but options + Send still not in view on this 908-px window (also in full window) | **PARTLY → NEW-c1** |
| Single answer from Overview: digit picks, ↵ sends; Claude got "Answer to your question: Node version only (option 2)." | **PASS** |
| UX-64 several answers from Overview: checkboxes, Recommended 1+3 ticked, "Pick any · 1–4 tick", Other… ticked + text, Send off while Other… is empty; Claude got "Unit tests (option 1), Lint (option 2). Also: Empty cart" | **PASS** (digits: NEW-b3, not re-tried) |
| UX-55 / UX-32 agent asks to send back to a skipped Product spec: question first on Overview; Send back → Product spec ran ("no longer skipped" in History with the agent's reason); reason posted as the **agent's** comment | **PASS** (RESULTS-C FAIL fixed) |
| UX-54 Product spec agent with nothing to settle finishes at once | **PASS** |
| Comment vs Tell Architect now (UX-43, UX-58) with attachments (tuq 001–008): two buttons + helper line, picker added 2 files, "· told Architect now", thumbnail + chip Open, Claude read it via `otaq prime` | **PASS** (Claude Code; Codex delivery NOT TESTED) |
| Edit ticket… in place (A46): note line, Title, Priority, locked started stages, Validate selectable, base branch locked, Save → P2 on the card | **PASS** |
| ⋯ menu (A46): Edit ticket…, Ask Noyan about this, Pin to top, Copy branch name, Move to next stage…, Close ticket…, Delete… disabled with reason | **PASS**; no "Merge into another ticket…" **NOTE** (as RESULTS-C) |
| Close ticket… with the agent at a prompt: "Its agent is running. Closing stops it…" → Closed · not merged, Reopen | **PASS** (NOTE-25) |
| Send back over the writer's open prompt (Codex): "A permission prompt is waiting in SHOP-42's terminal … Keep it and cancel" | **PASS** (as R7 check 2); a user's Send back to the end not re-run (part a did) |
| UX-60 checker fail, rounds 1: card "Validate failed 1 time · decide", strip "decide", Decide popover, Stop here keeps it, Overview Move on anyway / Give it another round / Answer the agent… | **PASS** (RESULTS-C "1 times" FAIL fixed) |
| UX-60 "Send it on / Move on anyway" on non-skippable Validate → Code review "Failed 1 time · moved by you" | **PASS** (RESULTS-C FAIL fixed) |
| Approve with "either" delivery: chooser (merge / open PR / merge into another / nothing to merge (disabled, reason) / don't merge) (UX-13) | **PASS** |
| UX-56 / UX-42 merge dialog: plain message + "Developer wants to run a command…" + Answer in terminal; 4 × "y" → written message filled, "Prepared by Developer while NOTE-21 waited in Review" | **PASS** (Codex writer) |
| Merge now with the plain message over an open writer prompt; message edited first (SHOP-42 "(r8c)" in the commit); the writer session ends with the merge | **PASS** (RESULTS-C orphan-writer FAIL fixed) |
| Merge into another ticket… offers NOTE-20, which waits in **Spec review** (before Dev) | **NEW-c2** |
| Done, nothing to merge (no commits): chooser puts it first and enabled, the rest "No changes in the repo"; toast "Done · NOTE-24 · Nothing to merge…" | **PASS**; sheet pill "Merged" **KNOWN (NEW-b5)** |
| Done, don't merge: in the chooser / menu with its line | **PASS** (not run again; part a ran it) |
| UX-57 PR without gh (fake origin): Repository "GitHub · example-org/notes · gh isn't installed · Set up…"; PR dialog "GitHub isn't set up… Set up GitHub…" → Settings → Git & pull requests; Push & open PR disabled; Or merge into main instead | **PASS**; PR title starts with the key **KNOWN (gap #32)** |
| PR dialog's commit-message field shows a two-paragraph message on one line ("…READMECo-Authored-By: Claude Opus…") | **NEW-c3** |
| UX-44 early conflict: NOTE-23 merged → NOTE-22 (waiting in Code review) went back to its agent on its own, restacked, back in ~1 min | **PASS** |
| UX-24 merge-time conflict: "Merging main in stopped — conflict · README.md · went back to its agent"; Overview "Conflicts with main · 1 file · c44323c changed README.md · Nothing was merged into main. NOTE-22 returns to Review when it's fixed. · 2 of 3"; merged after | **PASS** (RESULTS-C banner FAIL fixed) |
| WHAT THE AGENT DID after a restack shows the latest round | **PASS** (RESULTS-C FAIL fixed) |
| A43 Plan & helpers: Plan on Overview as numbered pieces ("Written by you", Click to edit); no Plan line on the card; helpers not run | **PARTLY** (helpers NOT TESTED: cost) |
| A Claude Code prompt (manual mode): card "Asks you · claude needs your permission", Overview "Architect wants your permission · Claude needs your permission · Answer in terminal" — command named nowhere | **KNOWN (R7 NEW-9)**, now seen on Overview too |
| Running chip gone while NOTE-21/NOTE-25 waited at a question / prompt (Pause only) | **KNOWN (NEW-b1)** |
| NEW-12 (Claude Code writer loses 1 KB) / NEW-10 (Ctrl+V into Claude Code) | **KNOWN**, not exercised (notes' writer is Codex; no paste into a Claude terminal) |

## Checks (details)

### Sheet and tabs · PASS (UX-4, UX-45, A19–A24)
- SHOP-42 in Review: "shop / SHOP-42", title, pill "Ready for you", Move…, ⋯, ⤢, ×; "Dev and Validate by Codex · 15 min
  · started by you"; stepper Dev → Validate → Review. Overview: WHAT THE AGENT DID, CHECKER "Check passed · on the
  first try" + Full report, THE TICKET, DEPENDENCIES; review bar "Send back · to Dev… / 1 file +1 / Approve & merge
  into main ▾" on every tab. `c-review-overview-shop42.jpg`
- Session: Agent · Validate · Terminal (no Run: no run command). Agent showed the writer's Codex prompt; Validate
  "Ended · passed", "Checker · Codex · tool default", "Read-only · this checker has ended".
  `c-session-agent-writer-prompt-shop42.jpg`, `c-session-validate-ended-read-only.jpg`
- Changes: "SHOP-42's copy · otaq/SHOP-42 · from main", Changed · 1, "1 file · +1 against main · Validate passed ·
  round 1", Agent summary, Validation report, Unified/Split, Open file. A click on the line gutter's + opened a line
  comment; the bar became "1 line comment being written · they go to Dev with Send back · Send back with comments"
  (Esc dropped it). `c-changes-line-comment-send-back-with-comments.jpg`
- History: All / Questions / Checks / Sessions, newest first, "Checker · Validate · round 1 commented", "Session ·
  Validate · Checker 01a1cfcd-… · 15:48–15:50 · 27k tokens"; its copy icon put `01a1cfcd-6794-7790-9a76-876829494f81`
  on the clipboard. `c-history-filters-session-rows.jpg`

### Questions on Overview · PASS / PARTLY (UX-62, UX-64, A21)
NOTE-21 "R8c usage line" (notes; Product spec, Tech spec, Milestone plan, Spec review skipped; Dev and Validate on
Architect/Claude Code) was told to ask a long question (from `r8c-q-long.txt`: 3 options with notes, recommended 3),
then a several-answers one (`--multiple`, 3 options, recommended 1,3), then a send-back. (Claude couldn't run my .ps1
under the VM's execution policy and ran its one command itself.)
- Long question: card turquoise, strip "NOTE-21 a question"; Overview first: **bold** "Should the README's usage line
  name the test command, the Node version, or both?", context in regular weight with **bold**, `inline code`, bullets,
  a numbered list and a code block that scrolls sideways; lines wrap at about 60 characters. The text box is capped
  (~180 shot px ≈ 270 CSS px, about half the sheet's scroll area) and the question line scrolls away with the context.
  Options with notes, "★ Recommended" on 3 (preselected), "Something else", "1–4 pick · ↵ sends", Answer in terminal,
  Send answer. `c-long-question-overview-send-below.jpg`, `c-long-question-overview-zoom.jpg`,
  `c-long-question-code-block-scrolls.jpg`
- But Send (and part of "Something else") sits below the sheet's fold: ~37 CSS px on this 1920×908 window; in the full
  window Send is cut at the bottom edge. → NEW-c1. `c-long-question-scrolled-send-1-4-pick.jpg`,
  `c-long-question-full-window-send-at-edge.jpg`
- Clicked radio 1, pressed 2 (picked 2), ↵ → Claude's transcript "Answer to your question: Node version only (option 2)."
- Several answers: checkboxes, 1 and 3 ticked with Recommended, "Pick any · 1–4 tick · ↵ sends"; I unticked 3, ticked
  Lint and Other… (Send disabled while its text was empty), typed "Empty cart", Send answer → "Answer to your question:
  Unit tests (option 1), Lint (option 2). Also: Empty cart". `c-multi-question-overview-recommended-ticked.jpg`,
  `c-multi-question-picks-with-other-text.jpg`, `c-multi-answer-received-by-claude.jpg`
- History → Messages lists the long question as one run-on line (Markdown and breaks not drawn), as part b noted.
  `c-history-messages-question-run-on.jpg`

### Agent asks to send back to a skipped stage · PASS (UX-55, UX-32, UX-31, UX-54)
- Overview: bold reason "The wording of the usage line is a product question that Product spec should settle.", "1
  Send back to Product spec", "2 Answer here… type your answer", "1–2 pick · ↵ sends". `c-send-back-question-overview.jpg`
  The Send back option still has no explanation line (RESULTS-C NOTE).
- Send back → NOTE-21 in Product spec (Spec writer, Claude Code), which finished at once with a note (UX-54), then
  Dev. Comments: **"Architect · Dev · Dev sent it back to Product spec: …"** (RESULTS-C FAIL: posted as You; now the
  agent's) and "Spec writer · Product spec: nothing to settle…". History: "Sent back to Product spec · Dev asked: … ·
  you sent it back · no longer skipped". `c-send-back-reason-comment-by-architect.jpg`,
  `c-history-sent-back-no-longer-skipped-session-rows.jpg`
- NOTE · the comment says "Dev sent it back" while the user did (the agent asked).

### Comment vs Tell <agent> now, attachments · PASS (UX-43, UX-58, tuq 001–008)
- Box: Attach · **Tell Architect now** · Ctrl+↵ · **Comment**; "Comment: the next stage reads it. Tell Architect now:
  Architect also gets it while it works." Attach → Windows picker → notes.txt + shot-blue.png → two chips.
  `c-comment-box-two-buttons-attachments.jpg`
- Tell Architect now → "You 4:05 pm · told Architect now", image thumbnail, "notes.txt · 38 bytes · Open"; ATTACHMENTS
  panel lists both. Claude (idle, waiting on its question) ran `otaq prime … ## Comments`, replied "OK" and named both
  files. `c-told-comment-tagged-thumbnail-chip.jpg`, `c-told-comment-reached-claude.jpg`
- On a merged ticket only Comment shows. `c-merged-ticket-comment-only.jpg`
- NOTE · Ctrl+↵ posts a plain **Comment** (code: the Kbd belongs to the Comment button), but it sits right after "Tell
  Architect now", so it reads as that button's key (RESULTS-C read it so too).
- NOTE · "Tell Architect now" also shows on a ticket in Code review whose only running agent is the Codex message
  writer (NOTE-23).
- Setup incident (not Otaq): the VM clipboard is shared with the Mac; one paste took the Mac's clipboard (a private
  link) instead of my test text and was posted as a told comment on NOTE-25. I deleted the comment at once (before the
  agent read it: Claude was at its permission prompt for `otaq prime`, and the ticket was then closed), but Otaq's
  delete dialog says "History keeps what it said". No screenshot of it was saved. See "State left".

### Edit ticket and the ⋯ menu · PASS (A46)
- ⋯: Edit ticket…, Ask Noyan about this, Pin to top, Copy branch name, Move to next stage…, Close ticket…, Delete…
  disabled "Started tickets can only be closed." `c-more-menu.jpg`. No Merge into another ticket… (NOTE, as RESULTS-C).
- Edit ticket… in place: "NOTE-21's agent reads your edit the next time it's idle.", Title, Priority, "Stages for this
  ticket" (started skips and Dev locked, Validate a dropdown), What and why (Markdown), "main · locked · the ticket has
  started", Ctrl+↵ save / esc discard. P2 → Save → card P2. `c-edit-ticket-in-place-locked-stages.jpg`
- Close ticket… (NOTE-25, Claude at a prompt): "Close NOTE-25 … without merging? · Its agent is running. Closing stops
  it; the session is not kept. · The branch is kept…" → Stop agent & close → "Closed · not merged", Reopen.

### Checker fail, rounds and Decide · PASS (UX-60)
- Validate (Architect) failed round 1 on purpose; notes' rounds = 1: card **"Validate failed 1 time · decide"** + Decide,
  strip "NOTE-21 decide". Decide: "VALIDATE FAILED 1 TIME IN A ROW · Allow 1 more round · Send it on to Code review ·
  Stop here · Rounds for Validate are set in Edit flow (1)." `c-validate-failed-1-time-decide-card.jpg`, `c-decide-menu.jpg`
- Stop here: stays. Overview: pill "Check failed 1 time", **Move on anyway** / Give it another round / Answer the
  agent…. `c-check-failed-overview-move-on-anyway.jpg`. Move on anyway → Code review, CHECKER "Failed 1 time · moved by
  you", bar "Your check · Failed Validate 1 time · moved by you". `c-moved-on-to-code-review.jpg` (RESULTS-C: refused
  "Validate can't be skipped"; fixed.)
- Allow 1 more round not clicked this time (PASS in RESULTS-C; part a saw a round-2 pass).

### Approve, prepared message, other ways to finish · PASS (UX-13, UX-42, UX-56)
- notes delivers "either": the Code review bar has **Approve** (Ctrl+↵), which opens "Approve NOTE-21 R8c usage line"
  with numbered choices. `c-approve-choices-dialog-either.jpg`. shop (merge only) keeps "Approve & merge into main ▾"
  with the menu; there Merge into another ticket… is disabled "No other ticket of this base waits in review…".
  `c-approve-menu-shop42.jpg`
- Arrival in Code review started the Codex writer (Running "NOTE-21 · Developer"). `c-running-codex-writer-at-code-review.jpg`
- Merge dialog: "Squashed into one commit on main…", "Runs node test.js first when main has moved.", plain message,
  "Developer wants to run a command to write the message. Merge now with this one, edit it, or wait…", Answer in
  terminal → popup "Developer · NOTE-21's message" (`git log --oneline main..HEAD`). `c-or-merge-instead-merge-dialog-writer-prompt.jpg`,
  `c-writer-prompt-popup-codex.jpg`. Four "y" (the popup closes after each; reopen with Answer in terminal) → the
  message filled in: "NOTE-21 Add the re-test 8c line to README …", "Prepared by Developer while NOTE-21 waited in
  Review. Edit it if you like." `c-merge-dialog-written-message-filled-codex.jpg`. Merge → toast "NOTE-21 merged into
  main · Squashed as 63a5494"; Running gone. `c-note21-merged-toast.jpg`
- NOTE-23 and NOTE-22 merged with the plain message while the writer still asked (688dbc9, 929db86).
- SHOP-42: Approve & merge over the writer's open prompt, first line edited to "… (r8c)", Ctrl+↵ → merged **e97df39**
  "SHOP-42: Say prices are in whole dollars in README (r8c)"; its Codex session ended (no process left).
  `c-shop42-merged-edited-message-over-writer-prompt.jpg`
- **Done, nothing to merge** (NOTE-24, Dev committed nothing): the chooser lists it first, enabled ("The work needed no
  change, or it's already in main"); the others disabled "No changes in the repo". → toast "Done · NOTE-24 · Nothing to
  merge; it counts as merged and its branch is kept". Sheet pill "Merged" (NEW-b5).
  `c-approve-no-commits-done-nothing-to-merge-first.jpg`, `c-done-nothing-to-merge-toast-pill-merged.jpg`
- Merge into another ticket… (NOTE-21) listed NOTE-20 "Ready for you" — which waits in **Spec review**. → NEW-c2.
  `c-merge-into-another-offers-spec-review-ticket.jpg`

### PR delivery without gh · PASS (UX-57, UX-25)
- Setup: `origin https://github.com/example-org/notes.git` added to notes (removed again before merging; nothing pushed).
- Approve → Approve & open PR: "Open a pull request for NOTE-21 · GitHub isn't set up on this computer · notes delivers
  pull requests through GitHub. Add the GitHub provider and sign in once; every project can use it. · Set up GitHub…",
  Branch otaq/NOTE-21 "Valid name · not taken on origin", Commit message (1 commit), Title "NOTE-21 R8c usage line"
  (KNOWN gap #32), Description from the handoff, the writer's prompt line + Answer in terminal, "main · on GitHub ·
  example-org/notes", Or merge into main instead, Cancel, **Push & open PR disabled**.
  `c-pr-dialog-no-gh-set-up-commit-message-one-line.jpg`
- Set up GitHub… → Settings → Git & pull requests: git 2.56.0.windows.4, GitHub "uses gh · gh isn't installed ·
  winget install GitHub.cli · Install", Azure DevOps likewise, "Otaq uses them to push and open pull requests; it never
  watches them." `c-set-up-github-opens-git-and-pr-settings.jpg`
- Project Settings → Repository: Delivery "Merge into main / Pull request / Either · On Approve / Automatically" with
  "This is the flow's setting…"; "Pull requests through · GitHub · example-org/notes · gh isn't installed · Set up…".
  `c-repository-pr-through-github-set-up.jpg`
- Or merge into main instead → the merge dialog.

### Conflicts going back on their own · PASS (UX-44, UX-24)
- NOTE-22 "Conflict test A" and NOTE-23 "Conflict test B" (both append a README line), both in Code review. Merging
  NOTE-23 sent NOTE-22 back to its agent by itself: History "Merge conflict · sent to the agent · conflicts with main ·
  Otaq 16:16"; Claude merged main, kept both lines, Validate passed, back in Code review within ~1 min (too fast to catch
  the card's "fixing" line). `c-early-conflict-history-sent-to-agent.jpg`
- Setup commit c44323c on notes main (rewords the README line next to NOTE-22's) → Approve & merge NOTE-22 →
  "**Merging main in stopped — conflict** · main moved since NOTE-22 was built… It was aborted; main and your own
  checkout are unchanged. · README.md · NOTE-22 went back to its agent, which merges main in, resolves the conflict and
  moves on again. Or resolve it yourself in the worktree." `c-merge-time-conflict-dialog.jpg`
- Overview: "⚠ Conflicts with main · 1 file (README.md) · c44323c changed README.md · Nothing was merged into main.
  NOTE-22 returns to Review when it's fixed. · **2 of 3**", Open agent session; pill "Conflicts with main · fixing".
  `c-conflict-banner-what-moved-2-of-3.jpg` (RESULTS-C FAIL fixed.) NOTE · it says "Review" where this flow's stage is
  "Code review".
- Back in Code review: WHAT THE AGENT DID = the restack round ("Merged main in again (4bca276)…").
  `c-what-agent-did-latest-round-after-restack.jpg`. Merged 929db86; README keeps main's line, then A, then B.
  `c-note22-merged-after-conflicts.jpg`

### Plan & helpers · PARTLY (A43)
- NOTE-25 created with a two-piece Plan (`--plan-file`): Overview "PLAN · Written by you" with the pieces as a numbered
  list (Click to edit once closed). `c-plan-on-overview-backlog.jpg`. The card shows no Plan line ("Plan · 0 of 2
  pieces" drawn in A43). Helpers were not started (cost), so the helper statuses, "piece N asks" and the card line with
  merged pieces are NOT TESTED.

### Claude Code permission prompt · KNOWN (R7 NEW-9)
- NOTE-25's session switched to manual (Shift+Tab → "manual mode on"); its next `otaq prime | sed …` asked permission:
  pill Needs you, strip "NOTE-25 allow a command", card "Asks you · claude needs your permission" + Open, Overview
  "**Architect wants your permission** · Claude needs your permission · Answer in terminal". The command shows only in
  the terminal (Codex's prompts are named on the card, Overview and dialogs). `c-claude-manual-prompt-session.jpg`,
  `c-claude-prompt-overview-not-naming-command.jpg`, `c-claude-prompt-card-no-command-no-plan-line.jpg`

## New problems
- **NEW-c1 · A long question on Overview still pushes Send below the fold on a 908-px window.** UX-62: "capping at half
  the page only when Send would fall below it… the options (with their notes) and Send stay in view". The cap is applied
  (the text is ~270 CSS px and scrolls), but with three options with notes + Something else, the box is taller than
  the sheet's view: Send sits ~37 CSS px below, and even the full-window sheet cuts it at the edge. Expected: the text's
  cap shrinks so the options and Send fit (or the options/Send stay sticky). Part b saw the same on 0cc7efc5 (NOTE).
  **Blocker: no** (one scroll reaches Send; ↵ sends from a picked option). `c-long-question-overview-send-below.jpg`,
  `c-long-question-full-window-send-at-edge.jpg`
- **NEW-c2 · "Merge into another ticket…" offers a ticket that waits in an earlier user check (Spec review).** On NOTE-21
  (Code review) the chooser's Merge into another ticket… was enabled and listed only NOTE-20, which sits in **Spec
  review** before any code ("that ticket goes back to review with the combined diff"). On shop the same item says "No
  other ticket of this base waits in review…". Expected: only tickets waiting in the final review (or with a PR open).
  Not clicked. **Blocker: no.** `c-merge-into-another-offers-spec-review-ticket.jpg`
- **NEW-c3 · The PR dialog shows a multi-line commit message in a one-line field.** "Commit message (1 commit)" read
  "NOTE-21: add the re-test 8c line to READMECo-Authored-By: Claude Opus 5.5 …" for the commit
  `NOTE-21: add … README⏎⏎Co-Authored-By: …` (every Claude Code commit has that trailer). `Dialogs.tsx` renders each
  commit message with `TextInput` (an `<input>`), which drops line breaks; since Open PR / Push to PR reword the commits
  from these fields, an edit in it would likely flatten the message into one line (not confirmed: Push needs gh).
  Expected: a multi-line field. **Blocker: no** (only when the user edits; PR path needs gh, out of scope here).
  `c-pr-dialog-no-gh-set-up-commit-message-one-line.jpg`

## Notes
- NOTE-c1 · The comment box's Ctrl+↵ is Comment's key but is drawn right after "Tell <agent> now".
- NOTE-c2 · "Tell Architect now" shows on a Code review ticket while only the delivery writer (Codex) runs.
- NOTE-c3 · The agent-asked send-back is posted as "Dev sent it back to Product spec: …" (the user sent it).
- NOTE-c4 · The conflict banner says "returns to Review" in a flow whose last stage is "Code review"; the writer's
  request says "waiting in Review" there too.
- NOTE-c5 · The ⋯ menu still has no Merge into another ticket… (canvas ticket-06; A46 doesn't list it).
- NOTE-c6 · No Plan line on a card whose ticket has a Plan but no helpers yet.
- NOTE-c7 · The send-back option has no explanation line (as RESULTS-C).
- NOTE-c8 · NOTE-21's Backlog card showed only "Dev: Architect instead of Develop…" (not its four skips); the Dev card
  showed "Skips Product spec, Tech spec, Mi…" only for NOTE-18. Gap #26's combined line not seen.

## Not tested
- Codex: a told comment reaching an idle Codex agent (RESULTS-C FAIL "unsent text"); Codex restack (Codex at 20 %).
  Re-test 7 passed Codex message delivery and restacks.
- UX-55 "Answer here" path of a send-back question (gap #14 History row).
- UX-24 third conflict in a row ("Keeps conflicting with main").
- Merge check failure sending the ticket back (RESULTS-C pill "Merge conflict · fixing" FAIL) — notes/shop have a
  merge check (`node test.js`) but no failing one was set up.
- Plan & helpers with real helpers (A43 card line, helper question as card status).
- Attachment size limit / Ctrl+V image paste in a comment (PASS in RESULTS-C; skipped because the VM clipboard is shared
  with the Mac); NEW-12 / NEW-10 not exercised (KNOWN).

## Setup changes and state left
| Change | State now |
|---|---|
| `otaq-reltest\o.cmd` (CLI helper with OTAQ_HOME / GIT_CONFIG_GLOBAL), `r8c-q-long.txt`, `r8c-q-multi.txt`, `r8c-ask-long.ps1`, `r8c-ask-multi.ps1`, `r8c-desc1.txt`, `r8c-plan.md` | left in `otaq-reltest` (test input, no secrets) |
| notes NOTE-21 "R8c usage line" (P2) | merged 63a5494 |
| notes NOTE-22 / NOTE-23 "R8c conflict A/B" | merged 929db86 / 688dbc9 |
| notes NOTE-24 "R8c nothing to merge" | Done · nothing to merge |
| notes NOTE-25 "R8c plan and prompt" | Closed · not merged (branch otaq/NOTE-25 kept, no commits) |
| notes main: setup commit c44323c "Test setup: README conflict (re-test 8c)" | not reverted; main = 929db86, clean |
| notes origin `https://github.com/example-org/notes.git` | added for UX-57, **removed** |
| shop SHOP-42 | **merged** e97df39 (its writer prompt was left unanswered; session ended with the merge) |
| shop SHOP-41 | Backlog, Not started (its dependency SHOP-42 is merged now) |
| NOTE-25 comment 2 (the Mac clipboard's private link pasted by accident) | deleted in the app; Otaq's History still keeps the text ("History keeps what it said"). **The lead may want to scrub it from `otaq-reltest\home` (otaq.db, NOTE-25's history)**; the agent never read it |
| VM clipboard | the Mac's (shared); not cleared |

Running at the end: the installed app **4147a741** on the normal release-test data (`otaq-reltest\home`), notes board on
screen, its service (PID 192). No ticket agent runs (only the fix check's idle Noyan Codex conversation, PID 17284). The
`~\otaq-qbox` Otaq was not touched.

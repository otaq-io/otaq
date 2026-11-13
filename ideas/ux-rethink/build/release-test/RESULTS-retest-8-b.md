# Re-test 8 · part b · phase B (Board and window) on Windows · results (Windows 11 ARM64 VM, builds 0cc7efc5 and 4147a741)

Date: 2026-11-12, 15:09–15:38 VM time on build **0cc7efc5**, then 15:53–15:58 on build **4147a741** (= release-fixes
826d4f7f: NEW-a1 and NEW-a2 fixed; see `RESULTS-retest-8-fixes.md`), installed by the lead between the two sessions.
The second session ran the checks left over (marked "4147a741" below). Plan: `PLAN.md` phase B, checked against DECISIONS.md (UX-5, 11, 12, 15, 19,
21, 22, 29, 31, 33, 34, 38–40, 46–48, 62, 64; A41, A42, A48), `RESULTS-B.md` (ef84b175) and `FAILURES.md`. Data: the
normal release-test data (`otaq-reltest\home`, `launch.cmd` via `otaqtest`), mostly project shop (Codex: Developer,
Checker, Noyan). Screenshots: `shots-retest-8/b-*.jpg`; the Windows user name is blacked out where a path showed it
(`b-terminal-drawer.jpg`, `b-running-chip-gone-click-opens-pause-menu.jpg`). Codex prompts answered "y" only (never
"p"); nothing signed in or out; no code changed; nothing committed or pushed. Screen geometry: 1920×908 physical at
96 DPI (100 %), screenshots 1280 wide, so CSS px = 1.5 × screenshot px.

## Summary
Phase B: **PASS 31 · FAIL 0 · PARTLY 1 · KNOWN 3 · NOTE 10 · new problems 5 (none a blocker)**; the two "FAIL→NEW" rows
are counted as new problems. NEW-b1 and NEW-b3 still happen on 4147a741. Four of RESULTS-B's five FAILs are
fixed (UX-11 own-agent line, A42 refused-drag toast, palette "Pause shop Ctrl+Shift+.", Noyan's CHANGED row; the fifth,
the PROPOSED card, was decided away).

| Check | Result |
|---|---|
| Columns = stages with "who acts" headers; Backlog and Done are the ends (Done now a real end column, "Done 36 · Merged") (UX-12) | **PASS**; Dev "An agent does the work" **KNOWN** (questions.md piece 15) |
| Box ↵ creates as typed (first line title, Alt+↵ description) in Backlog (UX-22) | **PASS** (SHOP-39) |
| Box Ctrl+↵ creates and starts: Working, "1 running" + Pause (UX-22, UX-15) | **PASS** (SHOP-40) |
| Box Shift+↵ → Noyan panel, shape (UX-22, noyan-intake); a pasted multi-line text splits into title + details | **PASS** |
| Full form: priority, Waits for, attachment chip, own agent per stage, base branch, "flow doesn't let tickets skip" (UX-11, 29, 31, 46) | **PASS**; Esc in an open dropdown now only closes it (RESULTS-B NOTE fixed) |
| Own agent line on the Backlog card and Overview before the stage starts (UX-11, UX-21) | **PASS** (RESULTS-B FAIL fixed) |
| Start → waiting card at the top of Dev, "Waits for SHOP-40 to merge", pill "Waiting to start", banner (UX-12, UX-15) | **PASS** |
| A waiting ticket starts by itself when its dependency merges (UX-15) | **PASS** (SHOP-39 after SHOP-40's merge) |
| Waits for from Overview; "Waiting for it" read-only on the parent (UX-29, UX-39) | **PASS** |
| Hover shows the dependency: others dim, connector, "SHOP-39 waits for it" (A41) | **PASS** (after a ~2 s dwell) |
| Refused drag names what it waits for (A42) | **PASS** "SHOP-39 can't go to Validate yet · It waits for SHOP-40 to merge, then starts by itself." (RESULTS-B FAIL fixed) |
| Card states: Working, Checking, Asks you (Codex names the command), Ready for you (magenta edge, Review), P2 chip, Done · not merged / merged / nothing to merge, Start failed (red) (UX-21, A11) | **PASS**; Asks-you status wraps to 2 lines **NOTE** (as RESULTS-B) |
| List layout: Dev → Validate → Review → Backlog → Done, board's words (UX-33, A40) | **PASS**; date column wraps "2 days / ago" **NOTE** |
| Fold empty columns: list groups collapse; board's empty stages become narrow strips (UX-38) | **PASS**; empty Backlog stays wide **NOTE** |
| Search: in-place filter + popover incl. Done, ↵ opens; in:/by: suggestions and chip; Esc clears (UX-40) | **PASS**; by: lists every project's agents (Spec writer, Architect… on shop) **NOTE** |
| Palette Ctrl+K: Next that needs you first, New ticket, Edit flow, Ask Noyan, Settings, Open a terminal, recent (A48) | **PASS** |
| Palette "Pause shop Ctrl+Shift+." / "Pause all projects" while an agent works (gap #46) | **PASS** (RESULTS-B FAIL fixed); absent while the only agent waits at a prompt (see NEW-b1) |
| Pause menu, Pause shop banner, Pause all (rail marks), Resume (UX-19, window-01) | **PASS**; Pause all's banner says "2 agents" where its menu said "1 agent" (Noyan counted) **NOTE**; Resume restarts the session ("You were interrupted by a restart…") **NOTE** as RESULTS-B |
| Running chip only while agents run (UX-19) | **FAIL→NEW-b1** (chip goes while the agent waits at a prompt and Pause slides under the pointer); same on 4147a741 with the agent limit off; with the limit on, "1/1 agents" stays |
| Running popover (agents, per-tool counts, usage % left, Open Terminal) (A37, UX-34) — 4147a741 | **PASS** (with the limit on it also shows Agent limits steppers); while the only agent prompts it reads "1/1 agents" over "AGENTS RUNNING 0 · Nothing running" **NOTE** |
| Amber usage chip at ≤ 20 % left (UX-19, UX-34) — 4147a741 | **PASS** "Codex · 20% left" appeared in the title bar when Codex reached 20 %; amber in the popover and Settings |
| Agent limit on, Run at most 1: second started ticket "Waits for a free agent · next in line", chip "1/1 agents", starts by itself when the first ends; limit put back (UX-12, UX-15, UX-50) — 4147a741 | **PASS** (SHOP-44 running, SHOP-43 waited, then started). "Don't start new tickets while Needs you has 3" would have blocked any start (needs you 5–6 across projects), so it was raised to 8 for the test; put back to off / 5 / 3 |
| Drag Backlog → Dev starts the ticket; Start on a Backlog card (UX-15) — 4147a741 | **PASS** (SHOP-44 dragged, SHOP-43 Start) |
| Needs you strip, chips, Ctrl+J within and across projects (A9, UX-8) | **PASS**; Ctrl+J on a start-failed / merge-stopped ticket opens an empty Session tab (NEW-b4) |
| Project switcher (A38) | **PASS** |
| Terminal drawer: rail icon, "g t", shell in the project folder, `git log` runs (UX-5) | **PASS**; help still says "Ask Noyan (Ctrl+Shift+L)" **NOTE** (as RESULTS-B); Ctrl+' does nothing on the UK keyboard **NOTE** (palette lists Ctrl+' in one place, Ctrl+` in another) |
| Noyan: Ask Noyan about this (⋯) prefilled "About SHOP-41:", agent picker, nothing runs until sent (UX-47) | **PASS** |
| Noyan: CREATED 1 · IN BACKLOG with Start; CHANGED 1 "SHOP-41 now waits for SHOP-42 · Undo"; Start on the row starts it (UX-15, UX-22) | **PASS** (CHANGED row: RESULTS-B FAIL fixed) |
| Noyan switcher / continue (UX-47) | **PARTLY**: Ctrl+L reopens the last conversation and the switcher lists it; ended conversations appear twice (top list and RECENTLY ENDED) **NOTE** |
| UX-62 long question: Overview (bold first line, Markdown context, options + notes, Recommended) | **PASS**; on this 908-px-high window Send sits below the fold until the sheet scrolls **NOTE** |
| UX-62 long question above Noyan's terminal | **PASS** |
| UX-62 Needs you strip popover (Noyan's question): ~500 px wide (332 shot px × 1.5 = 498), ≤ 560 px tall (≈555), question + context scroll, code block scrolls sideways, options, Send, "Open in the agent panel" in view | **PASS** |
| A ticket's own agent question in the strip: the chip opens Overview (no popover) | **NOTE** as built: the popover is for a helper's or Noyan's question (Strip.tsx) |
| Single answer: click a radio then a digit picks; agent gets "Answer to your question: Yes, whole dollars (option 1)." (ticket and Noyan) | **PASS** |
| UX-64 several answers: checkboxes, 1 and 3 ticked + Recommended; Other… ticks with text; Send disabled with none | **PASS** |
| UX-64: agent receives every pick; Messages logs the same | **PASS** "Answer to your question: Negative price (option 2), Rounding (option 4). Also: zero qty" |
| UX-64: number keys tick / untick | **FAIL→NEW-b3** (digits switch the sheet's tab and reset the ticks); same on 4147a741 (SHOP-44) |
| UX-64 on 4147a741: agent gets "Answer to your question: Empty list (option 1), Rounding (option 4)." | **PASS** |
| Done order newest first | **KNOWN** (gap #49; SHOP-40 "today" sits mid-column) |
| Noyan's prompt chip reads "Agent · allow a command" | **KNOWN** (RESULTS-B NOTE) |

## New problems
- **NEW-b1 · The Running chip disappears whenever the only running agent waits at a prompt, and Pause slides left into
  its place.** UX-19: Running and Pause show while agents run; today Pause stays but Running goes, so a click aimed at
  "1 running" lands on Pause and opens its menu (happened to me 3 times; one more click pauses the project). The
  palette's Pause rows and Ctrl+Shift+U's Running view are also missing in that state, and the Running popover can't be
  opened while a Codex agent is prompting. Expected: Running stays (perhaps "1 running · 1 waits for you") while the
  agent's session is live. Blocker: no. Still on 4147a741 with the agent limit off (with it on, the "1/1 agents" chip stays, but its popover then
  says "Nothing running"). `b-running-chip-gone-while-checker-asks.jpg`, `b-running-chip-gone-click-opens-pause-menu.jpg`,
  `b-4147-limit-restored-off-running-chip-gone-while-asking.jpg`
- **NEW-b2 · `otaq ask human` from a ticket in Review (its Dev session idle, the merge-message writer started at
  Review) is accepted and then shown nowhere.** `OTAQ_TICKET=SHOP-40 otaq ask human --stdin --option …` printed
  "SHOP-40 asks the user. The user answers on the ticket's Overview…", but the card stayed "Ready for you", no strip
  chip, nothing on Overview, History → Questions "Nothing has happened…", `otaq show` no question, nothing in the
  service log; the Approve dialog showed only the writer's own permission prompt. Likely cause (not confirmed):
  `board_requests.rs ask_human` sends any ask to `report_pr_problem` while `pr_flow(ticket)` is Some, which the merge
  message writer may count as. An agent chatting with the user in Review would wait forever. Blocker: no (agents rarely
  ask in Review). `b-review-ask-human-recorded-nowhere.jpg`
- **NEW-b3 · In a several-answers box (UX-64) the number keys don't tick: they switch the sheet's tabs** (2 → Session,
  3 → Changes, 4 → History), and coming back to Overview resets the ticks to the recommended ones. Clicking the row or
  the checkbox first doesn't help. The single-answer box takes digits (radio group). Cause seen in `app/keys.ts`: only
  `[role="radiogroup"]` holds the digits (`LetterHold "digits"`), so a checkbox group's digits go to the global tab
  keys. Blocker: no (mouse works). Still on 4147a741. `b-multiple-digit-key-switches-sheet-tab.jpg`,
  `b-4147-multiple-digit-key-switches-tab.jpg`
- **NEW-b4 · Ctrl+J (and the strip) open a start-failed or merge-stopped ticket on an empty Session tab.** NOTE-18
  (Start failed · post-checkout hook) → Session "No session · NOTE-18 has no agent session yet. It starts when the
  ticket goes to Dev." (it is in Dev); HOOK-4 (merge stopped) → an empty terminal. The reason and Retry are on Overview
  and the card. Expected: Overview when there is no live prompt. Blocker: no.
  `b-ctrl-j-start-failed-opens-empty-session.jpg`, `b-ctrl-j-across-projects-hook4-empty-session.jpg`
- **NEW-b5 · A ticket ended with `otaq stage done` ("Done · nothing to merge" on its card) shows the pill "Merged" in
  its sheet.** SHOP-39. Blocker: no. `b-done-nothing-to-merge-pill-merged.jpg`

## Notes (small)
- The waiting ticket's Overview button reads "View parent SHOP-40" for a Waits-for dependency.
- The box's buttons wrap ("Ask Noyan to / shape it", "new / line") at this window width.
- The Waits-for list in the form is empty with no line when there's nothing open to wait for.
- The stage's agent picker in the form lists every project's agents (Spec writer, Architect, Milestone planner on shop).
- History → Messages shows the question as one run-on line (Markdown and line breaks not drawn); UX-62 doesn't list it.
- After Approve & merge, SHOP-40's sheet stayed open on "Merged" (part a saw it close).
- The answer box resets its picks when you leave Overview and come back.

## Not tested
- Ctrl+' / Ctrl+` terminal toggle on the UK keyboard (did nothing; "g t" and the rail icon work).
- Idle card state (none came up); closing the window to the tray (phase A covered it).
- The answer-box hint wording and Ctrl+Q (covered by the lead's fix check, `RESULTS-retest-8-fixes.md`).

## Setup changes and state left
| Change | State now |
|---|---|
| shop SHOP-39 "R8b box enter ticket" (question-box test ticket: asked two questions, then `stage done`) | Done · nothing to merge; branch otaq/SHOP-39 at main |
| shop SHOP-40 "R8b add a comment above total in price.js" | merged into main as fc0e395 (replaced the comment above `total` with "// Sums the prices of the items.") |
| shop SHOP-41 "R8b full form ticket" (P2, Dev: Checker, package.json attached) | Backlog, waits for SHOP-42 (set by Noyan) |
| shop SHOP-42 "Say prices are in whole dollars in README" (made by Noyan) | Review · Ready for you (as the lead's fix check left it; its message writer at an unanswered permission prompt); not merged |
| Noyan conversation "Whole dollar README" (shop) | ended by the reinstall, not resumed |
| shop SHOP-43 "R8b limit one", SHOP-44 "R8b limit two" (4147a741) | both Done · nothing to merge (`stage done`), no code changed |
| Settings → Limits & usage | turned on (Run at most 1, Needs you 8) for the test, **put back**: off, 5, 3 |
| CLI question asked on SHOP-40 in Review (NEW-b2) | recorded nowhere; nothing to clean |
| Desktop files `r8b-q-long.txt`, `r8b-q-multi.txt`, `r8b-desc39.txt`, `r8b-noyan.txt`, `r8b-t1.txt`, `r8b-t2.txt` | left on the VM desktop (test input, no secrets) |
| Terminal 1 (shop) in the drawer | open session, drawer closed |
| Fold empty columns, layout | back to off / Board |
| Clipboard | the text of `r8b-t2.txt` (no secrets) |
| The other Otaq (`~\otaq-qbox`) | not touched |

Running at the end: the installed app **4147a741** on the normal release-test data (`otaq-reltest\home`), shop board on
screen, no ticket agent working (SHOP-42's writer session waits at its prompt in Review). The `~\otaq-qbox` Otaq was not touched.

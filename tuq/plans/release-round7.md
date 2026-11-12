# Plan · release-round7

- **Ticket:** none
- **Date:** 2026-11-12
- **Decisions:** 048, 049 (010, 025, 044 kept; UX-62 in ideas/ux-rethink/DECISIONS.md)
- **Tech spec:** none (bug fixes; each issue's cause, fix direction and risks are in ideas/ux-rethink/build/release-test/issues-round7/, the reproduction in ../RESULTS-retest-9-repro.md)
- **Orchestrator prompt:** tuq/prompts/release-round7-orchestrator.md

<!-- tuq:plan:start -->

Round 7 fixes the ten issues the Windows regression test of release-fixes fae1fd78 (re-test 8) left open, each
reproduced in the VM with its cause found and logged one per file in
ideas/ux-rethink/build/release-test/issues-round7/ (NN-….md: What happens, Repro, Cause, Fix direction, Risks). It is
built on release-fixes. One piece per behaviour: eleven pieces, since issue 02 holds two behaviours (Ctrl+V pastes,
and an empty line isn't unsent text). Most run side by side; only pieces that touch the same code wait for each other:
the empty-line fix builds on the Ctrl+V fix, and the three answer-box pieces on the Overview (one send, kept picks, the
question in Review) go one after another in NeedsYou.tsx. Pieces that change what Otaq types into a tool's terminal
are checked against the real tool in the Windows VM, not only a stand-in. Decision 049 settles the question in Review;
048 the Ctrl+V paste. The agent in charge follows tuq/prompts/release-round7-orchestrator.md.

## Pieces

1. A long one-line message Otaq types into Claude Code arrives whole — waits for nothing
   Issue 01 (re-test 7 NEW-12). Claude Code takes a terminal read over 800 characters as a paste, and a shorter read
   right after it replaces the input line; the terminal hands a long write over in 1,024-byte reads (macOS 1,022), so a
   1,296-byte one-line message (the merge-message writer's request, crates/otaq-core/src/pr/message.rs `prompt`)
   arrives as its last 272 characters. Not Windows-only. Fix in `type_and_submit`
   (crates/otaq-core/src/session/deliveries.rs): bracket (ESC[200~ … ESC[201~) a long one-line message too, not only a
   multi-line one, as one write, Enter as today. Choose between a Claude Code input quirk (`InputQuirks` in
   crates/otaq-core/src/agent_tools/mod.rs) and a length rule for every tool; with the length rule, check Codex's
   bytes with a bracketed one-line text first. Claude Code stores a paste over 800 characters inside
   `<pasted_content …>` tags: check Otaq's reader of user turns (transcript.rs) handles them. The probe harness the
   investigation used is in the VM at %USERPROFILE%\otaq-build\new12; round 6's WIP tests are on branch
   round6-piece7-claude-1kb-wip (c2a8d52e), a base for the regression test.
   Check: on real Claude Code in the Windows VM (and macOS, in a harness of your own), the writer's request, a 1,296-
   byte, a 1,824-byte, a 3,000-byte one-line and a multi-line message arrive whole; a real Otaq skill command typed
   bracketed still runs; on the release build the Approve dialog fills with the written message; a test records the
   bytes per tool.
2. On Windows, Ctrl+V in a terminal pastes text — waits for nothing
   Decision 048, issue 02 (NEW-10). Round 6 piece 8's fix is on branch round6-piece8-ctrl-v-paste (04d9543e: the key
   always taken; text on the clipboard pasted as Ctrl+Shift+V does, `term.paste`; else the raw key sent, so a tool's
   image paste keeps working). Bring it onto release-fixes, finish its vitest (clipboard stand-in holding text, then
   nothing) and check it in the app. crates/otaq-desktop/ui/src/terminal/ (Terminal.tsx's key handler, state.ts),
   crates/otaq-view/src/keymap.rs if the reserved keys must name it. macOS and Linux unchanged.
   Check: on the release build in the VM: Ctrl+V pastes text into Claude Code and Codex sessions (the unsent-text
   footer shows), an image on the clipboard still pastes as an image in Claude Code, PowerShell's Ctrl+V pastes once.
3. An empty input line isn't unsent text after a raw key — waits for 2
   Issue 02 (NEW-e1). crates/otaq-core/src/session/input.rs:170 counts a raw Ctrl+V byte as "the program filled the
   line" (`filled_by_program`, :319-330), which also sets `multi_line`; when Claude Code draws a bare `›` the screen
   reads `Blank`, and session/mod.rs:1211 (`keys && multi_line`) then calls the empty line unsent text, so answers,
   Send back, Deliver now and Otaq's notes stop at "… has unsent text in its terminal". Keep "the program may have
   filled the line" apart from "the draft has several lines". Piece 2 still sends the raw key with an image or nothing
   on the clipboard, so this case stays reachable after it.
   Check: a test drives the raw Ctrl+V then a bare-marker screen and reads no unsent text; a multi-line Codex paste,
   history recall (↑) of a multi-line entry, and Claude's `[Image #1]` still count as text. In the VM: Ctrl+V with an
   empty clipboard into Claude Code, then an answer from Overview is delivered with no unsent-text dialog.
4. ↵ in an answer's Something else field sends the answer once — waits for nothing
   Issue 03 (NEW-f1). crates/otaq-desktop/ui/src/…/overview/NeedsYou.tsx: the field's ↵ (:228) and the options div's
   handler (:163-171) both call `onSend`; the div's guard skips only HTMLInputElement and the field is a textarea; the
   `sending` state (:309) doesn't stop a second call in the same render, so a second `sheet.answer` is refused and
   shows "The answer to … wasn't sent". The same guard makes a digit typed in that field pick an option ("V2" picked
   Beta). Ignore events from any text field in the div's handler and make the send single-flight (a ref). The box is
   shared by Noyan's panel, Edit flow's box and the strip popover: fix it once.
   Check: a vitest: ↵ in Something else's field calls the answer once; digits type into the field; ↵ and 1–4 on a
   focused row still pick, tick (several answers) and send; Shift+↵ makes a new line. In the app: no toast after ↵.
5. An answer's picks and text survive leaving Overview — waits for 4
   Issue 04. sheet/Sheet.tsx:252 renders only the shown tab, so Overview unmounts and NeedsYou.tsx's picks (:93) and
   Something else's text (:304) are lost; coming back shows the recommended options again. Keep the draft answer per
   (ticket, question id) outside the tab body, as the description's draft does (overview/draft.ts); clear it when the
   question closes or is replaced.
   Check: a vitest: pick, tick and type, switch tab and back: all kept; a new or replaced question starts from its own
   recommended options; drafts don't cross tickets; the card's option click still picks.
6. A long question on Overview keeps Send in view — waits for nothing
   Issue 05 (NEW-c1), UX-62 in ideas/ux-rethink/DECISIONS.md ("the options (with their notes) and Send stay in view").
   QuestionText.tsx:179 caps the text at half the sheet body without counting the options and the Send bar. Cap the
   text at the room left (the body's height minus the box without its text), never under the existing floor; when even
   the floor leaves no room, the options scroll on their own, as Edit flow's box does. Measure from `scrollHeight` to
   avoid a resize loop.
   Check: a vitest or a browser check with the re-test's question (three noted options + Something else) on a 908-px
   window: Send in view without scrolling; the sheet expanded (⤢) and a very small window still show Send.
7. An agent's question shows while the ticket waits in your review — waits for 5
   Decision 049, issue 06 (NEW-b2). "Ready for you" isn't among the statuses that follow signals
   (crates/otaq-core/src/signals/mod.rs:59, signals/state.rs:118), so Asks you is never set, and a ticket's question
   only shows while it reads Asks you (board/tickets.rs:1462): the question is stored open and shown nowhere (also with
   `--send-back Dev`). Make a question on a ticket waiting in a check of the user's read Asks you on the card, pill,
   strip and in notifications, keeping its column; show the box on Overview above the review's actions; Approve, Send
   back, Move and drag stay available; Send back closes it (decision 025). The writer's permission prompt keeps
   decision 010. One behaviour across the signals, the board and the Overview, on purpose. Questions already stored
   open on Review tickets: they show once this lands (say so in the hand-back); an open question must not hold Asks
   you after the ticket goes back to Dev and the question is closed.
   Check: tests: ask on a Review ticket → Asks you, the box on Overview, the answer reaches the agent, then "Ready for
   you" again; Send back closes it; the writer's prompt still shows only in the Approve dialog. In the VM: `otaq ask
   human` on a ticket in Review shows on its card and Overview, and the answer arrives.
8. Ctrl+J and the strip open Overview when nothing waits in a terminal — waits for nothing
   Issue 07 (NEW-b4). crates/…/needs_you_strip.rs:59-76 `target()` sends every item but a review or a question to
   `session_tab::lands_on` (the Session tab), so a start-failed or delivery-stopped ticket opens an empty terminal while
   its reason and Retry are on Overview. Send StartInterrupted, DeliveryStopped (and `stopped_in_place`) and likely
   ResumeFailed to Overview; keep Session for a live prompt (Needs you, the checker's, a helper's) and for Interrupted
   tickets whose Resume lives there.
   Check: tests for each item kind's target; a delivery stop whose skill session still sits at a prompt opens that
   terminal.
9. A ticket done without a merge reads so everywhere — waits for nothing
   Issue 08 (NEW-b5). The ticket's state is merged with outcome done_without_merge (or nothing to merge); only the card
   reads that outcome (card.rs:1287). The sheet pill (api/sheet.rs:734), its subtitle (view_models/sheet.rs:280) and
   `otaq list` / `otaq show` (read.rs:509-516) show "Merged". One helper for a finished ticket's words, used by all of
   them. Filters (`otaq list --status merged`, the Done column) keep matching on the state; only the words change.
   Tickets landed with a host copy the host's outcome: keep that.
   Check: tests: "Done · nothing to merge" and "Done, don't merge" read so on the card, the pill, the subtitle and in
   `otaq list`; a merged ticket still reads "Merged".
10. Merge into another ticket offers only tickets in the final review — waits for nothing
   Issue 09 (NEW-c2). crates/otaq-core/src/board/merge_into.rs:40-51 `can_take` accepts any "Ready for you" host, so a
   ticket in Spec review (before any code) is offered; `can_be_merged_in` (:31-35) has the same gap. A host must wait
   in the user's check right before the Delivery (`engine::human_check_before_delivery(&flow)`), be at its Delivery,
   or have a PR open; flows without a check before Delivery keep their rule.
   Check: tests: a ticket in Spec review is not offered and can't be merged into; one in Code review or with a PR open
   is; the picker's empty line when none qualifies.
11. Noyan's conversation switcher lists each conversation once — waits for nothing
   Issue 10. crates/…/project_agents/mod.rs:1163-1186 puts not-closed conversations in the top list and every
   non-running one in Recently ended, so a conversation "Ended by restart" (not closed) shows in both;
   api/agent_panel.rs:786-809 doesn't drop the overlap. In `switcher()`, drop from Recently ended any conversation
   already in the top list; keep the empty panel's RECENTLY ENDED list complete and `find_agent` (agent_panel.rs:355-
   360) finding both.
   Check: a test with a running, an ended-by-restart and a closed conversation: each listed once in the switcher; the
   empty panel still lists the ended ones.

<!-- tuq:plan:end -->

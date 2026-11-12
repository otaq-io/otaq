# Plan · release-round6

- **Ticket:** none
- **Date:** 2026-11-11
- **Decisions:** 044, 045, 046 (010 and 042 kept)
- **Tech spec:** none (bug fixes; each issue's cause and fix direction is in ideas/ux-rethink/build/release-test/issues-round6/, the re-test in ../RESULTS-retest-6.md)
- **Orchestrator prompt:** tuq/prompts/release-round6-orchestrator.md

<!-- tuq:plan:start -->

Round 6 fixes the release blockers the Windows re-test of release-fixes d0a165b5 (re-test 6) found, logged one per
file in ideas/ux-rethink/build/release-test/issues-round6/ (NN-….md, with each cause found and the fix's risks). It is
built on release-fixes. Five pieces, and a sixth added after re-test 7. Every piece that changes how Otaq types into, reads or shows an agent's terminal
is checked against the real tool (Codex in the Windows VM), not only a stand-in: rounds 4 and 5 passed their tests and
failed in the app. The release bar after this round: no blocker on the core path (start a ticket, the agent works,
Needs you when it waits, review, merge or open a PR) on each OS and tool; everything else is a known issue
(issues-round6/README.md).

## Pieces

1. A skill Otaq types on its own into Codex is sent — waits for nothing
   Issue 01. A bare `$otaq:push` / `$otaq:restack` leaves Codex's skill popup open and the Enter only inserts the item.
   Codex gets a trailing space after the typed text, in the same write, before the one Enter: a Codex input quirk
   (`InputQuirks` in crates/otaq-core/src/agent_tools/mod.rs, set in agent_tools/codex/mod.rs), applied in
   `type_and_submit` (session/deliveries.rs). Other tools unchanged. Turn the issue's probe into a real-Codex test.
   Check: on real Codex in the VM, Push to PR, Retry push and a conflict restack start the skill's turn, and a
   multi-line message to Codex is sent whole; a test records the bytes per tool (Codex: text, space, (Windows) End,
   one Enter; Claude Code unchanged); a stuck line ending in a space still reads as text.
2. A prompt during a delivery skill turn reads Needs you while the ticket stays in Review — waits for 1
   Decision 044, issue 02 (with NEW-6). A needs-you mark on top of the Review/delivery status, filled from
   `Signals::open_prompt` for the ticket's Agent/Delivery session during Push to PR, Retry push, Open PR and an
   automatic PR (not the message writer, which keeps decision 010); read like `stopped_in_place`. The status never
   changes; Approve, Send back, Move and drag stay available. The PR dialog says the agent waits in Preparing/Pushing,
   with Answer in terminal. A session's header reads "Waiting for you" whenever a prompt is drawn. Send back, drag and
   Push to PR over an open prompt say a permission prompt is waiting, not "unsent text". Build the mark as one shape
   ("a ticket's session prompt, shown without changing its status") that piece 3 reuses.
   Check: on real Codex in the VM, a Push to PR whose skill hits a prompt: card, pill and strip read Needs you naming
   the command, the ticket stays in Review, "y" returns it to Ready for you; the writer's prompt still shows only in the
   Approve dialog; Send back over the prompt names the prompt. Tests for each, plus Approve & merge over an open prompt.
3. An answer session can be seen and reached — waits for 2
   Decision 045, issue 03. The Session tab, the strip's target, focus and `otaq attach` reach a running answer session
   when no agent session runs; its prompt gets its own Needs you item and the Done card's line reads "answering
   SHOP-36" / "Needs you · answering SHOP-36", using piece 2's shape (never the ticket's status: the card stays in Done
   and doesn't float or read live). Cleanup treats a Done ticket with a running answer session as in use.
   Check: on real Codex in the VM, a question to a merged ticket whose answer session hits a prompt: Needs you on the
   answering ticket, a click opens its session, "y" answers it, the answer reaches the asker; `otaq attach` works;
   Cleanup doesn't list it. Tests for each.
4. A question whose answer session ended unanswered goes to Needs you — waits for 3
   Decision 046, issue 04. When an answer session ends without `otaq answer` (crash, stop, turn end), the question goes
   to Needs you on the asking ticket (answer it / Resume the answering ticket), never restarted by itself; the worktree
   is removed if clean, else kept for Cleanup. The same at startup for a question marked resumed with no running answer
   session. A stop by pause is excluded. Removal never touches a worktree a live ticket uses: guard it (and the existing
   end of an answered session) with "still Done and no agent session runs".
   Check: tests: an answer session killed before answering (clean and dirty worktree), one that ends its turn without
   answering, a restart with a stale resumed question, a paused project, and a ticket reopened while its answer
   session runs (its worktree kept); in the VM, killing an answer session puts the question in Needs you.
5. In a Codex terminal on Windows, a selection survives until Ctrl+C — waits for nothing
   Issue 05 (decision 042). On Windows, while a selection exists and the program tracks every mouse move, buttonless
   mouse moves aren't reported to it, so xterm.js doesn't clear the selection (crates/otaq-desktop/ui/src/terminal/
   Terminal.tsx, a predicate in state.ts). Drags, shells and macOS/Linux unchanged; links keep their hover.
   Check: in the VM on a dev or release build, in a live Codex session: Shift+drag, move the mouse away, Ctrl+C copies
   and the draft stays; Ctrl+Shift+C copies; PowerShell copy and interrupt unchanged; a link still underlines on hover.
   A vitest whose terminal stand-in clears its selection on a reported mouse move, as xterm does.

6. A writer prompt left open when the ticket leaves Review reads Needs you — waits for 4 (added after re-test 7)
   Decision 010 (refined 2026-11-12), re-test 7 NEW-4 (../RESULTS-retest-7.md). On Codex the message writer is often
   at a permission prompt when Approve & merge runs; if the merge hits a conflict the ticket goes to Dev "Conflicts
   with main · fixing", but the writer's prompt, drawn while in Review, keeps its calm: nothing reads Needs you and the
   restack waits behind it. Re-read the calm of a prompt already drawn when the ticket's status changes (piece 2's
   session-prompt registry): out of Review it reads Needs you naming its command. Otaq types nothing to end the turn.
   A writer prompt in Review still shows only in the Approve dialog.
   Check: on real Codex in the VM, a writer prompt open at Approve & merge, then a conflict: card, pill and strip read
   Needs you in Dev naming the command; "y" lets the writer's turn end and the restack goes out. Tests for that, for a
   writer prompt staying calm in Review, and for a send back / park over an open writer prompt.

<!-- tuq:plan:end -->

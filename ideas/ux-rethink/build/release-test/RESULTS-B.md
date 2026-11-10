# Phase B · Board and window · results (Windows 11 ARM64 VM, build ux-rethink-fixes ef84b175)

Screenshots: vmpal saves them under /Users/nevzattopcu/.claude/projects/-Users-nevzattopcu-Desktop-Workspace-otaq/de3a40a2-9ec6-4414-af3c-524ad89a007e/tool-results/

## Second project
- PASS · UX-61 · Add project (rail +, after the first project) shows a light Flow choice: "Otaq default" (selected) "Dev → Validate → Review · merge or PR on Approve" / "Same as shop" / "Suggest one for this project" ("After adding, Noyan reads the project and drafts a flow in Edit flow. The board starts on Otaq default meanwhile."), "Change it any time with Edit flow." After choosing the folder: trust line, Ticket key NOTE (prefilled), Name notes, Base branch main, and the Suggest row renames to "Suggest one for notes". Added notes with "Same as shop": board opened on notes with Backlog → Dev → Validate → Review, "Done 0 →", Edit flow; rail tile NOTE under SHOP.

## Agent settings (setup for this phase)
- SETUP · Settings → Agents → Noyan (agent) → Model changed from "Default model · Claude Code · tool default" to Codex · tool default, Save. The table now shows Noyan / Developer / Checker all on "Codex · tool default". "Noyan uses" stays "Noyan".

## Box for new work (UX-22, A25)
- PASS · UX-22 / A25 · Ctrl+N or a click opens the box at the bottom centre; it grows upward: title line "New ticket, or ask Noyan…", "Add details (optional)", More options…, "Alt+↵ new line", Ask Noyan to shape it Shift+↵, Create ↵, Create & start Ctrl+↵ (matches new-01 · 2).
- PASS · gap #39/40 · Esc closes the box and keeps the draft (collapsed box shows "Comment total in price.js"); Ctrl+N reopens it with title and description intact.
- PASS · UX-22 · Ctrl+↵ created SHOP-3 "Comment total in price.js" and started it: straight into Dev, "Working · Developer (CX) · 0s"; "1 running" and Pause appeared; the box closed.
- PASS · UX-22 · ↵ created SHOP-4 exactly as typed (first line title; Alt+↵ then the description) in Backlog: "Not started", Start button, no agent run.
- PASS · UX-22 / A26 / UX-29 / UX-46 · More options… opens the full form "New ticket" with title and description carried over; Priority (P3 · Normal default; P1 Urgent / P2 High / P3 Normal / P4 Low), "Waits for · optional · it starts once they merge" with search (lists open tickets with their stage), Attachments "Drop files or paste screenshots" (click opens the Windows file picker; package.json added as a chip "76 bytes", "Posted as your first comment · agents read it before they start"), folded "Stages for this ticket · optional · another agent, or skip where the flow allows", folded "Base branch main" (picker lists local branches main "project default", otaq/SHOP-1…3; "locked once the ticket starts"), no External ID, Cancel / Create / Create & start. Created SHOP-5 "Fix the word tiny in README" with P2, waits for SHOP-4, attachment, Dev → Checker.
- PASS · UX-31 · on shop's flow (Otaq default, no `options: [skip]`) each stage's select offers only "Uses the stage's agent · Developer", Noyan, Checker (with the stage's description on top) and no "Skip this stage"; the form says "shop's flow doesn't let tickets skip a stage." (A flow with skippable stages wasn't set up; that's phase D's Edit flow.)
- NOTE · gap #15 · in the full form Ctrl+↵ = Create and Ctrl+Shift+↵ = Create & start (footer hints), while in the box Ctrl+↵ = Create & start. The gap analysis default was "⌘↵ = the primary button" (Create & start is the primary button here). Inconsistent between box and form.
- NOTE · Esc in the full form discards the whole form ("esc discard"), also when it was meant to close an open dropdown: pressing Esc to close the Waits-for list threw away priority/waits-for choices (title/description survive in the box). Easy to lose work.
- FAIL · UX-11 / UX-21 · a ticket's own agent shows only once the stage runs: SHOP-5 has Dev → Checker (Edit shows "Dev: Checker"), but while it sat in Backlog (with and without its waiting line) the card showed no own-agent line, and Overview never shows one. Once started (dragged into Dev) the card did show "👤 Dev: Checker instead of Developer" and "CX Checker". UX-11: "shown as one line on the ticket only when set". Screenshots mcp-vmpal-blob-1794305316142-vbst0r.jpg (Backlog card + Overview, nothing) and mcp-vmpal-blob-1794306122141-wm3k6z.jpg (running card, line shown).
- PASS · UX-15 · dragging SHOP-5 from Backlog into Dev started it (same as Start).

## Cards, Start and waiting (UX-12, UX-15, UX-21, A6)
- PASS · UX-15 · Backlog cards show "Not started" and a Start button; Start on SHOP-4's sheet moved it into Dev.
- PASS · UX-12 / UX-15 / A6 · SHOP-4 started while waiting for SHOP-3: it sits at the top of Dev as "⌛ Waits for SHOP-3 to merge" (amber hourglass, muted, no edge), sheet pill "Waiting to start", Overview banner "Waits for SHOP-3 to merge (in Review)." Backlog card SHOP-5 also shows "⌛ Waits for SHOP-4 to merge".
- NOTE · board-02 · the waiting card also has an agent line "Developer" (canvas draws the waiting card with no agent line); 4-line rule still holds.
- PASS · UX-21 · Working card (SHOP-3): key, title, "● Working", "CX Developer · 0s", no edge. Checking: "● Checking · CX Checker · 7s", no edge.
- PASS · UX-21 / UX-14 · Asks you (Codex permission prompt): turquoise edge, "Asks you · run command: …", Open button, Needs you strip "SHOP-3 allow a command", rail tile dot "1".
- NOTE · UX-21 · the Asks-you status line wraps to two lines (the whole command, cut with "…"), so the status isn't one line; canvas: "Asks you · allow `npm install`?".
- PASS · UX-21 / A11 · Your review: magenta edge, "Ready for you · passed Validate", "CX Developer · 5s", Review button; strip "Your review · 1 · SHOP-3 ready", Next Ctrl+J. Running/Pause left the title bar.
- PASS · UX-21 · P2 chip on SHOP-5 ("^ P2"); P3 cards show none.
- PASS · A11 · Done cards "Done · not merged" (grey dot) and "Merged into main" (green) with "today".
- KNOWN · board-02 / gap #49 · Done lists SHOP-1 above SHOP-2 (oldest first).
- KNOWN · UX-16 · Dev header "An agent does the work" (canvas "An agent writes the code").

## Dependencies (UX-29, UX-39, A41, A42)
- PASS · UX-29 / A42 · Overview "+ Waits for…" opens a search list (only open tickets; "A ticket can't wait for one that waits for it: Otaq refuses loops."); choosing SHOP-3 shows DEPENDENCIES · WAITS FOR · starts when it merges · SHOP-3 · Validate · asks you, with ×. The form's Waits for works too (SHOP-5).
- PASS · UX-39 · SHOP-4's Overview lists "WAITING FOR IT · SHOP-5 Fix the word tiny in READ… · Backlog · not started" read-only (no ×, no add); "+ Add" only adds what it waits for. No "Blocks".
- PASS · A41 · hovering SHOP-3 dims every other card except SHOP-4, draws a connector to it, and adds the line "🔗 SHOP-4 waits for it" on SHOP-3.
- FAIL · A42 / board-05 · 4 · dragging the waiting SHOP-4 from Dev to Validate is refused (nothing moves) but the toast only says "It starts by itself when it can, in priority order", without what it waits for (canvas: "SHOP-30 can't go to Validate yet · It waits for SHOP-27 to merge, then starts by itself."). Screenshot mcp-vmpal-blob-1794305122261-i7t9gf.jpg. (Dragging to Review: "Only agents move tickets to your review".)
- NOTE · the error toasts stay until closed with × (still there after 5 minutes).

## List and Fold (UX-33, UX-38, A40)
- PASS · A40 / gap "List's Backlog" · layout switch (board / list icons) beside Board · Files · Settings. List groups: Dev 1 → Validate 0 ("Nothing here") → Review 1 → Backlog 1 → Done 2, then Edit flow: Backlog after the stages, then Done, as the canvas draws (agreed 2026-11-10).
- PASS · UX-33 · rows keep the board's order and words; SHOP-3's row has the magenta edge, Review button; SHOP-5 row has P2 chip and Start.
- NOTE · gap #42 · the List's waiting rows use an amber dot, not the card's hourglass (open item, default was the hourglass).
- PASS · UX-38 · "Fold empty columns" toggles: on the List, empty Validate collapses to a header; on the board, Validate becomes a narrow "0 Validate" strip; off restores. Toolbar has no Group by.

## Search (UX-40, A47)
- PASS · UX-40 · "/" focuses search and lists FILTERS (is:needs-you, is:working, is:validating, is:queued, is:parked, has:run, is:backlog, is:review, is:approved, is:done with counts). Typing "readme" filters the board in place ("Showing 3 of 5", "N hidden by filter" per column, Clear) and the popover lists SHOP-5 (Waits for SHOP-4), SHOP-1 (Done), SHOP-2 (Done): Done included. ↵ on a highlighted row opened that ticket.
- PASS · UX-40 · "in:" suggests in:backlog / in:dev / in:validate / in:review / in:done with counts; in:review becomes a chip and shows only SHOP-3. "by:" suggests by:noyan / by:developer / by:checker; by:developer shows SHOP-4. Footer "Filters: is:needs-you · is:working · in:review · by:noyan".
- PASS · gap #51–53 · Esc clears the filter.
- NOTE · "All tickets ⌄" in the toolbar just opens the same search filter list.

## Command palette (A48)
- PASS · A48 / window-02 · Ctrl+K: "Type a command or a ticket…", SUGGESTED: "Next that needs you · SHOP-3 · Ctrl+J" first, New ticket Ctrl+N or c, Edit flow, Ask Noyan Ctrl+L, Settings Ctrl+,, Open a terminal Ctrl+' or g t; RECENT: Filter the board /, SHOP-1, SHOP-4, SHOP-5, SHOP-3. "> actions only".

## Terminal (UX-5, A39)
- PASS · UX-5 / window-02 · 3 · the rail's Terminal opens a drawer under the board: "Terminal 1 · shop · a shell in ~\otaq-reltest\shop", PowerShell prompt in the project folder; `git log --oneline -3` ran (edb05b8 SHOP-2…). "+" for more terminals; the rail icon closes it again.
- NOTE · the drawer's help says "Want an agent's help? Ask Noyan (Ctrl+Shift+L)" while the palette, UX-22 and the canvas say Ctrl+L.
- NOT TESTED · Ctrl+` / Ctrl+' toggle on the UK keyboard: Ctrl+Backquote and Ctrl+Quote did nothing visible (layout mapping unclear).

## Project switcher, Needs you strip, Ctrl+J (A38, A9, UX-8)
- PASS · A38 / UX-19 · clicking "shop ⌄" opens the switcher: shop (~\otaq-reltest\shop, dot = needs you, ✓ current), notes (~\otaq-reltest\notes), "+ Add project…". Picking notes switched the board; the rail's SHOP tile kept its "1" dot.
- PASS · A9 / UX-8 · on notes the strip says "Needs you · 0 here · You're through notes's items." with "→ Next · shop, 1 · Ctrl+J"; Ctrl+J from notes opened SHOP-3 in shop (across projects). In shop, Ctrl+J opened the asking ticket on its Session tab.
- PASS · A9 · strip shows "Needs you · N" (turquoise chips "SHOP-4 allow a command") and "Your review · 1" (magenta chip) and hides with nothing pending.
- NOTE · Noyan's permission prompt shows in the strip as "Agent · allow a command" (not "Noyan").

## Start by itself, Running, Pause (UX-15, UX-19, A37)
- PASS · UX-15 · after SHOP-3 was approved & merged, the waiting SHOP-4 started by itself (Overview "Dev · working", card Working, "1 running").
- PASS · A37 / UX-34 · "1 running ⌄" popover: AGENTS RUNNING 1 · SHOP-4 Comment formatPrice in price.js · Developer · 9s; per tool counts (Claude Code 0, Codex 1, Copilot 0, OpenCode 0); USAGE · WHAT'S LEFT: Codex "month 86% left · resets Nov 27", others "No plan limit · tokens only" / "Limits not reported"; Open Terminal.
- PASS · UX-19 / window-01 · 2 · Pause menu: "Pause shop · 1 agent stops where it is; queued tickets wait. Ctrl+Shift+." / "Pause all projects · 1 agent in 1 project." / "Resume continues each one where it was."
- PASS · window-01 · 6 · Pause shop: amber banner "shop is paused · 1 agent stopped where it was; queued tickets wait." with Resume; Running and Pause left the title bar. Pause all projects: "All projects are paused · 1 agent in 1 project stopped where it was…", rail tiles get a pause mark, card "● Paused · resumes where it was". Resume brought back Working, "1 running" and Pause.
- NOTE · a card paused while it was asking for permission keeps "Asks you · allow a command?" with Answer (not "Paused"). After Resume the card's time restarts from 0s ("Working · 2s") though it had worked for minutes.
- FAIL · gap #46 / window-02 · 2 · the palette suggests "Pause this project" (no shortcut shown) instead of "Pause shop" with ⇧⌘. as drawn ("Pause shop-web ⇧⌘."). Screenshot mcp-vmpal-blob-1794305554495-ivtt3d.jpg.
- PASS · A48 · while agents run, the palette's first suggestion is "Next that needs you · SHOP-4"; RECENT includes "Switch to notes Ctrl+2".
- NOT TESTED · UX-34 / UX-49 · amber "% left" chip and limit banner: no tool was near its limit (Codex 86% left).

## Noyan (UX-22, UX-47, noyan-intake)
- PASS · UX-22 / noyan-01 · 1 · Shift+↵ in the box with "Two small things: README should mention node 24, and test.js needs a check for total of an empty list. Make tickets." opened Noyan's panel on the left (board narrows): "Noyan · New conversation · Working", "CX Noyan · Codex", "Never edits your code", Rename; the conversation renamed itself "README And Empty Total".
- PASS · N2 / N3 · Noyan (on Codex) ran `otaq prime --project`, read README/test.js/price.js, proposed 2 tickets (separate deliverables) and asked "Say yes to create both, or tell me what to change."; descriptions carry "Where the code lives / What I saw (file:line) / Open questions". No question about the request was needed (none asked).
- FAIL · noyan-01 · 3 · the proposal is only text in Noyan's terminal (a markdown table); the drawn "PROPOSED · 2 TICKETS" card with untick / "Change…" / "Create 2 in Backlog" isn't shown, so the user answers "yes" by typing. Screenshot mcp-vmpal-blob-1794305724597-hzh5ve.jpg.
- PASS · UX-15 / noyan-01 · 4 · after "yes": "CREATED 2 · IN BACKLOG" on top of the panel with SHOP-6 and SHOP-7, each with Start and delete, and "Show on board"; both appear in Backlog as "Not started". Noyan started nothing.
- PASS · UX-15 · Start on SHOP-6's row in the panel started it (card Working in Dev; panel row "Working · Dev").
- PASS · UX-47 · closing the panel and Ctrl+L reopens it on the last conversation; the header switcher lists "README And Empty Total · CX · Idle" and "+ New conversation Ctrl+Shift+L"; picking it continues it. A follow-up "Make SHOP-7 wait for SHOP-6. Nothing else." ran `otaq dep add SHOP-7 SHOP-6` and SHOP-7's card now says "⌛ Waits for SHOP-6 to merge".
- FAIL · noyan-01 · 4 · that change isn't listed in the panel ("CHANGED 1 · SHOP-7 · now waits for SHOP-6 · Undo" is drawn); the panel still shows only CREATED 2. Screenshot mcp-vmpal-blob-1794305869850-ab431m.jpg.
- PASS · UX-47 · ticket ⋯ menu (Edit ticket…, Ask Noyan about this, Pin to top, Close ticket…, Delete… "Never started · nothing on disk") → "Ask Noyan about this" opens a new conversation prefilled "About SHOP-5:" with an agent picker (Noyan / Developer / Checker, UX-9) and "Nothing runs and no tokens are used until you send." (not sent).
- KNOWN · gap #58 · the conversation switcher has no "New agent on…" item.

## Waiting for a free agent, more card states
- PASS · UX-50 · Settings → Limits & usage: "Limit how many agents run at once" is off by default ("When on, tickets beyond the limit wait at the top of their first stage: 'Waits for a free agent'"). Turned on with Run at most 2 for the test, then back off and 5 (restored).
- PASS · UX-12 / UX-15 · with 2/2 running, Ctrl+↵ on notes' box created NOTE-1 and it sat at the top of Dev as "⌛ Waits for a free agent · next in line"; title bar showed "2/2 agents". When SHOP-6 reached Review, NOTE-1 started by itself.
- NOTE · board-03 · a check that ended without a result (I cancelled the checker's command in its terminal): card "Validate gave no result · decide" with Retry validation and a turquoise edge, while its Needs-you chip is red with an ✕ ("NOTE-1 decide"): card and strip disagree on the colour. Retry validation reran the check (round 1 of 3) and it passed.
- NOTE · UX-21 · SHOP-5's running card is five lines (key, title, two-line "Asks you · run command: …", "Dev: Checker instead of Developer", "CX Checker") — over the four-line rule when the status wraps.
- NOTE · Pause then Resume restarts the agent session: the agent got "You were interrupted by a restart. Carry on where you left off…" (Pause's text says "Resume continues each one where it was").
- NOTE · the waiting-in-Validate card shows "CX Developer" (the ticket's Dev agent) after the checker ended, not the checker.
- NOT TESTED · UX-21 Idle (KNOWN to differ: gap #4 says Idle has no edge, the build gives it turquoise; no Idle card came up this phase), Fixing · sent back by Validate (no check failed this phase; phase A saw it), Start failed / Interrupted / Held / Out of rounds / PR open / Conflict states (need project settings, a restart, a PR provider or a conflict: phases C/D).
- KNOWN · gap #57 · lift-on-answer: not observed either way (rows/cards didn't move when answered).
- NOTE · Done order after more merges is SHOP-3, SHOP-4, SHOP-1, SHOP-2: neither newest first nor oldest first (KNOWN #49 says it should be newest first).

## Counts
PASS 42 · FAIL 5 · KNOWN 4 · NOT TESTED 3 · NOTE 15 (plus one SETUP line)

## State left for phases C and D
- Otaq running on screen, maximized, on shop's board. Agent limit off (Run at most 5, as before). Noyan's agent now uses Codex (was Claude Code).
- shop (SHOP, main = edb05b8 + SHOP-3 4f605ad + SHOP-4 e77aea2):
  - SHOP-5 "Fix the word tiny in README" · P2 · Dev on its own agent (Dev → Checker) · running, waiting at a Codex permission prompt (answer Return/y in Session). Has package.json attached.
  - SHOP-6 "Mention Node 24 in README" (made by Noyan) · Review · Ready for you · passed Validate (not merged). SHOP-5 and SHOP-6 both edit README line 3 ("A tiny shop library…"), so merging one will make the other conflict (useful for phase C's conflict check).
  - SHOP-7 "Check total for an empty list" (made by Noyan) · Backlog · waits for SHOP-6.
  - SHOP-3, SHOP-4 merged; SHOP-1 Done · not merged; SHOP-2 merged.
- notes (NOTE, added this phase with "Same as shop"): NOTE-1 "Fix the typo libary in README" · Review · Ready for you · passed Validate. notes' test.js is still broken (setup), no ticket ran it.
- Noyan: one conversation "README And Empty Total" (Idle). A Terminal 1 for shop was opened in the drawer (closed, may still exist).

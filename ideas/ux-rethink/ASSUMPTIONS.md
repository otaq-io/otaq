# Otaq UX rethink · assumptions to walk through

Made while drawing step 5 on "Otaq — New UX" without stopping for questions (NT, 2026-11-07). Each is a design choice
not covered by UX-1 … UX-22 in `DECISIONS.md`. Agree, change or drop each; agreed ones become UX decisions.
`GAP-REPORT.md` notes which ones conflict with the app or with earlier decisions.

## Edit flow
- A1 · Edit flow turns the board into the editor in place: cards dim; each column header becomes its stage (name, what happens, agent); drag to reorder, × to remove, + between columns to add; a bar on top with Start from…, Cancel and Save flow.
- A2 · Three stage kinds, named "An agent does the work", "An agent checks it", "You check it".
- A3 · Stage panel: name, what happens, agent, optional instructions, and for checks "If the check fails, the ticket goes back to Dev ▾" plus "after 3 tries in a row it waits for you". Removing a stage moves its tickets to the stage before.
- A4 · A saved change applies once no ticket is inside a changed stage; new tickets use it at once. No "Apply now".
- A5 · Presets: Otaq default, Spec first, Solo autopilot, Team PR, with one-line descriptions I wrote. Solo autopilot has no Review stage (passing work merges by itself), so a flow without Review is allowed.

## Board and card
- A6 · Waiting is the status line itself (amber hourglass, "Waits for SHOP-27 to merge"), not a fourth line.
- A7 · No "Created by Noyan" line on cards (UX-21 strictly); Noyan's panel lists what it created.
- A8 · Columns share the width; Edit flow sits at the right end of the columns. Long flows (scroll, "N more stages") not redrawn.
- A9 · The Needs you strip also holds "Your review" (magenta); with nothing needing you, the strip isn't shown.
- A10 · Start failed sits in the first stage (Retry); Can't merge in Review (Resolve); a restart shows "Stopped by a restart" (Resume).
- A11 · Card wording: "Ready for you · passed Validate", "Checking · …", "Fixing · Validate found 2 problems", "Paused · resumes where it was", "Merged into main", "Done · not merged".
- A40 · List is a layout switch (board / list icons) next to Board · Files · Settings, not a fourth view. Rows are grouped by stage, in flow order, with what needs you first.
- A41 · Hovering a card highlights what waits for it (everything else dims) and adds one line, "SHOP-30 waits for it".
- A42 · What a ticket waits for is set on Overview ("Waits for" with search) or in the form; dragging a waiting ticket ahead is refused with a toast giving the reason.

## Onboarding
- A12 · Step indicator Welcome · Tools · Project. Tools shows the four agent tools and git only; gh / az move to Repository settings.
- A13 · Project asks Name, Ticket key and Base branch (all prefilled); base branch is a field, not just "detected".
- A14 · The first-ticket card has an accent outline; "about 5 minutes" is a fixed estimate.
- A15 · Notifications: the macOS prompt can't carry Otaq's words, so Otaq's one line shows as a hint beside it.
- A16 · One-time hints are dark popovers with one button (Got it / OK / Open SHOP-1 / Edit flow).
- A17 · After the first approve: toast "SHOP-1 merged into main · Squashed as 9f3e2c1", the flow hint (with Edit flow) and the "New work starts here" hint above the box for new work, which appears now.

## Ticket
- A18 · The sheet is 860 px, centred over the dimmed board (today 1040).
- A19 · Header: crumb, title, state pill, ⋯, full window, close; meta line "In Dev · Strong · started by you 12 min ago".
- A20 · In Review it opens on Overview: What the agent did · Checker · Decisions the agent made · The ticket. The review bar stays on every tab while in Review.
- A21 · A question is the first thing on Overview: numbered options with notes, Recommended preselected, "Something else", Answer in terminal, Send answer.
- A22 · A permission prompt shows the exact command and Deny / Allow once / Open terminal (no "always allow").
- A23 · Session's switch is Agent · Validate · Run · Terminal: the check part is named after the stage, not the checker agent.
- A24 · History is plain sentences, newest first. Send back is one note plus "1 line comment goes with it".
- A43 · Plan & helpers: still one card, with one muted line "Plan · 1 of 4 pieces merged"; a helper's question becomes the card's status ("Asks you · piece 3 asks"). Overview shows the Plan as numbered pieces with each helper's status and Edit plan.
- A44 · Approve & open PR opens a dialog with branch, title, description and target, all prefilled. An open PR card shows "PR #481 open · waiting for merge" and a muted "2 comments on GitHub" line.
- A45 · Delivery problems are red callouts on Overview: conflicts ("Ask Strong to fix" / Open terminal) and a failed merge check ("Send back to Dev" / See output). Each says that nothing was merged.
- A46 · The ⋯ menu holds Edit ticket…, Pin to top, Copy branch name, Move to next stage…, Close ticket…, Delete…; unavailable items say why. Edit ticket… turns Overview into the form inside the sheet.

## New work (UX-22)
- A25 · The box is docked at the bottom centre of the board ("New ticket, or ask Noyan…", ⌘N) and grows upward when focused. The first line is the title, the rest the description. Keys: ↵ Create, ⌘↵ Create & start, ⇧↵ Ask Noyan to shape it. The toolbar's New ticket focuses the same box.
- A26 · Full form (More options…): Title, Description, Priority (default P3), Waits for (dependencies set when creating), Attachments, and a folded "Agents for this ticket"; what you typed is carried over.

## Noyan
- A27 · The panel opens on the left at 420 px and narrows the board. Created tickets are listed on top with Start (you start them). ⌘L opens the panel itself (to read or continue a conversation).
- A28 · Noyan's question uses the same answer box as tickets, above its terminal. The agent line says "Reads code, never edits it".
- A29 · Header shows only new conversation, expand and close; Recently ended and rename stay as today (not redrawn).

## Settings
- A30 · App settings is a modal window with a left nav, and a footnote pointing to each project's Settings tab.
- A31 · Agents page: tools with Install; agents with "Used in"; an agent editor (name, tool, model, instructions, used in, delete); "Noyan uses ▾".
- A32 · Limits & usage: one "run at most N agents at once" across projects, usage bars per tool, amber at 20% or less.
- A33 · Notifications: four toggles of my choosing.
- A34 · Appearance: Match system / Light / Dark, with theme cards and "+ 11 more themes", plus the terminal font. Keyboard: a short list of the shortcuts that matter, editable. About: version, update, automatic checks, otaq in your PATH, licence.
- A35 · Project settings use the window's full width: a section list on the left (follows the scroll, jumps on click) and one card per section. Flow shows the stages as a strip with Edit flow; Repository has base branch, Deliver as (Merge into main / Pull request, with the GitHub status line when PR) and merge check; Run is a table of apps (command, Open URL, port variable) with "+ Add app" and the "Set by Dev … · Undo" line; Workspace has setup command, environment variables (table), files to copy, per-ticket resources and warm build; Notes for agents; Remove project as a red card.

## Files and window
- A36 · Files view: a "Showing" picker (main, a ticket's copy, your folder); files a ticket changed carry its key; New ticket from this file.
- A37 · Running popover: agents, apps, usage per tool. Pause menu: this project or all projects.
- A38 · Clicking the project name opens the switcher (projects, Add project…), with a dot on the one that needs you.
- A39 · Limit and paused banners in amber; the project Terminal is a 260 px drawer under the board.
- A47 · Search (/) shows matching tickets with their status in a popover, and offers filters as text (is:needs-you, in:review, by:Strong); no separate filter UI.
- A48 · The command palette (⌘K) lists actions by name with their shortcuts; the first suggestion is "Next that needs you".

## Restarts and upkeep
- A49 · After a restart, one dialog lists what stopped, ticked; a tool that can't resume (signed out) is unticked with the reason; "Resume 2" / Not now. If closed, stopped cards are red with Resume and in the Needs you strip.
- A50 · Cleanup opens from Settings → Data as a dialog: leftovers with sizes, nothing ticked, "Delete selected…".
- A51 · The menu-bar icon shows a count when something needs you; its menu lists those first, then what's running, then Open Otaq / Pause all / Restart to update / Quit.
- A52 · Quitting while agents work asks, with "Keep them running" as the default and "Quit and stop agents" as the second choice. Drawn as an Otaq dialog; in the app it may be the native alert.
- A53 · An update shows as a calm banner ("Otaq 0.2.1 is ready" · Restart to update), never a modal; the same row is in Settings → About.

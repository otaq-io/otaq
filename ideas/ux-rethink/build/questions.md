# UX rethink build · questions for later (non-blocking)

Questions that came up while building the ux-rethink pieces and didn't need an answer before work could go on. Each
says what was done meanwhile. Newest last.

## Orchestration

- **Where the pieces are merged.** Each piece is merged into `main` as the orchestrator prompt says ("merge into the
  current branch"). Nothing is pushed. Say if you'd rather integrate on the prepared `ux-rethink` branch instead.
- **CLAUDE.md / README.md local edits lost on the pull (2026-11-07).** The uncommitted edits were older wording that
  origin/main had replaced ("Product decisions (tuq)" section; README's "instructions and skills"); they were discarded
  without a backup. Nothing to do unless you miss something.
- **Node 24 vs 26.** `scripts/check.sh` wants Node 24 (crates/otaq-desktop/.nvmrc); this Mac has only Node 26. Agents
  ran the frontend steps with a local Node 24 copy (or by hand on 26, all passing). Install Node 24, or let the script
  accept 26?

## Piece 8 · "Ready for your review" notification (merged)

- A burst of review notifications is grouped as "N agents need you". Should the grouped wording change when some are
  reviews? (Kept as is.)
- A click-mode delivery waiting for its click ("To deliver") doesn't notify; only human check stages do. Should it?
- The title always says "ready for your review", also for a QA-style human check (the body names the stage). OK?

## Piece 11 · No External ID (merged)

- `PROTOCOL_VERSION` went 9 → 10 (a removed field needs a bump). Other pieces may bump it too; merges keep one bump.
- A `create -f` file or old Noyan request that still sends `"external_id"` is silently ignored, not refused. OK?
- The sheet's details line lost its "copy" kind (External ID was its only user). A later piece re-adds it if needed.

## Piece 5 · One-time hints (merged)

- Every hint's button reads "Got it". Should a hint name its own button?
- The test hint lives only in the design gallery (`OTAQ_GALLERY=1`); dismissing it there is permanent for that data
  folder. OK, or should the gallery draw a never-stored popover?
- There is no "Reset hints" (hints are never behind a setting). Should support or tests ever get one?

## Piece 3 · Stage descriptions (merged)

- The flow file's key is now `description:` (`why:` still loads as an alias).
- Editing only a stage's description now counts as a flow change (a new version, applied at once; proposals say
  "description added / changed / removed"). Before, a `why`-only change was "the same flow" and never saved.
- **Worth a look:** Noyan's flow skill now ends a stage's description with its evidence ("From 41 /security-review
  runs… (Claude Code)", or "No evidence found"), to keep the source visible in the editor. Agents read that sentence too.
  Keep sources out of descriptions?
- Existing projects' stored flows have no descriptions, so their prime lists stage names only. No migration fills them
  in (0086 unused). Want one for stored copies of the shipped flows?
- The flow editor's "Changed by Noyan: …" hover now shows the description (same field). Left as is.

## Piece 12 · Waits for on create (merged)

- Overview's lists are now "Waits for" / "Waiting for it" (the decision's words). Cards still say "Blocks SHOP-20" and
  "Blocked · waits for…": left for piece 13 / the card piece (15).
- "Waiting for it" is read-only (no add, no ✕): a link can now be removed only from the waiting ticket. Intended?
- `otaq dep add KEY DEPENDS-ON` is unchanged; an agent can still run `dep add OTHER SELF` (the parent's side through the
  same command). Left as is.
- `website/src/demo/Sheet.tsx` still says "Blocked by" / "Blocks" (piece 29 updates the website).

## Piece 9 · Theme follows the system (merged)

- **Upgrade:** someone who picked a theme before keeps a fixed mode (Light or Dark), so nothing changes for them. Move
  them to Match system instead?
- Clicking a theme card of the other kind while the mode is fixed switches the mode so the click shows; on Match system
  the mode stays. Right?
- The OS appearance is kept as a hidden settings field (`os_dark`, written only on Match system); a whole-settings save
  from another page could briefly overwrite it with an older value.
- Only the tray *menu* follows the mode; the macOS menu-bar icon is a template image macOS colours, and on Windows it
  follows the taskbar's setting.
- Pieces 9, 11 and 7 each bumped `PROTOCOL_VERSION` to 10; merged as one bump (nothing released in between).

## Piece 7 · Noyan's agent is one setting, roles go (merged)

- **Worth a look:** with roles gone, Noyan's agent is offered for ticket stages like any other (before, the role holder
  was left out); `prime --project`'s example now reads `--stage "Product spec=Noyan"`. Keep it offered, or leave it out
  (which would be a hidden rule)?
- The agent Noyan uses can't be deleted ("Pick another agent for Noyan first"), and its "What this agent reads" page
  shows the noyan skill "as Noyan". OK as "where it is used", not a role?
- `otaq prime --project` gives the setting's agent's instructions even in a conversation started on another agent (the
  prime request doesn't know which conversation asks), as before.
- "New agent on <model>" is gone everywhere; a conversation whose tool can't run shows only Sign in… / Resume after reset
  and waits.

## Piece 1 · Delivery is a setting of the flow (merged)

The flow file now ends with `delivery: { delivers: local|pr|either, mode: click|automatic, agent, skill }`; old files
with a Delivery stage or `column: with-previous` still load. `PROTOCOL_VERSION` is 11.
- **Worth a look:** a ticket being delivered waits in the flow's **last stage** as To deliver / PR open. On flows with no
  Review (Team PR, Solo autopilot) a "PR open" card sits in Validate's column. OK?
- **Worth a look:** PR rework by dragging is gone (it only existed as a drag out of Approved). "Send back with PR
  comments" and "Push to PR #N" stay. Should a "Rework" action come back on a PR-open card?
- The Open PR refusal toast's and the provider row's "Set the Delivery stage's skill" link now goes nowhere (no link
  passed). Should it open the flow editor / file? (Pieces 21 / 27 can place it.)
- "Approve & merge into main" names the target from the merge proposal; the To deliver card's own button still reads
  "Local merge" (the card piece 15 redraws it).
- `otaq list --status approved` still groups tickets being delivered as "Approved" (CLI words left alone).
- The "Otaq doesn't watch PRs" note lived on the Approved section and is gone; the PR-open card and Approve ▾ still say it.
- `delivery:` is required: a flow file without it is refused instead of defaulting to a local merge on click.
- One delivery for every ticket of the flow; no per-ticket override.

## Canvas open points (decided 2026-11-07: UX-56…UX-61 in ideas/ux-rethink/DECISIONS.md)

Left open by the canvas, to be settled by the pieces in the simplest way and listed here when they come up:
- R5: Apply now for a pure reorder / kind change (no stage removed) — what happens to agents running inside the
  affected stages; "Stop here" after a check runs out reads the same as not deciding; "Allow 1 more round" adds one.
- 3a: "Same as…" when there are several earlier projects; where the stage templates live.
- 3b: automatic PR delivery with no provider set up; provider detected from the remote or picked.
- UX-56: is the merge message rewritten after a send back? One squashed message vs "commit messages".

## Piece 4 · Send back to an earlier stage (merged)

`otaq ask human --send-back "<stage>" --stdin`; the question offers "Send back to <stage>" / "Answer here…". On send
back the asking session stops (its conversation kept, as when a check's fail sends a ticket back) and the stage sent
back to resumes or starts fresh.
- **Worth a look:** the agent's reason is posted as *your* comment ("Dev asked to send SHOP-1 back to Product spec:
  …") so the stage sent back to reads it. Should it be authored by the agent instead?
- An agent may name any earlier stage it is past, including a human check or an agent check. Only stages that run an
  agent?
- "Answer here" leaves no History row (the question and answer are in Messages, like any question). Record it in
  History too?
- Typing an answer straight into a checker's terminal doesn't close its question (the app's answer does).

## Piece 13 · Plain names (merged)

- The shipped flows' agent "Validator" is now "Checker". On an existing install the next project you add creates a
  "Checker" agent beside your "Validator" (a new project gets the agents its flow names). OK, or rename yours by hand?
- On an old install, adding an "An agent checks" stage in the editor names "Checker" by default and flags it as unknown
  until a Checker agent exists.
- The limit banner at a reached limit says "Resets at 3:45 pm" with no "0% left" ("% left" shows on the windows). OK?

## Piece 2 · Start puts a ticket into the first stage (merged)

- Starting a ticket that waits for a dependency is now allowed (it waits at the top of the first stage), and the
  "SHOP-20 is blocked · Keep it waiting" dialog is gone, as the decisions say.
- The migration leaves tickets untouched: a ticket in Ready for Dev already was "started, nothing begun yet".
- A waiting ticket always shows at the top of the flow's *first* stage, even if it skips that stage (its card then
  says "Dev next · skips…"). OK?
- "2nd in line" counts this project's waiting tickets only, though agent limits apply across projects.
- Start is a quiet secondary button in the card's one action line. Old wire names (`ready_for_dev` drop target, the
  Apply-now choice id) stay for older clients and the CLI.
- The website demo still shows Ready for Dev (piece 29).

## Piece 6 · Create & start (merged)

- **Shortcut:** in the full form ⌘↵ already means Create (Save when editing), so Create & start got ⇧⌘↵. In the box for
  new work (piece 19) ⌘↵ means create *and start*. Switch the full form to ⌘↵ = Create & start for consistency?
- "Create ticket" stays the primary button, Create & start secondary (the first-ticket card makes Create & start
  primary). Flip it?
- Limits and setup failures aren't start refusals: Start only queues the ticket, and those show afterwards as the
  card's waiting state ("Waits for a free agent", Held, Start interrupted). A refusal from Create & start comes only from
  a flow rule. Intended split?
- First-run "notifications are asked on the first Create & start" is left for piece 28.

## Piece 10 · Search lists tickets, `in:` and `by:` (merged)

- An unknown `in:`/`by:` word stays typed text with "0 results", like other unknown filters. Want "No stage named …"?
- ↵ with nothing highlighted opens the first listed ticket and leaves the board filtered. Clear the filter too?
- The list stops at 50 rows (the count shows all). Fine until piece 24's popover?
- Multi-word names are typed with dashes: `in:product-spec`, `by:rust-developer`. OK?

## Piece 14 · The window shell (merged)

Interim homes until pieces 21 / 27: the board toolbar has a **Settings** button (today's project settings overlay)
and an **Edit flow** button (today's editor). Cleanup's rail broom is gone (Settings, or `cleanup` in search).
Differs from the canvas: the Running popover's contents are unchanged (no per-ticket rows); Noyan, Terminal and the gear
sit at the rail's foot; the limit text says "agents on other tools keep working" without naming them; the paused and
limit banners have no ×.
- ⇧⌘. opens the Pause menu only while agents run; resuming is by the banner or the palette. Should ⇧⌘. resume a paused
  project?
- ~~Should the paused and limit banners get the canvas's ×?~~ **Yes (2026-11-07)**: built in piece 25. Closing
  hides the banner until its state changes; Resume stays in the palette (and the usage chip for a limit).
- `shell.cleanup` and `shell.open_folder` have no caller in the page now: remove, or leave for piece 25?
- Should the Running popover get the canvas's per-ticket rows later?

## Piece 26 · The Files view's copies (merged)

The picker: "SHOW FILES FROM" (base branch, then tickets in flow order) and "YOUR WORKTREES · COMMITTED ONLY" ("Your
folder · <branch>" first). To list your main folder, the core listing of worktrees now keeps the main checkout
(files are still read from git as committed, never from disk).
- Ticket rows keep today's detail ("Dev · 3 files") instead of the canvas's "Strong's copy, working now" / "In Review"
  (the picker's data has no agent name or live state). Add it?
- "No worktree yet… gets its worktree when it reaches Dev": say "copy" to match the canvas?
- The source bar still reads "main · every ticket's changes" rather than the canvas's "Showing main".

## Piece 24 · Search popover and palette (merged)

The empty palette leads with "Next that needs you" (⌘J's order), then New ticket, Edit flow, Ask Noyan, Pause this
project, Settings, Open a terminal; recent items follow under "Recent".
- Keep the recent items under the suggestions, or show only the suggestions?
- "Pause this project" → "Pause <project name>" as the canvas has it?
- The filter strip under the toolbar ("Showing 3 of 17 · chips · Clear") was handed to piece 15 (board).

## Piece 22 · App settings (merged)

Eight topics: Agents, Git & pull requests, Limits & usage, Notifications, Appearance, Keyboard, Data, About. Agent
edits are a draft kept only on Save. Git & pull requests shows git and one row each for GitHub and Azure DevOps (no
"plugin 1.2", Add a provider… or GitLab until providers are plugins).
Differs from the canvas: the agent editor replaces the list (not a floating panel); Limits has three rows; no data size
in Data (Cleanup doesn't measure sizes); the short keyboard list keeps the app's action names.
- "Not while you're looking at that ticket" shows as a fixed "Always" line, not a toggle (no such setting exists).
  Make it a real setting, as the canvas draws?
- Notifications are three groups; each sets every kind in it at once, and the individual kinds aren't reachable any
  more. OK?
- Short keyboard list: rename actions app-wide to the canvas's names ("Next that needs you", "Search", "Ask Noyan")?

## Piece 15 · The board and its card (merged)

Column headers come from the stage's kind ("An agent does the work", "A checker tests it", "You check it" / "You review
and approve" on the last stage). Card words follow the canvas; only three edge colours; hover lights what waits for a
card; dragging a waiting ticket ahead is refused with a toast; a filter strip shows under the toolbar while searching.
Differs from the canvas: "An agent does the work" (not "writes the code": words come from the kind, never the stage's
name); Review / PR cards show no agent name (no "last agent" fact); Done keeps oldest first.
- **Worth a look:** Idle keeps its turquoise edge (it is still a Needs-you item in the core); the canvas draws it calm.
  Should Idle leave Needs you?
- Done order: newest first, as drawn?
- Backlog's "Ready to start" now reads "Not started" on the card. OK?

## Piece 21 · The project's settings page (merged)

Settings is the 4th view (Board · List · Files · Settings), one page: Flow, Repository (base branch, Delivery as the
flow's setting, "Pull requests through", merge check), Run, Workspace, Notes for agents, Remove project. Delivery
writes a new flow version through the editor's path.
Differs from the canvas: no "Suggest a flow…" here (it's in Edit flow's Start from…); the Flow card's ⋯ keeps Use a
preset / Copy / Import / Reset; Run, per-ticket resources and the variables table keep today's rows; the provider line
has no gh version or account; the "changed" line doesn't say what changed.
- The Settings view is remembered per project like Files, so a restart can reopen on Settings. Always come back to
  the Board?
- Changing Delivery while Edit flow has an unsaved draft is refused ("save or cancel it in Edit flow first"). Apply
  on top of the draft instead?
- Should the project key ("Used in ticket IDs, can't be changed") reappear on the page?

## Piece 25 · Restarts and upkeep (merged)

One restart dialog ("N agents were stopped by a restart", rows ticked, a tool that can't run unticked with its reason,
Noyan conversations included, per-row Retry / Start from scratch); the "sessions were interrupted" banner is gone; the
paused and limit banners have a × (hidden until the state changes); the menu-bar menu as drawn, with **Quit Otaq**
asking "Keep them running / Quit and stop agents" while agents work; Cleanup from Settings → Data with its size.
- The dialog title counts every stopped row (the canvas's "3" over 4 rows suggests only the resumable ones). Which?
- An unticked signed-out row shows only the reason (as drawn); it used to offer Sign in…. Bring it back?
- "Restart to update · stops N agents" stays for an update the background service can't carry over. OK?

## Piece 17 · Reviewing a ticket (merged)

A Review ticket opens on Overview; the review bar on every tab names Approve by its action with a ▾ of the other
choices; Approve & merge / open PR open their dialogs at once. **The merge message / PR text is written when the
ticket reaches Review** by the flow's `delivery: agent:` (documented as such; a local delivery may now name one), via
`otaq pr message`; the dialog holds a plain message and fills in unless you've typed. Open PR's skill now runs only
after Push & open PR. "<Provider> isn't set up on this Mac" with Set up… in the PR dialog.
- The message is asked for again after a send back only if the branch moved (new commits).
- A message stuck "writing" (the agent died first) stays so until the next arrival with new commits. Clear it when the
  session ends?
- The writing request shows as a waiting agent message. A dedicated kind / wording?
- Automatic local deliveries still use the plain message and ask nobody. Right?
- The prepared title starts with the ticket key (one title serves the squash commit and the PR); the canvas's PR title
  has none. Drop it for PRs?
- The plain fallback message is today's ("SHOP-18: title" + the agent's summary), not the title alone.
- ⌘↵ in the review bar still opens the Approve choices rather than the primary action.

## Piece 19 · The box for new work (merged)

The pill "New ticket, or ask Noyan…" opens a box: ↵ Create, ⌘↵ Create & start, ⇧↵ Ask Noyan to shape it, More
options…; only on Board and List; quick create and the Ask bar are gone. Piece 28 hides it until the first approve.
- **New line is ⌥↵** (⇧↵ is Noyan's; the canvas doesn't say); a "⌥↵ new line" hint shows in the footer. OK?
- After ↵ creates a ticket the box closes (as quick create did). Keep it open for the next one?
- Esc closes the box and keeps the draft (the pill shows its title). Clear it instead?
- The box has a title line over a details area (the canvas styles them differently).

## Piece 16 · The List is a layout of the board (merged)

Views are Board · Files · Settings; beside them, while the board shows, a Columns / List layout switch (`g b` / `g l`,
remembered). Rows are the board's card word for word, grouped by stage in flow order; folds are shared with the board.
- Backlog sits **after** the stages in the List (as drawn), though the board puts it first. Which?
- Inside a stage the List keeps the board's order *without* the board's lift of needs-you cards to the top, so answering
  never moves a row (the board itself still moves an answered card). OK?
- List rows show a dot for waiting (as drawn); the card shows an amber hourglass. Match the card?

## Piece 23 · Noyan's panel (merged)

Created tickets on top ("CREATED 2 · IN BACKLOG"), each with Start (`board.start`); a started row says where it is and
why it waits; the switcher lists recently ended conversations with Resume; intake-only wording ("Write a ticket for …").
- **Worth a look:** the canvas's proposal card ("PROPOSED · 2 TICKETS", Create 2 in Backlog) isn't built: Noyan's
  proposal stays its own text in the terminal, as its skill says. Want a real card? (needs a service + skill hook)
- Changed rows have no Undo (the service has no undo for Noyan's edits).
- The agent line keeps today's naming (tool only with two or more tools, tokens, Rename) rather than "Strong · Claude
  Code · opus".
- The header keeps …, Recently ended and Rename beside New conversation (End agent lives in …).

## Piece 18 · The ticket's tabs, History and editing (merged)

History filters All · Questions · Checks · Sessions, plain sentences newest first, one quiet row per agent session
("Session · Dev · Strong", short id with a copy button, times and tokens). Session's switch named by stage. ⋯ menu:
Edit ticket…, Ask Noyan about this, Pin, Copy branch name, Move to next stage…, Close…, Delete…. Comment / Tell <agent>
now buttons (migration 0108 records who was told).
- "Ask Noyan about this" always opens a new conversation pre-filled "About SHOP-27: ". Continue an existing
  conversation about the same ticket instead?
- History doesn't name the agent on agent rows (events don't record which agent acted). Record it?
- Not built from the canvas: the Session terminal's "Your typing goes straight to the agent…" line; the thread box's
  new placeholder; "Merge into another ticket…" in ⋯ (it stays in the Approve menu).

## Piece 20 · The full new-ticket form (merged)

Title, description, priority (P3), Waits for, Attachments (several, any type; stored as your first comment "at
creation"), folded "Stages for this ticket" (uses the stage's agent / another agent / Skip this stage only where the
flow lists `skip`), folded base branch; Cancel / Create / Create & start (primary). Cancel from More options… gives
the text back to the box. Tried in the real app.
- A ticket with both a skip and another agent shows only one line on the card ("Skips Tech spec" wins). Order OK?
- The new picker wording also shows in Edit ticket…'s form (shared view model), but the new layout is only on the
  new-ticket form. Give Edit ticket the new layout too?
- `FirstComment.report` kept so older apps can still send one; drop it at the next protocol bump?

## Piece 27 · Editing the flow on the board (merged)

Edit flow sits at the end of the board and turns it into the editor in place: a bar (changes + Undo, Watch a ticket,
Start from…, ⋯, Cancel, Save flow), stage headers, a stage panel with "What it's for" (no Instructions), the + palette
of seven ready stages (shipped as data in crates/otaq-core/flows/stage-templates.yaml), Watch a ticket, Start from…
led by Suggest a flow (Noyan's agent), delivery shown on Done. Saved changes and Decide as today's model. The overlay
editor is gone. Not looked at on screen by its agent.
- The Security review template's skill is `security-check` (not `security-review`, a Claude Code built-in the flow
  check refuses); templates use the presets' agents (Checker, Developer, Spec writer, Architect). Use the agent of the
  nearest stage of the same kind instead?
- An agent stage with no skill keeps Save off ("needs a skill"), though the canvas says "Empty uses Otaq's own skill".
- The missing-skill warning shows on the palette row before a stage is added, too. OK?
- Undo history is lost when the editor closes (the draft is kept). Keep it?

## Piece 28 · A new user's first run (merged)

Welcome → Tools (agent tools + git; no gh/az, no permission warning) → Project → the board on Otaq default with
"Your first ticket" in Backlog. Create & start asks macOS for notifications first (not seen live: debug builds aren't
asked). One-time hints at the first permission prompt, question, checker send-back, Review and, after the first approve,
"That was your flow"; the box for new work and the usage-data banner wait for the first approve. Add project has the
Flow choice (Otaq default / Same as <latest project> / Suggest one). The old flow editor and Flow setup page are gone.
Migration 0111 treats any data folder with a project as past first run. Walked live in the debug app.
- The permission-warning banner (`notices.permission_warning`) can no longer be raised. Remove it?
- Core flow-setup machinery (`first_flow.rs`, `flow_setup_since`, `OnboardingUseDefault`) is still in otaq-core and
  maybe the CLI. Remove it?
- "First approve" is recorded when a ticket ends Merged, so a PR flow reaches it only at Mark merged. OK?
- The old "Allow notifications?" banner at the first agent start stays as a fallback for a first ticket started another
  way. Drop it?
- A newer app on an older service (same protocol) reads `first_approve` as false until the service restarts. Bump the
  protocol?
- Not built: the ticket view's "Approve or send back" hint (one first-Review hint is enough). "Same as" offers only the
  most recently added project. The suggest line names "Noyan" rather than a model.

## Piece 29 · The README, docs and website match the new app (merged)

README, docs/how-it-works.md, install.md, the website landing and its demo now use the new names and screens; six
screenshots refreshed from a real run (board, board-light, terminal, review, diff, new plan.png). The readme-screenshots
scripts now target only the demo's own app (they could have hit the installed /Applications Otaq). Website `npm run
check` and check.sh green.
- The README's hero video likely still shows the old UI (Ready for Dev…). Re-record it, or drop it for board.png?
- App oddities seen while shooting: the ticket sheet stays open over Edit flow when opened from the "That was your
  flow" hint; `otaq list` still prints "Ready for Review" / "Validating…"; "8 running" looked higher than the busy
  agents (helpers counted?); a Path read "Review — · round 1" after a conflict fix.
- Should plugin/skills/open-pr and restack drop "Local merge" too?
- The demo run added /private/tmp/otaqdemo/pocket-notes entries to ~/.claude.json (usual for the skill).

## Bringing it together · Check lines

Every piece's Check: line passes, in tests or with the live CLI; the parts only visible in the window are covered by
tests (9, 14, 17, 18, 19, 25, 28). The standing rules hold. Leftovers found, being fixed in one follow-up: the CLI and
Noyan's panel still named old columns (Ready for Review / Approved); UX numbers in piece 19's comments; "Validator"
on the website; stale comments about Move waiting to… and the Delivery stage.
- App Settings has eight topics (Git & pull requests added by UX-57); the plan said seven. Fine as is?
- The merge check is still a project setting outside the flow. Should it move into the flow, given that the flow file
  alone should say everything?

## Bringing it together · Live walk and canvas comparison

Walked Welcome → Tools → Project → "Your first ticket" → Create & start → first question → Review → Approve & merge →
box for new work in a debug build with real Claude Code, light and dark (shots in /tmp/otaqwalk/shots/). Found: the first
merge fails without a git identity; open questions can't be answered in the app; the Review ticket view is still the old
dense one; "running"/Pause count kept sessions; hints pile up after the first approve; layout and wording leftovers.
Four fix agents are on these (findings in the scratchpad walk-findings.md).
- Done is a "Done N →" link, not an always-there end column; UX-12 says Backlog and Done are the fixed ends. Make Done
  a real column on the board, or keep the link (it saves width)?
- Not exercised live: PR flows, conflicts, Spec first, restarts and the menu bar.

## Follow-up · CLI and Noyan's panel name the real stage (merged)

`otaq list`/`show` name the stage and the app's state words; `--status` takes stage names, state words and the old
values; Noyan's close dialog says "SHOP-35 in Validate". UX numbers gone from code; website says Checker.
- Core `Status::label` still says "Ready for Review", "Validating…", "Validation failed" (app pills and service
  messages such as `otaq stage next`). A follow-up will bring them to the plain names after the fix agents land.

## Follow-up · Running count, hints, board layout (merged; checked by tests, not seen in the app)

"N running" and Pause count only agents at work (a kept session in Review doesn't); the Running popover lists a row per
counted ticket; Pause rows only while something works. After the first approve the ticket view closes, "That was your
flow" then "New work starts here" (at the box) show one at a time, and the usage banner waits for them. List's Edit
flow sits at the end of the rows; the rail badge no longer covers the key; empty columns say plain words.
- An open Noyan/Project-agent conversation still counts toward Pause (not Running), since Pause stops it; an idle one
  can show Pause with no Running. Count it only while it replies?
- A delivery done by the ticket's own build agent isn't counted while it waits as "To deliver" (a separate delivery
  session is). OK for so short a step?
- Card hints wait for the board columns on the List. Point at List rows too?

## Follow-up · Onboarding, settings, palette, Files, Noyan wording (merged; seen in the app, light and dark)

Edit flow's ready stages say quietly "Uses a skill you add: …" (no warning on a fresh machine); the missing-skill
warning is amber. Tools shows install commands, says "Settings → Agents", no capability chips. Welcome's columns show
in dark. Tab keys g o / g s / g c / g h. Noyan's suggestions only ask for tickets. Project Delivery always shows On
Approve · Automatically (off under Either); switching to Merge no longer drops the delivery's agent.
- Security review / Test in the browser / Update the docs use skills you add. Should Otaq ship skills for them?
- Should Edit flow's Done panel also show On Approve · Automatically (off under Either), like project settings?
- Should the "Otaq can see" chips leave Settings → Agents too (the canvas doesn't draw them)?
- Users who customised the old g f / g a tab keys keep their keys; only defaults changed. OK?

## Follow-up · First merge without a git name and email (merged; checked by tests)

Tools and Settings → Git & pull requests show "Needs your name and email" with a small form (writes global git config
only on Save; doesn't block Continue). Approve & merge asks first ("Save and merge"); automatic delivery stops with a
plain reason. Every failed merge is logged; toasts wrap inside the window. The merge dialog no longer closes on a stray
outside press (the walk's "first click did nothing" was the screenshot script's focus click closing it).
- Require the identity on the Tools step? (Optional now, for people whose identity comes from env variables.)
- Offer "only this repository" (repo-local) as well as global?
- readme-screenshots' shot.sh focus click on the window's top edge closes open dialogs; bring the window forward
  another way?

## Follow-up · Ticket view in Review and open questions (merged; seen in the app)

An open question shows first on Overview with a "Type your answer" box (the strip chip opens Overview too); both primes
list "The user's answers" so the checker reads them. Review's Overview: one line under the title, no doubled Approve /
Send back, no Delivery step, no Choices/Path/model ids, empty blocks hidden, "Check passed · on the first try", the
Approve ▾ menu lists every way to finish with the canvas's line, one status badge on every tab. Core status words are
now Ready for you / Checking / Check failed / Check ended without a result.
- Hiding an empty Plan hides "Write a plan" too. Offer it somewhere quieter (⋯)?
- Drop the sheet view's unused `details`, `choices`, `path`, `token_split` from Rust?
- Ticket badge: "Ready for you" (board's word) or "Your review" (canvas's sheet header)?
- The Needs you strip's Review chip opens Changes, not Overview. Change it?
- The flow path stays (minus Delivery); the canvas draws none.

## Bringing it together · Second live walk (main 043d18e5)

Every finding of the first walk is fixed in the running app (no-identity merge, answering on Overview, the Review
ticket view, running/Pause, hints one at a time, palette notes, layout, wording, checker reading answers, Delivery
choice). New: "That was your flow" couldn't be dismissed (blocked the rest of the first-run sequence); agents committed
under a borrowed name when git has no identity; Edit flow's + between Dev and Validate didn't open; smaller layout and
wording items. Two fix agents are on them (scratchpad walk2-findings.md).
- I chose to ask for git's name and email before an agent first starts (Save and start), as the merge already does,
  so agents never commit under a borrowed name. Keep, or only at merge?
- The List orders Backlog after the stages and keeps "Fold empty columns" (known from piece 16).

## Follow-up · Walk-2 fixes: hints, Edit flow, Running popover (merged; seen in the app)

"That was your flow" has Got it (and Esc); then "New work starts here", then the usage banner. "Your turn: review" is
answered once its ticket is opened. One Esc closes only the topmost thing. Edit flow's palette closes on Esc / its
"esc" label and switches on another +. The Running popover closes when nothing runs, in plain words. Card times match
Running. The palette's example key is the open project's. Board columns have a 1px edge for dark mode.
- The "+ between Dev and Validate" in walk 2 was a macOS notification window over the app, not an app bug.
- The column edge shows faintly in light mode too. Dark-only?
- Esc does nothing while the project switcher or Pause menu is open without focus (it used to close the ticket). OK?

## Follow-up · Walk-2 fixes: git identity before start, ticket view, wording (merged; seen in the app)

Start, a drop from Backlog and Create & start (also "Your first ticket" and the box) ask for git's name and email first
(Save and start) when the repo has none; automatic starts are Held "git doesn't know your name and email yet" and go
back to the queue once saved. Tools opens at its top. The answer box is 3 lines (↵ sends, ⇧↵ new line) and "You
answered" stays visible. Agents' "used in" names the last stage ("Review · opens PRs"); Merge check help says Approve &
merge. Review's thread doesn't repeat "What the agent did" / Checker. Session's Run only with a run command; header is
"Developer · Claude Code · tool default".
- Resumes (send back, answers) aren't held for a missing identity, only new conversations. Hold them too?
- A Held-for-identity card offers the generic Retry. Open the identity form instead?
- A project added to a repo that already has otaq/<KEY>… branches picks them up for new tickets. Worth guarding?

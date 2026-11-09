# Gaps: New ticket box, Noyan panel, Files view

Slice: `new-01-ticket.txt` (5 frames), `noyan-01-panel.txt` (3 frames), `files-01-view.txt` (2 frames).
Paths are relative to the repo root. UI = `crates/otaq-desktop/ui/src/screens`, VM = `crates/otaq-view/src/view_models`.

## Summary: 5 biggest gaps by size of work

1. **One box for new work (M).** It replaces quick-create (`UI/editor/Modal.tsx:129`) and the Ask bar
   (`UI/ask_bar/AskBar.tsx:47`) with one docked multi-line box. The keys change meaning: ↵ = create, ⇧↵ = Noyan,
   ⌘↵ = create & start. Today ↵ in the Ask bar sends to Noyan, and ⇧↵ in quick-create opens the full form. This is
   mostly a UI merge (B). Create & start is new glue (small C). It drops the Ask bar's "about the selected ticket",
   "Continue <conversation>" and "New reply" (D).
2. **Per-stage agent picks on every stage (M, C/E).** Today `form_choices` (`VM/stage_choices.rs:231`) gives a
   picker only to stages whose flow declares options, and offers agent, skip and required there. The default flow
   has none. The design shows a select on every stage (Dev, Validate) of what looks like the default flow, and no
   skip. Picking any agent on any stage needs core and rule changes, and it bypasses what the flow YAML declares
   (conflicts with "the flow YAML alone defines behaviour").
3. **Files: "Your folder, as it is on disk" (M, C/E).** Today the code refuses this on purpose: a user worktree is
   read "from git objects at its HEAD … never the checkout's files on disk" (`crates/otaq-core/src/board/project_files.rs:74`).
   Showing it needs a new source in `project_files.rs` that reads the working tree, with uncommitted and untracked
   files marked, plus refresh.
4. **New-ticket full form fields (S–M, B/C).** It adds Waits for (today dependencies are the sheet's only, DPV-04:
   `crates/otaq-desktop/src/api/editor.rs:7`, though create can link deps, `:670`). It adds several optional
   attachments with no line (today: one report, and a line is required, d0109, `UI/editor/NewTicketReport.tsx:1`).
   It drops Start from / merge into and External ID (D).
5. **Noyan's behaviour shown in the mock conflicts with the intake-only rules (S in UI, but it's product copy and
   skill behaviour, E).** Noyan makes 2 tickets with no proposal (N2: "a proposal is still shown first for two or
   more tickets"). It asks an architecture question, "memory or Redis?" (N3: "never about design or
   architecture"). It diagnoses root causes (beyond a light look). If this is copied into the website or demo
   data, the wrong model spreads.

Smaller items: Start on Noyan's created rows (S, B); a Files view that drops the filters, Same file, All changes,
Changed by and Diff (D, removal only); a panel header that adds the model and "Reads code, never edits it" (A).

---

## new-01-ticket · "One box for new work"

### Frame 1 · One box for new work (docked at bottom; ⌘N / click / toolbar New ticket focus it; replaces quick create and the Ask bar)
- Today:
  - ⌘N → quick-create title popover over Backlog (`UI/board/index.tsx:26`, `UI/editor/Modal.tsx:121-196`);
    Backlog's + → quick-create (`UI/board/Board.tsx:953`);
  - toolbar New ticket → the full form (`Board.tsx:891`, `{full:true}`);
  - the Ask bar floats bottom centre over the board, List and ticket (`UI/ask_bar/AskBar.tsx:1-5`,
    placeholder "Ask Noyan… (“why is checkout slow?”)", `VM/ask_bar.rs:27`).
- **B**: three entry points collapse into one docked box, "New ticket, or ask Noyan…". ⌘N and the toolbar button
  focus it instead of opening overlays.
- **D**:
  - The Ask bar's "ASK ABOUT" suggestions: the selected ticket and Continue "<last conversation>"
    (`VM/ask_bar.rs:34-43`). The design only starts new Noyan conversations from the box. Looks like an omission:
    no caption mentions it.
  - "New reply" on the bar (`ask_bar.rs:58-66`). Questions still reach the strip, but a finished turn the user
    hasn't seen has nowhere to show except the rail dot.
  - Backlog column's + (no + is visible).
- **Open**:
  - Is the box on the List, the ticket view and the Files view too? Today the Ask bar is on board, List and ticket.
    Files' ⌘N today opens New ticket with the file's path (`UI/files_view/FilesView.tsx:5,398`), which clashes with
    "⌘N focuses the box".
  - Does the box hide while the Noyan panel is open, as the Ask bar does (`AskBar.tsx:173`)? Frame 2 of Noyan shows
    the box still on the board while the panel is open.

### Frame 2 · Typing a ticket (first line title, rest description; ↵ create in Backlog, ⌘↵ create & start, More options…)
- Today: quick-create is a single-line title only; ↵ create, ⇧↵ full form, Esc cancel (`UI/editor/logic.ts:12`).
- **B**: a multi-line box where the first line is the title and the rest is the description (S, page only). ⇧↵ is
  reassigned from "full form" to "Ask Noyan". The full form moves to a "More options…" link.
- **C (S)**: Create & start. There is no create-then-start call in `api/editor.rs`, and EditorForm has no start flag
  (`generated/EditorForm.ts`). It needs a create plus a move to the first stage (the Backlog card's Start path) in one
  service call, with the start's refusals (limits, setup failure) shown to the user.
- **E**: none. ↵ "no agent, no cost" matches today's rule that nothing starts an agent without the user's words
  (`UI/ask_bar/logic.ts:1-4`).
- **Open**:
  - What does ⌘↵ start into on a Spec-first flow: the first stage, whatever it is? It should be, by "the flow YAML
    alone defines behaviour".
  - Can a screenshot be pasted into the box? It can into today's form, as a report.
  - What priority does ⌘↵ get? Normal.
  - Typing "make tickets" then pressing ↵ would create a ticket titled that. Should there be a hint?

### Frame 3 · Ask Noyan to shape it (⇧↵): Noyan reads the code, writes one or more tickets to Backlog, panel opens on the left
- Today: Ask bar Return → `ask_bar.send` (New conversation) → the panel opens on it (`UI/ask_bar/logic.ts:61-87`).
- **B**: the same action, triggered by ⇧↵ from the shared box. The second field, "Add details (optional)", is new UI
  only.
- **E (wording)**: "writes one or more tickets" is acceptable only with N2's limits: several tickets only for
  separate deliverables, and a proposal first for 2+. The caption omits the proposal step. See Noyan frame 2.
- **Open**: does "Add details" go into the same first message, or is it a separate field? (No backend need.)

### Frame 4 · More options… the full form (title and description carried over; priority, waits for, attachments optional; the ticket's agents fold away)
- Today, the full form fields (`UI/editor/Fields.tsx:315-409`):
  - Title;
  - Priority, as a segmented control;
  - Stage choices (`Fields.tsx:53`);
  - What and why, a Markdown editor with toolbar;
  - **Start from and merge into** (base branch, `Fields.tsx:381`, `api/editor.rs:321`);
  - **External ID** (when shown);
  - one **report** with its required line (`UI/editor/NewTicketReport.tsx:1-6`).
- **A**: Priority as a select ("P3 · Normal"). The description hint becomes "Say what should change and how you'll
  know it works. Agents read this first."
- **B**:
  - "Waits for": dependencies are deliberately sheet-only today (DPV-04, `api/editor.rs:7`; the test asserts
    `deps == null`, `:1296`). Create already links deps and reports partial failure (`api/editor.rs:670`), so this
    is a form field plus the dep picker (`VM/dep_picker.rs`) (S). It reverses DPV-04.
  - Stage choices fold into an "Agents for this ticket" disclosure.
- **C (S–M)**: "Attachments · optional · Drop files or paste screenshots", several and with no line. Today it is one
  report and a line is required (d0109). It should reuse tuq 001–007, which cover comments only (several
  attachments, any type, attachments alone allowed). It needs the create API to take N attachments as the first
  comment, or as ticket attachments.
- **D**: Start from and merge into (base branch, d0051), and External ID. No caption explains either. Probably an
  omission. Base branch matters for release-branch work.
- **Open**:
  - Are the attachments a first comment (as reports are today) or a new ticket-level attachment list?
  - Does an attachment still need a line?
  - Where does a base-branch pick live now?

### Frame 5 · A ticket's own agents (one select per stage, empty = "Uses the stage's agent"; editable until that stage starts)
- Today:
  - `form_choices`/`ChoicesField` (`VM/stage_choices.rs:6-11,231`, `Fields.tsx:49-79`) is a picker only for stages
    with flow-declared options, and none on the default flow. Its values include `skip` and `required`.
  - It is locked once the stage started (`stage_choices.rs:141`); that part matches.
  - Who set it is shown ("set by Noyan", "· by you").
- **C/E**: one select on **every** stage, for any agent, on what looks like the default flow (Dev: Strong, Validate:
  Verifier). That is new behaviour. It also goes around the flow's declared options, which conflicts with "a flow
  YAML alone defines behaviour" unless the flow grants it.
- **D**: skip and required choices are absent. "Agents for this ticket" suggests agent picks only. If deliberate, the
  user loses the only way to skip a stage at creation. Today skips are the user's alone: N12, N4.
- **E-check**: Validate shows a single "Verifier", which matches "one agent per check stage".
- **Open**:
  - Should the picker list every configured agent, or only the flow's options?
  - Where did skip go? The sheet only?
  - Does "Uses the stage's agent · Strong" update if the flow's agent changes before the stage starts? Today it
    preselects the stage's own agent.

## noyan-01-panel · "Noyan · ask, panel, question"

### Frame 1 · From the box (⇧↵ starts a conversation on Noyan's agent; panel opens left; ⌘L opens the panel itself)
- Today: the same flow via the Ask bar (`UI/ask_bar/logic.ts:61`). ⌘L = `focus_agent_panel` (`VM/ask_bar.rs:1-6`).
  The panel is 420 px between the rail and the main area (`UI/agent_panel/AgentPanel.tsx:1-9`).
- **B**: only the entry point changes (see new-01 frame 3).
- **D**: Continue an existing conversation from the bar, and asking about the selected ticket (see above).

### Frame 2 · The panel, with what it made (Noyan's terminal; created tickets on top, in Backlog, with Start on each; you start them, never Noyan)
- Today:
  - The header has Noyan, a conversation switcher, a state pill, …, Recently ended, +, Expand and close
    (`AgentPanel.tsx:4-8`).
  - The agent line is "Strong · 52k", with the tool only when two tools differ (`VM/agent_panel.rs:15-18`).
  - "CREATED n · CHANGED m" rows show key, title, where it is, and Delete for a never-started created ticket
    (`VM/agent_panel.rs:257-331`), plus Show on board (`:272`).
  - Noyan can't start tickets (`plugin/skills/noyan/SKILL.md:216-217`). "Never Noyan" matches.
- **A**:
  - The agent line adds the tool and model ("Strong · Claude Code · opus") and drops tokens.
  - A tagline "Reads code, never edits it". It is true per the skill (`SKILL.md:37,210`), but incomplete: Noyan also
    changes the flow when asked (`SKILL.md` intro).
  - "CREATED 2 · IN BACKLOG".
  - The footer "Typing goes to Noyan ⌘L".
- **B (S)**: a **Start** button on each created row. Start exists on Backlog cards; it needs wiring into
  `CreatedRow` plus the start's refusals.
- **D**:
  - CHANGED rows (tickets Noyan edited).
  - Delete on a created row (TKT-08).
  - Show on board.
  - Recently ended, if not behind the icons.
  - The tool-problem callout.

  Probably simplification by omission; the caption doesn't say.
- **E (design content vs Noyan rules)**:
  - The transcript runs: "Two causes … Created SHOP-35 and SHOP-36 in Backlog." That is two tickets with **no
    proposal**. N2: "A proposal is still shown first for two or more tickets." `SKILL.md:134`: "propose first, then
    wait for the user's go-ahead."
  - Root-cause diagnosis ("38 calls", "blocks the first paint") goes beyond N3's light look. It's borderline, but the
    mock sells Noyan as an investigator.
- **E (current app, not the design)**: today's tool-problem callout offers "New agent on Sonnet (its named agent's
  fallback…)" (`VM/agent_panel.rs:20-23`). That is a fallback agent, against "no fallback agents". The design
  dropping the callout is an opportunity; the rule needs a replacement ("waits").
- **Open**:
  - Do created rows show tickets that were since moved out of Backlog? Today's rows say "where it is". The heading
    says "IN BACKLOG".
  - The top bar goes "4 running" → "5 running". Does an idle Noyan conversation count as running?
  - What does the panel do when the user picks a conversation in the switcher while the box has text?

### Frame 3 · Noyan asks you (same answer box as a ticket, above the terminal; also in the Needs you strip)
- Today, this exists:
  - `otaq ask human` gives a question with options answered above the terminal (`VM/agent_panel.rs:398`,
    `UI/agent_panel/AgentQuestion.tsx`).
  - The strip shows "Noyan · <name> · <reason>" (`VM/needs_you_strip.rs:238,527`).
  - "Asks you" on the pill and bar (`VM/ask_bar.rs:58`).
- **A**: only the look differs.
- **E**: the example question, "Should the price cache live in memory or in Redis? 1. In memory 2. Redis", is a
  design or architecture question. N3: "Noyan asks the user only about the *request* … never about design or
  architecture, which belong to the spec stages or the Dev agent" (`SKILL.md:123-124`). Noyan should leave it as an
  open question in the ticket. Swap the copy for a request-scope question ("Which page feels slow, cart or
  checkout?").
- **Open**: the user typed "cache product prices?" about an existing Backlog ticket. Is asking Noyan about a ticket
  still supported, now that the bar's "about the selected ticket" is gone?

## files-01-view · "Files · the project's code"

### Frame 1 · Files (browse main or any ticket's copy; changed files marked; New ticket from this file opens the full form)
- Today it is richer (`UI/files_view/FilesView.tsx:1-9`, `VM/files_view.rs:1-15`):
  - a source bar;
  - Go to file (⌥⌘P);
  - filters All · Changed · Needs you · Same file;
  - a tree with per-ticket chips and +/−;
  - a viewer with per-ticket bars beside the lines and a "Changed by" list (`files_view.rs:715`);
  - a Diff toggle and Copy path (`generated/FilesViewFile.ts:38`);
  - the All-changes view (`UI/files_view/AllChangesView.tsx`).

  New ticket from this file opens the full form with the path (`UI/editor/Modal.tsx:209-212`), which matches.
- **A**: "Changed in SHOP-18 (in Review)" stands in for Changed by's rows. A ticket's worktree is called a "copy".
- **D**: the filters, especially **Same file**, the overlap warning when two tickets edit one file; the All-changes
  view; Go to file; Diff; Copy path; the per-line ticket bars; and multiple tickets on one file (the design shows one
  owner). No caption says these were cut. Probably simplification by omission, and Same file looks like a real
  loss.
- **Open**:
  - With two tickets changing a file, does the line list all of them?
  - Is search or Go to file kept?

### Frame 2 · Which copy (Showing… picks main, a ticket's copy, or your own folder)
- Today the picker has BASE BRANCH, TICKETS · THEIR WORKTREE (and tickets without a worktree), and YOUR WORKTREES,
  read-only from git HEAD (`VM/files_view.rs:51-58`, `core/board/project_files.rs:74-92`). A picked ticket shows its
  own Files tab read-only.
- **A**: the row subtitles ("Strong's copy, working now", "In Review").
- **C/E**: "Your folder · ~/code/shop-web, as it is on disk". Today's code says explicitly "never the checkout's files
  on disk" (d0087, `project_files.rs:74`); d0087 is from the lost decision log. It needs a new working-tree source,
  showing uncommitted and untracked files, refresh, and possibly large or ignored files. It also changes what the
  view means: the user's uncommitted work becomes visible next to agents'.
- **D**: other user worktrees (the design shows only "your folder", the main checkout). Tickets with no worktree yet.
- **Open**:
  - Does "your folder" replace YOUR WORKTREES, or add to it?
  - When the main checkout isn't on the base branch, is it labelled by its branch?

## Open product questions (slice-wide)
1. Is the bottom box on every view, and how does ⌘N in Files resolve: focus the box, or New ticket from this file?
2. Ask Noyan about an existing ticket, or continue a conversation: dropped, or moved somewhere?
3. Stage picks: every stage with any agent, or the flow's options only? Is skip removed at creation?
4. New-ticket attachments: are they the first comment with a required line (today), or plain attachments under tuq
   001–007?
5. Base branch and External ID: where do they go?
6. Does ⌘↵ "start" mean the flow's first stage, and what does the box show when the start is refused (limits, setup
   failure)?
7. Should the mock's Noyan copy be rewritten to follow N2/N3 (proposal first; request-only questions)?
8. "Your folder as on disk" reverses d0087: confirm with the maintainer.

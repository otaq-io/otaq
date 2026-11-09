# Gap analysis: ticket sheet (ticket-01 … ticket-06)

Paths are relative to the repo root. `V` = crates/otaq-view/src/view_models, `UI` = crates/otaq-desktop/ui/src/screens,
`CORE` = crates/otaq-core/src.

## Summary: the 5 biggest gaps, by size of work

1. **Permission prompts answered in the app (Deny / Allow once)** — L, C. Today Otaq only shows the tool's prompt
   text and "Answer in terminal" (`CORE/agent_tools/events.rs:22`, `CORE/attention/needs_you.rs:789`). Answering
   needs a per-tool path (hook response or keystrokes into the PTY) for Claude Code, Codex, Copilot and OpenCode, plus
   a "why it asks" line Otaq can't currently derive.
2. **A live "what it's doing now" line** ("writing tests", "editing 4 files", "running the tests") — M–L, C. Today
   `working_on` is a fixed phrase per Otaq-shipped skill (`V/planning.rs:81`, `V/board.rs:449`); nothing reports what
   the agent is doing from moment to moment. Needs new tool events (PreToolUse-style, which differ across the 4 tools) or an
   agent-reported status (`otaq` CLI + skill text).
3. **PR watched until it merges / failed merge left in Review for the user to choose** — M each, C and E. Otaq never
   watches PRs (`CORE/review/pr_state.rs:1`, Mark merged / Mark closed), and conflicts and failed merge checks go
   back to the agent automatically (`CORE/review/mod.rs:122`, FLW-17, d0120). The design reverses both.
4. **Review-oriented Overview + tab restructure** — M, B. The sheet opens on Files during your check
   (`V/sheet.rs:139`); the agent summary sits on Files (`V/files_tab.rs:838`). The design opens on Overview with
   summary, checker, decisions and ticket, a split "Approve & merge into main" button, tabs renamed to
   Changes/History, and the details line, STAGES column, Attachments column and Move… removed.
5. **Comments always reach the working agent when it's idle; one-time hints; combined "New ticket, or ask Noyan…"
   box** — S–M, B/C. Today comments are "for the next stage", with an opt-in "Also tell the agent now"
   (`V/ticket_comments.rs:32,294`, d0123). There's no one-time hint system. The Ask bar is Noyan only
   (`V/ask_bar.rs:28`).

Smaller: the Changes tab loses the worktree tree and File view (S, D/B). History loses the session rows and filters (S, D).
The edit form gets "Waits for" and loses Base and External ID (S, B/D). The plan's card shrinks to one line (S, B).
The PR dialog's skill-prepared proposal and commit messages are dropped (S, D/question).

Comment attachments (this branch, tuq 001–008) appear nowhere in the design: no Attach button, no thumbnails, no
Attachments column, no attachments in Send back. This looks like an **omission**, because the design predates the
branch. Keep them.

---

## ticket-01 · Reviewing your first ticket

### 1.1 The ticket in Review
App: `UI/sheet/Sheet.tsx:385-409`, `UI/overview/Overview.tsx`, `UI/files/ReviewBar.tsx`, `V/files_tab.rs:1150`.
- **B** The design opens on Overview. The app opens on Files (All changes) while the ticket waits in your check
  (`V/sheet.rs:139`, SHT-06).
- **B** "WHAT THE AGENT DID" moves to Overview. Today it's the folded "Agent summary" above All changes on Files
  (`V/files_tab.rs:827-838`, d0017).
- **A** CHECKER "Validate passed · on the second try · Full report" is today's "Before you · Validate, round N of M"
  (`V/overview.rs:296`, `V/validation_report.rs`). With one agent per check stage, the multi-agent "Before you" form
  is moot.
- **A** DECISIONS THE AGENT MADE with Change… already exist (`UI/sections/Decisions`, `V/decision_item.rs`).
- **A** Tabs are renamed: Files→Changes, Activity→History (`V/sheet.rs:56-71`).
- **A/B** The review bar under every tab exists (`UI/sheet/Sheet.tsx:409`). The design's primary is a split button
  "Approve & merge into main" that names the action. Today it's "Approve", which opens the choice menu and then the
  Local merge confirmation (`UI/files/ReviewBar.tsx:27-31`).
- **D** Gone from the sheet header: the details line (Assignee, Model, Working, Priority, Branch, External ID), the flow
  path, the **Move…** button (`V/sheet.rs:1-5`), and Overview's right column (STAGES choices and path, Attachments)
  (`UI/overview/Overview.tsx:1-10`). There's no caption saying why. Probably deliberate simplification, but STAGES is
  where a ticket's per-stage agent choice is set today. Where does that go?
- **D** Not visible: the Local merge confirmation (editable squash message, "Runs `npm test` first when main has
  moved", who keeps waiting) (`V/approve.rs:340-390`). The caption says approve "merges … and moves it to Done". Is the
  confirmation step dropped?
- **C (S)** The one-time coach mark "Approve or send back · Got it". No generic one-time-hint store exists, so this needs
  persisted "seen" flags.
- **Question** "Goes back to the same agent with your notes." In flows where the human check follows several Agent
  stages, Send back goes to the `fail_to` stage (d0131, `V/send_back.rs`), which isn't always "the same agent".

### 1.2 The menu beside Approve
App: `V/approve.rs:27-40,153`, `UI/board/ApproveChoice.tsx`.
- **A** Approve & merge, Approve & open PR, Done nothing to merge, and Done don't merge all exist (approve-options plan
  pieces 1–3, merged on main).
- **D** "Merge into another ticket…" (A3, built) and "Push to PR #N" (open-PR tickets) aren't shown. Probably an omission.
- **Question/C (S)** The design lists "Done, nothing to merge" and "Done, don't merge" together. The app offers exactly
  one of them, depending on whether the branch has commits (A1/A2). "…or it's already in main" (commits already
  landed) is a case the app doesn't detect today.
- **E-risk** "The default for this project" has to come from the flow's Delivery `delivers`
  (`CORE/flow/model.rs:110-133`: local / pr / either). Offering "Approve & open PR" when the flow says `local` would
  be a rule the flow file doesn't state (flow-file-says-everything). The menu should list only what the flow's
  `delivers` allows, or the flow format needs an explicit "default + alternatives".

### 1.3 After the first approve
- **A** The toast "merged into main · Squashed as 9f3e2c1" exists (`V/review_page.rs:301`, `V/approve.rs:455`).
- **C (S)** Two one-time hints ("That was your flow · Edit flow", "New work starts here"), which need the same "seen" store
  as 1.1.
- **B/C (S–M)** A combined box "New ticket, or ask Noyan…": ↵ creates the ticket as typed, or hands it to Noyan. Today
  there's ⌘N's form and a separate Noyan-only Ask bar (`V/ask_bar.rs:1-30`). The box being "hidden until the first
  approve" is a new progressive-disclosure rule.
- **E-risk (wording)** "Noyan reads your code and writes it up, or splits it into several tickets". Under the intake-only
  rule, Noyan takes a light look and files several tickets only for clearly separate deliverables, never by size.
  "Splits" invites the size-splitting the rule forbids.
- Board-level: Done is its own "Merged" column here. Today Done is the Approved section of Review's column
  (`CORE/flow/default.rs:1`). That belongs to the board slice.

## ticket-02 · Overview while it works

### 2.1 An agent asks you
- **A** The question first, with numbered options, notes, the recommended option preselected, "Something else", 1–3/↵
  keys and "Answer in terminal". All of it exists (`UI/overview/NeedsYou.tsx:84-268`, `V/agent_question.rs:3`,
  `UI/sheet/actionRoutes.ts`).
- **A** Header "Asks you · In Dev · Strong · started by you 12 min ago" (`UI/overview/NeedsYou.tsx:449` MetaLine).
- **B/C (S–M)** The comment box reads "the agent reads it next time it's idle". Today the placeholder is "Add a comment
  for the next stage…", and the current agent is told only if you tick "Also tell the agent now" (d0123;
  `V/ticket_comments.rs:32,294`, `CORE/board/ticket_comments.rs:57-75`). The design drops the tick box, which means
  every comment notifies the running agent (or check).
- **D (omission)** No Attach button or thumbnails in the comment box/thread (tuq 001–004, 007, this branch).

### 2.2 An agent asks for permission
- **C (L)** "Strong wants to run a command · `$ npm install …` · Deny · Allow once", with an explanation ("Claude Code
  asks before installing packages") and where it runs. Today Otaq gets only the prompt string
  (`CORE/agent_tools/events.rs:18-28`) and answers it in the terminal (`CORE/attention/needs_you.rs:789`, "a permission
  prompt is answered in the terminal"). Needs: the structured command/tool from each tool's hook (Claude Code
  Notification/PermissionRequest, Codex PermissionRequest, Copilot permission_prompt, OpenCode permission.asked); a way to
  answer it for each tool (hook decision return or PTY keystrokes, fragile across tool versions); and the "why" text.
  Must work for all 4 tools, or the UI has to degrade to "Open terminal" per tool.
- **Question** Is there "Allow always", and does Otaq ever write tools' permission settings? (README: "your own …
  permission modes".)

### 2.3 Working, nothing needed
- **C (M–L)** "Working · writing tests for the share link" is a live activity line. Today it's a fixed phrase per
  Otaq-shipped skill only (`V/planning.rs:81`). The board cards in the design show the same thing ("editing 4 files",
  "running the tests").
- **A** WAITING FOR IT is the Blocks half of Dependencies (`V/dependencies.rs:1-10`).
- **D** "Blocked by" and "Add" (choose direction, search, `UI/dependencies`) are gone from Overview. Waits-for moves to
  the edit form (6.2).
- **A** "Open session" is a link to the Session tab.

## ticket-03 · Session, Changes, History

### 3.1 Session
App: `UI/session/Session.tsx:1-7`, `V/session_tab.rs`, `UI/session/TerminalTab.tsx`.
- **A** The switch "Agent · Validate · Run · Terminal" is today's "Agent · <check agent name> · Run · Shell". The check
  side is named by stage instead of agent, and Shell is renamed Terminal ("UX-5: one Terminal" refers to another
  artboard. If it merges the ticket Shell with the project Terminals drawer, `UI/terminals/Drawer.tsx`, that's B).
- **B** `otaq attach SHOP-27` shown under the terminal. Today it's "Copy attach command" in ⋯ (`V/sheet_actions.rs:429`).
- **A** "Your typing goes straight to the agent; Otaq's own notes wait until it's idle" matches the delivery queue
  (`CORE/board/change_note.rs:5`).
- **Question/D** The Session list for split tickets (in charge + one row per helper, `UI/session/Session.tsx:5-7`,
  `V/pieces.rs`) isn't drawn. How do you reach a helper's terminal in the new UX?

### 3.2 Changes
App: `UI/files/FilesTab.tsx`, `V/files_tab.rs:1-20`, `V/diff.rs`.
- **A** Unified/Side by side (Split), "2 files · +9 −1", line comments, and the review bar's "1 comment goes with it" all
  exist.
- **D** Gone: the worktree tree (All files / Changed / search ⌥⌘P), File view vs Diff for one file, "New ticket from this
  file", and Open file. The rail's Files screen (`UI/files_view`) may absorb some of this, but nothing says so.
- **B** The agent summary and Decisions leave this tab for Overview (1.1).

### 3.3 Send back
App: `V/send_back.rs:1-40`, `UI/review/Dialogs.tsx`.
- **A** Confirm dialog with the note and "1 line comment in api.ts goes with it".
- **D** Not shown: the list of tickets that keep waiting (`SendBackPlan.waiting`), and the note box's attachments
  (comment box chips, tuq 001–007).
- **A/Question** "Strong continues in the same session". True when the fail_to stage is the agent's own. See the 1.1
  question.

### 3.4 History
App: `UI/activity/History.tsx`, `V/history.rs:1-15,186,226`.
- **A** Newest first, plain sentences, questions and answers, decisions, check rounds, send-backs, "created by Noyan".
  All of these are recorded today (`CORE/board/tickets.rs:326+`, `V/history.rs:93-100,320`).
- **D** Gone: the session rows (tool session id, copyable for the tool's resume; duration; end reason), the filters All ·
  Comments · Stages · Description, and description before/after diffs. Session ids matter for recovery (REC-03), so
  that loss looks like an omission, not a deliberate choice.

## ticket-04 · Plan and helpers

### 4.1 On the board
- **B** One muted line, "Plan · 1 of 4 pieces merged", plus "Asks you · piece 3 asks". Today, once helpers start, the
  card grows a "PIECES · 4" block with a row per piece (`V/pieces.rs:309`). The one-line form already exists
  (`V/pieces.rs:382`, `V/plan.rs:127`).

### 4.2 The Plan on Overview
App: `UI/overview/Plan.tsx`, `V/plan.rs:1-40`, `V/pieces.rs:1-20`.
- **A** Pieces in order, with a status beside each (merged / working / asks you / waits for N), and Edit plan (today:
  edit in place, d0078 rules).
- **A** A helper's question answered on Overview. Today a helper's question goes to the agent in charge first, which
  may pass it on (d0133 audit 3, `V/pieces.rs:72-74,364`). The design shows it as already passed on, which is
  consistent.
- **A** Pieces numbered 1–4 vs "40.3". Wording only.
- **Question** "Written by Strong when it split the work." Today the Plan is written by Milestone plan, Noyan or the
  user (`CORE/domain/plan.rs:147`). Per the intake-only rule, Noyan shouldn't write size-based plans, so the
  design's attribution (Dev in charge or Milestone plan) fits the rule better than the current `NO_PLAN` text
  (`V/plan.rs:33`, "Milestone plan or Noyan writes one").
- **D** Gone: piece states "dropped", "sent back", "waiting for limit reset", "was working"; the tokens split; and the
  Session list (3.1).

## ticket-05 · Delivery: PRs and problems

### 5.1 Approve & open PR
App: `V/pr_dialog.rs:1-92`, `UI/pr/Dialogs.tsx`.
- **A** Branch, Title, Description, Into, provider/repo, and Push & open PR.
- **D/Question** "Prefilled from the ticket". Today a built-in skill prepares the proposal ("Preparing" stage) and each
  commit message is editable. The design shows no preparing step and no commit list. Is the skill step dropped (C:
  plain templating) or just not drawn? The live branch-name check, "not logged in" and Azure DevOps aren't shown.

### 5.2 On the board
- **E/C (M)** "An open PR stays in Review until it merges on GitHub." Otaq deliberately never watches PRs (no polling,
  no webhooks; `CORE/review/pr_state.rs:1-2`, `CORE/pr/mod.rs:2`, d0017/d0033/d0049). The user clicks Mark merged /
  Mark closed. Implementing this means polling `gh`/`az` and reversing that decision. If the design only *omits* Mark
  merged, that's a D.
- **E/C (M)** "A merge that can't happen turns the card red, in Review, with what to do." Today a conflict or failed
  Merge check goes back to the nearest earlier Agent stage automatically with `restack` (no click), and stops after 3
  in a row (`CORE/review/mod.rs:122-131`, FLW-17, d0120, `V/approve.rs:380-386`, `V/card.rs:1003-1084`).

### 5.3 What went wrong, on Overview
- **E/C (M)** "Ask Strong to fix" / "Send back to Dev" / "See output" are user choices. Today none of them is needed,
  because the hand-back is automatic. Choosing the design means a new stopped state in Review, and dropping or
  conditioning the automatic hand-back. A flow-level switch for that would itself be a new flow feature.
- **Question** "Open terminal" for a conflict. The rebase is aborted and the worktree is clean (`V/approve.rs:458-461`),
  so what would the user do in that terminal?
- **D** The early conflict flag ("Conflicts with main" before approve, `CORE/review/early_conflict.rs`) and the 3-in-a-row
  stop ("Keeps conflicting") aren't shown.
- **A** "Nothing was merged" and the cause in words already exist (`V/approve.rs:380-386,455-465`).

## ticket-06 · ⋯ menu and editing

### 6.1 The ⋯ menu
App: `V/sheet_actions.rs:395-433`, `V/close.rs:20-45`, `V/delete.rs:15-25`.
- **A** Edit ticket…, Pin to top, Copy branch name, Move to next stage… ("agent is working"), Close ticket…, and Delete…
  ("Started tickets can only be closed") all match today's menu, disabled reasons included.
- **B** Copy attach command moves out to Session (3.1). Stop this check / Skip this check (shown while checking) aren't
  shown in this frame (fine, the ticket isn't checking).

### 6.2 Editing in place
App: `UI/editor/EditForm.tsx:1-5`, `V/editor_fields.rs:20-35`, `V/agent_activity.rs:1-4`.
- **A** The form replaces Overview in the sheet. "Strong reads your edit the next time it's idle" exists (change note
  through the delivery queue).
- **B** "Waits for" moves into the form. Today dependencies are edited in Overview's Dependencies section (DPV-04,
  `V/editor_fields.rs:3`).
- **D** Not in the form: "Start from and merge into" (base branch, d0059) and External ID (d0133), both shown on every
  ticket today. Probably an omission. Per-stage agent choices are gone from the form too, and the STAGES column is gone
  (1.1).

## Open product questions

1. Where does a ticket's per-stage agent choice ("Dev: Light instead of Strong" on SHOP-29) get set and seen, now that the
   details line and the STAGES column are gone? And who set it? If Noyan, that's barred: Noyan sets no stage choices of
   its own, only the user's.
2. Does Approve still confirm (squash message, merge-check line, who keeps waiting), or merge on one click?
3. Which approve choices appear is set by the flow's `delivers`, or by a project default (not in the flow file)?
4. PRs: watch them (reverse "never watches"), or keep Mark merged / Mark closed and the design just omits them?
5. Merge conflicts and failed merge checks: keep the automatic hand-back, or stop in Review for a click?
6. Comments: always tell the running agent (drop d0123's opt-in), and what about comments posted during a check?
7. Permission prompts: how much of Allow/Deny is in scope per tool, and is "Allow always" ever offered?
8. Where do the worktree tree, File view, session ids/resume info, History filters, helpers' terminals, base branch and
   External ID live now?
9. Live activity line: does it come from tool events, or from the agent reporting via `otaq`? Which tools can't support
   it?
10. Comment attachments (tuq 001–008): confirm they stay in the comment box, thread, Overview, and Send back, even though
    the design doesn't draw them.

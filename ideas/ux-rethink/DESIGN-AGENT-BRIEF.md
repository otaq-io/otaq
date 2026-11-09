# For the design agent · changes to "Otaq — New UX" after the gap review

2026-11-07. The canvas was compared with the app (full report: `GAP-REPORT.md`, details per area in `gaps/`). The
maintainer decided R1–R4 and took two drawn behaviours out of scope; those are changes to draw. R5 is a gap for you to
work on: section 3 explains it.

## 1 · Decided: redraw these

**UX-23 · Otaq doesn't watch PRs.** (ticket-05, board-02/03, A44, UX-13)
- An open PR card stays in Review as "PR #481 open". Its action is the user's: **Mark merged** / **Mark closed**
  (the app has both today).
- Remove "moves to Done when merged on GitHub" and the "2 comments on GitHub" line: Otaq never polls GitHub or Azure
  DevOps.

**UX-24 · Delivery problems go back to the agent on their own.** (ticket-05 frame 3, board-03 "Can't merge", A10, A45)
- A conflict or a failed merge check merges nothing. The ticket goes back to the nearest earlier agent stage (Dev in
  the default flow), whose agent fixes it without a click. The card there reads like the app's "Conflict · fixing".
- Only after the **third conflict or failed check in a row** does it stop, in that agent stage, waiting for the user
  ("Keeps conflicting with main"). Draw that stop as the one red, needs-you case.
- Remove the red Review callouts with "Ask Strong to fix" / "Send back to Dev" / "Resolve" as the normal path.

**UX-25 · Delivery is a setting of the flow, in the flow file.** (board-04, settings-02, A5, A35, UX-10, UX-13)
- Still no Delivery stage and no Approved column; the Approve button still names the action.
- But how work leaves the flow is part of the flow: **local merge, PR or either**, and **on Approve or automatic**.
  A flow shared as a file must carry it.
- Edit flow shows it (for example at the Review/Done end of the editor). The project's Repository card shows the
  same setting and says it's the flow's (changing it there changes the flow).
- Presets set it: Solo autopilot = automatic, Team PR = PR.
- With "either", Approve offers both (today's app does: Approve & merge / Approve & open PR).

**UX-26 · A stage keeps its skill.** (board-04 stage panel, A2/A3)
- Add **Skill** to the stage panel, as agreed when Otaq stopped copying skills:
  - **Pick a skill…** lists the skills the user's tools would find (personal and project folders, read-only) plus
    Otaq's own (`/otaq:<name>`), with free text allowed;
  - **Ask the agent to write it** saves a new skill into the project's skills folder, for the user to commit;
  - a stage naming a skill nobody has shows a warning but still saves.
  - Empty means Otaq's own skill for that kind of stage.
- **Open question for you:** A3's new "Instructions · optional" field. With the skill back, is it still needed? The
  maintainer's rule is "no stage feature unless it is truly necessary". If you keep it, say what it does that a skill
  or the project's Notes for agents doesn't.

**UX-27 · Out of scope for now.** Draw without them:
- **The live "what it's doing" line** ("writing tests", "editing 4 files", "reading the README", "running the tests").
  Use the statuses the app has: "Working", "Checking", "Fixing · sent back by Validate", "Asks you · …".
- **Answering permission prompts in the app** (Deny / Allow once). A permission prompt card reads
  "Asks you · allow `npm install`?" and its action opens the ticket, where the prompt shows with **Answer in
  terminal**.
- "Validate found 2 problems" has no count source either. Drop the number ("Fixing · sent back by Validate").

**UX-28 · "Ready for your review" notification is off by default.** (settings-01 Notifications, A33)
- Keep the toggle, drawn off. The other notification toggles are for blocking things only (asks something,
  something broke).

**UX-30 · Files shows committed code only.** (files-01 frame 2, A36)
- Replace "Your folder · as it is on disk" with your folder as its checked-out branch, committed (for example
  "Your folder · main, as committed"), alongside your other worktrees. Otaq never shows uncommitted files.

**UX-31 · Skipping a stage, where the flow allows it.** (new-01 frame 5 "A ticket's own agents", board-03)
- On a stage whose flow lists `skip` in its options, the per-stage field also offers **Skip this stage**. Stages
  that can't be skipped don't offer it. The default flow has none; draw it on Spec first (Product spec, Tech spec).
- A card with a skip set shows one muted line, like the own-agent line: "Skips Product spec".
- In Edit flow's stage panel, a stage can be marked "Tickets may skip this stage".

**UX-32 / UX-55 · An agent asks to send a ticket back to an earlier stage.** (ticket-02 question, board-03 card states)
- Any agent that finds a problem belonging to an earlier stage (one the user skipped, or one that already ran) asks the
  user, naming the stage and why: "Product gap: refunds over 30 days aren't specified. Send back to Product spec, or
  answer here?" It is an ordinary agent question (turquoise, Needs you), with two ways out: answer in place, or send it
  back.
- After "Send back": the card is in that stage (a skip line, if any, is gone); History shows the reason and the answer.

**UX-53 · Every stage has a short description of what it is for.** (board-04 stage panel)
- Replaces the "why" line and is what agents read to understand the flow. Show it in the stage panel (one or two
  plain sentences: what the stage settles, when it matters) and wherever a user meets a stage for the first time
  (column header hover, the skip choice). This may answer your open "Instructions" question (UX-26): one field, not
  two.

**UX-54 · Agents don't skip.** A stage's agent with nothing to do finishes its stage at once with a note ("Nothing to
settle: README-only change"); draw that as an ordinary finished stage in History. Skips stay the user's (UX-31).

**UX-33 · List rows keep the board's order.** (board-05 frame 1, A40)
- Inside each stage group, draw rows in the board's order, not needs-you first. Needs-you rows stand out by their
  coloured edge only.

**Gap review 3 (UX-35 … UX-50; full text in DECISIONS.md).** Redraw:
- **UX-35** usage-data consent: a quiet one-time banner after the first approve; toggle in Settings → Data.
- **UX-36** the Claude Code permission-mode warning: a one-time hint at the first permission prompt, not on Tools.
- **UX-37** onb-02: Install and Sign in… open the tool's own terminal under its row, as today (passwords, device
  codes, single sign-on). Not "finish in your browser" with no terminal.
- **UX-38** no Group by Status / Agent, no Dependencies "Always"; Fold empty columns stays.
- **UX-39** dependencies only as "Waits for"; "Waiting for it" read-only on Overview.
- **UX-40** window-02: `/` filters the board in place (chips, as today) and also shows a popover of matching tickets.
- **UX-41** Import a flow (under Start from…) and Export (Edit flow, and the project's Flow card): draw both.
- **UX-42 · For you to solve:** Approve & merge keeps a confirmation where the user can edit the commit messages, but
  today the user waits while an agent prepares those messages. Find a way that doesn't block them (for example the
  messages prepared ahead, while the ticket waits in Review, or the dialog usable while they're being written).
- **UX-43 · For you to draw:** a comment goes to the next stage unless the user chooses to tell the running agent now
  (today a tick box, "Also tell the agent now"). Keep that choice; its form is yours.
- **UX-44** the early conflict (main moved while a ticket waits in Review): the ticket goes back to its agent stage on
  its own; the card just shows it there, fixing ("Conflicts with main · fixing"). No red mark unless it's the third in a
  row. Details on Overview.
- **UX-45** History: a quiet row per agent session, with its id copyable.
- **UX-46** the full form and Edit ticket keep base branch (folded); no External ID anywhere.
- **UX-47** the ticket's ⋯ menu gets "Ask Noyan about this"; conversations continue from the panel's switcher.
- **UX-48** the box for new work is on Board and List only.
- **UX-49** the limit banner has Resume <tool>, active once the reset time has passed. Never a fallback.
- **UX-50** Limits & usage: the agent limit is a switch, off by default ("Run at most N agents at once").
- **UX-51** every stage is its own column: no two stages in one column.
- **UX-52** Workspace's environment variables are only the ones the user sets; no values read from `.env.local`.

## 2 · Copy that breaks a rule or isn't true

- **UX-20 hint / new-01 frame 3.** "Noyan … splits it into several tickets" → Noyan is intake only. It files several
  tickets only when the request is clearly several separate things, never to make work smaller, and it proposes them
  first when there are two or more. Suggested: "…or files separate tickets when you asked for several things."
- **noyan-01.** Same rule:
  - Its transcript creates two tickets without proposing them first.
  - It asks "memory or Redis?", but Noyan asks only about the request, never design; that belongs to the spec stages
    or the Dev agent.
  - It diagnoses root causes, but Noyan takes a light look only.

  Swap in a request question ("Which page feels slow, cart or checkout?") and a proposal step.
- **A28** "Reads code, never edits it" → Noyan also changes the flow when asked. "Never edits your code" is true.
- **onb-03.** "Your folder and your current branch are never touched" is false: a local merge fast-forwards the base
  branch in your folder when it's checked out and clean. Say what is true: agents work in their own copies; only an
  approved merge updates your base branch.
- **Main / board-01 hint 5.** "Approve merges it into main" → true only when the flow delivers by local merge. The
  default flow offers either.
- **care-02.** The menu bar's "Restart to update · stops 4 agents" contradicts A53 ("working agents continue"). Today a
  normal update restarts only the app; agents keep working.
- **settings-01, window-01 (UX-34).** Usage as "85% of the 5-hour limit" → always **% left**: "15% left · resets
  3:45 pm", in the title-bar chip, the Running popover, Limits & usage and the limit banner.

- **board-04 Spec first preset.** The shipped Spec first is Product spec → Tech spec → Milestone plan → … , not
  "Spec → your check → Dev → Validate → Review". Use the shipped stages (`crates/otaq-core/flows/spec-first.yaml`).

## 2b · Not drawn, and they stay as today

The canvas was never meant to cover every detail. These exist in the app, nothing decided removes them, so they stay;
draw them where they fit your layout when you get to them:

- **Onboarding**: the folder-trust line ("Claude Code, Codex and Copilot will run their hooks and settings in this
  folder"); "Can't be changed later" on the ticket key; "already in Otaq" and "can't add this folder" errors; dropping a
  folder onto the Project step; the install command visible before Install; a tool row's "Check failed".
- **Card states** (keep each, with UX-21's colours): check out of rounds (Decide), Idle, Held (base diverged), Parent
  closed, Resume failed (Start from scratch), Merging · merge check (Stop), Merged into another ticket, Parked · has
  work, plan limit stop. The hover facts and quick actions; Resume all.
- **Ticket**: comment attachments (several files, paste/drop/Attach, image thumbnails; built on this branch, tuq
  decisions 001–008) in every comment box, the thread and Send back; "Push to PR #N" in the Approve menu of an open-PR
  ticket and "Merge into another ticket…"; the worktree tree, single-file view and Open file on Changes; History's
  filters; each helper's terminal in Session for a Plan; the PR proposal prepared by Otaq's skill and editable commit
  messages (that is what fills the PR dialog); Send back's list of tickets that keep waiting.
- **Noyan**: the panel's CHANGED rows, Delete on a never-started created ticket, Show on board, Recently ended; the
  tool-problem callout, but without any fallback (the agent waits).
- **Files**: filters (All · Changed · Needs you · Same file, the warning when two tickets edit one file), All changes,
  Go to file, Diff, Copy path, per-line ticket bars, several tickets on one file, your other worktrees.
- **Settings**: Logs folder (Data); Theme credits (About or Appearance); a tool's Sign in…, Hide, Re-check,
  Disconnect, "Otaq can see"; an agent's Effort, "What this agent reads", Duplicate, default model for new agents;
  back-pressure ("don't start while Needs you has N"); the notifications master switch and Play a sound; the terminal
  renderer fallback; the full shortcut list (behind the short one) and the single-letter switch; a project's Name;
  "Let finished tickets answer questions"; ticket resources' template refresh and schema paths; Run's "Ask the agent".
- **Restarts & upkeep**: Noyan conversations in the restart dialog; per-row results (Resumed / Couldn't resume · Retry ·
  Start from scratch); Cleanup's groups, its "Done tickets with uncommitted changes" warning and warm builds; the menu
  bar's "Your checks and stopped tickets".

## 3a · Open gap for you to investigate: how a project gets its flow, and how people design their own

Not decided; investigate and propose. This is the most important part of Otaq, so here is everything it is today.

**Why it matters.** The tagline is "Coding agents that follow your process", and the flow *is* the process: the board's
columns are its stages (UX-1). It is also the most complicated part of the app. The maintainer's two constraints pull
against each other, and your design has to satisfy both:
1. **Don't introduce it at install.** A new user starts on the default flow and learns it by watching their first
   ticket cross the board (UX-14, UX-16). No Flow step in first run.
2. **Designing your own flow must be proper and fun, never a boring form.** Because it is complicated, the editor is
   where the product earns trust. Think of how the user discovers it (Edit flow offered when the first ticket reaches
   Review), what they see first, and how changing it feels.

**What a flow is today (all of it in one YAML file, which can be exported and imported; UX-41):**
- An ordered list of stages between the fixed Backlog and Done (UX-12). Each stage is its own column (UX-51).
- Three kinds of stage: an agent does the work; an agent checks it; you check it (A2).
- Per stage: a name; its agent (any named agent; a ticket may pick another, UX-11); its **skill** (UX-26: Pick a
  skill… from the user's own skills and Otaq's, free text, or Ask the agent to write it; a warning for a skill nobody
  has); whether a ticket may skip it (`options: [skip]`, UX-31); for checks, where a fail goes back to ("fails go to",
  the nearest agent stage before it by default) and, for agent checks, how many fails in a row before it waits for you
  (rounds 1–5, default 3; see section 4).
- For the whole flow: how work is delivered at the end: local merge, PR or either; on Approve or automatic (UX-25).
- Shipped flows (presets): Otaq default (Dev → Validate → Review), Spec first (Product spec → Tech spec → Milestone plan →
  … → Dev → Validate → Code review), Solo autopilot (automatic delivery), Team PR. Start from… replaces the stages
  between Backlog and Done; nothing changes until Save.
- **Made for <project>**: an agent (one per connected tool) reads the project and proposes a flow for it. Today this is
  offered in onboarding's Flow step and in Start from…. The proposal lands as a draft with the agent's changes marked;
  the user applies it, cancels it, or asks for more changes, which go back to that agent.
- Noyan can also change the flow when the user asks; its proposal arrives the same way.
- Copy the flow from another project; go back to the previous flow; "changed by" line on the project's Flow card.
- Today's editor also has a "Watch it" animation of a ticket walking the draft flow.
- How a saved change reaches tickets that are already moving: section 4 (open).

**What to propose:**
- Where a project gets its first flow: every project starts on the default flow, or a lightweight choice when adding
  a second project, or something else. Today every project added after the first opens a Flow setup page.
- Where "Made for <project>" lives now that the Flow step is gone, and when it's suggested.
- How the in-place Edit flow (A1) makes building a flow enjoyable: discoverable, visual, previewable (what would a
  ticket go through?), forgiving (nothing changes until Save; Cancel).

## 3b · For you to propose: where PR provider tools live (gh, az)

Keep them out of onboarding: a new user doesn't need them until their first PR, and that was the point of moving them.
But the drawing puts them in each project's Repository settings (A12), and they are installed and signed in **once per
machine**, not per repository. The settings rule (UX-10) is: app settings = you and your machine; project settings =
this repository. The Tools page that checked git, gh and az is also gone, so git has no home after onboarding.

**The roadmap matters here:** pull request providers (GitHub, Azure DevOps, GitLab and later others) are going to
become **plugins the user installs**. Design for that:
- a place in app settings where provider plugins are added, with each one's tool status and sign-in;
- the project's Repository card choosing which installed provider this repository uses, with one status line where
  it's used and a way to the fix;
- what the user sees when a ticket is about to open a PR and the provider isn't installed or signed in.

Propose it; the maintainer will decide.

## 4 · R5 · When a flow change reaches tickets, and check rounds: the gap

The drawing (board-04, A3, A4) and the app disagree in four places. Two of them are the hardest behaviour in the
whole redesign, so please draw the app's model, or propose a simpler one and say exactly what it trades away.

### How the app works today

- **Adding a stage or editing one applies at once.** It's stored as a new flow version now. A ticket that hasn't
  reached that point follows it; tickets in it or past it finish as they are. A stage's agent, skill and rounds are
  read when that stage next starts.
- **Removing or reordering stages, or changing a stage's kind, waits.** It waits for its *affected span*: the stages
  from the first to the last one the change touches (moving Security before Validate touches Dev to Review).
  - The change becomes the project's **waiting change**.
  - It applies once no ticket inside the span is at work. Quiet means in Backlog, Done, or waiting in a human check
    the new flow still has.
  - **Meanwhile no ticket enters the span.** A ticket that would reach it stops at its edge, and its card reads
    "Waiting for the flow change". Tickets before the span move on freely, and tickets past it are untouched.
- **A whole-flow swap** (Start from… a preset) waits for the **whole board**: no ticket starts until it's quiet.
- **Apply now** forces a waiting change. The user picks a stage for each ticket in a removed stage; the default is
  the **next** stage.
- **Cancel change** drops the waiting change.
- **A second change while one waits** is merged into the waiting one.
- **Check rounds:** an agent check has a number of rounds, 3 when the flow doesn't say, editable 1–5 in the editor.
  When a ticket runs out, the card stops with **Decide**: the user allows more rounds, sends it on, or stops.

### Where the drawing differs

1. **"New tickets use it already"** (board-04 frame 5, A4). One project has one flow in use, plus at most one
   waiting change. Letting new tickets use the new flow while older ones finish on the old one means **two flow
   versions live at once on one board**: two column layouts, and tickets of each moving through their own. The app
   can't do that today, and it would be the largest single piece of work in the redesign.
   - The same frame also says "Applies when SHOP-26 leaves Validate", which is the app's rule. The two halves of
     frame 5 contradict each other.
2. **"Removing a stage: tickets in it move to the stage before"** (A3, frame 2). The app doesn't move them.
   - The removal waits until the stage is quiet.
   - If the user forces it with Apply now, they choose per ticket, and the default is the **next** stage, not the one
     before.
   - Moving work *back* would re-run a stage that already passed.
3. **No Apply now, no Cancel change, no "Waiting for the flow change" state.** The drawing has no way to see that a
   change is waiting, to force it, or to drop it. It also has no card status for a ticket held at the span's edge.
4. **Rounds fixed at 3** ("after 3 tries in a row it waits for you"), with no rounds field. The out-of-rounds card
   (**Decide**) isn't drawn either. The app lets a flow set rounds per check, and the flow file must say it (the flow
   file says everything).

### What to draw (or answer)

- The Edit flow bar after Save when the change has to wait. Suggested: "Saved · applies when SHOP-26 leaves Validate ·
  Apply now · Cancel change".
- Apply now's per-ticket stage pick for tickets in a removed stage.
- The card held at the edge of a changed span ("Waiting for the flow change"). It sits at the top of the stage before
  the span, like the other waiting cards (UX-12).
- The whole-board wait after Start from….
- The rounds field in the check stage panel, and the out-of-rounds card with Decide (allow more rounds / send on /
  stop).
- If you believe a simpler rule is better (for example "a change applies only when the board is quiet"), draw it and
  list what users lose. The maintainer will decide.

## 5 · Still open (more feedback will follow)

GAP-REPORT.md R6–R12 and its section 5 (things the design drops without a decision) haven't been decided yet. Don't
redraw them until they are.

# Otaq UX rethink · agreed decisions

Agreed with the maintainer on 2026-11-07, in a session with a design agent. Copied here as the record. The design is
drawn on the canvas "Otaq — New UX" (https://claude.ai/artifact/7tYuWTkYKLrq5gnUHSZWP1). The choices made while
drawing are in `ASSUMPTIONS.md`; the gap between the design and the app is in `GAP-REPORT.md`.

Rethinking Otaq's user experience as a whole, not screen by screen. Breaking changes are fine (released, never
announced, a couple of users). Inputs: the app at otaq-io/otaq (504ff17, the source of truth), README.md,
docs/how-it-works.md, and the as-is inventory from the alignment work. tuq/decisions/ hasn't been reviewed yet, so
decisions this UX would overturn aren't listed (known ones: UX-8 keeps the Inbox removed; UX-11 drops agents choosing
other stages' agents; UX-12 removes the Ready for Dev column; UX-13 removes the PR and merge stage kinds and the
Approved column; UX-16 removes the onboarding Theme and Flow steps; UX-22 removes quick create and the Ask bar as
separate controls).

Order of work: 1 mental model · 2 jobs · 3 information architecture · 4 onboarding · 5 screens. Steps 1–4 and the gap
review are agreed; step 5 is drawn on the canvas "Otaq — New UX" (a separate canvas; "Otaq — Screens" stays the record
of today's app until the redesign is built).

## Problems we're solving (from NT, 2026-11-07)

- P1 · Onboarding doesn't explain what Otaq is or how its parts fit together.
- P2 · Onboarding asks for the most advanced decision first (the flow).
- P3 · The tagline is "Coding agents that follow your process", yet the process (the flow) is reached only through a
  menu on the project name. The columns are the flow, so editing it belongs next to the columns, or at the top of a
  single project-settings page.
- P4 · Settings are split across three places with no clear rule; agents are where people don't expect them.
- P5 · Too many concepts show up on day one.
- P6 · The board is built for the power user.

## Step 1 · Mental model (agreed 2026-11-07)

How the core relates: a project is a git repo. Its flow is the ordered list of stages, and the board's columns are
those stages. A ticket walks the stages; at each stage an agent does the work or checks it, or you check it. When a
ticket needs you, it shows in Needs you. At the end you review and deliver (local merge or PR). Noyan sits beside
this: you talk to it to make tickets.

- **UX-1 · The board is the flow.** Its columns are the project's stages; the flow is taught on day one and changed
  from the board, not found through a menu.
- **UX-2 · One "check" concept.** Agent check, your check, Validate and Review are all check stages, done by an agent
  or by you; Review is your check at the end. "Validator", "Verifier" and "check agent" become "checker".
- **UX-3 · Fewer agent words.** "Tool" = the installed program (Claude Code, Codex, Copilot, OpenCode), named only in
  setup. "Agent" = a named agent (tool + model + instructions). Roles go: each stage names its agent. Helpers stay
  inside the ticket. Noyan is the agent you talk to, not a role.
- **UX-4 · No name collisions.** Title bar Activity → "Running"; ticket Activity tab → "History"; ticket Files tab →
  "Changes"; the project's Files view stays "Files".
- **UX-5 · One terminal idea.** Ticket Shell and project Terminals become "Terminal", scoped to the ticket's worktree
  or the project.
- **UX-6 · Hide the plumbing.** Worktrees, ports, rounds, slots, warm builds and ticket resources appear only where
  they need an action, never as concepts to learn.

Day-one concepts: project · ticket · board (= flow) · agent · Needs you · review & deliver. First week: changing the
flow · named agents · checks · Noyan · dependencies · Run · Pause. Later: Plan & helpers · worktrees & ports · rounds ·
a ticket's own agent for a stage (UX-11) · warm builds · ticket resources · merge check · limits · Cleanup.

## Step 2 · Jobs (agreed 2026-11-07)

- **UX-7 · Jobs by frequency:** answer an agent (many times an hour) › review and deliver (several a day) › start work
  (several a day) › capture work (daily) › watch or steer an agent (daily) › keep things running, plan a bigger
  feature, look around the code (weekly) › shape the flow, manage agents (setup, then rarely) › set up (once per
  machine / repo). The main screen serves the first three simply; rare jobs get obvious, plain places rather than
  shortcuts.

## Step 3 · Information architecture (agreed 2026-11-07)

- **UX-8 · Structure A, "one project, one place".** One app instance holds several projects (rail). Each project has
  Board · Files · Project settings. Project settings is one page: Flow at the top, then the project's other settings.
  The board's columns carry Edit flow, which edits the flow in place. App settings (one gear) hold only what's about
  you and your machine. Needs you stays on the board as the strip, with rail dots and ⌘J across projects. (Rejected:
  B, a cross-project Home for answering/review/starting, which would bring back the Inbox; C, a list-first project
  view, which buries the flow.)
- **UX-9 · Agents are defined once, globally,** on one Agents page in app settings (connected tools at the top, then
  your agents: tool + model + instructions). Each stage of a project's flow names its agent; a ticket may override it
  (UX-11). There are no role defaults and no per-project overrides. Noyan's agent is set on the Agents page and can be
  switched when starting a conversation.
- **UX-10 · Settings rule:** app settings = you and your machine; project settings = this repository.
  - App settings: Agents (tools · agents · Noyan's agent) · Limits & usage · Notifications · Appearance (theme,
    terminal font) · Keyboard · Data (where data lives, Cleanup) · About (version, updates, licence, PATH).
  - Project settings, one page in this order: Flow (stages and their agents; same editor as Edit flow) · Repository
    (base branch, delivery: local merge / PR provider, merge check) · Run (apps, commands, Open URL) · Workspace (setup
    command, environment variables and files, ticket resources, warm build) · Notes for agents · Remove project.
  - Agent limits and usage merge; Cleanup moves into Data (no rail button); the menu on the project name only switches
    projects.

## Gap review (agreed 2026-11-07)

- **UX-11 · A ticket can override a stage's agent,** in the smallest form: only you set it (agents never change
  another stage's agent; that feature goes); one optional field per stage in the ticket form, empty by default ("uses
  the stage's agent"); shown as one line on the ticket only when set ("Dev: Light instead of Strong"); editable until
  that stage starts, then read-only. No change lines, no Path, no lock concept.
- **UX-12 · Backlog and Done are the fixed ends of every flow:** always there, shown greyed at each end in Edit flow
  with no remove button ("every flow starts and ends here"); only the stages between them are editable. Ready for Dev
  goes as a column: starting a ticket moves it into the first stage; if it can't start yet, its card waits at the top
  of that column and says why ("waits for SHOP-12 to merge", "waits for a free agent").
- **UX-13 · Delivery is the Approve action that ends Review, not a stage.** The button says what happens, from the
  project's delivery setting: "Approve & merge into main" or "Approve & open PR"; a small menu beside it holds "Done,
  nothing to merge" and "Done, don't merge" (and, as a later case, "Merge into another ticket…"). A local merge goes
  straight to Done. With a PR the card stays in Review as "PR #481 open · waiting for merge" and moves to Done when
  merged, or to "Done · not merged" when closed unmerged. Delivery problems (conflicts, a failed merge check) put the
  card in Review with Needs you and say what to do. The Approved sub-column and the PR / merge stage kinds go: a flow's
  stages are only work and checks, and teammates' PR review happens in GitHub / Azure DevOps.
- **UX-14 · Concepts appear when they first matter,** with a one-time hint, never behind a setting. No simple/advanced
  modes. Controls appear when there's something to show (dependencies once a card has one, Run once there's a run
  command, a ticket's own agent line once set, Pause while agents run). One-sentence, dismissible hints at the first
  real moment: first agent question (turquoise card, ⌘J); first ticket in Review (what Approve does, Edit flow
  exists); first checker fail ("work that fails goes back to Dev"). The board's default look is calm: a card shows
  title, status, who's working and the one action; the rest is on hover or in the sheet. Board density itself is step
  5, following this rule.

## Step 4 · Onboarding (agreed 2026-11-07)

- **UX-15 · Only you start a ticket.** Every Backlog card has Start (dragging into the first stage does the same; ⌘D
  stays). The new-ticket form offers "Create & start" next to "Create". Noyan never starts tickets: it writes to
  Backlog, so spending only begins with your click. Once started, a ticket waiting for a dependency or a free agent
  starts by itself when it can, and its card says what it waits for.
- **UX-16 · First run is Welcome → Tools → Project → the board.** Welcome explains Otaq in three lines and one small
  diagram of the model ("You write a ticket → agents work it through your board's columns → you answer when they ask
  → you review and deliver"), with one button. Tools finds git and the installed, signed-in agent tools and helps fix
  what's missing (one tool is enough). Project picks a git folder (name and key filled in). The board opens with the
  default flow; each column header says in plain words who acts there ("Dev · an agent writes the code", "Validate · a
  checker tests it", "Review · you"). No Theme step (theme follows the system; change it in Appearance). No Flow step:
  the flow is learned by watching the first ticket, and Edit flow is offered when it reaches Review (UX-14).
- **UX-17 · The first ticket** is a "Your first ticket" card in Backlog, prefilled with a tiny task that works on any
  repo and is safe to merge (title "Tidy the README", description "Fix typos and broken links. Change nothing else."),
  editable or replaceable in place with the hint "small, clear tasks work best". Above the button, an honest line:
  "Takes about 5 minutes and uses your <Dev stage's tool> plan." Buttons: Create & start (primary) and Not now (leaves
  an empty Backlog with a plain New ticket button). No Noyan in onboarding. After the click, the UX-14 hints take over.
- **UX-18 · Notifications are asked for on the first Create & start:** the OS prompt comes with one line, "So you know
  when an agent asks you something, even with Otaq closed." If declined, a quiet banner offers it once more at the
  first real agent question; after that it lives only in Settings → Notifications. The first time you close the window
  while an agent works, a one-time note says "Agents keep working. Otaq is in the menu bar."

The path a new user walks: Welcome (the model) → Tools → Project → board with the default flow → "Your first ticket" →
Create & start (notifications asked) → the card crosses Dev and Validate → first question or fail, each with its hint →
Review: Approve & merge, with Edit flow offered.

## Step 5 · Screens

- **UX-19 · The title bar** holds: left, the project name and base branch (the name only switches projects); centre,
  search ("/"); right, Running (agents and Runs, with usage per tool at the foot of its popover) and Pause, shown only
  while agents run. Usage shows only when it needs you (an amber "Claude Code · 15% left" chip, or a limit banner).
  New ticket moves to the board's toolbar, beside the view switch Board · Files · Settings; ⌘N focuses the box for new
  work (UX-22). App settings is the gear at the foot of the rail, with Terminal and Noyan.
- **UX-20 · The box for new work (UX-22) is not on a new user's board.** It appears once the first ticket is approved,
  with a one-time hint: "Type a ticket and press ↵ to create it as written. Or Ask Noyan to shape it: Noyan reads your
  code and writes it up, or splits it into several tickets in Backlog. You start them."
- **UX-21 · The card shows at most four lines plus one action.** Always: key, title (two lines at most), one status
  line with a coloured dot, who's working and for how long, at most one action. Only when it matters: a P1/P2 chip (P3
  and P4 show nothing), one muted waiting line ("Waits for SHOP-12 to merge"), one line when you've set the ticket's
  own agent. Never on the card: tokens, model, rounds, Plan pieces, Run ports, decision counts, conflict details; they
  live in the sheet, and the status line says when one needs you. A coloured edge only when the card needs you:
  turquoise (asks you), magenta (your review), red (broke and waits).
- **UX-22 · One box for new work** (agreed 2026-11-07). Hand-written tickets stay. ⌘N, a click on the box or the
  toolbar's New ticket opens a single input docked at the bottom of the board. ↵ creates the ticket exactly as typed
  (first line = title, the rest = description), with no agent and no cost; ⌘↵ creates and starts it. "Ask Noyan to
  shape it" (⇧↵) hands the text to Noyan, which reads the code and writes one or more tickets to Backlog. The full form
  stays behind "More options…" for priority, waits-for, attachments and a ticket's own agents. The box replaces both
  quick create and the Ask bar; ⌘L still opens Noyan's panel.

Drawn on "Otaq — New UX" (light and dark): Onboarding (Welcome, Tools, Project) · Board (first ticket, an ordinary
day, every card state, Edit flow, List and what waits for what) · Ticket (first review, Overview while it works,
Session / Changes / History / Send back, Plan and helpers, delivery, ⋯ and editing) · New ticket (one box, typing, Ask
Noyan, full form, own agents) · Noyan (from the box, panel, question) · Settings (Agents, an agent, Limits & usage,
Notifications, Data, Appearance, Keyboard, About; the project's one page) · Files (view, which copy) · Window (Running,
Pause, switching projects, limit, paused, Terminal, search, command palette) · Restarts & upkeep (after a restart,
Cleanup, menu bar, quitting, updates).

## Gap review 2 (agreed 2026-11-07, from GAP-REPORT.md R1–R5)

These refine UX-10 and UX-13 and replace A35's "Deliver as" as drawn, A44's GitHub line, A45 and part of A3.

- **UX-23 · Otaq doesn't watch PRs (for now).** An open PR card stays in Review as "PR #481 open"; the user clicks
  Mark merged or Mark closed, as today. No polling of GitHub / Azure DevOps, no "moves to Done when merged", no
  "2 comments on GitHub" line. Refines UX-13.
- **UX-24 · Delivery problems go back to the agent on their own.** A conflict or a failed merge check merges nothing
  and sends the ticket back to the nearest earlier agent stage, whose session fixes it (no click), as today. After the
  third in a row it stops there and waits for the user ("Keeps conflicting"). Refines UX-13; replaces A45's callouts
  with "Ask Strong to fix" / "Send back to Dev".
- **UX-25 · The flow file says everything, delivery included.** Delivery is not a stage (UX-13 holds), but it is a
  setting of the flow, in the flow file: local merge, PR or either; on Approve or automatic. Edit flow shows it, and the
  project's Repository settings show the same flow setting (changing it there changes the flow). Presets set it (Solo
  autopilot = automatic). Refines UX-10 and UX-13.
- **UX-26 · A stage keeps its skill.** The stage panel has the skill, as agreed in no-skill-copies: Pick a skill…
  (the user's own skills and Otaq's) with free text, Ask the agent to write it, and a warning for a skill nobody has.
  The design must draw it. A3's "Instructions" field is open (see DESIGN-AGENT-BRIEF.md).
- **UX-27 · Out of the redesign for now:** the live "what it's doing" line ("writing tests", "editing 4 files") and
  answering permission prompts in the app (Deny / Allow once). Cards and Overview use the statuses the app has
  ("Working"); a permission prompt stays "Answer in terminal".
- **UX-28 · "Ready for your review" can notify, off by default (R6).** A new notification kind in Settings →
  Notifications; it starts off. Review already shows in the Needs you strip and the rail dot. Everything else that
  notifies stays blocking-only. Refines A33.
- **UX-29 · "Waits for" can be set when creating a ticket (R7).** The full form (More options…) has a Waits for
  field with search; it stays on Overview too. Replaces the rule that dependencies are set only in the sheet (DPV-04).
  Agrees with A26 and A42.
- **UX-30 · Files shows committed code only (R8).** The Showing picker lists the base branch, each ticket's copy and
  your own worktrees, each as committed (git HEAD), as today. No "your folder, as it is on disk": uncommitted and
  untracked files are your editor's job. Your main folder appears as its checked-out branch, committed. Refines A36.
- **UX-31 · A stage can be skipped only where the flow file says so (R9).** Today's behaviour stays: a stage's
  `options:` in the flow YAML lists `skip` when a ticket may skip it, so the file tells the user (and Noyan) which
  stages are skippable. The ticket form's per-stage field offers "Skip this stage" only on those stages; the card shows
  one line when set ("Skips Product spec"). Only the user sets a skip. Same as one-check-agent O2 (branch
  `no-skill-copies`); the agent field follows its O1 (any agent for any stage). Refines UX-11. Noyan's N4 stays as agreed: Noyan knows
  from the flow which stages are skippable and passes on a skip the user asked for (and says when a stage can't be
  skipped), but never suggests one itself.
- **UX-32 · A later agent can ask to undo a skip.** When the user skipped stages (for example all the spec stages),
  any agent working the ticket in a later stage (a work agent or a checker; nothing is special to Dev or to spec
  columns) that finds the work needs a skipped stage after all **asks the user**, with its reason: "This needs Product
  spec after all: the payment rules are unclear. Go back, or carry on?" The card turns turquoise (Needs you) until the
  user answers. On "go back", the stage is turned back on and the ticket returns to it; History records the agent's
  reason and the user's answer. The agent never moves the ticket back on its own: only the user skips, so only the user
  undoes a skip. Fits N12. New behaviour: today no agent can move a ticket back to an earlier stage.
- **UX-33 · List rows keep the board's order (R10).** Inside each stage group, rows are in the same order as the
  stage's cards on the board; answering a ticket changes its row's colour and words, never its place. What needs you
  stands out by its coloured edge; the Needs you strip and ⌘J take you there in order. Refines A40 (stage groups in
  flow order stay).
- **R11 · gh / az: left to the designer.** They stay out of onboarding (fewer things for a new user). Roadmap: GitHub,
  Azure DevOps, GitLab and other providers become installable plugins, so where provider tools are installed, signed
  in and shown is for the design agent to propose with that in mind (see DESIGN-AGENT-BRIEF.md). Machine-wide installs
  and sign-ins follow UX-10 (app settings = you and your machine).
- **UX-34 · Usage is always "% left" (R12).** Every place that shows a tool's plan window (title-bar chip, Running
  popover, Settings → Limits & usage, limit banner) says what is left, "15% left · resets 3:45 pm"; amber at 20% left
  or less. Refines A32 and A37.
- **R5 (applying a flow change, rounds) is open:** the gap is explained to the design agent in DESIGN-AGENT-BRIEF.md.

## Gap review 3 (agreed 2026-11-07, from QUESTIONS.md)

- **UX-35 · Usage-data consent (Q1)** is asked once, as a quiet one-time banner after the first ticket is approved;
  the toggle lives in Settings → Data.
- **UX-36 · Claude Code's permission-mode warning (Q2)** leaves the Tools step and becomes a one-time hint at the first
  permission prompt (UX-14).
- **UX-37 · Install and Sign in… run in a terminal under the tool's row, as today (Q4).** Sign-ins vary (passwords,
  device codes, single sign-on) and Otaq can't handle every case, so the user answers them in the terminal.
- **UX-38 · Board options (Q5):** Group by Status / Agent and Dependencies "Always" go; Fold empty columns stays.
- **UX-39 · Dependencies are added from the waiting ticket only (Q6):** "Waits for". Adding from the parent's side
  ("Blocks") goes; Overview keeps "Waiting for it" as a read-only list.
- **UX-40 · Search does both (Q7):** typing filters the board in place as today, and a popover lists matching tickets
  (Done included); ↵ opens one. New filters `in:<stage>` and `by:<agent>`.
- **UX-41 · Import and Export a flow stay (Q8).** Essential: teams and people share flows as files (the flow file
  says everything). Import under Edit flow's Start from…, Export in Edit flow and on the project's Flow card.
- **UX-42 · Local merge keeps its confirmation, without blocking the user (Q11).** The user can still edit the commit
  messages before merging. Today the user waits while an agent prepares the messages; the design must find a way that
  doesn't block (left to the designer).
- **UX-43 · A comment reaches the running agent only when the user says so (Q12).** Today's "Also tell the agent now"
  stays; how it is drawn is left to the designer.
- **UX-44 · An early conflict is handled like a delivery conflict (Q13).** When main moves and a ticket waiting in
  Review would conflict, it already goes back to the nearest earlier agent stage on its own (no click), like UX-24. So
  the card gets no extra mark: it is simply back in that stage, its status line saying it is fixing a conflict with
  main. The details are on Overview. Only the third conflict in a row makes it Needs you.
- **UX-45 · History keeps a quiet row per agent session (Q14)** with its tool session id, copyable, for resuming a
  session by hand.
- **UX-46 · A ticket keeps its base branch; External ID goes (Q15).** Base branch stays in More options… and Edit
  ticket, folded. External ID is only printed for the agent in `prime`; nothing else uses it, so it is dropped.
- **UX-47 · Noyan about a ticket, and continuing (Q16):** conversations continue from the panel's switcher (⌘L); the
  ticket's ⋯ menu gets "Ask Noyan about this".
- **UX-48 · The box for new work is on Board and List only (Q17).** In Files, ⌘N stays "New ticket from this file".
- **UX-49 · The limit banner keeps Resume <tool> (Q18),** the user's click once the reset time has passed. No fallback.
- **UX-50 · The agent limit stays optional, off by default (Q19).** Limits are a "later" concept (step 1).
- **UX-51 · Every stage is its own column (Q9).** `column: with-previous` goes. Its only shipped use was putting the
  Delivery stage in the Review/Done column, which UX-13 removes; one-check-agent's split checks get their own columns.
  Old files that say it still load, each stage in its own column. Matches UX-1.
- **UX-52 · No environment variables read from env files (Q20).** Workspace lists only the variables the user sets in
  Otaq; "Files to copy" names `.env.local` and the like. Otaq copies those files and never reads or shows their values.
- **Q3 · How a project's flow is chosen and designed: an open gap for the designer** (DESIGN-AGENT-BRIEF.md). Not
  decided. Constraints from the maintainer: the flow is the heart of the product, but its most complicated part must
  not be introduced at install; since it is complicated, designing your own flow must feel proper and fun, never a
  boring form.
- **UX-53 · Every stage says what it is for, and every agent reads it (Q10).** The stage's `why` in the flow file
  becomes a plain description of the stage's purpose and when it matters (for example Tech spec: "Settles how it's
  built: needed when the change touches several parts or its approach isn't obvious"). Noyan and every agent working a
  ticket read every stage's description (through `prime`), so they know what each stage of the flow does. Today it is
  an editor-only note nothing reads. Skipping: see UX-54. Sending back to an earlier stage: UX-55.
- **UX-54 · No skip mechanism for agents: a stage's agent that has nothing to do just finishes its stage.** (Agreed
  2026-11-07, replacing a same-day draft that let a stage's own agent "skip" its stage.) A stage's agent can always
  end its stage at once, with a short note ("Nothing to settle: README-only change"), using the stage's description
  (UX-53) to judge; that is an ordinary finish, not a skip, and needs no new feature. A skip (`options: [skip]`) stays
  the user's alone: it saves starting the stage at all. N12, one-check-agent O2 and UX-31 stand unchanged; Noyan never
  skips (N4). Skills may say plainly that finishing at once is fine when the stage has nothing to add.
- **UX-55 · An agent that finds a problem belonging to an earlier stage asks the user to send it back there.** Any
  agent working a ticket (a work agent or a checker) that finds, from the stages' descriptions (UX-53), that a problem
  belongs to an earlier stage (skipped or already run) asks the user, naming the stage and why: "Product gap: refunds
  over 30 days aren't specified. Send back to Product spec, or answer here?" The user answers the gap in place, or sends
  the ticket back; then that stage runs (turned back on if it was skipped) and History records the agent's reason and
  the user's answer. The agent never moves a ticket back on its own. Generalises UX-32, which it includes. New
  behaviour: today no agent can move a ticket to an earlier stage. Checks' "fails go to" and the conflict hand-back
  (UX-24, UX-44) are unchanged.


## Build decisions (agreed 2026-11-07, while building, from the revised canvas)

- **UX-56 · The merge message is prepared when the ticket reaches Review (UX-42, as the canvas draws it).** When a ticket
  arrives in Review on a flow that can deliver by local merge, the agent named in the flow's `delivery:` writes the
  squash commit message (and, for a PR, its title and description) while the ticket waits. The flow file says so: the
  delivery's agent prepares the message, for a local merge as for a PR, so it is no hidden rule. Approve & merge opens
  the confirmation at once; if the message isn't ready, it holds a plain one from the title, "<agent> is still writing
  the message. Merge now with this one, edit it, or wait: it fills in when ready unless you've typed." The merge check
  still runs first. Accepted cost: tokens on every arrival in Review, also for tickets then sent back.
- **UX-57 · Git and PR providers have their own app-settings page, "Git & pull requests" (brief 3b, as drawn;
  refines UX-10, which now has eight topics).** It shows git (version, path, Re-check), then one row per PR provider
  with its tool and sign-in state ("GitHub · uses gh 2.62 · Signed in as nt"); Install and Sign in… run in a terminal
  under the row, as for agent tools. Until providers become plugins, the rows are today's built-in GitHub and Azure
  DevOps. The project's Repository card shows one line, "Pull requests through: GitHub · otaq-io/shop-web", with
  "Set up…" when that provider isn't ready on this machine; the PR dialog offers "Set up GitHub…" (and "Or merge into
  main instead" when the flow delivers either) and keeps Push & open PR disabled until it is. The provider is a fact of
  the repository, not of the flow file. Otaq still never watches PRs.
- **UX-58 · "Tell <agent> now" is a second button (UX-43, as drawn).** The comment box has two buttons: Comment (the
  next stage reads it) and Tell <agent> now (the running agent also gets it while it works), with the helper line
  "Comment: the next stage reads it. Tell Strong now: Strong also gets it while it works." The second button shows only
  while an agent works the ticket; the message reaches it when it is idle, as Otaq's notes do today. A sent comment is
  tagged "· told <agent> now". Replaces the "Also tell the agent now" tick box.
- **UX-59 · A stage has one text field, "What it's for" (2c, as drawn).** The stage panel has no separate
  Instructions field: the description (UX-53) says what the stage settles and when it matters; the skill and the
  project's Notes for agents carry the how.
- **UX-60 · A saved flow change reaches moving tickets as today (R5, as drawn).** Adding or editing a stage applies at
  once ("tickets that haven't reached it will go through it; tickets past it finish as they are"). Removing,
  reordering or changing a stage's kind is saved but waits until no ticket is in the affected stages: they show
  "changing", a held card says "Waiting for the flow change", and the bar offers Cancel change and Apply now…, which
  asks where each affected ticket goes. A Start from… replacement waits for the whole board. Rounds: a −/+ field (1–5,
  default 3, "fails in a row, then it waits for you"); when a check runs out, the card says "<stage> failed 3 times ·
  decide", and Decide offers Allow 1 more round / Send it on to <next stage> / Stop here.
- **UX-61 · How a project gets its flow, and designing one (brief 3a, as drawn; refines UX-14 and UX-16 on when Edit
  flow is offered).** A new user's first project starts on Otaq default. After the first approve, the hint "That was
  your flow" offers Edit flow (not at the first Review). Edit flow turns the board into the editor; the first time, a
  hint says "This is your flow, as columns … Nothing changes until you save." **+** between columns opens a palette of
  ready stages (Security review, Test in the browser, Update the docs, Design review, Product spec, Tech spec, Blank
  stage), each with its description, agent and skill filled in; a template whose skill the user has none of shows the
  usual missing-skill warning. **Watch a ticket** steps a pretend ticket through the draft ("It passes / A check
  fails"), running nothing. Start from… leads with "Suggest a flow for <project>": Noyan's agent drafts one, shown as
  Added / Changed with "Why these changes", Ask for changes… / Discard / Save flow; then the presets and Import.
  Hovering a column header shows the stage's description. Add project (after the first) has a light Flow choice: Otaq
  default (selected) / Same as <project> / Suggest one for <new project> (drafted after the project is added, while
  its board runs on the default).
- **UX-62 · An agent's question: the first line asks, the rest is context (2026-11-12).** Every agent question
  (`otaq ask human`, from any agent) is drawn the same way wherever it shows (a ticket's Needs you, above Noyan's
  terminal, the Needs you strip's popover, and the flow editor's box if it gets one). The question's **first line is
  the question**, one sentence, in bold; **everything after it is context**, in regular weight, with its paragraphs and
  line breaks kept and simple Markdown drawn (code, lists, bold), at a readable line length (about 70 characters, 60
  in the agent panel). Nothing is cut short: in a long question the question line and its context scroll together as
  one text, while the options (with their notes) and Send stay in view; the icon and the option radios sit at the top
  of their text. Where it shows (as drawn, `question-box-1`…`8`): on a ticket's Overview it's first on the page with
  no cap of its own, capping at half the page only when Send would fall below it; above Noyan's terminal it caps at
  55% of the panel, the terminal keeping at least 200 px; the Needs you popover widens to 500 px for a question, caps
  at 560 px and offers Open ticket (or Open in the agent panel). No separate context field: `otaq ask human` stays as it is. The rule for agents
  ("first line: the question in one sentence; then the context; the reason for each choice goes in its option's
  note") is said once, in `otaq ask human --help`, where every agent sees it. In the context, a code block is a
  monospace block that scrolls sideways, a heading is a bold paragraph, and a table shows as plain text.
- **UX-63 · In Edit flow, the board's box asks Noyan (2026-11-12; alternative A of `flow-noyan`; refines UX-61,
  extends UX-22).** Edit flow keeps the board's docked box at its foot: "Ask Noyan to change this flow…" with a
  **Suggest a whole flow** shortcut; ↵ sends the user's words to Noyan's agent on the `flow` skill. Start from… holds
  only the presets and Import. While a proposal shows, the same box reads "Ask for changes to <agent>'s proposal…"
  (the bar's Ask for changes… and its popover go); the bar keeps Watch a ticket, Discard and Save flow. While Noyan
  drafts, the box shows it ("<agent> is drafting a change to <project>'s flow…") with Show (the agent panel) and Stop,
  replacing the line under the bar; sending doesn't open the panel. When Noyan asks, the box becomes the answer box
  (UX-62): it grows with the question up to 55% of the editor's height and scrolls past that; ⤢ opens it up to the
  editor's full height over a scrim, and ⤡, Esc or sending brings it back.
  The `flow` skill puts everything the user needs to decide in the question or in the stage's `why:`, never only in
  its terminal. Box sizes and the expanded view: designer round `DESIGN-AGENT-BRIEF-question-box.md`.
- **UX-64 · A question can take several answers (2026-11-12).** `otaq ask human` gets a flag that lets the user pick
  more than one of its options. The answer box (UX-62, everywhere it shows) draws checkboxes instead of radios; the
  recommended options are marked as today; Other… stays and can be ticked with the others; Send answer needs at least
  one. The agent receives every picked option (and Other…'s text). Any skill can ask that way. Not now: drawing a
  tool's own questions in Otaq (Claude Code's built-in question tool, which can be multi-select, and the other tools'
  equivalents). They stay in the tool's terminal with "Answer in terminal"; that comes later, one tool at a time,
  starting with Claude Code.

# Changes

What changed for you in each release. The maintainer copies a version's section into its GitHub release notes
before Publish (`scripts/README.md`, "Making a release").

## Unreleased

- **Edit flow on the board.** **Edit flow** at the end of the board turns the board into the flow's editor in place:
  each column header is its stage (what happens, its agent and skill, a check's rounds and where its fails go), the
  cards stay where they are, dimmed, and Backlog and Done are fixed at the ends. Drag a header to reorder it, × to
  remove it, and **+** between two columns adds a ready stage (Security review, Test in the browser, Update the docs,
  Design review, Product spec, Tech spec, or a blank one), its description, agent and skill filled in. A click opens
  the stage: Name, What happens, **What it's for** (which every agent reads), Agent, Skill, Fails go to, Rounds (−/+,
  1–5) and whether tickets may skip it; clicking Done sets how work leaves the flow. **Watch a ticket** walks a
  pretend ticket through your draft, passing or failing a check, running nothing; **Start from…** offers **Suggest a
  flow for <project>** (Noyan's agent drafts one, its changes marked with why, and you save it, ask for changes or
  discard it), the shipped flows, another project's and Import; ⋯ has Export. Nothing changes until **Save flow**.
  The flow editor window is gone.
- **A saved change says where it waits.** Adding or editing a stage applies at once; removing, reordering or
  changing a stage's kind waits until no ticket is in the stages it touches, which say **changing**, and the bar
  reads **Saved · waiting** with **Cancel change** and **Apply now…**.
- **Decide on a check out of rounds.** A card that says "Validate failed 3 times · decide" offers **Allow 1 more
  round**, **Send it on to <next stage>** or **Stop here** right on the board.
- **The ticket's History reads as a story.** Newest first, in plain sentences with who and when under each, under
  the filters **All · Questions · Checks · Sessions**. Each agent session is one quiet row, "Session · Dev · Strong",
  with the start of its session ID, a button that copies all of it (to resume that session by hand) and its times.
- **Session's switch names each side by what it is:** Agent · Validate · Run · Terminal (the check's stage, not its
  agent). The Changes tab counts "2 files".
- **Tell the running agent only when you mean to.** While an agent works the ticket, the comment box has two
  buttons: **Comment** (the next stage reads it) and **Tell Strong now** (Strong also gets it, once it's idle). A
  comment sent that way is tagged "· told Strong now". It replaces the "Also tell the agent now" tick box; comments
  keep their attachments as before.
- **Ask Noyan about a ticket from its ⋯ menu.** **Ask Noyan about this** opens Noyan's panel on a new conversation
  addressed to the ticket; it starts when you send. The menu no longer has Copy attach command (it's on Session's
  strip), and Edit ticket… says over the form that the working agent reads your edit the next time it's idle.
- **One box for new work.** "New ticket, or ask Noyan…" sits at the bottom of the board and the List; ⌘N, a click
  or the toolbar's New ticket opens it. The first line is the title and the rest the description. **↵** creates the
  ticket in Backlog exactly as typed, with no agent and no cost; **⌘↵** creates and starts it; **⇧↵** (Ask Noyan to
  shape it) hands the text to Noyan, which reads the code and writes one or more tickets to Backlog for you to start.
  **⌥↵** (Alt+↵) starts a new line, and **More options…** opens the full form with what you typed. It replaces quick
  create and the Ask bar; ⌘L still opens Noyan's panel, and in Files ⌘N is still New ticket from this file.
- **A project's settings are its own page.** The view switch's **Settings** (after Board, List and Files) shows the
  project's settings in its main area, one page with a section list on the left: **Flow** (its stages from Backlog to
  Done, who works in each, **Export** and **Edit flow**), **Repository** (name, base branch, how work is delivered,
  which pull request provider the repository uses, merge check), **Run**, **Workspace** (setup command, your
  variables, files to copy, ticket resources, warm build), **Notes for agents** and **Remove project**. Projects are
  no longer listed in the app's Settings (⌘,), which keeps what's about you and your machine.
- **Delivery is changed where you see it.** The Repository card's **Delivery** (Merge into main · Pull request ·
  Either; On Approve · Automatically) is the flow's own setting: changing it there saves a new version of the flow,
  as Edit flow does. Its line "Pull requests through: GitHub · owner/repo" says which provider the repository uses
  and, when it isn't ready on this computer, offers **Set up…**.
- **After a restart, one dialog.** When the background service started again and stopped agents that were working,
  Otaq opens with "3 agents were stopped by a restart": each stopped ticket and conversation with Noyan, ticked, and
  **Resume 3**. One whose tool can't run now (signed out, at its limit) says why and stays unticked; it is never
  resumed on another tool. Each row then says whether it resumed, or offers **Retry** and **Start from scratch**.
  **Not now** leaves them in Needs you, red with Resume. The "sessions were interrupted" banner is gone.
- **The paused and limit banners have a ×.** Closing one hides it until something changes: a new pause, a new
  limit, or the limit resetting. Resume stays in the command palette, and a limit's Resume in the usage chip's
  popover.
- **The menu-bar menu reads simpler.** NEEDS YOU first, then how many agents run in which project, then your checks
  and stopped tickets; **Open Otaq** ⌘O, **Pause all**, **Restart to update** (the app only: working agents
  continue) and **Quit Otaq** ⌘Q, which asks while agents work: **Keep them running** or **Quit and stop agents**.
- **Settings → Data's Cleanup row says how much it would free** ("3 leftovers · 412 MB"); **Review…** opens Cleanup
  over Settings.
- **Reviewing a ticket.** A ticket in Review opens on Overview: what the agent did, what the checker found, the
  decisions it made, then the ticket. The bar under every tab names Approve by what it does ("Approve & merge into
  main", "Approve & open PR"); the button beside it holds the other ways to finish (open a PR instead, Merge into
  another ticket…, Done, nothing to merge, Done, don't merge).
- **Nobody waits for the merge message.** When a ticket reaches Review, the agent the flow's `delivery:` names writes
  its message while it waits: the squash commit's for a merge, the branch, title and description for a PR (`otaq pr
  message`). Approve & merge opens its confirmation at once; if the message isn't there yet it starts from a plain
  one and fills in when it arrives, unless you typed. Approve & open PR opens the PR dialog at once the same way, and
  the Open PR skill only runs after you confirm. A local merge may name a delivery agent too; without one, the
  confirmation starts from the plain message. It is asked for again only when the ticket comes back with new commits.
- **When GitHub or Azure DevOps isn't set up on this machine**, the PR dialog says so with **Set up…** and, when the
  flow delivers either way, **Or merge into main instead**; Push & open PR stays off until it is. An open PR's box
  has **Mark closed** and **Mark merged**.
- **The command palette starts with what needs you.** Opened empty (⌘K), its first row is **Next that needs you**
  with the ticket it goes to: the same one ⌘J opens, in the Needs you strip's order. Below it: New ticket, Edit flow,
  Ask Noyan, Pause this project, Settings and Open a terminal, then your recent items.
- **Search's ticket list reads at a glance.** Each matching ticket shows its status in a few words after a coloured
  dot ("Your review", "Waits for SHOP-27", "Done"), and the list names the filters you can type under it
  (`is:needs-you · is:working · in:review · by:strong`). The field says "Search tickets", and shows esc while you type.
- **Files shows your folder, as committed.** The Files view's picker now reads "Show files from": your base branch,
  then each ticket's copy, then "Your worktrees · committed only", where your project folder comes first as its
  checked-out branch ("Your folder · main"), then your other worktrees. Each shows what is committed there; an edit
  you haven't committed never appears in Files (your editor shows those).
- **Search lists the tickets it finds.** Typing in the board's search (`/`) still filters the board, and the list
  under the field now also shows the matching tickets with their status, Done ones included; ↵ or a
  click opens one. Two new filters: `in:<stage>` keeps one column's tickets (`in:review`, `in:backlog`, `in:done`;
  the stage's name or id, any case) and `by:<agent>` the tickets whose current stage that agent works (`by:strong`).
- **Starting a ticket puts it into the first stage.** The Ready for Dev column is gone. Every Backlog card has
  **Start** (a drag into the first stage and ⌘D do the same). A ticket that can't begin yet waits at the top of that
  column and says why: "Waits for SHOP-12 to merge", "Waits for a free agent · 2nd in line", "Waiting for the flow
  change". It begins by itself when it can, and nothing is spent until then. A ticket waiting for a dependency can be
  started too; it no longer asks you to remove the dependency first. Tickets that were in Ready for Dev wait at the
  top of their first stage.
- **Create & start.** The New ticket form has **Create & start** next to **Create** (⇧⌘↵, Ctrl+Shift+↵ on Windows
  and Linux): the ticket is created with what it waits for and its stage choices, then starts at once, so one that
  waits for SHOP-12 waits at the top of the first stage. If it can't start, it stays in Backlog and the form says why.
- **Tickets have no External ID.** The field is gone from New ticket, Edit ticket, the ticket's details line, the
  PR dialog, `otaq show` and `prime`; History no longer lists changes to it. Your tickets keep everything else.
- **Your skills stay where they are.** Otaq no longer copies or imports skills. Each tool loads your skills the
  way it always does, and Otaq's own skills come with its plugin. A stage runs only on a tool that has its skill:
  otherwise its card waits in Needs you with "<tool> doesn't have the skill `/<name>`", **Check again** and
  **Change agent…**.
- **No fallback agents.** A tool at its limit, logged out, not installed, hidden or needing an update, or an agent
  whose model is no longer listed, makes its agents wait; the card says why. Settings → Agents' fallback per tool
  and the card's **Use fallback** are gone. Pick another agent for a ticket's stage with **Change agent…**.
- **A check stage has one agent.** A flow whose check listed several agents now reads as one check stage per agent
  ("Validate · Security reviewer"), in the same column. **Checks that ran side by side now run in turn**, one after
  the other, each with its own rounds. Flow files name a check's agent with `agent:`.
- **Any agent for any stage, for one ticket.** For a stage that hasn't started you can pick any agent whose tool has
  the stage's skill. Only you pick (or Noyan, passing on your words); a ticket's own agent no longer does.
- **Nothing is skipped on its own.** A check of yours is no longer skipped because the stages before it were; its
  choice "required" just means the check happens, as the flow says. **Skip this check** and **Move on anyway** show only on checks the flow marks skippable.
  A skipped or moved-past check no longer turns an automatic Delivery into a click: it delivers as the flow says,
  marked "Delivered without all checks".
- **Noyan uses one agent, set in Settings → Agents.** "Noyan uses" picks the agent every new conversation with
  Noyan starts on, and a new conversation can pick another agent for itself in the panel before you send. Agents no
  longer have a role: each stage names its agent, and the agent's page has no Role field. The agent that held the
  Noyan role is the one Noyan uses after the update. A conversation whose tool can't run waits for it: the panel no
  longer offers "New agent on …" in its place.
- **A stage's skill can't be a tool's built-in command.** A flow whose stage names one, such as Claude Code's
  `/security-review`, is refused instead of applying and then holding the stage. Write a small skill that runs the
  command and name that skill.

---
name: flow
description: Change an Otaq project's board flow as Noyan - read the current flow with `otaq flow show`, write the whole flow file (agents and stages, every field), give each stage a description of what it settles and when it matters, put the reason for each stage you add or change (with its source) in the proposal's `why:`, propose it with `otaq flow propose --stdin`, then end your turn and wait for the user's Apply, Cancel or Ask for more changes as your next message.
disable-model-invocation: true
---

# Change the project's flow

You are **Noyan** of an Otaq project, and the user wants its **flow** changed: the stages a ticket walks
between Backlog and Done, and the agents that run them. They started you with this skill (from the flow
editor (**Edit flow** → **Start from…** → **Suggest a flow for <project>**), or by typing `/flow`), they asked your agent tool for a
new project's first flow (see "A first flow"), or they just asked ("add a Security check after Validate").

**You never change the live flow.** You write the whole flow file and **propose** it; it lands in the user's flow
editor as the draft, each stage you changed marked, and nothing changes until they click **Apply**.

## Steps

1. **Get the context first.** Run `otaq prime --project` and read all of it, then **read the current flow**:

   ```sh
   otaq flow show
   ```

   It prints the project's applied flow and the agents it names, as the file `propose` reads (`--json` prints the
   same as JSON). This is your starting point: change it, don't start from nothing.
2. **Follow Noyan’s instructions**, which `prime --project` prints. They add to these steps; they never
   remove proposing before anything changes (step 5) or waiting for the answer (step 6).
3. **Understand the request.** If it leaves something open that changes the flow (which agent, which model, what a
   check looks for), ask the user one question at a time with `otaq ask human --stdin` (2–4 options with notes when
   there are clear alternatives), then end your turn and wait for the answer. Don't ask what the request already
   says. To start from a ready-made flow, `otaq flow presets` lists the ones Otaq ships, with one line each.
4. **Write the whole file**: every agent the stages name, and every stage, in the format below. Keep the stages and
   agents you aren't asked to change exactly as `flow show` printed them, and keep each stage's `id`: Otaq matches
   stages by id, name or position, so renaming Review to QA stays a rename.
5. **Propose it**, with a `description:` on each stage and a `why:` for each stage you add or change (see
   "Descriptions" below), on stdin:

   ```sh
   otaq flow propose --stdin <<'FLOW'
   agents:
     - name: Security reviewer
       instructions: |
         Never edit files.
         Fail on unescaped SQL.
       model: claude:opus
       why: The user asked for a security check on opus in this conversation.
   flow:
     - id: dev
       name: Dev
       kind: agent
       agent: Developer
       skill: work-ticket
       description: "Builds what the ticket asks for, with its tests, and commits it: every ticket that changes the code needs it."
     - id: security
       name: Security
       kind: check
       by: agent
       agent: Security reviewer
       skill: team-security-review
       description: "Checks the change for unescaped SQL and leaked secrets: matters when it touches queries, auth or config."
     …
   why:
     security: The user asked for a security check in this conversation.
   FLOW
   ```

   (`otaq flow propose -f flow.yaml` reads a file instead.) Otaq checks everything first: the rules of a flow (below),
   the agents' names and models. **If it refuses**, it names the stage or agent and the reason (for
   example "Review is a check by you before any Agent stage: a Fail would have nowhere to go"): fix exactly that and
   propose again. Don't tell the user about a refusal you can fix yourself.
6. **End your turn and wait.** A valid proposal returns at once ("Proposal 3 is waiting for the user") and the flow
   editor beside you shows it. Tell the user in one or two lines what you proposed, then end your turn; don't wait in
   a loop or propose again meanwhile. **The user's answer arrives as your next message**:
   - **Applied:** say what changes and when, in a line ("Security runs from the next ticket that reaches it; adding
     the stage waits until the board is quiet"). You're done.
   - **Cancelled:** acknowledge it in a line. Don't propose again unless the user asks.
   - **Ask for more changes**, with the user's text ("use sonnet for Security instead"): change the file, propose it
     again (step 5), and wait again.

## A first flow

When the first message says it is the project's **first flow** ("This is billing-api's first flow. Look at how the
user works with Claude Code: …"), the user asked you, through the agent tool you run in, for a new project's flow:
there is no request yet, so you **look at how the user works and propose a whole flow** from it. The user may ask
other agent tools too; each proposes its own flow, labelled with its tool, and the user picks one. Look at exactly
two things:

- **Your own tool's history on this Mac**: what the user typed into the agent tool you run in (Claude Code, Codex,
  Copilot, OpenCode…), in every project. Where your tool keeps it is for you to find; Otaq doesn't read it for you.
  Read **only what the user typed** (their messages and slash commands, with the project and time when the history
  has them). **Never** read login, token or credential data, and **never another tool's data**: if you run in
  Claude Code, don't open Codex's or OpenCode's folders, and the other way round. Look for the slash commands and
  skills they run often, and where (`/security-review`, `/commit`, a review or test skill), and what they ask for
  before and after code ("write the tests first", "check the migration").
- **This repo**, with its AI files: `AGENTS.md`, `CLAUDE.md`, `.claude/`, `.cursor/rules` and the like (the rules
  and skills the project gives agents); `git log` (how commits are written, who commits), branch names, CI files
  (`.github/workflows/`, `.gitlab-ci.yml`), PR templates and `CONTRIBUTING.md` (what a change needs before it
  merges).

**Nothing else.** Not GitHub or GitLab (no `gh`, no other host's CLI, no website), not the skills, plugins or
instruction files in the home folder, not other repos on this Mac.

Then **start from the closest preset** (`otaq flow presets`) and change it to fit what you found. Each stage you add
or change gets a reason in the proposal's `why:` that names its source **and your tool**, with a count you actually
found: "From 41 /security-review runs in billing-api (Claude Code)", "`CONTRIBUTING.md` in this repo asks for a
review before merging (Claude Code)". With nothing behind a stage, its reason says "No evidence found" (see
"Descriptions"); never guess a count. The stage's `description:` still says only what it settles. Don't add `skip` options,
or an automatic delivery with no check before it, unless the user asks (see "What you add only when the user asks").
Then propose it with `otaq flow propose --stdin` and **end your turn** (steps 5 and 6): the user picks **Use this
flow**, **Ask for changes** or **Use the default**, and their answer is your next message (if they pick another
tool's flow, it is "The user chose another flow."; you're done).

## Writing a stage's skill

When the user asks you to **write a skill for a stage** ("Write a skill for the Plan stage: …": the editor's **Ask
the agent to write it** starts the line, the user finishes it), you write it in the one folder Otaq gives you, inside
this project:

1. **Name it and get the folder**: `otaq flow write-skill <stage> --name <name>` (the stage's id or name, and a short
   name for the skill in lowercase letters, digits and hyphens, e.g. `otaq flow write-skill plan --name plan-ticket`).
   It prints the only folder you may write in, the skills folder the stage's agent's tool reads; the editor shows the
   stage's skill as being written. If the name is taken or is one of Otaq's own skills, it refuses: pick another.
2. **Read first**: `otaq prime --project`, the repo's `AGENTS.md` / `CLAUDE.md`, and the skills the flow's other
   stages run (`otaq flow show` names them), so the new one fits how this project works. Ask the user one question
   at a time with `otaq ask human --stdin` only when the request leaves out something you can't find.
3. **Write the skill** in that folder: a `SKILL.md` whose front matter has `name:` (the name you gave) and
   `description:` (one line saying what the stage does; the editor shows it as the stage's caption), then the steps
   an agent follows in that stage, and any files it mentions. Write nothing anywhere else.
4. **Hand it in**: `otaq flow write-skill <stage> --done`. Otaq checks the folder and puts the skill in the stage's
   field, where the user can open it or undo it. If it refuses, fix exactly what it names and hand it in again. Don't
   propose the flow for this; the skill applies with the flow when the user clicks Apply. Never commit it: the user
   commits it, so agents get it in their worktrees.
5. Tell the user in a line what the skill does, that it is saved in this project and needs committing, then **end
   your turn**.

## The file

Three parts: `agents:` (who runs the stages), `flow:` (the stages between Backlog and Done) and `delivery:` (how the
work leaves the flow after the last stage: a setting of the flow, not a stage). Backlog and Done are the fixed ends
of every flow, so never write them. Every stage is its own column. Starting a ticket puts it into the first stage; one
that can't begin yet (a dependency, no free agent) waits at the top of that column.

### Agents

```yaml
agents:
  - name: Rust developer           # unique; stages and options name it
    instructions: |                # optional: this agent's own rules, given at every start
      Follow our error-handling rules: no unwrap in library code.
    model: default                 # default · claude · claude:opus · codex:<model> · copilot · opencode:<model>
    why: From 41 /commit-compact runs in shop-web (Claude Code).   # optional; see "Descriptions"
```

- **`name`**: what the board and Settings show. Agents belong to the account and are shared by every project: an
  agent that already exists (in `flow show`, or one the user has) is reused by its name. Changing a shared agent
  changes it everywhere it is used, and the flow editor lists those projects; to change it for this project only, add a
  new agent with its own name.
- **`role`** (rare): `noyan`. Exactly one agent holds it; leave it out unless the user
  asks to move a role.
- **`instructions`**: Markdown, optional.
- **`why`**: optional, one line on why the agent is there, naming its source; the flow editor shows it. Otaq doesn't
  act on it.
- **`model`**: `default` (the account's default model; prefer it unless the user names one), a tool (`claude`,
  `codex`, `copilot`, `opencode`: that tool's default model) or `tool:model` (`claude:opus`, `claude:haiku`).

### Stages

```yaml
flow:
  - id: product-spec             # optional; keep the id flow show printed, never invent one starting with _
    name: Product spec           # unique; the column's title
    kind: agent                  # agent · check
    agent: Spec writer           # who runs it
    skill: product-spec          # the skill it runs first
    options: [skip]              # optional: a ticket may skip it
    description: "Settles what to build and how it behaves, with the user, before any code: needed for a new feature or a change to what the product does."
```

| Field | Kinds | What it says |
|---|---|---|
| `id` | every stage | Its permanent id. Keep the ones `flow show` printed; a new stage gets one from its name. |
| `name` | every stage | Its name on the board ("Security"). Unique. |
| `kind` | every stage | `agent`: an agent works on the ticket. `check`: an agent or the user passes or fails it. |
| `by` | check | `agent` (an agent check: read-only, ends with a verdict) or `you` (the user's check: Pass or Fail). |
| `agent` | agent, agent check | The agent that runs it, by name: one. A check has one agent; two checks are two check stages, one after the other. A check by you has none. |
| `skill` | agent, agent check | The skill it runs first (`work-ticket`, `validate`, `open-pr`…). Never a tool's built-in command such as `/security-review`, `/review` or `/init` (Otaq refuses it): to use one, write a small skill that runs it and name that skill. |
| `options` | agent, check | `[skip]` when a ticket may skip the stage; nothing else (a stage lists no agents: the user may pick any agent whose tool has the stage's skill for one ticket, and agent names written here are ignored). Default: none. |
| `rounds` | agent check | How many fails in a row (1–5) before the card stops for the user. |
| `fail_to` | check | Where a Fail goes back to: an Agent stage before the check, by name (`fail_to: Product spec`). Leave it out for the nearest Agent stage before it (the usual). |
| `description` | every stage | What the stage settles and when it matters (below). Every agent working a ticket reads every stage's in `otaq prime`, and so do you in `prime --project`; the flow editor shows it. An older file's `why:` reads as it. |

### Delivery

```yaml
delivery:
  delivers: either               # local · pr · either
  mode: click                    # click · automatic
  agent: Developer               # writes the delivery's message in Review, and opens the PR
  skill: open-pr                 # optional: the skill it opens the PR with
```

| Field | What it says |
|---|---|
| `delivers` | `local` (a merge into the base branch: **Approve & merge into <base>**), `pr` (its agent opens a PR with its skill), or `either` (the user picks at Approve; click mode only). |
| `mode` | `click`: waits for the user, whose Approve on the last stage (a check by you) says what it does ("Approve & merge into main", "Approve & open PR"). `automatic`: merges, or pushes and opens the PR, with no click and no dialog, once the ticket passes its last stage. |
| `agent` | The delivery's agent. When a ticket reaches Review in click mode, it writes the message the delivery uses while the ticket waits: the squash commit message for a merge into the base, and the branch, title and description for a PR, so the user's Approve opens its confirmation at once. It also opens the PR. Required for `pr` and `either`; optional for `local` (left out, the merge confirmation starts from a plain message). Each arrival in Review with new commits costs it one short turn. |
| `skill` | The skill it opens the PR with; Otaq's `open-pr` when left out. |

A ticket being delivered stays in the last stage's column (To deliver, PR open). Otaq doesn't watch PRs: the user marks
an open PR merged or closed. A conflict or a failed merge check goes back to the last Agent stage on its own.

A whole example, the stages and agents of the Spec first preset (`otaq flow presets` lists it): Backlog →
Product spec → Tech spec → Milestone plan → Spec review → Dev → Validate → Code review, delivered on Code review's
Approve. Spec writer, Architect
and Milestone planner settle the work first, and Spec review is the user's check of what they wrote. The shipped
preset also gives the three planning stages and Spec review `options: [skip]`, so ordinary tickets can skip planning:

```yaml
agents:
  - name: Spec writer
    model: default
  - name: Architect
    model: default
  - name: Milestone planner
    model: default
  - name: Developer
    model: default
  - name: Checker
    model: default
flow:
  - id: product-spec
    name: Product spec
    kind: agent
    agent: Spec writer
    skill: product-spec
    description: "Settles what to build and how it behaves, with the user, before any code: needed for a new feature or a change to what the product does; a bug fix or a purely technical change has nothing to settle here."
  - id: tech-spec
    name: Tech spec
    kind: agent
    agent: Architect
    skill: tech-spec
    description: "Settles how it's built: needed when the change touches several parts or its approach isn't obvious."
  - id: milestone-plan
    name: Milestone plan
    kind: agent
    agent: Milestone planner
    skill: milestone-plan
    description: "Splits the work into pieces, in order, in the ticket's Plan: needed when the change is too big to build and review in one go."
  - id: spec-review
    name: Spec review
    kind: check
    by: you
    fail_to: Product spec
    description: "The user reads what the spec stages settled and passes it, or sends it back: their say before any code is written."
  - id: dev
    name: Dev
    kind: agent
    agent: Developer
    skill: work-ticket
    description: "Builds what the specs and the Plan settled, with its tests, and commits it: every ticket that changes the code needs it."
  - id: validate
    name: Validate
    kind: check
    by: agent
    agent: Checker
    skill: validate
    rounds: 3
    fail_to: Dev
    description: "Checks the work against the ticket and its specs and runs the project's checks, so what Dev missed is caught before anyone reviews it."
  - id: code-review
    name: Code review
    kind: check
    by: you
    fail_to: Dev
    description: "The user reads the change and passes it, or sends it back with what to fix: their say before it is delivered."
delivery:
  delivers: either
  mode: click
  agent: Developer
  skill: open-pr
```

### The rules of a flow

`propose` refuses a file that breaks one, naming the stage:

- At least one `agent` stage, and one `delivery:` (a stage of `kind: delivery` is an older file's way of writing it).
- No check by you before the first Agent stage (a Fail must have an Agent stage to go back to).
- `either` only with `mode: click`; a PR delivery names an agent (a merge into the base may); `rounds` from 1 to 5.
- An agent check names one `agent:`. Never list several agents on a check: write one check stage per check.
- A check's `fail_to` names an Agent stage before it.
- With every `skip` taken, an Agent stage must still run, and every check by you that can't be skipped must still
  have one before it.
- Every agent a stage or the delivery names is in `agents:` or already exists.

### What you add only when the user asks

- **`skip`** on any stage, and an **automatic delivery with no check before it**. Otaq allows both; the flow editor warns
  about the second ("Delivery opens a PR on GitHub without any check before it"). Don't add them on your own.
- `skip` is the user's choice per ticket, never Otaq's: nothing is skipped on its own, even a check of yours after
  stages a ticket skipped (a check's other choice, "required", just means it happens). A skipped check, or one the user moved a ticket
  past, doesn't change the delivery: an automatic one still delivers on its own ("Delivered without all checks"), so a
  `skip` on a check before an automatic delivery lets tickets merge without that check. Say so when you add one.
- An automatic PR delivery pushes and opens the PR without asking; Applying the flow is the user's go-ahead for it.

## Descriptions

Every stage has a `description:`: **what the stage settles and when it matters**, in a sentence or two ("Settles how
it's built: needed when the change touches several parts or its approach isn't obvious"). Every agent working a
ticket reads every stage's description, so a stage's agent can tell when its stage has nothing to add for a ticket
(it then finishes at once, with a short note), and any agent can tell which stage a problem belongs to. Write it for
those agents and for the user: plain words, and **only what the stage settles and when it matters**: never where
the idea came from, no counts and no sources. It stays in the flow for as long as the stage does.

Keep the description of a stage you don't change exactly as `flow show` printed it. For each stage **you add or
change**, give your reason in the proposal's top-level `why:`, keyed by the stage's `id` (or its name): one short line
that **names its source**, what you read that led to it, with the numbers you actually counted. The user reads it
under **Why these changes** beside your proposal; it is never stored in the flow. (An agent you add or change carries
its source on its own `why:`.)

```yaml
why:
  security: The user asked for it in this conversation.
  security-review: From 41 /security-review runs in shop-web and api (Claude Code).
  review: "`CONTRIBUTING.md` asks for a review before merging."
```

**With no evidence, say so** ("No evidence found"); never guess a count or invent a source.

## Rules

- **Never edit files, never commit, never push.** You change the flow only by proposing it. The one exception is a
  stage's skill, written only in the folder `otaq flow write-skill` gives you (see "Writing a stage's skill").
- **Propose a whole file** every time, never a part of one. One proposal at a time: a new one replaces the waiting one.
- After proposing, **end your turn**: the answer is your next message.
- **Long text always goes through stdin** (`--stdin`), never quoted arguments.

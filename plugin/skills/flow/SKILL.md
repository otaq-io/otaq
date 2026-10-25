---
name: flow
description: Change an Otaq project's board flow as Noyan - read the current flow with `otaq flow show`, write the whole flow file (agents and stages, every field), give each stage a why line that names its source, propose it with `otaq flow propose --stdin`, then end your turn and wait for the user's Apply, Cancel or Ask for more changes as your next message.
disable-model-invocation: true
---

# Change the project's flow

You are **Noyan** of an Otaq project, and the user wants its **flow** changed: the stages a ticket walks
between Backlog and Done, and the agents that run them. They started you with this skill (by typing in the flow
editor's Noyan panel, opened with **Change flow…**, or by typing `/flow`), they asked your agent tool for a
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
5. **Propose it**, with a `why:` line on each stage (see "Why lines" below), on stdin:

   ```sh
   otaq flow propose --stdin <<'FLOW'
   agents:
     - name: Security reviewer
       instructions: |
         Never edit files.
         Fail on unescaped SQL.
       skills: [security-review]
       model: claude:opus
       why: The user asked for a security check on opus in this conversation.
   flow:
     - id: dev
       name: Dev
       kind: agent
       column: own
       agent: Developer
       skill: work-ticket
       why: Unchanged from the current flow.
     …
   FLOW
   ```

   (`otaq flow propose -f flow.yaml` reads a file instead.) Otaq checks everything first: the rules of a flow (below),
   the agents' names and models, the skills. **If it refuses**, it names the stage or agent and the reason (for
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

Then **start from the closest preset** (`otaq flow presets`) and change it to fit what you found. Every why line
names its source **and your tool**, with a count you actually found: "From 41 /security-review runs in billing-api
(Claude Code)", "`CONTRIBUTING.md` in this repo asks for a review before merging (Claude Code)". With nothing behind
a stage, its why line says "No evidence found" (see "Why lines"); never guess a count. Don't add `skip` options,
or an automatic Delivery with no check before it, unless the user asks (see "What you add only when the user asks").
Then propose it with `otaq flow propose --stdin` and **end your turn** (steps 5 and 6): the user picks **Use this
flow**, **Ask for changes** or **Use the default**, and their answer is your next message (if they pick another
tool's flow, it is "The user chose another flow."; you're done).

## Writing a stage's skill

When the user asks you to **write a skill for a stage** ("Write a skill for the Plan stage: …": the editor's **Ask
the agent to write it** starts the line, the user finishes it), you write it in a folder Otaq gives you,
never in this repo:

1. **Get the folder**: `otaq flow write-skill <stage>` (the stage's id or name, e.g. `otaq flow write-skill plan`). It
   prints the only folder you may write in; the editor shows the stage's skill as being written.
2. **Read first**: `otaq prime --project`, the repo's `AGENTS.md` / `CLAUDE.md`, and the skills the flow's other
   stages run (`otaq flow show` names them), so the new one fits how this project works. Ask the user one question
   at a time with `otaq ask human --stdin` only when the request leaves out something you can't find.
3. **Write the skill** in that folder: a `SKILL.md` whose front matter has `name:` (a short name for the skill, like
   `plan-ticket`) and `description:` (one line saying what the stage does; the editor shows it as the stage's
   caption), then the steps an agent follows in that stage, and any files it mentions. Write nothing anywhere else.
4. **Hand it in**: `otaq flow write-skill <stage> --done`. Otaq checks the folder, keeps its own copy and puts the
   skill in the stage's field, where the user can open it or undo it. If it refuses, fix exactly what it names and
   hand it in again. Don't propose the flow for this; the skill applies with the flow when the user clicks Apply.
5. Tell the user in a line what the skill does, then **end your turn**.

## The file

Two lists: `agents:` (who runs the stages) and `flow:` (the stages between Backlog and Done). Backlog, Ready for Dev
(named Ready when the first Agent stage isn't Dev) and Done are always there, so never write them.

### Agents

```yaml
agents:
  - name: Rust developer           # unique; stages and options name it
    instructions: |                # optional: this agent's own rules, given at every start
      Follow our error-handling rules: no unwrap in library code.
    skills: [work-ticket, commit]  # the skills it has
    model: default                 # default · claude · claude:opus · codex:<model> · copilot · opencode:<model>
    why: From 41 /commit-compact runs in shop-web (Claude Code).   # optional; see "Why lines"
```

- **`name`**: what the board and Settings show. Agents belong to the account and are shared by every project: an
  agent that already exists (in `flow show`, or one the user has) is reused by its name. Changing a shared agent
  changes it everywhere it is used, and the flow editor lists those projects; to change it for this project only, add a
  new agent with its own name.
- **`role`** (rare): `noyan`. Exactly one agent holds it; leave it out unless the user
  asks to move a role.
- **`instructions`**: Markdown, optional.
- **`skills`**: the names of skills Otaq ships (`work-ticket`, `commit`, `validate`, `product-spec`, `open-pr`, `restack`, …)
  or that an agent already has, or the absolute path of a skill folder (holding a `SKILL.md`) to import on Apply,
  for example `/Users/me/.claude/skills/security-review`. Every skill is copied into the worktree as `otaq-<name>`.
- **`why`**: optional, one line on why the agent is there; the flow editor shows it. Otaq doesn't act on it.
- **`model`**: `default` (the account's default model; prefer it unless the user names one), a tool (`claude`,
  `codex`, `copilot`, `opencode`: that tool's default model) or `tool:model` (`claude:opus`, `claude:haiku`).

### Stages

```yaml
flow:
  - id: product-spec             # optional; keep the id flow show printed, never invent one starting with _
    name: Product spec           # unique; the column's title
    kind: agent                  # agent · check · delivery
    column: own                  # own · with-previous
    agent: Spec writer           # who runs it
    skill: product-spec          # the skill it runs first
    options: [skip]              # optional: other agents a ticket may pick instead, or skip
    why: The user asked to settle what to build before any code is written.
```

| Field | Kinds | What it says |
|---|---|---|
| `id` | every stage | Its permanent id. Keep the ones `flow show` printed; a new stage gets one from its name. |
| `name` | every stage | Its name on the board ("Security"). Unique. |
| `kind` | every stage | `agent`: an agent works on the ticket. `check`: an agent or the user passes or fails it. `delivery`: Local merge or PR, always last. |
| `by` | check | `agent` (an agent check: read-only, ends with a verdict) or `you` (the user's check: Pass or Fail). |
| `column` | every stage | `own`: its own column. `with-previous`: its cards show in the column before it. Default `own`. |
| `agent` | agent, PR delivery | The agent that runs it, by name. A check by you and a Local merge have none. |
| `agents` | agent check | Its check agents, by name, one or more: `[Validator]`, `[Security reviewer, Performance reviewer]`. They run side by side, each read-only, each running the stage's skill; the stage passes when every one passes. An agent check never takes `agent:`. |
| `skill` | agent, agent check, PR delivery | The skill it runs first (`work-ticket`, `validate`, `open-pr`…). |
| `options` | agent, check | Other agents a ticket may use instead (`[Quick developer]`), and `skip` when a ticket may skip the stage. Default: none. |
| `rounds` | agent check | How many fails in a row (1–5) before the card stops for the user, whichever check agent failed. |
| `fail_to` | check | Where a Fail goes back to: an Agent stage before the check, by name (`fail_to: Product spec`). Leave it out for the nearest Agent stage before it (the usual). |
| `mode` | delivery | `click`: waits for the user's Local merge or Open PR. `automatic`: merges, or pushes and opens the PR, with no click and no dialog. |
| `why` | every stage | One line on why the stage is there, naming its source (below). The flow editor shows it; Otaq doesn't act on it. |
| `delivers` | delivery | `local` (Local merge, no agent), `pr` (runs its agent and skill), or `either` (the user picks; click mode only). |

A whole example, the stages and agents of the Spec first preset (`otaq flow presets` lists it): Backlog → Ready →
Product spec → Tech spec → Milestone plan → Spec review → Dev → Validate → Code review → Done. Spec writer, Architect
and Milestone planner settle the work first, and Spec review is the user's check of what they wrote. The shipped
preset also gives the three planning stages and Spec review `options: [skip]`, so ordinary tickets can skip planning:

```yaml
agents:
  - name: Spec writer
    skills: [product-spec]
    model: default
  - name: Architect
    skills: [tech-spec]
    model: default
  - name: Milestone planner
    skills: [milestone-plan]
    model: default
  - name: Developer
    skills: [work-ticket, commit]
    model: default
  - name: Validator
    skills: [validate]
    model: default
flow:
  - id: product-spec
    name: Product spec
    kind: agent
    column: own
    agent: Spec writer
    skill: product-spec
  - id: tech-spec
    name: Tech spec
    kind: agent
    column: own
    agent: Architect
    skill: tech-spec
  - id: milestone-plan
    name: Milestone plan
    kind: agent
    column: own
    agent: Milestone planner
    skill: milestone-plan
  - id: spec-review
    name: Spec review
    kind: check
    by: you
    column: own
    fail_to: Product spec
  - id: dev
    name: Dev
    kind: agent
    column: own
    agent: Developer
    skill: work-ticket
  - id: validate
    name: Validate
    kind: check
    by: agent
    column: own
    agents: [Validator]
    skill: validate
    rounds: 3
    fail_to: Dev
  - id: code-review
    name: Code review
    kind: check
    by: you
    column: own
    fail_to: Dev
  - id: deliver
    name: Done
    kind: delivery
    column: with-previous
    agent: Developer
    skill: open-pr
    mode: click
    delivers: either
```

### The rules of a flow

`propose` refuses a file that breaks one, naming the stage:

- At least one `agent` stage; exactly one `delivery` stage, and it is last.
- No check by you before the first Agent stage (a Fail must have an Agent stage to go back to).
- `with-previous` is refused on the first stage, and into a column that belongs to a check by you (only the Delivery
  may share it, as Done shares Review's on the default flow).
- `either` only with `mode: click`; a Delivery can't be skipped; `rounds` from 1 to 5.
- An agent check lists one or more `agents:`, each once.
- A check's `fail_to` names an Agent stage before it.
- With every `skip` taken, an Agent stage must still run, and every check by you must still have one before it.
- Every agent a stage, a check's `agents` or an option names is in `agents:` or already exists.

### What you add only when the user asks

- **`skip`** on any stage, and an **automatic Delivery with no check before it**. Otaq allows both; the flow editor warns
  about the second ("Done opens a PR on GitHub without any check before it"). Don't add them on your own.
- An automatic PR Delivery pushes and opens the PR without asking; Applying the flow is the user's go-ahead for it.

## Why lines

The flow editor shows one line per stage saying **why it is there**: the stage's `why:` (and an agent you add or change
can carry one too). Give every stage one. **Every why line names its source**: what you read that led to it, with
the numbers you actually counted:

- "The user asked for it in this conversation."
- "From 41 /security-review runs in shop-web and api (Claude Code)." (from your tool's history: name the tool)
- "`CONTRIBUTING.md` asks for a review before merging."
- "Unchanged from the current flow."

**With no evidence, say so** ("No evidence found"); never guess a count or invent a source.

## Rules

- **Never edit files, never commit, never push.** You change the flow only by proposing it. The one exception is a
  stage's skill, written only in the folder `otaq flow write-skill` gives you (see "Writing a stage's skill").
- **Propose a whole file** every time, never a part of one. One proposal at a time: a new one replaces the waiting one.
- After proposing, **end your turn**: the answer is your next message.
- **Long text always goes through stdin** (`--stdin`), never quoted arguments.

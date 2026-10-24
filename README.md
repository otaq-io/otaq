<p align="center">
  <img src="crates/otaq-desktop/icons/128x128@2x.png" width="96" alt="Otaq logo">
</p>

<h1 align="center">Otaq</h1>

<p align="center">
  <b>A kanban board that runs coding agents for you: Claude Code, Codex, GitHub Copilot and OpenCode, side by side.</b><br>
  Drag a ticket to <b>Dev</b>. An agent picks it up in its own git worktree, a second agent checks the work,
  and the ticket lands in <b>Ready for Review</b> with a diff waiting for you.
</p>

<p align="center">
  <img src="docs/images/board.png" alt="The Otaq board in Otaq Dark: two epic tracks above six columns, with tickets waiting, working, in review and merged">
</p>

---

Otaq is a desktop app for one developer supervising many coding agents at once. You write tickets the way you
would for a teammate. Otaq starts a real, interactive session of the agent tool you picked for that ticket (Claude
Code, Codex, GitHub Copilot or OpenCode, with the model you chose), keeps the agents out of each other's way, tells you
when one needs you, and gives you a normal code review at the end. Nothing is merged or pushed until you say so.

It is built in Rust (the engine and a background service) with a Tauri + React window, runs on macOS, Windows and
Linux, and is licensed under FSL-1.1-MIT, which becomes MIT two years after each release.

> **Status: early (v0.1).** Otaq is used daily by its author and is now public. There are no prebuilt releases
> yet; build it from source (below). Expect rough edges, and please open issues.

## Contents

- [Why Otaq](#why-otaq)
- [How it works](#how-it-works)
- [A tour of the app](#a-tour-of-the-app)
- [Getting started](#getting-started)
- [The `otaq` command](#the-otaq-command)
- [Architecture](#architecture)
- [Development](#development)
- [License](#license)

## Why Otaq

Running one coding agent is easy. Running eight at once is not: they edit the same checkout, you lose track of
which one is waiting on a question, and reviewing the result means digging through branches. Otaq turns that into a
board.

- **The right agent for each ticket.** Use Claude Code, Codex, GitHub Copilot and OpenCode on the same board, at the
  same time. A tricky bug can go to Opus in Claude Code while a docs fix goes to a small, cheap model, and the work can
  be checked by a model from another company. See [Agents: any tool, any model](#agents-any-tool-any-model).
- **One ticket, one agent, one worktree.** Every ticket gets its own git worktree and branch, so parallel agents never
  step on each other or on your own checkout.
- **Dependencies, so work happens in order.** Ticket B can wait for ticket A. B starts only once A is merged, so it
  always builds on finished work.
- **The real terminal of each tool.** Agents run the actual `claude`, `codex`, `copilot` or `opencode` program, not
  an imitation. You can watch any agent and type into its session at any moment.
- **You only look when it matters.** Cards that need you (a question, a permission prompt, an agent that stopped)
  turn turquoise and show in the Needs you strip above the board; ⌘J takes you to the next one, in any project.
- **A second pair of eyes.** When an agent says it's done, a separate read-only *validator* agent checks the change
  against the ticket. Failures go straight back to the agent; only passing work reaches you.
- **Review like a pull request.** Diff against the ticket's starting point, line comments, Send back, then Approve with a
  local squash merge or a GitHub / Azure DevOps pull request.
- **Agents keep running when the window is closed.** A background service owns every session; reopen the app and
  everything is where you left it.
- **Your setup, untouched.** Otaq uses each tool's own settings, sign-in and permission mode, your `git`, `gh` and
  `az` logins, and never adds files to your repository. What it does record in a tool's own config is listed under
  Settings → Agent tools: mainly that you trust the folders you add as projects, so agents don't stop on a trust
  screen.

## How it works

```
Backlog ──► Ready for Dev ──► Dev ──────────────► Ready for Review ──► Approved ──► Done
            (waits for its     agent works,        you read the diff,   local merge
             dependencies      validator checks    comment, send back   or open a PR
             and a free slot)  the result          or approve
```

1. **Write tickets.** A title and a description in a rich Markdown editor. Acceptance criteria are just text in the
   description. Give a ticket a priority (Urgent, High, Normal, Low) and the tickets it depends on.
2. **Drag a ticket to Dev** (or Ready for Dev). If its dependencies are merged, Otaq creates a worktree from your base
   branch, runs your setup command (for example `pnpm install`), and starts the ticket's agent: the tool and model
   picked for it, or the project's default.
3. **The agent works.** It reads its ticket through the `otaq` command, plans, implements and commits. If it hits a
   question it can't answer from the code or docs, it asks you (the card turns turquoise, with answer options). Smaller
   product choices it makes on its own and records on the ticket, so you can see them in review.
4. **The validator checks.** When the agent submits, a fresh read-only agent compares the change with the ticket and
   the checks you asked for. If something is missing, the findings go back to the agent (up to 3 rounds by default).
5. **You review.** The ticket arrives in Ready for Review with its diff, the agent's summary, the validator's report and
   every decision the agent recorded. Comment on lines and **Send back**, or **Approve**.
6. **Merge.** *Local merge* squashes the branch into your base branch (rebasing first if the base moved). *Open PR*
   pushes and opens a pull request, with a dialog where you edit the branch name, title and description first.
   When it's merged, tickets that were waiting on it start on their own.

### Agents: any tool, any model

Otaq runs four agent tools: **Claude Code**, **OpenAI Codex CLI**, **GitHub Copilot CLI** and **OpenCode**. Copilot
and OpenCode can run models from several companies, so they bring Gemini and others too. Connect any of them in
Settings → Agent tools (Otaq finds what's installed and signed in; it never handles your passwords or API keys).

In Settings → Agents you keep a short list of **named agents**. Each one is a tool, a model and a note on what it's
for:

| Named agent | Tool · model | Use for |
|---|---|---|
| Strong | Claude Code · `opus` | complex features and tricky bugs |
| Standard | Codex · a GPT model | normal tickets |
| Light | Claude Code · `haiku` | typos, docs, small config |
| Verifier | Copilot · a Gemini model | validating |

(These are examples you'd make yourself. A fresh install starts with one entry per connected tool, using that tool's
default model.) The model picker reads each tool's own model list, with "always the newest" choices such as `opus`, so
a new model needs no Otaq update.

- **Pick per ticket.** Every ticket has a **Developer** and a **Validator** row. `SHOP-18` can be written by Opus in
  Claude Code and checked by a GPT model in Codex, while `SHOP-19` goes to Light. An epic picks its **lead** the same
  way.
- **Defaults per role, overridable per project.** Choose which named agent writes code, validates, leads epics and
  runs the Project agent; a project can change any of the four.
- **Agents suggest agents.** The Project agent's plan and an agent's proposed tickets name an agent for each ticket
  from your notes ("Light: docs-only change"), and they create nothing until you agree. An epic lead may still add a
  child to its running epic on its own; the card says it was added by the lead. Your own choice always wins.
- **Limits per tool, with fallbacks.** When one tool hits its usage limit or is logged out, only its agents wait; the
  rest keep going. The banner offers **Move waiting to…** another tool, and with **Use fallback agents automatically**
  on, waiting tickets start on each agent's fallback by themselves. A running conversation never changes tools.
- **Tokens per tool.** Cards and the header show the tokens each tool reports, split by tool. Otaq shows no dollar
  estimates.

Asking you questions, recording decisions and submitting for review all go through the `otaq` command, so they work
the same in every tool. Cards show Working, Needs you and Idle from each tool's own hooks or plugin events.

### Epics: one feature, many tickets

Bigger features become an **epic** with child tickets (`NOTE-7.1`, `NOTE-7.2`, …). Children branch from the epic's own
integration branch and merge back into it, so the feature reaches your base branch, or a pull request, as **one**
change when every child is done. On the board an active epic is a **track** above the columns, with one stop per
child, so you see the whole feature at a glance.

An epic can have a **lead**: a read-only agent that plans the children with you, answers their questions first (and
passes on the ones it can't, with a recommended answer), shares what each merged child learned with the ones still to
come, and flags children that touch the same files. A child that a merge lets start waits for the lead's update
about that merge first. It never edits code or merges anything.

### Project agents and terminals

The **Project agent** waits in the Ask bar at the bottom of the board: type a message and it opens as a panel on the
left, where a session starts on its named agent, investigates the codebase and writes Backlog tickets for you, including importing them from your team's tracker with
your own skills. It reads code but never edits it. **Terminals** on the left rail (⇧⌘T) opens plain terminals in the
project.

## A tour of the app

> The screenshots show a small demo project, `pocket-notes` (a notes app for the terminal), with real Claude Code
> agents working on it.

### The board

![The board in the Daylight theme](docs/images/board-light.png)

The board has two views, **Board** and **List**. The Board has a column per stage of the project's flow (by default
Backlog, Ready for Dev, Dev, Validate, Review and Done); with **Lanes: Epic** each epic in flight gets a lane of its own,
and the List groups the same tickets by stage, status, agent or epic. Above the tickets, the **Needs you** strip lists
what waits for you. **Pause** stops every agent of a project (or of all projects) at once until you resume. Cards
show status, priority, what they wait for (`waits for NOTE-8 to merge`), validation rounds, recorded decisions and
conflicts with the base branch. Hover a card to highlight its dependency chain. Turquoise means "needs you", magenta means "in review".

### A live agent

![An agent's Claude Code session in the ticket's Session tab](docs/images/terminal.png)

Click a card to open the ticket as a sheet, with the tabs **Overview · Session · Files · Activity**. Session holds the
agent's real session in its own tool, and you can type into it at any time. Otaq never mixes its own messages into text
you've started typing. Here the agent has finished `NOTE-4` (colorize the output) and explains a product choice it
made on its own: dim only the id column for now, since the age column belongs to `NOTE-3`. From any terminal,
`otaq attach NOTE-4` opens the same session.

### Review

![Review: the agent's summary, a recorded decision and the validation report](docs/images/review.png)

A ticket in Ready for Review shows what the agent says it did, every decision it recorded (what it built, the other
option, and why), and the validator's report. **Approve** keeps the decisions; **Change…** on a decision, or **Send
back** with comments, returns the ticket to the same agent.

![The Files tab](docs/images/diff.png)

The **Files** tab shows the diff against the commit where the ticket started (All changes, unified or split) and every
file of the worktree, with line comments. Approve offers **Local merge** or **Open PR**.

### Epics and the epic lead

![An epic's lead agent asking a planning question with answer options](docs/images/epic-lead.png)

The epic opens as a sheet too: its Tickets tab lists the epic's tickets and their dependencies, and its Lead tab holds
the lead's terminal. Here the lead of the
*Notebooks* epic is planning: it has already made `NOTE-13.3` wait for `NOTE-13.2` (under **Changes by the lead**),
and now asks a question it can't settle from the code (what plain `notes ls` should show once there are notebooks),
with a recommended answer. You answer with a click (or in its terminal), and the children it refined start when the
epic is dragged to Ready for Dev.

### Run and Shell

Every ticket gets its own ports, exposed as `$PORT`, so several agents can run dev servers at once. **Run** starts the
project's run command for that ticket when you click it, with a log and an optional Open URL. **Shell** is a plain
terminal in the ticket's worktree.

### Themes

15 bundled themes, picked on first run and in Settings: Otaq Dark (the screenshot at the top) and Otaq Light,
Catppuccin (Latte, Frappé, Macchiato, Mocha), Tokyo Night, Dracula, Nord, Gruvbox Dark, Rosé Pine and Rosé Pine Dawn,
One Dark, Daylight (the board above) and Solarized Light.

## Getting started

### What you need

| | |
|---|---|
| **An agent tool** | at least one of [Claude Code](https://docs.anthropic.com/en/docs/claude-code), [Codex CLI](https://github.com/openai/codex), [GitHub Copilot CLI](https://github.com/github/copilot-cli) or [OpenCode](https://opencode.ai), installed and signed in |
| **git** | any recent version |
| **OS** | macOS (most tested), Windows 10/11, or Linux |
| **To build** | Rust 1.99+ (edition 2024) and Node 24 |
| **Optional** | `gh` (GitHub) or `az` (Azure DevOps) for pull requests |

### Build and run from source

```sh
git clone https://github.com/otaq-dev/otaq.git
cd otaq

# The desktop app (the web page is built into the binary)
cd crates/otaq-desktop
npm ci
npm run tauri build      # → target/release/otaq-app
cd ../..

# The otaq command: opens the app, runs the background service, and is what agents call
cargo build --release -p otaq-cli   # → target/release/otaq
```

Put `target/release` on your `PATH` and run `otaq`. With no arguments it opens the app, starting the background
service if needed.

Prebuilt downloads for macOS, Windows and Linux (with a one-line install script) are planned. The builds won't be code
signed, so the first open will show the usual "unidentified developer" warning (on macOS: right-click → Open, or
`xattr -dr com.apple.quarantine Otaq.app`; on Windows: More info → Run anyway).

### Your first project

1. Open Otaq and **add a project**: any folder that is a git repository. Otaq suggests a short key (`SHOP`) used in
   ticket names (`SHOP-12`) and picks your base branch from the remote. Adding a project also lets your connected
   agent tools work in that folder (it records the folder as trusted in each one, as accepting the tool's own trust
   prompt would, so the project's hooks and settings run).
2. Optional: make a few named agents in Settings → Agents (above), and set the defaults. In the project's settings: a **setup** command run in every new worktree (`pnpm install`), a **run**
   command for the Run tab (`pnpm dev --port $PORT`), environment variables, and Agent instructions (for example
   "run `cargo test` before you submit").
3. **Create a ticket**, pick its agent or leave the project default, drag it to **Dev**, and watch the agent start.

Agents use your own permission or approval mode for each tool; Otaq never changes it. In a mode that asks before
edits, they will stop on permission prompts and show up as "Needs you"; auto-accept modes let them run unattended.
Otaq pre-approves only its own `otaq` commands (for Codex, through an `otaq.rules` file in `~/.codex`, which Otaq
removes when you disconnect the tool).

### Where things live

- **Data:** `~/.otaq/` (a SQLite database, logs, the service socket). Set `OTAQ_HOME` to use another folder.
- **Worktrees:** `~/.otaq/wt/<repo>/<TICKET>`, outside your repository. Branches are named `otaq/SHOP-12` while
  work is in progress.
- **Warm builds:** a new worktree starts from a copy-on-write clone of the last finished ticket's build output (APFS,
  btrfs, XFS, ReFS), so its first build only redoes what changed.

### Anonymous usage data

When first run ends, Otaq asks once: **"Help improve Otaq"**. Nothing is sent unless you press *Send anonymous data*;
*No thanks* (or Esc) means nothing is ever sent, and Otaq works the same either way. Change it anytime in
Settings → Data → *Send anonymous usage data*.

If you agree, Otaq sends to [Aptabase](https://aptabase.com) (EU region) which features are used and how often, with
the Otaq version and your OS. It never sends ticket text, code, diffs, prompts, project, file or branch names, paths,
or any ID that follows you across launches: each launch gets a new random session, and nothing is stored on disk. The
full list of what counts as a feature is in [`crates/otaq-desktop/src/usage.rs`](crates/otaq-desktop/src/usage.rs).
Debug builds (`npm run tauri dev`) send nothing at all: the Aptabase App Key is only built into release builds.

## The `otaq` command

`otaq` is both your tool and the agents' tool. Agents use it (taught by Otaq's bundled skills) to read their ticket,
ask questions and submit; you can use the same commands from any terminal.

```sh
otaq                         # open the app
otaq list -P SHOP            # the project's tickets
otaq create "Fix login" -p 2 --deps SHOP-12
otaq create "Fix README typo" --developer Light   # pick its named agent
otaq create -f plan.md       # many tickets from one Markdown file
otaq show SHOP-18
otaq dep tree SHOP-18
otaq attach SHOP-18          # this ticket's live agent session, in your terminal
otaq daemon status           # the background service
```

Every command takes `--json`. Run `otaq --help` for the full list.

The agent's workflow lives in [`plugin/`](plugin) as skills: `work-ticket` (the main loop), `validate`, `commit`,
`push`, `open-pr`, `address-review`, `restack`, `epic-lead` and `project-agent`. Otaq gives the same skills to every
tool, in the way that tool loads them (a Claude Code plugin, an OpenCode plugin in [`plugin/opencode`](plugin/opencode),
and so on). You can replace the commit, push, open-PR and validate skills with your own, globally or per project, and
the setting applies to every tool. The hooks only act when `OTAQ_TICKET` is set, so your ordinary sessions are
unaffected.

## Architecture

```
Otaq app (Tauri + React) ◄── local socket ──► otaq daemon (Rust core) ◄── PTY ──► one agent per ticket:
                                                    │                              claude, codex, copilot or opencode
                                                    │                              (cwd = the ticket's worktree)
                                                    ├── SQLite (~/.otaq/otaq.db)
                                                    └── git worktrees, ports, Run and Shell processes
```

| Crate | What it is |
|---|---|
| [`otaq-core`](crates/otaq-core) | the engine: tickets, dependencies, scheduler, git and worktrees, sessions, validation, one adapter per agent tool, the socket server; runs as `otaq daemon` |
| [`otaq-view`](crates/otaq-view) | screen logic as plain, tested Rust view models, no UI framework |
| [`otaq-desktop`](crates/otaq-desktop) | the Tauri 2 app with a React + TypeScript page, xterm.js terminals and the Tiptap Markdown editor |
| [`otaq-cli`](crates/otaq-cli) | the `otaq` binary: opens the app, `daemon`, `attach`, hooks and the agent commands |
| [`plugin/`](plugin) | the skills and hooks that define how agents behave, for every tool |

The app, the CLI and the agents all talk to the same background service over a user-only local socket, using
newline-delimited JSON.

## Development

```sh
cd crates/otaq-desktop
npm ci
npm run tauri dev        # the app with live reload
```

Use a throwaway data folder while developing: `OTAQ_HOME=/tmp/otaq-dev npm run tauri dev`.

Every change must pass `scripts/check.sh` (macOS and Linux) or `scripts/check.ps1` (Windows): typecheck, lint, format
and tests for the page, then `cargo fmt`, `clippy -D warnings`, `cargo nextest run --workspace` and
`cargo test --workspace --doc`. The tests need [cargo-nextest](https://nexte.st) (`brew install cargo-nextest`, or
`cargo install cargo-nextest --locked`). Each crate's integration tests are one test program, `tests/suite/`; to run
one file, for example `tests/suite/naming_literals.rs`: `cargo nextest run -p otaq-core -E 'test(/^naming_literals::/)'`.
See [`scripts/README.md`](scripts/README.md) and [`crates/otaq-desktop/README.md`](crates/otaq-desktop/README.md) for how
the frontend is put together.

## License

[FSL-1.1-MIT](https://fsl.software), see [`LICENSE`](LICENSE): you can use, change and share Otaq for anything except a competing commercial product, and each version becomes MIT two years after its release. Third-party notices are in [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md) and in
the app under Settings → About.

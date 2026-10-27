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
  <img src="docs/images/board.png" alt="The Otaq board in Otaq Dark: six columns, with tickets waiting, working, in review and merged">
</p>

---

Otaq is a desktop app for one developer supervising many coding agents at once. You write tickets the way you
would for a teammate. Otaq starts a real, interactive session of the agent tool you picked for that ticket (Claude
Code, Codex, GitHub Copilot or OpenCode, with the model you chose), keeps the agents out of each other's way, tells you
when one needs you, and gives you a normal code review at the end. Nothing is merged or pushed until you say so.

It is built in Rust (the engine and a background service) with a Tauri + React window, runs on macOS, Windows and
Linux, and is licensed under FSL-1.1-MIT, which becomes MIT two years after each release.

> **Status: early (v0.1).** Otaq is used daily by its author and is now public. Setup files come with the first
> release on [GitHub Releases](https://github.com/otaq-io/otaq/releases); until then, build it from source (below).
> Expect rough edges, and please open issues.

## Contents

- [Why Otaq](#why-otaq)
- [How it works](#how-it-works)
- [A tour of the app](#a-tour-of-the-app)
- [Getting started](#getting-started)
- [Install](#install)
- [The `otaq` command](#the-otaq-command)
- [Uninstall Otaq](#uninstall-otaq)
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
  Claude Code and checked by a GPT model in Codex, while `SHOP-19` goes to Light. A ticket's helpers run on the same
  agent as the one in charge.
- **Defaults per role, overridable per project.** Choose which named agent writes code, validates and answers as
  Noyan; a project can change any of the three.
- **Agents suggest agents.** Noyan's plan and an agent's proposed tickets name an agent for each ticket from your
  notes ("Light: docs-only change"), and they create nothing until you agree. Your own choice always wins.
- **Limits per tool, with fallbacks.** When one tool hits its usage limit or is logged out, only its agents wait; the
  rest keep going. The banner offers **Move waiting to…** another tool, and with **Use fallback agents automatically**
  on, waiting tickets start on each agent's fallback by themselves. A running conversation never changes tools.
- **Tokens per tool.** Cards and the header show the tokens each tool reports, split by tool. Otaq shows no dollar
  estimates.

Asking you questions, recording decisions and submitting for review all go through the `otaq` command, so they work
the same in every tool. Cards show Working, Needs you and Idle from each tool's own hooks or plugin events.

### Bigger features: a Plan and helpers

A bigger feature is still **one ticket** that walks the flow, with one agent in charge at each stage. The ticket can
carry a Markdown **Plan**, written by Noyan, by a planning stage of your flow, or by you, with one **Pieces** list:

```markdown
## Pieces
1. Wishlist API
2. Wishlist button — waits for 1
```

In the build stage, the agent in charge may start **helpers**: other agents, one per piece, each in its own worktree.
It reviews each helper's work, sends it back with a note or merges it into the ticket's branch, so the feature
reaches your base branch, or a pull request, as **one** change. Helpers are rows inside the card
(`40.1 merged · 40.2 working`). A helper's questions go to the agent in charge, which answers them or passes them on
to you with a note; its permission prompts come straight to you, and you answer in its terminal. The ticket takes one
slot, and Pause, Park and a restart treat it and its helpers as one.

### Noyan and terminals

**Noyan** waits in the Ask bar at the bottom of the board: type a message and it opens as a panel on the
left, where a conversation starts on its named agent, investigates the codebase and writes Backlog tickets for you, including importing them from your team's tracker with
your own skills. It reads code but never edits it. **Terminals** on the left rail (⇧⌘T) opens plain terminals in the
project.

## A tour of the app

> The screenshots show a small demo project, `pocket-notes` (a notes app for the terminal), with real Claude Code
> agents working on it.

### The board

![The board in the Daylight theme](docs/images/board-light.png)

The board has two views, **Board** and **List**. The Board has a column per stage of the project's flow (by default
Backlog, Ready for Dev, Dev, Validate, Review and Done), and the List groups the same tickets by stage, status or
agent. Above the tickets, the **Needs you** strip lists
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

### The Plan and helpers

A ticket's Overview shows its Plan, with who wrote it and its pieces in order (`13.3 · waits for 2`). While the ticket
is in the build stage, each piece shows its helper's status, and the Files tab tags each change with the piece it came
from. A helper waiting on you shows in the Needs you strip as `NOTE-13 · 13.2 asks`; ⌘J opens its terminal.

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
| **To build** (only from source) | Rust 1.99+ (edition 2024) and Node 24 |
| **Optional** | `gh` (GitHub) or `az` (Azure DevOps) for pull requests |

### Install

Each release on [GitHub Releases](https://github.com/otaq-io/otaq/releases) has a setup file per system. Download
yours and open it like any app:

| System | Setup file | Updates |
|---|---|---|
| macOS (Apple silicon and Intel) | `Otaq_<version>_universal.dmg`: drag Otaq to Applications | by itself |
| Windows 10/11 (x64, arm64) | `Otaq_<version>_<arch>-setup.exe`: installs for your account only, no admin rights | by itself |
| Linux (x64, arm64) | `Otaq_<version>_<arch>.AppImage`: make it executable and run it from anywhere | by itself |
| Debian, Ubuntu (x64, arm64) | `Otaq_<version>_<arch>.deb`: `sudo apt install ./Otaq_<version>_<arch>.deb` | by hand: download the next `.deb` |

Otaq isn't code signed (paid OS signing isn't something it does), so the first open asks you to allow it once:

- **macOS:** the first open says Apple can't check Otaq. Open **System Settings → Privacy & Security**, scroll to
  "Otaq was blocked…" and click **Open Anyway**. It asks for an administrator's name and password: on a standard
  account, an administrator does this once. Otaq runs only from an Applications folder: opened from the `.dmg`
  or from Downloads, its window offers just **Move to Applications**, which moves it (to `/Applications`, or
  `~/Applications` when you can't write there) and opens it from there.
- **Windows:** SmartScreen says it protected your PC: click **More info → Run anyway**. Otaq goes into
  `%LOCALAPPDATA%\Programs\Otaq`, with a Start-menu shortcut.
- **Linux:** the status icon needs the AppIndicator library (the `.deb` installs it; for the AppImage,
  `sudo apt install libayatana-appindicator3-1` on Debian and Ubuntu), and stock GNOME shows tray icons only with the
  AppIndicator extension (`gnome-shell-extension-appindicator`, on by default in Ubuntu's desktop). Without either,
  Otaq still runs, without the icon (see [The status icon](#the-status-icon)).

**Updates happen by themselves.** Once a day Otaq asks GitHub for the newest version (one request for a fixed file;
nothing about you or your projects is sent), downloads it in the background and checks its signature. Then Settings
gets a dot and Settings → About says **Otaq 0.2.0 is ready · What's new · Restart now · Skip this version**. Nothing
changes until then: the new version installs when you pick **Restart now** (or **Restart to update** in the status
icon's menu), or the next time you open Otaq. Either way only the window reopens, on the new version, and says
"Updated to Otaq 0.2.0"; your agents keep working. The background service moves to the new version later, when you
pick **Restart now** on its "Service update ready" banner. An update the running service can't work with says so
first ("needs agents restarted (3 working)", with **Restart now · stops 3 agents**): it restarts the app and the
service together, as after a reboot (the agents show Interrupted and Resume all continues them), and it never
installs on its own when you open Otaq. The switch **Check for updates automatically** and the **Check for updates**
button are in Settings → About. The `.deb` doesn't update itself: About says "Otaq 0.2.0 is available · Download".

**Going back to an older version.** Before a new version changes the database's layout, Otaq keeps a copy of it
(`~/.otaq/otaq.db.before-0.2.0`). If you reinstall an older version, its window says the board was last opened by a
newer Otaq and offers **Restore the copy from before 0.2.0**; `otaq restore-database` does the same in a terminal.
Changes made on the newer version since the copy are lost.

### Build and run from source

```sh
git clone https://github.com/otaq-io/otaq.git
cd otaq
cd crates/otaq-desktop && npm ci && cd ../..
scripts/install-local.sh     # macOS: builds Otaq.app and installs ~/Applications/Otaq.app
```

`scripts/install-local.sh` builds the same `Otaq.app` a release has, with Tauri's bundler. It is **built from
source**: it never checks for updates, a release never replaces it, and Settings → About says "Built from source ·
<commit>". Build it again to update. On Windows and Linux, `npm run bundle` in `crates/otaq-desktop` makes the setup
`.exe` or the `.AppImage` and `.deb` the same way; [`scripts/README.md`](scripts/README.md) says how for each system.

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

### The status icon

While Otaq runs, its icon sits in the menu bar (macOS) or the notification area / tray (Windows, Linux). It shows when
an agent needs you or every project is paused, and its menu lists each project's needs-you items, checks and running
tickets: pick one to open Otaq on it.

- **Closing the window hides it.** Otaq keeps running in the menu bar / tray and agents keep working; the icon's Open
  Otaq (or running `otaq` again) brings the window back. Only one copy of Otaq runs per data folder.
- **Stop agents and quit**, in the icon's menu, stops every agent and terminal, stops the background service, pauses
  all projects so nothing starts on its own, and quits. Next time, Resume continues where each agent was.
- **⌘Q on macOS** quits only the app; agents keep working in the background service. While agents work it asks first:
  keep them working and quit, stop agents and quit, or cancel.
- **Linux without a tray** (stock GNOME without the AppIndicator extension, or without the library): a hidden window
  couldn't be found again, so closing the window acts as Stop agents and quit, asking first while agents work.

### Where things live

- **Data:** `~/.otaq/` (a SQLite database, logs, the service socket). Set `OTAQ_HOME` to use another folder.
- **The `otaq` command for your terminal:** `~/.otaq/bin/otaq`, kept by the app on every start (see
  [Your own terminal](#your-own-terminal)). Agents use their own copy in `~/.otaq/runtime/<build>/`, so an update
  never changes the version an agent's session runs. Downloaded updates wait in `~/.otaq/updates/`.
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

### Your own terminal

Agents don't need anything set up. To run `otaq` in your own terminal, open **Settings → About** and click **Add to
my shell's PATH** under "Use otaq in your own terminal". Nothing is edited until you click. It adds one line to your
login shell's file, marked `# added by Otaq`:

- zsh: `~/.zshrc` (or `$ZDOTDIR/.zshrc`); bash: `~/.bashrc`, and on macOS also `~/.bash_profile` when that doesn't
  read `~/.bashrc`; fish: `~/.config/fish/conf.d/otaq.fish`;
- Windows: `%USERPROFILE%\.otaq\bin` is added to your account's PATH (no PowerShell profile is edited).

The line is `export PATH="$HOME/.otaq/bin:$PATH" # added by Otaq` (fish: `set -gx PATH "$HOME/.otaq/bin" $PATH`).
Open a new terminal afterwards. If the file can't be written, About shows the line to paste yourself, with Copy.
`~/.otaq/bin/otaq` always points at the Otaq you opened last.

```sh
otaq                         # open the app
otaq list -P SHOP            # the project's tickets
otaq create "Fix login" -p 2 --deps SHOP-12
otaq create "Fix README typo" --developer Light   # pick its named agent
otaq create -f plan.md       # many tickets from one Markdown file
otaq create "Wishlist" --plan-file wishlist-plan.md   # one ticket with a Plan of pieces
otaq update SHOP-40 --plan-file wishlist-plan.md      # replace its Plan (or --plan, --plan-stdin)
otaq show SHOP-18
otaq show SHOP-40 --piece 2  # one piece of the Plan
otaq delete SHOP-41 --reason "folded into SHOP-40"   # a never-started ticket you created
otaq dep tree SHOP-18
otaq attach SHOP-18          # this ticket's live agent session, in your terminal
otaq daemon status           # the background service
otaq uninstall               # remove what Otaq left on this computer (asks first; see below)
```

The agent in charge of a ticket runs its helpers with `otaq helper`:

```sh
otaq helper start 2 --title "Wishlist button" --stdin   # piece 2, with a note on stdin
otaq helper list             # each helper's piece, status and branch
otaq helper diff 2           # what merging it would bring
otaq helper back 2 --stdin   # send it back with a note
otaq helper merge 2          # merge it into the ticket's branch
otaq helper answer 2 --stdin # answer its question
otaq helper pass-on <id> --note "a product question the Plan doesn't answer"
otaq helper done --stdin     # a helper hands its piece back, with a summary
```

Every command takes `--json`. Run `otaq --help` for the full list.

The agent's workflow lives in [`plugin/`](plugin) as skills: `work-ticket` (the main loop, for the agent in charge
and for its helpers), `validate`, `commit`, `push`, `open-pr`, `address-review`, `answer-question`, `restack`, `flow`,
`product-spec`, `tech-spec`, `milestone-plan` (the Spec first preset's planning stages) and `noyan` (one ticket
with a Plan, or several tickets). Otaq gives the same skills to every tool, in the way that tool loads them (a Claude
Code plugin, an OpenCode plugin in [`plugin/opencode`](plugin/opencode), and so on). You can replace the commit,
push, open-PR and validate skills with your own, globally or per project, and the setting applies to every tool. The
hooks only act when `OTAQ_TICKET` is set, so your ordinary sessions are unaffected.

## Uninstall Otaq

Quit the Otaq app (from its menu bar / tray icon: closing the window only hides it), then run in a terminal:

```sh
otaq uninstall
```

(With the app open it stops at "Quit Otaq from its menu bar / tray icon first: the app would start the service
again".) It says what it is about
to do and asks before changing anything. Both questions come first; nothing is deleted until you have answered yes
to each, and only `y` or `yes` counts: Enter means no.

1. **Worktrees, if any are left.** Every worktree Otaq still has (a ticket in progress, a helper's, a Done or Closed
   ticket's kept one, one no ticket owns, or a merge worktree a crash left with changes) is listed, with ✎ on those
   with uncommitted changes:

   ```
       SHOP-12 · shop-web · /Users/me/.otaq/wt/shop-web/SHOP-12
     ✎ SHOP-14 · shop-web · /Users/me/.otaq/wt/shop-web/SHOP-14
     ✎ SHOP-14.1 (helper) · shop-web · /Users/me/.otaq/wt/shop-web/SHOP-14.1
   Delete these 3 worktrees? Uncommitted changes in the 2 marked ✎ are lost. Their branches are kept. [y/N]
   ```

   **N** stops with "Nothing changed. Move them on or clear them first (Done, Close, the Cleanup view)". **y**
   removes each one with `git worktree remove`, so your repositories' records stay correct.
2. **The data folder** (`~/.otaq`, or the folder `OTAQ_HOME` names), with its size, and what is still running:

   ```
   Delete ~/.otaq (6.4 GB: 3 projects, 41 tickets, warm builds 5.9 GB)? 2 agents and 1 conversation with Noyan end. [y/N]
   ```

Then it stops the background service (agents still working are interrupted), removes the worktrees, removes
Otaq's skill copies from your projects (`.claude/skills/otaq-*`, `.agents/skills/otaq-*`), deletes the data folder
(with `~/.otaq/bin`), removes the `# added by Otaq` line from your shell's file (on Windows, the folder from your
PATH), and prints what it removed and what it kept, for example:

```
Kept on purpose:
  Claude Code, Codex: SHOP and PAY stay trusted
  shop-web: 23 otaq/ branches kept · git branch --list 'otaq/*'
Drag Otaq.app to the Trash
```

Anything it skipped or couldn't remove is listed with its path (and the exit code is non-zero). The last step is
yours, and the summary's last line names it:

- **macOS:** drag Otaq.app to the Trash.
- **Windows:** uninstall Otaq from **Settings → Apps**.
- **Linux:** remove the package (`sudo apt remove otaq`) or delete the AppImage.

Removing only the program (Settings → Apps, `apt remove otaq`, deleting the AppImage) stops the background service
and leaves your worktrees and data; the Windows uninstaller and the `.deb` say to run `otaq uninstall` first.

**Kept on purpose:**

- **Your `otaq/` branches**, merged or not, so committed work is never lost. List them with
  `git branch --list 'otaq/*'` and delete the ones you don't need with `git branch -d`.
- **Folder trust in your agent tools.** Adding a project marked its folder as trusted in Claude Code, Codex and
  Copilot. Otaq doesn't know whether you had trusted the folder yourself before, so it leaves the mark alone; it only
  means the tool won't ask again before working there. To remove one by hand, close the tool, keep a copy of the
  file, and remove only that folder's entry:
  - Claude Code: `hasTrustDialogAccepted` under the folder's entry in `projects`, in `~/.claude.json`
  - Codex: `trust_level = "trusted"` in the folder's `[projects."<folder>"]` table, in `~/.codex/config.toml`
  - Copilot: the folder's line in the `trustedFolders` list, in `~/.copilot/config.json`

**Coming back later?** `otaq uninstall --keep-data` keeps the database (with its `otaq.db.before-<version>` copy)
and the comment reports, so reinstalling brings your projects and tickets back; warm builds, logs, worktrees,
`~/.otaq/bin` and the PATH line still go. Its data-folder question reads as
below (with the same "… end." sentence when agents are running):

```
Delete everything in ~/.otaq but the database and comment reports (5.9 GB)? [y/N]
```

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

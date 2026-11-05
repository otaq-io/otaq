<p align="center">
  <img src="crates/otaq-desktop/icons/128x128@2x.png" width="96" alt="Otaq logo">
</p>

<h1 align="center">Otaq</h1>

<p align="center">
  <a href="https://otaq.io">otaq.io</a> · <a href="#install">install</a> · <a href="#first-project">first project</a> · <a href="#docs">docs</a> ·
  <a href="https://github.com/otaq-io/otaq/releases">releases</a>
</p>

<p align="center">
  <a href="https://github.com/otaq-io/otaq/releases/latest"><img src="https://img.shields.io/github/v/release/otaq-io/otaq" alt="Latest release"></a>
  <a href="https://github.com/otaq-io/otaq/releases"><img src="https://img.shields.io/github/downloads/otaq-io/otaq/total" alt="Downloads"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-FSL--1.1--MIT-blue" alt="License: FSL-1.1-MIT"></a>
</p>

---

https://github.com/user-attachments/assets/6ee3da50-5354-4e59-8fe2-33d9576e9732

**Coding agents that follow your process.** Otaq is a desktop app that runs Claude Code, Codex, GitHub Copilot and
OpenCode through stages you design. Say what you want, answer when asked, and approve a diff that other agents
already checked.

- **Your flow, your stages.** Choose the steps a ticket goes through: agents doing work, checks that pass or fail
  it, and how it's delivered. Start from a preset (Otaq default, Spec first, Solo autopilot, Team PR), or have an
  agent propose one based on how you already work.
- **Named agents.** Each one is a tool, a model, instructions and skills: a Rust developer on Opus, a security
  reviewer that only reads code, a docs updater on a small model. Choose one per stage, per project or per ticket.
- **Checked by other agents.** Validators and reviewers test the work against the ticket and run your app. Work that
  fails goes back to the stage that needs to fix it, so you only see work that passed.
- **Noyan, your planning agent.** Ask "why is checkout slow? make tickets." It reads the code and writes the
  tickets, ready for you to start.
- **Big features, one ticket.** The agent in charge splits the work into pieces, starts a helper agent for each one
  in its own worktree, and merges their work into one change.
- **Parallel without collisions.** Every ticket gets its own git worktree, branch and ports. Dependencies make
  ticket B wait until ticket A is merged.
- **Know which one needs you.** Questions and permission prompts turn the card turquoise and send a notification.
  ⌘J jumps to the next one, in any project. Pause stops spending at once.
- **Review, then deliver.** See the diff, every check's report and the decisions the agent made. Then do a local
  merge, or open a GitHub / Azure DevOps pull request.
- **The real tools, on your machine.** Otaq runs the actual `claude`, `codex`, `copilot` and `opencode` programs
  with your own subscriptions and permission modes, in terminals you can type into. It needs no account or server,
  and agents keep working when the window is closed.

> **Status: early (v0.1).** The author uses Otaq every day. Expect rough edges, and please open issues.

## install

Download the setup file for your system from the [latest release](https://github.com/otaq-io/otaq/releases/latest): `.dmg`
(macOS), `-setup.exe` (Windows), `.AppImage` or `.deb` (Linux). Otaq updates itself.

You also need git and at least one agent tool installed and signed in. Otaq isn't code signed, so your system asks
you to allow it the first time you open it. [Install details →](docs/install.md)

## first project

1. Open Otaq and add a project (any git repository).
2. Create a ticket and drag it to **Dev**.
3. Answer when the card turns turquoise, and review when the ticket reaches **Review**.

## docs

[How it works](docs/how-it-works.md) · [Install and setup](docs/install.md) ·
[Uninstall](docs/install.md#uninstall-otaq) · [The `otaq` command](docs/otaq-command.md) ·
[Architecture and development](docs/development.md)

## development

```sh
cd crates/otaq-desktop && npm ci
OTAQ_HOME=/tmp/otaq-dev npm run tauri dev   # the app with live reload, on a throwaway data folder
scripts/check.sh                             # every change must pass (scripts/check.ps1 on Windows)
```

## license

[FSL-1.1-MIT](LICENSE): use, change and share Otaq for anything except a competing commercial product. Each version
becomes MIT two years after its release. Third-party notices are in
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

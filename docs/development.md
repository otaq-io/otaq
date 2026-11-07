# Architecture and development

[← README](../README.md)

## Architecture

```
Otaq app (Tauri + React) ◄── local socket ──► otaq daemon (Rust core) ◄── PTY ──► one agent per ticket:
                                                    │                              claude, codex, copilot or opencode
                                                    │                              (cwd = the ticket's worktree)
                                                    ├── SQLite (~/.otaq/otaq.db)
                                                    └── git worktrees, ports, Run and Terminal processes
```

| Crate | What it is |
|---|---|
| [`otaq-core`](../crates/otaq-core) | the engine: tickets, dependencies, scheduler, git and worktrees, sessions, validation, one adapter per agent tool, the socket server; runs as `otaq daemon` |
| [`otaq-view`](../crates/otaq-view) | screen logic as plain, tested Rust view models, no UI framework |
| [`otaq-desktop`](../crates/otaq-desktop) | the Tauri 2 app with a React + TypeScript page, xterm.js terminals and the Tiptap Markdown editor |
| [`otaq-cli`](../crates/otaq-cli) | the `otaq` binary: opens the app, `daemon`, `attach`, hooks and the agent commands |
| [`plugin/`](../plugin) | Otaq's own skills and hooks that define how agents behave, shipped to each tool as its plugin |

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
See [`scripts/README.md`](../scripts/README.md) and [`crates/otaq-desktop/README.md`](../crates/otaq-desktop/README.md) for how
the frontend is put together.


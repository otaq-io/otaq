# 065 · Test in the browser goes; the checker runs the app

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 057, 064, 081

## Summary

Test in the browser leaves Edit flow's ready stages, and `validate-in-browser` goes with it. There is no new stage in
its place: checking the running app is part of how the checker (Validate) works, for every project.

## Decision

- The checker judges the plan and the work done, and verifies the app: when the app is something that can be run, it
  runs it first and checks that it does what the ticket asked.
- No tool is named: Chrome, Playwright, `curl` or the project's own scripts, whatever reaches what changed.
- With no way to run the app, it checks what it can and says so in its verdict.
- The `browser-validate` plugin is retired (decision 080).

## Why

Test in the browser worked only on macOS with Chrome and wired its browser watcher for Claude Code only, against
decision 064 (gap G16). Running the app belongs in every check, not in a stage the user has to add. Porting Test in the
browser to every tool and system, and a separate Verify stage, were turned down.

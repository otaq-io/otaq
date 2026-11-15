# 109 · One otaq plugin keeps the hooks and the workflow skills; no otaq-hooks rename

- **Date:** 2026-11-14
- **Status:** Accepted
- **Supersedes:** 082
- **Related:** 090, 102

## Summary

Each tool keeps getting one plugin named `otaq` that carries both Otaq's hooks and its workflow skills, as today. The
rename of the hooks plugin to `otaq-hooks` is dropped.

## Decision

- The `otaq` plugin per tool holds the hooks and the workflow skills; both change only with the app (decision 102).
- The hooks stay out of anything installed or updated apart from the app, as decision 082 says.
- No rename: Codex's hook trust, recorded for `otaq@otaq-local`, isn't recorded again.

## Why

Decision 082 renamed the hooks plugin only because the workflow skills were to become a separate plugin named `otaq`.
Since decision 102 ships the workflow skills inside the app like the hooks, one plugin carries both with nothing to
clash; nobody types the plugin's name. Splitting anyway was turned down as a second plugin per tool and a trust
migration on Codex for no gain.

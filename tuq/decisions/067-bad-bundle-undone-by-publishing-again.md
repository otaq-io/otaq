# 067 · A bad skill bundle is undone by publishing again

- **Date:** 2026-11-14
- **Status:** Superseded by 102
- **Related:** 061, 066

## Summary

The maintainer fixes the skill, or puts the earlier text back, and publishes; that newer bundle reaches every app as any update does. There is no withdraw command.

## Decision

- Customers click nothing.
- The newest usable bundle always wins; a session already running on the bad bundle finishes on it (decision 061).

## Why

A withdraw command would give apps a second way to change bundles ("go back" next to "move forward"); one rule is
simpler. Was S23.

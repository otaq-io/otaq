# 014 · New agent's Model list hides signed-out tools

- **Date:** 2026-11-10
- **Status:** Accepted

## Summary

Settings → Agents → New agent → Model lists only the tools that can run now. A signed-out tool, such as Claude Code
while signed out, isn't listed, as built.

## Decision

- The Model list leaves out a tool that is signed out. It shows up again once the tool is signed in.
- The Tools step and Settings → Agents still list it with "not signed in". That is where the user signs in.

## Why

The list offers only tools that can run an agent now, so a new agent never starts out on a tool that won't run. The
release test (issue 11) flagged the missing Claude Code. After first choosing to list it as pickable with "not signed
in", the user went back to the behaviour as built. Showing it greyed out so it can't be picked was also turned down.

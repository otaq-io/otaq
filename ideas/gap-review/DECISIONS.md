# Answers to the gap review

Agreed with the maintainer on 2026-11-06, going through `REPORT.md` in this folder. The answers are folded into the
decision files, which are the record; this file only says where each one went. "G<n>" is a gap in `REPORT.md`.

## The problem behind most answers

Since Otaq no longer copies skills, a skill exists only for the tools whose folders hold it. A stage whose skill is
`/some-skill` (in `~/.claude/skills`) can't run properly on Copilot. If Claude Code hits its limit and the work moves
to Copilot, the stage runs without its skill. The answer: **a stage runs only on a tool that has its skill**, and
**a flow file says everything a flow does**.

## Where each answer lives

| Answer | Gaps | Now in |
|---|---|---|
| A stage runs only on a tool that has its skill; held at start, not started with a warning | G4, G17, G31 | `ideas/no-skill-copies`, S5; Known gaps; F1 superseded; F12 |
| Which tool has a skill; plugin skills; the launch environment | G4, G6, G18 | `ideas/no-skill-copies`, S6; F12 |
| OpenCode's hands-on check before release | G9 | `ideas/no-skill-copies`, F13 |
| F10 in the same release | G5 | `ideas/no-skill-copies`, F14 |
| Open PR link, Codex plugin refresh, "What this agent reads", cleanup, no checksum repair | G10, G25, G28, G29, G30 | `ideas/no-skill-copies`, F15–F18 |
| What is removed (Fallback per tool, Use fallback, Use fallback now) | G2 | `ideas/no-fallback-agents`, Decision |
| What waits (limit, logged out, not installed, hidden, model gone) | G13, G14 | `ideas/no-fallback-agents`, Decision |
| One ticket at a time by hand; Noyan's New agent on…; agents don't pick stages | G1, G12, G16 | `ideas/no-fallback-agents`, What stays |
| Moving over from fallbacks | G15 | `ideas/no-fallback-agents`, Moving over |
| A flow file says everything a flow does | — | `ideas/one-check-agent`, Principles |
| `agent:` format, flow skill, rounds per stage, `--agent` | G19, G20, G26, G27 | `ideas/one-check-agent`, Decision |
| The split: when, names, columns, choices, old rounds, release notes | G7, G8, G21, G22, G23 | `ideas/one-check-agent`, Moving over |
| Any agent for any stage; skip only where marked; no auto-skip, no "required"; no forced click; overrides only on skippable checks; kept features | G3, G24 | `ideas/one-check-agent`, Other stage features O1–O6 |

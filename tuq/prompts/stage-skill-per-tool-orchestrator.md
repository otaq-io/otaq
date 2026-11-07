# Orchestrator · stage-skill-per-tool

- **Ticket:** none
- **Date:** 2026-11-06
- **Plan:** tuq/plans/stage-skill-per-tool.md
- **Tech spec:** none; ideas/gap-review/REPORT.md holds the code evidence (file:line) per gap
- **Decisions:** none in tuq/; this round's are ideas/no-skill-copies/DECISIONS.md (S5, S6, F12–F18), ideas/no-fallback-agents/DECISIONS.md, ideas/one-check-agent/DECISIONS.md (O1–O6)

Since Otaq no longer copies skills, a stage must run only on a tool that has its skill: no fallbacks, one agent per
check stage, a per-ticket agent choice limited to tools with the skill, and flows whose file says everything they do.

You are the agent in charge of building stage-skill-per-tool. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

## Setup (before any piece)

- **The base branch is `no-skill-copies`, not `main`.** `main` is behind: the no-skill-copies work (T1–T10, B1–B2,
  C1–C3, E1–E3) is merged only into `no-skill-copies`, and every piece builds on it. Start this session in its
  worktree, `/Users/nevzattopcu/Desktop/Workspace/otaq-no-skill-copies`, and check that `git rev-parse
  --abbrev-ref HEAD` prints `no-skill-copies`.
- **Never change the main checkout** (`/Users/nevzattopcu/Desktop/Workspace/otaq`): don't switch its branch, and don't
  commit, stash or merge anything in it. Don't merge into `main`; the maintainer does that.
- **Commit the plan first.** The decision files, the gap review and this plan aren't committed yet, so a piece's
  worktree wouldn't have them. Commit them on `no-skill-copies` as the first commit, `stage-skill-per-tool:
  decisions and plan`: `ideas/gap-review/`, `ideas/no-fallback-agents/`, `ideas/one-check-agent/`,
  `ideas/no-skill-copies/DECISIONS.md`, `tuq/`, and the "tuq" pointers in `README.md` and `CLAUDE.md`. Check
  `git status` first and commit nothing else.
- **One branch per piece**, made from `no-skill-copies` as it is when the piece starts (so it has the pieces it
  waits for): `no-skill-copies-G<N>` in its own worktree, `/Users/nevzattopcu/Desktop/Workspace/otaq-nsc-G<N>`.
  Merge each finished piece into `no-skill-copies` with a merge commit, `Merge no-skill-copies G<N>: <title>`, as
  the earlier rounds did.
- Run `scripts/check.sh` once on `no-skill-copies` before the first piece, so you know the starting state, and again
  after each merge.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 Otaq knows which tools have each skill, 2 No fallbacks of any kind, 9 The Delivery does what the flow says, 11 "Use your own Open PR skill" points at the Delivery stage, 12 Codex's plugin copy is refreshed on any plugin change, 14 The code left from imported skills is gone (side by side)
- After 1, 2: 3 A check stage runs one agent, 10 Skip this check and Move on anyway only on skippable checks, 13 "What this agent reads" shows the user's own skill (side by side)
- After 1, 2, 3: 4 Old flows with several check agents open split, 5 A ticket picks any agent for any stage, 6 The flow editor offers only agents whose tool has the stage's skill (side by side)
- After 1, 5: 7 A stage is held when its tool lacks its skill, 8 Nothing is skipped on its own; no "required" (side by side)
- After 2, 3, 4, 5, 7, 8, 9, 10: 15 The docs, the website and the flow skill describe the new rules
- After 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15: 16 Hands-on on all four tools

<!-- tuq:order:end -->

## Running a piece

### In an Otaq build stage ($OTAQ_TICKET set, prime shows this Plan)

- Start a piece with `otaq helper start N --title "<the Plan's title>"`; when it has a note below, pass the note on
  stdin (`--stdin`).
- Check what it hands back with `otaq helper diff N` and the project's tests in its worktree (`otaq helper list`
  names it).
- Merge with `otaq helper merge N`, one piece at a time, then start the pieces that waited for it.
- Follow the build stage's own rules for questions, hand-backs and re-plans; this prompt never replaces them.

### Anywhere else

- Start one Agent call per piece with isolation "worktree". Its prompt: the piece's details from the plan, its note
  below, and "commit on your branch; don't merge".
- Check its diff and the project's tests in its worktree.
- Merge its branch into the current branch with `git merge`, one piece at a time, then start the pieces that waited
  for it.

## Notes per piece (only pieces that need one)

Pass on to every piece: its branch is `no-skill-copies-G<N>`, made from `no-skill-copies` (never from `main`), in its
own worktree; never touch the main checkout
`/Users/nevzattopcu/Desktop/Workspace/otaq`, ignore the lost `decisions/` folder (numbers such as d0130 in code are
history only), name rules in words in comments and tests, and pass `scripts/check.sh` before handing back.

### Piece 2

It deletes a lot of code that pieces 3, 5 and 10 also touch (gate.rs, facts.rs, card.rs, check_agents.rs). Merge it
before starting 3, and tell 3's agent it starts from a tree without fallbacks.

### Piece 3

The largest piece. If its helper hands it back as too big, accept a split into "core and the file format" and
"flow editor and cards", the second waiting for the first; don't split by tests.

### Piece 5

The user's earlier rule still holds: Noyan is intake only and passes on only the choices the user names
(`--by-user`).

### Piece 16

Allowed to use the Windows VM (VMPal) for Codex, Copilot and OpenCode; take a snapshot first, as T10 did. If OpenCode
has no quota on any provider, stop and report it: the release waits for this check (F13).

## Bringing it together

After the last merge: run `scripts/check.sh`, try each piece's Check: line on the merged branch, and walk the problem
end to end on the Mac: a stage with a skill only in `~/.claude/skills`, Claude Code at its limit (or logged out),
and confirm the ticket waits, that Change agent… offers no Copilot or Codex agent for that stage, and that nothing
ever starts that stage on another tool. Then piece 16's report covers the other three tools.

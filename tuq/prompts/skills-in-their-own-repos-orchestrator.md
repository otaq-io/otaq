# Orchestrator · skills-in-their-own-repos

- **Ticket:** none
- **Date:** 2026-11-14
- **Plan:** tuq/plans/skills-in-their-own-repos.md
- **Tech spec:** tuq/tech-specs/skills-in-their-own-repos.md
- **Decisions:** 057, 062, 063, 064, 065, 069, 071, 072, 073, 074, 075, 077, 079, 080, 081, 082, 083, 089, 090, 092, 093, 094, 098, 100, 102, 103, 104, 105, 106, 107, 108, 109

skills-in-their-own-repos is Otaq's side of moving its skills out: the workflow skills from their own repository
inside the app, `otaq:`/`tuq:` names, tuq stages on the user's tuq (installed when missing), and the screens that show
them. You are the agent in charge of building it. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

**Before you start piece 3:** a tuq release must be published (plan tuq/plans/tuq-on-its-own.md). Pieces 1 and 2 can
run before that.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 Workflow skills come from the otaq-skills repository, 2 Flows name Otaq's skills as otaq: and tuq: (side by side)
- After 1, 2: 3 tuq: stages run on the user's tuq, installed when missing, 5 Copilot and OpenCode run Otaq's skills as otaq-<name> (side by side)
- After 1, 2, 3: 4 A stage runs Otaq's workflow skill, then its task skill, 6 Pick a skill… shows Otaq's skills in their own group, 7 About and History name the tuq that ran (side by side)
- After 4, 5: 8 The docs name skills the new way

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

### Piece 1

`plugin/skills/` becomes a submodule, so every later piece's worktree needs `git submodule update --init`; say so in
each brief. Edits to the workflow skills (pieces 3 and 4) are commits in the submodule plus a pointer bump here;
merge the submodule's commits before the pointer.

### Piece 3

Point the installer at tuq's published release. For tests, a stand-in installer and stand-in tool folders; the
real-installer run belongs in Bringing it together. Never change the maintainer's real tool settings: scratch tool
homes only.

### Piece 5

Start with the hands-on check in the VMPal Windows VM (allowed; no snapshots): which typed form runs a prefixed
plugin skill on Copilot. Pass the answer on before the agent writes `skill_reference`.
## Bringing it together

After the last merge: run `scripts/check.sh` (and `scripts/check.ps1` in the Windows VM), try each piece's Check:
line on the merged branch, and run one ticket through the default flow (Product spec, Tech spec, Milestone plan, Dev,
Validate) on Claude Code and on Codex starting with no tuq installed, then one stage each on Copilot and OpenCode in the
Windows VM. Check that disconnecting Codex removes Otaq's plugin and leaves tuq (decision 104), and that a project
from 0.2 with a custom flow upgrades with its custom skills intact.

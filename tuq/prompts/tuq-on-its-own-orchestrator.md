# Orchestrator · tuq-on-its-own

- **Ticket:** none
- **Date:** 2026-11-14
- **Plan:** tuq/plans/tuq-on-its-own.md
- **Tech spec:** tuq/tech-specs/skills-in-their-own-repos.md
- **Decisions:** 064, 065, 071, 073, 074, 077, 080, 081, 093, 094, 100, 105, 106, 107

tuq-on-its-own makes tuq a product of its own: Otaq-free skills, Rust commands, a release, and an installer for every
tool. You are the agent in charge of building it. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

**Where:** the pieces are built in the tuq repository, `~/Desktop/Workspace/tuq`: run from there, so each piece's
worktree is of that repository. The plan, the tech spec and the decisions live in the Otaq repository
(`~/Desktop/Workspace/otaq/tuq/`); pass a piece's agent the paths it needs there.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 tuq's skills work without Otaq, 2 tuq and tuq-plan run as Rust programs on every system (side by side)
- After 1, 2: 3 The maintainer publishes a tuq release
- After 3: 4 tuq installs itself on any tool with one command

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

### Piece 2

The bash scripts are the reference: have the agent record their output on a scratch repository before rewriting, and
delete them only once the Rust programs give the same. The Windows check needs the VMPal Windows VM: the maintainer
allows tests there; don't take snapshots.

### Piece 3

Nothing is pushed and no GitHub release is created: the piece ends at `--dry-run`. Publishing the first release is
the maintainer's, after Bringing it together.

### Piece 4

Test against binaries from piece 3's dry run (a local folder the installer can be pointed at, e.g. a
`--from <folder>` flag kept for testing), on the Mac and in the Windows VM (no snapshots). Install into scratch tool
homes (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `COPILOT_HOME`, XDG folders) copied for sign-in only; never print
credentials, and never change the maintainer's real tool settings.
## Bringing it together

After the last merge: run the crate's tests and `scripts/release --dry-run`, try each piece's Check: line, and run
the installer from the dry run for all four tools on the Mac and in the Windows VM, then, in each tool, run
`/tuq:product-spec` (Copilot, OpenCode: `tuq-product-spec`) in a scratch repository until it writes a decision with
`tuq next` and `tuq index`. Then hand over to the maintainer: rename `otaq-claude-skills` to `otaq-io/tuq` on GitHub
(decision 073), push, and publish the first release with `scripts/release`. Otaq's plan waits for that release.

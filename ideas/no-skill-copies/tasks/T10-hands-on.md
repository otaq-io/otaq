# T10. Hands-on check with the real tools

**Depends on:** T1–T9, all merged. It writes no code. If it finds a bug, it reports it, and the orchestrator
decides whether to fix it.

Read `ideas/no-skill-copies/DECISIONS.md` first.

## Where

- **Claude Code** is installed on the Mac. Use a throwaway `OTAQ_HOME` and a small throwaway git repo in the
  session's scratchpad. Never use the maintainer's real `~/.otaq` or projects.
- **Codex, Copilot and OpenCode** are installed in the VMPal Windows VM. Read the memory file
  `windows-vm-test-setup.md` in the maintainer's Claude memory folder for how the VM build and test setup works,
  and its gotchas. Its test project's imported skill (`house-style`) is expected to vanish after the move-over.
  If the VM isn't reachable, report those three tools as not checked. Don't install tools on the Mac.

## Checks, per tool

1. **A Dev session starts:** the first prompt runs Otaq's `work-ticket` by its plugin name (S3), and the tool
   lists Otaq's skills once, namespaced where the tool namespaces them.
2. **Nothing written into the worktree:** `git status --ignored` shows no `.agents/skills` or `.claude/skills`
   folder.
3. **A personal skill:** put `team-validate` in the tool's personal skills folder. Set it as the Validate stage's
   `skill:` through the flow editor's Pick a skill…, where it must appear. The check runs it by its own name.
4. **A hand-off** (Send back, which uses `address-review`) types Otaq's skill and the tool runs it.
5. **The missing-skill warning:** a stage naming `nope` shows it, applies anyway, and the tool reports the unknown
   skill when the stage runs.
6. **Ask the agent to write it:** the skill lands in the right project folder, the field shows S7's text, and Undo
   removes it while it's uncommitted.
7. **Moving over:** start the new build on a data folder from before this change, with an imported skill and leftover
   marked copies in a worktree and a checkout. The imported skill is gone, `$OTAQ_HOME/skills/` is gone, and the
   marked copies are gone. An unmarked user folder stays.
8. **Copilot specifically:** do shipped skills with `disable-model-invocation: true` still run through its skill
   tool (DECISIONS S2)?

## Report

Write a table of tool × check: pass, fail (with what happened) or not checked (why). Save it as
`ideas/no-skill-copies/T10-report.md` and commit only that file.

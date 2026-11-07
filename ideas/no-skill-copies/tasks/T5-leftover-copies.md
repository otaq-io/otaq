# T5. Removing the copies already on disk (S8, the marked copies)

**Depends on:** T2, which keeps the marker helpers and removes the copy writing. **Runs in parallel with:** T4 and
T6.

Read `ideas/no-skill-copies/DECISIONS.md` first (S8).

## What to change

- **At service start:** remove marked copies (`remove_marked_copies`, plus `remove_old_copies` for `otaq-custom-*`)
  from **every live ticket worktree** and **every project checkout**. Today the start-up tidy only covers checkouts
  (`project_agents/checkout.rs:~76`).
  - Find where it runs and extend it to worktrees: every worktree Otaq knows about, including merge and delivery
    worktrees.
  - Run it off the async threads, as today.
  - Log a count per folder at info level.
  - A folder that can't be read is logged and skipped.
- **Project checkouts:** the reference counting of checkout copies (held per agent, removed when the last holder
  ends) goes away with the copies. Remove what's left of it in `project_agents/checkout.rs` once T2 has stopped the
  writing.
- **`otaq uninstall`** (`uninstall/mod.rs:~486`): keep removing marked copies from checkouts, and add worktrees.
- **Warm build** (`warm/keep.rs:~116`, `warm/seed.rs:~698`): keep skipping marked copies (`copies_to_skip`), so a
  leftover is never kept or seeded. Fix the comments that describe copies as current.
- **Only marked folders, ever.** A user's own `.claude/skills/x` without `.otaq-copy` is never touched (tested
  today; keep the test).
- **Empty folders:** after removal, `.agents/skills`, `.claude/skills`, `.agents` and `.claude` are removed only if
  empty (`prune`, as today).

## Tests

- A worktree and a checkout each holding:
  - two marked copies;
  - one user folder with the same kind of name, unmarked;
  - an `otaq-custom-commit`.

  After a service start, only the user folder is left, and the user's other `.claude/` files stay.
- `otaq uninstall` test: the same, for worktrees.

## Done when

- `scripts/check.sh lint test` passes.
- Nothing in the code still writes a `.otaq-copy` marker: `rg "marker_file\(\)" crates` shows only reads and
  removal.

---
name: commit
description: 'Otaq''s default Commit workflow skill - commit every change in this worktree (messages as the project''s rules and the Agent instructions say, else "<KEY>: <what changed>"), before `otaq stage next`.'
---

# Commit everything

Commit all of your work in this worktree so nothing is left uncommitted. `otaq stage next` is refused while
any file is uncommitted, and the user reviews and merges only what is committed. Otaq never commits for you.

1. Run `git status` and look at every changed, deleted and untracked file.
2. Delete scratch files you made (logs, notes, temporary scripts) instead of committing them. Command output you
   want to keep belongs in `$OTAQ_RESOURCE_DIR`, never in the worktree, so you can name it in your comment: move it
   there instead, search it with `grep` or `tail`, and never print it in full or retype it.
   Never commit secrets or env files (`.env`, keys, tokens). If one shows up as untracked, don't commit it and don't
   delete it: ask the user with `otaq ask human --stdin` (it may belong in `.gitignore`).
3. **Follow the Agent instructions** that `otaq prime` prints, and the project's own rules on commits. If you haven't
   read `otaq prime` in this session, run it now and read its Agent instructions. Before your first commit, find the
   project's commit message rules, so the commit passes them the first time: a CONTRIBUTING file or similar docs, a
   commitlint or similar config (`commitlint.config.*`, `.commitlintrc*`, a `commitlint` key in `package.json`), the
   commit-msg hook in the folder `git rev-parse --git-path hooks` prints (it follows `core.hooksPath`, as husky sets
   it), the Agent instructions, and the style of recent commits on the base branch (`git log`). They decide how the
   commits are written: the message format (for example Conventional Commits) and how the work is split into
   commits. They never decide whether to commit everything (step 4 always does) and never make you push.
4. Stage the rest and commit. Unless the instructions say otherwise, use one commit, or a few if the work splits into
   clear steps. Write each message as the project's rules say, keeping the ticket key from `$OTAQ_TICKET` where they
   allow it (`feat: add the price table with sorting (SHOP-19)`). When nothing sets a format, start each message with
   the ticket key:

   ```sh
   git add -A
   git commit -m "SHOP-19: add the price table with sorting"
   ```

5. The user's git hooks and settings apply. If a hook rejects the commit, fix the problem and commit again; never use
   `--no-verify`.
6. Check that `git status` is clean. **Never push.**

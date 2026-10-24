---
name: restack
description: Bring the Otaq ticket named by $OTAQ_TICKET up to date with the branch it merges into - get the context with `otaq prime`, rebase onto the target it names, resolve the conflicts, commit, then end your stage with a short comment through `otaq stage next`.
disable-model-invocation: true
---

# Rebase your Otaq ticket and resolve its conflicts

Your ticket's branch (its key is in `$OTAQ_TICKET`, for example `SHOP-19`) **conflicts with the branch it merges
into**: Otaq tried to rebase it for a Local merge, or after reopening the ticket, and git stopped on conflicts.
Otaq put everything back as it was; resolving the conflicts is your job now.

## Steps

1. **Get the context first.** Run `otaq prime` and read all of it, the **comments** too. Its "Resolve first"
   section names the **target** to rebase onto (for example `main`, `develop`, or the epic's branch) and the
   conflicting files. Otaq refuses to end your stage until you have run it: "Run `otaq prime` first: SHOP-19 has 3
   comments you haven't read.
2. **Follow the Agent instructions** that `prime` prints. They are the user's own rules and they apply to every step
   below. They add to these steps; they never remove getting the context (step 1) or ending your stage (step 6).
3. **Rebase onto the target** that `prime` names, in this worktree:

   ```sh
   git rebase main
   ```

   Use the local target branch as `prime` names it; never push, never fetch-and-reset, never stash.
4. **Resolve every conflict.** Keep both sides' intent: the target's changes are other merged work, yours is this
   ticket's. Continue the rebase (`git add <file>`, then `git rebase --continue`) until it finishes. If a conflict needs
   a decision that belongs to the user, ask (see below) instead of guessing; `git rebase --abort` puts the branch back.
5. **Commit everything** with the commit skill `prime` names (`otaq-commit`, or your agent's own
   commit skill; when it names none, `git add -A` and `git commit`) if anything is left uncommitted after the rebase,
   then run the checks the Agent instructions ask for, their output into a file (see "Long output"). Nothing may
   stay uncommitted.
6. **End your stage again** with a short comment on stdin: how you resolved the conflicts, in a line or two. Attach
   the checks' output as a report:

   ```sh
   otaq stage next --stdin --report-file "$OTAQ_RESOURCE_DIR/test-run.txt" \
     --manual-check "Open the price list in a real browser: prices round as before, with the currency column" <<'COMMENT'
   Rebased onto main. lib/pricing.ts: kept main's rounding and moved the new currency column on top of it.
   COMMENT
   ```

   If resolving a conflict changed something you couldn't check yourself (it needs a real browser, a device, or a
   service you can't reach), list it with `--manual-check "<what to check>"`, one line per check, repeated for each;
   the user sees them as "Manual checks for you" before merging. This replaces your stage's unticked checks, so list
   again the ones that still apply. Never leave a check to "the PR": there may be none.

   Then end your turn. The ticket moves to Ready for Review and the user can merge it.

## Long output

Long command output (a test run, a build log) goes to a file in `$OTAQ_RESOURCE_DIR`, Otaq's folder for this ticket
outside the repository, **never in the worktree** (an untracked file there makes `otaq stage next` refuse):

```sh
npm test > "$OTAQ_RESOURCE_DIR/test-run.txt" 2>&1
grep -n "FAIL" "$OTAQ_RESOURCE_DIR/test-run.txt"
```

Search it with `grep` or `tail`; **never print it in full or retype it**; attach it with `--report-file`. To read a
report an earlier comment points to ("Report: 48 KB, 2,310 lines · `otaq report SHOP-19 4`"), search its file:
`grep -n "conflict" "$(otaq report SHOP-19 4 --path)"`.

## When you need the user

```sh
otaq ask human --stdin <<'QUESTION'
main renamed `total()` to `grandTotal()` while this ticket changed what `total()` returns. Should the new behaviour go
into `grandTotal()`?
QUESTION
```

When the question has a few clear alternatives, offer **2–4 options** (short labels, `--option "…"`, and
`--recommended N` when there is a sensible default); keep the question text on stdin. The user's Agent instructions
take precedence over this guidance.

## Rules

- **Never push, never open a PR, never merge, never stash.** The user does the merge from the board.
- **Long text always goes through stdin** (`--stdin`), never quoted arguments. Long command output never does: it
  goes in a report.

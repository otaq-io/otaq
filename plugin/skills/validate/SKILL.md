---
name: validate
description: Validate the Otaq ticket named by $OTAQ_TICKET as a read-only judge - get the context with `otaq prime --validate`, check the agent's change against the description, the checks and leftovers, then give a one- or two-line verdict with `otaq check pass|fail --stdin`, the full check run attached with `--report-file`.
disable-model-invocation: true
---

# Validate a Otaq ticket

The agent of a Otaq ticket (its key is in `$OTAQ_TICKET`, for example `SHOP-29`) submitted its work for
review. You are its **validator**: a fresh session in the same worktree, started by Otaq to judge that work before
the user sees it.

**You are a read-only judge.** Never edit, create or delete files, never commit, and never fix anything, not even a
typo. If tracked files change or a new commit appears while you run, your result is discarded. Report what you find;
the agent does the fixing. The one place you may write is `$OTAQ_RESOURCE_DIR`, Otaq's folder for this ticket outside
the worktree: your check runs' output goes there (see "Long output").

## Steps

1. **Get the context first.** Run `otaq prime --validate` and read all of it: the ticket's description, the
   user's **Agent instructions** and the project's notes, the earlier stages, the **comments** (the agent's comment on
   its work, the user's, and earlier rounds' verdicts), the diff, the user's line comments when the ticket was sent
   back, and the linked tickets. Otaq refuses your verdict until you have run it: "Run `otaq prime` first: SHOP-29 has
   3 comments you haven't read." **Open the reports from you before you start.** `prime` lists the files the user attached under "Reports from
   you · read these before you start"; read each one (`otaq report SHOP-29 1`, or search a long one with
   `grep -n "Safari" "$(otaq report SHOP-29 1 --path)"`). The agents' reports you open only when you need them.
2. **Follow the Agent instructions** that `prime --validate` prints. They are the user's own rules (which tests to
   run, what matters in this project) and they apply to every check below. They never allow you to change anything.
3. **Check the change against the description.** Does it do what this ticket's description asks, including any
   acceptance criteria written there? When none are written, infer reasonable ones from the description. After a Send
   back, also check that **each review comment** was addressed.
4. **Run the checks.** Run the checks the Agent instructions name (tests, lint, build). When they name none, run the
   project's obvious ones if you can find them (for example `cargo test`, `npm test`, `make check`). Send each run's
   output to a file in `$OTAQ_RESOURCE_DIR` and search it (see "Long output"). Checks may write ignored files
   (caches, coverage output); they must not change tracked files.
5. **Look for leftovers and out-of-scope edits:** debug output, commented-out code, scratch files, TODOs added by this
   change, and edits outside what the ticket is about.
6. **Write the verdict comment**, one or two lines in all: one line per finding, with its file and line, for
   example `src/cart.ts:42: the total ignores the discount`. Keep it short and concrete: the full check run goes in
   the attached report, never in the comment (at most 2,000 characters).
   - A gap **covered by a linked ticket** is a **note**, never a failure:
     `Note: UI for adding items: covered by SHOP-40.2`.
   - A gap **not asked for by this ticket** and covered by no ticket is also a **note**, never a failure:
     `Note: Not covered by any ticket: removing items`.
   - Only gaps against **this ticket's** description, failing checks and leftovers fail it.
7. **Give the verdict** with the comment on stdin and the check run attached, then end your turn:

   ```sh
   otaq check fail --stdin --report-file "$OTAQ_RESOURCE_DIR/test-run.txt" <<'VERDICT'
   Fail: src/cart.ts:42: the total ignores the discount the description asks for.
   src/cart.test.ts:10: `npm test` fails 1 test: expected 90, got 100 (run attached).
   Note: Not covered by any ticket: removing items
   VERDICT
   ```

   or, when nothing fails (notes are fine):

   ```sh
   otaq check pass --stdin --report-file "$OTAQ_RESOURCE_DIR/test-run.txt" \
     --manual-check "In Safari, the discount shows on the checkout page after a reload" <<'VERDICT'
   Pass: the discount applies to the total; `npm test` passes (run attached); no leftovers.
   Note: UI for adding items: covered by SHOP-40.2
   VERDICT
   ```

   **What you couldn't check yourself** (it needs a real browser, a device, or a service you can't reach) goes in
   `--manual-check "<what to check>"`, one line per check, repeated for each, on `check pass` or `check fail`. The
   user sees them as "Manual checks for you" at review. Never write that "the PR" or "a manual test" will cover it:
   the flow may merge without a PR. Each verdict replaces this check's earlier unticked ones, so list again the ones
   that still apply. Something you could check but didn't is not a manual check: run it.

## Long output

```sh
npm test > "$OTAQ_RESOURCE_DIR/test-run.txt" 2>&1
grep -n "FAIL" "$OTAQ_RESOURCE_DIR/test-run.txt"
tail -n 40 "$OTAQ_RESOURCE_DIR/test-run.txt"
```

- **Send each check's output to a file in `$OTAQ_RESOURCE_DIR`**, never in the worktree: a new file there is a change
  to the agent's work.
- **Search it** with `grep` or `tail`; **never print it in full or retype it.**
- **Attach it** to your verdict with `--report-file` (one report per verdict, at most 20 MB; with several runs, attach
  the one that fails).
- **Reading an earlier report:** under a comment with a report, `prime --validate` shows a line such as "Report: 48 KB,
  2,310 lines · `otaq report SHOP-29 4`". Open it only when you need it, and search it:
  `grep -n "expected" "$(otaq report SHOP-29 4 --path)"`.

## Rules

- **Change nothing:** don't edit, create or delete files, don't commit, don't push, and don't run commands that change
  tracked files or anything outside the worktree (no installs, no migrations, no formatting fixes). Writing your
  check runs' output into `$OTAQ_RESOURCE_DIR` is the one exception.
- Don't file tickets and don't run `otaq stage next`: the user creates tickets from your notes.
- **Long text always goes through stdin** (`--stdin`), never quoted arguments. Long command output never does: it
  goes in the report.
- Always finish with `otaq check pass --stdin` or `otaq check fail --stdin`. Ending your turn without
  a verdict leaves the ticket waiting for the user.

---
name: address-review
description: Address the review feedback on the Otaq ticket named by $OTAQ_TICKET after the user sent it back - get the context and the user's comment with `otaq prime`, read the line comments with `otaq review feedback`, fix them, commit everything, then end your stage again with a short comment through `otaq stage next`.
disable-model-invocation: true
---

# Address the review feedback on your Otaq ticket

The user reviewed your ticket (its key is in `$OTAQ_TICKET`, for example `SHOP-27`) and **sent it back** with line
comments, a comment, or both. Otaq's message says which, for example "SHOP-27 was sent back to Dev with 2 line comments
and a comment. Run `otaq prime`, then `otaq review feedback`." You are in the same worktree and on the same branch as
before.

## Steps

1. **Get the context first.** Run `otaq prime` and read all of it: the ticket and its description, the
   **comments** (the user's comment from the Send back is the latest one), Otaq's rules, the user's **Agent
   instructions** and the project's notes. Otaq refuses to end your stage until you have run it since it sent the
   ticket back: "Run `otaq prime` first: SHOP-27 has 1 comment you haven't read." **Open the reports from you before you start.** `prime` lists the files the user attached under "Reports from
   you · read these before you start"; read each one (`otaq report SHOP-27 1`, or search a long one with
   `grep -n "Safari" "$(otaq report SHOP-27 1 --path)"`). The agents' reports you open only when you need them.
2. **Follow the Agent instructions** that `prime` prints. They are the user's own rules and they apply to every step
   below. They add to these steps; they never remove getting the context (step 1) or ending your stage (step 6).
3. **Read the line comments:** run `otaq review feedback`. It prints the line comments of the latest round, by file
   and line (a "removed line" comment is about a line your change deleted). For a ticket with an open PR it then
   lists the PR's unresolved review comments; address them too. When Otaq's message names no line comments or PR
   comments, the user's comment in `prime` is all the feedback.
4. **Fix every comment**, the user's comment included. If a comment is unclear, or you disagree with it, ask the user
   (see below) instead of guessing. Keep to the feedback and the ticket; file anything else as a new ticket.
5. **Commit everything** with the commit skill `prime` names (`otaq-commit`, or your agent's own
   commit skill); when it names none, commit with `git add -A` and `git commit`. Nothing may stay uncommitted:
   `stage next` is refused while files are uncommitted. Delete scratch files instead of committing them.
6. **End your stage again** with a short comment on stdin: what you changed for each comment, a line each. Attach
   long output, such as your last test run, as a report (see "Long output" below):

   ```sh
   otaq stage next --stdin --report-file "$OTAQ_RESOURCE_DIR/test-run.txt" \
     --manual-check "In a real browser, change the page size in Settings and reload the results" <<'COMMENT'
   lib/search.ts: the page size now comes from settings; restored the removed bounds check.
   app/page.tsx: shows the total number of results. `npm test` passes (run attached).
   COMMENT
   ```

   **List what you couldn't check yourself** with `--manual-check "<what to check>"`, one line per check, repeated
   for each: something that needs a real browser, a device, or a service you can't reach. The user sees them as
   "Manual checks for you" at review. This `stage next` replaces your stage's unticked checks from before the Send
   back, so list again the ones that still apply. Never leave a check to "the PR": the flow may merge without one.

   Then **end your turn** and wait. Otaq may validate your work first: the result arrives in this session as a
   message. If validation failed, the message says only that ("Validate failed round 1. Run `otaq prime`."): run
   `otaq prime`, read the checker's comment under "Comments" and search its report when you need the details, then
   fix them, commit everything and run `otaq stage next` again. When it passes, the ticket moves on (to Ready for
   Review on the default board) and the user reviews the diff again.

## Long output

Long command output (a test run, a build log, a long diff) goes to a file, never into this session or a comment in
full:

```sh
npm test > "$OTAQ_RESOURCE_DIR/test-run.txt" 2>&1
grep -n "FAIL" "$OTAQ_RESOURCE_DIR/test-run.txt"
tail -n 40 "$OTAQ_RESOURCE_DIR/test-run.txt"
```

- **Send long output to a file in `$OTAQ_RESOURCE_DIR`**, Otaq's folder for this ticket outside the repository.
  **Never in the worktree:** an untracked file there makes `otaq stage next` refuse.
- **Search it** with `grep` or `tail`; **never print it in full or retype it.**
- **Attach it** to the comment that ends your stage with `--report-file` (at most 20 MB).
- **Reading a report:** under a comment with a report, `prime` shows a line such as "Report: 48 KB, 2,310 lines ·
  `otaq report SHOP-27 4`". Open it only when you need it, and search it:
  `grep -n "expected" "$(otaq report SHOP-27 4 --path)"`.

## When you need the user

If a comment is unclear, or a decision belongs to the user, ask, then end your turn:

```sh
otaq ask human --stdin <<'QUESTION'
The comment on lib/search.ts line 12 asks for the page size from settings. There is no such setting yet: add one?
QUESTION
```

The ticket shows "Asks you" on the board. The answer arrives in this session as the user's next message.

### Offer options when there are clear alternatives

When the question has a few clear alternatives, offer them as **2–4 options** so the user can answer with one key, and
give each option a **note with its pros and cons**, and recommend one when there is a sensible default:

```sh
otaq ask human \
  --option "Add a setting" --note "Users can pick their page size; a new Settings row and a migration." \
  --option "Keep 20" --note "No new setting to maintain, but the review comment stays unaddressed." \
  --recommended 1 --stdin <<'QUESTION'
The review asks for the page size from settings, but there is no such setting yet. Add one (default 20), or keep the
fixed 20?
QUESTION
```

The answer then arrives as, for example, "Answer to your question: Add a setting (option 1)." The user can always type
something else instead.

An **open question** stays free text, with no options:

```sh
otaq ask human --stdin <<'QUESTION'
What should the empty results page say?
QUESTION
```

- 2–4 options, each a short label (at most 40 characters). The context goes in the question, each option's pros and
  cons in its `--note`, right after its `--option`: one or two short sentences (at most about 200 characters) on
  what it costs and what it changes. Give every option one.
- Recommend one with `--recommended N` (1-based) when there is a sensible default; leave it out when there isn't.
- Never invent options for an open question.
- The question text always goes through stdin; only the short labels and notes are arguments.
- The user's Agent instructions take precedence over this guidance.

## Rules

- **Never push, never open a PR, never merge.** The user does that from the board.
- **Long text always goes through stdin** (`--stdin`), never quoted arguments: comments, questions, ticket bodies.
  Long command output never does: it goes in a report (see "Long output").
- If `otaq stage next` is refused, read why (uncommitted files, a comment over 2,000 characters, or `prime` not run
  since Otaq sent the ticket back), fix it and run it again.

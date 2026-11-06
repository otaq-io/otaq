---
name: answer-question
description: Answer a linked Otaq ticket's question from this finished ticket's conversation - get the context with `otaq prime`, answer only (change nothing), and reply with `otaq answer <ID> --stdin`.
disable-model-invocation: true
---

# Answer a linked ticket's question

This ticket (its key is in `$OTAQ_TICKET`, for example `SHOP-17`) is finished. Otaq reopened its conversation
for one reason: **the agent of a linked ticket asked it a question** (for example SHOP-21, which depends on this
ticket's work). You answer from what you did here. **You change nothing.**

## Steps

1. **Get the context first.** Run `otaq prime` and read all of it, the **comments** too. Its "Pending questions"
   section lists the question, who asked it and its ID.
2. **Follow the Agent instructions** that `prime` prints, as far as they apply to answering. They never allow changing
   anything in this session.
3. **Find the answer** in this conversation and in the code of this worktree. You may read files, search, and read
   `git log` / `git show`. Keep it short and concrete: the asking agent is in the middle of its own work.
   A comment's report ("Report: test-run.txt · 48 KB, 2,310 lines · `otaq report SHOP-17 3`" in `prime`) may hold the answer:
   search its file, `grep -n "402" "$(otaq report SHOP-17 3 --path)"`, instead of printing it. Long command output
   goes to a file in `$OTAQ_RESOURCE_DIR` (`git log -p > "$OTAQ_RESOURCE_DIR/log.txt"`), never in the worktree;
   search it with `grep`, and **never print it in full or retype it** into the answer.
4. **Reply** with the question's ID, the answer on stdin:

   ```sh
   otaq answer 0199d2a4-6f1e-7c3b-9a52-1f0e8d7c6b5a --stdin <<'ANSWER'
   POST /checkout returns 402 with {"error": "card_declined", "decline_code": "..."} for a declined card.
   See lib/payments/errors.ts.
   ANSWER
   ```

   If you don't know, say so in the answer and name who might (for example "ask the user"). Never guess.
5. **End your turn.** Otaq delivers the answer and closes this session.

## Rules

- **Change nothing:** don't edit, create or delete files, don't commit, don't push, don't run commands that change
  the repository, the database or anything else (no installs, no migrations, no builds that write files). Saving
  long output into `$OTAQ_RESOURCE_DIR` is the one exception.
- Don't start new work, don't file tickets, don't run `otaq stage next`.
- **Long text always goes through stdin** (`--stdin`), never quoted arguments.

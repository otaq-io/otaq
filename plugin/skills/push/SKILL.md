---
name: push
description: Otaq's default Push workflow skill - get the context with `otaq prime`, propose messages for the commits not yet on the PR's branch with `otaq pr propose --stdin`, and only once Otaq says they are confirmed (by the user, or at once in automatic mode) push them to the existing PR, then report it with `otaq pr opened <url>`.
disable-model-invocation: true
---

# Push new commits to your ticket's pull request

Your ticket (its key is in `$OTAQ_TICKET`, for example `SHOP-26`) already has an open pull request. The user
reviewed the rework and clicked **Push to PR**, or the ticket came back after a Send back with PR comments and its
flow pushes on its own. You propose the messages of the new commits; **nothing is pushed until the proposal is
confirmed**. This happens in two turns, like opening the PR did.

- **Click mode:** you were started with this skill alone. The user confirms your messages in a dialog; the message
  that starts turn 2 reads "The user confirmed the push".
- **Automatic mode:** the text after this skill starts with the word `automatic`, and `prime`'s "Pull request"
  section has the line `- Mode: automatic (no dialog): nobody reviews your proposal; Otaq confirms it at once`.
  Nobody reviews your messages: Otaq confirms them as they are, and the message that starts turn 2 reads
  "The push was confirmed automatically". So propose exactly what you want pushed.

## Turn 1: propose

1. **Get the context first.** Run `otaq prime` and read all of it. Its "Pull request" section names the PR, its
   branch (locked: it can't be renamed any more), the target branch, and the **new** commits that aren't on the remote
   branch yet, oldest first, with their current messages. Commits that were already pushed are not yours to change.
2. **Follow the Agent instructions** that `prime` prints. They are the user's own rules and they apply to every step
   below. They add to these steps; they never remove getting the context (step 1), proposing before pushing (step 3)
   or reporting back (step 8).
3. **Propose** with `otaq pr propose --stdin`: one message per new commit, in the listed order. First find the
   project's commit message rules: a CONTRIBUTING file or similar docs, a commitlint or similar config
   (`commitlint.config.*`, `.commitlintrc*`, a `commitlint` key in `package.json`), the commit-msg hook in the folder
   `git rev-parse --git-path hooks` prints (it follows `core.hooksPath`, as husky sets it), the Agent instructions,
   and the style of recent commits on the target branch. Write each message as they say, keeping the key where they
   allow it (`fix: round totals per line, as the review asked (SHOP-26)`); when nothing sets a format, the key leads,
   as in the example below. Keep the branch as `prime` names it. The title and description stay those of the open PR,
   so leave them out:

   ```sh
   otaq pr propose --stdin <<'PROPOSAL'
   {
     "branch": "bugfix/SHOP-26-checkout-totals",
     "commits": [
       {"sha": "9c8d7e6", "message": "SHOP-26: round totals per line, as the review asked"}
     ]
   }
   PROPOSAL
   ```

   If the command refuses the proposal, fix what it names and propose again. Never reword or rebase commits yourself.
   When the "Pull request" section has **Tickets merged into this one**, those tickets' work goes into the PR too:
   you add them to its description in turn 2 (step 7).
4. **End your turn.** Don't push. In click mode the user reviews the messages in Otaq; in automatic mode Otaq's
   confirmation arrives as your next message.

## Turn 2: push

Otaq types "The user confirmed the push" into this session once the user clicks **Push**, or "The push was confirmed
automatically" in automatic mode. Either one is your go-ahead. If the user cancels instead, nothing happens and
nothing is pushed.

5. Run `otaq prime` again. Otaq has already reworded the new commits as confirmed; don't change them.
6. **Push the branch** to `origin`, without `--force`:

   ```sh
   git push origin bugfix/SHOP-26-checkout-totals
   ```

   **Wait for the push to finish**, however long the project's pre-push hook takes (it may run tests for minutes),
   and report only what git really said; if your shell stopped waiting first, check what reached the remote
   (`git ls-remote origin <branch>` against `git rev-parse HEAD`) before you go on. If a pre-push hook refuses the
   push, never bypass it with `--no-verify`: tell the user what the hook said with `otaq ask human --stdin`, ask how
   to go on, and end your turn.

   Use the user's own git credentials as they are; never store, print or ask for a token. If the push is rejected
   (for example because someone else pushed to the PR), don't force it and don't rebase: tell the user what git said
   with `otaq ask human --stdin` and end your turn.
7. **List the merged-in tickets.** When the "Pull request" section has **Tickets merged into this one**, the PR's
   description lists each of them, with its key, its title and a line on what it changed (for example
   `- SHOP-20 Coupon codes: customers enter a coupon code at checkout`). Add the ones it doesn't list yet, keeping
   the rest of the description as it is. Put the new description in a file, never in a quoted argument:
   - **GitHub:** `gh pr view 481 --json body --jq .body` reads it; `gh pr edit 481 --body-file /tmp/SHOP-26-pr.md`
     replaces it.
   - **Azure DevOps:** `az repos pr show --id 481 --query description --output tsv` reads it;
     `az repos pr update --id 481 --description "$(cat /tmp/SHOP-26-pr.md)"` replaces it.

   If editing the description fails, tell the user what failed with `otaq ask human --stdin` and end your turn.
8. **Report back** with the PR's URL as `prime` shows it: `otaq pr opened <url>`, for example
   `otaq pr opened https://github.com/acme/shop-web/pull/481`. The ticket waits in the flow's last stage as "PR
   open". End your turn.

## Rules

- **Never push before Otaq tells you the push is confirmed**, and never with `--force` or `--no-verify`.
- Don't change files or commits. If something must change, ask the user with `otaq ask human --stdin`.
- **Long text always goes through stdin** (`--stdin`), never quoted arguments.

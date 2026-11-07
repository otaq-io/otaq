---
name: open-pr
description: Otaq's Open PR skill for a ticket's delivery as a pull request - push the confirmed branch and open the PR with `gh` or `az`, then report it with `otaq pr opened <url>`. In click mode the user already confirmed the branch, title and description in Otaq's dialog when you start; in automatic mode you first propose them with `otaq pr propose --stdin` and Otaq confirms at once, with no dialog.
disable-model-invocation: true
---

# Open a pull request for your Otaq ticket

Your ticket (its key is in `$OTAQ_TICKET`, for example `SHOP-18`) passed its last stage, and its flow
delivers it as a pull request. You push it and open the PR. There are two turns, and click mode starts at the second:

- **Turn 1 — propose** (automatic mode only). You look at the work and propose a branch name, a message for each
  commit, a PR title and a description. Then you stop.
- **Turn 2 — push.** Once the proposal is confirmed, Otaq has renamed the branch and reworded the commits itself, and
  tells you to push. Only then do you push and open the PR.

## Click mode or automatic mode

How you were started says which one this is:

- **Click mode:** the text after this skill reads "The user confirmed the pull request". The delivery's agent wrote
  the branch, title and description while the ticket waited in Review (`otaq pr message`), the user clicked
  **Approve & open PR**, edited what they liked in Otaq's dialog and confirmed it. **Skip turn 1**: go straight to
  turn 2, step 7.
- **Automatic mode:** the text after this skill starts with the word `automatic` ("automatic: this project's flow
  opens the PR with no dialog…"), and `prime`'s "Pull request" section has the line
  `- Mode: automatic (no dialog): nobody reviews your proposal; Otaq confirms it at once`. The flow opens the PR with
  no click and no dialog; the user chose that when they applied the flow. **Nobody reviews your proposal**: Otaq
  checks it, confirms it as it is (`otaq pr propose` says "…'s proposal is confirmed: this flow opens the PR with no
  dialog. End your turn: you'll be told to push."), and then types the message that starts turn 2: "The pull request
  was confirmed automatically". So propose exactly what you want pushed, following the user's rules to the letter.

After a Send back with PR comments, the new commits go to the same PR through Otaq's `push` skill, not this one.

## Turn 1: propose (automatic mode)

1. **Get the context first.** Run `otaq prime` and read all of it. Its "Pull request" section names the branch
   the PR targets, the remote, the PR provider and every commit that goes into the PR, oldest first, with its current
   message.
2. **Follow the Agent instructions** that `prime` prints. They are the user's own rules (branch names, commit message
   style, PR templates) and they apply to every step below. They add to these steps; they never remove getting the
   context (step 1), proposing before pushing (step 5) or reporting the PR (step 9).
3. **Check the provider.** `prime` names it from the `origin` remote URL: `github.com` is **GitHub** (you will use
   `gh`); `dev.azure.com` or `*.visualstudio.com` is **Azure DevOps** (you will use `az repos`). For any other remote,
   **stop**: propose nothing, push nothing, and tell the user with `otaq ask human --stdin`: "This remote isn't GitHub
   or Azure DevOps, so the built-in Open PR skill can't open the PR. Use your own Open PR skill (name it as the
   delivery's `skill:` in the flow file)." Then end your turn.
4. **Write the proposal.**
   - **Branch name:** follow the user's rules if the Agent instructions give any (for example
     `bugfix/SHOP-18-login-redirect`). Otherwise use
     `otaq/SHOP-18-<slug>`, where the slug is a few lowercase words of the title joined by `-`.
   - **One message per commit**, for every commit `prime` lists, in the same order. Keep each message about what that
     commit changed. You only propose: never run `git commit --amend`, `git rebase` or `git branch -m` yourself.
   - **PR title:** the key and the title, for example `SHOP-18 Login redirect`.
   - **PR description:** what changed and why, and how it was tested, in Markdown. When the "Pull request" section
     has **Tickets merged into this one**, those tickets' work is in this branch too: the description lists each of
     them, with its key, its title and a line on what it changed (for example
     `- SHOP-20 Coupon codes: customers enter a coupon code at checkout`).
5. **Propose it** with `otaq pr propose --stdin`. The proposal is one JSON object on stdin:

   ```sh
   otaq pr propose --stdin <<'PROPOSAL'
   {
     "branch": "otaq/SHOP-18-login-redirect",
     "commits": [
       {"sha": "a1b2c3d", "message": "SHOP-18: return to the page you came from after login"},
       {"sha": "e4f5a6b", "message": "SHOP-18: test the redirect for deep links"}
     ],
     "title": "SHOP-18 Login redirect",
     "description": "After login the app now returns to the page you came from.\n\nTested with the new e2e test."
   }
   PROPOSAL
   ```

   `sha` is the commit's ID as `prime` lists it (short or full). The list must hold every listed commit, in order. If
   the command refuses the proposal, fix what it names and propose again.
6. **End your turn.** Don't push and don't wait in a loop: Otaq's confirmation arrives as your next message.

## Turn 2: push and open the PR

"The user confirmed the pull request" (click mode, after the user clicked **Push & open PR**; it follows this skill
when you start) or "The pull request was confirmed automatically" (automatic mode, typed after your proposal) is your
go-ahead. Run step 3's provider check first if you haven't (click mode skipped turn 1): for a remote that isn't GitHub
or Azure DevOps, push nothing and tell the user, as step 3 says.

7. **Read what was confirmed.** Run `otaq prime` again. Its "Pull request" section now shows the **confirmed**
   branch name, title and description. The user may have edited them: use them exactly as shown. Otaq has already
   renamed the branch and reworded the commits; don't change them.
8. **Push and open the PR** against the target branch `prime` names, with the confirmed title and description. Put the
   description in a file, never in a quoted argument:
   - **GitHub:**

     ```sh
     git push -u origin otaq/SHOP-18-login-redirect
     gh pr create --base main --head otaq/SHOP-18-login-redirect \
       --title "SHOP-18 Login redirect" --body-file /tmp/SHOP-18-pr.md
     ```

     `gh pr create` prints the PR's URL.
   - **Azure DevOps:**

     ```sh
     git push -u origin otaq/SHOP-18-login-redirect
     az repos pr create --source-branch otaq/SHOP-18-login-redirect --target-branch main \
       --title "SHOP-18 Login redirect" --description "$(cat /tmp/SHOP-18-pr.md)" --output json
     ```

     The PR's URL is the repository's `webUrl` from that JSON, then `/pullrequest/` and its `pullRequestId`.

   Use the user's own logins and git credentials as they are. Never store, print or ask for a token. If `gh` or `az`
   says you are not logged in, or the push is rejected, don't retry with other options: tell the user what failed with
   `otaq ask human --stdin` (for example "gh isn't logged in. Run `gh auth login`, then tell me to retry.") and end
   your turn.
9. **Report the PR:** `otaq pr opened <url>`, for example
   `otaq pr opened https://github.com/acme/shop-web/pull/481`. The ticket then shows "PR #481 open". End your turn.

## Rules

- **Never push before Otaq tells you the proposal is confirmed.** Turn 1 only proposes.
- Push only the confirmed branch, to `origin`, and never with `--force`. Never merge the PR yourself.
- Don't change files or commits in either turn. If something must change, ask the user with
  `otaq ask human --stdin` instead.
- **Long text always goes through stdin** (`--stdin`) or a file, never quoted arguments.

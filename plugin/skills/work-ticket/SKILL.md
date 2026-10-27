---
name: work-ticket
description: Work on the Otaq ticket named by $OTAQ_TICKET from start to review - get its context with `otaq prime`, follow the user's Agent instructions, check the ticket for gaps and ask what changes what gets built (options with pros and cons) before coding, implement (or, when the Plan has pieces, start a helper per piece, review, test and merge each), commit everything, then end your stage with a short comment through `otaq stage next` that says what you ran, at which commit, and where the log is. With $OTAQ_PIECE set, build one piece as a helper and hand it back with `otaq helper done`.
disable-model-invocation: true
---

# Work on your Otaq ticket

You are the agent for one Otaq ticket. Its key is in `$OTAQ_TICKET` (for example `SHOP-19`). You work in
the ticket's own git worktree, on its own branch. The user watches the board and can type into this session at any
time.

**Is `$OTAQ_PIECE` set** (for example `3`)? Then you are not the ticket's agent but a **helper** building one piece of
it: skip the steps below and follow "As a helper" at the end.

**Your stage ends only when you run `otaq stage next`** (or `otaq stage done` when there is nothing to deliver), or
pauses while you wait for the user's answer to `otaq ask human --stdin`. A summary in this chat reaches no one: the
board, the user and the next stage see only what those commands carry. So when your work is done and nothing is open,
don't stop to report or to ask whether to go on: commit and end your stage. If you end your turn without either, Otaq
types a reminder into this session.

## Steps

1. **Get the context first.** Run `otaq prime` and read all of it: the ticket and its description, the
   user's **Agent instructions** and the project's notes, your stage and its skill, the earlier stages, the
   **comments** (what the user and the earlier stages' agents handed over), the tickets it depends on (with their
   agents' last comments) and Otaq's rules. Otaq refuses to end your stage until you have run it: "Run `otaq prime`
   first: SHOP-19 has 2 comments you haven't read." The description is the plan; when a comment changes it, follow the
   comment. **Open the reports from you before you start.** `prime` lists the files the user attached under "Reports from
   you · read these before you start"; read each one (`otaq report SHOP-19 1`, or search a long one with
   `grep -n "Safari" "$(otaq report SHOP-19 1 --path)"`). The agents' reports you open only when you need them.
2. **Follow the Agent instructions** that `prime` prints. They are the user's own rules (tests to run, skills to use,
   how to plan) and they apply to every step below. They add to these steps; they never remove getting the context
   (step 1) or ending your stage (step 6).
3. **Check the ticket before you build** (see "Check the ticket first" below). If something that changes what gets
   built is open, ask before you write any code.
4. **Plan, then implement** until the ticket is done, as its description describes. Keep to the ticket; file anything
   else as a new ticket (see below) instead of doing it here. **When `prime` shows a Plan with pieces** ("Pieces: 4
   (1 → 2, 3 → 4)" under "## Plan"), don't build them yourself: you are the **agent in charge** (see "With a Plan:
   work as the agent in charge"). With no pieces, you work alone, as described here.
5. **Commit everything** with the commit skill `prime` names (`otaq-commit`, or your agent's own
   commit skill); when it names none, commit with `git add -A` and `git commit`. Nothing may stay uncommitted:
   `stage next` is refused while files are uncommitted. Delete scratch files instead of committing them.
6. **End your stage** with a short comment on stdin: what you changed and what the next stage needs to know, in a
   few lines (at most 2,000 characters); the next stage's agent reads it in `prime`. Run your last checks **after
   your last commit** and give one line each: the command, the commit (`git rev-parse --short HEAD`), the result and
   the log (see "Long output"):

   ```sh
   otaq stage next --stdin \
     --manual-check "On a phone, the price table scrolls sideways and the header stays put" <<'COMMENT'
   Added the price table with sorting by price and name. Prices align right; the table scrolls on mobile widths.
   `npm test` at a1b2c3d: 48 passed; log: `$OTAQ_RESOURCE_DIR/test-run.txt`
   COMMENT
   ```

   Report a failing or partial run as it is ("2 failed, in `cart.test.ts`, which the ticket doesn't touch"), and any
   change after it.

   **List what you couldn't check yourself** with `--manual-check "<what to check>"`, one line per check, repeated
   for each: something that needs a real browser, a device, or a service you can't reach. The user sees them as
   "Manual checks for you" at review and ticks them. Never leave a check to "the PR" or "a manual test later": the
   flow may merge without a PR, and then nobody would try it. Each `stage next` replaces your stage's unticked
   checks, so after a failed check or a Send back list again the ones that still apply.

   If you learned something a later ticket should know (an API shape, a trap, a file others will touch), end the
   comment with a line that starts `Notes for later tickets:`.

   Then **end your turn** and wait. Otaq may validate your work first: the result arrives in this session as a
   message. If validation failed, the message says only that ("Validate failed round 2. Run `otaq prime`."): run
   `otaq prime`, read the checker's comment under "Comments" and search the log it names when you need the details (see
   "Long output"), then fix them, commit everything and run `otaq stage next` again. When it passes, the ticket
   moves on (to Ready for Review on the default board) and the user reviews the diff.

   **Nothing to deliver?** Only when the ticket needs no change at all (for example it was already fixed in SHOP-31)
   and you committed nothing, end the ticket instead, with the reason on stdin. It goes to Done as "Closed · nothing
   to merge". It is refused once the branch has commits: then use `otaq stage next`, so they're checked and delivered.

   ```sh
   otaq stage done --stdin <<'REASON'
   Already fixed in SHOP-31: the price table sorts by name since its merge.
   REASON
   ```

## With a Plan: work as the agent in charge

When the Plan under "## Plan" in `prime` has a Pieces list, the work is already split. You start a **helper** for each
piece, each in its own worktree on its own branch, check what it hands back and merge it into this ticket's branch. You
stay the ticket's agent: the gap check (step 3), the questions to the user and ending your stage (step 6) are yours.
**While helpers build the pieces, commit nothing yourself but the merges.**

1. **Start the pieces that can start.** A piece can start once every piece it waits for is merged. In "Pieces: 4
   (1 → 2, 3 → 4)" piece 1 starts first, then 2 and 3 side by side, then 4. Name each piece as the Plan does; a note
   on stdin gives its helper anything the Plan doesn't say:

   ```sh
   otaq helper start 1 --title "Wishlist API"
   otaq helper start 3 --title "Wishlist page" --stdin <<'NOTE'
   Reuse the product grid from app/products/page.tsx.
   NOTE
   ```

   Each helper runs your agent and model. When your tool's usage limit holds one ("waiting for limit reset"), carry on
   with the rest: Otaq starts it when the limit resets and tells you. Then **end your turn**: each helper's hand-back,
   question or stop is typed into this session as a message. `otaq helper list` shows every helper's status, branch
   and worktree.
2. **Check each piece it hands back** ("40.3 handed its piece back: …"):
   - Read what merging it would bring: `otaq helper diff 3` (a long one goes to a file in `$OTAQ_RESOURCE_DIR`; search
     it).
   - Run the project's tests in the helper's worktree (`otaq helper list` names it), the ones the project's notes and
     the Agent instructions name. Their output goes to a file in `$OTAQ_RESOURCE_DIR`, as in "Long output".
   - **Good:** merge it into this ticket's branch. Merge one piece at a time; then start the pieces that waited for
     it.

     ```sh
     otaq helper merge 3
     ```

   - **Not yet:** send it back with one clear note: what is wrong, where, and what done looks like. Its helper fixes it
     and hands back again.

     ```sh
     otaq helper back 3 --stdin <<'NOTE'
     The wishlist page doesn't show the empty state the description asks for (app/wishlist/page.tsx). Add it, with
     a test.
     NOTE
     ```

   A merge refused for a conflict merges nothing and tells you the note to send the helper back with: send it.
3. **Answer your helpers' questions** ("Piece 3's helper asks: …"). Answer from the Plan, the description, the comments
   and the decisions recorded on the ticket:

   ```sh
   otaq helper answer 3 --stdin <<'ANSWER'
   Guests can't save items: show the sign-in prompt, as the Plan's piece 1 says.
   ANSWER
   ```

   When your answer settles a product choice (what gets built, not how), also record it with `otaq decision record`
   (see "Product questions while you work"), so the user sees it in the review. When only the user can answer, pass the
   question on with its id from the message, and why:

   ```sh
   otaq helper pass-on 0199d2a4-6f1e-7c3b-9a52-1f0e8d7c6b5a --note "a product question the Plan doesn't answer"
   ```

   "Piece 3's helper stopped without handing back" means it ended its turn without `helper done`: read its last line,
   then answer it with a note (`otaq helper back 3 --stdin`) or check its work as a hand-back.
4. **The Plan changed** ("The plan changed: 40.4 removed. Run `otaq prime`…"): run `otaq prime` and read the Plan
   again. Tell running helpers what changed for them with `otaq helper back <n> --stdin`, start the new pieces, and
   never merge a removed piece (tell its helper to stop, with the same command).
5. **Finish the stage** once every piece is merged (`otaq helper list` shows them all "merged"): the whole ticket's
   tests pass on this branch, nothing is uncommitted, and `otaq stage next` names the pieces in its comment, for
   example "40.1 Wishlist API, 40.2 Wishlist button, 40.3 Wishlist page, 40.4 Share link: merged." Add the checks your
   helpers couldn't do themselves (their hand-backs list them) as `--manual-check` lines.

When the ticket comes back (a failed check, a Send back), the fix is yours as the agent in charge: make a small one
yourself, or start a helper for it with the next free piece number, for example piece 5 when the Plan has 4:

```sh
otaq helper start 5 --title "Fixing the Validate failure" --stdin <<'NOTE'
Validate failed: src/cart.ts:42 ignores the discount. Run `otaq prime` for the checker's comment.
NOTE
```

## Long output

Long command output (a test run, a build log, a long diff) goes to a file in `$OTAQ_RESOURCE_DIR`, Otaq's folder for
this ticket outside the repository, never into this session or a comment in full:

```sh
npm test > "$OTAQ_RESOURCE_DIR/test-run.txt" 2>&1
grep -n "FAIL" "$OTAQ_RESOURCE_DIR/test-run.txt"
```

- **Never in the worktree:** an untracked file there makes `otaq stage next` refuse.
- **Search it** with `grep` or `tail`; **never print it in full or retype it.**
- **Name it** in your comment; never attach command output, such as a run, a diff or a log. **Attach only a
  document you wrote**, such as an investigation write-up, with `--report-file` (at most 20 MB).

### Reading a report

Under a comment with a report, `prime` shows a line such as "Report: 48 KB, 2,310 lines · `otaq report SHOP-19 4`".
Open it only when you need it, and search it: `grep -n "expected" "$(otaq report SHOP-19 4 --path)"`.

## Check the ticket first

Before you write any code, make a quick gap check: read the description, the comments, the dependencies' last
comments and the code the ticket touches, then answer for yourself:

- **What is asked?** One or two sentences in your own words.
- **What is there already?** The files, screens and behaviour the ticket builds on.
- **What is missing or doesn't fit?** A vague word ("faster", "better", "export"), a screen or user group the ticket
  doesn't name, a description that contradicts the code or a dependency, two ways to read the same sentence.

Keep it quick: read, don't build or run long test suites for it. Then sort each open point:

- **It changes what gets built** (what the user sees or can do, which screens, which users, which data, how much work
  it is): ask before you code. Building the wrong thing wastes the whole ticket.
- **It only changes how you build it** (file layout, naming, a helper, a library that follows the project's habits):
  decide yourself. If it is still a product choice, record it (below).

**Most tickets are clear: then ask nothing and go straight on.** Don't ask what the description, the dependencies,
the code or the Agent instructions already answer, and don't ask about things the ticket plainly leaves to you.

When you do ask:

- **One question at a time, most important first.** A ticket holds one open question; a second one would replace the
  first. Say where you are: "Question 1 of 3". After each answer, check again: an answer can settle or drop the next
  question ("CSV only" makes a PDF layout question go away).
- **Offer options, each with its pros and cons**, and recommend one (see "Offer options" below). The user should see
  what each choice costs and what it changes before they pick.
- **Write no code while a gap-check question is open.** Ask, end your turn, and go on when the answer arrives.

```sh
otaq ask human \
  --option "CSV only" --note "One small library, about half a day. No printable copy for customers." \
  --option "CSV and PDF" --note "Printable invoices too, but needs a page layout and a PDF library: about two days." \
  --recommended 1 --stdin <<'QUESTION'
Question 1 of 2: SHOP-31 says "Add export" to the orders page but not which format. The accounting team's
spreadsheet import (docs/accounting.md) reads CSV.
QUESTION
```

## Product questions while you work: record, or ask only if you can't go on

When you meet a product question while you build that the code and the documents don't settle (what to build, not how):

- **If you can't go on without the answer**, ask the user (below).
- **Otherwise decide and record it.** Build one option, record the choice on your ticket, and carry on; nothing
  waits. The user sees every recorded decision in the review and can change it there.

Give each field on its own line, `Other option:` once per option:

```sh
otaq decision record --stdin <<'DECISION'
Question: Split `fullName` everywhere, or only in the profile form?
Built: only in the profile form
Other option: everywhere
Why: the description only mentions the profile form
DECISION
```

The user's Agent instructions may add to this rule (for example "always ask about copy text"); follow them.

## When you need the user

If you are stuck, or you can't go on without the user's decision, ask, then end your turn:

```sh
otaq ask human --stdin <<'QUESTION'
Should `fullName` be split into first and last name everywhere, or only in the profile form?
QUESTION
```

The ticket shows "Asks you" on the board. The answer arrives in this session as the user's next message.

### Offer options when there are clear alternatives

When the question has a few clear alternatives, offer them as **2–4 options** so the user can answer with one key, give
each option a **note with its pros and cons**, and recommend one when there is a sensible default:

```sh
otaq ask human \
  --option "Postmark" --note "Already sends password resets: no new account or keys. Costs more per email." \
  --option "SES" --note "Cheapest at volume, but needs AWS setup and domain checks first." \
  --option "Resend" --note "Simple API, but a new account and a new vendor to manage." \
  --recommended 1 --stdin <<'QUESTION'
Which email provider should send the order confirmation?
QUESTION
```

The answer then arrives as, for example, "Answer to your question: Postmark (option 1)." The user can always type
something else instead.

An **open question** stays free text, with no options:

```sh
otaq ask human --stdin <<'QUESTION'
What should the empty wishlist page say?
QUESTION
```

- 2–4 options, each a short label (at most 40 characters). The context goes in the question, each option's pros and
  cons in its `--note`, right after its `--option`.
- A note is one or two short sentences, at most about 200 characters: what the option costs (time, a new dependency, setup) and what it changes for
  the user or for later work. Give every option one, so the user can weigh the side effects of each choice.
- Recommend one with `--recommended N` (1-based) when there is a sensible default; leave it out when there isn't.
- Never invent options for an open question.
- The question text always goes through stdin; only the short labels and notes are arguments.
- The user's Agent instructions take precedence over this guidance.

## Rules

- **Always finish with `otaq stage next --stdin`** (or `otaq stage done --stdin` when there is nothing to deliver), as
  step 6 says. A summary in this chat isn't a hand-over.
- **Never push, never open a PR, never merge.** The user does that from the board. The one merge that is yours:
  your helpers' pieces into this ticket's branch, with `otaq helper merge`.
- **Long text always goes through stdin** (`--stdin`), never quoted arguments: comments, questions, ticket bodies.
  Long command output never does: it goes to a file (see "Long output").
- Useful commands: `otaq show <KEY>` (any ticket), `otaq dep tree` (your ticket's chain),
  `otaq create "Title" --stdin` (a follow-up in Backlog).
- If `otaq stage next` is refused, read why (uncommitted files, a comment over 2,000 characters, or `prime` not run
  since Otaq last resumed you), fix it and run it again.
- If `prime` lists **Stage choices**, you may change a later stage's choice for this ticket when what you learned
  changes how hard its work is, always with a reason (`otaq update <KEY> --stage "<stage>=<choice>" --reason "…"`).
  A choice the user set can't be changed.

## As a helper

`$OTAQ_PIECE` is set: you build **one piece** of the ticket for its agent in charge, in your own worktree, on your own
branch. The agent in charge reviews your work, merges it and moves the ticket; you never do.

1. **Run `otaq prime`.** As a helper it prints your piece first, the notes between you and the agent in charge, then
   the ticket, the whole Plan, its comments and your rules. Follow the user's Agent instructions and the project's
   notes it names, as far as they apply to your piece.
2. **Build only your piece**, as the Plan describes it. Anything else you notice goes in your hand-back, not in your
   code.
3. **Commit everything** with the `otaq-commit` skill (or your agent's own commit skill); without one, commit with
   `git add -A` and `git commit`. Never push and never merge: the agent in charge merges your branch.
4. **Questions** go through `otaq ask human --stdin`, as in "When you need the user": they reach the agent in charge
   first, which answers or passes them on to the user. End your turn and wait; the answer is typed into this session.
5. **Hand your piece back** with what you built and what you couldn't check yourself (a real browser, a device), then
   end your turn:

   ```sh
   otaq helper done --stdin <<'SUMMARY'
   Wishlist page with its empty state; `npm test` passes.
   Couldn't check: on a phone, the grid shows two columns.
   SUMMARY
   ```

   It is refused while files are uncommitted. The agent in charge lists your "Couldn't check" lines for the user.
6. **Sent back?** The agent in charge's note is typed into this session ("To 40.3: …" in `prime`). Read it, fix what it
   asks, commit, and hand back again with `otaq helper done --stdin`.

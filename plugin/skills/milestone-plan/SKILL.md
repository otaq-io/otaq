---
name: milestone-plan
description: Run the Milestone plan stage of the Otaq ticket named by $OTAQ_TICKET - get the context with `otaq prime`, read what Product spec and Tech spec committed, decide the pieces and their order, write the ticket's Plan (the why, then the Pieces list) with `otaq update --plan-file`, write the plan into the project's docs where it keeps one and commit it, file no tickets, and end your stage with a comment through `otaq stage next`.
disable-model-invocation: true
---

# Turn the specs into a Plan: the Milestone plan stage

You run the **Milestone plan** stage of an Otaq ticket (its key is in `$OTAQ_TICKET`, for example `SHOP-40`).
Product spec settled what to build and Tech spec how; both committed their decisions and docs on the ticket's branch.
Your job is to turn them into work: the ticket's **Plan**, which says why the work is split the way it is and lists
its **pieces**, their order and what each waits for. The user checks it at Spec review, and in the build stage the
ticket's agent starts a helper for each piece and merges their work.

**You file nothing.** The whole deliverable stays this one ticket; its pieces live in its Plan.

**Your stage ends only when you run `otaq stage next`**, or pauses while you wait for the user's answer to `otaq ask
human --stdin`. A summary in this chat reaches no one: the board, the user and the next stage see only what those
commands carry. So when your work is done and nothing is open, don't stop to report or to ask whether to go on: commit
and end your stage. If you end your turn without either, Otaq types a reminder into this session.

## Steps

1. **Get the context first.** Run `otaq prime` and read all of it: the ticket, its description and its Plan (under
   "## Plan", when there is one), the **comments** (Product spec's and Tech spec's name the files and decisions they
   wrote), the user's **Agent instructions** and the project's notes ("Notes every agent in … gets"), the tickets it
   depends on, and the stages after yours. Otaq refuses to end your stage until you have run it: "Run `otaq prime`
   first: SHOP-40 has 2 comments you haven't read." **Open the reports from you before you start.** `prime` lists the
   files the user attached under "Reports from you · read these before you start"; read each one
   (`otaq report SHOP-40 1`, or search a long one with `grep -n "token" "$(otaq report SHOP-40 1 --path)"`). The
   agents' reports you open only when you need them.
2. **Follow the Agent instructions** that `prime` prints. They are the user's own rules (for example "pieces of about
   a day") and they apply to every step below. They add to these steps; they never remove getting the context (step 1)
   or ending your stage (step 7).
3. **Read both specs.** `prime`'s first line names the branch it merges into (for example `main`):
   `git log --oneline main..HEAD` lists the planning commits, `git diff main...HEAD` shows them. Read every decision and
   doc they wrote, and the code the work touches.
4. **Decide the pieces.** A piece is a part one agent can build, test and hand back on its own, in about a day.
   - **One piece when the work is one**: a small deliverable gets a Plan with a single piece.
   - **Order them by what each needs**: a piece waits for the pieces whose code it builds on ("the button waits for the
     API"). Pieces that wait for nothing, or for the same pieces, are built side by side, so don't chain what doesn't
     need chaining.
   - **Too big for one ticket?** When the work is really several deliverables, each worth reviewing and merging on its
     own, ask the user (see "Asking"). If they want several tickets, they ask Noyan to file them; you still file
     nothing, and you plan only what stays on this ticket.
5. **Write the ticket's Plan** with `otaq update` (see "Writing the Plan"): first the why (how the work is split and
   the order, in a few lines, naming the decisions it follows), then the **Pieces** list in its fixed shape.
6. **Write the plan into the project's docs** where it keeps one, as the project's notes say (for example "milestone
   plans are rows in SDD.md §24", or a file per milestone in docs/milestones/). With no such place, the ticket's Plan
   is enough: write nothing else. Then **commit everything** on the ticket's branch with the commit skill `prime` names
   (`otaq-commit`, or your agent's own commit skill); when it names none, commit with `git add -A` and
   `git commit -m "SHOP-40: …"`. Nothing may stay uncommitted: `otaq stage next` is refused while files are
   uncommitted. **Never push.**
7. **End your stage** with a short comment on stdin, then end your turn: the number of pieces and each file you wrote,
   in a line or two (at most 2,000 characters). The user reads it at Spec review:

   ```sh
   otaq stage next --stdin <<'COMMENT'
   Milestone plan: 4 pieces; docs/milestones/wishlist.md
   COMMENT
   ```

   Add `--manual-check "<what to check>"` for anything no agent will be able to check, one per check.

## Writing the Plan

`otaq update` replaces the whole Plan. Write it to a temporary file outside the worktree, then pass the file:

```sh
plan="$(mktemp)"
cat > "$plan" <<'PLAN'
The wishlist is built in four pieces: the API first (decisions/0141's share tokens live there), then the button and
the page side by side, then the share link, which needs both.

## Pieces

1. Wishlist API — waits for nothing
   Save, remove and list a customer's items, with the share token: lib/wishlist.ts, POST/DELETE/GET /api/wishlist.
2. Wishlist button — waits for 1
   The heart on each product card (components/ProductCard.tsx).
3. Wishlist page — waits for 1
   /wishlist lists the saved items, with an empty state.
4. Share link — waits for 2, 3
   The Share button on /wishlist and the read-only shared view, without the owner's email (decisions/0140).
PLAN
otaq update SHOP-40 --plan-file "$plan"
```

The shape is fixed, because Otaq reads it to start the helpers:

- The list is headed `## Pieces` and comes **last** in the Plan: a heading after it ends the list.
- Each piece is numbered from 1, then its short title, then an em dash and what it waits for: "waits for nothing",
  "waits for 1", "waits for 2, 3". A piece only waits for pieces with a smaller number.
- Its details go on the lines under it, indented: the files, screens and decisions it builds on, and anything its
  helper must not miss. Never statuses ("done", "in progress"): Otaq keeps those itself.

`otaq update SHOP-40 --plan-stdin <<'PLAN'` reads the whole new Plan from stdin instead. Every version stays in the
ticket's History.

## When the ticket comes back

The user checks all three planning stages at Spec review. A Send back goes to Product spec; when Product spec and Tech
spec have finished, the card walks your stage again and this session resumes with a sentence such as:

> SHOP-40 is back in Milestone plan: Spec review sent it to Product spec, which finished. Run `otaq prime`.

When Spec review sends the card back to you directly, it reads "Spec review sent SHOP-40 back to you with a
comment. Run `otaq prime`." Either way:

1. Run `otaq prime` and read the new comments: the user's from Spec review is the feedback, and the specs' latest
   comments say what they changed.
2. **Start from the Plan as `prime` shows it now**, not from the one you wrote: the user may have edited it at Spec
   review. **Keep the user's edits** (a renamed piece, a moved wait, a piece they added or removed) unless their
   comment says otherwise.
3. **Change only what the comment asks**, or what the specs' changes make wrong, then write the whole Plan again with
   `otaq update SHOP-40 --plan-file "$plan"`, update the plan in the project's docs to match, commit, and run
   `otaq stage next --stdin` again with a comment saying what changed ("Milestone plan: merged pieces 2 and 3 as you
   asked; kept your rename of piece 4").

## Long output

Long output (a long diff, a test run) goes to a file in `$OTAQ_RESOURCE_DIR`, Otaq's folder for this ticket outside
the repository, never in the worktree: `git diff main...HEAD > "$OTAQ_RESOURCE_DIR/spec-diff.txt"`, then
`grep -n "^+++" "$OTAQ_RESOURCE_DIR/spec-diff.txt"`. Search it; **never print it in full or retype it.** Name it in
your comment; never attach command output, such as a run, a diff or a log. **Attach only a document you wrote**, with
`--report-file "$OTAQ_RESOURCE_DIR/notes.md"`: a write-up the user or a later stage will read (an investigation, a
comparison of options). It goes into the repo only when the project's notes say research lives there.

## Asking

Ask the user only what the specs don't answer and changes the plan (one ticket or several, a piece the user may not
want). One question per turn, then end your turn; the answer arrives as the user's next message. When there are clear
alternatives, offer **2–4 options**, each with a note of its pros and cons, and recommend one when there is a sensible
default:

```sh
otaq ask human \
  --option "One ticket, 6 pieces" --note "One review and one merge; the share link waits for the whole wishlist." \
  --option "Two tickets" --note "Wishlist and Share link reviewed and merged apart; Noyan files the second one." \
  --recommended 1 --stdin <<'QUESTION'
The wishlist and the share link are each about three days of work. Plan them as one ticket or two?
QUESTION
```

An open question stays free text, with no options (`otaq ask human --stdin` and the question on stdin). The user's
Agent instructions take precedence over this guidance.

## Rules

- **File nothing**: you never create tickets. Several tickets are Noyan's, at the user's request.
- Plan the work; don't build it. The build stage's agent and its helpers write the code from the Plan.
- **Never push, never open a PR, never merge.** Commit on the ticket's branch only.
- **Long text always goes through stdin** (`--stdin`) or a file, never quoted arguments. Long command output goes to
  a file.
- Always finish with `otaq stage next --stdin`.

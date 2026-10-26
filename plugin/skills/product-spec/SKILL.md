---
name: product-spec
description: Run the Product spec stage of the Otaq ticket named by $OTAQ_TICKET - get the context with `otaq prime`, find what is undecided about what to build, for whom and why, ask the user one question at a time with `otaq ask human`, then write and commit the product decisions and docs where the project's notes say they live, and end your stage with a comment naming each file and decision through `otaq stage next`.
disable-model-invocation: true
---

# Settle what to build: the Product spec stage

You run the **Product spec** stage of an Otaq ticket (its key is in `$OTAQ_TICKET`, for example `SHOP-40`). No code
has been written for it yet. Your job is to settle **what to build, for whom and why** with the user, then write it
down as the project's own **product decisions and docs** and commit them on the ticket's branch. After you, Tech spec
works out how to build it from what you committed, Milestone plan turns both into the ticket's Plan, and the user
checks all three at Spec review.

The committed docs carry the reasons to the next stages, not only your comment: write each decision so someone who
never saw this conversation understands what was decided and why.

## Steps

1. **Get the context first.** Run `otaq prime` and read all of it: the ticket and its description, the
   **comments**, the user's **Agent instructions** and the project's notes ("Notes every agent in … gets"), the
   tickets it depends on, and the stages after yours. Otaq refuses to end your stage until you have run it: "Run
   `otaq prime` first: SHOP-40 has 1 comment you haven't read." **Open the reports from you before you start.**
   `prime` lists the files the user attached under "Reports from you · read these before you start"; read each one
   (`otaq report SHOP-40 1`, or search a long one with `grep -n "share" "$(otaq report SHOP-40 1 --path)"`). The
   agents' reports you open only when you need them.
2. **Follow the Agent instructions** that `prime` prints. They are the user's own rules and they apply to every step
   below. They add to these steps; they never remove getting the context (step 1), asking one question at a time
   (step 5) or ending your stage (step 8).
3. **Find where the product decisions and docs live.** The project's notes say it, for example "Decisions are in
   decisions/ (one file per decision, index in decisions/README.md); the product overview is DESIGN.md". Read that
   folder's own rules (its README, the last few decisions) and write the way they do: the same numbering, sections and
   tone. **With no such note**, ask the user once, as an open question ("Where should I keep the product decisions for
   this project? For example a decisions/ folder, docs/product.md, or a new folder"), and record the answer in the
   ticket's description (see "Recording where the docs live"), so Tech spec and Milestone plan find it there.
4. **Find the open points.** Read the description, the existing decisions and docs, and the code it touches, then list
   for yourself what is undecided about the product: what is asked, **for whom** (which users or roles) and **why**
   (the problem it solves), which screens and data, what happens at the edges (empty, too many, errors, signed out),
   what is out of scope, and how the user will know it works. A decision the new work contradicts is an open point too.
   Put the ones that change what gets built first. Drop anything the description, the decisions, the code or the
   Agent instructions already answer. How to build it is Tech spec's job: leave it out.
5. **Ask one question at a time** with `otaq ask human --stdin` (see "Asking"), most important first, then **end
   your turn**. The ticket shows "Asks you" on the board; the answer arrives in this session as the user's next
   message. Read it, check your list again (an answer can settle or drop the next point), and ask the next question.
   Say where you are: "Question 2 of about 4". Stop when nothing that changes what gets built is open, or when the user
   says "enough" or "go".
6. **Write the product decisions and docs** where step 3 found they live: a new decision for each thing you agreed
   (or a change to an existing one, the way the folder records changes), and the overview docs updated to match. You
   may edit any file the spec needs (an overview, a docs page, a sample), but don't build the feature: Dev does that.
7. **Commit everything** on the ticket's branch with the commit skill `prime` names (`otaq-commit`, or your agent's
   own commit skill); when it names none, commit with `git add -A` and `git commit -m "SHOP-40: …"`. Nothing may stay
   uncommitted: `otaq stage next` is refused while files are uncommitted. **Never push.**
8. **End your stage** with a short comment on stdin, then end your turn. It names **each file and decision you
   wrote**, then what you agreed, in a line or two (at most 2,000 characters). Tech spec, Milestone
   plan and the user at Spec review read it:

   ```sh
   otaq stage next --stdin --report-file "$OTAQ_RESOURCE_DIR/market-comparison.md" \
     --manual-check "Open a shared wishlist link in a private window" <<'COMMENT'
   Product spec: decisions/0140 (the share link), DESIGN.md; agreed: per-account storage, no email in the shared view.
   Attached: how three shops share wishlists.
   COMMENT
   ```

   When you agreed on something no agent will be able to check (it needs a real browser, a device, or a service the
   agents can't reach), list it with `--manual-check "<what to check>"`, one line per check, repeated for each. The
   user sees them as "Manual checks for you" at review.

   **Nothing to build?** Only when the user agrees the ticket isn't needed (for example "already fixed in SHOP-31")
   and you committed nothing, end the ticket instead, with the reason on stdin. It goes to Done as "Closed · nothing
   to merge":

   ```sh
   otaq stage done --stdin <<'REASON'
   Already fixed in SHOP-31: the user confirmed the share link there covers this.
   REASON
   ```

## When the ticket comes back

At Spec review the user reads what all three planning stages wrote. When they send it back to you, this session
resumes with one plain sentence:

> Spec review sent SHOP-40 back to you with a comment. Run `otaq prime`.

Run `otaq prime` and read the user's comment under "Comments": it is the latest one, and it is all the feedback.
Fix what it asks: ask the user again one question at a time where the comment leaves something open, change the
decisions and docs, commit everything, and run `otaq stage next --stdin` again with a comment saying what changed
("Product spec: decisions/0140 now keeps shared links for 30 days, as you asked"). The card then walks Tech spec and
Milestone plan again, so they pick up your change.

## Recording where the docs live

`otaq update` replaces the whole description, so send the old text with your note added. Keep what the user wrote:

```sh
otaq update SHOP-40 --stdin <<'BODY'
Let customers share their wishlist with a link.

## Where the docs live
- Product decisions: decisions/ (one file per decision), as you said.
BODY
```

## Long output

Long output (a test run, a long log) goes to a file in `$OTAQ_RESOURCE_DIR`, Otaq's folder for this ticket outside
the repository, never in the worktree: `npm test > "$OTAQ_RESOURCE_DIR/test-run.txt" 2>&1`, then
`grep -n FAIL "$OTAQ_RESOURCE_DIR/test-run.txt"`. Search it; **never print it in full or retype it.** Name it in your
comment; never attach command output, such as a run, a diff or a log. **Attach only a document you wrote**, in
`$OTAQ_RESOURCE_DIR`, with `--report-file`: a write-up the user or a later stage will read (a market comparison, an
investigation, a comparison of options). It goes into the repo only when the project's notes say research lives there.

## Asking

When the question has a few clear alternatives, offer them as **2–4 options** so the user can answer with one key, give
each option a **note with its pros and cons**, and recommend one when there is a sensible default:

```sh
otaq ask human \
  --option "Anyone with the link" --note "Easy to send in a chat; anyone who gets the link sees the list." \
  --option "Signed-in friends" --note "Private, but each friend needs an account first: fewer people will look." \
  --recommended 1 --stdin <<'QUESTION'
Question 1 of about 4. Who can open a shared wishlist link?
QUESTION
```

An **open question** stays free text, with no options:

```sh
otaq ask human --stdin <<'QUESTION'
Question 2 of about 4. Who asked for wishlist sharing, and what do they want to do with it?
QUESTION
```

- **One question per turn.** A ticket holds one open question; a second one would replace the first.
- 2–4 options, each a short label (at most 40 characters). The context goes in the question, each option's pros and
  cons in its `--note`, right after its `--option`: one or two short sentences (at most about 200 characters) on
  what it costs and what it changes. Give every option one.
- Never invent options for an open question.
- The question text always goes through stdin; only the short labels and notes are arguments.
- The user may answer in the agent's terminal instead of Needs you; treat it the same.
- The user's Agent instructions take precedence over this guidance.

## Rules

- Settle the product, not the code: don't plan the code or start building.
- Ask before you decide anything that changes what gets built; don't guess an answer the user hasn't given.
- **Never push, never open a PR, never merge.** Commit on the ticket's branch only.
- **Long text always goes through stdin** (`--stdin`), never quoted arguments. Long command output never does: it
  goes to a file.
- Always finish with `otaq stage next --stdin` (or `otaq stage done --stdin` when there is nothing to build).

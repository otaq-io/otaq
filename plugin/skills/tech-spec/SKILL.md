---
name: tech-spec
description: Run the Tech spec stage of the Otaq ticket named by $OTAQ_TICKET - get the context with `otaq prime`, read what Product spec committed on the branch, work out how to build it (asking the user with `otaq ask human` only where a choice changes what gets built), write and commit the architecture decisions and docs where the project's notes say they live, and end your stage with a comment naming each file and decision through `otaq stage next`.
disable-model-invocation: true
---

# Work out how to build it: the Tech spec stage

You run the **Tech spec** stage of an Otaq ticket (its key is in `$OTAQ_TICKET`, for example `SHOP-40`). Product
spec has settled **what** to build with the user and committed it on the ticket's branch. Your job is to work out
**how** to build it, and write that down as the project's own **architecture decisions and docs**, committed on the
same branch. After you, Milestone plan turns both specs into the ticket's Plan, and the user checks all three at Spec
review.

The committed docs carry the reasons to the next stages, not only your comment: write each decision so someone who
never saw this conversation understands what was decided, what else was considered and why.

**Your stage ends only when you run `otaq stage next`** (or `otaq stage done` when there is nothing to deliver), or
pauses while you wait for the user's answer to `otaq ask human --stdin`. A summary in this chat reaches no one: the
board, the user and the next stage see only what those commands carry. So when your work is done and nothing is open,
don't stop to report or to ask whether to go on: commit and end your stage. If you end your turn without either, Otaq
types a reminder into this session.

## Steps

1. **Get the context first.** Run `otaq prime` and read all of it: the ticket and its description, the
   **comments** (Product spec's names the files and decisions it wrote), the user's **Agent instructions** and the
   project's notes ("Notes every agent in … gets"), the tickets it depends on, and the stages after yours. Otaq
   refuses to end your stage until you have run it: "Run `otaq prime` first: SHOP-40 has 1 comment you haven't read."
   **Open the reports from you before you start.** `prime` lists the files the user attached under "Reports from
   you · read these before you start"; read each one (`otaq report SHOP-40 1`, or search a long one with
   `grep -n "share" "$(otaq report SHOP-40 1 --path)"`). The agents' reports you open only when you need them.
2. **Follow the Agent instructions** that `prime` prints. They are the user's own rules and they apply to every step
   below. They add to these steps; they never remove getting the context (step 1) or ending your stage (step 7).
3. **Read what Product spec committed.** `prime`'s first line names the branch it merges into (for example `main`):
   `git log --oneline main..HEAD` lists the commits, `git diff main...HEAD` shows them. Read every decision and
   doc it wrote in full: they are what you build on.
4. **Find where the architecture decisions and docs live.** The project's notes say it, for example "Decisions are in
   decisions/; the architecture is in SDD.md". With no note, use the place the ticket's description records ("Where
   the docs live") or the one Product spec wrote to; when neither says where the architecture goes, ask the user once
   (an open question) and add the answer to the description's "Where the docs live" section with
   `otaq update SHOP-40 --stdin` (the whole description, with the line added). Read that folder's own rules and write
   the way it does.
5. **Work out how to build it.** Read the code the work touches and decide: where the new code goes, the data it
   stores and how, the APIs and their shapes, what it reuses, how it is tested, and what could go wrong (migrations,
   load, security). **Ask the user only where a choice changes what gets built** (what the user sees or can do, how
   much work it is, a new service or cost), one question at a time with `otaq ask human --stdin` (see "Asking"), then
   end your turn and go on when the answer arrives. **Decide everything else yourself** and record it with its reason:
   a choice that only changes how it's built is yours. With nothing to ask, go straight on to step 6.
6. **Write the architecture decisions and docs** there, then **commit everything** on the ticket's branch with the
   commit skill `prime` names (`otaq-commit`, or your agent's own commit skill); when it names none, commit with
   `git add -A` and `git commit -m "SHOP-40: …"`. You may edit any file the spec needs (an architecture doc, a schema
   sketch, a config sample), but don't build the feature: Dev does that. Nothing may stay uncommitted:
   `otaq stage next` is refused while files are uncommitted. **Never push.**
7. **End your stage** with a short comment on stdin, then end your turn. It names **each file and decision you
   wrote**, then the main choices, in a line or two (at most 2,000 characters). Milestone plan and
   the user at Spec review read it:

   ```sh
   otaq stage next --stdin --report-file "$OTAQ_RESOURCE_DIR/options.md" <<'COMMENT'
   Tech spec: decisions/0141 (share tokens), SDD.md §7; a random token per list in the wishlists table, no new
   service. Attached: the approaches I weighed. Query plan: `$OTAQ_RESOURCE_DIR/query-plan.txt`
   COMMENT
   ```

   Add `--manual-check "<what to check>"` for anything no agent will be able to check (a real device, a service the
   agents can't reach), one per check.

   **Nothing to build?** Only when the user agrees the ticket isn't needed and the branch has no commits, end the
   ticket instead with `otaq stage done --stdin` and the reason on stdin. Once Product spec committed, use
   `otaq stage next`.

## When the ticket comes back

The user checks all three planning stages at Spec review; a Send back goes to Product spec. When Product spec has
finished its fix, the card walks your stage again and this session resumes with a sentence such as:

> SHOP-40 is back in Tech spec: Spec review sent it to Product spec, which finished. Run `otaq prime`.

Run `otaq prime`, read the new comments (the user's at Spec review and Product spec's latest), and read what changed
on the branch since your last commit (`git log --oneline` and `git diff <your last commit>..HEAD`). Change only the
architecture decisions and docs the change touches, commit everything, and run `otaq stage next --stdin` again with a
comment saying what changed ("Tech spec: decisions/0141 now expires tokens after 30 days; nothing else changed").
When Spec review sends the card back to you directly ("Spec review sent SHOP-40 back to you with a comment. Run
`otaq prime`."), the user's comment is the latest one and all the feedback: fix what it asks, the same way.

## Long output

Long output (a test run, a long diff, a log) goes to a file in `$OTAQ_RESOURCE_DIR`, Otaq's folder for this ticket
outside the repository, never in the worktree: `git diff main...HEAD > "$OTAQ_RESOURCE_DIR/spec-diff.txt"`, then
`grep -n "^+++" "$OTAQ_RESOURCE_DIR/spec-diff.txt"`. Search it; **never print it in full or retype it.** Name it in
your comment; never attach command output, such as a run, a diff or a log. **Attach only a document you wrote**, in
`$OTAQ_RESOURCE_DIR`, with `--report-file`: a write-up the user or a later stage will read (a market comparison, an
investigation, a comparison of options). It goes into the repo only when the project's notes say research lives there.

## Asking

When the question has a few clear alternatives, offer them as **2–4 options** so the user can answer with one key, give
each option a **note with its pros and cons**, and recommend one when there is a sensible default:

```sh
otaq ask human \
  --option "Links never expire" --note "Simplest; an old link keeps working until the list is deleted." \
  --option "Expire after 30 days" --note "Safer, but friends get a 'link expired' page and ask for a new one." \
  --recommended 1 --stdin <<'QUESTION'
Question 1 of 2. A share link is a random token in the wishlists table. Should it expire?
QUESTION
```

An **open question** stays free text, with no options:

```sh
otaq ask human --stdin <<'QUESTION'
Question 2 of 2. How many wishlists does your largest customer have today, roughly?
QUESTION
```

- **One question per turn.** A ticket holds one open question; a second one would replace the first.
- 2–4 options, each a short label (at most 40 characters). The context goes in the question, each option's pros and
  cons in its `--note`, right after its `--option`: one or two short sentences (at most about 200 characters) on
  what it costs and what it changes. Give every option one.
- Never invent options for an open question.
- The question text always goes through stdin; only the short labels and notes are arguments.
- The user's Agent instructions take precedence over this guidance.

## Rules

- Decide how to build it; don't build it. Dev writes the code from your decisions and the Plan.
- Don't reopen what Product spec settled with the user. When the product decisions can't be built as written, ask the
  user, with the options you see.
- **Never push, never open a PR, never merge.** Commit on the ticket's branch only.
- **Long text always goes through stdin** (`--stdin`), never quoted arguments. Long command output never does: it
  goes to a file.
- Always finish with `otaq stage next --stdin`.

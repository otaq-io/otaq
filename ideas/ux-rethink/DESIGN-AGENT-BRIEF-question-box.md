# For the design agent · the question box, for long questions (follow-up to `flow-noyan`)

2026-11-12. Draw on "Otaq — New UX" (https://claude.ai/artifact/7tYuWTkYKLrq5gnUHSZWP1). Add new frames; don't
change existing ones.

The maintainer picked **alternative A** of `flow-noyan` (the board's box comes with you into Edit flow; UX-63). One
thing in A-4 doesn't hold: the answer box is a fixed 330 px, and agents ask long questions with context. That
problem isn't only Edit flow's. It's how Otaq draws **every** agent question today, and it's hard to read. Two
decisions to draw:

**UX-62 · The first line asks, the rest is context.** Everywhere a question shows:
- The **first line is the question**, one sentence, bold.
- **Everything after it is context**, regular weight, its paragraphs and line breaks kept, simple Markdown drawn
  (`code`, lists, **bold**), at a readable line length (about 70 characters).
- The options keep their notes; the recommended one is marked, as today.
- A **long question scrolls its context**, while the question line, the options and Send stay in view.
- The question icon and each option's radio sit at the **top** of their text (today they're centred, so they float
  halfway down a long paragraph).
- No new field: the question is the same plain text agents send today.

**UX-63 · A's answer box grows and scrolls.** In Edit flow the box becomes the answer box when Noyan asks: it grows
with the question up to a cap (suggest one: about half the editor's height), scrolls its context past that, and has an
**expand control** that opens the question larger for a really long one.

## What's wrong today (for reference)

The question is one bold 13 px line of text with its line breaks lost; Markdown shows raw; the icon and radios are
centred; the Needs you strip's popover is 320–380 px wide (up to 520 px tall), so a long question becomes a tall thin
column.

## Use these questions

A short one, for comparison:

> Should the security review run before or after Validate?

A long one (draw it as written; it's the kind Noyan's `flow` skill asks):

> Which agent should run the new Security review stage?
>
> Your project has a security skill in `.claude/skills/security-review`, and it expects the code to build first: it
> runs `npm audit` and reads the lockfile. Two things to weigh:
>
> - **Verifier** already runs Validate on Codex. Using it again keeps one checking agent, but Codex doesn't load
>   `.claude/skills`, so it would fall back to Otaq's own review skill.
> - **Strong** (Claude Code) loads your skill as it is, but it's the agent that writes the code, so it would review its
>   own work.
>
> A third agent costs a new connection in Settings → Agents.

Options, with notes:
1. **Strong, with your security-review skill** (recommended) · Uses your skill as written; reviews its own code.
2. **Verifier, with Otaq's review skill** · One checking agent for both checks; your skill isn't used.
3. **A new agent** · Separate reviewer; you set it up in Settings → Agents first.
4. Other…

And one long **open** question (no options, a text box for the answer), so the free-text case is drawn too.

## What to draw

1. **A-4 again (Edit flow):** the answer box with the short question, then with the long one (grown to its cap, the
   context scrolled part way, the question line and options in view), then expanded. Say where the expanded view
   lives (a larger box over the columns, a centred dialog…) and how it closes.
2. **The same question in the other places it shows:** a ticket's Overview (Needs you), above Noyan's terminal in the
   agent panel, and the Needs you strip's popover. Long question each time. If the strip's popover is too narrow for
   a long question, say what it should do (wider, or open the question somewhere larger).
3. The long open question, in one of those places.
4. The narrow window (1280) for A-4, and dark mode for A-4 expanded.

## What to hand back

- Frames named `question-box-1` …
- The numbers: the box's cap, the expanded size, the context's line length, the strip popover's size.
- Anything in UX-62 that doesn't work once drawn (for example a question whose first line runs long): name it.

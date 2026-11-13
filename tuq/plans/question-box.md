# Plan · question-box

- **Ticket:** none
- **Date:** 2026-11-12
- **Decisions:** UX-62, UX-63, UX-64 (ideas/ux-rethink/DECISIONS.md; UX-63 refines UX-61 and extends UX-22)
- **Tech spec:** none (the decisions, the designer's frames and the code named in each piece; frames `flow-noyan-1/2`, alternative A, and `question-box-1/2` on the canvas "Otaq — New UX", https://claude.ai/artifact/7tYuWTkYKLrq5gnUHSZWP1, briefs ideas/ux-rethink/DESIGN-AGENT-BRIEF-flow-noyan.md and DESIGN-AGENT-BRIEF-question-box.md)
- **Orchestrator prompt:** tuq/prompts/question-box-orchestrator.md

<!-- tuq:plan:start -->

question-box makes an agent's question readable however long it is, lets a question take several answers, and gives
Edit flow a place to talk to Noyan: the board's box comes along (decisions UX-62, UX-63, UX-64). Seven pieces, one
behaviour each. The question box comes first (1): every other question piece draws through it. Then side by side:
the strip popover's room for a long question (2), several answers (3), and the agent rules (4), which waits for
nothing. Edit flow's box is three behaviours in a row: asking Noyan from it (5), the box following the conversation
(6), and the box answering Noyan's question (7), which needs 1 and 6. The agent in charge follows
tuq/prompts/question-box-orchestrator.md.

## Pieces

1. A long question reads well wherever it shows — waits for nothing
   Decision UX-62; frames question-box-1, -4, -5, -7 (and "question-box · the numbers"). Every agent question is
   drawn by the shared answer UI: `AnswerChoices` and `OpenAnswer` in
   crates/otaq-desktop/ui/src/screens/overview/NeedsYou.tsx, used by a ticket's Overview (Needs you) and by
   `AgentQuestion` (screens/agent_panel/AgentQuestion.tsx, above Noyan's terminal and in the strip's popover); styles
   in screens/sheet/actions.css (`.sheet-answer*`). Today the whole question is one bold 13 px span, its line breaks
   lost, Markdown raw, the icon and radios centred. Change it once, there:
   - The question's first line (up to the first newline) is the question, bold. Everything after it is context,
     regular weight, paragraphs and line breaks kept, simple Markdown drawn: inline code, lists, bold; a code block as
     a monospace block that scrolls sideways; a heading as a bold paragraph; a table as plain text. No raw HTML.
     Split the text in Rust (the view model in crates/otaq-view/src/view_models/answer_options.rs `Answer` gets the
     question line and the context apart) so every place gets the same split; a question with no newline has no
     context.
   - Context lines at most about 70 characters (400 px at 13 px Geist, line height 1.55); 60 in the agent panel.
   - Nothing is cut short: in a long question the question line and the context scroll together as one text, while
     the options (with their notes) and Send stay in view. The icon and each option's radio sit at the top of their
     text.
   - Where it caps: on a ticket's Overview the box is first on the page with no cap of its own; only when Send would
     fall below the sheet does the text cap at half the sheet's body and scroll. Above Noyan's terminal it caps at
     55% of the panel's height, the terminal keeping at least 200 px.
   - `otaq ask human` and the stored question (`HumanQuestion`, crates/otaq-core/src/board/questions.rs) don't change.
   One behaviour across the view model and the page, on purpose.
   Check: vitest for the split (no newline, a question line plus context with a list, a code block, a heading); a
   page test that a long question on a ticket's Overview and above Noyan's terminal keeps Send in view and scrolls the
   text; the Gallery's question samples (answer_options.rs `note_samples`) gain a long one like the brief's
   ("Which agent should run the new Security review stage?") and it reads as frame question-box-4.
2. A long question in the Needs you strip opens wide, with a way to the larger place — waits for 1
   Decision UX-62; frame question-box-6. The strip's popover for a question (screens/strip/Strip.tsx; `.strip-asks`
   is 320 px and `.strip-agent-asks` 380 px wide, up to min(70vh, 520 px), in screens/strip/*.css) widens to 500 px
   for a question (context about 400 px, ~70 characters) and caps at 560 px tall, drawn by piece 1's box, and offers
   Open ticket (a ticket's question) or Open in the agent panel (Noyan's) to read it whole. Other strip popovers keep
   their size.
   Check: a page test: a long question's popover is 500 px wide, at most 560 px tall, Send in view, and its Open
   ticket / Open in the agent panel lands on the question; a short question's popover still fits its content.
3. A question can take several answers — waits for 1
   Decision UX-64. `otaq ask human` gets a flag (`--multiple`) that lets the user pick more than one of its options
   (crates/otaq-cli/src/cli.rs `Ask`; refused without options; `--recommended` may then name several, comma
   separated). The stored question (`HumanQuestion`) records it; old stored questions read as single-answer. The
   answer box (piece 1's) draws checkboxes instead of radios, the recommended ones ticked and marked, Other… tickable
   with the others; Send answer needs at least one ticked; number keys toggle. The answer reaches the agent as every
   picked option plus Other…'s text, in the wording of `format_answer` / `answer_text`
   (crates/otaq-core/src/attention/needs_you.rs; e.g. "Answer to your question: Postmark (option 1), SES (option
   3)."); the ticket's Messages log it the same. Works for a ticket's agent and for Noyan (`project_agent.answer`,
   `sheet.answer`). Tools' own multi-select questions (Claude Code's built-in one) stay in their terminal: out of
   scope. One behaviour across the CLI, core, the view model and the page, on purpose.
   Check: CLI tests for the flag and its refusals; core tests that a two-option pick types the right answer and is
   logged; a page test ticking two options and Other… and sending; an old stored question still answers with one
   pick.
4. Agents put the question first, and Noyan leaves nothing only in its terminal — waits for nothing
   Decisions UX-62, UX-63. `otaq ask human --help` (the `Ask` doc comments in crates/otaq-cli/src/cli.rs; the help
   test there checks the text) says once, for every agent: "The first line is the question, in one sentence; the
   context follows on the next lines; the reason for each choice goes in that option's --note." The `flow` skill
   (plugin/skills/flow/SKILL.md, step 3 "Understand the request") adds: everything the user needs to decide must be in
   Otaq, not only in your terminal, which they may never open: the background for a choice goes in the question's
   context and its options' notes; the reason for a change goes in that stage's `why:`. The skills' example questions
   (flow, noyan, product-spec, tech-spec, validate, address-review, milestone-plan) follow the first-line rule. No
   other skill text changes.
   Check: the CLI help test reads the new line; `grep -n "ask human" plugin/skills/*/SKILL.md` shows every example
   question with the question on its first line; the flow skill's rule is in step 3.
5. In Edit flow, the user asks Noyan for a change from the box — waits for nothing
   Decision UX-63 (refines UX-61, extends UX-22); frames flow-noyan-A-1, A-2. Edit flow
   (crates/otaq-desktop/ui/src/screens/flow_board/FlowBoard.tsx; its words from flow_board.rs in
   crates/otaq-view/src/view_models/) keeps a box docked at its foot, like the board's box for new work
   (screens/new_work/NewWork.tsx, whose look it reuses): "Ask Noyan to change this flow…", with a **Suggest a whole
   flow** chip and the line "Noyan · <agent> · <tool> — Proposes a draft · nothing changes until you save". ↵ sends
   the user's words to Noyan's agent on the `flow` skill: `flow_board.suggest` (crates/otaq-desktop/src/api/
   flow_board.rs) takes an optional `text`, sent with `flow_message(text)`; without it, the chip's whole-flow request
   (`suggest_message`). Sending doesn't open the agent panel. The chip is off with the reason while Noyan drafts
   (today's `suggest.off`). Start from… loses its Suggest row: presets and Import only. The columns get bottom room
   so the box never covers the last cards or the Done column's delivery line. The box stays in Edit flow while
   Noyan's panel is open (the board's box hides then; this one doesn't). One behaviour across the request and the
   page, on purpose.
   Check: a page test: typing "Add a security review after Validate" and ↵ starts Noyan's agent with that text on
   the flow skill and the panel stays closed; the chip sends the whole-flow request; Start from… lists no Suggest;
   the box doesn't cover the last card at 1280 × 800. A Rust test for `suggest` with and without text.
6. Edit flow's box follows Noyan's conversation — waits for 5
   Decision UX-63; frames flow-noyan-A-3, A-5. While Noyan drafts, the box shows "<agent> is drafting a change to
   <project>'s flow…" with **Show** (the agent panel on that conversation, `showAgentInPanel`) and **Stop** (ends
   that conversation, as the agent panel's end does, `agent_panel.end`; the draft keeps what was proposed); this
   replaces the `.fb-drafting` line under the bar. The hand tools keep working. While a proposal shows, the box
   reads "Ask for changes to <agent>'s proposal…" and ↵ sends to the same conversation (`flow_board.ask_changes`);
   the bar's Ask for changes… button and its popover go (the bar keeps Watch a ticket, Discard and Save flow). The
   skill field's "Ask the agent to write it" box (`AskBox` with `writing`) is unchanged.
   Check: page tests: drafting shows in the box with Show and Stop and no line under the bar; Stop ends the
   conversation; with a proposal the box sends to `ask_changes` and the bar has no Ask for changes….
7. Edit flow's box answers Noyan's question, and expands for a long one — waits for 1, 6
   Decisions UX-62, UX-63; frames flow-noyan-A-4, question-box-1, -2, -3, -8. When Noyan's agent asks
   (`otaq ask human`), the box becomes the answer box, turquoise: piece 1's question UI (and piece 3's checkboxes
   when merged), with "Answer in terminal" opening the agent panel. flow_board.rs exposes the editor agent's question
   (`agent_question::of_agent`, crates/otaq-view/src/view_models/agent_question.rs) next to `drafting`; answering
   goes through `project_agent.answer`. The box grows with the question up to 55% of the editor's height (440 px in a
   900 px window, 384 px at 1280 × 800), width 760 px while asking (680 px at 1280; 680 px otherwise); past the cap
   the question text scrolls with options and Send in view. **⤢** opens the same box up to the editor's full height
   (16 px under the bar, 20 px above the bottom) over a scrim; **⤡**, Esc or sending the answer brings it back. A
   permission prompt (not a question) keeps today's Needs you wording in the box with Show.
   Check: page tests: a short question fits; the brief's long question grows to the cap with Send in view; ⤢ and ⤡
   / Esc; sending answers Noyan; at 1280 × 800 the cap is 384 px.

<!-- tuq:plan:end -->

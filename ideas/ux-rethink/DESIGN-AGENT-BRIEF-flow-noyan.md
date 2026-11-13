# For the design agent · talking to Noyan while you design a flow (two alternatives)

2026-11-12. Draw on the canvas "Otaq — New UX" (https://claude.ai/artifact/7tYuWTkYKLrq5gnUHSZWP1), next to the Edit
flow frames (board-04). **Nothing is decided yet.** Draw **two alternatives, A and B**, side by side, so the
maintainer can compare them and pick one. Don't change the existing Edit flow frames; add new ones.

## 1 · The problem

Noyan can design a whole flow, and the user can ask it for any change ("add a security review after Validate"). In
Edit flow today, those are the editor's most useful actions, and they are the hardest to find:

| The user wants to… | Today |
|---|---|
| Have Noyan design a whole flow | Edit flow → **Start from…** (a popover named for presets) → "Suggest a flow for shop-web…". |
| Ask for one specific change | **No way from the editor.** The user must know to open Noyan's panel (⌘L, or Noyan at the foot of the rail) and type there. |
| Change Noyan's suggestion | **Ask for changes…** in the bar (only while a suggestion shows) → a small popover with a text box → Send. |
| See what Noyan is doing | A line under the bar: "Strong is drafting a flow for shop-web…" with **Show**, which opens the agent panel. |

There's also no chat box anywhere in the editor. On the board, the box for new work docked at the bottom (UX-22) is
where you talk to Noyan (⇧↵ Ask Noyan to shape it). Edit flow replaces the whole board, and that box goes with it,
so the one screen where Noyan helps most has no place to type to it.

**Goal:** in Edit flow, asking Noyan is always visible, one step away, and reads as the natural way to design a flow.
The hand tools (drag, **+**, ×, stage details) stay exactly as they are.

## 2 · How it works underneath (true for both alternatives)

- Noyan's agent works on the `flow` skill: it reads the flow in use, may ask the user questions one at a time
  (these show as **Needs you**), then **proposes** a whole flow. The proposal lands in the editor as the draft:
  stages marked **Added** / **Changed**, with **Why these changes**. Nothing changes on the board until **Save flow**.
- Any request works, from "design a flow for this repo" to "make Code review use Codex". Today the editor only sends
  the fixed "Suggest a flow" request; the free-text one is the new part.
- After a proposal, the user's words go back to the same conversation as changes to that proposal (today's Ask for
  changes…). **Discard** drops the proposal; the flow in use draws again.
- Noyan's conversation is a real agent session (Claude Code, Codex…) shown as a **terminal** in the agent panel, the
  panel at the right of the window (⌘L). Before the first message, the empty panel has a plain text box and Send;
  after that, it's the tool's own terminal. **Otaq can't draw chat bubbles** from it: don't draw a message list made
  of bubbles unless it's the terminal.
- One Noyan conversation drafts at a time: while it drafts, starting another suggestion is off ("Strong is drafting
  one").
- Rules that stay: Noyan never changes the live flow (it proposes; the user saves). The skill field's **Ask the agent
  to write it** (stage details) stays as it is; it's a different agent and out of scope here.

## 3 · Alternative A · The bottom box comes with you

The board's docked box (UX-22) stays at the foot of the editor and becomes the editor's way to talk to Noyan.

- Placeholder along the lines of "Ask Noyan to change this flow…", with a **Suggest a whole flow** shortcut (chip or
  link) beside it. ↵ sends.
- **Start from…** goes back to being only the presets and Import; Suggest moves out of it.
- While a proposal shows, the same box reads "Ask for changes…" (the bar's Ask for changes… button and its popover go
  away). Discard and Save flow stay in the bar.
- While Noyan drafts or waits on the user, the box shows that state ("Strong is drafting a flow…", "Needs you · Strong
  asks a question") with **Show**, replacing today's line under the bar.
- The conversation itself stays in the agent panel, which opens on Show (or on send — your call; say which).

## 4 · Alternative B · Noyan beside the flow

The editor gets a real conversation area: Noyan's panel docked as part of the editor, with a text box at its foot.

- When Edit flow opens, Noyan's side is there (open by default, collapsible), on the right; with nothing started it is
  the empty panel: a short line on what Noyan can do, **Suggest a whole flow**, and the text box.
- Once a conversation starts, the side shows its terminal (section 2), and the user answers Noyan's questions there.
- The proposal's **Why these changes** lives in this side too, next to the conversation that produced it (today they
  share the right side with stage details: say how the two share it — tabs, stacked, details over it…).
- Start from… and Ask for changes… change as in A (Suggest out of Start from…; Ask for changes is the side's box).
- Mind the width: the editor's columns already need it. Draw it at a laptop width (1440) and at the narrow window
  width, and say what collapses first.

## 5 · What to draw, for each alternative

1. Edit flow opened on a project's flow, no proposal, nothing started (the first look).
2. The user typing a specific request ("Add a security review after Validate").
3. Noyan drafting.
4. Noyan asking the user a question (Needs you), and where the user answers.
5. The proposal shown: Added / Changed marks, Why these changes, asking for a change, Discard, Save flow.
6. The narrow window.
7. Light and dark for frames 1 and 5.

Use the app's real names and copy ("Edit flow", "Start from…", "Save flow", "Needs you", the agent named Strong,
project shop-web). Keep the editor's bar (Watch a ticket, Start from…, ⋯, Cancel, Save flow) unless an alternative
needs it changed; then say what moved.

## 6 · What to hand back

- The frames, named `flow-noyan-A-1` … and `flow-noyan-B-1` …
- A short comparison (a frame or a note on the canvas): for each alternative, what the user sees first, clicks to
  ask for a change, what it costs in width, what's removed, and what you'd pick and why.
- Anything in section 2 that made a good design impossible: name it rather than drawing around it.

## 7 · Decisions this touches (for the maintainer, after the pick)

- **UX-61** says "Start from… leads with 'Suggest a flow for <project>' … Ask for changes… / Discard / Save flow".
  Both alternatives move Suggest and Ask for changes, so the pick refines UX-61.
- **UX-22** says the docked box "replaces both quick create and the Ask bar" on the board; A extends it to Edit flow.

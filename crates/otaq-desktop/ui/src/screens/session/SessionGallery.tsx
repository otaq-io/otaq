// The gallery's Session and Activity (T-0466; canvas `s25-06`, `s25-07`): the switch in each state (Agent working,
// waiting for you; the check stage running and waiting; Run up, failed), the agent's line and Run's line, and
// History's filters over a short story. The words are `session_tab.rs`' and `history.rs`' for the example tickets.
import { useState } from "react";

import type { HistoryView } from "../../generated/HistoryView";
import type { SessionPartView } from "../../generated/SessionPartView";
import type { SheetContent } from "../../generated/SheetContent";
import { HistoryBody } from "../activity/History";
import { SessionSwitch } from "./Session";

const part = (content: SheetContent, label: string, dot: SessionPartView["dot"] = null): SessionPartView => ({
  content,
  label,
  dot,
  agent: null,
});

const SWITCHES: {
  caption: string;
  parts: SessionPartView[];
  shown: SheetContent;
  line: string;
  right: string;
}[] = [
  {
    caption: "SHOP-22 · the agent waits for you, Run up",
    parts: [part("agent", "Agent", "waiting"), part("run", "Run", "running"), part("shell", "Terminal")],
    shown: "agent",
    line: "Rust developer · Codex · gpt-5.5 · ~/.otaq/wt/shop-web/SHOP-22",
    right: "Typing goes to Rust developer · SHOP-22",
  },
  {
    caption: "SHOP-30 · validating: the Checker runs",
    parts: [
      part("agent", "Agent"),
      part("check", "Validate", "running"),
      part("run", "Run"),
      part("shell", "Terminal"),
    ],
    shown: "check",
    line: "Checker · Codex · gpt-5.5 · ~/.otaq/wt/shop-web/SHOP-30",
    right: "Click to type to Checker · SHOP-30",
  },
  {
    caption: "SHOP-19 · in Security: the Security reviewer asks you",
    parts: [
      part("agent", "Agent"),
      part("check", "Security", "waiting"),
      part("run", "Run"),
      part("shell", "Terminal"),
    ],
    shown: "check",
    line: "Security reviewer · Claude Code · sonnet · ~/.otaq/wt/shop-web/SHOP-19",
    right: "Typing goes to Security reviewer · SHOP-19",
  },
  {
    caption: "SHOP-22 · Run",
    parts: [
      part("agent", "Agent", "running"),
      part("run", "Run", "running"),
      part("shell", "Terminal", "running"),
    ],
    shown: "run",
    line: "pnpm dev · in SHOP-22’s worktree · :4022",
    right: "Open http://localhost:4022",
  },
  {
    caption: "SHOP-22 · Run failed to start",
    parts: [part("agent", "Agent"), part("run", "Run", "problem"), part("shell", "Terminal")],
    shown: "run",
    line: "pnpm dev · in SHOP-22’s worktree · :4022",
    right: "",
  },
];

const row = (
  when: string,
  title: string,
  kind: HistoryView["rows"][number]["kind"],
  detail: string | null = null,
  mark: HistoryView["rows"][number]["mark"] = { kind: "agent" },
  cost: string | null = null,
): HistoryView["rows"][number] => ({
  title,
  detail,
  when,
  who: mark.kind === "you" ? `you · ${when}` : mark.kind === "otaq" ? `Otaq · ${when}` : when,
  cost,
  tool_session_id: null,
  quiet: false,
  short_id: null,
  range: null,
  mark,
  opens:
    kind === "description"
      ? {
          label: "DESCRIPTION",
          caption: "Before / after",
          lines: [
            {
              kind: "same",
              sign: "",
              text: "Users land on /login again right after signing in, about 1 in 20 times on Safari.",
            },
            { kind: "added", sign: "+", text: "· the redirect waits for the session cookie" },
            { kind: "added", sign: "+", text: "· e2e/login.spec passes 20 times in a row" },
          ],
          thumbnails: [],
          attachments: [],
        }
      : null,
  hook_changes: null,
  kind,
});

/** An agent session's quiet row: its id copyable, its times. */
const session = (when: string, title: string, id: string, range: string): HistoryView["rows"][number] => ({
  ...row(when, title, "session", "Claude Code · sonnet"),
  tool_session_id: id,
  quiet: true,
  short_id: `${id.slice(0, 9)}…`,
  range,
});

const ACTIVITY: HistoryView = {
  filters: [
    { filter: "all", label: "All", kind: null },
    { filter: "questions", label: "Questions", kind: "question" },
    { filter: "checks", label: "Checks", kind: "check" },
    { filter: "sessions", label: "Sessions", kind: "session" },
  ],
  order: "newest first",
  copy_hint: "Copy session ID",
  copied: "Copied",
  empty: "Nothing has happened to this ticket yet.",
  total: null,
  key: "SHOP-22",
  rows: [
    row("2:23 pm", "Interrupted by restart", "session", "conversation saved", { kind: "otaq" }),
    row("2:20 pm", "You commented", "comment", "“Don’t touch the auth middleware’s public API.”", {
      kind: "you",
    }),
    row("2:16 pm", "Moved to Dev", "stage", "Product spec passed", { kind: "you" }),
    row("2:15 pm", "Question answered · keep the old session cookie", "question", null, { kind: "you" }),
    row("2:14 pm", "Spec writer · Product spec commented", "comment", "“Repro: 18 of 20 pass on Safari…”"),
    row("2:14 pm", "Spec writer · Product spec edited the description · added 2 lines", "description"),
    session(
      "2:05 pm",
      "Session · Product spec · Spec writer",
      "8b2f61d3-4c1e-4a7b-9f00-1d2e3f4a5b6c",
      "2:05 pm–2:16 pm",
    ),
    row("2:02 pm", "You created the ticket", "other", null, { kind: "you" }),
  ],
};

export function SessionGallery() {
  const [picked, setPicked] = useState<SheetContent | null>(null);
  return (
    <div className="gallery-stack">
      {SWITCHES.map((s) => (
        <figure key={s.caption} className="gallery-stack-tight">
          <figcaption className="gallery-caption">{s.caption}</figcaption>
          <div className="session-line">
            <SessionSwitch parts={s.parts} shown={picked ?? s.shown} label="Session" onPick={setPicked} />
            <span className="session-line-text">{s.line}</span>
            <span className="terminal-tab-spacer" />
            {s.right !== "" && <span className="session-typing">{s.right}</span>}
          </div>
        </figure>
      ))}
      <figure className="gallery-stack-tight">
        <figcaption className="gallery-caption">History · SHOP-22, newest first</figcaption>
        <HistoryBody view={ACTIVITY} />
      </figure>
    </div>
  );
}

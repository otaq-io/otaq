// The gallery's Session and Activity (T-0466; canvas `s25-06`, `s25-07`): the switch in each state (Agent working,
// waiting for you; the check's agent running and waiting; Run up, failed), the agent's line and Run's line, and
// Activity's filters over a short story. The words are `session_tab.rs`' and `history.rs`' for the example tickets.
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
    parts: [part("agent", "Agent", "waiting"), part("run", "Run", "running"), part("shell", "Shell")],
    shown: "agent",
    line: "Rust developer · Codex · gpt-5.5 · ~/.otaq/wt/shop-web/SHOP-22",
    right: "Typing goes to Rust developer · SHOP-22",
  },
  {
    caption: "SHOP-30 · validating: the Validator runs",
    parts: [
      part("agent", "Agent"),
      part("check", "Validator", "running"),
      part("run", "Run"),
      part("shell", "Shell"),
    ],
    shown: "check",
    line: "Validator · Codex · gpt-5.5 · ~/.otaq/wt/shop-web/SHOP-30",
    right: "Click to type to Validator · SHOP-30",
  },
  {
    caption: "SHOP-19 · in Security: the Security reviewer asks you",
    parts: [
      part("agent", "Agent"),
      part("check", "Security reviewer", "waiting"),
      part("run", "Run"),
      part("shell", "Shell"),
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
      part("shell", "Shell", "running"),
    ],
    shown: "run",
    line: "pnpm dev · in SHOP-22’s worktree · :4022",
    right: "Open http://localhost:4022",
  },
  {
    caption: "SHOP-22 · Run failed to start",
    parts: [part("agent", "Agent"), part("run", "Run", "problem"), part("shell", "Shell")],
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
): HistoryView["rows"][number] => ({
  title,
  detail,
  when,
  duration: "—",
  tokens: "—",
  tool_session_id: null,
  is_session: false,
  icon: "history",
  palette: null,
  mark: kind === "comment" || kind === "description" ? { kind: "agent" } : null,
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
          report: null,
        }
      : null,
  kind,
});

const ACTIVITY: HistoryView = {
  columns: ["EVENT", "WHEN", "DURATION", "TOKENS"],
  filters: [
    { filter: "all", label: "All", kind: null },
    { filter: "comments", label: "Comments", kind: "comment" },
    { filter: "stages", label: "Stages", kind: "stage" },
    { filter: "description", label: "Description", kind: "description" },
  ],
  order: "oldest first",
  copy_hint: "Copy session ID",
  copied: "Copied",
  empty: "Nothing has happened to this ticket yet.",
  total: null,
  key: "SHOP-22",
  rows: [
    row("14:02", "Created", "other", "by you"),
    row("14:05", "Session started on Griller · Claude Code · sonnet", "stage"),
    row("14:14", "Griller · Grill edited the description · added 2 lines", "description"),
    row("14:14", "Griller · Grill commented", "comment", "“Repro: 18 of 20 pass on Safari…”"),
    row("14:16", "Moved to Dev", "stage", "Grill passed"),
    row("14:20", "You commented", "comment", "“Don’t touch the auth middleware’s public API.”"),
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
        <figcaption className="gallery-caption">Activity · SHOP-22, oldest first</figcaption>
        <HistoryBody view={ACTIVITY} />
      </figure>
    </div>
  );
}

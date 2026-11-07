// What the demo shows, worked out from its state (the canvas's `renderVals` in Demo3.dc.html, with the app's own
// names): plain functions, so React only draws their result and calls the store's methods.

import {
  type Answer,
  type CheckResult,
  type Column,
  COLUMN_LINE,
  type DiffFile,
  FLOWS,
  type Flow,
  type FlowId,
  type HistoryEntry,
  KIND_COLOR,
  L,
  type Line,
  MODELS,
  type Piece,
  PRESET_IDS,
  type Priority,
  project,
  type Project,
  PROJECTS,
  type ProjectKey,
  type StageKind,
  type Ticket,
  type TicketStatus,
  TOOL_NAMES,
  type ToolId,
  WHAT_HAPPENS,
} from "./data";
import type { Comment, DemoState, PaneTab, SessionPart } from "./state";

/** A project's flow in `s`. */
export function flowOf(s: DemoState, p: ProjectKey): Flow {
  return FLOWS[s.flows[p]];
}

/** The name of the column `t` is in: one of its flow's stages, Backlog or Done. */
export function stageName(s: DemoState, t: Ticket): string {
  const st = flowOf(s, t.p).stages.find((x) => x.id === t.col);
  if (st) return st.name;
  return t.col === "backlog" ? "Backlog" : "Done";
}

const asking = (t: Ticket) => t.st === "attn" || t.st === "helpers" || t.st === "failed";
const running = (t: Ticket) =>
  t.st === "working" ||
  t.st === "working2" ||
  t.st === "checking" ||
  t.st === "checking2" ||
  t.st === "helpers";
/** Started, but waiting at the top of its first stage for what it waits for or a free agent. */
const waitingToStart = (t: Ticket) => t.st === "queued" && t.col !== "backlog";

// --- Needs you -----------------------------------------------------------------------------------------------------

export interface NeedItem {
  k: string;
  p: ProjectKey;
  why: string;
  /** In the strip's Your review part: a ticket ready for you. */
  review: boolean;
}

/** What needs you in project `p`, in the strip's order: questions and decisions first, then Your review. */
export function needsOf(tickets: readonly Ticket[], p: ProjectKey): NeedItem[] {
  const items: NeedItem[] = [];
  for (const t of tickets)
    if (t.p === p && t.ask && asking(t)) items.push({ k: t.k, p, why: t.ask.short, review: false });
  for (const t of tickets)
    if (t.p === p && t.st === "review") items.push({ k: t.k, p, why: "ready", review: true });
  return items;
}

// --- status --------------------------------------------------------------------------------------------------------

/** A ticket's status line on its card and the pill in its sheet: the drawn class, icon and words (`stMap`). */
export interface Status {
  cls: "" | "w" | "pa" | "a" | "i" | "r" | "ok";
  icon: string;
  label: string;
}

const askLine: Record<string, string> = {
  "SHOP-22": "allow rm -rf .next?",
  "SHOP-12": "follow the system setting?",
};

export function status(t: Ticket): Status {
  const st: TicketStatus = t.st;
  switch (st) {
    case "queued":
      if (t.col === "backlog") return { cls: "", icon: "activity", label: "Not started" };
      return t.waits.length
        ? { cls: "", icon: "hourglass", label: "Waits for " + t.waits.join(", ") + " to merge" }
        : { cls: "", icon: "hourglass", label: "Waits for a free agent · 1st in line" };
    case "working":
      return { cls: "w", icon: "loader", label: "Working" };
    case "working2":
      return { cls: "pa", icon: "undo", label: "Working · back from Security · round 2" };
    case "attn":
      return {
        cls: "a",
        icon: "message-q",
        label:
          (t.ask?.kind === "perm" ? "Needs you: " : "Asks you: ") + (askLine[t.k] ?? "prorate to the day?"),
      };
    case "helpers":
      return { cls: "a", icon: "message-q", label: "40.2 asks: keep the heart filled after sign-out?" };
    case "checking":
      return { cls: "i", icon: "shield-check", label: "Checking · round 1 of 3" };
    case "checking2":
      return { cls: "i", icon: "shield-check", label: "Checking · round 2 · back from Dev" };
    case "failed":
      return { cls: "a", icon: "circle-alert", label: "Security failed 3 times · decide" };
    case "review":
      return {
        cls: "r",
        icon: "eye",
        label:
          "Ready for you · passed " + ((t.checks ?? []).length === 2 ? "Security and Validate" : "Validate"),
      };
    case "merging":
      return { cls: "ok", icon: "loader", label: "Merge check · pnpm test…" };
    case "pr":
      return {
        cls: "ok",
        icon: "git-pr",
        label: "PR open · " + (t.k === "SHOP-26" ? "#481" : "#492") + " · Mark merged when it lands",
      };
    case "done":
      return { cls: "", icon: "git-merge", label: "Merged into main" };
  }
}

/** The sheet's pill class for a status class. */
export const PILL_CLASS: Record<Status["cls"], string> = {
  a: "pl-a",
  w: "pl-w",
  pa: "pl-pa",
  i: "pl-i",
  r: "pl-r",
  ok: "pl-ok",
  "": "pl-q",
};

// --- the board -----------------------------------------------------------------------------------------------------

export interface CardView {
  k: string;
  title: string;
  tool: ToolId;
  agent: string;
  ago: string;
  prio: Priority;
  /** P1 and P2 show their priority. */
  hasPrio: boolean;
  status: Status;
  /** `attn`, `sel` and `drag`, as drawn. */
  cls: string;
  /** Backlog cards, and those still waiting at the top of their first stage, can be dragged. */
  drag: boolean;
  pieces: readonly Piece[];
  /** Up to three pieces not merged yet, for the card. */
  piecesShort: readonly Piece[];
  pieceSum: string;
  /** The card's button: Start, Review, Decide, Answer or Reply; null when there's nothing to do. */
  act: string | null;
}

/** A column header in Edit flow: its stage, as the app's flow editor shows it. */
export interface StageHead {
  what: string;
  who: string;
  skill: string;
  /** "3 rounds · fails go to Dev", "Fails go to Dev", or Done's delivery. */
  line: string;
  sel: boolean;
  fixed: boolean;
}

export interface ColumnView {
  id: Column;
  title: string;
  color: string;
  sub: string;
  cards: CardView[];
  emptyText: string;
  /** While dragging: `drop` on the column under the card, `nodrop` on the ones it can't go to. */
  cls: string;
  /** In Edit flow, the stage the header stands for. */
  head: StageHead | null;
}

export interface ProjectTab {
  key: ProjectKey;
  name: string;
  on: boolean;
  /** How many questions and decisions wait in it; tickets ready for review aren't counted. */
  badge: number;
}

export interface RunningTool {
  id: ToolId;
  name: string;
  running: string;
  limit: string;
  cls: string;
  /** The limit bar's width in percent, or null when the tool reports no limit. */
  pct: number | null;
  barColor: string;
  tokens: string;
}

export interface BoardView {
  proj: Project;
  /** "3 running": the header's Running button, shown only while agents run. */
  runningLabel: string | null;
  hintText: string;
  projects: ProjectTab[];
  /** Questions and decisions: the strip's Needs you. */
  needs: NeedItem[];
  /** Tickets ready for you: the strip's Your review. */
  reviews: NeedItem[];
  columns: ColumnView[];
  doneCount: number;
  /** Done's line: how work leaves the flow. */
  delivery: string;
  runningTools: RunningTool[];
  pause: { label: string; icon: string; cls: string; scope: string; menuTitle: string };
}

function card(s: DemoState, t: Ticket): CardView {
  const pieces = t.pieces ?? [];
  return {
    k: t.k,
    title: t.title,
    tool: t.tool,
    agent: t.agent,
    ago: t.ago,
    prio: t.prio,
    hasPrio: t.prio <= 2,
    status: status(t),
    cls: [asking(t) ? "attn" : "", s.openKey === t.k ? "sel" : "", s.dragKey === t.k ? "drag" : ""]
      .filter(Boolean)
      .join(" "),
    drag: t.col === "backlog" || waitingToStart(t),
    pieces,
    piecesShort: pieces.filter((pc) => pc.s !== "merged").slice(0, 3),
    pieceSum: String(pieces.filter((pc) => pc.s === "merged").length) + " merged",
    act:
      t.col === "backlog"
        ? "Start"
        : t.st === "review"
          ? "Review"
          : t.st === "failed"
            ? "Decide"
            : t.ask
              ? t.ask.kind === "perm"
                ? "Answer"
                : "Reply"
              : null,
  };
}

/** The open project's tickets that match the search. */
function shown(s: DemoState): Ticket[] {
  const q = s.search.trim().toLowerCase();
  const mine = s.tickets.filter((t) => t.p === s.project);
  return q ? mine.filter((t) => (t.k + " " + t.title).toLowerCase().includes(q)) : mine;
}

/** The flow the board shows: the project's, or the draft while Edit flow is open. */
function boardFlow(s: DemoState): Flow {
  return s.flowOpen && s.flowDraftId ? FLOWS[s.flowDraftId] : flowOf(s, s.project);
}

const failLine = (kind: StageKind, fail: string) =>
  kind === "check" ? "3 rounds · fails go to " + fail : kind === "you" ? "Fails go to " + fail : "";

/** The board's columns: Backlog, the project's flow, then Done once it's unfolded (always in Edit flow). */
export function columnsOf(s: DemoState): ColumnView[] {
  const mine = shown(s);
  const flow = boardFlow(s);
  const first = flow.stages[0]?.id;
  const defs: { id: Column; name: string; kind: StageKind; head: Omit<StageHead, "sel" | "fixed"> | null }[] =
    [
      { id: "backlog", name: "Backlog", kind: "wait", head: null },
      ...flow.stages.map((st) => ({
        id: st.id,
        name: st.name,
        kind: st.kind,
        head: { what: WHAT_HAPPENS[st.kind], who: st.who, skill: st.skill, line: failLine(st.kind, st.fail) },
      })),
    ];
  if (s.doneOpen || s.flowOpen) defs.push({ id: "done", name: "Done", kind: "delivery", head: null });
  // Tickets waiting to start sit at the top of their column, then the ones that need you, then by priority.
  const weight = (t: Ticket) => (waitingToStart(t) ? 0 : asking(t) ? 1 : 2);
  return defs.map((c) => {
    const droppable = c.id === "backlog" || c.id === first;
    const fixed = c.id === "backlog" || c.id === "done";
    return {
      id: c.id,
      title: c.name,
      color: KIND_COLOR[c.kind],
      sub: c.id === "done" ? flow.delivery : COLUMN_LINE[c.kind],
      cards: mine
        .filter((t) => t.col === c.id)
        .sort((a, b) => weight(a) - weight(b) || a.prio - b.prio)
        .map((t) => card(s, t)),
      emptyText:
        c.kind === "wait"
          ? "New tickets wait here until you start them"
          : c.kind === "you"
            ? "Agents move their ticket here when they submit it for your review"
            : c.kind === "check"
              ? "An agent checks each ticket here before it moves on"
              : c.kind === "agent"
                ? "Tickets move here when their agent starts"
                : "Nothing here",
      cls: s.dragKey ? (s.dropCol === c.id ? "drop" : droppable ? "" : "nodrop") : "",
      head: s.flowOpen
        ? {
            ...(c.head ?? {
              what: WHAT_HAPPENS[c.kind],
              who: "",
              skill: "",
              line: c.id === "done" ? flow.delivery : "",
            }),
            sel: s.flowSel === c.id,
            fixed,
          }
        : null,
    };
  });
}

function runningTools(s: DemoState): RunningTool[] {
  const byTool: Record<ToolId, number> = { CC: 0, CX: 0, CP: 0, OC: 0 };
  for (const t of s.tickets) if (running(t)) byTool[t.tool] += 1;
  if (s.tickets.some((t) => t.st === "checking")) byTool.CX += 1;
  const run = (id: ToolId) => String(byTool[id]) + " running";
  return [
    {
      id: "CC",
      name: "Claude Code",
      running: run("CC"),
      limit: "5h: 18% left · resets 3:45 pm · week: 60% left",
      cls: "pa",
      pct: 18,
      barColor: "var(--pau)",
      tokens: "today 412k tokens",
    },
    {
      id: "CX",
      name: "Codex",
      running: run("CX"),
      limit: "week: 71% left · credits: 120",
      cls: "",
      pct: 71,
      barColor: "var(--ok)",
      tokens: "today 96k tokens",
    },
    {
      id: "CP",
      name: "Copilot",
      running: run("CP"),
      limit: "month: 80% left · resets Nov 1",
      cls: "",
      pct: 80,
      barColor: "var(--ok)",
      tokens: "today 31k tokens",
    },
    {
      id: "OC",
      name: "OpenCode",
      running: run("OC"),
      limit: "Limits not reported · tokens only",
      cls: "",
      pct: null,
      barColor: "",
      tokens: "today 6k tokens",
    },
  ];
}

export function boardView(s: DemoState): BoardView {
  const proj = project(s.project);
  const n = s.tickets.filter(running).length;
  const all = needsOf(s.tickets, s.project);
  return {
    proj,
    runningLabel: n > 0 ? String(n) + " running" : null,
    hintText:
      s.view === "board"
        ? "Start a Backlog card, or drag it into " +
          (flowOf(s, s.project).stages[0]?.name ?? "the first stage")
        : "The board’s tickets, as a list",
    projects: PROJECTS.map((p) => ({
      key: p.key,
      name: p.name,
      on: p.key === s.project,
      badge: needsOf(s.tickets, p.key).filter((x) => !x.review).length,
    })),
    needs: all.filter((x) => !x.review),
    reviews: all.filter((x) => x.review),
    columns: columnsOf(s),
    doneCount: shown(s).filter((x) => x.col === "done").length,
    delivery: flowOf(s, s.project).delivery,
    runningTools: runningTools(s),
    pause: {
      label: s.paused ? "Resume" : "Pause",
      icon: s.paused ? "play" : "pause",
      cls: s.paused ? "pau" : "out",
      scope: s.paused === "all" ? "all projects" : proj.name,
      menuTitle: s.paused ? "Resume" : "Pause",
    },
  };
}

// --- the ticket sheet ----------------------------------------------------------------------------------------------

/** The sheet's tabs: id, label, icon and key. */
export const PANE_TABS: readonly (readonly [PaneTab, string, string, string])[] = [
  ["overview", "Overview", "file-text", "1"],
  ["session", "Session", "terminal", "2"],
  ["changes", "Changes", "file-diff", "3"],
  ["history", "History", "history", "4"],
];

export interface StepView {
  name: string;
  state: "done" | "current" | "to";
  /** The current step glows while the ticket waits for you. */
  attn: boolean;
}

export interface CheckView {
  name: string;
  tool: ToolId;
  verdict: string;
  model: string;
  color: string;
  s: string;
}

export interface ManualView {
  t: string;
  on: boolean;
  src: string;
}

export interface Attachment {
  name: string;
  size: string;
  tool: ToolId;
  who: string;
  stage: string;
  line: string;
}

export interface PartView {
  id: SessionPart;
  label: string;
  on: boolean;
  dot: string | null;
}

export interface TicketView {
  k: string;
  title: string;
  prio: Priority;
  pill: { cls: string; icon: string; label: string; spin: boolean };
  stageName: string;
  tool: ToolId;
  agent: string;
  model: string;
  stateLabel: string;
  stateValue: string;
  canApprove: boolean;
  /** Approve & open PR is offered: the flow delivers by pull request, or either way. */
  canPR: boolean;
  canDecide: boolean;
  canStart: boolean;
  isRunning: boolean;
  isPR: boolean;
  ask: { title: string; icon: string; opts: { n: number; label: string; rec: string; act: Answer }[] } | null;
  /** The number of the Other… option, after the ask's own. */
  otherN: number;
  desc: string;
  acc: readonly string[];
  pieces: readonly Piece[];
  helperCount: number;
  before: { head: string; sum: string; checks: CheckView[] } | null;
  decision: { t: string; s: string } | null;
  manual: ManualView[];
  /** Dependencies: what it waits for, and what waits for it. */
  waitsFor: string;
  waitingForIt: string;
  startNote: string;
  comments: Comment[];
  atts: Attachment[];
  steps: StepView[];
  parts: PartView[];
  log: readonly Line[];
  checkLog: readonly Line[];
  adds: number;
  dels: number;
  files: readonly DiffFile[];
  hist: readonly HistoryEntry[];
  port: number;
  rbarNote: string;
  /** The tabs' live dot (Session) and count (Changes). */
  liveSession: boolean;
  fileCount: number;
}

const isSecurity = (ck: CheckResult) => ck.t.startsWith("Security");

/** The open ticket's sheet, or null when none is open. */
export function ticketView(s: DemoState): TicketView | null {
  const ot = s.openKey ? s.tickets.find((t) => t.k === s.openKey) : undefined;
  if (!ot) return null;
  const st = status(ot);
  const flow = flowOf(s, ot.p);
  const curAttn = asking(ot);
  const runningT = running(ot);
  const checks = ot.checks ?? [];
  const pieces = ot.pieces ?? [];
  const beforeHead = checks.length === 2 ? "Security and Validate" : "Validate";

  // The path, as the app's sheet draws it: the flow's stages, then Delivery.
  const path: { name: string; id: Column | "delivery" }[] = [
    ...flow.stages.map((x) => ({ name: x.name, id: x.id })),
    { name: "Delivery", id: "delivery" },
  ];
  const cur =
    ot.col === "done"
      ? path.length
      : ot.col === "backlog"
        ? -1
        : ot.st === "pr" || ot.st === "merging"
          ? path.length - 1
          : path.findIndex((x) => x.id === ot.col);

  const unmet = ot.waits.filter((w) => {
    const x = s.tickets.find((y) => y.k === w);
    return x !== undefined && x.col !== "done";
  });
  const waitingForIt = s.tickets.filter((x) => x.waits.includes(ot.k)).map((x) => x.k);

  const comments: Comment[] = [...(s.comments[ot.k] ?? [])];
  if (ot.by === "Noyan" || ot.k === "SHOP-29")
    comments.unshift({
      tool: "CC",
      who: "Noyan",
      at: "yesterday",
      stage: "Backlog",
      t: "Filed from your note. The coupon code must never reach SQL unescaped; see lib/db.ts for the query helper.",
    });
  if (ot.st === "review")
    comments.unshift({
      tool: ot.tool,
      who: ot.agent,
      at: ot.ago || "now",
      stage: "Dev → " + (flow.stages.find((x) => x.kind === "check")?.name ?? "Review"),
      t: "Submitted. Summary in the attached report. One decision recorded.",
    });

  const atts: Attachment[] = [];
  if (ot.adds)
    atts.push({
      name: "Summary.md",
      size: "2 KB",
      tool: ot.tool,
      who: ot.agent,
      stage: "Dev",
      line: "What changed, and the one decision I made.",
    });
  for (const ck of checks) {
    const head = ck.t.split(" · ")[0] ?? ck.t;
    atts.push({
      name: head + " report.md",
      size: "4 KB",
      tool: isSecurity(ck) ? "CC" : "CX",
      who: isSecurity(ck) ? "Security reviewer" : "Validator",
      stage: head,
      line: ck.s,
    });
  }
  if (ot.st === "failed")
    atts.push({
      name: "Security report · round 3.md",
      size: "3 KB",
      tool: "CC",
      who: "Security reviewer",
      stage: "Security",
      line: "Unescaped SQL in lib/address.ts, line 41.",
    });

  // Session's switch names each side by what it is: Agent, the check's stage, Run and Terminal.
  const checking = ot.st === "checking" || ot.st === "checking2";
  const checkStage =
    ot.col === "validate" ? "Validate" : ot.st === "failed" || checking ? "Security" : "Validate";
  const partDefs: { id: SessionPart; label: string; dot: string | null }[] = [
    {
      id: "agent",
      label: "Agent",
      dot: runningT && !checking ? "var(--ok)" : curAttn && ot.st !== "failed" ? "var(--attn)" : null,
    },
  ];
  if (checks.length || checking || ot.st === "failed")
    partDefs.push({
      id: "check",
      label: checkStage,
      dot: checking ? "var(--ok)" : ot.st === "failed" ? "var(--attn)" : null,
    });
  partDefs.push({ id: "run", label: "Run", dot: null }, { id: "terminal", label: "Terminal", dot: null });

  const tokensSuffix = ot.tokens ? " · " + ot.tokens + " tokens" : "";
  const askIcon = ot.ask?.kind === "perm" ? "hand" : ot.ask?.kind === "decide" ? "circle-alert" : "message-q";

  return {
    k: ot.k,
    title: ot.title,
    prio: ot.prio,
    pill: { cls: PILL_CLASS[st.cls], icon: st.icon, label: st.label, spin: st.icon === "loader" },
    stageName: stageName(s, ot),
    tool: ot.tool,
    agent: ot.agent,
    model: TOOL_NAMES[ot.tool] + " · " + (MODELS[ot.agent] ?? "default model"),
    stateLabel: runningT
      ? "Working"
      : ot.st === "attn"
        ? "Waiting"
        : ot.st === "review"
          ? "Ready for you"
          : ot.st === "done"
            ? "Merged"
            : ot.st === "pr"
              ? "PR open"
              : "Status",
    stateValue: runningT
      ? (ot.ago || "now") + tokensSuffix
      : ot.st === "attn"
        ? "since " + (ot.ago || "now") + tokensSuffix
        : ot.tokens
          ? ot.tokens + " tokens"
          : "—",
    canApprove: ot.st === "review",
    canPR: flow.pr,
    canDecide: ot.st === "failed",
    canStart: ot.col === "backlog",
    isRunning: runningT && ot.st !== "helpers",
    isPR: ot.st === "pr",
    ask:
      ot.ask && curAttn
        ? {
            title: ot.ask.title,
            icon: askIcon,
            opts: ot.ask.opts.map((o, i) => ({ n: i + 1, label: o.label, rec: o.rec, act: o.act })),
          }
        : null,
    otherN: (ot.ask ? ot.ask.opts.length : 0) + 1,
    desc: ot.desc ?? "",
    acc: ot.acc,
    pieces,
    helperCount: pieces.filter((p) => p.s !== "merged").length,
    before:
      checks.length && ot.st === "review"
        ? {
            head: beforeHead,
            sum: String(checks.length) + (checks.length === 1 ? " check · passed" : " checks · both passed"),
            checks: checks.map((ck) => ({
              name: isSecurity(ck) ? "Security reviewer" : "Validator",
              tool: isSecurity(ck) ? "CC" : "CX",
              verdict: ck.t.includes("failed") ? "failed" : ck.t.split(" · ").slice(1).join(" · "),
              model: isSecurity(ck) ? "Claude Code · opus · effort high" : "Codex · gpt-sol",
              color: ck.color,
              s: ck.s,
            })),
          }
        : null,
    decision: ot.decision ? { t: ot.decisionT ?? "", s: ot.decision } : null,
    manual: (ot.manual ?? []).map((t, i) => ({
      t,
      on: !!s.manualDone[ot.k + ":" + String(i)],
      src: i === 0 ? "Validator" : "Security reviewer",
    })),
    waitsFor: ot.waits.length
      ? ot.waits.join(", ") + (unmet.length ? " · not merged yet" : " · merged")
      : ot.k + " waits for nothing.",
    waitingForIt: waitingForIt.length ? waitingForIt.join(", ") : "Nothing waits for " + ot.k + ".",
    startNote: unmet.length
      ? "Start puts it at the top of " +
        (flow.stages[0]?.name ?? "the first stage") +
        ", where it waits for " +
        unmet.join(", ") +
        " to merge"
      : "or drag the card into " + (flow.stages[0]?.name ?? "the first stage"),
    comments,
    atts,
    steps: path.map((x, i) => {
      const state = i < cur ? "done" : i === cur ? "current" : "to";
      return { name: x.name, state, attn: state === "current" && curAttn };
    }),
    parts: partDefs.map((p) => ({ ...p, on: s.part === p.id })),
    log: ot.log,
    checkLog:
      ot.st === "failed"
        ? [
            L("d", "Security reviewer · Claude Code · round 3 of 3"),
            L("tool", "/security-check"),
            L("e", "Failed: lib/address.ts:41 concatenates the query parameter into SQL."),
          ]
        : ot.st === "checking2"
          ? [
              L("d", "Security reviewer · Claude Code · round 2 of 3"),
              L("tool", "/security-check"),
              L("a", "Round 1 found unescaped SQL. Reading the fix in lib/coupon.ts."),
            ]
          : [
              L("d", "Validator · Codex · round 1"),
              L("tool", "/validate"),
              L("ok", "✓ tests pass · lint clean · change matches the ticket"),
            ],
    adds: ot.adds ?? 0,
    dels: ot.dels ?? 0,
    files: ot.files ?? [],
    hist: ot.hist.length
      ? [...ot.hist].reverse()
      : [
          { at: "today", t: "In " + stageName(s, ot) },
          { at: "yesterday", t: "Filed by " + (ot.by ?? "you") },
        ],
    port: 4000 + (parseInt(ot.k.replace(/\D/g, ""), 10) || 0),
    rbarNote: "Passed " + beforeHead + ". Send back goes to Dev with your notes.",
    liveSession: runningT,
    fileCount: (ot.files ?? []).length,
  };
}

// --- Edit flow -----------------------------------------------------------------------------------------------------

export interface StartFromItem {
  id: FlowId;
  name: string;
  desc: string;
  on: boolean;
}

/** A clicked column header's details: Name, What happens, Agent, Skill, Fails go to; or Done's delivery. */
export interface StageDetails {
  name: string;
  rows: readonly (readonly [label: string, value: string])[];
}

export interface EditFlowView {
  /** "Editing shop-web's flow". */
  title: string;
  /** "Drag to reorder · click a stage for details · + to add", or what the draft starts from. */
  hint: string;
  startFrom: StartFromItem[];
  /** The picked stage's details, Done's delivery, or null. */
  details: StageDetails | null;
  changed: boolean;
}

export function editFlowView(s: DemoState): EditFlowView {
  const proj = project(s.project);
  const inUseId = s.flows[s.project];
  const draftId = s.flowDraftId ?? inUseId;
  const draft = FLOWS[draftId];
  const sel = draft.stages.find((x) => x.id === s.flowSel);
  return {
    title: "Editing " + proj.name + "’s flow",
    hint:
      draftId === inUseId
        ? "Drag to reorder · click a stage for details · + to add"
        : "Starting from " + draft.name + " · nothing changes until you save",
    startFrom: PRESET_IDS.filter((id) => id !== "shop" || s.project === "SHOP").map((id) => ({
      id,
      name: FLOWS[id].name,
      desc: id === inUseId ? "In use" : FLOWS[id].desc,
      on: draftId === id,
    })),
    details: sel
      ? {
          name: sel.name,
          rows: [
            ["What happens", WHAT_HAPPENS[sel.kind]],
            ["Agent", sel.who],
            ["Skill", sel.skill],
            ["Fails go to", sel.fail],
          ],
        }
      : s.flowSel === "done"
        ? {
            name: "Done · how work leaves the flow",
            rows: [
              ["Deliver as", draft.delivery.split(" · ")[0] ?? ""],
              ["When", draft.delivery.split(" · ")[1] ?? ""],
            ],
          }
        : null,
    changed: draftId !== inUseId,
  };
}

// What the demo shows, worked out from its state (the canvas's `renderVals` in Demo3.dc.html): plain functions, so
// React only draws their result and calls the store's methods.

import {
  type Answer,
  type CheckResult,
  type Column,
  type DiffFile,
  FLOWS,
  type Flow,
  type FlowId,
  type HistoryEntry,
  KIND,
  KIND_COLOR,
  KIND_LONG,
  L,
  type Line,
  MODELS,
  type NoyanMsg,
  type Piece,
  PRESET_IDS,
  type Priority,
  project,
  type Project,
  PROJECTS,
  type ProjectKey,
  type StageId,
  type StageKind,
  type Ticket,
  type TicketStatus,
  TOOL_NAMES,
  type ToolId,
} from "./data";
import type { Comment, DemoState, PaneTab, SessionPart } from "./state";

/** The demo board's width: click-placed menus are clamped inside it. */
export const DEMO_WIDTH = 1280;

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

// --- Needs you -----------------------------------------------------------------------------------------------------

export interface NeedItem {
  k: string;
  p: ProjectKey;
  why: string;
}

/** What needs you in project `p`, in the strip's order: questions and decisions first, then your checks. */
export function needsOf(tickets: readonly Ticket[], p: ProjectKey): NeedItem[] {
  const items: NeedItem[] = [];
  for (const t of tickets) if (t.p === p && t.ask && asking(t)) items.push({ k: t.k, p, why: t.ask.short });
  for (const t of tickets) if (t.p === p && t.st === "review") items.push({ k: t.k, p, why: "your check" });
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
      return {
        cls: "",
        icon: "activity",
        label:
          t.col === "backlog"
            ? t.prio === 1
              ? "P1 · not started"
              : "not started"
            : t.waits.length
              ? "waits for " + t.waits.join(", ") + " to merge"
              : "starts in priority order",
      };
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
      return { cls: "a", icon: "circle-alert", label: "Failed 3 times: decide" };
    case "review":
      return {
        cls: "r",
        icon: "eye",
        label:
          "Your check · passed " + ((t.checks ?? []).length === 2 ? "Security and Validate" : "Validate"),
      };
    case "merging":
      return { cls: "ok", icon: "loader", label: "Merge check · pnpm test…" };
    case "pr":
      return {
        cls: "ok",
        icon: "git-pr",
        label: (t.k === "SHOP-26" ? "PR #481" : "PR #492") + " open · mark merged when it lands",
      };
    case "done":
      return { cls: "", icon: "git-merge", label: "Merged" };
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
  /** Only Backlog and Ready cards can be dragged. */
  drag: boolean;
  pieces: readonly Piece[];
  /** Up to three pieces not merged yet, for the card. */
  piecesShort: readonly Piece[];
  pieceSum: string;
  /** The card's button: Review, Decide, Answer or Reply; null when there's nothing to do. */
  act: string | null;
}

export interface ColumnView {
  id: Column;
  title: string;
  kind: string;
  color: string;
  sub: string;
  cards: CardView[];
  emptyText: string;
  /** While dragging: `drop` on the column under the card, `nodrop` on the ones it can't go to. */
  cls: string;
}

export interface ProjectTab {
  key: ProjectKey;
  name: string;
  on: boolean;
  /** How many questions and decisions wait in it; your checks aren't counted. */
  badge: number;
}

export interface ActivityTool {
  id: ToolId;
  name: string;
  running: string;
  limit: string;
  cls: string;
  /** The limit bar's width in percent, or null when the tool reports no limit. */
  pct: number | null;
  barColor: string;
  tokens: string;
  hasFallback: boolean;
}

export interface BoardView {
  proj: Project;
  flowLabel: string;
  runningLabel: string;
  hintText: string;
  projects: ProjectTab[];
  needs: NeedItem[];
  columns: ColumnView[];
  doneCount: number;
  actTools: ActivityTool[];
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
    drag: t.col === "backlog" || t.col === "ready",
    pieces,
    piecesShort: pieces.filter((pc) => pc.s !== "merged").slice(0, 3),
    pieceSum: String(pieces.filter((pc) => pc.s === "merged").length) + " merged",
    act:
      t.st === "review"
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

/** The board's columns: Backlog, the project's flow, then Done once it's unfolded. */
export function columnsOf(s: DemoState): ColumnView[] {
  const mine = shown(s);
  const defs: { id: Column; name: string; kind: StageKind; sub: string }[] = [
    {
      id: "backlog",
      name: "Backlog",
      kind: "wait",
      sub: "Noyan’s tickets and yours · drag to Ready, or Start",
    },
    ...flowOf(s, s.project).stages.map((st) => ({
      id: st.id,
      name: st.name,
      kind: st.kind,
      sub: st.kind === "wait" ? "Starts in priority order" : st.who,
    })),
  ];
  if (s.doneOpen) defs.push({ id: "done", name: "Done", kind: "delivery", sub: "Merged" });
  const weight = (t: Ticket) => (asking(t) ? 0 : 1);
  return defs.map((c) => {
    const droppable = c.id === "backlog" || c.id === "ready";
    return {
      id: c.id,
      title: c.name,
      kind: KIND[c.kind],
      color: KIND_COLOR[c.kind],
      sub: c.sub,
      cards: mine
        .filter((t) => t.col === c.id)
        .sort((a, b) => weight(a) - weight(b) || a.prio - b.prio)
        .map((t) => card(s, t)),
      emptyText:
        c.kind === "wait"
          ? "Drop a ticket here to start it when it’s ready"
          : c.kind === "you"
            ? "Nothing to check"
            : "Nothing here",
      cls: s.dragKey ? (s.dropCol === c.id ? "drop" : droppable ? "" : "nodrop") : "",
    };
  });
}

function activityTools(s: DemoState): ActivityTool[] {
  const byTool: Record<ToolId, number> = { CC: 0, CX: 0, CP: 0, OC: 0 };
  for (const t of s.tickets) if (running(t)) byTool[t.tool] += 1;
  if (s.tickets.some((t) => t.st === "checking")) byTool.CX += 1;
  const run = (id: ToolId) => String(byTool[id]) + " running";
  return [
    {
      id: "CC",
      name: "Claude Code",
      running: run("CC"),
      limit: s.fellBack
        ? "5h: 18% left · resets 3:45 pm · waiting agents moved to Codex"
        : "5h: 18% left · resets 3:45 pm · week: 60% left",
      cls: "pa",
      pct: 18,
      barColor: "var(--pau)",
      tokens: "today 412k tokens",
      hasFallback: !s.fellBack,
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
      hasFallback: false,
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
      hasFallback: false,
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
      hasFallback: false,
    },
  ];
}

export function boardView(s: DemoState): BoardView {
  const proj = project(s.project);
  const flow = flowOf(s, s.project);
  return {
    proj,
    flowLabel: flow.name.replace("’s flow", "") + " · " + String(flow.stages.length) + " stages",
    runningLabel: String(s.tickets.filter(running).length) + " working",
    hintText:
      s.view === "board"
        ? "Drag a Backlog card to Ready to start it · click a card to open it"
        : "Grouped by stage, in flow order",
    projects: PROJECTS.map((p) => ({
      key: p.key,
      name: p.name,
      on: p.key === s.project,
      badge: needsOf(s.tickets, p.key).filter((n) => n.why !== "your check").length,
    })),
    needs: needsOf(s.tickets, s.project),
    columns: columnsOf(s),
    doneCount: shown(s).filter((x) => x.col === "done").length,
    actTools: activityTools(s),
    pause: {
      label: s.paused ? "Resume" : "Pause",
      icon: s.paused ? "play" : "pause",
      cls: s.paused ? "pau" : "out",
      scope: s.paused === "all" ? "all projects" : proj.name,
      menuTitle: s.paused ? "Resume" : "Pause",
    },
  };
}

/** Where a column's stage menu opens, from its button's box in the demo's own coordinates (already divided by the
 * frame's scale): under the button, 180 px to its left, kept inside the board. */
export function stageMenuAt(btn: { bottom: number; left: number }): { top: number; left: number } {
  return { top: btn.bottom + 4, left: Math.max(70, Math.min(btn.left - 180, DEMO_WIDTH - 240)) };
}

// --- the ticket sheet ----------------------------------------------------------------------------------------------

/** The sheet's tabs: id, label, icon and key. */
export const PANE_TABS: readonly (readonly [PaneTab, string, string, string])[] = [
  ["overview", "Overview", "file-text", "1"],
  ["session", "Session", "terminal", "2"],
  ["files", "Files", "file-diff", "3"],
  ["activity", "Activity", "history", "4"],
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
  blockedBy: string;
  blocks: string;
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
  /** The tabs' live dot (Session) and count (Files). */
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

  const stepsAll: { name: string; id: Column }[] = [
    { name: "Backlog", id: "backlog" },
    ...flow.stages.map((x) => ({ name: x.name, id: x.id })),
    { name: "Done", id: "done" },
  ];
  const cur = stepsAll.findIndex((x) => x.id === ot.col);

  const unmet = ot.waits.filter((w) => {
    const x = s.tickets.find((y) => y.k === w);
    return x !== undefined && x.col !== "done";
  });
  const blocks = s.tickets.filter((x) => x.waits.includes(ot.k)).map((x) => x.k);

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

  const checking = ot.st === "checking" || ot.st === "checking2";
  const partDefs: { id: SessionPart; label: string; dot: string | null }[] = [
    {
      id: "agent",
      label: ot.agent,
      dot: runningT && !checking ? "var(--ok)" : curAttn && ot.st !== "failed" ? "var(--attn)" : null,
    },
  ];
  if (checks.length || checking || ot.st === "failed")
    partDefs.push({
      id: "check",
      label: ot.st === "failed" || checking ? "Security reviewer" : "Validator",
      dot: checking ? "var(--ok)" : ot.st === "failed" ? "var(--attn)" : null,
    });
  partDefs.push({ id: "run", label: "Run", dot: null }, { id: "shell", label: "Shell", dot: null });

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
          ? "Your check"
          : ot.st === "done"
            ? "Merged"
            : ot.st === "pr"
              ? "PR"
              : "Status",
    stateValue: runningT
      ? (ot.ago || "now") + tokensSuffix
      : ot.st === "attn"
        ? "since " + (ot.ago || "now") + tokensSuffix
        : ot.tokens
          ? ot.tokens + " tokens"
          : "—",
    canApprove: ot.st === "review",
    canDecide: ot.st === "failed",
    canStart: ot.col === "backlog" || ot.col === "ready",
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
            sum:
              String(checks.length) +
              (checks.length === 1 ? " check agent · passed" : " check agents · both passed"),
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
    blockedBy: ot.waits.length
      ? ot.waits.join(", ") + (unmet.length ? " · waits for " + unmet.join(", ") + " to merge" : " · merged")
      : "Nothing",
    blocks: blocks.length ? blocks.join(", ") : "Nothing",
    startNote: unmet.length
      ? "waits for " + unmet.join(", ") + " to merge, then starts on its own"
      : "or drag the card to Ready for Dev",
    comments,
    atts,
    steps: stepsAll.map((x, i) => {
      const state = i < cur ? "done" : i === cur ? "current" : "to";
      return { name: x.name, state, attn: state === "current" && curAttn };
    }),
    parts: partDefs.map((p) => ({ ...p, on: s.part === p.id })),
    log: ot.log,
    checkLog:
      ot.st === "failed"
        ? [
            L("d", "Security reviewer · Claude Code · round 3 of 3"),
            L("tool", "/security-review"),
            L("e", "Failed: lib/address.ts:41 concatenates the query parameter into SQL."),
          ]
        : ot.st === "checking2"
          ? [
              L("d", "Security reviewer · Claude Code · round 2 of 3"),
              L("tool", "/security-review"),
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
      ? ot.hist
      : [
          { at: "yesterday", t: "Filed by " + (ot.by ?? "you") },
          { at: "today", t: "In " + stageName(s, ot) },
        ],
    port: 4000 + (parseInt(ot.k.replace(/\D/g, ""), 10) || 0),
    rbarNote: "Passed " + beforeHead + ". Approve merges it the way Ship says; Send back goes to Dev.",
    liveSession: runningT,
    fileCount: (ot.files ?? []).length,
  };
}

// --- the flow editor -----------------------------------------------------------------------------------------------

export interface PresetView {
  id: FlowId;
  name: string;
  desc: string;
  on: boolean;
}

export interface DraftStage {
  id: StageId;
  name: string;
  kind: string;
  kindLong: string;
  who: string;
  skill: string;
  fail: string;
  color: string;
  /** The drawn pop-in's stagger. */
  delay: string;
  sel: boolean;
}

export interface FlowEditorView {
  presets: PresetView[];
  draftName: string;
  draftCount: number;
  /** " · in use", or which preset the draft starts from. */
  draftInUse: string;
  stages: DraftStage[];
  msgs: readonly NoyanMsg[];
  applyNote: string;
}

export function flowEditorView(s: DemoState): FlowEditorView {
  const inUseId = s.flows[s.project];
  const draftId = s.flowDraftId ?? inUseId;
  const draft = FLOWS[draftId];
  const inUse = draftId === inUseId;
  const inFlight = s.tickets.filter(
    (x) => x.p === s.project && x.col !== "backlog" && x.col !== "done" && x.col !== "ready",
  ).length;
  return {
    presets: PRESET_IDS.filter((id) => id !== "shop" || s.project === "SHOP").map((id) => ({
      id,
      name: FLOWS[id].name,
      desc: id === inUseId ? "In use · the board’s columns" : FLOWS[id].desc,
      on: draftId === id,
    })),
    draftName: draft.name,
    draftCount: draft.stages.length,
    draftInUse: inUse ? " · in use" : " · starting from " + draft.name,
    stages: draft.stages.map((st, i) => ({
      id: st.id,
      name: st.name,
      kind: KIND[st.kind],
      kindLong: KIND_LONG[st.kind],
      who: st.who,
      skill: st.skill,
      fail: st.fail,
      color: KIND_COLOR[st.kind],
      delay: String(i * 50) + "ms",
      sel: s.flowSel === st.id,
    })),
    msgs: s.flowMsgs,
    applyNote: inUse
      ? "No changes."
      : inFlight
        ? "Swapping flows waits until the board is quiet: " +
          String(inFlight) +
          " tickets are mid-flow. Tickets in Backlog and Ready follow the new flow at once."
        : "Applies at once.",
  };
}

// The demo's state and every change to it, apart from drawing (WEB-07; the app's rule in SDD §5.5): a small store
// ported from the canvas's Demo3.dc.html script and brought in line with the app's names. React subscribes and draws;
// the timed steps go through a scheduler, so tests can drive them with fake timers and `dispose()` clears every one.
// Nothing is stored and nothing leaves the page.

import {
  type Answer,
  type Column,
  FLOWS,
  type FlowId,
  initialTickets,
  L,
  type Line,
  NOYAN0,
  NOYAN_NOT_YET,
  NOYAN_REPLY,
  type NoyanMsg,
  noyanTickets,
  project,
  PROJECTS,
  type ProjectKey,
  type Stage,
  type StageId,
  T,
  type Ticket,
  type Tip,
  TIPS,
  type ToolId,
} from "./data";
import { flowOf, needsOf, stageName } from "./view";

export type PaneTab = "overview" | "session" | "changes" | "history";
export type SessionPart = "agent" | "check" | "run" | "terminal";
export type Menu = "proj" | "pause" | "running" | "approve" | "startFrom";
export type PauseScope = "this" | "all";
/** What a key does in the box for new work: ↵ creates, ⌘↵ creates and starts, ⇧↵ asks Noyan to shape it. */
export type BoxAction = "create" | "start" | "noyan";

export interface Comment {
  tool: ToolId | "U";
  who: string;
  at: string;
  stage: string;
  t: string;
  /** "· told Strong now": the comment also went to the working agent. */
  told?: string;
}

export interface DemoState {
  project: ProjectKey;
  tickets: readonly Ticket[];
  flows: Readonly<Record<ProjectKey, FlowId>>;
  view: "board" | "list";
  openKey: string | null;
  ptab: PaneTab;
  part: SessionPart;
  cdraft: string;
  comments: Readonly<Record<string, readonly Comment[]>>;
  doneOpen: boolean;
  noyanOpen: boolean;
  noyanMsgs: readonly NoyanMsg[];
  nyDraft: string;
  /** The box for new work at the bottom of the board. */
  boxDraft: string;
  tdraft: string;
  search: string;
  toast: string | null;
  paused: PauseScope | null;
  menu: Menu | null;
  /** Edit flow: the board is the flow's editor in place. */
  flowOpen: boolean;
  flowSel: StageId | "done" | null;
  flowDraftId: FlowId | null;
  /** Ticked manual checks, by `<key>:<index>`. */
  manualDone: Readonly<Record<string, boolean>>;
  dragKey: string | null;
  dropCol: Column | null;
  nextId: number;
}

export function initialState(): DemoState {
  return {
    project: "SHOP",
    tickets: initialTickets(),
    flows: { SHOP: project("SHOP").flow, BILL: project("BILL").flow },
    view: "board",
    openKey: null,
    ptab: "overview",
    part: "agent",
    cdraft: "",
    comments: {},
    doneOpen: false,
    noyanOpen: false,
    noyanMsgs: NOYAN0,
    nyDraft: "",
    boxDraft: "",
    tdraft: "",
    search: "",
    toast: null,
    paused: null,
    menu: null,
    flowOpen: false,
    flowSel: null,
    flowDraftId: null,
    manualDone: {},
    dragKey: null,
    dropCol: null,
    nextId: 45,
  };
}

/** Where the timed steps go: the browser's timers by default, fake ones in tests. */
export interface Scheduler {
  setTimeout(fn: () => void, ms: number): unknown;
  clearTimeout(handle: unknown): void;
}

const browserScheduler: Scheduler = {
  setTimeout: (fn, ms) => globalThis.setTimeout(fn, ms),
  clearTimeout: (handle) => {
    globalThis.clearTimeout(handle as ReturnType<typeof globalThis.setTimeout>);
  },
};

/** How long the toast stays (`say` in the canvas). */
export const TOAST_MS = 3600;
/** How long a merge's merge check runs. */
export const MERGE_MS = 1800;
/** How long after a merge the tickets waiting for it start. */
export const WAIT_START_MS = 800;
/** How long SHOP-22 builds after you allow its command, before it's submitted. */
export const BUILD_MS = 3000;
/** How long Noyan takes to reply in its panel. */
export const NOYAN_MS = 900;

export class DemoStore {
  private state: DemoState = initialState();
  private readonly listeners = new Set<() => void>();
  private readonly timers = new Set<unknown>();
  private toastTimer: unknown = null;

  constructor(private readonly scheduler: Scheduler = browserScheduler) {}

  // --- the store -----------------------------------------------------------------------------------------------

  getState = (): DemoState => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  private set(patch: Partial<DemoState> | ((s: DemoState) => Partial<DemoState>)): void {
    const p = typeof patch === "function" ? patch(this.state) : patch;
    this.state = { ...this.state, ...p };
    for (const l of this.listeners) l();
  }

  /** Clears every timer (the island unmounts). */
  dispose(): void {
    for (const h of this.timers) this.scheduler.clearTimeout(h);
    this.timers.clear();
    this.toastTimer = null;
  }

  /** How many timers are waiting; for tests. */
  pendingTimers(): number {
    return this.timers.size;
  }

  private later(ms: number, fn: () => void): unknown {
    const h = this.scheduler.setTimeout(() => {
      this.timers.delete(h);
      fn();
    }, ms);
    this.timers.add(h);
    return h;
  }

  say(text: string): void {
    if (this.toastTimer !== null) {
      this.scheduler.clearTimeout(this.toastTimer);
      this.timers.delete(this.toastTimer);
    }
    this.set({ toast: text });
    this.toastTimer = this.later(TOAST_MS, () => {
      this.toastTimer = null;
      this.set({ toast: null });
    });
  }

  /** A button the demo doesn't build says what it does in the app. */
  tip(name: Tip): void {
    this.say(TIPS[name]);
  }

  find(k: string): Ticket | undefined {
    return this.state.tickets.find((t) => t.k === k);
  }

  private upd(k: string, fn: (t: Ticket) => Partial<Ticket>): void {
    this.set((s) => ({ tickets: s.tickets.map((t) => (t.k === k ? { ...t, ...fn(t) } : t)) }));
  }

  private addLog(k: string, lines: readonly Line[]): void {
    this.upd(k, (t) => ({ log: t.log.concat(lines) }));
  }

  /** The first stage of `t`'s flow: where Start, a drag out of Backlog and a merged dependency send it. */
  private firstStage(t: Ticket): Stage {
    const first = flowOf(this.state, t.p).stages[0];
    if (!first) throw new Error("no stage in " + t.p + "’s flow");
    return first;
  }

  /** The tickets `t` waits for that aren't merged yet. */
  private unmet(t: Ticket): string[] {
    return t.waits.filter((w) => this.find(w)?.col !== "done");
  }

  // --- the board -----------------------------------------------------------------------------------------------

  pickProject(key: ProjectKey): void {
    this.set({ project: key, openKey: null, search: "", menu: null, flowOpen: false });
  }

  setSearch(search: string): void {
    this.set({ search });
  }

  setView(view: DemoState["view"]): void {
    this.set({ view });
  }

  /** Unfolds the Done column (the link under the last column opens it). */
  openDone(): void {
    this.set({ doneOpen: true });
  }

  /** Opens or closes a menu; opening one closes the others. */
  toggleMenu(menu: Menu): void {
    this.set((s) => ({ menu: s.menu === menu ? null : menu }));
  }

  /** Closes every menu (a click outside them). */
  closeMenus(): void {
    this.set({ menu: null });
  }

  /** Next (⌘J): the first thing that needs you here, else in the next project that has one. */
  nextItem(): void {
    const mine = needsOf(this.state.tickets, this.state.project);
    if (mine[0]) {
      this.openTicket(mine[0].k);
      return;
    }
    const other = PROJECTS.map((p) => p.key)
      .filter((k) => k !== this.state.project)
      .find((k) => needsOf(this.state.tickets, k).length > 0);
    const item = other ? needsOf(this.state.tickets, other)[0] : undefined;
    if (other && item) {
      this.set({ project: other });
      this.openTicket(item.k);
      this.say("Nothing more here. ⌘J moved on to " + project(other).name + ".");
    } else this.say("Nothing needs you in any project.");
  }

  // --- dragging ------------------------------------------------------------------------------------------------

  dragStart(k: string): void {
    this.set({ dragKey: k });
  }

  dragEnd(): void {
    this.set({ dragKey: null, dropCol: null });
  }

  dragOver(col: Column): void {
    if (!this.state.dragKey || this.state.dropCol === col) return;
    this.set({ dropCol: col });
  }

  /** A drop on the first stage starts the ticket, on Backlog parks it; the rest of the flow is the agents'. */
  drop(col: Column): void {
    const k = this.state.dragKey;
    this.set({ dragKey: null, dropCol: null });
    const t = k ? this.find(k) : undefined;
    if (!k || !t) return;
    const first = this.firstStage(t);
    if (col === first.id) this.start(k);
    else if (col === "backlog") this.upd(k, () => ({ col: "backlog", st: "queued" }));
    else
      this.say(
        "Agents move tickets through the flow. Drag a Backlog card into " +
          first.name +
          " to start it; the rest is the flow’s job.",
      );
  }

  // --- a ticket ------------------------------------------------------------------------------------------------

  openTicket(k: string, tab?: PaneTab): void {
    const t = this.find(k);
    this.set((s) => ({
      openKey: k,
      ptab: tab ?? "overview",
      part: "agent",
      project: t ? t.p : s.project,
      menu: null,
    }));
  }

  closeTicket(): void {
    this.set({ openKey: null, menu: null });
  }

  setTab(ptab: PaneTab): void {
    this.set({ ptab });
  }

  setPart(part: SessionPart): void {
    this.set({ part });
  }

  /** Open agent session: the Session tab on the agent's own terminal. */
  showSession(): void {
    this.set({ ptab: "session", part: "agent" });
  }

  /** Start: into the flow's first stage. A ticket that waits for another waits at the top of it and says why. */
  start(k: string): void {
    const t = this.find(k);
    if (!t) return;
    const first = this.firstStage(t);
    const unmet = this.unmet(t);
    if (unmet.length) {
      this.upd(k, () => ({ col: first.id, st: "queued", agent: first.who }));
      this.say(
        k +
          " waits at the top of " +
          first.name +
          " for " +
          unmet.join(", ") +
          " to merge, then starts by itself.",
      );
      return;
    }
    this.upd(k, () => ({
      col: first.id,
      st: "working",
      ago: "now",
      agent: first.who,
      tokens: "2k",
      log: [
        L("d", "Worktree ~/.otaq/wt/" + project(t.p).name + "/" + k + " from main · setup pnpm install"),
        L("d", first.who + " · " + first.skill),
        L("a", "Reading the ticket and the code it touches."),
      ],
    }));
    this.say(k + " started in " + first.name + ", in its own worktree.");
  }

  /** Picks one of the options a ticket asks you about. */
  answer(act: Answer): void {
    const go = (k: string, line: string, more: readonly Line[]) => {
      this.upd(k, () => ({ st: "working", ask: null, ago: "now" }));
      this.addLog(k, [L("u", "❯ " + line), ...more]);
    };
    switch (act) {
      case "g1":
        go("SHOP-12", "Follow the system setting, with a manual override", [
          L("a", "Recorded as decision 1. Wiring it now."),
          L("d", "Recorded on the ticket as a decision. Wiring the toggle to the system setting now."),
        ]);
        break;
      case "p22":
        go("SHOP-22", "Allow once", [
          L("tool", "Bash rm -rf .next && pnpm build"),
          L("o", "built in 48s"),
          L("a", "Clean build passes. Submitting."),
        ]);
        this.later(BUILD_MS, () => {
          this.upd("SHOP-22", () => ({
            col: "security",
            st: "checking",
            agent: "Security reviewer",
            ago: "now",
          }));
          this.say("SHOP-22 submitted. Security reviewer is checking it.");
        });
        break;
      case "d22":
        go("SHOP-22", "Deny", [L("a", "Understood. I’ll run pnpm build without clearing .next.")]);
        break;
      case "h40":
        this.upd("SHOP-40", (t) => ({
          st: "working",
          ask: null,
          pieces: (t.pieces ?? []).map((p) => (p.n === "40.2" ? { ...p, s: "working", cls: "w" } : p)),
        }));
        this.addLog("SHOP-40", [
          L("u", "❯ Keep the heart filled after sign-out"),
          L("a", "Passed to 40.2’s helper. It carries on."),
        ]);
        break;
      case "b7":
        go("BILL-7", "Prorate to the day", [
          L("a", "Recorded. Writing the proration with a test for mid-month."),
        ]);
        break;
      case "r32":
        this.upd("SHOP-32", () => ({
          st: "working2",
          col: "dev",
          ask: null,
          agent: "Rust developer",
          ago: "now",
        }));
        this.addLog("SHOP-32", [
          L("u", "❯ Allow 1 more round"),
          L("d", "Back to Dev with the Security report · round 4"),
        ]);
        this.say("SHOP-32 went back to Dev with the report.");
        break;
      case "m32":
        this.upd("SHOP-32", () => ({
          st: "checking",
          col: "validate",
          ask: null,
          agent: "Validator",
          tool: "CX",
          ago: "now",
          adds: 80,
          dels: 9,
          checks: [
            {
              t: "Security · failed 3 times · you sent it on",
              s: "Unescaped SQL in lib/address.ts. Still open.",
              color: "var(--err)",
            },
          ],
          manual: ["Fix or confirm the SQL finding yourself."],
        }));
        this.addLog("SHOP-32", [L("u", "❯ Send it on to Validate")]);
        this.say("SHOP-32 went on to Validate without a pass; Validate sees Security’s last report.");
        break;
    }
  }

  /** Approve & merge into main (a 1.8 s merge check), or Approve & open PR: it waits in Review as PR open. */
  approve(k: string, how: "merge" | "pr"): void {
    this.set({ menu: null });
    if (how === "pr") {
      this.upd(k, (t) => ({
        st: "pr",
        ago: "now",
        log: t.log.concat([L("ok", "Pushed otaq/" + k + " and opened PR #492 with gh")]),
      }));
      this.say(k + ": PR #492 is open. Your team reviews it there; Mark merged here when they do.");
      return;
    }
    this.upd(k, () => ({ st: "merging" }));
    this.say("main moved since the checks ran: the merge check runs pnpm test on the combined result…");
    this.later(MERGE_MS, () => {
      this.upd(k, (t) => ({
        col: "done",
        st: "done",
        log: t.log.concat([L("ok", "Merge check passed · squash-merged into main")]),
      }));
      this.say(k + " is merged into main.");
      this.startWaiting(k);
    });
  }

  /** The tickets waiting at the top of their first stage for `k` start 0.8 s after it merges. */
  private startWaiting(k: string): void {
    const waiting = this.state.tickets.filter(
      (x) => x.st === "queued" && x.col !== "backlog" && x.waits.includes(k),
    );
    for (const w of waiting) {
      this.later(WAIT_START_MS, () => {
        const first = this.firstStage(w);
        this.upd(w.k, () => ({
          col: first.id,
          st: "working",
          waits: [],
          ago: "now",
          agent: first.who,
          log: [L("d", "Worktree created from main · setup pnpm install · " + first.who)],
        }));
        this.say(w.k + " was waiting for " + k + ". It starts now.");
      });
    }
  }

  /** Mark merged: the PR landed on GitHub (Otaq doesn't watch PRs). */
  markMerged(k: string): void {
    this.upd(k, (t) => ({ col: "done", st: "done", log: t.log.concat([L("ok", "Marked merged by you")]) }));
    this.say(k + " is Done.");
    this.startWaiting(k);
  }

  sendBack(k: string): void {
    this.upd(k, () => ({ col: "dev", st: "working2", ago: "now", agent: "Rust developer" }));
    this.addLog(k, [
      L("u", "❯ (sent back to Dev) Please also handle an empty cart."),
      L("a", "Adding the empty-cart case and a test."),
    ]);
    this.say(k + " went back to Dev with your comment.");
  }

  toggleManual(k: string, i: number): void {
    const id = k + ":" + String(i);
    this.set((s) => ({ manualDone: { ...s.manualDone, [id]: !s.manualDone[id] } }));
  }

  setCDraft(cdraft: string): void {
    this.set({ cdraft });
  }

  /** Comment (the next stage reads it), or Tell <agent> now: the working agent also gets it, once it's idle. */
  addComment(tellNow = false): void {
    const s = this.state;
    const txt = s.cdraft.trim();
    const t = s.openKey ? this.find(s.openKey) : undefined;
    if (!txt || !t) return;
    const c: Comment = {
      tool: "U",
      who: "You",
      at: "now",
      stage: stageName(s, t),
      t: txt,
      ...(tellNow ? { told: "told " + t.agent + " now" } : {}),
    };
    this.set((st) => ({
      comments: { ...st.comments, [t.k]: (st.comments[t.k] ?? []).concat([c]) },
      cdraft: "",
    }));
    if (tellNow) this.addLog(t.k, [L("u", "❯ (comment) " + txt)]);
    this.say(
      tellNow
        ? "Comment added and told " + t.agent + " now."
        : "Comment added. The next stage’s agent reads it.",
    );
  }

  setTDraft(tdraft: string): void {
    this.set({ tdraft });
  }

  /** Types the terminal prompt's text into the open ticket's session. */
  sendToAgent(): void {
    const txt = this.state.tdraft.trim();
    const k = this.state.openKey;
    if (!txt || !k) return;
    this.addLog(k, [
      L("u", "❯ " + txt),
      L("d", "(Demo) In Otaq this goes straight into the agent’s own session."),
    ]);
    this.set({ tdraft: "" });
  }

  // --- the box for new work ------------------------------------------------------------------------------------

  setBoxDraft(boxDraft: string): void {
    this.set({ boxDraft });
  }

  /** ↵ creates the ticket in Backlog as typed (first line the title), ⌘↵ also starts it, ⇧↵ asks Noyan. */
  submitBox(action: BoxAction): void {
    const text = this.state.boxDraft.trim();
    if (!text) return;
    if (action === "noyan") {
      this.sendNoyan(text);
      return;
    }
    const [title = "", ...rest] = text.split("\n");
    const n = this.state.nextId;
    const k = this.state.project + "-" + String(n);
    const made = T({ k, p: this.state.project, title, col: "backlog", st: "queued", desc: rest.join("\n") });
    this.set((s) => ({ tickets: s.tickets.concat([made]), nextId: n + 1, boxDraft: "" }));
    if (action === "start") this.start(k);
    else this.say(k + " is in Backlog, as typed. No agent, no cost until you start it.");
  }

  // --- Noyan ---------------------------------------------------------------------------------------------------

  toggleNoyan(): void {
    this.set((s) => ({ noyanOpen: !s.noyanOpen, menu: null }));
  }

  setNyDraft(nyDraft: string): void {
    this.set({ nyDraft });
  }

  /** Sends `text` to Noyan (from its panel or the box's ⇧↵); it opens and replies after 0.9 s. */
  sendNoyan(text: string): void {
    const t = text.trim();
    if (!t) return;
    this.set((s) => ({
      noyanOpen: true,
      noyanMsgs: s.noyanMsgs.concat([{ cls: "u", t }]),
      nyDraft: "",
      boxDraft: "",
    }));
    this.later(NOYAN_MS, () => {
      this.set((s) => ({ noyanMsgs: s.noyanMsgs.concat([{ cls: "", t: NOYAN_REPLY }]) }));
    });
  }

  /** Create them: Noyan's three tickets go to Backlog, the second waiting for the first. */
  createFromNoyan(): void {
    const n = this.state.nextId;
    const made = noyanTickets(n);
    const ks = made.map((t) => t.k);
    const [first, second] = [String(ks[0]), String(ks[1])];
    this.set((s) => ({
      nextId: n + 3,
      tickets: s.tickets.concat(made),
      noyanMsgs: s.noyanMsgs
        .map((m): NoyanMsg => ({ ...m, ask: false }))
        .concat([
          {
            cls: "",
            t: "Created in Backlog. " + second + " waits for " + first + ": both touch lib/shipping.ts.",
            links: ks,
          },
        ]),
    }));
    this.say("Noyan created 3 tickets in Backlog.");
  }

  /** Not yet: Noyan asks what to change. */
  declineNoyan(): void {
    this.set((s) => ({
      noyanMsgs: s.noyanMsgs
        .map((m): NoyanMsg => ({ ...m, ask: false }))
        .concat([{ cls: "", t: NOYAN_NOT_YET }]),
    }));
  }

  // --- Pause ---------------------------------------------------------------------------------------------------

  /** Pause this project or all of them; when paused, either item resumes. */
  pause(scope: PauseScope): void {
    this.set((s) => ({ paused: s.paused ? null : scope, menu: null }));
  }

  resume(): void {
    this.set({ paused: null });
  }

  // --- Edit flow, on the board ---------------------------------------------------------------------------------

  /** Edit flow: the board becomes the flow's editor in place; the cards stay, dimmed. */
  openFlow(): void {
    this.set({ flowOpen: true, menu: null, openKey: null, flowDraftId: null, flowSel: null });
  }

  /** Cancel: nothing changed. */
  closeFlow(): void {
    this.set({ flowOpen: false, flowDraftId: null, flowSel: null, menu: null });
  }

  /** Start from…: the draft becomes one of the shipped flows (or the project's own). */
  pickPreset(id: FlowId): void {
    this.set({ flowDraftId: id, flowSel: null, menu: null });
  }

  /** A click on a column header shows its stage's details, or hides them when it's clicked again. */
  pickStage(id: StageId | "done"): void {
    this.set((s) => ({ flowSel: s.flowSel === id ? null : id }));
  }

  /** Save flow: the draft becomes the project's; tickets in a stage it lacks wait at the top of its first stage. */
  saveFlow(): void {
    const s = this.state;
    const draftId = s.flowDraftId ?? s.flows[s.project];
    if (draftId === s.flows[s.project]) {
      this.closeFlow();
      return;
    }
    const stages = FLOWS[draftId].stages;
    const keep = new Set<Column>(stages.map((x) => x.id));
    const first = stages[0];
    if (!first) return;
    this.set((st) => ({
      flows: { ...st.flows, [st.project]: draftId },
      flowOpen: false,
      flowDraftId: null,
      flowSel: null,
      tickets: st.tickets.map((x) =>
        x.p === st.project && !keep.has(x.col) && x.col !== "backlog" && x.col !== "done"
          ? { ...x, col: first.id, st: "queued", ask: null, ago: "", agent: first.who }
          : x,
      ),
    }));
    this.say(
      "Saved " +
        FLOWS[draftId].name +
        ". (Demo) Tickets in stages it removes wait at the top of " +
        first.name +
        "; in Otaq removing a stage waits until no ticket is in it.",
    );
  }
}

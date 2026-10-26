import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TIPS } from "./data";
import { DemoStore, MERGE_MS, NOYAN_MS, TOAST_MS, WAIT_START_MS } from "./state";
import { boardView, columnsOf, flowEditorView, needsOf, stageMenuAt, status, ticketView } from "./view";

let store: DemoStore;

beforeEach(() => {
  vi.useFakeTimers();
  store = new DemoStore();
});

afterEach(() => {
  store.dispose();
  vi.useRealTimers();
});

const ticket = (k: string) => {
  const t = store.find(k);
  if (!t) throw new Error("no ticket " + k);
  return t;
};
const texts = (k: string) => ticket(k).log.map((l) => l.t);
const needs = () => boardView(store.getState()).needs.map((n) => n.k + " " + n.why);
const titles = () => columnsOf(store.getState()).map((c) => c.title);
const cardsIn = (title: string) =>
  columnsOf(store.getState())
    .find((c) => c.title === title)
    ?.cards.map((c) => c.k);
const toast = () => store.getState().toast;

describe("the board", () => {
  it("starts on shop-web with its own flow's columns, Done folded", () => {
    const b = boardView(store.getState());
    expect(b.proj.name).toBe("shop-web");
    expect(b.flowLabel).toBe("shop-web · 6 stages");
    expect(titles()).toEqual(["Backlog", "Ready for Dev", "Dev", "Security", "Validate", "Review", "Ship"]);
    expect(b.doneCount).toBe(5);
    store.openDone();
    expect(titles().at(-1)).toBe("Done");
  });

  it("puts cards that need you first, then by priority", () => {
    expect(cardsIn("Dev")).toEqual(["SHOP-22", "SHOP-40", "SHOP-12"]);
    expect(cardsIn("Security")).toEqual(["SHOP-32", "SHOP-29", "SHOP-19"]);
  });

  it("shows the drawn status lines and buttons", () => {
    const card = (k: string) =>
      columnsOf(store.getState())
        .flatMap((c) => c.cards)
        .find((c) => c.k === k);
    expect(card("SHOP-22")).toMatchObject({ act: "Answer", cls: "attn" });
    expect(card("SHOP-22")?.status.label).toBe("Needs you: allow rm -rf .next?");
    expect(card("SHOP-12")?.status.label).toBe("Asks you: follow the system setting?");
    expect(card("SHOP-32")).toMatchObject({ act: "Decide", status: { label: "Failed 3 times: decide" } });
    expect(card("SHOP-18")).toMatchObject({
      act: "Review",
      status: { label: "Your check · passed Security and Validate" },
    });
    expect(card("SHOP-20")?.status.label).toBe("waits for SHOP-29 to merge");
    expect(card("SHOP-31")).toMatchObject({
      hasPrio: true,
      drag: true,
      status: { label: "P1 · not started" },
    });
    expect(card("SHOP-40")).toMatchObject({ pieceSum: "1 merged" });
    expect(card("SHOP-40")?.piecesShort.map((p) => p.n)).toEqual(["40.2", "40.3", "40.4"]);
    expect(card("SHOP-19")?.drag).toBe(false);
  });

  it("badges only the projects with questions, not your checks", () => {
    expect(boardView(store.getState()).projects.map((p) => [p.key, p.badge])).toEqual([
      ["SHOP", 4],
      ["BILL", 1],
    ]);
  });

  it("searches the open project by key and title", () => {
    store.setSearch("checkout");
    expect(columnsOf(store.getState()).flatMap((c) => c.cards.map((x) => x.k))).toEqual([
      "SHOP-31",
      "SHOP-20",
      "SHOP-21",
      "SHOP-18",
    ]);
    store.pickProject("BILL");
    expect(store.getState().search).toBe("");
  });

  it("counts the working agents across projects", () => {
    expect(boardView(store.getState()).runningLabel).toBe("3 working");
    const cc = boardView(store.getState()).actTools[0];
    expect(cc).toMatchObject({ name: "Claude Code", running: "3 running", hasFallback: true });
    store.useFallback();
    expect(boardView(store.getState()).actTools[0]?.limit).toContain("waiting agents moved to Codex");
  });
});

describe("Needs you and Next", () => {
  it("lists questions and decisions first, then your checks", () => {
    expect(needs()).toEqual([
      "SHOP-12 a question",
      "SHOP-22 allow a command",
      "SHOP-40 40.2 asks",
      "SHOP-32 decide",
      "SHOP-18 your check",
      "SHOP-23 your check",
    ]);
  });

  it("Next opens each item in turn as you answer, then moves on to billing-api", () => {
    store.nextItem();
    expect(store.getState().openKey).toBe("SHOP-12");
    store.answer("g1");
    store.nextItem();
    expect(store.getState().openKey).toBe("SHOP-22");
    store.answer("d22");
    store.nextItem();
    expect(store.getState().openKey).toBe("SHOP-40");
    store.answer("h40");
    store.nextItem();
    expect(store.getState().openKey).toBe("SHOP-32");
    store.answer("r32");
    store.nextItem();
    expect(store.getState()).toMatchObject({ openKey: "SHOP-18", ptab: "files" });
    store.approve("SHOP-18", "pr");
    store.nextItem();
    expect(store.getState().openKey).toBe("SHOP-23");
    store.approve("SHOP-23", "pr");
    expect(needs()).toEqual([]);

    store.nextItem();
    expect(store.getState()).toMatchObject({ project: "BILL", openKey: "BILL-7" });
    expect(toast()).toBe("Nothing more here. ⌘J moved on to billing-api.");
    store.answer("b7");
    store.approve("BILL-3", "pr");
    store.nextItem();
    expect(toast()).toBe("Nothing needs you in any project.");
  });
});

describe("answering", () => {
  it("SHOP-12: the answer goes into the session and the agent works on", () => {
    store.openTicket("SHOP-12");
    expect(ticketView(store.getState())?.ask?.opts.map((o) => o.label)).toEqual([
      "Follow the system setting, with a manual override",
      "Manual toggle only",
    ]);
    store.answer("g1");
    expect(ticket("SHOP-12")).toMatchObject({ st: "working", ask: null, ago: "now" });
    expect(texts("SHOP-12")).toContain("❯ Follow the system setting, with a manual override");
    expect(ticketView(store.getState())?.ask).toBeNull();
  });

  it("SHOP-22: Allow builds, then it's submitted to Security after 3 s", () => {
    store.answer("p22");
    expect(texts("SHOP-22").slice(-3)).toEqual([
      "Bash rm -rf .next && pnpm build",
      "built in 48s",
      "Clean build passes. Submitting.",
    ]);
    expect(ticket("SHOP-22").col).toBe("dev");
    vi.advanceTimersByTime(3000);
    expect(ticket("SHOP-22")).toMatchObject({ col: "security", st: "checking", agent: "Security reviewer" });
    expect(toast()).toBe("SHOP-22 submitted. Security reviewer is checking it.");
  });

  it("SHOP-22: Deny keeps it in Dev", () => {
    store.answer("d22");
    vi.runAllTimers();
    expect(ticket("SHOP-22")).toMatchObject({ col: "dev", st: "working" });
  });

  it("SHOP-40: the helper's piece carries on", () => {
    store.answer("h40");
    expect(ticket("SHOP-40").pieces?.find((p) => p.n === "40.2")).toMatchObject({ s: "working", cls: "w" });
  });

  it("SHOP-32: Move on anyway puts it in Review with the failed report", () => {
    store.answer("m32");
    expect(ticket("SHOP-32")).toMatchObject({ col: "review", st: "review" });
    store.openTicket("SHOP-32");
    const t = ticketView(store.getState());
    expect(t?.before?.checks[0]).toMatchObject({ verdict: "failed", color: "var(--err)" });
    expect(t?.manual.map((m) => m.t)).toEqual(["Fix or confirm the SQL finding yourself."]);
  });
});

describe("Approve, merge and Send back", () => {
  it("Approve SHOP-18 with a local merge: the 1.8 s merge check, then SHOP-21 starts 0.8 s later", () => {
    store.openTicket("SHOP-18");
    store.toggleMenu("approve");
    store.approve("SHOP-18", "local");
    expect(store.getState().menu).toBeNull();
    expect(ticket("SHOP-18").st).toBe("merging");
    expect(status(ticket("SHOP-18")).label).toBe("Merge check · pnpm test…");
    vi.advanceTimersByTime(MERGE_MS);
    expect(ticket("SHOP-18")).toMatchObject({ col: "done", st: "done" });
    expect(texts("SHOP-18").at(-1)).toBe("Merge check passed · squash-merged into main");
    expect(toast()).toBe("SHOP-18 is merged into main.");
    expect(ticket("SHOP-21").col).toBe("ready");
    vi.advanceTimersByTime(WAIT_START_MS);
    expect(ticket("SHOP-21")).toMatchObject({
      col: "dev",
      st: "working",
      waits: [],
      agent: "Rust developer",
    });
    expect(toast()).toBe("SHOP-21 was waiting for SHOP-18. It starts now.");
  });

  it("Open PR sends it to Ship; Mark merged finishes it", () => {
    store.approve("SHOP-23", "pr");
    expect(ticket("SHOP-23")).toMatchObject({ col: "ship", st: "pr" });
    expect(status(ticket("SHOP-23")).label).toBe("PR #492 open · mark merged when it lands");
    store.markMerged("SHOP-23");
    expect(ticket("SHOP-23")).toMatchObject({ col: "done", st: "done" });
    expect(toast()).toBe("SHOP-23 is Done.");
  });

  it("Send back returns it to Dev with your comment", () => {
    store.sendBack("SHOP-18");
    expect(ticket("SHOP-18")).toMatchObject({ col: "dev", st: "working2" });
    expect(texts("SHOP-18")).toContain("❯ (sent back to Dev) Please also handle an empty cart.");
    expect(toast()).toBe("SHOP-18 went back to Dev with your comment.");
  });
});

describe("Start and drag", () => {
  it("Start sends a Backlog ticket into the flow's first agent stage", () => {
    store.start("SHOP-31");
    expect(ticket("SHOP-31")).toMatchObject({
      col: "dev",
      st: "working",
      agent: "Rust developer",
      tokens: "2k",
    });
    expect(texts("SHOP-31")[0]).toBe("Worktree ~/.otaq/wt/shop-web/SHOP-31 from main · setup pnpm install");
    expect(toast()).toBe("SHOP-31 started in Dev, in its own worktree.");
  });

  it("a ticket that waits goes to Ready and says so", () => {
    store.start("SHOP-20");
    expect(ticket("SHOP-20").col).toBe("ready");
    expect(toast()).toBe("SHOP-20 waits for SHOP-29 to merge, then starts on its own.");
  });

  it("dragging to Ready starts it; to a later stage only explains", () => {
    store.dragStart("SHOP-41");
    store.dragOver("ready");
    const cols = columnsOf(store.getState());
    expect(cols.find((c) => c.id === "ready")?.cls).toBe("drop");
    expect(cols.find((c) => c.id === "dev")?.cls).toBe("nodrop");
    store.drop("ready");
    expect(ticket("SHOP-41").col).toBe("dev");
    expect(store.getState()).toMatchObject({ dragKey: null, dropCol: null });

    store.dragStart("SHOP-31");
    store.drop("review");
    expect(ticket("SHOP-31").col).toBe("backlog");
    expect(toast()).toBe(
      "Agents move tickets through the flow. Drag to Ready to start one; the rest is the flow’s job.",
    );
  });
});

describe("Change flow… and Apply", () => {
  it("previews a preset and applies it: the columns change and mid-flow tickets go to Ready", () => {
    store.openFlow();
    let f = flowEditorView(store.getState());
    expect(f.presets.map((p) => p.name)).toEqual([
      "shop-web’s flow",
      "Otaq default",
      "Solo autopilot",
      "Team PR",
      "Spec first",
    ]);
    expect(f).toMatchObject({ draftInUse: " · in use", applyNote: "No changes." });
    store.pickPreset("solo");
    f = flowEditorView(store.getState());
    expect(f.draftName).toBe("Solo autopilot");
    expect(f.applyNote).toMatch(/^Swapping flows waits until the board is quiet: \d+ tickets are mid-flow\./);
    store.pickStage("validate");
    expect(flowEditorView(store.getState()).stages.find((x) => x.sel)).toMatchObject({
      name: "Validate",
      kindLong: "Check · an agent",
      skill: "/validate",
      fail: "Dev",
    });
    store.applyFlow();
    expect(store.getState().flowOpen).toBe(false);
    expect(titles()).toEqual(["Backlog", "Ready for Dev", "Dev", "Validate", "Ship"]);
    expect(ticket("SHOP-19")).toMatchObject({ col: "ready", st: "queued" });
    expect(ticket("SHOP-18")).toMatchObject({ col: "ready", st: "queued", ask: null });
    expect(ticket("SHOP-12").col).toBe("dev");
    expect(boardView(store.getState()).flowLabel).toBe("Solo autopilot · 4 stages");
    expect(toast()).toContain("Applied Solo autopilot.");
  });

  it("Apply Spec first leaves no SHOP ticket in a column the board doesn't show", () => {
    store.openFlow();
    store.pickPreset("spec");
    store.applyFlow();
    store.openDone();
    const shownCols = new Set(columnsOf(store.getState()).map((c) => c.id));
    const lost = store.getState().tickets.filter((t) => t.p === "SHOP" && !shownCols.has(t.col));
    expect(lost.map((t) => t.k)).toEqual([]);
    for (const k of ["SHOP-20", "SHOP-21", "SHOP-44", "SHOP-19", "SHOP-32"])
      expect(ticket(k)).toMatchObject({ col: "backlog", st: "queued", ask: null });
    expect(ticket("SHOP-12").col).toBe("dev");
    expect(ticket("SHOP-18").col).toBe("review");
    expect(toast()).toBe(
      "Applied Spec first. (Demo) Tickets mid-flow moved to Backlog; in Otaq the change waits until the board is quiet.",
    );
  });

  it("billing-api has no shop-web preset", () => {
    store.pickProject("BILL");
    store.openFlow();
    expect(flowEditorView(store.getState()).presets.map((p) => p.id)).toEqual([
      "default",
      "solo",
      "team",
      "spec",
    ]);
  });

  it("a stage menu opens the editor on that stage, placed inside the board", () => {
    expect(stageMenuAt({ bottom: 100, left: 1260 })).toEqual({ top: 104, left: 1040 });
    expect(stageMenuAt({ bottom: 100, left: 120 })).toEqual({ top: 104, left: 70 });
    store.openStageMenu("security", "Security", 104, 300);
    store.openFlow(true);
    expect(store.getState()).toMatchObject({ flowOpen: true, flowSel: "security", stageMenu: null });
  });

  it("Noyan answers a question about the flow", () => {
    store.openFlow();
    store.setFlowDraft("Why is Security here?");
    store.sendFlowMsg();
    vi.advanceTimersByTime(800);
    expect(flowEditorView(store.getState()).msgs.map((m) => m.cls)).toEqual(["u", ""]);
  });
});

describe("Pause", () => {
  it("pauses this project or all, and resumes", () => {
    store.toggleMenu("pause");
    store.pause("this");
    expect(boardView(store.getState()).pause).toMatchObject({
      label: "Resume",
      scope: "shop-web",
      cls: "pau",
    });
    expect(store.getState().menu).toBeNull();
    store.resume();
    store.pause("all");
    expect(boardView(store.getState()).pause.scope).toBe("all projects");
    store.pause("all");
    expect(store.getState().paused).toBeNull();
  });
});

describe("Ask Noyan", () => {
  it("Create them puts three tickets in Backlog, the second waiting for the first", () => {
    store.createFromNoyan();
    expect(cardsIn("Backlog")).toEqual(["SHOP-31", "SHOP-45", "SHOP-46", "SHOP-41", "SHOP-47"]);
    expect(ticket("SHOP-46").waits).toEqual(["SHOP-45"]);
    expect(toast()).toBe("Noyan created 3 tickets in Backlog.");
    const msgs = store.getState().noyanMsgs;
    expect(msgs.at(-1)).toMatchObject({ links: ["SHOP-45", "SHOP-46", "SHOP-47"] });
    expect(msgs.some((m) => m.ask)).toBe(false);
    store.openTicket("SHOP-45");
    expect(ticketView(store.getState())?.comments[0]?.who).toBe("Noyan");
  });

  it("asking from the Ask bar opens Noyan, which replies after 0.9 s", () => {
    store.setAskDraft("why is checkout slow?");
    store.sendNoyan(store.getState().askDraft);
    expect(store.getState()).toMatchObject({ noyanOpen: true, askDraft: "" });
    const n = store.getState().noyanMsgs.length;
    vi.advanceTimersByTime(NOYAN_MS);
    expect(store.getState().noyanMsgs).toHaveLength(n + 1);
  });

  it("Not yet asks what to change", () => {
    store.declineNoyan();
    expect(store.getState().noyanMsgs.at(-1)?.t).toBe(
      "Okay. Tell me what to change and I’ll update the list.",
    );
  });
});

describe("the ticket sheet", () => {
  it("SHOP-18 shows the checks before you, the decision, manual checks and attachments", () => {
    store.openTicket("SHOP-18");
    const t = ticketView(store.getState());
    expect(t?.before).toMatchObject({ head: "Security and Validate", sum: "2 check agents · both passed" });
    expect(t?.decision?.t).toBe("Totals are recomputed on the server");
    expect(t?.atts.map((a) => a.name)).toEqual(["Summary.md", "Security report.md", "Validate report.md"]);
    expect(t?.steps.find((x) => x.state === "current")?.name).toBe("Review");
    expect(t?.blocks).toBe("SHOP-21");
    expect(t?.model).toBe("Claude Code · opus");
    store.toggleManual("SHOP-18", 1);
    expect(ticketView(store.getState())?.manual.map((m) => m.on)).toEqual([false, true]);
  });

  it("comments go to the next stage, or into the session too", () => {
    store.openTicket("SHOP-29");
    store.setCDraft("Use lib/db.ts");
    store.addComment();
    expect(toast()).toBe("Comment added. The next stage’s agent reads it.");
    store.toggleTell();
    store.setCDraft("Also check the limit");
    store.addComment();
    expect(toast()).toBe("Comment added and sent to the running agent.");
    expect(texts("SHOP-29").at(-1)).toBe("❯ (comment) Also check the limit");
    const c = ticketView(store.getState())?.comments;
    expect(c?.map((x) => x.who)).toEqual(["Noyan", "You", "You"]);
    expect(c?.[1]?.stage).toBe("Security");
  });

  it("typing into the session adds to the log", () => {
    store.openTicket("SHOP-12");
    store.setTDraft("use the media query");
    store.sendToAgent();
    expect(texts("SHOP-12").at(-2)).toBe("❯ use the media query");
    expect(store.getState().tdraft).toBe("");
  });

  it("SHOP-20 shows what it waits for", () => {
    store.openTicket("SHOP-20");
    expect(ticketView(store.getState())).toMatchObject({
      canStart: true,
      blockedBy: "SHOP-29 · waits for SHOP-29 to merge",
      startNote: "waits for SHOP-29 to merge, then starts on its own",
    });
  });
});

describe("toasts and timers", () => {
  it("a toast stays 3.6 s; a new one restarts it", () => {
    store.tip("settings");
    vi.advanceTimersByTime(TOAST_MS - 100);
    store.tip("terminals");
    expect(toast()).toBe(TIPS.terminals);
    vi.advanceTimersByTime(TOAST_MS - 100);
    expect(toast()).toBe(TIPS.terminals);
    vi.advanceTimersByTime(100);
    expect(toast()).toBeNull();
  });

  it("the Terminals toast shows the app's key (0137)", () => {
    expect(TIPS.terminals).toContain("⌃`");
    expect(TIPS.terminals).not.toContain("⇧⌘T");
  });

  it("dispose() leaves no timer", () => {
    store.answer("p22");
    store.approve("SHOP-18", "local");
    store.sendNoyan("hi");
    store.openFlow();
    store.setFlowDraft("why?");
    store.sendFlowMsg();
    expect(store.pendingTimers()).toBeGreaterThan(0);
    expect(vi.getTimerCount()).toBe(store.pendingTimers());
    store.dispose();
    expect(store.pendingTimers()).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe("needsOf", () => {
  it("is empty for a project with nothing waiting", () => {
    const s = store.getState();
    expect(
      needsOf(
        s.tickets.filter((t) => t.p !== "BILL"),
        "BILL",
      ),
    ).toEqual([]);
  });
});

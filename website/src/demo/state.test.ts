import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TIPS } from "./data";
import { DemoStore, MERGE_MS, NOYAN_MS, TOAST_MS, WAIT_START_MS } from "./state";
import { boardView, columnsOf, editFlowView, needsOf, status, ticketView } from "./view";

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
const strip = () => {
  const b = boardView(store.getState());
  return [...b.needs, ...b.reviews].map((n) => n.k + " " + n.why);
};
const titles = () => columnsOf(store.getState()).map((c) => c.title);
const cardsIn = (title: string) =>
  columnsOf(store.getState())
    .find((c) => c.title === title)
    ?.cards.map((c) => c.k);
const card = (k: string) =>
  columnsOf(store.getState())
    .flatMap((c) => c.cards)
    .find((c) => c.k === k);
const toast = () => store.getState().toast;

describe("the board", () => {
  it("starts on shop-web with its flow's columns between Backlog and a folded Done", () => {
    const b = boardView(store.getState());
    expect(b.proj.name).toBe("shop-web");
    expect(titles()).toEqual(["Backlog", "Dev", "Security", "Validate", "Review"]);
    expect(titles()).not.toContain("Ready for Dev");
    expect(b.doneCount).toBe(5);
    expect(b.delivery).toBe("Merge or PR · on Approve");
    store.openDone();
    const done = columnsOf(store.getState()).at(-1);
    expect(done).toMatchObject({ title: "Done", sub: "Merge or PR · on Approve" });
  });

  it("puts tickets waiting to start at the top of the first stage, then the ones that need you", () => {
    expect(cardsIn("Dev")).toEqual(["SHOP-20", "SHOP-21", "SHOP-44", "SHOP-22", "SHOP-40", "SHOP-12"]);
    expect(cardsIn("Security")).toEqual(["SHOP-32", "SHOP-29", "SHOP-19"]);
    expect(cardsIn("Review")).toEqual(["SHOP-18", "SHOP-23", "SHOP-26"]);
  });

  it("says what each card is doing, in the app's words", () => {
    expect(card("SHOP-22")).toMatchObject({ act: "Answer", cls: "attn" });
    expect(card("SHOP-22")?.status.label).toBe("Needs you: allow rm -rf .next?");
    expect(card("SHOP-12")?.status.label).toBe("Asks you: follow the system setting?");
    expect(card("SHOP-32")).toMatchObject({
      act: "Decide",
      status: { label: "Security failed 3 times · decide" },
    });
    expect(card("SHOP-18")).toMatchObject({
      act: "Review",
      status: { label: "Ready for you · passed Security and Validate" },
    });
    expect(card("SHOP-20")?.status).toMatchObject({ icon: "hourglass", label: "Waits for SHOP-29 to merge" });
    expect(card("SHOP-44")?.status.label).toBe("Waits for a free agent · 1st in line");
    expect(card("SHOP-26")?.status.label).toBe("PR open · #481 · Mark merged when it lands");
    expect(card("SHOP-31")).toMatchObject({
      hasPrio: true,
      drag: true,
      act: "Start",
      status: { label: "Not started" },
    });
    expect(card("SHOP-40")).toMatchObject({ pieceSum: "1 merged" });
    expect(card("SHOP-40")?.piecesShort.map((p) => p.n)).toEqual(["40.2", "40.3", "40.4"]);
    expect(card("SHOP-19")?.drag).toBe(false);
  });

  it("badges only the projects with questions, not the tickets ready for review", () => {
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

  it("shows Running only while agents run, with what's left per tool", () => {
    const b = boardView(store.getState());
    expect(b.runningLabel).toBe("3 running");
    expect(b.runningTools[0]).toMatchObject({ name: "Claude Code", running: "3 running" });
  });
});

describe("Needs you, Your review and Next", () => {
  it("lists questions and decisions first, then the tickets ready for you", () => {
    expect(strip()).toEqual([
      "SHOP-12 a question",
      "SHOP-22 allow a command",
      "SHOP-40 40.2 asks",
      "SHOP-32 decide",
      "SHOP-18 ready",
      "SHOP-23 ready",
    ]);
    expect(boardView(store.getState()).reviews.map((r) => r.k)).toEqual(["SHOP-18", "SHOP-23"]);
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
    expect(store.getState()).toMatchObject({ openKey: "SHOP-18", ptab: "overview" });
    store.approve("SHOP-18", "pr");
    store.nextItem();
    expect(store.getState().openKey).toBe("SHOP-23");
    store.approve("SHOP-23", "pr");
    expect(strip()).toEqual([]);

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

  it("SHOP-32: the decision offers Allow 1 more round and Send it on to the next stage", () => {
    store.openTicket("SHOP-32");
    expect(ticketView(store.getState())?.ask?.opts.map((o) => o.label)).toEqual([
      "Allow 1 more round",
      "Send it on to Validate",
    ]);
    store.answer("m32");
    expect(ticket("SHOP-32")).toMatchObject({ col: "validate", st: "checking", agent: "Checker" });
    expect(ticket("SHOP-32").checks?.[0]?.t).toBe("Security · failed 3 times · you sent it on");
  });
});

describe("Approve, merge and Send back", () => {
  it("Approve & merge into main: the 1.8 s merge check, then SHOP-21 starts 0.8 s later", () => {
    store.openTicket("SHOP-18");
    expect(ticketView(store.getState())).toMatchObject({ canApprove: true, canPR: true });
    store.toggleMenu("approve");
    store.approve("SHOP-18", "merge");
    expect(store.getState().menu).toBeNull();
    expect(ticket("SHOP-18").st).toBe("merging");
    expect(status(ticket("SHOP-18")).label).toBe("Merge check · pnpm test…");
    vi.advanceTimersByTime(MERGE_MS);
    expect(ticket("SHOP-18")).toMatchObject({ col: "done", st: "done" });
    expect(texts("SHOP-18").at(-1)).toBe("Merge check passed · squash-merged into main");
    expect(toast()).toBe("SHOP-18 is merged into main.");
    expect(ticket("SHOP-21")).toMatchObject({ col: "dev", st: "queued" });
    vi.advanceTimersByTime(WAIT_START_MS);
    expect(ticket("SHOP-21")).toMatchObject({
      col: "dev",
      st: "working",
      waits: [],
      agent: "Rust developer",
    });
    expect(toast()).toBe("SHOP-21 was waiting for SHOP-18. It starts now.");
  });

  it("Approve & open PR leaves it in Review as PR open; Mark merged finishes it", () => {
    store.approve("SHOP-23", "pr");
    expect(ticket("SHOP-23")).toMatchObject({ col: "review", st: "pr" });
    expect(status(ticket("SHOP-23")).label).toBe("PR open · #492 · Mark merged when it lands");
    store.openTicket("SHOP-23");
    expect(ticketView(store.getState())?.steps.find((x) => x.state === "current")?.name).toBe("Delivery");
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
  it("Start puts a Backlog ticket into the flow's first stage", () => {
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

  it("a ticket that waits for another waits at the top of the first stage and says why", () => {
    store.createFromNoyan();
    store.start("SHOP-46");
    expect(ticket("SHOP-46")).toMatchObject({ col: "dev", st: "queued" });
    expect(card("SHOP-46")?.status.label).toBe("Waits for SHOP-45 to merge");
    expect(toast()).toBe("SHOP-46 waits at the top of Dev for SHOP-45 to merge, then starts by itself.");
  });

  it("dragging into the first stage starts it; to a later stage only explains", () => {
    store.dragStart("SHOP-41");
    store.dragOver("dev");
    const cols = columnsOf(store.getState());
    expect(cols.find((c) => c.id === "dev")?.cls).toBe("drop");
    expect(cols.find((c) => c.id === "security")?.cls).toBe("nodrop");
    store.drop("dev");
    expect(ticket("SHOP-41")).toMatchObject({ col: "dev", st: "working" });
    expect(store.getState()).toMatchObject({ dragKey: null, dropCol: null });

    store.dragStart("SHOP-31");
    store.drop("review");
    expect(ticket("SHOP-31").col).toBe("backlog");
    expect(toast()).toBe(
      "Agents move tickets through the flow. Drag a Backlog card into Dev to start it; the rest is the flow’s job.",
    );
  });
});

describe("the box for new work", () => {
  it("↵ creates the ticket in Backlog as typed, the first line its title", () => {
    store.setBoxDraft("Add a coupon field\nOne per order.");
    store.submitBox("create");
    expect(ticket("SHOP-45")).toMatchObject({
      col: "backlog",
      title: "Add a coupon field",
      desc: "One per order.",
    });
    expect(store.getState().boxDraft).toBe("");
    expect(toast()).toBe("SHOP-45 is in Backlog, as typed. No agent, no cost until you start it.");
  });

  it("⌘↵ creates and starts it", () => {
    store.setBoxDraft("Fix the footer");
    store.submitBox("start");
    expect(ticket("SHOP-45")).toMatchObject({ col: "dev", st: "working" });
  });

  it("⇧↵ asks Noyan to shape it: its panel opens and it replies after 0.9 s", () => {
    store.setBoxDraft("why is checkout slow?");
    const n = store.getState().noyanMsgs.length;
    store.submitBox("noyan");
    expect(store.getState()).toMatchObject({ noyanOpen: true, boxDraft: "" });
    vi.advanceTimersByTime(NOYAN_MS);
    expect(store.getState().noyanMsgs).toHaveLength(n + 2);
  });
});

describe("Edit flow, on the board", () => {
  it("turns the board into the flow's editor: each column header is its stage, Done says how work leaves", () => {
    store.openFlow();
    const cols = columnsOf(store.getState());
    expect(cols.map((c) => c.title)).toEqual(["Backlog", "Dev", "Security", "Validate", "Review", "Done"]);
    expect(cols.find((c) => c.id === "security")?.head).toMatchObject({
      what: "An agent checks it",
      who: "Security reviewer",
      skill: "/security-check",
      line: "3 rounds · fails go to Dev",
    });
    expect(cols.at(-1)?.head).toMatchObject({
      what: "Every flow ends here",
      line: "Merge or PR · on Approve",
    });
    expect(editFlowView(store.getState())).toMatchObject({
      title: "Editing shop-web’s flow",
      changed: false,
    });
    store.pickStage("validate");
    expect(editFlowView(store.getState()).details?.rows).toEqual([
      ["What happens", "An agent checks it"],
      ["Agent", "Checker"],
      ["Skill", "/validate"],
      ["Fails go to", "Dev"],
    ]);
  });

  it("Start from… a shipped flow, then Save flow: the columns change and tickets in removed stages wait in Dev", () => {
    store.openFlow();
    expect(editFlowView(store.getState()).startFrom.map((p) => p.name)).toEqual([
      "shop-web’s flow",
      "Otaq default",
      "Solo autopilot",
      "Team PR",
      "Spec first",
    ]);
    store.pickPreset("solo");
    expect(editFlowView(store.getState()).changed).toBe(true);
    expect(columnsOf(store.getState()).at(-1)?.head?.line).toBe("Merge · automatically");
    store.saveFlow();
    expect(store.getState().flowOpen).toBe(false);
    expect(titles()).toEqual(["Backlog", "Dev", "Validate"]);
    expect(ticket("SHOP-19")).toMatchObject({ col: "dev", st: "queued" });
    expect(ticket("SHOP-18")).toMatchObject({ col: "dev", st: "queued", ask: null });
    expect(ticket("SHOP-12").col).toBe("dev");
    expect(boardView(store.getState()).delivery).toBe("Merge · automatically");
    expect(toast()).toContain("Saved Solo autopilot.");
  });

  it("Save Spec first leaves no SHOP ticket in a column the board doesn't show", () => {
    store.openFlow();
    store.pickPreset("spec");
    store.saveFlow();
    store.openDone();
    const shownCols = new Set(columnsOf(store.getState()).map((c) => c.id));
    const lost = store.getState().tickets.filter((t) => t.p === "SHOP" && !shownCols.has(t.col));
    expect(lost.map((t) => t.k)).toEqual([]);
    expect(ticket("SHOP-19")).toMatchObject({ col: "pspec", st: "queued" });
    expect(ticket("SHOP-18").col).toBe("review");
  });

  it("Cancel changes nothing, and billing-api has no shop-web flow to start from", () => {
    store.openFlow();
    store.pickPreset("team");
    store.closeFlow();
    expect(titles()).toEqual(["Backlog", "Dev", "Security", "Validate", "Review"]);
    store.pickProject("BILL");
    store.openFlow();
    expect(editFlowView(store.getState()).startFrom.map((p) => p.id)).toEqual([
      "default",
      "solo",
      "team",
      "spec",
    ]);
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

  it("Not yet asks what to change", () => {
    store.declineNoyan();
    expect(store.getState().noyanMsgs.at(-1)?.t).toBe(
      "Okay. Tell me what to change and I’ll update the list.",
    );
  });
});

describe("the ticket sheet", () => {
  it("has the tabs Overview, Session, Changes and History", async () => {
    const { PANE_TABS } = await import("./view");
    expect(PANE_TABS.map(([, label]) => label)).toEqual(["Overview", "Session", "Changes", "History"]);
  });

  it("SHOP-18 shows the checks before you, the decision, manual checks and attachments", () => {
    store.openTicket("SHOP-18");
    const t = ticketView(store.getState());
    expect(t?.before).toMatchObject({ head: "Security and Validate", sum: "2 checks · both passed" });
    expect(t?.decision?.t).toBe("Totals are recomputed on the server");
    expect(t?.atts.map((a) => a.name)).toEqual(["Summary.md", "Security report.md", "Validate report.md"]);
    expect(t?.steps.map((x) => x.name)).toEqual(["Dev", "Security", "Validate", "Review", "Delivery"]);
    expect(t?.steps.find((x) => x.state === "current")?.name).toBe("Review");
    expect(t?.waitingForIt).toBe("SHOP-21");
    expect(t?.model).toBe("Claude Code · opus");
    store.toggleManual("SHOP-18", 1);
    expect(ticketView(store.getState())?.manual.map((m) => m.on)).toEqual([false, true]);
  });

  it("Session's switch names each side by what it is: Agent, the check's stage, Run, Terminal", () => {
    store.openTicket("SHOP-29");
    expect(ticketView(store.getState())?.parts.map((p) => p.label)).toEqual([
      "Agent",
      "Security",
      "Run",
      "Terminal",
    ]);
  });

  it("Comment goes to the next stage; Tell <agent> now also tells the working agent", () => {
    store.openTicket("SHOP-29");
    store.setCDraft("Use lib/db.ts");
    store.addComment();
    expect(toast()).toBe("Comment added. The next stage’s agent reads it.");
    store.setCDraft("Also check the limit");
    store.addComment(true);
    expect(toast()).toBe("Comment added and told Security reviewer now.");
    expect(texts("SHOP-29").at(-1)).toBe("❯ (comment) Also check the limit");
    const c = ticketView(store.getState())?.comments;
    expect(c?.map((x) => x.who)).toEqual(["Noyan", "You", "You"]);
    expect(c?.[1]?.stage).toBe("Security");
    expect(c?.[2]?.told).toBe("told Security reviewer now");
  });

  it("typing into the session adds to the log", () => {
    store.openTicket("SHOP-12");
    store.setTDraft("use the media query");
    store.sendToAgent();
    expect(texts("SHOP-12").at(-2)).toBe("❯ use the media query");
    expect(store.getState().tdraft).toBe("");
  });

  it("SHOP-20 says what it waits for; a waiting Backlog ticket says where Start puts it", () => {
    store.openTicket("SHOP-20");
    expect(ticketView(store.getState())).toMatchObject({
      canStart: false,
      waitsFor: "SHOP-29 · not merged yet",
      waitingForIt: "Nothing waits for SHOP-20.",
    });
    store.createFromNoyan();
    store.openTicket("SHOP-46");
    expect(ticketView(store.getState())).toMatchObject({
      canStart: true,
      startNote: "Start puts it at the top of Dev, where it waits for SHOP-45 to merge",
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

  it("the Terminal toast shows the app's key", () => {
    expect(TIPS.terminals).toContain("⌃`");
    expect(TIPS.terminals).not.toContain("⇧⌘T");
  });

  it("dispose() leaves no timer", () => {
    store.answer("p22");
    store.approve("SHOP-18", "merge");
    store.sendNoyan("hi");
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

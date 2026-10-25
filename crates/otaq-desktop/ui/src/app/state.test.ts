import { describe, expect, it } from "vitest";

import { createOverlayStack, modalOpen } from "./overlays";
import {
  closeTicket,
  createSelection,
  focusTerminal,
  guardLeaving,
  openTicket,
  selectProject,
  showTab,
  stepTicket,
} from "./selection";
import { createStore } from "./store";

describe("the store", () => {
  it("tells listeners about changes only", () => {
    const store = createStore(1);
    let calls = 0;
    const stop = store.subscribe(() => calls++);
    store.set(1);
    store.update((n) => n + 1);
    expect([store.get(), calls]).toEqual([2, 1]);
    stop();
    store.set(5);
    expect(calls).toBe(1);
  });
});

describe("the selection", () => {
  it("opens a ticket on its first tab, j / k keep the tab, and a new project starts closed (SHT-02)", () => {
    const s = createSelection();
    selectProject("SHOP", s);
    openTicket("SHOP-22", "files", s);
    focusTerminal("session-1", s);
    expect(s.get()).toEqual({
      project: "SHOP",
      ticket: "SHOP-22",
      tab: "files",
      focusedTerminal: "session-1",
    });

    stepTicket("SHOP-23", s);
    expect(s.get().tab).toBe("files");
    openTicket("SHOP-23", undefined, s);
    expect(s.get().tab, "the open ticket keeps it").toBe("files");
    openTicket("SHOP-24", undefined, s);
    expect(s.get().tab).toBeNull();
    showTab("activity", s);
    closeTicket(s);
    expect(s.get()).toEqual({ project: "SHOP", ticket: null, tab: "activity", focusedTerminal: null });

    openTicket("SHOP-22", undefined, s);
    selectProject("CREW", s);
    expect(s.get()).toMatchObject({ project: "CREW", ticket: null });
  });

  it("switching projects keeps what each one showed", () => {
    const s = createSelection();
    selectProject("SHOP", s);
    openTicket("SHOP-22", "files", s);
    focusTerminal("session-1", s);
    selectProject("CREW", s);
    openTicket("CREW-3", "agent", s);
    selectProject("SHOP", s);
    expect(s.get()).toEqual({ project: "SHOP", ticket: "SHOP-22", tab: "files", focusedTerminal: null });
    selectProject("CREW", s);
    expect(s.get()).toMatchObject({ project: "CREW", ticket: "CREW-3", tab: "agent" });
  });
});

describe("the leave guard (T-0465: a changed description asks first)", () => {
  it("holds every move it says so, and the move happens once it calls go", () => {
    const s = createSelection();
    selectProject("SHOP", s);
    openTicket("SHOP-22", undefined, s);
    const held: (() => void)[] = [];
    const stop = guardLeaving((next, go) => {
      if (next.ticket === "SHOP-22" && next.tab === "overview") return false;
      held.push(go);
      return true;
    }, s);
    showTab("overview", s);
    expect(s.get().tab).toBe("overview");
    showTab("files", s);
    closeTicket(s);
    stepTicket("SHOP-23", s);
    openTicket("SHOP-30", undefined, s);
    selectProject("BILL", s);
    expect(s.get()).toMatchObject({ project: "SHOP", ticket: "SHOP-22", tab: "overview" });
    expect(held).toHaveLength(5);
    held[1]?.();
    expect(s.get().ticket).toBeNull();
    stop();
    openTicket("SHOP-30", undefined, s);
    expect(s.get().ticket).toBe("SHOP-30");
  });

  it("asks each guard in turn: the description's, then the Plan's (T-0540)", () => {
    const s = createSelection();
    selectProject("SHOP", s);
    openTicket("SHOP-40", undefined, s);
    const asked: string[] = [];
    const goes: (() => void)[] = [];
    const guard = (name: string) => (_next: unknown, go: () => void) => {
      asked.push(name);
      goes.push(go);
      return true;
    };
    const stopDescription = guardLeaving(guard("description"), s);
    const stopPlan = guardLeaving(guard("plan"), s);
    closeTicket(s);
    expect(asked).toEqual(["description"]);
    goes[0]?.();
    expect(asked).toEqual(["description", "plan"]);
    expect(s.get().ticket).toBe("SHOP-40");
    goes[1]?.();
    expect(s.get().ticket).toBeNull();
    stopDescription();
    stopPlan();
  });
});

describe("the overlay stack", () => {
  it("opens by id with params, newest on top, and knows when a modal is open", () => {
    const stack = createOverlayStack();
    stack.open("activity");
    stack.open("edit_ticket", { ticket: "SHOP-22" });
    expect(stack.store.get().map((o) => o.id)).toEqual(["activity", "edit_ticket"]);
    expect(modalOpen(stack.store.get())).toBe(true);

    stack.open("activity");
    expect(stack.store.get().map((o) => o.id)).toEqual(["edit_ticket", "activity"]);
    stack.closeTop();
    stack.toggle("edit_ticket", { ticket: "SHOP-22" });
    expect(stack.store.get()).toEqual([]);
    expect(modalOpen(stack.store.get())).toBe(false);

    stack.toggle("activity");
    expect(modalOpen(stack.store.get())).toBe(false);
    stack.toggle("activity");
    stack.closeTop();
    expect(stack.store.get()).toEqual([]);
  });
});

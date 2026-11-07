// @vitest-environment happy-dom
// The box for new work's own rules (UX-22): closed it says "New ticket, or ask Noyan…" with ⌘N; open, the first line
// is the title and the rest the description; ↵ creates in Backlog with no agent, ⌘↵ creates and starts, ⇧↵ hands the
// words to Noyan and opens its panel, ⌥↵ is a new line, Esc closes and keeps the words; More options… opens the full
// form with what was typed.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { INITIAL_LAYOUT, shellLayout } from "../../app/layout";
import { overlays } from "../../app/overlays";
import { registered, registerScreens } from "../../app/registry";
import { selection } from "../../app/selection";
import type { NewWorkView } from "../../generated/NewWorkView";
import { request } from "../../tauri";
import { agentPanelFocusRequests, shownAgents } from "../agent_panel/logic";
import { newWork } from ".";
import {
  askNoyan,
  boxKey,
  boxOnScreen,
  createTicket,
  EMPTY,
  boardAway,
  formOf,
  intoTitle,
  noyanText,
  openNewWork,
} from "./logic";
import { NewWork, NewWorkBody, type NewWorkBodyProps } from "./NewWork";

vi.mock("../../tauri", () => ({
  request: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
  windowAction: vi.fn(),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ask = vi.mocked(request);

/** `new_work.rs`'s words on a Mac. */
const VIEW: NewWorkView = {
  shown: true,
  placeholder: "New ticket, or ask Noyan…",
  keys: "⌘N",
  details: "Add details (optional)",
  new_line: { label: "new line", keys: "⌥↵" },
  more_options: "More options…",
  ask: { label: "Ask Noyan to shape it", keys: "⇧↵" },
  create: { label: "Create", keys: "↵" },
  start: { label: "Create & start", keys: "⌘↵" },
};

const TYPED = { title: "Coupon field rejects lowercase codes", details: "Accept any case." };

let root: Root;
let box: HTMLElement;

function show(props: NewWorkBodyProps) {
  act(() => {
    root.render(createElement(NewWorkBody, props));
  });
}

const titleField = () => {
  const found = box.querySelector<HTMLInputElement>(".new-work-title");
  if (!found) throw new Error("no title line");
  return found;
};

function key(target: Element, name: string, more: KeyboardEventInit = {}) {
  act(() => {
    target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, ...more }));
  });
}

async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

beforeEach(() => {
  ask.mockReset();
  box = document.body.appendChild(document.createElement("div"));
  root = createRoot(box);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  document.body.replaceChildren();
  shownAgents.set({});
  agentPanelFocusRequests.set(null);
  shellLayout.set(INITIAL_LAYOUT);
  overlays.store.set([]);
  boxOnScreen.set(false);
  boardAway.set(false);
  selection.set({ ...selection.get(), project: null });
});

describe("the keys", () => {
  it("↵ creates, ⌘↵ or Ctrl+↵ starts, ⇧↵ asks Noyan, ⌥↵ is a new line, Esc leaves", () => {
    const k = (key: string, more: Partial<Parameters<typeof boxKey>[0]> = {}) =>
      boxKey({ key, shiftKey: false, metaKey: false, ctrlKey: false, altKey: false, ...more });
    expect(k("Enter")).toBe("create");
    expect(k("Enter", { metaKey: true })).toBe("start");
    expect(k("Enter", { ctrlKey: true })).toBe("start");
    expect(k("Enter", { shiftKey: true })).toBe("ask");
    expect(k("Enter", { altKey: true })).toBe("new_line");
    expect(k("Escape")).toBe("leave");
    expect(k("a")).toBeNull();
    expect(k("Enter", { isComposing: true })).toBeNull();
  });
});

describe("the ticket as typed", () => {
  it("the first line is the title, the rest the description", () => {
    const pasted = intoTitle(EMPTY, "Coupon field\nAccept any case.\nDone when save10 applies.");
    expect(pasted).toEqual({ title: "Coupon field", details: "Accept any case.\nDone when save10 applies." });
    expect(intoTitle({ title: "", details: "More." }, "A\r\nB")).toEqual({ title: "A", details: "B\nMore." });
    expect(formOf({ title: "  Coupon field ", details: "\nAccept any case.\n\n" })).toEqual({
      title: "Coupon field",
      body: "Accept any case.",
      priority: null,
      base_branch: null,
      choices: [],
      waits_for: [],
    });
    expect(noyanText(TYPED)).toBe("Coupon field rejects lowercase codes\nAccept any case.");
    expect(noyanText({ title: "Checkout feels slow", details: " " })).toBe("Checkout feels slow");
  });
});

describe("the box", () => {
  it("registers the new_work slot and no keys of its own", () => {
    const app = registerScreens({ newWork });
    expect(registered(app)).toMatchObject({ slots: ["new_work"], shortcuts: [] });
  });

  it("closed: its words and ⌘N, or the title typed so far; a click opens it", () => {
    const onOpen = vi.fn();
    show({ view: VIEW, typed: EMPTY, open: false, onOpen });
    const closed = box.querySelector<HTMLButtonElement>(".new-work-closed");
    expect(closed?.textContent).toBe("New ticket, or ask Noyan…⌘N");
    act(() => {
      closed?.click();
    });
    expect(onOpen).toHaveBeenCalledOnce();
    show({ view: VIEW, typed: TYPED, open: false });
    expect(box.querySelector(".new-work-closed-text[data-typed]")?.textContent).toBe(TYPED.title);
  });

  it("open: the title line takes the keyboard, the details under it, then More options… and the three actions", () => {
    show({ view: VIEW, typed: EMPTY, open: true });
    expect(document.activeElement).toBe(titleField());
    expect(box.querySelector(".new-work-scrim")).not.toBeNull();
    expect(box.querySelector<HTMLTextAreaElement>(".new-work-details")?.placeholder).toBe(
      "Add details (optional)",
    );
    expect(box.querySelector(".new-work-more")?.textContent).toBe("More options…");
    expect(box.querySelector(".new-work-ask")?.textContent).toBe("Ask Noyan to shape it⇧↵");
    expect([...box.querySelectorAll(".new-work-action")].map((a) => a.textContent)).toEqual([
      "Create↵",
      "Create & start⌘↵",
    ]);
    // Nothing typed: nothing to create or ask.
    expect(box.querySelector<HTMLButtonElement>(".new-work-ask")?.disabled).toBe(true);
  });

  it("each key does its action once; ⌥↵ on the title line goes to the details; Esc closes", () => {
    const calls: string[] = [];
    show({
      view: VIEW,
      typed: TYPED,
      open: true,
      onCreate: () => calls.push("create"),
      onStart: () => calls.push("start"),
      onAsk: () => calls.push("ask"),
      onClose: () => calls.push("close"),
    });
    const title = titleField();
    key(title, "Enter");
    key(title, "Enter", { metaKey: true });
    key(title, "Enter", { shiftKey: true });
    key(title, "Escape");
    expect(calls).toEqual(["create", "start", "ask", "close"]);
    key(title, "Enter", { altKey: true });
    expect(document.activeElement).toBe(box.querySelector(".new-work-details"));
    // While one is on its way, the keys wait.
    show({ view: VIEW, typed: TYPED, open: true, sending: "create", onCreate: () => calls.push("again") });
    key(titleField(), "Enter");
    expect(calls).not.toContain("again");
  });

  it("a refusal shows under the fields", () => {
    show({ view: VIEW, typed: TYPED, open: true, note: "A ticket needs a title." });
    expect(box.querySelector(".field-note[data-error]")?.textContent).toBe("A ticket needs a title.");
  });
});

describe("what the box sends", () => {
  it("↵ creates the ticket as typed in Backlog through the editor's save; ⌘↵ through Create & start", async () => {
    ask.mockResolvedValue({ outcome: "saved", key: "SHOP-31", note: null });
    expect(await createTicket("SHOP", TYPED, false)).toEqual({ clear: true, note: null });
    const target = { mode: "create", project: "SHOP" };
    const form = formOf(TYPED);
    expect(ask).toHaveBeenLastCalledWith("editor.save", { target, form });
    await createTicket("SHOP", TYPED, true);
    expect(ask).toHaveBeenLastCalledWith("editor.create_and_start", { target, form });
  });

  it("a refused start leaves the ticket created: the box clears and says why; a refused create keeps the words", async () => {
    const refusal = {
      field: "form",
      title: "Not started",
      message: "SHOP-31 was created but didn’t start.",
      icon: "circle-x",
    };
    ask.mockResolvedValueOnce({ outcome: "refused", refusal, created: "SHOP-31" });
    expect(await createTicket("SHOP", TYPED, true)).toEqual({ clear: true, note: refusal.message });
    ask.mockResolvedValueOnce({ outcome: "refused", refusal: { ...refusal, message: "No." }, created: null });
    expect(await createTicket("SHOP", TYPED, false)).toEqual({ clear: false, note: "No." });
  });

  it("⇧↵ starts a conversation on Noyan's agent with the words and opens its panel on it", async () => {
    ask.mockResolvedValue("a7");
    expect(
      await askNoyan("SHOP", { title: "Checkout feels slow on mobile, make tickets", details: "" }),
    ).toBe(true);
    expect(ask).toHaveBeenCalledWith("agent_panel.send", {
      project: "SHOP",
      message: "Checkout feels slow on mobile, make tickets",
    });
    expect(shownAgents.get()).toEqual({ SHOP: "a7" });
    expect(shellLayout.get().agentOpen).toBe(true);
    expect(agentPanelFocusRequests.get()).toMatchObject({ agent: "a7" });
    // No words: nothing starts.
    ask.mockClear();
    expect(await askNoyan("SHOP", { title: " ", details: "" })).toBe(false);
    expect(ask).not.toHaveBeenCalled();
  });
});

describe("the slot", () => {
  const mount = async () => {
    ask.mockImplementation((method: string) =>
      Promise.resolve(method === "new_work.view" ? VIEW : { outcome: "saved", key: "SHOP-31", note: null }),
    );
    act(() => {
      root.render(createElement(NewWork));
    });
    await settle();
  };

  it("is on the board for a project; ⌘N opens it; More options… opens the full form with what was typed", async () => {
    selection.set({ ...selection.get(), project: "SHOP" });
    await mount();
    expect(boxOnScreen.get()).toBe(true);
    const form = vi.fn();
    act(() => {
      openNewWork(form);
    });
    expect(form).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(titleField());
    act(() => {
      titleField().focus();
    });
    // Typing goes through React's change: set the value as the page would.
    const setValue = (el: HTMLInputElement, value: string) => {
      const proto = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value");
      proto?.set?.call(el, value);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    };
    act(() => {
      setValue(titleField(), "Coupon field");
    });
    act(() => {
      box.querySelector<HTMLButtonElement>(".new-work-more")?.click();
    });
    expect(overlays.store.get()).toEqual([{ id: "new_ticket", params: { title: "Coupon field" } }]);
    expect(box.querySelector(".new-work-closed-text")?.textContent).toBe("New ticket, or ask Noyan…");
  });

  it("↵ creates and the box closes empty", async () => {
    selection.set({ ...selection.get(), project: "SHOP" });
    await mount();
    act(() => {
      openNewWork(vi.fn());
    });
    const input = titleField();
    act(() => {
      const proto = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value");
      proto?.set?.call(input, "Add sitemap.xml");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    key(input, "Enter");
    await settle();
    expect(ask).toHaveBeenCalledWith("editor.save", {
      target: { mode: "create", project: "SHOP" },
      form: formOf({ title: "Add sitemap.xml", details: "" }),
    });
    expect(box.querySelector(".new-work-open")).toBeNull();
    expect(box.querySelector(".new-work-closed-text")?.textContent).toBe("New ticket, or ask Noyan…");
  });

  it("isn't there in Files or while Noyan's panel is open; ⌘N then opens the full form", async () => {
    selection.set({ ...selection.get(), project: "SHOP" });
    boardAway.set(true);
    await mount();
    expect(box.querySelector(".new-work-closed")).toBeNull();
    expect(boxOnScreen.get()).toBe(false);
    const form = vi.fn();
    openNewWork(form);
    expect(form).toHaveBeenCalledOnce();
    act(() => {
      boardAway.set(false);
    });
    expect(box.querySelector(".new-work-closed")).not.toBeNull();
    act(() => {
      shellLayout.set({ ...INITIAL_LAYOUT, agentOpen: true });
    });
    expect(box.querySelector(".new-work-closed")).toBeNull();
  });

  it("a hidden box isn't drawn", async () => {
    selection.set({ ...selection.get(), project: "SHOP" });
    ask.mockResolvedValue({ ...VIEW, shown: false });
    act(() => {
      root.render(createElement(NewWork));
    });
    await settle();
    expect(box.querySelector(".new-work-closed")).toBeNull();
  });
});

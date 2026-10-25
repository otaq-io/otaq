// @vitest-environment happy-dom
// The Ask bar's own rules (T-0458; SHB-03, d0113): closed it asks with ⌘L; typing shows "ASK ABOUT" and ↑ / ↓ choose;
// Return sends once with the chosen row and opens the panel on the conversation; nothing goes without words; "New
// reply" opens the conversation that answered.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { INITIAL_LAYOUT, shellLayout } from "../../app/layout";
import { registered, registerScreens } from "../../app/registry";
import type { AskBarView } from "../../generated/AskBarView";
import type { AskSuggestion } from "../../generated/AskSuggestion";
import { request } from "../../tauri";
import { agentPanelFocusRequests, shownAgents } from "../agent_panel/logic";
import { askBar } from ".";
import { AskBarBody, type AskBarBodyProps } from "./AskBar";
import { lookedAt, lookedCount, nextChoice, sendFromBar } from "./logic";

vi.mock("../../tauri", () => ({ request: vi.fn(), subscribeEvents: vi.fn(), windowAction: vi.fn() }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ask = vi.mocked(request);

const TICKET: AskSuggestion = {
  about: "ticket",
  icon: "ticket",
  label: "SHOP-29 · Coupon field (selected)",
  ticket: "SHOP-29",
  agent: null,
};
const CONTINUE: AskSuggestion = {
  about: "continue",
  icon: "message-circle",
  label: "Continue “Checkout slowness”",
  ticket: null,
  agent: "a1",
};
const NEW: AskSuggestion = {
  about: "new",
  icon: "plus",
  label: "New conversation",
  ticket: null,
  agent: null,
};

/** `ask_bar.rs`'s words for shop-web with SHOP-29 selected and one conversation running. */
const VIEW: AskBarView = {
  icon: "bot",
  placeholder: "Ask Noyan… (“why is checkout slow?”)",
  short_placeholder: "Ask Noyan…",
  keys: "⌘L",
  send_keys: "↵",
  send: "Send",
  title: "ASK ABOUT",
  suggestions: [TICKET, CONTINUE, NEW],
  reply: null,
  button: { icon: "bot", label: "Noyan", hint: "Open or close · ⌘L", tinted: false, dot: false },
};

let root: Root;
let box: HTMLElement;

function show(props: AskBarBodyProps) {
  act(() => {
    root.render(createElement(AskBarBody, props));
  });
}

const field = () => {
  const found = box.querySelector("input");
  if (!found) throw new Error("no field");
  return found;
};

function key(name: string) {
  act(() => {
    field().dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true }));
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
  lookedCount.set(0);
});

describe("the bar", () => {
  it("registers the ask_bar slot and no keys of its own", () => {
    const app = registerScreens({ askBar });
    expect(registered(app)).toMatchObject({ slots: ["ask_bar"], shortcuts: [] });
  });

  it("closed: the question and ⌘L, no suggestions", () => {
    show({ view: VIEW, text: "", typing: false, choice: 0 });
    expect(field().placeholder).toBe("Ask Noyan… (“why is checkout slow?”)");
    expect(box.querySelector(".kbd")?.textContent).toBe("⌘L");
    expect(box.textContent).not.toContain("ASK ABOUT");
  });

  it("typing: ASK ABOUT with the ticket, Continue and New conversation; ↑ / ↓ choose, wrapping", () => {
    const onChoice = vi.fn();
    show({ view: VIEW, text: "why is SHOP-29 back in Dev?", typing: true, choice: 0, onChoice });
    expect(box.querySelector(".ask-suggestions-title")?.textContent).toBe("ASK ABOUT");
    expect([...box.querySelectorAll(".ask-suggestion")].map((b) => b.textContent)).toEqual([
      "SHOP-29 · Coupon field (selected)",
      "Continue “Checkout slowness”",
      "New conversation",
    ]);
    expect(box.querySelector(".ask-suggestion[data-chosen]")?.textContent).toContain("SHOP-29");
    expect([...box.querySelectorAll(".kbd")].map((k) => k.textContent)).toEqual(["↵", "⌘L"]);
    key("ArrowDown");
    key("ArrowUp");
    expect(onChoice.mock.calls).toEqual([[1], [2]]);
    expect(nextChoice(2, 3, 1)).toBe(0);
    expect(nextChoice(0, 0, 1)).toBe(0);
  });

  it("Return sends with the chosen row, never without words; Esc clears the words", () => {
    const onSend = vi.fn();
    const onText = vi.fn();
    show({ view: VIEW, text: "  ", typing: true, choice: 1, onSend, onText });
    key("Enter");
    expect(onSend).not.toHaveBeenCalled();
    show({ view: VIEW, text: "and the cart?", typing: true, choice: 1, onSend, onText });
    key("Enter");
    expect(onSend).toHaveBeenCalledWith(CONTINUE);
    key("Escape");
    expect(onText).toHaveBeenCalledWith("");
  });

  it("a reply waiting: the short question, New reply with the conversation, its dot; a click opens it", () => {
    const onReply = vi.fn();
    show({
      view: { ...VIEW, reply: { agent: "a1", label: "New reply", name: "Checkout slowness" } },
      text: "",
      typing: false,
      choice: 0,
      onReply,
    });
    expect(field().placeholder).toBe("Ask Noyan…");
    const reply = box.querySelector<HTMLButtonElement>(".ask-reply");
    expect(reply?.textContent).toBe("New replyCheckout slowness");
    expect(reply?.querySelector(".ask-dot")).not.toBeNull();
    act(() => {
      reply?.click();
    });
    expect(onReply).toHaveBeenCalledWith("a1");
  });
});

describe("sending", () => {
  it("sends once per Return and opens the panel on the conversation, its terminal focused", async () => {
    let answer: (sent: { agent: string; sent: boolean }) => void = () => undefined;
    ask.mockImplementation(
      () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
    );
    expect(await sendFromBar("SHOP", " ", NEW)).toBe(false);
    expect(ask).not.toHaveBeenCalled();
    const first = sendFromBar("SHOP", "why is SHOP-29 back in Dev?", TICKET);
    expect(await sendFromBar("SHOP", "again", NEW)).toBe(false);
    expect(ask).toHaveBeenCalledTimes(1);
    expect(ask).toHaveBeenCalledWith("ask_bar.send", {
      project: "SHOP",
      text: "why is SHOP-29 back in Dev?",
      about: "ticket",
      ticket: "SHOP-29",
    });
    answer({ agent: "a2", sent: true });
    expect(await first).toBe(true);
    expect(shownAgents.get()).toEqual({ SHOP: "a2" });
    expect(shellLayout.get().agentOpen).toBe(true);
    expect(agentPanelFocusRequests.get()).toMatchObject({ agent: "a2" });
  });

  it("Continue names its conversation; words that didn't go stay (the panel opens anyway)", async () => {
    ask.mockResolvedValue({ agent: "a1", sent: false });
    expect(await sendFromBar("SHOP", "and the cart?", CONTINUE)).toBe(false);
    expect(ask).toHaveBeenCalledWith("ask_bar.send", {
      project: "SHOP",
      text: "and the cart?",
      about: "continue",
      agent: "a1",
    });
    expect(shellLayout.get().agentOpen).toBe(true);
  });

  it("a look is told to Rust, and the bar asks again", async () => {
    ask.mockResolvedValue(null);
    lookedAt("a1");
    await act(async () => {
      await Promise.resolve();
    });
    expect(ask).toHaveBeenCalledWith("ask_bar.looked", { agent: "a1" });
    expect(lookedCount.get()).toBe(1);
  });
});

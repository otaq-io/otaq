// @vitest-environment happy-dom
// The one-line card's interactions (BRD-03, T-0460): resting on the key's row shows the full status after the
// tooltip delay and leaving hides it; the one action sends its kind and never selects the card.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TOOLTIP_DELAY } from "../components/Tooltip";
import type { CardActionKind } from "../generated/CardActionKind";
import type { TicketCardView } from "../generated/TicketCardView";
import { TicketCard } from ".";
import { RESTING } from "./look";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// SHOP-32 of *24 · The two views*: a check out of rounds, Decide.
const SHOP_32: TicketCardView = {
  key: "SHOP-32",
  key_label: "SHOP-32",
  epic: null,
  priority: null,
  title: "Address autocomplete",
  status: { text: "Security failed 3 times · unescaped SQL", dot: "attn", tone: "attention", icon: null },
  blocks: null,
  avatar: null,
  who: null,
  time: null,
  edge: "attention",
  action: { kind: "decide", label: "Decide", icon: null, primary: true },
  origin: null,
  hover: { status: "Security failed 3 times", details: ["Tokens: 312k"] },
  faded: null,
  fresh: false,
  open_panel: "Open panel",
  open_panel_keys: "↵",
  open_terminal: "Open terminal",
};

let host: HTMLDivElement;
let root: Root;
let actions: CardActionKind[];
let selected: number;

beforeEach(() => {
  vi.useFakeTimers();
  actions = [];
  selected = 0;
  host = document.body.appendChild(document.createElement("div"));
  root = createRoot(host);
  act(() => {
    root.render(
      createElement(TicketCard, {
        card: SHOP_32,
        onSelect: () => {
          selected += 1;
        },
        onAction: (kind) => {
          actions.push(kind);
        },
      }),
    );
  });
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
  vi.useRealTimers();
});

const bubble = () => host.querySelector<HTMLElement>(".card-hover");

describe("the one-line card", () => {
  it("resting on the key's row shows the full status; leaving hides it", () => {
    const head = host.querySelector<HTMLElement>(".card-head");
    if (!head) throw new Error("the key's row");
    act(() => {
      head.dispatchEvent(new MouseEvent("mouseover", { bubbles: true, relatedTarget: document.body }));
    });
    expect(bubble()).toBeNull();
    act(() => {
      vi.advanceTimersByTime(TOOLTIP_DELAY);
    });
    expect(bubble()?.textContent).toContain("Security failed 3 times");
    expect(bubble()?.textContent).toContain("Tokens: 312k");
    act(() => {
      head.dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: document.body }));
    });
    expect(bubble()).toBeNull();
  });

  const titleTip = () => host.querySelector<HTMLElement>(".tooltip");
  const restOnTitle = () => {
    const title = host.querySelector<HTMLElement>(".card-title");
    if (!title) throw new Error("the title");
    act(() => {
      title.dispatchEvent(new MouseEvent("mouseover", { bubbles: true, relatedTarget: document.body }));
    });
    act(() => {
      vi.advanceTimersByTime(TOOLTIP_DELAY);
    });
    return title;
  };

  it("the title's tooltip hides on pointer down, before the click opens the sheet (#6)", () => {
    const title = restOnTitle();
    expect(titleTip()?.textContent).toBe("Address autocomplete");
    act(() => {
      title.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
    });
    expect(titleTip()).toBeNull();
  });

  it("the key row's full status hides on pointer down and when the card becomes selected (its sheet opens, #6)", () => {
    const head = host.querySelector<HTMLElement>(".card-head");
    if (!head) throw new Error("the key's row");
    const rest = () => {
      act(() => {
        head.dispatchEvent(new MouseEvent("mouseover", { bubbles: true, relatedTarget: document.body }));
      });
      act(() => {
        vi.advanceTimersByTime(TOOLTIP_DELAY);
      });
    };
    rest();
    expect(bubble()).not.toBeNull();
    act(() => {
      head.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
    });
    expect(bubble()).toBeNull();
    rest();
    expect(bubble()).not.toBeNull();
    act(() => {
      root.render(createElement(TicketCard, { card: SHOP_32, state: { ...RESTING, selected: true } }));
    });
    expect(bubble()).toBeNull();
  });

  it("Decide sends its kind and doesn't select the card", () => {
    const button = host.querySelector<HTMLButtonElement>(".card-action button");
    if (!button) throw new Error("the one action");
    expect(button.textContent).toBe("Decide");
    act(() => {
      button.click();
    });
    expect(actions).toEqual(["decide"]);
    expect(selected).toBe(0);
  });
});

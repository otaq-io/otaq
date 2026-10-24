// @vitest-environment happy-dom
// The tooltip shows on focus, so it is mounted in a DOM.
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { Tooltip, TOOLTIP_DELAY, type TooltipProps } from "./Tooltip";

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

let unmount = (): void => undefined;

afterEach(() => {
  unmount();
  vi.useRealTimers();
});

/** Mounts a tooltip around a button, focuses the button and returns the bubble. */
function bubble(props: Omit<TooltipProps, "children">): HTMLElement | null {
  unmount();
  vi.useFakeTimers();
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  act(() => {
    root.render(
      createElement(Tooltip, { ...props, children: createElement("button", { type: "button" }, "x") }),
    );
  });
  unmount = () => {
    act(() => {
      root.unmount();
    });
    host.remove();
  };
  act(() => {
    host.querySelector("button")?.focus();
  });
  act(() => {
    vi.advanceTimersByTime(0);
  });
  return host.querySelector<HTMLElement>('[role="tooltip"]');
}

describe("Tooltip", () => {
  it("without an icon shows the text and the shortcut, as before", () => {
    expect(bubble({ text: "Save", keys: "⌘S" })?.innerHTML).toBe(
      '<span>Save</span><kbd class="tooltip-keys">⌘S</kbd>',
    );
    expect(bubble({ text: "Expand" })?.innerHTML).toBe("<span>Expand</span>");
  });

  it("with an icon shows it before the text, hidden from screen readers", () => {
    const tip = bubble({ text: "Copied", icon: "check" });
    const [icon, text] = Array.from(tip?.children ?? []);
    expect(icon?.className).toBe("icon");
    expect(icon?.getAttribute("aria-hidden")).toBe("true");
    expect((icon as HTMLElement | undefined)?.style.maskImage).toContain("lucide-check'");
    expect(text?.outerHTML).toBe("<span>Copied</span>");
    expect(tip?.textContent).toBe("Copied");
  });

  it("goes when the pointer moves off its anchor, even when the webview never said it left (T-0506)", () => {
    const { button, outside, tip } = hovered();
    expect(tip()?.textContent).toBe("Remove variable");
    // No mouseleave: what the pointer was over went away, or the view changed under it.
    act(() => {
      outside.dispatchEvent(new PointerEvent("pointermove", { bubbles: true }));
    });
    expect(tip()).toBeNull();
    // A move over the anchor itself keeps it.
    hover(button);
    act(() => {
      button.dispatchEvent(new PointerEvent("pointermove", { bubbles: true }));
    });
    expect(tip()).not.toBeNull();
  });

  it("goes when the pointer leaves the window", () => {
    const { button, tip } = hovered();
    act(() => {
      button.dispatchEvent(new PointerEvent("pointerout", { bubbles: true, relatedTarget: null }));
    });
    expect(tip()).toBeNull();
  });

  it("goes on Esc even when a field stops the key, and on any press", () => {
    const { button, outside, tip } = hovered();
    button.addEventListener("keydown", (e) => {
      e.stopPropagation();
    });
    act(() => {
      button.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(tip()).toBeNull();
    hover(button);
    expect(tip()).not.toBeNull();
    act(() => {
      outside.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    });
    expect(tip()).toBeNull();
  });

  it("a press that focuses its anchor shows nothing; keyboard focus does", () => {
    const { button, tip } = hovered();
    act(() => {
      button.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
      button.focus();
      button.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
    });
    act(() => {
      vi.advanceTimersByTime(TOOLTIP_DELAY);
    });
    // The Files button pressed: the view changes and its tooltip doesn't stay under the toolbar.
    expect(tip()).toBeNull();
    act(() => {
      button.blur();
    });
    act(() => {
      button.focus();
    });
    act(() => {
      vi.advanceTimersByTime(0);
    });
    expect(tip()).not.toBeNull();
    // Shown by the keyboard, it stays while the pointer moves elsewhere.
    act(() => {
      document.body.dispatchEvent(new PointerEvent("pointermove", { bubbles: true }));
    });
    expect(tip()).not.toBeNull();
  });
});

function hover(button: Element) {
  act(() => {
    button.dispatchEvent(new MouseEvent("mouseover", { bubbles: true, relatedTarget: document.body }));
  });
  act(() => {
    vi.advanceTimersByTime(TOOLTIP_DELAY);
  });
}

/** Mounts a tooltip around a button with a spot outside it, and shows it by hovering the button. */
function hovered(): { button: HTMLButtonElement; outside: HTMLElement; tip: () => HTMLElement | null } {
  unmount();
  vi.useFakeTimers();
  const host = document.body.appendChild(document.createElement("div"));
  const outside = document.body.appendChild(document.createElement("p"));
  const root = createRoot(host);
  act(() => {
    root.render(
      createElement(Tooltip, {
        text: "Remove variable",
        children: createElement("button", { type: "button" }, "x"),
      }),
    );
  });
  unmount = () => {
    act(() => {
      root.unmount();
    });
    host.remove();
    outside.remove();
  };
  const button = host.querySelector("button");
  if (!button) throw new Error("no button");
  hover(button);
  return { button, outside, tip: () => host.querySelector<HTMLElement>('[role="tooltip"]') };
}

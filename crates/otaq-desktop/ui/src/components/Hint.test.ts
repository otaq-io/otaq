// @vitest-environment happy-dom
// A one-time hint shows while the service says it wasn't dismissed; its button dismisses it for good, and nothing
// else closes it.
import { act, createElement, createRef } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { HintView } from "../generated/HintView";
import { request } from "../tauri";
import { OneTimeHint } from "./Hint";

vi.mock("../tauri", () => ({ request: vi.fn() }));
const requestMock = vi.mocked(request);

const VIEW: HintView = {
  id: "gallery_sample",
  title: null,
  text: "A hint like this shows once, when something first matters, and never again.",
  button: "Got it",
  dismiss: null,
};

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

let unmount = (): void => undefined;

afterEach(() => {
  unmount();
  requestMock.mockReset();
});

/** Mounts the hint next to a button and lets the view's answer arrive. */
async function mount(): Promise<HTMLElement> {
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  const anchor = createRef<HTMLButtonElement>();
  await act(async () => {
    root.render(
      createElement(
        "div",
        null,
        createElement("button", { type: "button", ref: anchor }, "anchor"),
        createElement(OneTimeHint, { hint: "gallery_sample", anchor }),
      ),
    );
    await Promise.resolve();
  });
  unmount = () => {
    act(() => {
      root.unmount();
    });
    host.remove();
  };
  return host;
}

describe("OneTimeHint", () => {
  it("shows its one sentence and one button, and the button dismisses it for good", async () => {
    requestMock.mockImplementation((method) => Promise.resolve(method === "hints.view" ? VIEW : null));
    const host = await mount();
    expect(requestMock).toHaveBeenCalledWith("hints.view", { hint: "gallery_sample" });
    const hint = host.querySelector(".hint");
    expect(hint?.querySelector(".hint-text")?.textContent).toBe(VIEW.text);
    const buttons = hint?.querySelectorAll("button") ?? [];
    expect(buttons).toHaveLength(1);

    // Esc and a press elsewhere leave it up.
    act(() => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
      document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    });
    expect(host.querySelector(".hint")).not.toBeNull();

    act(() => {
      buttons[0]?.click();
    });
    expect(requestMock).toHaveBeenCalledWith("hints.dismiss", { hint: "gallery_sample" });
    expect(host.querySelector(".hint")).toBeNull();
  });

  it("shows nothing once dismissed", async () => {
    requestMock.mockResolvedValue(null);
    const host = await mount();
    expect(host.querySelector(".hint")).toBeNull();
  });

  it("shows nothing when the service can't say", async () => {
    requestMock.mockRejectedValue(new Error("no service"));
    const host = await mount();
    expect(host.querySelector(".hint")).toBeNull();
  });
});

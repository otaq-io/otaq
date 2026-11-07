// @vitest-environment happy-dom
// Settings' own dropdowns (the model dropdowns of Settings → Agents, an agent's Model picker) and a press outside
// them: each closes, as every menu and popover does.
import { act, createElement, type ReactElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { type Choice, Dropdown, ModelPicker } from "./ModelPicker";

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

let unmount = (): void => undefined;

afterEach(() => {
  unmount();
});

function option(id: string, checked = false): Choice {
  return { kind: "option", id, label: id, detail: null, mono: false, detailMono: false, checked, icon: null };
}

/** Mounts `element`, opens its list with the field's button and returns the page and a spot outside it. */
function open(element: ReactElement, list: string): { host: HTMLElement; outside: HTMLElement } {
  unmount();
  const host = document.body.appendChild(document.createElement("div"));
  const outside = document.body.appendChild(document.createElement("p"));
  const root = createRoot(host);
  act(() => {
    root.render(element);
  });
  unmount = () => {
    act(() => {
      root.unmount();
    });
    host.remove();
    outside.remove();
  };
  act(() => {
    host.querySelector<HTMLButtonElement>("button.select")?.click();
  });
  expect(host.querySelector(list)).not.toBeNull();
  return { host, outside };
}

function press(target: Element | null) {
  act(() => {
    target?.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
  });
}

describe("Settings' dropdowns and a press outside", () => {
  it("closes the floating dropdown (Default model) and keeps it open on a press inside", () => {
    const onChoose = vi.fn();
    const { host, outside } = open(
      createElement(Dropdown, {
        label: "Default model",
        value: "Claude Code · sonnet",
        choices: () => [option("sonnet", true), option("opus")],
        readOnly: false,
        className: "settings-agents-dropdown",
        onChoose,
      }),
      '[role="listbox"]',
    );
    press(host.querySelector('[role="listbox"]'));
    expect(host.querySelector('[role="listbox"]')).not.toBeNull();
    press(outside);
    expect(host.querySelector('[role="listbox"]')).toBeNull();
    expect(host.querySelector("button.select")?.getAttribute("aria-expanded")).toBe("false");
    expect(onChoose).not.toHaveBeenCalled();
  });

  it("closes an agent's Model picker, whose list floats under the field", () => {
    const { host, outside } = open(
      createElement(ModelPicker, {
        label: "Model",
        picker: {
          value: { label: "sonnet", detail: null, mono: false, no_longer_listed: null },
          groups: [],
          status: null,
          hint: "",
        },
        onPick: vi.fn(),
        onRetry: vi.fn(),
      }),
      '[role="listbox"]',
    );
    press(outside);
    expect(host.querySelector('[role="listbox"]')).toBeNull();
  });
});

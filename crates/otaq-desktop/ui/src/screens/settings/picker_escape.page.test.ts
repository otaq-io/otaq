// @vitest-environment happy-dom
// Esc in a picker above Settings closes only the picker (F-B-4): the open list is the topmost Esc layer, so one Esc
// closes it wherever the focus really is, and the next Esc closes Settings. In the app the floating list is hidden
// until it is placed, and a hidden element takes no focus, so its first focus fails and the focus stays on the field;
// the page here refuses focus the same way, and the list takes it once shown. The same for a Settings row's dropdown.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import { RegistriesContext, registerScreens } from "../../app/registry";
import type { ModelPickerView } from "../../generated/ModelPickerView";
import type { SettingsView } from "../../generated/SettingsView";
import { request } from "../../tauri";
import { ModelPicker } from "./ModelPicker";
import { sectionComponent } from "./Rows";
import { Settings } from "./Settings";

vi.mock("../../tauri", async (original) => ({
  ...(await original<typeof import("../../tauri")>()),
  request: vi.fn(),
}));

// The view loads at once, with no event stream.
vi.mock("../../service", async (original) => ({
  ...(await original<typeof import("../../service")>()),
  watchView: (load: () => Promise<unknown>, _on: unknown, onChange: (state: { view?: unknown }) => void) => {
    void load().then((view) => {
      onChange({ view });
    });
    return () => undefined;
  },
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const PICKER: ModelPickerView = {
  value: { label: "sonnet", detail: null, mono: false, no_longer_listed: null },
  groups: [
    {
      heading: null,
      items: [
        { value: "sonnet", label: "sonnet", detail: null, mono: false, selected: true },
        { value: "opus", label: "opus", detail: null, mono: false, selected: false },
      ],
      empty: null,
    },
  ],
  status: null,
  hint: "",
};

// The shape `settings_page.rs` sends: one row with a select, drawn as a dropdown.
const VIEW: SettingsView = {
  title: "Settings",
  footnote: "",
  close_label: "Close settings",
  nav: [
    {
      label: "Demo",
      icon: "folder",
      target: { page: "global", section: "agent_tools", sections: ["agent_tools", "data"] },
    },
  ],
  page: {
    page: "global",
    title: "General",
    sections: [
      { id: "agent_tools", title: null, intro: null, rows: [], note: null },
      {
        id: "data",
        title: null,
        intro: null,
        rows: [
          {
            field: "theme",
            label: "Theme",
            description: "",
            control: {
              kind: "select",
              choices: [
                { id: "light", label: "Light" },
                { id: "dark", label: "Dark" },
              ],
              value: "light",
            },
            dimmed: false,
            reset: null,
            button: null,
            set_by: null,
          },
        ],
        note: null,
      },
    ],
  },
};

const app = registerScreens({
  settings: (a) => {
    a.overlays.register("settings", { Component: Settings });
    a.settings.global.register("agent_tools", {
      Component: () =>
        createElement(ModelPicker, {
          label: "Model",
          picker: PICKER,
          onPick: () => undefined,
          onRetry: () => undefined,
        }),
    });
    a.settings.global.register("data", { Component: sectionComponent("data") });
  },
});

let root: Root;
let host: HTMLElement;
let restoreFocus: () => void;

/** Whether `el` or an element around it is still hidden (a floating layer not yet placed). */
function hidden(el: Element): boolean {
  for (let at: Element | null = el; at !== null; at = at.parentElement)
    if (at instanceof HTMLElement && at.style.visibility === "hidden") return true;
  return false;
}

beforeEach(() => {
  vi.mocked(request).mockResolvedValue(VIEW);
  // A hidden element takes no focus, as in the app's web view.
  const original = Reflect.get<HTMLElement, "focus">(HTMLElement.prototype, "focus");
  const spy = vi.spyOn(HTMLElement.prototype, "focus").mockImplementation(function (
    this: HTMLElement,
    options?: FocusOptions,
  ) {
    if (!hidden(this)) original.call(this, options);
  });
  restoreFocus = () => {
    spy.mockRestore();
  };
  host = document.body.appendChild(document.createElement("div"));
  root = createRoot(host);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  restoreFocus();
  document.body.replaceChildren();
});

async function openSettings(onClose: () => void) {
  act(() => {
    root.render(
      createElement(
        RegistriesContext,
        { value: app },
        createElement(Settings, { params: undefined, close: onClose }),
      ),
    );
  });
  // The view's answer.
  await act(async () => {
    await Promise.resolve();
  });
}

/** Esc pressed where the focus really is, as the web view sends it. */
function escape() {
  act(() => {
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true, cancelable: true }),
    );
  });
}

/** Opens a field's list with a click, as the user does: the press first moves the focus to the field. */
function clickOpen(field: HTMLButtonElement | null) {
  if (!field) throw new Error("no field");
  act(() => {
    field.focus();
    field.click();
  });
}

const nextFrame = () =>
  act(
    () =>
      new Promise<void>((resolve) => {
        requestAnimationFrame(() => {
          resolve();
        });
      }),
  );

const list = () => document.querySelector('[role="listbox"]');

describe("Esc in a picker above Settings", () => {
  it.each([
    ["the Agents page's model picker", ".settings-model-field"],
    ["a Settings row's dropdown", 'button.select[aria-label="Theme"]'],
  ])(
    "%s: one Esc with the focus where it is closes only the list; the next closes Settings",
    async (_, selector) => {
      let closed = 0;
      await openSettings(() => {
        closed += 1;
      });
      const field = host.querySelector<HTMLButtonElement>(selector);
      clickOpen(field);
      expect(list()).not.toBeNull();
      // The list was hidden when it asked for the focus: the focus is still on the field.
      expect(document.activeElement).toBe(field);
      escape();
      expect(list()).toBeNull();
      expect(closed).toBe(0);
      expect(document.activeElement).toBe(field);
      escape();
      expect(closed).toBe(1);
    },
  );

  it.each([
    ["the Agents page's model picker", ".settings-model-field"],
    ["a Settings row's dropdown", 'button.select[aria-label="Theme"]'],
  ])("%s: the list takes the focus once shown, and its own Esc closes it first", async (_, selector) => {
    let closed = 0;
    await openSettings(() => {
      closed += 1;
    });
    const field = host.querySelector<HTMLButtonElement>(selector);
    clickOpen(field);
    await nextFrame();
    expect(document.activeElement).toBe(list());
    escape();
    expect(list()).toBeNull();
    expect(closed).toBe(0);
    expect(document.activeElement).toBe(field);
    escape();
    expect(closed).toBe(1);
  });
});

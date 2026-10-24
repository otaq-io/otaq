// @vitest-environment happy-dom
// Esc in the whole Settings window (T-0506; d0097): a field or a list that has something to close takes the first Esc
// (the Environment variables line saves, the Model picker's list closes), and the next Esc closes Settings. The window
// takes the focus once, when it opens, never again on a render.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import { RegistriesContext, registerScreens } from "../../app/registry";
import type { SettingsRow } from "../../generated/SettingsRow";
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

// The shape `settings_page.rs` sends (`env_vars_row`).
function envRow(rows: { name: string; value: string }[]): SettingsRow {
  return {
    field: "env_vars",
    label: "Environment variables",
    description: "Set in every agent session, Run and Shell of this project.",
    control: {
      kind: "env_vars",
      rows: rows.map((r) => ({ ...r, problem: null, remove_label: `Remove ${r.name}` })),
      hint_example: "CI = true",
      hint: "e.g. so tools never wait for input or start watch mode",
      empty: "No variables. Sessions get your normal environment.",
      add_label: "Add variable",
      drop_label: "Remove variable",
      applies: "Changes apply to sessions, Runs and Shells started after this.",
      name_placeholder: "NAME",
      value_placeholder: "value",
    },
    dimmed: false,
    reset: null,
    button: null,
  };
}

function view(vars: { name: string; value: string }[]): SettingsView {
  return {
    title: "Settings",
    global_heading: "Global",
    projects_heading: "Projects",
    close_label: "Close settings",
    nav: [{ label: "demo", icon: "folder", target: { page: "project", key: "DEMO" }, alert: false }],
    page: {
      page: "project",
      key: "DEMO",
      title: "demo",
      intro: "",
      sections: [
        { id: "general", title: null, intro: null, rows: [], note: null },
        { id: "commands", title: null, intro: null, rows: [envRow(vars)], note: null },
      ],
    },
  };
}

/** A section with an agent's Model picker, as one agent's page draws it. */
function PickerSection() {
  return createElement(ModelPicker, {
    label: "Model",
    picker: {
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
    },
    onPick: () => undefined,
    onRetry: () => undefined,
  });
}

const app = registerScreens({
  settings: (a) => {
    a.overlays.register("settings", { Component: Settings });
    a.settings.project.register("general", { Component: PickerSection });
    a.settings.project.register("commands", { Component: sectionComponent("commands") });
  },
});

let root: Root;
let host: HTMLElement;
let closed: number;
let edits: { field: string; value: unknown }[];

function mount() {
  // A new `close` on each render, as the frame hands it.
  act(() => {
    root.render(
      createElement(
        RegistriesContext,
        { value: app },
        createElement(Settings, {
          params: { project: "DEMO" },
          close: () => {
            closed += 1;
          },
        }),
      ),
    );
  });
}

async function open() {
  mount();
  // The view's answer.
  await act(async () => {
    await Promise.resolve();
  });
}

function type(input: HTMLInputElement, text: string) {
  act(() => {
    Reflect.set(HTMLInputElement.prototype, "value", text, input);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

function key(target: Element | null, k: string) {
  act(() => {
    target?.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));
  });
}

const button = (text: string) =>
  [...host.querySelectorAll("button")].find((b) => b.textContent === text) ?? null;

beforeEach(() => {
  closed = 0;
  edits = [];
  let vars: { name: string; value: string }[] = [];
  vi.mocked(request).mockImplementation((method: string, params: unknown = null) => {
    if (method === "settings.edit") {
      const edit = params as { field: string; value: { name: string; value: string } };
      edits.push({ field: edit.field, value: edit.value });
      if (edit.field === "env_vars.add") vars = [...vars, edit.value];
    }
    return Promise.resolve(view(vars));
  });
  host = document.body.appendChild(document.createElement("div"));
  root = createRoot(host);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  document.body.replaceChildren();
});

describe("Esc in Settings", () => {
  it("name, value, Tab, Esc keeps the variable; Esc again closes Settings", async () => {
    await open();
    act(() => {
      button("Add variable")?.click();
    });
    const [name, value] = [...host.querySelectorAll<HTMLInputElement>(".settings-envvar input")];
    const drop = host.querySelector<HTMLButtonElement>('button[aria-label="Remove variable"]');
    if (!name || !value || !drop) throw new Error("no new line");
    type(name, "CI");
    act(() => {
      value.focus();
    });
    type(value, "true");
    key(value, "Tab");
    act(() => {
      drop.focus();
    });
    key(drop, "Escape");
    expect(closed).toBe(0);
    await act(async () => {
      await Promise.resolve();
    });
    expect(edits).toEqual([{ field: "env_vars.add", value: { name: "CI", value: "true" } }]);
    const saved = [...host.querySelectorAll<HTMLInputElement>(".settings-envvar input")].map((i) => i.value);
    expect(saved).toEqual(["CI", "true"]);
    expect(host.textContent).not.toContain("No variables");
    key(document.activeElement, "Escape");
    expect(closed).toBe(1);
  });

  it("closes the Model picker's list first, then Settings", async () => {
    await open();
    const field = host.querySelector<HTMLButtonElement>(".settings-model-field");
    act(() => {
      field?.click();
    });
    const list = host.querySelector('[role="listbox"]');
    expect(list).not.toBeNull();
    expect(document.activeElement).toBe(list);
    key(list, "Escape");
    expect(host.querySelector('[role="listbox"]')).toBeNull();
    expect(closed).toBe(0);
    expect(document.activeElement).toBe(field);
    key(field, "Escape");
    expect(closed).toBe(1);
  });

  it("takes the focus once: a render with a new close leaves the field being typed in", async () => {
    await open();
    expect(document.activeElement).toBe(host.querySelector(".settings"));
    act(() => {
      button("Add variable")?.click();
    });
    const [name] = [...host.querySelectorAll<HTMLInputElement>(".settings-envvar input")];
    if (!name) throw new Error("no new line");
    expect(document.activeElement).toBe(name);
    mount();
    expect(document.activeElement).toBe(name);
  });
});

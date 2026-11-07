// @vitest-environment happy-dom
// The Repository card's PR provider line: "GitHub · otaq-io/shop-web", a fact of the repository's remote, with how its
// tool stands on this computer; when it isn't ready here, Set up… opens app Settings where it is set up.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { overlays } from "../../app/overlays";
import type { SettingsPage } from "../../generated/SettingsPage";
import type { SettingsProvider } from "../../generated/SettingsProvider";
import type { SettingsSection } from "../../generated/SettingsSection";
import { request } from "../../tauri";
import { SettingsContext } from "./context";
import { PROVIDER_SET_UP_SECTION, Section } from "./Rows";

vi.mock("../../tauri", async (original) => ({
  ...(await original<typeof import("../../tauri")>()),
  request: vi.fn(),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SIGNED_OUT: SettingsProvider = {
  line: "GitHub · otaq-io/shop-web",
  status: "gh: not signed in",
  icon: "triangle-alert",
  look: "attention",
  set_up: "Set up…",
};

const SECTION: SettingsSection<string> = {
  id: "repository",
  title: "Repository",
  intro: null,
  rows: [
    {
      field: "",
      label: "Pull requests through",
      description: "Set up once on this computer, in app Settings.",
      control: { kind: "provider" },
      dimmed: false,
      reset: null,
      button: null,
      set_by: null,
    },
  ],
  note: null,
};

const PAGE = { page: "project", key: "SHOP", title: "shop-web", intro: "", sections: [] } as SettingsPage;

let root: Root;
let box: HTMLElement;

beforeEach(() => {
  box = document.createElement("div");
  document.body.append(box);
  root = createRoot(box);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  box.remove();
  overlays.store.set([]);
  vi.mocked(request).mockReset();
});

async function draw(provider: SettingsProvider) {
  vi.mocked(request).mockResolvedValue(provider);
  await act(async () => {
    root.render(
      createElement(
        SettingsContext,
        { value: { page: PAGE, project: "SHOP", edit: () => undefined, act: () => undefined } },
        createElement(Section, { section: SECTION }),
      ),
    );
    await Promise.resolve();
  });
}

describe("the PR provider line", () => {
  it("names the provider and the repository; Set up… opens app Settings when it isn't ready here", async () => {
    await draw(SIGNED_OUT);
    expect(request).toHaveBeenCalledWith("project_settings.provider", { project: "SHOP" });
    expect(box.textContent).toContain("Pull requests through");
    expect(box.textContent).toContain("GitHub · otaq-io/shop-web");
    expect(box.textContent).toContain("gh: not signed in");
    const setUp = [...box.querySelectorAll("button")].find((b) => b.textContent === "Set up…");
    act(() => {
      setUp?.click();
    });
    expect(overlays.store.get()).toEqual([{ id: "settings", params: { section: PROVIDER_SET_UP_SECTION } }]);
  });

  it("has no Set up… once the provider is ready on this computer", async () => {
    await draw({ ...SIGNED_OUT, status: "gh: signed in", icon: "circle-check", look: "ready", set_up: null });
    expect(box.textContent).toContain("gh: signed in");
    expect([...box.querySelectorAll("button")].some((b) => b.textContent === "Set up…")).toBe(false);
  });
});

// @vitest-environment happy-dom
// Settings → About's tuq row (decisions 100, 102): the line `settings.about` words, under the version, with tuq on
// two tools at different versions naming both; nothing at all when no connected tool has tuq.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { TuqRow } from "./About";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => Promise.resolve(null)),
  subscribeEvents: vi.fn(() => Promise.resolve()),
  windowAction: vi.fn(),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

let root: Root | null = null;

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  document.body.innerHTML = "";
});

function render(tuq: string | null): HTMLElement {
  const host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => {
    root?.render(
      createElement(TuqRow, {
        view: { tuq, tuq_description: "Otaq’s own workflow skills come with this version." },
      }),
    );
  });
  return host;
}

describe("About's tuq row", () => {
  it("names each tool's tuq when they differ", () => {
    const host = render("tuq 2026.11.12; Claude Code: 2026.11.10");
    const row = host.querySelector('[data-row="tuq"]');
    expect(row?.querySelector(".settings-row-label")?.textContent).toBe(
      "tuq 2026.11.12; Claude Code: 2026.11.10",
    );
    expect(row?.querySelector(".settings-row-description")?.textContent).toContain("workflow skills");
  });

  it("says nothing when no connected tool has tuq", () => {
    const host = render(null);
    expect(host.querySelector('[data-row="tuq"]')).toBeNull();
    expect(host.textContent).toBe("");
  });
});

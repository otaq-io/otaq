// @vitest-environment happy-dom
// A file opened at a line (d0117 point 8, a terminal's `src/a.ts:42`): File scrolls the line to the middle and marks it,
// once per open.
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";

import type { FilesFilePage } from "../../generated/FilesFilePage";
import { FileBody } from "./FilesTab";
import type { CommentContext } from "./Hunks";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => ({ ready: new Promise(() => undefined), stop: () => undefined })),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const page: FilesFilePage = {
  path: "src/pricing.ts",
  child: null,
  subtitle: "in SHOP-22’s worktree",
  modes: ["File", "Diff"],
  opens_in: "diff",
  split_modes: ["Unified", "Split"],
  side_by_side: false,
  side_by_side_from: 900,
  can_file: true,
  can_diff: true,
  new_ticket: "New ticket from this file",
  new_ticket_body: "",
  lines: [1, 2, 3].map((number) => ({ number, spans: [{ text: `line ${String(number)}`, token: "plain" }] })),
  note: null,
  writing: null,
};

describe("OpenFile.line", () => {
  it("opens as File scrolled to the line, the line marked", () => {
    const scrolled: string[] = [];
    Element.prototype.scrollIntoView = vi.fn(function (this: Element) {
      scrolled.push(this.getAttribute("data-line") ?? "");
    });
    const container = document.createElement("div");
    const root = createRoot(container);
    const render = (file: { path: string; child: null; mode: "file" | null; line: number | null }) => {
      act(() => {
        root.render(
          createElement(FileBody, {
            ticket: "SHOP-22",
            page,
            file,
            ctx: {} as CommentContext,
            width: 1200,
            onMode: () => undefined,
            onSplit: () => undefined,
          }),
        );
      });
    };
    const file = { path: page.path, child: null, mode: "file" as const, line: 2 };
    render(file);
    expect(scrolled).toEqual(["2"]);
    expect(container.querySelector("[data-marked]")?.getAttribute("data-line")).toBe("2");
    expect(container.querySelectorAll("[data-marked]")).toHaveLength(1);
    // The same open drawn again doesn't scroll again; asked again, it does.
    render(file);
    expect(scrolled).toEqual(["2"]);
    render({ ...file });
    expect(scrolled).toEqual(["2", "2"]);
    // From the tree: no line, nothing marked.
    render({ path: page.path, child: null, mode: null, line: null });
    expect(container.querySelector("[data-marked]")).toBeNull();
    act(() => {
      root.unmount();
    });
  });
});

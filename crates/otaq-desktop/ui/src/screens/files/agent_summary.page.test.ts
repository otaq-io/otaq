// @vitest-environment happy-dom
// Changes' Agent summary is drawn as Markdown, as the same text is in the thread (tuq 110 refined; 0.3 regression
// F-C-1), in its own 12px secondary look under the title (`diff-summary-text`), still folded by a click.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { FilesReviewNotes } from "../../generated/FilesReviewNotes";
import { Notes } from "./FilesTab";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => new Promise(() => undefined)),
}));

const NOTES: FilesReviewNotes = {
  summary: {
    title: "Agent summary",
    text: "Added `formatPrice`'s comment:\n- Piece 1\n- Piece 2",
    note: "no tests are run here",
  },
  review: null,
};

let host: HTMLElement;
let root: Root;

beforeAll(async () => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  // The test runner leaves CSS imports empty: the description's and the Files tab's style sheets, as the app loads them.
  const fsModule = "node:fs";
  const fs = (await import(fsModule)) as { readFileSync(path: string, encoding: "utf8"): string };
  const here = (import.meta as ImportMeta & { dirname: string }).dirname;
  const style = document.createElement("style");
  style.textContent = ["../../markdown/markdown.css", "files.css"]
    .map((sheet) => fs.readFileSync(`${here}/${sheet}`, "utf8"))
    .join("\n");
  document.head.append(style);
});

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => {
    root.render(
      createElement(Notes, {
        ticket: "SHOP-4",
        notes: NOTES,
        decisions: null,
        onChangeDecision: () => undefined,
      }),
    );
  });
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  host.remove();
});

describe("the Agent summary", () => {
  it("draws its inline code and its list as Markdown, in the summary's own look", () => {
    const text = host.querySelector(".diff-summary .md-comment.diff-summary-text");
    expect(text).not.toBeNull();
    expect(text?.querySelector("code")?.textContent).toBe("formatPrice");
    expect([...(text?.querySelectorAll("li") ?? [])].map((li) => li.textContent)).toEqual([
      "Piece 1",
      "Piece 2",
    ]);
    expect(text?.textContent).not.toContain("`");
    // 12px secondary, indented under the title, not a comment's 13px primary.
    const prose = text?.querySelector(".ProseMirror");
    expect(prose && getComputedStyle(prose).fontSize).toBe("12px");
    expect(text && getComputedStyle(text).paddingLeft).toBe("36px");
  });

  it("still folds with a click on its title", () => {
    const head = host.querySelector<HTMLButtonElement>(".diff-summary-head");
    act(() => {
      head?.click();
    });
    expect(host.querySelector(".diff-summary-text")).toBeNull();
    expect(head?.getAttribute("aria-expanded")).toBe("false");
  });
});

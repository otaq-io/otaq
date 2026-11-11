// @vitest-environment happy-dom
// Show changes after "Your pre-commit hook changed 2 files": History's Merged row has it, and the window draws each
// file's lines from what was reviewed to what landed (`files.diff` with `since` and `until`).
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { beforeAll, expect, test, vi } from "vitest";

import type { DiffFileView } from "../../generated/DiffFileView";
import type { HistoryView } from "../../generated/HistoryView";
import type { HookChangesView } from "../../generated/HookChangesView";
import { request } from "../../tauri";
import { HistoryBody } from "../activity/History";
import { HookChangesDialog, loadHookChanges } from "./HookChanges";

vi.mock("../../tauri", () => ({
  request: vi.fn(),
  subscribeEvents: vi.fn(() => Promise.resolve()),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const CHANGES: HookChangesView = {
  title: "Your pre-commit hook changed 2 files",
  show: "Show changes",
  note: "What the hook changed after your review: the difference between what you reviewed and what landed.",
  reviewed: "aaa111",
  landed: "bbb222",
  files: ["calc.js", "util.js"],
  close: "Close",
};

/** One file with the hook's header line added. */
const header = (path: string) =>
  ({
    path,
    hunks: [
      {
        lines: [
          { kind: "added", sign: "+", spans: [{ text: "// formatted" }] },
          { kind: "context", sign: " ", spans: [{ text: "add" }] },
        ],
      },
    ],
  }) as unknown as DiffFileView;

const asked = vi.mocked(request);

test("History's Merged row says which hook changed how many files, with Show changes", () => {
  const view: HistoryView = {
    filters: [{ filter: "all", label: "All", kind: null }],
    order: "newest first",
    copy_hint: "Copy session ID",
    copied: "Copied",
    empty: "Nothing has happened to this ticket yet.",
    total: null,
    key: "SHOP-18",
    rows: [
      {
        title: "Merged",
        detail: "into main · Your pre-commit hook changed 2 files",
        when: "10:22",
        who: "you · 10:22",
        cost: null,
        tool_session_id: null,
        quiet: false,
        short_id: null,
        range: null,
        mark: { kind: "you" },
        opens: null,
        hook_changes: CHANGES,
        kind: "stage",
      },
      {
        title: "Merged",
        detail: "into main",
        when: "09:22",
        who: "you · 09:22",
        cost: null,
        tool_session_id: null,
        quiet: false,
        short_id: null,
        range: null,
        mark: { kind: "you" },
        opens: null,
        hook_changes: null,
        kind: "stage",
      },
    ],
  };
  const html = renderToStaticMarkup(createElement(HistoryBody, { view }));
  expect(html).toContain("into main · Your pre-commit hook changed 2 files");
  // Only the merge a hook changed has it.
  expect(html.split("Show changes")).toHaveLength(2);
});

test("Show changes reads each file from what was reviewed to what landed and draws its lines", async () => {
  asked.mockReset();
  asked.mockImplementation((method: string, params?: unknown) => {
    if (method === "files.diff") return Promise.resolve(header((params as { path: string }).path));
    return new Promise(() => undefined);
  });
  const files = await loadHookChanges("SHOP-18", CHANGES);
  expect(files.map((f) => f.path)).toEqual(["calc.js", "util.js"]);
  expect(asked).toHaveBeenCalledWith("files.diff", {
    key: "SHOP-18",
    path: "calc.js",
    since: "aaa111",
    until: "bbb222",
  });

  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  let closed = 0;
  await act(async () => {
    root.render(
      createElement(HookChangesDialog, {
        params: { ticket: "SHOP-18", changes: CHANGES },
        close: () => {
          closed += 1;
        },
      }),
    );
    await Promise.resolve();
  });
  const text = document.body.textContent;
  expect(text).toContain("Your pre-commit hook changed 2 files");
  expect(text).toContain("calc.js");
  expect(text).toContain("util.js");
  expect(text.split("// formatted")).toHaveLength(3);
  const close = [...document.body.querySelectorAll("button")].find((b) => b.textContent === "Close");
  act(() => {
    close?.click();
  });
  expect(closed).toBe(1);
  act(() => {
    root.unmount();
  });
  host.remove();
});

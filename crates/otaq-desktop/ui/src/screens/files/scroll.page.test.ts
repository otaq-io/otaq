// @vitest-environment happy-dom
// OTQ-8: under Changed, the highlighted row follows All changes as it scrolls (the file at the top of the pane, none
// above the first file or in a closed folder), a click highlights its file at once without running through the files
// it scrolls past, and a second click on the same row scrolls back to it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { FilesChangeRow } from "../../generated/FilesChangeRow";
import type { FilesChangesView } from "../../generated/FilesChangesView";
import type { FilesTabView } from "../../generated/FilesTabView";
import type { FilesTreeRow } from "../../generated/FilesTreeRow";
import { FilesTabBody } from "./FilesTab";
import { type FilesState, NOTHING_OPEN } from "./logic";

vi.mock("../../tauri", () => ({
  request: vi.fn(() => new Promise(() => undefined)),
  subscribeEvents: vi.fn(() => ({ ready: new Promise(() => undefined), stop: () => undefined })),
}));

beforeAll(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

const row = (path: string, extra: Partial<FilesTreeRow> = {}): FilesTreeRow => ({
  name: path.split("/").pop() ?? path,
  path,
  kind: "file",
  depth: 0,
  mark: null,
  quiet: false,
  opens: true,
  tags: [],
  child: null,
  chips: [],
  changed_below: null,
  branch: null,
  struck: false,
  ...extra,
});

const change = (path: string): FilesChangeRow => ({
  id: path,
  path,
  label: path,
  child: null,
  mark: { letter: "M", status: "modified", counts: "+1 −1", uncommitted: null },
  note: null,
  collapsed: "Collapsed.",
  binary_label: null,
  rename_label: null,
  rows: 5,
  can_open: true,
  commentable: false,
  tags: [],
  pieces: [],
});

const FILES = ["README.md", "e2e/login.spec.ts", "src/middleware.ts"];

const VIEW: FilesTabView = {
  key: "OTQ-8",
  source: {
    kind: "worktree",
    title: "OTQ-8’s worktree",
    subtitle: "otaq/OTQ-8 · from main",
    legend: [],
    stages: [],
  },
  search: { placeholder: "Search files by name", keys: "⌥⌘P" },
  filters: ["All files", "Changed · 3"],
  changed: 3,
  opens_on: "changed",
  rows: [],
  pick: "Pick a file.",
};

const CHANGES: FilesChangesView = {
  key: "OTQ-8",
  revision: "r1",
  title: "All changes",
  summary: "3 files",
  note: null,
  modes: ["Unified", "Split"],
  side_by_side: false,
  pieces: null,
  side_by_side_from: 900,
  files: FILES.map(change),
  tree: [
    row("README.md"),
    row("e2e", { kind: "folder" }),
    row("e2e/login.spec.ts", { depth: 1 }),
    row("src", { kind: "folder" }),
    row("src/middleware.ts", { depth: 1 }),
  ],
  empty: null,
  open_file: "Open file",
  load_diff: "Load diff",
};

/** Each file's section is 500px tall below a 200px header; `scrolled` is how far the pane has scrolled. */
const SECTION = 500;
const HEADER = 200;
let scrolled = 0;
const topOf = (file: string) => HEADER + FILES.indexOf(file) * SECTION - scrolled;

let root: Root;
/** The frames asked for since the last one ran (the pane reports on the next frame). */
let frames: FrameRequestCallback[] = [];
let into: ReturnType<typeof vi.fn>;

beforeEach(() => {
  vi.useFakeTimers();
  scrolled = 0;
  frames = [];
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => frames.push(cb));
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  const rect = (top: number) => ({
    top,
    bottom: top + SECTION,
    left: 0,
    right: 0,
    width: 0,
    height: SECTION,
    x: 0,
    y: top,
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const label = this.getAttribute("aria-label");
    return (this.matches("section.files-change") && label !== null ? rect(topOf(label)) : rect(0)) as DOMRect;
  });
  into = vi.fn(function (this: HTMLElement) {
    // scrollIntoView({ block: "start" }) on a section: the pane scrolls it to the top, one scroll event per step.
    const label = this.getAttribute("aria-label") ?? "";
    const target = HEADER + FILES.indexOf(label) * SECTION;
    const from = scrolled;
    for (let i = 1; i <= 4; i++) scrollPane(from + ((target - from) * i) / 4);
  });
  HTMLElement.prototype.scrollIntoView = into as unknown as HTMLElement["scrollIntoView"];
  document.body.innerHTML = "";
  root = createRoot(document.body.appendChild(document.createElement("div")));
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function render(state: FilesState = NOTHING_OPEN) {
  act(() => {
    root.render(
      createElement(FilesTabBody, {
        ticket: "OTQ-8",
        view: VIEW,
        comments: undefined,
        changes: CHANGES,
        state,
        onState: () => undefined,
        reload: () => undefined,
      }),
    );
  });
}

function scrollPane(to: number) {
  scrolled = to;
  document.querySelector(".files-main")?.dispatchEvent(new Event("scroll"));
}

/** The user scrolls (after any clicked scroll has settled). */
function userScroll(to: number) {
  act(() => {
    vi.advanceTimersByTime(1000);
    scrollPane(to);
    for (const frame of frames.splice(0)) frame(0);
  });
}

const highlighted = () =>
  [...document.querySelectorAll(".files-list [aria-current]")].map((el) => el.textContent.trim());

function click(name: string) {
  const button = [...document.querySelectorAll<HTMLElement>(".files-list .files-row")].find((el) =>
    el.textContent.includes(name),
  );
  if (!button) throw new Error(name);
  act(() => {
    button.click();
  });
}

describe("the Changed tree's highlight", () => {
  it("follows the file at the top of All changes as the user scrolls", () => {
    render();
    expect(highlighted()).toEqual([]);
    // The second file's section reaches the top.
    userScroll(HEADER + SECTION + 40);
    expect(highlighted()).toEqual([expect.stringContaining("login.spec.ts")]);
    userScroll(HEADER + 2 * SECTION);
    expect(highlighted()).toEqual([expect.stringContaining("middleware.ts")]);
    // Back up to the header and the notes: nothing.
    userScroll(50);
    expect(highlighted()).toEqual([]);
  });

  it("goes to a clicked file at once and stays there while the click scrolls past the others", () => {
    render();
    const seen: string[][] = [];
    into.mockImplementation(function (this: HTMLElement) {
      const target = HEADER + FILES.indexOf(this.getAttribute("aria-label") ?? "") * SECTION;
      for (let i = 1; i <= 4; i++) {
        scrollPane((target * i) / 4);
        seen.push(highlighted());
      }
    });
    click("middleware.ts");
    expect(into).toHaveBeenCalledTimes(1);
    expect(highlighted()).toEqual([expect.stringContaining("middleware.ts")]);
    expect(seen.flat().some((name) => name.includes("README") || name.includes("login"))).toBe(false);
    // A last step of the clicked scroll, just after it, still doesn't move the highlight.
    act(() => {
      scrollPane(HEADER + 2 * SECTION - 300);
      for (const frame of frames.splice(0)) frame(0);
    });
    expect(highlighted()).toEqual([expect.stringContaining("middleware.ts")]);
  });

  it("scrolls back to a row clicked a second time after the user scrolled away", () => {
    render();
    click("login.spec.ts");
    expect(into).toHaveBeenCalledTimes(1);
    expect(scrolled).toBe(HEADER + SECTION);
    userScroll(0);
    expect(highlighted()).toEqual([]);
    click("login.spec.ts");
    expect(into).toHaveBeenCalledTimes(2);
    expect(scrolled).toBe(HEADER + SECTION);
    expect(highlighted()).toEqual([expect.stringContaining("login.spec.ts")]);
  });

  it("highlights nothing for a file in a folder the user closed", () => {
    render({ ...NOTHING_OPEN, shut: new Set(["e2e"]) });
    userScroll(HEADER + SECTION + 40);
    expect(highlighted()).toEqual([]);
    expect(document.querySelector(".files-list [aria-expanded=false]")?.textContent).toContain("e2e");
  });
});

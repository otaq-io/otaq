import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { createRegistries, registered } from "../../app/registry";
import type { AfterRestartRow } from "../../generated/AfterRestartRow";
import type { AfterRestartSummary } from "../../generated/AfterRestartSummary";
import type { CleanupConfirm } from "../../generated/CleanupConfirm";
import type { CleanupPage } from "../../generated/CleanupPage";
import type { CleanupWarmRow } from "../../generated/CleanupWarmRow";
import type { CleanupWarmSection } from "../../generated/CleanupWarmSection";
import type { Notices } from "../../generated/Notices";
import type { ScratchConfirm } from "../../generated/ScratchConfirm";
import { recovery } from ".";
import { SummaryBody } from "./AfterRestart";
import { CleanupBody, CleanupConfirmDialog, WarmConfirmDialog } from "./Cleanup";
import { setUnchecked, startFolded, summaryToOpen, toggleFold } from "./logic";
import { ScratchBody } from "./StartFromScratch";

const nothing = () => undefined;

const notices = (summary_for: number | null): Notices => ({
  banners: [],
  toasts: [],
  only_restart_allowed: false,
  terminal_note: null,
  service_down: false,
  input_enabled: true,
  summary_for,
  limit_lines: [],
});

function row(id: string, extra: Partial<AfterRestartRow> = {}): AfterRestartRow {
  return {
    id,
    kind: "ticket",
    tag: id,
    title: "Wishlist share link",
    state: "stopped",
    detail: "was working in Dev",
    problem: null,
    checked: true,
    ...extra,
  };
}

const SUMMARY: AfterRestartSummary = {
  title: "4 agents were stopped by a restart",
  text: "Each can continue its same conversation, in its own copy. Resuming uses tokens.",
  rows: [
    row("SHOP-27"),
    row("a1", { kind: "conversation", tag: "Noyan", title: "Checkout and footer", detail: "conversation" }),
    row("SHOP-19", { state: "resumed", detail: "Resumed", checked: false }),
    row("SHOP-30", {
      state: "failed",
      detail: "Couldn’t resume: Claude has no saved conversation",
      checked: false,
    }),
    row("BILL-4", { problem: "Codex isn’t signed in · sign in first", checked: false }),
  ],
  notes: ["SHOP-22 still waits for your answer; answering resumes it."],
  retry: "Retry",
  scratch: "Start from scratch",
  resuming: "Resuming…",
  foot: "They wait in Needs you if you close this.",
  later: "Not now",
  resume_selected: "Resume 2",
  selected: ["SHOP-27"],
  selected_agents: ["a1"],
};

const CLEANUP: CleanupPage = {
  title: "Cleanup",
  count: "3 items",
  text: "Copies, branches and processes no ticket uses any more. Nothing is selected until you choose.",
  groups: [
    {
      group: "worktrees",
      title: "Worktrees",
      icon: "folder-git-2",
      count: "1",
      all_checked: true,
      starts_folded: false,
      select_all: "select all",
      rows: [
        {
          target: { group: "worktrees", id: "/h/wt/shop-web/SHOP-14" },
          title: "~/.otaq/wt/shop-web/SHOP-14",
          detail: "no ticket owns it",
          size: "412 MB",
          age: "9 d",
          checked: true,
          warning: false,
        },
      ],
    },
    {
      group: "kept_worktrees",
      title: "Done tickets with uncommitted changes",
      icon: "triangle-alert",
      count: "3",
      all_checked: false,
      starts_folded: true,
      select_all: "select all",
      rows: [],
    },
  ],
  empty: null,
  footer: "1 selected · 412 MB",
  delete: "Delete selected…",
  confirm: {
    title: "Delete 1 item?",
    text: "This can’t be undone.",
    lines: [{ icon: "folder-git-2", text: "~/.otaq/wt/shop-web/SHOP-14 · 412 MB (remove worktree)" }],
    confirm: "Delete 1 item",
  },
  warm: null,
};

function warmRow(repo: string, size: string, extra: Partial<CleanupWarmRow> = {}): CleanupWarmRow {
  return {
    repo,
    name: repo,
    path: `~/.otaq/warm/${repo}`,
    built_at: "built at 3e1f9a2",
    clearing: null,
    size,
    age: "2 h",
    clear: "Clear…",
    clear_enabled: true,
    confirm: {
      title: `Clear ${repo}’s warm build (up to ${size})?`,
      text: `Deletes ${repo}’s warm build, built at 3e1f9a2, 2 hours ago.`,
      shared: "Tickets copied from it share part of this space, so it may free less.",
      line: { icon: "flame", text: `~/.otaq/warm/${repo} · up to ${size} (delete)` },
      confirm: `Clear up to ${size}`,
    },
    ...extra,
  };
}

const WARM: CleanupWarmSection = {
  title: "Warm builds",
  icon: "flame",
  count: "2",
  total: "5.5 GB on disk",
  note: "Kept to speed up new tickets. It grows over time; clear it to reclaim disk.",
  rows: [
    warmRow("shop-web", "4.2 GB", {
      clearing: "Clearing… (waiting for SHOP-21’s copy)",
      clear: "Clearing…",
      clear_enabled: false,
    }),
    warmRow("billing-api", "1.3 GB"),
  ],
  focus: null,
};

describe("the after-restart sheet", () => {
  it("opens by itself once per service start", () => {
    expect(summaryToOpen(undefined, null)).toBeNull();
    expect(summaryToOpen(notices(null), null)).toBeNull();
    expect(summaryToOpen(notices(42), null)).toBe(42);
    expect(summaryToOpen(notices(42), 42)).toBeNull();
    expect(summaryToOpen(notices(43), 42)).toBe(43);
  });

  it("draws the rows, what each Resume did and the notes", () => {
    const html = renderToStaticMarkup(
      createElement(SummaryBody, {
        view: SUMMARY,
        resuming: new Set(["SHOP-27"]),
        onCheck: nothing,
        onRetry: nothing,
      }),
    );
    for (const text of [
      "Each can continue its same conversation",
      "Resuming…",
      "Noyan",
      "Checkout and footer",
      "Resumed",
      "Couldn’t resume",
      "Retry",
      "Start from scratch",
      "SHOP-22 still waits for your answer",
      "They wait in Needs you if you close this.",
    ]) {
      expect(html).toContain(text);
    }
    expect(html).toContain('data-state="failed"');
  });

  it("leaves a row whose tool can't run unticked, with why, and its checkbox off", () => {
    const html = renderToStaticMarkup(
      createElement(SummaryBody, {
        view: SUMMARY,
        resuming: new Set<string>(),
        onCheck: nothing,
        onRetry: nothing,
      }),
    );
    const bill = html.slice(html.lastIndexOf("<input", html.indexOf("BILL-4")));
    expect(bill.slice(0, bill.indexOf(">"))).toContain("disabled");
    expect(html).toContain('data-problem="true">Codex isn’t signed in · sign in first');
  });

  it("ticks and unticks keys together", () => {
    const off = setUnchecked(new Set(), ["SHOP-30", "SHOP-31"], false);
    expect([...off]).toEqual(["SHOP-30", "SHOP-31"]);
    expect([...setUnchecked(off, ["SHOP-30"], true)]).toEqual(["SHOP-31"]);
  });
});

describe("cleanup", () => {
  it("draws groups, folded ones with select all, the footer and the confirmation", () => {
    const folded = startFolded(CLEANUP);
    expect([...folded]).toEqual(["kept_worktrees"]);
    expect([...toggleFold(folded, "kept_worktrees")]).toEqual([]);
    const html = renderToStaticMarkup(
      createElement(CleanupBody, {
        page: CLEANUP,
        folded,
        error: null,
        onFold: nothing,
        onToggle: nothing,
        onToggleGroup: nothing,
        onDelete: nothing,
        onClose: nothing,
        onClear: nothing,
      }),
    );
    for (const text of [
      "3 items",
      "~/.otaq/wt/shop-web/SHOP-14",
      "412 MB",
      "select all",
      "1 selected · 412 MB",
    ]) {
      expect(html).toContain(text);
    }
    expect(html).toContain("Delete selected…");
    const confirm = renderToStaticMarkup(
      createElement(CleanupConfirmDialog, {
        confirm: CLEANUP.confirm as CleanupConfirm,
        deleting: false,
        onConfirm: nothing,
        onCancel: nothing,
      }),
    );
    expect(confirm).toContain("(remove worktree)");
    expect(confirm).toContain("Delete 1 item");
  });

  it("says when there is nothing to clean up, without a Delete button", () => {
    const html = renderToStaticMarkup(
      createElement(CleanupBody, {
        page: {
          ...CLEANUP,
          count: "0 items",
          groups: [],
          footer: null,
          confirm: null,
          empty: { title: "Nothing to clean up", text: "Every session belongs to a ticket." },
        },
        folded: new Set<string>(),
        error: null,
        onFold: nothing,
        onToggle: nothing,
        onToggleGroup: nothing,
        onDelete: nothing,
        onClose: nothing,
        onClear: nothing,
      }),
    );
    expect(html).toContain("Nothing to clean up");
    expect(html).not.toContain("Delete selected…");
    expect(html).not.toContain("Warm builds");
  });

  it("draws the warm builds under the action bar, without checkboxes, and a waiting Clear", () => {
    const html = renderToStaticMarkup(
      createElement(CleanupBody, {
        page: { ...CLEANUP, warm: WARM },
        folded: new Set<string>(),
        error: null,
        onFold: nothing,
        onToggle: nothing,
        onToggleGroup: nothing,
        onDelete: nothing,
        onClose: nothing,
        onClear: nothing,
      }),
    );
    const warm = html.slice(html.indexOf("cleanup-warm"));
    expect(html.indexOf("Delete selected…")).toBeLessThan(html.indexOf("cleanup-warm"));
    for (const text of [
      "Warm builds",
      "5.5 GB on disk",
      "Kept to speed up new tickets.",
      "~/.otaq/warm/billing-api",
      "built at 3e1f9a2",
      "Clearing… (waiting for SHOP-21’s copy)",
      "Clear…",
    ]) {
      expect(warm).toContain(text);
    }
    expect(warm).not.toContain('type="checkbox"');
    expect(warm).toContain("disabled");
    // The count stays the leftovers'.
    expect(html).toContain("3 items");

    const confirm = renderToStaticMarkup(
      createElement(WarmConfirmDialog, {
        row: warmRow("billing-api", "1.3 GB"),
        clearing: false,
        onConfirm: nothing,
        onCancel: nothing,
      }),
    );
    for (const text of [
      "Clear billing-api’s warm build (up to 1.3 GB)?",
      "so it may free less.",
      "· up to 1.3 GB (delete)",
      "Clear up to 1.3 GB",
    ]) {
      expect(confirm).toContain(text);
    }
  });
});

describe("start from scratch", () => {
  it("says what is kept and what is lost", () => {
    const view: ScratchConfirm = {
      title: "Start BILL-4 Invoice PDF export from scratch?",
      text: "The previous conversation can’t be restored.",
      kept_title: "Kept",
      kept: ["Branch otaq/BILL-4 and its commits"],
      lost_title: "Lost",
      lost: "The previous conversation",
      note: "History records this.",
      confirm: "Start from scratch",
    };
    const html = renderToStaticMarkup(createElement(ScratchBody, { view, error: null }));
    expect(html).toContain("Kept");
    expect(html).toContain("Branch otaq/BILL-4 and its commits");
    expect(html).toContain("Lost");
  });
});

describe("the screen", () => {
  it("registers its three overlays", () => {
    const app = createRegistries();
    recovery(app);
    const ids = registered(app).overlays;
    expect(ids).toEqual(expect.arrayContaining(["after_restart", "start_from_scratch", "cleanup"]));
  });
});

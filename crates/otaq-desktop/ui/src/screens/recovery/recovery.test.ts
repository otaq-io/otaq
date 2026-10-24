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
});

function row(key: string, extra: Partial<AfterRestartRow> = {}): AfterRestartRow {
  return {
    key,
    project: "shop-web",
    title: "Upgrade to Next.js 15",
    state: "interrupted",
    headline: null,
    detail: "was Working · 311k",
    checked: true,
    resuming: `${key} · Resuming…`,
    resuming_detail: "Reattaching claude --resume in ~/.otaq/wt/shop-web/" + key,
    agent: null,
    problem: null,
    sign_in: null,
    ...extra,
  };
}

const SUMMARY: AfterRestartSummary = {
  title: "8 sessions were interrupted",
  text: "Only Dev tickets that were working become Interrupted.",
  rows: [
    row("SHOP-25"),
    row("SHOP-19", { state: "resumed", headline: "SHOP-19 · Resumed", detail: "Working again" }),
    row("BILL-4", {
      state: "failed",
      headline: "BILL-4 · Couldn’t resume",
      detail: "Claude has no saved conversation",
    }),
  ],
  more: {
    count: "+3",
    keys: "SHOP-30, SHOP-40.4, SHOP-40.2",
    detail: "was working · expand to resume one by one",
    checked: true,
    resumable: ["SHOP-30", "SHOP-40.4", "SHOP-40.2"],
  },
  agents: {
    title: "PROJECT AGENTS · NOT IN RESUME SELECTED · TERMINALS ARE GONE",
    rows: [
      {
        id: "a1",
        project: "shop-web",
        tag: "agent",
        name: "Checkout slowness",
        detail: "Ended by restart · same conversation",
        resumable: true,
        agent: null,
        problem: null,
        sign_in: null,
      },
    ],
    discarded: "A “New agent” that had no request yet is discarded, not listed.",
  },
  notes_title: "NOT IN RESUME SELECTED",
  notes: ["Turquoise tickets keep their question — SHOP-22."],
  resume: "Resume",
  retry: "Retry",
  scratch: "Start from scratch",
  foot: "They stay in Needs you if you close this.",
  later: "Not now",
  resume_selected: "Resume selected (4)",
  selected: ["SHOP-25", "SHOP-30", "SHOP-40.4", "SHOP-40.2"],
  skipped: [],
};

const CLEANUP: CleanupPage = {
  title: "Cleanup",
  count: "3 items",
  text: "Leftovers that no ticket owns. Review and select — nothing is deleted automatically.",
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

  it("draws rows, what each Resume did, the folded row and the notes", () => {
    const html = renderToStaticMarkup(
      createElement(SummaryBody, {
        view: SUMMARY,
        resuming: new Set(["SHOP-25"]),
        onCheck: nothing,
        onResume: nothing,
        onRetry: nothing,
        onUnfold: nothing,
        onLater: nothing,
        onResumeAgent: nothing,
      }),
    );
    for (const text of [
      "8 sessions were interrupted",
      "SHOP-25 · Resuming…",
      "SHOP-19 · Resumed",
      "BILL-4 · Couldn’t resume",
      "Start from scratch",
      "+3",
      "SHOP-30, SHOP-40.4, SHOP-40.2",
      "NOT IN RESUME SELECTED",
      "PROJECT AGENTS",
      "Checkout slowness",
      "Ended by restart · same conversation",
      "is discarded, not listed",
      "Resume selected (4)",
      "Not now",
    ]) {
      expect(html).toContain(text);
    }
    expect(html).toContain('data-state="failed"');
  });

  it("with several tools: the agent and tool, Sign in… in place of Resume, what Resume selected skips", () => {
    const view: AfterRestartSummary = {
      ...SUMMARY,
      rows: [
        row("SHOP-25", { agent: "Strong · Claude Code" }),
        row("SHOP-19", {
          agent: "Standard · Codex",
          problem: "Codex isn't available (signed out)",
          checked: false,
          sign_in: { label: "Sign in…", action: { kind: "sign_in", tool: "codex" }, primary: true },
        }),
      ],
      more: null,
      skipped: ["Resume selected skips the Codex session until you sign in"],
    };
    const html = renderToStaticMarkup(
      createElement(SummaryBody, {
        view,
        resuming: new Set<string>(),
        onCheck: nothing,
        onResume: nothing,
        onRetry: nothing,
        onUnfold: nothing,
        onLater: nothing,
        onResumeAgent: nothing,
      }),
    );
    expect(html).toContain("Strong · Claude Code");
    expect(html).toContain('data-problem="true">Standard · Codex · Codex isn&#x27;t available (signed out)');
    expect(html).toContain("Sign in…");
    expect(html).toContain("Resume selected skips the Codex session until you sign in");
    const signInRow = html.slice(html.indexOf("SHOP-19"));
    expect(signInRow.slice(0, signInRow.indexOf("Sign in…"))).not.toContain(">Resume<");
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
      note: "Activity records this.",
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

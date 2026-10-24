import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { createRegistries, registered } from "../../app/registry";
import type { ApproveView } from "../../generated/ApproveView";
import type { ReviewChanged } from "../../generated/ReviewChanged";
import { review } from ".";
import type { DecisionChangeView } from "../../generated/DecisionChangeView";
import { passOverlay } from "./CheckDialogs";
import { ChangeDecisionBody, ResultDialog } from "./Dialogs";
import { afterApproveAnyway, afterChoice, afterMerge, firstStep, isSubmitKey } from "./logic";

const CHANGED: ReviewChanged = {
  title: "SHOP-18 changed since you reviewed it · 1 new commit",
  commits: [{ sha: "4d9e2b1", subject: "fix(checkout): handle missing payment_intent" }],
  files: ["src/app/api/checkout/route.ts"],
  reviewed: "abc",
  note: "Only the new commits are shown. Approve anyway merges them too.",
  anyway: "Approve anyway",
  send_back: "Send back",
};

const VIEW: ApproveView = {
  key: "SHOP-18",
  menu: {
    key: "SHOP-18",
    title: "Approve SHOP-18 Checkout API",
    items: [
      {
        choice: "local_merge",
        number: 1,
        icon: "git-merge",
        title: "Local merge",
        detail: "Squash into main",
        enabled: true,
      },
    ],
    note: "Both open a confirmation step before anything runs.",
  },
  push_to_pr: false,
  confirm: {
    title: "Local merge SHOP-18 Checkout API",
    into: "Into (SHOP-18’s base branch)",
    target: "main",
    repo: "~/code/shop-web",
    locked: false,
    message_label: "Squash commit message",
    message: "SHOP-18: Checkout API",
    note: "The worktree for SHOP-18 is removed unless it has uncommitted changes.",
    conflict: null,
    merge_check: { text: "No merge check · ", link: "set one in project settings", project: "SHOP" },
    manual_checks: null,
    merge: "Squash & merge",
    cancel: "Cancel",
    hint: "cancel · ⌘↵ merge",
  },
  merging: {
    title: "Local merge SHOP-18 Checkout API",
    steps: ["Rebase onto main"],
    label: "Merging…",
    note: "",
    close: "Close",
  },
  changed: null,
};

describe("the review screen", () => {
  it("registers both dialogs and ⌘↵ / ⌘⌫", () => {
    const app = createRegistries();
    review(app);
    const r = registered(app);
    expect(r.overlays).toEqual(
      expect.arrayContaining(["local_merge", "send_back", "change_decision", "fail", "send_back_pr"]),
    );
    expect(r.shortcuts).toEqual(expect.arrayContaining(["approve", "send_back"]));
  });

  it("Pass opens today's delivery choices only when a click Delivery comes next (FLW-14)", () => {
    expect(passOverlay("SHOP-18", "moved", false)).toBeNull();
    expect(passOverlay("SHOP-18", "local_merge", false)).toEqual({
      id: "local_merge",
      params: { ticket: "SHOP-18" },
    });
    // Open recorded decisions: the choice first (DEC-05).
    expect(passOverlay("SHOP-18", "local_merge", true)).toEqual({
      id: "local_merge",
      params: { ticket: "SHOP-18", menu: true },
    });
    expect(passOverlay("SHOP-18", "open_pr", false)).toEqual({
      id: "open_pr",
      params: { ticket: "SHOP-18" },
    });
    // The default flow's Done (`either`): Local merge or Open PR, as Approve offered them.
    expect(passOverlay("NOTE-1", "either", false)).toEqual({
      id: "local_merge",
      params: { ticket: "NOTE-1", menu: true },
    });
  });

  it("draws the conflict and uncommitted results with their texts", () => {
    const conflict = renderToStaticMarkup(
      createElement(ResultDialog, {
        ticket: "SHOP-18",
        result: {
          kind: "conflict",
          title: "Rebase onto main stopped — conflict",
          text: "main moved since SHOP-18 was built.",
          files: ["src/lib/stripe/session.ts", "package.json"],
          help: "SHOP-18 went back to its agent",
          worktree: "~/.otaq/wt/shop-web/SHOP-18",
          close: "Close",
        },
        close: () => undefined,
        retry: () => undefined,
        toSendBack: () => undefined,
      }),
    );
    for (const text of [
      "Rebase onto main stopped — conflict",
      "package.json",
      "SHOP-18 went back to its agent",
      "Close",
    ]) {
      expect(conflict).toContain(text);
    }
    const uncommitted = renderToStaticMarkup(
      createElement(ResultDialog, {
        ticket: "SHOP-19",
        result: {
          kind: "uncommitted",
          title: "SHOP-19 has 2 uncommitted files",
          files: ["src/components/pricing/PriceTable.tsx"],
          text: "Send it back so the agent commits them, or look at them first.",
          view: "View",
          send_back: "Send back",
          cancel: "Cancel",
        },
        close: () => undefined,
        retry: () => undefined,
        toSendBack: () => undefined,
      }),
    );
    for (const text of ["SHOP-19 has 2 uncommitted files", "View", "Send back", "PriceTable.tsx"]) {
      expect(uncommitted).toContain(text);
    }
  });
});

describe("the approve flow", () => {
  it("starts at the confirmation from a button or a drop, at the choice from ⌘↵", () => {
    expect(firstStep(VIEW, false)).toEqual({ kind: "confirm" });
    expect(firstStep(VIEW, true)).toEqual({ kind: "menu" });
    expect(firstStep({ ...VIEW, push_to_pr: true }, false)).toEqual({ kind: "open_pr" });
  });

  it("shows new commits since the review first; Approve anyway goes on", () => {
    const first = firstStep({ ...VIEW, changed: CHANGED }, true);
    expect(first).toEqual({ kind: "changed", changed: CHANGED, checkedFirst: true });
    if (first.kind !== "changed") throw new Error("not changed");
    expect(afterApproveAnyway(first, VIEW, true)).toEqual({ kind: "menu" });
    expect(afterApproveAnyway(first, VIEW, false)).toEqual({ kind: "confirm" });
    expect(afterApproveAnyway(first, { ...VIEW, push_to_pr: true }, false)).toEqual({ kind: "open_pr" });
    expect(afterApproveAnyway({ ...first, checkedFirst: false }, VIEW, false)).toBe("merge");
  });

  it("hands Open PR and Push to PR to their dialog", () => {
    expect(afterChoice("local_merge")).toEqual({ kind: "confirm" });
    expect(afterChoice("open_pr")).toEqual({ kind: "open_pr" });
    expect(afterChoice("push_to_pr")).toEqual({ kind: "open_pr" });
  });

  it("closes after a merge and shows every other answer", () => {
    expect(afterMerge({ kind: "merged", toasts: [] })).toEqual({ kind: "done" });
    expect(afterMerge({ kind: "changed", changed: CHANGED })).toEqual({
      kind: "changed",
      changed: CHANGED,
      checkedFirst: false,
    });
    const refused = {
      kind: "refused",
      title: "Can’t merge into main yet",
      message: "m",
      retry: "Retry",
      close: "Close",
    } as const;
    expect(afterMerge(refused)).toEqual({ kind: "result", result: refused });
    const stopped = {
      kind: "stopped",
      title: "Merge check stopped",
      message: "The merge check was stopped; nothing was merged into main.",
      close: "Close",
    } as const;
    expect(afterMerge(stopped)).toEqual({ kind: "result", result: stopped });
  });

  it("merges with ⌘↵ or Ctrl+↵ in the message", () => {
    expect(isSubmitKey({ key: "Enter", metaKey: true, ctrlKey: false })).toBe(true);
    expect(isSubmitKey({ key: "Enter", metaKey: false, ctrlKey: true })).toBe(true);
    expect(isSubmitKey({ key: "Enter", metaKey: false, ctrlKey: false })).toBe(false);
  });
});

// Hand-written views, as `decision_change.rs` words them (*07 → Change… on SHOP-23*, *Add a fix child*).
const SEND_BACK: DecisionChangeView = {
  kind: "send_back",
  id: "SHOP-23#1",
  key: "SHOP-23",
  title: "Change this decision",
  question: "Split fullName everywhere, or only in the profile form?",
  summary: "Built: only in the profile form · Other option: everywhere",
  label: "SEND BACK TO THE AGENT WITH",
  text: "everywhere",
  help: "Prefilled with the other option. Edit it or write your own.",
  note: "This is a Send back: SHOP-23 goes back to Dev with this message and any line comments you’ve written, and its agent continues in the same session. Its next submit is validated again.",
  comments: 0,
  confirm: "Send back to the agent",
  cancel: "Cancel",
  hint: "cancel",
};

const FIX_CHILD: DecisionChangeView = {
  kind: "fix_child",
  id: "SHOP-40.1#1",
  epic: "SHOP-40",
  title: "Add a fix child",
  question: "Order of saved items?",
  summary: "Built: newest first · Other option: in the order they were added",
  mark: {
    text: "Lead: contradicts PAY-123: “items are listed in the order they were added”.",
    source: "PAY-123",
  },
  title_label: "NEW CHILD OF SHOP-40 · TITLE",
  child_title: "Change: in the order they were added",
  child_description:
    "SHOP-40.1 built newest first for “Order of saved items?” (recorded decision). Change it to: in the order they were added.",
  description_label: "Description",
  help: "Prefilled from the decision; edit both before adding.",
  note: "Creates SHOP-40.7, added by you. The epic becomes a track again and 40.7 joins the queue; its lead resumes the same conversation.",
  confirm: "Add fix child",
  cancel: "Cancel",
  hint: "cancel",
};

const changeHtml = (view: DecisionChangeView) =>
  renderToStaticMarkup(createElement(ChangeDecisionBody, { view, close: () => undefined }));

describe("Change… on a recorded decision (DEC-06)", () => {
  it("on a ticket: Change this decision, prefilled with the other option, sends it back", () => {
    const out = changeHtml(SEND_BACK);
    for (const text of [
      "Change this decision",
      "Split fullName everywhere, or only in the profile form?",
      "Built: only in the profile form · Other option: everywhere",
      "SEND BACK TO THE AGENT WITH",
      'value="everywhere"',
      "Prefilled with the other option.",
      "This is a Send back:",
      "Send back to the agent",
    ]) {
      expect(out).toContain(text);
    }
    expect(out).not.toContain("decision-mark");
  });

  it("in an epic's review: Add a fix child with the lead's mark, title and description editable", () => {
    const out = changeHtml(FIX_CHILD);
    for (const text of [
      "Add a fix child",
      "Order of saved items?",
      'class="decision-mark"',
      "Lead: contradicts PAY-123",
      "NEW CHILD OF SHOP-40 · TITLE",
      'value="Change: in the order they were added"',
      "SHOP-40.1 built newest first",
      "Creates SHOP-40.7, added by you.",
      "Add fix child",
    ]) {
      expect(out).toContain(text);
    }
    expect(out).toContain("<textarea");
  });

  it("without a mark, no violet row", () => {
    expect(changeHtml({ ...FIX_CHILD, mark: null })).not.toContain("decision-mark");
  });
});

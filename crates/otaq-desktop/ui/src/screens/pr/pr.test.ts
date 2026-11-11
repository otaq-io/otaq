import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { createRegistries, registered } from "../../app/registry";
import type { PrDialogView } from "../../generated/PrDialogView";
import { pr } from ".";
import { Intro, OpenBox, Proposed, SkillWaits, Waiting } from "./Dialogs";
import { checkDue, fieldsOf, isStale, polls, proposalArrived } from "./logic";

const VIEW: PrDialogView = {
  key: "SHOP-18",
  push: false,
  heading: "Open PR for SHOP-18 Checkout API",
  stage: { kind: "proposed" },
  proposed_by: "Proposed by /otaq:open-pr · edit anything",
  provider: "GitHub · acme/shop-web",
  login: "gh: signed in",
  branch_label: "Branch name",
  branch: "feature/SHOP-18-checkout-api",
  branch_locked: false,
  branch_hint: { text: "Valid name · not taken on origin · was otaq/SHOP-18", error: false },
  check: 1,
  check_after_ms: 300,
  commits_label: "Commit messages (2 commits)",
  commits: [
    { sha: "a1b2c3d4", message: "feat(checkout): add POST /checkout", refused: false },
    { sha: "e5f6a7b8", message: "test(checkout): cover declined cards", refused: false },
  ],
  title_label: "PR title",
  title: "Checkout API (SHOP-18)",
  description_label: "Description",
  description: "Adds POST /api/checkout.",
  target_line: "Into main · SHOP-18’s base branch",
  confirm: "Push & open PR",
  can_confirm: true,
  cancel: "Cancel",
  hint: "cancel",
  prepared: null,
  writing: null,
  asks: null,
  waits: null,
  set_up: null,
  merge_instead: null,
  filled: 1,
  use_proposed: null,
};

const PREPARING: PrDialogView = {
  ...VIEW,
  stage: {
    kind: "preparing",
    line: "Open PR skill is preparing…",
    detail: "/otaq:open-pr · reading the commits",
  },
  commits_label: "Commit messages",
  commits: [],
  can_confirm: false,
};

describe("the PR screen", () => {
  it("registers the open_pr overlay and the PR box under the tabs", () => {
    const app = createRegistries();
    pr(app);
    const r = registered(app);
    expect(r.overlays).toEqual(["open_pr"]);
    expect(r.sheetSections).toEqual(["pr_open"]);
  });

  it("draws the confirmation like *Open PR · confirmation*", () => {
    const html = renderToStaticMarkup(
      createElement(Proposed, {
        view: VIEW,
        fields: fieldsOf(VIEW),
        onEdit: () => undefined,
        onUseProposed: () => undefined,
      }),
    );
    for (const text of [
      "Proposed by /otaq:open-pr · edit anything",
      "feature/SHOP-18-checkout-api",
      "Valid name · not taken on origin",
      "PR title",
      "Description",
      "Into main · SHOP-18’s base branch",
    ]) {
      expect(html).toContain(text);
    }
    // Open PR rewords the commits it pushes, as Push to PR does: each message has its field.
    expect(html).toContain("Commit messages (2 commits)");
    expect(html).toContain("feat(checkout): add POST /checkout");
    expect(html).toContain("test(checkout): cover declined cards");
    expect(html).not.toContain("aria-invalid");
    expect(html).not.toContain("Use the proposed message");
  });

  it("offers Use the proposed message while the fields hold the user's kept edit", () => {
    const edited: PrDialogView = {
      ...VIEW,
      title: "feat: checkout API",
      use_proposed: "Use the proposed message",
    };
    const html = renderToStaticMarkup(
      createElement(Proposed, {
        view: edited,
        fields: fieldsOf(edited),
        onEdit: () => undefined,
        onUseProposed: () => undefined,
      }),
    );
    expect(html).toContain("feat: checkout API");
    expect(html).toContain("Use the proposed message");
  });

  it("says who prepared the text, who still writes it, and when the provider isn't set up", () => {
    const writing: PrDialogView = {
      ...VIEW,
      writing: "Strong is still writing the title and description.",
      set_up: {
        title: "GitHub isn’t set up on this Mac",
        text: "shop-web delivers pull requests through GitHub.",
        button: "Set up GitHub…",
      },
      can_confirm: false,
    };
    const html = renderToStaticMarkup(
      createElement(Proposed, {
        view: writing,
        fields: fieldsOf(writing),
        onEdit: () => undefined,
        onUseProposed: () => undefined,
      }),
    );
    for (const text of [
      "GitHub isn’t set up on this Mac",
      "Set up GitHub…",
      "Strong is still writing the title and description.",
    ]) {
      expect(html).toContain(text);
    }
    const prepared = renderToStaticMarkup(
      createElement(Proposed, {
        view: { ...VIEW, prepared: "Prepared by Strong while SHOP-18 waited in Review." },
        fields: fieldsOf(VIEW),
        onEdit: () => undefined,
        onUseProposed: () => undefined,
      }),
    );
    expect(prepared).toContain("Prepared by Strong while SHOP-18 waited in Review.");
    expect(polls(writing)).toBe(true);
    // The agent's message filled the fields again: the page takes them.
    expect(proposalArrived(VIEW, { ...VIEW, filled: 2 })).toBe(true);
  });

  it("says when the writer waits for permission, with Answer in terminal beside the plain text", () => {
    const asks: PrDialogView = {
      ...VIEW,
      writing: "Developer wants to run a command to write the title and description.",
      asks: {
        session: "s-writer",
        answer: "Answer in terminal",
        terminal_title: "Developer · SHOP-18’s message",
      },
    };
    const html = renderToStaticMarkup(
      createElement(Proposed, {
        view: asks,
        fields: fieldsOf(asks),
        onEdit: () => undefined,
        onUseProposed: () => undefined,
      }),
    );
    expect(html).toContain("Developer wants to run a command to write the title and description.");
    expect(html).toContain("Answer in terminal");
    const still = renderToStaticMarkup(
      createElement(Proposed, {
        view: { ...asks, writing: "Developer is still writing the title and description.", asks: null },
        fields: fieldsOf(asks),
        onEdit: () => undefined,
        onUseProposed: () => undefined,
      }),
    );
    expect(still).not.toContain("Answer in terminal");
  });

  it("says the skill's agent waits for permission, with its command and Answer in terminal", () => {
    const html = renderToStaticMarkup(
      createElement(SkillWaits, {
        waits: {
          line: "The agent is waiting for your permission to run a command",
          text: "git push origin otaq/SHOP-18",
          command: true,
          asks: { session: "s-push", answer: "Answer in terminal", terminal_title: "SHOP-18 · Push to PR" },
        },
      }),
    );
    expect(html).toContain("The agent is waiting for your permission to run a command");
    expect(html).toContain("$ git push origin otaq/SHOP-18");
    expect(html).toContain("Answer in terminal");
  });

  it("locks the branch and drops title and description on Push to PR", () => {
    const push: PrDialogView = {
      ...VIEW,
      push: true,
      branch_locked: true,
      branch: "feature/SHOP-26-add-sitemap",
      branch_hint: { text: "Locked: set at the first push (Open PR)", error: false },
    };
    const html = renderToStaticMarkup(
      createElement(Proposed, {
        view: push,
        fields: fieldsOf(push),
        onEdit: () => undefined,
        onUseProposed: () => undefined,
      }),
    );
    expect(html).toContain("pr-locked");
    expect(html).toContain("Locked: set at the first push (Open PR)");
    expect(html).not.toContain("PR title");
    const bad = renderToStaticMarkup(
      createElement(Proposed, {
        view: { ...VIEW, branch_hint: { text: "Use hyphens instead of spaces", error: true } },
        fields: fieldsOf(VIEW),
        onEdit: () => undefined,
        onUseProposed: () => undefined,
      }),
    );
    expect(bad).toContain('data-error="true"');
  });

  it("greys the fields while the skill prepares, and shows the Push intro", () => {
    const waiting = renderToStaticMarkup(createElement(Waiting, { view: PREPARING }));
    expect(waiting.match(/pr-placeholder/g)).toHaveLength(3);
    const intro = renderToStaticMarkup(
      createElement(Intro, {
        intro: {
          title: "Push SHOP-26 Add sitemap to PR #481",
          pr: "PR #481 · Add sitemap · GitHub",
          branch: "feature/SHOP-26-add-sitemap",
          commits: "2 new commits since the PR was opened",
          note: "Pushes the branch to origin.",
          confirm: "Push to PR #481",
          cancel: "Cancel",
        },
        busy: false,
        onPush: () => undefined,
        onCancel: () => undefined,
      }),
    );
    expect(intro).toContain("2 new commits since the PR was opened");
    expect(intro).toContain("Push to PR #481");
  });

  it("draws the PR box of an open PR", () => {
    const html = renderToStaticMarkup(
      createElement(OpenBox, {
        view: {
          label: "acme/shop-web #481",
          url: "https://github.com/acme/shop-web/pull/481",
          opened: "opened 2 h ago",
          branch: "feature/SHOP-26-add-sitemap → main",
          note: "Otaq doesn’t watch this PR.",
          mark_merged: "Mark merged",
          mark_closed: "Mark closed",
          close_title: "Mark PR #481 as closed?",
          close_body: "The ticket moves to Done as Closed · not merged.",
        },
        onOpen: () => undefined,
        onMerged: () => undefined,
        onClosed: () => undefined,
      }),
    );
    for (const text of [
      "acme/shop-web #481",
      "opened 2 h ago",
      "feature/SHOP-26-add-sitemap → main",
      "watch this PR",
    ]) {
      expect(html).toContain(text);
    }
  });
});

describe("the PR dialog's interaction rules", () => {
  it("fills the fields once, polls while the skill works, and checks the newest name", () => {
    expect(proposalArrived(PREPARING, VIEW)).toBe(true);
    expect(proposalArrived(VIEW, VIEW)).toBe(false);
    expect(proposalArrived(null, VIEW)).toBe(true);
    expect(polls(PREPARING)).toBe(true);
    expect(polls(VIEW)).toBe(false);
    expect(polls({ ...VIEW, stage: { kind: "pushing", title: "Pushing SHOP-18…", steps: [] } })).toBe(true);
    expect(checkDue(VIEW, 0)).toBe(true);
    expect(checkDue(VIEW, 1)).toBe(false);
    expect(checkDue({ ...VIEW, branch_locked: true }, 0)).toBe(false);
    expect(checkDue(PREPARING, 0)).toBe(false);
    expect(isStale({ outcome: "dialog", view: VIEW, intro: null }, 2)).toBe(true);
    expect(isStale({ outcome: "dialog", view: VIEW, intro: null }, 1)).toBe(false);
    expect(isStale({ outcome: "closed" }, 9)).toBe(false);
    expect(fieldsOf(VIEW).commits).toEqual([
      "feat(checkout): add POST /checkout",
      "test(checkout): cover declined cards",
    ]);
  });
});

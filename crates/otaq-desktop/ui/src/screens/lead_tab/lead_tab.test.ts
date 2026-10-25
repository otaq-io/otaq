import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { createRegistries, registered, registerScreens, RegistriesContext } from "../../app/registry";
import type { LeadChangeGroup } from "../../generated/LeadChangeGroup";
import type { LeadChanges } from "../../generated/LeadChanges";
import type { LeadStatusView } from "../../generated/LeadStatusView";
import type { LeadTabView } from "../../generated/LeadTabView";
import { leadTab } from ".";
import { LeadTabBody } from "./LeadTab";

/** Hand-written as Rust words them (`lead_tab.rs`, `lead_status.rs`). */
const status = (over: Partial<LeadStatusView>): LeadStatusView => ({
  epic: "SHOP-40",
  name: "Wishlist lead",
  state: "idle",
  short: "idle",
  full: "Wishlist lead · idle",
  tone: "idle",
  icon: "bot",
  action: null,
  tooltip: { title: "Wishlist lead · idle", lines: ["212k today"] },
  opens_tab: true,
  ...over,
});

const GROUP: LeadChangeGroup = {
  label: "after SHOP-40.1 merged · 10:52",
  lines: [
    {
      icon: "pencil",
      added: false,
      verb: "Edited",
      key: "SHOP-40.3",
      text: "description",
      open_hint: "Open SHOP-40.3",
    },
    {
      icon: "link-2",
      added: false,
      verb: null,
      key: "SHOP-40.3",
      text: "waits for 40.4",
      open_hint: "Open SHOP-40.3",
    },
    {
      icon: "plus",
      added: true,
      verb: "Added",
      key: "SHOP-40.5",
      text: "Wishlist empty state",
      open_hint: "Open SHOP-40.5",
    },
  ],
};

const CHANGES: LeadChanges = { title: "Changes by the lead", empty: "No changes yet", groups: [GROUP] };

const IDLE: LeadTabView = {
  epic: "SHOP-40",
  status: status({}),
  read_only: "read-only",
  tokens: "212k today · on the epic",
  changes: CHANGES,
  body: { kind: "live", session: "lead-session", label: "Wishlist lead · terminal" },
  asks_you: false,
  waiting: null,
};

const html = (view: LeadTabView) =>
  renderToStaticMarkup(
    createElement(
      RegistriesContext,
      { value: createRegistries() },
      createElement(LeadTabBody, { view, onAction: () => Promise.resolve() }),
    ),
  );

describe("the Lead tab renders each state (T-0290)", () => {
  it("registers the Lead tab", () => {
    expect(registered(registerScreens({ leadTab })).sheetTabs).toEqual(["lead"]);
  });

  it("idle: the header, the changes by turn with their links, the live terminal", () => {
    const out = html(IDLE);
    expect(out).toContain("Wishlist lead");
    expect(out).toContain("read-only");
    expect(out).toContain("212k today · on the epic");
    expect(out).toContain("Changes by the lead");
    expect(out).toContain("after SHOP-40.1 merged · 10:52");
    expect(out).toContain('aria-label="Open SHOP-40.3"');
    expect(out).toContain('<span class="lead-tab-change-verb">Edited</span>');
    expect(out).toContain("waits for 40.4");
    expect(out).toContain('data-added="true"');
    expect(out).toContain("terminal-tab-terminal");
  });

  it("no lead: Open lead, without tokens or changes", () => {
    const out = html({
      ...IDLE,
      status: status({
        name: null,
        full: "Open lead",
        tone: "none",
        action: { kind: "open", label: "Open lead", icon: "bot", split: null },
      }),
      tokens: null,
      changes: null,
      body: {
        kind: "open_lead",
        title: "SHOP-40 has no lead",
        text: "Open lead starts a read-only agent…",
        button: { kind: "open", label: "Open lead", icon: "bot", split: null },
      },
    });
    expect(out).toContain("SHOP-40 has no lead");
    expect(out).toContain("Open lead");
    expect(out).not.toContain("lead-tab-muted");
    expect(out).not.toContain("Changes by the lead");
  });

  it("no lead, no children: Split with Noyan, no text (T-0377)", () => {
    const split = { project: "SHOP", message: "Split SHOP-40 into tickets" };
    const button = { kind: "split", label: "Split with Noyan", icon: "bot", split } as const;
    const out = html({
      ...IDLE,
      status: status({
        name: null,
        full: "Split with Noyan",
        tone: "none",
        action: button,
      }),
      tokens: null,
      changes: null,
      body: { kind: "open_lead", title: "SHOP-40 has no lead", text: null, button },
    });
    expect(out).toContain("SHOP-40 has no lead");
    expect(out).toContain("<span>Split with Noyan</span>");
    expect(out).not.toContain("Open lead");
    expect(out).not.toContain("<p>");
  });

  it("ended by restart: Before the restart with Resume", () => {
    const out = html({
      ...IDLE,
      status: status({
        state: "ended by restart",
        tone: "ended",
        action: { kind: "resume", label: "Resume", icon: "rotate-ccw", split: null },
      }),
      body: {
        kind: "before_restart",
        title: "Before the restart",
        source: "from Claude’s saved conversation · read-only",
        session: "session 7c21a9e4",
        lines: [{ time: "10:53", who: "otaq", text: "update SHOP-40.3", action: true }],
        ended: "The lead ended with a restart at 07:58.",
        resume: "Resume",
      },
    });
    expect(out).toContain("Before the restart");
    expect(out).toContain("session 7c21a9e4");
    expect(out).toContain("The lead ended with a restart at 07:58.");
    expect(out).toContain('data-action="true"');
    expect(out).toContain("Resume");
  });

  it("ended in review: the whole epic, the read-only terminal and the ended bar, no Resume", () => {
    const out = html({
      ...IDLE,
      status: status({ state: "ended · epic in review", tone: "ended" }),
      changes: { ...CHANGES, groups: [{ ...GROUP, label: "whole epic · 4 changes" }] },
      body: {
        kind: "ended",
        session: "lead-session",
        label: "Wishlist lead · terminal",
        title: "Ended · epic in review",
        text: "Reopening a child or adding one resumes this conversation.",
        icon: "eye",
        resume: null,
      },
    });
    expect(out).toContain("whole epic · 4 changes");
    expect(out).toContain("Ended · epic in review");
    expect(out).toContain("Reopening a child or adding one resumes this conversation.");
    expect(out).not.toContain(">Resume<");
  });

  it("asks you: marked for the turquoise bar; loading and an empty list say so", () => {
    expect(html({ ...IDLE, asks_you: true })).toContain('data-asks="true"');
    const out = html({
      ...IDLE,
      changes: { ...CHANGES, groups: [] },
      body: { kind: "loading", text: "Reading Claude’s saved conversation…" },
    });
    expect(out).toContain("No changes yet");
    expect(out).toContain("Reading Claude’s saved conversation…");
  });
});

describe("a lead that waits on its tool (M23)", () => {
  it("says so above the body, amber, with Sign in…", () => {
    const out = html({
      ...IDLE,
      waiting: {
        role: "lead",
        tone: "amber",
        avatar: null,
        heading:
          "Claude Code isn't available · signed out. The lead waits; children on Claude Code wait too.",
        text: null,
        model: null,
        buttons: [{ label: "Sign in…", action: { kind: "sign_in", tool: "claude" }, primary: true }],
      },
    });
    expect(out).toContain('class="lead-tab-waiting"');
    expect(out).toContain('data-tone="amber"');
    expect(out).toContain("The lead waits; children on Claude Code wait too.");
    expect(out).toContain("Sign in…");
    expect(out.indexOf("lead-tab-waiting")).toBeLessThan(out.indexOf("terminal-tab-terminal"));
  });

  it("a limit is neutral; no notice without a wait", () => {
    const out = html({
      ...IDLE,
      waiting: {
        role: "lead",
        tone: "neutral",
        avatar: null,
        heading: "Claude Code limit reached · resets 3:45 pm. The lead continues after the reset.",
        text: null,
        model: null,
        buttons: [],
      },
    });
    expect(out).toContain('data-tone="neutral"');
    expect(html(IDLE)).not.toContain("lead-tab-waiting");
  });
});

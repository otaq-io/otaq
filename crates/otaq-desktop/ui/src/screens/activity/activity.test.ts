// The sheet's History: its rows newest first under All · Questions · Checks · Sessions, an agent session as a quiet row.
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { registered, registerScreens } from "../../app/registry";
import type { ActivityFilterView } from "../../generated/ActivityFilterView";
import type { HistoryRowView } from "../../generated/HistoryRowView";
import type { HistoryView } from "../../generated/HistoryView";
import { activity } from ".";
import { HistoryBody, shownRows } from "./History";

const html = (el: Parameters<typeof renderToStaticMarkup>[0]) => renderToStaticMarkup(el);

/** `sections::history_of`'s filters. */
const FILTERS: ActivityFilterView[] = [
  { filter: "all", label: "All", kind: null },
  { filter: "questions", label: "Questions", kind: "question" },
  { filter: "checks", label: "Checks", kind: "check" },
  { filter: "sessions", label: "Sessions", kind: "session" },
];

/** A row as `sections::history_of` gives it, an event's by default. */
const base = (over: Partial<HistoryRowView>): HistoryRowView => ({
  title: "",
  detail: null,
  when: "2:14 pm",
  who: "2:14 pm",
  cost: null,
  tool_session_id: null,
  quiet: false,
  short_id: null,
  range: null,
  mark: { kind: "agent" },
  opens: null,
  hook_changes: null,
  kind: "other",
  ...over,
});

describe("Activity", () => {
  it("reads as plain sentences over who and when; an agent session is a quiet row with its id, copyable", () => {
    const view: HistoryView = {
      filters: FILTERS,
      order: "newest first",
      copy_hint: "Copy session ID",
      copied: "Copied",
      empty: "Nothing has happened to this ticket yet.",
      total: null,
      key: "SHOP-28",
      rows: [
        base({
          title: "Interrupted by restart",
          detail: "conversation saved",
          when: "10:21 am",
          who: "Otaq · 10:21 am",
          mark: { kind: "otaq" },
          kind: "session",
        }),
        base({
          title: "Validate passed · round 1",
          when: "10:20 am",
          who: "10:20 am",
          cost: "· 38 min · 74k tokens",
          kind: "check",
        }),
        base({
          title: "Session · Dev · Rust developer",
          detail: "Codex · gpt-5.5",
          when: "9:42 am",
          tool_session_id: "9f1c2d3e-77aa",
          quiet: true,
          short_id: "9f1c2d3e-…",
          range: "9:42 am–10:20 am",
          cost: "· 74k tokens",
          kind: "session",
        }),
        base({
          title: "You created the ticket",
          when: "9:10 am",
          who: "you · 9:10 am",
          mark: { kind: "you" },
        }),
      ],
    };
    const out = html(createElement(HistoryBody, { view }));
    expect(out).toMatch(
      /Interrupted by restart.*Otaq · 10:21 am.*Session · Dev · Rust developer.*You created the ticket/,
    );
    expect(out).toContain(
      'Validate passed · round 1<span class="history-cost"> · 38 min · 74k tokens</span>',
    );
    expect(out).toContain('class="history-session-id">9f1c2d3e-…</span>');
    expect(out).toContain("9:42 am–10:20 am · 74k tokens");
    expect(out).toContain('title="Codex · gpt-5.5"');
    expect(out).toContain("you · 9:10 am");
    expect(out.match(/history-session"/g)).toHaveLength(1);
    expect(out).toContain("otaq-mark");
    expect(out).not.toContain("$");
    expect(out.match(/Copy session ID/g)).toHaveLength(1);
    expect(html(createElement(HistoryBody, { view: { ...view, rows: [] } }))).toContain(view.empty);
  });

  it(" validation rounds open nothing any more; the total sits under the rows", () => {
    const view: HistoryView = {
      filters: FILTERS,
      order: "newest first",
      copy_hint: "Copy session ID",
      copied: "Copied",
      empty: "Nothing has happened to this ticket yet.",
      total: { label: "Validation total · 2 entries", tokens: "15k" },
      key: "SHOP-32",
      rows: [
        base({
          title: "Round 1 · failed",
          detail: "agent submitted",
          when: "10:02",
          who: "10:02",
          cost: "· 4 min · 12k tokens",
          kind: "check",
        }),
      ],
    };
    const out = html(createElement(HistoryBody, { view }));
    expect(out).toContain("Round 1 · failed");
    expect(out).not.toContain("aria-expanded");
    expect(out).toContain("Validation total · 2 entries");
    expect(out).toContain("15k");
  });

  it(" description and comment lines open their before / after and a report (s05-45, CMT-14)", () => {
    const row = (over: Partial<HistoryRowView>): HistoryRowView => base({ kind: "comment", ...over });
    const view: HistoryView = {
      filters: FILTERS,
      order: "newest first",
      copy_hint: "Copy session ID",
      copied: "Copied",
      empty: "Nothing has happened to this ticket yet.",
      total: null,
      key: "SHOP-34",
      rows: [
        row({
          title: "Spec writer · Product spec edited the description · added Agreed (4 lines)",
          kind: "description",
          opens: {
            label: "DESCRIPTION",
            caption: "Before / after",
            lines: [
              { kind: "same", sign: "", text: "Admins can export the orders table as CSV." },
              { kind: "added", sign: "+", text: "## Agreed" },
            ],
            thumbnails: [],
            attachments: [],
          },
        }),
        row({
          title: "Validator · Validate · round 2 commented · with a report (48 KB)",
          detail: "“Fail: the 10,000 row limit isn't…”",
          opens: {
            label: "COMMENT",
            caption: null,
            lines: [{ kind: "same", sign: "", text: "Fail: the 10,000 row limit isn't enforced." }],
            thumbnails: [
              {
                n: 4,
                thumbnail: {
                  attachment: 2,
                  name: "shot.png",
                  size: "120 KB",
                  open: "Open shot.png full size",
                  close: "Close",
                },
                change: null,
              },
            ],
            attachments: [
              {
                n: 4,
                chip: { attachment: 1, name: "test-run.txt", kind: "text", size: "· 48 KB", open: "Open" },
                change: null,
              },
            ],
          },
        }),
        row({
          title: "You edited your comment",
          detail: "Removed before.png · added after.png",
          mark: { kind: "you" },
          opens: {
            label: "COMMENT",
            caption: "Before / after",
            lines: [{ kind: "same", sign: "", text: "The badge." }],
            thumbnails: [
              {
                n: 5,
                thumbnail: {
                  attachment: 1,
                  name: "before.png",
                  size: "1 KB",
                  open: "Open before.png full size",
                  close: "Close",
                },
                change: "Removed",
              },
              {
                n: 5,
                thumbnail: {
                  attachment: 2,
                  name: "after.png",
                  size: "1 KB",
                  open: "Open after.png full size",
                  close: "Close",
                },
                change: "Added",
              },
            ],
            attachments: [],
          },
        }),
        row({ title: "You deleted a comment", mark: { kind: "you" } }),
      ],
    };
    const closed = html(createElement(HistoryBody, { view, opened: [] }));
    expect(closed.match(/aria-expanded="false"/g)).toHaveLength(3);
    expect(closed).toContain("comment-mark");
    expect(closed).not.toContain("DESCRIPTION");
    const opened = html(createElement(HistoryBody, { view, opened: [0, 1] }));
    expect(opened).toContain("DESCRIPTION");
    expect(opened).toContain("Before / after");
    expect(opened).toContain('data-kind="added"');
    expect(opened).toContain("## Agreed");
    expect(opened).toContain("· 48 KB");
    expect(opened).toContain(">Open</button>");
    // tuq 004: the image is a thumbnail, loaded by its numbers.
    expect(opened).toContain('aria-label="Open shot.png full size"');
    expect(opened).toContain('src="otaq-attachment://localhost/SHOP-34%2F4%2F2"');
    expect(opened).not.toContain("history-change");
    // tuq 008: an edit's removed image still opens, marked as removed; the added one is marked too.
    const edit = html(createElement(HistoryBody, { view, opened: [2] }));
    expect(edit).toContain('data-change="removed"');
    expect(edit).toContain('src="otaq-attachment://localhost/SHOP-34%2F5%2F1"');
    expect(edit).toContain(">Removed</span>");
    expect(edit).toContain(">Added</span>");
  });

  it("filters under All · Questions · Checks · Sessions, newest first; description edits open", () => {
    const row = (
      title: string,
      kind: HistoryRowView["kind"],
      opens: HistoryRowView["opens"] = null,
    ): HistoryRowView => base({ title, kind, opens, mark: { kind: "you" } });
    const view: HistoryView = {
      filters: FILTERS,
      order: "newest first",
      copy_hint: "Copy session ID",
      copied: "Copied",
      empty: "Nothing has happened to this ticket yet.",
      total: null,
      key: "SHOP-22",
      rows: [
        row("Created", "other"),
        row("Spec writer · Product spec edited the description · added 2 lines", "description", {
          label: "DESCRIPTION",
          caption: "Before / after",
          lines: [{ kind: "added", sign: "+", text: "· the redirect waits for the session cookie" }],
          thumbnails: [],
          attachments: [],
        }),
        row("Question answered", "question"),
        row("Validate passed", "check"),
        base({ title: "Session · Dev · Strong", kind: "session", quiet: true }),
        row("Moved to Dev", "stage"),
      ],
    };
    expect(shownRows(view, "all")).toEqual([0, 1, 2, 3, 4, 5]);
    expect(shownRows(view, "questions")).toEqual([2]);
    expect(shownRows(view, "checks")).toEqual([3]);
    expect(shownRows(view, "sessions")).toEqual([4]);
    const all = html(createElement(HistoryBody, { view }));
    for (const text of ["All", "Questions", "Checks", "Sessions", "newest first"])
      expect(all).toContain(text);
    expect(all).toContain('aria-checked="true">All');
    // The description edit's before / after shows without a click.
    expect(all).toContain("the redirect waits for the session cookie");
    const sessions = html(createElement(HistoryBody, { view, filter: "sessions" }));
    expect(sessions).toContain("Session · Dev · Strong");
    expect(sessions).not.toContain("Moved to Dev");
    expect(sessions).not.toContain("Created");
    expect(html(createElement(HistoryBody, { view: { ...view, rows: [] }, filter: "checks" }))).toContain(
      view.empty,
    );
  });

  it("registers the sheet's Activity tab", () => {
    expect(registered(registerScreens({ activity })).sheetTabs).toEqual(["activity"]);
  });
});

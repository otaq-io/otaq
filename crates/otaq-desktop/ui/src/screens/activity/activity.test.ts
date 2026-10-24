// The sheet's Activity (SHT-05, `s25-07`): History's rows oldest first under All · Comments · Stages · Description.
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
  { filter: "comments", label: "Comments", kind: "comment" },
  { filter: "stages", label: "Stages", kind: "stage" },
  { filter: "description", label: "Description", kind: "description" },
];

describe("Activity", () => {
  it("reads as a timeline: who, the sentence, the cost only where an agent ran, the copyable session ID (Q432)", () => {
    const view: HistoryView = {
      filters: FILTERS,
      order: "oldest first",
      copy_hint: "Copy session ID",
      copied: "Copied",
      empty: "Nothing has happened to this ticket yet.",
      total: null,
      key: "SHOP-28",
      rows: [
        {
          title: "You created the ticket",
          detail: null,
          when: "9:10 am",
          cost: null,
          tool_session_id: null,
          mark: { kind: "you" },
          opens: null,
          kind: "other",
        },
        {
          title: "Dev started · Rust developer",
          detail: "Codex · gpt-5.5",
          when: "9:42 am",
          cost: null,
          tool_session_id: "9f1c-77",
          mark: { kind: "agent" },
          opens: null,
          kind: "stage",
        },
        {
          title: "Dev finished · Rust developer",
          detail: "exited",
          when: "10:20 am",
          cost: "· 38 min · 74k tokens",
          tool_session_id: null,
          mark: { kind: "agent" },
          opens: null,
          kind: "stage",
        },
        {
          title: "Interrupted by restart",
          detail: "conversation saved",
          when: "10:21 am",
          cost: null,
          tool_session_id: null,
          mark: { kind: "otaq" },
          opens: null,
          kind: "stage",
        },
      ],
    };
    const out = html(createElement(HistoryBody, { view }));
    expect(out).toMatch(
      /9:10 am.*data-you.*You created the ticket.*9:42 am.*Dev started · Rust developer.*Codex · gpt-5.5/,
    );
    expect(out).toContain(
      'Dev finished · Rust developer<span class="history-cost"> · 38 min · 74k tokens</span>',
    );
    expect(out.match(/history-cost/g)).toHaveLength(1);
    expect(out).toContain("otaq-mark");
    expect(out).not.toContain("EVENT");
    expect(out).not.toContain("$");
    expect(out.match(/Copy session ID/g)).toHaveLength(1);
    expect(html(createElement(HistoryBody, { view: { ...view, rows: [] } }))).toContain(view.empty);
  });

  it(" validation rounds open nothing any more; the total sits under the rows", () => {
    const view: HistoryView = {
      filters: FILTERS,
      order: "oldest first",
      copy_hint: "Copy session ID",
      copied: "Copied",
      empty: "Nothing has happened to this ticket yet.",
      total: { label: "Validation total · 2 entries", tokens: "15k" },
      key: "SHOP-32",
      rows: [
        {
          title: "Round 1 · failed",
          detail: "agent submitted",
          when: "10:02",
          cost: "· 4 min · 12k tokens",
          tool_session_id: null,
          mark: { kind: "agent" },
          opens: null,
          kind: "stage",
        },
      ],
    };
    const out = html(createElement(HistoryBody, { view }));
    expect(out).toContain("Round 1 · failed");
    expect(out).not.toContain("aria-expanded");
    expect(out).toContain("Validation total · 2 entries");
    expect(out).toContain("15k");
  });

  it(" description and comment lines open their before / after and a report (s05-45, CMT-14)", () => {
    const row = (over: Partial<HistoryRowView>): HistoryRowView => ({
      title: "",
      detail: null,
      when: "2:14 pm",
      cost: null,
      tool_session_id: null,
      mark: { kind: "agent" },
      opens: null,
      kind: "comment",
      ...over,
    });
    const view: HistoryView = {
      filters: FILTERS,
      order: "oldest first",
      copy_hint: "Copy session ID",
      copied: "Copied",
      empty: "Nothing has happened to this ticket yet.",
      total: null,
      key: "SHOP-34",
      rows: [
        row({
          title: "Griller · Grill edited the description · added Agreed (4 lines)",
          kind: "description",
          opens: {
            label: "DESCRIPTION",
            caption: "Before / after",
            lines: [
              { kind: "same", sign: "", text: "Admins can export the orders table as CSV." },
              { kind: "added", sign: "+", text: "## Agreed" },
            ],
            report: null,
          },
        }),
        row({
          title: "Validator · Validate · round 2 commented · with a report (48 KB)",
          detail: "“Fail: the 10,000 row limit isn't…”",
          opens: {
            label: "COMMENT",
            caption: null,
            lines: [{ kind: "same", sign: "", text: "Fail: the 10,000 row limit isn't enforced." }],
            report: { n: 4, chip: { label: "Report", size: "· 48 KB", open: "Open" } },
          },
        }),
        row({ title: "You deleted a comment", mark: { kind: "you" } }),
      ],
    };
    const closed = html(createElement(HistoryBody, { view, opened: [] }));
    expect(closed.match(/aria-expanded="false"/g)).toHaveLength(2);
    expect(closed).toContain("comment-mark");
    expect(closed).not.toContain("DESCRIPTION");
    const opened = html(createElement(HistoryBody, { view, opened: [0, 1] }));
    expect(opened).toContain("DESCRIPTION");
    expect(opened).toContain("Before / after");
    expect(opened).toContain('data-kind="added"');
    expect(opened).toContain("## Agreed");
    expect(opened).toContain("· 48 KB");
    expect(opened).toContain(">Open</button>");
  });

  it("filters by kind under All · Comments · Stages · Description, oldest first; description edits open", () => {
    const row = (
      title: string,
      kind: HistoryRowView["kind"],
      opens: HistoryRowView["opens"] = null,
    ): HistoryRowView => ({
      title,
      detail: null,
      when: "2:02 pm",
      cost: null,
      tool_session_id: null,
      mark: { kind: "you" },
      opens,
      kind,
    });
    const view: HistoryView = {
      filters: FILTERS,
      order: "oldest first",
      copy_hint: "Copy session ID",
      copied: "Copied",
      empty: "Nothing has happened to this ticket yet.",
      total: null,
      key: "SHOP-22",
      rows: [
        row("Created", "other"),
        row("Griller · Grill edited the description · added 2 lines", "description", {
          label: "DESCRIPTION",
          caption: "Before / after",
          lines: [{ kind: "added", sign: "+", text: "· the redirect waits for the session cookie" }],
          report: null,
        }),
        row("Griller · Grill commented", "comment"),
        row("Moved to Dev", "stage"),
        row("You commented", "comment"),
      ],
    };
    expect(shownRows(view, "all")).toEqual([0, 1, 2, 3, 4]);
    expect(shownRows(view, "comments")).toEqual([2, 4]);
    expect(shownRows(view, "stages")).toEqual([3]);
    expect(shownRows(view, "description")).toEqual([1]);
    const all = html(createElement(HistoryBody, { view }));
    for (const text of ["All", "Comments", "Stages", "Description", "oldest first"])
      expect(all).toContain(text);
    expect(all).toContain('aria-checked="true">All');
    // The description edit's before / after shows without a click.
    expect(all).toContain("the redirect waits for the session cookie");
    const comments = html(createElement(HistoryBody, { view, filter: "comments" }));
    expect(comments).toContain("You commented");
    expect(comments).not.toContain("Moved to Dev");
    expect(comments).not.toContain("Created");
    expect(html(createElement(HistoryBody, { view: { ...view, rows: [] }, filter: "stages" }))).toContain(
      view.empty,
    );
  });

  it("registers the sheet's Activity tab", () => {
    expect(registered(registerScreens({ activity })).sheetTabs).toEqual(["activity"]);
  });
});

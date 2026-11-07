// What this agent reads: a SKILL.md's paragraphs flow (T10 item 11), its code and structure kept.
import { describe, expect, it } from "vitest";

import { flowSoftBreaks } from "./reads";

describe("flowSoftBreaks", () => {
  it("joins a paragraph's lines and a list item's, and keeps the front matter, headings and blank lines", () => {
    const skill = [
      "---",
      "name: work-ticket",
      "description: Works a ticket",
      "  across lines.",
      "---",
      "",
      "# Work the ticket",
      "",
      "You work in",
      "the ticket's own git worktree,",
      "on its branch.",
      "",
      "1. **Name it**: run this, with a",
      "   short name.",
      "2. Read first.",
      "- a point",
      "  that goes on",
      "",
      "> quoted",
      "> too",
      "",
      "| a | b |",
      "| - | - |",
      "",
      "Hard break  ",
      "stays.",
      "",
      "---",
      "After the rule.",
    ].join("\n");
    expect(flowSoftBreaks(skill).split("\n")).toEqual([
      "---",
      "name: work-ticket",
      "description: Works a ticket",
      "  across lines.",
      "---",
      "",
      "# Work the ticket",
      "",
      "You work in the ticket's own git worktree, on its branch.",
      "",
      "1. **Name it**: run this, with a short name.",
      "2. Read first.",
      "- a point that goes on",
      "",
      "> quoted",
      "> too",
      "",
      "| a | b |",
      "| - | - |",
      "",
      "Hard break  ",
      "stays.",
      "",
      "---",
      "After the rule.",
    ]);
  });

  it("keeps fenced and indented code as written", () => {
    const text = [
      "Run it:",
      "",
      "```sh",
      "otaq flow write-skill plan",
      "  --name plan-ticket",
      "```",
      "",
      "    indented code",
      "    second line",
      "",
      "Then",
      "go on.",
    ].join("\n");
    expect(flowSoftBreaks(text).split("\n")).toEqual([
      "Run it:",
      "",
      "```sh",
      "otaq flow write-skill plan",
      "  --name plan-ticket",
      "```",
      "",
      "    indented code",
      "    second line",
      "",
      "Then go on.",
    ]);
  });

  it("leaves text without soft breaks alone", () => {
    expect(flowSoftBreaks("One line.")).toBe("One line.");
    expect(flowSoftBreaks("")).toBe("");
  });
});

// An agent question's context read as simple Markdown: paragraphs with their line breaks, lists, inline code and bold;
// a code block kept whole, a heading as a bold paragraph, a table as plain text; nothing else is read.
import { describe, expect, it } from "vitest";

import { contextBlocks, inlineSpans } from "./questionContext";

describe("a question's context", () => {
  it("is nothing for a question with no newline (the view model gives no context)", () => {
    expect(contextBlocks("")).toEqual([]);
  });

  it("keeps paragraphs and their line breaks, and draws a list with code and bold", () => {
    const context = [
      "Your project has a security skill in `.claude/skills/security-review`.",
      "Two things to weigh:",
      "",
      "- **Verifier** already runs Validate on Codex,",
      "  so it would fall back to Otaq's own review skill.",
      "- **Strong** (Claude Code) loads your skill.",
      "",
      "A third agent costs a new connection.",
    ].join("\n");
    expect(contextBlocks(context)).toEqual([
      {
        kind: "paragraph",
        spans: [
          { kind: "text", text: "Your project has a security skill in " },
          { kind: "code", text: ".claude/skills/security-review" },
          { kind: "text", text: ".\nTwo things to weigh:" },
        ],
      },
      {
        kind: "list",
        items: [
          {
            marker: "•",
            spans: [
              { kind: "bold", text: "Verifier" },
              {
                kind: "text",
                text: " already runs Validate on Codex,\nso it would fall back to Otaq's own review skill.",
              },
            ],
          },
          {
            marker: "•",
            spans: [
              { kind: "bold", text: "Strong" },
              { kind: "text", text: " (Claude Code) loads your skill." },
            ],
          },
        ],
      },
      { kind: "paragraph", spans: [{ kind: "text", text: "A third agent costs a new connection." }] },
    ]);
  });

  it("keeps an ordered list's numbers", () => {
    expect(contextBlocks("1. First\n2) Second")).toEqual([
      {
        kind: "list",
        items: [
          { marker: "1.", spans: [{ kind: "text", text: "First" }] },
          { marker: "2.", spans: [{ kind: "text", text: "Second" }] },
        ],
      },
    ]);
  });

  it("keeps a code block whole, its Markdown not read", () => {
    const context =
      "Run this:\n\n```sh\nnpm audit --json | jq '.**critical**'\n  indented\n```\nThen tell me.";
    expect(contextBlocks(context)).toEqual([
      { kind: "paragraph", spans: [{ kind: "text", text: "Run this:" }] },
      { kind: "code", text: "npm audit --json | jq '.**critical**'\n  indented" },
      { kind: "paragraph", spans: [{ kind: "text", text: "Then tell me." }] },
    ]);
  });

  it("draws a heading as a bold paragraph and a table as plain text", () => {
    const context =
      "## What it checks\nThe lockfile.\n\n| Agent | Tool |\n|---|---|\n| Strong | Claude Code |";
    expect(contextBlocks(context)).toEqual([
      { kind: "heading", spans: [{ kind: "text", text: "What it checks" }] },
      { kind: "paragraph", spans: [{ kind: "text", text: "The lockfile." }] },
      { kind: "table", text: "| Agent | Tool |\n|---|---|\n| Strong | Claude Code |" },
    ]);
  });

  it("reads no HTML and no links: they show as written", () => {
    expect(inlineSpans('<img src=x onerror="alert(1)"> [docs](https://example.com)')).toEqual([
      { kind: "text", text: '<img src=x onerror="alert(1)"> [docs](https://example.com)' },
    ]);
    expect(inlineSpans("a `b` and __c__ but ** d**")).toEqual([
      { kind: "text", text: "a " },
      { kind: "code", text: "b" },
      { kind: "text", text: " and " },
      { kind: "bold", text: "c" },
      { kind: "text", text: " but ** d**" },
    ]);
  });
});

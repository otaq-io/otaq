// An agent question's context (everything after its first line, split in `otaq-view`) read as simple Markdown: its
// paragraphs and line breaks kept, lists, inline code and bold drawn; a code block is a monospace block, a heading a
// bold paragraph, a table plain text. Nothing else is read: no HTML, no links, no images (they show as written).

/** A run of text inside a paragraph, a list item or a heading. */
export type Span = { kind: "text" | "code" | "bold"; text: string };

/** A block of the context. */
export type Block =
  | { kind: "paragraph"; spans: Span[] }
  /** A heading, drawn as a bold paragraph. */
  | { kind: "heading"; spans: Span[] }
  /** Each item's spans, with the marker as written ("•" for an unordered list, "1." for an ordered one). */
  | { kind: "list"; items: { marker: string; spans: Span[] }[] }
  /** A fenced code block: monospace, scrolling sideways. */
  | { kind: "code"; text: string }
  /** A table's lines, shown as plain text. */
  | { kind: "table"; text: string };

const FENCE = /^\s{0,3}(```|~~~)/;
const HEADING = /^\s{0,3}#{1,6}\s+(.*?)\s*#*\s*$/;
const ITEM = /^\s{0,3}([-*+]|\d{1,9}[.)])\s+(.*)$/;
const TABLE = /^\s*\|/;

/** The inline runs of `text`: `code` and **bold** (or __bold__); anything else is text as written. */
export function inlineSpans(text: string): Span[] {
  const spans: Span[] = [];
  const push = (kind: Span["kind"], piece: string) => {
    if (piece === "") return;
    const last = spans.at(-1);
    if (kind === "text" && last?.kind === "text") last.text += piece;
    else spans.push({ kind, text: piece });
  };
  let i = 0;
  while (i < text.length) {
    const rest = text.slice(i);
    const code = /^(`+)([^`]|[^`][\s\S]*?[^`])\1(?!`)/.exec(rest);
    if (code !== null) {
      push("code", code[2] ?? "");
      i += code[0].length;
      continue;
    }
    const bold = /^(\*\*|__)(?=\S)([\s\S]*?\S)\1/.exec(rest);
    if (bold !== null) {
      push("bold", bold[2] ?? "");
      i += bold[0].length;
      continue;
    }
    push("text", text.charAt(i));
    i += 1;
  }
  return spans;
}

/** The blocks of a question's context, in order. */
export function contextBlocks(context: string): Block[] {
  const lines = context.split(/\r?\n/);
  const at = (n: number): string => lines[n] ?? "";
  const blocks: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = at(i);
    if (line.trim() === "") {
      i += 1;
      continue;
    }
    const fence = FENCE.exec(line);
    if (fence !== null) {
      const body: string[] = [];
      i += 1;
      while (
        i < lines.length &&
        !at(i)
          .trimStart()
          .startsWith(fence[1] ?? "```")
      ) {
        body.push(at(i));
        i += 1;
      }
      i += 1; // the closing fence (or the end)
      blocks.push({ kind: "code", text: body.join("\n") });
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading !== null) {
      blocks.push({ kind: "heading", spans: inlineSpans(heading[1] ?? "") });
      i += 1;
      continue;
    }
    if (TABLE.test(line)) {
      const rows: string[] = [];
      while (i < lines.length && TABLE.test(at(i))) {
        rows.push(at(i).trim());
        i += 1;
      }
      blocks.push({ kind: "table", text: rows.join("\n") });
      continue;
    }
    if (ITEM.test(line)) {
      const items: { marker: string; text: string }[] = [];
      while (i < lines.length && at(i).trim() !== "") {
        const item = ITEM.exec(at(i));
        const last = items.at(-1);
        if (item !== null) {
          const kind = item[1] ?? "-";
          const marker = /^\d/.test(kind) ? kind.replace(")", ".") : "•";
          items.push({ marker, text: item[2] ?? "" });
        } else if (last !== undefined) {
          // A wrapped item: its next line goes on with it.
          last.text += `\n${at(i).trim()}`;
        }
        i += 1;
      }
      blocks.push({
        kind: "list",
        items: items.map((it) => ({ marker: it.marker, spans: inlineSpans(it.text) })),
      });
      continue;
    }
    const para: string[] = [];
    while (
      i < lines.length &&
      at(i).trim() !== "" &&
      !FENCE.test(at(i)) &&
      !HEADING.test(at(i)) &&
      !ITEM.test(at(i)) &&
      !TABLE.test(at(i))
    ) {
      para.push(at(i));
      i += 1;
    }
    blocks.push({ kind: "paragraph", spans: inlineSpans(para.join("\n")) });
  }
  return blocks;
}

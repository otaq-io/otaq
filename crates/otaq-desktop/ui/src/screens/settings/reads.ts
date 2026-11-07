// What this agent reads draws a SKILL.md as written, in the mono face, but its paragraphs flow: a SKILL.md is
// hard-wrapped at ~120 columns, and those line breaks plus the panel's own wrapping broke long lines twice (T10).

/** A line that starts a Markdown block of its own (a heading, a list item, a quote, a table row, a fence, a rule). */
function startsBlock(trimmed: string): boolean {
  return (
    /^#{1,6}(\s|$)/.test(trimmed) ||
    /^[-*+]\s/.test(trimmed) ||
    /^\d{1,9}[.)]\s/.test(trimmed) ||
    trimmed.startsWith(">") ||
    trimmed.startsWith("|") ||
    trimmed.startsWith("```") ||
    trimmed.startsWith("~~~") ||
    /^([-*_])(\s*\1){2,}\s*$/.test(trimmed)
  );
}

/** A line nothing joins onto: a heading, a table row, a rule, or one that ends in a hard break. */
function endsBlock(line: string): boolean {
  const trimmed = line.trim();
  return (
    /^#{1,6}(\s|$)/.test(trimmed) ||
    trimmed.startsWith("|") ||
    /^([-*_])(\s*\1){2,}\s*$/.test(trimmed) ||
    line.endsWith("  ") ||
    line.endsWith("\\")
  );
}

const indent = (line: string) => line.length - line.trimStart().length;

/**
 * `text` (Markdown) with its soft line breaks taken out: the lines of a paragraph or of a list item join into one,
 * so the text wraps once, where its box ends. The front matter, fenced and indented code, headings, list items,
 * quotes, tables and blank lines stay as they are.
 */
export function flowSoftBreaks(text: string): string {
  const lines = text.split("\n");
  const out: string[] = [];
  let i = 0;
  // The front matter, as written.
  if (lines[0]?.trim() === "---") {
    const end = lines.findIndex((line, n) => n > 0 && line.trim() === "---");
    if (end > 0) {
      out.push(...lines.slice(0, end + 1));
      i = end + 1;
    }
  }
  let fence: string | null = null;
  // Whether the last line written may take the next one: it is paragraph or list text.
  let open = false;
  let inList = false;
  for (; i < lines.length; i++) {
    const line = lines[i] ?? "";
    const trimmed = line.trim();
    if (fence !== null) {
      out.push(line);
      if (trimmed.startsWith(fence)) fence = null;
      continue;
    }
    if (trimmed.startsWith("```") || trimmed.startsWith("~~~")) {
      fence = trimmed.slice(0, 3);
      out.push(line);
      open = false;
      continue;
    }
    if (trimmed === "") {
      out.push(line);
      open = false;
      continue;
    }
    const previous = out.length > 0 ? out[out.length - 1] : undefined;
    // Indented code: four spaces in, after a blank line, outside a list.
    const code = !open && !inList && indent(line) >= 4;
    if (code) {
      out.push(line);
      continue;
    }
    if (/^([-*+]|\d{1,9}[.)])\s/.test(trimmed)) inList = true;
    else if (indent(line) === 0 && !open) inList = false;
    if (open && previous !== undefined && !startsBlock(trimmed) && !endsBlock(previous)) {
      out[out.length - 1] = `${previous.trimEnd()} ${trimmed}`;
      continue;
    }
    out.push(line);
    open = !endsBlock(line);
  }
  return out.join("\n");
}

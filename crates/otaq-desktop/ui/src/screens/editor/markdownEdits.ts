// The comment box's Markdown edits (tuq 111, plan comments-as-markdown piece 7): what a toolbar button or a shortcut
// does to the text, as one replacement and the selection after it. Pure, so each edit is tested on its own; the box
// applies it through the textarea's own editing (`applyEdit` in `textareaEdit.ts`), so ⌘Z undoes it. Enter and a paste
// go the same way (piece 8): `continueList` and `pasteLink` give their edit, or `null` to leave the key or the paste to
// the textarea.
//
// - Bold, italic and code wrap the selection in their marks (`**fix**`, `_fix_`, `` `fix` ``), or with nothing selected
//   insert the marks with the caret between them. Pressed again on marked text (the marks inside or just around the
//   selection, or around the caret) they take the marks away. Spaces at the ends of a selection stay outside the
//   marks. Code over several lines is a fenced block.
// - Link makes `[text](url)` with `url` selected for typing over; a selected URL, or nothing, becomes `[](url)` with
//   the caret in the brackets; on a link it gives back the text.
// - The lists and quote act on every line the selection touches: each gets the marker (numbered lists count 1, 2,
//   3…), replacing another list's; when every line already has it, it is taken away.
// - Enter in a list item's text (`- `, `* `, `+ `, `1. `, `1) `, `- [ ] `, at any indent) starts the next item at the
//   same indent: the same bullet, the next number (the items after it aren't renumbered), an unticked box for a task.
//   Text after the caret moves into the new item. Enter on an item with no text takes its marker away and ends the
//   list. With a selection, or the caret in the indent or the marker, Enter is plain.
// - A single `https:`/`http:` URL pasted over selected text makes `[text](url)`, with the caret after it. Any other
//   paste is plain: nothing or only spaces selected, a selected URL (the paste replaces it), or anything but one web URL.
import type { CommentFormat } from "../../generated/CommentFormat";
import { LINK_SCHEMES } from "../../markdown/link/url";

/** The text and its selection (`start === end`: the caret). */
export interface Selected {
  text: string;
  start: number;
  end: number;
}

/** Replace `from`…`to` with `insert`, then select `selStart`…`selEnd` (offsets in the new text). */
export interface TextEdit {
  from: number;
  to: number;
  insert: string;
  selStart: number;
  selEnd: number;
}

/** The text after `edit`. */
export function applied(text: string, edit: TextEdit): string {
  return text.slice(0, edit.from) + edit.insert + text.slice(edit.to);
}

/** Wraps the selection in `open`…`close`, or takes them away when they are already there. */
export function toggleWrap(s: Selected, open: string, close: string): TextEdit {
  const { text, start, end } = s;
  const inner = text.slice(start, end);
  // The marks just around the selection (or the caret): take them away.
  if (
    start >= open.length &&
    text.slice(start - open.length, start) === open &&
    text.startsWith(close, end)
  ) {
    const from = start - open.length;
    return { from, to: end + close.length, insert: inner, selStart: from, selEnd: from + inner.length };
  }
  // The selection holds the marks.
  if (inner.length >= open.length + close.length && inner.startsWith(open) && inner.endsWith(close)) {
    const core = inner.slice(open.length, inner.length - close.length);
    return { from: start, to: end, insert: core, selStart: start, selEnd: start + core.length };
  }
  const core = inner.trim();
  const lead = core === "" ? 0 : inner.length - inner.trimStart().length;
  const word = core === "" ? inner : core;
  const trail = inner.slice(lead + word.length);
  const insert = inner.slice(0, lead) + open + word + close + trail;
  const selStart = start + lead + open.length;
  return { from: start, to: end, insert, selStart, selEnd: selStart + word.length };
}

const FENCE_OPEN = "```\n";
const FENCE_CLOSE = "\n```";

/** Code: inline backticks, or a fenced block when the selection spans lines (on lines of its own). */
export function toggleCode(s: Selected): TextEdit {
  const inner = s.text.slice(s.start, s.end);
  if (!inner.includes("\n")) return toggleWrap(s, "`", "`");
  const fenced = toggleWrap(s, FENCE_OPEN, FENCE_CLOSE);
  if (fenced.insert.length < inner.length) return fenced;
  // A new block starts and ends on lines of its own.
  const before = s.start > 0 && s.text[s.start - 1] !== "\n" ? "\n" : "";
  const after = s.end < s.text.length && s.text[s.end] !== "\n" ? "\n" : "";
  return {
    ...fenced,
    insert: before + fenced.insert + after,
    selStart: fenced.selStart + before.length,
    selEnd: fenced.selEnd + before.length,
  };
}

const URL_ONLY = /^https?:\/\/\S+$/;
const LINK = /^\[([^\]\n]*)\]\(([^)\n]*)\)$/;
const URL_PLACEHOLDER = "url";

/** Link: `[text](url)` with `url` selected; a selected URL gets empty brackets for its text; a link gives its text. */
export function toggleLink(s: Selected): TextEdit {
  const { text, start, end } = s;
  const inner = text.slice(start, end);
  const whole = LINK.exec(inner);
  if (whole) {
    const words = whole[1] ?? "";
    return { from: start, to: end, insert: words, selStart: start, selEnd: start + words.length };
  }
  // The selection is a link's text: `[` before it and `](…)` after it.
  if (start > 0 && text[start - 1] === "[" && text.startsWith("](", end)) {
    const close = text.indexOf(")", end + 2);
    const newline = text.indexOf("\n", end);
    if (close >= 0 && (newline < 0 || close < newline)) {
      const from = start - 1;
      return { from, to: close + 1, insert: inner, selStart: from, selEnd: from + inner.length };
    }
  }
  const core = inner.trim();
  // Nothing (or only spaces) selected: empty brackets with the caret in them, for the link's text.
  if (core === "") {
    const caret = end + 1;
    return { from: end, to: end, insert: `[](${URL_PLACEHOLDER})`, selStart: caret, selEnd: caret };
  }
  const lead = inner.length - inner.trimStart().length;
  const trail = inner.slice(lead + core.length);
  const head = inner.slice(0, lead);
  if (URL_ONLY.test(core)) {
    const insert = `${head}[](${core})${trail}`;
    const caret = start + head.length + 1;
    return { from: start, to: end, insert, selStart: caret, selEnd: caret };
  }
  const insert = `${head}[${core}](${URL_PLACEHOLDER})${trail}`;
  const url = start + head.length + core.length + 3;
  return { from: start, to: end, insert, selStart: url, selEnd: url + URL_PLACEHOLDER.length };
}

/** A line-level format: the three lists and quote. */
export type LineFormat = "bullet_list" | "numbered_list" | "task_list" | "quote";

type ListKind = "bullet_list" | "numbered_list" | "task_list";

/** A line's indent, its list marker (if any) and what follows. */
interface ListLine {
  indent: string;
  kind: ListKind | null;
  marker: string;
  rest: string;
}

const LIST_MARKER = /^(\s*)(?:([-*+] \[[ xX]\] )|([-*+] )|(\d+[.)] ))?/;

export function parseListLine(line: string): ListLine {
  const m = LIST_MARKER.exec(line);
  const indent = m?.[1] ?? "";
  const [kind, marker]: [ListKind | null, string] = m?.[2]
    ? ["task_list", m[2]]
    : m?.[3]
      ? ["bullet_list", m[3]]
      : m?.[4]
        ? ["numbered_list", m[4]]
        : [null, ""];
  return { indent, kind, marker, rest: line.slice(indent.length + marker.length) };
}

function marker(kind: ListKind, n: number): string {
  switch (kind) {
    case "bullet_list":
      return "- ";
    case "numbered_list":
      return `${String(n)}. `;
    case "task_list":
      return "- [ ] ";
  }
}

/** The whole lines the selection touches: from the first line's start to the last line's end. A selection ending
 * right after a line break doesn't take in the next line. */
export function linesOf(s: Selected): { from: number; to: number } {
  const from = s.text.lastIndexOf("\n", s.start - 1) + 1;
  const last = s.end > s.start && s.text[s.end - 1] === "\n" ? s.end - 1 : s.end;
  const next = s.text.indexOf("\n", Math.max(last, from));
  return { from, to: next < 0 ? s.text.length : next };
}

const QUOTED = /^(\s*)> ?/;

/** One line, changed by a line format: its new text and how long its prefix was before and is after. */
interface LineChange {
  line: string;
  oldPrefix: number;
  newPrefix: number;
}

function quoteLines(lines: readonly string[]): LineChange[] {
  const filled = lines.filter((l) => l.trim() !== "");
  const all = filled.length > 0 && filled.every((l) => QUOTED.test(l));
  return lines.map((line) => {
    if (all) {
      const m = QUOTED.exec(line);
      if (!m) return { line, oldPrefix: 0, newPrefix: 0 };
      const indent = m[1] ?? "";
      return { line: indent + line.slice(m[0].length), oldPrefix: m[0].length, newPrefix: indent.length };
    }
    // A blank line inside the quote keeps it one quote.
    if (line.trim() === "" && lines.length > 1) return { line: ">", oldPrefix: line.length, newPrefix: 1 };
    return { line: `> ${line}`, oldPrefix: 0, newPrefix: 2 };
  });
}

function listLines(lines: readonly string[], kind: ListKind): LineChange[] {
  const parsed = lines.map(parseListLine);
  // Blank lines among several are left alone; a single blank line gets the marker.
  const counts = (i: number) => lines.length === 1 || (lines[i] ?? "").trim() !== "";
  const counted = parsed.filter((_, i) => counts(i));
  const all = counted.length > 0 && counted.every((p) => p.kind === kind);
  let n = 0;
  return parsed.map((p, i) => {
    const line = lines[i] ?? "";
    const oldPrefix = p.indent.length + p.marker.length;
    if (!counts(i)) return { line, oldPrefix: 0, newPrefix: 0 };
    if (all) return { line: p.indent + p.rest, oldPrefix, newPrefix: p.indent.length };
    n += 1;
    const head = p.indent + marker(kind, n);
    return { line: head + p.rest, oldPrefix, newPrefix: head.length };
  });
}

/** A list or quote on the lines the selection touches (or the caret's line); pressed again, taken away. */
export function toggleLines(s: Selected, format: LineFormat): TextEdit {
  const { from, to } = linesOf(s);
  const lines = s.text.slice(from, to).split("\n");
  const changed = format === "quote" ? quoteLines(lines) : listLines(lines, format);
  const insert = changed.map((c) => c.line).join("\n");
  const only = changed[0];
  if (changed.length === 1 && only) {
    // One line: the caret or selection stays on the same words (an end past the line moves with the text after it).
    const at = (offset: number) =>
      offset > to
        ? offset + insert.length - (to - from)
        : from + only.newPrefix + Math.max(0, offset - from - only.oldPrefix);
    return { from, to, insert, selStart: at(s.start), selEnd: at(s.end) };
  }
  return { from, to, insert, selStart: from, selEnd: from + insert.length };
}

/** What a toolbar button or shortcut does to the text. */
export function formatEdit(format: CommentFormat, s: Selected): TextEdit {
  switch (format) {
    case "bold":
      return toggleWrap(s, "**", "**");
    case "italic":
      return toggleWrap(s, "_", "_");
    case "code":
      return toggleCode(s);
    case "link":
      return toggleLink(s);
    case "bullet_list":
    case "numbered_list":
    case "task_list":
    case "quote":
      return toggleLines(s, format);
  }
}

/** The marker of the item after one marked `marker`: the same bullet, the next number with the same delimiter, an
 * unticked box with the same bullet. */
function nextMarker(kind: ListKind, marker: string): string {
  switch (kind) {
    case "bullet_list":
      return marker;
    case "task_list":
      return `${marker.charAt(0)} [ ] `;
    case "numbered_list": {
      const m = /^(\d+)([.)])/.exec(marker);
      return `${String(Number(m?.[1] ?? "0") + 1)}${m?.[2] ?? "."} `;
    }
  }
}

/** Enter in a list item: the next item, or on an item with no text the end of the list. `null`: a plain Enter. */
export function continueList(s: Selected): TextEdit | null {
  if (s.start !== s.end) return null;
  const { from, to } = linesOf(s);
  const item = parseListLine(s.text.slice(from, to));
  if (item.kind === null || s.start < from + item.indent.length + item.marker.length) return null;
  if (item.rest.trim() === "") return { from, to, insert: "", selStart: from, selEnd: from };
  const insert = `\n${item.indent}${nextMarker(item.kind, item.marker)}`;
  const caret = s.start + insert.length;
  return { from: s.start, to: s.start, insert, selStart: caret, selEnd: caret };
}

/** Whether `text` is one web URL: `https:` or `http:` (`LINK_SCHEMES`), with a host and no spaces. */
export function isWebUrl(text: string): boolean {
  if (!/^https?:\/\/\S+$/i.test(text)) return false;
  try {
    const url = new URL(text);
    return LINK_SCHEMES.includes(url.protocol) && url.hostname !== "";
  } catch {
    return false;
  }
}

/** A paste over a selection: a single web URL makes `[selection](url)` with the caret after it; spaces at the ends of
 * the selection stay outside the link. `null`: a plain paste. */
export function pasteLink(s: Selected, pasted: string): TextEdit | null {
  if (s.start === s.end) return null;
  const url = pasted.trim();
  if (!isWebUrl(url)) return null;
  const inner = s.text.slice(s.start, s.end);
  const core = inner.trim();
  if (core === "" || isWebUrl(core)) return null;
  const head = inner.slice(0, inner.length - inner.trimStart().length);
  const trail = inner.slice(head.length + core.length);
  const link = `[${core}](${url})`;
  const caret = s.start + head.length + link.length;
  return { from: s.start, to: s.end, insert: head + link + trail, selStart: caret, selEnd: caret };
}

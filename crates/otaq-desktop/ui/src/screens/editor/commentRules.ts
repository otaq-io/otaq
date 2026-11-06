// The comment box's rules while typing (CMT-11, d0109; *05 → Row · Writing, editing and deleting*): a paste over the
// limit becomes a "Pasted text" chip, never text; typing past it shows "2,140 / 2,000" and turns the button off; with
// only chips the comment still needs its own line. The limit comes with the view (`CommentLimits`, from core's
// constant); the service checks it again. Typed text is never turned into a report.
//
// The box holds a list of attachments (tuq 001, 003), each its own chip with ✕, sent together in the box's order
// (`sentAttachments`): pasted text, and files picked with Attach, dropped, or pasted (a screenshot, files copied in
// Finder), each sent by its path. A box that holds one report (the New ticket form's, RPT-03: `CommentBoxView.attach`
// is null) keeps only the last one added (`withAdded`).
import type { CommentAttachmentParams } from "../../generated/CommentAttachmentParams";
import type { CommentLimits } from "../../generated/CommentLimits";
import type { OverviewFileChip } from "../../generated/OverviewFileChip";

/** A paste kept as one of the comment's attachments ("Pasted text"): its text and its size in UTF-8 bytes. */
export interface PastedAttachment {
  kind: "pasted";
  /** The chip's own, while the box holds it. */
  id: number;
  text: string;
  bytes: number;
}

/**
 * A file in the box: picked with Attach or Overview's Add, dropped, or pasted (kept in the data folder until the
 * comment is posted); sent by its path.
 */
export interface FileAttachment {
  kind: "file";
  id: number;
  path: string;
  name: string;
  /** "2 KB". */
  size: string;
  /** Shown with a thumbnail (tuq 002). */
  image: boolean;
  /** The thumbnail as a `data:` URL, once read; `null` before (or when it can't be drawn: the image icon stays). */
  thumb: string | null;
}

export type BoxAttachment = PastedAttachment | FileAttachment;

/** What the box holds: the typed text and its attachments, in the order they were added. */
export interface BoxValue {
  text: string;
  attachments: readonly BoxAttachment[];
}

export const EMPTY_BOX: BoxValue = { text: "", attachments: [] };

let lastId = 0;
function nextId(): number {
  lastId += 1;
  return lastId;
}

/** A file's chip (from `overview.file` or a paste's staging) as an attachment of the box. */
export function fileAttachment(chip: OverviewFileChip): FileAttachment {
  return {
    kind: "file",
    id: nextId(),
    path: chip.path,
    name: chip.name,
    size: chip.size,
    image: chip.image,
    thumb: null,
  };
}

/** Pasted text as an attachment of the box. */
export function pastedAttachment(text: string): PastedAttachment {
  return { kind: "pasted", id: nextId(), text, bytes: new TextEncoder().encode(text).length };
}

/** The box with `added` after what it holds; a box of one report (`one`) keeps only the last of them. */
export function withAdded(value: BoxValue, added: readonly BoxAttachment[], one = false): BoxValue {
  if (added.length === 0) return value;
  const last = added.at(-1);
  return { ...value, attachments: one && last ? [last] : [...value.attachments, ...added] };
}

/** The box after a paste that becomes the "Pasted text" chip. */
export function withPasted(value: BoxValue, text: string, one = false): BoxValue {
  return withAdded(value, [pastedAttachment(text)], one);
}

/** The box without attachment `id` (its ✕). */
export function withoutAttachment(value: BoxValue, id: number): BoxValue {
  return { ...value, attachments: value.attachments.filter((a) => a.id !== id) };
}

/** The box with attachment `id`'s thumbnail drawn. */
export function withThumb(value: BoxValue, id: number, thumb: string): BoxValue {
  return {
    ...value,
    attachments: value.attachments.map((a) => (a.id === id && a.kind === "file" ? { ...a, thumb } : a)),
  };
}

/** What the box sends (`ticket_comments.post`, the dialogs' requests): each attachment by its path or its text. */
export function sentAttachments(value: BoxValue): CommentAttachmentParams[] {
  return value.attachments.map((a) =>
    a.kind === "file" ? { kind: "file", path: a.path } : { kind: "pasted", text: a.text },
  );
}

/** Unicode characters (code points), as the service counts them. */
export function chars(text: string): number {
  return Array.from(text).length;
}

/** What a paste does: over the limit it becomes the report chip; otherwise it goes into the text as usual. */
export function pasteAction(pasted: string, limits: CommentLimits): "chip" | "text" {
  return chars(pasted) > limits.chars ? "chip" : "text";
}

/** "2,140". */
function grouped(n: number): string {
  return n.toLocaleString("en-US");
}

export interface BoxState {
  /** The text's length as the service counts it (trimmed). */
  count: number;
  /** Past the limit: the red box. */
  over: boolean;
  /** "2,140 / 2,000" while over the limit. */
  counter: string | null;
  /** Only pasted text, no line of text: "Say what the next agent should look for". */
  needsLine: boolean;
  /** A file waits in the box: "A comment always has its own line; the file goes with it." */
  fileLine: boolean;
  /** The button may send it. */
  canSubmit: boolean;
}

/** The box's state; `optional` is a dialog's box that may stay empty (no text and no chip). */
export function boxState(value: BoxValue, limits: CommentLimits, optional = false): BoxState {
  const count = chars(value.text.trim());
  const over = count > limits.chars;
  const empty = count === 0;
  const fileLine = value.attachments.some((a) => a.kind === "file");
  const needsLine = value.attachments.length > 0 && empty;
  return {
    count,
    over,
    counter: over ? `${grouped(count)} / ${grouped(limits.chars)}` : null,
    needsLine: needsLine && !fileLine,
    fileLine,
    canSubmit: !over && !needsLine && (!empty || optional),
  };
}

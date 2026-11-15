// The comment box's rules while typing (CMT-11, d0109; *05 → Row · Writing, editing and deleting*): a paste over the
// limit becomes a "Pasted text" chip, never text; typing past it shows "2,140 / 2,000" and turns the button off. A
// comment needs text, or at least one attachment, or both (tuq 007): a pasted screenshot is sent on its own. The limit
// comes with the view (`CommentLimits`, from core's constant); the service checks it again. Typed text is never turned
// into a report. The user's comments have no character limit (tuq 112: `CommentLimits.chars` is `null`); the one
// ceiling left is what one message to the service carries, and text that would not fit says so in words
// (`tooLongToSend`, `CommentLimits.too_long_to_send`) instead of being sent.
//
// The box holds a list of attachments (tuq 001, 003), each its own chip with ✕, sent together in the box's order
// (`sentAttachments`): pasted text, and files picked with Attach, dropped, or pasted (a screenshot, files copied in
// Finder), each sent by its path.
//
// Editing a posted comment uses the same box (tuq 008): it starts with the comment's text and its attachments as
// `posted` chips (`editBox`); ✕ removes one, paste, drop and Attach add new ones, and Save sends the text with the
// numbers of the posted ones gone and the new ones (`editChanges`), together.
//
// One comment never has two attachments of the same name: a new chip whose name another chip of the box already has
// (a second screenshot, two folders' `run.txt`, a second paste) is shown as "Pasted image 2.png", "run 2.txt",
// "Pasted text 2" (`uniqueName`). Core names the attachments by the same rule when it copies them (`unique_name` in
// otaq-core's `board/reports.rs`), in the order the box sends them, after the ones an edited comment keeps, so a
// chip's name is the attachment's.
import type { CommentAttachmentParams } from "../../generated/CommentAttachmentParams";
import type { CommentLimits } from "../../generated/CommentLimits";
import type { CommentRow } from "../../generated/CommentRow";
import type { OverviewFileChip } from "../../generated/OverviewFileChip";

/** A paste kept as one of the comment's attachments ("Pasted text"): its text and its size in UTF-8 bytes. */
export interface PastedAttachment {
  kind: "pasted";
  /** The chip's own, while the box holds it. */
  id: number;
  /** "Pasted text", or "Pasted text 2" beside another (`named`); unset until the box names it. */
  name?: string;
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
  /** The name the box shows and the comment will give it: `base`, or "shot 2.png" beside another `shot.png`. */
  name: string;
  /** The name it was added under (its file's), once the box has named it (`named`). */
  base?: string;
  /** "2 KB". */
  size: string;
  /** Shown with a thumbnail (tuq 002). */
  image: boolean;
  /** The thumbnail as a `data:` URL, once read; `null` before (or when it can't be drawn: the image icon stays). */
  thumb: string | null;
}

/**
 * An attachment the comment being edited already has (tuq 008), by its numbers: kept unless its ✕ removes it, which
 * Save sends as its number. An image shows its thumbnail, loaded like the thread's.
 */
export interface PostedAttachment {
  kind: "posted";
  id: number;
  /** The comment's number. */
  n: number;
  /** The attachment's number in it. */
  attachment: number;
  name: string;
  /** "120 KB". */
  size: string;
  image: boolean;
}

export type BoxAttachment = PastedAttachment | FileAttachment | PostedAttachment;

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

/** What pasted text is called, as core's `PASTED_TEXT`. */
export const PASTED_TEXT = "Pasted text";

/**
 * `name` made unique among `taken`: itself when free, else " 2", " 3"… before its extension ("shot 2.png",
 * "Pasted text 2", ".env 2"). The same rule as core's `unique_name`.
 */
export function uniqueName(name: string, taken: readonly string[]): string {
  if (!taken.includes(name)) return name;
  const dot = name.lastIndexOf(".");
  const [stem, ext] = dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ""];
  for (let k = 2; ; k += 1) {
    const candidate = `${stem} ${String(k)}${ext}`;
    if (!taken.includes(candidate)) return candidate;
  }
}

/**
 * The box's new attachments named apart (`uniqueName`): after the names of the posted ones it keeps, each in order
 * from the name it was added under. Run after every change, so a removed chip's name is free again, as it is for core.
 */
function named(attachments: readonly BoxAttachment[]): BoxAttachment[] {
  const taken = attachments.flatMap((a) => (a.kind === "posted" ? [a.name] : []));
  return attachments.map((a): BoxAttachment => {
    switch (a.kind) {
      case "posted":
        return a;
      case "pasted": {
        const name = uniqueName(PASTED_TEXT, taken);
        taken.push(name);
        return a.name === name ? a : { ...a, name };
      }
      case "file": {
        const base = a.base ?? a.name;
        const name = uniqueName(base, taken);
        taken.push(name);
        return a.name === name && a.base === base ? a : { ...a, base, name };
      }
    }
  });
}

/** The box with `added` after what it holds. */
export function withAdded(value: BoxValue, added: readonly BoxAttachment[]): BoxValue {
  if (added.length === 0) return value;
  return { ...value, attachments: named([...value.attachments, ...added]) };
}

/** The box after a paste that becomes the "Pasted text" chip. */
export function withPasted(value: BoxValue, text: string): BoxValue {
  return withAdded(value, [pastedAttachment(text)]);
}

/** The box without attachment `id` (its ✕). */
export function withoutAttachment(value: BoxValue, id: number): BoxValue {
  return { ...value, attachments: named(value.attachments.filter((a) => a.id !== id)) };
}

/** The box with attachment `id`'s thumbnail drawn. */
export function withThumb(value: BoxValue, id: number, thumb: string): BoxValue {
  return {
    ...value,
    attachments: value.attachments.map((a) => (a.id === id && a.kind === "file" ? { ...a, thumb } : a)),
  };
}

/**
 * What the box sends (`ticket_comments.post`, the dialogs' requests, an edit's new ones): each new attachment by its
 * path or its text; one the comment already has isn't sent again.
 */
export function sentAttachments(value: BoxValue): CommentAttachmentParams[] {
  return value.attachments.flatMap((a): CommentAttachmentParams[] => {
    switch (a.kind) {
      case "file":
        return [{ kind: "file", path: a.path }];
      case "pasted":
        return [{ kind: "pasted", text: a.text }];
      case "posted":
        return [];
    }
  });
}

/** A posted comment's attachments as the edit box's chips, in their order (images and the rest, by number). */
export function postedAttachments(
  row: Pick<CommentRow, "n" | "thumbnails" | "attachments">,
): PostedAttachment[] {
  const images = row.thumbnails.map((t): PostedAttachment => ({
    kind: "posted",
    id: nextId(),
    n: row.n,
    attachment: t.attachment,
    name: t.name,
    size: t.size,
    image: true,
  }));
  const others = row.attachments.map((c): PostedAttachment => ({
    kind: "posted",
    id: nextId(),
    n: row.n,
    attachment: c.attachment,
    name: c.name,
    // The thread's chip says "· 48 KB"; the box's say "48 KB".
    size: c.size.replace(/^\u00b7\s*/, ""),
    image: false,
  }));
  return [...images, ...others].sort((a, b) => a.attachment - b.attachment);
}

/** The edit box of a posted comment (tuq 008): its text and its attachments. */
export function editBox(row: Pick<CommentRow, "n" | "body" | "thumbnails" | "attachments">): BoxValue {
  return { text: row.body, attachments: postedAttachments(row) };
}

/** What Save sends for the comment being edited (tuq 008), and whether anything changed at all. */
export interface EditChanges {
  /** The numbers of the attachments it had that the box no longer holds. */
  remove: number[];
  /** The new ones. */
  attachments: CommentAttachmentParams[];
  changed: boolean;
}

export function editChanges(
  row: Pick<CommentRow, "n" | "body" | "thumbnails" | "attachments">,
  value: BoxValue,
): EditChanges {
  const kept = new Set(value.attachments.flatMap((a) => (a.kind === "posted" ? [a.attachment] : [])));
  const remove = postedAttachments(row)
    .map((a) => a.attachment)
    .filter((a) => !kept.has(a));
  const attachments = sentAttachments(value);
  const changed = value.text.trim() !== row.body || remove.length > 0 || attachments.length > 0;
  return { remove, attachments, changed };
}

/** Unicode characters (code points), as the service counts them. */
export function chars(text: string): number {
  return Array.from(text).length;
}

/** What a paste does: over the limit it becomes the report chip; otherwise (and always with no limit, the user's
 * comments' since tuq 112) it goes into the text as usual. */
export function pasteAction(pasted: string, limits: CommentLimits): "chip" | "text" {
  return limits.chars !== null && chars(pasted) > limits.chars ? "chip" : "text";
}

/** The bytes `text` takes as sent: its JSON string in UTF-8, quotes and escapes included. */
export function sentBytes(text: string): number {
  return new TextEncoder().encode(JSON.stringify(text)).length;
}

/**
 * The box's text and pasted text would not fit in one message to the service (`limits.message_bytes`, 16 MB less
 * room for the rest): the one ceiling left on the user's comments (tuq 112).
 */
export function tooLongToSend(value: BoxValue, limits: CommentLimits): boolean {
  const texts = [value.text, ...value.attachments.flatMap((a) => (a.kind === "pasted" ? [a.text] : []))];
  // At most 6 bytes per UTF-16 unit (a control character's `\u0000`) and the quotes: most text needs no count.
  const most = texts.reduce((n, t) => n + t.length * 6 + 2, 0);
  if (most <= limits.message_bytes) return false;
  return texts.reduce((n, t) => n + sentBytes(t), 0) > limits.message_bytes;
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
  /** Why it can't be sent, in words, when it would not fit in one message (`tooLongToSend`); else `null`. */
  tooLong: string | null;
  /** The button may send it. */
  canSubmit: boolean;
}

/** The box's state; `optional` is a dialog's box that may stay empty (no text and no chip). Text, an attachment, or
 * both may be sent (tuq 007). */
export function boxState(value: BoxValue, limits: CommentLimits, optional = false): BoxState {
  const count = chars(value.text.trim());
  const limit = limits.chars;
  const over = limit !== null && count > limit;
  const tooLong = tooLongToSend(value, limits) ? limits.too_long_to_send : null;
  const empty = count === 0;
  const attached = value.attachments.length > 0;
  return {
    count,
    over,
    counter: over ? `${grouped(count)} / ${grouped(limit)}` : null,
    tooLong,
    canSubmit: !over && tooLong === null && (!empty || attached || optional),
  };
}

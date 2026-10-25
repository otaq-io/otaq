// The editor's form state (T-0228): what is typed and what a key does. The rules (title required, only what changed
// is saved, priority only once picked) are
// Rust's (`src/api/editor.rs`, `editor.rs`); this file keeps only the moment of typing.
import type { EditorForm } from "../../generated/EditorForm";
import type { EditorRefusal } from "../../generated/EditorRefusal";
import type { EditorView } from "../../generated/EditorView";
import type { BoxValue } from "./commentRules";

/** The service events that change what the editor shows. */
export const EDITOR_EVENTS = ["ticket_changed", "ticket_deleted", "session_state"] as const;

/** What a key does in quick-create: ↵ creates, ⇧↵ opens the full form with the title kept, Esc cancels. */
export function quickKey(e: {
  key: string;
  shiftKey: boolean;
  isComposing?: boolean;
}): "create" | "full" | "cancel" | null {
  if (e.isComposing) return null;
  if (e.key === "Escape") return "cancel";
  if (e.key !== "Enter") return null;
  return e.shiftKey ? "full" : "create";
}

/** ⌘↵ (Ctrl+↵ off the Mac) saves the form from any field, the description too. */
/** ⌘↵ on macOS, Ctrl+↵ elsewhere. */
export const SAVE_KEYS = navigator.userAgent.includes("Mac") ? "⌘↵" : "Ctrl+↵";

export function isSaveKey(e: {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  isComposing?: boolean;
}): boolean {
  return e.key === "Enter" && (e.metaKey || e.ctrlKey) && !e.isComposing;
}

/** Save is enabled once there is a title (`EditorModel::can_save`; Rust checks again). */
export function canSave(form: EditorForm, saving: boolean): boolean {
  return form.title.trim() !== "" && !saving;
}

/**
 * The form's report fields from the New ticket form's box (RPT-03): the file by its path or the pasted text, and the
 * line typed with it; no report, no line.
 */
export function formReport(box: BoxValue): Pick<EditorForm, "report" | "report_line"> {
  if (box.file !== null) return { report: { kind: "file", path: box.file.path }, report_line: box.text };
  if (box.pasted !== null)
    return { report: { kind: "pasted", text: box.pasted.text }, report_line: box.text };
  return { report: undefined, report_line: undefined };
}

/** The priority the segmented control shows: the picked one, else the saved one, else Normal. */
export function shownPriority(view: EditorView, form: EditorForm): number {
  return form.priority ?? view.shown_priority;
}

/** The refusal shown under one field (`null`: none there). */
export function refusalFor(
  refusal: EditorRefusal | null,
  field: EditorRefusal["field"],
): EditorRefusal | null {
  return refusal?.field === field ? refusal : null;
}

/** Whether the form differs from what it opened with (Esc on a changed form still discards: the frames say so). */
export function changed(initial: EditorForm, form: EditorForm): boolean {
  return JSON.stringify(initial) !== JSON.stringify(form);
}

/** How long Copy as Markdown shows its check and "Copied" (*05 → c · Copied state*), in ms. */
export const COPIED_FOR = 1500;

/**
 * Copy as Markdown (d0078 D7): the description exactly as stored, byte for byte, as plain text only (no HTML). Never a
 * re-serialisation from the editor, which would spell some Markdown differently.
 */
export function copyMarkdown(
  stored: string,
  clipboard: Pick<Clipboard, "writeText"> = navigator.clipboard,
): Promise<void> {
  return clipboard.writeText(stored);
}

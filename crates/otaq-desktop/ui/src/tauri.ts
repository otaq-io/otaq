// The one TypeScript file that names Tauri (UIT-02). Everything else imports these functions, so a Tauri upgrade
// touches only this file and the Rust side's `src/door.rs`. Screens don't edit it: they call `request` with their own
// method names and use the generated types for what goes in and comes back.
import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";

import type { OverviewFileChip } from "./generated/OverviewFileChip";
import type { TerminalFeed } from "./generated/TerminalFeed";
import type { ThemeMode } from "./generated/ThemeMode";
import type { WindowAction } from "./generated/WindowAction";

/**
 * The one request call: a method name and JSON params in, the JSON reply out. Rejects with the Rust side's
 * `RequestError` (see `asRequestError` in `service.ts`).
 */
export function request(method: string, params: unknown = null): Promise<unknown> {
  return invoke("request", { method, params });
}

/** The scheme the Rust side serves comment images on (`src/attachment_images.rs`). */
const ATTACHMENT_SCHEME = "otaq-attachment";

/**
 * Where an `<img>` loads image `attachment` of comment `n` on `ticket` (tuq 004): by its numbers, never a path; the
 * Rust side finds the file and serves only images inside the data folder's reports. Tauri builds the URL for the
 * platform (`otaq-attachment://localhost/…`, `http://otaq-attachment.localhost/…` on Windows); outside Tauri (tests)
 * the macOS form.
 */
export function attachmentImage(ticket: string, n: number, attachment: number): string {
  const path = `${ticket}/${String(n)}/${String(attachment)}`;
  try {
    return convertFileSrc(path, ATTACHMENT_SCHEME);
  } catch {
    return `${ATTACHMENT_SCHEME}://localhost/${encodeURIComponent(path)}`;
  }
}

/** Subscribes to the app's event stream; `onEvent` gets every event from now on. */
export async function subscribeEvents(onEvent: (event: unknown) => void): Promise<void> {
  const channel = new Channel<unknown>();
  channel.onmessage = onEvent;
  await invoke("subscribe_events", { channel });
}

/**
 * Attaches a terminal to a session over its own channel; `onFeed` gets its stored output, then live output and phase
 * changes, until `terminal.detach` with the number this resolves to. A `readOnly` terminal doesn't answer the
 * program's questions, so the service answers its colour questions while no other terminal does (d0117 point 4).
 */
export function attachTerminal(
  session: string,
  onFeed: (feed: TerminalFeed) => void,
  readOnly = false,
): Promise<number> {
  const channel = new Channel<TerminalFeed>();
  channel.onmessage = onFeed;
  return invoke<number>("attach_terminal", { session, channel, readOnly });
}

/** An action on the page's window: a caption button's (minimize, maximize or restore, close), or bringing it forward. */
export function windowAction(action: WindowAction): Promise<void> {
  return invoke("window", { action });
}

/** Draws the window's own parts (title bar, menus, the menu-bar icon's menu) for the theme mode: as the OS on Match
 * system, else light or dark. */
export function windowTheme(mode: ThemeMode): Promise<void> {
  return invoke("window_theme", { mode });
}

/** The system folder picker titled `title`: the folder chosen, or `null` when cancelled (Add project). */
export function pickFolder(title: string): Promise<string | null> {
  return invoke<string | null>("pick_folder", { title });
}

/** The system file picker for a flow file (Import flow…): the YAML file chosen, or `null` when cancelled. */
export function pickFlowFile(title: string): Promise<string | null> {
  return invoke<string | null>("pick_flow_file", { title });
}

/** The system file picker for a report (Overview's Add): the file chosen, or `null` when cancelled. */
export function pickReportFile(title: string): Promise<string | null> {
  return invoke<string | null>("pick_report_file", { title });
}

/** The system file picker for a comment box's Attach (tuq 003): every file chosen, none when cancelled. */
export function pickAttachmentFiles(title: string): Promise<string[]> {
  return invoke<string[]>("pick_attachment_files", { title });
}

/**
 * A file pasted into a comment box (a screenshot, files copied in Finder), which has no path: its bytes go as a raw
 * body (binary IPC, never JSON) and its name in a header; the data folder keeps it until the comment is posted. Its
 * chip comes back, sent by path like a picked file.
 */
export async function stageAttachment(file: Blob, name: string): Promise<OverviewFileChip> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  return invoke<OverviewFileChip>("stage_attachment", bytes, {
    headers: { "x-name": encodeURIComponent(name) },
  });
}

/** An image in a comment box, for its thumbnail: its bytes, as a raw answer. */
export function boxImage(path: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("box_image", { path });
}

/** The system save dialog for a flow file (Export flow…), offering `fileName`: where to save, or `null` when cancelled. */
export function saveFlowFile(title: string, fileName: string): Promise<string | null> {
  return invoke<string | null>("save_flow_file", { title, fileName });
}

/** The system save dialog for a report (the report view's Download), offering `fileName`: where to save, or `null`. */
export function saveReportFile(title: string, fileName: string): Promise<string | null> {
  return invoke<string | null>("save_report_file", { title, fileName });
}

/**
 * Spread on an element to make it the window's drag region, children included: pressing it moves the window, a double
 * click maximizes (zooms on macOS). Buttons, inputs and links inside keep their clicks.
 */
export const DRAG_REGION = { "data-tauri-drag-region": "deep" } as const;

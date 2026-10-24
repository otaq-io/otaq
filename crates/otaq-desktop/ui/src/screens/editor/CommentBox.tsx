// The comment box (CMT-11, d0109; *05 → Row · Ticket comments and a report* and *Row · Writing, editing and
// deleting*, `s05-41`…`s05-44`): at the end of the Ticket tab's thread, and inside the Send back, Fail and drag-back
// dialogs (T-0447), which give it their label and line (`CommentBoxView.dialog`) and send it with their own button.
// A paste over the limit becomes the "Pasted text · 48 KB" chip (open, remove), never text; with only the chip the
// button stays off and the hint asks for a line; typing past the limit shows "2,140 / 2,000", the red box, and turns
// the button off (`commentRules.ts`). A file added in Overview (Add, a drop; RPT-02) waits as its own chip
// ("perf-notes.md · 2 KB ✕") with the line "A comment always has its own line; the file goes with it."; a file pasted
// into the box becomes the pasted chip. Its words and the limit come from Rust (`CommentBoxView`); the box keeps only
// what is typed. ⌘↵ sends it.
import { type ClipboardEvent, type KeyboardEvent, type Ref, useCallback, useId } from "react";

import { Button, Kbd, PROSE_TEXT } from "../../components";
import type { CommentBoxView } from "../../generated/CommentBoxView";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import {
  type AttachedFile,
  type BoxValue,
  boxState,
  type Pasted,
  pasteAction,
  withPasted,
} from "./commentRules";
import { isSaveKey, SAVE_KEYS } from "./logic";
import { openReport } from "./reportOpener";

const NO_EVENTS: readonly string[] = [];

/** "Pasted text · 48 KB ✕": the label opens it, ✕ removes it. */
function PastedChip({
  ticket,
  pasted,
  view,
  onRemove,
}: {
  ticket: string;
  pasted: Pasted;
  view: CommentBoxView;
  onRemove: () => void;
}) {
  const { bytes } = pasted;
  const load = useCallback(
    () => request("ticket_comments.pasted_size", { bytes }) as Promise<string>,
    [bytes],
  );
  const size = useServiceView(load, NO_EVENTS).view ?? "";
  return (
    <span className="comment-chip">
      <button
        type="button"
        className="comment-chip-open"
        title={view.open}
        onClick={() => {
          openReport({ kind: "pasted", ticket, text: pasted.text, size });
        }}
      >
        <Icon name="file-text" size={13} />
        <span className="comment-chip-label">{view.pasted}</span>
        <span className="comment-chip-size">{size}</span>
      </button>
      <button type="button" className="comment-chip-remove" aria-label={view.remove} onClick={onRemove}>
        <Icon name="x" size={13} />
      </button>
    </span>
  );
}

/** "perf-notes.md 2 KB ✕": a file added in Overview, sent as the comment's report. */
function FileChip({
  file,
  view,
  onRemove,
}: {
  file: AttachedFile;
  view: CommentBoxView;
  onRemove: () => void;
}) {
  return (
    <span className="comment-chip" data-file>
      <span className="comment-chip-open" data-static>
        <Icon name="file-text" size={13} />
        <span className="comment-chip-label">{file.name}</span>
        <span className="comment-chip-size">{file.size}</span>
      </span>
      <button type="button" className="comment-chip-remove" aria-label={view.remove_file} onClick={onRemove}>
        <Icon name="x" size={13} />
      </button>
    </span>
  );
}

export interface CommentBoxProps {
  /** The ticket it comments on (Open on the chip names it). */
  ticket: string;
  view: CommentBoxView;
  value: BoxValue;
  onChange: (value: BoxValue) => void;
  /** ⌘↵, and the box's own Comment button when it has one; called only when the box may be sent. */
  onSubmit: () => void;
  /** Sending: the button shows its loader. */
  busy?: boolean;
  /** The service's refusal, under the box. */
  error?: string | null;
  ref?: Ref<HTMLTextAreaElement>;
}

export function CommentBox({
  ticket,
  view,
  value,
  onChange,
  onSubmit,
  busy = false,
  error,
  ref,
}: CommentBoxProps) {
  const id = useId();
  const state = boxState(value, view.limits, view.optional);
  const submit = () => {
    if (state.canSubmit && !busy) onSubmit();
  };
  const onPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    // A file (copied in Finder): its text becomes the pasted report, the comment still needs its line.
    const file = (e.clipboardData.files as FileList | undefined)?.[0];
    if (file !== undefined) {
      e.preventDefault();
      file.text().then(
        (text) => {
          onChange(withPasted(value, text));
        },
        () => undefined,
      );
      return;
    }
    const text = e.clipboardData.getData("text/plain");
    if (pasteAction(text, view.limits) !== "chip") return;
    // Over the limit: the report chip, never text (d0109).
    e.preventDefault();
    onChange(withPasted(value, text));
  };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (!isSaveKey(e)) return;
    e.preventDefault();
    // The dialog around it or the panel acts on none of it.
    e.stopPropagation();
    submit();
  };
  return (
    <div className="comment-box" data-over={state.over || undefined}>
      {view.label !== null && (
        <label className="comment-box-label" htmlFor={id}>
          {view.label}
        </label>
      )}
      <textarea
        {...PROSE_TEXT}
        ref={ref}
        id={id}
        className="text-input text-area comment-box-text"
        value={value.text}
        placeholder={view.placeholder}
        aria-label={view.label ?? view.placeholder}
        aria-invalid={state.over || undefined}
        onChange={(e) => {
          onChange({ ...value, text: e.target.value });
        }}
        onPaste={onPaste}
        onKeyDown={onKeyDown}
      />
      {value.file !== null && (
        <div className="comment-box-chips">
          <FileChip
            file={value.file}
            view={view}
            onRemove={() => {
              onChange({ ...value, file: null });
            }}
          />
        </div>
      )}
      {value.pasted !== null && (
        <div className="comment-box-chips">
          <PastedChip
            ticket={ticket}
            pasted={value.pasted}
            view={view}
            onRemove={() => {
              onChange({ ...value, pasted: null });
            }}
          />
        </div>
      )}
      <div className="comment-box-actions">
        {state.fileLine && (
          <span className="comment-box-hint" role="status">
            {view.file_line}
          </span>
        )}
        {state.needsLine && (
          <span className="comment-box-hint" role="status">
            <Icon name="info" size={13} />
            {view.needs_line}
          </span>
        )}
        {state.counter !== null && (
          <span className="comment-box-counter" role="status">
            {state.counter}
          </span>
        )}
        <span className="editor-spacer" />
        {view.submit !== null && (
          <>
            <Kbd keys={SAVE_KEYS} />
            <Button
              label={view.submit}
              icon="send-horizontal"
              disabled={!state.canSubmit}
              loading={busy}
              onClick={submit}
            />
          </>
        )}
      </div>
      {error != null && (
        <p className="comment-box-error" role="alert">
          {error}
        </p>
      )}
      {view.footer !== null && (
        <p className="comment-box-footer">
          <Icon name={view.footer_icon} size={12} />
          {view.footer}
        </p>
      )}
    </div>
  );
}

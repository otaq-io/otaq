// The comment box (CMT-11, d0109; *05 → Row · Ticket comments and a report* and *Row · Writing, editing and
// deleting*, `s05-41`…`s05-44`): at the end of the Ticket tab's thread, and inside the Send back, Fail and drag-back
// dialogs (T-0447), which give it their label and line (`CommentBoxView.dialog`) and send it with their own button.
// It holds the text and a list of attachments (tuq 001), each its own chip with ✕, added three ways (tuq 003): a paste
// (a screenshot becomes an image, files copied in Finder keep their names, text over the limit becomes "Pasted text ·
// 48 KB", never text), files dropped on the box, and Attach (the system picker, several at once). An image's chip shows
// a thumbnail (tuq 002); other files show name and size. A file over 20 MB is refused when it is added, by name, and
// the rest of the box stays (tuq 005; `attachmentAdding.ts`). With only pasted text the button stays off and the hint
// asks for a line; with a file the line "A comment always has its own line; the file goes with it." shows; typing
// past the limit shows "2,140 / 2,000", the red box, and turns the button off (`commentRules.ts`). Its words and the
// limits come from Rust (`CommentBoxView`); the box keeps only what is typed and added. ⌘↵ sends it. While the
// ticket's agent or check runs the thread's box shows "Also tell the agent now" (d0123, `CommentBoxView.tell_agent`),
// ticked by default; the thread keeps the tick and posts it as `tell_agent`. Editing a posted comment uses this box
// too (tuq 008): its attachments are `posted` chips with ✕, and the thread gives it Cancel · Save (`actions`).
import {
  type ClipboardEvent,
  type KeyboardEvent,
  type ReactNode,
  type Ref,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import { Button, Checkbox, Kbd, PROSE_TEXT } from "../../components";
import type { CommentBoxView } from "../../generated/CommentBoxView";
import { useServiceView } from "../../service";
import { attachmentImage, pickAttachmentFiles, request } from "../../tauri";
import { dropTargets } from "../../terminal/drops";
import { Icon } from "../../theme/Icon";
import { type Added, addFiles, addPaths, clipboardFiles, thumbnailOf } from "./attachmentAdding";
import {
  type BoxValue,
  boxState,
  type FileAttachment,
  type PastedAttachment,
  pasteAction,
  type PostedAttachment,
  withAdded,
  withoutAttachment,
  withPasted,
  withThumb,
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
  pasted: PastedAttachment;
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

/** "perf-notes.md 2 KB ✕", or an image's thumbnail before its name (tuq 002). */
function FileChip({
  file,
  view,
  onRemove,
}: {
  file: FileAttachment;
  view: CommentBoxView;
  onRemove: () => void;
}) {
  return (
    <span className="comment-chip" data-file data-image={file.image || undefined}>
      <span className="comment-chip-open" data-static>
        {file.image && file.thumb !== null ? (
          <img className="comment-chip-thumb" src={file.thumb} alt="" />
        ) : (
          <Icon name={file.image ? "image" : "file-text"} size={13} />
        )}
        <span className="comment-chip-label">{file.name}</span>
        <span className="comment-chip-size">{file.size}</span>
      </span>
      <button
        type="button"
        className="comment-chip-remove"
        aria-label={`${view.remove_file} ${file.name}`}
        onClick={onRemove}
      >
        <Icon name="x" size={13} />
      </button>
    </span>
  );
}

/** One of an edited comment's own attachments (tuq 008): its thumbnail (loaded by its numbers) or icon, name, size, ✕. */
function PostedChip({
  ticket,
  posted,
  view,
  onRemove,
}: {
  ticket: string;
  posted: PostedAttachment;
  view: CommentBoxView;
  onRemove: () => void;
}) {
  return (
    <span className="comment-chip" data-file data-posted data-image={posted.image || undefined}>
      <span className="comment-chip-open" data-static>
        {posted.image ? (
          <img
            className="comment-chip-thumb"
            src={attachmentImage(ticket, posted.n, posted.attachment)}
            alt=""
          />
        ) : (
          <Icon name="file-text" size={13} />
        )}
        <span className="comment-chip-label">{posted.name}</span>
        <span className="comment-chip-size">{posted.size}</span>
      </span>
      <button
        type="button"
        className="comment-chip-remove"
        aria-label={`${view.remove_file} ${posted.name}`}
        onClick={onRemove}
      >
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
  /** "Also tell the agent now" ticked; shown only when the view has the tick box (d0123). */
  tellAgent?: boolean;
  onTellAgent?: (tell: boolean) => void;
  /** Buttons at the end of the box's row instead of its own (an edit's Cancel · Save, tuq 008). */
  actions?: ReactNode;
  /** Esc: an edit's Cancel. */
  onCancel?: () => void;
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
  tellAgent = true,
  onTellAgent,
  actions,
  onCancel,
}: CommentBoxProps) {
  const id = useId();
  const state = boxState(value, view.limits, view.optional);
  // A box of one report (the New ticket form's): a file added replaces the last; its form takes drops itself.
  const one = view.attach === null;
  // Files arrive after a moment (a request, a picker): they join what the box holds then, not when they were added.
  const latest = useRef(value);
  useLayoutEffect(() => {
    latest.current = value;
  }, [value]);
  const change = useCallback(
    (next: BoxValue) => {
      latest.current = next;
      onChange(next);
    },
    [onChange],
  );
  // Why a file couldn't be added (over 20 MB, gone): under the box until the next one is added.
  const [refused, setRefused] = useState<readonly string[]>([]);
  const take = useCallback(
    ({ added, refused: why }: Added) => {
      change(withAdded(latest.current, added, one));
      setRefused(why);
    },
    [change, one],
  );
  const addPathsToBox = useCallback(
    (paths: readonly string[]) => {
      if (paths.length > 0) void addPaths(paths).then(take);
    },
    [take],
  );

  // Files dropped on the box (the door's `files_dropped`, as Overview takes them).
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = root.current;
    if (!el || one) return;
    return dropTargets.register(el, {
      takesTyping: () => false,
      paste: () => undefined,
      takeFiles: addPathsToBox,
    });
  }, [one, addPathsToBox]);

  // Each image's thumbnail, read once from its file.
  const drawing = useRef(new Set<number>());
  useEffect(() => {
    for (const a of value.attachments) {
      if (a.kind !== "file" || !a.image || a.thumb !== null || drawing.current.has(a.id)) continue;
      drawing.current.add(a.id);
      thumbnailOf(a.path).then(
        (thumb) => {
          change(withThumb(latest.current, a.id, thumb));
        },
        () => undefined,
      );
    }
  }, [value.attachments, change]);

  const submit = () => {
    if (state.canSubmit && !busy) onSubmit();
  };
  const onPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    // A screenshot or files copied in Finder: each becomes an attachment under its own name (tuq 003).
    const files = clipboardFiles(e.clipboardData);
    if (files.length > 0) {
      e.preventDefault();
      void addFiles(files).then(take);
      return;
    }
    const text = e.clipboardData.getData("text/plain");
    if (pasteAction(text, view.limits) !== "chip") return;
    // Over the limit: the report chip, never text (d0109).
    e.preventDefault();
    change(withPasted(value, text, one));
  };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Escape" && onCancel) {
      e.preventDefault();
      e.stopPropagation();
      onCancel();
      return;
    }
    if (!isSaveKey(e)) return;
    e.preventDefault();
    // The dialog around it or the sheet acts on none of it.
    e.stopPropagation();
    submit();
  };
  const remove = (attachment: number) => {
    change(withoutAttachment(value, attachment));
  };
  return (
    <div ref={root} className="comment-box" data-over={state.over || undefined}>
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
          change({ ...value, text: e.target.value });
        }}
        onPaste={onPaste}
        onKeyDown={onKeyDown}
      />
      {value.attachments.length > 0 && (
        <div className="comment-box-chips">
          {value.attachments.map((a) => {
            const onRemove = () => {
              remove(a.id);
            };
            switch (a.kind) {
              case "file":
                return <FileChip key={a.id} file={a} view={view} onRemove={onRemove} />;
              case "pasted":
                return <PastedChip key={a.id} ticket={ticket} pasted={a} view={view} onRemove={onRemove} />;
              case "posted":
                return <PostedChip key={a.id} ticket={ticket} posted={a} view={view} onRemove={onRemove} />;
            }
          })}
        </div>
      )}
      <div className="comment-box-actions">
        {view.attach !== null && (
          <Button
            variant="ghost"
            compact
            icon="paperclip"
            label={view.attach}
            onClick={() => {
              pickAttachmentFiles(view.attach_title).then(addPathsToBox, () => undefined);
            }}
          />
        )}
        {state.fileLine && view.file_line !== null && (
          <span className="comment-box-hint" role="status">
            {view.file_line}
          </span>
        )}
        {state.counter !== null && (
          <span className="comment-box-counter" role="status">
            {state.counter}
          </span>
        )}
        <span className="editor-spacer" />
        {view.tell_agent !== null && onTellAgent && (
          <Checkbox checked={tellAgent} label={view.tell_agent} onChange={onTellAgent} />
        )}
        {actions}
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
      {refused.map((why) => (
        <p key={why} className="comment-box-error" role="alert">
          {why}
        </p>
      ))}
      {view.footer !== null && (
        <p className="comment-box-footer">
          <Icon name={view.footer_icon} size={12} />
          {view.footer}
        </p>
      )}
    </div>
  );
}

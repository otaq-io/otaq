// The comment box (CMT-11, d0109; *05 → Row · Ticket comments and a report* and *Row · Writing, editing and
// deleting*, `s05-41`…`s05-44`): at the end of the Ticket tab's thread, and inside the Send back, Fail and drag-back
// dialogs (T-0447), which give it their label and line (`CommentBoxView.dialog`) and send it with their own button.
// It holds the text and a list of attachments (tuq 001), each its own chip with ✕, added three ways (tuq 003): a paste
// (a screenshot becomes an image, files copied in Finder keep their names), files dropped on the box, and Attach (the
// system picker, several at once). Pasted text always goes into the text, however long (tuq 112). An image's chip
// shows a thumbnail (tuq 002); other files show name and size. A file over 20 MB is refused when it is added, by name,
// and the rest of the box stays (tuq 005; `attachmentAdding.ts`). Attachments alone may be sent (tuq 007). The box
// never blocks on length: no counter, no red box; only text that would not fit in one message to the service says so
// in words instead of being sent (`commentRules.ts`). Its words and the
// limits come from Rust (`CommentBoxView`); the box keeps only what is typed and added. ⌘↵ sends it as a Comment. While
// the ticket's agent or check runs, the thread's box has a second button, "Tell Strong now" (`CommentBoxView.tell_agent`):
// it posts the comment with `tell_agent`, so the running agent also gets it when idle; Comment never does. Editing a posted comment uses this box
// too (tuq 008): its attachments are `posted` chips with ✕, and the thread gives it Cancel · Save (`actions`).
//
// The text is Markdown, written with GitHub's helpers (tuq 111) while the box stays a plain textarea: Write and Preview
// tabs over it (Preview draws the text as the thread will, `CommentMarkdown`; Write brings the textarea back with its
// caret and selection where they were: the textarea only hides meanwhile, so its undo history stays too), and a toolbar
// whose buttons add Markdown marks (`markdownEdits.ts`). ⌘B, ⌘I, ⌘E and ⌘K (Ctrl on Windows and Linux) are Bold,
// Italic, Code and Link while the text has the focus: the page's key dispatcher leaves them to the box (`keysKeptBy`),
// and everywhere else ⌘K stays the command palette. Every edit goes through the textarea's own editing
// (`textareaEdit.ts`), so ⌘Z undoes it.
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

import { keymap } from "../../app/keyboard";
import { commandLetter } from "../../app/keys";
import { useStore } from "../../app/store";
import { Button, Kbd, PROSE_TEXT, Tabs } from "../../components";
import type { CommentBoxView } from "../../generated/CommentBoxView";
import type { CommentFormat } from "../../generated/CommentFormat";
import { CommentMarkdown, ToolButton } from "../../markdown";
import { attachmentImage, pickAttachmentFiles } from "../../tauri";
import { dropTargets } from "../../terminal/drops";
import { Icon } from "../../theme/Icon";
import { type Added, addFiles, addPaths, clipboardFiles, thumbnailOf } from "./attachmentAdding";
import {
  type BoxValue,
  boxState,
  type FileAttachment,
  type PostedAttachment,
  withAdded,
  withoutAttachment,
  withThumb,
} from "./commentRules";
import { isSaveKey, SAVE_KEYS } from "./logic";
import { formatEdit } from "./markdownEdits";
import { applyEdit } from "./textareaEdit";

/** Whether keys are ⌘ keys: the keymap's platform, or the browser's guess before it arrived (as the editor's toolbar). */
function useMac(): boolean {
  const map = useStore(keymap);
  return map ? map.platform === "mac" : navigator.userAgent.includes("Mac");
}

/** The Write and Preview tabs. */
const WRITE = 0;
const PREVIEW = 1;

/** "perf-notes.md 2 KB ✕", or an image's thumbnail before its name (tuq 002). */
export function FileChip({
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
  /** "Tell Strong now": the comment also goes to the running agent; shown only when the view has the button. */
  onTell?: () => void;
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
  onTell,
  actions,
  onCancel,
}: CommentBoxProps) {
  const id = useId();
  const state = boxState(value, view.limits, view.optional);
  const mac = useMac();
  const writing = view.writing;

  // The textarea, for the toolbar and the tabs, and for whoever passed `ref`.
  const text = useRef<HTMLTextAreaElement | null>(null);
  const setText = useCallback(
    (el: HTMLTextAreaElement | null) => {
      text.current = el;
      if (typeof ref === "function") ref(el);
      else if (ref) ref.current = el;
    },
    [ref],
  );
  const format = (f: CommentFormat) => {
    const el = text.current;
    if (!el) return;
    applyEdit(el, formatEdit(f, { text: el.value, start: el.selectionStart, end: el.selectionEnd }));
  };

  // Write / Preview. Preview hides the textarea (its caret, selection and undo stay) and Write focuses it again.
  const [tab, setTab] = useState(WRITE);
  /** The selection Write left, put back when it comes back. */
  const left = useRef<{ start: number; end: number; direction: "forward" | "backward" | "none" } | null>(
    null,
  );
  const selectTab = (next: number) => {
    const el = text.current;
    if (tab === WRITE && next !== WRITE && el) {
      left.current = {
        start: el.selectionStart,
        end: el.selectionEnd,
        direction: el.selectionDirection,
      };
    }
    setTab(next);
  };
  useLayoutEffect(() => {
    const was = left.current;
    const el = text.current;
    if (tab !== WRITE || !was || !el) return;
    left.current = null;
    el.focus();
    el.setSelectionRange(was.start, was.end, was.direction);
  }, [tab]);
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
      change(withAdded(latest.current, added));
      setRefused(why);
    },
    [change],
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
    if (!el) return;
    return dropTargets.register(el, {
      takesTyping: () => false,
      paste: () => undefined,
      takeFiles: addPathsToBox,
    });
  }, [addPathsToBox]);

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
    }
    // Text, however long, goes into the text as any paste does (tuq 112): the box doesn't read it.
  };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Escape" && onCancel) {
      e.preventDefault();
      e.stopPropagation();
      onCancel();
      return;
    }
    // ⌘B, ⌘I, ⌘E, ⌘K: the button whose shortcut it is (the dispatcher left these keys to the box, `keysKeptBy`).
    const letter = e.nativeEvent.isComposing ? null : commandLetter(e, mac);
    const button =
      letter === null ? undefined : writing.buttons.find((b) => b.keys?.slice(-1).toLowerCase() === letter);
    if (button) {
      e.preventDefault();
      e.stopPropagation();
      format(button.format);
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
    <div ref={root} className="comment-box">
      {view.label !== null && (
        <label className="comment-box-label" htmlFor={id}>
          {view.label}
        </label>
      )}
      <div className="comment-box-head">
        <Tabs
          label={writing.tabs}
          items={[
            { icon: "pencil", label: writing.write },
            { icon: "eye", label: writing.preview },
          ]}
          active={tab}
          onSelect={selectTab}
        />
        {tab === WRITE && (
          <div className="comment-box-tools" role="toolbar" aria-label={writing.toolbar}>
            {writing.buttons.map((b) => (
              <ToolButton
                key={b.format}
                label={b.label}
                icon={b.icon}
                keys={b.keys ?? undefined}
                onPress={() => {
                  format(b.format);
                }}
              />
            ))}
          </div>
        )}
      </div>
      <textarea
        {...PROSE_TEXT}
        ref={setText}
        id={id}
        className="text-input text-area comment-box-text"
        hidden={tab !== WRITE}
        value={value.text}
        placeholder={view.placeholder}
        aria-label={view.label ?? view.placeholder}
        onChange={(e) => {
          change({ ...value, text: e.target.value });
        }}
        onPaste={onPaste}
        onKeyDown={onKeyDown}
      />
      {tab === PREVIEW && (
        <div className="comment-box-preview" role="tabpanel" aria-label={writing.preview}>
          {value.text.trim() === "" ? (
            <p className="comment-box-nothing">{writing.nothing_to_preview}</p>
          ) : (
            <CommentMarkdown markdown={value.text} />
          )}
        </div>
      )}
      {value.attachments.length > 0 && (
        <div className="comment-box-chips">
          {value.attachments.map((a) => {
            const onRemove = () => {
              remove(a.id);
            };
            switch (a.kind) {
              case "file":
                return <FileChip key={a.id} file={a} view={view} onRemove={onRemove} />;
              case "posted":
                return <PostedChip key={a.id} ticket={ticket} posted={a} view={view} onRemove={onRemove} />;
            }
          })}
        </div>
      )}
      <div className="comment-box-actions">
        <Button
          variant="ghost"
          compact
          icon="paperclip"
          label={view.attach}
          onClick={() => {
            pickAttachmentFiles(view.attach_title).then(addPathsToBox, () => undefined);
          }}
        />
        <span className="editor-spacer" />
        {view.tell_agent !== null && onTell && (
          <Button
            variant="secondary"
            label={view.tell_agent}
            disabled={!state.canSubmit || busy}
            onClick={() => {
              if (state.canSubmit && !busy) onTell();
            }}
          />
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
      {state.tooLong !== null && (
        <p className="comment-box-error" role="alert">
          {state.tooLong}
        </p>
      )}
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

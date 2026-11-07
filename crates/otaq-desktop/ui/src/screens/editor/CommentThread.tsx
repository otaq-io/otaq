// The comments in the Ticket tab (CMT-10, CMT-11, d0109; *05 → Row · Ticket comments and a report* `s05-41` /
// `s05-42`, *Row · Writing, editing and deleting* `s05-43` / `s05-44`): "Comments · 4" under the description, newest
// first, each with who wrote it, in which stage and when, yours tinted with the accent; a report's chip with Open; the
// ⋯ menu (Edit in place, Copy text, Delete… with its dialog); the box above them. Everything it says comes from
// `ticket_comments.thread` (Rust); it reloads on `ticket_changed`, so a comment an agent posts shows at once. React
// keeps only the moment: the text being typed, which comment is edited, the open menu and dialog. Overview's
// Attachments (Add, a drop; RPT-02) hand it a file with `attachReport`: the box takes it as its chip and the keyboard.
import { useCallback, useEffect, useRef, useState } from "react";

import type { TicketProps } from "../../app/registry";
import { createStore, type Store, useStore } from "../../app/store";
import { AnchoredMenu, Button, ConfirmDialog, IconButton, OtaqMark, ToolAvatar } from "../../components";
import type { CommentAttachmentChip } from "../../generated/CommentAttachmentChip";
import type { CommentRow } from "../../generated/CommentRow";
import type { CommentThreadView } from "../../generated/CommentThreadView";
import { asRequestError, useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { CommentBox } from "./CommentBox";
import { Thumbnails } from "./ImageViewer";
import {
  type BoxAttachment,
  type BoxValue,
  boxState,
  editBox,
  editChanges,
  EMPTY_BOX,
  sentAttachments,
  withAdded,
} from "./commentRules";
import { openReport } from "./reportOpener";

/** A comment posted, edited or deleted changes the ticket; a session starting or ending changes the quiet line. */
export const COMMENT_EVENTS = ["ticket_changed", "ticket_deleted", "session_state"] as const;

const ignore = () => undefined;

/** Attachments for a ticket's comment box: files from Overview's Add, a drop or a paste (tuq 003). */
export interface AttachRequest {
  ticket: string;
  attachments: readonly BoxAttachment[];
  /** Counts up with each request, so the same file twice is two requests. */
  seq: number;
}

/** The newest file handed to a comment box; the thread of its ticket takes it, once. */
export const attachRequests: Store<AttachRequest | null> = createStore<AttachRequest | null>(null);

/** Opens `ticket`'s comment box with `attachments` added to its chips (RPT-02): sent with a line or alone (tuq 007). */
export function attachReport(
  ticket: string,
  attachments: AttachRequest["attachments"],
  store: Store<AttachRequest | null> = attachRequests,
): void {
  store.update((last) => ({ ticket, attachments, seq: (last?.seq ?? 0) + 1 }));
}

/** A comment author's mark: yours, an agent's tool avatar, or the plain agent mark (also the report view's poster). */
export function Mark({ row }: { row: Pick<CommentRow, "mark"> }) {
  switch (row.mark.kind) {
    case "you":
      return (
        <span className="comment-mark" data-you>
          <Icon name="user" size={11} />
        </span>
      );
    case "tool":
      return <ToolAvatar avatar={row.mark.avatar} />;
    case "agent":
      return (
        <span className="comment-mark">
          <Icon name="bot" size={11} />
        </span>
      );
    // Activity's rows of what happened on its own (Q432).
    case "otaq":
      return (
        <span className="comment-mark" data-otaq>
          <OtaqMark size={12} />
        </span>
      );
  }
}

/** The icon of an attachment's kind. */
function kindIcon(kind: CommentAttachmentChip["kind"]): "image" | "file-text" | "file" {
  switch (kind) {
    case "image":
      return "image";
    case "text":
      return "file-text";
    case "other":
      return "file";
  }
}

/** One attachment: "test-run.txt · 48 KB", and Open for text (images and other files don't open here yet). */
export function AttachmentChip({ chip, onOpen }: { chip: CommentAttachmentChip; onOpen?: () => void }) {
  return (
    <span className="comment-report" data-kind={chip.kind}>
      <span className="comment-chip">
        <span className="comment-chip-open" data-static>
          <Icon name={kindIcon(chip.kind)} size={13} />
          <span className="comment-chip-label">{chip.name}</span>
          <span className="comment-chip-size">{chip.size}</span>
        </span>
      </span>
      {onOpen && chip.open !== null && (
        <button type="button" className="comment-report-open" onClick={onOpen}>
          {chip.open}
        </button>
      )}
    </span>
  );
}

/** A comment's attachments, one chip each; `onOpen` opens attachment `a` (text only). */
export function AttachmentChips({
  chips,
  onOpen,
}: {
  chips: CommentAttachmentChip[];
  onOpen?: (attachment: number) => void;
}) {
  if (chips.length === 0) return null;
  return (
    <span className="comment-attachments">
      {chips.map((chip) => (
        <AttachmentChip
          key={chip.attachment}
          chip={chip}
          onOpen={
            onOpen &&
            (() => {
              onOpen(chip.attachment);
            })
          }
        />
      ))}
    </span>
  );
}

function MoreMenu({ view, onChoose }: { view: CommentThreadView; onChoose: (id: string) => void }) {
  const more = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  return (
    <>
      <IconButton
        ref={more}
        icon="ellipsis"
        label={view.menu_label}
        onClick={() => {
          setOpen((o) => !o);
        }}
      />
      {open && (
        <AnchoredMenu
          anchor={more}
          align="end"
          label={view.menu_label}
          items={view.menu}
          width={180}
          onChoose={(id) => {
            setOpen(false);
            onChoose(id);
          }}
          onClose={(how) => {
            setOpen(false);
            if (how === "key") more.current?.focus();
          }}
        />
      )}
    </>
  );
}

/**
 * Edit in place (tuq 008): the comment box with its text and its attachments as chips with ✕, taking new ones by
 * paste, drop and Attach; Cancel · Save; ⌘↵ saves, Esc cancels. Save sends the text, the attachments removed and the
 * new ones together. The limit holds as in the box, and it must keep text or an attachment (tuq 007).
 */
export function EditInPlace({
  view,
  row,
  onDone,
}: {
  view: CommentThreadView;
  row: CommentRow;
  onDone: () => void;
}) {
  const [value, setValue] = useState<BoxValue>(() => editBox(row));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const text = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    // Edit was just chosen for this comment.
    text.current?.focus();
  }, []);
  const state = boxState(value, view.box.limits);
  const save = () => {
    if (!state.canSubmit || busy) return;
    const { remove, attachments, changed } = editChanges(row, value);
    if (!changed) {
      onDone();
      return;
    }
    setBusy(true);
    request("ticket_comments.edit", { key: view.key, n: row.n, body: value.text, remove, attachments }).then(
      onDone,
      (e: unknown) => {
        setBusy(false);
        setError(asRequestError(e).message);
      },
    );
  };
  // The thread's box, without its own button, line and tick box: Cancel · Save instead.
  const box = { ...view.box, label: null, submit: null, footer: null, tell_agent: null, optional: false };
  return (
    <div className="comment-edit">
      <CommentBox
        ticket={view.key}
        view={box}
        value={value}
        onChange={(next) => {
          setValue(next);
          setError(null);
        }}
        onSubmit={save}
        busy={busy}
        error={error}
        ref={text}
        onCancel={onDone}
        actions={
          <>
            <Button variant="ghost" label={view.edit.cancel} onClick={onDone} />
            <Button label={view.edit.save} disabled={!state.canSubmit} loading={busy} onClick={save} />
          </>
        }
      />
    </div>
  );
}

function Comment({
  view,
  row,
  editing,
  onEdit,
  onEditDone,
  onDelete,
}: {
  view: CommentThreadView;
  row: CommentRow;
  editing: boolean;
  onEdit: () => void;
  onEditDone: () => void;
  onDelete: () => void;
}) {
  const choose = (id: string) => {
    switch (id) {
      case "edit":
        onEdit();
        break;
      case "copy":
        navigator.clipboard.writeText(row.body).catch(ignore);
        break;
      case "delete":
        onDelete();
        break;
      default:
        break;
    }
  };
  return (
    <article className="comment" data-mine={row.mine || undefined} aria-label={row.heading}>
      <div className="comment-head">
        <Mark row={row} />
        <span className="comment-author">{row.author}</span>
        <span className="comment-meta">{editing ? row.editing_meta : row.meta}</span>
        {row.edited !== null && !editing && <span className="comment-meta">{row.edited}</span>}
        <span className="editor-spacer" />
        {!editing && <MoreMenu view={view} onChoose={choose} />}
      </div>
      {editing ? (
        // Its attachments are the edit box's chips while it is edited (tuq 008).
        <EditInPlace view={view} row={row} onDone={onEditDone} />
      ) : (
        <>
          {row.body !== "" && <p className="comment-body">{row.body}</p>}
          <Thumbnails ticket={view.key} n={row.n} thumbnails={row.thumbnails} />
          <AttachmentChips
            chips={row.attachments}
            onOpen={(attachment) => {
              openReport({ kind: "comment", ticket: view.key, n: row.n, attachment });
            }}
          />
        </>
      )}
    </article>
  );
}

/** Delete…: the comment as it is, and what deleting means; Esc · Cancel · Delete. */
function DeleteDialog({
  view,
  row,
  onClose,
}: {
  view: CommentThreadView;
  row: CommentRow;
  onClose: () => void;
}) {
  const [error, setError] = useState<string | null>(null);
  return (
    <ConfirmDialog
      title={view.delete.title}
      destructive
      confirmLabel={view.delete.confirm}
      confirmIcon="trash-2"
      cancelLabel={view.delete.cancel}
      onCancel={onClose}
      onConfirm={() => {
        request("ticket_comments.delete", { key: view.key, n: row.n }).then(onClose, (e: unknown) => {
          setError(asRequestError(e).message);
        });
      }}
      body={
        <div className="comment-delete">
          <p className="comment-meta">{row.heading}</p>
          <div className="comment comment-quoted">
            {row.body !== "" && <p className="comment-body">{row.body}</p>}
            <Thumbnails ticket={view.key} n={row.n} thumbnails={row.thumbnails} />
            <AttachmentChips chips={row.attachments} />
          </div>
          <p className="comment-meta">{view.delete.note}</p>
          {error !== null && (
            <p className="comment-box-error" role="alert">
              {error}
            </p>
          )}
        </div>
      }
    />
  );
}

export function CommentThread({ ticket }: TicketProps) {
  const load = useCallback(
    () => request("ticket_comments.thread", { key: ticket }) as Promise<CommentThreadView>,
    [ticket],
  );
  const { view } = useServiceView(load, COMMENT_EVENTS);
  const [value, setValue] = useState<BoxValue>(EMPTY_BOX);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<number | null>(null);
  const [deleting, setDeleting] = useState<number | null>(null);
  // "Also tell the agent now" (d0123): ticked by default, as the user leaves it while the thread shows.
  const [tellAgent, setTellAgent] = useState(true);
  const box = useRef<HTMLTextAreaElement>(null);
  const attach = useStore(attachRequests);
  useEffect(() => {
    // Once the box shows: it takes the keyboard.
    if (attach?.ticket !== ticket || view?.key !== ticket) return;
    attachRequests.set(null);
    const { attachments } = attach;
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the file arrives from outside React (a store), once
    setValue((v) => withAdded(v, attachments));
    setError(null);
    box.current?.scrollIntoView({ block: "nearest" });
    box.current?.focus();
  }, [attach, ticket, view]);
  if (view?.key !== ticket) return null;
  const post = () => {
    setBusy(true);
    setError(null);
    request("ticket_comments.post", {
      key: ticket,
      body: value.text,
      attachments: sentAttachments(value),
      tell_agent: view.box.tell_agent !== null && tellAgent,
    }).then(
      () => {
        setBusy(false);
        setValue(EMPTY_BOX);
      },
      (e: unknown) => {
        setBusy(false);
        setError(asRequestError(e).message);
      },
    );
  };
  const toDelete = view.comments.find((c) => c.n === deleting);
  return (
    <section className="comments" aria-label={view.heading}>
      <div className="comments-head">
        <span className="editor-section-name">{view.heading.toUpperCase()}</span>
        <span className="editor-spacer" />
        <span className="comments-hint">{view.hint}</span>
      </div>
      <CommentBox
        ticket={ticket}
        view={view.box}
        value={value}
        onChange={(next) => {
          setValue(next);
          setError(null);
        }}
        onSubmit={post}
        busy={busy}
        error={error}
        ref={box}
        tellAgent={tellAgent}
        onTellAgent={setTellAgent}
      />
      {view.comments.map((row) => (
        <Comment
          key={row.n}
          view={view}
          row={row}
          editing={editing === row.n}
          onEdit={() => {
            setEditing(row.n);
          }}
          onEditDone={() => {
            setEditing(null);
          }}
          onDelete={() => {
            setDeleting(row.n);
          }}
        />
      ))}
      {toDelete && (
        <DeleteDialog
          view={view}
          row={toDelete}
          onClose={() => {
            setDeleting(null);
          }}
        />
      )}
    </section>
  );
}

// The comments in the Ticket tab (CMT-10, CMT-11, d0109; *05 → Row · Ticket comments and a report* `s05-41` /
// `s05-42`, *Row · Writing, editing and deleting* `s05-43` / `s05-44`): "Comments · 4" under the description, oldest
// first, each with who wrote it, in which stage and when, yours tinted with the accent; a report's chip with Open; the
// ⋯ menu (Edit in place, Copy text, Delete… with its dialog); the box at the end. Everything it says comes from
// `ticket_comments.thread` (Rust); it reloads on `ticket_changed`, so a comment an agent posts shows at once. React
// keeps only the moment: the text being typed, which comment is edited, the open menu and dialog. Overview's
// Attachments (Add, a drop; RPT-02) hand it a file with `attachReport`: the box takes it as its chip and the keyboard.
import { type KeyboardEvent, useCallback, useEffect, useRef, useState } from "react";

import type { TicketProps } from "../../app/registry";
import { createStore, type Store, useStore } from "../../app/store";
import { AnchoredMenu, Button, ConfirmDialog, IconButton, ToolAvatar, PROSE_TEXT } from "../../components";
import type { CommentReportChip } from "../../generated/CommentReportChip";
import type { CommentRow } from "../../generated/CommentRow";
import type { CommentThreadView } from "../../generated/CommentThreadView";
import { asRequestError, useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { CommentBox } from "./CommentBox";
import { type AttachedFile, type BoxValue, boxState, EMPTY_BOX, withFile, withPasted } from "./commentRules";
import { isSaveKey } from "./logic";
import { openReport } from "./reportOpener";

/** A comment posted, edited or deleted changes the ticket; a session starting or ending changes the quiet line. */
export const COMMENT_EVENTS = ["ticket_changed", "ticket_deleted", "session_state"] as const;

const ignore = () => undefined;

/** A report for a ticket's comment box: a file (Overview's Add or a drop) or a pasted file's text. */
export interface AttachRequest {
  ticket: string;
  report: { kind: "file"; file: AttachedFile } | { kind: "pasted"; text: string };
  /** Counts up with each request, so the same file twice is two requests. */
  seq: number;
}

/** The newest file handed to a comment box; the thread of its ticket takes it, once. */
export const attachRequests: Store<AttachRequest | null> = createStore<AttachRequest | null>(null);

/** Opens `ticket`'s comment box with `report` as its chip (RPT-02: a comment always has its own line). */
export function attachReport(
  ticket: string,
  report: AttachRequest["report"],
  store: Store<AttachRequest | null> = attachRequests,
): void {
  store.update((last) => ({ ticket, report, seq: (last?.seq ?? 0) + 1 }));
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
  }
}

/** "Report · 48 KB" and Open. */
export function ReportChip({ chip, onOpen }: { chip: CommentReportChip; onOpen?: () => void }) {
  return (
    <span className="comment-report">
      <span className="comment-chip">
        <span className="comment-chip-open" data-static>
          <Icon name="file-text" size={13} />
          <span className="comment-chip-label">{chip.label}</span>
          <span className="comment-chip-size">{chip.size}</span>
        </span>
      </span>
      {onOpen && (
        <button type="button" className="comment-report-open" onClick={onOpen}>
          {chip.open}
        </button>
      )}
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

/** Edit in place: the text, Cancel · Save; ⌘↵ saves, Esc cancels. The limit holds as in the box. */
function EditInPlace({
  view,
  row,
  onDone,
}: {
  view: CommentThreadView;
  row: CommentRow;
  onDone: () => void;
}) {
  const [text, setText] = useState(row.body);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const state = boxState({ text, pasted: null, file: null }, view.box.limits);
  const save = () => {
    if (!state.canSubmit || busy) return;
    if (text.trim() === row.body) {
      onDone();
      return;
    }
    setBusy(true);
    request("ticket_comments.edit", { key: view.key, n: row.n, body: text }).then(onDone, (e: unknown) => {
      setBusy(false);
      setError(asRequestError(e).message);
    });
  };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (isSaveKey(e)) {
      e.preventDefault();
      e.stopPropagation();
      save();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onDone();
    }
  };
  return (
    <div className="comment-edit" data-over={state.over || undefined}>
      <textarea
        {...PROSE_TEXT}
        className="text-input text-area comment-box-text"
        value={text}
        aria-label={row.heading}
        aria-invalid={state.over || undefined}
        // eslint-disable-next-line jsx-a11y/no-autofocus -- Edit was just chosen for this comment
        autoFocus
        onChange={(e) => {
          setText(e.target.value);
        }}
        onKeyDown={onKeyDown}
      />
      <div className="comment-box-actions">
        {state.counter !== null && (
          <span className="comment-box-counter" role="status">
            {state.counter}
          </span>
        )}
        <span className="editor-spacer" />
        <Button variant="ghost" label={view.edit.cancel} onClick={onDone} />
        <Button label={view.edit.save} disabled={!state.canSubmit} loading={busy} onClick={save} />
      </div>
      {error !== null && (
        <p className="comment-box-error" role="alert">
          {error}
        </p>
      )}
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
        <EditInPlace view={view} row={row} onDone={onEditDone} />
      ) : (
        <p className="comment-body">{row.body}</p>
      )}
      {row.report !== null && (
        <ReportChip
          chip={row.report}
          onOpen={() => {
            openReport({ kind: "comment", ticket: view.key, n: row.n });
          }}
        />
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
            <p className="comment-body">{row.body}</p>
            {row.report !== null && <ReportChip chip={row.report} />}
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
    const { report } = attach;
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the file arrives from outside React (a store), once
    setValue((v) => (report.kind === "file" ? withFile(v, report.file) : withPasted(v, report.text)));
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
      pasted: value.pasted?.text ?? null,
      file: value.file?.path ?? null,
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

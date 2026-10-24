// A diff's hunks with their line comments (T-0225, T-0468; REV-03, FTB-04, FTB-05; `s25-08`, `s25-09`): the lines
// highlighted by Rust (`diff_page.rs`), Unified or Split, in chunks the webview lays out only near the view; with
// comments allowed, a line's number turns into + on hover and opens the comment box under it; the line's thread sits
// under it ("You · on line 21 · not sent yet"). All changes, a file's Diff and the Approve dialog's "changed since you
// reviewed it" draw them.
import {
  type Dispatch,
  type ReactNode,
  type RefObject,
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";

import { Button, PROSE_TEXT } from "../../components";
import type { CodeSpan } from "../../generated/CodeSpan";
import type { DiffAnchor } from "../../generated/DiffAnchor";
import type { DiffHunkView } from "../../generated/DiffHunkView";
import type { DiffLineView } from "../../generated/DiffLineView";
import type { FilesComments } from "../../generated/FilesComments";
import type { FilesThread } from "../../generated/FilesThread";
import type { NewToast } from "../../generated/NewToast";
import { asRequestError } from "../../service";
import { request } from "../../tauri";
import {
  type Composer,
  type ComposerAction,
  canSave,
  chunks,
  composerKey,
  composerReducer,
  LINE_HEIGHT,
  rowAnchors,
  saveRequest,
  threadsByAnchor,
  writesNew,
  writing,
} from "./logic";

const ignore = () => undefined;

export function toastError(title: string, error: unknown) {
  const toast: NewToast = {
    level: "error",
    title,
    text: asRequestError(error).message,
    link: null,
    ticket: null,
  };
  request("notices.toast", toast).catch(ignore);
}

/** What the comment parts need, passed down unchanged. `comments` null: lines take no comments (a child's change on
 * top of an epic, the Approve dialog). */
export interface CommentContext {
  ticket: string;
  comments: FilesComments | undefined;
  threads: Map<string, FilesThread>;
  composer: Composer | null;
  dispatch: Dispatch<ComposerAction>;
  save: (text: string) => void;
}

/** The comment box's state for the open ticket: one box at a time across the tab; the bar hears of a new one. */
export function useComments(ticket: string, comments: FilesComments | undefined): CommentContext {
  const [composer, dispatch] = useReducer(composerReducer, null);
  // Comments can't be added any more (sent back, approved): the open box goes.
  useEffect(() => {
    if (comments && !comments.can_comment) dispatch({ kind: "cancel" });
  }, [comments]);
  const writes = writesNew(composer);
  useEffect(() => {
    writing.set(writes ? ticket : null);
    return () => {
      writing.update((w) => (w === ticket ? null : w));
    };
  }, [writes, ticket]);
  const save = useCallback(
    (text: string) => {
      if (composer === null || !canSave(composer, text)) return;
      const { method, params } = saveRequest(composer, text, ticket);
      dispatch({ kind: "saving" });
      request(method, params).then(
        () => {
          dispatch({ kind: "saved" });
        },
        (error: unknown) => {
          dispatch({ kind: "failed", message: asRequestError(error).message });
        },
      );
    },
    [composer, ticket],
  );
  const threads = useMemo(() => threadsByAnchor(comments), [comments]);
  return useMemo(
    () => ({ ticket, comments, threads, composer, dispatch, save }),
    [ticket, comments, threads, composer, save],
  );
}

function Code({ spans }: { spans: readonly CodeSpan[] }) {
  return (
    <span className="diff-code">
      {spans.map((s, i) =>
        s.token === "plain" ? (
          s.text
        ) : (
          <span key={i} className="tok" data-token={s.token}>
            {s.text}
          </span>
        ),
      )}
    </span>
  );
}

/** A code line of a file as it is (File): its number and its highlighted text; `bars` before the number (the project
 * Files view's bar per ticket); `marked`: the line a terminal's path link opened at (d0117 point 8). */
export function CodeLineRow({
  number,
  spans,
  bars,
  marked = false,
}: {
  number: number;
  spans: readonly CodeSpan[];
  bars?: ReactNode;
  marked?: boolean;
}) {
  return (
    <div className="diff-line" data-kind="context" data-line={number} data-marked={marked || undefined}>
      {bars}
      <span className="diff-gutter">
        <span className="diff-num">{number}</span>
      </span>
      <span className="diff-sign"> </span>
      <Code spans={spans} />
    </div>
  );
}

/**
 * Scrolls the line `line` of the File shown in `container` to the middle, once per `ask` (a new value each time the
 * line is asked for), as soon as the line is drawn.
 */
export function useScrollToLine(
  container: RefObject<HTMLElement | null>,
  line: number | null | undefined,
  ask: unknown,
  drawn: unknown,
): void {
  const done = useRef<unknown>(null);
  useEffect(() => {
    if (line == null || done.current === ask) return;
    const row = container.current?.querySelector(`.diff-line[data-line="${String(line)}"]`);
    if (!row) return;
    done.current = ask;
    row.scrollIntoView({ block: "center" });
  }, [container, line, ask, drawn]);
}

/** The line numbers; with comments allowed, a button that opens a comment box on the line (+ on hover). */
function Gutter({
  numbers,
  anchor,
  ctx,
}: {
  numbers: ReactNode;
  anchor: DiffAnchor | null;
  ctx: CommentContext | null;
}) {
  if (anchor === null || !ctx?.comments?.can_comment) return <span className="diff-gutter">{numbers}</span>;
  const thread = ctx.threads.get(anchor.key);
  const texts = ctx.comments.texts;
  return (
    <button
      type="button"
      className="diff-gutter"
      data-commentable
      aria-label={texts.add_on_line}
      title={texts.add_on_line}
      onClick={() => {
        ctx.dispatch({ kind: "open", anchor, replyTo: thread?.comments[0]?.id ?? null });
      }}
    >
      {numbers}
      <span className="diff-plus" aria-hidden>
        +
      </span>
    </button>
  );
}

function Num({ n }: { n: number | null }) {
  return <span className="diff-num">{n ?? ""}</span>;
}

function ComposerBox({ ctx, composer }: { ctx: CommentContext; composer: Composer }) {
  const texts = ctx.comments?.texts;
  const [text, setText] = useState(composer.initial);
  const input = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    input.current?.focus();
  }, []);
  if (!texts) return null;
  return (
    <div className="diff-composer">
      <textarea
        {...PROSE_TEXT}
        ref={input}
        className="text-input text-area diff-composer-input"
        rows={2}
        value={text}
        placeholder={texts.placeholder}
        aria-label={texts.placeholder}
        onChange={(e) => {
          setText(e.target.value);
        }}
        onKeyDown={(e) => {
          const what = composerKey(e);
          if (what === null) return;
          e.preventDefault();
          e.stopPropagation();
          if (what === "save") ctx.save(text);
          else ctx.dispatch({ kind: "cancel" });
        }}
      />
      {composer.error !== null && <p className="diff-composer-error">{composer.error}</p>}
      <div className="diff-composer-bar">
        <span className="diff-composer-hint">{texts.hint}</span>
        <Button
          compact
          variant="ghost"
          label={texts.cancel}
          onClick={() => {
            ctx.dispatch({ kind: "cancel" });
          }}
        />
        <Button
          compact
          label={composer.editing === null ? texts.add : texts.save}
          loading={composer.saving}
          disabled={!canSave(composer, text)}
          onClick={() => {
            ctx.save(text);
          }}
        />
      </div>
    </div>
  );
}

/** The comments on one line, and the comment box when it is open there (`s25-09`). */
function ThreadBlock({ anchor, ctx }: { anchor: DiffAnchor; ctx: CommentContext | null }) {
  if (!ctx) return null;
  const thread = ctx.threads.get(anchor.key);
  const composing = ctx.composer?.anchor.key === anchor.key ? ctx.composer : null;
  if (!thread && !composing) return null;
  const texts = ctx.comments?.texts;
  return (
    <div className="diff-thread">
      {thread?.comments.map((c) =>
        composing?.editing === c.id ? (
          <div key={c.id} className="diff-comment">
            <span className="diff-avatar" aria-hidden />
            <ComposerBox ctx={ctx} composer={composing} />
          </div>
        ) : (
          <div key={c.id} className="diff-comment" data-pending={c.pending || undefined}>
            <span className="diff-avatar" aria-hidden />
            <div className="diff-comment-main">
              <div className="diff-comment-head">
                <span className="diff-author">{c.author}</span>
                <span className="diff-meta">{c.meta}</span>
                <span className="diff-spacer" />
                {c.editable && texts && (
                  <>
                    <button
                      type="button"
                      className="diff-link"
                      onClick={() => {
                        ctx.dispatch({ kind: "edit", anchor, id: c.id, body: c.body });
                      }}
                    >
                      {texts.edit}
                    </button>
                    <button
                      type="button"
                      className="diff-link"
                      onClick={() => {
                        request("files.comment_delete", { id: c.id }).catch((e: unknown) => {
                          toastError(texts.delete, e);
                        });
                      }}
                    >
                      {texts.delete}
                    </button>
                  </>
                )}
              </div>
              <p className="diff-comment-body">{c.body}</p>
            </div>
          </div>
        ),
      )}
      {composing && composing.editing === null && (
        <div className="diff-comment" data-pending>
          <span className="diff-avatar" aria-hidden />
          <ComposerBox ctx={ctx} composer={composing} />
        </div>
      )}
      {!composing && thread && ctx.comments?.can_comment && texts && (
        <button
          type="button"
          className="diff-reply"
          onClick={() => {
            ctx.dispatch({ kind: "open", anchor, replyTo: thread.comments[0]?.id ?? null });
          }}
        >
          {texts.reply}
        </button>
      )}
    </div>
  );
}

function UnifiedLine({ line, ctx }: { line: DiffLineView; ctx: CommentContext | null }) {
  return (
    <>
      <div className="diff-line" data-kind={line.kind}>
        <Gutter
          anchor={line.anchor}
          ctx={ctx}
          numbers={
            <>
              <Num n={line.old} />
              <Num n={line.new} />
            </>
          }
        />
        <span className="diff-sign">{line.sign}</span>
        <Code spans={line.spans} />
      </div>
      {line.anchor && <ThreadBlock anchor={line.anchor} ctx={ctx} />}
    </>
  );
}

function Half({
  line,
  old,
  ctx,
}: {
  line: DiffLineView | undefined;
  old: boolean;
  ctx: CommentContext | null;
}) {
  if (!line) return <div className="diff-half" data-blank />;
  return (
    <div className="diff-half" data-kind={line.kind}>
      <Gutter anchor={line.anchor} ctx={ctx} numbers={<Num n={old ? line.old : line.new} />} />
      <span className="diff-sign">{line.sign}</span>
      <Code spans={line.spans} />
    </div>
  );
}

function SideRow({
  hunk,
  row,
  ctx,
}: {
  hunk: DiffHunkView;
  row: [number | null, number | null];
  ctx: CommentContext | null;
}) {
  const left = row[0] === null ? undefined : hunk.lines[row[0]];
  const right = row[1] === null ? undefined : hunk.lines[row[1]];
  return (
    <>
      <div className="diff-side-row">
        <Half line={left} old ctx={ctx} />
        <Half line={right} old={false} ctx={ctx} />
      </div>
      {rowAnchors(left?.anchor, right?.anchor).map((a) => (
        <ThreadBlock key={a.key} anchor={a} ctx={ctx} />
      ))}
    </>
  );
}

/** A hunk's rows, in chunks the webview lays out only near the view (`content-visibility`). */
export function Hunk({
  hunk,
  sideBySide,
  ctx,
}: {
  hunk: DiffHunkView;
  sideBySide: boolean;
  ctx: CommentContext | null;
}) {
  const count = sideBySide ? hunk.rows.length : hunk.lines.length;
  return (
    <>
      <div className="diff-hunk-header">{hunk.header}</div>
      {chunks(count).map(([start, end]) => (
        <div
          key={start}
          className="diff-chunk"
          style={{ containIntrinsicSize: `auto ${String((end - start) * LINE_HEIGHT)}px` }}
        >
          {sideBySide
            ? hunk.rows
                .slice(start, end)
                .map((row, i) => <SideRow key={start + i} hunk={hunk} row={row} ctx={ctx} />)
            : hunk.lines
                .slice(start, end)
                .map((line, i) => <UnifiedLine key={start + i} line={line} ctx={ctx} />)}
        </div>
      ))}
    </>
  );
}

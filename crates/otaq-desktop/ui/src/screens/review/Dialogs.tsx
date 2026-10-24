// The review dialogs (REV-04, REV-06, REV-08, GIT-03, GIT-05, UI-02; *07 · Diff, review, approve & send back*):
// Approve ▾ and Local merge in one overlay that walks its steps (the choice, changed since review, the confirmation
// with the squash message, Merging…, and the conflict / refused / uncommitted results), and Send back with the
// pending line comments and a comment for Dev in the comment box (CMT-13, `s07-20`: a long paste becomes its report,
// the limit applies). Their words come from `review.approve`, `review.local_merge` and
// `review.send_back_view`; Open PR and Push to PR hand over to T-0227's `open_pr` dialog. Change… on a recorded
// decision (DEC-06, T-0288) opens "Change this decision" (a Send back with the other option) on a ticket, or "Add a
// fix child" in an epic's review, from `review.change_view`.
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import type { OverlayProps } from "../../app/registry";
import { openTicket, selection, showTab } from "../../app/selection";
import { Button, CodeText, Dialog, Kbd, TextArea, TextInput, PLAIN_TEXT } from "../../components";
import type { ApproveView } from "../../generated/ApproveView";
import type { DecisionChangeParams } from "../../generated/DecisionChangeParams";
import type { DecisionChangeView } from "../../generated/DecisionChangeView";
import type { DecisionFixChildView } from "../../generated/DecisionFixChildView";
import type { DecisionMarkView } from "../../generated/DecisionMarkView";
import type { DecisionSendBackView } from "../../generated/DecisionSendBackView";
import type { DiffFileView } from "../../generated/DiffFileView";
import type { LocalMergeResult } from "../../generated/LocalMergeResult";
import type { PanelSent } from "../../generated/PanelSent";
import type { ReviewChanged } from "../../generated/ReviewChanged";
import type { SendBackParams } from "../../generated/SendBackParams";
import type { SendBackView } from "../../generated/SendBackView";
import { asRequestError, useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { CommentBox } from "../editor/CommentBox";
import { type BoxValue, boxState, EMPTY_BOX } from "../editor/commentRules";
import {
  afterApproveAnyway,
  afterChoice,
  afterMerge,
  CHANGED_FILES_SHOWN,
  firstStep,
  isSubmitKey,
  type Step,
} from "./logic";

const ignore = () => undefined;

function copy(text: string) {
  navigator.clipboard.writeText(text).catch(ignore);
}

/** Done: the agent has it; its terminal shows if the ticket is open. Asked: the unsent-text question (d0048). */
export function afterSend(key: string, sent: PanelSent, again: () => Promise<unknown>) {
  if (sent.outcome === "done") {
    if (selection.get().ticket === key) showTab("agent");
    return;
  }
  openOverlay("confirm", {
    title: sent.title,
    body: sent.body,
    confirmLabel: sent.clear,
    cancelLabel: sent.cancel,
    destructive: false,
    onConfirm: again,
  });
}

function Files({
  files,
  tone,
  icon,
}: {
  files: readonly string[];
  tone: "error" | "plain";
  icon?: "git-merge";
}) {
  return (
    <div className="review-files" data-tone={tone}>
      {files.map((f) => (
        <div key={f} className="review-file">
          {icon && <Icon name={icon} size={13} />}
          <span>{f}</span>
        </div>
      ))}
    </div>
  );
}

/** The lines of one file of the new commits. */
function NewCommitFile({ file }: { file: DiffFileView }) {
  return (
    <div className="review-commit-file">
      <div className="review-commit-file-name">{file.path}</div>
      {file.hunks.map((hunk, h) =>
        hunk.lines.map((line, i) => (
          <div key={`${String(h)}-${String(i)}`} className="review-commit-line" data-kind={line.kind}>
            <span className="review-commit-sign">{line.sign}</span>
            <span className="review-commit-code">{line.spans.map((s) => s.text).join("")}</span>
          </div>
        )),
      )}
    </div>
  );
}

/** *Approve · changed since review*: the new commits, and only their lines (also the PR dialog's, T-0227). */
export function ChangedBody({ ticket, changed }: { ticket: string; changed: ReviewChanged }) {
  const [files, setFiles] = useState<DiffFileView[]>([]);
  useEffect(() => {
    let current = true;
    const wanted = changed.files.slice(0, CHANGED_FILES_SHOWN);
    Promise.all(
      wanted.map((path) =>
        (
          request("files.diff", { key: ticket, path, since: changed.reviewed }) as Promise<DiffFileView>
        ).catch(() => null),
      ),
    ).then((loaded) => {
      if (current) setFiles(loaded.filter((f): f is DiffFileView => f !== null));
    }, ignore);
    return () => {
      current = false;
    };
  }, [ticket, changed]);
  return (
    <div className="review-body">
      {changed.commits.map((c) => (
        <div key={c.sha} className="review-commit">
          <span className="review-sha">{c.sha}</span>
          <span>{c.subject}</span>
        </div>
      ))}
      {files.map((f) => (
        <NewCommitFile key={f.path} file={f} />
      ))}
      <p className="review-muted">{changed.note}</p>
    </div>
  );
}

function MenuBody({
  view,
  onChoose,
}: {
  view: ApproveView;
  onChoose: (choice: ApproveView["menu"]["items"][number]["choice"]) => void;
}) {
  return (
    // The number keys pick, as on the board's drop (*03 → Approve choice on drop*).
    // eslint-disable-next-line jsx-a11y/no-static-element-interactions -- the keys belong to the buttons inside; this only reads their number shortcuts
    <div
      className="review-body"
      onKeyDown={(e) => {
        const item = view.menu.items.find((i) => String(i.number) === e.key);
        if (!item?.enabled) return;
        e.preventDefault();
        e.stopPropagation();
        onChoose(item.choice);
      }}
    >
      {view.menu.items.map((item) => (
        <button
          key={item.choice}
          type="button"
          className="review-choice"
          disabled={!item.enabled}
          onClick={() => {
            onChoose(item.choice);
          }}
        >
          <span className="review-choice-icon">
            <Icon name={item.icon} size={14} />
          </span>
          <span className="review-choice-text">
            <span className="review-choice-title">{item.title}</span>
            <span className="review-choice-detail">{item.detail}</span>
          </span>
          <Kbd keys={String(item.number)} />
        </button>
      ))}
      <p className="review-muted">{view.menu.note}</p>
    </div>
  );
}

function Callout({
  icon,
  tone,
  children,
}: {
  icon: Parameters<typeof Icon>[0]["name"];
  tone: string;
  children: ReactNode;
}) {
  return (
    <div className="review-callout" data-tone={tone}>
      <Icon name={icon} size={14} />
      <div>{children}</div>
    </div>
  );
}

export function LocalMergeDialog({ params, close }: OverlayProps<"local_merge">) {
  const ticket = params.ticket;
  const menu = params.menu ?? false;
  const load = useCallback(
    () => request("review.approve", { key: ticket }) as Promise<ApproveView>,
    [ticket],
  );
  const { view, error } = useServiceView(load, []);
  const [step, setStep] = useState<Step | null>(null);
  const [message, setMessage] = useState("");
  const [anyway, setAnyway] = useState(false);
  const started = useRef(false);

  useEffect(() => {
    if (!view || started.current) return;
    started.current = true;
    setMessage(view.confirm.message);
    setStep(firstStep(view, menu));
  }, [view, menu]);
  useEffect(() => {
    if (step?.kind === "open_pr") {
      close();
      // Approve anyway was this flow's click: Open PR mustn't ask it again (REV-06). Push to PR came without a choice,
      // so its dialog shows what goes to the PR first.
      openOverlay("open_pr", { ticket, approveAnyway: anyway, intro: view?.push_to_pr ?? false });
    } else if (step?.kind === "done") {
      close();
    }
  }, [step, close, ticket, anyway, view]);
  useEffect(() => {
    if (error && !view) close();
  }, [error, view, close]);

  const merge = (approveAnyway = anyway) => {
    setStep({ kind: "merging" });
    (
      request("review.local_merge", {
        key: ticket,
        message,
        approve_anyway: approveAnyway,
      }) as Promise<LocalMergeResult>
    ).then(
      (result) => {
        setStep(afterMerge(result));
      },
      // The refusal is a toast (Rust's).
      close,
    );
  };
  const toSendBack = () => {
    close();
    openOverlay("send_back", { ticket });
  };

  if (!view || !step) return null;
  switch (step.kind) {
    case "open_pr":
    case "done":
      return null;
    case "menu": {
      const first = view.menu.items[0];
      return (
        <Dialog
          title={view.menu.title}
          width={480}
          onConfirm={() => {
            if (first?.enabled) setStep(afterChoice(first.choice));
          }}
          onCancel={close}
          footer={
            <>
              <Button variant="ghost" label="Cancel" onClick={close} />
              {first && (
                <Button
                  label={first.title}
                  icon={first.icon}
                  disabled={!first.enabled}
                  onClick={() => {
                    setStep(afterChoice(first.choice));
                  }}
                />
              )}
            </>
          }
        >
          <MenuBody
            view={view}
            onChoose={(choice) => {
              setStep(afterChoice(choice));
            }}
          />
        </Dialog>
      );
    }
    case "changed": {
      const go = () => {
        setAnyway(true);
        const next = afterApproveAnyway(step, view, menu);
        if (next === "merge") merge(true);
        else setStep(next);
      };
      return (
        <Dialog
          title={step.changed.title}
          width={560}
          onConfirm={go}
          onCancel={close}
          footer={
            <>
              <Button variant="secondary" icon="undo-2" label={step.changed.send_back} onClick={toSendBack} />
              <Button icon="check" label={step.changed.anyway} onClick={go} />
            </>
          }
        >
          <ChangedBody ticket={ticket} changed={step.changed} />
        </Dialog>
      );
    }
    case "confirm": {
      const c = view.confirm;
      return (
        <Dialog
          title={c.title}
          width={480}
          hint={c.hint}
          onConfirm={() => {
            merge();
          }}
          onCancel={close}
          footer={
            <>
              <Button variant="ghost" label={c.cancel} onClick={close} />
              <Button
                icon="git-merge"
                label={c.merge}
                disabled={message.trim() === ""}
                onClick={() => {
                  merge();
                }}
              />
            </>
          }
        >
          <div className="review-body">
            <span className="review-label">{c.into}</span>
            <div className="review-target">
              <Icon name="git-branch" size={13} />
              <span>
                {c.target}
                {c.repo !== "" && `  ·  ${c.repo}`}
              </span>
              <span className="review-spacer" />
              {c.locked && <Icon name="lock" size={13} />}
            </div>
            <p className="review-muted review-merge-check">
              <CodeText text={c.merge_check.text} />
              {c.merge_check.link !== null && (
                <button
                  type="button"
                  className="review-link"
                  onClick={() => {
                    close();
                    // The Merge check row is in the project's Commands, next to the setup command (d0120).
                    openOverlay("settings", { project: c.merge_check.project, section: "commands" });
                  }}
                >
                  {c.merge_check.link}
                </button>
              )}
            </p>
            {c.conflict !== null && (
              <Callout icon="git-compare-arrows" tone="paused">
                {c.conflict}
              </Callout>
            )}
            <label className="review-label" htmlFor="review-message">
              {c.message_label}
            </label>
            <textarea
              {...PLAIN_TEXT}
              id="review-message"
              className="text-input text-area review-message"
              data-mono
              rows={5}
              value={message}
              onChange={(e) => {
                setMessage(e.target.value);
              }}
              onKeyDown={(e) => {
                if (!isSubmitKey(e)) return;
                e.preventDefault();
                e.stopPropagation();
                if (message.trim() !== "") merge();
              }}
            />
            <Callout icon="hourglass" tone="quiet">
              {c.note}
            </Callout>
          </div>
        </Dialog>
      );
    }
    case "merging":
      return (
        // It can be closed while it runs (d0120): the merge finishes on its own and its toasts say how it ended.
        <Dialog
          title={view.merging.title}
          width={480}
          onCancel={close}
          footer={
            <>
              <Button variant="ghost" label={view.merging.close} onClick={close} />
              <Button label={view.merging.label} loading disabled />
            </>
          }
        >
          <div className="review-body">
            {view.merging.steps.map((s, i) => (
              <div key={s} className="review-step" data-first={i === 0 || undefined}>
                <Icon name={i === 0 ? "loader-circle" : "circle"} size={14} />
                <span>{s}</span>
              </div>
            ))}
            <p className="review-muted">{view.merging.note}</p>
          </div>
        </Dialog>
      );
    case "result":
      return (
        <ResultDialog
          ticket={ticket}
          result={step.result}
          close={close}
          retry={merge}
          toSendBack={toSendBack}
        />
      );
  }
}

export function ResultDialog({
  ticket,
  result,
  close,
  retry,
  toSendBack,
}: {
  ticket: string;
  result: Extract<Step, { kind: "result" }>["result"];
  close: () => void;
  retry: () => void;
  toSendBack: () => void;
}) {
  switch (result.kind) {
    case "conflict": {
      // The agent fixes it with no click (FLW-17): the dialog only says so.
      return (
        <Dialog
          title={result.title}
          width={480}
          onConfirm={close}
          onCancel={close}
          footer={<Button label={result.close} onClick={close} />}
        >
          <div className="review-body" data-tone="error">
            <p className="review-text">{result.text}</p>
            <Files files={result.files} tone="error" icon="git-merge" />
            <p className="review-text">{result.help}</p>
            {result.worktree !== null && (
              <div className="review-path">
                <span>{result.worktree}</span>
                <button
                  type="button"
                  className="review-copy"
                  aria-label="Copy"
                  onClick={() => {
                    copy(result.worktree ?? "");
                  }}
                >
                  <Icon name="copy" size={13} />
                </button>
              </div>
            )}
          </div>
        </Dialog>
      );
    }
    case "refused":
      return (
        <Dialog
          title={result.title}
          width={480}
          onConfirm={retry}
          onCancel={close}
          footer={
            <>
              <Button variant="ghost" label={result.close} onClick={close} />
              <Button icon="rotate-ccw" label={result.retry} onClick={retry} />
            </>
          }
        >
          <div className="review-body">
            <Callout icon="triangle-alert" tone="paused">
              {result.message}
            </Callout>
          </div>
        </Dialog>
      );
    case "stopped":
      return (
        <Dialog
          title={result.title}
          width={480}
          onConfirm={close}
          onCancel={close}
          footer={<Button label={result.close} onClick={close} />}
        >
          <div className="review-body">
            <p className="review-text">{result.message}</p>
          </div>
        </Dialog>
      );
    case "uncommitted":
      return (
        <Dialog
          title={result.title}
          width={480}
          onConfirm={toSendBack}
          onCancel={close}
          footer={
            <>
              <Button variant="ghost" label={result.cancel} onClick={close} />
              <Button
                variant="secondary"
                icon="eye"
                label={result.view}
                onClick={() => {
                  close();
                  openTicket(ticket, "files");
                }}
              />
              <Button icon="undo-2" label={result.send_back} onClick={toSendBack} />
            </>
          }
        >
          <div className="review-body">
            <Files files={result.files} tone="plain" />
            <p className="review-text">{result.text}</p>
          </div>
        </Dialog>
      );
  }
}

export function SendBackDialog({ params, close }: OverlayProps<"send_back">) {
  const ticket = params.ticket;
  const load = useCallback(
    () => request("review.send_back_view", { key: ticket }) as Promise<SendBackView>,
    [ticket],
  );
  const { view, error } = useServiceView(load, []);
  const [comment, setComment] = useState<BoxValue>(EMPTY_BOX);
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  useEffect(() => {
    if (error && !view) close();
  }, [error, view, close]);
  if (!view) return null;
  const canSend = boxState(comment, view.comment.limits, view.comment.optional).canSubmit;
  // "Clear it and send" sends the same comment and pasted report again.
  const send = (clear: boolean): Promise<unknown> =>
    (
      request("review.send_back", {
        key: ticket,
        note: comment.text,
        report: comment.pasted?.text,
        clear,
        comments: view.comments.length,
      } satisfies SendBackParams) as Promise<PanelSent>
    ).then((sent) => {
      afterSend(ticket, sent, () => send(true));
    });
  const confirm = () => {
    if (busy || !canSend) return;
    setBusy(true);
    setRefusal(null);
    send(false).then(close, (e: unknown) => {
      setRefusal(asRequestError(e).message);
      setBusy(false);
    });
  };
  return (
    <Dialog
      title={view.title}
      width={520}
      hint={view.hint}
      onConfirm={confirm}
      onCancel={close}
      footer={
        <>
          <Button variant="ghost" label={view.cancel} onClick={close} />
          <Button icon="undo-2" label={view.confirm} loading={busy} disabled={!canSend} onClick={confirm} />
        </>
      }
    >
      <div className="review-body">
        <Callout icon="undo-2" tone="accent">
          {view.intro}
        </Callout>
        {view.pr && (
          <Callout icon="git-pull-request" tone="approved">
            <strong>{view.pr.title}</strong>
            <span className="review-callout-text">{view.pr.text}</span>
          </Callout>
        )}
        <span className="review-section">{view.receive_label}</span>
        {view.comments.length === 0 ? (
          <p className="review-muted">{view.no_comments}</p>
        ) : (
          <div className="review-comments">
            {view.comments.map((c, i) => (
              <div key={`${c.at}-${String(i)}`} className="review-comment">
                <span className="review-comment-at">{c.at}</span>
                <span>{c.body}</span>
              </div>
            ))}
          </div>
        )}
        <CommentBox
          ticket={ticket}
          view={view.comment}
          value={comment}
          onChange={setComment}
          onSubmit={confirm}
        />
        {view.waiting_title !== null && (
          <div className="review-waiting">
            <span className="review-waiting-title">{view.waiting_title}</span>
            {view.waiting.map((w) => (
              <div key={w.key} className="review-waiting-row">
                <Icon name="hourglass" size={13} />
                <span className="review-sha">{w.key}</span>
                <span>{w.text}</span>
              </div>
            ))}
          </div>
        )}
        {refusal !== null && <p className="review-error">{refusal}</p>}
      </div>
    </Dialog>
  );
}

/** Sends Change… (`review.change`); asks first over unsent text in the agent's terminal, then sends with `clear`. */
function sendChange(
  ticket: string,
  params: Omit<DecisionChangeParams, "clear">,
  clear = false,
): Promise<unknown> {
  return (request("review.change", { ...params, clear }) as Promise<PanelSent>).then((sent) => {
    afterSend(ticket, sent, () => sendChange(ticket, params, true));
  });
}

/** The decision the dialog changes: its question, Built and the other option, and the lead's mark. */
function DecisionSummary({
  question,
  summary,
  mark,
}: {
  question: string;
  summary: string;
  mark?: DecisionMarkView | null;
}) {
  return (
    <>
      <div className="review-decision">
        <span className="review-decision-question">{question}</span>
        <span className="review-decision-summary">{summary}</span>
      </div>
      {mark && (
        <div className="decision-mark">
          <Icon name="bot" size={13} />
          <span className="decision-mark-text">{mark.text}</span>
        </div>
      )}
    </>
  );
}

/** "Change this decision" (*07 → Change… on SHOP-23*): a Send back with the prefilled, editable text. */
export function ChangeSendBackDialog({ view, close }: { view: DecisionSendBackView; close: () => void }) {
  const [text, setText] = useState(view.text);
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const empty = text.trim() === "";
  const confirm = () => {
    if (busy || empty) return;
    setBusy(true);
    setRefusal(null);
    sendChange(view.key, { id: view.id, text, title: null, description: null, comments: view.comments }).then(
      close,
      (e: unknown) => {
        setRefusal(asRequestError(e).message);
        setBusy(false);
      },
    );
  };
  return (
    <Dialog
      title={view.title}
      width={480}
      hint={view.hint}
      onConfirm={confirm}
      onCancel={close}
      footer={
        <>
          <Button variant="ghost" label={view.cancel} onClick={close} />
          <Button icon="undo-2" label={view.confirm} loading={busy} disabled={empty} onClick={confirm} />
        </>
      }
    >
      <div className="review-body">
        <DecisionSummary question={view.question} summary={view.summary} />
        <label className="review-section" htmlFor="change-decision-text">
          {view.label}
        </label>
        <TextInput id="change-decision-text" value={text} onChange={setText} />
        <p className="review-muted">{view.help}</p>
        <Callout icon="undo-2" tone="quiet">
          {view.note}
        </Callout>
        {refusal !== null && <p className="review-error">{refusal}</p>}
      </div>
    </Dialog>
  );
}

/** "Add a fix child" (*07 → Change… on 40.1's decision*): a new child of the epic, title and description editable. */
export function AddFixChildDialog({ view, close }: { view: DecisionFixChildView; close: () => void }) {
  const [title, setTitle] = useState(view.child_title);
  const [description, setDescription] = useState(view.child_description);
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const empty = title.trim() === "";
  const confirm = () => {
    if (busy || empty) return;
    setBusy(true);
    setRefusal(null);
    sendChange(view.epic, { id: view.id, text: null, title, description, comments: 0 }).then(
      close,
      (e: unknown) => {
        setRefusal(asRequestError(e).message);
        setBusy(false);
      },
    );
  };
  return (
    <Dialog
      title={view.title}
      width={480}
      hint={view.hint}
      onConfirm={confirm}
      onCancel={close}
      footer={
        <>
          <Button variant="ghost" label={view.cancel} onClick={close} />
          <Button icon="plus" label={view.confirm} loading={busy} disabled={empty} onClick={confirm} />
        </>
      }
    >
      <div className="review-body">
        <DecisionSummary question={view.question} summary={view.summary} mark={view.mark} />
        <label className="review-section" htmlFor="fix-child-title">
          {view.title_label}
        </label>
        <TextInput id="fix-child-title" value={title} onChange={setTitle} />
        <TextArea value={description} onChange={setDescription} rows={4} label={view.description_label} />
        <p className="review-muted">{view.help}</p>
        <Callout icon="plus" tone="quiet">
          {view.note}
        </Callout>
        {refusal !== null && <p className="review-error">{refusal}</p>}
      </div>
    </Dialog>
  );
}

/** One of the two, by what Rust decided. */
export function ChangeDecisionBody({ view, close }: { view: DecisionChangeView; close: () => void }) {
  switch (view.kind) {
    case "send_back":
      return <ChangeSendBackDialog view={view} close={close} />;
    case "fix_child":
      return <AddFixChildDialog view={view} close={close} />;
  }
}

export function ChangeDecisionDialog({ params, close }: OverlayProps<"change_decision">) {
  const load = useCallback(
    () => request("review.change_view", { id: params.id }) as Promise<DecisionChangeView>,
    [params.id],
  );
  const { view, error } = useServiceView(load, []);
  useEffect(() => {
    if (error && !view) close();
  }, [error, view, close]);
  if (!view) return null;
  return <ChangeDecisionBody view={view} close={close} />;
}

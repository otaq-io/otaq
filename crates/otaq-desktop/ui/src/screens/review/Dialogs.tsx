// The review dialogs (REV-04, REV-06, REV-08, GIT-03, GIT-05, UI-02; *07 · Diff, review, approve & send back*):
// Approve ▾ and Local merge in one overlay that walks its steps (the choice, changed since review, the confirmation
// with the squash message, Merging…, and the conflict / refused / uncommitted results), and Send back with the
// pending line comments and a comment for Dev in the comment box (CMT-13, `s07-20`: a long paste becomes its report,
// the limit applies). Their words come from `review.approve`, `review.local_merge` and
// `review.send_back_view`; Open PR and Push to PR hand over to T-0227's `open_pr` dialog. Change… on a recorded
// decision (DEC-06, T-0288) opens "Change this decision" (a Send back with the other option), from
// `review.change_view`.
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { openProjectSettings } from "../project_settings/logic";
import type { OverlayProps } from "../../app/registry";
import { openTicket, selection, showTab } from "../../app/selection";
import { Button, CodeText, Dialog, Kbd, TextArea, PLAIN_TEXT } from "../../components";
import type { ApproveView } from "../../generated/ApproveView";
import type { DecisionChangeParams } from "../../generated/DecisionChangeParams";
import type { DecisionChangeView } from "../../generated/DecisionChangeView";
import type { DecisionSendBackView } from "../../generated/DecisionSendBackView";
import type { DiffFileView } from "../../generated/DiffFileView";
import type { DoneNotMergedView } from "../../generated/DoneNotMergedView";
import type { LocalMergeResult } from "../../generated/LocalMergeResult";
import type { SheetSent } from "../../generated/SheetSent";
import type { ReviewChanged } from "../../generated/ReviewChanged";
import type { SendBackParams } from "../../generated/SendBackParams";
import type { SendBackView } from "../../generated/SendBackView";
import { asRequestError, useEverySecond, useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { CommentBox } from "../editor/CommentBox";
import { GitIdentityFields, useGitIdentity } from "../settings/GitIdentity";
import { type BoxValue, boxState, EMPTY_BOX, sentAttachments } from "../editor/commentRules";
import {
  afterApproveAnyway,
  afterChoice,
  afterMerge,
  CHANGED_FILES_SHOWN,
  firstStep,
  isEdited,
  KEEP_AFTER_MS,
  messageAfter,
  type Step,
} from "./logic";
import { AnswerInTerminal } from "./WriterAsks";

const ignore = () => undefined;

function copy(text: string) {
  navigator.clipboard.writeText(text).catch(ignore);
}

/** Done: the agent has it; its terminal shows if the ticket is open. Asked: the unsent-text question (d0048). */
export function afterSend(key: string, sent: SheetSent, again: () => Promise<unknown>) {
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

/** A failed Merge check's output (its end when long), scrolled to its last line, where the failure usually is. */
function CheckOutput({ output }: { output: string }) {
  const box = useRef<HTMLPreElement>(null);
  useEffect(() => {
    if (box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [output]);
  return (
    <pre ref={box} className="review-output" data-tone="error">
      {output}
    </pre>
  );
}

/** The lines of one file of the new commits (also of what a hook changed in the commit that landed). */
export function NewCommitFile({ file }: { file: DiffFileView }) {
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
      {view.menu.delivery_note !== "" && <p className="review-muted">{view.menu.delivery_note}</p>}
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

/** "Done, nothing to merge": the ticket finishes at once; the toasts (Rust's) say how it ended, a refusal too. */
export function finishTicket(ticket: string, ask: typeof request = request): void {
  ask("review.done", { key: ticket }).catch(() => undefined);
}

/** "Done, don't merge", confirmed: the ticket finishes as Done; the toasts (Rust's) say how it ended, a refusal too. */
export function finishNotMerged(ticket: string, ask: typeof request = request): Promise<unknown> {
  return ask("review.done_not_merged", { key: ticket }).catch(() => undefined);
}

/** "Done, don't merge"'s confirmation: what won't land ("SHOP-18's 3 commits won't be merged; its branch is kept"),
 * from `review.done_not_merged_view`. */
export function DoneNotMergedDialog({ params, close }: OverlayProps<"done_not_merged">) {
  const ticket = params.ticket;
  const load = useCallback(
    () => request("review.done_not_merged_view", { key: ticket }) as Promise<DoneNotMergedView>,
    [ticket],
  );
  const { view, error } = useServiceView(load, []);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (error && !view) close();
  }, [error, view, close]);
  if (!view) return null;
  const confirm = () => {
    if (busy) return;
    setBusy(true);
    void finishNotMerged(ticket).then(close);
  };
  return (
    <Dialog
      title={view.title}
      width={480}
      onConfirm={confirm}
      onCancel={close}
      footer={
        <>
          <Button variant="ghost" label="Cancel" onClick={close} />
          <Button label={view.confirm} icon="circle-minus" disabled={busy} onClick={confirm} />
        </>
      }
    >
      <div className="review-body">
        <p>{view.message}</p>
        <p className="review-muted">{view.note}</p>
      </div>
    </Dialog>
  );
}

/** While the delivery's agent writes the message, the confirmation is asked again when its ticket changes (the agent's
 * `otaq pr message` says so). */
const MESSAGE_EVENTS = ["ticket_changed"] as const;

export function LocalMergeDialog({ params, close }: OverlayProps<"local_merge">) {
  const ticket = params.ticket;
  const menu = params.menu ?? false;
  const choice = params.choice;
  const [step, setStep] = useState<Step | null>(null);
  const [tick, setTick] = useState(0);
  const load = useCallback(
    () => request("review.approve", { key: ticket }) as Promise<ApproveView>,
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a new tick is a new load: the hook's time moves on
    [ticket, tick],
  );
  const { view, error } = useServiceView(load, MESSAGE_EVENTS);
  // While a hook runs during the merge its time grows: the view is asked again every second.
  useEverySecond(step?.kind === "merging" && view?.merging.hook != null, setTick);
  const [message, setMessage] = useState("");
  const [anyway, setAnyway] = useState(false);
  const started = useRef(false);
  /** The user typed in the message: the agent's arriving later doesn't replace it. */
  const typed = useRef(false);
  /** The confirmation as last shown, for the agent's message arriving. */
  const shown = useRef<ApproveView["confirm"] | null>(null);
  const messageBox = useRef<HTMLTextAreaElement>(null);
  /** The message as the user left it, not kept on the ticket yet (it is, a moment after the last key, and when the
   * dialog closes): Approve & merge opens on it again after a refusal, a close or a restart. */
  const unkept = useRef<string | null>(null);
  const keepTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const keepNow = useCallback(() => {
    if (keepTimer.current !== null) clearTimeout(keepTimer.current);
    keepTimer.current = null;
    if (unkept.current === null) return;
    request("review.keep_message", { key: ticket, message: unkept.current }).catch(ignore);
    unkept.current = null;
  }, [ticket]);
  const keepSoon = (text: string) => {
    unkept.current = text;
    if (keepTimer.current !== null) clearTimeout(keepTimer.current);
    keepTimer.current = setTimeout(keepNow, KEEP_AFTER_MS);
  };
  useEffect(() => keepNow, [keepNow]);

  useEffect(() => {
    if (!view) return;
    if (!started.current) {
      started.current = true;
      setMessage(view.confirm.message);
      // A kept edit is the user's typing: the agent's message arriving later doesn't replace it.
      typed.current = isEdited(view.confirm.message, view.confirm.proposed);
      setStep(firstStep(view, menu, choice));
    } else {
      const next = messageAfter(shown.current, view.confirm, typed.current);
      if (next !== null) setMessage(next);
    }
    shown.current = view.confirm;
  }, [view, menu, choice]);
  useEffect(() => {
    if (step?.kind === "open_pr") {
      close();
      // Approve anyway was this flow's click: Open PR mustn't ask it again (REV-06). Push to PR came without a choice,
      // so its dialog shows what goes to the PR first.
      openOverlay("open_pr", { ticket, approveAnyway: anyway, intro: view?.push_to_pr ?? false });
    } else if (step?.kind === "merge_into") {
      close();
      openOverlay("merge_into", { ticket });
    } else if (step?.kind === "done_not_merged") {
      close();
      openOverlay("done_not_merged", { ticket });
    } else if (step?.kind === "done") {
      close();
    } else if (step?.kind === "finish") {
      close();
      finishTicket(ticket);
    }
  }, [step, close, ticket, anyway, view]);
  useEffect(() => {
    if (error && !view) close();
  }, [error, view, close]);

  const merge = (approveAnyway = anyway) => {
    // The merge keeps the message it is given itself.
    if (keepTimer.current !== null) clearTimeout(keepTimer.current);
    unkept.current = null;
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
    case "merge_into":
    case "done_not_merged":
    case "done":
    case "finish":
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
        const next = afterApproveAnyway(step, view, menu, choice);
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
          // ⌘↵ merges from the message and from anywhere else in the dialog (dogfood #60).
          onSubmit={() => {
            if (message.trim() !== "") merge();
          }}
          // The message has the keyboard whenever the confirmation shows: from the start (not the merge-check link
          // above it), and after the choice or Approve anyway (the same dialog, the focus given again).
          initialFocus={messageBox}
          // The message may be edited: a press outside the box leaves it be (a stray one closed it with nothing
          // merged, which read as Merge doing nothing).
          keepOnOutsidePress
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
            <p className="review-muted review-lead">{c.lead}</p>
            {c.locked && (
              <div className="review-target">
                <Icon name="git-branch" size={13} />
                <span>
                  {c.target}
                  {c.repo !== "" && `  ·  ${c.repo}`}
                </span>
                <span className="review-spacer" />
                <Icon name="lock" size={13} />
              </div>
            )}
            <p className="review-muted review-merge-check">
              <CodeText text={c.merge_check.text} />
              {c.merge_check.link !== null && (
                <button
                  type="button"
                  className="review-link"
                  onClick={() => {
                    close();
                    // The Merge check row is on the project's Settings page, in its Repository card.
                    openProjectSettings(c.merge_check.project, "repository");
                  }}
                >
                  {c.merge_check.link}
                </button>
              )}
            </p>
            {c.manual_checks !== null && (
              <p className="review-muted review-manual-checks">
                <Icon name="list-checks" size={13} />
                <span>{c.manual_checks}</span>
              </p>
            )}
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
              ref={messageBox}
              id="review-message"
              className="text-input text-area review-message"
              data-mono
              rows={5}
              value={message}
              onChange={(e) => {
                typed.current = true;
                setMessage(e.target.value);
                keepSoon(e.target.value);
              }}
            />
            {isEdited(message, c.proposed) && (
              <button
                type="button"
                className="review-link review-use-proposed"
                onClick={() => {
                  // The proposal again, and nothing kept: the agent's message still fills in if it arrives.
                  typed.current = false;
                  setMessage(c.proposed);
                  keepSoon(c.proposed);
                  keepNow();
                }}
              >
                {c.use_proposed}
              </button>
            )}
            {c.writing !== null ? (
              <p className="review-muted review-writing">
                <Icon name={c.asks === null ? "loader-circle" : "square-terminal"} size={13} />
                <span>{c.writing}</span>
                {c.asks !== null && <AnswerInTerminal asks={c.asks} />}
              </p>
            ) : (
              c.prepared !== null && <p className="review-muted review-prepared">{c.prepared}</p>
            )}
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
            <MergingSteps merging={view.merging} />
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

/** The merge's steps: the one git is really on turns, the ones before it are done; while one of the user's hooks
 * runs, a line under it says which one and for how long ("running your pre-commit hook · 40 s"). */
export function MergingSteps({ merging }: { merging: ApproveView["merging"] }) {
  return (
    <>
      {merging.steps.map((s, i) => (
        <div key={s} className="review-step-row">
          <div
            className="review-step"
            data-current={i === merging.current || undefined}
            data-done={i < merging.current || undefined}
          >
            <Icon
              name={i === merging.current ? "loader-circle" : i < merging.current ? "circle-check" : "circle"}
              size={14}
            />
            <span>{s}</span>
          </div>
          {i === merging.current && merging.hook !== null && (
            // The running step's own line; its Stop belongs beside it.
            <div className="review-step-hook">
              <span>{merging.hook}</span>
            </div>
          )}
        </div>
      ))}
    </>
  );
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
            {result.files.length > 0 && <Files files={result.files} tone="error" icon="git-merge" />}
            {result.output !== null && <CheckOutput output={result.output} />}
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
    case "identity":
      return <IdentityDialog result={result} close={close} retry={retry} />;
  }
}

/** git has no name and email for the user: the two fields, and "Save and merge" saves them, then merges again. */
function IdentityDialog({
  result,
  close,
  retry,
}: {
  result: Extract<LocalMergeResult, { kind: "identity" }>;
  close: () => void;
  retry: () => void;
}) {
  const state = useGitIdentity(result.form);
  const saveAndMerge = () => {
    if (state.saving) return;
    void state.save().then((saved) => {
      if (saved) retry();
    });
  };
  return (
    <Dialog
      title={result.form.title}
      width={480}
      onConfirm={saveAndMerge}
      keepOnOutsidePress
      onCancel={close}
      footer={
        <>
          <Button variant="ghost" label={result.cancel} onClick={close} />
          <Button icon="git-merge" label={result.merge} loading={state.saving} onClick={saveAndMerge} />
        </>
      }
    >
      <div className="review-body">
        <p className="review-text">{result.form.text}</p>
        <GitIdentityFields form={result.form} state={state} />
      </div>
    </Dialog>
  );
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
        attachments: sentAttachments(comment),
        clear,
        comments: view.comments.length,
      } satisfies SendBackParams) as Promise<SheetSent>
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
        {view.footnote !== null && <p className="review-muted">{view.footnote}</p>}
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
  return (request("review.change", { ...params, clear }) as Promise<SheetSent>).then((sent) => {
    afterSend(ticket, sent, () => sendChange(ticket, params, true));
  });
}

/** The decision the dialog changes: its question, Built and the other option. */
function DecisionSummary({ question, summary }: { question: string; summary: string }) {
  return (
    <div className="review-decision">
      <span className="review-decision-question">{question}</span>
      <span className="review-decision-summary">{summary}</span>
    </div>
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
    sendChange(view.key, { id: view.id, text, comments: view.comments }).then(close, (e: unknown) => {
      setRefusal(asRequestError(e).message);
      setBusy(false);
    });
  };
  return (
    <Dialog
      title={view.title}
      width={480}
      hint={view.hint}
      onConfirm={confirm}
      onSubmit={confirm}
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
        {/* A sentence or two: it grows with the text instead of scrolling sideways (dogfood #27); ↵ is a new line,
            ⌘↵ sends. */}
        <TextArea id="change-decision-text" value={text} onChange={setText} rows={2} grow />
        <p className="review-muted">{view.help}</p>
        <Callout icon="undo-2" tone="quiet">
          {view.note}
        </Callout>
        {refusal !== null && <p className="review-error">{refusal}</p>}
      </div>
    </Dialog>
  );
}

/** The dialog, as Rust words it. */
export function ChangeDecisionBody({ view, close }: { view: DecisionChangeView; close: () => void }) {
  return <ChangeSendBackDialog view={view} close={close} />;
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

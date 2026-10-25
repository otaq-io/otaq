// The Open PR / Push to PR dialog (REV-10, REV-11, REV-12, REV-14, UI-07; *07 → Open PR · skill is preparing…,
// confirmation, pushing, invalid or taken branch name, not logged in, Approve · Push to PR #481, Push to PR ·
// confirmation*) and the PR box of an open PR (REV-13, *05 → SHOP-26 · Approved, PR open*).
// The dialog's state lives in Rust (`src/api/pr.rs`); this file keeps the fields as typed, asks the flow again while
// the skill works, and times the branch name's check. Nothing is pushed before the confirm click.
import { useCallback, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import type { OverlayProps, TicketProps } from "../../app/registry";
import { Button, Dialog, Field, TextArea, TextInput } from "../../components";
import type { PrAnswer } from "../../generated/PrAnswer";
import type { PrDialogView } from "../../generated/PrDialogView";
import type { PrOpenBox } from "../../generated/PrOpenBox";
import type { PrPushIntro } from "../../generated/PrPushIntro";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { ChangedBody, ResultDialog } from "../review/Dialogs";
import {
  checkDue,
  type Fields,
  fieldsOf,
  isStale,
  POLL_MS,
  polls,
  type Press,
  proposalArrived,
} from "./logic";

const ignore = () => undefined;

function copy(text: string) {
  navigator.clipboard.writeText(text).catch(ignore);
}

function CodeBox({ text }: { text: string }) {
  return (
    <div className="pr-code">
      <span>{text}</span>
      <button
        type="button"
        className="pr-copy"
        aria-label="Copy"
        onClick={() => {
          copy(text.replace(/^\$ /, ""));
        }}
      >
        <Icon name="copy" size={13} />
      </button>
    </div>
  );
}

/** The greyed fields while the skill prepares (*Open PR · skill is preparing…*). */
export function Waiting({ view }: { view: PrDialogView }) {
  const labels = view.push
    ? [view.branch_label, view.commits_label]
    : [view.branch_label, view.commits_label, view.title_label];
  return (
    <>
      {labels.map((label) => (
        <div key={label} className="pr-field">
          <span className="pr-label">{label}</span>
          <div className="pr-placeholder" />
        </div>
      ))}
    </>
  );
}

export function Proposed({
  view,
  fields,
  onEdit,
}: {
  view: PrDialogView;
  fields: Fields;
  onEdit: (field: "branch" | "title" | "description" | "commit", value: string, index?: number) => void;
}) {
  const hint = view.branch_hint;
  return (
    <>
      <p className="pr-muted">{view.proposed_by}</p>
      {view.provider !== null && (
        <div className="pr-provider">
          <Icon name="git-pull-request" size={12} />
          <span>{view.provider}</span>
          <span className="pr-spacer" />
          {view.login !== null && (
            <span className="pr-login">
              <Icon name="circle-check" size={12} />
              {view.login}
            </span>
          )}
        </div>
      )}
      <Field label={view.branch_label}>
        {(id) =>
          view.branch_locked ? (
            <div id={id} className="pr-locked">
              <Icon name="git-branch" size={12} />
              <span className="pr-spacer">{view.branch}</span>
              <Icon name="lock" size={12} />
            </div>
          ) : (
            <TextInput
              id={id}
              mono
              invalid={hint.error}
              value={fields.branch}
              onChange={(v) => {
                onEdit("branch", v);
              }}
            />
          )
        }
      </Field>
      <span className="pr-hint" data-error={hint.error || undefined}>
        {!view.branch_locked && <Icon name={hint.error ? "circle-x" : "circle-check"} size={12} />}
        {hint.text}
      </span>
      <span className="pr-label">{view.commits_label}</span>
      {fields.commits.map((message, i) => (
        <TextInput
          key={view.commits[i]?.sha ?? i}
          mono
          label={view.commits[i]?.sha ?? view.commits_label}
          value={message}
          onChange={(v) => {
            onEdit("commit", v, i);
          }}
        />
      ))}
      {!view.push && (
        <>
          <Field label={view.title_label}>
            {(id) => (
              <TextInput
                id={id}
                value={fields.title}
                onChange={(v) => {
                  onEdit("title", v);
                }}
              />
            )}
          </Field>
          <Field label={view.description_label}>
            {(id) => (
              <TextArea
                id={id}
                rows={6}
                value={fields.description}
                onChange={(v) => {
                  onEdit("description", v);
                }}
              />
            )}
          </Field>
        </>
      )}
      <span className="pr-target">
        <Icon name="git-merge" size={12} />
        {view.target_line}
      </span>
    </>
  );
}

export function Intro({
  intro,
  busy,
  onPush,
  onCancel,
}: {
  intro: PrPushIntro;
  busy: boolean;
  onPush: () => void;
  onCancel: () => void;
}) {
  return (
    <Dialog
      title={intro.title}
      width={460}
      onConfirm={onPush}
      onCancel={onCancel}
      footer={
        <>
          <Button variant="ghost" label={intro.cancel} onClick={onCancel} />
          <Button icon="upload" label={intro.confirm} loading={busy} onClick={onPush} />
        </>
      }
    >
      <div className="pr-body">
        <div className="pr-provider">
          <Icon name="git-pull-request" size={12} />
          <span>{intro.pr}</span>
          <span className="pr-spacer" />
          <span className="pr-mono">{intro.branch}</span>
        </div>
        <p className="pr-text">{intro.commits}</p>
        <p className="pr-muted">{intro.note}</p>
      </div>
    </Dialog>
  );
}

export function OpenPrDialog({ params, close }: OverlayProps<"open_pr">) {
  const key = params.ticket;
  const anyway = useRef(params.approveAnyway ?? false);
  const [answer, setAnswer] = useState<PrAnswer | null>(null);
  const [intro, setIntro] = useState<PrPushIntro | null>(null);
  const [fields, setFields] = useState<Fields>({ branch: "", commits: [], title: "", description: "" });
  const [busy, setBusy] = useState(false);
  const shown = useRef<PrDialogView | null>(null);
  const newestCheck = useRef(0);
  const asked = useRef(0);
  const opened = useRef(false);
  /** `apply` for the unsent-text question's later click. */
  const applied = useRef<(next: PrAnswer, press?: Press) => void>(ignore);

  const apply = useCallback(
    (next: PrAnswer, press?: Press) => {
      if (isStale(next, newestCheck.current)) return;
      switch (next.outcome) {
        case "closed":
          close();
          return;
        case "ask": {
          // The terminal has unsent text (d0048): Clear it and send repeats the press.
          const method = press === "confirm" ? "pr.confirm" : press === "retry" ? "pr.retry" : "pr.start";
          openOverlay("confirm", {
            title: next.unsent.title,
            body: next.unsent.body,
            confirmLabel: next.unsent.clear,
            cancelLabel: next.unsent.cancel,
            destructive: false,
            onConfirm: () =>
              (
                request(method, { key, approve_anyway: anyway.current, clear: true }) as Promise<PrAnswer>
              ).then((a) => {
                applied.current(a, press);
              }, ignore),
          });
          return;
        }
        case "dialog":
          newestCheck.current = next.view.check;
          if (proposalArrived(shown.current, next.view)) setFields(fieldsOf(next.view));
          shown.current = next.view;
          if (next.intro !== null) setIntro(next.intro);
          break;
        case "uncommitted":
        case "changed":
          break;
      }
      setAnswer(next);
    },
    [close, key],
  );

  useEffect(() => {
    applied.current = apply;
  }, [apply]);

  useEffect(() => {
    if (opened.current) return;
    opened.current = true;
    (
      request("pr.open", {
        key,
        approve_anyway: params.approveAnyway ?? false,
        intro: params.intro ?? false,
      }) as Promise<PrAnswer>
    ).then(
      (a) => {
        apply(a, "start");
      },
      // The refusal is a toast (Rust's).
      close,
    );
  }, [key, params.approveAnyway, params.intro, apply, close]);

  const view = answer?.outcome === "dialog" ? answer.view : null;
  // While the skill works, the flow is asked again every second.
  const polling = view !== null && intro === null && polls(view);
  useEffect(() => {
    if (!polling) return;
    const timer = setInterval(() => {
      (request("pr.view", { key }) as Promise<PrAnswer>).then(apply, ignore);
    }, POLL_MS);
    return () => {
      clearInterval(timer);
    };
  }, [polling, key, apply]);
  // The branch name is checked once it stayed the same for a moment.
  useEffect(() => {
    if (view === null || !checkDue(view, asked.current)) return;
    const check = view.check;
    const timer = setTimeout(() => {
      asked.current = check;
      (request("pr.check", { key, check }) as Promise<PrAnswer>).then(apply, ignore);
    }, view.check_after_ms);
    return () => {
      clearTimeout(timer);
    };
  }, [view, key, apply]);

  const run = (method: "pr.start" | "pr.confirm" | "pr.retry", press: Press) => {
    if (busy) return;
    setBusy(true);
    (request(method, { key, approve_anyway: anyway.current, clear: false }) as Promise<PrAnswer>)
      .then(
        (a) => {
          apply(a, press);
        },
        () => {
          if (press === "start") close();
        },
      )
      .finally(() => {
        setBusy(false);
      });
  };
  const cancel = () => {
    request("pr.cancel", { key }).then(close, close);
  };
  const onEdit = (field: "branch" | "title" | "description" | "commit", value: string, index = 0) => {
    setFields((f) =>
      field === "commit"
        ? { ...f, commits: f.commits.map((m, i) => (i === index ? value : m)) }
        : { ...f, [field]: value },
    );
    (request("pr.edit", { key, field, index, value }) as Promise<PrAnswer>).then(apply, ignore);
  };
  const toSendBack = () => {
    request("pr.cancel", { key }).catch(ignore);
    close();
    openOverlay("send_back", { ticket: key });
  };

  if (answer === null) return null;
  if (answer.outcome === "uncommitted") {
    if (answer.result.kind !== "uncommitted") return null;
    return (
      <ResultDialog
        ticket={key}
        result={answer.result}
        close={close}
        retry={() => {
          run("pr.start", "start");
        }}
        toSendBack={() => {
          close();
          openOverlay("send_back", { ticket: key });
        }}
      />
    );
  }
  if (answer.outcome === "changed") {
    const go = () => {
      anyway.current = true;
      run("pr.start", "start");
    };
    return (
      <Dialog
        title={answer.changed.title}
        width={560}
        onConfirm={go}
        onCancel={cancel}
        footer={
          <>
            <Button variant="secondary" icon="undo-2" label={answer.changed.send_back} onClick={toSendBack} />
            <Button icon="check" label={answer.changed.anyway} loading={busy} onClick={go} />
          </>
        }
      >
        <ChangedBody ticket={key} changed={answer.changed} />
      </Dialog>
    );
  }
  if (view === null) return null;
  if (intro !== null) {
    return (
      <Intro
        intro={intro}
        busy={busy}
        onCancel={cancel}
        onPush={() => {
          setIntro(null);
          run("pr.start", "start");
        }}
      />
    );
  }
  const stage = view.stage;
  switch (stage.kind) {
    case "not_logged_in":
      return (
        <Dialog
          title={stage.title}
          width={420}
          destructive
          onConfirm={() => {
            run("pr.retry", "retry");
          }}
          onCancel={cancel}
          footer={
            <>
              <Button variant="ghost" label={view.cancel} onClick={cancel} />
              <Button
                icon="rotate-ccw"
                label={stage.retry}
                loading={busy}
                onClick={() => {
                  run("pr.retry", "retry");
                }}
              />
            </>
          }
        >
          <div className="pr-body" data-tone="error">
            <p className="pr-text">{stage.text}</p>
            <CodeBox text={stage.login} />
            <CodeBox text={stage.azure} />
          </div>
        </Dialog>
      );
    case "reported":
      return (
        <Dialog
          title={stage.title}
          width={480}
          destructive
          onConfirm={() => {
            run("pr.retry", "retry");
          }}
          onCancel={cancel}
          footer={
            <>
              <Button variant="ghost" label={view.cancel} onClick={cancel} />
              <Button
                icon="rotate-ccw"
                label={stage.retry}
                loading={busy}
                onClick={() => {
                  run("pr.retry", "retry");
                }}
              />
            </>
          }
        >
          <div className="pr-body" data-tone="error">
            <CodeBox text={stage.message} />
            <p className="pr-text">{stage.text}</p>
          </div>
        </Dialog>
      );
    case "pushing":
      return (
        <Dialog
          title={stage.title}
          width={480}
          onCancel={ignore}
          footer={<Button variant="ghost" label={view.cancel} disabled />}
        >
          <div className="pr-body">
            {stage.steps.map((s) => (
              <div key={s.text} className="pr-step" data-state={s.state}>
                <Icon
                  name={
                    s.state === "done" ? "circle-check" : s.state === "running" ? "loader-circle" : "circle"
                  }
                  size={14}
                />
                <span>{s.text}</span>
              </div>
            ))}
          </div>
        </Dialog>
      );
    case "preparing":
    case "proposed": {
      const confirm = () => {
        if (view.can_confirm) run("pr.confirm", "confirm");
      };
      return (
        <Dialog
          title={view.heading}
          width={stage.kind === "proposed" ? 560 : 520}
          hint={view.hint}
          onConfirm={confirm}
          onCancel={cancel}
          footer={
            <>
              <Button variant="ghost" label={view.cancel} onClick={cancel} />
              <Button
                icon="upload"
                label={view.confirm}
                loading={busy}
                disabled={!view.can_confirm}
                onClick={confirm}
              />
            </>
          }
        >
          <div className="pr-body">
            {stage.kind === "preparing" ? (
              <>
                <div className="pr-preparing">
                  <Icon name="loader-circle" size={14} />
                  <div>
                    <span>{stage.line}</span>
                    <span className="pr-mono pr-muted">{stage.detail}</span>
                  </div>
                </div>
                <Waiting view={view} />
              </>
            ) : (
              <Proposed view={view} fields={fields} onEdit={onEdit} />
            )}
          </div>
        </Dialog>
      );
    }
  }
}

/** The PR box under the tabs while the ticket's PR is open (REV-13): Otaq doesn't watch it. */
export function PrOpenSection({ ticket }: TicketProps) {
  const load = useCallback(
    () => request("pr.open_box", { key: ticket }) as Promise<PrOpenBox | null>,
    [ticket],
  );
  const { view } = useServiceView(load, ["ticket_changed"]);
  if (!view) return null;
  return (
    <OpenBox
      view={view}
      onOpen={() => {
        request("sheet.action", { key: ticket, action: "open_pr_in_browser" }).catch(ignore);
      }}
    />
  );
}

export function OpenBox({ view, onOpen }: { view: PrOpenBox; onOpen: () => void }) {
  return (
    <section className="pr-open">
      <div className="pr-open-head">
        <Icon name="git-pull-request" size={13} />
        {view.url === null ? (
          <span className="pr-open-label">{view.label}</span>
        ) : (
          <button type="button" className="pr-open-label pr-link" onClick={onOpen}>
            {view.label}
          </button>
        )}
        <span className="pr-spacer" />
        {view.opened !== null && <span className="pr-muted">{view.opened}</span>}
      </div>
      <span className="pr-mono pr-secondary">{view.branch}</span>
      <div className="pr-open-note">
        <Icon name="info" size={12} />
        <span>{view.note}</span>
      </div>
    </section>
  );
}

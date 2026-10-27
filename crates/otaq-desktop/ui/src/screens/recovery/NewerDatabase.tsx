// A board from a newer Otaq (*32 → Item 7*; UPD-08, d0140 design audit AT): while the service refuses a database a
// newer build changed, the main area shows why instead of the board, with Restore the copy from before 0.2.0 (its one
// confirmation), Download 0.2.0, Try again and the command for a terminal; then the restore's three steps while the
// service is down. What it says is Rust's (`otaq-view`'s `newer_database.rs`); the steps arrive as `connection`
// events. React keeps only whether the confirmation is open and whether a click is being answered.
import { type ComponentType, type ReactNode, useState } from "react";

import { Button, ConfirmDialog } from "../../components";
import type { NewerDatabaseMessage } from "../../generated/NewerDatabaseMessage";
import type { NewerDatabaseScreen } from "../../generated/NewerDatabaseScreen";
import type { RestoringView } from "../../generated/RestoringView";
import type { StepState } from "../../generated/StepState";
import { useServiceConnection } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";

export interface NewerDatabaseActions {
  restore: () => Promise<unknown>;
  download: () => Promise<unknown>;
  tryAgain: () => Promise<unknown>;
}

const SERVICE_ACTIONS: NewerDatabaseActions = {
  restore: () => request("service.restore_database"),
  download: () => request("service.download_release"),
  tryAgain: () => request("service.restart"),
};

const STEP_ICON = { done: "check", current: "loader-circle", waiting: "circle" } as const satisfies Record<
  StepState,
  string
>;

export function RestoringBody({ view }: { view: RestoringView }) {
  return (
    <div className="newer-db" data-kind="restoring" aria-live="polite">
      <div className="newer-db-icon" data-tone="working">
        <Icon name={view.icon} size={26} />
      </div>
      <h2 className="newer-db-title">{view.title}</h2>
      <p className="newer-db-body">{view.note}</p>
      <ol className="newer-db-steps">
        {view.steps.map((step) => (
          <li key={step.label} data-state={step.state} data-loading={step.state === "current" || undefined}>
            <Icon name={STEP_ICON[step.state]} size={13} />
            <span>{step.label}</span>
          </li>
        ))}
      </ol>
    </div>
  );
}

interface MessageProps {
  view: NewerDatabaseMessage;
  /** A click is being answered: the buttons wait. */
  busy: boolean;
  onRestore: () => void;
  onDownload: () => void;
  onTryAgain: () => void;
}

export function MessageBody({ view, busy, onRestore, onDownload, onTryAgain }: MessageProps) {
  return (
    <div className="newer-db" data-kind="message">
      <div className="newer-db-icon" data-tone="paused">
        <Icon name={view.icon} size={26} />
      </div>
      <h2 className="newer-db-title">{view.title}</h2>
      <p className="newer-db-body">{view.body}</p>
      {view.failure !== null && (
        <p className="newer-db-failure" role="alert">
          {view.failure}
        </p>
      )}
      <div className="newer-db-actions">
        <Button variant="ghost" icon="external-link" label={view.download} onClick={onDownload} />
        <Button variant="secondary" label={view.try_again} disabled={busy} onClick={onTryAgain} />
        {view.restore !== null && (
          <Button variant="primary" icon="history" label={view.restore} disabled={busy} onClick={onRestore} />
        )}
      </div>
      {view.terminal !== null && (
        <p className="newer-db-terminal">
          <span>{view.terminal.lead}</span> <code>{view.terminal.command}</code>
        </p>
      )}
    </div>
  );
}

/** The screen for `screen`, with its confirmation; `actions` are the service's requests (a stand-in in tests). */
export function NewerDatabasePage({
  screen,
  actions = SERVICE_ACTIONS,
}: {
  screen: NewerDatabaseScreen;
  actions?: NewerDatabaseActions;
}) {
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  if (screen.kind === "restoring") return <RestoringBody view={screen} />;
  const run = (action: () => Promise<unknown>) => {
    setBusy(true);
    // How it went shows as the next screen (a failure says why on it), so the answer itself is not shown.
    action().then(
      () => {
        setBusy(false);
      },
      () => {
        setBusy(false);
      },
    );
  };
  const confirm = screen.confirm;
  return (
    <>
      <MessageBody
        view={screen}
        busy={busy}
        onRestore={() => {
          setConfirming(true);
        }}
        onDownload={() => {
          actions.download().catch(() => undefined);
        }}
        onTryAgain={() => {
          run(actions.tryAgain);
        }}
      />
      {confirming && confirm !== null && (
        <ConfirmDialog
          title={confirm.title}
          width={520}
          body={<ConfirmBody what={confirm.what} untouched={confirm.untouched} />}
          confirmLabel={confirm.restore}
          cancelLabel={confirm.cancel}
          hint={confirm.cancel}
          onConfirm={() => {
            setConfirming(false);
            run(actions.restore);
          }}
          onCancel={() => {
            setConfirming(false);
          }}
        />
      )}
    </>
  );
}

export function ConfirmBody({ what, untouched }: { what: string; untouched: string }): ReactNode {
  return (
    <>
      <p className="dialog-text">{what}</p>
      <p className="newer-db-untouched">{untouched}</p>
    </>
  );
}

/** The board, or this screen while the service refuses a newer database. */
export function withNewerDatabase(Board: ComponentType): ComponentType {
  return function BoardOrNewerDatabase() {
    const connection = useServiceConnection();
    if (connection?.state === "newer_database") return <NewerDatabasePage screen={connection.screen} />;
    return <Board />;
  };
}

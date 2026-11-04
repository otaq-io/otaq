// The run-command dialog (RUN-02; *13 · Run & Shell → 1 · Empty, 2 · Claude thinking, 3 · Suggestion, 4 · Editing,
// 5 · Saved*): a project's first Run asks for the command that starts it, with an optional Open URL. Ask the agent is
// a click (P1) that asks the project's Project agent (d0086); its progress arrives as `run_suggestion` events, which go back to Rust for the dialog's state. Save
// stores both in the project's settings (never the repo) and starts the ticket's Run.
import { useCallback, useEffect, useRef, useState } from "react";

import type { OverlayProps } from "../../app/registry";
import { Button, Dialog, TextInput, Tooltip } from "../../components";
import type { RunCommandOpened } from "../../generated/RunCommandOpened";
import type { RunCommandView } from "../../generated/RunCommandView";
import { pageEvents } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { suggestionFor } from "./logic";

const ignore = () => undefined;

export function RunCommandBody({
  view,
  command,
  openUrl,
  onCommand,
  onOpenUrl,
  onAsk,
}: {
  view: RunCommandView;
  command: string;
  openUrl: string;
  onCommand: (value: string) => void;
  onOpenUrl: (value: string) => void;
  onAsk: () => void;
}) {
  const saved = view.saved !== null;
  return (
    <div className="run-command-body">
      {!saved && <p className="run-message">{view.intro}</p>}
      <div className="run-field" data-locked={saved || undefined}>
        <span className="run-prefix">$</span>
        <TextInput
          mono
          label={view.title}
          placeholder={view.command_placeholder}
          value={command}
          disabled={saved}
          onChange={onCommand}
        />
      </div>
      <label className="run-label" htmlFor="run-open-url">
        {view.url_label}
        <span className="run-muted">{view.optional}</span>
      </label>
      <div className="run-field" data-locked={saved || undefined}>
        <Icon name="external-link" size={12} />
        <TextInput
          id="run-open-url"
          mono
          placeholder={view.url_placeholder}
          value={openUrl}
          disabled={saved}
          onChange={onOpenUrl}
        />
      </div>
      <span className="run-hint">{view.url_hint}</span>
      {view.ask !== null && (
        <div className="run-ask">
          <Tooltip text={view.ask.tooltip}>
            <Button variant="secondary" icon="sparkles" label={view.ask.label} onClick={onAsk} />
          </Tooltip>
          <span className="run-muted">{view.ask.note}</span>
        </div>
      )}
      {view.thinking !== null && (
        <div className="run-thinking">
          <span className="run-thinking-title">
            <Icon name="loader-circle" size={13} />
            {view.thinking.title}
          </span>
          {view.thinking.lines.map((l, i) => (
            <span key={`${String(i)}-${l}`} className="run-mono run-muted">
              {l}
            </span>
          ))}
        </div>
      )}
      {view.why !== null && (
        <div className="run-why">
          <Icon name="sparkles" size={13} />
          <span>{view.why}</span>
        </div>
      )}
      {view.saved !== null && (
        <div className="run-saved">
          <Icon name="circle-check" size={14} />
          <span>{view.saved}</span>
        </div>
      )}
      {view.error !== null && <p className="run-error">{view.error}</p>}
    </div>
  );
}

export function RunCommandDialog({ params, close }: OverlayProps<"run_command">) {
  const project = params.project;
  const [view, setView] = useState<RunCommandView | null>(null);
  const [command, setCommand] = useState("");
  const [openUrl, setOpenUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const typed = useRef("");
  const opened = useRef(false);

  const show = useCallback((next: RunCommandView) => {
    if (next.fill !== null) {
      typed.current = next.fill.command;
      setCommand(next.fill.command);
      setOpenUrl(next.fill.open_url);
    }
    setView(next);
  }, []);
  const ask = useCallback(() => {
    (request("run.command_ask", { project, command: typed.current }) as Promise<RunCommandView>).then(
      show,
      ignore,
    );
  }, [project, show]);

  useEffect(() => {
    if (opened.current) return;
    opened.current = true;
    (
      request("run.command_open", {
        project,
        ticket: params.ticket ?? null,
        app: params.app ?? null,
      }) as Promise<RunCommandOpened>
    ).then((o) => {
      typed.current = o.command;
      setCommand(o.command);
      setOpenUrl(o.open_url);
      setView(o.view);
      // Settings' Ask the agent was the click (P1).
      if (params.ask === true) ask();
    }, close);
  }, [project, params.ticket, params.app, params.ask, ask, close]);
  // Ask the agent's progress and answer go back to Rust, which keeps the dialog's state.
  useEffect(() => {
    const listening = pageEvents.listen((event) => {
      const update = suggestionFor(event, project);
      if (update === undefined) return;
      (
        request("run.command_suggestion", {
          project,
          command: typed.current,
          update,
        }) as Promise<RunCommandView>
      ).then(show, ignore);
    });
    return listening.stop;
  }, [project, show]);

  const dismiss = () => {
    request("run.command_cancel", { project, command: typed.current }).then(close, close);
  };
  const save = () => {
    if (busy || !view?.can_save) return;
    setBusy(true);
    (
      request("run.command_save", {
        project,
        command,
        open_url: openUrl,
        app: params.app ?? null,
      }) as Promise<RunCommandView>
    )
      .then(show, ignore)
      .finally(() => {
        setBusy(false);
      });
  };
  if (view === null) return null;
  const saved = view.saved !== null;
  return (
    <Dialog
      title={view.title}
      width={400}
      onConfirm={saved ? dismiss : save}
      onCancel={dismiss}
      footer={
        saved ? (
          <Button label={view.done} onClick={dismiss} />
        ) : (
          <>
            <Button variant="ghost" label={view.cancel} onClick={dismiss} />
            <Button label={view.save} loading={busy} disabled={!view.can_save} onClick={save} />
          </>
        )
      }
    >
      <RunCommandBody
        view={view}
        command={command}
        openUrl={openUrl}
        onCommand={(value) => {
          typed.current = value;
          setCommand(value);
          (request("run.command_edit", { project, command: value }) as Promise<RunCommandView>).then(
            show,
            ignore,
          );
        }}
        onOpenUrl={setOpenUrl}
        onAsk={ask}
      />
    </Dialog>
  );
}

// Session's Run (SHT-04, RUN-01, RUN-03; canvas `s25-06`, *13 · Run & Shell → Not started … Stopped by restart, Run ·
// backend*; Q433): one line after Session's switch, "pnpm dev · in SHOP-22’s worktree · :4022 · Serving", with Open URL
// (only when the project sets one), Run or Stop, and Edit command under its ⋯. No state card: a failure's reason sits
// under the line, and the Run's output below takes the rest, in T-0216's terminal, read-only. Nothing starts without
// a click.
import { type ReactNode, useCallback, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import { AnchoredMenu, Button, IconButton, Tooltip } from "../../components";
import type { RunButtonView } from "../../generated/RunButtonView";
import type { RunTabView } from "../../generated/RunTabView";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Terminal } from "../../terminal/Terminal";
import { Icon } from "../../theme/Icon";
import { pressOf, RUN_EVENTS, startRun } from "./logic";

const ignore = () => undefined;

/** The line's ⋯ and its menu: Edit command. */
function RunMenu({ view, onChoose }: { view: RunTabView; onChoose: (id: string) => void }) {
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

/** The Run's line: what runs where, its state in words, Open, the state's buttons and ⋯. */
export function RunLine({
  view,
  head,
  busy,
  onPress,
  onMenu,
}: {
  view: RunTabView;
  head?: ReactNode;
  busy: boolean;
  onPress: (button: RunButtonView) => void;
  onMenu: (id: string) => void;
}) {
  const words = (
    <span className="run-line-words">
      {view.line !== null && <span className="session-line-text">{view.line}</span>}
      <span className="run-state" data-tone={view.state.tone}>
        <Icon name={view.state.icon} size={12} />
        {view.state.text}
      </span>
    </span>
  );
  return (
    <div className="session-line">
      {head}
      {view.note !== null ? <Tooltip text={view.note}>{words}</Tooltip> : words}
      <span className="run-spacer" />
      {view.open_url !== null && (
        <button
          type="button"
          className="session-open-url"
          onClick={() => {
            if (view.open_url) request("run.open", { url: view.open_url.url }).catch(ignore);
          }}
        >
          {view.open_url.label}
          <Icon name="external-link" size={12} />
        </button>
      )}
      {view.buttons.map((b) => (
        <Button
          key={b.kind}
          compact
          variant={b.primary ? "primary" : "secondary"}
          icon={b.icon}
          label={b.label}
          loading={busy && b.kind !== "copy_lsof"}
          onClick={() => {
            onPress(b);
          }}
        />
      ))}
      <RunMenu view={view} onChoose={onMenu} />
    </div>
  );
}

export function RunTabBody({ view, ticket, head }: { view: RunTabView; ticket: string; head?: ReactNode }) {
  const [busy, setBusy] = useState(false);
  const press = (button: RunButtonView) => {
    const p = pressOf(button);
    switch (p.kind) {
      case "request":
        if (busy) return;
        setBusy(true);
        (p.method === "run.start" ? startRun(ticket) : request(p.method, { key: ticket }))
          .catch(ignore)
          .finally(() => {
            setBusy(false);
          });
        return;
      case "copy":
        navigator.clipboard.writeText(p.text).catch(ignore);
        return;
      case "none":
        return;
    }
  };
  const menu = (id: string) => {
    if (id === "edit_command") openOverlay("run_command", { project: view.project, ticket });
  };
  return (
    <div className="run-tab">
      <RunLine view={view} head={head} busy={busy} onPress={press} onMenu={menu} />
      {view.message !== null && (
        <p className="run-message" data-tone={view.state.tone}>
          {view.message}
        </p>
      )}
      <div className="run-output">
        {view.session === null ? (
          <div className="run-idle">{view.idle}</div>
        ) : (
          <Terminal
            key={view.session}
            session={view.session}
            label={`${ticket} · run`}
            readOnly
            files={{ kind: "ticket", key: ticket }}
          />
        )}
      </div>
    </div>
  );
}

export function RunTab({ ticket, head }: { ticket: string; head?: ReactNode }) {
  const load = useCallback(() => request("run.tab", { key: ticket }) as Promise<RunTabView>, [ticket]);
  const { view, error } = useServiceView(load, RUN_EVENTS);
  if (!view || view.key !== ticket)
    return (
      <>
        <div className="session-line">{head}</div>
        <p className="run-problem">{error?.message ?? ""}</p>
      </>
    );
  return <RunTabBody view={view} ticket={ticket} head={head} />;
}

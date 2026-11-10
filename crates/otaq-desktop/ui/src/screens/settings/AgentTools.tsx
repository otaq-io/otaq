// Settings → Agent tools (T-0365; §20 Agent tools, SET-02, UI-08; d0086, d0085, d0077; *15* `bGHVl` / `ehz4J`): one
// row per tool with its avatar, state pill and line, Sign in…, Show, How to install and the update command (copy
// only), "Otaq can see" and its chips, the quiet notes, what Otaq added and what stays, a setup that failed and the
// tool's permission warning (both amber, never turquoise), and the … menu (Hide / Show, Re-check this tool,
// Disconnect…, which asks first). Re-check under the list, with when it last checked. Opening the section rechecks
// every tool (`agent_tools.view {recheck: true}`); the service's `agent_tools_changed` asks the page again. Sign in…
// (`agent_tools.sign_in`) and Install (`agent_tools.install`, the How to install command, on the user's click) open
// a short terminal under the row (`onboarding/RowTerminal.tsx`), and the row updates when it ends. The onboarding
// Tools step draws the same rows in its compact form (`onboarding/Tools.tsx`). Every word is
// `agent_tools_page.rs`'s; this keeps the open menu and dialogs.
import { useCallback, useEffect, useRef, useState } from "react";

import { AnchoredMenu, Button, ConfirmDialog, Dialog, IconButton, ToolAvatar } from "../../components";
import type { AgentToolCommand } from "../../generated/AgentToolCommand";
import type { AgentToolDisconnectDialog } from "../../generated/AgentToolDisconnectDialog";
import type { AgentToolDisconnectOutcome } from "../../generated/AgentToolDisconnectOutcome";
import type { AgentToolMark } from "../../generated/AgentToolMark";
import type { AgentToolMenuAction } from "../../generated/AgentToolMenuAction";
import type { AgentToolMenuItem } from "../../generated/AgentToolMenuItem";
import type { AgentToolPillLook } from "../../generated/AgentToolPillLook";
import type { AgentToolRow } from "../../generated/AgentToolRow";
import type { AgentToolSignIn } from "../../generated/AgentToolSignIn";
import type { AgentToolsPage } from "../../generated/AgentToolsPage";
import type { IconName } from "../../generated/IconName";
import type { MenuItem } from "../../generated/MenuItem";
import { asRequestError, watchView } from "../../service";
import { request } from "../../tauri";
import { Terminal } from "../../terminal/Terminal";
import { Icon } from "../../theme/Icon";
import type { ToolTerminalTexts } from "../../generated/ToolTerminalTexts";
import { RowTerminal, type RowTerminals, useRowTerminals } from "../onboarding/RowTerminal";

/** What a click on a row asks for. */
export type AgentToolAction =
  | { kind: "sign_in" }
  | { kind: "install" }
  | { kind: "show" }
  | { kind: "docs" }
  | { kind: "menu"; action: AgentToolMenuAction };

/** What the page does for an action: a request answered with the fresh page, the sign-in terminal, the docs, or
 * Disconnect's dialog first (Disconnect is never sent straight from the menu). */
export type AgentToolStep =
  | { kind: "page"; method: "agent_tools.set_hidden"; params: { tool: string; hidden: boolean } }
  | { kind: "page"; method: "agent_tools.recheck"; params: { tool: string | null } }
  | { kind: "sign_in"; params: { tool: string } }
  | { kind: "install"; params: { tool: string } }
  | { kind: "docs"; params: { tool: string } }
  | { kind: "ask_disconnect"; params: { tool: string } };

export function stepOf(tool: string, action: AgentToolAction): AgentToolStep {
  switch (action.kind) {
    case "sign_in":
      return { kind: "sign_in", params: { tool } };
    case "install":
      return { kind: "install", params: { tool } };
    case "show":
      return { kind: "page", method: "agent_tools.set_hidden", params: { tool, hidden: false } };
    case "docs":
      return { kind: "docs", params: { tool } };
    case "menu":
      switch (action.action) {
        case "hide":
          return { kind: "page", method: "agent_tools.set_hidden", params: { tool, hidden: true } };
        case "show":
          return { kind: "page", method: "agent_tools.set_hidden", params: { tool, hidden: false } };
        case "recheck":
          return { kind: "page", method: "agent_tools.recheck", params: { tool } };
        case "disconnect":
          return { kind: "ask_disconnect", params: { tool } };
      }
  }
}

const MENU_ICONS: Record<AgentToolMenuAction, IconName> = {
  hide: "eye-off",
  show: "eye",
  recheck: "rotate-ccw",
  disconnect: "unlink",
};

/** The row's … entries as menu rows; Disconnect… is set apart and red. */
export function agentToolMenu(items: readonly AgentToolMenuItem[]): MenuItem[] {
  return items.map((item) => ({
    id: item.action,
    label: item.label,
    icon: MENU_ICONS[item.action],
    shortcut: null,
    danger: item.destructive,
    disabled: false,
    hint: null,
    separated: item.destructive,
    checked: false,
    header: false,
  }));
}

const PILL_ICONS: Record<AgentToolPillLook, IconName> = {
  connected: "circle-check",
  not_signed_in: "circle-alert",
  could_not_check: "circle-alert",
  update_needed: "circle-alert",
  not_installed: "circle-dashed",
  hidden: "eye-off",
};

/** The onboarding row's mark, as Rust says it (Settings shows the pill instead). */
export function markOf(row: AgentToolRow): AgentToolMark {
  return row.mark;
}

export const MARK_ICONS: Record<AgentToolMark, IconName> = {
  connected: "circle-check",
  attention: "circle-alert",
  missing: "circle-x",
  quiet: "circle-dashed",
};

/** Re-check on a row whose sign-in couldn't be checked: checks that tool again (`agent_tools.recheck {tool}`). */
export function RecheckButton({
  row,
  onAction,
  compact = false,
}: {
  row: AgentToolRow;
  onAction: (action: AgentToolAction) => void;
  compact?: boolean;
}) {
  if (row.recheck === null) return null;
  return (
    <Button
      label={row.recheck}
      variant="secondary"
      icon="rotate-ccw"
      compact={compact}
      onClick={() => {
        onAction({ kind: "menu", action: "recheck" });
      }}
    />
  );
}

/** How to install or the update command: its label, the command and Copy. Otaq never runs it. */
export function CommandBox({ command, label = true }: { command: AgentToolCommand; label?: boolean }) {
  return (
    <div className="agent-tool-command">
      {label && <span className="agent-tool-quiet">{command.label}</span>}
      <div className="agent-tool-command-box">
        <code>{command.command}</code>
        <IconButton
          icon="copy"
          label={command.copy}
          onClick={() => {
            navigator.clipboard.writeText(command.command).catch(() => undefined);
          }}
        />
      </div>
    </div>
  );
}

/** "Otaq can see" and its chips; a crossed-out one ("limits not reported") with a dash. */
export function CanSee({ row }: { row: AgentToolRow }) {
  if (row.can_see === null) return null;
  return (
    <div className="agent-tool-see">
      <span className="agent-tool-quiet">{row.can_see.label}</span>
      {row.can_see.chips.map((chip) => (
        <span key={chip.label} className="agent-tool-chip" data-seen={chip.seen || undefined}>
          <Icon name={chip.seen ? "check" : "minus"} size={11} />
          {chip.label}
        </span>
      ))}
    </div>
  );
}

/** The tool's own permission warning, amber (d0077), with its docs link where the tool has one. */
export function ToolWarning({
  row,
  onAction,
}: {
  row: AgentToolRow;
  onAction: (action: AgentToolAction) => void;
}) {
  if (row.warning === null) return null;
  return (
    <div className="agent-tool-warning" role="note">
      <Icon name="triangle-alert" size={14} />
      <span>
        <strong>{row.warning.title}</strong> {row.warning.text}
        {row.warning.link !== null && (
          <>
            {" "}
            <button
              type="button"
              className="settings-link"
              onClick={() => {
                onAction({ kind: "docs" });
              }}
            >
              {row.warning.link}
            </button>
          </>
        )}
      </span>
    </div>
  );
}

/** The … button and its menu; nothing when the row has no entries. */
export function RowMenu({
  row,
  label,
  onAction,
}: {
  row: AgentToolRow;
  label: string;
  onAction: (action: AgentToolAction) => void;
}) {
  const more = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  if (row.menu.length === 0) return <span className="agent-tool-more" />;
  return (
    <span className="agent-tool-more">
      <IconButton
        ref={more}
        icon="ellipsis"
        label={label}
        onClick={() => {
          setOpen((o) => !o);
        }}
      />
      {open && (
        <AnchoredMenu
          anchor={more}
          align="end"
          label={label}
          items={agentToolMenu(row.menu)}
          width={200}
          onChoose={(id) => {
            onAction({ kind: "menu", action: id as AgentToolMenuAction });
          }}
          onClose={(how) => {
            setOpen(false);
            if (how === "key") more.current?.focus();
          }}
        />
      )}
    </span>
  );
}

/** "Found, hidden by you · Show": the row's line, with Show after it for a hidden tool. */
export function DetailLine({
  row,
  onAction,
}: {
  row: AgentToolRow;
  onAction: (action: AgentToolAction) => void;
}) {
  return (
    <span className="agent-tool-detail">
      {row.detail}
      {row.show !== null && (
        <>
          {" · "}
          <button
            type="button"
            className="settings-link"
            onClick={() => {
              onAction({ kind: "show" });
            }}
          >
            {row.show}
          </button>
        </>
      )}
    </span>
  );
}

/** One tool's row on Settings → Agent tools. */
/** A row's Install / Sign in… terminal, when the page has one (not the gallery). */
export interface AgentRowTerminal {
  texts: ToolTerminalTexts;
  terminals: RowTerminals;
}

/** Whether the rows still offer `key`'s action ("codex:install", "codex:sign_in"): not, once it worked. */
export function agentStillOffered(rows: readonly AgentToolRow[], key: string): boolean {
  return rows.some(
    (r) =>
      (key === `${r.tool}:install` && r.install_button !== null) ||
      (key === `${r.tool}:sign_in` && r.sign_in !== null),
  );
}

/** A not-installed row's Install button and why there is none ("Needs Node.js (npm) first…"). */
export function InstallLine({
  row,
  onAction,
}: {
  row: AgentToolRow;
  onAction: (action: AgentToolAction) => void;
}) {
  return (
    <>
      {row.install_button !== null && (
        <span className="agent-tool-install">
          <Button
            label={row.install_button}
            variant="secondary"
            icon="arrow-down-to-line"
            compact
            onClick={() => {
              onAction({ kind: "install" });
            }}
          />
        </span>
      )}
      {row.install_note !== null && <span className="agent-tool-quiet">{row.install_note}</span>}
    </>
  );
}

export function AgentToolRowView({
  row,
  menuLabel,
  onAction,
  terminal,
}: {
  row: AgentToolRow;
  menuLabel: string;
  onAction: (action: AgentToolAction) => void;
  terminal?: AgentRowTerminal;
}) {
  return (
    <li className="agent-tool-row" data-look={row.pill?.look}>
      <div className="agent-tool-line">
        <span className="agent-tool-avatar">
          <ToolAvatar avatar={row.avatar} />
        </span>
        <div className="agent-tool-main">
          <div className="agent-tool-head">
            <span className="agent-tool-name">{row.name}</span>
            {row.pill !== null && (
              <span className="agent-tool-pill" data-look={row.pill.look}>
                <Icon name={PILL_ICONS[row.pill.look]} size={12} />
                {row.pill.label}
              </span>
            )}
          </div>
          <DetailLine row={row} onAction={onAction} />
          {row.install !== null && <CommandBox command={row.install} />}
          <InstallLine row={row} onAction={onAction} />
          {row.update !== null && <CommandBox command={row.update} />}
          {terminal && <RowTerminal row={row.tool} texts={terminal.texts} terminals={terminal.terminals} />}
          <CanSee row={row} />
          {row.notes.map((note) => (
            <span key={note} className="agent-tool-quiet">
              {note}
            </span>
          ))}
          {row.added !== null && (
            <span className="agent-tool-added">
              <Icon name="plug" size={13} />
              <span>
                {row.added.text}
                {row.added.stays !== null && <span className="agent-tool-quiet"> · {row.added.stays}</span>}
              </span>
            </span>
          )}
          {row.setup_failed !== null && (
            <span className="agent-tool-failed" role="alert">
              <Icon name="triangle-alert" size={13} />
              <span>{row.setup_failed}</span>
            </span>
          )}
        </div>
        <div className="agent-tool-actions">
          {row.sign_in !== null && (
            <Button
              label={row.sign_in}
              icon="square-terminal"
              onClick={() => {
                onAction({ kind: "sign_in" });
              }}
            />
          )}
          <RecheckButton row={row} onAction={onAction} />
          <RowMenu row={row} label={menuLabel} onAction={onAction} />
        </div>
      </div>
      <ToolWarning row={row} onAction={onAction} />
    </li>
  );
}

/** The section as the view says, with the page's calls. */
export function AgentToolsPageBody({
  page,
  rechecking = false,
  onAction,
  onRecheck,
  terminals,
}: {
  page: AgentToolsPage;
  rechecking?: boolean;
  onAction: (tool: string, action: AgentToolAction) => void;
  onRecheck: () => void;
  /** The rows' Install / Sign in… terminal; none in the gallery. */
  terminals?: RowTerminals;
}) {
  return (
    <section className="settings-section agent-tools" data-section="agent_tools" aria-label={page.title}>
      <h3 className="settings-section-title">{page.title}</h3>
      <p className="settings-intro">{page.intro}</p>
      {page.error !== null && (
        <p className="settings-error" role="alert">
          {page.error}
        </p>
      )}
      {page.rows.length > 0 && (
        <ul className="agent-tool-rows">
          {page.rows.map((row, index) => (
            <AgentToolRowView
              // One row per tool in the app; the gallery's "every state" page repeats a tool.
              key={`${row.tool}-${String(index)}`}
              row={row}
              menuLabel={page.menu_label}
              onAction={(action) => {
                onAction(row.tool, action);
              }}
              terminal={terminals && { texts: page.terminal, terminals }}
            />
          ))}
        </ul>
      )}
      <div className="agent-tools-foot">
        <Button
          label={page.recheck}
          variant="secondary"
          icon="rotate-ccw"
          loading={rechecking}
          onClick={onRecheck}
        />
        {page.checked !== null && <span className="agent-tool-quiet">{page.checked}</span>}
      </div>
      <div className="agent-tools-not-shown">
        <h4 className="settings-section-title">{page.not_shown_title}</h4>
        <p className="settings-intro">{page.not_shown}</p>
      </div>
    </section>
  );
}

/** Disconnect…'s confirmation, or its refusal while the tool's agents run (Disconnect then off). */
export function DisconnectDialogView({
  dialog,
  onCancel,
  onConfirm,
}: {
  dialog: AgentToolDisconnectDialog;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <ConfirmDialog
      title={dialog.title}
      destructive
      confirmLabel={dialog.confirm}
      confirmIcon="unlink"
      confirmDisabled={!dialog.can_confirm}
      width={460}
      onCancel={onCancel}
      onConfirm={onConfirm}
      body={
        <div className="agent-tool-disconnect">
          <p className="dialog-text">{dialog.text}</p>
          {dialog.stays !== null && <p className="dialog-text">{dialog.stays}</p>}
          <p className="dialog-text">{dialog.hidden}</p>
          {dialog.error !== null && (
            <p className="agent-tool-disconnect-error" role="alert">
              <Icon name="circle-x" size={13} />
              <span>{dialog.error}</span>
            </p>
          )}
        </div>
      }
    />
  );
}

/** Sign in…'s terminal: the tool's own login, typed by the user (Otaq never reads it). Closing it rechecks. */
export function SignInDialog({ signIn, onClose }: { signIn: AgentToolSignIn; onClose: () => void }) {
  return (
    <Dialog title={signIn.title} hint="close" width={720} height="480px" onCancel={onClose} footer={null}>
      <div className="agent-tool-sign-in">
        <Terminal session={signIn.session} label={signIn.title} />
      </div>
    </Dialog>
  );
}

type Open =
  | { kind: "disconnect"; dialog: AgentToolDisconnectDialog }
  | { kind: "sign_in"; signIn: AgentToolSignIn }
  | null;

/** Runs a row's action: the request behind it, the sign-in terminal or Disconnect's dialog. `onPage` takes the fresh
 * page a request answers; `onDone` hears that the tools may have changed (the onboarding step checks again). */
export function useAgentToolActions(
  onPage: (page: AgentToolsPage) => void,
  onError: (message: string) => void,
  onDone?: () => void,
  /** Sign in… and Install open here, under the row; without it Sign in… opens its dialog. */
  terminals?: RowTerminals,
) {
  const [open, setOpen] = useState<Open>(null);
  const fail = useCallback(
    (e: unknown) => {
      onError(asRequestError(e).message);
    },
    [onError],
  );
  const act = useCallback(
    (tool: string, action: AgentToolAction) => {
      const step = stepOf(tool, action);
      switch (step.kind) {
        case "page":
          request(step.method, step.params).then((page) => {
            onPage(page as AgentToolsPage);
            onDone?.();
          }, fail);
          return;
        case "sign_in":
        case "install": {
          const method = step.kind === "sign_in" ? "agent_tools.sign_in" : "agent_tools.install";
          if (terminals) {
            terminals.start(tool, `${tool}:${step.kind}`, () =>
              request(method, step.params).then((opened) => opened as AgentToolSignIn),
            );
          } else if (step.kind === "sign_in") {
            request(method, step.params).then((signIn) => {
              setOpen({ kind: "sign_in", signIn: signIn as AgentToolSignIn });
            }, fail);
          }
          return;
        }
        case "docs":
          request("agent_tools.open_docs", step.params).catch(fail);
          return;
        case "ask_disconnect":
          request("agent_tools.disconnect_dialog", step.params).then((dialog) => {
            setOpen({ kind: "disconnect", dialog: dialog as AgentToolDisconnectDialog });
          }, fail);
          return;
      }
    },
    [onPage, onDone, fail, terminals],
  );
  const close = () => {
    setOpen(null);
  };
  const dialogs =
    open?.kind === "disconnect" ? (
      <DisconnectDialogView
        dialog={open.dialog}
        onCancel={close}
        onConfirm={() => {
          request("agent_tools.disconnect", { tool: open.dialog.tool }).then((answer) => {
            const outcome = answer as AgentToolDisconnectOutcome;
            if (outcome.kind === "done") {
              close();
              onPage(outcome.page);
              onDone?.();
            } else setOpen({ kind: "disconnect", dialog: outcome.dialog });
          }, fail);
        }}
      />
    ) : open?.kind === "sign_in" ? (
      <SignInDialog
        signIn={open.signIn}
        onClose={() => {
          close();
          onDone?.();
        }}
      />
    ) : null;
  return { act, dialogs };
}

/** The registered section: the page from `agent_tools.view`, rechecked when it opens. */
export function AgentToolsSection() {
  const [page, setPage] = useState<AgentToolsPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [rechecking, setRechecking] = useState(false);
  // The first load rechecks every tool (opening the section, d0086); later ones only read the rows.
  const first = useRef(true);
  useEffect(
    () =>
      watchView(
        () => {
          const recheck = first.current;
          first.current = false;
          return request("agent_tools.view", { recheck }) as Promise<AgentToolsPage>;
        },
        ["agent_tools_changed"],
        (state) => {
          if (state.view) setPage(state.view);
          setError(state.error?.message ?? null);
        },
      ),
    [],
  );
  const onPage = useCallback((fresh: AgentToolsPage) => {
    setPage(fresh);
    setError(null);
  }, []);
  const rows = page?.rows;
  const terminals = useRowTerminals(
    (key) => agentStillOffered(rows ?? [], key),
    // The service checks the tool again when its terminal ends; this answers once that check is in.
    () =>
      request("agent_tools.recheck", { tool: null }).then((fresh) => {
        onPage(fresh as AgentToolsPage);
      }),
  );
  const { act, dialogs } = useAgentToolActions(onPage, setError, undefined, terminals);
  if (!page) return null;
  return (
    <>
      {error !== null && (
        <p className="settings-error" role="alert">
          {error}
        </p>
      )}
      <AgentToolsPageBody
        page={page}
        rechecking={rechecking}
        onAction={act}
        terminals={terminals}
        onRecheck={() => {
          setRechecking(true);
          request("agent_tools.recheck", { tool: null }).then(
            (fresh) => {
              setRechecking(false);
              onPage(fresh as AgentToolsPage);
            },
            (e: unknown) => {
              setRechecking(false);
              setError(asRequestError(e).message);
            },
          );
        }}
      />
      {dialogs}
    </>
  );
}

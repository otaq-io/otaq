// Settings → Git & pull requests (the topic id `git`): git on this machine (its version and where it is, Re-check),
// then one row per pull request provider with the tool it uses and whether that tool is signed in. Install and
// Sign in… run in a terminal under the row, as the agent tools' do; when the command ends the tools are checked again.
// Everything comes from `settings.git`, which runs the tools check each time it is asked.
import { useCallback, useEffect, useState } from "react";

import { Button, IconButton } from "../../components";
import type { GitSettingsRow } from "../../generated/GitSettingsRow";
import type { IconName } from "../../generated/IconName";
import type { GitSettingsView } from "../../generated/GitSettingsView";
import type { ToolTerminalOpened } from "../../generated/ToolTerminalOpened";
import { asRequestError } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { RowTerminal, type RowTerminals, useRowTerminals } from "../onboarding/RowTerminal";

/** A row's icon by its look; a missing git is an error, a missing provider only muted. */
export function rowIcon(row: GitSettingsRow, required: boolean): IconName {
  if (row.look === "ready") return "circle-check";
  if (row.look === "attention") return "circle-alert";
  return required ? "circle-x" : "circle-minus";
}

/** The key a row's button opens its terminal under. */
const buttonKey = (row: GitSettingsRow) => (row.button ? `${row.button.tool}:${row.button.action}` : null);

function GitRow({
  row,
  required,
  view,
  terminals,
}: {
  row: GitSettingsRow;
  required: boolean;
  view: GitSettingsView;
  terminals: RowTerminals;
}) {
  const button = row.button;
  return (
    <div
      className="settings-tool settings-git-row"
      data-look={row.look === "missing" ? (required ? "missing_required" : "missing_optional") : row.look}
    >
      <Icon name={rowIcon(row, required)} />
      <span className="settings-tool-name">{row.name}</span>
      <span className="settings-git-detail">{row.detail}</span>
      <span className="settings-git-state">{row.state}</span>
      {row.command !== null && (
        <span className="settings-tool-command">
          <code>{row.command}</code>
          <IconButton
            icon="copy"
            label={view.copy}
            onClick={() => {
              if (row.command !== null) navigator.clipboard.writeText(row.command).catch(() => undefined);
            }}
          />
          {button !== null && (
            <Button
              compact
              variant="secondary"
              icon={button.action === "login" ? "square-terminal" : "arrow-down-to-line"}
              label={button.label}
              disabled={terminals.open?.row === row.tool}
              onClick={() => {
                terminals.start(row.tool, `${button.tool}:${button.action}`, () =>
                  request("tools.terminal", { tool: button.tool, action: button.action }).then((opened) => ({
                    session: (opened as ToolTerminalOpened).session,
                    title: button.running,
                  })),
                );
              }}
            />
          )}
        </span>
      )}
      {row.note !== null && <span className="settings-tool-note">{row.note}</span>}
      <span className="settings-tool-terminal">
        <RowTerminal row={row.tool} texts={view.terminal} terminals={terminals} />
      </span>
    </div>
  );
}

/** The page as the view says it. */
export function GitBody({
  view,
  checking,
  onRecheck,
  terminals,
}: {
  view: GitSettingsView;
  checking: boolean;
  onRecheck: () => void;
  terminals: RowTerminals;
}) {
  return (
    <section className="settings-section settings-git" data-section="git" aria-label={view.git_title}>
      <p className="settings-intro">{view.intro}</p>
      {view.error !== null && <p className="settings-problem">{view.error}</p>}
      <div className="settings-git-head">
        <h3 className="settings-section-title">{view.git_title}</h3>
        <Button
          compact
          variant="secondary"
          icon="rotate-ccw"
          label={view.recheck_label}
          loading={checking}
          onClick={onRecheck}
        />
      </div>
      <div className="settings-tools">
        {view.git && <GitRow row={view.git} required view={view} terminals={terminals} />}
      </div>
      <h3 className="settings-section-title">{view.providers_title}</h3>
      <div className="settings-tools">
        {view.providers.map((row) => (
          <GitRow key={row.tool} row={row} required={false} view={view} terminals={terminals} />
        ))}
      </div>
      <p className="settings-git-footnote">{view.footnote}</p>
    </section>
  );
}

/** The registered section: asks `settings.git` on opening and on Re-check. */
export function GitSection() {
  const [view, setView] = useState<GitSettingsView | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [checking, setChecking] = useState(true);
  const run = useCallback(
    () =>
      request("settings.git").then(
        (v) => {
          setView(v as GitSettingsView);
          setFailure(null);
          setChecking(false);
        },
        (e: unknown) => {
          setFailure(asRequestError(e).message);
          setChecking(false);
        },
      ),
    [],
  );
  useEffect(() => {
    void run();
  }, [run]);
  const check = useCallback(() => {
    setChecking(true);
    return run();
  }, [run]);
  const rows = view ? [view.git, ...view.providers] : [];
  const terminals = useRowTerminals((key) => rows.some((r) => r !== null && buttonKey(r) === key), check);
  if (failure !== null)
    return (
      <p className="settings-problem" role="alert">
        {failure}
      </p>
    );
  if (!view) return null;
  return (
    <GitBody
      view={view}
      checking={checking}
      onRecheck={() => {
        void check();
      }}
      terminals={terminals}
    />
  );
}

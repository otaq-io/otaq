// The Tools step: connect an agent tool, only what's needed to start (d0021, d0011, d0086). The agent tools first
// ("AGENT TOOLS", how many are ready, the blocking line said once at the top while none is, and the same rows as
// Settings → Agent tools in their compact form: Sign in…, Install, Show, the install command before Install, the
// tool's own warning, the … menu without Disconnect…), then git ("ALSO NEEDED") with its version or why it is
// missing, its command and Install button; each button runs its command in a short terminal under the row
// (`RowTerminal`); Check again, Continue (off while no agent tool is ready), and where more tools are added later. All
// of it is `OnboardingTools`'.
import { Button, IconButton, ToolAvatar } from "../../components";
import type { AgentToolRow } from "../../generated/AgentToolRow";
import type { OnboardingAgentTools } from "../../generated/OnboardingAgentTools";
import type { OnboardingTools } from "../../generated/OnboardingTools";
import type { ToolRowView } from "../../generated/ToolRowView";
import type { ToolTerminalTexts } from "../../generated/ToolTerminalTexts";
import type { ToolTerminalOpened } from "../../generated/ToolTerminalOpened";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import {
  type AgentToolAction,
  CommandBox,
  DetailLine,
  InstallLine,
  MARK_ICONS,
  markOf,
  RowMenu,
  ToolWarning,
} from "../settings/AgentTools";
import { GitIdentityBox } from "../settings/GitIdentity";
import { RowTerminal, type RowTerminals } from "./RowTerminal";

/** Whether the checklist rows still offer `key`'s action ("gh:login"): not, once it worked. */
export function checklistStillOffered(rows: readonly ToolRowView[], key: string): boolean {
  return rows.some((r) => r.button !== null && `${r.button.tool}:${r.button.action}` === key);
}

function AgentRow({
  row,
  menuLabel,
  onAction,
  texts,
  terminals,
}: {
  row: AgentToolRow;
  menuLabel: string;
  onAction: (action: AgentToolAction) => void;
  texts: ToolTerminalTexts;
  terminals?: RowTerminals;
}) {
  const mark = markOf(row);
  return (
    <li className="tool-row agent-tool-step-row" data-mark={mark}>
      <div className="tool-row-head">
        <span className="tool-row-icon">
          <Icon name={MARK_ICONS[mark]} size={16} />
        </span>
        <ToolAvatar avatar={row.avatar} />
        <span className="tool-row-name agent-tool-step-name">{row.name}</span>
        <span className="tool-row-status">
          <DetailLine row={row} onAction={onAction} />
        </span>
        {row.sign_in !== null && (
          <Button
            label={row.sign_in}
            variant="secondary"
            icon="square-terminal"
            compact
            onClick={() => {
              onAction({ kind: "sign_in" });
            }}
          />
        )}
        <RowMenu row={row} label={menuLabel} onAction={onAction} />
      </div>
      {row.install !== null && <CommandBox command={row.install} label={false} />}
      <InstallLine row={row} onAction={onAction} />
      {row.update !== null && <CommandBox command={row.update} />}
      {terminals && <RowTerminal row={row.tool} texts={texts} terminals={terminals} />}
      <ToolWarning row={row} onAction={onAction} />
    </li>
  );
}

/** The agent tools block: heading and count, the blocking line, the rows. */
export function AgentToolsBlock({
  block,
  onAction,
  texts,
  terminals,
}: {
  block: OnboardingAgentTools;
  onAction: (tool: string, action: AgentToolAction) => void;
  texts: ToolTerminalTexts;
  /** The rows' Install / Sign in… terminal; none in the gallery. */
  terminals?: RowTerminals;
}) {
  return (
    <>
      {block.blocked !== null && (
        <div className="agent-tools-blocked" role="alert">
          <Icon name="circle-x" size={15} />
          <span>{block.blocked}</span>
        </div>
      )}
      <div className="tool-heading">
        <span>{block.heading}</span>
        <span className="tool-heading-count">{block.count}</span>
      </div>
      {block.rows.length > 0 && (
        <ul className="tool-rows">
          {block.rows.map((row, index) => (
            <AgentRow
              // One row per tool in the app; a gallery cell may repeat a tool.
              key={`${row.tool}-${String(index)}`}
              row={row}
              menuLabel={block.menu_label}
              onAction={(action) => {
                onAction(row.tool, action);
              }}
              texts={texts}
              terminals={terminals}
            />
          ))}
        </ul>
      )}
    </>
  );
}

function Row({
  row,
  copy,
  texts,
  terminals,
  onRecheck,
}: {
  row: ToolRowView;
  copy: string;
  texts: ToolTerminalTexts;
  terminals?: RowTerminals;
  /** After the name and email are saved. */
  onRecheck?: () => void;
}) {
  const button = row.button;
  return (
    <li className="tool-row" data-look={row.look} data-identity={row.identity !== null || undefined}>
      <div className="tool-row-head">
        <span className="tool-row-icon">
          <Icon name={row.icon} size={16} />
        </span>
        <span className="tool-row-name">{row.name}</span>
        <span className="tool-row-badge">{row.badge}</span>
        <span className="tool-row-status">{row.status}</span>
      </div>
      {row.explanation !== null && <p className="tool-row-explanation">{row.explanation}</p>}
      {row.command !== null && (
        <div className="tool-row-command">
          <code>{row.command}</code>
          <IconButton
            icon="copy"
            label={copy}
            onClick={() => {
              if (row.command !== null) navigator.clipboard.writeText(row.command).catch(() => undefined);
            }}
          />
          {button !== null && terminals && (
            <Button
              label={button.label}
              variant="secondary"
              icon={button.action === "login" ? "square-terminal" : "arrow-down-to-line"}
              compact
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
        </div>
      )}
      {row.note !== null && <p className="tool-row-note">{row.note}</p>}
      {row.identity !== null && <GitIdentityBox form={row.identity} onSaved={() => onRecheck?.()} />}
      {terminals && <RowTerminal row={row.tool} texts={texts} terminals={terminals} />}
    </li>
  );
}

export interface ToolsListProps {
  tools: OnboardingTools;
  checking: boolean;
  onRecheck: () => void;
  onContinue: () => void;
  /** A click on an agent tool's row (Sign in…, Install, Show, its … menu). */
  onAgentAction: (tool: string, action: AgentToolAction) => void;
  /** The rows' Install / Sign in… terminal (agent tools and git, gh, az); none in the gallery. */
  terminals?: RowTerminals;
}

export function ToolsList({
  tools,
  checking,
  onRecheck,
  onContinue,
  onAgentAction,
  terminals,
}: ToolsListProps) {
  return (
    <>
      <h1 className="onboarding-title">{tools.title}</h1>
      <p className="onboarding-text">{tools.text}</p>
      {tools.waiting === null && (
        <AgentToolsBlock
          block={tools.agent_tools}
          onAction={onAgentAction}
          texts={tools.terminal}
          terminals={terminals}
        />
      )}
      {tools.waiting === null && <div className="tool-heading">{tools.other_heading}</div>}
      {tools.rows.length > 0 ? (
        <ul className="tool-rows">
          {tools.rows.map((row) => (
            <Row
              key={row.name}
              row={row}
              copy={tools.copy}
              texts={tools.terminal}
              terminals={terminals}
              onRecheck={onRecheck}
            />
          ))}
        </ul>
      ) : (
        <p className="tool-rows tool-rows-waiting">{tools.waiting}</p>
      )}
      {tools.agents_line !== null && <p className="tool-agents-line">{tools.agents_line}</p>}
      {tools.error !== null && <p className="onboarding-failure">{tools.error}</p>}
      <div className="onboarding-foot">
        <Button
          label={tools.recheck}
          variant="secondary"
          icon="rotate-ccw"
          loading={checking}
          onClick={onRecheck}
        />
        <span className="onboarding-spacer" />
        {tools.continue_reason !== null && <span className="tool-reason">{tools.continue_reason}</span>}
        <Button
          label={tools.continue_label}
          icon="arrow-right"
          disabled={!tools.continue_enabled || checking}
          onClick={onContinue}
        />
      </div>
    </>
  );
}

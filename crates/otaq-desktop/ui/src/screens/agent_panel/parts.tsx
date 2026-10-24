// The agent panel's parts that came from the drawer's agent tab (T-0243, moved by T-0330; PAG-06, d0068): End agent's
// confirmation mid-turn and the Recently ended list. Every text is Rust's (`project_agent_page.rs`).
import { Button, ConfirmDialog } from "../../components";
import type { AgentCloseDialog } from "../../generated/AgentCloseDialog";
import type { RecentAgents } from "../../generated/RecentAgents";
import { Icon } from "../../theme/Icon";

/** *Close an agent mid-turn · confirm*: the attention line, what stays, and Stop and close (Cancel the default). */
export function CloseAgentDialog({
  dialog,
  onConfirm,
  onCancel,
}: {
  dialog: AgentCloseDialog;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <ConfirmDialog
      title={dialog.title}
      width={480}
      destructive
      confirmLabel={dialog.confirm}
      confirmIcon="circle-stop"
      body={
        <>
          <p className="agent-panel-close-turn">
            <Icon name="activity" size={13} />
            <span>{dialog.mid_turn}</span>
          </p>
          <p className="dialog-text">{dialog.text}</p>
        </>
      }
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}

/** The Recently ended list (*Recently ended · list*), with entries or empty: the history icon's popover. */
export function RecentList({ recent, onResume }: { recent: RecentAgents; onResume: (id: string) => void }) {
  return (
    <div className="agent-panel-recent">
      <span className="agent-panel-recent-title">{recent.title}</span>
      {recent.rows.length === 0 ? (
        <p className="agent-panel-recent-empty">{recent.empty}</p>
      ) : (
        recent.rows.map((row) => (
          <div key={row.id} className="agent-panel-recent-row">
            <Icon name="bot" size={14} />
            <span className="agent-panel-recent-text">
              <span className="agent-panel-recent-name">{row.name}</span>
              <span className="agent-panel-recent-when">{row.when}</span>
            </span>
            <Button
              compact
              variant="secondary"
              icon="play"
              label={row.resume}
              onClick={() => {
                onResume(row.id);
              }}
            />
          </div>
        ))
      )}
    </div>
  );
}

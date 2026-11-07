// The Activity popover under the header's Activity button (SHL-03, RUN-04, SCH-03; d0087, d0086, d0016; *02 → Row ·
// Header parts: Activity, rail (0086 · V) → Activity popover*, *Row · Activity popover states (0087)*), opened by a
// click or ⇧⌘U. Top to bottom: while a tool is at its limit, the limit banner's lines with Resume <tool> (so Resume
// stays reachable from the usage chip after the banner's × closed it); the agent-limit stepper while limits are on (each press saves at once and reaches the
// scheduler without restarting anything), AGENTS RUNNING, USAGE · WHAT'S LEFT with a bar per window, PROCESSES (the
// Run rows of `Processes.tsx`; a row opens its ticket's Run tab) and "Open Terminals ⌃`". Every word, number and
// which bar is amber is Rust's (`activity.rs`, `limits.rs`); no dollar figure anywhere.
import { useEffect, useState } from "react";

import type { OverlayProps } from "../../app/registry";
import { openTicket } from "../../app/selection";
import { useStore } from "../../app/store";
import { Kbd, Popover, ToolAvatar } from "../../components";
import type { ActivityPopover as ActivityView } from "../../generated/ActivityPopover";
import type { ActivityToolUsage } from "../../generated/ActivityToolUsage";
import type { ActivityWindow } from "../../generated/ActivityWindow";
import type { HeaderView } from "../../generated/HeaderView";
import type { LimitField } from "../../generated/LimitField";
import type { LimitLine } from "../../generated/LimitLine";
import type { LimitStep } from "../../generated/LimitStep";
import type { LimitStepper } from "../../generated/LimitStepper";
import type { LimitsPopover } from "../../generated/LimitsPopover";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { LimitLines } from "../notices/LimitLines";
import { noticesStore } from "../notices/notices";
import { ProcessesBody } from "../session/Processes";
import { openTerminals } from "../terminals";
import { toolProblemAction } from "../tool_problems/toolProblemRunner";
import { activityAnchor, headerStore } from "./Header";

function Stepper({ stepper, onStep }: { stepper: LimitStepper; onStep: (up: boolean) => void }) {
  return (
    <div className="limits-row">
      <span className="limits-label">{stepper.label}</span>
      <div className="stepper" role="group" aria-label={stepper.label}>
        <button
          type="button"
          aria-label="Fewer"
          disabled={!stepper.can_lower}
          onClick={() => {
            onStep(false);
          }}
        >
          <Icon name="minus" size={12} />
        </button>
        <span className="stepper-value">{stepper.value}</span>
        <button
          type="button"
          aria-label="More"
          disabled={!stepper.can_raise}
          onClick={() => {
            onStep(true);
          }}
        >
          <Icon name="plus" size={12} />
        </button>
      </div>
    </div>
  );
}

/** The agent-limit stepper: the same two values as Settings. */
export function LimitsBody({
  view,
  onStep,
}: {
  view: LimitsPopover;
  onStep: (field: LimitField, up: boolean) => void;
}) {
  return (
    <div className="limits">
      <strong className="limits-title">{view.title}</strong>
      <Stepper
        stepper={view.max_agents}
        onStep={(up) => {
          onStep("max_agents", up);
        }}
      />
      <Stepper
        stepper={view.back_pressure}
        onStep={(up) => {
          onStep("back_pressure", up);
        }}
      />
      <p className="limits-note">{view.note}</p>
    </div>
  );
}

function SectionHead({ title, aside }: { title: string; aside: string }) {
  return (
    <div className="activity-head">
      <span className="activity-title">{title}</span>
      <span className="activity-aside">{aside}</span>
    </div>
  );
}

function WindowRow({ row }: { row: ActivityWindow }) {
  return (
    <div className="activity-window" data-amber={row.amber || undefined}>
      <span className="activity-window-label">{row.label}</span>
      <span className="activity-bar" aria-hidden="true">
        <span className="activity-bar-fill" style={{ width: `${String(row.fill)}%` }} />
      </span>
      <span className="activity-left">{row.left}</span>
      <span className="activity-reset">{row.reset ?? ""}</span>
    </div>
  );
}

/** Each tool's plan windows as "% left", amber at 20% left or less: in the popover and on Settings → Limits & usage. */
export function UsageTools({ tools }: { tools: readonly ActivityToolUsage[] }) {
  return (
    <>
      {tools.map((tool) => (
        <div key={tool.name} className="activity-usage">
          <div className="activity-tool">
            {tool.avatar && <ToolAvatar avatar={tool.avatar} />}
            <span className="activity-tool-name">{tool.name}</span>
            <span className="activity-tokens">{tool.tokens}</span>
          </div>
          {tool.windows.map((row) => (
            <WindowRow key={row.label} row={row} />
          ))}
          {tool.notes.map((note) => (
            <p key={note} className="activity-note">
              {note}
            </p>
          ))}
        </div>
      ))}
    </>
  );
}

export interface ActivityBodyProps {
  view: ActivityView;
  /** The limit banner's lines while a tool is at its limit, with their Resume. */
  limitLines?: readonly LimitLine[];
  onStep: (field: LimitField, up: boolean) => void;
  onOpenTicket: (key: string) => void;
  onOpenTerminals: () => void;
}

/** The popover's content, top to bottom. */
export function ActivityBody({
  view,
  limitLines = [],
  onStep,
  onOpenTicket,
  onOpenTerminals,
}: ActivityBodyProps) {
  const { agents, usage, processes } = view;
  return (
    <div className="activity">
      {limitLines.length > 0 && (
        <section className="activity-section">
          <LimitLines
            lines={limitLines}
            onAction={(action) => {
              toolProblemAction(action, {});
            }}
          />
        </section>
      )}
      {view.limits && (
        <section className="activity-section">
          <LimitsBody view={view.limits} onStep={onStep} />
        </section>
      )}
      <section className="activity-section">
        <SectionHead title={agents.title} aside={agents.count} />
        {agents.empty !== null && <p className="activity-empty">{agents.empty}</p>}
        {agents.rows.map((row) => (
          <div key={row.name} className="activity-tool">
            {row.avatar && <ToolAvatar avatar={row.avatar} />}
            <span className="activity-tool-name">{row.name}</span>
            <span className="activity-count">{row.count}</span>
          </div>
        ))}
      </section>
      <section className="activity-section">
        <SectionHead title={usage.title} aside={usage.today} />
        <UsageTools tools={usage.tools} />
      </section>
      {processes && (
        <section className="activity-section">
          <ProcessesBody
            view={processes.list}
            heading={
              <span className="activity-head">
                <span className="activity-title">{processes.title}</span>
                <span className="activity-aside">{processes.count}</span>
              </span>
            }
            onOpenTicket={onOpenTicket}
          />
        </section>
      )}
      <button type="button" className="activity-link" onClick={onOpenTerminals}>
        <span>{view.open_terminals.label}</span>
        {view.open_terminals.keys !== null && <Kbd keys={view.open_terminals.keys} />}
      </button>
    </div>
  );
}

/** The `activity` overlay, asked again with each new header view. A step's answer shows at once, until the next one. */
export function ActivityOverlay({ close }: OverlayProps<"activity">) {
  const header = useStore(headerStore);
  const limitLines = useStore(noticesStore)?.limit_lines ?? [];
  const [view, setView] = useState<ActivityView | null>(null);
  const [answer, setAnswer] = useState<{ after: HeaderView | undefined; limits: LimitsPopover }>();
  useEffect(() => {
    request("shell.activity").then(
      (popover) => {
        setView(popover as ActivityView);
      },
      () => undefined,
    );
  }, [header]);
  if (!view) return null;
  const shown: ActivityView =
    answer && answer.after === header && view.limits ? { ...view, limits: answer.limits } : view;
  const step = (field: LimitField, up: boolean) => {
    const params: LimitStep = { field, up };
    request("shell.step_limit", params).then(
      (next) => {
        setAnswer({ after: headerStore.get(), limits: next as LimitsPopover });
      },
      // A refused save keeps the values shown; the header's next view says what is stored.
      () => undefined,
    );
  };
  return (
    <Popover
      label={header?.activity.label ?? view.agents.title}
      anchor={activityAnchor}
      align="end"
      onClose={close}
    >
      <ActivityBody
        view={shown}
        limitLines={limitLines}
        onStep={step}
        onOpenTicket={(key) => {
          close();
          openTicket(key, "run");
        }}
        onOpenTerminals={() => {
          close();
          openTerminals();
        }}
      />
    </Popover>
  );
}

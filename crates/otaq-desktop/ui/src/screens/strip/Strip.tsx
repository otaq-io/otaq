// The Needs you strip (T-0459; NYS-01, NYS-02; d0114; *24 → Row · The two views* `s24-03`, *Row · Needs you: the
// next project, and every group* `s24-12`): under the board's toolbar in both views, the open project's items as
// chips in the queue's order and groups, with Next (⌘J) at its end; "Needs you · 0 here" and "Next · billing-api, 1"
// when this project has none. A chip opens its ticket or the Project agent panel; Resume all asks first. Everything it
// says is `strip.view`'s. The Board view and the List view each render `<NeedsYouStrip />` once.
import { Fragment, type RefObject, useCallback, useRef, useState } from "react";

import { shellLayout } from "../../app/layout";
import { openOverlay } from "../../app/overlays";
import { useSelection } from "../../app/selection";
import { useStore } from "../../app/store";
import { Button, Kbd, Tooltip, useFloatingStyle } from "../../components";
import type { StripAsks } from "../../generated/StripAsks";
import type { StripChip } from "../../generated/StripChip";
import type { StripGroup } from "../../generated/StripGroup";
import type { StripParams } from "../../generated/StripParams";
import type { StripTarget } from "../../generated/StripTarget";
import type { StripView } from "../../generated/StripView";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { shownAgents } from "../agent_panel/logic";
import { lastOpened, next, openOf, openTarget, stripParams } from "./logic";

/** The service events that change the strip: its items, the projects' names and order, the user's key for Next. */
export const STRIP_EVENTS = [
  "needs_you_changed",
  "projects_changed",
  "project_agent_changed",
  "ticket_changed",
  "ticket_deleted",
  "settings_changed",
] as const;

const ignore = () => undefined;

/**
 * A helper's question the agent in charge passed on (d0133, `s30-02`): the question, the agent in charge's note,
 * Answer 40.2… and Open in the ticket; next to its chip, and closed by a press outside or a scroll.
 */
export function AsksPopover({
  asks,
  anchor,
  onOpen,
  onClose,
}: {
  asks: StripAsks;
  anchor: RefObject<HTMLElement | null>;
  onOpen: (target: StripTarget) => void;
  onClose: () => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  const style = useFloatingStyle(anchor, root, "start", onClose);
  return (
    <div ref={root} className="strip-asks" role="dialog" aria-label={asks.title} style={style}>
      <div className="strip-asks-title">{asks.title}</div>
      <p className="strip-asks-question">{asks.question}</p>
      {asks.note !== null && <p className="strip-asks-note">{asks.note}</p>}
      <div className="strip-asks-buttons">
        <Button
          compact
          label={asks.answer}
          onClick={() => {
            onClose();
            onOpen(asks.answer_target);
          }}
        />
        <Button
          compact
          variant="secondary"
          label={asks.open}
          onClick={() => {
            onClose();
            onOpen(asks.open_target);
          }}
        />
      </div>
    </div>
  );
}

function Chip({ chip, onOpen }: { chip: StripChip; onOpen: (target: StripTarget) => void }) {
  const [asking, setAsking] = useState(false);
  const close = useCallback(() => {
    setAsking(false);
  }, []);
  const button = useRef<HTMLButtonElement>(null);
  return (
    <>
      <Tooltip text={chip.title}>
        <button
          ref={button}
          type="button"
          className="strip-chip"
          data-tone={chip.tone}
          aria-expanded={chip.asks !== null ? asking : undefined}
          onClick={() => {
            if (chip.asks !== null) setAsking((a) => !a);
            else onOpen(chip.target);
          }}
        >
          <Icon name={chip.icon} size={12} />
          <span className="strip-chip-key">{chip.label}</span>
          <span className="strip-chip-reason">{chip.reason}</span>
        </button>
      </Tooltip>
      {asking && chip.asks !== null && (
        <AsksPopover asks={chip.asks} anchor={button} onOpen={onOpen} onClose={close} />
      )}
    </>
  );
}

function Group({
  group,
  project,
  onOpen,
}: {
  group: StripGroup;
  project: string;
  onOpen: (target: StripTarget) => void;
}) {
  return (
    <div className="strip-group" data-group={group.id}>
      <span className="strip-head" data-tone={group.tone}>
        <Icon name={group.icon} size={13} />
        <span>{group.title}</span>
      </span>
      {group.chips.map((chip) => (
        <Chip key={`${chip.label} ${chip.reason}`} chip={chip} onOpen={onOpen} />
      ))}
      {group.resume_all !== null && (
        <button
          type="button"
          className="strip-resume-all"
          onClick={() => {
            openOverlay("resume_all", { project });
          }}
        >
          <Icon name="play" size={14} />
          <span>{group.resume_all}</span>
        </button>
      )}
    </div>
  );
}

export interface StripBodyProps {
  view: StripView;
  project: string;
  onOpen?: (target: StripTarget) => void;
  onNext?: () => void;
}

/** The strip for a loaded view (also what the tests and the gallery render). */
export function StripBody({ view, project, onOpen = ignore, onNext = ignore }: StripBodyProps) {
  return (
    <div className="strip" role="region" aria-label="Needs you" data-empty={view.empty !== null || undefined}>
      {view.empty !== null && (
        <div className="strip-empty">
          <Icon name={view.empty.icon} size={14} />
          <span className="strip-empty-title">{view.empty.title}</span>
          {view.empty.text !== null && <span className="strip-empty-text">{view.empty.text}</span>}
        </div>
      )}
      <div className="strip-groups">
        {view.groups.map((group, i) => (
          <Fragment key={group.id}>
            {i > 0 && <span className="strip-divider" />}
            <Group group={group} project={project} onOpen={onOpen} />
          </Fragment>
        ))}
      </div>
      {view.next !== null && (
        <div className="strip-next-wrap">
          <button
            type="button"
            className="strip-next"
            data-elsewhere={view.next.elsewhere || undefined}
            onClick={onNext}
          >
            <Icon name={view.next.icon} size={view.next.elsewhere ? 13 : 14} />
            <span>{view.next.label}</span>
          </button>
          {view.next.keys !== null && <Kbd keys={view.next.keys} />}
        </div>
      )}
    </div>
  );
}

/** The strip of the project on screen; asks again with each change of what the page shows (Next's label follows). */
export function NeedsYouStrip() {
  // Read for their changes: what the page shows comes from these stores (`openOf`).
  const { project } = useSelection();
  useStore(shellLayout);
  useStore(shownAgents);
  useStore(lastOpened);
  const params = project === null ? null : stripParams(project, openOf());
  const asked = JSON.stringify(params);
  const load = useCallback(
    () =>
      asked === "null"
        ? Promise.resolve(null)
        : (request("strip.view", JSON.parse(asked) as StripParams) as Promise<StripView>),
    [asked],
  );
  const { view } = useServiceView(load, STRIP_EVENTS);
  if (project === null || view === undefined || view === null) return null;
  return (
    <StripBody
      view={view}
      project={project}
      onOpen={(target) => {
        openTarget(target);
      }}
      onNext={next}
    />
  );
}

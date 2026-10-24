// The Needs you strip (T-0459; NYS-01, NYS-02; d0114; *24 → Row · The two views* `s24-03`, *Row · Needs you: the
// next project, and every group* `s24-12`): under the board's toolbar in both views, the open project's items as
// chips in the queue's order and groups, with Next (⌘J) at its end; "Needs you · 0 here" and "Next · billing-api, 1"
// when this project has none. A chip opens its ticket, its epic's Lead tab or the Project agent panel; a lead's
// suggestion opens "Wishlist lead suggests" with Not now · Confirm; Resume all asks first. Everything it says is
// `strip.view`'s. The Board view and the List view each render `<NeedsYouStrip />` once.
import { Fragment, useCallback, useRef, useState } from "react";

import { shellLayout } from "../../app/layout";
import { openOverlay } from "../../app/overlays";
import { useSelection } from "../../app/selection";
import { useStore } from "../../app/store";
import { Button, Kbd, Popover, Tooltip } from "../../components";
import type { StripChip } from "../../generated/StripChip";
import type { StripGroup } from "../../generated/StripGroup";
import type { StripParams } from "../../generated/StripParams";
import type { StripSuggestion } from "../../generated/StripSuggestion";
import type { StripTarget } from "../../generated/StripTarget";
import type { StripView } from "../../generated/StripView";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { shownAgents } from "../agent_panel/logic";
import { dismissSuggestion, lastOpened, next, openOf, openTarget, stripParams } from "./logic";

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

function Chip({ chip, onOpen }: { chip: StripChip; onOpen: (target: StripTarget) => void }) {
  return (
    <Tooltip text={chip.title}>
      <button
        type="button"
        className="strip-chip"
        data-tone={chip.tone}
        onClick={() => {
          onOpen(chip.target);
        }}
      >
        <Icon name={chip.icon} size={12} />
        <span className="strip-chip-key">{chip.label}</span>
        <span className="strip-chip-reason">{chip.reason}</span>
      </button>
    </Tooltip>
  );
}

/** A lead's suggestion: its chip, and "Wishlist lead suggests" with Not now · Confirm under it while open. */
function SuggestionChip({ suggestion, open = false }: { suggestion: StripSuggestion; open?: boolean }) {
  const anchor = useRef<HTMLButtonElement>(null);
  const [shown, setShown] = useState(open);
  const close = useCallback(() => {
    setShown(false);
  }, []);
  return (
    <>
      <button
        ref={anchor}
        type="button"
        className="strip-chip"
        data-tone="investigate"
        data-suggestion
        aria-expanded={shown}
        onClick={() => {
          setShown((s) => !s);
        }}
      >
        <span className="strip-chip-reason">{suggestion.label}</span>
        <Icon name="chevron-down" size={11} />
      </button>
      {shown && (
        <Popover label={suggestion.heading} anchor={anchor} onClose={close}>
          <SuggestionCard
            suggestion={suggestion}
            onNotNow={() => {
              close();
              dismissSuggestion(suggestion.id).catch(ignore);
            }}
            onConfirm={() => {
              close();
              openOverlay(suggestion.dialog === "close" ? "close_ticket" : "delete_ticket", {
                ticket: suggestion.ticket,
                suggestion: { id: suggestion.id, line: suggestion.line },
              });
            }}
          />
        </Popover>
      )}
    </>
  );
}

/** "Wishlist lead suggests", the lead's reason, Not now · Confirm (also what the gallery draws in place). */
export function SuggestionCard({
  suggestion,
  onNotNow,
  onConfirm,
}: {
  suggestion: StripSuggestion;
  onNotNow?: () => void;
  onConfirm?: () => void;
}) {
  return (
    <div className="strip-suggestion">
      <div className="strip-suggestion-head">
        <Icon name="lightbulb" size={14} />
        <span>{suggestion.heading}</span>
      </div>
      <p className="strip-suggestion-text">{suggestion.text}</p>
      <div className="strip-suggestion-buttons">
        <Button variant="ghost" label={suggestion.not_now} onClick={onNotNow} />
        <span className="strip-gap" />
        <Button label={suggestion.confirm} icon="check" onClick={onConfirm} />
      </div>
    </div>
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
      {group.suggestions.map((s) => (
        <SuggestionChip key={s.id} suggestion={s} />
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

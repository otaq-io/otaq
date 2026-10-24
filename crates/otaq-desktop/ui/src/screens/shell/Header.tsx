// The header (SHL-02, SHL-03; d0087; *02 · App shell → Row · Title bars (0086 · V)*, *Row · Header states (0086 ·
// V)*): three items. The shown project with its base branch (the folder on the name's hover; a click opens the project
// menu, FED-09 `s26-08`: Change flow…, Export flow…, Project settings…, Open in Finder, Open a terminal here),
// the search entry, Activity (agents and processes across all projects, the amber usage warning; a click or ⇧⌘U opens
// its popover) and the place for Pause after it (the `header_pause` slot, T-0457; SHB-01); Needs you is the board's
// strip (d0114, T-0459). Everything it says comes from `shell.header`. A project in Flow setup (ASK-04, T-0436)
// shows "billing-api · Flow setup" with "2 agents at work" or "2 proposals" in place of its base branch, and no search
// (it has no board to filter). The row is the title bar: its empty parts
// move the window, and the title bar's ends (T-0214) give it its outer padding.
import { createRef, type ReactNode, type RefObject, useCallback, useEffect, useRef, useState } from "react";

import { toggleOverlay, useOpenOverlays } from "../../app/overlays";
import { useRegistries } from "../../app/registry";
import { useSelection } from "../../app/selection";
import { createStore, useStore } from "../../app/store";
import { AnchoredMenu, Kbd, MetaChip, StatusPill, Tooltip } from "../../components";
import type { ActivityButton } from "../../generated/ActivityButton";
import type { HeaderProject } from "../../generated/HeaderProject";
import type { HeaderView } from "../../generated/HeaderView";
import type { ShellParams } from "../../generated/ShellParams";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { noticesStore } from "../notices/notices";
import { projectMenuChoice } from "./projectMenu";

/** The service events that change what the header shows. */
export const HEADER_EVENTS = [
  "projects_changed",
  "ticket_changed",
  "ticket_deleted",
  "session_state",
  "usage_changed",
  "run_changed",
  "settings_changed",
  // A project's Flow setup: its agents at work and proposals.
  "flow_setup_changed",
  // The project menu's flow in use and its waiting change.
  "flow_changed",
] as const;

/** Activity's reset times pass with the clock, not with an event: the header asks again this often. */
const CLOCK_REFRESH_MS = 60_000;

/** The header's last view: the Activity popover asks again with each new one. */
export const headerStore = createStore<HeaderView | undefined>(undefined);

/** Activity, which its popover (and the processes menu) sits under. */
export const activityAnchor = createRef<HTMLButtonElement>();

const ignore = () => undefined;

/** The project's name and base branch; the name's hover is the folder, a click the project menu (FED-09). */
function ProjectItem({ project }: { project: HeaderProject }) {
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  return (
    <div className="header-project">
      <Tooltip text={project.path}>
        <button
          ref={trigger}
          type="button"
          className="header-project-name"
          aria-haspopup="menu"
          aria-expanded={open}
          onClick={() => {
            setOpen((o) => !o);
          }}
        >
          {project.name}
        </button>
      </Tooltip>
      {project.flow_setup === null ? (
        <MetaChip icon="git-branch" label={project.base_branch} />
      ) : (
        <>
          <span className="header-flow-setup-dot">·</span>
          <span className="header-flow-setup">{project.flow_setup.label}</span>
          {project.flow_setup.status !== null && <StatusPill pill={project.flow_setup.status} />}
        </>
      )}
      {open && (
        <AnchoredMenu
          anchor={trigger}
          label={project.name}
          head={{ title: project.menu.title, detail: project.menu.path }}
          items={project.menu.items}
          width={300}
          onChoose={(id) => {
            projectMenuChoice(project, id).catch(ignore);
          }}
          onClose={(how) => {
            setOpen(false);
            if (how === "key") trigger.current?.focus();
          }}
        />
      )}
    </div>
  );
}

export interface ActivityChipProps {
  activity: ActivityButton;
  /** Whether its popover is open (the outline turns accent). */
  open?: boolean;
  /** The live header's button is the popover's anchor; the gallery's copies are not. */
  anchor?: RefObject<HTMLButtonElement | null>;
  /** The link to the service is down: the count is the last one heard, so it dims (#58). */
  stale?: boolean;
}

/** Activity: "● 6 running · ▷ 3", the amber warning when the view has one, and its hover. A click toggles the popover. */
export function ActivityChip({ activity, open = false, anchor, stale = false }: ActivityChipProps) {
  const { agents, warning } = activity;
  return (
    <Tooltip text={activity.hover.title} detail={activity.hover.lines}>
      <button
        ref={anchor}
        type="button"
        className="header-activity"
        data-open={open || undefined}
        data-limit={warning?.limit || undefined}
        data-stale={stale || undefined}
        aria-label={activity.label}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => {
          toggleOverlay("activity");
        }}
      >
        <span
          className="header-dot"
          data-active={(agents.active && !stale) || undefined}
          data-paused={agents.paused || undefined}
        />
        <span className="header-mono">{agents.count}</span>
        <span className="header-muted">{agents.label}</span>
        {activity.processes !== null && (
          <span className="header-processes">
            <span className="header-muted">·</span>
            <span className="header-mono">▷ {activity.processes}</span>
          </span>
        )}
        {warning && (
          <span className="header-warning" style={{ color: `var(--${warning.token})` }}>
            <Icon name="gauge" size={12} />
            <span>{warning.text}</span>
          </span>
        )}
        <Icon name="chevron-down" size={12} />
      </button>
    </Tooltip>
  );
}

export interface HeaderBarProps {
  view: HeaderView;
  /** Whether the Activity popover is open (the button's outline turns accent). */
  activityOpen?: boolean;
  /** The search entry opens the board's filter (Filter the board's handler, KEY-09). */
  onSearch: () => void;
  /** The board's search and filter field (the `header_search` slot, T-0221), drawn in place of the entry. */
  search?: ReactNode;
  /** Pause, after Activity (the `header_pause` slot, PAU-08, T-0457; SHB-01). */
  pause?: ReactNode;
  /** The link to the service is down (`Notices.service_down`): Activity's count dims. */
  serviceDown?: boolean;
}

export function HeaderBar({
  view,
  activityOpen = false,
  onSearch,
  search,
  pause,
  serviceDown = false,
}: HeaderBarProps) {
  // A project in Flow setup has no board, so no search.
  const searches = view.project !== null && view.project.flow_setup === null;
  return (
    <div className="header">
      {view.project && <ProjectItem project={view.project} />}
      <div className="header-gap" />
      {searches && search}
      {searches && !search && (
        <button type="button" className="header-search" onClick={onSearch}>
          <Icon name="search" size={13} />
          <span className="header-search-text">{view.search.text}</span>
          {view.search.keys !== null && <Kbd keys={view.search.keys} />}
        </button>
      )}
      <div className="header-gap" />
      <ActivityChip
        activity={view.activity}
        open={activityOpen}
        anchor={activityAnchor}
        stale={serviceDown}
      />
      {pause}
    </div>
  );
}

/** The header slot: the view for the shown project, refetched on every event that changes a count. */
export function ShellHeader() {
  const app = useRegistries();
  const { project } = useSelection();
  const activityOpen = useOpenOverlays().some((o) => o.id === "activity");
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => {
      setTick((n) => n + 1);
    }, CLOCK_REFRESH_MS);
    return () => {
      window.clearInterval(timer);
    };
  }, []);
  const load = useCallback(() => {
    const params: ShellParams = { project };
    return request("shell.header", params) as Promise<HeaderView>;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a new tick is a new load: Activity's reset times move with the clock
  }, [project, tick]);
  const { view } = useServiceView(load, HEADER_EVENTS);
  const serviceDown = useStore(noticesStore)?.service_down ?? false;
  useEffect(() => {
    headerStore.set(view);
  }, [view]);
  if (view === undefined) return null;
  const Search = app.slots.get("header_search")?.Component;
  const Pause = app.slots.get("header_pause")?.Component;
  return (
    <HeaderBar
      view={view}
      activityOpen={activityOpen}
      search={Search && <Search />}
      pause={Pause && <Pause />}
      serviceDown={serviceDown}
      onSearch={() => {
        app.shortcuts.run({ action: "filter_board" });
      }}
    />
  );
}

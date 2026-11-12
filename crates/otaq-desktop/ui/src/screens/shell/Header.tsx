// The header (SHL-02, SHL-03; d0087): the title bar. Left, the shown project with its base branch (the folder on the
// name's hover; a click opens the project switcher: every project with a dot on one that needs you and a check on this
// one, then Add project…; the name only switches projects or adds one); centre, the search entry; right, the usage
// chip only when it needs you (amber, "Claude Code · 15% left"), then Running (agents and apps across all projects; a
// click or ⇧⌘U opens its popover) and Pause (the `header_pause` slot, T-0457), both only while something runs, held
// at the right end so Running leaving never moves Pause under the pointer (NEW-b1). Needs
// you is the board's strip (d0114, T-0459). Everything it says comes from `shell.header`. A project in Flow setup
// (ASK-04, T-0436) shows "billing-api · Flow setup" with "2 agents at work" or "2 proposals" in place of its base
// branch, and no search (it has no board to filter). The row is the title bar: its empty parts move the window, and
// the title bar's ends (T-0214) give it its outer padding.
import {
  createRef,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";

import { toggleOverlay, useOpenOverlays } from "../../app/overlays";
import { useRegistries } from "../../app/registry";
import { useSelection } from "../../app/selection";
import { createStore, useStore } from "../../app/store";
import { Kbd, MetaChip, Popover, Tooltip } from "../../components";
import type { ActivityButton } from "../../generated/ActivityButton";
import type { ActivityWarning } from "../../generated/ActivityWarning";
import type { HeaderProject } from "../../generated/HeaderProject";
import type { HeaderView } from "../../generated/HeaderView";
import type { ProjectMenuView } from "../../generated/ProjectMenuView";
import type { ShellParams } from "../../generated/ShellParams";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { noticesStore } from "../notices/notices";
import { switcherChoice } from "./projectMenu";

/** The service events that change what the header shows. */
export const HEADER_EVENTS = [
  "projects_changed",
  "ticket_changed",
  "ticket_deleted",
  "session_state",
  "usage_changed",
  "run_changed",
  "settings_changed",
  // The switcher's needs-you dots.
  "needs_you_changed",
] as const;

/** The usage chip's reset times pass with the clock, not with an event: the header asks again this often. */
const CLOCK_REFRESH_MS = 60_000;

/** The header's last view: the Running popover asks again with each new one. */
export const headerStore = createStore<HeaderView | undefined>(undefined);

/** What the Running popover sits under: Running while it shows, else the usage chip. */
export const activityAnchor = createRef<HTMLButtonElement>();

export interface SwitcherBoxProps {
  menu: ProjectMenuView;
  /** A project's key, or `null` for Add project…. */
  onChoose: (key: string | null) => void;
  /** Takes the keyboard on its first row when it opens (the title bar's; not the gallery's copies). */
  focus?: boolean;
}

/** Takes the keyboard when it appears (a stable ref, so only then). */
function focusOnMount(element: HTMLButtonElement | null): void {
  element?.focus();
}

/** The switcher's rows: each project's tile, name and folder, the needs-you dot and the check; then Add project…. */
export function SwitcherBox({ menu, onChoose, focus = false }: SwitcherBoxProps) {
  const first = focus ? focusOnMount : undefined;
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const items = Array.from(e.currentTarget.querySelectorAll<HTMLButtonElement>("button"));
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = e.key === "ArrowDown" ? (at + 1) % items.length : (at - 1 + items.length) % items.length;
    items[next]?.focus();
    e.preventDefault();
  };
  return (
    // eslint-disable-next-line jsx-a11y/interactive-supports-focus -- its items take the focus
    <div className="switcher" role="menu" aria-label={menu.title} onKeyDown={onKeyDown}>
      {menu.rows.map((row, i) => (
        <button
          key={row.key}
          ref={i === 0 ? first : undefined}
          type="button"
          role="menuitem"
          className="switcher-row"
          aria-label={row.label}
          aria-current={row.current || undefined}
          onClick={() => {
            onChoose(row.key);
          }}
        >
          <span className="switcher-tile" data-current={row.current || undefined}>
            {row.key}
          </span>
          <span className="switcher-text">
            <span className="switcher-name">{row.name}</span>
            <span className="switcher-path">{row.path}</span>
          </span>
          {row.needs_you && <span className="switcher-dot" aria-hidden />}
          {row.current && (
            <span className="switcher-check">
              <Icon name="check" size={14} />
            </span>
          )}
        </button>
      ))}
      <div className="switcher-rule" />
      <button
        type="button"
        role="menuitem"
        className="switcher-add"
        ref={menu.rows.length === 0 ? first : undefined}
        onClick={() => {
          onChoose(null);
        }}
      >
        <Icon name="plus" size={14} />
        <span>{menu.add}</span>
      </button>
    </div>
  );
}

/** The project's name and base branch; the name's hover is the folder, a click the project switcher. */
function ProjectItem({ project }: { project: HeaderProject }) {
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const close = useCallback(() => {
    setOpen(false);
  }, []);
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
          <span className="header-project-text">{project.name}</span>
          <Icon name="chevron-down" size={12} />
        </button>
      </Tooltip>
      <MetaChip icon="git-branch" label={project.base_branch} />
      {open && (
        <Popover label={project.menu.title} anchor={trigger} onClose={close}>
          <SwitcherBox
            menu={project.menu}
            focus
            onChoose={(key) => {
              close();
              switcherChoice(key);
            }}
          />
        </Popover>
      )}
    </div>
  );
}

export interface UsageChipProps {
  warning: ActivityWarning;
  open?: boolean;
  /** The Running popover's anchor while Running is hidden. */
  anchor?: RefObject<HTMLButtonElement | null> | undefined;
}

/** The usage chip: "Claude Code · 15% left", amber, only when it needs you; a click opens the Running popover. */
export function UsageChip({ warning, open = false, anchor }: UsageChipProps) {
  return (
    <button
      ref={anchor}
      type="button"
      className="header-usage"
      data-open={open || undefined}
      aria-haspopup="dialog"
      aria-expanded={open}
      onClick={() => {
        toggleOverlay("activity");
      }}
    >
      <Icon name="gauge" size={13} />
      <span>{warning.text}</span>
    </button>
  );
}

export interface ActivityChipProps {
  activity: ActivityButton;
  /** Whether its popover is open (the outline turns accent). */
  open?: boolean;
  /** The live header's button is the popover's anchor; the gallery's copies are not. */
  anchor?: RefObject<HTMLButtonElement | null> | undefined;
  /** The link to the service is down: the count is the last one heard, so it dims (#58). */
  stale?: boolean;
}

/** Running: "● 4 running · ▷ 1" and its hover. A click toggles the popover. */
export function ActivityChip({ activity, open = false, anchor, stale = false }: ActivityChipProps) {
  const { agents } = activity;
  return (
    <Tooltip text={activity.hover.title} detail={activity.hover.lines}>
      <button
        ref={anchor}
        type="button"
        className="header-activity"
        data-open={open || undefined}
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
        <Icon name="chevron-down" size={12} />
      </button>
    </Tooltip>
  );
}

export interface HeaderBarProps {
  view: HeaderView;
  /** Whether the Running popover is open (the button's outline turns accent). */
  activityOpen?: boolean;
  /** The search entry opens the board's filter (Filter the board's handler, KEY-09). */
  onSearch: () => void;
  /** The board's search and filter field (the `header_search` slot, T-0221), drawn in place of the entry. */
  search?: ReactNode;
  /** Pause, after Running (the `header_pause` slot, PAU-08, T-0457), which shows only while agents run. */
  pause?: ReactNode;
  /** The link to the service is down (`Notices.service_down`): Running's count dims. */
  serviceDown?: boolean;
  /** The live header's: the Running popover's anchor. */
  anchor?: RefObject<HTMLButtonElement | null>;
}

export function HeaderBar({
  view,
  activityOpen = false,
  onSearch,
  search,
  pause,
  serviceDown = false,
  anchor,
}: HeaderBarProps) {
  const searches = view.project !== null;
  const { activity } = view;
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
      {/* Held at the title bar's right end: Running leaving (its last agent waits at a prompt) moves nothing after
          it, so Pause never slides under a click aimed at Running (NEW-b1). */}
      <div className="header-end">
        {activity.warning && (
          <UsageChip
            warning={activity.warning}
            open={activityOpen && !activity.shown}
            anchor={activity.shown ? undefined : anchor}
          />
        )}
        {activity.shown && (
          <ActivityChip activity={activity} open={activityOpen} anchor={anchor} stale={serviceDown} />
        )}
        {pause}
      </div>
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
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a new tick is a new load: the reset times move with the clock
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
      anchor={activityAnchor}
      onSearch={() => {
        app.shortcuts.run({ action: "filter_board" });
      }}
    />
  );
}

// The ticket sheet (SHT-01, SHT-02; T-0464; *25 → Opening a ticket*, *Full page, and with the agent open*): 1040 px
// over the board (920 px beside the Project agent; only the board dims), or the whole main area at full page (⤢,
// "← Board"). The header: breadcrumb, title and status, Move… (the drag's columns), ⋯ (the ticket's menu), ⤢ and ✕;
// the details line (Assignee, Model, Working, Priority, Branch, Epic; a value with a menu changes it); the flow
// path; the tabs Overview · Session · Files · Activity (`1`–`4`, `g o` / `s` / `f` / `a`, ⌃Tab), each tab's body
// registered in `sheetTabs`. An epic (SHT-07, `s25-11`) shows "1 of 5 merged" and its lead's state by its title, no
// Move…, and its tabs Overview · Tickets (with their count) · Lead · Files · Activity. It comes in as the drawer of 0095 (the frame's panel layer). Everything it says is Rust's
// (`panel.view`'s `sheet`); it keeps open menus only. Its keys: Esc, `e`, j / k (⌘[ / ⌘]), the tab keys.
import { createContext, Fragment, useCallback, useContext, useEffect, useRef, useState } from "react";

import { mainWidth, shellLayout, toggleExpanded } from "../../app/layout";
import { openOverlay } from "../../app/overlays";
import { useRegistries, useShortcut } from "../../app/registry";
import { closeTicket, openTicket, selection, showTab, stepTicket, useSelection } from "../../app/selection";
import { useStore } from "../../app/store";
import { AnchoredMenu, IconButton, StatusPill, Tabs, ToolAvatar, Tooltip } from "../../components";
import type { MenuItem } from "../../generated/MenuItem";
import type { PanelHint } from "../../generated/PanelHint";
import type { PanelLoad } from "../../generated/PanelLoad";
import type { PanelView } from "../../generated/PanelView";
import type { SheetContent } from "../../generated/SheetContent";
import type { SheetDetail } from "../../generated/SheetDetail";
import type { SheetStep } from "../../generated/SheetStep";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { dropCard } from "../board/Board";
import { DescriptionSaveBar } from "../overview/SaveBar";
import { EpicHeadPills } from "../epic/EpicSheet";
import { ReviewBar } from "../files/ReviewBar";
import { MoreMenu, PANEL_EVENTS, type PanelActions, usePanelActions } from "./actions";
import {
  contentOf,
  numberedSheetTab,
  sheetWidth,
  shownSheetTab,
  steppedSheetTab,
  tabIndicator,
} from "./logic";

const ignore = () => undefined;

/**
 * What the sheet's tab bodies read: the ticket's view, its actions (Overview's action bar) and what the selection asks
 * to show (Session's Agent, the check, Run or Shell).
 */
export interface SheetState {
  view: PanelView;
  actions: PanelActions;
  selected: SheetContent | null;
}

export const SheetContext = createContext<SheetState | null>(null);

/** The open sheet's view and actions; `null` outside a sheet. */
export function useSheet(): SheetState | null {
  return useContext(SheetContext);
}

/** A crumb, a value or a button's tooltip with the user's key. */
function HintButton({
  icon,
  hint,
  onClick,
}: {
  icon: Parameters<typeof IconButton>[0]["icon"];
  hint: PanelHint;
  onClick: () => void;
}) {
  return (
    <Tooltip text={hint.text} keys={hint.keys ?? undefined}>
      <IconButton icon={icon} label={hint.text} onClick={onClick} />
    </Tooltip>
  );
}

/** Move… (SHT-01): the board's columns with the drag's verdicts (`panel.move_menu`); a pick is the drag's drop. */
function MoveButton({ view }: { view: PanelView }) {
  const anchor = useRef<HTMLButtonElement>(null);
  const [items, setItems] = useState<MenuItem[] | null>(null);
  const key = view.key;
  return (
    <>
      <button
        ref={anchor}
        type="button"
        className="sheet-move"
        aria-haspopup="menu"
        aria-expanded={items !== null}
        onClick={() => {
          if (items !== null) {
            setItems(null);
            return;
          }
          (request("panel.move_menu", { key }) as Promise<MenuItem[]>).then(setItems, ignore);
        }}
      >
        <Icon name="arrow-right" size={14} />
        <span>{view.sheet.move_label}</span>
      </button>
      {items !== null && (
        <AnchoredMenu
          anchor={anchor}
          align="end"
          label={view.sheet.move_label}
          items={items}
          width={240}
          onChoose={(target) => {
            setItems(null);
            // The Approve choice a drop on Approved asks for opens as the panel's ▾ did.
            void dropCard(key, target, () => {
              openOverlay("local_merge", { ticket: key, menu: true });
            });
          }}
          onClose={() => {
            setItems(null);
          }}
        />
      )}
    </>
  );
}

/** One value of the details line; with a menu (Priority, the stage's agent before it starts) or a link (Epic). */
function DetailValue({ view, detail }: { view: PanelView; detail: SheetDetail }) {
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const content = (
    <>
      {detail.working && <span className="sheet-detail-dot" />}
      {detail.avatar && <ToolAvatar avatar={detail.avatar} />}
      {detail.value !== "" && (
        <span className="sheet-detail-value" data-mono={detail.mono || undefined}>
          {detail.value}
        </span>
      )}
      {detail.note !== null && <span className="sheet-detail-note">{detail.note}</span>}
    </>
  );
  const change = detail.change;
  if (change === null) return <span className="sheet-detail-content">{content}</span>;
  if (change.kind === "open") {
    return (
      <button
        type="button"
        className="sheet-detail-content sheet-detail-button"
        onClick={() => {
          openTicket(change.key);
        }}
      >
        {content}
      </button>
    );
  }
  return (
    <>
      <button
        ref={anchor}
        type="button"
        className="sheet-detail-content sheet-detail-button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`${detail.label}: ${detail.value}`}
        onClick={() => {
          setOpen(!open);
        }}
      >
        {content}
        <Icon name="chevron-down" size={11} />
      </button>
      {open && (
        <AnchoredMenu
          anchor={anchor}
          label={detail.label}
          items={change.items}
          width={200}
          onChoose={(item) => {
            setOpen(false);
            if (change.kind === "priority") {
              request("panel.priority", { key: view.key, item }).catch(ignore);
            } else {
              request("panel.set_choice", { key: view.key, stage: change.stage, choice: item }).catch(ignore);
            }
          }}
          onClose={() => {
            setOpen(false);
          }}
        />
      )}
    </>
  );
}

function Details({ view }: { view: PanelView }) {
  return (
    <div className="sheet-details">
      {view.sheet.details.map((detail) => (
        <span key={detail.label} className="sheet-detail">
          <span className="sheet-detail-label">{detail.label}</span>
          <DetailValue view={view} detail={detail} />
        </span>
      ))}
    </div>
  );
}

/** The flow path (`s25-03`): a node per stage, done ✓, the one it is in ringed (turquoise when it needs you). */
function FlowPath({ steps }: { steps: readonly SheetStep[] }) {
  if (steps.length === 0) return null;
  const reached = (step: SheetStep | undefined) => step?.state === "done" || step?.state === "current";
  return (
    <ol className="sheet-path">
      {steps.map((step, i) => (
        <Fragment key={`${String(i)}-${step.stage}`}>
          {i > 0 && (
            <li
              className="sheet-step-line"
              role="presentation"
              data-done={(steps[i - 1]?.state === "done" && reached(step)) || undefined}
            />
          )}
          <li
            className="sheet-step"
            data-state={step.state}
            data-attention={step.attention || undefined}
            aria-current={step.state === "current" ? "step" : undefined}
          >
            <span className="sheet-step-node">
              {step.state === "done" && <Icon name="check" size={11} />}
              {step.state === "current" && <span className="sheet-step-dot" />}
            </span>
            <span className="sheet-step-name">{step.stage}</span>
          </li>
        </Fragment>
      ))}
    </ol>
  );
}

export interface SheetBodyProps {
  view: PanelView;
  /** What the selection asks the sheet to show (`null`: the tab it opens on). */
  tab: SheetContent | null;
  fullPage: boolean;
  /** Its width over the board; at full page it fills the main area. */
  width: number;
}

/** The sheet for a loaded view (also what the markup tests render). */
export function SheetBody({ view, tab, fullPage, width }: SheetBodyProps) {
  const app = useRegistries();
  const actions = usePanelActions(view);
  const sheet = view.sheet;
  const shown = shownSheetTab(sheet.tabs, tab, sheet.opens_on);
  const Body = shown ? app.sheetTabs.get(shown.tab)?.Component : undefined;
  // At full page the project's crumb is "← Board", back to the sheet over the board (`s25-10`).
  const crumbs = fullPage ? sheet.crumbs.slice(1) : sheet.crumbs;
  return (
    <SheetContext value={{ view, actions, selected: tab }}>
      <section
        className="sheet"
        aria-label={`${view.key} ${view.title}`}
        data-full-page={fullPage || undefined}
        style={fullPage ? undefined : { width }}
      >
        <div className="sheet-header">
          <div className="sheet-heading">
            <nav className="sheet-crumbs" aria-label="Breadcrumb">
              {fullPage && (
                <Tooltip text={sheet.back.text} keys={sheet.back.keys ?? undefined}>
                  <button type="button" className="sheet-crumb sheet-crumb-link" onClick={toggleExpanded}>
                    <Icon name="arrow-left" size={13} />
                    <span>{sheet.board}</span>
                  </button>
                </Tooltip>
              )}
              {crumbs.map((crumb, i) => (
                <span key={`${String(i)}-${crumb.text}`} className="sheet-crumb-part">
                  {(i > 0 || fullPage) && <span className="sheet-crumb-sep">/</span>}
                  {crumb.opens === null ? (
                    <span className="sheet-crumb" data-key={crumb.key || undefined}>
                      {crumb.text}
                    </span>
                  ) : (
                    <button
                      type="button"
                      className="sheet-crumb sheet-crumb-link"
                      data-key={crumb.key || undefined}
                      onClick={() => {
                        if (crumb.opens !== null) openTicket(crumb.opens);
                      }}
                    >
                      {crumb.text}
                    </button>
                  )}
                </span>
              ))}
            </nav>
            <div className="sheet-title-row">
              {sheet.epic !== null && (
                <span className="sheet-title-icon">
                  <Icon name="layers" size={18} />
                </span>
              )}
              <h2 className="sheet-title">{view.title}</h2>
              {sheet.epic !== null && <EpicHeadPills head={sheet.epic} />}
              {sheet.epic?.lead == null && <StatusPill pill={view.pill} />}
            </div>
          </div>
          <div className="sheet-actions">
            {sheet.epic === null && <MoveButton key={view.key} view={view} />}
            <MoreMenu view={view} onChoose={actions.choose} />
            <HintButton
              icon={fullPage ? "minimize-2" : "maximize-2"}
              hint={fullPage ? sheet.back : sheet.full_page}
              onClick={toggleExpanded}
            />
            <HintButton
              icon="x"
              hint={view.close_hint}
              onClick={() => {
                closeTicket();
              }}
            />
          </div>
        </div>
        <Details view={view} />
        <FlowPath steps={sheet.steps} />
        <div className="sheet-tabs">
          <Tabs
            label={view.key}
            items={sheet.tabs.map((t) => ({
              icon: t.icon,
              label: t.label,
              indicator: tabIndicator(t.mark, t.count),
              keys: t.keys ?? undefined,
            }))}
            active={Math.max(
              sheet.tabs.findIndex((t) => t.tab === shown?.tab),
              0,
            )}
            onSelect={(i) => {
              const next = sheet.tabs[i];
              if (next) showTab(contentOf(next, tab));
            }}
          />
          <DescriptionSaveBar ticket={view.key} />
        </div>
        <div className="sheet-body" data-tab={shown?.tab}>
          {Body && <Body ticket={view.key} />}
        </div>
        {/* While the check is yours, under every tab (SHT-06): the sheet then opens on Files (`sheet::opens_on`). */}
        {sheet.opens_on === "files" && <ReviewBar ticket={view.key} />}
      </section>
    </SheetContext>
  );
}

/** Whether the keyboard is in the Project agent's panel (the frame's `pane` slot). */
function inAgentPanel(): boolean {
  return (
    typeof document !== "undefined" && document.activeElement?.closest('[data-slot="agent_panel"]') != null
  );
}

/**
 * The sheet's keys while it shows `view` (0097, SHT-02): Esc closes, `e` is full page, j / k (⌘] / ⌘[) the next and
 * previous ticket of its column keeping the tab, `1`–`4` and `g o` / `s` / `f` / `a` a tab (`g a` is Activity's
 * popover while no sheet takes it), ⌃Tab / ⌃⇧Tab the next and previous tab, Open diff Files.
 */
export function useSheetKeys(view: PanelView | null): void {
  const current = useRef<PanelView | null>(view);
  useEffect(() => {
    current.current = view;
  }, [view]);
  // Esc in the Project agent's panel is the panel's (it folds): the sheet stays open (T-0458's Q-D). So is Esc while
  // the panel lies expanded over the sheet (T-0502).
  useShortcut(
    "close_panel",
    useCallback(() => {
      if (inAgentPanel() || shellLayout.get().agentExpanded) return false;
      closeTicket();
      return true;
    }, []),
  );
  useShortcut("expand_panel", toggleExpanded);
  useShortcut(
    "previous_ticket",
    useCallback(() => {
      const previous = current.current?.previous;
      if (previous) stepTicket(previous);
    }, []),
  );
  useShortcut(
    "next_ticket",
    useCallback(() => {
      const next = current.current?.next;
      if (next) stepTicket(next);
    }, []),
  );
  useShortcut(
    "switch_tab",
    useCallback(({ number }: { number: number }) => {
      const tabs = current.current?.sheet.tabs ?? [];
      const next = numberedSheetTab(tabs, number);
      if (!next) return false;
      showTab(contentOf(next, selection.get().tab));
      return true;
    }, []),
  );
  const stepTab = useCallback((step: 1 | -1) => {
    const sheet = current.current?.sheet;
    if (!sheet) return false;
    const selectedTab = selection.get().tab;
    const next = steppedSheetTab(sheet.tabs, shownSheetTab(sheet.tabs, selectedTab, sheet.opens_on), step);
    if (!next) return false;
    showTab(contentOf(next, selectedTab));
    return true;
  }, []);
  useShortcut(
    "next_tab",
    useCallback(() => stepTab(1), [stepTab]),
  );
  useShortcut(
    "previous_tab",
    useCallback(() => stepTab(-1), [stepTab]),
  );
  // Open diff (0097, palette only unless the user gives it a key): the Files tab, where the ticket has it.
  useShortcut(
    "open_diff",
    useCallback(() => {
      if (!current.current?.sheet.tabs.some((t) => t.contents.includes("files"))) return false;
      showTab("files");
      return true;
    }, []),
  );
}

/** The panel slot's component: the open ticket's sheet, with its keys while it shows. */
export function TicketSheet() {
  const { ticket: selected, tab } = useSelection();
  // While the sheet slides out (the frame keeps it mounted, OT-6) it goes on showing the ticket that was open.
  const [last, setLast] = useState(selected);
  if (selected !== null && selected !== last) setLast(selected);
  const ticket = selected ?? last;
  const layout = useStore(shellLayout);
  const main = useStore(mainWidth);
  const key = ticket ?? "";
  const load = useCallback(() => request("panel.view", { key }) as Promise<PanelLoad>, [key]);
  const { view: load_ } = useServiceView(load, PANEL_EVENTS);
  const view = load_?.view ?? null;
  useSheetKeys(view);

  if (ticket === null) return null;
  // Beside the Project agent's panel the sheet narrows (SHT-01).
  const besideAgent = layout.agentOpen;
  if (view === null || view.key !== ticket) {
    // Loading the next ticket, or gone: `data-loading` until there is something to show (the ticket, or that it's
    // gone), so the frame waits for it before sliding the sheet in (0095).
    return (
      <section className="sheet sheet-gone" data-loading={load_ && load_.view === null ? undefined : ""}>
        {load_ && load_.view === null ? load_.gone : null}
      </section>
    );
  }
  return (
    <SheetBody
      view={view}
      tab={tab}
      fullPage={layout.panelExpanded}
      width={sheetWidth(view.sheet, besideAgent, main)}
    />
  );
}

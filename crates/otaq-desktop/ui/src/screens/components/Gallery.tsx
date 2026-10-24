// The design gallery (T-0217): every foundation and component of *crewboard.pen → 01* on one page, to compare with
// the frames by hand, light and dark, and in any of the 15 themes (the palette switch recolors the page for a look,
// without saving; the theme picker in it saves, as it does in Settings). What comes from Rust (legend, priority, type
// scale, icons, keyboard map, menus) is `gallery.view`; the rest are the frames' sample texts.
import {
  type CSSProperties,
  Fragment,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { FrameBody } from "../../app/Frame";
import { INITIAL_LAYOUT, layoutView, loadLayout } from "../../app/layout";
import { createRegistries, type OverlayProps, RegistriesContext } from "../../app/registry";
import { SequenceHintLine } from "../../app/SequenceHint";
import { PaletteGallery } from "../palette/PaletteGallery";
import { useStore } from "../../app/store";
import { openOverlay } from "../../app/overlays";
import { EpicCard, stateOfLook, TicketCard } from "../../cards";
import {
  AnchoredMenu,
  Banner,
  Button,
  type ButtonVariant,
  Checkbox,
  CountBadge,
  DecisionItem,
  DecisionsList,
  Field,
  IconButton,
  Kbd,
  LeadStatus,
  Menu,
  MetaChip,
  OtaqMark,
  pickTheme,
  Popover,
  PriorityChip,
  PriorityLabel,
  Radio,
  Segmented,
  Select,
  type ShownState,
  StageChoices,
  StatusPill,
  Tabs,
  TextArea,
  TextInput,
  ThemePicker,
  TicketPath,
  Toast,
  Toggle,
  ToolAvatar,
  ToolProblemCallout,
  Tooltip,
  useThemePicker,
} from "../../components";
import type { BoardColumn } from "../../generated/BoardColumn";
import type { BoardGallery } from "../../generated/BoardGallery";
import type { DragVerdict } from "../../generated/DragVerdict";
import type { CardGallery } from "../../generated/CardGallery";
import type { CleanupGallery } from "../../generated/CleanupGallery";
import type { DecisionChangeGallery } from "../../generated/DecisionChangeGallery";
import type { DecisionChangeView } from "../../generated/DecisionChangeView";
import type { DecisionGallery } from "../../generated/DecisionGallery";
import type { EnvVarsGallery } from "../../generated/EnvVarsGallery";
import type { AgentToolsGallery } from "../../generated/AgentToolsGallery";
import type { FlowEditorGallery } from "../../generated/FlowEditorGallery";
import type { ProjectMenuGallery } from "../../generated/ProjectMenuGallery";
import type { OnboardingToolsGallery } from "../../generated/OnboardingToolsGallery";
import type { WarmSwitchGallery } from "../../generated/WarmSwitchGallery";
import type { GalleryView } from "../../generated/GalleryView";
import type { IconName } from "../../generated/IconName";
import type { KeyMapView } from "../../generated/KeyMapView";
import type { ShortcutKey } from "../../generated/ShortcutKey";
import type { LeadGallery } from "../../generated/LeadGallery";
import type { LeadSplit } from "../../generated/LeadSplit";
import type { LeadTabGallery } from "../../generated/LeadTabGallery";
import type { LegendTier } from "../../generated/LegendTier";
import type { PaneGallery } from "../../generated/PaneGallery";
import type { AskBarGallery } from "../../generated/AskBarGallery";
import type { DrawerGallery } from "../../generated/DrawerGallery";
import type { PillView } from "../../generated/PillView";
import type { ToolAvatarGallery } from "../../generated/ToolAvatarGallery";
import type { TrustGallery } from "../../generated/TrustGallery";
import type { NotificationsGallery } from "../../generated/NotificationsGallery";
import type { ShortcutsGallery } from "../../generated/ShortcutsGallery";
import type { StageChoicesGallery } from "../../generated/StageChoicesGallery";
import type { OptionNotesGallery } from "../../generated/OptionNotesGallery";
import type { StripGallery } from "../../generated/StripGallery";
import type { PauseGallery } from "../../generated/PauseGallery";
import {
  MarkdownEditor,
  MarkdownSurface,
  MarkdownToolbar,
  SourceArea,
  SourceToggle,
  useFocusOnLeave,
  useMarkdownEditor,
  useSourceMode,
} from "../../markdown";
import { useServiceView } from "../../service";
import { request } from "../../tauri";
import { Icon } from "../../theme/Icon";
import { THEME_KEYS, type ThemeKey } from "../../theme/generated/tokens";
import { TokenSwatches } from "../../theme/Swatches";
import { Column, type ColumnProps } from "../board/Column";
import { Toolbar } from "../board/Toolbar";
import { LaneHeadRow } from "../board/Lanes";
import { AnswerBox as PanelAnswerBox } from "../overview/NeedsYou";
import { AskBarBody } from "../ask_bar/AskBar";
import "../ask_bar/ask_bar.css";
import { OverviewGallery } from "../overview/OverviewGallery";
import { NewTicketReportGallery } from "../editor/NewTicketReportGallery";
import { SheetGallery } from "../sheet/SheetGallery";
import { FilesGallery } from "../files/FilesGallery";
import { FilesViewGallery } from "../files_view/FilesViewGallery";
import { SessionGallery } from "../session/SessionGallery";
import { ListGallery } from "../list/ListGallery";
import { EpicGallery } from "../epic/EpicGallery";
import { DependenciesGallery } from "../dependencies/DependenciesGallery";
import "../files/files.css";
import { PaneBody, SwitcherList } from "../pane/Pane";
import "../pane/pane.css";
import { DrawerBody } from "../terminals/Drawer";
import "../terminals/terminals.css";
import { CleanupBody } from "../recovery/Cleanup";
import "../recovery/recovery.css";
import { StripBody, SuggestionCard } from "../strip/Strip";
import { PauseButton, PauseMenuBox } from "../pause/Pause";
import { SuggestedBy } from "../strip/SuggestedBy";
import { ChangeDecisionBody } from "../review/Dialogs";
import { ShortcutCard } from "../shortcuts/ShortcutList";
import "../shortcuts/shortcuts.css";
import { SettingsContext, type SettingsContextValue } from "../settings/context";
import { WarmOffDialog } from "../settings/Dialogs";
import { EnvVarsRow } from "../settings/EnvVars";
import { AgentToolsPageBody, DisconnectDialogView } from "../settings/AgentTools";
import { type ShortcutsCalls, ShortcutsView } from "../settings/Keyboard";
import { Section as SettingsSectionRows, WarmBuildRow } from "../settings/Rows";
import "../settings/settings.css";
import { ActivityGallery } from "../shell/ActivityGallery";
import { RailAgentButton } from "../shell/Rail";
import { LimitLinesGallery } from "../notices/LimitLinesGallery";
import { LeadTabBody } from "../lead_tab/LeadTab";
import { AddButton, AddProjectFields, staticAddProject } from "../onboarding/AddProjectForm";
import { FlowEditorBody, recordingActions } from "../flow_editor/FlowEditor";
import { ToolsList } from "../onboarding/Tools";
import "../onboarding/onboarding.css";
import "./gallery.css";

/** The frame's regions with their names, for the shell's frames below. */
const SHELL_PREVIEW = (() => {
  const app = createRegistries();
  const named = (name: string) => () => <div className="frame-placeholder">{name}</div>;
  app.slots.register("agent_panel", { Component: named("Project agent panel") });
  app.slots.register("ask_bar", { Component: named("Ask bar") });
  return app;
})();

/**
 * The shell at the design's three windows with the Project agent closed (the Ask bar over the board), and open at
 * 1440 (T-0327, T-0458; SHB-01). Drawn at half size.
 */
function ShellFrames() {
  const view = useStore(layoutView);
  useEffect(loadLayout, []);
  if (!view) return null;
  const frames: [number, number, boolean][] = [
    [1280, 800, false],
    [1440, 900, false],
    [1920, 1080, false],
    [1440, 900, true],
  ];
  return (
    <div className="gallery-wrap gallery-wrap-wide">
      {frames.map(([width, height, open]) => (
        <figure key={`${String(width)}${String(open)}`} className="gallery-shell">
          <figcaption>
            {width} × {height}
            {open ? " · Project agent open (⌘L)" : ""}
          </figcaption>
          <div className="gallery-shell-box" style={{ width: width / 2, height: height / 2 }}>
            <div className="frame gallery-shell-frame" style={{ width, height }}>
              <RegistriesContext value={SHELL_PREVIEW}>
                <FrameBody view={view} layout={{ ...INITIAL_LAYOUT, agentOpen: open }} ticketOpen={false} />
              </RegistriesContext>
            </div>
          </div>
        </figure>
      ))}
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="gallery-section" aria-label={title}>
      <h3>{title}</h3>
      {children}
    </section>
  );
}

function Caption({ children }: { children: ReactNode }) {
  return <span className="gallery-caption">{children}</span>;
}

/** The palette switch: recolors the whole page for a look (not saved); closing the gallery puts the theme back. */
function PaletteSwitch({ shown, onShow }: { shown: string; onShow: (key: ThemeKey) => void }) {
  return (
    <div className="gallery-palettes" role="radiogroup" aria-label="Palette">
      {THEME_KEYS.map((key) => (
        <button
          key={key}
          type="button"
          role="radio"
          aria-checked={key === shown}
          className="gallery-palette"
          onClick={() => {
            onShow(key);
          }}
        >
          {key}
        </button>
      ))}
    </div>
  );
}

function Legend({ tiers, view }: { tiers: readonly LegendTier[]; view: GalleryView }) {
  return (
    <div className="gallery-stack">
      {tiers.map((tier) => (
        <div key={tier.title} className="gallery-stack-tight">
          <h4>{tier.title}</h4>
          <div className="gallery-wrap">
            {tier.pills.map((pill) => (
              <StatusPill key={pill.label} pill={pill} />
            ))}
          </div>
          <div className="gallery-wrap">
            <span className="gallery-tag">M12</span>
            {tier.m12.map((pill) => (
              <StatusPill key={pill.label} pill={pill} />
            ))}
          </div>
        </div>
      ))}
      <div className="gallery-stack-tight">
        <div className="gallery-wrap">
          <h4>{view.priority.title}</h4>
          <span className="gallery-note">{view.priority.note}</span>
        </div>
        <div className="gallery-wrap gallery-wrap-wide">
          {view.priority.chips.map((chip) => (
            <PriorityLabel key={chip.short} priority={chip} />
          ))}
          <span className="gallery-secondary">{view.priority.order}</span>
        </div>
      </div>
    </div>
  );
}

function TypeScale({ view }: { view: GalleryView }) {
  return (
    <div className="gallery-stack">
      {view.type_scale.map((row) => (
        <div key={row.token} className="gallery-type-row">
          <span className="gallery-type-name">
            <span>{row.name}</span>
            <Caption>{row.spec}</Caption>
          </span>
          <span
            style={{ font: `var(--text-${row.token})`, letterSpacing: `var(--text-${row.token}-tracking)` }}
          >
            {row.sample}
          </span>
        </div>
      ))}
    </div>
  );
}

function Spacing({ view }: { view: GalleryView }) {
  return (
    <div className="gallery-stack">
      <div className="gallery-measures">
        {view.spacing.map((step) => (
          <span key={step.name} className="gallery-measure">
            <span className="gallery-space" style={{ width: step.px, height: step.px }} />
            <Caption>{step.name}</Caption>
            <Caption>{step.px} px</Caption>
          </span>
        ))}
      </div>
      <div className="gallery-measures">
        {view.radii.map((r) => (
          <span key={r.name} className="gallery-measure">
            <span className="gallery-radius" style={{ borderRadius: r.px }} />
            <Caption>
              {r.name} {r.px}
            </Caption>
          </span>
        ))}
      </div>
      <Caption>{view.density}</Caption>
    </div>
  );
}

function Icons({ view }: { view: GalleryView }) {
  return (
    <div className="gallery-icons">
      {view.icons.map((entry) => (
        <span key={entry.name} className="gallery-icon">
          <Icon name={entry.icon} />
          <Caption>{entry.name}</Caption>
        </span>
      ))}
    </div>
  );
}

const CELLS: readonly { name: string; shown?: ShownState; disabled?: boolean; loading?: boolean }[] = [
  { name: "DEFAULT" },
  { name: "HOVER", shown: "hover" },
  { name: "PRESSED", shown: "pressed" },
  { name: "FOCUSED", shown: "focused" },
  { name: "DISABLED", disabled: true },
  { name: "LOADING", loading: true },
];

const VARIANTS: readonly {
  variant: ButtonVariant;
  name: string;
  label: string;
  busy: string;
  icon: IconName;
}[] = [
  { variant: "primary", name: "Primary", label: "Approve", busy: "Merging…", icon: "check" },
  { variant: "secondary", name: "Secondary", label: "Send back", busy: "Working…", icon: "undo-2" },
  { variant: "destructive", name: "Destructive", label: "Mark closed", busy: "Working…", icon: "x" },
  { variant: "ghost", name: "Ghost", label: "View parent", busy: "Working…", icon: "arrow-up-right" },
];

function ButtonMatrix() {
  const [loading, setLoading] = useState(false);
  const [runs, setRuns] = useState(0);
  return (
    <div className="gallery-stack">
      <div className="gallery-matrix">
        <span />
        {CELLS.map((cell) => (
          <Caption key={cell.name}>{cell.name}</Caption>
        ))}
        {VARIANTS.map((v) => (
          <MatrixRow key={v.variant} name={v.name}>
            {CELLS.map((cell) => (
              <Button
                key={cell.name}
                variant={v.variant}
                icon={v.icon}
                label={cell.loading ? v.busy : v.label}
                shown={cell.shown}
                disabled={cell.disabled}
                loading={cell.loading}
              />
            ))}
          </MatrixRow>
        ))}
        <MatrixRow name="Icon button">
          {CELLS.map((cell) => (
            <IconButton
              key={cell.name}
              icon="square-terminal"
              label="Open terminal"
              shown={cell.shown}
              disabled={cell.disabled}
              loading={cell.loading}
            />
          ))}
        </MatrixRow>
      </div>
      <div className="gallery-wrap">
        <Button
          label={loading ? "Merging…" : "Approve"}
          icon="check"
          loading={loading}
          onClick={() => {
            setLoading(true);
            setRuns((n) => n + 1);
            window.setTimeout(() => {
              setLoading(false);
            }, 2000);
          }}
        />
        <Caption>Live: a press loads for 2 s; presses meanwhile are ignored · runs: {runs}</Caption>
      </div>
    </div>
  );
}

function MatrixRow({ name, children }: { name: string; children: ReactNode }) {
  return (
    <>
      <span className="gallery-secondary">{name}</span>
      {children}
    </>
  );
}

function Primitives({ view }: { view: GalleryView }) {
  const sample = (label: string): PillView | undefined =>
    view.legend.flatMap((t) => [...t.pills, ...t.m12]).find((p) => p.label === label);
  const working = sample("Working");
  return (
    <div className="gallery-stack">
      <div className="gallery-wrap gallery-wrap-wide">
        {working && <StatusPill pill={working} />}
        <MetaChip icon="link-2" label="SHOP-18 SHOP-19" />
        <Kbd keys="⌘K" />
        <Button label="Approve" icon="check" />
        <IconButton icon="square-terminal" label="Open terminal" />
        {view.priority.chips[1] && <PriorityChip priority={view.priority.chips[1]} />}
      </div>
      <Caption>COUNT</Caption>
      <div className="gallery-wrap">
        <CountBadge count={3} tone="attention" />
        <CountBadge count={12} tone="neutral" />
        <CountBadge count={2} tone="accent" />
      </div>
      <Caption>DEPENDENCY · META</Caption>
      <div className="gallery-wrap">
        <MetaChip icon="link-2" label="SHOP-18 SHOP-19" />
        <MetaChip icon="git-branch" label="otaq/SHOP-18" />
        <MetaChip icon="coins" label="182k" />
        <MetaChip icon="play" label=":4020" tone="running" />
        <MetaChip icon="git-pull-request" label="PR #481" />
      </div>
      <Caption>LAST SIGNAL</Caption>
      <div className="gallery-wrap">
        <MetaChip icon="activity" label="40s ago" />
        <MetaChip icon="activity" label="4 min ago" tone="warning" />
      </div>
      <Caption>KBD</Caption>
      <div className="gallery-wrap">
        {["⌘K", "⌘↵", "⇧⌘I", "Ctrl+Shift+J", "esc"].map((k) => (
          <Kbd key={k} keys={k} />
        ))}
      </div>
    </div>
  );
}

function TabsAndToggles() {
  const [tab, setTab] = useState(0);
  const [narrow, setNarrow] = useState(1);
  const [segment, setSegment] = useState(0);
  const [on, setOn] = useState(true);
  const [off, setOff] = useState(false);
  const tabs = [
    { icon: "square-terminal", label: "Terminal", indicator: { kind: "live" }, keys: "1" },
    { icon: "file-diff", label: "Diff", indicator: { kind: "note", text: "+412 −38" }, keys: "2" },
    { icon: "play", label: "Run", indicator: { kind: "note", text: ":4020" }, keys: "3" },
    { icon: "terminal", label: "Shell", keys: "4" },
    { icon: "file-text", label: "Ticket", keys: "5" },
    { icon: "history", label: "History", indicator: { kind: "live_attention" }, keys: "6" },
  ] as const;
  return (
    <div className="gallery-stack">
      <Caption>TABS</Caption>
      <Tabs label="Panel tabs" items={tabs} active={tab} onSelect={setTab} />
      <Caption>TABS · ICONS ONLY (narrow panel)</Caption>
      <Tabs label="Panel tabs, narrow" items={tabs} active={narrow} onSelect={setNarrow} iconsOnly />
      <Caption>SEGMENTED</Caption>
      <Segmented
        label="Diff layout"
        options={["Unified", "Side-by-side"]}
        selected={segment}
        onSelect={setSegment}
      />
      <Caption>TOGGLES</Caption>
      <div className="gallery-wrap gallery-wrap-wide">
        <Toggle on={on} label={on ? "On" : "Off"} onChange={setOn} />
        <Toggle on={off} label="Off · default" onChange={setOff} />
        <Toggle on={false} label="Disabled" disabled onChange={setOff} />
      </div>
    </div>
  );
}

function Fields({ view }: { view: GalleryView }) {
  const [empty, setEmpty] = useState("");
  const [title, setTitle] = useState("Checkout API");
  const [env, setEnv] = useState(".env.local");
  const [body, setBody] = useState(
    "POST /api/checkout creates a Stripe session. Uses the client from SHOP-17.",
  );
  const [priority, setPriority] = useState("priority-2");
  const [checked, setChecked] = useState(true);
  const [radio, setRadio] = useState("squash");
  const chosen = view.priority_menu.find((i) => i.id === priority);
  const items = view.priority_menu.map((i) => ({ ...i, checked: i.id === priority }));
  return (
    <div className="gallery-fields">
      <Field label="Title">
        {(id) => (
          <TextInput id={id} value={empty} placeholder="Short, imperative title" onChange={setEmpty} />
        )}
      </Field>
      <Field label="Title" hint="Focused · 1.5 px focus ring">
        {(id) => <TextInput id={id} value={title} onChange={setTitle} />}
      </Field>
      <Field
        label="Env file"
        error="Can’t find .env.local in ~/code/billing-api. Add it to the repo root, or remove it from this list."
      >
        {(id) => <TextInput id={id} value={env} mono invalid onChange={setEnv} />}
      </Field>
      <Field label="Priority">
        {(id) => (
          <Select
            id={id}
            value={chosen?.label ?? priority}
            icon={chosen?.icon}
            items={items}
            onChange={setPriority}
          />
        )}
      </Field>
      <Field label="PR provider">
        {(id) => (
          <Select
            id={id}
            value="GitHub"
            icon="git-pull-request"
            items={[]}
            disabled
            onChange={() => undefined}
          />
        )}
      </Field>
      <Field label="What and why">{(id) => <TextArea id={id} value={body} onChange={setBody} />}</Field>
      <Field hint="Disabled">
        {() => <TextInput label="Locked" value="Locked after start" disabled onChange={() => undefined} />}
      </Field>
      <Checkbox checked={checked} label="Delete the branch after merging" onChange={setChecked} />
      <div className="gallery-wrap gallery-wrap-wide">
        <Radio
          name="gallery-merge"
          checked={radio === "squash"}
          label="Squash"
          onChange={() => {
            setRadio("squash");
          }}
        />
        <Radio
          name="gallery-merge"
          checked={radio === "merge"}
          label="Merge commit"
          onChange={() => {
            setRadio("merge");
          }}
        />
        <Radio name="gallery-merge" checked={false} label="Rebase" disabled onChange={() => undefined} />
      </div>
    </div>
  );
}

function Overlays({ view }: { view: GalleryView }) {
  const menuButton = useRef<HTMLButtonElement>(null);
  const popoverButton = useRef<HTMLButtonElement>(null);
  const [menu, setMenu] = useState<"button" | { x: number; y: number } | null>(null);
  const [popover, setPopover] = useState(false);
  const [chosen, setChosen] = useState("nothing yet");
  const closePopover = useCallback(() => {
    setPopover(false);
  }, []);
  return (
    <div className="gallery-overlays">
      <div className="gallery-stack">
        <Caption>CONTEXT MENU · ↑/↓, ↵, Esc</Caption>
        <div className="gallery-wrap">
          <Button
            ref={menuButton}
            variant="secondary"
            label="Open menu"
            icon="ellipsis"
            onClick={() => {
              setMenu("button");
            }}
          />
          <button
            type="button"
            className="gallery-target"
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ x: e.clientX, y: e.clientY });
            }}
          >
            Right-click here
          </button>
        </div>
        <Caption>Chosen: {chosen}</Caption>
        {menu && (
          <AnchoredMenu
            label="Card actions"
            anchor={menu === "button" ? menuButton : { left: menu.x, top: menu.y, width: 0, height: 0 }}
            items={view.context_menu}
            onChoose={setChosen}
            onClose={(how) => {
              setMenu(null);
              if (how === "key" && menu === "button") menuButton.current?.focus();
            }}
          />
        )}
        <Caption>TOOLTIP · POPOVER</Caption>
        <div className="gallery-wrap">
          <Tooltip text="Open terminal" keys="1">
            <IconButton icon="square-terminal" label="Open terminal" />
          </Tooltip>
          <Tooltip
            text="2 running"
            detail={["SHOP-18 · npm run dev :4020", "SHOP-22 · npm test"]}
            note="Click to manage"
          >
            <Button variant="ghost" label="Hover for details" icon="play" />
          </Tooltip>
          <Button
            ref={popoverButton}
            variant="secondary"
            label="Popover"
            icon="gauge"
            onClick={() => {
              setPopover((p) => !p);
            }}
          />
          {popover && (
            <Popover label="Agent limits" anchor={popoverButton} onClose={closePopover}>
              <div className="gallery-popover">
                <strong>Agent limits</strong>
                <span className="gallery-secondary">Esc or a click outside closes it.</span>
              </div>
            </Popover>
          )}
        </div>
      </div>
      <div className="gallery-stack">
        <Caption>CONFIRM DIALOG · Esc cancels; ↵ = Cancel on destructive dialogs</Caption>
        <div className="gallery-wrap">
          <Button
            variant="secondary"
            label="Primary dialog"
            onClick={() => {
              openOverlay("confirm", {
                title: "Send SHOP-18 Checkout API back?",
                body: "SHOP-18 goes straight back to its agent in Dev (Working) and continues in the same session.",
                confirmLabel: "Send back",
                destructive: false,
                onConfirm: () => {
                  setChosen("dialog: Send back");
                },
              });
            }}
          />
          <Button
            variant="destructive"
            label="Destructive dialog"
            onClick={() => {
              openOverlay("confirm", {
                title: "Delete SHOP-21 Order confirmation email?",
                body: "It never started, so nothing else is lost. Its number is never reused.",
                confirmLabel: "Delete",
                destructive: true,
                onConfirm: () => {
                  setChosen("dialog: Delete");
                },
              });
            }}
          />
        </div>
        <Caption>TOASTS</Caption>
        <div className="gallery-toasts">
          <Toast
            kind="success"
            icon="git-pull-request"
            iconTone="review"
            title="PR #481 opened on GitHub"
            text="SHOP-26 Add sitemap → main · Open in browser"
            onClose={() => undefined}
          />
          <Toast
            kind="success"
            icon="git-merge"
            title="Local merge done · SHOP-18"
            text="Squashed into main as 9f3e2c1"
            onClose={() => undefined}
          />
          <Toast
            kind="success"
            icon="sparkles"
            iconTone="investigate"
            title="Ticket created by agent"
            text="SHOP-19 created SHOP-29 → Backlog"
            onClose={() => undefined}
          />
          <Toast
            kind="error"
            title="gh is not signed in"
            text="Run gh auth login, then retry Open PR."
            action={{ label: "Copy command", onClick: () => undefined }}
            onClose={() => undefined}
          />
        </div>
      </div>
    </div>
  );
}

/** The epic queue toast (T-0272, EPC-15): flows 2 (several children), 9 (one child) and 12 (Auto-merge off). */
function EpicQueueToasts() {
  const [pressed, setPressed] = useState<string>();
  const press = (label: string) => ({
    label,
    onClick: () => {
      setPressed(label);
    },
  });
  return (
    <div className="gallery-toasts">
      <Toast
        kind="success"
        title="Moved 2 children of SHOP-45 to Ready for Dev"
        actions={[press("Undo")]}
        onClose={() => undefined}
      />
      <Toast
        kind="success"
        title="Moved 2 children of SHOP-45 to Ready for Dev · its lead started"
        actions={[press("Undo")]}
        onClose={() => undefined}
      />
      <Toast
        kind="success"
        title="Moved SHOP-45.1 of Search to Ready for Dev"
        actions={[press("Undo")]}
        onClose={() => undefined}
      />
      <Toast
        kind="success"
        title="Moved 3 tickets to Ready for Dev. Auto-merge is off for shop-web, so each one waits for your review."
        actions={[press("Turn on for epic children"), press("Undo")]}
        onClose={() => undefined}
      />
      <Caption>Last press: {pressed ?? "—"}</Caption>
    </div>
  );
}

function Banners() {
  const noop = () => undefined;
  return (
    <div className="gallery-stack">
      <Banner
        tone="attention"
        icon="gauge"
        title="Usage limit reached"
        text="Resets at 3:00 pm (in 1 h 12 min) · 8 agents stopped · no new tickets will start"
        actions={[
          { label: "Resume", icon: "play", disabled: true, onClick: noop },
          { label: "Retry", icon: "rotate-ccw", onClick: noop },
        ]}
        onDismiss={noop}
      />
      <Banner
        tone="warning"
        icon="shield-alert"
        title="Permission prompts are on"
        text="With 5–10 agents, permission prompts will stop agents often. Read about allowlists and auto mode →"
        onDismiss={noop}
      />
      <Banner
        tone="info"
        icon="refresh-cw"
        title="Service update ready"
        text="Restart now (3 agents working, 2 waiting) · or later, when it suits you"
        actions={[{ label: "Restart now", icon: "rotate-cw", onClick: noop }]}
        onDismiss={noop}
      />
      <Banner
        tone="warning"
        icon="server-cog"
        title="Restart required"
        text="This app can’t talk to the running service (older version) · 3 agents working, 2 waiting"
        actions={[{ label: "Restart now", icon: "rotate-cw", onClick: noop }]}
      />
      <Banner tone="error" icon="zap-off" title="Background service stopped" text="Agents are not running" />
      <Banner
        tone="neutral"
        icon="loader-circle"
        title="Reconnecting…"
        text="Trying to reach the background service"
      />
    </div>
  );
}

/** A map cell's keys, joined by "or", a sequence's two by "then"; none is "—". */
function MapKeys({ keys, map }: { keys: readonly ShortcutKey[]; map: KeyMapView }) {
  if (keys.length === 0) return <span className="gallery-secondary">—</span>;
  return (
    <>
      {keys.map((key, i) => (
        <Fragment key={key.kbds.join(" ")}>
          {i > 0 && <span className="gallery-keyword">{map.or}</span>}
          {key.kbds.map((kbd, j) => (
            <Fragment key={kbd}>
              {j > 0 && <span className="gallery-keyword">{map.then}</span>}
              <Kbd keys={kbd} />
            </Fragment>
          ))}
        </Fragment>
      ))}
    </>
  );
}

/** *01 · Components → Keyboard map (update AF · 0097, update AO)*: the single letters, the modifier keys in three
 * columns, and the notes. */
function KeyboardMap({ map }: { map: KeyMapView }) {
  return (
    <div className="gallery-stack">
      <div className="gallery-keymap">
        <div className="gallery-keygroup gallery-keyletters">
          <div className="gallery-keygroup-head">
            <h4>{map.letters_title}</h4>
            <span className="gallery-note">{map.letters_note}</span>
          </div>
          {map.letters.map((row) => (
            <div key={row.label} className="gallery-keyrow">
              <span className="gallery-keylabel">{row.label}</span>
              <span className="gallery-keys">
                <MapKeys keys={[row.key]} map={map} />
              </span>
            </div>
          ))}
        </div>
        <div className="gallery-keygroup gallery-keymods">
          <div className="gallery-keyrow gallery-keymods-head">
            <h4 className="gallery-keylabel">{map.modifiers_title}</h4>
            {map.columns.map((column) => (
              <span key={column} className="gallery-note">
                {column}
              </span>
            ))}
          </div>
          {map.modifiers.map((row) => (
            <div key={row.label} className="gallery-keyrow">
              <span className="gallery-keylabel">{row.label}</span>
              {[row.mac, row.win_linux, row.in_terminal].map((keys, i) => (
                <span key={map.columns[i]} className="gallery-keys">
                  <MapKeys keys={keys} map={map} />
                </span>
              ))}
            </div>
          ))}
        </div>
      </div>
      {map.notes.map((note, i) => (
        <p key={note} className={i === 0 ? "gallery-keynote-first" : "gallery-note"}>
          {note}
        </p>
      ))}
    </div>
  );
}

function ThemeSection() {
  const picker = useThemePicker();
  if (picker.error) return <p role="alert">{picker.error.message}</p>;
  if (!picker.view) return null;
  return (
    <div className="gallery-picker">
      <ThemePicker
        sections={picker.view}
        onPick={(key) => {
          void pickTheme(key);
        }}
      />
    </div>
  );
}

/** *04 · Ticket card variants → Ticket Card v2 · all variants*: one cell per variant, named as in the frame, at the
 * card width Rust gives. */
function CardGrid({
  cells,
  width,
  onPick,
}: {
  cells: CardGallery["variants"];
  width?: number;
  onPick: (what: string) => void;
}) {
  return (
    <div className="gallery-cards">
      {cells.map((cell) => (
        <div
          key={cell.name}
          className="gallery-card-cell"
          style={width === undefined ? undefined : { width, boxSizing: "content-box" }}
        >
          <Caption>{cell.name.toUpperCase()}</Caption>
          <TicketCard
            card={cell.card}
            state={stateOfLook(cell.look)}
            onSelect={() => {
              onPick(`${cell.card.key}: selected`);
            }}
            onAction={(kind) => {
              onPick(`${cell.card.key}: ${JSON.stringify(kind)}`);
            }}
            onOpenPanel={() => {
              onPick(`${cell.card.key}: open panel`);
            }}
          />
        </div>
      ))}
    </div>
  );
}

function EpicGrid({ cards, onPick }: { cards: CardGallery; onPick: (what: string) => void }) {
  return (
    <div className="gallery-cards">
      {cards.epics.map((cell) => (
        <div key={cell.name} className="gallery-card-cell">
          <Caption>{cell.name.toUpperCase()}</Caption>
          <EpicCard
            card={cell.card}
            onSelect={() => {
              onPick(`${cell.card.key}: selected`);
            }}
            onChild={(key) => {
              onPick(`${key}: selected`);
            }}
            onAdd={() => {
              onPick(`${cell.card.key}: add ticket`);
            }}
            onAction={(kind) => {
              onPick(`${cell.card.key}: ${JSON.stringify(kind)}`);
            }}
          />
        </div>
      ))}
    </div>
  );
}

function Cards() {
  const { view, error } = useServiceView(loadCards, []);
  const [picked, setPicked] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <>
      <Section title="24 · One-line cards · all variants (BRD-03, T-0460)">
        <Caption>Last click: {picked ?? "—"}</Caption>
        <CardGrid cells={view.variants} width={view.width} onPick={setPicked} />
      </Section>
      <Section title="24 · One-line cards · all variants, narrowest column (BRD-03, T-0460)">
        <CardGrid cells={view.variants} width={view.width_1280} onPick={setPicked} />
      </Section>
      <Section title="04 · Ticket card variants · Row · epics (fixtures)">
        <EpicGrid cards={view} onPick={setPicked} />
      </Section>
      <Section title="24 · One-line cards · epic children name their epic (BRD-03, T-0460)">
        <CardGrid cells={view.sleeves} onPick={setPicked} />
      </Section>
      <Section title="04 · Ticket card variants · Row · Epic children & the lead · Unread notes & same file (T-0291, d0083)">
        <CardGrid cells={view.leads} onPick={setPicked} />
      </Section>
      <Section title="04 · Variants · one tool (M22) · set by you, picked by, Start failed, No result (T-0345)">
        <CardGrid cells={view.agents} width={view.width} onPick={setPicked} />
      </Section>
      <Section title="04 · Variants · one tool (M22) at 1280 (T-0345)">
        <CardGrid cells={view.agents} width={view.width_1280} onPick={setPicked} />
      </Section>
      <Section title="04 · Variants · several tools (M23) · avatars, Unavailable, waiting on a tool, moved by a fallback (T-0367)">
        <CardGrid cells={view.tools} width={view.width} onPick={setPicked} />
      </Section>
      <Section title="04 · Variants · several tools (M23) at 1280 (T-0367)">
        <CardGrid cells={view.tools} width={view.width_1280} onPick={setPicked} />
      </Section>
      <Section title="03 · Cards in a check and a delivery · any flow (M33, T-0424)">
        <CardGrid cells={view.flows} width={view.width} onPick={setPicked} />
      </Section>
    </>
  );
}

/** SHOP-47's description, the update P example (tickets/m15-tickets.md, "The example"). */
const SHOP_47 = [
  "## Why",
  "Users mistype addresses at checkout; **12%** of failed deliveries trace back to it (see [the support report](https://example.com/report)).",
  "",
  "## What",
  "- Suggest addresses as the user types in *Street*",
  "- Keyboard: ↑ ↓ to move, Enter to pick, Esc to close",
  "",
  "## Done when",
  "- [x] Suggestions appear after 3 characters",
  "- [ ] Arrow keys move the highlight",
  "- [ ] Works with the existing `AddressForm` validation",
  "",
  "| Field | Source |",
  "|---|---|",
  "| Street | Places API |",
  "| Postcode | Places API, editable |",
  "",
  "```ts",
  'const results = await places.autocomplete(query, { country: "TR" });',
  "```",
].join("\n");

/** The description editor (T-0249) with the example, in edit and read mode; read mode follows the edits. */
function MarkdownSample() {
  const [body, setBody] = useState(SHOP_47);
  const [empty, setEmpty] = useState("");
  return (
    <div className="gallery-two">
      <div className="gallery-stack-tight">
        <Caption>Edit mode · b · Filled · edit mode (the toolbar comes with T-0250)</Caption>
        <MarkdownEditor value={body} onChange={setBody} label="What and why" />
        <Caption>Edit mode · empty, with the placeholder</Caption>
        <MarkdownEditor
          value={empty}
          onChange={setEmpty}
          label="What and why"
          placeholder="Context, constraints, links…"
        />
      </div>
      <div className="gallery-stack-tight">
        <Caption>Read mode · editable=false (task boxes don&apos;t react)</Caption>
        <MarkdownEditor value={body} editable={false} label="What and why" />
      </div>
    </div>
  );
}

/** Code blocks (T-0254): the language label and picker, the Files tab's colours, a long line, and read mode's Copy. */
const CODE_SAMPLE = [
  "## Sketch",
  "",
  "```ts",
  'const results = await places.autocomplete(query, { country: "TR" }); // a long line scrolls sideways, never wraps',
  "if (!results.length) {",
  "  showNoMatches();",
  "}",
  "```",
  "",
  "````md",
  "```rust",
  "fn main() {}",
  "```",
  "````",
].join("\n");

function CodeSample() {
  const [body, setBody] = useState(CODE_SAMPLE);
  return (
    <div className="gallery-two">
      <div className="gallery-stack-tight">
        <Caption>
          Edit mode · g · the label opens the picker; ``` on an empty last line or ↓ at the end leaves
        </Caption>
        <MarkdownEditor value={body} onChange={setBody} label="What and why" />
      </div>
      <div className="gallery-stack-tight">
        <Caption>Read mode · c · Copy on hover</Caption>
        <MarkdownEditor value={body} editable={false} label="What and why" />
      </div>
    </div>
  );
}

function ToolbarSample() {
  const [body, setBody] = useState(SHOP_47);
  const [narrow, setNarrow] = useState(SHOP_47);
  return (
    <div className="gallery-stack-tight">
      <Caption>Full width · b · Filled · edit mode (the caret in a task item lights Task list)</Caption>
      <MarkdownEditor value={body} onChange={setBody} label="What and why" toolbar={<MarkdownToolbar />} />
      <Caption>
        At 480 px · h · Narrow Ticket tab · toolbar overflow (Strikethrough, Quote, Table, Horizontal rule in
        “…”)
      </Caption>
      <div style={{ width: 480 }}>
        <MarkdownEditor
          value={narrow}
          onChange={setNarrow}
          label="What and why"
          toolbar={<MarkdownToolbar />}
        />
      </div>
    </div>
  );
}

const TABLE_SAMPLE = `## Sources
Where each address field comes from:

| Field | Source |
|---|---|
| Street | Places API |
| Postcode | Places API, editable |
`;

function TableSample() {
  const [body, setBody] = useState(TABLE_SAMPLE);
  return (
    <div className="gallery-stack-tight">
      <Caption>f · Caret in a table: click a cell; the bar floats above the table (edit mode only)</Caption>
      <MarkdownEditor value={body} onChange={setBody} label="What and why" toolbar={<MarkdownToolbar />} />
    </div>
  );
}

/** Links (T-0252): the ⌘K popover and the hover bubble in edit mode; the tooltip and a click to open in read mode. */
const LINK_SAMPLE = [
  "## Why",
  "Users mistype addresses at checkout; **12%** of failed deliveries trace back to it (see [the support report](https://example.com/report)).",
  "",
  "Write to [the team](mailto:checkout@example.com) about it.",
].join("\n");

function LinkSample() {
  const [body, setBody] = useState(LINK_SAMPLE);
  return (
    <div className="gallery-two">
      <div className="gallery-stack-tight">
        <Caption>
          Edit mode · e · select text and press ⌘K, or hover a link (Open opens the system browser)
        </Caption>
        <MarkdownEditor value={body} onChange={setBody} label="What and why" toolbar={<MarkdownToolbar />} />
      </div>
      <div className="gallery-stack-tight">
        <Caption>Read mode · c · hover shows the URL; a click opens it</Caption>
        <MarkdownEditor value={body} editable={false} label="What and why" />
      </div>
    </div>
  );
}

/** The Markdown source toggle (T-0251): opens with the source on; switching here isn't saved. */
function SourceSample() {
  const [body, setBody] = useState(SHOP_47);
  const mode = useSourceMode(body, true);
  const editor = useMarkdownEditor({ value: mode.editorValue, onChange: setBody, label: "What and why" });
  useFocusOnLeave(editor, mode);
  return (
    <div className="gallery-stack-tight">
      <Caption>
        d · Markdown source on: the raw source in Geist Mono; the formatting buttons are disabled
      </Caption>
      <MarkdownSurface
        editor={editor}
        toolbar={
          <MarkdownToolbar
            disabled={mode.on}
            sourceToggle={<SourceToggle on={mode.on} label="Markdown" onChange={mode.toggle} />}
          />
        }
        {...(mode.on
          ? {
              source: (
                <SourceArea value={body} label="What and why" takeFocus={mode.switched} onChange={setBody} />
              ),
            }
          : {})}
      />
    </div>
  );
}

const loadLeads = () => request("gallery.leads") as Promise<LeadGallery>;

/** *01 → Row · Lead Status (0083)*, the lane heads of Lanes: Epic (`s24-06`, T-0462) at 1440 and 1280, and *Row ·
 * Epic Card with a lead (0083)* (T-0289): every form from `lead_status.rs` and `lanes.rs`; clicks are only recorded. */
function Leads() {
  const { view, error } = useServiceView(loadLeads, []);
  const [picked, setPicked] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const onOpen = (epic: string) => {
    setPicked(`${epic}: open the Lead tab`);
  };
  const onAction = (kind: string, epic: string) => {
    setPicked(`${epic}: ${kind}`);
  };
  // Split with the Project agent (T-0377): what it would send the Project agent pane.
  const onSplit = (split: LeadSplit) => {
    setPicked(`${split.project}: ${split.message}`);
  };
  return (
    <>
      <Section title="01 · Epics on the board · Row · Lead Status (0083) · full and short (T-0289)">
        <Caption>Last click: {picked ?? "—"}</Caption>
        {(["full", "short"] as const).map((form) => (
          <div key={form} className="gallery-wrap">
            {view.statuses.map((cell) => (
              <div key={cell.name} className="gallery-stack">
                <Caption>
                  {form.toUpperCase()} · {cell.name.toUpperCase()}
                </Caption>
                <LeadStatus
                  lead={cell.view}
                  form={form}
                  onOpen={onOpen}
                  onAction={onAction}
                  onSplit={onSplit}
                />
              </div>
            ))}
          </div>
        ))}
      </Section>
      <Section title="24 · Epic lanes · lane heads (EPL-01, T-0462) · 1376 and 1216 px">
        {["1376px", "1216px"].map((width) => (
          <div key={width} className="gallery-stack board-lanes" style={{ width }}>
            {view.lanes.map((cell) => (
              <div key={cell.name} className="gallery-stack">
                <Caption>{cell.name.toUpperCase()}</Caption>
                <section className="board-lane" data-folded={cell.folded || undefined}>
                  <LaneHeadRow
                    head={cell.head}
                    epic={cell.head.progress === null ? null : "SHOP-40"}
                    folded={cell.folded}
                    foldLabel={cell.folded ? "Unfold this lane" : "Fold this lane"}
                    onFold={() => {
                      setPicked(`${cell.name}: fold`);
                    }}
                  />
                </section>
              </div>
            ))}
          </div>
        ))}
      </Section>
      <Section title="01 · Epics on the board · Row · Epic Card with a lead (0083)">
        <div className="gallery-cards">
          {view.epics.map((cell) => (
            <div key={cell.name} className="gallery-card-cell">
              <Caption>{cell.name.toUpperCase()}</Caption>
              <EpicCard card={cell.card} onLead={onOpen} onLeadAction={onAction} onSplit={onSplit} />
            </div>
          ))}
        </div>
      </Section>
    </>
  );
}

const loadLeadTabs = () => request("gallery.lead_tabs") as Promise<LeadTabGallery>;

/** *05 → Row · Epic panel · Lead tab (0083)*, *Row · Lead tab · ended (0083)* and *16 → Before the restart*
 * (T-0290): the Lead tab in each state from `lead_tab.rs`, at the panel's default width; buttons are only recorded. */
function LeadTabs() {
  const { view, error } = useServiceView(loadLeadTabs, []);
  const [picked, setPicked] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <Section title="05 · Epic sheet · Lead tab (0083) · every state (T-0290)">
      <Caption>Last click: {picked ?? "—"}</Caption>
      <div className="gallery-wrap">
        {view.tabs.map((cell) => (
          <div key={cell.name} className="gallery-stack gallery-lead-tab">
            <Caption>{cell.name.toUpperCase()}</Caption>
            <LeadTabBody
              view={cell.view}
              onAction={(kind, epic) => {
                setPicked(`${epic}: ${kind}`);
                return Promise.resolve();
              }}
              onSplit={(split) => {
                setPicked(`${split.project}: ${split.message}`);
                return Promise.resolve();
              }}
              onToolProblem={(action) => {
                setPicked(`${cell.view.epic}: ${JSON.stringify(action)}`);
              }}
            />
          </div>
        ))}
      </div>
    </Section>
  );
}

const loadStageChoices = () => request("gallery.stage_choices") as Promise<StageChoicesGallery>;

/** *08 → Row · Stage choices (update AG §9)*, *05 → Row · Stage choices and path* (T-0411): the ⌘N form's pickers,
 * the panel's choices and path mid-flow, after a Fail, finished, a choice you set and the default flow, from
 * `gallery.stage_choices`; picks are only recorded. */
function StageChoicesSection() {
  const { view, error } = useServiceView(loadStageChoices, []);
  const [picked, setPicked] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <Section title="05/08 · Stage choices and the path (T-0411)">
      <Caption>Last pick: {picked ?? "\u2014"}</Caption>
      <div className="gallery-wrap gallery-wrap-wide">
        {view.samples.map((sample, n) => (
          <div key={sample.caption} className="gallery-stack-tight gallery-agent-rows">
            <Caption>{sample.caption}</Caption>
            {sample.token_line !== null && (
              <div className="panel-split">
                <Icon name="coins" size={11} />
                <span>{sample.token_line}</span>
              </div>
            )}
            {sample.callouts.map((callout) => (
              <ToolProblemCallout
                key={callout.heading}
                callout={callout}
                look="edge"
                onAction={(action) => {
                  setPicked(`${sample.caption}: ${JSON.stringify(action)}`);
                }}
              />
            ))}
            {sample.choices !== null && (
              <StageChoices
                view={sample.choices}
                look={n === 0 ? "form" : "panel"}
                onPick={(stage, choice) => {
                  setPicked(`${sample.caption}: ${stage} → ${choice}`);
                }}
              />
            )}
            {sample.path !== null && <TicketPath view={sample.path} />}
          </div>
        ))}
      </div>
    </Section>
  );
}

const loadGallery = () => request("gallery.view") as Promise<GalleryView>;
const loadCards = () => request("gallery.cards") as Promise<CardGallery>;
const loadDecisions = () => request("gallery.decisions") as Promise<DecisionGallery>;

/** *07 → Row · Decision Item (0084)* and the SHOP-23 / SHOP-40 review lists (T-0287), from `gallery.decisions`. */
function Decisions() {
  const { view, error } = useServiceView(loadDecisions, []);
  const [picked, setPicked] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const onChange = (item: { id: string }) => {
    setPicked(`Change… ${item.id}`);
  };
  return (
    <>
      <Section title="07 · Decision Item (0084) · every variant (T-0287)">
        <Caption>Last click: {picked ?? "—"}</Caption>
        <div className="gallery-decisions">
          {view.variants.map((v) => (
            <div key={v.name} className="gallery-decision">
              <Caption>{v.name.toUpperCase()}</Caption>
              <DecisionItem item={v.item} onChange={onChange} />
            </div>
          ))}
        </div>
      </Section>
      <Section title="07 · SHOP-23 review · Decisions · SHOP-40 epic review · Decisions (T-0287)">
        <div className="gallery-decisions">
          <DecisionsList section={view.ticket} onChange={onChange} />
          <DecisionsList section={view.epic} onChange={onChange} />
        </div>
      </Section>
    </>
  );
}

const loadEnvVars = () => request("gallery.env_vars") as Promise<EnvVarsGallery>;

/** *15 → Row · Environment variables (0082)* and *… · edit, remove, errors (0082)* (T-0300): the row in each state
 * from `gallery.env_vars`; edits are only recorded, nothing is saved. */
function EnvVarsSection() {
  const { view, error } = useServiceView(loadEnvVars, []);
  const [picked, setPicked] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const settings: SettingsContextValue = {
    page: { page: "project", key: "SHOP", title: "shop-web", intro: "", sections: [] },
    project: "SHOP",
    edit: (field, value) => {
      setPicked(`${field} = ${JSON.stringify(value)}`);
    },
    act: () => undefined,
  };
  return (
    <Section title="15 · Settings · Row · Environment variables (0082) · every state (T-0300)">
      <Caption>Last edit: {picked ?? "—"}</Caption>
      <SettingsContext.Provider value={settings}>
        <div className="gallery-wrap">
          {view.cells.map((cell) => (
            <div key={cell.name} className="gallery-stack" style={{ width: 640 }}>
              <Caption>{cell.name.toUpperCase()}</Caption>
              <EnvVarsRow row={cell.row} />
            </div>
          ))}
        </div>
      </SettingsContext.Provider>
    </Section>
  );
}

const loadWarmSwitch = () => request("gallery.warm_switch") as Promise<WarmSwitchGallery>;

/** *15 → Row · Start new worktrees from a warm build (0082)* and the dark frame's Linux state (T-0301): the row in
 * each state from `gallery.warm_switch`; turning the switch off on the second one opens its confirmation. Nothing is
 * sent. */
function WarmSwitchSection() {
  const { view, error } = useServiceView(loadWarmSwitch, []);
  const [picked, setPicked] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const settings: SettingsContextValue = {
    page: { page: "project", key: "SHOP", title: "shop-web", intro: "", sections: [] },
    project: "SHOP",
    edit: (field, value) => {
      setPicked(`${field} = ${JSON.stringify(value)}`);
    },
    act: () => undefined,
  };
  const withBuild = view.cells.find(
    (c) => c.row.control.kind === "warm_build" && c.row.control.confirm !== null,
  );
  const confirm = withBuild?.row.control.kind === "warm_build" ? withBuild.row.control.confirm : null;
  return (
    <Section title="15 · Settings · Row · Start new worktrees from a warm build (0082) · every state (T-0301)">
      <Caption>Last edit: {picked ?? "—"}</Caption>
      <SettingsContext.Provider value={settings}>
        <div className="gallery-wrap">
          {view.cells.map((cell) =>
            cell.row.control.kind === "warm_build" ? (
              <div key={cell.name} className="gallery-stack" style={{ width: 640 }}>
                <Caption>{cell.name.toUpperCase()}</Caption>
                <strong>{cell.project}</strong>
                <WarmBuildRow row={cell.row} control={cell.row.control} />
              </div>
            ) : null,
          )}
          {confirm && (
            <div className="gallery-stack">
              <Caption>TURNING IT OFF · CONFIRM</Caption>
              <Button
                variant="secondary"
                label="Open the confirmation"
                onClick={() => {
                  setAsking(true);
                }}
              />
            </div>
          )}
        </div>
        {asking && confirm && (
          <WarmOffDialog
            confirm={confirm}
            onConfirm={() => {
              setAsking(false);
              setPicked("warm_builds = false");
            }}
            onCancel={() => {
              setAsking(false);
            }}
          />
        )}
      </SettingsContext.Provider>
    </Section>
  );
}

const loadCleanupWarm = () => request("gallery.cleanup_warm") as Promise<CleanupGallery>;

/** *16 → Row · Cleanup with warm builds (0082)* and *… · no leftovers / no warm builds (0082)* (T-0302): the overlay in
 * each state from `gallery.cleanup_warm`, and the first row's Clear… confirmation. Nothing is sent. */
function CleanupWarmGallery() {
  const { view, error } = useServiceView(loadCleanupWarm, []);
  const [picked, setPicked] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const confirm = view.cells[0]?.page.warm?.rows[0]?.confirm;
  return (
    <Section title="16 · Recovery & cleanup · Cleanup with warm builds (0082) · every state (T-0302)">
      <Caption>Last Clear…: {picked ?? "—"}</Caption>
      <div className="gallery-wrap">
        {view.cells.map((cell) => (
          <div key={cell.name} className="gallery-stack">
            <Caption>{cell.name.toUpperCase()}</Caption>
            <div className="cleanup" data-warm={cell.page.warm ? true : undefined}>
              <CleanupBody
                page={cell.page}
                folded={new Set<string>()}
                error={null}
                onFold={() => undefined}
                onToggle={() => undefined}
                onToggleGroup={() => undefined}
                onDelete={() => undefined}
                onClose={() => undefined}
                onClear={(row) => {
                  setPicked(row.confirm.title);
                }}
              />
            </div>
          </div>
        ))}
        {confirm && (
          <div className="gallery-stack" style={{ width: 480 }}>
            <Caption>CLEAR… · CONFIRM</Caption>
            <strong>{confirm.title}</strong>
            <p className="dialog-text">{confirm.text}</p>
            <p className="dialog-text">{confirm.shared}</p>
            <ul className="cleanup-confirm-lines">
              <li>
                <Icon name={confirm.line.icon} size={13} />
                <span>{confirm.line.text}</span>
              </li>
            </ul>
            <Button variant="destructive" icon="trash-2" label={confirm.confirm} />
          </div>
        )}
      </div>
    </Section>
  );
}

const loadTrust = () => request("gallery.trust") as Promise<TrustGallery>;
const loadAgentTools = () => request("gallery.agent_tools") as Promise<AgentToolsGallery>;
const loadOnboardingTools = () => request("gallery.onboarding_tools") as Promise<OnboardingToolsGallery>;
const loadFlowEditor = () => request("gallery.flow_editor") as Promise<FlowEditorGallery>;
const loadProjectMenu = () => request("gallery.project_menu") as Promise<ProjectMenuGallery>;

const THEMES = ["otaq-light", "otaq-dark"] as const;

/** The ? list's three samples (`GalleryView.shortcut_lists`), as the artboards name them. */
const SHORTCUT_LISTS = [
  "? · every shortcut, grouped",
  "After you change a shortcut",
  "Single letters switched off",
];
const themed = { padding: 16, background: "var(--bg-app)", color: "var(--text-primary)" };

/** *15* `bGHVl` / `ehz4J` (T-0365, d0086): Settings → Agent tools from `gallery.agent_tools` in Otaq Light and Dark: the
 * sample, every state for each tool, Codex's notes and a setup that failed, and Disconnect's dialog and its refusal.
 * Calls are only recorded; nothing is sent. */
function AgentToolsGallerySection() {
  const { view, error } = useServiceView(loadAgentTools, []);
  const [picked, setPicked] = useState<string | null>(null);
  const [dialog, setDialog] = useState<number | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const shown = dialog === null ? undefined : view.dialogs[dialog];
  return (
    <Section title="15 · Settings → Agent tools: every state per tool, the notes, Disconnect (T-0365)">
      <Caption>Last call: {picked ?? "—"}</Caption>
      <div className="gallery-wrap">
        {view.dialogs.map((d, index) => (
          <Button
            key={`${d.title}-${String(index)}`}
            variant="secondary"
            icon="unlink"
            label={d.error === null ? d.title : `${d.title} · refused`}
            onClick={() => {
              setDialog(index);
            }}
          />
        ))}
      </div>
      <div className="gallery-two">
        {THEMES.map((theme) => (
          <div key={theme} data-theme={theme} className="gallery-stack" style={themed}>
            <Caption>{theme.toUpperCase()}</Caption>
            {view.pages.map((cell) => (
              <div
                key={cell.name}
                className="gallery-stack-tight"
                style={{ padding: "0 16px 16px", borderRadius: 8, background: "var(--bg-panel)" }}
              >
                <Caption>{cell.name.toUpperCase()}</Caption>
                <AgentToolsPageBody
                  page={cell.page}
                  onAction={(tool, action) => {
                    setPicked(`${tool} · ${action.kind === "menu" ? action.action : action.kind}`);
                  }}
                  onRecheck={() => {
                    setPicked("Re-check");
                  }}
                />
              </div>
            ))}
          </div>
        ))}
      </div>
      {shown && (
        <DisconnectDialogView
          dialog={shown}
          onCancel={() => {
            setDialog(null);
          }}
          onConfirm={() => {
            setPicked(`disconnect ${shown.tool}`);
            setDialog(null);
          }}
        />
      )}
    </Section>
  );
}

/** *14* `y2FCwP` / `vnBdi`, `z8OVF` / `rZRuR` (T-0365, d0086): the Tools step with agent tools from
 * `gallery.onboarding_tools` in Otaq Light and Dark: three connected, none connected (Continue off), one connected,
 * a hidden tool beside a warning. Calls are only recorded. */
function OnboardingToolsGallerySection() {
  const { view, error } = useServiceView(loadOnboardingTools, []);
  const [picked, setPicked] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const record = (what: string) => () => {
    setPicked(what);
  };
  return (
    <Section title="14 · Tools step with agent tools: sample, none connected, one connected, hidden and warned (T-0365)">
      <Caption>Last call: {picked ?? "—"}</Caption>
      <div className="gallery-two">
        {THEMES.map((theme) => (
          <div key={theme} data-theme={theme} className="gallery-stack" style={themed}>
            <Caption>{theme.toUpperCase()}</Caption>
            {view.cells.map((cell) => (
              <div key={cell.name} className="gallery-stack-tight">
                <Caption>{cell.name.toUpperCase()}</Caption>
                <section className="onboarding-card">
                  <ToolsList
                    tools={cell.tools}
                    checking={false}
                    warning={cell.tools.warning !== null}
                    onDismiss={record("close the warning")}
                    onRecheck={record("Re-check")}
                    onContinue={record("Continue")}
                    onAgentAction={(tool, action) => {
                      setPicked(`${tool} · ${action.kind === "menu" ? action.action : action.kind}`);
                    }}
                  />
                </section>
              </div>
            ))}
          </div>
        ))}
      </div>
    </Section>
  );
}

/** *27 → The flow editor* `s27-03`/`s27-05`/`s27-06` (T-0471, T-0472, FED-01…FED-07): the editor in the project
 * (shop-web's flow) and in onboarding (Otaq default, two tools), then a new stage with Plan picked (its templates, the
 * empty skill), its skill being written and written, from `gallery.flow_editor`, in Otaq Light and Dark at the frames'
 * 1376 × 808. Clicks are only recorded. */
function FlowEditorGallerySection() {
  const { view, error } = useServiceView(loadFlowEditor, []);
  const [picked, setPicked] = useState<string | null>(null);
  const actions = useMemo(() => recordingActions(setPicked), []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <Section title="27 · The flow editor (T-0471, T-0472)">
      <Caption>Last call: {picked ?? "—"}</Caption>
      {THEMES.map((theme) => (
        <div key={theme} data-theme={theme} className="gallery-stack" style={themed}>
          <Caption>{theme.toUpperCase()}</Caption>
          {view.cells.map(([name, cell]) => (
            <div key={name} className="gallery-stack-tight">
              <Caption>{name.toUpperCase()}</Caption>
              <div className="fe-dialog" style={{ width: 1376, height: 808, flex: "none" }}>
                <FlowEditorBody view={cell} actions={actions} />
              </div>
            </div>
          ))}
        </div>
      ))}
    </Section>
  );
}

/** *26 → Row · Changing the flow later* `s26-08`/`s26-12` (T-0473, FED-09): the project menu with the flow in use and
 * with a change waiting, from `gallery.project_menu`, in Otaq Light and Dark. Choices are only recorded. */
function ProjectMenuGallerySection() {
  const { view, error } = useServiceView(loadProjectMenu, []);
  const [picked, setPicked] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <Section title="26 · The project menu (T-0473)">
      <Caption>Last call: {picked ?? "—"}</Caption>
      {THEMES.map((theme) => (
        <div key={theme} data-theme={theme} className="gallery-wrap" style={themed}>
          {view.cells.map(([name, menu]) => (
            <div key={name} className="gallery-stack-tight">
              <Caption>{`${theme} · ${name}`.toUpperCase()}</Caption>
              <Menu
                label={menu.title}
                head={{ title: menu.title, detail: menu.path }}
                items={menu.items}
                width={300}
                onChoose={setPicked}
                onClose={() => undefined}
              />
            </div>
          ))}
        </div>
      ))}
    </Section>
  );
}

/** The folder trust (T-0306, d0085; *14 → step 4*, *11 → 4c*), in Otaq Light and Otaq Dark side by side: the
 * add-project form with its line and after a failed trust (Continue), the note, the failed warning for one and two
 * projects (hover the text for the reasons), and both around the permission warning. Nothing is sent. */
const loadDrawer = () => request("gallery.drawer") as Promise<DrawerGallery>;

/** *17 · Project terminals → Row · Terminal 1* (`rGcYC`) and *Row* (`LX0lX`) (T-0331, d0087): the Terminals drawer with
 * Project terminals only, from `drawer.rs`: a fresh Terminal 1 with its hint, Terminal 2 exited, and the empty drawer
 * after a restart. No terminal attaches here; + and the tabs talk to the service as in the app. */
function DrawerSection() {
  const { view, error } = useServiceView(loadDrawer, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <Section title="17 · Project terminals: Terminal 1 with its hint, ended, after a restart (T-0331)">
      <div className="gallery-stack">
        {view.states.map((state) => (
          <div key={state.name} className="gallery-stack-tight">
            <Caption>{state.name.toUpperCase()}</Caption>
            <div style={{ width: 1100, maxWidth: "100%" }}>
              <DrawerBody
                view={state.view}
                project="NOTE"
                maximized={false}
                onMaximize={() => undefined}
                onHide={() => undefined}
                preview
              />
            </div>
          </div>
        ))}
      </div>
    </Section>
  );
}

const loadPane = () => request("gallery.pane") as Promise<PaneGallery>;

/** *18 → Row · Pane: empty, hidden, tool problems, menu (0087)*, *Row · More pane states (0086 · V)* and *Row ·
 * Switching agents, attention (0086 · V)* (T-0330): the panel in each state, the switcher and the … menu open.
 * Clicks talk to the service as in the app. */
function PaneSection() {
  const { view, error } = useServiceView(loadPane, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const running = view.states.find((s) => s.view.agent !== null)?.view;
  return (
    <Section title="18 · Project agent panel: empty, running, waiting, ended, ended by restart, switcher, … menu (T-0330)">
      <div className="gallery-wrap gallery-wrap-wide" style={{ alignItems: "flex-start" }}>
        {view.states.map((state) => (
          <div key={state.name} className="gallery-stack-tight">
            <Caption>{state.name.toUpperCase()}</Caption>
            <div className="gallery-pane">
              <PaneBody view={state.view} project="NOTE" />
            </div>
          </div>
        ))}
        {running && (
          <>
            <div className="gallery-stack-tight">
              <Caption>SWITCHER (NAME ▾) OPEN</Caption>
              <div className="popover" style={{ position: "static" }}>
                <SwitcherList switcher={running.switcher} onPick={() => undefined} />
              </div>
            </div>
            <div className="gallery-stack-tight">
              <Caption>THE PANE&apos;S … MENU</Caption>
              <Menu
                label={running.header.more.label}
                items={running.menu}
                onChoose={() => undefined}
                onClose={() => undefined}
              />
            </div>
          </>
        )}
      </div>
    </Section>
  );
}

const loadAskBar = () => request("gallery.ask_bar") as Promise<AskBarGallery>;

/**
 * *24 → Row · The Project agent* `s24-05` (T-0458): the Ask bar closed, typing with "ASK ABOUT", a reply waiting, and
 * the rail's Project agent button in each state (tinted while open, a dot while a reply waits).
 */
function AskBarSection() {
  const { view, error } = useServiceView(loadAskBar, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <Section title="24 · The Ask bar and the rail's Project agent button (T-0458)">
      <div className="gallery-wrap gallery-wrap-wide" style={{ alignItems: "flex-end" }}>
        {view.states.map((state) => (
          <div key={state.name} className="gallery-stack-tight gallery-ask">
            <Caption>{state.name.toUpperCase()}</Caption>
            <AskBarBody view={state.view} text={state.typed} typing={state.typed !== ""} choice={0} />
            <div className="rail gallery-ask-rail">
              <RailAgentButton button={state.view.button} />
            </div>
          </div>
        ))}
      </div>
    </Section>
  );
}

const loadBoardReview = () => request("gallery.board_review") as Promise<BoardGallery>;

function verdictText(verdict: DragVerdict): string {
  switch (verdict.kind) {
    case "home":
      return "home";
    case "allowed":
      return `✓ ${verdict.hint}`;
    case "forbidden":
      return `⊘ ${verdict.title}`;
  }
}

/** *03 → Row · Review column (0086 · V)*, *Empty column & empty board* and the agent filter (T-0324, d0087): the
 * view models' words for the Review column in each state, every drop target's verdict for a card from each column,
 * the empty board and the "Created or changed by" chip. The board page draws them from T-0328. */
function BoardReviewSection() {
  const { view, error } = useServiceView(loadBoardReview, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <Section title="03 · Board & columns · Review column, drop targets, empty board, agent filter (T-0324, d0087)">
      <div className="gallery-wrap">
        {view.review.map((cell) => (
          <div key={cell.name} className="gallery-stack-tight" style={{ width: 240 }}>
            <Caption>{cell.name.toUpperCase()}</Caption>
            <strong>
              {cell.column.title} · {cell.column.count}
            </strong>
            {cell.column.sections.map((section) => (
              <div key={section.id} className="gallery-stack-tight">
                <Caption>
                  {section.title} {section.count}
                </Caption>
                {section.cards.map((card) => (
                  <span key={card.card.key}>
                    {card.card.key} · {card.card.title}
                  </span>
                ))}
                {section.cards.length === 0 && <Caption>{section.empty}</Caption>}
                {section.note && <Caption>{section.note.text}</Caption>}
                {section.drag_hint !== null && section.cards.length === 0 && (
                  <Caption>While dragging: {section.drag_hint}</Caption>
                )}
              </div>
            ))}
          </div>
        ))}
      </div>
      <table style={{ fontSize: 12, borderSpacing: "8px 4px", textAlign: "left" }}>
        <thead>
          <tr>
            <th>Dragged</th>
            {view.drags[0]?.answer.targets.map((t) => (
              <th key={t.target}>{t.target}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {view.drags.map((drag) => (
            <tr key={drag.from}>
              <td>{drag.from}</td>
              {drag.answer.targets.map((t) => (
                <td key={t.target}>{verdictText(t.verdict)}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      <div className="gallery-stack-tight" style={{ maxWidth: 520 }}>
        <Caption>EMPTY BOARD</Caption>
        <strong>{view.empty.title}</strong>
        <span>{view.empty.text}</span>
        <span>{view.empty.buttons.map((b) => b.label).join(" · ")}</span>
        <Caption>{view.empty.hints.map((h) => `${h.keys ?? ""} ${h.label}`).join(" · ")}</Caption>
      </div>
      <div className="gallery-stack-tight">
        <Caption>AGENT FILTER CHIP</Caption>
        <span title={view.agent_chip.clear}>{view.agent_chip.label} ✕</span>
      </div>
    </Section>
  );
}

const NO_CARD_HANDLERS: ColumnProps["handlers"] = {
  press: () => undefined,
  pressChild: () => undefined,
  key: () => undefined,
  open: () => undefined,
  action: () => undefined,
  addToEpic: () => undefined,
  menu: () => undefined,
  hover: () => undefined,
};

/** *03 → Row · Review column (0086 · V)* (`pheSu`, Dark `SDm9C`; T-0328, d0087): the board's own Review column from
 * the T-0324 fixtures: both sections, an empty Approved, Approved as the drop target of a dragged Ready for Review
 * card (its verdict from the fixtures' drag answer), Approved empty while that card is dragged elsewhere, a folded
 * section, and the empty column. Nothing is sent; the labels fold nothing here. */
function ReviewColumnSection() {
  const { view, error } = useServiceView(loadBoardReview, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const [both, emptyApproved, emptyReady, empty] = view.review;
  const rfr = view.drags.find((d) =>
    d.answer.targets.some((t) => t.target === "approved" && t.verdict.kind === "allowed"),
  );
  const verdict = rfr?.answer.targets.find((t) => t.target === "approved")?.verdict;
  const cells: { name: string; column: BoardColumn | undefined; props?: Partial<ColumnProps> }[] = [
    { name: "sections", column: both?.column },
    { name: "empty section", column: emptyApproved?.column },
    {
      name: "drop target",
      column: both?.column,
      props: { dragging: true, lifted: rfr?.answer.key ?? null, approved: { verdict, over: true } },
    },
    {
      name: "dragging, Approved empty",
      column: emptyApproved?.column,
      props: { dragging: true, lifted: rfr?.answer.key ?? null, approved: { verdict, over: false } },
    },
    { name: "folded", column: both?.column, props: { foldedSections: ["ready_for_review"] } },
    { name: "Ready for Review empty", column: emptyReady?.column },
    { name: "empty", column: empty?.column },
  ];
  return (
    <Section title="03 · Review column: sections, empty, drop target, folded (T-0328)">
      <div className="gallery-wrap">
        {cells.map(
          (cell) =>
            cell.column && (
              <div key={cell.name} className="gallery-stack-tight">
                <Caption>{cell.name.toUpperCase()}</Caption>
                <div
                  className="board"
                  style={
                    {
                      width: 260,
                      height: 520,
                      "--column-width": "240px",
                      "--open-columns": 1,
                    } as CSSProperties
                  }
                >
                  <div className="board-columns">
                    <Column
                      column={cell.column}
                      chipWords={{ above: "{n} above", need_you: "{n} need you" }}
                      dragging={false}
                      selected={null}
                      lifted={null}
                      handlers={NO_CARD_HANDLERS}
                      scrollTop={0}
                      onScroll={() => undefined}
                      onView={() => undefined}
                      onAdd={() => undefined}
                      element={() => undefined}
                      {...cell.props}
                    />
                  </div>
                </div>
              </div>
            ),
        )}
      </div>
    </Section>
  );
}

/** T-0460 (BRD-01, BRD-02; *24 · The two views*, `s26-10`): the toolbar on the Board and on the List, folded
 * columns (a rail with the needs-you dot, an empty one), Done's line and the "7 more stages" pill. */
function BoardViewSection() {
  const { view, error } = useServiceView(loadBoardReview, []);
  const [picked, setPicked] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <Section title="24 · The board's toolbar, folded columns, Done's line, more stages (T-0460)">
      <Caption>Last click: {picked ?? "—"}</Caption>
      {view.toolbars.map((t) => (
        <div key={t.name} className="gallery-stack-tight">
          <Caption>{t.name.toUpperCase()}</Caption>
          <Toolbar
            toolbar={t.toolbar}
            onView={(change) => {
              setPicked(JSON.stringify(change));
            }}
            onAllTickets={() => {
              setPicked("all tickets");
            }}
            onNewTicket={() => {
              setPicked("new ticket");
            }}
          />
        </div>
      ))}
      <div className="gallery-wrap">
        {view.rails.map((cell) => (
          <div key={cell.name} className="gallery-stack-tight">
            <Caption>{cell.name.toUpperCase()}</Caption>
            <div className="board-columns" style={{ height: 240 }}>
              <Column
                column={cell.column}
                chipWords={{ above: "{n} above", need_you: "{n} need you" }}
                dragging={false}
                selected={null}
                lifted={null}
                handlers={NO_CARD_HANDLERS}
                scrollTop={0}
                onScroll={() => undefined}
                onView={(change) => {
                  setPicked(JSON.stringify(change));
                }}
                onAdd={() => undefined}
                element={() => undefined}
              />
            </div>
          </div>
        ))}
        <div className="gallery-stack-tight">
          <Caption>DONE FOLDED</Caption>
          <button type="button" className="board-done-line">
            {view.done.text}
          </button>
        </div>
        <div className="gallery-stack-tight">
          <Caption>MORE STAGES</Caption>
          <button type="button" className="board-more-stages" style={{ position: "static" }}>
            <span>{view.more}</span>
            <Icon name="arrow-right" size={12} />
          </button>
        </div>
      </div>
    </Section>
  );
}

const loadNotifications = () => request("gallery.notifications") as Promise<NotificationsGallery>;

/** T-0372 (ATT-13, ATT-14, ATT-16; d0090, no frames): the notifications ask and the blocked banner with their
 * stacking, then Settings → Notifications in every status-line state and with the switch off, from
 * `gallery.notifications`, in Otaq Light and Dark. Clicks are only recorded; nothing is sent. */
function NotificationsGallerySection() {
  const { view, error } = useServiceView(loadNotifications, []);
  const [picked, setPicked] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const settings: SettingsContextValue = {
    page: { page: "global", title: "Notifications", sections: [] },
    project: null,
    edit: (field, value) => {
      setPicked(`${field} = ${JSON.stringify(value)}`);
    },
    act: (action) => {
      setPicked(action);
    },
  };
  return (
    <Section title="11 · Notifications ask and blocked banner · 15 · Settings → Notifications (T-0372)">
      <Caption>Last click: {picked ?? "—"}</Caption>
      <SettingsContext.Provider value={settings}>
        <div className="gallery-two">
          {THEMES.map((theme) => (
            <div key={theme} data-theme={theme} className="gallery-stack" style={themed}>
              <Caption>{theme.toUpperCase()}</Caption>
              {view.banners.map((cell) => (
                <div key={cell.name} className="gallery-stack-tight">
                  <Caption>{cell.name.toUpperCase()}</Caption>
                  {cell.banners.map((b) => (
                    <Banner
                      key={b.kind}
                      tone={b.tone}
                      icon={b.icon}
                      title={b.title}
                      text={b.text}
                      actions={b.buttons.map((button) => ({
                        label: button.label,
                        icon: button.icon,
                        onClick: () => {
                          setPicked(`${b.kind} · ${button.label}`);
                        },
                      }))}
                      onDismiss={
                        b.dismissable
                          ? () => {
                              setPicked(`${b.kind} · dismiss`);
                            }
                          : undefined
                      }
                    />
                  ))}
                </div>
              ))}
              {view.pages.map((cell) => (
                <div
                  key={cell.name}
                  className="gallery-stack-tight"
                  style={{ padding: "0 16px 16px", borderRadius: 8, background: "var(--bg-panel)" }}
                >
                  <Caption>{cell.name.toUpperCase()}</Caption>
                  <SettingsSectionRows section={cell.section} />
                </div>
              ))}
            </div>
          ))}
        </div>
      </SettingsContext.Provider>
    </Section>
  );
}

const loadShortcuts = () => request("gallery.keyboard_page") as Promise<ShortcutsGallery>;

/** T-0395 (KEY-05, KEY-04; d0097; *15 · Keyboard shortcuts* `s15-46`…`s15-55`): Settings → Keyboard shortcuts in
 * every frame, macOS and Windows / Linux, from `gallery.keyboard_page`, in Otaq Light and Dark. Clicks and pressed
 * keys are only recorded; nothing is saved. */
function ShortcutsGallerySection() {
  const { view, error } = useServiceView(loadShortcuts, []);
  const [picked, setPicked] = useState<string | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const calls: ShortcutsCalls = {
    record: (slot, press) => {
      setPicked(`record ${JSON.stringify(slot.action)} ${String(slot.slot)} ${press.key}`);
      return Promise.resolve({ outcome: "ignored" });
    },
    edit: (edit) => {
      setPicked(JSON.stringify(edit));
      return Promise.resolve({ outcome: "done", changed: 0 });
    },
    resetRow: (keys) => {
      setPicked(`reset ${JSON.stringify(keys)}`);
      return Promise.resolve({ outcome: "done", changed: 0 });
    },
  };
  return (
    <Section title="15 · Settings → Keyboard shortcuts: defaults, changed, letters off, recording, taken, refused, Windows / Linux (T-0395)">
      <Caption>Last click: {picked ?? "—"}</Caption>
      <div className="gallery-two">
        {THEMES.map((theme) => (
          <div key={theme} data-theme={theme} className="gallery-stack" style={themed}>
            <Caption>{theme.toUpperCase()}</Caption>
            {view.cells.map((cell) => (
              <div
                key={cell.name}
                className="gallery-stack-tight"
                data-cell={cell.name}
                style={{ padding: "0 24px 14px", borderRadius: 10, background: "var(--bg-panel)" }}
              >
                <Caption>
                  {cell.name.toUpperCase()} · {cell.note}
                </Caption>
                <ShortcutsView
                  page={cell.page}
                  calls={calls}
                  moment={cell.moment}
                  rows={cell.rows.length === 0 ? undefined : cell.rows}
                  header={cell.header}
                />
              </div>
            ))}
          </div>
        ))}
      </div>
    </Section>
  );
}

function TrustSection() {
  const { view, error } = useServiceView(loadTrust, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const noop = () => undefined;
  return (
    <Section title="14 · Add project · folder trust line and failed state · 11 · 4c folder trust banners (T-0306)">
      <div className="gallery-two">
        {(["otaq-light", "otaq-dark"] as const).map((theme) => (
          <div
            key={theme}
            data-theme={theme}
            className="gallery-stack"
            style={{ padding: 16, background: "var(--bg-app)", color: "var(--text-primary)" }}
          >
            <Caption>{theme.toUpperCase()}</Caption>
            {view.forms.map((cell) => {
              const state = staticAddProject(cell.form);
              return (
                <div
                  key={cell.name}
                  className="gallery-stack-tight"
                  style={{ padding: 16, borderRadius: 8, background: "var(--bg-panel)" }}
                >
                  <Caption>{cell.name.toUpperCase()}</Caption>
                  <AddProjectFields state={state} />
                  <div style={{ alignSelf: "flex-end" }}>
                    <AddButton state={state} />
                  </div>
                </div>
              );
            })}
            {view.banners.map((cell) => (
              <div key={cell.name} className="gallery-stack-tight">
                <Caption>{cell.name.toUpperCase()}</Caption>
                {cell.banners.map((b) => (
                  <Banner
                    key={b.kind}
                    tone={b.tone}
                    icon={b.icon}
                    title={b.title}
                    text={b.text}
                    tooltip={b.tooltip ?? undefined}
                    actions={b.buttons.map((button) => ({
                      label: button.label,
                      icon: button.icon,
                      onClick: noop,
                    }))}
                    onDismiss={b.dismissable ? noop : undefined}
                  />
                ))}
              </div>
            ))}
          </div>
        ))}
      </div>
    </Section>
  );
}

const loadStrip = () => request("gallery.needs_you_strip") as Promise<StripGallery>;

/**
 * *24 → Row · The two views* `s24-03` and *Row · Needs you: the next project, and every group* `s24-12` (T-0459): the
 * Needs you strip for the example's shop-web, with every group, "Needs you · 0 here" with Next to billing-api and with
 * nothing anywhere; the lead's suggestion opened, and the line its Confirm adds to the usual confirmation. Nothing is
 * sent.
 */
function NeedsYouStripSection() {
  const { view, error } = useServiceView(loadStrip, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  const suggestion = view.states.flatMap((s) => s.view.groups).flatMap((g) => g.suggestions)[0];
  return (
    <Section title="24 · The Needs you strip (T-0459)">
      <div className="gallery-stack">
        {view.states.map((state) => (
          <div key={state.label} className="gallery-stack-tight">
            <Caption>{state.label}</Caption>
            <StripBody view={state.view} project="SHOP" />
          </div>
        ))}
        {suggestion && (
          <div className="gallery-wrap">
            <div className="gallery-stack-tight">
              <Caption>A suggestion opened</Caption>
              <div className="popover gallery-strip-suggestion">
                <SuggestionCard suggestion={suggestion} />
              </div>
            </div>
            <div className="gallery-stack-tight">
              <Caption>Confirm → the usual confirmation (0044), with the lead's line</Caption>
              <SuggestedBy line={suggestion.line} />
            </div>
          </div>
        )}
      </div>
    </Section>
  );
}

const loadPause = () => request("gallery.pause") as Promise<PauseGallery>;

/**
 * *24 → Row · Pause and resume* `s24-07` and *Row · Resume asks for the scope too, and a restart* `s24-08` (T-0457):
 * Pause and Resume in the title bar with their scope menus open (This project highlighted), and the paused banner's
 * three texts. Nothing is sent.
 */
function PauseSection() {
  const { view, error } = useServiceView(loadPause, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <Section title="24 · Pause and Resume (T-0457)">
      <div className="gallery-stack">
        <div className="gallery-wrap">
          {view.states.map((state) => (
            <div key={state.name} className="gallery-stack-tight">
              <Caption>{state.name}</Caption>
              <PauseButton button={state.view.button} open />
              <div className="popover">
                <PauseMenuBox menu={state.view.menu} cursor={0} />
              </div>
            </div>
          ))}
        </div>
        {view.banners.map((b) => (
          <Banner key={b.title} tone={b.tone} icon={b.icon} title={b.title} text={b.text} />
        ))}
      </div>
    </Section>
  );
}

const loadOptionNotes = () => request("gallery.option_notes") as Promise<OptionNotesGallery>;

/** ATT-17, d0061 (T-0375): answer options with notes, from `gallery.option_notes`: NOTE-1's four options (one note
 * about 300 characters long, one option without a note) and a question the lead passed on, in the panel and the
 * card. The card's hover card shows on hover or Tab; each one is also drawn open beside its card. Nothing is
 * sent. */
function OptionNotes() {
  const { view, error } = useServiceView(loadOptionNotes, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <Section title="05 · 04 · Answer options with notes (0061) · panel, card hover (T-0375)">
      <div className="gallery-wrap gallery-wrap-wide">
        {view.panels.map(
          (panel) =>
            panel.answer && (
              <div key={panel.key} className="gallery-option-notes-panel">
                <PanelAnswerBox view={panel} answer={panel.answer} more={null} fromCard={null} />
              </div>
            ),
        )}
      </div>
      <div className="gallery-wrap gallery-wrap-wide">
        {view.cards.map((card) => (
          <div key={card.key} className="gallery-wrap">
            <div className="gallery-card-cell">
              {/* The options and their notes are the ticket's (BRD-03): the card's one action is Reply. */}
              <Caption>{card.key} · Reply opens the answer</Caption>
              <TicketCard card={card} />
            </div>
          </div>
        ))}
      </div>
    </Section>
  );
}

const loadDecisionChange = () => request("gallery.decision_change") as Promise<DecisionChangeGallery>;

/** *07 → Change… on SHOP-23* and *Change… on 40.1's decision: add a fix child* (T-0288), from
 * `gallery.decision_change`; each opens over the gallery. */
function DecisionChange() {
  const { view, error } = useServiceView(loadDecisionChange, []);
  const [open, setOpen] = useState<DecisionChangeView | null>(null);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <Section title="07 · Change… on SHOP-23 · Add a fix child on 40.1 (T-0288)">
      <div className="gallery-wrap">
        <Button
          variant="secondary"
          label="Change this decision"
          onClick={() => {
            setOpen(view.send_back);
          }}
        />
        <Button
          variant="secondary"
          label="Add a fix child"
          onClick={() => {
            setOpen(view.fix_child);
          }}
        />
      </div>
      {open && (
        <ChangeDecisionBody
          view={open}
          close={() => {
            setOpen(null);
          }}
        />
      )}
    </Section>
  );
}

/** The gallery over the whole window; Esc or the close button leaves it. */
const loadToolAvatars = () => request("gallery.tool_avatars") as Promise<ToolAvatarGallery>;

/** The status colours an avatar must never resemble (d0087), beside the avatars for the hand check. */
const NOT_LIKE = ["st-attn", "st-investigate", "st-paused", "st-error"] as const;

/** *01 · Components → Provider Mark (tool avatar)* and *01 · Foundations → Update U + V · Tool avatars* (T-0321,
 * d0087): the four avatars in the shown theme (hover each for its tool), next to the four status colours they must
 * not look like. The palette switch shows them in every theme. */
function ToolAvatars() {
  const { view, error } = useServiceView(loadToolAvatars, []);
  if (error) return <p role="alert">{error.message}</p>;
  if (!view) return null;
  return (
    <Section title="01 · Components · Provider Mark (tool avatar) (0087) · the four tools (T-0321)">
      <div className="gallery-wrap gallery-wrap-wide">
        {view.avatars.map((avatar) => (
          <span key={avatar.tool} className="gallery-wrap">
            <ToolAvatar avatar={avatar} />
            <Caption>
              {avatar.letters} · {avatar.hover} · {avatar.background}
            </Caption>
          </span>
        ))}
      </div>
      <div className="gallery-wrap gallery-wrap-wide">
        {NOT_LIKE.map((token) => (
          <span key={token} className="gallery-wrap">
            <span
              aria-hidden="true"
              style={{ width: 16, height: 16, borderRadius: "50%", background: `var(--${token})` }}
            />
            <Caption>{token}</Caption>
          </span>
        ))}
      </div>
      <p className="gallery-note">{view.note}</p>
    </Section>
  );
}

export function Gallery({ close }: OverlayProps<"gallery">) {
  const { view, error } = useServiceView(loadGallery, []);
  const root = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState(() => document.documentElement.dataset.theme ?? "");
  // The palette switch recolors the page without saving; leaving the gallery puts the stored theme back.
  useEffect(() => {
    const before = document.documentElement.dataset.theme;
    return () => {
      if (before !== undefined) document.documentElement.dataset.theme = before;
    };
  }, []);
  useEffect(() => {
    const box = root.current;
    if (!box) return;
    box.focus();
    // Esc leaves the gallery, unless it belongs to a menu or popover inside it (a dialog's Esc never gets here).
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || (e.target as Element | null)?.closest(".menu, .popover")) return;
      e.stopPropagation();
      close();
    };
    box.addEventListener("keydown", onKey);
    return () => {
      box.removeEventListener("keydown", onKey);
    };
  }, [close]);
  const show = (key: ThemeKey) => {
    document.documentElement.dataset.theme = key;
    setShown(key);
  };
  return (
    <div ref={root} className="gallery" role="dialog" aria-modal="true" aria-label="Gallery" tabIndex={-1}>
      <header className="gallery-header">
        <h2>Gallery · 01 · Foundations &amp; Components</h2>
        <PaletteSwitch shown={shown} onShow={show} />
        <IconButton icon="x" label="Close the gallery" onClick={close} />
      </header>
      <div className="gallery-body">
        {error && <p role="alert">{error.message}</p>}
        <Section title="Color Tokens · Otaq Light and Dark (01 · Foundations → Color Tokens)">
          <div className="gallery-two">
            <TokenSwatches theme="otaq-light" />
            <TokenSwatches theme="otaq-dark" />
          </div>
        </Section>
        <Section title="08 · Ticket editor · MD Content · component (T-0249, d0078)">
          <MarkdownSample />
        </Section>
        <Section title="08 · Ticket editor · g · Code block · language picker (T-0254, d0078)">
          <CodeSample />
        </Section>
        <Section title="08 · Ticket editor · MD Toolbar · component (T-0250, d0078)">
          <ToolbarSample />
        </Section>
        <Section title="08 · Ticket editor · f · Caret in a table · the table bar (T-0253, d0078)">
          <TableSample />
        </Section>
        <Section title="08 · Ticket editor · e · Link popover and link bubble (T-0252, d0078)">
          <LinkSample />
        </Section>
        <Section title="08 · Ticket editor · d · Markdown source on (T-0251, d0078)">
          <SourceSample />
        </Section>
        <Section title="02 · The shell at 1280, 1440 and 1920 (T-0327)">
          <ShellFrames />
        </Section>
        <PaneSection />
        <AskBarSection />
        <DrawerSection />
        <Section title="02 · Header v2, Activity states and popover (T-0329)">
          <ActivityGallery />
        </Section>
        <Section title="11 · Limit banner per tool; 02 · Activity with four tools (T-0368)">
          <LimitLinesGallery />
        </Section>
        <Section title="Mark · Command tent (d0076; 15 · Settings → About)">
          <div className="gallery-wrap gallery-wrap-wide">
            {[64, 40, 32, 20, 16].map((size) => (
              <OtaqMark key={size} size={size} label={`Otaq, ${String(size)} px`} />
            ))}
          </div>
        </Section>
        <Decisions />
        <DecisionChange />
        <LeadTabs />
        <StageChoicesSection />
        <Section title="24 · The List view · by stage, status and agent (LST-01, T-0461)">
          <ListGallery />
        </Section>
        <Section title="24 · Dependencies · a blocked card, a blocker, the sheet's section, Add, the blocked dialog (T-0463)">
          <DependenciesGallery />
        </Section>
        <NeedsYouStripSection />
        <PauseSection />
        <OptionNotes />
        <Section title="25 · The ticket sheet · header, details line, path and tabs (T-0464)">
          <SheetGallery />
        </Section>
        <Section title="25 · Overview · the description's bar and the Attachments column (T-0465)">
          <OverviewGallery />
        </Section>
        <Section title="25 · The New ticket form's report, posted as your first comment (RPT-03, T-0474)">
          <NewTicketReportGallery />
        </Section>
        <Section title="25 · Files · the tree, a file, search, All changes, a line comment, the review bar (T-0468)">
          <FilesGallery />
        </Section>
        <Section title="28 · The project Files view · the tree with chips, filters, states, the picker (T-0478)">
          <FilesViewGallery />
        </Section>
        <Section title="25 · Session's switch and lines, Activity's filters (T-0466)">
          <SessionGallery />
        </Section>
        <Section title="25 · The epic's sheet · progress, the lead, TICKETS and the Tickets tab (SHT-07, T-0469)">
          <EpicGallery />
        </Section>
        <EnvVarsSection />
        <WarmSwitchSection />
        <CleanupWarmGallery />
        <ReviewColumnSection />
        <BoardViewSection />
        <TrustSection />
        <NotificationsGallerySection />
        <ShortcutsGallerySection />
        <AgentToolsGallerySection />
        <OnboardingToolsGallerySection />
        <FlowEditorGallerySection />
        <ProjectMenuGallerySection />
        <BoardReviewSection />
        <Section title="Theme picker · 14 → 2 · Theme (a click applies and saves)">
          <ThemeSection />
        </Section>
        {view && (
          <>
            <Section title="Status Legend">
              <Legend tiers={view.legend} view={view} />
            </Section>
            <Section title="Type scale">
              <TypeScale view={view} />
            </Section>
            <Section title="Spacing · radius · density">
              <Spacing view={view} />
            </Section>
            <Section title="Icons · Lucide (ISC), 14 px / 1.5 stroke">
              <Icons view={view} />
            </Section>
            <Section title="Primitives">
              <Primitives view={view} />
            </Section>
            <Section title="Buttons · Button matrix">
              <ButtonMatrix />
            </Section>
            <Section title="Badges & Inputs · Tabs & segmented">
              <TabsAndToggles />
            </Section>
            <Section title="Badges & Inputs · Fields">
              <Fields view={view} />
            </Section>
            <Section title="Overlays & feedback">
              <Overlays view={view} />
            </Section>
            <Section title="Banners">
              <Banners />
            </Section>
            <Section title="Keyboard map (update AF · 0097)">
              <KeyboardMap map={view.keyboard_map} />
            </Section>
            <Section title="02 · The g hint, close up and with a recorded g p (update AF §7, s02-30 / s02-31, T-0391)">
              <div className="gallery-two">
                {THEMES.map((theme) => (
                  <div key={theme} data-theme={theme} className="gallery-stack" style={themed}>
                    <Caption>{theme.toUpperCase()}</Caption>
                    <Caption>The hint, close up</Caption>
                    <SequenceHintLine hint={view.g_hints[0]} />
                    <Caption>With a sequence you recorded</Caption>
                    <SequenceHintLine hint={view.g_hints[1]} />
                  </div>
                ))}
              </div>
            </Section>
            <Section title="02 · The ? shortcut list (update AF §4, s02-24…s02-27, T-0394)">
              <div className="gallery-stack">
                {THEMES.map((theme) => (
                  <div key={theme} data-theme={theme} className="gallery-stack" style={themed}>
                    <Caption>{theme.toUpperCase()}</Caption>
                    {view.shortcut_lists.map((list, i) => (
                      <Fragment key={SHORTCUT_LISTS[i]}>
                        <Caption>{SHORTCUT_LISTS[i]}</Caption>
                        <div className="shortcuts">
                          <ShortcutCard view={list} onSettings={() => undefined} />
                        </div>
                      </Fragment>
                    ))}
                  </div>
                ))}
              </div>
            </Section>
            <Section title="02 · Command palette: just opened, typing, >, no results, another project, can't run, Windows / Linux (update AF §3, s02-20 … s02-23, T-0393)">
              {THEMES.map((theme) => (
                <div key={theme} data-theme={theme} className="gallery-stack" style={themed}>
                  <Caption>{theme.toUpperCase()}</Caption>
                  <PaletteGallery frames={view.palette} />
                </div>
              ))}
            </Section>
          </>
        )}
        <Section title="03 · Board & columns · epic flows 2, 9, 12 · the queue toast (T-0272, d0080)">
          <EpicQueueToasts />
        </Section>
        <Cards />
        <Leads />
        <ToolAvatars />
      </div>
    </div>
  );
}

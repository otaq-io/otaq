// The theme picker (UI-11, d0069; *14 · First run & onboarding → 2 · Theme*, *15 · Settings → Appearance*), shared
// by onboarding (T-0233) and Settings (T-0234): the mode (Match system, Light, Dark), then the 15 theme cards under a
// Dark and a Light heading, each preview a small window drawn in that theme's own variables. One card is checked in
// each heading: the dark and the light theme. A click on a card or a mode applies and saves it at once.
import { useCallback } from "react";

import type { ActiveTheme } from "../generated/ActiveTheme";
import type { ThemeMode } from "../generated/ThemeMode";
import type { ThemeModePick } from "../generated/ThemeModePick";
import type { ThemeParams } from "../generated/ThemeParams";
import type { ThemePick } from "../generated/ThemePick";
import type { ThemePickerCard } from "../generated/ThemePickerCard";
import type { ThemePickerView } from "../generated/ThemePickerView";
import { useServiceView, type ViewState } from "../service";
import { request } from "../tauri";
import { Icon } from "../theme/Icon";
import { osIsDark } from "../theme/theme";
import { Segmented } from "./Tabs";

/** Stores `key` as the light or the dark theme, by its kind (`theme.pick`), and recolors this page at once with the
 * theme that shows now; every window follows on the service's `settings_changed`. */
export async function pickTheme(key: string, root: Pick<HTMLElement, "dataset"> = document.documentElement) {
  const params: ThemePick = { key };
  const active = (await request("theme.pick", params)) as ActiveTheme;
  root.dataset.theme = active.key;
  return active;
}

/** Stores the theme mode (`theme.mode`) and recolors this page at once, like [`pickTheme`]. */
export async function chooseThemeMode(
  mode: ThemeMode,
  root: Pick<HTMLElement, "dataset"> = document.documentElement,
) {
  const params: ThemeModePick = { mode };
  const active = (await request("theme.mode", params)) as ActiveTheme;
  root.dataset.theme = active.key;
  return active;
}

/** The picker: the mode chosen, the dark and the light theme checked, refetched on every settings change. */
export function useThemePicker(): ViewState<ThemePickerView> {
  const load = useCallback(() => {
    const params: ThemeParams = { os_dark: osIsDark() };
    return request("theme.picker", params) as Promise<ThemePickerView>;
  }, []);
  return useServiceView(load, ["settings_changed"]);
}

/** A small window in the card's theme: title bar, sidebar and three columns of cards. */
function Preview({ theme }: { theme: string }) {
  const column = (statuses: readonly string[]) => (
    <span className="preview-column">
      <span className="preview-heading" />
      {statuses.map((s, i) => (
        <span key={i} className="preview-card">
          <span className="preview-dot" style={{ background: `var(--st-${s})` }} />
          <span className="preview-line" />
        </span>
      ))}
    </span>
  );
  return (
    <span className="preview" data-theme={theme} aria-hidden>
      <span className="preview-bar">
        <span className="preview-light" data-light="error" />
        <span className="preview-light" data-light="paused" />
        <span className="preview-light" data-light="approved" />
        <span className="preview-fill" />
        <span className="preview-stub" style={{ width: 18 }} />
      </span>
      <span className="preview-main">
        <span className="preview-side">
          <span className="preview-accent" />
          <span className="preview-stub" style={{ width: 12 }} />
          <span className="preview-stub" style={{ width: 14 }} />
          <span className="preview-stub" style={{ width: 10 }} />
        </span>
        <span className="preview-columns">
          {column(["working", "investigate"])}
          {column(["paused", "error"])}
          {column(["approved"])}
        </span>
      </span>
    </span>
  );
}

function Card({ card, onPick }: { card: ThemePickerCard; onPick: (key: string) => void }) {
  return (
    <button
      type="button"
      role="radio"
      className="theme-card"
      aria-checked={card.selected}
      onClick={() => {
        onPick(card.id);
      }}
    >
      <span className="theme-card-ring">
        <Preview theme={card.id} />
      </span>
      <span className="theme-card-text">
        <span className="theme-card-name">
          <span>{card.name}</span>
          {card.selected && <Icon name="check" size={12} />}
        </span>
        <span className="theme-card-tagline">{card.tagline}</span>
      </span>
    </button>
  );
}

export interface ThemePickerProps {
  view: ThemePickerView;
  onPick: (key: string) => void;
  onMode: (mode: ThemeMode) => void;
  /** Under Settings' row "Theme", which draws the line above; a line closes the picker. */
  inRows?: boolean;
}

export function ThemePicker({ view, onPick, onMode, inRows = false }: ThemePickerProps) {
  return (
    <div className="theme-picker" data-in-rows={inRows || undefined}>
      <div className="theme-modes">
        <Segmented
          label="Theme mode"
          options={view.modes.map((m) => m.label)}
          selected={Math.max(
            0,
            view.modes.findIndex((m) => m.selected),
          )}
          onSelect={(i) => {
            const option = view.modes[i];
            if (option) onMode(option.mode);
          }}
        />
      </div>
      {view.sections.map((section) => (
        <div key={section.label} className="theme-section" role="radiogroup" aria-label={section.label}>
          <span className="theme-section-label">{section.label}</span>
          {section.rows.map((row, r) => (
            <div key={r} className="theme-row">
              {row.map((card) => (
                <Card key={card.id} card={card} onPick={onPick} />
              ))}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

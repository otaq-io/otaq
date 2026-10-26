import { createElement, type ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";

import { overlays } from "../app/overlays";
import { registered, registerScreens } from "../app/registry";
import type { MenuItem } from "../generated/MenuItem";
import type { PriorityView } from "../generated/PriorityView";
import { components, openGalleryIfWanted } from "../screens/components";
import {
  Banner,
  Button,
  ConfirmDialog,
  IconButton,
  Menu,
  PriorityChip,
  StatusPill,
  ThemePicker,
  Toast,
} from ".";

// Rendered without a DOM, with React's server renderer (as the frame's tests).
const html = (element: ReactElement) => renderToStaticMarkup(element);

const P2: PriorityView = {
  number: 2,
  short: "P2",
  word: "High",
  label: "P2 · High",
  icon: "chevron-up",
  ink: "strong",
};

afterEach(() => {
  overlays.store.set([]);
});

describe("primitives draw what the view model says", () => {
  it("a Status Pill has its palette, an icon and the label: never colour alone", () => {
    const out = html(
      createElement(StatusPill, { pill: { palette: "attn", icon: "hand", label: "Needs you" } }),
    );
    expect(out).toMatch(
      /^<span class="status-pill" data-palette="attn"><span class="icon" style="[^"]*hand[^"]*"/,
    );
    expect(out).toContain("<span>Needs you</span>");
  });

  it("a Priority chip is inked as told, with its icon and number", () => {
    const out = html(createElement(PriorityChip, { priority: P2 }));
    expect(out).toContain('data-ink="strong"');
    expect(out).toContain('title="P2 · High"');
    expect(out).toContain("<span>P2</span>");
  });

  it("buttons: variant, the gallery's drawn state, disabled, and the loader while loading", () => {
    const shown = html(createElement(Button, { label: "Approve", variant: "destructive", shown: "hover" }));
    expect(shown).toContain('data-variant="destructive"');
    expect(shown).toContain('data-shown="hover"');
    expect(html(createElement(Button, { label: "Approve", disabled: true }))).toContain("disabled");
    const loading = html(createElement(Button, { label: "Merging…", icon: "check", loading: true }));
    expect(loading).toContain('aria-busy="true"');
    expect(loading).toContain("loader-circle");
    expect(loading).not.toContain("check.svg");
    const icon = html(createElement(IconButton, { icon: "square-terminal", label: "Open terminal" }));
    expect(icon).toContain('aria-label="Open terminal"');
  });
});

describe("overlays and feedback", () => {
  const items: MenuItem[] = [
    {
      id: "open",
      label: "Open panel",
      icon: "panel-right",
      shortcut: "↵",
      danger: false,
      disabled: false,
      hint: null,
      separated: false,
      checked: false,
      header: false,
    },
    {
      id: "",
      label: "PRIORITY",
      icon: null,
      shortcut: null,
      danger: false,
      disabled: false,
      hint: null,
      separated: true,
      checked: false,
      header: true,
    },
    {
      id: "delete",
      label: "Delete…",
      icon: null,
      shortcut: null,
      danger: true,
      disabled: true,
      hint: "Only for tickets that never started",
      separated: false,
      checked: false,
      header: false,
    },
  ];

  it("a menu has its rows, separators, headers, the disabled row's hint and the danger tone", () => {
    const noop = () => undefined;
    const out = html(createElement(Menu, { label: "Card actions", items, onChoose: noop, onClose: noop }));
    expect(out).toContain('role="menu" aria-label="Card actions" tabindex="-1"');
    expect(out.match(/role="menuitem"/g)).toHaveLength(2);
    expect(out).toContain('role="separator"');
    expect(out).toContain('<div class="menu-header">PRIORITY</div>');
    expect(out).toMatch(/aria-disabled="true" class="menu-item" data-danger="true"/);
    expect(out).toContain("Only for tickets that never started");
    expect(out).toContain('<span class="menu-item-keys">↵</span>');
    const list = html(
      createElement(Menu, { label: "Priority", items, list: true, onChoose: noop, onClose: noop }),
    );
    expect(list).toContain('role="menuitemradio" aria-checked="false"');
    // A dropdown's list is marked, so its height stops at 320 px and it scrolls (components.css).
    expect(list).toContain('class="menu" data-list="true"');
    expect(out).not.toContain("data-list");
  });

  it("a destructive confirm dialog makes Cancel the default button, outlined; an ordinary one the confirm", () => {
    const noop = () => undefined;
    const props = {
      title: "Delete SHOP-21?",
      body: "It never started.",
      confirmLabel: "Delete",
      onConfirm: noop,
      onCancel: noop,
    };
    const destructive = html(createElement(ConfirmDialog, { ...props, destructive: true }));
    expect(destructive).toContain('role="dialog" aria-modal="true"');
    expect(destructive).toMatch(/data-variant="secondary"[^>]*><span>Cancel<\/span>/);
    expect(destructive).toMatch(/data-variant="destructive"[^>]*><span>Delete<\/span>/);
    expect(destructive).toContain('<kbd class="kbd">esc</kbd><span>cancel</span>');
    const primary = html(createElement(ConfirmDialog, props));
    expect(primary).toMatch(/data-variant="ghost"[^>]*><span>Cancel<\/span>/);
    expect(primary).toMatch(/data-variant="primary"[^>]*><span>Delete<\/span>/);
  });

  it("a confirm dialog without a confirm button: only its cancel, with its own label (Q-N)", () => {
    const noop = () => undefined;
    const only = html(
      createElement(ConfirmDialog, {
        title: "SHOP-18 has unsent text in its terminal",
        body: "Codex can’t clear a multi-line draft. Clear it in the terminal, then send again.",
        confirmLabel: null,
        cancelLabel: "Keep it and cancel",
        onConfirm: noop,
        onCancel: noop,
      }),
    );
    expect(only).toMatch(/data-variant="primary"[^>]*><span>Keep it and cancel<\/span>/);
    expect(only.match(/<button/g)).toHaveLength(1);
    expect(only).not.toContain("Clear it and send");
  });

  it("banners and toasts", () => {
    const noop = () => undefined;
    const banner = html(
      createElement(Banner, {
        tone: "attention",
        icon: "gauge",
        title: "Usage limit reached",
        text: "Resets at 3:00 pm",
        actions: [{ label: "Resume", disabled: true, onClick: noop }],
        onDismiss: noop,
      }),
    );
    expect(banner).toContain('data-tone="attention" role="status"');
    expect(banner).toMatch(/data-compact="true"[^>]*disabled=""/);
    expect(banner).toContain('aria-label="Dismiss"');
    const toast = html(createElement(Toast, { kind: "error", title: "gh is not signed in" }));
    expect(toast).toContain('role="alert"');
    expect(toast).toContain("triangle-alert");
  });
});

describe("the theme picker", () => {
  it("draws each card's preview in its own theme and checks the one in use", () => {
    const card = (id: string, selected: boolean) => ({ id, name: id, tagline: "t", selected });
    const out = html(
      createElement(ThemePicker, {
        sections: [{ label: "Dark", rows: [[card("dracula", true), card("nord", false)]] }],
        onPick: () => undefined,
      }),
    );
    expect(out).toContain('role="radiogroup" aria-label="Dark"');
    expect(out).toContain('data-theme="dracula"');
    expect(out).toMatch(/aria-checked="true"[^]*data-theme="dracula"/);
    expect(out.match(/aria-checked="false"/g)).toHaveLength(1);
  });
});

describe("the components screen", () => {
  it("registers the confirm dialog and the gallery, and nothing else", () => {
    const app = registerScreens({ components });
    expect(registered(app)).toEqual({
      slots: [],
      overlays: ["confirm", "gallery"],
      sheetTabs: [],
      sheetSections: [],
      globalSettings: [],
      projectSettings: [],
      shortcuts: [],
    });
  });

  it("opens the gallery only when the app was started with OTAQ_GALLERY", async () => {
    const asked: string[] = [];
    const answer = (value: unknown) => (method: string) => {
      asked.push(method);
      return Promise.resolve(value);
    };
    expect(await openGalleryIfWanted(answer(false))).toBe(false);
    expect(overlays.store.get()).toEqual([]);
    expect(await openGalleryIfWanted(() => Promise.reject(new Error("no Tauri")))).toBe(false);
    expect(await openGalleryIfWanted(answer(true))).toBe(true);
    expect(overlays.store.get()).toEqual([{ id: "gallery", params: undefined }]);
    expect(asked).toEqual(["gallery.wanted", "gallery.wanted"]);
  });
});

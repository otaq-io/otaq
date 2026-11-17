// A small select's choices (a model, an effort) as menu items, and what a picked one sends: shared by the screens
// that pick a model and an effort (Noyan's panel, the flow editor).
import type { AgentOption } from "../../generated/AgentOption";
import type { MenuItem } from "../../generated/MenuItem";

/** The options as a menu's items, the selected one checked. */
export function optionItems(options: readonly AgentOption[]): MenuItem[] {
  return options.map((o) => ({
    id: o.id,
    label: o.label,
    icon: null,
    shortcut: null,
    danger: false,
    disabled: false,
    hint: null,
    separated: false,
    checked: o.selected,
    header: false,
  }));
}

/** What a picked option sends: its id, or none for the empty id (the default model, the tool's effort). */
export function optionValue(id: string): string | null {
  return id === "" ? null : id;
}

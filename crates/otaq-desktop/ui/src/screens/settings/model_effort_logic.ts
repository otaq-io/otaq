// The model menu's lines and what a chosen line picks (`ModelEffort.tsx`), apart from React so the tests read them.
// Everything the lines say is Rust's (`otaq-view`'s `model_effort.rs`).
import type { ModelEffortEffort } from "../../generated/ModelEffortEffort";
import type { ModelEffortMenu } from "../../generated/ModelEffortMenu";
import type { Choice } from "./ModelPicker";

/** What a chosen line of the model menu picks: a model alone (its effort stays, or moves to the nearest level the
 * new tool offers), or a pair of "As in your other flows", which sets both. */
export type ModelEffortPick =
  { kind: "model"; model: string } | { kind: "pair"; model: string; effort: string | null };

const line = (kind: Choice["kind"], id: string, label: string): Choice => ({
  kind,
  id,
  label,
  detail: null,
  mono: false,
  detailMono: false,
  checked: false,
  icon: null,
  avatar: null,
});

/** The short menu's last line, "Another model or effort…", which opens the whole menu (the Noyan panel's header). */
export const ANOTHER_ID = "another";

const EFFORT_PREFIX = "effort:";

/** Which lines: a short menu (`menu.another`) shows only its pairs and "Another model or effort…" until `full`; the
 * whole one adds `effort`'s levels under the groups (the header has no room for the Effort switch beside it). */
export interface ChoicesOf {
  full?: boolean;
  effort?: ModelEffortEffort | null;
}

/** The menu's lines in order: the pairs under their heading, then each tool's group (`default` first), a group's
 * loading line after its models; a short menu stops after the pairs with "Another model or effort…". */
export function modelEffortChoices(menu: ModelEffortMenu, of: ChoicesOf = {}): Choice[] {
  const out: Choice[] = [];
  if (menu.pairs_heading !== null && menu.pairs.length > 0) {
    out.push(line("heading", "h-pairs", menu.pairs_heading));
    for (const p of menu.pairs)
      out.push({
        ...line("option", p.id, p.label),
        detail: p.places === "" ? null : p.places,
        checked: p.selected,
        avatar: p.avatar,
      });
  }
  if (menu.another !== null && of.full !== true) {
    out.push(line("option", ANOTHER_ID, menu.another));
    return out;
  }
  for (const g of menu.groups) {
    out.push(line("heading", `h-${g.tool}`, g.heading));
    for (const item of g.items)
      out.push({
        ...line("option", `model:${item.model}`, item.label),
        detail: item.detail,
        mono: item.mono,
        detailMono: item.detail?.startsWith("now ") ?? false,
        checked: item.selected,
        avatar: g.avatar,
      });
    if (g.status !== null) out.push(line("note", `s-${g.tool}`, g.status));
  }
  const effort = of.effort ?? null;
  if (effort !== null) {
    out.push(line("heading", "h-effort", effort.label.toUpperCase()));
    for (const l of effort.levels)
      out.push({ ...line("option", `${EFFORT_PREFIX}${l.id}`, l.label), checked: l.selected });
  }
  return out;
}

/** The Effort level the line `id` picks (`null`: Default, the tool's own); `undefined` for another line. */
export function effortPick(id: string): string | null | undefined {
  if (!id.startsWith(EFFORT_PREFIX)) return undefined;
  const level = id.slice(EFFORT_PREFIX.length);
  return level === "" ? null : level;
}

/** What the line `id` picks; `undefined` for a heading or a note. */
export function modelEffortPick(menu: ModelEffortMenu, id: string): ModelEffortPick | undefined {
  const pair = menu.pairs.find((p) => p.id === id);
  if (pair !== undefined) return { kind: "pair", model: pair.model, effort: pair.effort };
  if (!id.startsWith("model:")) return undefined;
  const model = id.slice("model:".length);
  return menu.groups.some((g) => g.items.some((i) => i.model === model))
    ? { kind: "model", model }
    : undefined;
}

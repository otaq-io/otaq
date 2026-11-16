// The model menu's lines and what a chosen line picks (`ModelEffort.tsx`), apart from React so the tests read them.
// Everything the lines say is Rust's (`otaq-view`'s `model_effort.rs`).
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

/** The menu's lines in order: the pairs under their heading, then each tool's group (`default` first), a group's
 * loading line after its models. */
export function modelEffortChoices(menu: ModelEffortMenu): Choice[] {
  const out: Choice[] = [];
  if (menu.pairs_heading !== null && menu.pairs.length > 0) {
    out.push(line("heading", "h-pairs", menu.pairs_heading));
    for (const p of menu.pairs)
      out.push({
        ...line("option", p.id, p.label),
        detail: p.places,
        checked: p.selected,
        avatar: p.avatar,
      });
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
  return out;
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

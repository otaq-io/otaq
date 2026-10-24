// Priority (*01 · Components → Primitives → Priority*, *Status Legend → 5 · Priority*; TKT-09, d0065): ink only,
// never turquoise or red, never colour alone. Built from a `PriorityView` (`priority.rs`).
import type { PriorityView } from "../generated/PriorityView";
import { Icon } from "../theme/Icon";

/** The chip: a 12 px icon and the number in mono 10.5/600; P1 inverted, P2 outlined in full ink, P3 and P4 muted. */
export function PriorityChip({ priority }: { priority: PriorityView }) {
  return (
    <span className="priority-chip" data-ink={priority.ink} title={priority.label}>
      <Icon name={priority.icon} size={12} />
      <span>{priority.short}</span>
    </span>
  );
}

/** The chip and its word, as in the sheet and the legend: full ink for P1 and P2, muted for P3 and P4. */
export function PriorityLabel({ priority }: { priority: PriorityView }) {
  return (
    <span className="priority-label" data-ink={priority.ink}>
      <PriorityChip priority={priority} />
      <span className="priority-word">{priority.word}</span>
    </span>
  );
}

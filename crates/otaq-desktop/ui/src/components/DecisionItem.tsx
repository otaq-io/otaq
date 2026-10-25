// Decision Item and the Decisions list (DEC-04, DEC-05; d0084; *07 → Row · Decision Item (0084)*, *SHOP-23 review ·
// Decisions*): an agent's recorded decision with its question, Built, Other option and Why under a signpost icon in
// neutral colours; what the review did with it; and Change… as a quiet button while the review is open. Every text and rule comes from
// `otaq-view`'s `decision_item.rs`; the list keeps only whether it is folded.
import { useState } from "react";

import type { DecisionItemView } from "../generated/DecisionItemView";
import type { DecisionsSection } from "../generated/DecisionsSection";
import { Icon } from "../theme/Icon";
import { Button } from "./Button";

export interface DecisionItemProps {
  item: DecisionItemView;
  /** Change… on this item (shown only when the view offers it). */
  onChange?: (item: DecisionItemView) => void;
}

function Field({ label, text, quiet }: { label: string; text: string; quiet?: boolean }) {
  return (
    <div className="decision-field">
      <span className="decision-label">{label}</span>
      <span className="decision-value" data-quiet={quiet || undefined}>
        {text}
      </span>
    </div>
  );
}

export function DecisionItem({ item, onChange }: DecisionItemProps) {
  return (
    <div className="decision-item" data-id={item.id}>
      <div className="decision-head">
        <Icon name="signpost" size={14} />
        <span className="decision-question">{item.question}</span>
        {item.change && (
          <Button
            compact
            variant="secondary"
            label={item.labels.change}
            onClick={() => {
              onChange?.(item);
            }}
          />
        )}
      </div>
      <div className="decision-fields">
        <Field label={item.labels.built} text={item.built} />
        {item.other.map((other) => (
          <Field key={other} label={item.labels.other} text={other} quiet />
        ))}
        <Field label={item.labels.why} text={item.why} quiet />
      </div>
      {item.status && (
        <div className="decision-status" data-kept={item.status.kept || undefined}>
          <Icon name={item.status.icon} size={12} />
          <span>{item.status.text}</span>
        </div>
      )}
    </div>
  );
}

export interface DecisionsListProps {
  section: DecisionsSection;
  /** Change… on an item; T-0288 opens its dialog (Send back). */
  onChange?: (item: DecisionItemView) => void;
}

/** The Decisions section: the count, the hint, and the items. */
export function DecisionsList({ section, onChange }: DecisionsListProps) {
  const [open, setOpen] = useState(true);
  return (
    <div className="decisions">
      <button
        type="button"
        className="decisions-head"
        aria-expanded={open}
        onClick={() => {
          setOpen((o) => !o);
        }}
      >
        <Icon name={open ? "chevron-down" : "chevron-right"} size={12} />
        <Icon name="signpost" size={13} />
        <span className="decisions-title">{section.title}</span>
        <span className="decisions-count">{section.count}</span>
        <span className="decisions-spacer" />
        {section.hint !== null && <span className="decisions-hint">{section.hint}</span>}
      </button>
      {open &&
        section.groups.map((group) => (
          <div key={group.items[0]?.id ?? ""} className="decisions-group">
            {group.items.map((item) => (
              <DecisionItem key={item.id} item={item} onChange={onChange} />
            ))}
          </div>
        ))}
    </div>
  );
}

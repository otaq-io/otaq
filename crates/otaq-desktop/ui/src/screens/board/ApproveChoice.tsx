// The approve choice on a drop into Approved (*03 → Approve choice on drop*, UI-02): a drop never approves by
// itself; the popover asks how. 1 / 2 (or a click) picks, Esc or a press outside cancels and the card stays in
// Ready for Review. Local merge opens T-0226's confirmation, Open PR and Push to PR open T-0227's dialog; "Done,
// nothing to merge" (first, when the branch has no commits) finishes the ticket at once. Its words are the
// `ApproveMenu`'s (`approve.rs`).
import { useEffect, useRef } from "react";

import { type Anchor, Kbd, Popover } from "../../components";
import type { ApproveChoice as Choice } from "../../generated/ApproveChoice";
import type { ApproveMenu } from "../../generated/ApproveMenu";
import { Icon } from "../../theme/Icon";

export interface ApproveChoiceProps {
  menu: ApproveMenu;
  anchor: Anchor;
  onChoose: (choice: Choice) => void;
  onCancel: () => void;
}

export function ApproveChoice({ menu, anchor, onChoose, onCancel }: ApproveChoiceProps) {
  const first = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    first.current?.focus();
  }, []);
  return (
    <Popover label={menu.title} anchor={anchor} onClose={onCancel}>
      {/* The number keys pick; the popover's own Esc cancels (`useDismiss`). */}
      {/* eslint-disable-next-line jsx-a11y/no-static-element-interactions -- the keys belong to the buttons inside; this only reads their number shortcuts */}
      <div
        className="board-approve"
        onKeyDown={(e) => {
          const item = menu.items.find((i) => String(i.number) === e.key);
          if (!item?.enabled) return;
          e.preventDefault();
          e.stopPropagation();
          onChoose(item.choice);
        }}
      >
        <div className="board-approve-title">{menu.title}</div>
        {menu.items.map((item, i) => (
          <button
            key={item.choice}
            ref={i === 0 ? first : undefined}
            type="button"
            className="board-approve-item"
            disabled={!item.enabled}
            onClick={() => {
              onChoose(item.choice);
            }}
          >
            <span className="board-approve-icon">
              <Icon name={item.icon} size={14} />
            </span>
            <span className="board-approve-text">
              <span className="board-approve-name">{item.title}</span>
              <span className="board-approve-detail">{item.detail}</span>
            </span>
            <Kbd keys={String(item.number)} />
          </button>
        ))}
        <div className="board-approve-note">{menu.note}</div>
      </div>
    </Popover>
  );
}

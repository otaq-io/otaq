// The usage limit banner with one line per tool (USG-10, AGM-03, d0086, d0108): the tool's avatar (with several tools),
// its title and text, what is left and when it resets on the right, then Resume <tool> or Sign in…. Only that tool's
// agents wait; nothing ever moves to another tool. Amber, never turquoise (d0077). Every word and which buttons show
// come from Rust (`limit_banner.rs`); a press goes out as the line's `ToolProblemAction`.
import { Button, ToolAvatar } from "../../components";
import type { LimitLine } from "../../generated/LimitLine";
import type { ToolProblemAction } from "../../generated/ToolProblemAction";
import { Icon } from "../../theme/Icon";

/** What the lines' presses do: the app's own (`BannerRows`), or the gallery's recorder. */
export interface LimitLineHandlers {
  onAction: (action: ToolProblemAction) => void;
}

/** One tool's line. */
export function LimitLineRow({ line, onAction }: { line: LimitLine } & LimitLineHandlers) {
  return (
    <div className="banner notices-line" data-tone={line.tone} data-tool={line.tool} role="status">
      {line.avatar ? (
        <ToolAvatar avatar={line.avatar} />
      ) : (
        <span className="banner-icon">
          <Icon name={line.icon} size={15} />
        </span>
      )}
      <span className="banner-title">{line.title}</span>
      <span className="banner-text">{line.text}</span>
      {line.aside !== null && <span className="banner-aside">{line.aside}</span>}
      {line.buttons.map((button) => (
        <Button
          key={button.label}
          compact
          variant={button.primary ? "primary" : "secondary"}
          label={button.label}
          icon={button.icon}
          disabled={!button.enabled}
          onClick={() => {
            onAction(button.action);
          }}
        />
      ))}
    </div>
  );
}

/** The banner's lines, top first, in the order the tools hit their limits. */
export function LimitLines({ lines, onAction }: { lines: readonly LimitLine[] } & LimitLineHandlers) {
  return (
    <div className="notices-lines">
      {lines.map((line) => (
        <LimitLineRow key={line.tool + line.title} line={line} onAction={onAction} />
      ))}
    </div>
  );
}

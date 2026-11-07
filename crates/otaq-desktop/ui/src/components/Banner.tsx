// Banner (*01 · Components → Banners → Banner*): a full-width 36 px strip with an icon, a title in the banner's
// colour, the text, optional compact buttons and an optional close button. Which banners show, and their wording, come
// from Rust (`banners.rs`, T-0231). A banner without a title is its text alone (the folder trust note); one with a
// tooltip shows it on hovering the text (the reasons of several projects whose folder trust failed).
import type { IconName } from "../generated/IconName";
import { Icon } from "../theme/Icon";
import { Button } from "./Button";
import { Tooltip } from "./Tooltip";

/** Turquoise: the usage limit; amber: warnings; accent: calm news; red: broke and waits; grey: reconnecting. */
export type BannerTone = "attention" | "warning" | "info" | "error" | "neutral";

export interface BannerAction {
  label: string;
  icon?: IconName;
  disabled?: boolean;
  /** The banner's call to action, highlighted (Resume after the limit reset). */
  primary?: boolean;
  onClick: () => void;
}

export interface BannerProps {
  tone: BannerTone;
  icon: IconName;
  title: string;
  text: string;
  actions?: readonly BannerAction[];
  /** Shows the close button; without it the banner can't be dismissed. */
  onDismiss?: () => void;
  /** On hovering the text: a bold line, then one line each. */
  tooltip?: { title: string; lines: readonly string[] } | undefined;
  /** Muted words on the right, before the buttons. */
  aside?: string | undefined;
}

export function Banner({ tone, icon, title, text, actions = [], onDismiss, tooltip, aside }: BannerProps) {
  const body = <span className="banner-text">{text}</span>;
  return (
    <div className="banner" data-tone={tone} role={tone === "error" ? "alert" : "status"}>
      <span className="banner-icon">
        <Icon name={icon} size={15} />
      </span>
      {title !== "" && <span className="banner-title">{title}</span>}
      {tooltip === undefined ? (
        body
      ) : (
        <Tooltip text={tooltip.title} detail={tooltip.lines}>
          {body}
        </Tooltip>
      )}
      {aside !== undefined && <span className="banner-aside">{aside}</span>}
      {actions.map((a) => (
        <Button
          key={a.label}
          compact
          variant={a.primary ? "primary" : "secondary"}
          label={a.label}
          icon={a.icon}
          disabled={a.disabled}
          onClick={a.onClick}
        />
      ))}
      {onDismiss && (
        <button type="button" className="banner-close" aria-label="Dismiss" onClick={onDismiss}>
          <Icon name="x" />
        </button>
      )}
    </div>
  );
}

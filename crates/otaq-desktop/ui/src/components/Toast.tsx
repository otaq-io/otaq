// Toast (*01 · Components → Overlays & feedback*): a 340 px floating card with a coloured icon, a title, a line of
// text, an optional action link and a close button. Stacking and time-outs are the toast host's (`toasts.rs`,
// T-0231).
import type { IconName } from "../generated/IconName";
import { Icon } from "../theme/Icon";

export type ToastKind = "success" | "error";

/** The icon's colour when it isn't the kind's (green for success, red for errors). */
export type ToastIconTone = "review" | "investigate" | "working" | "secondary";

export interface ToastActionProps {
  label: string;
  onClick: () => void;
}

export interface ToastProps {
  kind: ToastKind;
  title: string;
  text?: string | null;
  /** Another icon than the kind's (circle-check, triangle-alert), optionally in another colour. */
  icon?: IconName;
  iconTone?: ToastIconTone;
  /** A link after the text ("Copy command"). */
  action?: ToastActionProps;
  /** Makes the title a button that opens what the toast is about (its ticket). */
  onOpen?: () => void;
  onClose?: () => void;
}

export function Toast({ kind, title, text, icon, iconTone, action, onOpen, onClose }: ToastProps) {
  const name = icon ?? (kind === "success" ? "circle-check" : "triangle-alert");
  return (
    <div className="toast" role={kind === "error" ? "alert" : "status"}>
      <span className="toast-icon" data-tone={iconTone ?? kind}>
        <Icon name={name} size={15} />
      </span>
      <div className="toast-body">
        <span className="toast-line">
          {onOpen ? (
            <button type="button" className="toast-title toast-open" onClick={onOpen}>
              {title}
            </button>
          ) : (
            <span className="toast-title">{title}</span>
          )}
        </span>
        {(text || action) && (
          <span className="toast-text">
            {text}
            {action && (
              <button type="button" className="toast-action" onClick={action.onClick}>
                {action.label}
              </button>
            )}
          </span>
        )}
      </div>
      {onClose && (
        <button type="button" className="toast-close" aria-label="Close" onClick={onClose}>
          <Icon name="x" size={13} />
        </button>
      )}
    </div>
  );
}

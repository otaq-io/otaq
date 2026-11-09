// A posted comment's images (tuq 004, 006): a row of small thumbnails under its text, the user's and agents' alike,
// in the thread, History and the Attachments column; a click opens the image full size over the sheet (the `image`
// overlay), closed by Esc, its ✕ or a click beside it. The page loads an image only by its ticket and numbers
// (`attachmentImage`); the Rust side finds and serves the file. Every word comes from `CommentThumbnail`.
import { type KeyboardEvent, useEffect, useRef, useState } from "react";

import { openOverlay } from "../../app/overlays";
import type { OverlayProps } from "../../app/registry";
import { Tooltip } from "../../components";
import { useEscapeLayer } from "../../components/escape";
import { useEnterExit } from "../../components/motion";
import type { CommentThumbnail } from "../../generated/CommentThumbnail";
import { attachmentImage } from "../../tauri";
import { Icon } from "../../theme/Icon";

/** Opens image `thumbnail` of comment `n` on `ticket` full size. */
export function openImage(ticket: string, n: number, thumbnail: CommentThumbnail): void {
  openOverlay("image", { ticket, n, thumbnail });
}

/**
 * The image itself, scaled into its box; its name in place of it when it can't load (gone, or a type the webview
 * can't draw). Only ever an `<img>`: an SVG's scripts never run.
 */
export function AttachmentImage({
  ticket,
  n,
  thumbnail,
  className,
  decorative = false,
}: {
  ticket: string;
  n: number;
  thumbnail: CommentThumbnail;
  className: string;
  /** Its name is already written beside it (the Attachments column): no alt text of its own. */
  decorative?: boolean;
}) {
  const [failed, setFailed] = useState(false);
  if (failed)
    return (
      <span className={className} data-failed>
        <Icon name="image" size={14} />
      </span>
    );
  return (
    <img
      className={className}
      src={attachmentImage(ticket, n, thumbnail.attachment)}
      alt={decorative ? "" : thumbnail.name}
      draggable={false}
      onError={() => {
        setFailed(true);
      }}
    />
  );
}

/** One thumbnail: a click opens it full size. */
export function Thumbnail({
  ticket,
  n,
  thumbnail,
}: {
  ticket: string;
  n: number;
  thumbnail: CommentThumbnail;
}) {
  return (
    <Tooltip text={thumbnail.name}>
      <button
        type="button"
        className="comment-thumbnail"
        aria-label={thumbnail.open}
        onClick={() => {
          openImage(ticket, n, thumbnail);
        }}
      >
        <AttachmentImage ticket={ticket} n={n} thumbnail={thumbnail} className="comment-thumbnail-image" />
      </button>
    </Tooltip>
  );
}

/** A comment's images as a row of thumbnails; nothing without any. */
export function Thumbnails({
  ticket,
  n,
  thumbnails,
}: {
  ticket: string;
  n: number;
  thumbnails: readonly CommentThumbnail[];
}) {
  if (thumbnails.length === 0) return null;
  return (
    <div className="comment-thumbnails">
      {thumbnails.map((t) => (
        <Thumbnail key={t.attachment} ticket={ticket} n={n} thumbnail={t} />
      ))}
    </div>
  );
}

/** The image full size over the sheet: its name and size, ✕; Esc or a click beside it closes it. */
export function ImageViewer({ params, close }: OverlayProps<"image">) {
  const { ticket, n, thumbnail } = params;
  const backdrop = useRef<HTMLDivElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const view = useRef<HTMLDivElement>(null);
  useEnterExit(backdrop, "backdrop", { selector: ".image-view", preset: "dialog" });
  // The topmost layer while it shows: Esc closes it wherever the focus went.
  useEscapeLayer(close, view, true);
  useEffect(() => {
    closeButton.current?.focus();
  }, [params]);
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    close();
  };
  return (
    <div
      ref={backdrop}
      className="dialog-backdrop"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      {/* eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- the view's own key: Esc */}
      <div
        ref={view}
        className="image-view"
        role="dialog"
        aria-modal="true"
        aria-label={thumbnail.name}
        tabIndex={-1}
        onKeyDown={onKeyDown}
      >
        <div className="report-view-head">
          <Icon name="image" size={15} />
          <span className="report-view-title">{thumbnail.name}</span>
          <span className="report-view-meta">{`· ${thumbnail.size}`}</span>
          <span className="report-view-spacer" />
          <Tooltip text={thumbnail.close}>
            <button
              ref={closeButton}
              type="button"
              className="report-view-button"
              aria-label={thumbnail.close}
              onClick={close}
            >
              <Icon name="x" size={14} />
            </button>
          </Tooltip>
        </div>
        <div className="image-view-body">
          <AttachmentImage ticket={ticket} n={n} thumbnail={thumbnail} className="image-view-image" />
        </div>
      </div>
    </div>
  );
}

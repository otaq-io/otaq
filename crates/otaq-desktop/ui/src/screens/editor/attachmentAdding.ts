// Adding files to a comment box (tuq 002, 003, 005), shared by the box (paste, drop, Attach) and Overview (Add, a
// drop, a paste outside a text field): a path (picked or dropped) becomes its chip through `overview.file`; a pasted
// file (a screenshot, files copied in Finder), which has no path, is checked against the limit first, never read when
// it is over, then its bytes are kept in the data folder (`stageAttachment`) and it is sent by that path. A file over
// 20 MB is refused by name with core's words, and the rest are added. An image's thumbnail is drawn from its bytes.
import type { AttachmentSizeParams } from "../../generated/AttachmentSizeParams";
import type { OverviewFileChip } from "../../generated/OverviewFileChip";
import { asRequestError } from "../../service";
import { boxImage, request, stageAttachment } from "../../tauri";
import { type BoxAttachment, fileAttachment } from "./commentRules";

/** What adding gave: the new attachments, in order, and why the others couldn't be added. */
export interface Added {
  added: BoxAttachment[];
  refused: string[];
}

/** Files by their paths (Attach, Overview's Add, a drop): each its chip, or why it can't be attached. */
export async function addPaths(paths: readonly string[]): Promise<Added> {
  const results = await Promise.all(
    paths.map((path) =>
      (request("overview.file", { path }) as Promise<OverviewFileChip>).then(
        (chip) => ({ ok: fileAttachment(chip) }) as const,
        (e: unknown) => ({ refused: asRequestError(e).message }) as const,
      ),
    ),
  );
  return collect(results);
}

/** What a pasted file is called: its own name, or "Pasted image.png" for a screenshot that has none. */
export function pastedName(file: File): string {
  if (file.name !== "") return file.name;
  const ext = /^image\/([a-z0-9.+-]+)$/
    .exec(file.type)?.[1]
    ?.replace("jpeg", "jpg")
    .replace("svg+xml", "svg");
  return ext ? `Pasted image.${ext}` : "Pasted file";
}

/**
 * Pasted files (a screenshot, files copied in Finder): each checked by its size first (`attachments.refusal`, the
 * limit and its words are core's), so one over the limit is refused by name and never read.
 */
export async function addFiles(files: readonly File[]): Promise<Added> {
  const results = await Promise.all(
    files.map(async (file) => {
      const name = pastedName(file);
      try {
        const params: AttachmentSizeParams = { name, bytes: file.size };
        const why = (await request("attachments.refusal", params)) as string | null;
        if (why !== null) return { refused: why } as const;
        return { ok: fileAttachment(await stageAttachment(file, name)) } as const;
      } catch (e: unknown) {
        return { refused: asRequestError(e).message } as const;
      }
    }),
  );
  return collect(results);
}

function collect(results: readonly ({ ok: BoxAttachment } | { refused: string })[]): Added {
  const added: BoxAttachment[] = [];
  const refused: string[] = [];
  for (const r of results) {
    if ("ok" in r) added.push(r.ok);
    else refused.push(r.refused);
  }
  return { added, refused };
}

/** The files on the clipboard of a paste, if any (Finder's copied files, a screenshot). */
export function clipboardFiles(data: DataTransfer | null): File[] {
  return Array.from(data?.files ?? []);
}

/** The thumbnail's longest side, in device pixels (the chip shows it at 32 CSS pixels). */
const THUMB_PIXELS = 96;

function dataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      resolve(typeof reader.result === "string" ? reader.result : "");
    };
    reader.onerror = () => {
      reject(reader.error ?? new Error("can't read the image"));
    };
    reader.readAsDataURL(blob);
  });
}

/** A small `data:` image of `blob`; the whole image when it can't be scaled down here (an SVG, a test page). */
export async function thumbnail(blob: Blob): Promise<string> {
  try {
    if (typeof createImageBitmap === "function") {
      const bitmap = await createImageBitmap(blob);
      const scale = Math.min(1, THUMB_PIXELS / Math.max(bitmap.width, bitmap.height, 1));
      const canvas = document.createElement("canvas");
      canvas.width = Math.max(1, Math.round(bitmap.width * scale));
      canvas.height = Math.max(1, Math.round(bitmap.height * scale));
      const context = canvas.getContext("2d");
      if (context) {
        context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
        bitmap.close();
        return canvas.toDataURL("image/png");
      }
      bitmap.close();
    }
  } catch {
    // Not decodable here: the whole image below.
  }
  return dataUrl(blob);
}

/** An image's type by its extension, so its `data:` URL draws ("image/png"; core's `IMAGE_EXTENSIONS`). */
export function imageType(path: string): string {
  const ext = /\.([^./\\]+)$/.exec(path)?.[1]?.toLowerCase() ?? "";
  const types: Record<string, string> = {
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    svg: "image/svg+xml",
    ico: "image/x-icon",
    tif: "image/tiff",
  };
  return types[ext] ?? `image/${ext}`;
}

/** The thumbnail of the image at `path` in a box (picked, dropped, or kept from a paste). */
export async function thumbnailOf(path: string): Promise<string> {
  return thumbnail(new Blob([await boxImage(path)], { type: imageType(path) }));
}

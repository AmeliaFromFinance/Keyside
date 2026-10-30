// Key images: files dropped on the canvas go through a square crop, then become assets.

import { req } from "../lib/deck.svelte.ts";
import { t } from "../lib/i18n.svelte.ts";
import { ed, page, change, newId, fits, notify, pick, reloadCatalog } from "./state.svelte.ts";

export const IMAGE_TYPES = "image/png,image/jpeg,image/webp,image/gif,image/svg+xml";
/** Bigger files are refused before cropping, so a huge photo can't stall the page. */
const MAX_SOURCE = 25 * 1024 * 1024;
const MAX_ASSET = 512 * 1024;

/** An image waiting in the crop step, and the key or empty cell it is for. */
export const cropping = $state({ file: null as File | null, page: "", key: null as string | null, cell: [0, 0] as [number, number] });

export function isImage(f: File) {
  return IMAGE_TYPES.split(",").includes(f.type) || /\.(png|jpe?g|webp|gif|svg)$/i.test(f.name);
}

/** First image among dropped files, or a notice when there is none. */
export function droppedImage(files: FileList | undefined | null): File | null {
  const f = [...(files ?? [])].find(isImage);
  if (!f && files?.length) notify(t("Drop a PNG, JPEG, WebP, GIF or SVG image"));
  if (f && f.size > MAX_SOURCE) {
    notify(t("That image is too large"));
    return null;
  }
  return f ?? null;
}

export function startCrop(file: File, key: string | null, cell: [number, number]) {
  cropping.file = file;
  cropping.page = ed.page;
  cropping.key = key;
  cropping.cell = cell;
}

function base64(buf: Uint8Array): string {
  let bin = "";
  for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
  return btoa(bin);
}

/** Uploads an image and returns the icon value for keys, "asset:<id>". */
export async function uploadImage(blob: Blob, name: string): Promise<string> {
  if (blob.size > MAX_ASSET) throw new Error(t("Images can be up to 512 KB."));
  const meta = await req<{ id: string }>("assets.put", { name, data: base64(new Uint8Array(await blob.arrayBuffer())) });
  await reloadCatalog().catch(() => {});
  return `asset:${meta.id}`;
}

/** A label for a new key from the file name: "my-logo.png" becomes "my logo". */
export function labelFrom(name: string): string {
  return (
    name
      .replace(/\.[^.]+$/, "")
      .replace(/[_-]+/g, " ")
      .trim()
      .slice(0, 64) || t("Image")
  );
}

/** Puts the uploaded image on the key it was dropped on, or on a new key in the empty cell. */
export function applyCrop(icon: string) {
  const p = page();
  const name = cropping.file?.name ?? "";
  const { key, cell } = cropping;
  cropping.file = null;
  if (!p || p.id !== cropping.page) return;
  const k = key ? p.keys.find((q) => q.id === key) : undefined;
  if (k) {
    change(() => (k.icon = icon));
    pick(k.id);
    return;
  }
  if (!fits(p, cell[0], cell[1], 1, 1)) return notify(t("It doesn't fit there"));
  const id = newId("k", p.keys.map((q) => q.id));
  change(() => p.keys.push({ id, x: cell[0], y: cell[1], kind: "button", label: labelFrom(name), icon, actions: [] }));
  pick(id);
}

/**
 * Draws the visible square of an image into a new square image. `zoom` 1 fills the square
 * with the image; `x` and `y` are the offset of the image centre, in output pixels.
 */
export async function renderCrop(img: HTMLImageElement, size: number, zoom: number, x: number, y: number): Promise<Blob> {
  const c = document.createElement("canvas");
  c.width = c.height = size;
  const g = c.getContext("2d")!;
  const w = img.naturalWidth || size;
  const h = img.naturalHeight || size;
  const scale = (size / Math.min(w, h)) * zoom;
  g.imageSmoothingQuality = "high";
  g.drawImage(img, size / 2 + x - (w * scale) / 2, size / 2 + y - (h * scale) / 2, w * scale, h * scale);
  const out = (type: string, q?: number) => new Promise<Blob | null>((r) => c.toBlob(r, type, q));
  // PNG keeps transparency. Photos that come out too large fall back to WebP.
  let blob = await out("image/png");
  if (!blob || blob.size > MAX_ASSET) blob = (await out("image/webp", 0.9)) ?? blob;
  if (!blob) throw new Error(t("Could not read that image"));
  return blob;
}

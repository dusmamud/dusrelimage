/**
 * dusrelimage — offline image toolkit (TypeScript wrapper over the Rust core).
 *
 * Like dusrelpdf, the binary work lives in the Rust core and this
 * package wraps it via WASM (`crates/dusrelimage-wasm`,
 * `--target nodejs`). Every operation takes bytes in and returns
 * bytes out, synchronously — the WASM calls are synchronous, so there
 * is nothing to `await` (awaiting a plain value still works, it just
 * does nothing).
 *
 * WASM contract (see crates/dusrelimage-wasm/src/lib.rs):
 * - Binary results are `Uint8Array`s.
 * - On failure a wasm function THROWS a JSON string:
 *   `{"ok":false,"code":"INVALID_IMAGE","message_en":"…",
 *   "message_hi":"…"}` (both `message_*` and `reason_*` keys are
 *   emitted; this wrapper accepts either). Every failure surfaces
 *   here as a `DusrelError` with the same `code` / `messageEn` /
 *   `messageHi` — never a raw TypeError, never a raw JSON string.
 * - `get_image_info`, `pick_colour` and `extract_palette` return
 *   JSON strings; this wrapper parses them into camelCase objects.
 *   Palette entries arrive FLATTENED (`{ hex, r, g, b, a, hsl,
 *   share }`) — the wasm layer flattens `PaletteColour` for the
 *   playground, which reads `c.hex` / `c.share` at the top level.
 * - `bulk_process` returns a JSON string of `BulkResult`s whose
 *   outputs are base64; this wrapper decodes them to `Uint8Array`s.
 * - Options are plain JS objects matching the Rust `*Options`
 *   structs, except this wrapper ALWAYS sends complete objects:
 *   every field is filled with the Rust default, because serde
 *   rejects partial objects for non-`Option` fields. (The wasm
 *   layer ALSO deep-merges partial objects over the defaults, so
 *   the playground's `{ quality: 70 }` works — but this wrapper
 *   does not rely on that.)
 *
 * Naming: the TS API uses camelCase option keys (`fontSize`,
 * `blockSize`, `targetBytes`, …) and the wrapper translates them to
 * the Rust snake_case JSON keys (`font_size`, `block_size`,
 * `target_bytes`, …) before calling wasm. Colours in TS are hex
 * strings (`"#ff0000"`) or full `Colour` objects; the wrapper
 * expands hex strings to full `Colour` objects (with HSL) before
 * sending them. Layouts are `"2x2"` strings / `"row"` / `"column"`
 * / `{ rows, cols }`; annotations use the `{ type: "Arrow", … }`
 * form, translated to serde's tagged form for wasm.
 *
 * Input rule (dusrelpdf lesson, applied to bytes): functions accept
 * `Uint8Array` only. ANY non-Uint8Array input (string, number,
 * null, plain object) throws a `DusrelError` with code
 * `INVALID_TYPE` — never a raw TypeError. (A Node `Buffer` IS a
 * `Uint8Array` subclass, so Buffers are accepted as-is.)
 *
 * Honest limits (same as the Rust core, v0.1.0): AVIF input does NOT
 * decode (`UNSUPPORTED_FORMAT`; AVIF output works, lossy 8-bit).
 * WebP output is always lossless — quality/effort are ignored for
 * WebP. `keep_exif` / `dpi` / `progressive` are accepted but not
 * honoured. `compress_to_size` quality search is JPEG/AVIF-only
 * (other formats take one best-effort encode). TIFF input decodes
 * but falls back to PNG output. GIF output is the first frame only.
 */

// The wasm-pack `--target nodejs` output. Built by `npm run build`
// (wasm-pack first, then tsc); gitignored — see packages/js/.gitignore.
import * as wasm from "../wasm-pkg/dusrelimage_wasm";

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/** Stable error codes — verbatim from crates/dusrelimage/src/error.rs. */
export type DusrelErrorCode =
  | "INVALID_IMAGE"
  | "UNSUPPORTED_FORMAT"
  | "IMAGE_TOO_LARGE"
  | "TARGET_SIZE_UNREACHABLE"
  | "CROP_OUT_OF_BOUNDS"
  | "PIXEL_OUT_OF_BOUNDS"
  | "INVALID_QUALITY"
  | "INVALID_DIMENSIONS"
  | "INVALID_ARGUMENT"
  | "EMPTY_INPUT"
  | "INVALID_TYPE"
  | "OPERATION_FAILED";

/**
 * Every failure from this package. `code` is stable and
 * machine-readable; `messageEn` / `messageHi` are the human-readable
 * messages from the Rust core (Roman-script Hinglish, like indycheck).
 */
export class DusrelError extends Error {
  readonly code: DusrelErrorCode;
  readonly messageEn: string;
  readonly messageHi: string;
  /** Alias of `messageEn` (the wasm JSON also carries `reason_*`). */
  readonly reasonEn: string;
  /** Alias of `messageHi`. */
  readonly reasonHi: string;

  constructor(code: DusrelErrorCode, messageEn: string, messageHi: string) {
    super(`${code}: ${messageEn}`);
    this.name = "DusrelError";
    this.code = code;
    this.messageEn = messageEn;
    this.messageHi = messageHi;
    this.reasonEn = messageEn;
    this.reasonHi = messageHi;
  }
}

function invalidType(what: string): DusrelError {
  return new DusrelError(
    "INVALID_TYPE",
    `${what} must be a Uint8Array.`,
    `${what} Uint8Array hona chahiye.`,
  );
}

function invalidArgument(messageEn: string, messageHi: string): DusrelError {
  return new DusrelError("INVALID_ARGUMENT", messageEn, messageHi);
}

/** Turn whatever wasm threw into a DusrelError (pass-through if it
 *  already is one, e.g. from the client-side checks below). */
function toDusrelError(e: unknown): DusrelError {
  if (e instanceof DusrelError) return e;
  if (typeof e === "string") {
    try {
      const parsed = JSON.parse(e) as {
        code?: unknown;
        message_en?: unknown;
        message_hi?: unknown;
        reason_en?: unknown;
        reason_hi?: unknown;
      };
      if (parsed && typeof parsed.code === "string") {
        const en =
          typeof parsed.message_en === "string"
            ? parsed.message_en
            : typeof parsed.reason_en === "string"
              ? parsed.reason_en
              : "Operation failed.";
        const hi =
          typeof parsed.message_hi === "string"
            ? parsed.message_hi
            : typeof parsed.reason_hi === "string"
              ? parsed.reason_hi
              : "Operation fail ho gaya.";
        return new DusrelError(parsed.code as DusrelErrorCode, en, hi);
      }
    } catch {
      // Not JSON — fall through to the plain-string case below.
    }
    return new DusrelError("OPERATION_FAILED", e, e);
  }
  if (e instanceof Error) {
    return new DusrelError("OPERATION_FAILED", e.message, e.message);
  }
  return new DusrelError(
    "OPERATION_FAILED",
    "Unknown error.",
    "Unknown error hai.",
  );
}

/** Run a wasm call, converting any throw into a DusrelError. */
function call<T>(fn: () => T): T {
  try {
    return fn();
  } catch (e) {
    throw toDusrelError(e);
  }
}

// ---------------------------------------------------------------------------
// Value types
// ---------------------------------------------------------------------------

/** A colour: 8-bit RGBA + hex string + HSL (h degrees 0..360, s/l 0..1). */
export interface Colour {
  hex: string;
  r: number;
  g: number;
  b: number;
  a: number;
  hsl: [number, number, number];
}

/** Where a colour is accepted, a `"#RRGGBB"` (or `"#RGB"` /
 *  `"#RRGGBBAA"`) hex string works too — the wrapper expands it. */
export type ColourInput = string | Colour;

/** Summary of an image, from {@link getImageInfo}. */
export interface ImageInfo {
  /** Lowercase short name: "jpeg" | "png" | "webp" | "avif" | "gif" | "bmp" | "tiff". */
  format: string;
  width: number;
  height: number;
  hasAlpha: boolean;
  /** Lowercase `image`-crate colour type, e.g. "rgb8", "rgba8". */
  colourType: string;
  /** Input byte length. */
  fileSize: number;
}

/** One dominant colour and its share of the sampled pixels (0..1). */
export interface PaletteColour extends Colour {
  share: number;
}

/** One item of a {@link bulkProcess} result, in input order. */
export interface BulkResult {
  /** Input index (0-based). */
  index: number;
  /** True when this item succeeded. */
  ok: boolean;
  /** Output bytes on success (image ops); absent on failure. */
  output?: Uint8Array;
  /** Picked colour on success (op `"pick_colour"` only). */
  colour?: Colour;
  /** Dominant colours on success (op `"palette"` only). */
  palette?: PaletteColour[];
  /** Error on failure; absent on success. */
  error?: { code: string; messageEn: string; messageHi: string };
}

/** Output image format (serde variant names — case matters). */
export type ImageFormat = "Jpeg" | "Png" | "WebP" | "Avif" | "Gif" | "Bmp";

/** How a resize fits the image into the target box. */
export type FitMode = "Fit" | "Fill" | "Stretch" | "Crop";

/** Resampling filter. */
export type Filter =
  | "Nearest"
  | "Triangle"
  | "CatmullRom"
  | "Gaussian"
  | "Lanczos3";

/** Anchor position: 9 anchors + `Tiled` (watermark only). */
export type Position =
  | "TopLeft"
  | "TopCenter"
  | "TopRight"
  | "CenterLeft"
  | "Center"
  | "CenterRight"
  | "BottomLeft"
  | "BottomCenter"
  | "BottomRight"
  | "Tiled";

/** Collage layout: a `"WxH"` grid string, a named layout, or `{ rows, cols }`. */
export type CollageLayout =
  | `${number}x${number}`
  | "grid"
  | "row"
  | "column"
  | { rows: number; cols: number };

/** One annotation shape (all coordinates are absolute pixels). */
export type Annotation =
  | {
      type: "Arrow";
      from: [number, number];
      to: [number, number];
      colour: ColourInput;
      width: number;
    }
  | {
      type: "Rect";
      x: number;
      y: number;
      w: number;
      h: number;
      colour: ColourInput;
      width: number;
      fill?: ColourInput | null;
    }
  | {
      type: "Ellipse";
      x: number;
      y: number;
      w: number;
      h: number;
      colour: ColourInput;
      width: number;
      fill?: ColourInput | null;
    }
  | {
      type: "Line";
      from: [number, number];
      to: [number, number];
      colour: ColourInput;
      width: number;
    }
  | {
      type: "Text";
      x: number;
      y: number;
      text: string;
      size: number;
      colour: ColourInput;
    };

// ---------------------------------------------------------------------------
// Options — camelCase TS keys, translated to Rust snake_case (see README
// for the full mapping table). Every field is optional: omitted fields
// use the Rust defaults listed in the doc comments below.
// ---------------------------------------------------------------------------

/** Options shared by every operation. */
export interface CommonOptions {
  /** Apply EXIF orientation on load (default true). */
  autoOrient?: boolean;
  /** Keep EXIF / ICC / XMP on output (default false). v0.1.0: accepted but not honoured — output is always stripped. */
  keepExif?: boolean;
  /** DPI to write on JPEG/PNG output (default unset). v0.1.0: accepted but not embedded. */
  dpi?: [number, number] | null;
  /** Refuse images with more pixels than this (default 120,000,000). */
  maxPixels?: number;
  /** Refuse images with a side longer than this (default 16000). */
  maxDimension?: number;
}

/** Options for {@link compressImage} / {@link compressToSize}. */
export interface CompressOptions extends CommonOptions {
  /** Quality 1-100 (default 80). */
  quality?: number;
  /** Output format; omitted = keep the input format (TIFF falls back to PNG). */
  format?: ImageFormat;
  /** Progressive JPEG / interlaced PNG (default false). v0.1.0: accepted but not honoured. */
  progressive?: boolean;
  /** Lossless output (WebP/PNG only; default false). */
  lossless?: boolean;
  /** Encoding effort 0-6 (default 4). */
  effort?: number;
}

/** Options for {@link resizeImage}. */
export interface ResizeOptions extends CommonOptions {
  /** Target width in px. With `height` unset, height follows the aspect ratio. */
  width?: number;
  /** Target height in px. With `width` unset, width follows the aspect ratio. */
  height?: number;
  /** Scale in percent (`100` = original size). Cannot be combined with `width`/`height`. */
  percent?: number;
  /** How the image fits the target box (default "Fit"). */
  fit?: FitMode;
  /** Resampling filter (default "Lanczos3"). */
  filter?: Filter;
  /** Canvas colour used by `Fill` (default white). */
  background?: ColourInput;
}

/** Options for {@link cropImage}. */
export interface CropOptions extends CommonOptions {
  /** Left edge in px (used when `anchor` is unset; default 0). */
  x?: number;
  /** Top edge in px (used when `anchor` is unset; default 0). */
  y?: number;
  /** Crop width in px (must be > 0). */
  width: number;
  /** Crop height in px (must be > 0). */
  height: number;
  /** Anchor preset: when set, the `width` x `height` rectangle is placed at the anchor and `x`/`y` are ignored. */
  anchor?: Position;
}

/** Options for {@link convertImage}. */
export interface ConvertOptions extends CommonOptions {
  /** Target format (required). */
  format: ImageFormat;
  /** Quality 1-100 (default 80). */
  quality?: number;
  /** Background used to flatten alpha when the target has none (default white). */
  background?: ColourInput;
  /** Progressive JPEG / interlaced PNG (default false). v0.1.0: accepted but not honoured. */
  progressive?: boolean;
}

/** A text watermark: string, size in px, colour. */
export interface TextWatermark {
  text: string;
  /** Font size in px (default 48). */
  size?: number;
  /** Colour (default white). */
  colour?: ColourInput;
}

/** A logo watermark: raw image bytes + a scale factor (1.0 = natural size). */
export interface ImageWatermark {
  image: Uint8Array;
  /** Scale factor (default 1.0). */
  scale?: number;
}

/** Options for {@link addWatermark}. */
export interface WatermarkOptions extends CommonOptions {
  /** Text watermark, if any. */
  text?: TextWatermark;
  /** Logo watermark (image bytes + scale), if any. */
  logo?: ImageWatermark;
  /** Opacity 0-1 (default 0.5). */
  opacity?: number;
  /** Anchor position, incl. "Tiled" for a repeated pattern (default "BottomRight"). */
  position?: Position;
  /** Rotation in degrees (default 0). */
  rotationDeg?: number;
  /** Margin from the image edge in px (default 16). */
  margin?: number;
}

/** Options for {@link makeCollage}. */
export interface CollageOptions extends CommonOptions {
  /** Layout (default 2x2 grid). */
  layout?: CollageLayout;
  /** Gap between cells in px (default 8). */
  gap?: number;
  /** Background colour (default white). */
  background?: ColourInput;
  /** Output width in px (default 1200). */
  width?: number;
  /** Output height in px (default 1200). */
  height?: number;
  /** Per-cell fit mode (default "Fit"). */
  cellFit?: FitMode;
}

/** Options for {@link pixelateImage}. */
export interface PixelateOptions extends CommonOptions {
  /** Left edge in px (default 0). */
  x?: number;
  /** Top edge in px (default 0). */
  y?: number;
  /** Rectangle width in px (0 = to the image edge; default 0 = whole image). */
  width?: number;
  /** Rectangle height in px (0 = to the image edge; default 0 = whole image). */
  height?: number;
  /** Block size in px, 2-100 (default 10). */
  blockSize?: number;
}

/** Extra options for {@link bulkProcess} with op `"compress_to_size"`. */
export interface BulkCompressToSizeOptions extends CompressOptions {
  /** Target size in bytes (required for `"compress_to_size"`). */
  targetBytes: number;
}

/** Extra options for {@link bulkProcess} with op `"annotate"`. */
export interface BulkAnnotateOptions extends CommonOptions {
  /** Annotations to draw on every input. */
  annotations: Annotation[];
}

/** Extra options for {@link bulkProcess} with op `"pick_colour"`. */
export interface BulkPickColourOptions extends CommonOptions {
  /** Pixel x (default 0). */
  x?: number;
  /** Pixel y (default 0). */
  y?: number;
}

/** Extra options for {@link bulkProcess} with op `"palette"`. */
export interface BulkPaletteOptions extends CommonOptions {
  /** Colours to extract, 1-16 (default 5). */
  count?: number;
}

/** Operation names {@link bulkProcess} accepts. */
export type BulkOp =
  | "compress"
  | "compress_image"
  | "compress_to_size"
  | "resize"
  | "resize_image"
  | "crop"
  | "crop_image"
  | "convert"
  | "convert_image"
  | "watermark"
  | "add_watermark"
  | "annotate"
  | "annotate_image"
  | "pick_colour"
  | "palette"
  | "collage"
  | "make_collage"
  | "pixelate"
  | "pixelate_image";

/** Options accepted by {@link bulkProcess} for a given op. */
export type BulkOptions =
  | CompressOptions
  | BulkCompressToSizeOptions
  | ResizeOptions
  | CropOptions
  | ConvertOptions
  | WatermarkOptions
  | BulkAnnotateOptions
  | BulkPickColourOptions
  | BulkPaletteOptions
  | CollageOptions
  | PixelateOptions;

// ---------------------------------------------------------------------------
// Client-side validation (cheap checks; the core does the real ones)
// ---------------------------------------------------------------------------

/** Assert a value is a Uint8Array (Buffers pass: Buffer extends
 *  Uint8Array). Empty bytes are EMPTY_INPUT, like the Rust core. */
function assertImageBytes(value: unknown, what = "Image bytes"): Uint8Array {
  if (!(value instanceof Uint8Array)) throw invalidType(what);
  if (value.byteLength === 0) {
    throw new DusrelError(
      "EMPTY_INPUT",
      `${what} is empty.`,
      `${what} khaali hai.`,
    );
  }
  return value;
}

/** Assert an array of Uint8Arrays (collage / bulk inputs). */
function assertImageArray(
  value: unknown,
  what: string,
  emptyWhat: string,
): Uint8Array[] {
  if (!Array.isArray(value)) throw invalidType(what);
  if (value.length === 0) {
    throw new DusrelError(
      "EMPTY_INPUT",
      `${emptyWhat} is empty.`,
      `${emptyWhat} khaali hai.`,
    );
  }
  for (const item of value) {
    if (!(item instanceof Uint8Array)) throw invalidType(what);
  }
  return value as Uint8Array[];
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function assertNonNegativeInt(value: unknown, what: string): number {
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value < 0
  ) {
    throw invalidArgument(
      `${what} must be a non-negative integer.`,
      `${what} non-negative integer hona chahiye.`,
    );
  }
  return value;
}

// ---------------------------------------------------------------------------
// Colour helpers — hex string <-> full Colour object (mirrors the Rust
// Colour::from_hex / rgb_to_hsl so a hex string round-trips exactly).
// ---------------------------------------------------------------------------

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h: number;
  if (max === rn) h = 60 * (((gn - bn) / d) % 6);
  else if (max === gn) h = 60 * ((bn - rn) / d + 2);
  else h = 60 * ((rn - gn) / d + 4);
  return [((h % 360) + 360) % 360, s, l];
}

/** Expand a hex string (`"#RGB"`, `"#RRGGBB"`, `"#RRGGBBAA"`, with
 *  or without `#`) into a full `Colour` object. */
export function colourFromHex(hex: string): Colour {
  if (typeof hex !== "string") {
    throw invalidArgument(
      `Invalid hex colour "${String(hex)}". Use "#RRGGBB".`,
      `Galat hex colour "${String(hex)}" hai. "#RRGGBB" use karein.`,
    );
  }
  const h = hex.startsWith("#") ? hex.slice(1) : hex;
  const bad = (): DusrelError =>
    invalidArgument(
      `Invalid hex colour "${hex}". Use "#RRGGBB".`,
      `Galat hex colour "${hex}" hai. "#RRGGBB" use karein.`,
    );
  const nibble = (ch: string): number => {
    const v = parseInt(ch, 16);
    if (Number.isNaN(v)) throw bad();
    return v;
  };
  const pair = (i: number): number => {
    const v = parseInt(h.slice(i, i + 2), 16);
    if (Number.isNaN(v)) throw bad();
    return v;
  };
  let r: number;
  let g: number;
  let b: number;
  let a = 255;
  if (h.length === 3) {
    r = nibble(h[0]) * 17;
    g = nibble(h[1]) * 17;
    b = nibble(h[2]) * 17;
  } else if (h.length === 6) {
    r = pair(0);
    g = pair(2);
    b = pair(4);
  } else if (h.length === 8) {
    r = pair(0);
    g = pair(2);
    b = pair(4);
    a = pair(6);
  } else {
    throw bad();
  }
  const hexOut = `#${r.toString(16).padStart(2, "0")}${g.toString(16).padStart(2, "0")}${b.toString(16).padStart(2, "0")}`.toUpperCase();
  return { hex: hexOut, r, g, b, a, hsl: rgbToHsl(r, g, b) };
}

/** Normalise a `ColourInput` to a full `Colour` object. A `Colour`
 *  object passes through (missing `hsl` is recomputed from rgb). */
export function toColour(value: ColourInput): Colour {
  if (typeof value === "string") return colourFromHex(value);
  if (
    typeof value === "object" &&
    value !== null &&
    isFiniteNumber(value.r) &&
    isFiniteNumber(value.g) &&
    isFiniteNumber(value.b)
  ) {
    const r = value.r;
    const g = value.g;
    const b = value.b;
    const a = isFiniteNumber(value.a) ? value.a : 255;
    const hex =
      typeof value.hex === "string" && value.hex.length > 0
        ? value.hex
        : `#${r.toString(16).padStart(2, "0")}${g.toString(16).padStart(2, "0")}${b.toString(16).padStart(2, "0")}`.toUpperCase();
    const hsl =
      Array.isArray(value.hsl) &&
      value.hsl.length === 3 &&
      value.hsl.every(isFiniteNumber)
        ? (value.hsl as [number, number, number])
        : rgbToHsl(r, g, b);
    return { hex, r, g, b, a, hsl };
  }
  throw invalidArgument(
    'Colour must be a "#RRGGBB" hex string or a { hex, r, g, b, a, hsl } object.',
    'Colour "#RRGGBB" hex string ya { hex, r, g, b, a, hsl } object hona chahiye.',
  );
}

// ---------------------------------------------------------------------------
// Options translation — camelCase TS -> complete snake_case Rust objects.
// Every translator fills ALL Rust-required fields with the Rust
// defaults, because serde rejects partial objects (see header).
// Undefined optional fields are omitted from the object sent to wasm.
// ---------------------------------------------------------------------------

function toCommon(options?: CommonOptions): Record<string, unknown> {
  return {
    auto_orient: options?.autoOrient ?? true,
    keep_exif: options?.keepExif ?? false,
    dpi:
      options?.dpi === undefined || options.dpi === null
        ? null
        : [options.dpi[0], options.dpi[1]],
    max_pixels: options?.maxPixels ?? 120_000_000,
    max_dimension: options?.maxDimension ?? 16000,
  };
}

function toCompressOptions(
  options?: CompressOptions,
): Record<string, unknown> {
  const quality = options?.quality ?? 80;
  if (!Number.isInteger(quality) || quality < 1 || quality > 100) {
    throw new DusrelError(
      "INVALID_QUALITY",
      `Quality ${quality} is invalid. Quality must be between 1 and 100.`,
      `Quality ${quality} galat hai. Quality 1 se 100 ke beech honi chahiye.`,
    );
  }
  const effort = options?.effort ?? 4;
  if (!Number.isInteger(effort) || effort < 0 || effort > 6) {
    throw invalidArgument(
      `Effort ${effort} is invalid. Effort must be between 0 and 6.`,
      `Effort ${effort} galat hai. Effort 0 se 6 ke beech hona chahiye.`,
    );
  }
  return {
    common: toCommon(options),
    quality,
    format: options?.format ?? null,
    progressive: options?.progressive ?? false,
    lossless: options?.lossless ?? false,
    effort,
  };
}

function toResizeOptions(options?: ResizeOptions): Record<string, unknown> {
  const out: Record<string, unknown> = {
    common: toCommon(options),
    width: options?.width ?? null,
    height: options?.height ?? null,
    percent: options?.percent ?? null,
    fit: options?.fit ?? "Fit",
    filter: options?.filter ?? "Lanczos3",
    background: toColour(options?.background ?? "#FFFFFF"),
  };
  return out;
}

function toCropOptions(options: CropOptions): Record<string, unknown> {
  if (!Number.isInteger(options.width)) {
    throw invalidArgument(
      "Crop width must be a positive integer.",
      "Crop width positive integer honi chahiye.",
    );
  }
  if (!Number.isInteger(options.height)) {
    throw invalidArgument(
      "Crop height must be a positive integer.",
      "Crop height positive integer honi chahiye.",
    );
  }
  if (options.width <= 0 || options.height <= 0) {
    // Core semantics: a zero crop size is INVALID_DIMENSIONS, not
    // INVALID_ARGUMENT (matches the shared test vectors).
    throw new DusrelError(
      "INVALID_DIMENSIONS",
      "Invalid dimensions: crop width and height must be greater than 0.",
      "Galat dimensions hain: crop width and height must be greater than 0.",
    );
  }
  return {
    common: toCommon(options),
    x: options.x ?? 0,
    y: options.y ?? 0,
    width: options.width,
    height: options.height,
    anchor: options.anchor ?? null,
  };
}

function toConvertOptions(options: ConvertOptions): Record<string, unknown> {
  const quality = options?.quality ?? 80;
  if (!Number.isInteger(quality) || quality < 1 || quality > 100) {
    throw new DusrelError(
      "INVALID_QUALITY",
      `Quality ${quality} is invalid. Quality must be between 1 and 100.`,
      `Quality ${quality} galat hai. Quality 1 se 100 ke beech honi chahiye.`,
    );
  }
  return {
    common: toCommon(options),
    format: options.format,
    quality,
    background: toColour(options.background ?? "#FFFFFF"),
    progressive: options.progressive ?? false,
  };
}

function toWatermarkOptions(
  options?: WatermarkOptions,
): Record<string, unknown> {
  const hasText = options?.text !== undefined;
  const hasLogo = options?.logo !== undefined;
  // Text and logo may be combined (core draws text first, then logo);
  // neither is still INVALID_ARGUMENT (checked by the core).
  if (hasLogo && !(options?.logo?.image instanceof Uint8Array)) {
    throw invalidType("Watermark logo image");
  }
  const opacity = options?.opacity ?? 0.5;
  if (!isFiniteNumber(opacity) || opacity < 0 || opacity > 1) {
    throw invalidArgument(
      "Opacity must be between 0.0 and 1.0.",
      "Opacity 0.0 se 1.0 ke beech honi chahiye.",
    );
  }
  const out: Record<string, unknown> = {
    common: toCommon(options),
    text: null,
    logo: null,
    opacity,
    position: options?.position ?? "BottomRight",
    rotation_deg: options?.rotationDeg ?? 0,
    margin: options?.margin ?? 16,
  };
  if (hasText) {
    const text = options!.text!;
    out["text"] = {
      text: text.text,
      size: text.size ?? 48,
      colour: toColour(text.colour ?? "#FFFFFF"),
    };
  }
  if (hasLogo) {
    const logo = options!.logo!;
    out["logo"] = {
      // Plain JS number array, NOT a Uint8Array: serde-wasm-bindgen
      // cannot decode a Uint8Array into Vec<u8> (wasm lib.rs also has
      // a manual fallback for this field — the array form is canonical).
      image: Array.from(logo.image),
      scale: logo.scale ?? 1,
    };
  }
  return out;
}

function toLayout(layout: CollageLayout | undefined): unknown {
  if (layout === undefined) return { Grid: { rows: 2, cols: 2 } };
  if (typeof layout === "string") {
    const lower = layout.trim().toLowerCase();
    if (lower === "row") return "Row";
    if (lower === "column") return "Column";
    if (lower === "grid") return { Grid: { rows: 2, cols: 2 } };
    const m = /^(\d+)x(\d+)$/.exec(lower);
    if (m) return { Grid: { rows: Number(m[1]), cols: Number(m[2]) } };
    throw invalidArgument(
      `Unknown collage layout "${layout}". Use grid, row, column or a WxH grid like 2x2.`,
      `Unknown collage layout "${layout}" hai. grid, row, column ya 2x2 jaisa WxH grid use karein.`,
    );
  }
  return { Grid: { rows: layout.rows, cols: layout.cols } };
}

function toCollageOptions(options?: CollageOptions): Record<string, unknown> {
  return {
    common: toCommon(options),
    layout: toLayout(options?.layout),
    gap: options?.gap ?? 8,
    background: toColour(options?.background ?? "#FFFFFF"),
    width: options?.width ?? 1200,
    height: options?.height ?? 1200,
    cell_fit: options?.cellFit ?? "Fit",
  };
}

function toPixelateOptions(
  options?: PixelateOptions,
): Record<string, unknown> {
  const blockSize = options?.blockSize ?? 10;
  if (!Number.isInteger(blockSize) || blockSize < 2 || blockSize > 100) {
    throw invalidArgument(
      `Block size ${blockSize} is invalid. Block size must be between 2 and 100.`,
      `Block size ${blockSize} galat hai. Block size 2 se 100 ke beech hona chahiye.`,
    );
  }
  return {
    common: toCommon(options),
    x: options?.x ?? 0,
    y: options?.y ?? 0,
    width: options?.width ?? 0,
    height: options?.height ?? 0,
    block_size: blockSize,
  };
}

/** Translate one `{ type: "Arrow", … }` annotation to serde's
 *  externally-tagged form `{ "Arrow": { … } }`, expanding colours. */
function toAnnotation(annotation: Annotation): Record<string, unknown> {
  const { type, ...rest } = annotation as Annotation & Record<string, unknown>;
  const fields: Record<string, unknown> = { ...rest };
  if (fields["colour"] !== undefined) {
    fields["colour"] = toColour(fields["colour"] as ColourInput);
  }
  if (fields["fill"] !== undefined && fields["fill"] !== null) {
    fields["fill"] = toColour(fields["fill"] as ColourInput);
  }
  return { [type]: fields };
}

function toAnnotations(value: unknown): Array<Record<string, unknown>> {
  if (!Array.isArray(value)) {
    throw new DusrelError(
      "INVALID_TYPE",
      "Annotations must be an array.",
      "Annotations array honi chahiye.",
    );
  }
  return (value as Annotation[]).map(toAnnotation);
}

// ---------------------------------------------------------------------------
// Wasm result helpers
// ---------------------------------------------------------------------------

function asBytes(value: unknown): Uint8Array {
  if (value instanceof Uint8Array) return value;
  // wasm-bindgen returns Uint8Array for Vec<u8>; anything else is a bug.
  throw new DusrelError(
    "OPERATION_FAILED",
    "WASM returned a non-bytes result.",
    "WASM ne bytes result nahi diya.",
  );
}

function base64ToBytes(b64: string): Uint8Array {
  // Node-only package (wasm --target nodejs); Buffer is available.
  return new Uint8Array(Buffer.from(b64, "base64"));
}

function parseColourJson(json: string): Colour {
  const parsed = JSON.parse(json) as {
    hex: string;
    r: number;
    g: number;
    b: number;
    a: number;
    hsl: [number, number, number];
  };
  return {
    hex: parsed.hex,
    r: parsed.r,
    g: parsed.g,
    b: parsed.b,
    a: parsed.a,
    hsl: parsed.hsl,
  };
}

// ---------------------------------------------------------------------------
// Public API — all synchronous, all throw DusrelError on failure.
// ---------------------------------------------------------------------------

/** Crate version, e.g. `"0.1.0"` (lockstep with npm / PyPI / crates.io). */
export function version(): string {
  return call(() => wasm.version());
}

/**
 * Build a deterministic test image (PNG bytes). `kind` is `"solid"`,
 * `"gradient"` or `"noise"`; `seed` defaults to 0. Test data only,
 * mirroring dusrelpdf's `makeTestPdf`.
 */
export function makeTestImage(
  kind: "solid" | "gradient" | "noise",
  width: number,
  height: number,
  seed = 0,
): Uint8Array {
  if (kind !== "solid" && kind !== "gradient" && kind !== "noise") {
    throw invalidArgument(
      'Test image kind must be "solid", "gradient" or "noise".',
      'Test image kind "solid", "gradient" ya "noise" hona chahiye.',
    );
  }
  assertNonNegativeInt(width, "Test image width");
  assertNonNegativeInt(height, "Test image height");
  // The Rust `seed` is a u64, which wasm-bindgen exposes as `bigint`.
  return call(() =>
    asBytes(wasm.make_test_image(kind, width, height, BigInt(seed))),
  );
}

/** Re-encode an image at a fixed quality (and `format`, or the input format when unset). */
export function compressImage(
  input: unknown,
  options?: CompressOptions,
): Uint8Array {
  const bytes = assertImageBytes(input);
  return call(() =>
    asBytes(wasm.compress_image(bytes, toCompressOptions(options))),
  );
}

/**
 * Re-encode an image so the result is at most `targetBytes` bytes,
 * at the highest quality that fits. Fails with
 * `TARGET_SIZE_UNREACHABLE` when even the lowest quality is too big.
 * The quality search is JPEG/AVIF-only — other formats take one
 * best-effort encode.
 */
export function compressToSize(
  input: unknown,
  targetBytes: unknown,
  options?: CompressOptions,
): Uint8Array {
  const bytes = assertImageBytes(input);
  const target = assertNonNegativeInt(targetBytes, "Target size");
  return call(() =>
    asBytes(wasm.compress_to_size(bytes, target, toCompressOptions(options))),
  );
}

/** Resize an image (width and/or height, percent, fit, filter). */
export function resizeImage(
  input: unknown,
  options?: ResizeOptions,
): Uint8Array {
  const bytes = assertImageBytes(input);
  return call(() =>
    asBytes(wasm.resize_image(bytes, toResizeOptions(options))),
  );
}

/** Crop an image to a rectangle (or an anchor preset). */
export function cropImage(input: unknown, options: CropOptions): Uint8Array {
  const bytes = assertImageBytes(input);
  if (typeof options !== "object" || options === null) {
    throw invalidArgument(
      "Crop options are required (width and height).",
      "Crop options chahiye (width aur height).",
    );
  }
  return call(() =>
    asBytes(wasm.crop_image(bytes, toCropOptions(options))),
  );
}

/** Convert an image to another format. */
export function convertImage(
  input: unknown,
  options: ConvertOptions,
): Uint8Array {
  const bytes = assertImageBytes(input);
  if (typeof options !== "object" || options === null || !options.format) {
    throw invalidArgument(
      "Convert needs a target format, e.g. { format: \"WebP\" }.",
      "Convert ke liye target format chahiye, jaise { format: \"WebP\" }.",
    );
  }
  return call(() =>
    asBytes(wasm.convert_image(bytes, toConvertOptions(options))),
  );
}

/**
 * Stamp a text or logo watermark. `options.text` and `options.logo`
 * may be combined (text first, then logo); at least one is required.
 * Defaults: opacity 0.5, position "BottomRight", rotation 0, margin 16.
 */
export function addWatermark(
  input: unknown,
  options?: WatermarkOptions,
): Uint8Array {
  const bytes = assertImageBytes(input);
  return call(() =>
    asBytes(wasm.add_watermark(bytes, toWatermarkOptions(options))),
  );
}

/** Draw annotations (arrows, rects, ellipses, lines, text) on an image. */
export function annotateImage(
  input: unknown,
  annotations: unknown,
): Uint8Array {
  const bytes = assertImageBytes(input);
  const parsed = toAnnotations(annotations);
  if (parsed.length === 0) {
    // Core semantics: an empty annotation list is INVALID_ARGUMENT,
    // not EMPTY_INPUT (matches the shared test vectors).
    throw invalidArgument(
      "No annotations given: pass at least one annotation.",
      "Koi annotation nahi diya: kam se kam ek annotation dein.",
    );
  }
  return call(() => asBytes(wasm.annotate_image(bytes, parsed)));
}

/** Read the colour of one pixel. Out-of-bounds is `PIXEL_OUT_OF_BOUNDS`. */
export function pickColour(
  input: unknown,
  x: unknown,
  y: unknown,
): Colour {
  const bytes = assertImageBytes(input);
  const px = assertNonNegativeInt(x, "Pixel x");
  const py = assertNonNegativeInt(y, "Pixel y");
  const json = call(() => wasm.pick_colour(bytes, px, py));
  return parseColourJson(json);
}

/** Extract the dominant colours, most frequent first (count 1-16). */
export function extractPalette(
  input: unknown,
  count: unknown = 5,
): PaletteColour[] {
  const bytes = assertImageBytes(input);
  if (
    typeof count !== "number" ||
    !Number.isInteger(count) ||
    count < 1 ||
    count > 16
  ) {
    throw invalidArgument(
      "Palette count must be an integer between 1 and 16.",
      "Palette count 1 se 16 ke beech integer hona chahiye.",
    );
  }
  const json = call(() => wasm.extract_palette(bytes, count));
  const parsed = JSON.parse(json) as Array<{
    hex: string;
    r: number;
    g: number;
    b: number;
    a: number;
    hsl: [number, number, number];
    share: number;
  }>;
  return parsed.map((entry) => ({
    hex: entry.hex,
    r: entry.r,
    g: entry.g,
    b: entry.b,
    a: entry.a,
    hsl: entry.hsl,
    share: entry.share,
  }));
}

/** Build a collage from several images (grid, row or column). */
export function makeCollage(
  inputs: unknown,
  options?: CollageOptions,
): Uint8Array {
  const bufs = assertImageArray(inputs, "Collage inputs", "collage inputs");
  return call(() =>
    asBytes(wasm.make_collage(bufs, toCollageOptions(options))),
  );
}

/** Pixelate a rectangle (whole image when width/height are 0/omitted). */
export function pixelateImage(
  input: unknown,
  options?: PixelateOptions,
): Uint8Array {
  const bytes = assertImageBytes(input);
  return call(() =>
    asBytes(wasm.pixelate_image(bytes, toPixelateOptions(options))),
  );
}

/** Read an image's summary without modifying it. */
export function getImageInfo(
  input: unknown,
  options?: CommonOptions,
): ImageInfo {
  const bytes = assertImageBytes(input);
  const json = call(() => wasm.get_image_info(bytes, toCommon(options)));
  const parsed = JSON.parse(json) as {
    format: string;
    width: number;
    height: number;
    has_alpha: boolean;
    colour_type: string;
    file_size: number;
  };
  return {
    format: parsed.format,
    width: parsed.width,
    height: parsed.height,
    hasAlpha: parsed.has_alpha,
    colourType: parsed.colour_type,
    fileSize: parsed.file_size,
  };
}

/**
 * Run one operation over many inputs in a single call. One bad input
 * fails only its own item (with `error.code` / `error.messageEn`),
 * never the whole batch. An empty input list returns `[]` (core
 * semantics). `collage` is not a per-item op: it returns one
 * `INVALID_ARGUMENT` result per input — call `makeCollage` once with
 * the full list instead. For op `"compress_to_size"` pass `{ targetBytes, …
 * }`; for `"annotate"` pass `{ annotations }`; for `"pick_colour"`
 * pass `{ x, y }` (result items carry `colour`); for `"palette"`
 * pass `{ count }` (result items carry `palette`).
 */
export function bulkProcess(
  inputs: unknown,
  op: BulkOp,
  options?: BulkOptions,
): BulkResult[] {
  if (Array.isArray(inputs) && inputs.length === 0) return [];
  const bufs = assertImageArray(inputs, "Bulk inputs", "bulk inputs");
  if (typeof op !== "string") {
    throw invalidArgument(
      "Bulk op must be a string, e.g. \"compress\" or \"resize\".",
      "Bulk op string hona chahiye, jaise \"compress\" ya \"resize\".",
    );
  }
  // Translate the op-specific options to the snake_case object wasm
  // expects, plus the op-specific extras (target_bytes / annotations).
  let translated: Record<string, unknown>;
  switch (op) {
    case "compress":
    case "compress_image":
      translated = toCompressOptions(options as CompressOptions);
      break;
    case "compress_to_size": {
      const opts = options as BulkCompressToSizeOptions;
      if (
        opts === undefined ||
        opts === null ||
        typeof opts.targetBytes !== "number"
      ) {
        throw invalidArgument(
          'Bulk "compress_to_size" needs targetBytes in options.',
          'Bulk "compress_to_size" ke liye options me targetBytes chahiye.',
        );
      }
      translated = toCompressOptions(opts);
      translated["target_bytes"] = assertNonNegativeInt(
        opts.targetBytes,
        "Target size",
      );
      break;
    }
    case "resize":
    case "resize_image":
      translated = toResizeOptions(options as ResizeOptions);
      break;
    case "crop":
    case "crop_image":
      translated = toCropOptions(options as CropOptions);
      break;
    case "convert":
    case "convert_image":
      translated = toConvertOptions(options as ConvertOptions);
      break;
    case "watermark":
    case "add_watermark":
      translated = toWatermarkOptions(options as WatermarkOptions);
      break;
    case "annotate":
    case "annotate_image": {
      const opts = options as BulkAnnotateOptions;
      translated = { common: toCommon(opts) };
      translated["annotations"] = toAnnotations(opts?.annotations);
      break;
    }
    case "pick_colour": {
      const opts = options as BulkPickColourOptions;
      translated = {
        common: toCommon(opts),
        x: opts?.x ?? 0,
        y: opts?.y ?? 0,
      };
      break;
    }
    case "palette": {
      const opts = options as BulkPaletteOptions;
      translated = { common: toCommon(opts), count: opts?.count ?? 5 };
      break;
    }
    case "collage":
    case "make_collage":
      // Not a per-item op: the core answers INVALID_ARGUMENT per
      // item (a collage combines all inputs — use makeCollage).
      translated = toCollageOptions(options as CollageOptions);
      break;
    case "pixelate":
    case "pixelate_image":
      translated = toPixelateOptions(options as PixelateOptions);
      break;
    default:
      throw invalidArgument(
        `Unknown bulk op "${op}". Use compress, compress_to_size, resize, crop, convert, watermark, annotate, pick_colour, palette, collage or pixelate.`,
        `Unknown bulk op "${op}" hai. compress, compress_to_size, resize, crop, convert, watermark, annotate, pick_colour, palette, collage ya pixelate use karein.`,
      );
  }
  const json = call(() => wasm.bulk_process(bufs, op, translated));
  const items = JSON.parse(json) as Array<{
    index: number;
    ok: boolean;
    output?: string | null;
    colour?: {
      hex: string;
      r: number;
      g: number;
      b: number;
      a: number;
      hsl: [number, number, number];
    } | null;
    palette?: Array<{
      hex: string;
      r: number;
      g: number;
      b: number;
      a: number;
      hsl: [number, number, number];
      share: number;
    }> | null;
    error?: { code: string; message_en: string; message_hi: string } | null;
  }>;
  return items.map((item) => {
    const out: BulkResult = { index: item.index, ok: item.ok };
    if (typeof item.output === "string") out.output = base64ToBytes(item.output);
    if (item.colour) {
      out.colour = {
        hex: item.colour.hex,
        r: item.colour.r,
        g: item.colour.g,
        b: item.colour.b,
        a: item.colour.a,
        hsl: item.colour.hsl,
      };
    }
    if (Array.isArray(item.palette)) {
      out.palette = item.palette.map((entry) => ({
        hex: entry.hex,
        r: entry.r,
        g: entry.g,
        b: entry.b,
        a: entry.a,
        hsl: entry.hsl,
        share: entry.share,
      }));
    }
    if (item.error) {
      out.error = {
        code: item.error.code,
        messageEn: item.error.message_en,
        messageHi: item.error.message_hi,
      };
    }
    return out;
  });
}

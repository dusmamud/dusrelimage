/**
 * Shared test vectors (SPEC section 6): Rust, Python and JS all run
 * `shared/test-vectors.json` and must agree.
 *
 * This runner loads the JSON (path relative to this file), builds
 * each fixture — `dusrelimage mock` kinds (`solid`/`gradient`/`noise`
 * + seed) via the wrapper's `makeTestImage` (same Rust builders),
 * plus the documented derived fixtures (`solid_rgb`,
 * `transparent_rgba`, `two_colour`, `tiff`, `jpeg`, `avif`,
 * `garbage`, `empty`) built here with a minimal PNG/TIFF encoder
 * (no binary blobs committed) — runs the op through the public TS
 * wrapper, and asserts the expected dimensions / pixel spot-checks /
 * error code.
 *
 * It FAILS LOUDLY on any case whose fixture/op kind it cannot handle:
 * unknown fixture kinds, ops, bulk ops or expected keys throw naming
 * the case id instead of silently skipping the case. Pixel checks
 * decode PNG output with a small PNG decoder below; non-PNG outputs
 * fall back to `pickColour` per pixel (only the JPEG flatten case
 * needs that).
 */
import { deflateSync, inflateSync } from "node:zlib";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  DusrelError,
  addWatermark,
  annotateImage,
  bulkProcess,
  compressImage,
  compressToSize,
  convertImage,
  cropImage,
  extractPalette,
  getImageInfo,
  makeCollage,
  makeTestImage,
  pickColour,
  pixelateImage,
  resizeImage,
  type Annotation,
  type BulkOp,
  type BulkResult,
  type CollageOptions,
  type Colour,
  type CompressOptions,
  type ConvertOptions,
  type CropOptions,
  type PixelateOptions,
  type ResizeOptions,
  type WatermarkOptions,
} from "./index";

type Json = Record<string, any>;

/** Locate shared/test-vectors.json without `import.meta` (the build
 *  tsconfig is CommonJS, so `import.meta` is not available): try
 *  `__dirname` first, then the working directory. Fails loudly. */
function vectorsPath(): string {
  const candidates: string[] = [];
  if (typeof __dirname === "string") {
    candidates.push(join(__dirname, "..", "..", "..", "shared", "test-vectors.json"));
  }
  candidates.push(join(process.cwd(), "..", "..", "shared", "test-vectors.json"));
  candidates.push(join(process.cwd(), "shared", "test-vectors.json"));
  for (const p of candidates) {
    if (existsSync(p)) return p;
  }
  throw new Error(`shared/test-vectors.json not found (tried: ${candidates.join(", ")})`);
}

const CASES: Json[] = JSON.parse(readFileSync(vectorsPath(), "utf8"));

// Expected keys this runner knows how to assert. Anything else is a
// schema gap and fails the case (see assertExpected).
const KNOWN_EXPECTED_KEYS = new Set([
  "format",
  "width",
  "height",
  "has_alpha",
  "colour_type",
  "error",
  "max_bytes",
  "avif_header",
  "pixels",
  "region_all_white",
  "region_has_dark",
  "regions_have_dark",
  "colour",
  "different_from",
  "palette_hexes",
  "palette_length",
  "share_each",
  "share_sum",
  "share_sum_tolerance",
  "share_tolerance",
  "palette",
  "differs_from_input",
  "pixel_equalities",
  "unchanged_pixels",
  "results",
]);

// ---------------------------------------------------------------------------
// Minimal PNG encoder / decoder + TIFF writer (test fixtures only)
// ---------------------------------------------------------------------------

const CRC_TABLE: Uint32Array = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(buf: Buffer): number {
  let c = 0xffffffff;
  for (const byte of buf) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type: string, data: Buffer): Buffer {
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, "ascii");
  data.copy(out, 8);
  out.writeUInt32BE(crc32(Buffer.concat([Buffer.from(type, "ascii"), data])), 8 + data.length);
  return out;
}

type RGBA = [number, number, number, number];

function encodePng(
  width: number,
  height: number,
  pixel: (x: number, y: number) => RGBA,
  rgba = false,
): Uint8Array {
  const channels = rgba ? 4 : 3;
  const raw = Buffer.alloc(height * (1 + width * channels));
  for (let y = 0; y < height; y++) {
    raw[y * (1 + width * channels)] = 0; // filter: none
    for (let x = 0; x < width; x++) {
      const px = pixel(x, y);
      const off = y * (1 + width * channels) + 1 + x * channels;
      for (let c = 0; c < channels; c++) raw[off + c] = px[c];
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = rgba ? 6 : 2; // colour type
  const parts = [
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", deflateSync(raw)),
    pngChunk("IEND", Buffer.alloc(0)),
  ];
  return new Uint8Array(Buffer.concat(parts));
}

function paeth(a: number, b: number, c: number): number {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  if (pb <= pc) return b;
  return c;
}

function decodePng(data: Uint8Array): RGBA[][] {
  const buf = Buffer.from(data);
  if (!buf.subarray(0, 8).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]))) {
    throw new Error("not a PNG");
  }
  let pos = 8;
  let idat = Buffer.alloc(0);
  let width = 0;
  let height = 0;
  let colourType = 0;
  while (pos < buf.length) {
    const length = buf.readUInt32BE(pos);
    const type = buf.subarray(pos + 4, pos + 8).toString("ascii");
    const chunk = buf.subarray(pos + 8, pos + 8 + length);
    if (type === "IHDR") {
      width = chunk.readUInt32BE(0);
      height = chunk.readUInt32BE(4);
      if (chunk[8] !== 8) throw new Error(`unsupported PNG bit depth ${chunk[8]}`);
      colourType = chunk[9];
      if (colourType !== 2 && colourType !== 6) {
        throw new Error(`unsupported PNG colour type ${colourType}`);
      }
    } else if (type === "IDAT") {
      idat = Buffer.concat([idat, chunk]);
    } else if (type === "IEND") {
      break;
    }
    pos += 12 + length;
  }
  const channels = colourType === 2 ? 3 : 4;
  const stride = width * channels;
  const raw = inflateSync(idat);
  const rows: RGBA[][] = [];
  let prev = Buffer.alloc(stride);
  let p = 0;
  for (let y = 0; y < height; y++) {
    const f = raw[p];
    p += 1;
    const line = Buffer.from(raw.subarray(p, p + stride));
    p += stride;
    for (let i = 0; i < stride; i++) {
      const a = i >= channels ? line[i - channels] : 0;
      const b = prev[i];
      const c = i >= channels ? prev[i - channels] : 0;
      if (f === 1) line[i] = (line[i] + a) & 0xff;
      else if (f === 2) line[i] = (line[i] + b) & 0xff;
      else if (f === 3) line[i] = (line[i] + ((a + b) >> 1)) & 0xff;
      else if (f === 4) line[i] = (line[i] + paeth(a, b, c)) & 0xff;
      else if (f !== 0) throw new Error(`unsupported PNG filter ${f}`);
    }
    prev = line;
    const row: RGBA[] = [];
    for (let x = 0; x < width; x++) {
      row.push([
        line[x * channels],
        line[x * channels + 1],
        line[x * channels + 2],
        channels === 4 ? line[x * channels + 3] : 255,
      ]);
    }
    rows.push(row);
  }
  return rows;
}

/** Minimal uncompressed little-endian TIFF, RGB 8-bit. */
function encodeTiff(width: number, height: number, rgb: [number, number, number]): Uint8Array {
  const bitsOffset = 8 + 2 + 10 * 12 + 4;
  const dataOffset = bitsOffset + 6;
  const pixels = Buffer.alloc(width * height * 3);
  for (let i = 0; i < width * height; i++) {
    pixels[i * 3] = rgb[0];
    pixels[i * 3 + 1] = rgb[1];
    pixels[i * 3 + 2] = rgb[2];
  }
  const entry = (tag: number, type: number, count: number, value: number): Buffer => {
    const b = Buffer.alloc(12);
    b.writeUInt16LE(tag, 0);
    b.writeUInt16LE(type, 2);
    b.writeUInt32LE(count, 4);
    b.writeUInt32LE(value, 8);
    return b;
  };
  const ifd = Buffer.concat([
    entry(256, 4, 1, width),
    entry(257, 4, 1, height),
    entry(258, 3, 3, bitsOffset),
    entry(259, 3, 1, 1),
    entry(262, 3, 1, 2),
    entry(273, 4, 1, dataOffset),
    entry(277, 3, 1, 3),
    entry(278, 4, 1, height),
    entry(279, 4, 1, pixels.length),
    entry(284, 3, 1, 1),
  ]);
  const header = Buffer.alloc(8);
  header.write("II", 0, "ascii");
  header.writeUInt16LE(42, 2);
  header.writeUInt32LE(8, 4);
  const count = Buffer.alloc(2);
  count.writeUInt16LE(10, 0);
  const next = Buffer.alloc(4);
  const bits = Buffer.alloc(6);
  bits.writeUInt16LE(8, 0);
  bits.writeUInt16LE(8, 2);
  bits.writeUInt16LE(8, 4);
  return new Uint8Array(Buffer.concat([header, count, ifd, next, bits, pixels]));
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function parseDimsFromDerived(text: string, caseId: string): [number, number] {
  for (const token of text.replace(/,/g, " ").split(/\s+/)) {
    const m = /^(\d+)x(\d+)$/.exec(token);
    if (m) return [Number(m[1]), Number(m[2])];
  }
  throw new Error(`case ${caseId}: no WxH dimensions in derived_from: ${text}`);
}

function parseRgbFromDerived(text: string, caseId: string): [number, number, number] {
  const m = /\((\d+),\s*(\d+),\s*(\d+)\)/.exec(text);
  if (!m) throw new Error(`case ${caseId}: no rgb tuple in derived_from: ${text}`);
  return [Number(m[1]), Number(m[2]), Number(m[3])];
}

function fixtureBytes(fixture: Json, caseId: string): Uint8Array {
  const kind = fixture.kind as string;
  if (kind === "solid" || kind === "gradient" || kind === "noise") {
    return makeTestImage(kind, fixture.width, fixture.height, fixture.seed);
  }
  if (kind === "solid_rgb") {
    const [r, g, b] = fixture.rgb as [number, number, number];
    return encodePng(fixture.width, fixture.height, () => [r, g, b, 255]);
  }
  if (kind === "transparent_rgba") {
    return encodePng(fixture.width, fixture.height, () => [0, 0, 0, 0], true);
  }
  if (kind === "two_colour") {
    // Left half red, right half blue, 50/50 by construction.
    const width = fixture.width as number;
    return encodePng(fixture.width, fixture.height, (x) =>
      x < width / 2 ? [255, 0, 0, 255] : [0, 0, 255, 255],
    );
  }
  if (kind === "tiff") {
    // rgb + dimensions live only in derived_from ("rgb (10,20,30)
    // 48x32 TIFF, written with the image crate").
    const derived = (fixture.derived_from as string) ?? "";
    const rgb = (fixture.rgb as [number, number, number] | undefined) ??
      parseRgbFromDerived(derived, caseId);
    const [width, height] =
      fixture.width !== undefined
        ? [fixture.width as number, fixture.height as number]
        : parseDimsFromDerived(derived, caseId);
    return encodeTiff(width, height, rgb);
  }
  if (kind === "jpeg") {
    // Derived: compress gradient 100x80 seed 2 to JPEG q80.
    if (!String(fixture.derived_from ?? "").includes("jpeg")) {
      throw new Error(`case ${caseId}: jpeg fixture derived_from does not mention jpeg`);
    }
    return compressImage(makeTestImage("gradient", 100, 80, 2), {
      format: "Jpeg",
      quality: 80,
    });
  }
  if (kind === "avif") {
    // Derived: convert gradient 64x48 seed 4 to AVIF q80.
    if (!String(fixture.derived_from ?? "").includes("avif")) {
      throw new Error(`case ${caseId}: avif fixture derived_from does not mention avif`);
    }
    return convertImage(makeTestImage("gradient", 64, 48, 4), {
      format: "Avif",
      quality: 80,
    });
  }
  if (kind === "garbage") {
    const derived = (fixture.derived_from as string) ?? "";
    const bytesMatch = /^bytes \[(.*)\]$/.exec(derived);
    if (bytesMatch) {
      return new Uint8Array(bytesMatch[1].split(",").map((n) => Number(n.trim())));
    }
    const asciiMatch = /^ASCII bytes: '(.*)'$/.exec(derived);
    if (asciiMatch) return new Uint8Array(Buffer.from(asciiMatch[1], "utf8"));
    throw new Error(`case ${caseId}: unhandled garbage derived_from: ${derived}`);
  }
  if (kind === "empty") return new Uint8Array(0);
  throw new Error(`case ${caseId}: unknown fixture kind ${kind} — update this runner`);
}

// ---------------------------------------------------------------------------
// Options translation (JSON snake_case -> TS camelCase wrapper options)
// ---------------------------------------------------------------------------

function toCommonTs(options: Json): Record<string, unknown> {
  const common = options.common ?? {};
  const out: Record<string, unknown> = {};
  if (common.auto_orient !== undefined) out.autoOrient = common.auto_orient;
  if (common.keep_exif !== undefined) out.keepExif = common.keep_exif;
  if (common.dpi !== undefined) out.dpi = common.dpi;
  if (common.max_pixels !== undefined) out.maxPixels = common.max_pixels;
  if (common.max_dimension !== undefined) out.maxDimension = common.max_dimension;
  return out;
}

function withCommon(options: Json): Record<string, unknown> {
  const { common: _common, ...rest } = options;
  return { ...toCommonTs(options), ...rest };
}

function toWatermarkTs(options: Json, caseId: string): Record<string, unknown> {
  const out: Record<string, unknown> = withCommon(options);
  if (options.rotation_deg !== undefined) {
    out.rotationDeg = options.rotation_deg;
    delete out.rotation_deg;
  }
  if (options.logo !== undefined) {
    out.logo = {
      image: fixtureBytes(options.logo.image, caseId),
      scale: options.logo.scale,
    };
  }
  return out;
}

function toCollageTs(options: Json): Record<string, unknown> {
  const out: Record<string, unknown> = withCommon(options);
  if (options.layout !== undefined) {
    if (typeof options.layout === "string") {
      out.layout = options.layout.toLowerCase() as CollageOptions["layout"];
    } else if (options.layout.Grid !== undefined) {
      out.layout = { rows: options.layout.Grid.rows, cols: options.layout.Grid.cols };
    } else {
      throw new Error(`unknown collage layout ${JSON.stringify(options.layout)}`);
    }
  }
  if (options.cell_fit !== undefined) {
    out.cellFit = options.cell_fit;
    delete out.cell_fit;
  }
  return out;
}

function toPixelateTs(options: Json): Record<string, unknown> {
  const out: Record<string, unknown> = withCommon(options);
  if (options.block_size !== undefined) {
    out.blockSize = options.block_size;
    delete out.block_size;
  }
  return out;
}

// ---------------------------------------------------------------------------
// Running one case
// ---------------------------------------------------------------------------

type Outcome =
  | { kind: "image"; bytes: Uint8Array }
  | { kind: "info"; info: ReturnType<typeof getImageInfo> }
  | { kind: "colour"; colour: Colour }
  | { kind: "palette"; palette: Array<Colour & { share: number }> }
  | { kind: "bulk"; results: BulkResult[] };

function runCase(c: Json, caseId: string): { outcome: Outcome; input?: Uint8Array } {
  const options: Json = c.options ?? {};
  let inputs: Uint8Array[];
  let input: Uint8Array | undefined;
  if (Array.isArray(c.fixtures)) {
    inputs = c.fixtures.map((f: Json) => fixtureBytes(f, caseId));
    input = inputs[0];
  } else if (c.fixture !== undefined) {
    input = fixtureBytes(c.fixture, caseId);
    inputs = [input];
  } else {
    inputs = [];
  }
  const needImage = (): Uint8Array => {
    if (input === undefined) throw new Error(`case ${caseId}: op needs one fixture`);
    return input;
  };
  switch (c.op as string) {
    case "get_image_info":
      return {
        outcome: { kind: "info", info: getImageInfo(needImage(), toCommonTs(options)) },
        input,
      };
    case "compress_image":
      return {
        outcome: {
          kind: "image",
          bytes: compressImage(needImage(), withCommon(options) as CompressOptions),
        },
        input,
      };
    case "compress_to_size":
      return {
        outcome: {
          kind: "image",
          bytes: compressToSize(
            needImage(),
            c.target_bytes,
            withCommon(options) as CompressOptions,
          ),
        },
        input,
      };
    case "resize_image":
      return {
        outcome: {
          kind: "image",
          bytes: resizeImage(needImage(), withCommon(options) as ResizeOptions),
        },
        input,
      };
    case "crop_image":
      return {
        outcome: {
          kind: "image",
          bytes: cropImage(needImage(), withCommon(options) as unknown as CropOptions),
        },
        input,
      };
    case "convert_image":
      return {
        outcome: {
          kind: "image",
          bytes: convertImage(needImage(), withCommon(options) as unknown as ConvertOptions),
        },
        input,
      };
    case "add_watermark":
      return {
        outcome: {
          kind: "image",
          bytes: addWatermark(needImage(), toWatermarkTs(options, caseId) as WatermarkOptions),
        },
        input,
      };
    case "annotate_image":
      return {
        outcome: {
          kind: "image",
          bytes: annotateImage(needImage(), c.annotations as Annotation[]),
        },
        input,
      };
    case "pick_colour":
      return {
        outcome: {
          kind: "colour",
          colour: pickColour(needImage(), options.x, options.y),
        },
        input,
      };
    case "extract_palette":
      return {
        outcome: { kind: "palette", palette: extractPalette(needImage(), options.count) },
        input,
      };
    case "make_collage":
      return {
        outcome: {
          kind: "image",
          bytes: makeCollage(inputs, toCollageTs(options) as CollageOptions),
        },
      };
    case "pixelate_image":
      return {
        outcome: {
          kind: "image",
          bytes: pixelateImage(needImage(), toPixelateTs(options) as PixelateOptions),
        },
        input,
      };
    case "bulk_process": {
      const bulkOp = c.bulk_op as string;
      const opMap: Record<string, BulkOp> = {
        Pixelate: "pixelate",
        PickColour: "pick_colour",
        Palette: "palette",
        Collage: "collage",
        Watermark: "watermark",
        Annotate: "annotate",
      };
      const op = opMap[bulkOp];
      if (op === undefined) {
        throw new Error(`case ${caseId}: unknown bulk_op ${bulkOp} — update this runner`);
      }
      let bulkOptions: Record<string, unknown>;
      if (bulkOp === "Pixelate") bulkOptions = toPixelateTs(options);
      else if (bulkOp === "PickColour") bulkOptions = { x: options.x, y: options.y };
      else if (bulkOp === "Palette") bulkOptions = { count: options.count };
      else if (bulkOp === "Collage") bulkOptions = toCollageTs(options);
      else if (bulkOp === "Watermark") bulkOptions = toWatermarkTs(options, caseId);
      else bulkOptions = { annotations: c.annotations as Annotation[] };
      return { outcome: { kind: "bulk", results: bulkProcess(inputs, op, bulkOptions as never) } };
    }
    default:
      throw new Error(`case ${caseId}: unknown op ${c.op} — update this runner`);
  }
}

// ---------------------------------------------------------------------------
// Assertions on the expected object
// ---------------------------------------------------------------------------

class Pixels {
  private rows: RGBA[][] | null;
  private data: Uint8Array;
  private caseId: string;

  constructor(data: Uint8Array, caseId: string) {
    this.data = data;
    this.caseId = caseId;
    this.rows =
      data.length >= 8 && data[1] === 0x50 && data[2] === 0x4e && data[3] === 0x47
        ? decodePng(data)
        : null;
  }

  pixel(x: number, y: number): RGBA {
    if (this.rows !== null) return this.rows[y][x];
    const c = pickColour(this.data, x, y);
    return [c.r, c.g, c.b, c.a];
  }

  regionHasDark(x0: number, y0: number, x1: number, y1: number): boolean {
    if (this.rows === null) {
      throw new Error(`case ${this.caseId}: region check needs a PNG output`);
    }
    for (let y = y0; y < y1; y++) {
      for (let x = x0; x < x1; x++) {
        const px = this.rows[y][x];
        if (px[0] < 128 || px[1] < 128 || px[2] < 128) return true;
      }
    }
    return false;
  }

  regionAllWhite(x0: number, y0: number, x1: number, y1: number): boolean {
    if (this.rows === null) {
      throw new Error(`case ${this.caseId}: region check needs a PNG output`);
    }
    for (let y = y0; y < y1; y++) {
      for (let x = x0; x < x1; x++) {
        const px = this.rows[y][x];
        if (px[0] !== 255 || px[1] !== 255 || px[2] !== 255) return false;
      }
    }
    return true;
  }
}

function checkPixelAssert(px: RGBA, spec: Json, caseId: string, failures: string[]): void {
  const { x, y } = spec;
  if (spec.rgba !== undefined) {
    if (px.join(",") !== (spec.rgba as number[]).join(",")) {
      failures.push(
        `case ${caseId}: pixel (${x},${y}) expected rgba ${spec.rgba}, got ${px.join(",")}`,
      );
    }
  }
  if (spec.rgba_approx !== undefined) {
    const tol = spec.tolerance ?? 0;
    (spec.rgba_approx as number[]).forEach((want, i) => {
      if (Math.abs(px[i] - want) > tol) {
        failures.push(
          `case ${caseId}: pixel (${x},${y}) channel ${i} expected ~${want}±${tol}, got ${px[i]}`,
        );
      }
    });
  }
  for (const [key, ch] of [
    ["r", 0],
    ["g", 1],
    ["b", 2],
    ["a", 3],
  ] as const) {
    if (spec[key] !== undefined && px[ch] !== spec[key]) {
      failures.push(
        `case ${caseId}: pixel (${x},${y}) channel ${key} expected ${spec[key]}, got ${px[ch]}`,
      );
    }
  }
  if (spec.r_max !== undefined && px[0] > spec.r_max) {
    failures.push(`case ${caseId}: pixel (${x},${y}) r expected <= ${spec.r_max}, got ${px[0]}`);
  }
}

function assertExpected(
  c: Json,
  caseId: string,
  outcome: Outcome | null,
  error: DusrelError | null,
  input: Uint8Array | undefined,
  failures: string[],
): void {
  const expected: Json = c.expected;
  for (const key of Object.keys(expected)) {
    if (!KNOWN_EXPECTED_KEYS.has(key)) {
      failures.push(`case ${caseId}: unhandled expected key ${key} — update this runner`);
    }
  }
  if (expected.error !== undefined) {
    if (error === null) {
      failures.push(`case ${caseId}: expected error ${expected.error}, got success`);
    } else if (error.code !== expected.error) {
      failures.push(`case ${caseId}: expected error ${expected.error}, got ${error.code}`);
    }
    return;
  }
  if (error !== null) {
    failures.push(`case ${caseId}: expected success, got ${error.code} (${error.messageEn})`);
    return;
  }
  if (outcome === null) {
    failures.push(`case ${caseId}: no outcome and no error`);
    return;
  }

  let info: ReturnType<typeof getImageInfo> | null = null;
  if (outcome.kind === "info") {
    info = outcome.info;
  } else if (outcome.kind === "image") {
    // AVIF output cannot be decoded back in this build (no
    // avif-native decoder): a case that only asserts the header/size
    // may skip the decode; anything asserting dims must be decodable.
    try {
      info = getImageInfo(outcome.bytes);
    } catch (e) {
      if (["format", "width", "height", "has_alpha", "colour_type"].some((k) => k in expected)) {
        throw e;
      }
      info = null;
    }
  }
  if (info !== null) {
    if (expected.format !== undefined && info.format !== expected.format) {
      failures.push(`case ${caseId}: expected format ${expected.format}, got ${info.format}`);
    }
    if (expected.width !== undefined && info.width !== expected.width) {
      failures.push(`case ${caseId}: expected width ${expected.width}, got ${info.width}`);
    }
    if (expected.height !== undefined && info.height !== expected.height) {
      failures.push(`case ${caseId}: expected height ${expected.height}, got ${info.height}`);
    }
    if (expected.has_alpha !== undefined && info.hasAlpha !== expected.has_alpha) {
      failures.push(
        `case ${caseId}: expected has_alpha ${expected.has_alpha}, got ${info.hasAlpha}`,
      );
    }
    if (expected.colour_type !== undefined && info.colourType !== expected.colour_type) {
      failures.push(
        `case ${caseId}: expected colour_type ${expected.colour_type}, got ${info.colourType}`,
      );
    }
  }

  if (outcome.kind === "image") {
    const bytes = outcome.bytes;
    if (expected.max_bytes !== undefined && bytes.byteLength > expected.max_bytes) {
      failures.push(
        `case ${caseId}: expected <= ${expected.max_bytes} bytes, got ${bytes.byteLength}`,
      );
    }
    if (expected.avif_header === true) {
      if (bytes.byteLength < 12 || Buffer.from(bytes.subarray(4, 12)).toString("ascii") !== "ftypavif") {
        failures.push(`case ${caseId}: expected AVIF ftypavif header`);
      }
    }
    const pixels = new Pixels(bytes, caseId);
    const inputPixels = input !== undefined ? new Pixels(input, caseId) : null;
    for (const spec of expected.pixels ?? []) {
      checkPixelAssert(pixels.pixel(spec.x, spec.y), spec, caseId, failures);
    }
    for (const [key, check] of [
      ["region_all_white", (r: Json) => pixels.regionAllWhite(r.x0, r.y0, r.x1, r.y1)],
      ["region_has_dark", (r: Json) => pixels.regionHasDark(r.x0, r.y0, r.x1, r.y1)],
    ] as const) {
      if (expected[key] !== undefined) {
        const r = expected[key];
        if (!check(r)) {
          failures.push(
            `case ${caseId}: region (${r.x0},${r.y0})-(${r.x1},${r.y1}) failed ${key}`,
          );
        }
      }
    }
    for (const r of expected.regions_have_dark ?? []) {
      if (!pixels.regionHasDark(r.x0, r.y0, r.x1, r.y1)) {
        failures.push(
          `case ${caseId}: region (${r.x0},${r.y0})-(${r.x1},${r.y1}) should contain dark pixels`,
        );
      }
    }
    if (inputPixels !== null) {
      for (const [x, y] of expected.differs_from_input ?? []) {
        if (pixels.pixel(x, y).join(",") === inputPixels.pixel(x, y).join(",")) {
          failures.push(`case ${caseId}: pixel (${x},${y}) should differ from the input`);
        }
      }
      for (const pair of expected.pixel_equalities ?? []) {
        const [ax, ay] = pair.a as [number, number];
        const [bx, by] = pair.b as [number, number];
        const same = pixels.pixel(ax, ay).join(",") === pixels.pixel(bx, by).join(",");
        if (same !== pair.equal) {
          failures.push(
            `case ${caseId}: pixels (${ax},${ay}) vs (${bx},${by}) equal=${same}, expected ${pair.equal}`,
          );
        }
      }
      for (const [x, y] of expected.unchanged_pixels ?? []) {
        if (pixels.pixel(x, y).join(",") !== inputPixels.pixel(x, y).join(",")) {
          failures.push(`case ${caseId}: pixel (${x},${y}) should be unchanged from the input`);
        }
      }
    }
  }

  if (outcome.kind === "colour") {
    const colour = outcome.colour;
    if (expected.colour !== undefined) {
      const want = expected.colour;
      for (const key of ["hex", "r", "g", "b", "a"]) {
        if (want[key] !== undefined && (colour as unknown as Json)[key] !== want[key]) {
          failures.push(
            `case ${caseId}: expected colour ${key}=${want[key]}, got ${(colour as unknown as Json)[key]}`,
          );
        }
      }
    }
    if (expected.different_from !== undefined && input !== undefined) {
      const other = expected.different_from;
      const otherColour = pickColour(input, other.x, other.y);
      if (otherColour.hex === colour.hex) {
        failures.push(
          `case ${caseId}: colour should differ from the pixel at (${other.x},${other.y})`,
        );
      }
    }
  }

  if (outcome.kind === "palette") {
    const palette = outcome.palette;
    if (expected.palette_length !== undefined && palette.length !== expected.palette_length) {
      failures.push(
        `case ${caseId}: expected palette length ${expected.palette_length}, got ${palette.length}`,
      );
    }
    if (expected.palette_hexes !== undefined) {
      // Compared as a set: the contract orders by frequency ("most
      // frequent first") and these shares are equal, so the tie order
      // is unspecified — the core breaks ties by hex ascending.
      // Shares are asserted separately.
      const got = palette.map((p) => p.hex).sort();
      const want = [...(expected.palette_hexes as string[])].sort();
      if (got.join(",") !== want.join(",")) {
        failures.push(
          `case ${caseId}: expected palette hexes ${want.join(",")}, got ${got.join(",")}`,
        );
      }
    }
    const shareTol = expected.share_tolerance ?? 0;
    if (expected.share_each !== undefined) {
      for (const entry of palette) {
        if (Math.abs(entry.share - expected.share_each) > shareTol) {
          failures.push(
            `case ${caseId}: palette ${entry.hex} share expected ${expected.share_each}±${shareTol}, got ${entry.share}`,
          );
        }
      }
    }
    if (expected.share_sum !== undefined) {
      const sum = palette.reduce((s, p) => s + p.share, 0);
      if (Math.abs(sum - expected.share_sum) > (expected.share_sum_tolerance ?? 0)) {
        failures.push(`case ${caseId}: palette share sum expected ${expected.share_sum}, got ${sum}`);
      }
    }
    if (expected.palette !== undefined) {
      const want = expected.palette as Json[];
      if (want.length !== palette.length) {
        failures.push(
          `case ${caseId}: expected palette of ${want.length}, got ${palette.length}`,
        );
      } else {
        want.forEach((w, i) => {
          if (w.hex !== undefined && palette[i].hex !== w.hex) {
            failures.push(`case ${caseId}: expected palette hex ${w.hex}, got ${palette[i].hex}`);
          }
          if (
            w.share !== undefined &&
            Math.abs(palette[i].share - w.share) > (w.share_tolerance ?? 0)
          ) {
            failures.push(
              `case ${caseId}: palette ${palette[i].hex} share expected ${w.share}, got ${palette[i].share}`,
            );
          }
        });
      }
    }
  }

  if (outcome.kind === "bulk") {
    const results = outcome.results;
    const want = (expected.results ?? []) as Json[];
    if (results.length !== want.length) {
      failures.push(
        `case ${caseId}: expected ${want.length} bulk results, got ${results.length}`,
      );
      return;
    }
    results.forEach((got, i) => {
      const wantItem = want[i];
      const idx = wantItem.index ?? got.index;
      if (got.index !== idx) {
        failures.push(`case ${caseId}: bulk item index expected ${idx}, got ${got.index}`);
      }
      const wantOk = wantItem.ok ?? true;
      if (got.ok !== wantOk) {
        failures.push(`case ${caseId}: bulk item ${idx} ok expected ${wantOk}, got ${got.ok}`);
        return;
      }
      if (!got.ok) {
        if (wantItem.error !== undefined && got.error?.code !== wantItem.error) {
          failures.push(
            `case ${caseId}: bulk item ${idx} error expected ${wantItem.error}, got ${got.error?.code}`,
          );
        }
        return;
      }
      if (wantItem.output === "Image") {
        if (got.output === undefined) {
          failures.push(`case ${caseId}: bulk item ${idx} output expected Image bytes`);
          return;
        }
        const info = getImageInfo(got.output);
        if (wantItem.width !== undefined && info.width !== wantItem.width) {
          failures.push(
            `case ${caseId}: bulk item ${idx} width expected ${wantItem.width}, got ${info.width}`,
          );
        }
        if (wantItem.height !== undefined && info.height !== wantItem.height) {
          failures.push(
            `case ${caseId}: bulk item ${idx} height expected ${wantItem.height}, got ${info.height}`,
          );
        }
      } else if (wantItem.output === "Colour") {
        if (wantItem.colour !== undefined && got.colour?.hex !== wantItem.colour.hex) {
          failures.push(
            `case ${caseId}: bulk item ${idx} colour hex expected ${wantItem.colour.hex}, got ${got.colour?.hex}`,
          );
        }
      } else if (wantItem.output === "Palette") {
        if (
          wantItem.palette_length !== undefined &&
          got.palette?.length !== wantItem.palette_length
        ) {
          failures.push(
            `case ${caseId}: bulk item ${idx} palette length expected ${wantItem.palette_length}, got ${got.palette?.length}`,
          );
        }
        (wantItem.palette ?? []).forEach((w: Json, j: number) => {
          if (w.hex !== undefined && got.palette?.[j]?.hex !== w.hex) {
            failures.push(
              `case ${caseId}: bulk item ${idx} palette hex expected ${w.hex}, got ${got.palette?.[j]?.hex}`,
            );
          }
        });
      }
    });
  }
}

describe("shared test vectors (shared/test-vectors.json)", () => {
  it("has 82 cases", () => {
    expect(CASES).toHaveLength(82);
  });

  for (const c of CASES) {
    const caseId = c.id as string;
    it(caseId, () => {
      const failures: string[] = [];
      let outcome: Outcome | null = null;
      let error: DusrelError | null = null;
      let input: Uint8Array | undefined;
      try {
        const run = runCase(c, caseId);
        outcome = run.outcome;
        input = run.input;
      } catch (e) {
        if (e instanceof DusrelError) {
          error = e;
          // The input is only needed for pixel comparisons on success.
          if (c.fixture !== undefined) input = fixtureBytes(c.fixture, caseId);
        } else {
          throw e;
        }
      }
      assertExpected(c, caseId, outcome, error, input, failures);
      expect(failures).toEqual([]);
    });
  }
});

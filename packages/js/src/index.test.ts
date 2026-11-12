/**
 * dusrelimage JS tests — WASM wrapper over the Rust core.
 *
 * Every test runs through the real wasm build (`npm run build` via
 * `pretest`), so these assert the whole chain: TS validation ->
 * camelCase-to-snake_case translation -> wasm -> Rust core.
 * Dimension assertions go through `getImageInfo` (the wasm
 * round-trip); pixel assertions go through `pickColour`.
 *
 * Fixtures: tiny PNGs generated in-test with a minimal encoder
 * (zlib deflate + CRC32, RGB8, no interlace) — no binary blobs
 * committed. A solid red 10x10, a 64x48 gradient, and a 64x48
 * photo-like noise image (deterministic hash noise, so
 * `compressToSize` has something real to squeeze).
 */
import { deflateSync } from "node:zlib";
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
  version,
} from "./index";
import {
  annotationSchema,
  compressOptionsSchema,
  imageBytesSchema,
  pixelateOptionsSchema,
} from "./zod";

// ---------------------------------------------------------------------------
// Minimal PNG encoder (RGB8, filter 0 per row) — test fixtures only.
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

type RGB = [number, number, number];

function png(
  width: number,
  height: number,
  pixel: (x: number, y: number) => RGB,
): Uint8Array {
  const raw = Buffer.alloc(height * (1 + width * 3));
  for (let y = 0; y < height; y++) {
    raw[y * (1 + width * 3)] = 0; // filter: none
    for (let x = 0; x < width; x++) {
      const [r, g, b] = pixel(x, y);
      const off = y * (1 + width * 3) + 1 + x * 3;
      raw[off] = r;
      raw[off + 1] = g;
      raw[off + 2] = b;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // colour type RGB
  const parts = [
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", deflateSync(raw)),
    pngChunk("IEND", Buffer.alloc(0)),
  ];
  return new Uint8Array(Buffer.concat(parts));
}

const redPng = (): Uint8Array => png(10, 10, () => [255, 0, 0]);

const gradientPng = (): Uint8Array =>
  png(64, 48, (x, y) => [
    Math.round((x / 63) * 255),
    Math.round((y / 47) * 255),
    128,
  ]);

/** Deterministic hash noise over a gradient (photo-like: compresses badly). */
const noisePng = (): Uint8Array =>
  png(64, 48, (x, y) => {
    const h = (x * 2654435761 + y * 40503) >>> 0;
    return [
      (h & 0xff) ^ Math.round((x / 63) * 120),
      ((h >> 8) & 0xff) ^ Math.round((y / 47) * 120),
      ((h >> 16) & 0xff) ^ 64,
    ];
  });

const garbage = (): Uint8Array =>
  new Uint8Array(Buffer.from("this is not an image at all", "utf8"));

const dims = (bytes: Uint8Array): [number, number] => {
  const info = getImageInfo(bytes);
  return [info.width, info.height];
};

function expectDusrelError(fn: () => unknown, code: string): void {
  try {
    fn();
  } catch (e) {
    expect(e).toBeInstanceOf(DusrelError);
    expect((e as DusrelError).code).toBe(code);
    expect((e as DusrelError).messageEn.length).toBeGreaterThan(0);
    expect((e as DusrelError).messageHi.length).toBeGreaterThan(0);
    return;
  }
  throw new Error(`expected DusrelError ${code}, but no error was thrown`);
}

// ---------------------------------------------------------------------------
// Meta
// ---------------------------------------------------------------------------

describe("version + test images", () => {
  it("reports the lockstep version", () => {
    expect(version()).toBe("0.1.0");
  });

  it("makeTestImage builds solid / gradient / noise PNGs", () => {
    for (const kind of ["solid", "gradient", "noise"] as const) {
      const img = makeTestImage(kind, 40, 30, 7);
      const info = getImageInfo(img);
      expect(info.format).toBe("png");
      expect([info.width, info.height]).toEqual([40, 30]);
    }
  });

  it("rejects an unknown test-image kind", () => {
    expectDusrelError(
      () => makeTestImage("photo" as never, 10, 10),
      "INVALID_ARGUMENT",
    );
  });
});

// ---------------------------------------------------------------------------
// Tool 1 — compressor
// ---------------------------------------------------------------------------

describe("compressImage / compressToSize", () => {
  it("re-encodes PNG at default quality, dimensions preserved", () => {
    const out = compressImage(gradientPng());
    expect(out.byteLength).toBeGreaterThan(0);
    expect(dims(out)).toEqual([64, 48]);
    expect(getImageInfo(out).format).toBe("png");
  });

  it("compresses to JPEG when format is given", () => {
    const out = compressImage(noisePng(), { quality: 50, format: "Jpeg" });
    expect(getImageInfo(out).format).toBe("jpeg");
    expect(dims(out)).toEqual([64, 48]);
  });

  it("compressToSize fits a generous target", () => {
    const out = compressToSize(noisePng(), 100_000, {
      quality: 90,
      format: "Jpeg",
    });
    expect(out.byteLength).toBeLessThanOrEqual(100_000);
    expect(getImageInfo(out).format).toBe("jpeg");
  });

  it("compressToSize fails when the target is unreachable", () => {
    expectDusrelError(
      () => compressToSize(noisePng(), 10, { format: "Jpeg" }),
      "TARGET_SIZE_UNREACHABLE",
    );
  });

  it("rejects quality 0 client-side", () => {
    expectDusrelError(
      () => compressImage(gradientPng(), { quality: 0 }),
      "INVALID_QUALITY",
    );
  });
});

// ---------------------------------------------------------------------------
// Tool 2 — resizer
// ---------------------------------------------------------------------------

describe("resizeImage", () => {
  it("resizes by width, keeping aspect ratio", () => {
    expect(dims(resizeImage(gradientPng(), { width: 32 }))).toEqual([32, 24]);
  });

  it("resizes by percent", () => {
    expect(dims(resizeImage(gradientPng(), { percent: 50 }))).toEqual([32, 24]);
  });

  it("Fill produces exactly the target canvas", () => {
    expect(
      dims(resizeImage(gradientPng(), { width: 100, height: 100, fit: "Fill" })),
    ).toEqual([100, 100]);
  });

  it("Stretch produces exactly the target size", () => {
    expect(
      dims(
        resizeImage(gradientPng(), { width: 100, height: 20, fit: "Stretch" }),
      ),
    ).toEqual([100, 20]);
  });

  it("Nearest filter works", () => {
    expect(
      dims(resizeImage(gradientPng(), { width: 16, filter: "Nearest" })),
    ).toEqual([16, 12]);
  });
});

// ---------------------------------------------------------------------------
// Tool 3 — cropper
// ---------------------------------------------------------------------------

describe("cropImage", () => {
  it("crops an explicit rectangle", () => {
    expect(dims(cropImage(gradientPng(), { x: 10, y: 10, width: 20, height: 15 }))).toEqual([
      20, 15,
    ]);
  });

  it("crops at a Center anchor", () => {
    expect(
      dims(cropImage(gradientPng(), { width: 20, height: 10, anchor: "Center" })),
    ).toEqual([20, 10]);
  });

  it("out-of-bounds crop is CROP_OUT_OF_BOUNDS", () => {
    expectDusrelError(
      () => cropImage(gradientPng(), { x: 60, y: 40, width: 20, height: 20 }),
      "CROP_OUT_OF_BOUNDS",
    );
  });
});

// ---------------------------------------------------------------------------
// Tool 4 — converter
// ---------------------------------------------------------------------------

describe("convertImage", () => {
  it.each([
    ["Jpeg", "jpeg"],
    ["WebP", "webp"],
    ["Bmp", "bmp"],
    ["Gif", "gif"],
    ["Png", "png"],
  ] as const)("converts PNG to %s", (format, expected) => {
    const out = convertImage(gradientPng(), { format });
    expect(getImageInfo(out).format).toBe(expected);
    expect(dims(out)).toEqual([64, 48]);
  });

  it("requires a target format", () => {
    expectDusrelError(
      () => convertImage(gradientPng(), {} as never),
      "INVALID_ARGUMENT",
    );
  });
});

// ---------------------------------------------------------------------------
// Tool 5 — watermark
// ---------------------------------------------------------------------------

describe("addWatermark", () => {
  it("stamps a text watermark, dimensions preserved", () => {
    const out = addWatermark(gradientPng(), {
      text: { text: "© dusrel", size: 20, colour: "#FFFFFF" },
      opacity: 0.5,
      position: "BottomRight",
    });
    expect(dims(out)).toEqual([64, 48]);
    expect(out.byteLength).toBeGreaterThan(0);
  });

  it("stamps a logo watermark", () => {
    const out = addWatermark(gradientPng(), {
      logo: { image: redPng(), scale: 0.5 },
      opacity: 0.8,
      position: "TopLeft",
    });
    expect(dims(out)).toEqual([64, 48]);
  });

  it("allows text and logo together (text first, then logo)", () => {
    const out = addWatermark(gradientPng(), {
      text: { text: "x" },
      logo: { image: redPng() },
    });
    expect(dims(out)).toEqual([64, 48]);
    expect(out.byteLength).toBeGreaterThan(0);
  });
});

// ---------------------------------------------------------------------------
// Tool 6 — annotator
// ---------------------------------------------------------------------------

describe("annotateImage", () => {
  it("draws arrow + rect + text, dimensions preserved", () => {
    const out = annotateImage(gradientPng(), [
      { type: "Arrow", from: [5, 5], to: [50, 30], colour: "#FF0000", width: 3 },
      { type: "Rect", x: 10, y: 10, w: 20, h: 15, colour: "#00FF00", width: 2 },
      { type: "Text", x: 5, y: 40, text: "Hi", size: 12, colour: "#0000FF" },
    ]);
    expect(dims(out)).toEqual([64, 48]);
  });

  it("an empty annotation list is INVALID_ARGUMENT", () => {
    expectDusrelError(() => annotateImage(gradientPng(), []), "INVALID_ARGUMENT");
  });
});

// ---------------------------------------------------------------------------
// Tool 7 — colour picker
// ---------------------------------------------------------------------------

describe("pickColour", () => {
  it("reads the solid red pixel", () => {
    const c = pickColour(redPng(), 0, 0);
    expect(c.hex).toBe("#FF0000");
    expect([c.r, c.g, c.b, c.a]).toEqual([255, 0, 0, 255]);
    expect(c.hsl[0]).toBeCloseTo(0, 5);
  });

  it("out-of-bounds pixel is PIXEL_OUT_OF_BOUNDS", () => {
    expectDusrelError(() => pickColour(redPng(), 10, 0), "PIXEL_OUT_OF_BOUNDS");
  });
});

// ---------------------------------------------------------------------------
// Tool 8 — palette
// ---------------------------------------------------------------------------

describe("extractPalette", () => {
  it("a solid image has one colour with share ~1", () => {
    const palette = extractPalette(redPng(), 1);
    expect(palette).toHaveLength(1);
    expect(palette[0].hex).toBe("#FF0000");
    expect(palette[0].share).toBeCloseTo(1, 5);
  });

  it("a gradient has several colours, shares sum to ~1", () => {
    const palette = extractPalette(gradientPng(), 4);
    expect(palette.length).toBeGreaterThan(1);
    const total = palette.reduce((sum, entry) => sum + entry.share, 0);
    expect(total).toBeCloseTo(1, 2);
  });

  it("rejects count outside 1-16", () => {
    expectDusrelError(() => extractPalette(redPng(), 0), "INVALID_ARGUMENT");
    expectDusrelError(() => extractPalette(redPng(), 17), "INVALID_ARGUMENT");
  });
});

// ---------------------------------------------------------------------------
// Tool 9 — collage
// ---------------------------------------------------------------------------

describe("makeCollage", () => {
  it("builds a 2x2 collage at the requested size", () => {
    const out = makeCollage([redPng(), gradientPng(), noisePng(), redPng()], {
      layout: "2x2",
      gap: 4,
      width: 200,
      height: 200,
    });
    expect(dims(out)).toEqual([200, 200]);
  });

  it("row layout works", () => {
    const out = makeCollage([redPng(), gradientPng()], {
      layout: "row",
      width: 200,
      height: 100,
    });
    expect(dims(out)).toEqual([200, 100]);
  });

  it("needs at least one input", () => {
    expectDusrelError(() => makeCollage([], {}), "EMPTY_INPUT");
  });
});

// ---------------------------------------------------------------------------
// Tool 10 — pixelator
// ---------------------------------------------------------------------------

describe("pixelateImage", () => {
  it("pixelates the whole image by default", () => {
    expect(dims(pixelateImage(gradientPng(), { blockSize: 8 }))).toEqual([64, 48]);
  });

  it("pixelates a rectangle", () => {
    const out = pixelateImage(gradientPng(), {
      x: 0,
      y: 0,
      width: 32,
      height: 24,
      blockSize: 4,
    });
    expect(dims(out)).toEqual([64, 48]);
  });
});

// ---------------------------------------------------------------------------
// Helpers: getImageInfo + bulkProcess
// ---------------------------------------------------------------------------

describe("getImageInfo", () => {
  it("describes a PNG", () => {
    const info = getImageInfo(gradientPng());
    expect(info.format).toBe("png");
    expect(info.width).toBe(64);
    expect(info.height).toBe(48);
    expect(info.hasAlpha).toBe(false);
    expect(info.fileSize).toBe(gradientPng().byteLength);
  });

  it("a JPEG converted from PNG reports jpeg", () => {
    const jpg = convertImage(gradientPng(), { format: "Jpeg" });
    expect(getImageInfo(jpg).format).toBe("jpeg");
  });
});

describe("bulkProcess", () => {
  it("one result per input, same order, partial failure tolerated", () => {
    const results = bulkProcess([gradientPng(), garbage(), redPng()], "compress", {
      quality: 70,
    });
    expect(results).toHaveLength(3);
    expect(results[0].ok).toBe(true);
    expect(results[0].output).toBeInstanceOf(Uint8Array);
    expect(results[1].ok).toBe(false);
    expect(results[1].error?.code).toBe("INVALID_IMAGE");
    expect(results[2].ok).toBe(true);
  });

  it("compress_to_size via bulk honours the target", () => {
    const results = bulkProcess([noisePng()], "compress_to_size", {
      targetBytes: 100_000,
      quality: 90,
      format: "Jpeg",
    });
    expect(results[0].ok).toBe(true);
    expect(results[0].output!.byteLength).toBeLessThanOrEqual(100_000);
  });

  it("pick_colour via bulk returns a colour per item", () => {
    const results = bulkProcess([redPng(), gradientPng()], "pick_colour", {
      x: 0,
      y: 0,
    });
    expect(results[0].ok).toBe(true);
    expect(results[0].colour?.hex).toBe("#FF0000");
    expect(results[1].ok).toBe(true);
    expect(results[1].colour?.hex).toBe("#000080");
  });

  it("palette via bulk returns palettes per item", () => {
    const results = bulkProcess([redPng()], "palette", { count: 1 });
    expect(results[0].ok).toBe(true);
    expect(results[0].palette).toHaveLength(1);
    expect(results[0].palette![0].hex).toBe("#FF0000");
  });

  it("an empty input list returns [] (core semantics)", () => {
    expect(bulkProcess([], "compress", {})).toEqual([]);
  });

  it("collage returns per-item INVALID_ARGUMENT, it does not throw", () => {
    const results = bulkProcess([redPng()], "collage", {});
    expect(results).toHaveLength(1);
    expect(results[0].ok).toBe(false);
    expect(results[0].error?.code).toBe("INVALID_ARGUMENT");
  });

  it("an unknown op is INVALID_ARGUMENT", () => {
    expectDusrelError(
      () => bulkProcess([redPng()], "no_such_op" as never, {}),
      "INVALID_ARGUMENT",
    );
  });
});

// ---------------------------------------------------------------------------
// Errors — input validation
// ---------------------------------------------------------------------------

describe("input validation", () => {
  it("garbage bytes are INVALID_IMAGE", () => {
    expectDusrelError(() => getImageInfo(garbage()), "INVALID_IMAGE");
  });

  it("empty bytes are EMPTY_INPUT", () => {
    expectDusrelError(
      () => getImageInfo(new Uint8Array(0)),
      "EMPTY_INPUT",
    );
  });

  it("non-Uint8Array input is INVALID_TYPE", () => {
    expectDusrelError(
      () => getImageInfo("not bytes" as never),
      "INVALID_TYPE",
    );
    expectDusrelError(() => compressImage(null as never), "INVALID_TYPE");
  });
});

// ---------------------------------------------------------------------------
// Zod schemas
// ---------------------------------------------------------------------------

describe("zod schemas", () => {
  it("accepts valid compress options", () => {
    expect(
      compressOptionsSchema.parse({ quality: 80, format: "Jpeg" }),
    ).toEqual({ quality: 80, format: "Jpeg" });
  });

  it("rejects quality outside 1-100", () => {
    expect(() => compressOptionsSchema.parse({ quality: 0 })).toThrow();
    expect(() => compressOptionsSchema.parse({ quality: 101 })).toThrow();
  });

  it("accepts a valid annotation, rejects a bad colour", () => {
    expect(
      annotationSchema.parse({
        type: "Arrow",
        from: [50, 50],
        to: [300, 200],
        colour: "#ff0000",
        width: 4,
      }),
    ).toBeTruthy();
    expect(() =>
      annotationSchema.parse({
        type: "Arrow",
        from: [50, 50],
        to: [300, 200],
        colour: "red",
        width: 4,
      }),
    ).toThrow();
  });

  it("accepts image bytes, rejects strings", () => {
    expect(imageBytesSchema.parse(redPng())).toBeInstanceOf(Uint8Array);
    expect(() => imageBytesSchema.parse("nope")).toThrow();
  });

  it("pixelate options validate the block-size range", () => {
    expect(pixelateOptionsSchema.parse({ blockSize: 16 }).blockSize).toBe(16);
    expect(() => pixelateOptionsSchema.parse({ blockSize: 1 })).toThrow();
    expect(() => pixelateOptionsSchema.parse({ blockSize: 101 })).toThrow();
  });
});

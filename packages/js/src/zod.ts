/**
 * dusrelimage — Zod schemas.
 *
 * Optional integration: `zod` is a dependency of this package.
 * Importing this module requires zod to be installed; the main entry
 * point (`index.ts`) never imports zod.
 *
 * The schemas mirror the TS option interfaces in `index.ts` (which use
 * camelCase keys; the wrapper translates them to the Rust snake_case
 * JSON keys). They check shape only — image validity and value ranges
 * like quality 1–100 or opacity 0–1 are enforced by the core and
 * reported as `DusrelError`s, not Zod errors.
 */
import { z } from "zod";

export const imageBytesSchema = z.instanceof(Uint8Array, {
  message: "Input must be a Uint8Array. / Input Uint8Array hona chahiye.",
});

export const imageArraySchema = z.array(imageBytesSchema);

export const imageFormatSchema = z.enum([
  "Jpeg",
  "Png",
  "WebP",
  "Avif",
  "Gif",
  "Bmp",
]);

export const fitModeSchema = z.enum(["Fit", "Fill", "Stretch", "Crop"]);

export const filterSchema = z.enum([
  "Nearest",
  "Triangle",
  "CatmullRom",
  "Gaussian",
  "Lanczos3",
]);

export const positionSchema = z.enum([
  "TopLeft",
  "TopCenter",
  "TopRight",
  "CenterLeft",
  "Center",
  "CenterRight",
  "BottomLeft",
  "BottomCenter",
  "BottomRight",
  "Tiled",
]);

export const collageLayoutSchema = z.union([
  z
    .string()
    .regex(
      /^[1-4]x[1-4]$/,
      'Collage layout must be a "WxH" grid like "2x2", "grid", "row" or "column". / Collage layout "2x2" jaisa WxH grid, "grid", "row" ya "column" hona chahiye.',
    ),
  z.enum(["grid", "row", "column"]),
  z.object({
    rows: z.number().int(),
    cols: z.number().int(),
  }),
]);

const hexColour = z
  .string()
  .regex(
    /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/,
    'Colour must be a "#RRGGBB" hex string. / Colour "#RRGGBB" hex string hona chahiye.',
  );

/** A colour as a hex string or a full `{ hex, r, g, b, a, hsl }` object. */
export const colourSchema = z.union([
  hexColour,
  z.object({
    hex: z.string(),
    r: z.number().int().min(0).max(255),
    g: z.number().int().min(0).max(255),
    b: z.number().int().min(0).max(255),
    a: z.number().int().min(0).max(255),
    hsl: z.tuple([z.number(), z.number(), z.number()]),
  }),
]);

/** A colour as returned by `pickColour` / `extractPalette`. */
export const colourObjectSchema = z.object({
  hex: z.string(),
  r: z.number().int().min(0).max(255),
  g: z.number().int().min(0).max(255),
  b: z.number().int().min(0).max(255),
  a: z.number().int().min(0).max(255),
  hsl: z.tuple([z.number(), z.number(), z.number()]),
});

export const imageInfoSchema = z.object({
  format: z.string(),
  width: z.number().int().nonnegative(),
  height: z.number().int().nonnegative(),
  hasAlpha: z.boolean(),
  colourType: z.string(),
  fileSize: z.number().int().nonnegative(),
});

export const paletteColourSchema = colourObjectSchema.extend({
  share: z.number().min(0).max(1),
});

export const commonOptionsSchema = z
  .object({
    autoOrient: z.boolean(),
    keepExif: z.boolean(),
    dpi: z.tuple([z.number().int().positive(), z.number().int().positive()]).nullable(),
    maxPixels: z.number().int().nonnegative(),
    maxDimension: z.number().int().nonnegative(),
  })
  .partial();

export const compressOptionsSchema = commonOptionsSchema
  .extend({
  quality: z.number().int().min(1).max(100),
  format: imageFormatSchema.nullable(),
  progressive: z.boolean(),
  lossless: z.boolean(),
  effort: z.number().int().min(0).max(6),
}).partial();

export const resizeOptionsSchema = commonOptionsSchema
  .extend({
  width: z.number().int().positive().nullable(),
  height: z.number().int().positive().nullable(),
  percent: z.number().positive().nullable(),
  fit: fitModeSchema,
  filter: filterSchema,
  background: colourSchema,
}).partial();

export const cropOptionsSchema = commonOptionsSchema.extend({
  x: z.number().int().nonnegative().optional(),
  y: z.number().int().nonnegative().optional(),
  width: z.number().int().positive(),
  height: z.number().int().positive(),
  anchor: positionSchema.nullable().optional(),
});

export const convertOptionsSchema = commonOptionsSchema.extend({
  format: imageFormatSchema,
  quality: z.number().int().min(1).max(100).optional(),
  background: colourSchema.optional(),
  progressive: z.boolean().optional(),
});

export const textWatermarkSchema = z.object({
  text: z.string(),
  size: z.number().positive(),
  colour: colourSchema,
});

export const imageWatermarkSchema = z.object({
  image: imageBytesSchema,
  scale: z.number().positive(),
});

export const watermarkOptionsSchema = commonOptionsSchema
  .extend({
  text: textWatermarkSchema.nullable(),
  logo: imageWatermarkSchema.nullable(),
  opacity: z.number().min(0).max(1),
  position: positionSchema,
  rotationDeg: z.number(),
  margin: z.number().int().nonnegative(),
}).partial();

export const annotationSchema = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("Arrow"),
    from: z.tuple([z.number().int().nonnegative(), z.number().int().nonnegative()]),
    to: z.tuple([z.number().int().nonnegative(), z.number().int().nonnegative()]),
    colour: colourSchema,
    width: z.number().int().nonnegative(),
  }),
  z.object({
    type: z.literal("Rect"),
    x: z.number().int().nonnegative(),
    y: z.number().int().nonnegative(),
    w: z.number().int().nonnegative(),
    h: z.number().int().nonnegative(),
    colour: colourSchema,
    width: z.number().int().nonnegative(),
    fill: colourSchema.nullable().optional(),
  }),
  z.object({
    type: z.literal("Ellipse"),
    x: z.number().int().nonnegative(),
    y: z.number().int().nonnegative(),
    w: z.number().int().nonnegative(),
    h: z.number().int().nonnegative(),
    colour: colourSchema,
    width: z.number().int().nonnegative(),
    fill: colourSchema.nullable().optional(),
  }),
  z.object({
    type: z.literal("Line"),
    from: z.tuple([z.number().int().nonnegative(), z.number().int().nonnegative()]),
    to: z.tuple([z.number().int().nonnegative(), z.number().int().nonnegative()]),
    colour: colourSchema,
    width: z.number().int().nonnegative(),
  }),
  z.object({
    type: z.literal("Text"),
    x: z.number().int().nonnegative(),
    y: z.number().int().nonnegative(),
    text: z.string(),
    size: z.number().positive(),
    colour: colourSchema,
  }),
]);

export const collageOptionsSchema = commonOptionsSchema
  .extend({
  layout: collageLayoutSchema,
  gap: z.number().int().nonnegative(),
  background: colourSchema,
  width: z.number().int().positive(),
  height: z.number().int().positive(),
  cellFit: fitModeSchema,
}).partial();

export const pixelateOptionsSchema = commonOptionsSchema
  .extend({
  x: z.number().int().nonnegative(),
  y: z.number().int().nonnegative(),
  width: z.number().int().nonnegative(),
  height: z.number().int().nonnegative(),
  blockSize: z.number().int().min(2).max(100),
}).partial();

export const bulkOpSchema = z.enum([
  "compress",
  "compress_image",
  "compress_to_size",
  "resize",
  "resize_image",
  "crop",
  "crop_image",
  "convert",
  "convert_image",
  "watermark",
  "add_watermark",
  "annotate",
  "annotate_image",
  "pick_colour",
  "palette",
  "pixelate",
  "pixelate_image",
]);

export const bulkResultSchema = z.object({
  index: z.number().int().nonnegative(),
  ok: z.boolean(),
  output: imageBytesSchema.optional(),
  colour: colourObjectSchema.optional(),
  palette: z.array(paletteColourSchema).optional(),
  error: z
    .object({
      code: z.string(),
      messageEn: z.string(),
      messageHi: z.string(),
    })
    .optional(),
});

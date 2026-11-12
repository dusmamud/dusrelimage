/* dusrelimage.h — C API for dusrelimage v0.1.0 (offline image toolkit).
 *
 * This header matches the exported symbols of `libdusrelimage`
 * (`crates/dusrelimage-ffi`, crate-type cdylib/staticlib) exactly.
 * Every function is `dusrelimage_`-prefixed; all image work is done by
 * the same Rust core the CLI / Python / WASM bindings use, so results
 * agree across languages.
 *
 * Status codes (return value of every function except the three
 * diagnostics at the bottom):
 *   0  OK
 *   1  INVALID_IMAGE            input bytes are not a recognisable image
 *   2  UNSUPPORTED_FORMAT       format known but not supported by this build
 *   3  IMAGE_TOO_LARGE          exceeds max_pixels / max_dimension
 *   4  TARGET_SIZE_UNREACHABLE  even the lowest quality exceeds the target
 *   5  CROP_OUT_OF_BOUNDS       crop rectangle sticks out of the image
 *   6  PIXEL_OUT_OF_BOUNDS      pixel coordinate is outside the image
 *   7  INVALID_QUALITY          quality outside 1-100
 *   8  INVALID_DIMENSIONS       a dimension is zero, missing or unusable
 *   9  INVALID_ARGUMENT         bad JSON, bad value, NULL pointer, ...
 *   10 EMPTY_INPUT              empty input where input is required
 *   11 INVALID_TYPE             wrong-typed value reached a typed API
 *   99 UNKNOWN                   anything else (including an internal error)
 *
 * On failure, call `dusrelimage_last_error()` for the JSON error:
 *   {"code": "...", "message_en": "...", "message_hi": "..."}
 *
 * Memory ownership: on success, `*out` / `*out_json` points at a heap
 * buffer of `*out_len` bytes owned by the caller. Free it exactly once
 * with `dusrelimage_free_buffer(ptr, len)`. JSON outputs are UTF-8,
 * `*out_len` bytes long, and NOT NUL-terminated — use the length.
 * `dusrelimage_last_error()` and `dusrelimage_version()` return
 * NUL-terminated C strings owned by the library; do NOT free them.
 *
 * Options JSON: every `options_json` parameter is a JSON object using
 * the Rust Options structs' serde snake_case field names (see
 * `crates/dusrelimage/src/options.rs`). NULL means "all defaults".
 * Partial objects are deep-merged over the defaults, so
 * `{"quality": 70}` is a complete CompressOptions. Flat CommonOptions
 * fields (`max_pixels`, `max_dimension`, `auto_orient`, `keep_exif`,
 * `dpi`) may sit at the top level, or nested under `"common"`.
 * Human-friendly shorthands are accepted: hex-string colours
 * (`"#ff0000"`) in `colour` / `background` / `fill` fields, collage
 * layout strings (`"2x2"`, `"grid"`, `"row"`, `"column"`), and a
 * base64 string for a logo watermark's `image` bytes. Invalid JSON or
 * wrong-typed values return 9 (INVALID_ARGUMENT).
 *
 * Threading: the last error is thread-local; a call on one thread
 * never overwrites another thread's error. The image functions are
 * safe to call concurrently from multiple threads.
 */

#ifndef DUSRELIMAGE_H
#define DUSRELIMAGE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Status codes */
#define DUSRELIMAGE_OK 0
#define DUSRELIMAGE_INVALID_IMAGE 1
#define DUSRELIMAGE_UNSUPPORTED_FORMAT 2
#define DUSRELIMAGE_IMAGE_TOO_LARGE 3
#define DUSRELIMAGE_TARGET_SIZE_UNREACHABLE 4
#define DUSRELIMAGE_CROP_OUT_OF_BOUNDS 5
#define DUSRELIMAGE_PIXEL_OUT_OF_BOUNDS 6
#define DUSRELIMAGE_INVALID_QUALITY 7
#define DUSRELIMAGE_INVALID_DIMENSIONS 8
#define DUSRELIMAGE_INVALID_ARGUMENT 9
#define DUSRELIMAGE_EMPTY_INPUT 10
#define DUSRELIMAGE_INVALID_TYPE 11
#define DUSRELIMAGE_UNKNOWN 99

/* Tool 1 — re-encode at a fixed quality (and `format`, or the input
 * format when unset). `options_json` is a CompressOptions JSON object
 * (fields: common, quality 1-100, format, progressive, lossless,
 * effort), or NULL for all defaults (quality 80). */
int32_t dusrelimage_compress(const uint8_t *input, size_t input_len,
                             const char *options_json,
                             uint8_t **out, size_t *out_len);

/* Tool 1 — re-encode so the result is at most `target_bytes` bytes
 * (iterative quality search; status 4 when unreachable). Same options
 * as dusrelimage_compress. */
int32_t dusrelimage_compress_to_size(const uint8_t *input, size_t input_len,
                                     uint64_t target_bytes,
                                     const char *options_json,
                                     uint8_t **out, size_t *out_len);

/* Tool 2 — resize. `options_json` is a ResizeOptions JSON object
 * (fields: common, width, height, percent, fit, filter, background),
 * or NULL. At least one of width / height / percent is required. */
int32_t dusrelimage_resize(const uint8_t *input, size_t input_len,
                           const char *options_json,
                           uint8_t **out, size_t *out_len);

/* Tool 3 — crop to a rectangle (or an anchor preset). `options_json`
 * is a CropOptions JSON object (fields: common, x, y, width, height,
 * anchor), or NULL. */
int32_t dusrelimage_crop(const uint8_t *input, size_t input_len,
                         const char *options_json,
                         uint8_t **out, size_t *out_len);

/* Tool 4 — convert to another format. `options_json` is a
 * ConvertOptions JSON object (fields: common, format, quality,
 * background, progressive), or NULL (defaults to JPEG). */
int32_t dusrelimage_convert(const uint8_t *input, size_t input_len,
                            const char *options_json,
                            uint8_t **out, size_t *out_len);

/* Tool 5 — stamp a text or logo watermark. `options_json` is a
 * WatermarkOptions JSON object (fields: common, text, logo, opacity,
 * position, rotation_deg, margin), or NULL. `text` is
 * {"text": "...", "size": 48.0, "colour": "#000000"}; `logo` is
 * {"image": "<base64>" or [byte, ...], "scale": 1.0}. */
int32_t dusrelimage_add_watermark(const uint8_t *input, size_t input_len,
                                  const char *options_json,
                                  uint8_t **out, size_t *out_len);

/* Tool 6 — draw annotations. `annotations_json` is a JSON array of
 * annotations in serde externally-tagged form, e.g.
 *   [{"Rect": {"x": 10, "y": 10, "w": 40, "h": 30,
 *              "colour": "#FF0000", "width": 2, "fill": "#FF0000"}}]
 * or in {"type": "Rect", ...} form. Variants: Arrow {from, to, colour,
 * width}, Rect {x, y, w, h, colour, width, fill}, Ellipse {x, y, w, h,
 * colour, width, fill}, Line {from, to, colour, width},
 * Text {x, y, text, size, colour}. Coordinates are absolute pixels. */
int32_t dusrelimage_annotate(const uint8_t *input, size_t input_len,
                             const char *annotations_json,
                             uint8_t **out, size_t *out_len);

/* Tool 9 — collage from several images. `options_json` is a
 * CollageOptions JSON object (fields: common, layout, gap, background,
 * width, height, cell_fit), or NULL (2x2 grid, 1200x1200). `layout`
 * is "Row", "Column", or {"Grid": {"rows": 2, "cols": 2}} (the
 * shorthands "2x2", "grid", "row", "column" are also accepted). */
int32_t dusrelimage_make_collage(const uint8_t *const *inputs,
                                 const size_t *input_lens, size_t count,
                                 const char *options_json,
                                 uint8_t **out, size_t *out_len);

/* Tool 10 — pixelate a rectangle (whole image when width/height are
 * 0). `options_json` is a PixelateOptions JSON object (fields: common,
 * x, y, width, height, block_size 2-100), or NULL. */
int32_t dusrelimage_pixelate(const uint8_t *input, size_t input_len,
                             const char *options_json,
                             uint8_t **out, size_t *out_len);

/* Bulk — run one operation over many inputs; one result per input,
 * same order, never aborting the batch. `op` is a short name
 * ("compress", "compress_to_size", "resize", "crop", "convert",
 * "watermark", "annotate", "pick_colour", "palette", "pixelate",
 * "collage") or the full snake_case name ("compress_image", ...).
 * `options_json` carries the op's options; "compress_to_size" also
 * reads "target_bytes" from it, "annotate" reads "annotations",
 * "pick_colour" reads "x"/"y", "palette" reads "count". A zero count
 * returns "[]" with status 0. "collage" is accepted but reported per
 * item as INVALID_ARGUMENT (call dusrelimage_make_collage instead).
 *
 * The JSON array holds one object per input:
 *   {"index": 0, "ok": true,  "output": "<base64>"}            image ops
 *   {"index": 0, "ok": true,  "colour": {"hex": ..., ...}}     pick_colour
 *   {"index": 0, "ok": true,  "palette": [{"hex": ..., "share": ...}]}
 *   {"index": 0, "ok": false, "error": {"code": ..., "message_en": ...,
 *                                       "message_hi": ...}} */
int32_t dusrelimage_bulk_process(const uint8_t *const *inputs,
                                 const size_t *input_lens, size_t count,
                                 const char *op, const char *options_json,
                                 uint8_t **out_json, size_t *out_len);

/* Tool 7 — colour of one pixel, as JSON:
 *   {"hex": "#RRGGBB", "r": 0-255, "g": 0-255, "b": 0-255,
 *    "a": 0-255, "hsl": [h, s, l]} */
int32_t dusrelimage_pick_colour(const uint8_t *input, size_t input_len,
                                uint32_t x, uint32_t y,
                                uint8_t **out_json, size_t *out_len);

/* Tool 8 — dominant colours as a JSON array of PaletteColour, most
 * frequent first:
 *   [{"colour": {"hex": ..., "r": ..., "g": ..., "b": ..., "a": ...,
 *                "hsl": [...]}, "share": 0.0-1.0}, ...]
 * `count` is 1-16. */
int32_t dusrelimage_extract_palette(const uint8_t *input, size_t input_len,
                                    uint8_t count,
                                    uint8_t **out_json, size_t *out_len);

/* Image summary as JSON:
 *   {"format": "png", "width": 64, "height": 48, "has_alpha": false,
 *    "colour_type": "rgb8", "file_size": 1234}
 * `options_json` is a CommonOptions JSON object (nested `common`
 * object and/or flat common fields, e.g. {"max_dimension": 10}),
 * or NULL for all defaults — same convention as the other ops. */
int32_t dusrelimage_get_image_info(const uint8_t *input, size_t input_len,
                                   const char *options_json,
                                   uint8_t **out_json, size_t *out_len);

/* The last error on this thread, as JSON:
 *   {"code": "...", "message_en": "...", "message_hi": "..."}
 * Returns NULL when the last call on this thread succeeded (or no FFI
 * call has failed yet). Do NOT free the pointer; it stays valid until
 * the next FFI call on the same thread. */
const char *dusrelimage_last_error(void);

/* Free a buffer returned by any *out / *out_json parameter: exactly
 * the pointer and length that call returned, exactly once. A NULL
 * pointer or a zero length is a no-op. */
void dusrelimage_free_buffer(uint8_t *ptr, size_t len);

/* Library version, e.g. "0.1.0". Do NOT free the pointer. */
const char *dusrelimage_version(void);

#ifdef __cplusplus
}
#endif

#endif /* DUSRELIMAGE_H */

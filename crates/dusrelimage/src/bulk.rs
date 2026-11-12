//! Bulk processing (SPEC section 1): one op over many inputs.
//!
//! `bulk_process` runs the same [`BulkOp`] over every input and
//! returns one [`BulkItemResult`] per input, in the same order —
//! partial failures are reported per item and never abort the batch.
//!
//! Signature note: the SPEC sketch is `bulk_process(inputs, op,
//! options)`; here the op selector and its options are one enum,
//! [`BulkOp`], so the call is `bulk_process(inputs, &BulkOp::...)`.
//! Each variant carries the options struct of its tool (or, for the
//! non-image tools, its parameters), so the ten tools map 1:1:
//!
//! * `Compress` / `Resize` / `Crop` / `Convert` / `Watermark` /
//!   `Annotate` / `Pixelate` — per-item image output
//!   ([`BulkOutput::Image`]).
//! * `PickColour { x, y }` — per-item [`BulkOutput::Colour`].
//! * `Palette { count }` — per-item [`BulkOutput::Palette`].
//! * `Collage` — does NOT fit bulk: a collage combines *all* inputs
//!   into one image, so there is no per-item result. It is present in
//!   the enum so the ten tools map 1:1, but `bulk_process` returns
//!   `INVALID_ARGUMENT` for every item, telling the caller to use
//!   [`crate::make_collage`] once with the full list instead.

use crate::error::{DusrelError, Result};
use crate::ops_watermark::add_watermark;
use crate::options::{
    CollageOptions, CompressOptions, ConvertOptions, CropOptions, PixelateOptions, ResizeOptions,
    WatermarkOptions,
};
use crate::types::{Annotation, Colour, PaletteColour};
use crate::{
    annotate_image, compress_image, convert_image, crop_image, extract_palette, pick_colour,
    pixelate_image, resize_image,
};

/// One of the ten tools, with its options, for [`bulk_process`].
#[derive(Debug, Clone, PartialEq)]
pub enum BulkOp {
    /// Tool 1 — compress each input.
    Compress(CompressOptions),
    /// Tool 2 — resize each input.
    Resize(ResizeOptions),
    /// Tool 3 — crop each input.
    Crop(CropOptions),
    /// Tool 4 — convert each input.
    Convert(ConvertOptions),
    /// Tool 5 — watermark each input.
    Watermark(WatermarkOptions),
    /// Tool 6 — draw these annotations on each input.
    Annotate(Vec<Annotation>),
    /// Tool 7 — pick the colour at `(x, y)` of each input.
    PickColour { x: u32, y: u32 },
    /// Tool 8 — extract the `count` dominant colours of each input.
    Palette { count: u8 },
    /// Tool 9 — collage. Not a per-item op: see the module docs.
    Collage(CollageOptions),
    /// Tool 10 — pixelate each input.
    Pixelate(PixelateOptions),
}

/// The per-item output of [`bulk_process`]: an image, a single colour,
/// or a palette, depending on the op.
#[derive(Debug, Clone, PartialEq)]
pub enum BulkOutput {
    /// Encoded image bytes (tools 1-6, 10).
    Image(Vec<u8>),
    /// One picked colour (tool 7).
    Colour(Colour),
    /// Dominant colours, most frequent first (tool 8).
    Palette(Vec<PaletteColour>),
}

/// One item of a [`bulk_process`] result: the input's `index` and its
/// `result` (`Ok` output or the per-item [`DusrelError`]).
#[derive(Debug)]
pub struct BulkItemResult {
    pub index: usize,
    pub result: Result<BulkOutput>,
}

impl BulkItemResult {
    /// True when this item succeeded.
    pub fn is_ok(&self) -> bool {
        self.result.is_ok()
    }

    /// True when this item failed.
    pub fn is_err(&self) -> bool {
        self.result.is_err()
    }
}

/// Run `op` over every input; one [`BulkItemResult`] per input, same
/// order. A failing item never aborts the batch.
pub fn bulk_process(inputs: &[Vec<u8>], op: &BulkOp) -> Vec<BulkItemResult> {
    inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            let result = match op {
                BulkOp::Compress(options) => {
                    compress_image(input, options).map(BulkOutput::Image)
                }
                BulkOp::Resize(options) => resize_image(input, options).map(BulkOutput::Image),
                BulkOp::Crop(options) => crop_image(input, options).map(BulkOutput::Image),
                BulkOp::Convert(options) => {
                    convert_image(input, options).map(BulkOutput::Image)
                }
                BulkOp::Watermark(options) => {
                    add_watermark(input, options).map(BulkOutput::Image)
                }
                BulkOp::Annotate(annotations) => {
                    annotate_image(input, annotations).map(BulkOutput::Image)
                }
                BulkOp::PickColour { x, y } => {
                    pick_colour(input, *x, *y).map(BulkOutput::Colour)
                }
                BulkOp::Palette { count } => {
                    extract_palette(input, *count).map(BulkOutput::Palette)
                }
                BulkOp::Collage(_) => Err(DusrelError::invalid_argument(
                    "Collage combines all inputs into one image, so it cannot run per item. Call make_collage once with the full list instead.",
                    "Collage saari images ko ek me jodta hai, isliye ye per-item nahi chal sakta. Poori list ke saath make_collage ko ek baar call karein.",
                )),
                BulkOp::Pixelate(options) => {
                    pixelate_image(input, options).map(BulkOutput::Image)
                }
            };
            BulkItemResult { index, result }
        })
        .collect()
}

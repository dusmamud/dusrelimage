package dusrelimage

import (
	"fmt"
)

// All C calls live in ffi.go; this file only holds the public API and
// the pure-Go option resolution / validation around it.

// resolve returns opts when non-nil, otherwise the defaults. Every
// public function takes a nil-able options pointer; nil means "all
// defaults" and is passed to the FFI layer as NULL.
func resolveCompress(opts *CompressOptions) CompressOptions {
	if opts != nil {
		return *opts
	}
	return DefaultCompressOptions()
}

func resolveResize(opts *ResizeOptions) ResizeOptions {
	if opts != nil {
		return *opts
	}
	return DefaultResizeOptions()
}

func resolveCrop(opts *CropOptions) CropOptions {
	if opts != nil {
		return *opts
	}
	return DefaultCropOptions()
}

func resolveConvert(opts *ConvertOptions) ConvertOptions {
	if opts != nil {
		return *opts
	}
	return DefaultConvertOptions()
}

func resolveWatermark(opts *WatermarkOptions) WatermarkOptions {
	if opts != nil {
		return *opts
	}
	return DefaultWatermarkOptions()
}

func resolveCollage(opts *CollageOptions) CollageOptions {
	if opts != nil {
		return *opts
	}
	return DefaultCollageOptions()
}

func resolvePixelate(opts *PixelateOptions) PixelateOptions {
	if opts != nil {
		return *opts
	}
	return DefaultPixelateOptions()
}

// assertImageBytes mirrors the other bindings' input rule: only
// non-empty byte slices are accepted, and an empty input is
// EMPTY_INPUT (never a native crash or a bare error).
func assertImageBytes(input []byte) error {
	if len(input) == 0 {
		return emptyInputError("Image bytes")
	}
	return nil
}

// Compress re-encodes an image at a fixed quality (and Format, or the
// input format when unset).
func Compress(input []byte, opts *CompressOptions) ([]byte, error) {
	if err := assertImageBytes(input); err != nil {
		return nil, err
	}
	o := resolveCompress(opts)
	if err := o.validate(); err != nil {
		return nil, err
	}
	cOpts, err := cOptionsJSON(o)
	if err != nil {
		return nil, err
	}
	defer freeCString(cOpts)
	return cCompress(input, cOpts)
}

// CompressToSize re-encodes an image so the result is at most
// targetBytes bytes, at the highest quality that fits. It fails with
// TARGET_SIZE_UNREACHABLE when even the lowest quality is too big.
// The quality search is JPEG/AVIF-only — other formats take one
// best-effort encode.
func CompressToSize(input []byte, targetBytes uint64, opts *CompressOptions) ([]byte, error) {
	if err := assertImageBytes(input); err != nil {
		return nil, err
	}
	o := resolveCompress(opts)
	if err := o.validate(); err != nil {
		return nil, err
	}
	cOpts, err := cOptionsJSON(o)
	if err != nil {
		return nil, err
	}
	defer freeCString(cOpts)
	return cCompressToSize(input, targetBytes, cOpts)
}

// Resize resizes an image (width and/or height, percent, fit, filter).
// At least one of width / height / percent is required; percent
// cannot be combined with width/height.
func Resize(input []byte, opts *ResizeOptions) ([]byte, error) {
	if err := assertImageBytes(input); err != nil {
		return nil, err
	}
	o := resolveResize(opts)
	cOpts, err := cOptionsJSON(o)
	if err != nil {
		return nil, err
	}
	defer freeCString(cOpts)
	return cResize(input, cOpts)
}

// Crop crops an image to a rectangle (or an anchor preset). A zero
// width or height is INVALID_DIMENSIONS; a rectangle sticking out of
// the image is CROP_OUT_OF_BOUNDS.
func Crop(input []byte, opts *CropOptions) ([]byte, error) {
	if err := assertImageBytes(input); err != nil {
		return nil, err
	}
	o := resolveCrop(opts)
	if err := o.validate(); err != nil {
		return nil, err
	}
	cOpts, err := cOptionsJSON(o)
	if err != nil {
		return nil, err
	}
	defer freeCString(cOpts)
	return cCrop(input, cOpts)
}

// Convert converts an image to another format. Alpha is flattened
// onto Background when the target has none (JPEG/BMP).
func Convert(input []byte, opts *ConvertOptions) ([]byte, error) {
	if err := assertImageBytes(input); err != nil {
		return nil, err
	}
	o := resolveConvert(opts)
	if err := o.validate(); err != nil {
		return nil, err
	}
	cOpts, err := cOptionsJSON(o)
	if err != nil {
		return nil, err
	}
	defer freeCString(cOpts)
	return cConvert(input, cOpts)
}

// AddWatermark stamps a text and/or logo watermark. Text and logo may
// be combined (text first, then logo); at least one is required
// (otherwise INVALID_ARGUMENT from the core). Defaults: opacity 0.5,
// position BottomRight, rotation 0, margin 16.
func AddWatermark(input []byte, opts *WatermarkOptions) ([]byte, error) {
	if err := assertImageBytes(input); err != nil {
		return nil, err
	}
	o := resolveWatermark(opts)
	if err := o.validate(); err != nil {
		return nil, err
	}
	cOpts, err := cOptionsJSON(o)
	if err != nil {
		return nil, err
	}
	defer freeCString(cOpts)
	return cAddWatermark(input, cOpts)
}

// Annotate draws annotations (arrows, rects, ellipses, lines, text)
// on an image. An empty list is INVALID_ARGUMENT.
func Annotate(input []byte, annotations []Annotation) ([]byte, error) {
	if err := assertImageBytes(input); err != nil {
		return nil, err
	}
	if len(annotations) == 0 {
		return nil, invalidArgumentError(
			"No annotations given: pass at least one annotation.",
			"Koi annotation nahi diya: kam se kam ek annotation dein.")
	}
	raw, err := marshalAnnotations(annotations)
	if err != nil {
		return nil, invalidArgumentError(
			"Could not serialise annotations to JSON: "+err.Error(),
			"Annotations JSON me nahi badle ja sake.")
	}
	return cAnnotate(input, string(raw))
}

// PickColour reads the colour of one pixel. Out-of-bounds is
// PIXEL_OUT_OF_BOUNDS.
func PickColour(input []byte, x, y uint32) (Colour, error) {
	var colour Colour
	if err := assertImageBytes(input); err != nil {
		return colour, err
	}
	raw, err := cPickColour(input, x, y)
	if err := callJSON(raw, err, &colour); err != nil {
		return colour, err
	}
	return colour, nil
}

// ExtractPalette extracts the dominant colours, most frequent first
// (count 1-16).
func ExtractPalette(input []byte, count uint8) ([]PaletteColour, error) {
	var palette []PaletteColour
	if err := assertImageBytes(input); err != nil {
		return nil, err
	}
	if count < 1 || count > 16 {
		return nil, invalidArgumentError(
			fmt.Sprintf("Palette count %d is invalid. Count must be between 1 and 16.", count),
			fmt.Sprintf("Palette count %d galat hai. Count 1 se 16 ke beech hona chahiye.", count))
	}
	raw, err := cExtractPalette(input, count)
	if err := callJSON(raw, err, &palette); err != nil {
		return nil, err
	}
	return palette, nil
}

// MakeCollage builds a collage from several images (grid, row or
// column). Output is always PNG. An empty input list is EMPTY_INPUT.
func MakeCollage(inputs [][]byte, opts *CollageOptions) ([]byte, error) {
	if len(inputs) == 0 {
		return nil, emptyInputError("collage inputs")
	}
	for _, in := range inputs {
		if err := assertImageBytes(in); err != nil {
			return nil, err
		}
	}
	o := resolveCollage(opts)
	cOpts, err := cOptionsJSON(o)
	if err != nil {
		return nil, err
	}
	defer freeCString(cOpts)
	return cMakeCollage(inputs, cOpts)
}

// Pixelate pixelates a rectangle (the whole image when Width/Height
// are 0). BlockSize is 2-100.
func Pixelate(input []byte, opts *PixelateOptions) ([]byte, error) {
	if err := assertImageBytes(input); err != nil {
		return nil, err
	}
	o := resolvePixelate(opts)
	if err := o.validate(); err != nil {
		return nil, err
	}
	cOpts, err := cOptionsJSON(o)
	if err != nil {
		return nil, err
	}
	defer freeCString(cOpts)
	return cPixelate(input, cOpts)
}

// resolveCommon returns opts when non-nil, otherwise the defaults.
func resolveCommon(opts *CommonOptions) CommonOptions {
	if opts != nil {
		return *opts
	}
	return DefaultCommonOptions()
}

// GetImageInfo reads an image's summary without modifying it. The
// CommonOptions limits (MaxPixels / MaxDimension) are honoured by the
// core through the FFI layer's 5-arg get_image_info; a nil opts means
// all defaults.
func GetImageInfo(input []byte, opts *CommonOptions) (ImageInfo, error) {
	var info ImageInfo
	if err := assertImageBytes(input); err != nil {
		return info, err
	}
	cOpts, err := cOptionsJSON(resolveCommon(opts))
	if err != nil {
		return info, err
	}
	defer freeCString(cOpts)
	raw, err := cGetImageInfo(input, cOpts)
	if err := callJSON(raw, err, &info); err != nil {
		return info, err
	}
	return info, nil
}

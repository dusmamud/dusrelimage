package dusrelimage

import "fmt"

// Options structs — the pro control layer (SPEC section 2), mirroring
// crates/dusrelimage/src/options.rs field for field. JSON tags are the
// Rust serde snake_case names; the FFI layer deep-merges a partial
// object over the Rust defaults, but this package always sends
// complete objects (start from a Default*Options() constructor, as
// the JS wrapper does), so what the core sees is exactly what the
// caller set.
//
// Every op's options struct embeds Common as a named field (the Rust
// structs embed `common: CommonOptions`); flat CommonOptions fields at
// the top level of an options JSON object are also accepted by the FFI
// layer, but this package always sends the nested "common" object.

// CommonOptions are shared by every tool.
type CommonOptions struct {
	// Apply EXIF orientation on load (default true).
	AutoOrient bool `json:"auto_orient"`
	// Keep EXIF / ICC / XMP on output (default false = strip).
	// v0.1.0: accepted but not honoured — output is always stripped.
	KeepExif bool `json:"keep_exif"`
	// DPI to write on JPEG/PNG output, where the encoder supports it.
	// v0.1.0: accepted but not embedded.
	DPI *[2]uint32 `json:"dpi"`
	// Refuse images with more pixels than this (default 120,000,000).
	MaxPixels uint64 `json:"max_pixels"`
	// Refuse images with a side longer than this (default 16000).
	MaxDimension uint32 `json:"max_dimension"`
}

// DefaultCommonOptions returns the Rust CommonOptions defaults.
func DefaultCommonOptions() CommonOptions {
	return CommonOptions{
		AutoOrient:   true,
		KeepExif:     false,
		DPI:          nil,
		MaxPixels:    120_000_000,
		MaxDimension: 16000,
	}
}

// CompressOptions are the options for Compress / CompressToSize.
type CompressOptions struct {
	Common CommonOptions `json:"common"`
	// Quality 1-100 (default 80). JPEG/WebP/AVIF use it directly;
	// PNG maps it to a compression level.
	Quality uint8 `json:"quality"`
	// Output format; nil keeps the input format (TIFF input falls
	// back to PNG, which is the closest lossless output).
	Format *ImageFormat `json:"format"`
	// Progressive JPEG / interlaced PNG (default false).
	// v0.1.0: accepted but not honoured.
	Progressive bool `json:"progressive"`
	// Lossless output (WebP/PNG only; any other format + true is
	// INVALID_ARGUMENT).
	Lossless bool `json:"lossless"`
	// Encoding effort 0-6 (default 4; AVIF speed = 10 - effort).
	Effort uint8 `json:"effort"`
}

// DefaultCompressOptions returns the Rust CompressOptions defaults.
func DefaultCompressOptions() CompressOptions {
	return CompressOptions{
		Common:      DefaultCommonOptions(),
		Quality:     80,
		Format:      nil,
		Progressive: false,
		Lossless:    false,
		Effort:      4,
	}
}

func (o CompressOptions) validate() error {
	if o.Quality < 1 || o.Quality > 100 {
		return invalidQualityError(int(o.Quality))
	}
	if o.Effort > 6 {
		return invalidArgumentError(
			"Effort is invalid. Effort must be between 0 and 6.",
			"Effort galat hai. Effort 0 se 6 ke beech hona chahiye.")
	}
	return nil
}

// ResizeOptions are the options for Resize.
type ResizeOptions struct {
	Common CommonOptions `json:"common"`
	// Target width in px. With Height unset, height follows the
	// aspect ratio.
	Width *uint32 `json:"width"`
	// Target height in px. With Width unset, width follows the
	// aspect ratio.
	Height *uint32 `json:"height"`
	// Scale in percent (100.0 = original size, 50.0 = half).
	// Cannot be combined with Width/Height.
	Percent *float32 `json:"percent"`
	// How the image fits the target box (default Fit).
	Fit FitMode `json:"fit"`
	// Resampling filter (default Lanczos3).
	Filter Filter `json:"filter"`
	// Canvas colour used by FitMode Fill (letterbox).
	Background Colour `json:"background"`
}

// DefaultResizeOptions returns the Rust ResizeOptions defaults.
func DefaultResizeOptions() ResizeOptions {
	return ResizeOptions{
		Common:     DefaultCommonOptions(),
		Width:      nil,
		Height:     nil,
		Percent:    nil,
		Fit:        FitFit,
		Filter:     FilterLanczos3,
		Background: ColourWhite(),
	}
}

// CropOptions are the options for Crop.
type CropOptions struct {
	Common CommonOptions `json:"common"`
	// Left edge in px (used when Anchor is nil).
	X uint32 `json:"x"`
	// Top edge in px (used when Anchor is nil).
	Y uint32 `json:"y"`
	// Crop width in px (must be > 0).
	Width uint32 `json:"width"`
	// Crop height in px (must be > 0).
	Height uint32 `json:"height"`
	// Anchor preset: when non-nil, the Width x Height rectangle is
	// placed at the anchor and X/Y are ignored. Position Tiled is
	// treated as Center for cropping. Nil (default) uses X/Y.
	Anchor *Position `json:"anchor"`
}

// DefaultCropOptions returns the Rust CropOptions defaults (a zero
// rectangle — callers must set Width and Height, or use
// NewCropOptions / NewAnchoredCropOptions).
func DefaultCropOptions() CropOptions {
	return CropOptions{
		Common: DefaultCommonOptions(),
		X:      0,
		Y:      0,
		Width:  0,
		Height: 0,
		Anchor: nil,
	}
}

// NewCropOptions builds an explicit-rectangle CropOptions.
func NewCropOptions(x, y, width, height uint32) CropOptions {
	o := DefaultCropOptions()
	o.X, o.Y, o.Width, o.Height = x, y, width, height
	return o
}

// NewAnchoredCropOptions builds a Width x Height rectangle placed at
// an anchor preset.
func NewAnchoredCropOptions(anchor Position, width, height uint32) CropOptions {
	o := DefaultCropOptions()
	o.Anchor = &anchor
	o.Width, o.Height = width, height
	return o
}

func (o CropOptions) validate() error {
	if o.Width == 0 || o.Height == 0 {
		return invalidDimensionsError(
			"crop width and height must be greater than 0",
			"crop width aur height 0 se badi honi chahiye")
	}
	return nil
}

// ConvertOptions are the options for Convert.
type ConvertOptions struct {
	Common CommonOptions `json:"common"`
	// Target format (required; the Default is JPEG — callers should
	// always set it explicitly).
	Format ImageFormat `json:"format"`
	// Quality 1-100 (default 80; JPEG/AVIF direct, PNG level-mapped).
	Quality uint8 `json:"quality"`
	// Background used to flatten alpha when the target has none
	// (JPEG/BMP). Default white.
	Background Colour `json:"background"`
	// Progressive JPEG / interlaced PNG (default false).
	// v0.1.0: accepted but not honoured.
	Progressive bool `json:"progressive"`
}

// DefaultConvertOptions returns the Rust ConvertOptions defaults.
func DefaultConvertOptions() ConvertOptions {
	return ConvertOptions{
		Common:      DefaultCommonOptions(),
		Format:      FormatJPEG,
		Quality:     80,
		Background:  ColourWhite(),
		Progressive: false,
	}
}

func (o ConvertOptions) validate() error {
	if o.Quality < 1 || o.Quality > 100 {
		return invalidQualityError(int(o.Quality))
	}
	return nil
}

// WatermarkOptions are the options for AddWatermark.
type WatermarkOptions struct {
	Common CommonOptions `json:"common"`
	// Text watermark, if any.
	Text *TextWatermark `json:"text"`
	// Logo watermark (image bytes + scale), if any.
	Logo *ImageWatermark `json:"logo"`
	// Opacity 0-1 (default 0.5).
	Opacity float32 `json:"opacity"`
	// Anchor position, incl. Position Tiled for a repeated pattern
	// (default BottomRight).
	Position Position `json:"position"`
	// Rotation in degrees (default 0.0).
	RotationDeg float32 `json:"rotation_deg"`
	// Margin from the image edge in px (default 16).
	Margin uint32 `json:"margin"`
}

// DefaultWatermarkOptions returns the Rust WatermarkOptions defaults.
func DefaultWatermarkOptions() WatermarkOptions {
	return WatermarkOptions{
		Common:      DefaultCommonOptions(),
		Text:        nil,
		Logo:        nil,
		Opacity:     0.5,
		Position:    PositionBottomRight,
		RotationDeg: 0.0,
		Margin:      16,
	}
}

func (o WatermarkOptions) validate() error {
	if o.Opacity < 0 || o.Opacity > 1 {
		return invalidArgumentError(
			"Opacity must be between 0.0 and 1.0.",
			"Opacity 0.0 se 1.0 ke beech honi chahiye.")
	}
	return nil
}

// CollageOptions are the options for MakeCollage.
type CollageOptions struct {
	Common CommonOptions `json:"common"`
	// Layout (default 2x2 grid).
	Layout CollageLayout `json:"layout"`
	// Gap between cells in px (default 8).
	Gap uint32 `json:"gap"`
	// Background colour (default white).
	Background Colour `json:"background"`
	// Output width in px (default 1200).
	Width uint32 `json:"width"`
	// Output height in px (default 1200).
	Height uint32 `json:"height"`
	// Per-cell fit mode (default Fit).
	CellFit FitMode `json:"cell_fit"`
}

// DefaultCollageOptions returns the Rust CollageOptions defaults.
func DefaultCollageOptions() CollageOptions {
	return CollageOptions{
		Common:     DefaultCommonOptions(),
		Layout:     DefaultCollageLayout(),
		Gap:        8,
		Background: ColourWhite(),
		Width:      1200,
		Height:     1200,
		CellFit:    FitFit,
	}
}

// PixelateOptions are the options for Pixelate.
type PixelateOptions struct {
	Common CommonOptions `json:"common"`
	// Left edge in px.
	X uint32 `json:"x"`
	// Top edge in px.
	Y uint32 `json:"y"`
	// Rectangle width in px (0 = to the image edge).
	Width uint32 `json:"width"`
	// Rectangle height in px (0 = to the image edge).
	Height uint32 `json:"height"`
	// Block size in px, 2-100 (default 10).
	BlockSize uint32 `json:"block_size"`
}

// DefaultPixelateOptions returns the Rust PixelateOptions defaults
// (all zeros = pixelate the whole image with 10px blocks).
func DefaultPixelateOptions() PixelateOptions {
	return PixelateOptions{
		Common:    DefaultCommonOptions(),
		X:         0,
		Y:         0,
		Width:     0,
		Height:    0,
		BlockSize: 10,
	}
}

func (o PixelateOptions) validate() error {
	if o.BlockSize < 2 || o.BlockSize > 100 {
		return invalidArgumentError(
			fmt.Sprintf("Block size %d is invalid. Block size must be between 2 and 100.", o.BlockSize),
			fmt.Sprintf("Block size %d galat hai. Block size 2 se 100 ke beech hona chahiye.", o.BlockSize))
	}
	return nil
}

package dusrelimage

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"math"
	"strconv"
	"strings"
)

// ---------------------------------------------------------------------------
// Formats, fit modes, filters, positions
// ---------------------------------------------------------------------------

// ImageFormat is an output image format. The JSON spelling is the Rust
// serde variant name — case matters ("Jpeg", not "jpeg").
type ImageFormat string

const (
	FormatJPEG ImageFormat = "Jpeg"
	FormatPNG  ImageFormat = "Png"
	FormatWebP ImageFormat = "WebP"
	FormatAVIF ImageFormat = "Avif"
	FormatGIF  ImageFormat = "Gif"
	FormatBMP  ImageFormat = "Bmp"
)

// FitMode says how a resize fits the image into the target box.
type FitMode string

const (
	FitFit     FitMode = "Fit"
	FitFill    FitMode = "Fill"
	FitStretch FitMode = "Stretch"
	FitCrop    FitMode = "Crop"
)

// Filter is a resampling filter for resize.
type Filter string

const (
	FilterNearest    Filter = "Nearest"
	FilterTriangle   Filter = "Triangle"
	FilterCatmullRom Filter = "CatmullRom"
	FilterGaussian   Filter = "Gaussian"
	FilterLanczos3   Filter = "Lanczos3"
)

// Position is an anchor position: 9 anchors + Tiled (watermark only).
type Position string

const (
	PositionTopLeft      Position = "TopLeft"
	PositionTopCenter    Position = "TopCenter"
	PositionTopRight     Position = "TopRight"
	PositionCenterLeft   Position = "CenterLeft"
	PositionCenter       Position = "Center"
	PositionCenterRight  Position = "CenterRight"
	PositionBottomLeft   Position = "BottomLeft"
	PositionBottomCenter Position = "BottomCenter"
	PositionBottomRight  Position = "BottomRight"
	PositionTiled        Position = "Tiled"
)

// ---------------------------------------------------------------------------
// Colour
// ---------------------------------------------------------------------------

// Colour is a colour: 8-bit RGBA + hex string + HSL (h in degrees
// 0..360, s/l in 0..1). Hex is always "#RRGGBB" derived from r, g, b;
// alpha is NOT in the hex string.
type Colour struct {
	Hex string     `json:"hex"`
	R   uint8      `json:"r"`
	G   uint8      `json:"g"`
	B   uint8      `json:"b"`
	A   uint8      `json:"a"`
	HSL [3]float32 `json:"hsl"`
}

// rgbToHSL mirrors the Rust Colour::rgb_to_hsl exactly (h degrees
// 0..360, s/l 0..1).
func rgbToHSL(r, g, b uint8) [3]float32 {
	rf := float64(r) / 255
	gf := float64(g) / 255
	bf := float64(b) / 255
	max := math.Max(rf, math.Max(gf, bf))
	min := math.Min(rf, math.Min(gf, bf))
	l := (max + min) / 2
	if math.Abs(max-min) < 1e-9 {
		return [3]float32{0, 0, float32(l)}
	}
	d := max - min
	var s float64
	if l > 0.5 {
		s = d / (2 - max - min)
	} else {
		s = d / (max + min)
	}
	var h float64
	switch {
	case math.Abs(max-rf) < 1e-9:
		h = 60 * math.Mod((gf-bf)/d, 6)
		// Rust uses rem_euclid(6) here, which is always >= 0;
		// math.Mod can go negative, so normalise.
		if h < 0 {
			h += 360
		}
	case math.Abs(max-gf) < 1e-9:
		h = 60 * ((bf-rf)/d + 2)
	default:
		h = 60 * ((rf-gf)/d + 4)
	}
	h = math.Mod(h, 360)
	if h < 0 {
		h += 360
	}
	return [3]float32{float32(h), float32(s), float32(l)}
}

// ColourFromRGB builds a Colour from 8-bit RGB (alpha 255).
func ColourFromRGB(r, g, b uint8) Colour {
	return ColourFromRGBA(r, g, b, 255)
}

// ColourFromRGBA builds a Colour from 8-bit RGBA.
func ColourFromRGBA(r, g, b, a uint8) Colour {
	return Colour{
		Hex: fmt.Sprintf("#%02X%02X%02X", r, g, b),
		R:   r,
		G:   g,
		B:   b,
		A:   a,
		HSL: rgbToHSL(r, g, b),
	}
}

// ColourWhite is white (#FFFFFF).
func ColourWhite() Colour { return ColourFromRGB(255, 255, 255) }

// ColourBlack is black (#000000).
func ColourBlack() Colour { return ColourFromRGB(0, 0, 0) }

// ColourFromHex parses "#RRGGBB", "RRGGBB", "#RGB" or "#RRGGBBAA",
// mirroring Rust Colour::from_hex. Anything else is INVALID_ARGUMENT.
func ColourFromHex(hex string) (Colour, error) {
	h := strings.TrimPrefix(hex, "#")
	bad := func() (Colour, error) {
		return Colour{}, invalidArgumentError(
			fmt.Sprintf("Invalid hex colour %q. Use \"#RRGGBB\".", hex),
			fmt.Sprintf("Galat hex colour %q hai. \"#RRGGBB\" use karein.", hex))
	}
	hexVal := func(s string) (uint64, bool) {
		v, err := strconv.ParseUint(s, 16, 8)
		if err != nil {
			return 0, false
		}
		return v, true
	}
	nibble := func(i int) (uint8, bool) {
		if i >= len(h) {
			return 0, false
		}
		v, ok := hexVal(h[i : i+1])
		return uint8(v), ok
	}
	pair := func(i int) (uint8, bool) {
		if i+2 > len(h) {
			return 0, false
		}
		v, ok := hexVal(h[i : i+2])
		return uint8(v), ok
	}
	var r, g, b, a uint8
	a = 255
	switch len(h) {
	case 3:
		var ok bool
		var n0, n1, n2 uint8
		if n0, ok = nibble(0); !ok {
			return bad()
		}
		if n1, ok = nibble(1); !ok {
			return bad()
		}
		if n2, ok = nibble(2); !ok {
			return bad()
		}
		r, g, b = n0*17, n1*17, n2*17
	case 6:
		var ok bool
		if r, ok = pair(0); !ok {
			return bad()
		}
		if g, ok = pair(2); !ok {
			return bad()
		}
		if b, ok = pair(4); !ok {
			return bad()
		}
	case 8:
		var ok bool
		if r, ok = pair(0); !ok {
			return bad()
		}
		if g, ok = pair(2); !ok {
			return bad()
		}
		if b, ok = pair(4); !ok {
			return bad()
		}
		if a, ok = pair(6); !ok {
			return bad()
		}
	default:
		return bad()
	}
	return ColourFromRGBA(r, g, b, a), nil
}

// MustColourFromHex is ColourFromHex for constants and tests: it
// panics on an invalid hex string (only ever pass a literal you know
// is valid).
func MustColourFromHex(hex string) Colour {
	c, err := ColourFromHex(hex)
	if err != nil {
		panic(err)
	}
	return c
}

// ---------------------------------------------------------------------------
// Image info / palette
// ---------------------------------------------------------------------------

// ImageInfo is the summary of an image, from GetImageInfo. Format is
// a lowercase short name ("jpeg", "png", "webp", "avif", "gif", "bmp",
// "tiff"). ColourType is the lowercase image-crate colour type
// ("rgb8", "rgba8", ...). FileSize is the input byte length.
type ImageInfo struct {
	Format     string `json:"format"`
	Width      uint32 `json:"width"`
	Height     uint32 `json:"height"`
	HasAlpha   bool   `json:"has_alpha"`
	ColourType string `json:"colour_type"`
	FileSize   uint64 `json:"file_size"`
}

// PaletteColour is one dominant colour and its share of the sampled
// pixels (0..1), most frequent first.
type PaletteColour struct {
	Colour Colour  `json:"colour"`
	Share  float32 `json:"share"`
}

// UnmarshalJSON accepts both the Rust serde shape
// ({"colour": {...}, "share": 0.5}, as dusrelimage_extract_palette
// returns) and the flattened shape ({"hex": ..., "r": ..., ...,
// "share": 0.5}, as dusrelimage_bulk_process returns for palettes).
func (p *PaletteColour) UnmarshalJSON(data []byte) error {
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	if colourRaw, ok := raw["colour"]; ok {
		if err := json.Unmarshal(colourRaw, &p.Colour); err != nil {
			return err
		}
	} else {
		// Flattened: the colour fields sit at the top level.
		if err := json.Unmarshal(data, &p.Colour); err != nil {
			return err
		}
	}
	if shareRaw, ok := raw["share"]; ok {
		if err := json.Unmarshal(shareRaw, &p.Share); err != nil {
			return err
		}
	}
	return nil
}

// ---------------------------------------------------------------------------
// Collage layout
// ---------------------------------------------------------------------------

// CollageLayout is a collage layout: a rows x cols grid (1..4 each),
// one row, or one column. "masonry" is out of scope for v0.1.0.
type CollageLayout struct {
	Kind string // "Grid", "Row" or "Column"
	Rows uint32 // grid only
	Cols uint32 // grid only
}

// GridLayout returns a rows x cols grid layout.
func GridLayout(rows, cols uint32) CollageLayout {
	return CollageLayout{Kind: "Grid", Rows: rows, Cols: cols}
}

// RowLayout is one row, one column per input image.
var RowLayout = CollageLayout{Kind: "Row"}

// ColumnLayout is one column, one row per input image.
var ColumnLayout = CollageLayout{Kind: "Column"}

// DefaultCollageLayout is the 2x2 grid.
func DefaultCollageLayout() CollageLayout { return GridLayout(2, 2) }

// MarshalJSON emits the Rust serde shape: {"Grid": {"rows": 2,
// "cols": 2}} for grids, "Row" / "Column" for the named layouts.
// Structs (not maps) keep the key order rows-then-cols, matching
// the serde output exactly — encoding/json sorts map keys, which
// would emit cols before rows.
func (l CollageLayout) MarshalJSON() ([]byte, error) {
	switch l.Kind {
	case "Row":
		return []byte(`"Row"`), nil
	case "Column":
		return []byte(`"Column"`), nil
	default:
		type grid struct {
			Rows uint32 `json:"rows"`
			Cols uint32 `json:"cols"`
		}
		type gridWrap struct {
			Grid grid `json:"Grid"`
		}
		return json.Marshal(gridWrap{Grid: grid{Rows: l.Rows, Cols: l.Cols}})
	}
}

// UnmarshalJSON accepts the serde shape and the "WxH" / "row" /
// "column" / "grid" shorthands the FFI layer also accepts.
func (l *CollageLayout) UnmarshalJSON(data []byte) error {
	var s string
	if err := json.Unmarshal(data, &s); err == nil {
		switch strings.ToLower(strings.TrimSpace(s)) {
		case "row":
			*l = RowLayout
			return nil
		case "column":
			*l = ColumnLayout
			return nil
		case "grid":
			*l = DefaultCollageLayout()
			return nil
		default:
			var rows, cols uint32
			if _, err := fmt.Sscanf(strings.ToLower(s), "%dx%d", &rows, &cols); err == nil {
				*l = GridLayout(rows, cols)
				return nil
			}
			return fmt.Errorf("unknown collage layout %q", s)
		}
	}
	var grid struct {
		Grid struct {
			Rows uint32 `json:"rows"`
			Cols uint32 `json:"cols"`
		} `json:"Grid"`
	}
	if err := json.Unmarshal(data, &grid); err != nil {
		return err
	}
	*l = GridLayout(grid.Grid.Rows, grid.Grid.Cols)
	return nil
}

// ---------------------------------------------------------------------------
// Watermarks
// ---------------------------------------------------------------------------

// TextWatermark is a text watermark: string, size in px, colour.
type TextWatermark struct {
	Text   string  `json:"text"`
	Size   float32 `json:"size"`
	Colour Colour  `json:"colour"`
}

// ImageWatermark is a logo watermark: raw image bytes + a scale factor
// (1.0 = natural size, 0.5 = half size).
type ImageWatermark struct {
	Image []byte  `json:"image"`
	Scale float32 `json:"scale"`
}

// MarshalJSON emits the image bytes as a base64 string, which the FFI
// layer accepts for a logo watermark (a JSON number array is also
// accepted by the FFI layer). Standard base64 with padding, matching
// the FFI layer's decoder.
func (w ImageWatermark) MarshalJSON() ([]byte, error) {
	type watermarkJSON struct {
		Image string  `json:"image"`
		Scale float32 `json:"scale"`
	}
	return json.Marshal(watermarkJSON{
		Image: base64.StdEncoding.EncodeToString(w.Image),
		Scale: w.Scale,
	})
}

// ---------------------------------------------------------------------------
// Annotations
// ---------------------------------------------------------------------------

// Annotation is one annotation shape (all coordinates are absolute
// pixels). Use one of the concrete types below.
type Annotation interface {
	annotationKind() string
}

// ArrowAnnotation draws an arrow from From to To.
type ArrowAnnotation struct {
	From   [2]uint32 `json:"from"`
	To     [2]uint32 `json:"to"`
	Colour Colour    `json:"colour"`
	Width  uint32    `json:"width"`
}

func (ArrowAnnotation) annotationKind() string { return "Arrow" }

// RectAnnotation draws a rectangle; Fill nil means outline-only.
type RectAnnotation struct {
	X      uint32  `json:"x"`
	Y      uint32  `json:"y"`
	W      uint32  `json:"w"`
	H      uint32  `json:"h"`
	Colour Colour  `json:"colour"`
	Width  uint32  `json:"width"`
	Fill   *Colour `json:"fill"`
}

func (RectAnnotation) annotationKind() string { return "Rect" }

// EllipseAnnotation draws an ellipse; Fill nil means outline-only.
type EllipseAnnotation struct {
	X      uint32  `json:"x"`
	Y      uint32  `json:"y"`
	W      uint32  `json:"w"`
	H      uint32  `json:"h"`
	Colour Colour  `json:"colour"`
	Width  uint32  `json:"width"`
	Fill   *Colour `json:"fill"`
}

func (EllipseAnnotation) annotationKind() string { return "Ellipse" }

// LineAnnotation draws a line from From to To.
type LineAnnotation struct {
	From   [2]uint32 `json:"from"`
	To     [2]uint32 `json:"to"`
	Colour Colour    `json:"colour"`
	Width  uint32    `json:"width"`
}

func (LineAnnotation) annotationKind() string { return "Line" }

// TextAnnotation draws text at X, Y.
type TextAnnotation struct {
	X      uint32  `json:"x"`
	Y      uint32  `json:"y"`
	Text   string  `json:"text"`
	Size   float32 `json:"size"`
	Colour Colour  `json:"colour"`
}

func (TextAnnotation) annotationKind() string { return "Text" }

// marshalAnnotations renders annotations as the JSON array the FFI
// layer expects, in serde externally-tagged form:
// [{"Rect": {"x": 10, ...}}, ...].
func marshalAnnotations(annotations []Annotation) ([]byte, error) {
	if annotations == nil {
		return []byte("[]"), nil
	}
	parts := make([]json.RawMessage, 0, len(annotations))
	for _, a := range annotations {
		fields, err := json.Marshal(a)
		if err != nil {
			return nil, err
		}
		tagged, err := json.Marshal(map[string]json.RawMessage{
			a.annotationKind(): fields,
		})
		if err != nil {
			return nil, err
		}
		parts = append(parts, tagged)
	}
	return json.Marshal(parts)
}

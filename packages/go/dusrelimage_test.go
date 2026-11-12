package dusrelimage

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
)

func TestDefaultOptions(t *testing.T) {
	common := DefaultCommonOptions()
	if !common.AutoOrient || common.KeepExif || common.DPI != nil ||
		common.MaxPixels != 120_000_000 || common.MaxDimension != 16000 {
		t.Fatalf("DefaultCommonOptions = %+v", common)
	}

	compress := DefaultCompressOptions()
	if compress.Quality != 80 || compress.Format != nil || compress.Progressive ||
		compress.Lossless || compress.Effort != 4 {
		t.Fatalf("DefaultCompressOptions = %+v", compress)
	}

	resize := DefaultResizeOptions()
	if resize.Width != nil || resize.Height != nil || resize.Percent != nil ||
		resize.Fit != FitFit || resize.Filter != FilterLanczos3 ||
		resize.Background.Hex != "#FFFFFF" {
		t.Fatalf("DefaultResizeOptions = %+v", resize)
	}

	crop := DefaultCropOptions()
	if crop.Width != 0 || crop.Height != 0 || crop.Anchor != nil {
		t.Fatalf("DefaultCropOptions = %+v", crop)
	}

	convert := DefaultConvertOptions()
	if convert.Format != FormatJPEG || convert.Quality != 80 ||
		convert.Background.Hex != "#FFFFFF" {
		t.Fatalf("DefaultConvertOptions = %+v", convert)
	}

	watermark := DefaultWatermarkOptions()
	if watermark.Text != nil || watermark.Logo != nil || watermark.Opacity != 0.5 ||
		watermark.Position != PositionBottomRight || watermark.RotationDeg != 0 ||
		watermark.Margin != 16 {
		t.Fatalf("DefaultWatermarkOptions = %+v", watermark)
	}

	collage := DefaultCollageOptions()
	if collage.Layout.Kind != "Grid" || collage.Layout.Rows != 2 || collage.Layout.Cols != 2 ||
		collage.Gap != 8 || collage.Width != 1200 || collage.Height != 1200 ||
		collage.CellFit != FitFit {
		t.Fatalf("DefaultCollageOptions = %+v", collage)
	}

	pixelate := DefaultPixelateOptions()
	if pixelate.BlockSize != 10 || pixelate.Width != 0 || pixelate.Height != 0 {
		t.Fatalf("DefaultPixelateOptions = %+v", pixelate)
	}
}

func TestCropOptionHelpers(t *testing.T) {
	o := NewCropOptions(10, 20, 30, 40)
	if o.X != 10 || o.Y != 20 || o.Width != 30 || o.Height != 40 || o.Anchor != nil {
		t.Fatalf("NewCropOptions = %+v", o)
	}
	a := NewAnchoredCropOptions(PositionCenter, 30, 40)
	if a.Anchor == nil || *a.Anchor != PositionCenter || a.Width != 30 || a.Height != 40 {
		t.Fatalf("NewAnchoredCropOptions = %+v", a)
	}
}

func TestColourFromHex(t *testing.T) {
	cases := []struct {
		in      string
		r, g, b uint8
		a       uint8
		hex     string
	}{
		{"#FF0000", 255, 0, 0, 255, "#FF0000"},
		{"FF0000", 255, 0, 0, 255, "#FF0000"},
		{"#fff", 255, 255, 255, 255, "#FFFFFF"},
		{"#0A141E", 10, 20, 30, 255, "#0A141E"},
		{"#FF000080", 255, 0, 0, 128, "#FF0000"},
	}
	for _, c := range cases {
		got, err := ColourFromHex(c.in)
		if err != nil {
			t.Fatalf("ColourFromHex(%q): %v", c.in, err)
		}
		if got.R != c.r || got.G != c.g || got.B != c.b || got.A != c.a || got.Hex != c.hex {
			t.Fatalf("ColourFromHex(%q) = %+v", c.in, got)
		}
	}
	for _, bad := range []string{"", "#", "#GGGGGG", "#12345", "red", "#1234567"} {
		if _, err := ColourFromHex(bad); err == nil {
			t.Fatalf("ColourFromHex(%q) should fail", bad)
		} else {
			var de *DusrelError
			if !errors.As(err, &de) || de.Code != CodeInvalidArgument {
				t.Fatalf("ColourFromHex(%q) error = %v, want DusrelError INVALID_ARGUMENT", bad, err)
			}
		}
	}
}

func TestRGBToHSL(t *testing.T) {
	red := ColourFromRGB(255, 0, 0)
	if red.HSL != [3]float32{0, 1, 0.5} {
		t.Fatalf("red HSL = %v, want [0 1 0.5]", red.HSL)
	}
	white := ColourWhite()
	if white.HSL != [3]float32{0, 0, 1} {
		t.Fatalf("white HSL = %v, want [0 0 1]", white.HSL)
	}
	if got := ColourFromRGB(10, 20, 30).Hex; got != "#0A141E" {
		t.Fatalf("hex = %s, want #0A141E", got)
	}
}

func TestOptionsJSONShape(t *testing.T) {
	raw, err := json.Marshal(DefaultCompressOptions())
	if err != nil {
		t.Fatal(err)
	}
	var decoded map[string]any
	if err := json.Unmarshal(raw, &decoded); err != nil {
		t.Fatal(err)
	}
	common, ok := decoded["common"].(map[string]any)
	if !ok {
		t.Fatalf("compress JSON has no common object: %s", raw)
	}
	if common["auto_orient"] != true || common["max_pixels"] != float64(120_000_000) ||
		common["max_dimension"] != float64(16000) || common["dpi"] != nil {
		t.Fatalf("compress common = %v", common)
	}
	if decoded["quality"] != float64(80) || decoded["format"] != nil ||
		decoded["effort"] != float64(4) {
		t.Fatalf("compress JSON = %s", raw)
	}

	// Collage layout: serde externally-tagged form.
	raw, err = json.Marshal(GridLayout(1, 2))
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != `{"Grid":{"rows":1,"cols":2}}` {
		t.Fatalf("grid layout JSON = %s", raw)
	}
	raw, err = json.Marshal(RowLayout)
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != `"Row"` {
		t.Fatalf("row layout JSON = %s", raw)
	}

	// Annotations: serde externally-tagged form.
	red := MustColourFromHex("#FF0000")
	raw, err = marshalAnnotations([]Annotation{
		RectAnnotation{X: 10, Y: 10, W: 40, H: 30, Colour: red, Width: 2, Fill: &red},
		LineAnnotation{From: [2]uint32{0, 0}, To: [2]uint32{99, 99}, Colour: red, Width: 1},
	})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(raw), `"Rect":{"x":10`) ||
		!strings.Contains(string(raw), `"fill":{"hex":"#FF0000"`) ||
		!strings.Contains(string(raw), `"Line":{"from":[0,0]`) {
		t.Fatalf("annotations JSON = %s", raw)
	}

	// Logo watermark bytes go out as base64 (the FFI layer accepts
	// base64 for a logo image).
	raw, err = json.Marshal(ImageWatermark{Image: []byte{0, 1, 2, 3}, Scale: 1})
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != `{"image":"AAECAw==","scale":1}` {
		t.Fatalf("logo watermark JSON = %s", raw)
	}
}

func TestDusrelError(t *testing.T) {
	e := &DusrelError{Code: CodeInvalidImage, MessageEn: "Invalid image: x.", MessageHi: "Galat image hai: x."}
	if e.Error() != "INVALID_IMAGE: Invalid image: x." {
		t.Fatalf("Error() = %q", e.Error())
	}
	if e.ReasonEn() != e.MessageEn || e.ReasonHi() != e.MessageHi {
		t.Fatalf("reason aliases mismatch")
	}
	if got := statusCode(1); got != CodeInvalidImage {
		t.Fatalf("statusCode(1) = %q", got)
	}
	if got := statusCode(99); got != CodeUnknown {
		t.Fatalf("statusCode(99) = %q", got)
	}
	if got := statusCode(12345); got != CodeOperationFailed {
		t.Fatalf("statusCode(12345) = %q", got)
	}
}

func TestParseErrorJSON(t *testing.T) {
	parsed := parseErrorJSON(`{"code":"CROP_OUT_OF_BOUNDS","message_en":"Crop.","message_hi":"Crop."}`)
	if parsed == nil || parsed.Code != CodeCropOutOfBounds || parsed.MessageEn != "Crop." {
		t.Fatalf("parseErrorJSON = %+v", parsed)
	}
	// reason_* fallback (the WASM layer emits both spellings).
	parsed = parseErrorJSON(`{"code":"INVALID_IMAGE","reason_en":"Bad.","reason_hi":"Galat."}`)
	if parsed == nil || parsed.MessageEn != "Bad." || parsed.MessageHi != "Galat." {
		t.Fatalf("parseErrorJSON reason fallback = %+v", parsed)
	}
	if parseErrorJSON("") != nil || parseErrorJSON("not json") != nil ||
		parseErrorJSON(`{"no_code":true}`) != nil {
		t.Fatalf("parseErrorJSON should return nil for bad input")
	}
}

func TestClientSideValidation(t *testing.T) {
	// These checks run before any FFI call, so they must fail the
	// same way even if the native library were absent.
	img := []byte{0x89, 0x50, 0x4e, 0x47} // not a real image; validation precedes decode
	cases := []struct {
		name string
		call func() error
		code string
	}{
		{"compress quality 0", func() error {
			o := DefaultCompressOptions()
			o.Quality = 0
			_, err := Compress(img, &o)
			return err
		}, CodeInvalidQuality},
		{"compress quality 101", func() error {
			o := DefaultCompressOptions()
			o.Quality = 101
			_, err := Compress(img, &o)
			return err
		}, CodeInvalidQuality},
		{"convert quality 0", func() error {
			o := DefaultConvertOptions()
			o.Quality = 0
			_, err := Convert(img, &o)
			return err
		}, CodeInvalidQuality},
		{"crop zero width", func() error {
			_, err := Crop(img, &CropOptions{Width: 0, Height: 20})
			return err
		}, CodeInvalidDimensions},
		{"palette count 0", func() error {
			_, err := ExtractPalette(img, 0)
			return err
		}, CodeInvalidArgument},
		{"palette count 17", func() error {
			_, err := ExtractPalette(img, 17)
			return err
		}, CodeInvalidArgument},
		{"pixelate block 1", func() error {
			o := DefaultPixelateOptions()
			o.BlockSize = 1
			_, err := Pixelate(img, &o)
			return err
		}, CodeInvalidArgument},
		{"annotate empty", func() error {
			_, err := Annotate(img, nil)
			return err
		}, CodeInvalidArgument},
		{"empty input", func() error {
			_, err := Compress(nil, nil)
			return err
		}, CodeEmptyInput},
		{"collage empty list", func() error {
			_, err := MakeCollage(nil, nil)
			return err
		}, CodeEmptyInput},
		{"bulk unknown op", func() error {
			_, err := BulkProcess([][]byte{img}, BulkOp{Op: "nope"})
			return err
		}, CodeInvalidArgument},
		{"watermark opacity", func() error {
			o := DefaultWatermarkOptions()
			o.Opacity = 1.5
			_, err := AddWatermark(img, &o)
			return err
		}, CodeInvalidArgument},
	}
	for _, c := range cases {
		err := c.call()
		var de *DusrelError
		if !errors.As(err, &de) {
			t.Fatalf("%s: error = %v (%T), want *DusrelError", c.name, err, err)
		}
		if de.Code != c.code {
			t.Fatalf("%s: code = %s, want %s (%v)", c.name, de.Code, c.code, de)
		}
		if de.MessageEn == "" || de.MessageHi == "" {
			t.Fatalf("%s: empty message in %+v", c.name, de)
		}
	}
}

func TestBulkEmptyList(t *testing.T) {
	results, err := BulkProcess(nil, BulkCompress(nil))
	if err != nil {
		t.Fatal(err)
	}
	if len(results) != 0 {
		t.Fatalf("bulk empty list = %d results, want 0", len(results))
	}
}

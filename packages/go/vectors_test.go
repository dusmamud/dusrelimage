package dusrelimage

// Shared test vectors (SPEC section 6): Go runs shared/test-vectors.json
// and must agree with the Rust, Python and JS runners.
//
// Fixtures are built the same way the other bindings build them:
// `dusrelimage mock` kinds (solid/gradient/noise + seed) come from the
// CLI binary (built with `cargo build -p dusrelimage-cli`; see
// TestMain / cliPath), plus the documented derived fixtures
// (solid_rgb, transparent_rgba, two_colour, tiff, jpeg, avif, garbage,
// empty) built here with a minimal PNG/TIFF encoder — no binary blobs
// are committed.
//
// It FAILS LOUDLY on any case whose fixture/op kind it cannot handle:
// unknown fixture kinds, ops, bulk ops or expected keys fail the case
// by name instead of silently skipping it. Pixel checks decode PNG
// output with the standard library; non-PNG outputs fall back to
// PickColour per pixel (only the JPEG flatten case needs that).

import (
	"bytes"
	"compress/zlib"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"hash/crc32"
	"image/color"
	"image/png"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"testing"
)

// ---------------------------------------------------------------------------
// Loading the vectors
// ---------------------------------------------------------------------------

type vectorCase struct {
	ID          string                     `json:"id"`
	Op          string                     `json:"op"`
	BulkOp      string                     `json:"bulk_op"`
	Fixture     json.RawMessage            `json:"fixture"`
	Fixtures    []json.RawMessage          `json:"fixtures"`
	Options     json.RawMessage            `json:"options"`
	TargetBytes *uint64                    `json:"target_bytes"`
	Annotations json.RawMessage            `json:"annotations"`
	Expected    map[string]json.RawMessage `json:"expected"`
}

func loadVectors(t *testing.T) []vectorCase {
	t.Helper()
	path := os.Getenv("DUSRELIMAGE_VECTORS")
	if path == "" {
		_, thisFile, _, ok := runtime.Caller(0)
		if !ok {
			t.Fatal("runtime.Caller failed")
		}
		candidates := []string{
			filepath.Join(filepath.Dir(thisFile), "..", "..", "shared", "test-vectors.json"),
			filepath.Join("..", "..", "shared", "test-vectors.json"),
			filepath.Join("shared", "test-vectors.json"),
		}
		for _, c := range candidates {
			if _, err := os.Stat(c); err == nil {
				path = c
				break
			}
		}
	}
	if path == "" {
		t.Fatal("shared/test-vectors.json not found (set DUSRELIMAGE_VECTORS)")
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var cases []vectorCase
	if err := json.Unmarshal(raw, &cases); err != nil {
		t.Fatal(err)
	}
	return cases
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

type fixtureSpec struct {
	Kind        string `json:"kind"`
	Width       int    `json:"width"`
	Height      int    `json:"height"`
	Seed        uint64 `json:"seed"`
	RGB         [3]int `json:"rgb"`
	DerivedFrom string `json:"derived_from"`
}

var (
	fixtureCache   = map[string][]byte{}
	fixtureCacheMu sync.Mutex
)

// cliPath locates the `dusrelimage` CLI binary used for mock fixtures:
// $DUSRELIMAGE_CLI, else the release binary next to this repo, else
// PATH. It fails loudly when nothing is found — a missing CLI is a
// broken test setup, not a skippable case.
func cliPath(t *testing.T) string {
	t.Helper()
	if p := os.Getenv("DUSRELIMAGE_CLI"); p != "" {
		return p
	}
	_, thisFile, _, ok := runtime.Caller(0)
	if ok {
		p := filepath.Join(filepath.Dir(thisFile), "..", "..", "target", "release", "dusrelimage")
		if _, err := os.Stat(p); err == nil {
			return p
		}
	}
	if p, err := exec.LookPath("dusrelimage"); err == nil {
		return p
	}
	t.Fatal("dusrelimage CLI binary not found: build it with `cargo build -p dusrelimage-cli --release` (or set DUSRELIMAGE_CLI)")
	return ""
}

// mockFixture runs `dusrelimage mock` once per (kind, width, height,
// seed) and caches the bytes.
func mockFixture(t *testing.T, kind string, width, height int, seed uint64) []byte {
	t.Helper()
	key := fmt.Sprintf("mock:%s:%dx%d:%d", kind, width, height, seed)
	fixtureCacheMu.Lock()
	defer fixtureCacheMu.Unlock()
	if b, ok := fixtureCache[key]; ok {
		return b
	}
	outPath := filepath.Join(t.TempDir(), "mock.png")
	cmd := exec.Command(cliPath(t), "mock",
		"--kind", kind,
		"--width", strconv.Itoa(width),
		"--height", strconv.Itoa(height),
		"--seed", strconv.FormatUint(seed, 10),
		"-o", outPath)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		t.Fatalf("dusrelimage mock %s: %v: %s", key, err, stderr.String())
	}
	b, err := os.ReadFile(outPath)
	if err != nil {
		t.Fatal(err)
	}
	fixtureCache[key] = b
	return b
}

// Minimal PNG encoder (test fixtures only): filter-0 rows, zlib
// compressed, colour type 2 (RGB) or 6 (RGBA), 8-bit.
func encodePNG(width, height int, rgba bool, pixel func(x, y int) [4]uint8) []byte {
	channels := 3
	colourType := byte(2)
	if rgba {
		channels = 4
		colourType = 6
	}
	raw := make([]byte, 0, height*(1+width*channels))
	for y := 0; y < height; y++ {
		raw = append(raw, 0) // filter: none
		for x := 0; x < width; x++ {
			px := pixel(x, y)
			raw = append(raw, px[:channels]...)
		}
	}
	var idat bytes.Buffer
	zw := zlib.NewWriter(&idat)
	if _, err := zw.Write(raw); err != nil {
		panic(err)
	}
	if err := zw.Close(); err != nil {
		panic(err)
	}
	ihdr := make([]byte, 13)
	binary.BigEndian.PutUint32(ihdr[0:4], uint32(width))
	binary.BigEndian.PutUint32(ihdr[4:8], uint32(height))
	ihdr[8] = 8 // bit depth
	ihdr[9] = colourType
	var out bytes.Buffer
	out.Write([]byte{0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a})
	out.Write(pngChunk("IHDR", ihdr))
	out.Write(pngChunk("IDAT", idat.Bytes()))
	out.Write(pngChunk("IEND", nil))
	return out.Bytes()
}

func pngChunk(typ string, data []byte) []byte {
	out := make([]byte, 12+len(data))
	binary.BigEndian.PutUint32(out[0:4], uint32(len(data)))
	copy(out[4:8], typ)
	copy(out[8:], data)
	crc := crc32.NewIEEE()
	crc.Write([]byte(typ))
	crc.Write(data)
	binary.BigEndian.PutUint32(out[8+len(data):], crc.Sum32())
	return out
}

// Minimal uncompressed little-endian TIFF, RGB 8-bit (mirrors the JS
// runner's encodeTiff).
func encodeTIFF(width, height int, rgb [3]int) []byte {
	bitsOffset := 8 + 2 + 10*12 + 4
	dataOffset := bitsOffset + 6
	pixels := make([]byte, width*height*3)
	for i := range width * height {
		pixels[i*3] = byte(rgb[0])
		pixels[i*3+1] = byte(rgb[1])
		pixels[i*3+2] = byte(rgb[2])
	}
	entry := func(tag, typ uint16, count, value uint32) []byte {
		b := make([]byte, 12)
		binary.LittleEndian.PutUint16(b[0:2], tag)
		binary.LittleEndian.PutUint16(b[2:4], typ)
		binary.LittleEndian.PutUint32(b[4:8], count)
		binary.LittleEndian.PutUint32(b[8:12], value)
		return b
	}
	var ifd bytes.Buffer
	for _, e := range [][4]uint32{
		{256, 4, 1, uint32(width)},
		{257, 4, 1, uint32(height)},
		{258, 3, 3, uint32(bitsOffset)},
		{259, 3, 1, 1},
		{262, 3, 1, 2},
		{273, 4, 1, uint32(dataOffset)},
		{277, 3, 1, 3},
		{278, 4, 1, uint32(height)},
		{279, 4, 1, uint32(len(pixels))},
		{284, 3, 1, 1},
	} {
		ifd.Write(entry(uint16(e[0]), uint16(e[1]), e[2], e[3]))
	}
	header := make([]byte, 8)
	copy(header[0:2], "II")
	binary.LittleEndian.PutUint16(header[2:4], 42)
	binary.LittleEndian.PutUint32(header[4:8], 8)
	count := make([]byte, 2)
	binary.LittleEndian.PutUint16(count, 10)
	next := make([]byte, 4)
	bits := make([]byte, 6)
	binary.LittleEndian.PutUint16(bits[0:2], 8)
	binary.LittleEndian.PutUint16(bits[2:4], 8)
	binary.LittleEndian.PutUint16(bits[4:6], 8)
	var out bytes.Buffer
	out.Write(header)
	out.Write(count)
	out.Write(ifd.Bytes())
	out.Write(next)
	out.Write(bits)
	out.Write(pixels)
	return out.Bytes()
}

var (
	bytesDerivedRe = regexp.MustCompile(`^bytes \[(.*)\]$`)
	asciiDerivedRe = regexp.MustCompile(`^ASCII bytes: '(.*)'$`)
	dimsRe         = regexp.MustCompile(`(\d+)x(\d+)`)
	rgbRe          = regexp.MustCompile(`\((\d+),\s*(\d+),\s*(\d+)\)`)
)

func parseDimsFromDerived(text, caseID string) (int, int) {
	m := dimsRe.FindStringSubmatch(text)
	if m == nil {
		panic(fmt.Sprintf("case %s: no WxH dimensions in derived_from: %s", caseID, text))
	}
	w, _ := strconv.Atoi(m[1])
	h, _ := strconv.Atoi(m[2])
	return w, h
}

func parseRGBFromDerived(text, caseID string) [3]int {
	m := rgbRe.FindStringSubmatch(text)
	if m == nil {
		panic(fmt.Sprintf("case %s: no rgb tuple in derived_from: %s", caseID, text))
	}
	var rgb [3]int
	for i := range 3 {
		rgb[i], _ = strconv.Atoi(m[i+1])
	}
	return rgb
}

// fixtureBytes builds one fixture's bytes. t is needed for the CLI
// mock calls; the returned bytes are cached by spec.
func fixtureBytes(t *testing.T, raw json.RawMessage, caseID string) []byte {
	t.Helper()
	var spec fixtureSpec
	if err := json.Unmarshal(raw, &spec); err != nil {
		t.Fatalf("case %s: bad fixture: %v", caseID, err)
	}
	key := string(raw)
	fixtureCacheMu.Lock()
	if b, ok := fixtureCache[key]; ok {
		fixtureCacheMu.Unlock()
		return b
	}
	fixtureCacheMu.Unlock()
	var out []byte
	switch spec.Kind {
	case "solid", "gradient", "noise":
		out = mockFixture(t, spec.Kind, spec.Width, spec.Height, spec.Seed)
	case "solid_rgb":
		rgb := spec.RGB
		out = encodePNG(spec.Width, spec.Height, false, func(_, _ int) [4]uint8 {
			return [4]uint8{uint8(rgb[0]), uint8(rgb[1]), uint8(rgb[2]), 255}
		})
	case "transparent_rgba":
		out = encodePNG(spec.Width, spec.Height, true, func(_, _ int) [4]uint8 {
			return [4]uint8{0, 0, 0, 0}
		})
	case "two_colour":
		// Left half red, right half blue, 50/50 by construction.
		out = encodePNG(spec.Width, spec.Height, false, func(x, _ int) [4]uint8 {
			if x < spec.Width/2 {
				return [4]uint8{255, 0, 0, 255}
			}
			return [4]uint8{0, 0, 255, 255}
		})
	case "tiff":
		rgb := spec.RGB
		if spec.DerivedFrom != "" {
			// rgb + dimensions live only in derived_from ("rgb
			// (10,20,30) 48x32 TIFF, written with the image crate").
			if rgb == [3]int{} {
				rgb = parseRGBFromDerived(spec.DerivedFrom, caseID)
			}
			if spec.Width == 0 {
				spec.Width, spec.Height = parseDimsFromDerived(spec.DerivedFrom, caseID)
			}
		}
		out = encodeTIFF(spec.Width, spec.Height, rgb)
	case "jpeg":
		// Derived: compress gradient 100x80 seed 2 to JPEG q80.
		if !strings.Contains(spec.DerivedFrom, "jpeg") {
			t.Fatalf("case %s: jpeg fixture derived_from does not mention jpeg", caseID)
		}
		src := mockFixture(t, "gradient", 100, 80, 2)
		opts := DefaultCompressOptions()
		opts.Format = ptrFormat(FormatJPEG)
		opts.Quality = 80
		converted, err := Compress(src, &opts)
		if err != nil {
			t.Fatalf("case %s: building jpeg fixture: %v", caseID, err)
		}
		out = converted
	case "avif":
		// Derived: convert gradient 64x48 seed 4 to AVIF q80.
		if !strings.Contains(spec.DerivedFrom, "avif") {
			t.Fatalf("case %s: avif fixture derived_from does not mention avif", caseID)
		}
		src := mockFixture(t, "gradient", 64, 48, 4)
		opts := DefaultConvertOptions()
		opts.Format = FormatAVIF
		opts.Quality = 80
		converted, err := Convert(src, &opts)
		if err != nil {
			t.Fatalf("case %s: building avif fixture: %v", caseID, err)
		}
		out = converted
	case "garbage":
		if m := bytesDerivedRe.FindStringSubmatch(spec.DerivedFrom); m != nil {
			parts := strings.Split(m[1], ",")
			out = make([]byte, 0, len(parts))
			for _, p := range parts {
				n, err := strconv.Atoi(strings.TrimSpace(p))
				if err != nil {
					t.Fatalf("case %s: bad garbage bytes: %v", caseID, err)
				}
				out = append(out, byte(n))
			}
		} else if m := asciiDerivedRe.FindStringSubmatch(spec.DerivedFrom); m != nil {
			out = []byte(m[1])
		} else {
			t.Fatalf("case %s: unhandled garbage derived_from: %s", caseID, spec.DerivedFrom)
		}
	case "empty":
		out = []byte{}
	default:
		t.Fatalf("case %s: unknown fixture kind %s — update this runner", caseID, spec.Kind)
	}
	fixtureCacheMu.Lock()
	fixtureCache[key] = out
	fixtureCacheMu.Unlock()
	return out
}

func ptrFormat(f ImageFormat) *ImageFormat { return &f }

// ---------------------------------------------------------------------------
// Options translation (vector JSON snake_case -> complete Go structs)
// ---------------------------------------------------------------------------

type rawCommon struct {
	AutoOrient   *bool      `json:"auto_orient"`
	KeepExif     *bool      `json:"keep_exif"`
	DPI          *[2]uint32 `json:"dpi"`
	MaxPixels    *uint64    `json:"max_pixels"`
	MaxDimension *uint32    `json:"max_dimension"`
}

func (r *rawCommon) apply(c *CommonOptions) {
	if r == nil {
		return
	}
	if r.AutoOrient != nil {
		c.AutoOrient = *r.AutoOrient
	}
	if r.KeepExif != nil {
		c.KeepExif = *r.KeepExif
	}
	if r.DPI != nil {
		c.DPI = r.DPI
	}
	if r.MaxPixels != nil {
		c.MaxPixels = *r.MaxPixels
	}
	if r.MaxDimension != nil {
		c.MaxDimension = *r.MaxDimension
	}
}

// optionsMap parses the vector's options object into a key map plus
// the common options (nested under "common", or flat at the top
// level — both are accepted by the FFI layer).
func optionsMap(t *testing.T, raw json.RawMessage, caseID string) (CommonOptions, map[string]json.RawMessage) {
	t.Helper()
	common := DefaultCommonOptions()
	rest := map[string]json.RawMessage{}
	if len(raw) == 0 {
		return common, rest
	}
	if err := json.Unmarshal(raw, &rest); err != nil {
		t.Fatalf("case %s: bad options: %v", caseID, err)
	}
	if nested, ok := rest["common"]; ok {
		var rc rawCommon
		if err := json.Unmarshal(nested, &rc); err != nil {
			t.Fatalf("case %s: bad common options: %v", caseID, err)
		}
		rc.apply(&common)
	}
	// Flat common keys at the top level.
	var flat rawCommon
	flatRaw, _ := json.Marshal(rest)
	if err := json.Unmarshal(flatRaw, &flat); err == nil {
		flat.apply(&common)
	}
	return common, rest
}

// unmarshalRest re-decodes the options map into a per-op raw struct.
func unmarshalRest(t *testing.T, rest map[string]json.RawMessage, out any, caseID string) {
	t.Helper()
	raw, err := json.Marshal(rest)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(raw, out); err != nil {
		t.Fatalf("case %s: bad options for struct %T: %v", caseID, out, err)
	}
}

func colourFromHexOrFail(t *testing.T, s, caseID string) Colour {
	t.Helper()
	c, err := ColourFromHex(s)
	if err != nil {
		t.Fatalf("case %s: bad hex colour %q: %v", caseID, s, err)
	}
	return c
}

func buildCompress(t *testing.T, c vectorCase) CompressOptions {
	t.Helper()
	common, rest := optionsMap(t, c.Options, c.ID)
	opts := DefaultCompressOptions()
	opts.Common = common
	var raw struct {
		Quality     *uint8       `json:"quality"`
		Format      *ImageFormat `json:"format"`
		Progressive *bool        `json:"progressive"`
		Lossless    *bool        `json:"lossless"`
		Effort      *uint8       `json:"effort"`
	}
	unmarshalRest(t, rest, &raw, c.ID)
	if raw.Quality != nil {
		opts.Quality = *raw.Quality
	}
	if raw.Format != nil {
		opts.Format = raw.Format
	}
	if raw.Progressive != nil {
		opts.Progressive = *raw.Progressive
	}
	if raw.Lossless != nil {
		opts.Lossless = *raw.Lossless
	}
	if raw.Effort != nil {
		opts.Effort = *raw.Effort
	}
	return opts
}

func buildResize(t *testing.T, c vectorCase) ResizeOptions {
	t.Helper()
	common, rest := optionsMap(t, c.Options, c.ID)
	opts := DefaultResizeOptions()
	opts.Common = common
	var raw struct {
		Width      *uint32  `json:"width"`
		Height     *uint32  `json:"height"`
		Percent    *float32 `json:"percent"`
		Fit        *FitMode `json:"fit"`
		Filter     *Filter  `json:"filter"`
		Background *string  `json:"background"`
	}
	unmarshalRest(t, rest, &raw, c.ID)
	if raw.Width != nil {
		opts.Width = raw.Width
	}
	if raw.Height != nil {
		opts.Height = raw.Height
	}
	if raw.Percent != nil {
		opts.Percent = raw.Percent
	}
	if raw.Fit != nil {
		opts.Fit = *raw.Fit
	}
	if raw.Filter != nil {
		opts.Filter = *raw.Filter
	}
	if raw.Background != nil {
		opts.Background = colourFromHexOrFail(t, *raw.Background, c.ID)
	}
	return opts
}

func buildCrop(t *testing.T, c vectorCase) CropOptions {
	t.Helper()
	common, rest := optionsMap(t, c.Options, c.ID)
	opts := DefaultCropOptions()
	opts.Common = common
	var raw struct {
		X      *uint32   `json:"x"`
		Y      *uint32   `json:"y"`
		Width  *uint32   `json:"width"`
		Height *uint32   `json:"height"`
		Anchor *Position `json:"anchor"`
	}
	unmarshalRest(t, rest, &raw, c.ID)
	if raw.X != nil {
		opts.X = *raw.X
	}
	if raw.Y != nil {
		opts.Y = *raw.Y
	}
	if raw.Width != nil {
		opts.Width = *raw.Width
	}
	if raw.Height != nil {
		opts.Height = *raw.Height
	}
	if raw.Anchor != nil {
		opts.Anchor = raw.Anchor
	}
	return opts
}

func buildConvert(t *testing.T, c vectorCase) ConvertOptions {
	t.Helper()
	common, rest := optionsMap(t, c.Options, c.ID)
	opts := DefaultConvertOptions()
	opts.Common = common
	var raw struct {
		Format      *ImageFormat `json:"format"`
		Quality     *uint8       `json:"quality"`
		Background  *string      `json:"background"`
		Progressive *bool        `json:"progressive"`
	}
	unmarshalRest(t, rest, &raw, c.ID)
	if raw.Format != nil {
		opts.Format = *raw.Format
	}
	if raw.Quality != nil {
		opts.Quality = *raw.Quality
	}
	if raw.Background != nil {
		opts.Background = colourFromHexOrFail(t, *raw.Background, c.ID)
	}
	if raw.Progressive != nil {
		opts.Progressive = *raw.Progressive
	}
	return opts
}

func buildWatermark(t *testing.T, c vectorCase) WatermarkOptions {
	t.Helper()
	common, rest := optionsMap(t, c.Options, c.ID)
	opts := DefaultWatermarkOptions()
	opts.Common = common
	var raw struct {
		Text *struct {
			Text   string   `json:"text"`
			Size   *float32 `json:"size"`
			Colour *string  `json:"colour"`
		} `json:"text"`
		Logo *struct {
			Image json.RawMessage `json:"image"`
			Scale *float32        `json:"scale"`
		} `json:"logo"`
		Opacity     *float32  `json:"opacity"`
		Position    *Position `json:"position"`
		RotationDeg *float32  `json:"rotation_deg"`
		Margin      *uint32   `json:"margin"`
	}
	unmarshalRest(t, rest, &raw, c.ID)
	if raw.Text != nil {
		text := TextWatermark{Text: raw.Text.Text, Size: 48, Colour: ColourWhite()}
		if raw.Text.Size != nil {
			text.Size = *raw.Text.Size
		}
		if raw.Text.Colour != nil {
			text.Colour = colourFromHexOrFail(t, *raw.Text.Colour, c.ID)
		}
		opts.Text = &text
	}
	if raw.Logo != nil {
		logo := ImageWatermark{Image: fixtureBytes(t, raw.Logo.Image, c.ID), Scale: 1}
		if raw.Logo.Scale != nil {
			logo.Scale = *raw.Logo.Scale
		}
		opts.Logo = &logo
	}
	if raw.Opacity != nil {
		opts.Opacity = *raw.Opacity
	}
	if raw.Position != nil {
		opts.Position = *raw.Position
	}
	if raw.RotationDeg != nil {
		opts.RotationDeg = *raw.RotationDeg
	}
	if raw.Margin != nil {
		opts.Margin = *raw.Margin
	}
	return opts
}

func buildCollage(t *testing.T, c vectorCase) CollageOptions {
	t.Helper()
	common, rest := optionsMap(t, c.Options, c.ID)
	opts := DefaultCollageOptions()
	opts.Common = common
	var raw struct {
		Layout     json.RawMessage `json:"layout"`
		Gap        *uint32         `json:"gap"`
		Background *string         `json:"background"`
		Width      *uint32         `json:"width"`
		Height     *uint32         `json:"height"`
		CellFit    *FitMode        `json:"cell_fit"`
	}
	unmarshalRest(t, rest, &raw, c.ID)
	if len(raw.Layout) > 0 {
		var layout CollageLayout
		if err := json.Unmarshal(raw.Layout, &layout); err != nil {
			t.Fatalf("case %s: bad collage layout: %v", c.ID, err)
		}
		opts.Layout = layout
	}
	if raw.Gap != nil {
		opts.Gap = *raw.Gap
	}
	if raw.Background != nil {
		opts.Background = colourFromHexOrFail(t, *raw.Background, c.ID)
	}
	if raw.Width != nil {
		opts.Width = *raw.Width
	}
	if raw.Height != nil {
		opts.Height = *raw.Height
	}
	if raw.CellFit != nil {
		opts.CellFit = *raw.CellFit
	}
	return opts
}

func buildPixelate(t *testing.T, c vectorCase) PixelateOptions {
	t.Helper()
	common, rest := optionsMap(t, c.Options, c.ID)
	opts := DefaultPixelateOptions()
	opts.Common = common
	var raw struct {
		X         *uint32 `json:"x"`
		Y         *uint32 `json:"y"`
		Width     *uint32 `json:"width"`
		Height    *uint32 `json:"height"`
		BlockSize *uint32 `json:"block_size"`
	}
	unmarshalRest(t, rest, &raw, c.ID)
	if raw.X != nil {
		opts.X = *raw.X
	}
	if raw.Y != nil {
		opts.Y = *raw.Y
	}
	if raw.Width != nil {
		opts.Width = *raw.Width
	}
	if raw.Height != nil {
		opts.Height = *raw.Height
	}
	if raw.BlockSize != nil {
		opts.BlockSize = *raw.BlockSize
	}
	return opts
}

// ---------------------------------------------------------------------------
// Annotations (vector {type, ...} form -> Go annotation types)
// ---------------------------------------------------------------------------

func parseAnnotations(t *testing.T, raw json.RawMessage, caseID string) []Annotation {
	t.Helper()
	if len(raw) == 0 {
		return nil
	}
	var items []map[string]json.RawMessage
	if err := json.Unmarshal(raw, &items); err != nil {
		t.Fatalf("case %s: bad annotations: %v", caseID, err)
	}
	out := make([]Annotation, 0, len(items))
	for _, item := range items {
		var kind string
		if err := json.Unmarshal(item["type"], &kind); err != nil {
			t.Fatalf("case %s: annotation without type: %v", caseID, item)
		}
		colour := func(key string) Colour {
			var s string
			if err := json.Unmarshal(item[key], &s); err != nil {
				t.Fatalf("case %s: annotation %s has no %s: %v", caseID, kind, key, err)
			}
			return colourFromHexOrFail(t, s, caseID)
		}
		fill := func() *Colour {
			rawFill, ok := item["fill"]
			if !ok || string(rawFill) == "null" {
				return nil
			}
			c := colour("fill")
			return &c
		}
		u32field := func(key string) uint32 {
			var v uint32
			if err := json.Unmarshal(item[key], &v); err != nil {
				t.Fatalf("case %s: annotation %s has no %s: %v", caseID, kind, key, err)
			}
			return v
		}
		point := func(key string) [2]uint32 {
			var p [2]uint32
			if err := json.Unmarshal(item[key], &p); err != nil {
				t.Fatalf("case %s: annotation %s has no %s: %v", caseID, kind, key, err)
			}
			return p
		}
		switch kind {
		case "Arrow":
			out = append(out, ArrowAnnotation{From: point("from"), To: point("to"), Colour: colour("colour"), Width: u32field("width")})
		case "Rect":
			out = append(out, RectAnnotation{X: u32field("x"), Y: u32field("y"), W: u32field("w"), H: u32field("h"), Colour: colour("colour"), Width: u32field("width"), Fill: fill()})
		case "Ellipse":
			out = append(out, EllipseAnnotation{X: u32field("x"), Y: u32field("y"), W: u32field("w"), H: u32field("h"), Colour: colour("colour"), Width: u32field("width"), Fill: fill()})
		case "Line":
			out = append(out, LineAnnotation{From: point("from"), To: point("to"), Colour: colour("colour"), Width: u32field("width")})
		case "Text":
			var text string
			if err := json.Unmarshal(item["text"], &text); err != nil {
				t.Fatalf("case %s: text annotation has no text", caseID)
			}
			var size float32
			if err := json.Unmarshal(item["size"], &size); err != nil {
				t.Fatalf("case %s: text annotation has no size", caseID)
			}
			out = append(out, TextAnnotation{X: u32field("x"), Y: u32field("y"), Text: text, Size: size, Colour: colour("colour")})
		default:
			t.Fatalf("case %s: unknown annotation type %s — update this runner", caseID, kind)
		}
	}
	return out
}

// ---------------------------------------------------------------------------
// Running one case
// ---------------------------------------------------------------------------

type caseOutcome struct {
	kind    string // "image", "info", "colour", "palette", "bulk"
	bytes   []byte
	info    ImageInfo
	colour  Colour
	palette []PaletteColour
	bulk    []BulkResult
}

func runCase(t *testing.T, c vectorCase) (caseOutcome, *DusrelError, []byte) {
	t.Helper()
	var inputs [][]byte
	var input []byte
	if len(c.Fixtures) > 0 {
		for _, f := range c.Fixtures {
			inputs = append(inputs, fixtureBytes(t, f, c.ID))
		}
		input = inputs[0]
	} else if len(c.Fixture) > 0 {
		input = fixtureBytes(t, c.Fixture, c.ID)
		inputs = [][]byte{input}
	}
	needImage := func() []byte {
		if input == nil {
			t.Fatalf("case %s: op needs one fixture", c.ID)
		}
		return input
	}
	call := func(fn func() (any, error), in []byte) (caseOutcome, *DusrelError, []byte) {
		out, err := fn()
		if err != nil {
			var de *DusrelError
			if errors.As(err, &de) {
				return caseOutcome{}, de, in
			}
			t.Fatalf("case %s: non-DusrelError: %v", c.ID, err)
		}
		switch v := out.(type) {
		case []byte:
			return caseOutcome{kind: "image", bytes: v}, nil, in
		case ImageInfo:
			return caseOutcome{kind: "info", info: v}, nil, in
		case Colour:
			return caseOutcome{kind: "colour", colour: v}, nil, in
		case []PaletteColour:
			return caseOutcome{kind: "palette", palette: v}, nil, in
		case []BulkResult:
			return caseOutcome{kind: "bulk", bulk: v}, nil, in
		default:
			t.Fatalf("case %s: unexpected outcome type %T", c.ID, out)
			return caseOutcome{}, nil, in
		}
	}
	switch c.Op {
	case "get_image_info":
		common, _ := optionsMap(t, c.Options, c.ID)
		return call(func() (any, error) {
			return GetImageInfo(needImage(), &common)
		}, input)
	case "compress_image":
		opts := buildCompress(t, c)
		return call(func() (any, error) { return Compress(needImage(), &opts) }, input)
	case "compress_to_size":
		opts := buildCompress(t, c)
		if c.TargetBytes == nil {
			t.Fatalf("case %s: compress_to_size without target_bytes", c.ID)
		}
		return call(func() (any, error) {
			return CompressToSize(needImage(), *c.TargetBytes, &opts)
		}, input)
	case "resize_image":
		opts := buildResize(t, c)
		return call(func() (any, error) { return Resize(needImage(), &opts) }, input)
	case "crop_image":
		opts := buildCrop(t, c)
		return call(func() (any, error) { return Crop(needImage(), &opts) }, input)
	case "convert_image":
		opts := buildConvert(t, c)
		return call(func() (any, error) { return Convert(needImage(), &opts) }, input)
	case "add_watermark":
		opts := buildWatermark(t, c)
		return call(func() (any, error) { return AddWatermark(needImage(), &opts) }, input)
	case "annotate_image":
		annotations := parseAnnotations(t, c.Annotations, c.ID)
		return call(func() (any, error) { return Annotate(needImage(), annotations) }, input)
	case "pick_colour":
		_, rest := optionsMap(t, c.Options, c.ID)
		var raw struct {
			X uint32 `json:"x"`
			Y uint32 `json:"y"`
		}
		unmarshalRest(t, rest, &raw, c.ID)
		return call(func() (any, error) { return PickColour(needImage(), raw.X, raw.Y) }, input)
	case "extract_palette":
		_, rest := optionsMap(t, c.Options, c.ID)
		var raw struct {
			Count uint8 `json:"count"`
		}
		unmarshalRest(t, rest, &raw, c.ID)
		return call(func() (any, error) { return ExtractPalette(needImage(), raw.Count) }, input)
	case "make_collage":
		opts := buildCollage(t, c)
		return call(func() (any, error) { return MakeCollage(inputs, &opts) }, nil)
	case "pixelate_image":
		opts := buildPixelate(t, c)
		return call(func() (any, error) { return Pixelate(needImage(), &opts) }, input)
	case "bulk_process":
		var bulkOp BulkOp
		switch c.BulkOp {
		case "Pixelate":
			bulkOp = BulkPixelate(ptrPixelate(buildPixelate(t, c)))
		case "PickColour":
			_, rest := optionsMap(t, c.Options, c.ID)
			var raw struct {
				X uint32 `json:"x"`
				Y uint32 `json:"y"`
			}
			unmarshalRest(t, rest, &raw, c.ID)
			bulkOp = BulkPickColour(raw.X, raw.Y)
		case "Palette":
			_, rest := optionsMap(t, c.Options, c.ID)
			var raw struct {
				Count uint8 `json:"count"`
			}
			unmarshalRest(t, rest, &raw, c.ID)
			bulkOp = BulkPalette(raw.Count)
		case "Collage":
			bulkOp = BulkCollage(ptrCollage(buildCollage(t, c)))
		case "Watermark":
			bulkOp = BulkWatermark(ptrWatermark(buildWatermark(t, c)))
		case "Annotate":
			bulkOp = BulkAnnotate(parseAnnotations(t, c.Annotations, c.ID))
		default:
			t.Fatalf("case %s: unknown bulk_op %s — update this runner", c.ID, c.BulkOp)
		}
		return call(func() (any, error) { return BulkProcess(inputs, bulkOp) }, nil)
	default:
		t.Fatalf("case %s: unknown op %s — update this runner", c.ID, c.Op)
		return caseOutcome{}, nil, nil
	}
}

func ptrPixelate(o PixelateOptions) *PixelateOptions    { return &o }
func ptrCollage(o CollageOptions) *CollageOptions       { return &o }
func ptrWatermark(o WatermarkOptions) *WatermarkOptions { return &o }

// ---------------------------------------------------------------------------
// Pixels
// ---------------------------------------------------------------------------

// pixelGrid decodes a PNG into an RGBA grid. Non-PNG outputs fall back
// to PickColour per pixel (only the JPEG flatten case needs that).
type pixelGrid struct {
	rows   [][]color.NRGBA
	data   []byte
	caseID string
}

func newPixelGrid(t *testing.T, data []byte, caseID string) *pixelGrid {
	t.Helper()
	g := &pixelGrid{data: data, caseID: caseID}
	if len(data) >= 8 && data[1] == 0x50 && data[2] == 0x4e && data[3] == 0x47 {
		img, err := png.Decode(bytes.NewReader(data))
		if err != nil {
			t.Fatalf("case %s: cannot decode PNG output: %v", caseID, err)
		}
		bounds := img.Bounds()
		for y := bounds.Min.Y; y < bounds.Max.Y; y++ {
			row := make([]color.NRGBA, 0, bounds.Dx())
			for x := bounds.Min.X; x < bounds.Max.X; x++ {
				row = append(row, color.NRGBAModel.Convert(img.At(x, y)).(color.NRGBA))
			}
			g.rows = append(g.rows, row)
		}
	}
	return g
}

func (g *pixelGrid) pixel(t *testing.T, x, y int) [4]uint8 {
	t.Helper()
	if g.rows != nil {
		px := g.rows[y][x]
		return [4]uint8{px.R, px.G, px.B, px.A}
	}
	c, err := PickColour(g.data, uint32(x), uint32(y))
	if err != nil {
		t.Fatalf("case %s: pickColour fallback failed: %v", g.caseID, err)
	}
	return [4]uint8{c.R, c.G, c.B, c.A}
}

func (g *pixelGrid) isPNG() bool { return g.rows != nil }

func (g *pixelGrid) regionHasDark(x0, y0, x1, y1 int) bool {
	for y := y0; y < y1; y++ {
		for x := x0; x < x1; x++ {
			px := g.rows[y][x]
			if px.R < 128 || px.G < 128 || px.B < 128 {
				return true
			}
		}
	}
	return false
}

func (g *pixelGrid) regionAllWhite(x0, y0, x1, y1 int) bool {
	for y := y0; y < y1; y++ {
		for x := x0; x < x1; x++ {
			px := g.rows[y][x]
			if px.R != 255 || px.G != 255 || px.B != 255 {
				return false
			}
		}
	}
	return true
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

var knownExpectedKeys = map[string]bool{
	"format": true, "width": true, "height": true, "has_alpha": true,
	"colour_type": true, "error": true, "max_bytes": true,
	"avif_header": true, "pixels": true, "region_all_white": true,
	"region_has_dark": true, "regions_have_dark": true, "colour": true,
	"different_from": true, "palette_hexes": true, "palette_length": true,
	"share_each": true, "share_sum": true, "share_sum_tolerance": true,
	"share_tolerance": true, "palette": true, "differs_from_input": true,
	"pixel_equalities": true, "unchanged_pixels": true, "results": true,
}

func num(t *testing.T, raw json.RawMessage, caseID, what string) float64 {
	t.Helper()
	var v float64
	if err := json.Unmarshal(raw, &v); err != nil {
		t.Fatalf("case %s: expected %s is not a number: %v", caseID, what, err)
	}
	return v
}

func strVal(t *testing.T, raw json.RawMessage, caseID, what string) string {
	t.Helper()
	var v string
	if err := json.Unmarshal(raw, &v); err != nil {
		t.Fatalf("case %s: expected %s is not a string: %v", caseID, what, err)
	}
	return v
}

func assertExpected(t *testing.T, c vectorCase, outcome caseOutcome, de *DusrelError, input []byte) {
	t.Helper()
	failures := []string{}
	fail := func(format string, args ...any) {
		failures = append(failures, fmt.Sprintf("case %s: "+format, append([]any{c.ID}, args...)...))
	}
	for key := range c.Expected {
		if !knownExpectedKeys[key] {
			fail("unhandled expected key %s — update this runner", key)
		}
	}
	if rawErr, ok := c.Expected["error"]; ok {
		want := strVal(t, rawErr, c.ID, "error")
		if de == nil {
			fail("expected error %s, got success", want)
		} else if de.Code != want {
			fail("expected error %s, got %s", want, de.Code)
		}
		if len(failures) > 0 {
			t.Error(strings.Join(failures, "\n"))
		}
		return
	}
	if de != nil {
		fail("expected success, got %s (%s)", de.Code, de.MessageEn)
		t.Error(strings.Join(failures, "\n"))
		return
	}

	var info *ImageInfo
	if outcome.kind == "info" {
		info = &outcome.info
	} else if outcome.kind == "image" {
		// AVIF output cannot be decoded back in this build (no
		// avif-native decoder): a case that only asserts the
		// header/size may skip the decode; anything asserting dims
		// must be decodable.
		if got, err := GetImageInfo(outcome.bytes, nil); err == nil {
			info = &got
		} else if _, ok := c.Expected["format"]; ok {
			fail("cannot decode output to check format: %v", err)
		} else if _, ok := c.Expected["width"]; ok {
			fail("cannot decode output to check width: %v", err)
		} else if _, ok := c.Expected["height"]; ok {
			fail("cannot decode output to check height: %v", err)
		} else if _, ok := c.Expected["has_alpha"]; ok {
			fail("cannot decode output to check has_alpha: %v", err)
		} else if _, ok := c.Expected["colour_type"]; ok {
			fail("cannot decode output to check colour_type: %v", err)
		}
	}
	if info != nil {
		if raw, ok := c.Expected["format"]; ok && info.Format != strVal(t, raw, c.ID, "format") {
			fail("expected format %s, got %s", strVal(t, raw, c.ID, "format"), info.Format)
		}
		if raw, ok := c.Expected["width"]; ok && info.Width != uint32(num(t, raw, c.ID, "width")) {
			fail("expected width %v, got %d", num(t, raw, c.ID, "width"), info.Width)
		}
		if raw, ok := c.Expected["height"]; ok && info.Height != uint32(num(t, raw, c.ID, "height")) {
			fail("expected height %v, got %d", num(t, raw, c.ID, "height"), info.Height)
		}
		if raw, ok := c.Expected["has_alpha"]; ok {
			var want bool
			if err := json.Unmarshal(raw, &want); err == nil && info.HasAlpha != want {
				fail("expected has_alpha %v, got %v", want, info.HasAlpha)
			}
		}
		if raw, ok := c.Expected["colour_type"]; ok && info.ColourType != strVal(t, raw, c.ID, "colour_type") {
			fail("expected colour_type %s, got %s", strVal(t, raw, c.ID, "colour_type"), info.ColourType)
		}
	}

	if outcome.kind == "image" {
		data := outcome.bytes
		if raw, ok := c.Expected["max_bytes"]; ok && len(data) > int(num(t, raw, c.ID, "max_bytes")) {
			fail("expected <= %v bytes, got %d", num(t, raw, c.ID, "max_bytes"), len(data))
		}
		if raw, ok := c.Expected["avif_header"]; ok {
			var want bool
			_ = json.Unmarshal(raw, &want)
			if want && (len(data) < 12 || string(data[4:12]) != "ftypavif") {
				fail("expected AVIF ftypavif header")
			}
		}
		pixels := newPixelGrid(t, data, c.ID)
		var inputPixels *pixelGrid
		if input != nil {
			inputPixels = newPixelGrid(t, input, c.ID)
		}
		checkPixel := func(px [4]uint8, spec map[string]json.RawMessage) {
			x := int(num(t, spec["x"], c.ID, "x"))
			y := int(num(t, spec["y"], c.ID, "y"))
			if raw, ok := spec["rgba"]; ok {
				var want []int
				_ = json.Unmarshal(raw, &want)
				got := fmt.Sprintf("%d,%d,%d,%d", px[0], px[1], px[2], px[3])
				wantStr := ""
				for i, v := range want {
					if i > 0 {
						wantStr += ","
					}
					wantStr += strconv.Itoa(v)
				}
				if got != wantStr {
					fail("pixel (%d,%d) expected rgba %s, got %s", x, y, wantStr, got)
				}
			}
			if raw, ok := spec["rgba_approx"]; ok {
				var want []int
				_ = json.Unmarshal(raw, &want)
				tol := 0.0
				if tr, ok := spec["tolerance"]; ok {
					tol = num(t, tr, c.ID, "tolerance")
				}
				for i, w := range want {
					if abs(int(px[i])-w) > int(tol) {
						fail("pixel (%d,%d) channel %d expected ~%d±%v, got %d", x, y, i, w, tol, px[i])
					}
				}
			}
			for _, ch := range []struct {
				key string
				idx int
			}{{"r", 0}, {"g", 1}, {"b", 2}, {"a", 3}} {
				if raw, ok := spec[ch.key]; ok && px[ch.idx] != uint8(num(t, raw, c.ID, ch.key)) {
					fail("pixel (%d,%d) channel %s expected %v, got %d", x, y, ch.key, num(t, raw, c.ID, ch.key), px[ch.idx])
				}
			}
			if raw, ok := spec["r_max"]; ok && px[0] > uint8(num(t, raw, c.ID, "r_max")) {
				fail("pixel (%d,%d) r expected <= %v, got %d", x, y, num(t, raw, c.ID, "r_max"), px[0])
			}
		}
		if raw, ok := c.Expected["pixels"]; ok {
			var specs []map[string]json.RawMessage
			if err := json.Unmarshal(raw, &specs); err != nil {
				t.Fatalf("case %s: bad pixels: %v", c.ID, err)
			}
			for _, spec := range specs {
				x := int(num(t, spec["x"], c.ID, "x"))
				y := int(num(t, spec["y"], c.ID, "y"))
				checkPixel(pixels.pixel(t, x, y), spec)
			}
		}
		region := func(key string, check func(x0, y0, x1, y1 int) bool) {
			raw, ok := c.Expected[key]
			if !ok {
				return
			}
			if !pixels.isPNG() {
				fail("region check needs a PNG output")
				return
			}
			var r struct {
				X0 int `json:"x0"`
				Y0 int `json:"y0"`
				X1 int `json:"x1"`
				Y1 int `json:"y1"`
			}
			if err := json.Unmarshal(raw, &r); err != nil {
				t.Fatalf("case %s: bad region: %v", c.ID, err)
			}
			if !check(r.X0, r.Y0, r.X1, r.Y1) {
				fail("region (%d,%d)-(%d,%d) failed %s", r.X0, r.Y0, r.X1, r.Y1, key)
			}
		}
		region("region_all_white", pixels.regionAllWhite)
		region("region_has_dark", pixels.regionHasDark)
		if raw, ok := c.Expected["regions_have_dark"]; ok {
			var regions []struct {
				X0 int `json:"x0"`
				Y0 int `json:"y0"`
				X1 int `json:"x1"`
				Y1 int `json:"y1"`
			}
			_ = json.Unmarshal(raw, &regions)
			if !pixels.isPNG() {
				fail("region check needs a PNG output")
			} else {
				for _, r := range regions {
					if !pixels.regionHasDark(r.X0, r.Y0, r.X1, r.Y1) {
						fail("region (%d,%d)-(%d,%d) should contain dark pixels", r.X0, r.Y0, r.X1, r.Y1)
					}
				}
			}
		}
		if inputPixels != nil {
			if raw, ok := c.Expected["differs_from_input"]; ok {
				var pts [][2]int
				_ = json.Unmarshal(raw, &pts)
				for _, p := range pts {
					if pixels.pixel(t, p[0], p[1]) == inputPixels.pixel(t, p[0], p[1]) {
						fail("pixel (%d,%d) should differ from the input", p[0], p[1])
					}
				}
			}
			if raw, ok := c.Expected["pixel_equalities"]; ok {
				var pairs []struct {
					A     [2]int `json:"a"`
					B     [2]int `json:"b"`
					Equal bool   `json:"equal"`
				}
				_ = json.Unmarshal(raw, &pairs)
				for _, pair := range pairs {
					same := pixels.pixel(t, pair.A[0], pair.A[1]) == pixels.pixel(t, pair.B[0], pair.B[1])
					if same != pair.Equal {
						fail("pixels (%d,%d) vs (%d,%d) equal=%v, expected %v", pair.A[0], pair.A[1], pair.B[0], pair.B[1], same, pair.Equal)
					}
				}
			}
			if raw, ok := c.Expected["unchanged_pixels"]; ok {
				var pts [][2]int
				_ = json.Unmarshal(raw, &pts)
				for _, p := range pts {
					if pixels.pixel(t, p[0], p[1]) != inputPixels.pixel(t, p[0], p[1]) {
						fail("pixel (%d,%d) should be unchanged from the input", p[0], p[1])
					}
				}
			}
		}
	}

	if outcome.kind == "colour" {
		if raw, ok := c.Expected["colour"]; ok {
			var want map[string]json.RawMessage
			_ = json.Unmarshal(raw, &want)
			colourMap := map[string]any{
				"hex": outcome.colour.Hex, "r": outcome.colour.R, "g": outcome.colour.G,
				"b": outcome.colour.B, "a": outcome.colour.A,
			}
			for key, wantRaw := range want {
				var wantVal any
				_ = json.Unmarshal(wantRaw, &wantVal)
				if fmt.Sprintf("%v", colourMap[key]) != fmt.Sprintf("%v", wantVal) {
					fail("expected colour %s=%v, got %v", key, wantVal, colourMap[key])
				}
			}
		}
		if raw, ok := c.Expected["different_from"]; ok && input != nil {
			var other struct {
				X uint32 `json:"x"`
				Y uint32 `json:"y"`
			}
			_ = json.Unmarshal(raw, &other)
			otherColour, err := PickColour(input, other.X, other.Y)
			if err != nil {
				t.Fatalf("case %s: different_from pick failed: %v", c.ID, err)
			}
			if otherColour.Hex == outcome.colour.Hex {
				fail("colour should differ from the pixel at (%d,%d)", other.X, other.Y)
			}
		}
	}

	if outcome.kind == "palette" {
		palette := outcome.palette
		if raw, ok := c.Expected["palette_length"]; ok && len(palette) != int(num(t, raw, c.ID, "palette_length")) {
			fail("expected palette length %v, got %d", num(t, raw, c.ID, "palette_length"), len(palette))
		}
		if raw, ok := c.Expected["palette_hexes"]; ok {
			var want []string
			_ = json.Unmarshal(raw, &want)
			got := make([]string, 0, len(palette))
			for _, p := range palette {
				got = append(got, p.Colour.Hex)
			}
			// Compared as a set: equal shares make the tie order
			// unspecified; shares are asserted separately.
			if strings.Join(sorted(got), ",") != strings.Join(sorted(want), ",") {
				fail("expected palette hexes %v, got %v", want, got)
			}
		}
		shareTol := 0.0
		if raw, ok := c.Expected["share_tolerance"]; ok {
			shareTol = num(t, raw, c.ID, "share_tolerance")
		}
		if raw, ok := c.Expected["share_each"]; ok {
			want := num(t, raw, c.ID, "share_each")
			for _, entry := range palette {
				if absf(float64(entry.Share)-want) > shareTol {
					fail("palette %s share expected %v±%v, got %v", entry.Colour.Hex, want, shareTol, entry.Share)
				}
			}
		}
		if raw, ok := c.Expected["share_sum"]; ok {
			want := num(t, raw, c.ID, "share_sum")
			tol := 0.0
			if tr, ok := c.Expected["share_sum_tolerance"]; ok {
				tol = num(t, tr, c.ID, "share_sum_tolerance")
			}
			sum := 0.0
			for _, p := range palette {
				sum += float64(p.Share)
			}
			if absf(sum-want) > tol {
				fail("palette share sum expected %v, got %v", want, sum)
			}
		}
		if raw, ok := c.Expected["palette"]; ok {
			var want []struct {
				Hex            string  `json:"hex"`
				Share          float64 `json:"share"`
				ShareTolerance float64 `json:"share_tolerance"`
			}
			_ = json.Unmarshal(raw, &want)
			if len(want) != len(palette) {
				fail("expected palette of %d, got %d", len(want), len(palette))
			} else {
				for i, w := range want {
					if w.Hex != "" && palette[i].Colour.Hex != w.Hex {
						fail("expected palette hex %s, got %s", w.Hex, palette[i].Colour.Hex)
					}
					if w.Share != 0 && absf(float64(palette[i].Share)-w.Share) > w.ShareTolerance {
						fail("palette %s share expected %v, got %v", palette[i].Colour.Hex, w.Share, palette[i].Share)
					}
				}
			}
		}
	}

	if outcome.kind == "bulk" {
		var want []map[string]json.RawMessage
		if raw, ok := c.Expected["results"]; ok {
			_ = json.Unmarshal(raw, &want)
		}
		if len(outcome.bulk) != len(want) {
			fail("expected %d bulk results, got %d", len(want), len(outcome.bulk))
		} else {
			for i, got := range outcome.bulk {
				wantItem := want[i]
				idx := got.Index
				if raw, ok := wantItem["index"]; ok {
					idx = int(num(t, raw, c.ID, "index"))
					if got.Index != idx {
						fail("bulk item index expected %d, got %d", idx, got.Index)
					}
				}
				wantOK := true
				if raw, ok := wantItem["ok"]; ok {
					_ = json.Unmarshal(raw, &wantOK)
				}
				if got.OK != wantOK {
					fail("bulk item %d ok expected %v, got %v", idx, wantOK, got.OK)
					continue
				}
				if !got.OK {
					if raw, ok := wantItem["error"]; ok && got.Error != nil && got.Error.Code != strVal(t, raw, c.ID, "error") {
						fail("bulk item %d error expected %s, got %s", idx, strVal(t, raw, c.ID, "error"), got.Error.Code)
					}
					continue
				}
				if raw, ok := wantItem["output"]; ok {
					switch strVal(t, raw, c.ID, "output") {
					case "Image":
						if got.Output == nil {
							fail("bulk item %d output expected Image bytes", idx)
							continue
						}
						info, err := GetImageInfo(got.Output, nil)
						if err != nil {
							fail("bulk item %d output is not a decodable image: %v", idx, err)
							continue
						}
						if wraw, ok := wantItem["width"]; ok && info.Width != uint32(num(t, wraw, c.ID, "width")) {
							fail("bulk item %d width expected %v, got %d", idx, num(t, wraw, c.ID, "width"), info.Width)
						}
						if hraw, ok := wantItem["height"]; ok && info.Height != uint32(num(t, hraw, c.ID, "height")) {
							fail("bulk item %d height expected %v, got %d", idx, num(t, hraw, c.ID, "height"), info.Height)
						}
					case "Colour":
						if craw, ok := wantItem["colour"]; ok {
							var wantColour struct {
								Hex string `json:"hex"`
							}
							_ = json.Unmarshal(craw, &wantColour)
							if got.Colour == nil || got.Colour.Hex != wantColour.Hex {
								gotHex := ""
								if got.Colour != nil {
									gotHex = got.Colour.Hex
								}
								fail("bulk item %d colour hex expected %s, got %s", idx, wantColour.Hex, gotHex)
							}
						}
					case "Palette":
						if lraw, ok := wantItem["palette_length"]; ok && len(got.Palette) != int(num(t, lraw, c.ID, "palette_length")) {
							fail("bulk item %d palette length expected %v, got %d", idx, num(t, lraw, c.ID, "palette_length"), len(got.Palette))
						}
						if praw, ok := wantItem["palette"]; ok {
							var wantPalette []struct {
								Hex string `json:"hex"`
							}
							_ = json.Unmarshal(praw, &wantPalette)
							for j, w := range wantPalette {
								if w.Hex != "" && (j >= len(got.Palette) || got.Palette[j].Colour.Hex != w.Hex) {
									gotHex := ""
									if j < len(got.Palette) {
										gotHex = got.Palette[j].Colour.Hex
									}
									fail("bulk item %d palette hex expected %s, got %s", idx, w.Hex, gotHex)
								}
							}
						}
					}
				}
			}
		}
	}

	if len(failures) > 0 {
		t.Error(strings.Join(failures, "\n"))
	}
}

func abs(n int) int {
	if n < 0 {
		return -n
	}
	return n
}

func absf(f float64) float64 {
	if f < 0 {
		return -f
	}
	return f
}

func sorted(in []string) []string {
	out := append([]string(nil), in...)
	for i := 1; i < len(out); i++ {
		for j := i; j > 0 && out[j] < out[j-1]; j-- {
			out[j], out[j-1] = out[j-1], out[j]
		}
	}
	return out
}

// ---------------------------------------------------------------------------
// The test
// ---------------------------------------------------------------------------

func TestSharedVectors(t *testing.T) {
	cases := loadVectors(t)
	if len(cases) != 82 {
		t.Fatalf("expected 82 shared vector cases, got %d", len(cases))
	}
	for _, c := range cases {
		c := c
		t.Run(c.ID, func(t *testing.T) {
			outcome, de, input := runCase(t, c)
			assertExpected(t, c, outcome, de, input)
		})
	}
}

func TestVectorsHaveUniqueIDs(t *testing.T) {
	cases := loadVectors(t)
	seen := map[string]bool{}
	for _, c := range cases {
		if seen[c.ID] {
			t.Fatalf("duplicate vector id %s", c.ID)
		}
		seen[c.ID] = true
	}
}

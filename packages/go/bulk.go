package dusrelimage

import (
	"encoding/base64"
	"encoding/json"
)

// BulkOp selects one tool for BulkProcess, with its options. Build
// one with the Bulk* constructors, or literally:
//
//	BulkOp{Op: "compress", Compress: &opts}
//
// Op is the short name the FFI layer accepts ("compress",
// "compress_to_size", "resize", "crop", "convert", "watermark",
// "annotate", "pick_colour", "palette", "collage", "pixelate") or the
// full snake_case name ("compress_image", ...).
type BulkOp struct {
	Op string

	Compress  *CompressOptions
	Resize    *ResizeOptions
	Crop      *CropOptions
	Convert   *ConvertOptions
	Watermark *WatermarkOptions
	Collage   *CollageOptions
	Pixelate  *PixelateOptions

	// CompressToSize only.
	TargetBytes uint64
	// Annotate only.
	Annotations []Annotation
	// PickColour only.
	X uint32
	Y uint32
	// Palette only (0 = the FFI default of 5).
	Count uint8
}

// BulkCompress runs Compress over every input.
func BulkCompress(opts *CompressOptions) BulkOp {
	return BulkOp{Op: "compress", Compress: opts}
}

// BulkCompressToSize runs CompressToSize over every input.
func BulkCompressToSize(targetBytes uint64, opts *CompressOptions) BulkOp {
	return BulkOp{Op: "compress_to_size", Compress: opts, TargetBytes: targetBytes}
}

// BulkResize runs Resize over every input.
func BulkResize(opts *ResizeOptions) BulkOp {
	return BulkOp{Op: "resize", Resize: opts}
}

// BulkCrop runs Crop over every input.
func BulkCrop(opts *CropOptions) BulkOp {
	return BulkOp{Op: "crop", Crop: opts}
}

// BulkConvert runs Convert over every input.
func BulkConvert(opts *ConvertOptions) BulkOp {
	return BulkOp{Op: "convert", Convert: opts}
}

// BulkWatermark runs AddWatermark over every input.
func BulkWatermark(opts *WatermarkOptions) BulkOp {
	return BulkOp{Op: "watermark", Watermark: opts}
}

// BulkAnnotate runs Annotate over every input.
func BulkAnnotate(annotations []Annotation) BulkOp {
	return BulkOp{Op: "annotate", Annotations: annotations}
}

// BulkPickColour runs PickColour over every input.
func BulkPickColour(x, y uint32) BulkOp {
	return BulkOp{Op: "pick_colour", X: x, Y: y}
}

// BulkPalette runs ExtractPalette over every input.
func BulkPalette(count uint8) BulkOp {
	return BulkOp{Op: "palette", Count: count}
}

// BulkCollage runs the collage op over every input. Like the core, it
// is NOT a per-item op: every item comes back INVALID_ARGUMENT — a
// collage combines all inputs, so call MakeCollage once instead.
func BulkCollage(opts *CollageOptions) BulkOp {
	return BulkOp{Op: "collage", Collage: opts}
}

// BulkPixelate runs Pixelate over every input.
func BulkPixelate(opts *PixelateOptions) BulkOp {
	return BulkOp{Op: "pixelate", Pixelate: opts}
}

// BulkResult is one item of a BulkProcess result, in input order.
type BulkResult struct {
	// Input index (0-based).
	Index int
	// True when this item succeeded.
	OK bool
	// Output bytes on success (image ops); nil on failure.
	Output []byte
	// Picked colour on success (op "pick_colour" only).
	Colour *Colour
	// Dominant colours on success (op "palette" only).
	Palette []PaletteColour
	// Error on failure; nil on success.
	Error *DusrelError
}

// bulkItemJSON is one item of the JSON array
// dusrelimage_bulk_process returns (see dusrelimage.h): image outputs
// are base64, palette entries are flattened
// ({"hex": ..., "share": ...}).
type bulkItemJSON struct {
	Index   int             `json:"index"`
	OK      bool            `json:"ok"`
	Output  string          `json:"output"`
	Colour  *Colour         `json:"colour"`
	Palette []PaletteColour `json:"palette"`
	Error   *errorJSON      `json:"error"`
}

func (item bulkItemJSON) toBulkResult() (BulkResult, error) {
	out := BulkResult{Index: item.Index, OK: item.OK}
	if item.Output != "" {
		decoded, err := base64.StdEncoding.DecodeString(item.Output)
		if err != nil {
			return out, newError(CodeOperationFailed,
				"Could not decode a bulk output: "+err.Error(),
				"Bulk output decode nahi ho paya.")
		}
		out.Output = decoded
	}
	out.Colour = item.Colour
	out.Palette = item.Palette
	if item.Error != nil {
		en := item.Error.MessageEn
		if en == "" {
			en = item.Error.ReasonEn
		}
		hi := item.Error.MessageHi
		if hi == "" {
			hi = item.Error.ReasonHi
		}
		out.Error = &DusrelError{
			Code:      item.Error.Code,
			MessageEn: en,
			MessageHi: hi,
		}
	}
	return out, nil
}

// bulkOptionsJSON builds the options JSON object the FFI bulk call
// expects for op, merging the op-specific extras (target_bytes /
// annotations / x,y / count) the way the WASM layer does. A nil
// return means "no options" (NULL = defaults).
func (op BulkOp) bulkOptionsJSON() (any, error) {
	switch op.Op {
	case "compress", "compress_image":
		return resolveCompress(op.Compress), nil
	case "compress_to_size":
		merged, err := structToMap(resolveCompress(op.Compress))
		if err != nil {
			return nil, err
		}
		merged["target_bytes"] = op.TargetBytes
		return merged, nil
	case "resize", "resize_image":
		return resolveResize(op.Resize), nil
	case "crop", "crop_image":
		return resolveCrop(op.Crop), nil
	case "convert", "convert_image":
		return resolveConvert(op.Convert), nil
	case "watermark", "add_watermark":
		return resolveWatermark(op.Watermark), nil
	case "annotate", "annotate_image":
		raw, err := marshalAnnotations(op.Annotations)
		if err != nil {
			return nil, invalidArgumentError(
				"Could not serialise annotations to JSON: "+err.Error(),
				"Annotations JSON me nahi badle ja sake.")
		}
		return map[string]any{"annotations": json.RawMessage(raw)}, nil
	case "pick_colour", "pick-colour", "pickColour":
		return map[string]any{"x": op.X, "y": op.Y}, nil
	case "palette", "extract_palette":
		if op.Count == 0 {
			return map[string]any{}, nil
		}
		return map[string]any{"count": op.Count}, nil
	case "collage", "make_collage":
		return resolveCollage(op.Collage), nil
	case "pixelate", "pixelate_image":
		return resolvePixelate(op.Pixelate), nil
	default:
		return nil, invalidArgumentError(
			"Unknown bulk op \""+op.Op+"\". Use compress, compress_to_size, resize, crop, convert, watermark, annotate, pick_colour, palette, collage or pixelate.",
			"Unknown bulk op \""+op.Op+"\" hai. compress, compress_to_size, resize, crop, convert, watermark, annotate, pick_colour, palette, collage ya pixelate use karein.")
	}
}

// structToMap round-trips a struct through JSON into a map, so one
// extra key can be added (compress_to_size's target_bytes).
func structToMap(v any) (map[string]any, error) {
	raw, err := json.Marshal(v)
	if err != nil {
		return nil, invalidArgumentError(
			"Could not serialise options to JSON: "+err.Error(),
			"Options JSON me nahi badle ja sake.")
	}
	var out map[string]any
	if err := json.Unmarshal(raw, &out); err != nil {
		return nil, invalidArgumentError(
			"Could not serialise options to JSON: "+err.Error(),
			"Options JSON me nahi badle ja sake.")
	}
	return out, nil
}

// BulkProcess runs one operation over many inputs in a single call.
// One bad input fails only its own item (with Error set), never the
// whole batch. An empty input list returns an empty result list
// (core semantics). Collage is not a per-item op: it returns one
// INVALID_ARGUMENT result per input — call MakeCollage once with the
// full list instead.
func BulkProcess(inputs [][]byte, op BulkOp) ([]BulkResult, error) {
	if len(inputs) == 0 {
		return []BulkResult{}, nil
	}
	options, err := op.bulkOptionsJSON()
	if err != nil {
		return nil, err
	}
	cOpts, err := cOptionsJSON(options)
	if err != nil {
		return nil, err
	}
	defer freeCString(cOpts)
	raw, err := cBulkProcess(inputs, op.Op, cOpts)
	if err != nil {
		return nil, err
	}
	var items []bulkItemJSON
	if err := json.Unmarshal(raw, &items); err != nil {
		return nil, newError(CodeOperationFailed,
			"Could not parse the bulk results: "+err.Error(),
			"Bulk results parse nahi ho paye.")
	}
	results := make([]BulkResult, 0, len(items))
	for _, item := range items {
		result, err := item.toBulkResult()
		if err != nil {
			return nil, err
		}
		results = append(results, result)
	}
	return results, nil
}

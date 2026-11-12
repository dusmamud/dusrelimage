package dusrelimage

/*
#cgo linux LDFLAGS: ${SRCDIR}/../../target/release/libdusrelimage_ffi.a -ldl -lpthread -lm
#cgo darwin LDFLAGS: ${SRCDIR}/../../target/release/libdusrelimage_ffi.a -lpthread -lm
#cgo windows LDFLAGS: ${SRCDIR}/../../target/release/dusrelimage_ffi.lib -lws2_32 -luserenv -lbcrypt
#include <stdint.h>
#include <stdlib.h>
#include <stddef.h>

// Declarations copied from crates/dusrelimage-ffi/dusrelimage.h —
// this file is the ONLY place cgo declarations live, so if the C API
// changes, this is the one file to fix. Do NOT #include the header:
// the Go package must build from the checked-in declarations alone.
//
// Two names differ from the original task brief — the header is
// authoritative:
//   - pixelate is dusrelimage_pixelate, NOT dusrelimage_pixelate_image;
//   - get_image_info takes options_json (5-arg form; the 4-arg form
//     the FFI worker first shipped dropped CommonOptions, so the
//     coordinator updated it — see packages/go/README.md
//     "FFI corrections applied (v0.1.0)" and SPEC §5.1);
//   - bulk takes op as a separate const char* plus options_json.
int32_t dusrelimage_compress(const uint8_t *input, size_t input_len,
                             const char *options_json,
                             uint8_t **out, size_t *out_len);
int32_t dusrelimage_compress_to_size(const uint8_t *input, size_t input_len,
                                     uint64_t target_bytes,
                                     const char *options_json,
                                     uint8_t **out, size_t *out_len);
int32_t dusrelimage_resize(const uint8_t *input, size_t input_len,
                           const char *options_json,
                           uint8_t **out, size_t *out_len);
int32_t dusrelimage_crop(const uint8_t *input, size_t input_len,
                         const char *options_json,
                         uint8_t **out, size_t *out_len);
int32_t dusrelimage_convert(const uint8_t *input, size_t input_len,
                            const char *options_json,
                            uint8_t **out, size_t *out_len);
int32_t dusrelimage_add_watermark(const uint8_t *input, size_t input_len,
                                  const char *options_json,
                                  uint8_t **out, size_t *out_len);
int32_t dusrelimage_annotate(const uint8_t *input, size_t input_len,
                             const char *annotations_json,
                             uint8_t **out, size_t *out_len);
int32_t dusrelimage_make_collage(const uint8_t *const *inputs,
                                 const size_t *input_lens, size_t count,
                                 const char *options_json,
                                 uint8_t **out, size_t *out_len);
int32_t dusrelimage_pixelate(const uint8_t *input, size_t input_len,
                             const char *options_json,
                             uint8_t **out, size_t *out_len);
int32_t dusrelimage_pick_colour(const uint8_t *input, size_t input_len,
                                uint32_t x, uint32_t y,
                                uint8_t **out_json, size_t *out_len);
int32_t dusrelimage_extract_palette(const uint8_t *input, size_t input_len,
                                    uint8_t count,
                                    uint8_t **out_json, size_t *out_len);
int32_t dusrelimage_get_image_info(const uint8_t *input, size_t input_len,
                                   const char *options_json,
                                   uint8_t **out_json, size_t *out_len);
int32_t dusrelimage_bulk_process(const uint8_t *const *inputs,
                                 const size_t *input_lens, size_t count,
                                 const char *op, const char *options_json,
                                 uint8_t **out_json, size_t *out_len);
const char *dusrelimage_last_error(void);
void dusrelimage_free_buffer(uint8_t *ptr, size_t len);
const char *dusrelimage_version(void);
*/
import "C"

import (
	"encoding/json"
	"runtime"
	"unsafe"
)

// cInput returns a C pointer to the first byte of b, or nil for an
// empty slice. The FFI layer accepts a NULL pointer with length 0
// (the core then reports EMPTY_INPUT); a NULL pointer with a non-zero
// length would be INVALID_ARGUMENT.
func cInput(b []byte) *C.uint8_t {
	if len(b) == 0 {
		return nil
	}
	return (*C.uint8_t)(unsafe.Pointer(&b[0]))
}

// cOptionsJSON marshals v to JSON and returns it as a C string the
// caller must free with C.free. A nil v means "all defaults" and
// returns a NULL pointer, which the FFI layer treats as defaults.
func cOptionsJSON(v any) (*C.char, error) {
	if v == nil {
		return nil, nil
	}
	raw, err := json.Marshal(v)
	if err != nil {
		return nil, invalidArgumentError(
			"Could not serialise options to JSON: "+err.Error(),
			"Options JSON me nahi badle ja sake.")
	}
	return C.CString(string(raw)), nil
}

// freeCString frees a C string from cOptionsJSON (a no-op for NULL).
func freeCString(p *C.char) {
	if p != nil {
		C.free(unsafe.Pointer(p))
	}
}

// takeOutput copies a Rust-owned output buffer into a Go slice and
// frees the Rust buffer exactly once, with the exact pointer and
// length the call returned. A zero-length output needs no free (the
// FFI layer documents free_buffer(NULL/0) as a no-op).
func takeOutput(ptr *C.uint8_t, length C.size_t) []byte {
	if ptr == nil || length == 0 {
		return []byte{}
	}
	out := C.GoBytes(unsafe.Pointer(ptr), C.int(length))
	C.dusrelimage_free_buffer(ptr, length)
	return out
}

// errorFromStatus builds the DusrelError for a non-zero FFI status.
// It MUST run on the same OS thread as the failed call: the last
// error is thread-local, and a read from another thread would see
// NULL. All call helpers below hold runtime.LockOSThread across the
// call and this read.
//
// When the last error is present it is parsed as
// {"code", "message_en", "message_hi"}; when it is NULL (or
// unparsable) the error is synthesised from the status code.
func errorFromStatus(status int32) *DusrelError {
	p := C.dusrelimage_last_error()
	if p == nil {
		return &DusrelError{
			Code:      statusCode(status),
			MessageEn: "Operation failed.",
			MessageHi: "Operation fail ho gaya.",
		}
	}
	if parsed := parseErrorJSON(C.GoString(p)); parsed != nil {
		return parsed
	}
	raw := C.GoString(p)
	return &DusrelError{Code: statusCode(status), MessageEn: raw, MessageHi: raw}
}

// callOut runs one FFI call that returns bytes (image output) or JSON
// (info / colour / palette / bulk) in *out/*outLen. On status 0 it
// returns the copied output; otherwise it returns the DusrelError
// from the thread-local last error. The goroutine is pinned to its OS
// thread for the call and the error read (see errorFromStatus).
func callOut(call func(out **C.uint8_t, outLen *C.size_t) C.int32_t) ([]byte, error) {
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	var out *C.uint8_t
	var outLen C.size_t
	if status := call(&out, &outLen); status != statusOK {
		return nil, errorFromStatus(int32(status))
	}
	return takeOutput(out, outLen), nil
}

// callImage is callOut for a single-image input. The input bytes are
// kept alive until the C call returns.
func callImage(input []byte, call func(in *C.uint8_t, inLen C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t) ([]byte, error) {
	in := cInput(input)
	out, err := callOut(func(out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return call(in, C.size_t(len(input)), out, outLen)
	})
	runtime.KeepAlive(input)
	return out, err
}

// callImages is callOut for a multi-image input (collage / bulk).
// The pointer array MUST live in C memory (C.malloc): a Go slice of
// pointers into Go byte slices fails cgo's pointer check ("Go pointer
// to unpinned Go pointer") because the array itself holds Go
// pointers. The length array holds no pointers, so a Go slice is
// fine for it. All inputs are kept alive until the call returns.
func callImages(inputs [][]byte, call func(ptrs **C.uint8_t, lens *C.size_t, count C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t) ([]byte, error) {
	var ptrs **C.uint8_t
	var lens *C.size_t
	var lenSlice []C.size_t
	var ptrArray unsafe.Pointer
	if len(inputs) > 0 {
		ptrArray = C.malloc(C.size_t(len(inputs)) * C.size_t(unsafe.Sizeof((*C.uint8_t)(nil))))
		if ptrArray == nil {
			return nil, newError(CodeOperationFailed,
				"Out of memory allocating the input pointer array.",
				"Memory khatm ho gayi.")
		}
		defer C.free(ptrArray)
		ptrs = (**C.uint8_t)(ptrArray)
		arr := unsafe.Slice(ptrs, len(inputs))
		lenSlice = make([]C.size_t, len(inputs))
		for i, in := range inputs {
			arr[i] = cInput(in)
			lenSlice[i] = C.size_t(len(in))
		}
		lens = &lenSlice[0]
	}
	out, err := callOut(func(out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return call(ptrs, lens, C.size_t(len(inputs)), out, outLen)
	})
	runtime.KeepAlive(inputs)
	runtime.KeepAlive(lenSlice)
	return out, err
}

// callJSON runs an FFI call whose output is a JSON document and
// unmarshals it into out (which must be a pointer).
func callJSON(raw []byte, err error, out any) error {
	if err != nil {
		return err
	}
	if err := json.Unmarshal(raw, out); err != nil {
		return newError(CodeOperationFailed,
			"Could not parse the native library's response: "+err.Error(),
			"Native library ka response parse nahi ho paya.")
	}
	return nil
}

// ---------------------------------------------------------------------------
// Per-operation wrappers — the only functions in this package that
// name C types. Callers in other files pass Go values only (the
// *C.char options pointers are created here via cOptionsJSON and
// passed through by type inference).
// ---------------------------------------------------------------------------

// Version returns the native library version, e.g. "0.1.0" (lockstep
// with the core crate's CARGO_PKG_VERSION). The pointer is owned by
// the library and is never freed.
func Version() string {
	p := C.dusrelimage_version()
	if p == nil {
		return ""
	}
	return C.GoString(p)
}

func cCompress(input []byte, options *C.char) ([]byte, error) {
	return callImage(input, func(in *C.uint8_t, inLen C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_compress(in, inLen, options, out, outLen)
	})
}

func cCompressToSize(input []byte, targetBytes uint64, options *C.char) ([]byte, error) {
	return callImage(input, func(in *C.uint8_t, inLen C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_compress_to_size(in, inLen, C.uint64_t(targetBytes), options, out, outLen)
	})
}

func cResize(input []byte, options *C.char) ([]byte, error) {
	return callImage(input, func(in *C.uint8_t, inLen C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_resize(in, inLen, options, out, outLen)
	})
}

func cCrop(input []byte, options *C.char) ([]byte, error) {
	return callImage(input, func(in *C.uint8_t, inLen C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_crop(in, inLen, options, out, outLen)
	})
}

func cConvert(input []byte, options *C.char) ([]byte, error) {
	return callImage(input, func(in *C.uint8_t, inLen C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_convert(in, inLen, options, out, outLen)
	})
}

func cAddWatermark(input []byte, options *C.char) ([]byte, error) {
	return callImage(input, func(in *C.uint8_t, inLen C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_add_watermark(in, inLen, options, out, outLen)
	})
}

func cAnnotate(input []byte, annotationsJSON string) ([]byte, error) {
	cJSON := C.CString(annotationsJSON)
	defer freeCString(cJSON)
	return callImage(input, func(in *C.uint8_t, inLen C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_annotate(in, inLen, cJSON, out, outLen)
	})
}

func cMakeCollage(inputs [][]byte, options *C.char) ([]byte, error) {
	return callImages(inputs, func(ptrs **C.uint8_t, lens *C.size_t, count C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_make_collage(ptrs, lens, count, options, out, outLen)
	})
}

func cPixelate(input []byte, options *C.char) ([]byte, error) {
	return callImage(input, func(in *C.uint8_t, inLen C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_pixelate(in, inLen, options, out, outLen)
	})
}

func cPickColour(input []byte, x, y uint32) ([]byte, error) {
	return callImage(input, func(in *C.uint8_t, inLen C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_pick_colour(in, inLen, C.uint32_t(x), C.uint32_t(y), out, outLen)
	})
}

func cExtractPalette(input []byte, count uint8) ([]byte, error) {
	return callImage(input, func(in *C.uint8_t, inLen C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_extract_palette(in, inLen, C.uint8_t(count), out, outLen)
	})
}

func cGetImageInfo(input []byte, options *C.char) ([]byte, error) {
	return callImage(input, func(in *C.uint8_t, inLen C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_get_image_info(in, inLen, options, out, outLen)
	})
}

func cBulkProcess(inputs [][]byte, op string, options *C.char) ([]byte, error) {
	cOp := C.CString(op)
	defer freeCString(cOp)
	return callImages(inputs, func(ptrs **C.uint8_t, lens *C.size_t, count C.size_t, out **C.uint8_t, outLen *C.size_t) C.int32_t {
		return C.dusrelimage_bulk_process(ptrs, lens, count, cOp, options, out, outLen)
	})
}

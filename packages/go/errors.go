package dusrelimage

import (
	"encoding/json"
	"fmt"
)

// Stable error codes — verbatim from crates/dusrelimage/src/error.rs
// and the C header (crates/dusrelimage-ffi/dusrelimage.h).
const (
	CodeInvalidImage          = "INVALID_IMAGE"
	CodeUnsupportedFormat     = "UNSUPPORTED_FORMAT"
	CodeImageTooLarge         = "IMAGE_TOO_LARGE"
	CodeTargetSizeUnreachable = "TARGET_SIZE_UNREACHABLE"
	CodeCropOutOfBounds       = "CROP_OUT_OF_BOUNDS"
	CodePixelOutOfBounds      = "PIXEL_OUT_OF_BOUNDS"
	CodeInvalidQuality        = "INVALID_QUALITY"
	CodeInvalidDimensions     = "INVALID_DIMENSIONS"
	CodeInvalidArgument       = "INVALID_ARGUMENT"
	CodeEmptyInput            = "EMPTY_INPUT"
	CodeInvalidType           = "INVALID_TYPE"
	// CodeOperationFailed is used only when the native layer reports a
	// failure with no stable code (status 99 UNKNOWN, or an unparsable
	// last-error string) — the same fallback the JS/Python bindings use.
	CodeOperationFailed = "OPERATION_FAILED"
	// CodeUnknown is the code the FFI layer itself reports for status 99.
	CodeUnknown = "UNKNOWN"
)

// FFI status codes — the int32 return of every C function
// (crates/dusrelimage-ffi/dusrelimage.h). Deliberately untyped:
// ffi.go compares them against C.int32_t values, and a typed int32
// constant would not compare against a C.int32_t.
const (
	statusOK                    = 0
	statusInvalidImage          = 1
	statusUnsupportedFormat     = 2
	statusImageTooLarge         = 3
	statusTargetSizeUnreachable = 4
	statusCropOutOfBounds       = 5
	statusPixelOutOfBounds      = 6
	statusInvalidQuality        = 7
	statusInvalidDimensions     = 8
	statusInvalidArgument       = 9
	statusEmptyInput            = 10
	statusInvalidType           = 11
	statusUnknown               = 99
)

// statusCode maps an FFI status int to its stable error code string.
func statusCode(status int32) string {
	switch status {
	case statusInvalidImage:
		return CodeInvalidImage
	case statusUnsupportedFormat:
		return CodeUnsupportedFormat
	case statusImageTooLarge:
		return CodeImageTooLarge
	case statusTargetSizeUnreachable:
		return CodeTargetSizeUnreachable
	case statusCropOutOfBounds:
		return CodeCropOutOfBounds
	case statusPixelOutOfBounds:
		return CodePixelOutOfBounds
	case statusInvalidQuality:
		return CodeInvalidQuality
	case statusInvalidDimensions:
		return CodeInvalidDimensions
	case statusInvalidArgument:
		return CodeInvalidArgument
	case statusEmptyInput:
		return CodeEmptyInput
	case statusInvalidType:
		return CodeInvalidType
	case statusUnknown:
		return CodeUnknown
	default:
		return CodeOperationFailed
	}
}

// DusrelError is every dusrelimage failure: a stable machine-readable
// Code plus human-readable messages in English and Hindi, taken from
// the Rust core via dusrelimage_last_error().
type DusrelError struct {
	Code      string
	MessageEn string
	MessageHi string
}

// Error implements error. Format matches the other bindings:
// "CODE: english message".
func (e *DusrelError) Error() string {
	return fmt.Sprintf("%s: %s", e.Code, e.MessageEn)
}

// ReasonEn is an alias of MessageEn (the other bindings expose both
// message_* and reason_* spellings).
func (e *DusrelError) ReasonEn() string { return e.MessageEn }

// ReasonHi is an alias of MessageHi.
func (e *DusrelError) ReasonHi() string { return e.MessageHi }

// errorJSON is the shape of dusrelimage_last_error():
// {"code": "...", "message_en": "...", "message_hi": "..."}.
// The WASM layer also emits reason_* keys; accept those too.
type errorJSON struct {
	Code      string `json:"code"`
	MessageEn string `json:"message_en"`
	MessageHi string `json:"message_hi"`
	ReasonEn  string `json:"reason_en"`
	ReasonHi  string `json:"reason_hi"`
}

// parseErrorJSON turns a last-error JSON string into a DusrelError.
// It returns nil when the string is empty or not a recognisable error
// object, so the caller can fall back to synthesising from the status.
func parseErrorJSON(s string) *DusrelError {
	if s == "" {
		return nil
	}
	var parsed errorJSON
	if err := json.Unmarshal([]byte(s), &parsed); err != nil {
		return nil
	}
	if parsed.Code == "" {
		return nil
	}
	en := parsed.MessageEn
	if en == "" {
		en = parsed.ReasonEn
	}
	hi := parsed.MessageHi
	if hi == "" {
		hi = parsed.ReasonHi
	}
	return &DusrelError{Code: parsed.Code, MessageEn: en, MessageHi: hi}
}

// newError builds a client-side DusrelError (for the cheap checks this
// package does before calling the native layer, mirroring the JS
// wrapper's client-side validation and the Rust core's messages).
func newError(code, messageEn, messageHi string) *DusrelError {
	return &DusrelError{Code: code, MessageEn: messageEn, MessageHi: messageHi}
}

func emptyInputError(what string) *DusrelError {
	return newError(CodeEmptyInput,
		fmt.Sprintf("%s is empty.", what),
		fmt.Sprintf("%s khaali hai.", what))
}

func invalidQualityError(quality int) *DusrelError {
	return newError(CodeInvalidQuality,
		fmt.Sprintf("Quality %d is invalid. Quality must be between 1 and 100.", quality),
		fmt.Sprintf("Quality %d galat hai. Quality 1 se 100 ke beech honi chahiye.", quality))
}

func invalidArgumentError(messageEn, messageHi string) *DusrelError {
	return newError(CodeInvalidArgument, messageEn, messageHi)
}

func invalidDimensionsError(detailEn, detailHi string) *DusrelError {
	return newError(CodeInvalidDimensions,
		fmt.Sprintf("Invalid dimensions: %s", detailEn),
		fmt.Sprintf("Galat dimensions hain: %s", detailHi))
}

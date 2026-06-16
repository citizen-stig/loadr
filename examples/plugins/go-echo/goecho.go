// go-echo — an example loadr protocol plugin written in Go.
//
// Built with `go build -buildmode=c-shared`, it implements loadr's frozen
// C-ABI plugin contract (docs/src/plugins/c-abi.md) via //export directives —
// proving loadr plugins can be authored in Go, not just Rust or C. It serves
// the `goecho://` URL scheme and echoes each request body back, status 200.
//
// Build (Linux):   make            -> libloadr_plugin_goecho.so
// Build (macOS):   make            -> libloadr_plugin_goecho.dylib
// Build (Windows): make            -> loadr_plugin_goecho.dll
//
// The JSON shapes match loadr_plugin_api::native::{FfiRequest, FfiResponse}.
// Unlike the dependency-free C example, Go gives us encoding/json for free, so
// parsing and emitting the contract JSON is just struct (un)marshalling.
package main

/*
#include <stdint.h>
#include <stdlib.h>
*/
import "C"

import (
	"encoding/json"
	"unsafe"
)

// The C-ABI version this plugin targets. Must equal the host's
// LOADR_C_ABI_VERSION or the host refuses to load the plugin.
const loadrCABIVersion = 1

func main() {} // required by -buildmode=c-shared; never called

//export loadr_plugin_abi_version
func loadr_plugin_abi_version() C.uint32_t {
	return C.uint32_t(loadrCABIVersion)
}

//export loadr_plugin_free
func loadr_plugin_free(ptr *C.uint8_t, length C.size_t) {
	_ = length // C.free tracks the real size; len is passed for ABI symmetry
	C.free(unsafe.Pointer(ptr))
}

//export loadr_plugin_info
func loadr_plugin_info(outLen *C.size_t) *C.uint8_t {
	info, _ := json.Marshal(map[string]any{
		"name":        "goecho",
		"version":     "0.1.0",
		"kind":        "protocol",
		"description": "Echo protocol plugin written in Go (C-ABI)",
		"schemes":     []string{"goecho"},
	})
	return cBytes(info, outLen)
}

// ffiRequest mirrors the fields of loadr_plugin_api::native::FfiRequest that
// this plugin reads. The request body arrives base64-encoded in body_b64.
type ffiRequest struct {
	Method  string `json:"method"`
	URL     string `json:"url"`
	BodyB64 string `json:"body_b64"`
}

//export loadr_plugin_execute
func loadr_plugin_execute(req *C.uint8_t, reqLen C.size_t, outLen *C.size_t) *C.uint8_t {
	in := C.GoBytes(unsafe.Pointer(req), C.int(reqLen))

	var r ffiRequest
	_ = json.Unmarshal(in, &r) // tolerate malformed input; echo an empty body
	method := r.Method
	if method == "" {
		method = "GET"
	}

	// FfiResponse: echo the (still base64-encoded) request body verbatim with
	// an HTTP-style 200. extras flows through to plugin metrics/output.
	resp, _ := json.Marshal(map[string]any{
		"status":      200,
		"status_text": "OK",
		"headers":     [][]string{{"x-goecho", "1"}},
		"body_b64":    r.BodyB64,
		"duration_ms": 0.0,
		"extras": map[string]any{
			"echoed_by": "go-echo",
			"method":    method,
		},
	})
	return cBytes(resp, outLen)
}

// cBytes copies b into a buffer allocated by the C allocator (so the host's
// loadr_plugin_free — which calls C.free — matches), writing the length to
// *outLen. A nil return with *outLen == 0 is the contract's "empty buffer".
func cBytes(b []byte, outLen *C.size_t) *C.uint8_t {
	if len(b) == 0 {
		*outLen = 0
		return nil
	}
	p := C.malloc(C.size_t(len(b)))
	if p == nil {
		*outLen = 0
		return nil
	}
	copy(unsafe.Slice((*byte)(p), len(b)), b)
	*outLen = C.size_t(len(b))
	return (*C.uint8_t)(p)
}

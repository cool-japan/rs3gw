// Text Replace WASM Plugin (TinyGo)
//
// This plugin performs simple text find-and-replace operations.
//
// Build with:
//   tinygo build -o text_replace.wasm -target wasi main.go

package main

import (
	"strings"
	"unsafe"
)

//export transform_with_params
func transformWithParams(inputPtr, inputLen, paramsPtr, paramsLen uint32) uint64 {
	// Read input data
	input := ptrToString(inputPtr, inputLen)

	// Read parameters (format: "find|replace")
	params := ptrToString(paramsPtr, paramsLen)
	parts := strings.SplitN(params, "|", 2)

	if len(parts) != 2 {
		// Invalid parameters, return original
		return packResult(input)
	}

	find := parts[0]
	replace := parts[1]

	// Perform replacement
	result := strings.ReplaceAll(input, find, replace)

	return packResult(result)
}

//export allocate
func allocate(size uint32) uint32 {
	buf := make([]byte, size)
	ptr := &buf[0]
	return uint32(uintptr(unsafe.Pointer(ptr)))
}

//export deallocate
func deallocate(ptr uint32, size uint32) {
	// Go's garbage collector will handle this
	// This is a no-op for compatibility
}

// Helper: convert pointer and length to string
func ptrToString(ptr, length uint32) string {
	if length == 0 {
		return ""
	}

	data := make([]byte, length)
	srcPtr := uintptr(ptr)

	for i := range data {
		data[i] = *(*byte)(unsafe.Pointer(srcPtr + uintptr(i)))
	}

	return string(data)
}

// Helper: pack result string into return value
func packResult(s string) uint64 {
	data := []byte(s)
	length := uint64(len(data))

	if length == 0 {
		return 0
	}

	ptr := uint64(uintptr(unsafe.Pointer(&data[0])))

	// Pack: high 32 bits = length, low 32 bits = pointer
	return (length << 32) | (ptr & 0xFFFFFFFF)
}

// Required for WASI
func main() {}

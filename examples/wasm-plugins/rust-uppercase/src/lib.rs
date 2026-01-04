//! Simple WASM plugin that converts text to uppercase
//!
//! This example demonstrates how to create a basic WASM plugin for rs3gw
//! that performs a simple text transformation.
//!
//! Build with:
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release
//! ```

use core::slice;

/// Transform input data to uppercase
///
/// This function is called from the rs3gw WASM runtime.
/// It receives a pointer to the input data and its length,
/// and returns a pointer to the transformed data.
#[no_mangle]
pub extern "C" fn transform(input_ptr: *const u8, input_len: usize) -> *const u8 {
    // Safety: We trust the host to provide valid pointers
    let input = unsafe { slice::from_raw_parts(input_ptr, input_len) };

    // Convert input to string
    let text = match core::str::from_utf8(input) {
        Ok(s) => s,
        Err(_) => return input_ptr, // Return original if not valid UTF-8
    };

    // Transform to uppercase
    let transformed = text.to_uppercase();

    // Allocate memory for output
    let output = transformed.into_bytes();
    let output_ptr = output.as_ptr();

    // Leak the memory so it persists for the host to read
    core::mem::forget(output);

    output_ptr
}

/// Get the length of the last transformation output
///
/// The host calls this after transform() to determine how many bytes to read
#[no_mangle]
pub extern "C" fn get_output_length() -> usize {
    // This is a simplified example - in production you'd track this in global state
    0 // Placeholder - host should use transform_with_length instead
}

/// Transform input data to uppercase and return both pointer and length
///
/// This is a more robust version that returns both the output pointer and length.
/// Returns a packed u64 where high 32 bits = length, low 32 bits = pointer offset
#[no_mangle]
pub extern "C" fn transform_with_length(input_ptr: *const u8, input_len: usize) -> u64 {
    // Safety: We trust the host to provide valid pointers
    let input = unsafe { slice::from_raw_parts(input_ptr, input_len) };

    // Convert input to string
    let text = match core::str::from_utf8(input) {
        Ok(s) => s,
        Err(_) => {
            // Return original pointer and length on error
            return ((input_len as u64) << 32) | (input_ptr as u64 & 0xFFFFFFFF);
        }
    };

    // Transform to uppercase
    let transformed = text.to_uppercase();
    let output_len = transformed.len();

    // Allocate memory for output
    let output = transformed.into_bytes();
    let output_ptr = output.as_ptr() as usize;

    // Leak the memory so it persists for the host to read
    core::mem::forget(output);

    // Pack length and pointer into single u64
    ((output_len as u64) << 32) | (output_ptr as u64 & 0xFFFFFFFF)
}

/// Allocate memory in WASM linear memory
///
/// The host can call this to allocate memory for input data
#[no_mangle]
pub extern "C" fn allocate(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    core::mem::forget(buf);
    ptr
}

/// Deallocate memory
///
/// The host should call this to free memory when done
#[no_mangle]
pub extern "C" fn deallocate(ptr: *mut u8, size: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uppercase_transform() {
        let input = b"hello world";
        let result = transform_with_length(input.as_ptr(), input.len());

        // Extract length and pointer
        let output_len = (result >> 32) as usize;
        let output_ptr = (result & 0xFFFFFFFF) as *const u8;

        // Read the output
        let output = unsafe { slice::from_raw_parts(output_ptr, output_len) };
        let output_str = core::str::from_utf8(output).expect("valid UTF-8");

        assert_eq!(output_str, "HELLO WORLD");
    }
}

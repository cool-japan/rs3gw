//! JSON Field Filter WASM Plugin
//!
//! This plugin filters JSON objects to include only specified fields.
//! Useful for data minimization and privacy compliance.
//!
//! Build with:
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release
//! ```

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::slice;
use serde_json::Value;

/// Filter JSON to include only specified fields
///
/// Parameters format: comma-separated list of field names
/// Example: "name,email,age"
///
/// Returns filtered JSON with only the specified fields
#[no_mangle]
pub extern "C" fn transform_with_params(
    input_ptr: *const u8,
    input_len: usize,
    params_ptr: *const u8,
    params_len: usize,
) -> u64 {
    // Safety: Trust the host to provide valid pointers
    let input = unsafe { slice::from_raw_parts(input_ptr, input_len) };
    let params = unsafe { slice::from_raw_parts(params_ptr, params_len) };

    // Parse parameters (comma-separated field names)
    let params_str = match core::str::from_utf8(params) {
        Ok(s) => s,
        Err(_) => return pack_error(b"Invalid UTF-8 in parameters"),
    };

    let fields: Vec<&str> = params_str.split(',').map(|s| s.trim()).collect();

    // Parse input JSON
    let json_value: Value = match serde_json::from_slice(input) {
        Ok(v) => v,
        Err(_) => return pack_error(b"Invalid JSON input"),
    };

    // Filter the JSON object
    let filtered = match json_value {
        Value::Object(mut map) => {
            // Keep only specified fields
            map.retain(|k, _| fields.contains(&k.as_str()));
            Value::Object(map)
        }
        Value::Array(arr) => {
            // Filter each object in the array
            let filtered_arr: Vec<Value> = arr
                .into_iter()
                .map(|item| {
                    if let Value::Object(mut map) = item {
                        map.retain(|k, _| fields.contains(&k.as_str()));
                        Value::Object(map)
                    } else {
                        item
                    }
                })
                .collect();
            Value::Array(filtered_arr)
        }
        other => other, // Non-object values pass through unchanged
    };

    // Serialize back to JSON
    let output_string = match serde_json::to_string(&filtered) {
        Ok(s) => s,
        Err(_) => return pack_error(b"JSON serialization failed"),
    };

    let output = output_string.into_bytes();
    pack_result(output)
}

/// Pack successful result into return value
fn pack_result(mut output: Vec<u8>) -> u64 {
    let output_len = output.len();
    let output_ptr = output.as_mut_ptr() as usize;
    core::mem::forget(output);
    ((output_len as u64) << 32) | (output_ptr as u64 & 0xFFFFFFFF)
}

/// Pack error message into return value
fn pack_error(error_msg: &[u8]) -> u64 {
    let mut error_vec = Vec::from(error_msg);
    pack_result(error_vec)
}

/// Allocate memory in WASM linear memory
#[no_mangle]
pub extern "C" fn allocate(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    core::mem::forget(buf);
    ptr
}

/// Deallocate memory
#[no_mangle]
pub extern "C" fn deallocate(ptr: *mut u8, size: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, size);
    }
}

/// Initialize global allocator
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

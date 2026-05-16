//! Uppercase WASM Plugin Example
//!
//! This is a simple example WASM plugin for rs3gw that converts text to uppercase.
//!
//! # Plugin Contract
//!
//! The rs3gw WASM plugin system expects the following exports:
//! - `memory`: Linear memory for data transfer
//! - `alloc(size: u32) -> ptr: u32`: Allocate memory for input data
//! - `transform(ptr: u32, len: u32) -> result: u64`: Transform data (returns packed ptr:len)
//!
//! # Build Instructions
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release
//! ```
//!
//! The compiled WASM module will be in:
//! `target/wasm32-unknown-unknown/release/uppercase_wasm_plugin.wasm`

// WASM target: custom allocator and panic handler (no_std environment)
#[cfg(target_arch = "wasm32")]
mod wasm_support {
    use core::alloc::{GlobalAlloc, Layout};
    use core::panic::PanicInfo;

    /// Simple bump allocator for WASM
    pub struct BumpAllocator;

    static mut HEAP: [u8; 64 * 1024] = [0; 64 * 1024]; // 64KB heap
    static mut HEAP_POS: usize = 0;

    unsafe impl GlobalAlloc for BumpAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let size = layout.size();
            let align = layout.align();

            // Align the current position
            let pos = (HEAP_POS + align - 1) & !(align - 1);

            // Check if we have enough space
            if pos + size > HEAP.len() {
                return core::ptr::null_mut();
            }

            HEAP_POS = pos + size;
            HEAP.as_mut_ptr().add(pos)
        }

        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
            // Bump allocator doesn't support deallocation
        }
    }

    #[global_allocator]
    static ALLOCATOR: BumpAllocator = BumpAllocator;

    /// Panic handler for no_std
    #[panic_handler]
    fn panic(_info: &PanicInfo) -> ! {
        loop {}
    }

    /// Allocate memory using our custom bump allocator
    pub fn bump_alloc(size: u32) -> u32 {
        let layout = match Layout::from_size_align(size as usize, 1) {
            Ok(layout) => layout,
            Err(_) => return 0,
        };

        unsafe {
            let ptr = ALLOCATOR.alloc(layout);
            if ptr.is_null() {
                0
            } else {
                ptr as u32
            }
        }
    }
}

/// Allocate memory for input data
///
/// # Arguments
/// * `size` - Number of bytes to allocate
///
/// # Returns
/// Pointer to allocated memory (0 if allocation failed)
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> u32 {
    #[cfg(target_arch = "wasm32")]
    {
        wasm_support::bump_alloc(size)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let layout = match std::alloc::Layout::from_size_align(size as usize, 1) {
            Ok(layout) => layout,
            Err(_) => return 0,
        };
        unsafe {
            let ptr = std::alloc::alloc(layout);
            if ptr.is_null() {
                0
            } else {
                ptr as u32
            }
        }
    }
}

/// Transform input data to uppercase
///
/// # Arguments
/// * `ptr` - Pointer to input data in linear memory
/// * `len` - Length of input data in bytes
///
/// # Returns
/// Packed u64 with output pointer in high 32 bits and length in low 32 bits
///
/// # Example
/// ```
/// let result = transform(input_ptr, input_len);
/// let output_ptr = (result >> 32) as u32;
/// let output_len = (result & 0xFFFFFFFF) as u32;
/// ```
#[no_mangle]
pub extern "C" fn transform(ptr: u32, len: u32) -> u64 {
    unsafe {
        // Read input data from linear memory
        let input_slice = core::slice::from_raw_parts(ptr as *const u8, len as usize);

        // Allocate output buffer (same size as input)
        let output_ptr = alloc(len);
        if output_ptr == 0 {
            return 0; // Allocation failed
        }

        let output_slice = core::slice::from_raw_parts_mut(output_ptr as *mut u8, len as usize);

        // Transform: convert to uppercase
        for (i, &byte) in input_slice.iter().enumerate() {
            output_slice[i] = if byte.is_ascii_lowercase() {
                byte - 32 // Convert to uppercase
            } else {
                byte // Keep as-is
            };
        }

        // Pack output pointer and length into u64
        ((output_ptr as u64) << 32) | (len as u64)
    }
}

/// Get plugin version (optional, for plugin metadata)
#[no_mangle]
pub extern "C" fn plugin_version() -> u32 {
    1 // Version 1
}

/// Get plugin name pointer and length (optional, for plugin metadata)
#[no_mangle]
pub extern "C" fn plugin_name() -> u64 {
    const NAME: &[u8] = b"uppercase";
    let ptr = NAME.as_ptr() as u32;
    let len = NAME.len() as u32;
    ((ptr as u64) << 32) | (len as u64)
}

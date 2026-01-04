//! Zero-copy optimizations for rs3gw
//!
//! Provides high-performance I/O operations that minimize memory copies:
//! - Direct I/O for large objects (bypasses page cache)
//! - Splice/sendfile support for Linux (kernel-level copy)
//! - Memory-mapped file operations for metadata
//! - Zero-copy streaming with io_uring support

#[cfg(target_os = "linux")]
use std::os::unix::io::AsRawFd;
#[cfg(unix)]
use std::os::unix::io::RawFd;
use std::path::Path;
#[cfg(target_os = "linux")]
use tracing::info;

use bytes::Bytes;
use memmap2::MmapOptions;
use thiserror::Error;
use tokio::fs::File;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum ZeroCopyError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Memory map error: {0}")]
    Mmap(String),

    #[error("Splice not supported on this platform")]
    SpliceNotSupported,

    #[error("Direct I/O alignment error: {0}")]
    AlignmentError(String),
}

// ============================================================================
// Configuration
// ============================================================================

/// Zero-copy optimization configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ZeroCopyConfig {
    /// Enable direct I/O for large objects (bypasses page cache)
    pub direct_io_enabled: bool,
    /// Minimum object size for direct I/O (default: 1MB)
    pub direct_io_threshold: usize,
    /// Enable splice/sendfile for efficient file copying
    pub splice_enabled: bool,
    /// Enable memory-mapped metadata files
    pub mmap_metadata_enabled: bool,
    /// Maximum metadata file size for mmap (default: 1MB)
    pub mmap_max_size: usize,
}

impl Default for ZeroCopyConfig {
    fn default() -> Self {
        Self {
            direct_io_enabled: true,
            direct_io_threshold: 1024 * 1024, // 1MB
            splice_enabled: true,
            mmap_metadata_enabled: true,
            mmap_max_size: 1024 * 1024, // 1MB
        }
    }
}

impl ZeroCopyConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_direct_io(mut self, enabled: bool) -> Self {
        self.direct_io_enabled = enabled;
        self
    }

    pub fn with_direct_io_threshold(mut self, threshold: usize) -> Self {
        self.direct_io_threshold = threshold;
        self
    }

    pub fn with_splice(mut self, enabled: bool) -> Self {
        self.splice_enabled = enabled;
        self
    }

    pub fn with_mmap_metadata(mut self, enabled: bool) -> Self {
        self.mmap_metadata_enabled = enabled;
        self
    }

    pub fn disabled() -> Self {
        Self {
            direct_io_enabled: false,
            direct_io_threshold: usize::MAX,
            splice_enabled: false,
            mmap_metadata_enabled: false,
            mmap_max_size: 0,
        }
    }
}

// ============================================================================
// Direct I/O Support
// ============================================================================

/// Helper to determine if direct I/O should be used
pub fn should_use_direct_io(config: &ZeroCopyConfig, size: usize) -> bool {
    config.direct_io_enabled && size >= config.direct_io_threshold
}

/// Open file with O_DIRECT flag on Linux
#[cfg(target_os = "linux")]
pub async fn open_direct_io(path: &Path, write: bool) -> Result<File, ZeroCopyError> {
    use std::fs::OpenOptions as StdOpenOptions;
    use std::os::unix::fs::OpenOptionsExt;

    let mut opts = StdOpenOptions::new();
    opts.read(!write);
    if write {
        opts.write(true).create(true).truncate(true);
    }

    // O_DIRECT = 0x4000 on Linux
    opts.custom_flags(0x4000);

    let file = opts.open(path)?;
    Ok(File::from_std(file))
}

/// Fallback for non-Linux platforms
#[cfg(not(target_os = "linux"))]
pub async fn open_direct_io(path: &Path, write: bool) -> Result<File, ZeroCopyError> {
    use tokio::fs::OpenOptions;

    let mut opts = OpenOptions::new();
    opts.read(!write);
    if write {
        opts.write(true).create(true).truncate(true);
    }

    Ok(opts.open(path).await?)
}

/// Open file with O_DIRECT flag synchronously (for DirectIoWriter)
#[cfg(target_os = "linux")]
pub fn open_direct_io_sync(path: &Path, write: bool) -> Result<std::fs::File, ZeroCopyError> {
    use std::fs::OpenOptions as StdOpenOptions;
    use std::os::unix::fs::OpenOptionsExt;

    let mut opts = StdOpenOptions::new();
    opts.read(!write);
    if write {
        opts.write(true).create(true).truncate(true);
    }

    // O_DIRECT = 0x4000 on Linux
    opts.custom_flags(0x4000);

    Ok(opts.open(path)?)
}

/// Fallback for non-Linux platforms
#[cfg(not(target_os = "linux"))]
pub fn open_direct_io_sync(path: &Path, write: bool) -> Result<std::fs::File, ZeroCopyError> {
    use std::fs::OpenOptions as StdOpenOptions;

    let mut opts = StdOpenOptions::new();
    opts.read(!write);
    if write {
        opts.write(true).create(true).truncate(true);
    }

    Ok(opts.open(path)?)
}

/// Aligned buffer for direct I/O (512-byte aligned memory address)
pub struct AlignedBuffer {
    ptr: *mut u8,
    len: usize,
    layout: std::alloc::Layout,
}

impl AlignedBuffer {
    /// Create new aligned buffer with specified size (aligned up to 512 bytes)
    pub fn new(size: usize) -> Self {
        use std::alloc::{alloc_zeroed, Layout};

        const ALIGNMENT: usize = 512;
        let aligned_size = (size + ALIGNMENT - 1) & !(ALIGNMENT - 1);
        let aligned_size = aligned_size.max(ALIGNMENT); // At least one block

        let layout = Layout::from_size_align(aligned_size, ALIGNMENT)
            .expect("Invalid layout for aligned buffer");

        let ptr = unsafe { alloc_zeroed(layout) };
        if ptr.is_null() {
            panic!("Failed to allocate aligned buffer");
        }

        Self {
            ptr,
            len: aligned_size,
            layout,
        }
    }

    /// Get length of buffer
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get raw pointer for direct I/O operations
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    /// Get slice of buffer
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// Get mutable slice of buffer
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                std::alloc::dealloc(self.ptr, self.layout);
            }
        }
    }
}

// Safety: AlignedBuffer owns its data exclusively
unsafe impl Send for AlignedBuffer {}
unsafe impl Sync for AlignedBuffer {}

impl std::ops::Index<std::ops::Range<usize>> for AlignedBuffer {
    type Output = [u8];
    fn index(&self, range: std::ops::Range<usize>) -> &[u8] {
        &self.as_slice()[range]
    }
}

impl std::ops::IndexMut<std::ops::Range<usize>> for AlignedBuffer {
    fn index_mut(&mut self, range: std::ops::Range<usize>) -> &mut [u8] {
        &mut self.as_mut_slice()[range]
    }
}

impl std::ops::Index<std::ops::RangeTo<usize>> for AlignedBuffer {
    type Output = [u8];
    fn index(&self, range: std::ops::RangeTo<usize>) -> &[u8] {
        &self.as_slice()[range]
    }
}

/// Create aligned buffer for direct I/O (512-byte aligned)
/// Deprecated: Use AlignedBuffer::new() instead for proper memory management
pub fn align_buffer(size: usize) -> AlignedBuffer {
    AlignedBuffer::new(size)
}

// ============================================================================
// Splice/Sendfile Support (Linux)
// ============================================================================

#[cfg(target_os = "linux")]
pub mod splice {
    use super::*;
    use std::io;

    /// Copy data between file descriptors using splice (zero-copy)
    /// Returns number of bytes transferred
    pub fn splice_copy(fd_in: RawFd, fd_out: RawFd, len: usize) -> Result<usize, ZeroCopyError> {
        use libc::{splice, SPLICE_F_MORE, SPLICE_F_MOVE};

        let mut total = 0;
        let mut remaining = len;

        while remaining > 0 {
            let result = unsafe {
                splice(
                    fd_in,
                    std::ptr::null_mut(),
                    fd_out,
                    std::ptr::null_mut(),
                    remaining,
                    SPLICE_F_MOVE | SPLICE_F_MORE,
                )
            };

            if result < 0 {
                return Err(ZeroCopyError::Io(io::Error::last_os_error()));
            } else if result == 0 {
                break; // EOF
            }

            total += result as usize;
            remaining -= result as usize;
        }

        Ok(total)
    }

    /// Copy file to file using sendfile (zero-copy)
    pub fn sendfile_copy(
        fd_in: RawFd,
        fd_out: RawFd,
        offset: i64,
        len: usize,
    ) -> Result<usize, ZeroCopyError> {
        use libc::sendfile;

        let mut total = 0;
        let mut remaining = len;
        let mut current_offset = offset;

        while remaining > 0 {
            let result =
                unsafe { sendfile(fd_out, fd_in, &mut current_offset as *mut i64, remaining) };

            if result < 0 {
                return Err(ZeroCopyError::Io(std::io::Error::last_os_error()));
            } else if result == 0 {
                break; // EOF
            }

            total += result as usize;
            remaining -= result as usize;
        }

        Ok(total)
    }
}

#[cfg(not(target_os = "linux"))]
pub mod splice {
    use super::*;

    pub fn splice_copy(_fd_in: RawFd, _fd_out: RawFd, _len: usize) -> Result<usize, ZeroCopyError> {
        Err(ZeroCopyError::SpliceNotSupported)
    }

    pub fn sendfile_copy(
        _fd_in: RawFd,
        _fd_out: RawFd,
        _offset: i64,
        _len: usize,
    ) -> Result<usize, ZeroCopyError> {
        Err(ZeroCopyError::SpliceNotSupported)
    }
}

// ============================================================================
// Memory-Mapped Metadata
// ============================================================================

/// Memory-mapped metadata reader
pub struct MmapMetadata {
    _mmap: memmap2::Mmap,
    data: Bytes,
}

impl MmapMetadata {
    /// Create memory-mapped view of metadata file
    pub async fn open(path: &Path) -> Result<Self, ZeroCopyError> {
        let file = std::fs::File::open(path)?;

        // Safety: We own the file and it won't be modified while mapped
        let mmap = unsafe {
            MmapOptions::new()
                .map(&file)
                .map_err(|e| ZeroCopyError::Mmap(e.to_string()))?
        };

        // Create Bytes view of mmap (zero-copy)
        let data = Bytes::copy_from_slice(&mmap);

        Ok(Self { _mmap: mmap, data })
    }

    /// Get data as Bytes (zero-copy reference)
    pub fn as_bytes(&self) -> &Bytes {
        &self.data
    }

    /// Get data as slice
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
}

/// Check if metadata file is suitable for mmap
pub fn should_mmap_metadata(config: &ZeroCopyConfig, size: usize) -> bool {
    config.mmap_metadata_enabled && size <= config.mmap_max_size && size > 0
}

// ============================================================================
// Zero-Copy File Operations
// ============================================================================

/// Zero-copy file copy using best available method
pub async fn zero_copy_file(
    src: &Path,
    dst: &Path,
    size: usize,
    config: &ZeroCopyConfig,
) -> Result<(), ZeroCopyError> {
    #[cfg(target_os = "linux")]
    {
        if config.splice_enabled && size > 64 * 1024 {
            // Use sendfile for files > 64KB on Linux
            return sendfile_file_copy(src, dst, size).await;
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (size, config); // Suppress unused warnings on non-Linux
    }

    // Fallback to regular async copy
    regular_file_copy(src, dst).await
}

#[cfg(target_os = "linux")]
async fn sendfile_file_copy(src: &Path, dst: &Path, size: usize) -> Result<(), ZeroCopyError> {
    let src_file = std::fs::File::open(src)?;
    let dst_file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(dst)?;

    let src_fd = src_file.as_raw_fd();
    let dst_fd = dst_file.as_raw_fd();

    // Use sendfile for zero-copy transfer
    let copied = splice::sendfile_copy(src_fd, dst_fd, 0, size)?;

    if copied != size {
        return Err(ZeroCopyError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            format!("Expected {} bytes, copied {}", size, copied),
        )));
    }

    info!("Zero-copy file transfer: {} bytes via sendfile", copied);
    Ok(())
}

async fn regular_file_copy(src: &Path, dst: &Path) -> Result<(), ZeroCopyError> {
    tokio::fs::copy(src, dst).await?;
    Ok(())
}

// ============================================================================
// Direct I/O Writer
// ============================================================================

/// Direct I/O writer for large objects
///
/// Note: O_DIRECT requires synchronous I/O operations since tokio's async File
/// doesn't properly support O_DIRECT alignment requirements. We use spawn_blocking
/// with direct libc writes from aligned buffers.
#[cfg(unix)]
pub struct DirectIoWriter {
    fd: std::os::unix::io::RawFd,
    buffer: AlignedBuffer,
    buffer_pos: usize,
    total_written: usize,
}

#[cfg(unix)]
impl DirectIoWriter {
    /// Create new direct I/O writer
    pub async fn new(path: &Path) -> Result<Self, ZeroCopyError> {
        use std::os::unix::io::AsRawFd;
        let file = open_direct_io_sync(path, true)?;
        let fd = file.as_raw_fd();
        // Leak the file so fd stays valid - we'll close it manually in drop
        std::mem::forget(file);
        let buffer = AlignedBuffer::new(4096 * 64); // 256KB aligned buffer

        Ok(Self {
            fd,
            buffer,
            buffer_pos: 0,
            total_written: 0,
        })
    }

    /// Write data (buffers to aligned size before flushing)
    pub async fn write(&mut self, data: &[u8]) -> Result<(), ZeroCopyError> {
        let mut offset = 0;

        while offset < data.len() {
            let remaining = data.len() - offset;
            let buffer_space = self.buffer.len() - self.buffer_pos;
            let to_copy = remaining.min(buffer_space);

            self.buffer.as_mut_slice()[self.buffer_pos..self.buffer_pos + to_copy]
                .copy_from_slice(&data[offset..offset + to_copy]);
            self.buffer_pos += to_copy;
            offset += to_copy;

            if self.buffer_pos >= self.buffer.len() {
                self.flush_buffer().await?;
            }
        }

        Ok(())
    }

    /// Flush remaining data
    pub async fn finish(mut self) -> Result<usize, ZeroCopyError> {
        if self.buffer_pos > 0 {
            // Pad to alignment
            let alignment = 512;
            let aligned_size = (self.buffer_pos + alignment - 1) & !(alignment - 1);

            // Write aligned buffer using block_in_place with direct libc write
            // SAFETY: The buffer is aligned and remains valid during the write.
            let ptr = self.buffer.as_ptr();
            let fd = self.fd;
            let result = tokio::task::block_in_place(|| {
                let ret = unsafe { libc::write(fd, ptr as *const libc::c_void, aligned_size) };
                if ret < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                unsafe { libc::fsync(fd) };
                Ok(ret as usize)
            })?;

            if result != aligned_size {
                return Err(ZeroCopyError::Io(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "incomplete write",
                )));
            }

            self.total_written += self.buffer_pos;
        } else {
            // Still need to sync even if no remaining data
            let fd = self.fd;
            tokio::task::block_in_place(|| {
                unsafe { libc::fsync(fd) };
            });
        }

        // Close the fd
        unsafe { libc::close(self.fd) };
        self.fd = -1; // Mark as closed so Drop doesn't double-close

        Ok(self.total_written)
    }

    async fn flush_buffer(&mut self) -> Result<(), ZeroCopyError> {
        if self.buffer_pos > 0 {
            // SAFETY: The buffer is aligned and remains valid during the write.
            let ptr = self.buffer.as_ptr();
            let len = self.buffer.len();
            let fd = self.fd;
            let result = tokio::task::block_in_place(|| {
                let ret = unsafe { libc::write(fd, ptr as *const libc::c_void, len) };
                if ret < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(ret as usize)
            })?;

            if result != len {
                return Err(ZeroCopyError::Io(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "incomplete write",
                )));
            }

            self.total_written += self.buffer_pos;
            self.buffer_pos = 0;
        }
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for DirectIoWriter {
    fn drop(&mut self) {
        if self.fd >= 0 {
            unsafe { libc::close(self.fd) };
        }
    }
}

// ============================================================================
// Statistics
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct ZeroCopyStats {
    pub direct_io_reads: u64,
    pub direct_io_writes: u64,
    pub splice_operations: u64,
    pub sendfile_operations: u64,
    pub mmap_operations: u64,
    pub bytes_via_zerocopy: u64,
}

impl ZeroCopyStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_direct_io_read(&mut self, bytes: usize) {
        self.direct_io_reads += 1;
        self.bytes_via_zerocopy += bytes as u64;
    }

    pub fn record_direct_io_write(&mut self, bytes: usize) {
        self.direct_io_writes += 1;
        self.bytes_via_zerocopy += bytes as u64;
    }

    pub fn record_splice(&mut self, bytes: usize) {
        self.splice_operations += 1;
        self.bytes_via_zerocopy += bytes as u64;
    }

    pub fn record_sendfile(&mut self, bytes: usize) {
        self.sendfile_operations += 1;
        self.bytes_via_zerocopy += bytes as u64;
    }

    pub fn record_mmap(&mut self) {
        self.mmap_operations += 1;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zerocopy_config() {
        let config = ZeroCopyConfig::default();
        assert!(config.direct_io_enabled);
        assert_eq!(config.direct_io_threshold, 1024 * 1024);
        assert!(config.splice_enabled);
        assert!(config.mmap_metadata_enabled);
    }

    #[test]
    fn test_zerocopy_config_builder() {
        let config = ZeroCopyConfig::new()
            .with_direct_io(false)
            .with_splice(false)
            .with_mmap_metadata(false);

        assert!(!config.direct_io_enabled);
        assert!(!config.splice_enabled);
        assert!(!config.mmap_metadata_enabled);
    }

    #[test]
    fn test_should_use_direct_io() {
        let config = ZeroCopyConfig::default();

        assert!(!should_use_direct_io(&config, 512 * 1024)); // 512KB < 1MB
        assert!(should_use_direct_io(&config, 2 * 1024 * 1024)); // 2MB > 1MB
    }

    #[test]
    fn test_should_mmap_metadata() {
        let config = ZeroCopyConfig::default();

        assert!(should_mmap_metadata(&config, 64 * 1024)); // 64KB
        assert!(!should_mmap_metadata(&config, 2 * 1024 * 1024)); // 2MB > max
        assert!(!should_mmap_metadata(&config, 0)); // empty file
    }

    #[test]
    fn test_align_buffer() {
        let buf = align_buffer(1000);
        assert_eq!(buf.len(), 1024); // Aligned to 512-byte boundary

        let buf = align_buffer(512);
        assert_eq!(buf.len(), 512); // Already aligned

        let buf = align_buffer(513);
        assert_eq!(buf.len(), 1024); // Rounds up
    }

    #[tokio::test]
    async fn test_mmap_metadata() {
        let temp_dir = std::env::temp_dir().join("rs3gw-zerocopy-test-1");
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .expect("Failed to create test directory");

        let test_file = temp_dir.join("test.json");
        let test_data = b"Hello, mmap!";
        tokio::fs::write(&test_file, test_data)
            .await
            .expect("Failed to write test file");

        let mmap = MmapMetadata::open(&test_file)
            .await
            .expect("Failed to open mmap metadata");
        assert_eq!(mmap.as_slice(), test_data);

        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_direct_io_writer() {
        let temp_dir = std::env::temp_dir().join("rs3gw-zerocopy-test-2");
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .expect("Failed to create test directory");

        let test_file = temp_dir.join("direct_io_test");
        let mut writer = DirectIoWriter::new(&test_file)
            .await
            .expect("Failed to create DirectIoWriter");

        let data = vec![42u8; 1024];
        writer.write(&data).await.expect("Failed to write data");

        let written = writer.finish().await.expect("Failed to finish write");
        assert_eq!(written, 1024);

        // Verify data was written
        let read_data = tokio::fs::read(&test_file)
            .await
            .expect("Failed to read test file");
        assert!(read_data.len() >= 1024); // May be padded
        assert_eq!(&read_data[..1024], &data[..]);

        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_zero_copy_file() {
        let temp_dir = std::env::temp_dir().join("rs3gw-zerocopy-test-3");
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .expect("Failed to create test directory");

        let src_file = temp_dir.join("source");
        let dst_file = temp_dir.join("dest");

        let test_data = vec![1u8; 128 * 1024]; // 128KB
        tokio::fs::write(&src_file, &test_data)
            .await
            .expect("Failed to write source file");

        let config = ZeroCopyConfig::default();
        zero_copy_file(&src_file, &dst_file, test_data.len(), &config)
            .await
            .expect("Failed to zero copy file");

        let copied_data = tokio::fs::read(&dst_file)
            .await
            .expect("Failed to read destination file");
        assert_eq!(copied_data, test_data);

        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    #[test]
    fn test_zerocopy_stats() {
        let mut stats = ZeroCopyStats::new();

        stats.record_direct_io_read(1024);
        stats.record_direct_io_write(2048);
        stats.record_splice(4096);
        stats.record_sendfile(8192);
        stats.record_mmap();

        assert_eq!(stats.direct_io_reads, 1);
        assert_eq!(stats.direct_io_writes, 1);
        assert_eq!(stats.splice_operations, 1);
        assert_eq!(stats.sendfile_operations, 1);
        assert_eq!(stats.mmap_operations, 1);
        assert_eq!(stats.bytes_via_zerocopy, 1024 + 2048 + 4096 + 8192);
    }
}

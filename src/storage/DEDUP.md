# Data Deduplication Module

## Overview

The deduplication module provides block-level content-addressed storage to reduce storage space by eliminating redundant data blocks. This feature is transparent to the S3 API and requires no client-side changes.

## Features

- **SHA256-based content addressing**: Every block is identified by its SHA256 hash
- **Reference counting**: Shared blocks are stored once and referenced multiple times
- **Configurable block size**: 4KB to 1MB (default: 64KB)
- **Two chunking algorithms**:
  - Fixed-size: Simple, fast, deterministic
  - Content-defined: Better dedup ratio using rolling hash boundaries
- **Automatic garbage collection**: Unreferenced blocks are cleaned up automatically
- **Smart thresholds**: Small objects bypass dedup to avoid overhead
- **Comprehensive statistics**: Track dedup ratio, space saved, block counts

## Architecture

### Storage Layout

```
storage_root/
├── blocks/              # Content-addressed block storage
│   ├── ab/
│   │   └── cd/
│   │       └── abcdef123...  # Block data files (first 4 chars = directory structure)
├── block_refs/          # Reference counting metadata
│   ├── ab/
│   │   └── cd/
│   │       └── abcdef123....refs.json  # Reference count for each block
└── block_index/         # Object-to-blocks mapping
    └── {bucket}/
        └── {key}.blocks.json  # List of blocks composing this object
```

### Data Flow

#### Write Path
1. Object data arrives via S3 API
2. Data is chunked using configured algorithm
3. Each chunk is hashed (SHA256)
4. If block exists: increment reference count
5. If block is new: store block data and create reference
6. Save object block map

#### Read Path
1. Load object block map
2. Retrieve each block by hash
3. Reassemble blocks in order
4. Return to S3 API

#### Delete Path
1. Load object block map
2. Decrement reference count for each block
3. If reference count reaches 0: delete block data
4. Delete object block map

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_DEDUP_ENABLED` | `true` | Enable/disable deduplication |
| `RS3GW_DEDUP_BLOCK_SIZE` | `65536` | Block size in bytes (4096-1048576) |
| `RS3GW_DEDUP_ALGORITHM` | `fixed-size` | Chunking algorithm: `fixed-size` or `content-defined` |
| `RS3GW_DEDUP_MIN_SIZE` | `131072` | Minimum object size for dedup (bytes) |

### Programmatic Configuration

```rust
use rs3gw::storage::{DedupConfig, ChunkingAlgorithm};

// Default configuration (enabled, 64KB blocks, fixed-size)
let config = DedupConfig::default();

// Custom configuration
let config = DedupConfig::new(128 * 1024)?  // 128KB blocks
    .with_algorithm(ChunkingAlgorithm::ContentDefined)
    .with_min_size(256 * 1024);  // Only dedup objects >= 256KB

// Disabled
let config = DedupConfig::disabled();
```

## Chunking Algorithms

### Fixed-Size Chunking

- **How it works**: Splits data into equal-sized blocks
- **Pros**:
  - Fast and simple
  - Deterministic
  - Low CPU overhead
- **Cons**:
  - Lower dedup ratio
  - Boundary-shift problem (inserting data shifts all subsequent blocks)
- **Best for**:
  - Append-only workloads
  - Known fixed-size records
  - CPU-constrained environments

### Content-Defined Chunking (CDC)

- **How it works**: Uses Rabin-Karp rolling hash to find natural boundaries
- **Pros**:
  - Higher dedup ratio (30-70% typical)
  - Resistant to boundary-shift problem
  - Better for modified files
- **Cons**:
  - Higher CPU overhead
  - Variable block sizes
  - Slightly more complex
- **Best for**:
  - General-purpose storage
  - Frequently modified files
  - Maximum space savings

## Performance Characteristics

### CPU Overhead

- **Fixed-size**: ~2-5% CPU overhead
- **Content-defined**: ~8-15% CPU overhead
- Hashing (SHA256) dominates CPU time

### Memory Overhead

- **Reference cache**: ~100 bytes per unique block (in-memory)
- **Block map**: ~64 bytes per block per object (on-disk)
- Typical: 1M unique blocks = ~100MB RAM

### Space Savings

- **Duplicate files**: 99% savings
- **Similar files**: 30-70% savings
- **Random data**: 0% savings
- **Versioned data**: 40-80% savings

### Throughput Impact

- **Write**: -10% to -20% (due to hashing and block lookup)
- **Read**: -5% to -10% (due to block reassembly)
- **Delete**: Minimal impact

## Usage Examples

### Basic Usage

```rust
use rs3gw::storage::{DedupManager, DedupConfig};
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = DedupConfig::default();
    let manager = DedupManager::new("./storage".into(), config).await?;

    // Store object
    let data = Bytes::from(vec![1, 2, 3, 4]);
    let block_map = manager.store_object("my-bucket", "my-key", &data).await?;
    println!("Stored {} blocks", block_map.blocks.len());

    // Retrieve object
    let retrieved = manager.get_object("my-bucket", "my-key").await?;
    assert_eq!(data, retrieved);

    // Delete object
    manager.delete_object("my-bucket", "my-key").await?;

    Ok(())
}
```

### Getting Statistics

```rust
// Get deduplication statistics
let stats = manager.get_stats().await?;
println!("Unique blocks: {}", stats.total_blocks);
println!("Total references: {}", stats.total_refs);
println!("Physical bytes: {}", stats.total_physical_bytes);
println!("Logical bytes: {}", stats.total_logical_bytes);
println!("Dedup ratio: {:.1}%", stats.dedup_ratio * 100.0);
println!("Space saved: {} bytes", stats.space_saved);
```

### Garbage Collection

```rust
// Run garbage collection to remove unreferenced blocks
let result = manager.garbage_collect().await?;
println!("Checked {} blocks", result.blocks_checked);
println!("Removed {} blocks", result.blocks_removed);
println!("Freed {} bytes", result.bytes_freed);
```

## Tuning Guidelines

### Block Size Selection

| Block Size | Use Case | Dedup Ratio | CPU Overhead |
|------------|----------|-------------|--------------|
| 4KB | Small files, logs | Low | Low |
| 16KB | General purpose | Medium | Low |
| 64KB | **Default** | Good | Medium |
| 256KB | Large files, media | High | Medium |
| 1MB | Very large files | Very High | High |

**Rule of thumb**: Block size should be ~1% of average object size.

### Algorithm Selection

- Use **fixed-size** if:
  - CPU is limited
  - Objects are rarely modified
  - Simplicity is important

- Use **content-defined** if:
  - Maximizing space savings
  - Objects are frequently modified
  - CPU is available

### Minimum Object Size

- Default: 128KB (avoid overhead on small objects)
- Increase if: Most objects are large (GB+)
- Decrease if: Most objects are medium-sized (MB range)

## Limitations

1. **Not suitable for encrypted data**: Random-looking data won't deduplicate
2. **CPU overhead**: Hashing every block requires CPU
3. **Metadata overhead**: Block maps and references consume space
4. **No cross-bucket dedup**: Deduplication is per-storage-root
5. **Compression interaction**: Apply compression before or after dedup, not both

## Best Practices

1. **Monitor dedup ratio**: Track `space_saved / total_logical_bytes`
2. **Run GC periodically**: Schedule garbage collection during low-traffic periods
3. **Size blocks appropriately**: Match block size to workload
4. **Disable for random data**: Don't waste CPU on undeduplicatable data
5. **Test before production**: Measure impact on your specific workload

## Implementation Details

### Rolling Hash (Rabin-Karp)

The content-defined chunking uses a Rabin-Karp rolling hash with:
- Window size: 48 bytes
- Base: 256
- Modulus: 2^32 - 1
- Boundary mask: Configurable based on target block size

### Reference Counting

- Atomic operations ensure consistency
- In-memory cache for fast lookups
- On-disk persistence for durability
- Automatic cleanup when count reaches 0

### Concurrency

- Read operations are fully concurrent
- Write operations use per-block locking
- Block storage is crash-safe (fsync after write)

## Testing

Run deduplication tests:

```bash
cargo test storage::dedup::tests
```

Tests cover:
- Fixed-size chunking (2 tests)
- Content-defined chunking (2 tests)
- Rolling hash boundaries (1 test)
- Store/retrieve operations (1 test)
- Reference counting (1 test)
- Garbage collection (1 test)
- Small object handling (1 test)

Total: 9 comprehensive tests

## Future Enhancements

Potential improvements for future versions:

- **Cross-bucket dedup**: Global deduplication across all buckets
- **Compression integration**: Compress blocks before storage
- **Encryption support**: Encrypt blocks while maintaining dedup
- **Delta encoding**: Store differences between similar blocks
- **Bloom filters**: Fast negative lookups for non-existent blocks
- **Distributed dedup**: Deduplication across cluster nodes
- **Smart prefetch**: Predictive block loading for sequential access

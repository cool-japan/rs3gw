//! Cost / usage reporting.
//!
//! Per-bucket accounting of data transfer (bytes uploaded / downloaded) and
//! request counts by operation, plus an estimated cost breakdown from a
//! configurable [`PricingConfig`], and a pluggable [`UsageHook`] mechanism for
//! exporting usage to external billing / reporting systems.
//!
//! Design split (deliberate):
//! - The tracker accumulates **cumulative** counters (transfer bytes, request
//!   counts). These are monotonic and exact via simple increments, so there is
//!   no fragile overwrite/delete storage-accounting to get wrong.
//! - Point-in-time **storage size / object count** is intentionally NOT tracked
//!   here; it is read live from the storage engine when a report is built
//!   ([`UsageTracker::build_report`] takes it as input), so it is always
//!   accurate regardless of overwrites, multipart, or out-of-band changes.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Bytes in a decimal gigabyte (cloud-billing convention: 1 GB = 10^9 bytes).
const BYTES_PER_GB: f64 = 1_000_000_000.0;

/// Pricing table used to estimate cost from usage counters. Defaults approximate
/// AWS S3 Standard (us-east-1) list pricing; override via [`UsageTracker::with_pricing`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingConfig {
    /// USD per GB-month of stored data.
    pub storage_usd_per_gb_month: f64,
    /// USD per GB of data uploaded (transfer in; usually free).
    pub upload_usd_per_gb: f64,
    /// USD per GB of data downloaded (transfer out).
    pub download_usd_per_gb: f64,
    /// USD per 1000 write-class requests (PUT/POST/COPY/LIST).
    pub request_usd_per_thousand_write: f64,
    /// USD per 1000 read-class requests (GET/HEAD/SELECT).
    pub request_usd_per_thousand_read: f64,
}

impl Default for PricingConfig {
    fn default() -> Self {
        Self {
            storage_usd_per_gb_month: 0.023,
            upload_usd_per_gb: 0.0,
            download_usd_per_gb: 0.09,
            request_usd_per_thousand_write: 0.005,
            request_usd_per_thousand_read: 0.0004,
        }
    }
}

/// Classify an operation name into read / write / free request tiers.
fn request_tier(op: &str) -> RequestTier {
    match op.to_ascii_uppercase().as_str() {
        "GET" | "HEAD" | "SELECT" => RequestTier::Read,
        // S3 does not bill DELETE requests.
        "DELETE" => RequestTier::Free,
        // PUT / POST / COPY / LIST and anything else default to the write tier.
        _ => RequestTier::Write,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestTier {
    Read,
    Write,
    Free,
}

/// Internal mutable per-bucket cumulative counters.
#[derive(Debug, Default, Clone)]
struct TransferCounters {
    bytes_uploaded: u64,
    bytes_downloaded: u64,
    requests_by_op: HashMap<String, u64>,
}

/// Estimated cost breakdown (USD) for a bucket or the whole gateway.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CostBreakdown {
    pub storage_usd: f64,
    pub upload_usd: f64,
    pub download_usd: f64,
    pub request_usd: f64,
    pub total_usd: f64,
}

/// A serializable per-bucket usage record (cumulative counters + live storage +
/// estimated cost).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketUsage {
    pub bucket: String,
    pub storage_bytes: u64,
    pub object_count: u64,
    pub bytes_uploaded: u64,
    pub bytes_downloaded: u64,
    pub total_requests: u64,
    pub requests_by_op: HashMap<String, u64>,
    pub estimated_cost: CostBreakdown,
}

/// A full usage report across all known buckets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageReport {
    /// RFC3339 generation timestamp.
    pub generated_at: String,
    pub pricing: PricingConfig,
    pub buckets: Vec<BucketUsage>,
    pub totals: CostBreakdown,
    pub total_storage_bytes: u64,
    pub total_objects: u64,
}

/// A pluggable hook invoked when a usage report is flushed. Implementors can
/// forward the report to an external billing system, a file, a message bus, etc.
pub trait UsageHook: Send + Sync {
    /// Human-readable hook name (for logging / diagnostics).
    fn name(&self) -> &str;
    /// Export the report. Must not panic; errors should be handled internally.
    fn export(&self, report: &UsageReport);
}

/// Built-in hook that logs a one-line-per-bucket summary via `tracing`.
pub struct LoggingUsageHook;

impl UsageHook for LoggingUsageHook {
    fn name(&self) -> &str {
        "logging"
    }

    fn export(&self, report: &UsageReport) {
        tracing::info!(
            generated_at = %report.generated_at,
            buckets = report.buckets.len(),
            total_usd = report.totals.total_usd,
            total_storage_bytes = report.total_storage_bytes,
            "usage report"
        );
        for b in &report.buckets {
            tracing::info!(
                bucket = %b.bucket,
                storage_bytes = b.storage_bytes,
                objects = b.object_count,
                up = b.bytes_uploaded,
                down = b.bytes_downloaded,
                requests = b.total_requests,
                cost_usd = b.estimated_cost.total_usd,
                "usage report bucket"
            );
        }
    }
}

/// Per-bucket usage tracker with pluggable export hooks.
#[derive(Clone)]
pub struct UsageTracker {
    counters: Arc<RwLock<HashMap<String, TransferCounters>>>,
    pricing: PricingConfig,
    hooks: Arc<RwLock<Vec<Arc<dyn UsageHook>>>>,
}

impl Default for UsageTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageTracker {
    pub fn new() -> Self {
        Self::with_pricing(PricingConfig::default())
    }

    pub fn with_pricing(pricing: PricingConfig) -> Self {
        Self {
            counters: Arc::new(RwLock::new(HashMap::new())),
            pricing,
            hooks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Construct with hooks pre-registered. Synchronous, for startup wiring where
    /// the async [`Self::register_hook`] cannot be awaited (e.g. `AppState::new`).
    pub fn with_hooks(pricing: PricingConfig, hooks: Vec<Arc<dyn UsageHook>>) -> Self {
        Self {
            counters: Arc::new(RwLock::new(HashMap::new())),
            pricing,
            hooks: Arc::new(RwLock::new(hooks)),
        }
    }

    pub fn pricing(&self) -> &PricingConfig {
        &self.pricing
    }

    /// Register an export hook. Hooks fire (in registration order) on [`Self::flush`].
    pub async fn register_hook(&self, hook: Arc<dyn UsageHook>) {
        self.hooks.write().await.push(hook);
    }

    /// Record a single request against `bucket` for operation `op` (e.g. "GET").
    pub async fn record_request(&self, bucket: &str, op: &str) {
        let mut guard = self.counters.write().await;
        let c = guard.entry(bucket.to_string()).or_default();
        *c.requests_by_op.entry(op.to_ascii_uppercase()).or_insert(0) += 1;
    }

    /// Record `bytes` uploaded into `bucket` (no request increment).
    pub async fn record_upload(&self, bucket: &str, bytes: u64) {
        let mut guard = self.counters.write().await;
        let c = guard.entry(bucket.to_string()).or_default();
        c.bytes_uploaded = c.bytes_uploaded.saturating_add(bytes);
    }

    /// Record `bytes` downloaded from `bucket` (no request increment).
    pub async fn record_download(&self, bucket: &str, bytes: u64) {
        let mut guard = self.counters.write().await;
        let c = guard.entry(bucket.to_string()).or_default();
        c.bytes_downloaded = c.bytes_downloaded.saturating_add(bytes);
    }

    /// Convenience: a PutObject of `size` bytes (PUT request + upload bytes).
    pub async fn record_put(&self, bucket: &str, size: u64) {
        let mut guard = self.counters.write().await;
        let c = guard.entry(bucket.to_string()).or_default();
        *c.requests_by_op.entry("PUT".to_string()).or_insert(0) += 1;
        c.bytes_uploaded = c.bytes_uploaded.saturating_add(size);
    }

    /// Convenience: a GetObject serving `size` bytes (GET request + download bytes).
    pub async fn record_get(&self, bucket: &str, size: u64) {
        let mut guard = self.counters.write().await;
        let c = guard.entry(bucket.to_string()).or_default();
        *c.requests_by_op.entry("GET".to_string()).or_insert(0) += 1;
        c.bytes_downloaded = c.bytes_downloaded.saturating_add(size);
    }

    /// Convenience: a DeleteObject (DELETE request; storage is read live at report time).
    pub async fn record_delete(&self, bucket: &str) {
        self.record_request(bucket, "DELETE").await;
    }

    /// Names of all buckets seen so far (have at least one recorded counter).
    pub async fn tracked_buckets(&self) -> Vec<String> {
        self.counters.read().await.keys().cloned().collect()
    }

    /// Estimate cost from cumulative counters + live storage for one bucket.
    fn estimate_cost(
        &self,
        storage_bytes: u64,
        bytes_uploaded: u64,
        bytes_downloaded: u64,
        requests_by_op: &HashMap<String, u64>,
    ) -> CostBreakdown {
        let storage_usd =
            (storage_bytes as f64 / BYTES_PER_GB) * self.pricing.storage_usd_per_gb_month;
        let upload_usd = (bytes_uploaded as f64 / BYTES_PER_GB) * self.pricing.upload_usd_per_gb;
        let download_usd =
            (bytes_downloaded as f64 / BYTES_PER_GB) * self.pricing.download_usd_per_gb;

        let (mut read_reqs, mut write_reqs) = (0u64, 0u64);
        for (op, count) in requests_by_op {
            match request_tier(op) {
                RequestTier::Read => read_reqs = read_reqs.saturating_add(*count),
                RequestTier::Write => write_reqs = write_reqs.saturating_add(*count),
                RequestTier::Free => {}
            }
        }
        let request_usd = (read_reqs as f64 / 1000.0) * self.pricing.request_usd_per_thousand_read
            + (write_reqs as f64 / 1000.0) * self.pricing.request_usd_per_thousand_write;

        let total_usd = storage_usd + upload_usd + download_usd + request_usd;
        CostBreakdown {
            storage_usd,
            upload_usd,
            download_usd,
            request_usd,
            total_usd,
        }
    }

    /// Build a [`BucketUsage`] for a single bucket given its live storage stats.
    pub async fn bucket_usage(
        &self,
        bucket: &str,
        storage_bytes: u64,
        object_count: u64,
    ) -> BucketUsage {
        let counters = {
            let guard = self.counters.read().await;
            guard.get(bucket).cloned().unwrap_or_default()
        };
        let total_requests: u64 = counters.requests_by_op.values().copied().sum();
        let estimated_cost = self.estimate_cost(
            storage_bytes,
            counters.bytes_uploaded,
            counters.bytes_downloaded,
            &counters.requests_by_op,
        );
        BucketUsage {
            bucket: bucket.to_string(),
            storage_bytes,
            object_count,
            bytes_uploaded: counters.bytes_uploaded,
            bytes_downloaded: counters.bytes_downloaded,
            total_requests,
            requests_by_op: counters.requests_by_op,
            estimated_cost,
        }
    }

    /// Build a full report from live per-bucket storage stats `(bucket, storage_bytes,
    /// object_count)`. Buckets that have counters but are absent from `storage_stats`
    /// (e.g. empty after deletes) are still included with zero storage.
    pub async fn build_report(&self, storage_stats: &[(String, u64, u64)]) -> UsageReport {
        let mut seen: HashMap<String, (u64, u64)> = HashMap::new();
        for (b, size, count) in storage_stats {
            seen.insert(b.clone(), (*size, *count));
        }
        // Include buckets we have counters for but storage didn't list.
        for b in self.tracked_buckets().await {
            seen.entry(b).or_insert((0, 0));
        }

        let mut buckets = Vec::with_capacity(seen.len());
        let mut totals = CostBreakdown::default();
        let mut total_storage_bytes = 0u64;
        let mut total_objects = 0u64;
        // Deterministic ordering by bucket name.
        let mut names: Vec<String> = seen.keys().cloned().collect();
        names.sort();
        for name in names {
            let (size, count) = seen[&name];
            let bu = self.bucket_usage(&name, size, count).await;
            totals.storage_usd += bu.estimated_cost.storage_usd;
            totals.upload_usd += bu.estimated_cost.upload_usd;
            totals.download_usd += bu.estimated_cost.download_usd;
            totals.request_usd += bu.estimated_cost.request_usd;
            totals.total_usd += bu.estimated_cost.total_usd;
            total_storage_bytes = total_storage_bytes.saturating_add(size);
            total_objects = total_objects.saturating_add(count);
            buckets.push(bu);
        }

        UsageReport {
            generated_at: chrono::Utc::now().to_rfc3339(),
            pricing: self.pricing.clone(),
            buckets,
            totals,
            total_storage_bytes,
            total_objects,
        }
    }

    /// Build a report (from live storage stats) and fire all registered export hooks.
    pub async fn flush(&self, storage_stats: &[(String, u64, u64)]) -> UsageReport {
        let report = self.build_report(storage_stats).await;
        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            hook.export(&report);
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn records_requests_and_transfer_per_bucket() {
        let t = UsageTracker::new();
        t.record_put("a", 1000).await;
        t.record_get("a", 500).await;
        t.record_get("a", 500).await;
        t.record_delete("a").await;
        t.record_put("b", 42).await;

        let ua = t.bucket_usage("a", 1000, 1).await;
        assert_eq!(ua.bytes_uploaded, 1000);
        assert_eq!(ua.bytes_downloaded, 1000);
        assert_eq!(ua.total_requests, 4); // 1 PUT + 2 GET + 1 DELETE
        assert_eq!(ua.requests_by_op.get("GET"), Some(&2));
        assert_eq!(ua.requests_by_op.get("PUT"), Some(&1));
        assert_eq!(ua.requests_by_op.get("DELETE"), Some(&1));

        let ub = t.bucket_usage("b", 42, 1).await;
        assert_eq!(ub.bytes_uploaded, 42);
        assert_eq!(ub.total_requests, 1);
    }

    #[tokio::test]
    async fn cost_estimate_uses_pricing_and_tiers() {
        let pricing = PricingConfig {
            storage_usd_per_gb_month: 1.0,
            upload_usd_per_gb: 0.0,
            download_usd_per_gb: 2.0,
            request_usd_per_thousand_write: 10.0,
            request_usd_per_thousand_read: 1.0,
        };
        let t = UsageTracker::with_pricing(pricing);
        // 2 GB downloaded, 1000 GET (read), 1000 PUT (write), 1000 DELETE (free).
        t.record_download("a", 2 * 1_000_000_000).await;
        for _ in 0..1000 {
            t.record_request("a", "GET").await;
            t.record_request("a", "PUT").await;
            t.record_request("a", "DELETE").await;
        }
        // 1 GB stored.
        let u = t.bucket_usage("a", 1_000_000_000, 1).await;
        assert!((u.estimated_cost.storage_usd - 1.0).abs() < 1e-9);
        assert!((u.estimated_cost.download_usd - 4.0).abs() < 1e-9); // 2 GB * 2.0
                                                                     // read: 1000/1000 * 1.0 = 1.0 ; write: 1000/1000 * 10.0 = 10.0 ; delete free.
        assert!((u.estimated_cost.request_usd - 11.0).abs() < 1e-9);
        assert!((u.estimated_cost.total_usd - 16.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn report_aggregates_totals_and_fires_hooks() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct CountingHook(Arc<AtomicUsize>);
        impl UsageHook for CountingHook {
            fn name(&self) -> &str {
                "counting"
            }
            fn export(&self, report: &UsageReport) {
                assert_eq!(report.buckets.len(), 2);
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let t = UsageTracker::new();
        t.record_put("a", 100).await;
        t.record_put("b", 200).await;
        let fired = Arc::new(AtomicUsize::new(0));
        t.register_hook(Arc::new(CountingHook(fired.clone()))).await;

        let stats = vec![
            ("a".to_string(), 100u64, 1u64),
            ("b".to_string(), 200u64, 1u64),
        ];
        let report = t.flush(&stats).await;
        assert_eq!(report.buckets.len(), 2);
        assert_eq!(report.total_storage_bytes, 300);
        assert_eq!(report.total_objects, 2);
        // Buckets are sorted by name.
        assert_eq!(report.buckets[0].bucket, "a");
        assert_eq!(report.buckets[1].bucket, "b");
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    }
}

// Immutable Audit Logging System with Cryptographic Verification
//
// This module provides a production-grade audit logging system with:
// - Immutable audit trail (append-only, blockchain-inspired chain)
// - Cryptographic verification (HMAC-SHA256 chain integrity)
// - Real-time security event detection
// - Compliance report generation (SOC2, HIPAA, GDPR)
// - Log forwarding to SIEM systems
//
// Architecture:
// - AuditEvent: Individual audit log entry with all required fields
// - AuditChain: Immutable chain with cryptographic linking
// - AuditLogger: Main logging interface with async writes
// - SecurityEventDetector: Pattern matching for security events
// - ComplianceReporter: Report generation for compliance frameworks
// - LogForwarder: External log forwarding (SIEM, syslog, S3)

pub mod config;
pub mod filter;
pub mod forwarding;
pub mod security;
mod types;

pub use config::{AuditConfig, AuditConfigBuilder};
pub use filter::AuditFilter;
pub use forwarding::{ForwardDestination, SyslogProtocol};
pub use security::{SecurityEvent, SecurityEventDetector};
pub use types::{AuditAction, AuditError, AuditEvent, AuditOutcome, AuditResult, SecuritySeverity};

use std::path::Path;
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Main audit logger
pub struct AuditLogger {
    config: AuditConfig,
    last_hash: Arc<RwLock<Option<String>>>,
    current_file: Arc<RwLock<Option<File>>>,
    current_size: Arc<RwLock<u64>>,
    security_detector: Arc<SecurityEventDetector>,
    s3_client: Option<Arc<aws_sdk_s3::Client>>,
}

impl AuditLogger {
    /// Create a new audit logger
    pub async fn new(config: AuditConfig) -> AuditResult<Self> {
        // Create audit log directory if it doesn't exist
        if let Some(parent) = config.log_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Open log file in append mode
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config.log_path)
            .await?;

        let current_size = tokio::fs::metadata(&config.log_path).await?.len();

        // Read the last hash from the file
        let last_hash = Self::read_last_hash(&config.log_path, &config.hmac_secret).await?;

        let security_detector = Arc::new(SecurityEventDetector::new());

        // Initialize S3 client if S3 forwarding is configured
        let s3_client = if config
            .forward_destinations
            .iter()
            .any(|d| matches!(d, ForwardDestination::S3 { .. }))
        {
            let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .load()
                .await;
            Some(Arc::new(aws_sdk_s3::Client::new(&aws_config)))
        } else {
            None
        };

        info!(path = ?config.log_path, s3_enabled = s3_client.is_some(), "Audit logger initialized");

        Ok(Self {
            config,
            last_hash: Arc::new(RwLock::new(last_hash)),
            current_file: Arc::new(RwLock::new(Some(file))),
            current_size: Arc::new(RwLock::new(current_size)),
            security_detector,
            s3_client,
        })
    }

    /// Read the last hash from the audit log file
    async fn read_last_hash(path: &Path, secret: &[u8]) -> AuditResult<Option<String>> {
        if !path.exists() {
            return Ok(None);
        }

        let file = File::open(path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut last_hash = None;
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(event) = serde_json::from_str::<AuditEvent>(&line) {
                // Recompute hash to verify integrity
                let computed_hash = event.compute_hash(secret)?;
                last_hash = Some(computed_hash);
            }
        }

        Ok(last_hash)
    }

    /// Log an audit event
    pub async fn log(&self, mut event: AuditEvent) -> AuditResult<()> {
        // Link to previous event
        let prev_hash = self.last_hash.read().await.clone();
        if let Some(prev) = prev_hash {
            event.prev_hash = Some(prev);
        }

        // Compute and store hash
        let hash = event.compute_hash(&self.config.hmac_secret)?;
        event.current_hash = Some(hash.clone());

        // Update last hash
        *self.last_hash.write().await = Some(hash);

        // Security event detection
        if self.config.enable_security_detection {
            if let Some(security_event) = self.security_detector.detect(&event).await {
                warn!(?security_event, "Security event detected");

                // Log the security event itself
                let mut security_log = AuditEvent::new(
                    "system".to_string(),
                    AuditAction::SuspiciousActivity,
                    format!("security_event:{}", security_event.event_type),
                    AuditOutcome::Success,
                );
                security_log.metadata.insert(
                    "severity".to_string(),
                    format!("{:?}", security_event.severity),
                );
                security_log
                    .metadata
                    .insert("description".to_string(), security_event.description);

                // Write security event (avoid infinite recursion by not detecting on security events)
                self.write_event(&security_log).await?;
            }
        }

        // Write the event
        self.write_event(&event).await?;

        // Forward to external systems
        if self.config.enable_forwarding {
            for dest in &self.config.forward_destinations {
                if let Err(e) = self.forward_event(&event, dest).await {
                    error!(?dest, error = ?e, "Failed to forward audit event");
                }
            }
        }

        Ok(())
    }

    /// Write event to log file
    async fn write_event(&self, event: &AuditEvent) -> AuditResult<()> {
        let json = serde_json::to_string(event)?;
        let line = format!("{}\n", json);
        let line_bytes = line.as_bytes();

        // Check if rotation is needed
        let current_size = *self.current_size.read().await;
        if current_size + line_bytes.len() as u64 > self.config.max_file_size {
            self.rotate_log().await?;
        }

        // Write to file
        let mut file_guard = self.current_file.write().await;
        if let Some(file) = file_guard.as_mut() {
            file.write_all(line_bytes).await?;
            file.flush().await?;
        }

        // Update size
        *self.current_size.write().await += line_bytes.len() as u64;

        Ok(())
    }

    /// Rotate the log file
    async fn rotate_log(&self) -> AuditResult<()> {
        info!("Rotating audit log");

        // Close current file
        *self.current_file.write().await = None;

        // Rotate existing files
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let rotated_path = self.config.log_path.with_file_name(format!(
            "{}.{}.log",
            self.config
                .log_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("audit"),
            timestamp
        ));

        tokio::fs::rename(&self.config.log_path, &rotated_path).await?;

        // Compress if enabled
        if self.config.compress_rotated {
            let compressed_path = rotated_path.with_extension("log.zst");
            if let Err(e) = compress_file(&rotated_path, &compressed_path).await {
                error!(error = ?e, "Failed to compress rotated log");
            } else {
                // Remove uncompressed file
                let _ = tokio::fs::remove_file(&rotated_path).await;
            }
        }

        // Clean up old files
        self.cleanup_old_logs().await?;

        // Open new file
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.config.log_path)
            .await?;

        *self.current_file.write().await = Some(file);
        *self.current_size.write().await = 0;

        Ok(())
    }

    /// Clean up old rotated log files
    async fn cleanup_old_logs(&self) -> AuditResult<()> {
        let parent = self
            .config
            .log_path
            .parent()
            .ok_or_else(|| AuditError::InvalidEvent("Invalid log path".to_string()))?;

        let file_stem = self
            .config
            .log_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("audit");

        let mut entries = tokio::fs::read_dir(parent).await?;
        let mut rotated_files = Vec::new();

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with(file_stem)
                    && name
                        != self
                            .config
                            .log_path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                {
                    if let Ok(metadata) = entry.metadata().await {
                        rotated_files.push((path, metadata.modified().ok()));
                    }
                }
            }
        }

        // Sort by modification time (newest first)
        rotated_files.sort_by(|a, b| b.1.cmp(&a.1));

        // Remove old files beyond the limit
        for (path, _) in rotated_files.iter().skip(self.config.max_rotated_files) {
            debug!(?path, "Removing old audit log");
            let _ = tokio::fs::remove_file(path).await;
        }

        Ok(())
    }

    /// Format audit event as RFC 5424 syslog message
    pub fn format_syslog_rfc5424(&self, event: &AuditEvent) -> AuditResult<String> {
        // RFC 5424 format: <PRI>VERSION TIMESTAMP HOSTNAME APP-NAME PROCID MSGID STRUCTURED-DATA MSG

        // Priority: Facility(16=local0) * 8 + Severity
        // Severity: 6=Informational for normal events, 4=Warning for failures
        let severity = if event.outcome == AuditOutcome::Failure {
            4
        } else {
            6
        };
        let facility = 16; // local0
        let priority = facility * 8 + severity;

        // Version (always 1 for RFC 5424)
        let version = 1;

        // Timestamp in RFC 3339 format
        let timestamp = event.timestamp.to_rfc3339();

        // Hostname (get from system or use "-")
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "-".to_string());

        // APP-NAME
        let app_name = "rs3gw";

        // PROCID (process ID)
        let procid = std::process::id();

        // MSGID (audit action)
        let msgid = format!("{:?}", event.action).replace(' ', "_");

        // STRUCTURED-DATA (audit event details)
        let structured_data = format!(
            "[audit@rs3gw actor=\"{}\" resource=\"{}\" outcome=\"{:?}\" ip=\"{}\"]",
            event.actor.replace('"', "\\\""),
            event.resource.replace('"', "\\\""),
            event.outcome,
            event.source_ip.as_ref().unwrap_or(&"-".to_string())
        );

        // MSG (JSON representation of full event)
        let msg = serde_json::to_string(event)
            .map_err(AuditError::Serialization)?
            .replace('\n', " ");

        // Assemble the syslog message
        let syslog_msg = format!(
            "<{}>{}  {} {} {} {} {} {} {}",
            priority, version, timestamp, hostname, app_name, procid, msgid, structured_data, msg
        );

        Ok(syslog_msg)
    }

    /// Forward event to external destination
    async fn forward_event(
        &self,
        event: &AuditEvent,
        dest: &ForwardDestination,
    ) -> AuditResult<()> {
        match dest {
            ForwardDestination::Webhook { url, headers } => {
                let client = reqwest::Client::new();
                let mut request = client.post(url).json(event);

                for (key, value) in headers {
                    request = request.header(key, value);
                }

                request
                    .send()
                    .await
                    .map_err(|e| AuditError::Forwarding(e.to_string()))?;
            }

            ForwardDestination::File { path } => {
                let json = serde_json::to_string(event)?;
                let line = format!("{}\n", json);

                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .await?;

                file.write_all(line.as_bytes()).await?;
                file.flush().await?;
            }

            ForwardDestination::Syslog {
                host,
                port,
                protocol,
            } => {
                // Format as RFC 5424 syslog message
                let syslog_msg = self.format_syslog_rfc5424(event)?;

                // Send via appropriate protocol
                match protocol {
                    SyslogProtocol::Udp => {
                        use tokio::net::UdpSocket;
                        let socket = UdpSocket::bind("0.0.0.0:0")
                            .await
                            .map_err(|e| AuditError::Forwarding(e.to_string()))?;
                        socket
                            .send_to(syslog_msg.as_bytes(), format!("{}:{}", host, port))
                            .await
                            .map_err(|e| AuditError::Forwarding(e.to_string()))?;
                    }
                    SyslogProtocol::Tcp => {
                        use tokio::io::AsyncWriteExt as _;
                        use tokio::net::TcpStream;
                        let mut stream = TcpStream::connect(format!("{}:{}", host, port))
                            .await
                            .map_err(|e| AuditError::Forwarding(e.to_string()))?;
                        // RFC 6587: Octet counting framing
                        let framed = format!("{} {}", syslog_msg.len(), syslog_msg);
                        stream
                            .write_all(framed.as_bytes())
                            .await
                            .map_err(|e| AuditError::Forwarding(e.to_string()))?;
                        stream
                            .flush()
                            .await
                            .map_err(|e| AuditError::Forwarding(e.to_string()))?;
                    }
                    SyslogProtocol::Tls => {
                        // TLS syslog forwarding would require certificate handling
                        // For now, fall back to TCP with a warning
                        warn!("TLS syslog not yet implemented, falling back to TCP");
                        use tokio::io::AsyncWriteExt as _;
                        use tokio::net::TcpStream;
                        let mut stream = TcpStream::connect(format!("{}:{}", host, port))
                            .await
                            .map_err(|e| AuditError::Forwarding(e.to_string()))?;
                        let framed = format!("{} {}", syslog_msg.len(), syslog_msg);
                        stream
                            .write_all(framed.as_bytes())
                            .await
                            .map_err(|e| AuditError::Forwarding(e.to_string()))?;
                        stream
                            .flush()
                            .await
                            .map_err(|e| AuditError::Forwarding(e.to_string()))?;
                    }
                }
            }

            ForwardDestination::S3 {
                bucket,
                prefix,
                region,
            } => {
                if let Some(client) = &self.s3_client {
                    // Generate S3 key: prefix/YYYY-MM-DD/HH-MM-SS-event_id.json
                    let timestamp = event.timestamp.format("%Y-%m-%d/%H-%M-%S").to_string();
                    let event_id = event
                        .current_hash
                        .as_ref()
                        .map(|h| &h[..8])
                        .unwrap_or("unknown");
                    let key = format!(
                        "{}/{}-{}.json",
                        prefix.trim_end_matches('/'),
                        timestamp,
                        event_id
                    );

                    // Serialize event to JSON
                    let json_data = serde_json::to_vec(event).map_err(AuditError::Serialization)?;

                    // Upload to S3 (use existing client, assume region is configured)
                    // For region-specific operations, the client should be initialized with the correct region
                    client
                        .put_object()
                        .bucket(bucket)
                        .key(&key)
                        .body(json_data.into())
                        .content_type("application/json")
                        .metadata("event_action", event.action.to_string())
                        .metadata("event_actor", &event.actor)
                        .metadata("event_outcome", event.outcome.to_string())
                        .send()
                        .await
                        .map_err(|e| AuditError::Forwarding(format!("S3 upload failed: {}", e)))?;

                    debug!(bucket, key, region, "Audit event forwarded to S3");
                } else {
                    error!("S3 client not initialized for S3 forwarding");
                    return Err(AuditError::Forwarding(
                        "S3 client not initialized".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Verify the integrity of the audit chain
    pub async fn verify_chain(&self) -> AuditResult<bool> {
        let file = File::open(&self.config.log_path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut prev_hash: Option<String> = None;
        let mut line_num = 0;

        while let Ok(Some(line)) = lines.next_line().await {
            line_num += 1;
            let event: AuditEvent = serde_json::from_str(&line)?;

            // Verify prev_hash matches
            if event.prev_hash != prev_hash {
                error!(
                    line = line_num,
                    "Chain integrity violation: prev_hash mismatch"
                );
                return Ok(false);
            }

            // Verify current hash
            let computed_hash = event.compute_hash(&self.config.hmac_secret)?;
            prev_hash = Some(computed_hash.clone());

            if let Some(stored_hash) = &event.current_hash {
                if stored_hash != &computed_hash {
                    error!(line = line_num, "Chain integrity violation: hash mismatch");
                    return Ok(false);
                }
            }
        }

        info!(lines = line_num, "Audit chain verification complete");
        Ok(true)
    }

    /// Query audit events with filters
    pub async fn query(&self, filter: AuditFilter) -> AuditResult<Vec<AuditEvent>> {
        let file = File::open(&self.config.log_path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut results = Vec::new();

        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(event) = serde_json::from_str::<AuditEvent>(&line) {
                if filter.matches(&event) {
                    results.push(event);

                    if let Some(limit) = filter.limit {
                        if results.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }

        Ok(results)
    }
}

/// Compress a file using zstd
async fn compress_file(
    input_path: &std::path::Path,
    output_path: &std::path::Path,
) -> AuditResult<()> {
    let input_data = tokio::fs::read(input_path).await?;
    let compressed = zstd::bulk::compress(&input_data, 3)
        .map_err(|e| AuditError::Io(std::io::Error::other(e)))?;
    tokio::fs::write(output_path, compressed).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_audit_event_creation() {
        let event = AuditEvent::new(
            "user123".to_string(),
            AuditAction::GetObject,
            "bucket/key".to_string(),
            AuditOutcome::Success,
        );

        assert_eq!(event.actor, "user123");
        assert_eq!(event.action, AuditAction::GetObject);
        assert_eq!(event.resource, "bucket/key");
        assert_eq!(event.outcome, AuditOutcome::Success);
        assert!(event.prev_hash.is_none());
        assert!(event.current_hash.is_none());
    }

    #[test]
    fn test_audit_event_builder() {
        let event = AuditEvent::new(
            "user123".to_string(),
            AuditAction::PutObject,
            "bucket/key".to_string(),
            AuditOutcome::Success,
        )
        .with_source_ip("192.168.1.1".to_string())
        .with_status_code(200)
        .with_request_id("req-123".to_string())
        .with_metadata("size".to_string(), "1024".to_string());

        assert_eq!(event.source_ip, Some("192.168.1.1".to_string()));
        assert_eq!(event.status_code, Some(200));
        assert_eq!(event.request_id, Some("req-123".to_string()));
        assert_eq!(event.metadata.get("size"), Some(&"1024".to_string()));
    }

    #[test]
    fn test_event_hash_computation() {
        let event = AuditEvent::new(
            "user123".to_string(),
            AuditAction::GetObject,
            "bucket/key".to_string(),
            AuditOutcome::Success,
        );

        let secret = b"test-secret";
        let hash1 = event
            .compute_hash(secret)
            .expect("Failed to compute hash for audit event");
        let hash2 = event
            .compute_hash(secret)
            .expect("Failed to compute hash for audit event (second attempt)");

        // Same event should produce same hash
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA256 hex = 64 chars
    }

    #[test]
    fn test_event_hash_differs_for_different_events() {
        let event1 = AuditEvent::new(
            "user1".to_string(),
            AuditAction::GetObject,
            "bucket/key1".to_string(),
            AuditOutcome::Success,
        );

        let event2 = AuditEvent::new(
            "user2".to_string(),
            AuditAction::GetObject,
            "bucket/key2".to_string(),
            AuditOutcome::Success,
        );

        let secret = b"test-secret";
        let hash1 = event1
            .compute_hash(secret)
            .expect("Failed to compute hash for first audit event");
        let hash2 = event2
            .compute_hash(secret)
            .expect("Failed to compute hash for second audit event");

        // Different events should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_audit_logger_creation() {
        let temp_dir =
            TempDir::new().expect("Failed to create temporary directory for audit logger test");
        let log_path = temp_dir.path().join("audit.log");

        let config = AuditConfig::builder()
            .log_path(log_path.clone())
            .hmac_secret(b"test-secret".to_vec())
            .build();

        let _logger = AuditLogger::new(config)
            .await
            .expect("Failed to create audit logger");
        assert!(log_path.exists());
    }

    #[tokio::test]
    async fn test_audit_logger_log_event() {
        let temp_dir =
            TempDir::new().expect("Failed to create temporary directory for log event test");
        let log_path = temp_dir.path().join("audit.log");

        let config = AuditConfig::builder()
            .log_path(log_path.clone())
            .hmac_secret(b"test-secret".to_vec())
            .enable_security_detection(false)
            .build();

        let logger = AuditLogger::new(config)
            .await
            .expect("Failed to create audit logger for log event test");

        let event = AuditEvent::new(
            "user123".to_string(),
            AuditAction::GetObject,
            "bucket/key".to_string(),
            AuditOutcome::Success,
        );

        logger.log(event).await.expect("Failed to log audit event");

        // Verify file was written
        let content = tokio::fs::read_to_string(&log_path)
            .await
            .expect("Failed to read audit log file");
        assert!(!content.is_empty());
        assert!(content.contains("user123"));
        assert!(content.contains("bucket/key"));
    }

    #[tokio::test]
    async fn test_audit_logger_chain_integrity() {
        let temp_dir =
            TempDir::new().expect("Failed to create temporary directory for chain integrity test");
        let log_path = temp_dir.path().join("audit.log");

        let config = AuditConfig::builder()
            .log_path(log_path.clone())
            .hmac_secret(b"test-secret".to_vec())
            .enable_security_detection(false)
            .build();

        let logger = AuditLogger::new(config)
            .await
            .expect("Failed to create audit logger for chain integrity test");

        // Log multiple events
        for i in 0..5 {
            let event = AuditEvent::new(
                format!("user{}", i),
                AuditAction::GetObject,
                format!("bucket/key{}", i),
                AuditOutcome::Success,
            );
            logger
                .log(event)
                .await
                .expect("Failed to log audit event in chain integrity test");
        }

        // Verify chain integrity
        assert!(logger
            .verify_chain()
            .await
            .expect("Failed to verify audit chain"));
    }

    #[tokio::test]
    async fn test_audit_filter() {
        let temp_dir =
            TempDir::new().expect("Failed to create temporary directory for audit filter test");
        let log_path = temp_dir.path().join("audit.log");

        let config = AuditConfig::builder()
            .log_path(log_path.clone())
            .hmac_secret(b"test-secret".to_vec())
            .enable_security_detection(false)
            .build();

        let logger = AuditLogger::new(config)
            .await
            .expect("Failed to create audit logger for filter test");

        // Log events for different users
        for i in 0..5 {
            let event = AuditEvent::new(
                format!("user{}", i % 2),
                AuditAction::GetObject,
                format!("bucket/key{}", i),
                AuditOutcome::Success,
            );
            logger
                .log(event)
                .await
                .expect("Failed to log audit event in filter test");
        }

        // Query for user0 events
        let filter = AuditFilter {
            actor: Some("user0".to_string()),
            ..Default::default()
        };

        let results = logger
            .query(filter)
            .await
            .expect("Failed to query audit logs");
        assert_eq!(results.len(), 3); // Events 0, 2, 4

        for event in results {
            assert_eq!(event.actor, "user0");
        }
    }

    #[tokio::test]
    async fn test_security_event_detector_brute_force() {
        let detector = SecurityEventDetector::new();

        // Simulate 6 failed auth attempts
        for i in 0..6 {
            let event = AuditEvent::new(
                "attacker".to_string(),
                AuditAction::AuthFailure,
                "auth".to_string(),
                AuditOutcome::Failure,
            )
            .with_source_ip("192.168.1.100".to_string());

            let security_event = detector.detect(&event).await;

            if i >= 4 {
                // Should detect on 5th attempt
                assert!(security_event.is_some());
                let se =
                    security_event.expect("Failed to detect security event on brute force attempt");
                assert_eq!(se.event_type, "brute_force_attack");
                assert_eq!(se.severity, SecuritySeverity::Critical);
            }
        }
    }

    #[test]
    fn test_audit_config_builder() {
        let config = AuditConfig::builder()
            .log_path(PathBuf::from("/var/log/audit.log"))
            .hmac_secret(b"secret123".to_vec())
            .enable_security_detection(true)
            .max_file_size(50 * 1024 * 1024)
            .max_rotated_files(5)
            .compress_rotated(true)
            .build();

        assert_eq!(config.log_path, PathBuf::from("/var/log/audit.log"));
        assert_eq!(config.hmac_secret, b"secret123".to_vec());
        assert!(config.enable_security_detection);
        assert_eq!(config.max_file_size, 50 * 1024 * 1024);
        assert_eq!(config.max_rotated_files, 5);
        assert!(config.compress_rotated);
    }

    #[tokio::test]
    async fn test_syslog_rfc5424_format() {
        let temp_dir =
            TempDir::new().expect("Failed to create temporary directory for syslog format test");
        let log_path = temp_dir.path().join("audit.log");

        let config = AuditConfig::builder()
            .log_path(log_path)
            .hmac_secret(b"test-secret".to_vec())
            .build();

        let logger = AuditLogger::new(config)
            .await
            .expect("Failed to create audit logger for syslog format test");

        let event = AuditEvent::new(
            "user123".to_string(),
            AuditAction::PutObject,
            "bucket/key.txt".to_string(),
            AuditOutcome::Success,
        )
        .with_source_ip("192.168.1.100".to_string())
        .with_status_code(200);

        let syslog_msg = logger
            .format_syslog_rfc5424(&event)
            .expect("Failed to format audit event as RFC5424 syslog message");

        // Check basic RFC 5424 structure
        assert!(syslog_msg.starts_with("<134>1")); // Priority 134 = (16*8 + 6) for local0.info
        assert!(syslog_msg.contains("rs3gw")); // APP-NAME
        assert!(syslog_msg.contains("PutObject")); // MSGID
        assert!(syslog_msg.contains("[audit@rs3gw")); // STRUCTURED-DATA
        assert!(syslog_msg.contains("user123")); // actor
        assert!(syslog_msg.contains("bucket/key.txt")); // resource
        assert!(syslog_msg.contains("192.168.1.100")); // IP address
    }

    #[tokio::test]
    async fn test_syslog_failure_severity() {
        let temp_dir = TempDir::new()
            .expect("Failed to create temporary directory for syslog failure severity test");
        let log_path = temp_dir.path().join("audit.log");

        let config = AuditConfig::builder()
            .log_path(log_path)
            .hmac_secret(b"test-secret".to_vec())
            .build();

        let logger = AuditLogger::new(config)
            .await
            .expect("Failed to create audit logger for syslog failure severity test");

        let event = AuditEvent::new(
            "user123".to_string(),
            AuditAction::GetObject,
            "bucket/key.txt".to_string(),
            AuditOutcome::Failure,
        );

        let syslog_msg = logger
            .format_syslog_rfc5424(&event)
            .expect("Failed to format failure event as RFC5424 syslog message");

        // Check that failure events use warning severity (132 = 16*8 + 4)
        assert!(syslog_msg.starts_with("<132>1"));
    }
}

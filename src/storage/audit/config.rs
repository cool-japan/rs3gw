//! Audit logger configuration and builder.

use super::forwarding::ForwardDestination;
use std::path::PathBuf;

/// Configuration for audit logger
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// Path to audit log file
    pub log_path: PathBuf,

    /// HMAC secret for chain integrity
    pub hmac_secret: Vec<u8>,

    /// Enable real-time security event detection
    pub enable_security_detection: bool,

    /// Maximum log file size before rotation (bytes)
    pub max_file_size: u64,

    /// Maximum number of rotated files to keep
    pub max_rotated_files: usize,

    /// Enable log forwarding
    pub enable_forwarding: bool,

    /// Log forwarding destinations
    pub forward_destinations: Vec<ForwardDestination>,

    /// Enable compression for rotated logs
    pub compress_rotated: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            log_path: PathBuf::from("./audit/audit.log"),
            hmac_secret: b"change-me-in-production".to_vec(),
            enable_security_detection: true,
            max_file_size: 100 * 1024 * 1024, // 100 MB
            max_rotated_files: 10,
            enable_forwarding: false,
            forward_destinations: Vec::new(),
            compress_rotated: true,
        }
    }
}

impl AuditConfig {
    pub fn builder() -> AuditConfigBuilder {
        AuditConfigBuilder::default()
    }
}

/// Builder for AuditConfig
#[derive(Debug, Default)]
pub struct AuditConfigBuilder {
    log_path: Option<PathBuf>,
    hmac_secret: Option<Vec<u8>>,
    enable_security_detection: Option<bool>,
    max_file_size: Option<u64>,
    max_rotated_files: Option<usize>,
    enable_forwarding: Option<bool>,
    forward_destinations: Vec<ForwardDestination>,
    compress_rotated: Option<bool>,
}

impl AuditConfigBuilder {
    pub fn log_path(mut self, path: PathBuf) -> Self {
        self.log_path = Some(path);
        self
    }

    pub fn hmac_secret(mut self, secret: Vec<u8>) -> Self {
        self.hmac_secret = Some(secret);
        self
    }

    pub fn enable_security_detection(mut self, enable: bool) -> Self {
        self.enable_security_detection = Some(enable);
        self
    }

    pub fn max_file_size(mut self, size: u64) -> Self {
        self.max_file_size = Some(size);
        self
    }

    pub fn max_rotated_files(mut self, count: usize) -> Self {
        self.max_rotated_files = Some(count);
        self
    }

    pub fn enable_forwarding(mut self, enable: bool) -> Self {
        self.enable_forwarding = Some(enable);
        self
    }

    pub fn add_forward_destination(mut self, dest: ForwardDestination) -> Self {
        self.forward_destinations.push(dest);
        self
    }

    pub fn compress_rotated(mut self, compress: bool) -> Self {
        self.compress_rotated = Some(compress);
        self
    }

    pub fn build(self) -> AuditConfig {
        let default = AuditConfig::default();
        AuditConfig {
            log_path: self.log_path.unwrap_or(default.log_path),
            hmac_secret: self.hmac_secret.unwrap_or(default.hmac_secret),
            enable_security_detection: self
                .enable_security_detection
                .unwrap_or(default.enable_security_detection),
            max_file_size: self.max_file_size.unwrap_or(default.max_file_size),
            max_rotated_files: self.max_rotated_files.unwrap_or(default.max_rotated_files),
            enable_forwarding: self.enable_forwarding.unwrap_or(default.enable_forwarding),
            forward_destinations: if self.forward_destinations.is_empty() {
                default.forward_destinations
            } else {
                self.forward_destinations
            },
            compress_rotated: self.compress_rotated.unwrap_or(default.compress_rotated),
        }
    }
}

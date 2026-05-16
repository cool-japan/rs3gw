//! Core audit types: errors, actions, outcomes, severity, and audit events.

use chrono::{DateTime, Utc};
use hmac::KeyInit;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

/// Errors that can occur during audit logging
#[derive(Debug, Error)]
pub enum AuditError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Chain integrity violation: {0}")]
    ChainIntegrity(String),

    #[error("Invalid event: {0}")]
    InvalidEvent(String),

    #[error("HMAC error: {0}")]
    Hmac(String),

    #[error("Forwarding error: {0}")]
    Forwarding(String),
}

pub type AuditResult<T> = Result<T, AuditError>;

/// Action types in the audit log
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    // Bucket operations
    CreateBucket,
    DeleteBucket,
    ListBuckets,
    GetBucketPolicy,
    PutBucketPolicy,
    DeleteBucketPolicy,

    // Object operations
    GetObject,
    PutObject,
    DeleteObject,
    DeleteObjects,
    CopyObject,
    ListObjects,
    HeadObject,

    // Multipart operations
    CreateMultipartUpload,
    UploadPart,
    CompleteMultipartUpload,
    AbortMultipartUpload,

    // Authentication
    AuthSuccess,
    AuthFailure,

    // Admin operations
    UpdateConfig,
    ViewAuditLogs,
    ModifyPermissions,

    // Security events
    PrivilegeEscalation,
    UnauthorizedAccess,
    SuspiciousActivity,

    // Other
    Custom(String),
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Custom(s) => write!(f, "{}", s),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// Outcome of an operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuditOutcome {
    Success,
    Failure,
    Denied,
}

impl std::fmt::Display for AuditOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Failure => write!(f, "failure"),
            Self::Denied => write!(f, "denied"),
        }
    }
}

/// Security event severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum SecuritySeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// Individual audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique event ID (UUID)
    pub event_id: String,

    /// Timestamp (ISO 8601)
    pub timestamp: DateTime<Utc>,

    /// Actor (user ID, API key, or "anonymous")
    pub actor: String,

    /// Source IP address
    pub source_ip: Option<String>,

    /// Action performed
    pub action: AuditAction,

    /// Resource accessed (bucket/key path)
    pub resource: String,

    /// Outcome of the operation
    pub outcome: AuditOutcome,

    /// HTTP status code (if applicable)
    pub status_code: Option<u16>,

    /// Error message (if failure)
    pub error_message: Option<String>,

    /// Request ID for correlation
    pub request_id: Option<String>,

    /// Session ID for correlation
    pub session_id: Option<String>,

    /// User agent string
    pub user_agent: Option<String>,

    /// Additional metadata (key-value pairs)
    #[serde(default)]
    pub metadata: HashMap<String, String>,

    /// Previous event hash (for chain integrity)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_hash: Option<String>,

    /// Current event hash (HMAC-SHA256 of all fields)
    #[serde(skip)]
    pub current_hash: Option<String>,
}

impl AuditEvent {
    /// Create a new audit event
    pub fn new(
        actor: String,
        action: AuditAction,
        resource: String,
        outcome: AuditOutcome,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            actor,
            source_ip: None,
            action,
            resource,
            outcome,
            status_code: None,
            error_message: None,
            request_id: None,
            session_id: None,
            user_agent: None,
            metadata: HashMap::new(),
            prev_hash: None,
            current_hash: None,
        }
    }

    /// Set source IP address
    pub fn with_source_ip(mut self, ip: String) -> Self {
        self.source_ip = Some(ip);
        self
    }

    /// Set status code
    pub fn with_status_code(mut self, code: u16) -> Self {
        self.status_code = Some(code);
        self
    }

    /// Set error message
    pub fn with_error(mut self, message: String) -> Self {
        self.error_message = Some(message);
        self
    }

    /// Set request ID
    pub fn with_request_id(mut self, id: String) -> Self {
        self.request_id = Some(id);
        self
    }

    /// Set session ID
    pub fn with_session_id(mut self, id: String) -> Self {
        self.session_id = Some(id);
        self
    }

    /// Set user agent
    pub fn with_user_agent(mut self, ua: String) -> Self {
        self.user_agent = Some(ua);
        self
    }

    /// Add metadata key-value pair
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Set previous hash for chain integrity
    pub fn with_prev_hash(mut self, hash: String) -> Self {
        self.prev_hash = Some(hash);
        self
    }

    /// Compute HMAC-SHA256 hash of this event
    pub fn compute_hash(&self, secret: &[u8]) -> AuditResult<String> {
        let mut mac =
            HmacSha256::new_from_slice(secret).map_err(|e| AuditError::Hmac(e.to_string()))?;

        // Include all fields in the hash (except current_hash itself)
        mac.update(self.event_id.as_bytes());
        mac.update(self.timestamp.to_rfc3339().as_bytes());
        mac.update(self.actor.as_bytes());
        if let Some(ip) = &self.source_ip {
            mac.update(ip.as_bytes());
        }
        let action_str = serde_json::to_string(&self.action)?;
        mac.update(action_str.as_bytes());
        mac.update(self.resource.as_bytes());
        let outcome_str = serde_json::to_string(&self.outcome)?;
        mac.update(outcome_str.as_bytes());
        if let Some(code) = self.status_code {
            mac.update(&code.to_be_bytes());
        }
        if let Some(prev) = &self.prev_hash {
            mac.update(prev.as_bytes());
        }

        let result = mac.finalize();
        Ok(hex::encode(result.into_bytes()))
    }

    /// Verify this event's hash
    pub fn verify_hash(&self, secret: &[u8]) -> AuditResult<bool> {
        if let Some(stored_hash) = &self.current_hash {
            let computed_hash = self.compute_hash(secret)?;
            Ok(stored_hash == &computed_hash)
        } else {
            Ok(false)
        }
    }
}

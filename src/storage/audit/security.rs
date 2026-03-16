//! Security event detection via pattern matching.

use super::types::{AuditAction, AuditEvent, SecuritySeverity};
use chrono::Utc;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

// Type aliases for complex types
type FailedAuthMap = Arc<RwLock<HashMap<String, VecDeque<chrono::DateTime<Utc>>>>>;
type AccessPatternMap = Arc<RwLock<HashMap<String, VecDeque<(chrono::DateTime<Utc>, String)>>>>;

/// Security event detected by the detector
#[derive(Debug, Clone)]
pub struct SecurityEvent {
    pub event_type: String,
    pub severity: SecuritySeverity,
    pub description: String,
    pub related_events: Vec<String>,
}

/// Security event detector using pattern matching
pub struct SecurityEventDetector {
    // Track failed auth attempts per IP
    failed_auth_attempts: FailedAuthMap,

    // Track unusual access patterns
    access_patterns: AccessPatternMap,
}

impl SecurityEventDetector {
    pub fn new() -> Self {
        Self {
            failed_auth_attempts: Arc::new(RwLock::new(HashMap::new())),
            access_patterns: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Detect security events from an audit event
    pub async fn detect(&self, event: &AuditEvent) -> Option<SecurityEvent> {
        // Detect multiple failed authentication attempts
        if event.action == AuditAction::AuthFailure {
            if let Some(ip) = &event.source_ip {
                return self.detect_brute_force(ip, event).await;
            }
        }

        // Detect privilege escalation
        if event.action == AuditAction::ModifyPermissions
            || event.action == AuditAction::PutBucketPolicy
        {
            return Some(SecurityEvent {
                event_type: "privilege_escalation_attempt".to_string(),
                severity: SecuritySeverity::High,
                description: format!("User {} attempted to modify permissions", event.actor),
                related_events: vec![event.event_id.clone()],
            });
        }

        // Detect unusual access patterns
        if matches!(
            event.action,
            AuditAction::GetObject | AuditAction::DeleteObject
        ) {
            return self.detect_unusual_access(&event.actor, event).await;
        }

        None
    }

    /// Detect brute force authentication attempts
    async fn detect_brute_force(&self, ip: &str, event: &AuditEvent) -> Option<SecurityEvent> {
        let mut attempts = self.failed_auth_attempts.write().await;
        let entry = attempts.entry(ip.to_string()).or_insert_with(VecDeque::new);

        // Add current attempt
        entry.push_back(event.timestamp);

        // Remove attempts older than 5 minutes
        let cutoff = Utc::now() - chrono::Duration::minutes(5);
        while let Some(front) = entry.front() {
            if front < &cutoff {
                entry.pop_front();
            } else {
                break;
            }
        }

        // Check if we have more than 5 failed attempts in 5 minutes
        if entry.len() >= 5 {
            return Some(SecurityEvent {
                event_type: "brute_force_attack".to_string(),
                severity: SecuritySeverity::Critical,
                description: format!("Multiple failed authentication attempts from IP {}", ip),
                related_events: vec![event.event_id.clone()],
            });
        }

        None
    }

    /// Detect unusual access patterns
    async fn detect_unusual_access(
        &self,
        actor: &str,
        event: &AuditEvent,
    ) -> Option<SecurityEvent> {
        let mut patterns = self.access_patterns.write().await;
        let entry = patterns
            .entry(actor.to_string())
            .or_insert_with(VecDeque::new);

        // Add current access
        entry.push_back((event.timestamp, event.resource.clone()));

        // Remove accesses older than 1 minute
        let cutoff = Utc::now() - chrono::Duration::minutes(1);
        while let Some((timestamp, _)) = entry.front() {
            if timestamp < &cutoff {
                entry.pop_front();
            } else {
                break;
            }
        }

        // Check if we have more than 100 accesses in 1 minute (potential data exfiltration)
        if entry.len() >= 100 {
            return Some(SecurityEvent {
                event_type: "unusual_access_pattern".to_string(),
                severity: SecuritySeverity::Medium,
                description: format!("User {} made {} accesses in 1 minute", actor, entry.len()),
                related_events: vec![event.event_id.clone()],
            });
        }

        None
    }
}

impl Default for SecurityEventDetector {
    fn default() -> Self {
        Self::new()
    }
}

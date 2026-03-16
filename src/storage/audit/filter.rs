//! Audit event filter for querying the audit log.

use super::types::{AuditAction, AuditEvent, AuditOutcome};
use chrono::{DateTime, Utc};

/// Filter for querying audit events
#[derive(Debug, Default)]
pub struct AuditFilter {
    pub actor: Option<String>,
    pub action: Option<AuditAction>,
    pub resource: Option<String>,
    pub outcome: Option<AuditOutcome>,
    pub from_time: Option<DateTime<Utc>>,
    pub to_time: Option<DateTime<Utc>>,
    pub source_ip: Option<String>,
    pub limit: Option<usize>,
}

impl AuditFilter {
    pub fn matches(&self, event: &AuditEvent) -> bool {
        if let Some(ref actor) = self.actor {
            if &event.actor != actor {
                return false;
            }
        }

        if let Some(ref action) = self.action {
            if &event.action != action {
                return false;
            }
        }

        if let Some(ref resource) = self.resource {
            if !event.resource.contains(resource) {
                return false;
            }
        }

        if let Some(ref outcome) = self.outcome {
            if &event.outcome != outcome {
                return false;
            }
        }

        if let Some(ref from) = self.from_time {
            if &event.timestamp < from {
                return false;
            }
        }

        if let Some(ref to) = self.to_time {
            if &event.timestamp > to {
                return false;
            }
        }

        if let Some(ref ip) = self.source_ip {
            if event.source_ip.as_ref() != Some(ip) {
                return false;
            }
        }

        true
    }
}

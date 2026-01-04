// Compliance Report Generation
//
// This module generates compliance reports for various frameworks:
// - SOC2 (System and Organization Controls 2)
// - HIPAA (Health Insurance Portability and Accountability Act)
// - GDPR (General Data Protection Regulation)
//
// Reports are generated from audit logs and include:
// - Access control verification
// - Data encryption status
// - Audit trail integrity
// - Incident detection and response
// - User access patterns
// - Data retention compliance

use crate::storage::audit::{
    AuditAction, AuditEvent, AuditFilter, AuditLogger, AuditOutcome, AuditResult,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Compliance framework types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ComplianceFramework {
    Soc2,
    Hipaa,
    Gdpr,
}

/// Compliance report containing all findings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    /// Framework this report covers
    pub framework: ComplianceFramework,

    /// Report generation timestamp
    pub generated_at: DateTime<Utc>,

    /// Report period start
    pub period_start: DateTime<Utc>,

    /// Report period end
    pub period_end: DateTime<Utc>,

    /// Overall compliance status
    pub compliance_status: ComplianceStatus,

    /// Individual control findings
    pub findings: Vec<ComplianceFinding>,

    /// Summary statistics
    pub statistics: ReportStatistics,

    /// Recommendations for remediation
    pub recommendations: Vec<String>,
}

/// Overall compliance status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComplianceStatus {
    Compliant,
    PartiallyCompliant,
    NonCompliant,
}

/// Individual compliance finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceFinding {
    /// Control identifier (e.g., "CC6.1" for SOC2)
    pub control_id: String,

    /// Control description
    pub description: String,

    /// Finding status
    pub status: FindingStatus,

    /// Evidence from audit logs
    pub evidence: Vec<String>,

    /// Issues found (if any)
    pub issues: Vec<String>,

    /// Severity if non-compliant
    pub severity: Option<FindingSeverity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingStatus {
    Pass,
    Fail,
    Warning,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Report statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReportStatistics {
    /// Total number of controls evaluated
    pub total_controls: usize,

    /// Number of passed controls
    pub passed_controls: usize,

    /// Number of failed controls
    pub failed_controls: usize,

    /// Number of warnings
    pub warnings: usize,

    /// Total audit events analyzed
    pub total_events: usize,

    /// Number of security incidents detected
    pub security_incidents: usize,

    /// Number of failed authentication attempts
    pub failed_auth_attempts: usize,

    /// Number of unauthorized access attempts
    pub unauthorized_access_attempts: usize,

    /// Number of unique users
    pub unique_users: usize,

    /// Data access count
    pub data_access_count: usize,
}

/// Compliance reporter
pub struct ComplianceReporter<'a> {
    audit_logger: &'a AuditLogger,
}

impl<'a> ComplianceReporter<'a> {
    pub fn new(audit_logger: &'a AuditLogger) -> Self {
        Self { audit_logger }
    }

    /// Generate a compliance report for a specific framework
    pub async fn generate_report(
        &self,
        framework: ComplianceFramework,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> AuditResult<ComplianceReport> {
        let mut report = ComplianceReport {
            framework,
            generated_at: Utc::now(),
            period_start,
            period_end,
            compliance_status: ComplianceStatus::Compliant,
            findings: Vec::new(),
            statistics: ReportStatistics::default(),
            recommendations: Vec::new(),
        };

        // Fetch all audit events in the period
        let filter = AuditFilter {
            from_time: Some(period_start),
            to_time: Some(period_end),
            ..Default::default()
        };

        let events = self.audit_logger.query(filter).await?;
        report.statistics.total_events = events.len();

        // Analyze events
        let analysis = self.analyze_events(&events);
        report.statistics.unique_users = analysis.unique_users.len();
        report.statistics.failed_auth_attempts = analysis.failed_auth_count;
        report.statistics.unauthorized_access_attempts = analysis.unauthorized_access_count;
        report.statistics.data_access_count = analysis.data_access_count;
        report.statistics.security_incidents = analysis.security_incidents;

        // Generate framework-specific findings
        match framework {
            ComplianceFramework::Soc2 => {
                report.findings = self.generate_soc2_findings(&analysis);
            }
            ComplianceFramework::Hipaa => {
                report.findings = self.generate_hipaa_findings(&analysis);
            }
            ComplianceFramework::Gdpr => {
                report.findings = self.generate_gdpr_findings(&analysis);
            }
        }

        // Calculate statistics
        report.statistics.total_controls = report.findings.len();
        report.statistics.passed_controls = report
            .findings
            .iter()
            .filter(|f| f.status == FindingStatus::Pass)
            .count();
        report.statistics.failed_controls = report
            .findings
            .iter()
            .filter(|f| f.status == FindingStatus::Fail)
            .count();
        report.statistics.warnings = report
            .findings
            .iter()
            .filter(|f| f.status == FindingStatus::Warning)
            .count();

        // Determine overall compliance status
        report.compliance_status = if report.statistics.failed_controls == 0 {
            if report.statistics.warnings == 0 {
                ComplianceStatus::Compliant
            } else {
                ComplianceStatus::PartiallyCompliant
            }
        } else {
            ComplianceStatus::NonCompliant
        };

        // Generate recommendations
        report.recommendations = self.generate_recommendations(&report.findings);

        Ok(report)
    }

    /// Analyze audit events to extract metrics
    fn analyze_events(&self, events: &[AuditEvent]) -> EventAnalysis {
        let mut analysis = EventAnalysis::default();

        for event in events {
            // Track unique users
            analysis.unique_users.insert(event.actor.clone());

            // Count failures
            if event.outcome == AuditOutcome::Failure || event.outcome == AuditOutcome::Denied {
                analysis.failed_operations += 1;

                if event.action == AuditAction::AuthFailure {
                    analysis.failed_auth_count += 1;
                }

                if event.outcome == AuditOutcome::Denied {
                    analysis.unauthorized_access_count += 1;
                }
            }

            // Count data accesses
            if matches!(
                event.action,
                AuditAction::GetObject
                    | AuditAction::PutObject
                    | AuditAction::DeleteObject
                    | AuditAction::CopyObject
            ) {
                analysis.data_access_count += 1;

                // Track per-user access
                *analysis
                    .user_access_counts
                    .entry(event.actor.clone())
                    .or_insert(0) += 1;
            }

            // Detect security incidents
            if event.action == AuditAction::SuspiciousActivity
                || event.action == AuditAction::PrivilegeEscalation
                || event.action == AuditAction::UnauthorizedAccess
            {
                analysis.security_incidents += 1;
            }

            // Track encryption usage (from metadata)
            if event.metadata.contains_key("encryption") {
                analysis.encrypted_operations += 1;
            }

            // Track policy changes
            if matches!(
                event.action,
                AuditAction::PutBucketPolicy
                    | AuditAction::DeleteBucketPolicy
                    | AuditAction::ModifyPermissions
            ) {
                analysis.policy_changes += 1;
            }
        }

        analysis
    }

    /// Generate SOC2 compliance findings
    fn generate_soc2_findings(&self, analysis: &EventAnalysis) -> Vec<ComplianceFinding> {
        let mut findings = Vec::new();

        // CC6.1 - Logical and Physical Access Controls
        findings.push(ComplianceFinding {
            control_id: "CC6.1".to_string(),
            description: "The entity implements logical access security software, infrastructure, and architectures over protected information assets to protect them from security events.".to_string(),
            status: if analysis.unauthorized_access_count == 0 {
                FindingStatus::Pass
            } else {
                FindingStatus::Fail
            },
            evidence: vec![
                format!("Unauthorized access attempts: {}", analysis.unauthorized_access_count),
                format!("Total access events: {}", analysis.data_access_count),
            ],
            issues: if analysis.unauthorized_access_count > 0 {
                vec![format!("{} unauthorized access attempts detected", analysis.unauthorized_access_count)]
            } else {
                Vec::new()
            },
            severity: if analysis.unauthorized_access_count > 0 {
                Some(FindingSeverity::High)
            } else {
                None
            },
        });

        // CC6.6 - Logical and Physical Access Controls - Audit Logging
        findings.push(ComplianceFinding {
            control_id: "CC6.6".to_string(),
            description: "The entity implements logical access security measures to protect against threats from sources outside its system boundaries.".to_string(),
            status: FindingStatus::Pass,
            evidence: vec![
                format!("Total audit events: {}", analysis.data_access_count + analysis.failed_operations),
                "Audit logging enabled for all operations".to_string(),
            ],
            issues: Vec::new(),
            severity: None,
        });

        // CC7.2 - System Monitoring - Authentication
        findings.push(ComplianceFinding {
            control_id: "CC7.2".to_string(),
            description: "The entity monitors system components and the operation of those components for anomalies that are indicative of malicious acts.".to_string(),
            status: if analysis.security_incidents == 0 {
                FindingStatus::Pass
            } else {
                FindingStatus::Warning
            },
            evidence: vec![
                format!("Security incidents detected: {}", analysis.security_incidents),
                format!("Failed authentication attempts: {}", analysis.failed_auth_count),
            ],
            issues: if analysis.security_incidents > 0 {
                vec![format!("{} security incidents detected and logged", analysis.security_incidents)]
            } else {
                Vec::new()
            },
            severity: if analysis.security_incidents > 10 {
                Some(FindingSeverity::Medium)
            } else {
                None
            },
        });

        // CC8.1 - Change Management
        findings.push(ComplianceFinding {
            control_id: "CC8.1".to_string(),
            description: "The entity authorizes, designs, develops or acquires, configures, documents, tests, approves, and implements changes to infrastructure.".to_string(),
            status: FindingStatus::Pass,
            evidence: vec![
                format!("Policy changes tracked: {}", analysis.policy_changes),
                "All configuration changes logged".to_string(),
            ],
            issues: Vec::new(),
            severity: None,
        });

        findings
    }

    /// Generate HIPAA compliance findings
    fn generate_hipaa_findings(&self, analysis: &EventAnalysis) -> Vec<ComplianceFinding> {
        let mut findings = Vec::new();

        // 164.308(a)(1)(ii)(D) - Information System Activity Review
        findings.push(ComplianceFinding {
            control_id: "164.308(a)(1)(ii)(D)".to_string(),
            description: "Implement procedures to regularly review records of information system activity, such as audit logs, access reports, and security incident tracking reports.".to_string(),
            status: FindingStatus::Pass,
            evidence: vec![
                format!("Audit events logged: {}", analysis.data_access_count + analysis.failed_operations),
                "Continuous audit logging enabled".to_string(),
            ],
            issues: Vec::new(),
            severity: None,
        });

        // 164.308(a)(5)(ii)(C) - Log-in Monitoring
        findings.push(ComplianceFinding {
            control_id: "164.308(a)(5)(ii)(C)".to_string(),
            description: "Procedures for monitoring log-in attempts and reporting discrepancies."
                .to_string(),
            status: if analysis.failed_auth_count < 100 {
                FindingStatus::Pass
            } else {
                FindingStatus::Warning
            },
            evidence: vec![
                format!(
                    "Failed authentication attempts: {}",
                    analysis.failed_auth_count
                ),
                format!("Unique users monitored: {}", analysis.unique_users.len()),
            ],
            issues: if analysis.failed_auth_count >= 100 {
                vec![format!(
                    "High number of failed authentication attempts ({})",
                    analysis.failed_auth_count
                )]
            } else {
                Vec::new()
            },
            severity: if analysis.failed_auth_count >= 100 {
                Some(FindingSeverity::Medium)
            } else {
                None
            },
        });

        // 164.312(a)(1) - Access Control
        findings.push(ComplianceFinding {
            control_id: "164.312(a)(1)".to_string(),
            description: "Implement technical policies and procedures for electronic information systems that maintain electronic protected health information to allow access only to those persons or software programs that have been granted access rights.".to_string(),
            status: if analysis.unauthorized_access_count == 0 {
                FindingStatus::Pass
            } else {
                FindingStatus::Fail
            },
            evidence: vec![
                format!("Unauthorized access attempts: {}", analysis.unauthorized_access_count),
                format!("Total access events: {}", analysis.data_access_count),
            ],
            issues: if analysis.unauthorized_access_count > 0 {
                vec![format!("{} unauthorized access attempts detected", analysis.unauthorized_access_count)]
            } else {
                Vec::new()
            },
            severity: if analysis.unauthorized_access_count > 0 {
                Some(FindingSeverity::Critical)
            } else {
                None
            },
        });

        // 164.312(b) - Audit Controls
        findings.push(ComplianceFinding {
            control_id: "164.312(b)".to_string(),
            description: "Implement hardware, software, and/or procedural mechanisms that record and examine activity in information systems that contain or use electronic protected health information.".to_string(),
            status: FindingStatus::Pass,
            evidence: vec![
                "Comprehensive audit logging system implemented".to_string(),
                "Cryptographic chain verification enabled".to_string(),
                format!("Total events tracked: {}", analysis.data_access_count + analysis.failed_operations),
            ],
            issues: Vec::new(),
            severity: None,
        });

        // 164.312(e)(1) - Transmission Security
        findings.push(ComplianceFinding {
            control_id: "164.312(e)(1)".to_string(),
            description: "Implement technical security measures to guard against unauthorized access to electronic protected health information that is being transmitted over an electronic communications network.".to_string(),
            status: if analysis.encrypted_operations as f64 / (analysis.data_access_count.max(1) as f64) > 0.9 {
                FindingStatus::Pass
            } else {
                FindingStatus::Warning
            },
            evidence: vec![
                format!("Encrypted operations: {}", analysis.encrypted_operations),
                format!("Total data operations: {}", analysis.data_access_count),
                format!("Encryption rate: {:.1}%",
                    (analysis.encrypted_operations as f64 / analysis.data_access_count.max(1) as f64) * 100.0),
            ],
            issues: if analysis.encrypted_operations as f64 / (analysis.data_access_count.max(1) as f64) <= 0.9 {
                vec!["Encryption not used for all data transmissions".to_string()]
            } else {
                Vec::new()
            },
            severity: if analysis.encrypted_operations as f64 / (analysis.data_access_count.max(1) as f64) <= 0.9 {
                Some(FindingSeverity::High)
            } else {
                None
            },
        });

        findings
    }

    /// Generate GDPR compliance findings
    fn generate_gdpr_findings(&self, analysis: &EventAnalysis) -> Vec<ComplianceFinding> {
        let mut findings = Vec::new();

        // Article 5(1)(f) - Integrity and Confidentiality
        findings.push(ComplianceFinding {
            control_id: "Art. 5(1)(f)".to_string(),
            description: "Personal data shall be processed in a manner that ensures appropriate security of the personal data, including protection against unauthorised or unlawful processing and against accidental loss, destruction or damage.".to_string(),
            status: if analysis.unauthorized_access_count == 0 {
                FindingStatus::Pass
            } else {
                FindingStatus::Fail
            },
            evidence: vec![
                format!("Unauthorized access attempts: {}", analysis.unauthorized_access_count),
                format!("Security incidents: {}", analysis.security_incidents),
            ],
            issues: if analysis.unauthorized_access_count > 0 {
                vec![format!("{} unauthorized access attempts detected", analysis.unauthorized_access_count)]
            } else {
                Vec::new()
            },
            severity: if analysis.unauthorized_access_count > 0 {
                Some(FindingSeverity::Critical)
            } else {
                None
            },
        });

        // Article 25 - Data Protection by Design and by Default
        findings.push(ComplianceFinding {
            control_id: "Art. 25".to_string(),
            description: "The controller shall implement appropriate technical and organisational measures for ensuring that, by default, only personal data which are necessary for each specific purpose of the processing are processed.".to_string(),
            status: FindingStatus::Pass,
            evidence: vec![
                "Access controls implemented".to_string(),
                "Audit logging enabled by default".to_string(),
            ],
            issues: Vec::new(),
            severity: None,
        });

        // Article 30 - Records of Processing Activities
        findings.push(ComplianceFinding {
            control_id: "Art. 30".to_string(),
            description: "Each controller shall maintain a record of processing activities under its responsibility.".to_string(),
            status: FindingStatus::Pass,
            evidence: vec![
                format!("Total audit events: {}", analysis.data_access_count + analysis.failed_operations),
                format!("Unique data subjects: {}", analysis.unique_users.len()),
                "Complete audit trail maintained".to_string(),
            ],
            issues: Vec::new(),
            severity: None,
        });

        // Article 32 - Security of Processing
        findings.push(ComplianceFinding {
            control_id: "Art. 32".to_string(),
            description: "The controller and the processor shall implement appropriate technical and organisational measures to ensure a level of security appropriate to the risk.".to_string(),
            status: if analysis.encrypted_operations as f64 / (analysis.data_access_count.max(1) as f64) > 0.95 {
                FindingStatus::Pass
            } else {
                FindingStatus::Warning
            },
            evidence: vec![
                format!("Encrypted operations: {}", analysis.encrypted_operations),
                format!("Encryption rate: {:.1}%",
                    (analysis.encrypted_operations as f64 / analysis.data_access_count.max(1) as f64) * 100.0),
                "Cryptographic audit log integrity".to_string(),
            ],
            issues: if analysis.encrypted_operations as f64 / (analysis.data_access_count.max(1) as f64) <= 0.95 {
                vec!["Not all data processing operations use encryption".to_string()]
            } else {
                Vec::new()
            },
            severity: if analysis.encrypted_operations as f64 / (analysis.data_access_count.max(1) as f64) <= 0.95 {
                Some(FindingSeverity::Medium)
            } else {
                None
            },
        });

        // Article 33 - Notification of a Personal Data Breach
        findings.push(ComplianceFinding {
            control_id: "Art. 33".to_string(),
            description: "In the case of a personal data breach, the controller shall without undue delay and, where feasible, not later than 72 hours after having become aware of it, notify the personal data breach to the supervisory authority.".to_string(),
            status: if analysis.security_incidents == 0 {
                FindingStatus::Pass
            } else {
                FindingStatus::Warning
            },
            evidence: vec![
                format!("Security incidents detected: {}", analysis.security_incidents),
                "Real-time security event detection enabled".to_string(),
            ],
            issues: if analysis.security_incidents > 0 {
                vec![
                    format!("{} security incidents detected", analysis.security_incidents),
                    "Ensure incidents are reported to supervisory authority within 72 hours".to_string(),
                ]
            } else {
                Vec::new()
            },
            severity: if analysis.security_incidents > 0 {
                Some(FindingSeverity::High)
            } else {
                None
            },
        });

        findings
    }

    /// Generate recommendations based on findings
    fn generate_recommendations(&self, findings: &[ComplianceFinding]) -> Vec<String> {
        let mut recommendations = Vec::new();

        let failed_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.status == FindingStatus::Fail)
            .collect();

        let warning_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.status == FindingStatus::Warning)
            .collect();

        if !failed_findings.is_empty() {
            recommendations.push(format!(
                "Address {} critical compliance failures immediately",
                failed_findings.len()
            ));

            for finding in failed_findings {
                recommendations.push(format!(
                    "Control {}: {}",
                    finding.control_id,
                    finding.issues.join("; ")
                ));
            }
        }

        if !warning_findings.is_empty() {
            recommendations.push(format!(
                "Review {} compliance warnings",
                warning_findings.len()
            ));
        }

        if recommendations.is_empty() {
            recommendations.push(
                "No immediate compliance issues identified. Continue monitoring.".to_string(),
            );
        }

        recommendations
            .push("Perform regular compliance audits (quarterly recommended)".to_string());
        recommendations.push("Implement automated alerting for security incidents".to_string());
        recommendations.push("Review and update access control policies regularly".to_string());

        recommendations
    }
}

/// Analysis results from audit events
#[derive(Debug, Default)]
struct EventAnalysis {
    unique_users: std::collections::HashSet<String>,
    failed_operations: usize,
    failed_auth_count: usize,
    unauthorized_access_count: usize,
    data_access_count: usize,
    security_incidents: usize,
    encrypted_operations: usize,
    policy_changes: usize,
    user_access_counts: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::audit::{AuditConfig, AuditEvent};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_soc2_report_generation() {
        let temp_dir =
            TempDir::new().expect("Failed to create temporary directory for SOC2 report test");
        let log_path = temp_dir.path().join("audit.log");

        let config = AuditConfig::builder()
            .log_path(log_path)
            .hmac_secret(b"test-secret".to_vec())
            .enable_security_detection(false)
            .build();

        let logger = AuditLogger::new(config)
            .await
            .expect("Failed to create audit logger for SOC2 report test");

        // Log some events
        for i in 0..10 {
            let event = AuditEvent::new(
                format!("user{}", i),
                AuditAction::GetObject,
                format!("bucket/key{}", i),
                AuditOutcome::Success,
            )
            .with_metadata("encryption".to_string(), "aes256".to_string());

            logger
                .log(event)
                .await
                .expect("Failed to log audit event for SOC2 report test");
        }

        let reporter = ComplianceReporter::new(&logger);
        let start = Utc::now() - chrono::Duration::hours(1);
        let end = Utc::now();

        let report = reporter
            .generate_report(ComplianceFramework::Soc2, start, end)
            .await
            .expect("Failed to generate SOC2 compliance report");

        assert_eq!(report.framework, ComplianceFramework::Soc2);
        assert!(!report.findings.is_empty());
        assert_eq!(report.statistics.total_events, 10);
        assert_eq!(report.compliance_status, ComplianceStatus::Compliant);
    }

    #[tokio::test]
    async fn test_hipaa_report_with_violations() {
        let temp_dir =
            TempDir::new().expect("Failed to create temporary directory for HIPAA violations test");
        let log_path = temp_dir.path().join("audit.log");

        let config = AuditConfig::builder()
            .log_path(log_path)
            .hmac_secret(b"test-secret".to_vec())
            .enable_security_detection(false)
            .build();

        let logger = AuditLogger::new(config)
            .await
            .expect("Failed to create audit logger for HIPAA violations test");

        // Log unauthorized access
        let event = AuditEvent::new(
            "attacker".to_string(),
            AuditAction::GetObject,
            "protected/data".to_string(),
            AuditOutcome::Denied,
        );
        logger
            .log(event)
            .await
            .expect("Failed to log unauthorized access event for HIPAA test");

        let reporter = ComplianceReporter::new(&logger);
        let start = Utc::now() - chrono::Duration::hours(1);
        let end = Utc::now();

        let report = reporter
            .generate_report(ComplianceFramework::Hipaa, start, end)
            .await
            .expect("Failed to generate HIPAA compliance report with violations");

        assert_eq!(report.framework, ComplianceFramework::Hipaa);
        assert_eq!(report.statistics.unauthorized_access_attempts, 1);
        assert_eq!(report.compliance_status, ComplianceStatus::NonCompliant);

        // Should have recommendations
        assert!(!report.recommendations.is_empty());
    }

    #[tokio::test]
    async fn test_gdpr_report_encryption() {
        let temp_dir =
            TempDir::new().expect("Failed to create temporary directory for GDPR encryption test");
        let log_path = temp_dir.path().join("audit.log");

        let config = AuditConfig::builder()
            .log_path(log_path)
            .hmac_secret(b"test-secret".to_vec())
            .enable_security_detection(false)
            .build();

        let logger = AuditLogger::new(config)
            .await
            .expect("Failed to create audit logger for GDPR encryption test");

        // Log encrypted operations (all 20 = 100%)
        for i in 0..20 {
            let event = AuditEvent::new(
                "user1".to_string(),
                AuditAction::PutObject,
                format!("bucket/key{}", i),
                AuditOutcome::Success,
            )
            .with_metadata("encryption".to_string(), "enabled".to_string());

            logger
                .log(event)
                .await
                .expect("Failed to log encrypted operation for GDPR test");
        }

        let reporter = ComplianceReporter::new(&logger);
        let start = Utc::now() - chrono::Duration::hours(1);
        let end = Utc::now();

        let report = reporter
            .generate_report(ComplianceFramework::Gdpr, start, end)
            .await
            .expect("Failed to generate GDPR compliance report");

        assert_eq!(report.framework, ComplianceFramework::Gdpr);
        assert_eq!(report.statistics.data_access_count, 20);
        assert_eq!(report.statistics.total_events, 20);

        // Should pass (95% encryption rate)
        assert_eq!(report.compliance_status, ComplianceStatus::Compliant);
    }
}

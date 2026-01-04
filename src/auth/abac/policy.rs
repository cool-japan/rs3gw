//! ABAC policy definitions

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Policy effect (Allow or Deny)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Effect {
    Allow,
    Deny,
}

/// Access decision result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessDecision {
    /// Access is explicitly allowed
    Allow,
    /// Access is explicitly denied
    Deny,
    /// No applicable policy (implicit deny in AWS model)
    NoMatch,
}

/// Policy statement with conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PolicyStatement {
    /// Statement ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sid: Option<String>,

    /// Effect (Allow or Deny)
    pub effect: Effect,

    /// Actions (e.g., "s3:GetObject", "s3:PutObject", "s3:*")
    pub action: Vec<String>,

    /// Resources (e.g., "arn:aws:s3:::bucket/*")
    pub resource: Vec<String>,

    /// Optional conditions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<PolicyCondition>,
}

/// Policy conditions for fine-grained control
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PolicyCondition {
    /// IP address restrictions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<IpAddressCondition>,

    /// Time-based restrictions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_time: Option<DateTimeCondition>,

    /// String conditions (for tags, prefixes, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub string_like: Option<HashMap<String, Vec<String>>>,

    /// Numeric conditions (for object size, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric: Option<NumericCondition>,

    /// Boolean conditions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bool: Option<HashMap<String, bool>>,
}

/// IP address condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpAddressCondition {
    /// Whitelist of IP addresses/CIDR ranges
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ip: Option<Vec<String>>,

    /// Blacklist of IP addresses/CIDR ranges
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_source_ip: Option<Vec<String>>,
}

/// Date/time condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateTimeCondition {
    /// Access allowed after this time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_greater_than: Option<DateTime<Utc>>,

    /// Access allowed before this time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_less_than: Option<DateTime<Utc>>,

    /// Access allowed only on these days of week
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_of_week: Option<Vec<u8>>, // 0 = Sunday, 6 = Saturday

    /// Access allowed only during these hours (0-23)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hours_of_day: Option<Vec<u8>>,
}

/// Numeric condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericCondition {
    /// Greater than
    #[serde(skip_serializing_if = "Option::is_none")]
    pub greater_than: Option<HashMap<String, i64>>,

    /// Less than
    #[serde(skip_serializing_if = "Option::is_none")]
    pub less_than: Option<HashMap<String, i64>>,

    /// Equals
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equals: Option<HashMap<String, i64>>,
}

/// Complete ABAC policy document
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AbacPolicy {
    /// Policy version
    pub version: String,

    /// Policy ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Policy statements
    pub statement: Vec<PolicyStatement>,
}

impl AbacPolicy {
    /// Create a new empty policy
    pub fn new() -> Self {
        Self {
            version: "2012-10-17".to_string(),
            id: None,
            statement: Vec::new(),
        }
    }

    /// Add a statement to the policy
    pub fn add_statement(&mut self, statement: PolicyStatement) {
        self.statement.push(statement);
    }

    /// Create a policy from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize policy to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

impl Default for AbacPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyStatement {
    /// Create a new policy statement
    pub fn new(effect: Effect, action: Vec<String>, resource: Vec<String>) -> Self {
        Self {
            sid: None,
            effect,
            action,
            resource,
            condition: None,
        }
    }

    /// Add a condition to the statement
    pub fn with_condition(mut self, condition: PolicyCondition) -> Self {
        self.condition = Some(condition);
        self
    }

    /// Set statement ID
    pub fn with_sid(mut self, sid: String) -> Self {
        self.sid = Some(sid);
        self
    }
}

impl PolicyCondition {
    /// Create a new empty condition
    pub fn new() -> Self {
        Self {
            ip_address: None,
            date_time: None,
            string_like: None,
            numeric: None,
            bool: None,
        }
    }

    /// Add IP address condition
    pub fn with_ip_whitelist(mut self, ips: Vec<String>) -> Self {
        self.ip_address = Some(IpAddressCondition {
            source_ip: Some(ips),
            not_source_ip: None,
        });
        self
    }

    /// Add IP address blacklist
    pub fn with_ip_blacklist(mut self, ips: Vec<String>) -> Self {
        self.ip_address = Some(IpAddressCondition {
            source_ip: None,
            not_source_ip: Some(ips),
        });
        self
    }

    /// Add time window condition
    pub fn with_time_window(
        mut self,
        after: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
    ) -> Self {
        self.date_time = Some(DateTimeCondition {
            date_greater_than: after,
            date_less_than: before,
            days_of_week: None,
            hours_of_day: None,
        });
        self
    }

    /// Add days of week restriction
    pub fn with_days_of_week(mut self, days: Vec<u8>) -> Self {
        if let Some(ref mut dt) = self.date_time {
            dt.days_of_week = Some(days);
        } else {
            self.date_time = Some(DateTimeCondition {
                date_greater_than: None,
                date_less_than: None,
                days_of_week: Some(days),
                hours_of_day: None,
            });
        }
        self
    }

    /// Add hours of day restriction
    pub fn with_hours_of_day(mut self, hours: Vec<u8>) -> Self {
        if let Some(ref mut dt) = self.date_time {
            dt.hours_of_day = Some(hours);
        } else {
            self.date_time = Some(DateTimeCondition {
                date_greater_than: None,
                date_less_than: None,
                days_of_week: None,
                hours_of_day: Some(hours),
            });
        }
        self
    }
}

impl Default for PolicyCondition {
    fn default() -> Self {
        Self::new()
    }
}

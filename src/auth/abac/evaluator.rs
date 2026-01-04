//! Policy evaluator for ABAC

use super::ip_filter::IpFilter;
use super::policy::{AbacPolicy, AccessDecision, Effect, PolicyCondition, PolicyStatement};
use super::time_window::TimeWindow;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::net::IpAddr;

/// Request context for policy evaluation
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// Client IP address
    pub source_ip: Option<IpAddr>,
    /// Request time
    pub time: DateTime<Utc>,
    /// S3 action being performed
    pub action: String,
    /// Resource being accessed (e.g., bucket/key)
    pub resource: String,
    /// User/principal performing the action
    pub principal: String,
    /// Request metadata (tags, headers, etc.)
    pub metadata: HashMap<String, String>,
    /// Object size (for numeric conditions)
    pub object_size: Option<i64>,
}

impl RequestContext {
    /// Create a new request context
    pub fn new(action: String, resource: String, principal: String) -> Self {
        Self {
            source_ip: None,
            time: Utc::now(),
            action,
            resource,
            principal,
            metadata: HashMap::new(),
            object_size: None,
        }
    }

    /// Set source IP
    pub fn with_source_ip(mut self, ip: IpAddr) -> Self {
        self.source_ip = Some(ip);
        self
    }

    /// Set request time
    pub fn with_time(mut self, time: DateTime<Utc>) -> Self {
        self.time = time;
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Set object size
    pub fn with_object_size(mut self, size: i64) -> Self {
        self.object_size = Some(size);
        self
    }
}

/// Policy evaluator
pub struct PolicyEvaluator {
    policy: AbacPolicy,
}

impl PolicyEvaluator {
    /// Create a new policy evaluator
    pub fn new(policy: AbacPolicy) -> Self {
        Self { policy }
    }

    /// Evaluate a request against the policy
    pub fn evaluate(&self, context: &RequestContext) -> AccessDecision {
        let mut has_allow = false;
        let mut has_deny = false;

        for statement in &self.policy.statement {
            let matches = self.statement_matches(statement, context);

            if matches {
                match statement.effect {
                    Effect::Allow => has_allow = true,
                    Effect::Deny => has_deny = true,
                }
            }
        }

        // Explicit deny always wins
        if has_deny {
            return AccessDecision::Deny;
        }

        // Then explicit allow
        if has_allow {
            return AccessDecision::Allow;
        }

        // No match (implicit deny in AWS model)
        AccessDecision::NoMatch
    }

    /// Check if a statement matches the request context
    fn statement_matches(&self, statement: &PolicyStatement, context: &RequestContext) -> bool {
        // Check action
        if !self.action_matches(&statement.action, &context.action) {
            return false;
        }

        // Check resource
        if !self.resource_matches(&statement.resource, &context.resource) {
            return false;
        }

        // Check conditions
        if let Some(ref condition) = statement.condition {
            if !self.condition_matches(condition, context) {
                return false;
            }
        }

        true
    }

    /// Check if action matches
    fn action_matches(&self, statement_actions: &[String], request_action: &str) -> bool {
        for action in statement_actions {
            if action == "*" || action == request_action {
                return true;
            }

            // Wildcard matching (e.g., "s3:Get*" matches "s3:GetObject")
            if action.ends_with('*') {
                let prefix = &action[..action.len() - 1];
                if request_action.starts_with(prefix) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if resource matches
    fn resource_matches(&self, statement_resources: &[String], request_resource: &str) -> bool {
        for resource in statement_resources {
            if resource == "*" || resource == request_resource {
                return true;
            }

            // Wildcard matching (e.g., "bucket/*" matches "bucket/key")
            if resource.ends_with('*') {
                let prefix = &resource[..resource.len() - 1];
                if request_resource.starts_with(prefix) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if condition matches
    fn condition_matches(&self, condition: &PolicyCondition, context: &RequestContext) -> bool {
        // Check IP address condition
        if let Some(ref ip_cond) = condition.ip_address {
            if let Some(source_ip) = context.source_ip {
                // Build IP filter
                let mut filter = IpFilter::new();

                if let Some(ref whitelist) = ip_cond.source_ip {
                    filter = match IpFilter::with_whitelist(whitelist.clone()) {
                        Ok(f) => f,
                        Err(_) => return false,
                    };
                }

                if let Some(ref blacklist) = ip_cond.not_source_ip {
                    filter = match IpFilter::with_blacklist(blacklist.clone()) {
                        Ok(f) => f,
                        Err(_) => return false,
                    };
                }

                if !filter.is_allowed(&source_ip) {
                    return false;
                }
            } else {
                // No source IP in context, but IP condition exists
                return false;
            }
        }

        // Check date/time condition
        if let Some(ref dt_cond) = condition.date_time {
            let mut window = TimeWindow::new();

            window.start = dt_cond.date_greater_than;
            window.end = dt_cond.date_less_than;
            window.days_of_week = dt_cond.days_of_week.clone();
            window.hours_of_day = dt_cond.hours_of_day.clone();

            if !window.is_allowed(context.time) {
                return false;
            }
        }

        // Check string conditions
        if let Some(ref string_like) = condition.string_like {
            for (key, patterns) in string_like {
                if let Some(value) = context.metadata.get(key) {
                    let mut matches = false;
                    for pattern in patterns {
                        if self.string_matches(value, pattern) {
                            matches = true;
                            break;
                        }
                    }
                    if !matches {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }

        // Check numeric conditions
        if let Some(ref numeric) = condition.numeric {
            if let Some(ref gt_map) = numeric.greater_than {
                for (key, threshold) in gt_map {
                    if key == "s3:object-size" {
                        if let Some(size) = context.object_size {
                            if size <= *threshold {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                }
            }

            if let Some(ref lt_map) = numeric.less_than {
                for (key, threshold) in lt_map {
                    if key == "s3:object-size" {
                        if let Some(size) = context.object_size {
                            if size >= *threshold {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                }
            }

            if let Some(ref eq_map) = numeric.equals {
                for (key, value) in eq_map {
                    if key == "s3:object-size" {
                        if let Some(size) = context.object_size {
                            if size != *value {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                }
            }
        }

        // Check boolean conditions
        if let Some(ref bool_cond) = condition.bool {
            for (key, expected_value) in bool_cond {
                if let Some(value_str) = context.metadata.get(key) {
                    let value = value_str.to_lowercase() == "true";
                    if value != *expected_value {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }

        true
    }

    /// Simple string matching with wildcards
    fn string_matches(&self, value: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        if pattern.contains('*') {
            // Simple wildcard matching
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];
                return value.starts_with(prefix) && value.ends_with(suffix);
            }
        }

        value == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::abac::policy::{PolicyCondition, PolicyStatement};

    #[test]
    fn test_simple_allow_policy() {
        let mut policy = AbacPolicy::new();
        policy.add_statement(PolicyStatement::new(
            Effect::Allow,
            vec!["s3:GetObject".to_string()],
            vec!["bucket/*".to_string()],
        ));

        let evaluator = PolicyEvaluator::new(policy);
        let context = RequestContext::new(
            "s3:GetObject".to_string(),
            "bucket/key".to_string(),
            "user1".to_string(),
        );

        assert_eq!(evaluator.evaluate(&context), AccessDecision::Allow);
    }

    #[test]
    fn test_simple_deny_policy() {
        let mut policy = AbacPolicy::new();
        policy.add_statement(PolicyStatement::new(
            Effect::Deny,
            vec!["s3:DeleteObject".to_string()],
            vec!["bucket/*".to_string()],
        ));

        let evaluator = PolicyEvaluator::new(policy);
        let context = RequestContext::new(
            "s3:DeleteObject".to_string(),
            "bucket/key".to_string(),
            "user1".to_string(),
        );

        assert_eq!(evaluator.evaluate(&context), AccessDecision::Deny);
    }

    #[test]
    fn test_deny_overrides_allow() {
        let mut policy = AbacPolicy::new();
        policy.add_statement(PolicyStatement::new(
            Effect::Allow,
            vec!["s3:*".to_string()],
            vec!["bucket/*".to_string()],
        ));
        policy.add_statement(PolicyStatement::new(
            Effect::Deny,
            vec!["s3:DeleteObject".to_string()],
            vec!["bucket/*".to_string()],
        ));

        let evaluator = PolicyEvaluator::new(policy);
        let context = RequestContext::new(
            "s3:DeleteObject".to_string(),
            "bucket/key".to_string(),
            "user1".to_string(),
        );

        assert_eq!(evaluator.evaluate(&context), AccessDecision::Deny);
    }

    #[test]
    fn test_ip_whitelist_condition() {
        let condition =
            PolicyCondition::new().with_ip_whitelist(vec!["192.168.1.0/24".to_string()]);

        let mut policy = AbacPolicy::new();
        policy.add_statement(
            PolicyStatement::new(
                Effect::Allow,
                vec!["s3:GetObject".to_string()],
                vec!["bucket/*".to_string()],
            )
            .with_condition(condition),
        );

        let evaluator = PolicyEvaluator::new(policy);

        // Allowed IP
        let context1 = RequestContext::new(
            "s3:GetObject".to_string(),
            "bucket/key".to_string(),
            "user1".to_string(),
        )
        .with_source_ip("192.168.1.100".parse().expect("Failed to parse IP address"));

        assert_eq!(evaluator.evaluate(&context1), AccessDecision::Allow);

        // Blocked IP
        let context2 = RequestContext::new(
            "s3:GetObject".to_string(),
            "bucket/key".to_string(),
            "user1".to_string(),
        )
        .with_source_ip("10.0.0.1".parse().expect("Failed to parse IP address"));

        assert_eq!(evaluator.evaluate(&context2), AccessDecision::NoMatch);
    }
}

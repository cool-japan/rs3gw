//! Comprehensive tests for ABAC system

use super::*;
use chrono::{TimeZone, Utc};
use std::net::IpAddr;

#[test]
fn test_policy_json_serialization() {
    let mut policy = AbacPolicy::new();
    policy.id = Some("test-policy".to_string());
    policy.add_statement(PolicyStatement::new(
        Effect::Allow,
        vec!["s3:GetObject".to_string()],
        vec!["bucket/*".to_string()],
    ));

    let json = policy
        .to_json()
        .expect("Failed to serialize ABAC policy to JSON");
    assert!(json.contains("\"Version\": \"2012-10-17\""));
    assert!(json.contains("\"Effect\": \"Allow\""));

    let parsed = AbacPolicy::from_json(&json).expect("Failed to deserialize ABAC policy from JSON");
    assert_eq!(parsed.version, "2012-10-17");
    assert_eq!(parsed.statement.len(), 1);
}

#[test]
fn test_time_window_business_hours() {
    let window = TimeWindow::business_hours();

    // Monday 10 AM
    let monday_morning = Utc
        .with_ymd_and_hms(2024, 1, 1, 10, 0, 0)
        .single()
        .expect("Failed to create Monday 10 AM datetime for business hours test");
    assert!(window.is_allowed(monday_morning));

    // Saturday 10 AM
    let saturday = Utc
        .with_ymd_and_hms(2024, 1, 6, 10, 0, 0)
        .single()
        .expect("Failed to create Saturday 10 AM datetime for business hours test");
    assert!(!window.is_allowed(saturday));

    // Monday 6 PM
    let monday_evening = Utc
        .with_ymd_and_hms(2024, 1, 1, 18, 0, 0)
        .single()
        .expect("Failed to create Monday 6 PM datetime for business hours test");
    assert!(!window.is_allowed(monday_evening));
}

#[test]
fn test_ip_filter_cidr_ranges() {
    let filter =
        IpFilter::with_whitelist(vec!["192.168.0.0/16".to_string(), "10.0.0.0/8".to_string()])
            .expect("Failed to create IP filter with CIDR ranges");

    let ip1: IpAddr = "192.168.1.100"
        .parse()
        .expect("Failed to parse IP address 192.168.1.100");
    assert!(filter.is_allowed(&ip1));

    let ip2: IpAddr = "10.5.6.7"
        .parse()
        .expect("Failed to parse IP address 10.5.6.7");
    assert!(filter.is_allowed(&ip2));

    let ip3: IpAddr = "172.16.0.1"
        .parse()
        .expect("Failed to parse IP address 172.16.0.1");
    assert!(!filter.is_allowed(&ip3));
}

#[test]
fn test_policy_with_time_restriction() {
    let start = Utc
        .with_ymd_and_hms(2024, 1, 1, 0, 0, 0)
        .single()
        .expect("Failed to create start datetime for time restriction test");
    let end = Utc
        .with_ymd_and_hms(2024, 12, 31, 23, 59, 59)
        .single()
        .expect("Failed to create end datetime for time restriction test");

    let condition = PolicyCondition::new().with_time_window(Some(start), Some(end));

    let mut policy = AbacPolicy::new();
    policy.add_statement(
        PolicyStatement::new(
            Effect::Allow,
            vec!["s3:*".to_string()],
            vec!["bucket/*".to_string()],
        )
        .with_condition(condition),
    );

    let evaluator = PolicyEvaluator::new(policy);

    // Valid time
    let valid_context = RequestContext::new(
        "s3:GetObject".to_string(),
        "bucket/key".to_string(),
        "user1".to_string(),
    )
    .with_time(
        Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0)
            .single()
            .expect("Failed to create valid datetime for time restriction test"),
    );

    assert_eq!(evaluator.evaluate(&valid_context), AccessDecision::Allow);

    // Invalid time (year 2025)
    let invalid_context = RequestContext::new(
        "s3:GetObject".to_string(),
        "bucket/key".to_string(),
        "user1".to_string(),
    )
    .with_time(
        Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
            .single()
            .expect("Failed to create invalid datetime (2025) for time restriction test"),
    );

    assert_eq!(
        evaluator.evaluate(&invalid_context),
        AccessDecision::NoMatch
    );
}

#[test]
fn test_policy_with_days_of_week() {
    let condition = PolicyCondition::new().with_days_of_week(vec![1, 2, 3, 4, 5]); // Mon-Fri

    let mut policy = AbacPolicy::new();
    policy.add_statement(
        PolicyStatement::new(
            Effect::Allow,
            vec!["s3:PutObject".to_string()],
            vec!["uploads/*".to_string()],
        )
        .with_condition(condition),
    );

    let evaluator = PolicyEvaluator::new(policy);

    // Monday - allowed
    let monday_context = RequestContext::new(
        "s3:PutObject".to_string(),
        "uploads/file".to_string(),
        "user1".to_string(),
    )
    .with_time(
        Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0)
            .single()
            .expect("Failed to create Monday datetime for days of week policy test"),
    );

    assert_eq!(evaluator.evaluate(&monday_context), AccessDecision::Allow);

    // Saturday - not allowed
    let saturday_context = RequestContext::new(
        "s3:PutObject".to_string(),
        "uploads/file".to_string(),
        "user1".to_string(),
    )
    .with_time(
        Utc.with_ymd_and_hms(2024, 1, 6, 12, 0, 0)
            .single()
            .expect("Failed to create Saturday datetime for days of week policy test"),
    );

    assert_eq!(
        evaluator.evaluate(&saturday_context),
        AccessDecision::NoMatch
    );
}

#[test]
fn test_policy_with_hours_restriction() {
    let condition = PolicyCondition::new().with_hours_of_day(vec![9, 10, 11, 12, 13, 14, 15, 16]); // 9 AM - 4 PM

    let mut policy = AbacPolicy::new();
    policy.add_statement(
        PolicyStatement::new(
            Effect::Allow,
            vec!["s3:DeleteObject".to_string()],
            vec!["temp/*".to_string()],
        )
        .with_condition(condition),
    );

    let evaluator = PolicyEvaluator::new(policy);

    // 10 AM - allowed
    let morning_context = RequestContext::new(
        "s3:DeleteObject".to_string(),
        "temp/file".to_string(),
        "user1".to_string(),
    )
    .with_time(
        Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0)
            .single()
            .expect("Failed to create 10 AM datetime for hours restriction test"),
    );

    assert_eq!(evaluator.evaluate(&morning_context), AccessDecision::Allow);

    // 8 AM - not allowed
    let early_context = RequestContext::new(
        "s3:DeleteObject".to_string(),
        "temp/file".to_string(),
        "user1".to_string(),
    )
    .with_time(
        Utc.with_ymd_and_hms(2024, 1, 1, 8, 0, 0)
            .single()
            .expect("Failed to create 8 AM datetime for hours restriction test"),
    );

    assert_eq!(evaluator.evaluate(&early_context), AccessDecision::NoMatch);
}

#[test]
fn test_wildcard_action_matching() {
    let mut policy = AbacPolicy::new();
    policy.add_statement(PolicyStatement::new(
        Effect::Allow,
        vec!["s3:Get*".to_string()],
        vec!["bucket/*".to_string()],
    ));

    let evaluator = PolicyEvaluator::new(policy);

    // s3:GetObject - matches s3:Get*
    let get_context = RequestContext::new(
        "s3:GetObject".to_string(),
        "bucket/key".to_string(),
        "user1".to_string(),
    );
    assert_eq!(evaluator.evaluate(&get_context), AccessDecision::Allow);

    // s3:GetObjectAcl - matches s3:Get*
    let get_acl_context = RequestContext::new(
        "s3:GetObjectAcl".to_string(),
        "bucket/key".to_string(),
        "user1".to_string(),
    );
    assert_eq!(evaluator.evaluate(&get_acl_context), AccessDecision::Allow);

    // s3:PutObject - does not match s3:Get*
    let put_context = RequestContext::new(
        "s3:PutObject".to_string(),
        "bucket/key".to_string(),
        "user1".to_string(),
    );
    assert_eq!(evaluator.evaluate(&put_context), AccessDecision::NoMatch);
}

#[test]
fn test_wildcard_resource_matching() {
    let mut policy = AbacPolicy::new();
    policy.add_statement(PolicyStatement::new(
        Effect::Allow,
        vec!["s3:GetObject".to_string()],
        vec!["bucket/public/*".to_string()],
    ));

    let evaluator = PolicyEvaluator::new(policy);

    // Matches bucket/public/*
    let public_context = RequestContext::new(
        "s3:GetObject".to_string(),
        "bucket/public/file.txt".to_string(),
        "user1".to_string(),
    );
    assert_eq!(evaluator.evaluate(&public_context), AccessDecision::Allow);

    // Does not match bucket/public/*
    let private_context = RequestContext::new(
        "s3:GetObject".to_string(),
        "bucket/private/file.txt".to_string(),
        "user1".to_string(),
    );
    assert_eq!(
        evaluator.evaluate(&private_context),
        AccessDecision::NoMatch
    );
}

#[test]
fn test_combined_conditions() {
    // Policy: Allow GET operations during business hours from internal IPs
    let condition = PolicyCondition::new()
        .with_ip_whitelist(vec!["192.168.0.0/16".to_string()])
        .with_days_of_week(vec![1, 2, 3, 4, 5])
        .with_hours_of_day((9..17).collect());

    let mut policy = AbacPolicy::new();
    policy.add_statement(
        PolicyStatement::new(
            Effect::Allow,
            vec!["s3:GetObject".to_string()],
            vec!["data/*".to_string()],
        )
        .with_condition(condition),
    );

    let evaluator = PolicyEvaluator::new(policy);

    // Valid: Monday, 10 AM, internal IP
    let valid_context = RequestContext::new(
        "s3:GetObject".to_string(),
        "data/file.csv".to_string(),
        "user1".to_string(),
    )
    .with_time(
        Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0)
            .single()
            .expect("Failed to create Monday 10 AM datetime for combined conditions test"),
    )
    .with_source_ip(
        "192.168.1.100"
            .parse()
            .expect("Failed to parse internal IP address for combined conditions test"),
    );

    assert_eq!(evaluator.evaluate(&valid_context), AccessDecision::Allow);

    // Invalid: Monday, 10 AM, external IP
    let external_ip_context = RequestContext::new(
        "s3:GetObject".to_string(),
        "data/file.csv".to_string(),
        "user1".to_string(),
    )
    .with_time(
        Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0)
            .single()
            .expect("Failed to create Monday 10 AM datetime for external IP test"),
    )
    .with_source_ip(
        "8.8.8.8"
            .parse()
            .expect("Failed to parse external IP address for combined conditions test"),
    );

    assert_eq!(
        evaluator.evaluate(&external_ip_context),
        AccessDecision::NoMatch
    );

    // Invalid: Saturday, 10 AM, internal IP
    let weekend_context = RequestContext::new(
        "s3:GetObject".to_string(),
        "data/file.csv".to_string(),
        "user1".to_string(),
    )
    .with_time(
        Utc.with_ymd_and_hms(2024, 1, 6, 10, 0, 0)
            .single()
            .expect("Failed to create Saturday 10 AM datetime for weekend test"),
    )
    .with_source_ip(
        "192.168.1.100"
            .parse()
            .expect("Failed to parse internal IP address for weekend test"),
    );

    assert_eq!(
        evaluator.evaluate(&weekend_context),
        AccessDecision::NoMatch
    );
}

#[test]
fn test_multiple_statements_with_priorities() {
    let mut policy = AbacPolicy::new();

    // Statement 1: Allow all operations on bucket/*
    policy.add_statement(PolicyStatement::new(
        Effect::Allow,
        vec!["s3:*".to_string()],
        vec!["bucket/*".to_string()],
    ));

    // Statement 2: Deny delete operations
    policy.add_statement(PolicyStatement::new(
        Effect::Deny,
        vec!["s3:DeleteObject".to_string()],
        vec!["bucket/*".to_string()],
    ));

    let evaluator = PolicyEvaluator::new(policy);

    // GET is allowed
    let get_context = RequestContext::new(
        "s3:GetObject".to_string(),
        "bucket/file".to_string(),
        "user1".to_string(),
    );
    assert_eq!(evaluator.evaluate(&get_context), AccessDecision::Allow);

    // DELETE is denied (deny overrides allow)
    let delete_context = RequestContext::new(
        "s3:DeleteObject".to_string(),
        "bucket/file".to_string(),
        "user1".to_string(),
    );
    assert_eq!(evaluator.evaluate(&delete_context), AccessDecision::Deny);
}

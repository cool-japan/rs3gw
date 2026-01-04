//! Attribute-Based Access Control (ABAC) for rs3gw
//!
//! Provides fine-grained access control based on:
//! - Time windows
//! - IP addresses/CIDR ranges
//! - Resource attributes (tags, prefixes, storage class)
//! - User attributes
//! - Request context

pub mod evaluator;
pub mod ip_filter;
pub mod policy;
pub mod time_window;

pub use evaluator::{PolicyEvaluator, RequestContext};
pub use ip_filter::{IpFilter, IpRange};
pub use policy::{AbacPolicy, AccessDecision, Effect, PolicyCondition, PolicyStatement};
pub use time_window::{DayOfWeek, TimeWindow};

#[cfg(test)]
mod tests;

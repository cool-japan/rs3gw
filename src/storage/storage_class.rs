//! Storage class transitions and tiering
//!
//! Provides storage class management and transitions between:
//! - STANDARD
//! - STANDARD_IA (Infrequent Access)
//! - INTELLIGENT_TIERING
//! - GLACIER
//! - GLACIER_IR (Instant Retrieval)
//! - DEEP_ARCHIVE

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Storage class error types
#[derive(Debug, Error)]
pub enum StorageClassError {
    #[error("Invalid storage class: {0}")]
    InvalidStorageClass(String),
    #[error("Invalid transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },
    #[error("Object not eligible for transition")]
    NotEligible,
}

/// S3 Storage classes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, Default)]
pub enum StorageClass {
    /// Standard storage (default)
    #[default]
    Standard,
    /// Reduced redundancy (deprecated, treated as Standard)
    ReducedRedundancy,
    /// Standard Infrequent Access
    StandardIa,
    /// One Zone Infrequent Access
    OnezoneIa,
    /// Intelligent Tiering (automatic transitions)
    IntelligentTiering,
    /// Glacier Flexible Retrieval
    Glacier,
    /// Glacier Instant Retrieval
    GlacierIr,
    /// Glacier Deep Archive
    DeepArchive,
}

impl FromStr for StorageClass {
    type Err = StorageClassError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "STANDARD" => Ok(Self::Standard),
            "REDUCED_REDUNDANCY" => Ok(Self::ReducedRedundancy),
            "STANDARD_IA" => Ok(Self::StandardIa),
            "ONEZONE_IA" => Ok(Self::OnezoneIa),
            "INTELLIGENT_TIERING" => Ok(Self::IntelligentTiering),
            "GLACIER" => Ok(Self::Glacier),
            "GLACIER_IR" => Ok(Self::GlacierIr),
            "DEEP_ARCHIVE" => Ok(Self::DeepArchive),
            _ => Err(StorageClassError::InvalidStorageClass(s.to_string())),
        }
    }
}

impl StorageClass {
    /// Get S3 API string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Standard => "STANDARD",
            Self::ReducedRedundancy => "REDUCED_REDUNDANCY",
            Self::StandardIa => "STANDARD_IA",
            Self::OnezoneIa => "ONEZONE_IA",
            Self::IntelligentTiering => "INTELLIGENT_TIERING",
            Self::Glacier => "GLACIER",
            Self::GlacierIr => "GLACIER_IR",
            Self::DeepArchive => "DEEP_ARCHIVE",
        }
    }

    /// Check if transition to another class is valid
    pub fn can_transition_to(&self, to: StorageClass) -> bool {
        use StorageClass::*;
        matches!(
            (self, to),
            // Standard can go anywhere
            (Standard, StandardIa)
                | (Standard, OnezoneIa)
                | (Standard, IntelligentTiering)
                | (Standard, GlacierIr)
                | (Standard, Glacier)
                | (Standard, DeepArchive)
                // IA classes can go colder
                | (StandardIa, IntelligentTiering)
                | (StandardIa, GlacierIr)
                | (StandardIa, Glacier)
                | (StandardIa, DeepArchive)
                | (OnezoneIa, Glacier)
                | (OnezoneIa, DeepArchive)
                // Intelligent can go colder
                | (IntelligentTiering, Glacier)
                | (IntelligentTiering, DeepArchive)
                // Glacier IR to Glacier
                | (GlacierIr, Glacier)
                | (GlacierIr, DeepArchive)
                // Glacier to Deep Archive
                | (Glacier, DeepArchive)
                // Same class (no-op)
                | (Standard, Standard)
                | (StandardIa, StandardIa)
                | (OnezoneIa, OnezoneIa)
                | (IntelligentTiering, IntelligentTiering)
                | (Glacier, Glacier)
                | (GlacierIr, GlacierIr)
                | (DeepArchive, DeepArchive)
        )
    }

    /// Get relative cost factor (STANDARD = 1.0)
    pub fn cost_factor(&self) -> f64 {
        match self {
            Self::Standard | Self::ReducedRedundancy => 1.0,
            Self::StandardIa => 0.5,
            Self::OnezoneIa => 0.4,
            Self::IntelligentTiering => 0.6, // Variable
            Self::GlacierIr => 0.25,
            Self::Glacier => 0.1,
            Self::DeepArchive => 0.05,
        }
    }

    /// Check if this is a cold storage class
    pub fn is_cold_storage(&self) -> bool {
        matches!(self, Self::Glacier | Self::GlacierIr | Self::DeepArchive)
    }
}

impl fmt::Display for StorageClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Storage class transition action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransitionAction {
    /// Target storage class
    pub storage_class: StorageClass,
    /// Days after object creation to transition
    pub days: u32,
    /// Date to transition (alternative to days)
    pub date: Option<DateTime<Utc>>,
}

/// Noncurrent version transition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NoncurrentVersionTransition {
    /// Target storage class
    pub storage_class: StorageClass,
    /// Days after becoming noncurrent
    pub noncurrent_days: u32,
}

/// Storage class analysis for Intelligent Tiering (per-object)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectTieringAnalysis {
    /// Object key
    pub key: String,
    /// Current storage class
    pub current_class: StorageClass,
    /// Recommended storage class
    pub recommended_class: StorageClass,
    /// Days since last access
    pub days_since_access: u32,
    /// Access pattern (frequency)
    pub access_frequency: AccessFrequency,
    /// Potential cost savings
    pub potential_savings_pct: f64,
}

/// Access frequency pattern
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum AccessFrequency {
    /// Accessed frequently (< 30 days)
    Frequent,
    /// Accessed infrequently (30-90 days)
    Infrequent,
    /// Rarely accessed (90-180 days)
    Rare,
    /// Archive (>180 days)
    Archive,
}

impl AccessFrequency {
    /// Determine from days since last access
    pub fn from_days(days: u32) -> Self {
        match days {
            0..=30 => Self::Frequent,
            31..=90 => Self::Infrequent,
            91..=180 => Self::Rare,
            _ => Self::Archive,
        }
    }

    /// Recommend storage class based on access pattern
    pub fn recommend_storage_class(&self) -> StorageClass {
        match self {
            Self::Frequent => StorageClass::Standard,
            Self::Infrequent => StorageClass::StandardIa,
            Self::Rare => StorageClass::GlacierIr,
            Self::Archive => StorageClass::Glacier,
        }
    }
}

/// Storage class manager
#[derive(Debug)]
pub struct StorageClassManager {
    /// Object storage classes (bucket -> key -> class)
    storage_classes: HashMap<String, HashMap<String, StorageClass>>,
    /// Last access times (bucket -> key -> timestamp)
    last_access: HashMap<String, HashMap<String, DateTime<Utc>>>,
}

impl StorageClassManager {
    /// Create a new storage class manager
    pub fn new() -> Self {
        Self {
            storage_classes: HashMap::new(),
            last_access: HashMap::new(),
        }
    }

    /// Get storage class for an object
    pub fn get_storage_class(&self, bucket: &str, key: &str) -> StorageClass {
        self.storage_classes
            .get(bucket)
            .and_then(|bucket_map| bucket_map.get(key))
            .copied()
            .unwrap_or_default()
    }

    /// Set storage class for an object
    pub fn set_storage_class(&mut self, bucket: &str, key: &str, class: StorageClass) {
        self.storage_classes
            .entry(bucket.to_string())
            .or_default()
            .insert(key.to_string(), class);
    }

    /// Transition object to a new storage class
    pub fn transition_object(
        &mut self,
        bucket: &str,
        key: &str,
        to_class: StorageClass,
    ) -> Result<(), StorageClassError> {
        let current_class = self.get_storage_class(bucket, key);

        if !current_class.can_transition_to(to_class) {
            return Err(StorageClassError::InvalidTransition {
                from: current_class.to_string(),
                to: to_class.to_string(),
            });
        }

        self.set_storage_class(bucket, key, to_class);
        Ok(())
    }

    /// Record object access
    pub fn record_access(&mut self, bucket: &str, key: &str) {
        self.last_access
            .entry(bucket.to_string())
            .or_default()
            .insert(key.to_string(), Utc::now());
    }

    /// Get days since last access
    pub fn days_since_access(&self, bucket: &str, key: &str) -> Option<u32> {
        self.last_access
            .get(bucket)
            .and_then(|bucket_map| bucket_map.get(key))
            .map(|last_access| (Utc::now() - *last_access).num_days() as u32)
    }

    /// Analyze object for storage class optimization
    pub fn analyze_object(
        &self,
        bucket: &str,
        key: &str,
        object_age_days: u32,
    ) -> ObjectTieringAnalysis {
        let current_class = self.get_storage_class(bucket, key);
        let days_since_access = self
            .days_since_access(bucket, key)
            .unwrap_or(object_age_days);
        let access_frequency = AccessFrequency::from_days(days_since_access);
        let recommended_class = access_frequency.recommend_storage_class();

        let current_cost = current_class.cost_factor();
        let recommended_cost = recommended_class.cost_factor();
        let potential_savings_pct =
            ((current_cost - recommended_cost) / current_cost * 100.0).max(0.0);

        ObjectTieringAnalysis {
            key: key.to_string(),
            current_class,
            recommended_class,
            days_since_access,
            access_frequency,
            potential_savings_pct,
        }
    }

    /// Process transitions based on lifecycle rules
    pub fn process_transitions(
        &mut self,
        bucket: &str,
        transitions: &[TransitionAction],
        object_age_days: u32,
        key: &str,
    ) -> Result<Option<StorageClass>, StorageClassError> {
        let current_class = self.get_storage_class(bucket, key);

        // Find the most recent applicable transition (latest days that object has reached)
        let mut best_transition: Option<&TransitionAction> = None;

        for transition in transitions {
            if object_age_days >= transition.days
                && current_class.can_transition_to(transition.storage_class)
            {
                // Keep the transition with the most days (most recent/coldest)
                let should_update = match best_transition {
                    None => true,
                    Some(best) => transition.days > best.days,
                };
                if should_update {
                    best_transition = Some(transition);
                }
            }
        }

        if let Some(transition) = best_transition {
            self.set_storage_class(bucket, key, transition.storage_class);
            Ok(Some(transition.storage_class))
        } else {
            Ok(None)
        }
    }

    /// Get all objects in a specific storage class for a bucket
    pub fn get_objects_by_class(&self, bucket: &str, class: StorageClass) -> Vec<String> {
        self.storage_classes
            .get(bucket)
            .map(|bucket_map| {
                bucket_map
                    .iter()
                    .filter(|&(_, &obj_class)| obj_class == class)
                    .map(|(key, _)| key.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get storage class distribution for a bucket
    pub fn get_class_distribution(&self, bucket: &str) -> HashMap<StorageClass, usize> {
        let mut distribution = HashMap::new();

        if let Some(bucket_map) = self.storage_classes.get(bucket) {
            for class in bucket_map.values() {
                *distribution.entry(*class).or_insert(0) += 1;
            }
        }

        distribution
    }
}

impl Default for StorageClassManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_class_parsing() {
        assert_eq!(
            StorageClass::from_str("STANDARD").expect("Failed to parse STANDARD"),
            StorageClass::Standard
        );
        assert_eq!(
            StorageClass::from_str("GLACIER").expect("Failed to parse GLACIER"),
            StorageClass::Glacier
        );
        assert_eq!(
            StorageClass::from_str("INTELLIGENT_TIERING")
                .expect("Failed to parse INTELLIGENT_TIERING"),
            StorageClass::IntelligentTiering
        );
        assert!(StorageClass::from_str("INVALID").is_err());
    }

    #[test]
    fn test_storage_class_transitions() {
        assert!(StorageClass::Standard.can_transition_to(StorageClass::StandardIa));
        assert!(StorageClass::StandardIa.can_transition_to(StorageClass::Glacier));
        assert!(!StorageClass::Glacier.can_transition_to(StorageClass::Standard)); // Can't go warmer
        assert!(!StorageClass::DeepArchive.can_transition_to(StorageClass::Glacier));
        // Can't go warmer
    }

    #[test]
    fn test_storage_class_manager() {
        let mut manager = StorageClassManager::new();

        // Default is STANDARD
        assert_eq!(
            manager.get_storage_class("bucket1", "key1"),
            StorageClass::Standard
        );

        // Set a class
        manager.set_storage_class("bucket1", "key1", StorageClass::StandardIa);
        assert_eq!(
            manager.get_storage_class("bucket1", "key1"),
            StorageClass::StandardIa
        );

        // Transition
        manager
            .transition_object("bucket1", "key1", StorageClass::Glacier)
            .expect("Failed to transition object");
        assert_eq!(
            manager.get_storage_class("bucket1", "key1"),
            StorageClass::Glacier
        );
    }

    #[test]
    fn test_invalid_transition() {
        let mut manager = StorageClassManager::new();
        manager.set_storage_class("bucket1", "key1", StorageClass::Glacier);

        // Try to transition to warmer storage (not allowed)
        let result = manager.transition_object("bucket1", "key1", StorageClass::Standard);
        assert!(result.is_err());
    }

    #[test]
    fn test_access_tracking() {
        let mut manager = StorageClassManager::new();

        manager.record_access("bucket1", "key1");
        let days = manager.days_since_access("bucket1", "key1");
        assert_eq!(days, Some(0)); // Just accessed
    }

    #[test]
    fn test_storage_class_analysis() {
        let mut manager = StorageClassManager::new();
        manager.set_storage_class("bucket1", "key1", StorageClass::Standard);

        // Simulate old object that hasn't been accessed
        let analysis = manager.analyze_object("bucket1", "key1", 200);

        assert_eq!(analysis.current_class, StorageClass::Standard);
        assert_eq!(analysis.recommended_class, StorageClass::Glacier);
        assert!(analysis.potential_savings_pct > 0.0);
    }

    #[test]
    fn test_access_frequency() {
        assert_eq!(AccessFrequency::from_days(10), AccessFrequency::Frequent);
        assert_eq!(AccessFrequency::from_days(60), AccessFrequency::Infrequent);
        assert_eq!(AccessFrequency::from_days(120), AccessFrequency::Rare);
        assert_eq!(AccessFrequency::from_days(200), AccessFrequency::Archive);
    }

    #[test]
    fn test_lifecycle_transitions() {
        let mut manager = StorageClassManager::new();
        manager.set_storage_class("bucket1", "key1", StorageClass::Standard);

        let transitions = vec![
            TransitionAction {
                storage_class: StorageClass::StandardIa,
                days: 30,
                date: None,
            },
            TransitionAction {
                storage_class: StorageClass::Glacier,
                days: 90,
                date: None,
            },
        ];

        // Object is 45 days old - should transition to IA
        let result = manager
            .process_transitions("bucket1", &transitions, 45, "key1")
            .expect("Failed to process transitions");
        assert_eq!(result, Some(StorageClass::StandardIa));
        assert_eq!(
            manager.get_storage_class("bucket1", "key1"),
            StorageClass::StandardIa
        );

        // Object is now 95 days old - should transition to Glacier
        let result = manager
            .process_transitions("bucket1", &transitions, 95, "key1")
            .expect("Failed to process transitions");
        assert_eq!(result, Some(StorageClass::Glacier));
        assert_eq!(
            manager.get_storage_class("bucket1", "key1"),
            StorageClass::Glacier
        );
    }

    #[test]
    fn test_class_distribution() {
        let mut manager = StorageClassManager::new();
        manager.set_storage_class("bucket1", "key1", StorageClass::Standard);
        manager.set_storage_class("bucket1", "key2", StorageClass::Standard);
        manager.set_storage_class("bucket1", "key3", StorageClass::Glacier);

        let distribution = manager.get_class_distribution("bucket1");
        assert_eq!(distribution.get(&StorageClass::Standard), Some(&2));
        assert_eq!(distribution.get(&StorageClass::Glacier), Some(&1));
    }
}

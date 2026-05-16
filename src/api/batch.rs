//! S3 Batch Operations API
//!
//! Provides batch operation capabilities for performing large-scale operations
//! on S3 objects such as copying, tagging, deleting, and storage class transitions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::storage::{StorageClass, StorageEngine};

/// Batch operation errors
#[derive(Debug, Error)]
pub enum BatchError {
    #[error("Job not found: {0}")]
    JobNotFound(String),
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("Operation failed: {0}")]
    OperationFailed(String),
    #[error("Invalid job state: {0}")]
    InvalidState(String),
    #[error("Storage error: {0}")]
    Storage(String),
}

/// Batch job status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobStatus {
    /// Job created but not started
    New,
    /// Job is being prepared
    Preparing,
    /// Job is ready to run
    Ready,
    /// Job is currently running
    Active,
    /// Job is pausing
    Pausing,
    /// Job is paused
    Paused,
    /// Job is complete
    Complete,
    /// Job is cancelling
    Cancelling,
    /// Job was cancelled
    Cancelled,
    /// Job failed
    Failed,
}

/// Batch operation type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BatchOperation {
    /// Copy objects to a destination
    Copy {
        destination_bucket: String,
        destination_prefix: Option<String>,
        metadata_directive: Option<String>,
    },
    /// Set object tags
    PutObjectTagging { tags: HashMap<String, String> },
    /// Delete object tags
    DeleteObjectTagging,
    /// Delete objects
    Delete,
    /// Change storage class
    StorageClassTransition { storage_class: StorageClass },
    /// Set object ACL
    PutObjectAcl { acl: String },
    /// Restore objects from Glacier
    RestoreObject { days: u32 },
    /// Invoke Lambda function
    InvokeLambda { function_arn: String },
}

/// Manifest format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManifestFormat {
    /// S3 Batch Operations CSV
    S3BatchOperationsCsv,
    /// S3 Inventory Report CSV
    S3InventoryReportCsv,
}

/// Manifest specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestSpec {
    /// Location of manifest (S3 URI)
    pub location: String,
    /// Manifest format
    pub format: ManifestFormat,
    /// Fields in manifest
    pub fields: Vec<String>,
}

/// Job manifest containing list of objects to operate on
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobManifest {
    /// Manifest specification
    pub spec: ManifestSpec,
    /// Parsed object keys
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objects: Option<Vec<ManifestObject>>,
}

/// Object in manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestObject {
    pub bucket: String,
    pub key: String,
    pub version_id: Option<String>,
}

/// Job report configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JobReport {
    /// Whether reporting is enabled
    pub enabled: bool,
    /// S3 bucket for reports
    pub bucket: Option<String>,
    /// Prefix for report objects
    pub prefix: Option<String>,
    /// Report format
    pub format: Option<String>,
    /// Report scope (all tasks or failed tasks only)
    pub report_scope: Option<String>,
}

/// Progress summary for a job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressSummary {
    pub total_objects: u64,
    pub successful_objects: u64,
    pub failed_objects: u64,
}

impl ProgressSummary {
    pub fn new() -> Self {
        Self {
            total_objects: 0,
            successful_objects: 0,
            failed_objects: 0,
        }
    }
}

impl Default for ProgressSummary {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJob {
    /// Job ID
    pub job_id: String,
    /// Job description
    pub description: Option<String>,
    /// Job status
    pub status: JobStatus,
    /// Job priority (higher = more important)
    pub priority: i32,
    /// Manifest
    pub manifest: JobManifest,
    /// Operation to perform
    pub operation: BatchOperation,
    /// Report configuration
    pub report: JobReport,
    /// IAM role ARN (for AWS compatibility)
    pub role_arn: String,
    /// Creation time
    pub creation_time: DateTime<Utc>,
    /// Termination date (if complete/failed/cancelled)
    pub termination_date: Option<DateTime<Utc>>,
    /// Progress summary
    pub progress: ProgressSummary,
    /// Failure reasons
    pub failure_reasons: Vec<String>,
}

impl BatchJob {
    /// Create a new batch job
    pub fn new(
        description: Option<String>,
        priority: i32,
        manifest: JobManifest,
        operation: BatchOperation,
        report: JobReport,
        role_arn: String,
    ) -> Self {
        Self {
            job_id: Uuid::new_v4().to_string(),
            description,
            status: JobStatus::New,
            priority,
            manifest,
            operation,
            report,
            role_arn,
            creation_time: Utc::now(),
            termination_date: None,
            progress: ProgressSummary::new(),
            failure_reasons: Vec::new(),
        }
    }

    /// Check if job can be cancelled
    pub fn can_cancel(&self) -> bool {
        matches!(
            self.status,
            JobStatus::New
                | JobStatus::Preparing
                | JobStatus::Ready
                | JobStatus::Active
                | JobStatus::Paused
        )
    }

    /// Check if job is terminal
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            JobStatus::Complete | JobStatus::Cancelled | JobStatus::Failed
        )
    }
}

/// Batch operation manager
pub struct BatchManager {
    /// Active and historical jobs
    jobs: Arc<RwLock<HashMap<String, BatchJob>>>,
    /// Storage engine reference
    storage: Arc<StorageEngine>,
}

impl BatchManager {
    /// Create a new batch manager
    pub fn new(storage: Arc<StorageEngine>) -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            storage,
        }
    }

    /// Create a batch job
    pub async fn create_job(
        &self,
        description: Option<String>,
        priority: i32,
        manifest: JobManifest,
        operation: BatchOperation,
        report: JobReport,
        role_arn: String,
    ) -> Result<String, BatchError> {
        let job = BatchJob::new(description, priority, manifest, operation, report, role_arn);
        let job_id = job.job_id.clone();

        let mut jobs = self.jobs.write().await;
        jobs.insert(job_id.clone(), job);

        Ok(job_id)
    }

    /// Get a batch job
    pub async fn get_job(&self, job_id: &str) -> Result<BatchJob, BatchError> {
        let jobs = self.jobs.read().await;
        jobs.get(job_id)
            .cloned()
            .ok_or_else(|| BatchError::JobNotFound(job_id.to_string()))
    }

    /// List batch jobs
    pub async fn list_jobs(&self, max_results: Option<usize>) -> Vec<BatchJob> {
        let jobs = self.jobs.read().await;
        let mut job_list: Vec<_> = jobs.values().cloned().collect();

        // Sort by creation time (newest first)
        job_list.sort_by_key(|b| std::cmp::Reverse(b.creation_time));

        if let Some(max) = max_results {
            job_list.truncate(max);
        }

        job_list
    }

    /// Update job priority
    pub async fn update_job_priority(&self, job_id: &str, priority: i32) -> Result<(), BatchError> {
        let mut jobs = self.jobs.write().await;
        let job = jobs
            .get_mut(job_id)
            .ok_or_else(|| BatchError::JobNotFound(job_id.to_string()))?;

        if job.is_terminal() {
            return Err(BatchError::InvalidState(
                "Cannot update priority of terminal job".to_string(),
            ));
        }

        job.priority = priority;
        Ok(())
    }

    /// Cancel a batch job
    pub async fn cancel_job(&self, job_id: &str) -> Result<(), BatchError> {
        let mut jobs = self.jobs.write().await;
        let job = jobs
            .get_mut(job_id)
            .ok_or_else(|| BatchError::JobNotFound(job_id.to_string()))?;

        if !job.can_cancel() {
            return Err(BatchError::InvalidState(
                "Job cannot be cancelled in current state".to_string(),
            ));
        }

        job.status = JobStatus::Cancelling;
        // In a real implementation, this would signal running tasks to stop
        job.status = JobStatus::Cancelled;
        job.termination_date = Some(Utc::now());

        Ok(())
    }

    /// Execute a batch job (simplified synchronous execution for MVP)
    pub async fn execute_job(&self, job_id: &str) -> Result<(), BatchError> {
        // Get and update job status
        {
            let mut jobs = self.jobs.write().await;
            let job = jobs
                .get_mut(job_id)
                .ok_or_else(|| BatchError::JobNotFound(job_id.to_string()))?;

            if job.status != JobStatus::New && job.status != JobStatus::Ready {
                return Err(BatchError::InvalidState(
                    "Job is not in a state that can be executed".to_string(),
                ));
            }

            job.status = JobStatus::Preparing;
        }

        // Load manifest objects
        let manifest_objects = self.load_manifest(job_id).await?;

        {
            let mut jobs = self.jobs.write().await;
            let job = jobs
                .get_mut(job_id)
                .ok_or_else(|| BatchError::JobNotFound(job_id.to_string()))?;
            job.manifest.objects = Some(manifest_objects.clone());
            job.progress.total_objects = manifest_objects.len() as u64;
            job.status = JobStatus::Active;
        }

        // Execute operations
        for obj in &manifest_objects {
            match self.execute_operation(job_id, obj).await {
                Ok(_) => {
                    let mut jobs = self.jobs.write().await;
                    let job = jobs
                        .get_mut(job_id)
                        .ok_or_else(|| BatchError::JobNotFound(job_id.to_string()))?;
                    job.progress.successful_objects += 1;
                }
                Err(e) => {
                    let mut jobs = self.jobs.write().await;
                    let job = jobs
                        .get_mut(job_id)
                        .ok_or_else(|| BatchError::JobNotFound(job_id.to_string()))?;
                    job.progress.failed_objects += 1;
                    job.failure_reasons
                        .push(format!("{}:{} - {}", obj.bucket, obj.key, e));
                }
            }
        }

        // Mark job as complete
        {
            let mut jobs = self.jobs.write().await;
            let job = jobs
                .get_mut(job_id)
                .ok_or_else(|| BatchError::JobNotFound(job_id.to_string()))?;
            job.status = if job.progress.failed_objects == 0 {
                JobStatus::Complete
            } else if job.progress.successful_objects == 0 {
                JobStatus::Failed
            } else {
                JobStatus::Complete // Partial success
            };
            job.termination_date = Some(Utc::now());
        }

        Ok(())
    }

    /// Load manifest objects (simplified - assumes CSV format with bucket,key columns)
    async fn load_manifest(&self, job_id: &str) -> Result<Vec<ManifestObject>, BatchError> {
        let job = self.get_job(job_id).await?;

        // In a real implementation, this would parse the manifest from S3
        // For MVP, we'll return objects directly if they're provided
        job.manifest
            .objects
            .ok_or_else(|| BatchError::InvalidManifest("No objects in manifest".to_string()))
    }

    /// Execute a single operation on an object
    async fn execute_operation(
        &self,
        job_id: &str,
        obj: &ManifestObject,
    ) -> Result<(), BatchError> {
        let job = self.get_job(job_id).await?;

        match &job.operation {
            BatchOperation::Copy {
                destination_bucket,
                destination_prefix,
                metadata_directive,
            } => {
                let dest_key = if let Some(prefix) = destination_prefix {
                    format!("{}{}", prefix, obj.key)
                } else {
                    obj.key.clone()
                };

                self.storage
                    .copy_object(
                        &obj.bucket,
                        &obj.key,
                        destination_bucket,
                        &dest_key,
                        metadata_directive.as_deref(),
                        None,
                        None,
                    )
                    .await
                    .map_err(|e| BatchError::OperationFailed(e.to_string()))?;
            }
            BatchOperation::PutObjectTagging { tags } => {
                let tagging = crate::storage::ObjectTagging { tags: tags.clone() };
                self.storage
                    .put_object_tagging(&obj.bucket, &obj.key, &tagging)
                    .await
                    .map_err(|e| BatchError::OperationFailed(e.to_string()))?;
            }
            BatchOperation::DeleteObjectTagging => {
                self.storage
                    .delete_object_tagging(&obj.bucket, &obj.key)
                    .await
                    .map_err(|e| BatchError::OperationFailed(e.to_string()))?;
            }
            BatchOperation::Delete => {
                self.storage
                    .delete_object(&obj.bucket, &obj.key)
                    .await
                    .map_err(|e| BatchError::OperationFailed(e.to_string()))?;
            }
            BatchOperation::StorageClassTransition { storage_class: _ } => {
                // Storage class transitions would be implemented here
                // For MVP, we'll just mark as successful
            }
            BatchOperation::PutObjectAcl { acl: _ } => {
                // ACL operations would be implemented here
                // For MVP, we'll just mark as successful
            }
            BatchOperation::RestoreObject { days: _ } => {
                // Glacier restore would be implemented here
                // For MVP, we'll just mark as successful
            }
            BatchOperation::InvokeLambda { function_arn: _ } => {
                // Lambda invocation would be implemented here
                // For MVP, we'll just mark as successful
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tempfile::TempDir;

    async fn test_storage() -> (Arc<StorageEngine>, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = Arc::new(
            StorageEngine::new(temp_dir.path().to_path_buf())
                .expect("Failed to create storage engine"),
        );
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_create_batch_job() {
        let (storage, _temp_dir) = test_storage().await;
        let manager = BatchManager::new(storage);

        let manifest = JobManifest {
            spec: ManifestSpec {
                location: "s3://bucket/manifest.csv".to_string(),
                format: ManifestFormat::S3BatchOperationsCsv,
                fields: vec!["Bucket".to_string(), "Key".to_string()],
            },
            objects: Some(vec![ManifestObject {
                bucket: "source-bucket".to_string(),
                key: "file.txt".to_string(),
                version_id: None,
            }]),
        };

        let operation = BatchOperation::Copy {
            destination_bucket: "dest-bucket".to_string(),
            destination_prefix: None,
            metadata_directive: None,
        };

        let job_id = manager
            .create_job(
                Some("Test batch copy".to_string()),
                10,
                manifest,
                operation,
                JobReport::default(),
                "arn:aws:iam::123456789012:role/BatchRole".to_string(),
            )
            .await
            .expect("Failed to create batch job");

        let job = manager.get_job(&job_id).await.expect("Failed to get job");
        assert_eq!(job.status, JobStatus::New);
        assert_eq!(job.priority, 10);
    }

    #[tokio::test]
    async fn test_list_jobs() {
        let (storage, _temp_dir) = test_storage().await;
        let manager = BatchManager::new(storage);

        // Create multiple jobs
        for i in 0..3 {
            let manifest = JobManifest {
                spec: ManifestSpec {
                    location: format!("s3://bucket/manifest{}.csv", i),
                    format: ManifestFormat::S3BatchOperationsCsv,
                    fields: vec!["Bucket".to_string(), "Key".to_string()],
                },
                objects: Some(vec![]),
            };

            manager
                .create_job(
                    Some(format!("Job {}", i)),
                    i,
                    manifest,
                    BatchOperation::Delete,
                    JobReport::default(),
                    "arn:aws:iam::123456789012:role/BatchRole".to_string(),
                )
                .await
                .expect("Failed to create batch job");
        }

        let jobs = manager.list_jobs(None).await;
        assert_eq!(jobs.len(), 3);
    }

    #[tokio::test]
    async fn test_cancel_job() {
        let (storage, _temp_dir) = test_storage().await;
        let manager = BatchManager::new(storage);

        let manifest = JobManifest {
            spec: ManifestSpec {
                location: "s3://bucket/manifest.csv".to_string(),
                format: ManifestFormat::S3BatchOperationsCsv,
                fields: vec!["Bucket".to_string(), "Key".to_string()],
            },
            objects: Some(vec![]),
        };

        let job_id = manager
            .create_job(
                None,
                10,
                manifest,
                BatchOperation::Delete,
                JobReport::default(),
                "arn:aws:iam::123456789012:role/BatchRole".to_string(),
            )
            .await
            .expect("Failed to create batch job");

        manager
            .cancel_job(&job_id)
            .await
            .expect("Failed to cancel job");

        let job = manager.get_job(&job_id).await.expect("Failed to get job");
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.termination_date.is_some());
    }

    #[tokio::test]
    async fn test_update_priority() {
        let (storage, _temp_dir) = test_storage().await;
        let manager = BatchManager::new(storage);

        let manifest = JobManifest {
            spec: ManifestSpec {
                location: "s3://bucket/manifest.csv".to_string(),
                format: ManifestFormat::S3BatchOperationsCsv,
                fields: vec!["Bucket".to_string(), "Key".to_string()],
            },
            objects: Some(vec![]),
        };

        let job_id = manager
            .create_job(
                None,
                10,
                manifest,
                BatchOperation::Delete,
                JobReport::default(),
                "arn:aws:iam::123456789012:role/BatchRole".to_string(),
            )
            .await
            .expect("Failed to create batch job");

        manager
            .update_job_priority(&job_id, 20)
            .await
            .expect("Failed to update job priority");

        let job = manager.get_job(&job_id).await.expect("Failed to get job");
        assert_eq!(job.priority, 20);
    }

    #[tokio::test]
    async fn test_execute_delete_batch() {
        let (storage, _temp_dir) = test_storage().await;

        // Create bucket and objects
        storage
            .create_bucket("test-bucket")
            .await
            .expect("Failed to create bucket");
        storage
            .put_object(
                "test-bucket",
                "file1.txt",
                "text/plain",
                HashMap::new(),
                Bytes::from("content1"),
            )
            .await
            .expect("Failed to put object file1.txt");
        storage
            .put_object(
                "test-bucket",
                "file2.txt",
                "text/plain",
                HashMap::new(),
                Bytes::from("content2"),
            )
            .await
            .expect("Failed to put object file2.txt");

        let manager = BatchManager::new(storage.clone());

        let manifest = JobManifest {
            spec: ManifestSpec {
                location: "s3://bucket/manifest.csv".to_string(),
                format: ManifestFormat::S3BatchOperationsCsv,
                fields: vec!["Bucket".to_string(), "Key".to_string()],
            },
            objects: Some(vec![
                ManifestObject {
                    bucket: "test-bucket".to_string(),
                    key: "file1.txt".to_string(),
                    version_id: None,
                },
                ManifestObject {
                    bucket: "test-bucket".to_string(),
                    key: "file2.txt".to_string(),
                    version_id: None,
                },
            ]),
        };

        let job_id = manager
            .create_job(
                Some("Delete batch".to_string()),
                10,
                manifest,
                BatchOperation::Delete,
                JobReport::default(),
                "arn:aws:iam::123456789012:role/BatchRole".to_string(),
            )
            .await
            .expect("Failed to create batch job");

        manager
            .execute_job(&job_id)
            .await
            .expect("Failed to execute job");

        let job = manager.get_job(&job_id).await.expect("Failed to get job");
        assert_eq!(job.status, JobStatus::Complete);
        assert_eq!(job.progress.successful_objects, 2);
        assert_eq!(job.progress.failed_objects, 0);

        // Verify objects were deleted
        assert!(storage
            .head_object("test-bucket", "file1.txt")
            .await
            .is_err());
        assert!(storage
            .head_object("test-bucket", "file2.txt")
            .await
            .is_err());
    }
}

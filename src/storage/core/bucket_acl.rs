use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

use super::path_utils::sanitize_key_for_fs;
use super::types::{StorageEngine, StorageError};

/// Per-bucket or per-object ACL configuration stored as JSON.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AclConfig {
    pub owner_id: String,
    pub owner_display_name: String,
    pub grants: Vec<AclGrant>,
}

/// A single ACL grant entry.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AclGrant {
    /// "CanonicalUser" | "Group" | "AmazonCustomerByEmail"
    pub grantee_type: String,
    pub grantee_id: Option<String>,
    pub grantee_display_name: Option<String>,
    pub grantee_uri: Option<String>,
    pub grantee_email: Option<String>,
    /// FULL_CONTROL | READ | WRITE | READ_ACP | WRITE_ACP
    pub permission: String,
}

impl AclConfig {
    /// Canned full-control ACL: owner has FULL_CONTROL, no other grants.
    pub fn canned_full_control(owner_id: &str, owner_display_name: &str) -> Self {
        Self {
            owner_id: owner_id.to_string(),
            owner_display_name: owner_display_name.to_string(),
            grants: vec![AclGrant {
                grantee_type: "CanonicalUser".to_string(),
                grantee_id: Some(owner_id.to_string()),
                grantee_display_name: Some(owner_display_name.to_string()),
                grantee_uri: None,
                grantee_email: None,
                permission: "FULL_CONTROL".to_string(),
            }],
        }
    }

    /// Materialize a canned ACL value into an explicit grant list.
    ///
    /// `owner_id` / `owner_display_name` are the bucket/object owner.
    pub fn from_canned(
        canned: &str,
        owner_id: &str,
        owner_display_name: &str,
    ) -> Result<Self, StorageError> {
        let owner_grant = AclGrant {
            grantee_type: "CanonicalUser".to_string(),
            grantee_id: Some(owner_id.to_string()),
            grantee_display_name: Some(owner_display_name.to_string()),
            grantee_uri: None,
            grantee_email: None,
            permission: "FULL_CONTROL".to_string(),
        };

        let all_users_uri = "http://acs.amazonaws.com/groups/global/AllUsers";
        let auth_users_uri = "http://acs.amazonaws.com/groups/global/AuthenticatedUsers";
        let log_delivery_uri = "http://acs.amazonaws.com/groups/s3/LogDelivery";

        let extra_grants: Vec<AclGrant> = match canned {
            "private" => vec![],
            "public-read" => vec![AclGrant {
                grantee_type: "Group".to_string(),
                grantee_id: None,
                grantee_display_name: None,
                grantee_uri: Some(all_users_uri.to_string()),
                grantee_email: None,
                permission: "READ".to_string(),
            }],
            "public-read-write" => vec![
                AclGrant {
                    grantee_type: "Group".to_string(),
                    grantee_id: None,
                    grantee_display_name: None,
                    grantee_uri: Some(all_users_uri.to_string()),
                    grantee_email: None,
                    permission: "READ".to_string(),
                },
                AclGrant {
                    grantee_type: "Group".to_string(),
                    grantee_id: None,
                    grantee_display_name: None,
                    grantee_uri: Some(all_users_uri.to_string()),
                    grantee_email: None,
                    permission: "WRITE".to_string(),
                },
            ],
            "authenticated-read" => vec![AclGrant {
                grantee_type: "Group".to_string(),
                grantee_id: None,
                grantee_display_name: None,
                grantee_uri: Some(auth_users_uri.to_string()),
                grantee_email: None,
                permission: "READ".to_string(),
            }],
            // EC2-only; no AllUsers/AuthUsers grants — effectively private
            "aws-exec-read" => vec![],
            // Object-level: bucket owner gets READ on the object.
            "bucket-owner-read" => vec![AclGrant {
                grantee_type: "CanonicalUser".to_string(),
                grantee_id: Some(owner_id.to_string()),
                grantee_display_name: Some(owner_display_name.to_string()),
                grantee_uri: None,
                grantee_email: None,
                permission: "READ".to_string(),
            }],
            // Object-level: bucket owner gets FULL_CONTROL on the object.
            "bucket-owner-full-control" => vec![AclGrant {
                grantee_type: "CanonicalUser".to_string(),
                grantee_id: Some(owner_id.to_string()),
                grantee_display_name: Some(owner_display_name.to_string()),
                grantee_uri: None,
                grantee_email: None,
                permission: "FULL_CONTROL".to_string(),
            }],
            "log-delivery-write" => vec![AclGrant {
                grantee_type: "Group".to_string(),
                grantee_id: None,
                grantee_display_name: None,
                grantee_uri: Some(log_delivery_uri.to_string()),
                grantee_email: None,
                permission: "WRITE".to_string(),
            }],
            other => {
                return Err(StorageError::Internal(format!(
                    "Unknown canned ACL: {}",
                    other
                )));
            }
        };

        let mut grants = vec![owner_grant];
        grants.extend(extra_grants);
        Ok(Self {
            owner_id: owner_id.to_string(),
            owner_display_name: owner_display_name.to_string(),
            grants,
        })
    }
}

impl StorageEngine {
    fn bucket_acl_path(&self, bucket: &str) -> PathBuf {
        self.get_root_path().join(bucket).join("bucket_acl.json")
    }

    fn object_acl_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("acl")
            .join(format!("{}.json", sanitize_key_for_fs(key)))
    }

    /// Read the ACL configuration for a bucket.
    ///
    /// Returns `StorageError::NotFound` when no ACL has been configured yet.
    pub async fn get_bucket_acl(&self, bucket: &str) -> Result<AclConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_acl_path(bucket);
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "No ACL configured for bucket {}",
                bucket
            )));
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    /// Write the ACL configuration for a bucket.
    pub async fn put_bucket_acl(&self, bucket: &str, cfg: &AclConfig) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(self.bucket_acl_path(bucket), data).await?;
        Ok(())
    }

    /// Read the ACL configuration for an object.
    ///
    /// Returns `StorageError::NotFound` when no ACL has been configured yet.
    pub async fn get_object_acl(&self, bucket: &str, key: &str) -> Result<AclConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.object_acl_path(bucket, key);
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "No ACL configured for object {}/{}",
                bucket, key
            )));
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    /// Write the ACL configuration for an object.
    ///
    /// Creates the `acl/` subdirectory within the bucket directory if it does
    /// not exist yet.
    pub async fn put_object_acl(
        &self,
        bucket: &str,
        key: &str,
        cfg: &AclConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.object_acl_path(bucket, key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(&path, data).await?;
        Ok(())
    }
}

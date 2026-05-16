//! S3 XML Response Structures
//!
//! Defines XML serializable structures for S3 API responses.

pub mod acl;
pub mod object_lock;

pub use acl::{AccessControlList, AccessControlPolicy, Grant, Grantee};
pub use object_lock::{LegalHoldXml, ObjectLockConfigurationXml, RetentionXml};

use chrono::{DateTime, Utc};
use quick_xml::se::to_string as to_xml_string;
use serde::Serialize;

/// Error response for S3 API
#[derive(Debug, Serialize)]
#[serde(rename = "Error")]
pub struct ErrorResponse {
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "Message")]
    pub message: String,
    #[serde(rename = "Resource")]
    pub resource: String,
    #[serde(rename = "RequestId")]
    pub request_id: String,
}

impl ErrorResponse {
    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// Bucket information
#[derive(Debug, Serialize)]
#[serde(rename = "Bucket")]
pub struct BucketInfo {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "CreationDate")]
    pub creation_date: String,
}

/// Owner information
#[derive(Debug, Serialize)]
#[serde(rename = "Owner")]
pub struct Owner {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "DisplayName")]
    pub display_name: String,
}

impl Default for Owner {
    fn default() -> Self {
        Self {
            id: "rs3gw".to_string(),
            display_name: "rs3gw".to_string(),
        }
    }
}

/// Buckets container
#[derive(Debug, Serialize)]
#[serde(rename = "Buckets")]
pub struct Buckets {
    #[serde(rename = "Bucket")]
    pub bucket: Vec<BucketInfo>,
}

/// ListAllMyBucketsResult response
#[derive(Debug, Serialize)]
#[serde(rename = "ListAllMyBucketsResult")]
pub struct ListAllMyBucketsResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Owner")]
    pub owner: Owner,
    #[serde(rename = "Buckets")]
    pub buckets: Buckets,
}

impl ListAllMyBucketsResult {
    pub fn new(bucket_names: Vec<(String, DateTime<Utc>)>) -> Self {
        let buckets: Vec<BucketInfo> = bucket_names
            .into_iter()
            .map(|(name, created)| BucketInfo {
                name,
                creation_date: created.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
            })
            .collect();

        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            owner: Owner::default(),
            buckets: Buckets { bucket: buckets },
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// Object metadata for listing
#[derive(Debug, Serialize)]
#[serde(rename = "Contents")]
pub struct ObjectContents {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
    #[serde(rename = "ETag")]
    pub etag: String,
    #[serde(rename = "Size")]
    pub size: u64,
    #[serde(rename = "StorageClass")]
    pub storage_class: String,
}

/// Common prefix for virtual directories
#[derive(Debug, Serialize)]
#[serde(rename = "CommonPrefixes")]
pub struct CommonPrefix {
    #[serde(rename = "Prefix")]
    pub prefix: String,
}

/// ListObjectsV2 response
#[derive(Debug, Serialize)]
#[serde(rename = "ListBucketResult")]
pub struct ListBucketResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Prefix")]
    pub prefix: String,
    #[serde(rename = "KeyCount")]
    pub key_count: usize,
    #[serde(rename = "MaxKeys")]
    pub max_keys: usize,
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(rename = "Contents", skip_serializing_if = "Vec::is_empty")]
    pub contents: Vec<ObjectContents>,
    #[serde(rename = "CommonPrefixes", skip_serializing_if = "Vec::is_empty")]
    pub common_prefixes: Vec<CommonPrefix>,
    #[serde(rename = "ContinuationToken", skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
    #[serde(
        rename = "NextContinuationToken",
        skip_serializing_if = "Option::is_none"
    )]
    pub next_continuation_token: Option<String>,
    #[serde(rename = "Delimiter", skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    #[serde(rename = "EncodingType", skip_serializing_if = "Option::is_none")]
    pub encoding_type: Option<String>,
}

impl ListBucketResult {
    pub fn new(bucket_name: &str) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            name: bucket_name.to_string(),
            prefix: String::new(),
            key_count: 0,
            max_keys: 1000,
            is_truncated: false,
            contents: Vec::new(),
            common_prefixes: Vec::new(),
            continuation_token: None,
            next_continuation_token: None,
            delimiter: None,
            encoding_type: None,
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// ListObjectsV1 response (original S3 API)
#[derive(Debug, Serialize)]
#[serde(rename = "ListBucketResult")]
pub struct ListBucketResultV1 {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Prefix")]
    pub prefix: String,
    #[serde(rename = "Marker")]
    pub marker: String,
    #[serde(rename = "MaxKeys")]
    pub max_keys: usize,
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(rename = "Contents", skip_serializing_if = "Vec::is_empty")]
    pub contents: Vec<ObjectContents>,
    #[serde(rename = "CommonPrefixes", skip_serializing_if = "Vec::is_empty")]
    pub common_prefixes: Vec<CommonPrefix>,
    #[serde(rename = "NextMarker", skip_serializing_if = "Option::is_none")]
    pub next_marker: Option<String>,
    #[serde(rename = "Delimiter", skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    #[serde(rename = "EncodingType", skip_serializing_if = "Option::is_none")]
    pub encoding_type: Option<String>,
}

impl ListBucketResultV1 {
    pub fn new(bucket_name: &str) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            name: bucket_name.to_string(),
            prefix: String::new(),
            marker: String::new(),
            max_keys: 1000,
            is_truncated: false,
            contents: Vec::new(),
            common_prefixes: Vec::new(),
            next_marker: None,
            delimiter: None,
            encoding_type: None,
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// CopyObjectResult response
#[derive(Debug, Serialize)]
#[serde(rename = "CopyObjectResult")]
pub struct CopyObjectResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "ETag")]
    pub etag: String,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
}

impl CopyObjectResult {
    pub fn new(etag: &str, last_modified: &str) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            etag: etag.to_string(),
            last_modified: last_modified.to_string(),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// CopyPartResult response for UploadPartCopy
#[derive(Debug, Serialize)]
#[serde(rename = "CopyPartResult")]
pub struct CopyPartResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "ETag")]
    pub etag: String,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
}

impl CopyPartResult {
    pub fn new(etag: &str, last_modified: &str) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            etag: etag.to_string(),
            last_modified: last_modified.to_string(),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// InitiateMultipartUploadResult response
#[derive(Debug, Serialize)]
#[serde(rename = "InitiateMultipartUploadResult")]
pub struct InitiateMultipartUploadResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Bucket")]
    pub bucket: String,
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "UploadId")]
    pub upload_id: String,
}

impl InitiateMultipartUploadResult {
    pub fn new(bucket: &str, key: &str, upload_id: &str) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            bucket: bucket.to_string(),
            key: key.to_string(),
            upload_id: upload_id.to_string(),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// Part element for ListPartsResult
#[derive(Debug, Serialize)]
#[serde(rename = "Part")]
pub struct PartElement {
    #[serde(rename = "PartNumber")]
    pub part_number: u32,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
    #[serde(rename = "ETag")]
    pub etag: String,
    #[serde(rename = "Size")]
    pub size: u64,
}

/// ListPartsResult response
#[derive(Debug, Serialize)]
#[serde(rename = "ListPartsResult")]
pub struct ListPartsResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Bucket")]
    pub bucket: String,
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "UploadId")]
    pub upload_id: String,
    #[serde(rename = "Initiator")]
    pub initiator: Owner,
    #[serde(rename = "Owner")]
    pub owner: Owner,
    #[serde(rename = "StorageClass")]
    pub storage_class: String,
    #[serde(rename = "PartNumberMarker")]
    pub part_number_marker: u32,
    #[serde(rename = "NextPartNumberMarker")]
    pub next_part_number_marker: u32,
    #[serde(rename = "MaxParts")]
    pub max_parts: u32,
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(rename = "Part", skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<PartElement>,
}

impl ListPartsResult {
    pub fn new(bucket: &str, key: &str, upload_id: &str) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            bucket: bucket.to_string(),
            key: key.to_string(),
            upload_id: upload_id.to_string(),
            initiator: Owner::default(),
            owner: Owner::default(),
            storage_class: "STANDARD".to_string(),
            part_number_marker: 0,
            next_part_number_marker: 0,
            max_parts: 1000,
            is_truncated: false,
            parts: Vec::new(),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// CompleteMultipartUploadResult response
#[derive(Debug, Serialize)]
#[serde(rename = "CompleteMultipartUploadResult")]
pub struct CompleteMultipartUploadResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Location")]
    pub location: String,
    #[serde(rename = "Bucket")]
    pub bucket: String,
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "ETag")]
    pub etag: String,
}

impl CompleteMultipartUploadResult {
    pub fn new(location: &str, bucket: &str, key: &str, etag: &str) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            location: location.to_string(),
            bucket: bucket.to_string(),
            key: key.to_string(),
            etag: etag.to_string(),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// DeleteResult for batch delete operations
#[derive(Debug, Serialize)]
#[serde(rename = "DeleteResult")]
pub struct DeleteResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Deleted", skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<DeletedObject>,
    #[serde(rename = "Error", skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<DeleteError>,
}

#[derive(Debug, Serialize)]
pub struct DeletedObject {
    #[serde(rename = "Key")]
    pub key: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteError {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "Message")]
    pub message: String,
}

impl DeleteResult {
    pub fn new() -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            deleted: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn add_deleted(&mut self, key: String) {
        self.deleted.push(DeletedObject { key });
    }

    pub fn add_error(&mut self, key: String, code: &str, message: &str) {
        self.errors.push(DeleteError {
            key,
            code: code.to_string(),
            message: message.to_string(),
        });
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

impl Default for DeleteResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Upload element for ListMultipartUploads
#[derive(Debug, Serialize)]
#[serde(rename = "Upload")]
pub struct UploadElement {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "UploadId")]
    pub upload_id: String,
    #[serde(rename = "Initiator")]
    pub initiator: Owner,
    #[serde(rename = "Owner")]
    pub owner: Owner,
    #[serde(rename = "StorageClass")]
    pub storage_class: String,
    #[serde(rename = "Initiated")]
    pub initiated: String,
}

/// ListMultipartUploadsResult response
#[derive(Debug, Serialize)]
#[serde(rename = "ListMultipartUploadsResult")]
pub struct ListMultipartUploadsResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Bucket")]
    pub bucket: String,
    #[serde(rename = "KeyMarker")]
    pub key_marker: String,
    #[serde(rename = "UploadIdMarker")]
    pub upload_id_marker: String,
    #[serde(rename = "NextKeyMarker", skip_serializing_if = "Option::is_none")]
    pub next_key_marker: Option<String>,
    #[serde(rename = "NextUploadIdMarker", skip_serializing_if = "Option::is_none")]
    pub next_upload_id_marker: Option<String>,
    #[serde(rename = "MaxUploads")]
    pub max_uploads: u32,
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(rename = "Upload", skip_serializing_if = "Vec::is_empty")]
    pub uploads: Vec<UploadElement>,
    #[serde(rename = "Prefix", skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(rename = "Delimiter", skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
}

impl ListMultipartUploadsResult {
    pub fn new(bucket: &str) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            bucket: bucket.to_string(),
            key_marker: String::new(),
            upload_id_marker: String::new(),
            next_key_marker: None,
            next_upload_id_marker: None,
            max_uploads: 1000,
            is_truncated: false,
            uploads: Vec::new(),
            prefix: None,
            delimiter: None,
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// Tag element for GetObjectTagging
#[derive(Debug, Serialize)]
#[serde(rename = "Tag")]
pub struct Tag {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Value")]
    pub value: String,
}

/// TagSet container for GetObjectTagging
#[derive(Debug, Serialize)]
#[serde(rename = "TagSet")]
pub struct TagSet {
    #[serde(rename = "Tag", default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
}

/// GetObjectTagging response
#[derive(Debug, Serialize)]
#[serde(rename = "Tagging")]
pub struct TaggingResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "TagSet")]
    pub tag_set: TagSet,
}

impl TaggingResult {
    pub fn new(tags: Vec<(String, String)>) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            tag_set: TagSet {
                tags: tags
                    .into_iter()
                    .map(|(key, value)| Tag { key, value })
                    .collect(),
            },
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// LocationConstraint response for GetBucketLocation
#[derive(Debug, Serialize)]
#[serde(rename = "LocationConstraint")]
pub struct LocationConstraint {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "$text", skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

impl LocationConstraint {
    pub fn new(location: Option<&str>) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            location: location.map(String::from),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// VersioningConfiguration response for GetBucketVersioning
#[derive(Debug, Serialize)]
#[serde(rename = "VersioningConfiguration")]
pub struct VersioningConfiguration {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Status", skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

impl VersioningConfiguration {
    pub fn new(status: Option<&str>) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            status: status.map(String::from),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// GetObjectAttributesResult response
/// Returns object attributes like ETag, size, storage class, and optionally object parts info
#[derive(Debug, Serialize)]
#[serde(rename = "GetObjectAttributesResponse")]
pub struct GetObjectAttributesResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "ETag", skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(rename = "Checksum", skip_serializing_if = "Option::is_none")]
    pub checksum: Option<ObjectChecksum>,
    #[serde(rename = "ObjectParts", skip_serializing_if = "Option::is_none")]
    pub object_parts: Option<ObjectParts>,
    #[serde(rename = "StorageClass", skip_serializing_if = "Option::is_none")]
    pub storage_class: Option<String>,
    #[serde(rename = "ObjectSize", skip_serializing_if = "Option::is_none")]
    pub object_size: Option<u64>,
}

/// Checksum information for an object
#[derive(Debug, Serialize)]
pub struct ObjectChecksum {
    #[serde(rename = "ChecksumSHA256", skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,
}

/// Object parts information (for multipart objects)
#[derive(Debug, Serialize)]
pub struct ObjectParts {
    #[serde(rename = "TotalPartsCount", skip_serializing_if = "Option::is_none")]
    pub total_parts_count: Option<u32>,
}

impl GetObjectAttributesResult {
    pub fn new(etag: &str, size: u64) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            etag: Some(etag.to_string()),
            checksum: None,
            object_parts: None,
            storage_class: Some("STANDARD".to_string()),
            object_size: Some(size),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// ListVersionsResult response (stub - versioning not fully implemented)
/// Returns object versions. Currently returns current objects as the only version.
#[derive(Debug, Serialize)]
#[serde(rename = "ListVersionsResult")]
pub struct ListVersionsResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Prefix")]
    pub prefix: String,
    #[serde(rename = "KeyMarker")]
    pub key_marker: String,
    #[serde(rename = "VersionIdMarker")]
    pub version_id_marker: String,
    #[serde(rename = "Delimiter", skip_serializing_if = "String::is_empty")]
    pub delimiter: String,
    #[serde(rename = "MaxKeys")]
    pub max_keys: u32,
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(rename = "Version", skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<ObjectVersion>,
    #[serde(rename = "DeleteMarker", skip_serializing_if = "Vec::is_empty")]
    pub delete_markers: Vec<DeleteMarkerEntry>,
    #[serde(rename = "CommonPrefixes", skip_serializing_if = "Vec::is_empty")]
    pub common_prefixes: Vec<CommonPrefix>,
}

/// Object version entry
#[derive(Debug, Serialize)]
pub struct ObjectVersion {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "VersionId")]
    pub version_id: String,
    #[serde(rename = "IsLatest")]
    pub is_latest: bool,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
    #[serde(rename = "ETag")]
    pub etag: String,
    #[serde(rename = "Size")]
    pub size: u64,
    #[serde(rename = "StorageClass")]
    pub storage_class: String,
    #[serde(rename = "Owner")]
    pub owner: Owner,
}

/// Delete marker entry
#[derive(Debug, Serialize)]
pub struct DeleteMarkerEntry {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "VersionId")]
    pub version_id: String,
    #[serde(rename = "IsLatest")]
    pub is_latest: bool,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
    #[serde(rename = "Owner")]
    pub owner: Owner,
}

impl ListVersionsResult {
    pub fn new(bucket: &str) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            name: bucket.to_string(),
            prefix: String::new(),
            key_marker: String::new(),
            version_id_marker: String::new(),
            delimiter: String::new(),
            max_keys: 1000,
            is_truncated: false,
            versions: Vec::new(),
            delete_markers: Vec::new(),
            common_prefixes: Vec::new(),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// ServerSideEncryptionConfiguration XML response
#[derive(Debug, Serialize)]
#[serde(rename = "ServerSideEncryptionConfiguration")]
pub struct SseConfigurationXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Rule")]
    pub rules: Vec<SseRuleXml>,
}

/// A single Rule element in ServerSideEncryptionConfiguration
#[derive(Debug, Serialize)]
pub struct SseRuleXml {
    #[serde(rename = "ApplyServerSideEncryptionByDefault")]
    pub apply_default: SseDefaultXml,
    #[serde(rename = "BucketKeyEnabled", skip_serializing_if = "Option::is_none")]
    pub bucket_key_enabled: Option<bool>,
}

/// ApplyServerSideEncryptionByDefault element
#[derive(Debug, Serialize)]
pub struct SseDefaultXml {
    #[serde(rename = "SSEAlgorithm")]
    pub sse_algorithm: String,
    #[serde(rename = "KMSMasterKeyID", skip_serializing_if = "Option::is_none")]
    pub kms_master_key_id: Option<String>,
}

impl SseConfigurationXml {
    /// Build from storage EncryptionConfig
    pub fn from_config(cfg: &crate::storage::EncryptionConfig) -> Self {
        SseConfigurationXml {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            rules: cfg
                .rules
                .iter()
                .map(|r| SseRuleXml {
                    apply_default: SseDefaultXml {
                        sse_algorithm: r.sse_algorithm.clone(),
                        kms_master_key_id: r.kms_master_key_id.clone(),
                    },
                    bucket_key_enabled: if r.bucket_key_enabled {
                        Some(true)
                    } else {
                        None
                    },
                })
                .collect(),
        }
    }

    /// Serialize to XML string with declaration
    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// CORSConfiguration XML response
#[derive(Debug, Serialize)]
#[serde(rename = "CORSConfiguration")]
pub struct CorsConfigurationXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "CORSRule")]
    pub rules: Vec<CorsRuleXml>,
}

/// A single CORSRule element
#[derive(Debug, Serialize)]
pub struct CorsRuleXml {
    #[serde(rename = "ID", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "AllowedOrigin")]
    pub allowed_origins: Vec<String>,
    #[serde(rename = "AllowedMethod")]
    pub allowed_methods: Vec<String>,
    #[serde(
        rename = "AllowedHeader",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub allowed_headers: Vec<String>,
    #[serde(
        rename = "ExposeHeader",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub expose_headers: Vec<String>,
    #[serde(rename = "MaxAgeSeconds", skip_serializing_if = "Option::is_none")]
    pub max_age_seconds: Option<u32>,
}

impl CorsConfigurationXml {
    /// Build from storage CorsConfig
    pub fn from_config(cfg: &crate::storage::CorsConfig) -> Self {
        CorsConfigurationXml {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            rules: cfg
                .rules
                .iter()
                .map(|r| CorsRuleXml {
                    id: r.id.clone(),
                    allowed_origins: r.allowed_origins.clone(),
                    allowed_methods: r.allowed_methods.clone(),
                    allowed_headers: r.allowed_headers.clone(),
                    expose_headers: r.expose_headers.clone(),
                    max_age_seconds: r.max_age_seconds,
                })
                .collect(),
        }
    }

    /// Serialize to XML string with declaration
    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// LifecycleConfiguration XML response
#[derive(Debug, Serialize)]
#[serde(rename = "LifecycleConfiguration")]
pub struct LifecycleConfigurationXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Rule")]
    pub rules: Vec<LifecycleRuleXml>,
}

#[derive(Debug, Serialize)]
pub struct LifecycleRuleXml {
    #[serde(rename = "ID", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "Status")]
    pub status: String,
    #[serde(rename = "Filter", skip_serializing_if = "Option::is_none")]
    pub filter: Option<LifecycleFilterXml>,
    #[serde(rename = "Expiration", skip_serializing_if = "Option::is_none")]
    pub expiration: Option<LifecycleExpirationXml>,
    #[serde(rename = "Transition", skip_serializing_if = "Vec::is_empty", default)]
    pub transitions: Vec<LifecycleTransitionXml>,
    #[serde(
        rename = "AbortIncompleteMultipartUpload",
        skip_serializing_if = "Option::is_none"
    )]
    pub abort_incomplete_mpu: Option<DaysXml>,
    #[serde(
        rename = "NoncurrentVersionExpiration",
        skip_serializing_if = "Option::is_none"
    )]
    pub noncurrent_expiration: Option<DaysXml>,
    #[serde(
        rename = "NoncurrentVersionTransition",
        skip_serializing_if = "Vec::is_empty",
        default
    )]
    pub noncurrent_transitions: Vec<LifecycleTransitionXml>,
}

#[derive(Debug, Serialize)]
pub struct LifecycleFilterXml {
    #[serde(rename = "Prefix")]
    pub prefix: String,
}

#[derive(Debug, Serialize)]
pub struct LifecycleExpirationXml {
    #[serde(rename = "Days")]
    pub days: u64,
}

#[derive(Debug, Serialize)]
pub struct LifecycleTransitionXml {
    #[serde(rename = "Days")]
    pub days: u64,
    #[serde(rename = "StorageClass")]
    pub storage_class: String,
}

#[derive(Debug, Serialize)]
pub struct DaysXml {
    #[serde(rename = "Days")]
    pub days: u64,
}

impl LifecycleConfigurationXml {
    pub fn from_config(cfg: &crate::storage::LifecycleConfig) -> Self {
        LifecycleConfigurationXml {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            rules: cfg
                .rules
                .iter()
                .map(|r| LifecycleRuleXml {
                    id: r.id.clone(),
                    status: r.status.clone(),
                    filter: r
                        .filter_prefix
                        .as_ref()
                        .map(|p| LifecycleFilterXml { prefix: p.clone() }),
                    expiration: r
                        .expiration_days
                        .map(|d| LifecycleExpirationXml { days: d }),
                    transitions: r
                        .transitions
                        .iter()
                        .map(|t| LifecycleTransitionXml {
                            days: t.days,
                            storage_class: t.storage_class.clone(),
                        })
                        .collect(),
                    abort_incomplete_mpu: r.abort_incomplete_mpu_days.map(|d| DaysXml { days: d }),
                    noncurrent_expiration: r
                        .noncurrent_expiration_days
                        .map(|d| DaysXml { days: d }),
                    noncurrent_transitions: r
                        .noncurrent_transitions
                        .iter()
                        .map(|t| LifecycleTransitionXml {
                            days: t.days,
                            storage_class: t.storage_class.clone(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// WebsiteConfiguration XML response
#[derive(Debug, Serialize)]
#[serde(rename = "WebsiteConfiguration")]
pub struct WebsiteConfigurationXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "IndexDocument", skip_serializing_if = "Option::is_none")]
    pub index_document: Option<IndexDocumentXml>,
    #[serde(rename = "ErrorDocument", skip_serializing_if = "Option::is_none")]
    pub error_document: Option<ErrorDocumentXml>,
    #[serde(
        rename = "RedirectAllRequestsTo",
        skip_serializing_if = "Option::is_none"
    )]
    pub redirect_all: Option<RedirectAllXml>,
    #[serde(rename = "RoutingRules", skip_serializing_if = "Option::is_none")]
    pub routing_rules: Option<RoutingRulesXml>,
}

#[derive(Debug, Serialize)]
pub struct IndexDocumentXml {
    #[serde(rename = "Suffix")]
    pub suffix: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorDocumentXml {
    #[serde(rename = "Key")]
    pub key: String,
}

#[derive(Debug, Serialize)]
pub struct RedirectAllXml {
    #[serde(rename = "HostName")]
    pub host_name: String,
    #[serde(rename = "Protocol", skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RoutingRulesXml {
    #[serde(rename = "RoutingRule")]
    pub rules: Vec<RoutingRuleXml>,
}

#[derive(Debug, Serialize)]
pub struct RoutingRuleXml {
    #[serde(rename = "Condition", skip_serializing_if = "Option::is_none")]
    pub condition: Option<RoutingConditionXml>,
    #[serde(rename = "Redirect")]
    pub redirect: RoutingRedirectXml,
}

#[derive(Debug, Serialize)]
pub struct RoutingConditionXml {
    #[serde(rename = "KeyPrefixEquals", skip_serializing_if = "Option::is_none")]
    pub key_prefix_equals: Option<String>,
    #[serde(
        rename = "HttpErrorCodeReturnedEquals",
        skip_serializing_if = "Option::is_none"
    )]
    pub http_error_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RoutingRedirectXml {
    #[serde(rename = "HostName", skip_serializing_if = "Option::is_none")]
    pub host_name: Option<String>,
    #[serde(rename = "Protocol", skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(
        rename = "ReplaceKeyPrefixWith",
        skip_serializing_if = "Option::is_none"
    )]
    pub replace_key_prefix_with: Option<String>,
    #[serde(rename = "ReplaceKeyWith", skip_serializing_if = "Option::is_none")]
    pub replace_key_with: Option<String>,
    #[serde(rename = "HttpRedirectCode", skip_serializing_if = "Option::is_none")]
    pub http_redirect_code: Option<String>,
}

impl WebsiteConfigurationXml {
    pub fn from_config(cfg: &crate::storage::WebsiteConfig) -> Self {
        let routing_rules = if cfg.routing_rules.is_empty() {
            None
        } else {
            Some(RoutingRulesXml {
                rules: cfg
                    .routing_rules
                    .iter()
                    .map(|r| RoutingRuleXml {
                        condition: if r.condition_key_prefix_equals.is_some()
                            || r.condition_http_error_code.is_some()
                        {
                            Some(RoutingConditionXml {
                                key_prefix_equals: r.condition_key_prefix_equals.clone(),
                                http_error_code: r.condition_http_error_code.clone(),
                            })
                        } else {
                            None
                        },
                        redirect: RoutingRedirectXml {
                            host_name: r.redirect_host.clone(),
                            protocol: r.redirect_protocol.clone(),
                            replace_key_prefix_with: r.redirect_replace_key_prefix_with.clone(),
                            replace_key_with: r.redirect_replace_key_with.clone(),
                            http_redirect_code: r.redirect_http_redirect_code.clone(),
                        },
                    })
                    .collect(),
            })
        };
        WebsiteConfigurationXml {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            index_document: cfg
                .index_document
                .as_ref()
                .map(|s| IndexDocumentXml { suffix: s.clone() }),
            error_document: cfg
                .error_document
                .as_ref()
                .map(|s| ErrorDocumentXml { key: s.clone() }),
            redirect_all: cfg
                .redirect_all_requests_to
                .as_ref()
                .map(|r| RedirectAllXml {
                    host_name: r.host_name.clone(),
                    protocol: r.protocol.clone(),
                }),
            routing_rules,
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// PublicAccessBlockConfiguration XML response
#[derive(Debug, Serialize)]
#[serde(rename = "PublicAccessBlockConfiguration")]
pub struct PublicAccessBlockConfigurationXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "BlockPublicAcls")]
    pub block_public_acls: bool,
    #[serde(rename = "IgnorePublicAcls")]
    pub ignore_public_acls: bool,
    #[serde(rename = "BlockPublicPolicy")]
    pub block_public_policy: bool,
    #[serde(rename = "RestrictPublicBuckets")]
    pub restrict_public_buckets: bool,
}

impl PublicAccessBlockConfigurationXml {
    pub fn from_config(cfg: &crate::storage::PublicAccessBlockConfig) -> Self {
        PublicAccessBlockConfigurationXml {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            block_public_acls: cfg.block_public_acls,
            ignore_public_acls: cfg.ignore_public_acls,
            block_public_policy: cfg.block_public_policy,
            restrict_public_buckets: cfg.restrict_public_buckets,
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// OwnershipControls XML response
#[derive(Debug, Serialize)]
#[serde(rename = "OwnershipControls")]
pub struct OwnershipControlsXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Rule")]
    pub rules: Vec<OwnershipRuleXml>,
}

#[derive(Debug, Serialize)]
pub struct OwnershipRuleXml {
    #[serde(rename = "ObjectOwnership")]
    pub object_ownership: String,
}

impl OwnershipControlsXml {
    pub fn from_config(cfg: &crate::storage::OwnershipControlsConfig) -> Self {
        OwnershipControlsXml {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            rules: cfg
                .rules
                .iter()
                .map(|r| OwnershipRuleXml {
                    object_ownership: r.object_ownership.clone(),
                })
                .collect(),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// BucketLoggingStatus XML response
#[derive(Debug, Serialize)]
#[serde(rename = "BucketLoggingStatus")]
pub struct BucketLoggingStatusXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "LoggingEnabled", skip_serializing_if = "Option::is_none")]
    pub logging_enabled: Option<LoggingEnabledXml>,
}

#[derive(Debug, Serialize)]
pub struct LoggingEnabledXml {
    #[serde(rename = "TargetBucket")]
    pub target_bucket: String,
    #[serde(rename = "TargetPrefix")]
    pub target_prefix: String,
}

impl BucketLoggingStatusXml {
    pub fn from_config(cfg: &crate::storage::LoggingConfig) -> Self {
        let logging_enabled = cfg.target_bucket.as_ref().map(|tb| LoggingEnabledXml {
            target_bucket: tb.clone(),
            target_prefix: cfg.target_prefix.clone().unwrap_or_default(),
        });
        BucketLoggingStatusXml {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            logging_enabled,
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// RequestPaymentConfiguration XML response
#[derive(Debug, Serialize)]
#[serde(rename = "RequestPaymentConfiguration")]
pub struct RequestPaymentConfigurationXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Payer")]
    pub payer: String,
}

impl RequestPaymentConfigurationXml {
    pub fn from_config(cfg: &crate::storage::RequestPaymentConfig) -> Self {
        RequestPaymentConfigurationXml {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            payer: cfg.payer.clone(),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{}"#,
            to_xml_string(self).unwrap_or_default()
        )
    }
}

/// NotificationConfiguration XML response
/// Serializes bucket notification config with optional Topic, Queue, and Lambda entries.
#[derive(Debug, Serialize)]
#[serde(rename = "NotificationConfiguration")]
pub struct NotificationConfigXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(
        rename = "TopicConfiguration",
        skip_serializing_if = "Vec::is_empty",
        default
    )]
    pub topic_configurations: Vec<TopicConfigurationXml>,
    #[serde(
        rename = "QueueConfiguration",
        skip_serializing_if = "Vec::is_empty",
        default
    )]
    pub queue_configurations: Vec<QueueConfigurationXml>,
    #[serde(
        rename = "CloudFunctionConfiguration",
        skip_serializing_if = "Vec::is_empty",
        default
    )]
    pub lambda_function_configurations: Vec<LambdaFunctionConfigurationXml>,
}

#[derive(Debug, Serialize)]
pub struct TopicConfigurationXml {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Topic")]
    pub topic_arn: String,
    #[serde(rename = "Event")]
    pub events: Vec<String>,
    #[serde(rename = "Filter", skip_serializing_if = "Option::is_none")]
    pub filter: Option<NotificationFilterXml>,
}

#[derive(Debug, Serialize)]
pub struct QueueConfigurationXml {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Queue")]
    pub queue_arn: String,
    #[serde(rename = "Event")]
    pub events: Vec<String>,
    #[serde(rename = "Filter", skip_serializing_if = "Option::is_none")]
    pub filter: Option<NotificationFilterXml>,
}

#[derive(Debug, Serialize)]
pub struct LambdaFunctionConfigurationXml {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "CloudFunction")]
    pub lambda_function_arn: String,
    #[serde(rename = "Event")]
    pub events: Vec<String>,
    #[serde(rename = "Filter", skip_serializing_if = "Option::is_none")]
    pub filter: Option<NotificationFilterXml>,
}

#[derive(Debug, Serialize)]
pub struct NotificationFilterXml {
    #[serde(rename = "S3Key")]
    pub s3_key: S3KeyFilterXml,
}

#[derive(Debug, Serialize)]
pub struct S3KeyFilterXml {
    #[serde(rename = "FilterRule")]
    pub filter_rules: Vec<FilterRuleXml>,
}

#[derive(Debug, Serialize)]
pub struct FilterRuleXml {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Value")]
    pub value: String,
}

fn build_notification_filter(
    rules: &[crate::storage::FilterRule],
) -> Option<NotificationFilterXml> {
    if rules.is_empty() {
        return None;
    }
    Some(NotificationFilterXml {
        s3_key: S3KeyFilterXml {
            filter_rules: rules
                .iter()
                .map(|r| FilterRuleXml {
                    name: r.name.clone(),
                    value: r.value.clone(),
                })
                .collect(),
        },
    })
}

impl NotificationConfigXml {
    pub fn from_config(cfg: &crate::storage::NotificationConfig) -> Self {
        NotificationConfigXml {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            topic_configurations: cfg
                .topic_configurations
                .iter()
                .map(|t| TopicConfigurationXml {
                    id: t.id.clone(),
                    topic_arn: t.topic_arn.clone(),
                    events: t.events.clone(),
                    filter: build_notification_filter(&t.filter_rules),
                })
                .collect(),
            queue_configurations: cfg
                .queue_configurations
                .iter()
                .map(|q| QueueConfigurationXml {
                    id: q.id.clone(),
                    queue_arn: q.queue_arn.clone(),
                    events: q.events.clone(),
                    filter: build_notification_filter(&q.filter_rules),
                })
                .collect(),
            lambda_function_configurations: cfg
                .lambda_function_configurations
                .iter()
                .map(|l| LambdaFunctionConfigurationXml {
                    id: l.id.clone(),
                    lambda_function_arn: l.lambda_function_arn.clone(),
                    events: l.events.clone(),
                    filter: build_notification_filter(&l.filter_rules),
                })
                .collect(),
        }
    }

    pub fn to_xml(&self) -> String {
        let body =
            to_xml_string(self).unwrap_or_else(|e| format!("<!-- serialization error: {} -->", e));
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}", body)
    }
}

/// ReplicationConfiguration XML response
#[derive(Debug, Serialize)]
#[serde(rename = "ReplicationConfiguration")]
pub struct ReplicationConfigurationXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Role")]
    pub role: String,
    #[serde(rename = "Rule")]
    pub rules: Vec<ReplicationRuleXml>,
}

#[derive(Debug, Serialize)]
pub struct ReplicationRuleXml {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Status")]
    pub status: String,
    #[serde(rename = "Priority", skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
    #[serde(rename = "Filter", skip_serializing_if = "Option::is_none")]
    pub filter: Option<ReplicationFilterXml>,
    #[serde(rename = "Destination")]
    pub destination: ReplicationDestinationXml,
    #[serde(
        rename = "DeleteMarkerReplication",
        skip_serializing_if = "Option::is_none"
    )]
    pub delete_marker_replication: Option<DeleteMarkerReplicationXml>,
}

#[derive(Debug, Serialize)]
pub struct ReplicationFilterXml {
    #[serde(rename = "Prefix")]
    pub prefix: String,
}

#[derive(Debug, Serialize)]
pub struct ReplicationDestinationXml {
    #[serde(rename = "Bucket")]
    pub bucket: String,
    #[serde(rename = "StorageClass", skip_serializing_if = "Option::is_none")]
    pub storage_class: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DeleteMarkerReplicationXml {
    #[serde(rename = "Status")]
    pub status: String,
}

impl ReplicationConfigurationXml {
    pub fn from_config(cfg: &crate::storage::ReplicationConfig) -> Self {
        ReplicationConfigurationXml {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            role: cfg.role.clone(),
            rules: cfg
                .rules
                .iter()
                .map(|r| ReplicationRuleXml {
                    id: r.id.clone(),
                    status: r.status.clone(),
                    priority: r.priority,
                    filter: r
                        .filter_prefix
                        .as_ref()
                        .map(|p| ReplicationFilterXml { prefix: p.clone() }),
                    destination: ReplicationDestinationXml {
                        bucket: r.destination.bucket.clone(),
                        storage_class: r.destination.storage_class.clone(),
                    },
                    delete_marker_replication: r
                        .delete_marker_replication
                        .as_ref()
                        .map(|s| DeleteMarkerReplicationXml { status: s.clone() }),
                })
                .collect(),
        }
    }

    pub fn to_xml(&self) -> String {
        let body =
            to_xml_string(self).unwrap_or_else(|e| format!("<!-- serialization error: {} -->", e));
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}", body)
    }
}

/// AccelerateConfiguration XML response
#[derive(Debug, Serialize)]
#[serde(rename = "AccelerateConfiguration")]
pub struct AccelerateConfigurationXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Status")]
    pub status: String,
}

impl AccelerateConfigurationXml {
    pub fn from_config(cfg: &crate::storage::AccelerateConfig) -> Self {
        AccelerateConfigurationXml {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            status: cfg.status.clone(),
        }
    }

    pub fn to_xml(&self) -> String {
        let body =
            to_xml_string(self).unwrap_or_else(|e| format!("<!-- serialization error: {} -->", e));
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}", body)
    }
}

/// IntelligentTieringConfiguration XML response
#[derive(Debug, Serialize)]
#[serde(rename = "IntelligentTieringConfiguration")]
pub struct IntelligentTieringConfigurationXml {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Status")]
    pub status: String,
    #[serde(rename = "Filter", skip_serializing_if = "Option::is_none")]
    pub filter: Option<IntelligentTieringFilterXml>,
    #[serde(rename = "Tiering")]
    pub tierings: Vec<TieringXml>,
}

#[derive(Debug, Serialize)]
pub struct IntelligentTieringFilterXml {
    #[serde(rename = "Prefix")]
    pub prefix: String,
}

#[derive(Debug, Serialize)]
pub struct TieringXml {
    #[serde(rename = "Days")]
    pub days: u32,
    #[serde(rename = "AccessTier")]
    pub access_tier: String,
}

impl IntelligentTieringConfigurationXml {
    pub fn from_config(cfg: &crate::storage::IntelligentTieringConfig) -> Self {
        IntelligentTieringConfigurationXml {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            id: cfg.id.clone(),
            status: cfg.status.clone(),
            filter: cfg
                .filter_prefix
                .as_ref()
                .map(|p| IntelligentTieringFilterXml { prefix: p.clone() }),
            tierings: cfg
                .tierings
                .iter()
                .map(|t| TieringXml {
                    days: t.days,
                    access_tier: t.access_tier.clone(),
                })
                .collect(),
        }
    }

    pub fn to_xml(&self) -> String {
        let body =
            to_xml_string(self).unwrap_or_else(|e| format!("<!-- serialization error: {} -->", e));
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}", body)
    }
}

// === XML escaping helper ===

pub(crate) fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// === Metrics XML serializers ===

pub struct MetricsConfigurationXml;

impl MetricsConfigurationXml {
    /// Produce a complete, standalone XML document for a single metrics config.
    /// Used by `GetBucketMetricsConfiguration` responses.
    pub fn from_config(cfg: &crate::storage::MetricsConfig) -> String {
        let mut xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
            <MetricsConfiguration xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">"
            .to_string();
        xml.push_str(&format!("<Id>{}</Id>", escape_xml(&cfg.id)));
        if let Some(ref f) = cfg.filter {
            if let Some(ref prefix) = f.prefix {
                xml.push_str(&format!(
                    "<Filter><Prefix>{}</Prefix></Filter>",
                    escape_xml(prefix)
                ));
            }
        }
        xml.push_str("</MetricsConfiguration>");
        xml
    }

    /// Produce a bare `<MetricsConfiguration>` element without XML prolog or
    /// namespace declaration for embedding inside a list response document.
    /// Embedding `from_config` output causes malformed XML (nested `<?xml?>`
    /// declarations) which breaks the AWS SDK's XML parser.
    fn to_xml_fragment(cfg: &crate::storage::MetricsConfig) -> String {
        let mut xml = "<MetricsConfiguration>".to_string();
        xml.push_str(&format!("<Id>{}</Id>", escape_xml(&cfg.id)));
        if let Some(ref f) = cfg.filter {
            if let Some(ref prefix) = f.prefix {
                xml.push_str(&format!(
                    "<Filter><Prefix>{}</Prefix></Filter>",
                    escape_xml(prefix)
                ));
            }
        }
        xml.push_str("</MetricsConfiguration>");
        xml
    }
}

pub struct ListMetricsConfigurationsResultXml;

impl ListMetricsConfigurationsResultXml {
    pub fn from_configs(configs: &[crate::storage::MetricsConfig]) -> String {
        let mut xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
            <ListMetricsConfigurationsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">"
            .to_string();
        xml.push_str("<IsTruncated>false</IsTruncated>");
        for cfg in configs {
            xml.push_str(&MetricsConfigurationXml::to_xml_fragment(cfg));
        }
        xml.push_str("</ListMetricsConfigurationsResult>");
        xml
    }
}

// === Analytics XML serializers ===

pub struct AnalyticsConfigurationXml;

impl AnalyticsConfigurationXml {
    /// Produce a complete, standalone XML document for a single analytics config.
    /// Used by `GetBucketAnalyticsConfiguration` responses.
    pub fn from_config(cfg: &crate::storage::AnalyticsConfig) -> String {
        let mut xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
            <AnalyticsConfiguration xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">"
            .to_string();
        xml.push_str(&Self::analytics_body(cfg));
        xml.push_str("</AnalyticsConfiguration>");
        xml
    }

    /// Produce a bare `<AnalyticsConfiguration>` element without XML prolog or
    /// namespace declaration for embedding inside a list response document.
    fn to_xml_fragment(cfg: &crate::storage::AnalyticsConfig) -> String {
        let mut xml = "<AnalyticsConfiguration>".to_string();
        xml.push_str(&Self::analytics_body(cfg));
        xml.push_str("</AnalyticsConfiguration>");
        xml
    }

    /// Inner XML content shared by `from_config` and `to_xml_fragment`.
    fn analytics_body(cfg: &crate::storage::AnalyticsConfig) -> String {
        let mut xml = String::new();
        xml.push_str(&format!("<Id>{}</Id>", escape_xml(&cfg.id)));
        if let Some(ref f) = cfg.filter {
            if let Some(ref prefix) = f.prefix {
                xml.push_str(&format!(
                    "<Filter><Prefix>{}</Prefix></Filter>",
                    escape_xml(prefix)
                ));
            }
        }
        if let Some(ref sca) = cfg.storage_class_analysis {
            xml.push_str("<StorageClassAnalysis>");
            if let Some(ref de) = sca.data_export {
                xml.push_str("<DataExport>");
                xml.push_str(&format!(
                    "<OutputSchemaVersion>{}</OutputSchemaVersion>",
                    escape_xml(&de.output_schema_version)
                ));
                if let Some(ref dest) = de.destination {
                    xml.push_str("<Destination>");
                    if let Some(ref s3) = dest.s3_bucket_destination {
                        xml.push_str("<S3BucketDestination>");
                        xml.push_str(&format!("<Format>{}</Format>", escape_xml(&s3.format)));
                        xml.push_str(&format!("<Bucket>{}</Bucket>", escape_xml(&s3.bucket)));
                        if let Some(ref p) = s3.prefix {
                            xml.push_str(&format!("<Prefix>{}</Prefix>", escape_xml(p)));
                        }
                        xml.push_str("</S3BucketDestination>");
                    }
                    xml.push_str("</Destination>");
                }
                xml.push_str("</DataExport>");
            }
            xml.push_str("</StorageClassAnalysis>");
        }
        xml
    }
}

pub struct ListBucketAnalyticsConfigurationsResultXml;

impl ListBucketAnalyticsConfigurationsResultXml {
    pub fn from_configs(configs: &[crate::storage::AnalyticsConfig]) -> String {
        let mut xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
            <ListBucketAnalyticsConfigurationsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">"
            .to_string();
        xml.push_str("<IsTruncated>false</IsTruncated>");
        for cfg in configs {
            xml.push_str(&AnalyticsConfigurationXml::to_xml_fragment(cfg));
        }
        xml.push_str("</ListBucketAnalyticsConfigurationsResult>");
        xml
    }
}

// === Inventory XML serializers ===

pub struct InventoryConfigurationXml;

impl InventoryConfigurationXml {
    /// Produce a complete, standalone XML document for a single inventory config.
    /// Used by `GetBucketInventoryConfiguration` responses.
    pub fn from_config(cfg: &crate::storage::InventoryConfig) -> String {
        let mut xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
            <InventoryConfiguration xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">"
            .to_string();
        xml.push_str(&Self::inventory_body(cfg));
        xml.push_str("</InventoryConfiguration>");
        xml
    }

    /// Produce a bare `<InventoryConfiguration>` element without XML prolog or
    /// namespace declaration for embedding inside a list response document.
    fn to_xml_fragment(cfg: &crate::storage::InventoryConfig) -> String {
        let mut xml = "<InventoryConfiguration>".to_string();
        xml.push_str(&Self::inventory_body(cfg));
        xml.push_str("</InventoryConfiguration>");
        xml
    }

    /// Inner XML content shared by `from_config` and `to_xml_fragment`.
    fn inventory_body(cfg: &crate::storage::InventoryConfig) -> String {
        let mut xml = String::new();
        xml.push_str(&format!("<Id>{}</Id>", escape_xml(&cfg.id)));
        xml.push_str(&format!("<IsEnabled>{}</IsEnabled>", cfg.is_enabled));
        xml.push_str(&format!(
            "<IncludedObjectVersions>{}</IncludedObjectVersions>",
            escape_xml(&cfg.included_object_versions)
        ));
        xml.push_str("<Schedule>");
        xml.push_str(&format!(
            "<Frequency>{}</Frequency>",
            escape_xml(&cfg.schedule_frequency)
        ));
        xml.push_str("</Schedule>");
        xml.push_str("<Destination><S3BucketDestination>");
        xml.push_str(&format!(
            "<Bucket>{}</Bucket>",
            escape_xml(&cfg.destination.s3_bucket_destination.bucket)
        ));
        xml.push_str(&format!(
            "<Format>{}</Format>",
            escape_xml(&cfg.destination.s3_bucket_destination.format)
        ));
        if let Some(ref p) = cfg.destination.s3_bucket_destination.prefix {
            xml.push_str(&format!("<Prefix>{}</Prefix>", escape_xml(p)));
        }
        xml.push_str("</S3BucketDestination></Destination>");
        if !cfg.optional_fields.is_empty() {
            xml.push_str("<OptionalFields>");
            for field in &cfg.optional_fields {
                xml.push_str(&format!("<Field>{}</Field>", escape_xml(field)));
            }
            xml.push_str("</OptionalFields>");
        }
        xml
    }
}

pub struct ListInventoryConfigurationsResultXml;

impl ListInventoryConfigurationsResultXml {
    pub fn from_configs(configs: &[crate::storage::InventoryConfig]) -> String {
        let mut xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
            <ListInventoryConfigurationsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">"
            .to_string();
        xml.push_str("<IsTruncated>false</IsTruncated>");
        for cfg in configs {
            xml.push_str(&InventoryConfigurationXml::to_xml_fragment(cfg));
        }
        xml.push_str("</ListInventoryConfigurationsResult>");
        xml
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_response_xml() {
        let err = ErrorResponse {
            code: "NoSuchKey".to_string(),
            message: "The specified key does not exist.".to_string(),
            resource: "/bucket/key".to_string(),
            request_id: "test-request-id".to_string(),
        };
        let xml = err.to_xml();
        assert!(xml.contains("NoSuchKey"));
        assert!(xml.contains("The specified key does not exist."));
    }

    #[test]
    fn test_list_bucket_result_xml() {
        let result = ListBucketResult::new("test-bucket");
        let xml = result.to_xml();
        assert!(xml.contains("test-bucket"));
        assert!(xml.contains("ListBucketResult"));
    }

    #[test]
    fn test_list_bucket_result_v1_xml() {
        let mut result = ListBucketResultV1::new("test-bucket");
        result.marker = "start-key".to_string();
        result.max_keys = 100;
        let xml = result.to_xml();
        assert!(xml.contains("test-bucket"));
        assert!(xml.contains("ListBucketResult"));
        assert!(xml.contains("<Marker>start-key</Marker>"));
        assert!(xml.contains("<MaxKeys>100</MaxKeys>"));
        // V1 should not contain KeyCount (that's V2 only)
        assert!(!xml.contains("KeyCount"));
    }
}

//! S3 XML Response Structures
//!
//! Defines XML serializable structures for S3 API responses.

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

/// Grant element for ACL
#[derive(Debug, Serialize)]
#[serde(rename = "Grant")]
pub struct Grant {
    #[serde(rename = "Grantee")]
    pub grantee: Grantee,
    #[serde(rename = "Permission")]
    pub permission: String,
}

/// Grantee element for ACL
#[derive(Debug, Serialize)]
#[serde(rename = "Grantee")]
pub struct Grantee {
    #[serde(rename = "@xmlns:xsi")]
    pub xmlns_xsi: String,
    #[serde(rename = "@xsi:type")]
    pub xsi_type: String,
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "DisplayName")]
    pub display_name: String,
}

/// AccessControlList element
#[derive(Debug, Serialize)]
#[serde(rename = "AccessControlList")]
pub struct AccessControlList {
    #[serde(rename = "Grant")]
    pub grants: Vec<Grant>,
}

/// AccessControlPolicy response for GetBucketAcl
#[derive(Debug, Serialize)]
#[serde(rename = "AccessControlPolicy")]
pub struct AccessControlPolicy {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Owner")]
    pub owner: Owner,
    #[serde(rename = "AccessControlList")]
    pub access_control_list: AccessControlList,
}

impl AccessControlPolicy {
    pub fn new_full_control() -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            owner: Owner::default(),
            access_control_list: AccessControlList {
                grants: vec![Grant {
                    grantee: Grantee {
                        xmlns_xsi: "http://www.w3.org/2001/XMLSchema-instance".to_string(),
                        xsi_type: "CanonicalUser".to_string(),
                        id: "rs3gw".to_string(),
                        display_name: "rs3gw".to_string(),
                    },
                    permission: "FULL_CONTROL".to_string(),
                }],
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

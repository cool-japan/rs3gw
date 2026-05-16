//! ACL XML response types
//!
//! Grant, Grantee, AccessControlList, and AccessControlPolicy structures
//! for S3 ACL API responses.

use quick_xml::se::to_string as to_xml_string;
use serde::Serialize;

use super::Owner;
use crate::storage::{AclConfig, AclGrant};

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
///
/// Supports CanonicalUser (ID + DisplayName), Group (URI), and
/// AmazonCustomerByEmail (EmailAddress) grantee types.
#[derive(Debug, Serialize)]
#[serde(rename = "Grantee")]
pub struct Grantee {
    #[serde(rename = "@xmlns:xsi")]
    pub xmlns_xsi: String,
    #[serde(rename = "@xsi:type")]
    pub xsi_type: String,
    #[serde(rename = "ID", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "DisplayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(rename = "URI", skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(rename = "EmailAddress", skip_serializing_if = "Option::is_none")]
    pub email_address: Option<String>,
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
                        id: Some("rs3gw".to_string()),
                        display_name: Some("rs3gw".to_string()),
                        uri: None,
                        email_address: None,
                    },
                    permission: "FULL_CONTROL".to_string(),
                }],
            },
        }
    }

    /// Build an `AccessControlPolicy` from an `AclConfig` stored in the engine.
    pub fn from_acl_config(cfg: &AclConfig) -> Self {
        Self {
            xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
            owner: Owner {
                id: cfg.owner_id.clone(),
                display_name: cfg.owner_display_name.clone(),
            },
            access_control_list: AccessControlList {
                grants: cfg
                    .grants
                    .iter()
                    .map(|g| Grant {
                        grantee: Grantee::from_acl_grant(g),
                        permission: g.permission.clone(),
                    })
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

impl Grantee {
    /// Construct a `Grantee` from an `AclGrant`, emitting only the fields
    /// relevant to the grantee type (no empty `<ID/>` for Group grantees, etc.).
    pub fn from_acl_grant(g: &AclGrant) -> Self {
        match g.grantee_type.as_str() {
            "Group" => Self {
                xmlns_xsi: "http://www.w3.org/2001/XMLSchema-instance".to_string(),
                xsi_type: "Group".to_string(),
                id: None,
                display_name: None,
                uri: g.grantee_uri.clone(),
                email_address: None,
            },
            "AmazonCustomerByEmail" => Self {
                xmlns_xsi: "http://www.w3.org/2001/XMLSchema-instance".to_string(),
                xsi_type: "AmazonCustomerByEmail".to_string(),
                id: None,
                display_name: None,
                uri: None,
                email_address: g.grantee_email.clone(),
            },
            // Default: CanonicalUser
            _ => Self {
                xmlns_xsi: "http://www.w3.org/2001/XMLSchema-instance".to_string(),
                xsi_type: "CanonicalUser".to_string(),
                id: g.grantee_id.clone(),
                display_name: g.grantee_display_name.clone(),
                uri: None,
                email_address: None,
            },
        }
    }
}

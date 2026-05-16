//! Object Lock XML response types
//!
//! LegalHoldXml, RetentionXml, and ObjectLockConfigurationXml structures
//! for S3 Object Lock API responses.

use chrono::{DateTime, Utc};

// === Object Lock XML response types ===

/// XML response for GetObjectLegalHold.
pub struct LegalHoldXml {
    pub status: String,
}

impl LegalHoldXml {
    pub fn from_status(status: &str) -> Self {
        Self {
            status: status.to_string(),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
            <LegalHold xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
            <Status>{}</Status>\
            </LegalHold>",
            self.status
        )
    }
}

/// XML response for GetObjectRetention.
pub struct RetentionXml {
    pub mode: String,
    pub retain_until_date: String,
}

impl RetentionXml {
    pub fn new(mode: &str, until: &DateTime<Utc>) -> Self {
        Self {
            mode: mode.to_string(),
            retain_until_date: until.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
            <Retention xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
            <Mode>{}</Mode>\
            <RetainUntilDate>{}</RetainUntilDate>\
            </Retention>",
            self.mode, self.retain_until_date
        )
    }
}

/// XML builder for GetObjectLockConfiguration / PutObjectLockConfiguration responses.
pub struct ObjectLockConfigurationXml;

impl ObjectLockConfigurationXml {
    pub fn from_config(cfg: &crate::storage::ObjectLockConfig) -> String {
        let ns = "http://s3.amazonaws.com/doc/2006-03-01/";
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
            <ObjectLockConfiguration xmlns=\"{}\">",
            ns
        );
        xml.push_str(&format!(
            "<ObjectLockEnabled>{}</ObjectLockEnabled>",
            cfg.object_lock_enabled
        ));
        if let Some(ref rule) = cfg.rule {
            xml.push_str("<Rule>");
            if let Some(ref dr) = rule.default_retention {
                xml.push_str("<DefaultRetention>");
                xml.push_str(&format!("<Mode>{}</Mode>", dr.mode));
                if let Some(days) = dr.days {
                    xml.push_str(&format!("<Days>{}</Days>", days));
                }
                if let Some(years) = dr.years {
                    xml.push_str(&format!("<Years>{}</Years>", years));
                }
                xml.push_str("</DefaultRetention>");
            }
            xml.push_str("</Rule>");
        }
        xml.push_str("</ObjectLockConfiguration>");
        xml
    }
}

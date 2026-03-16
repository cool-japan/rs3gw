//! Log forwarding destination types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Log forwarding destination types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ForwardDestination {
    /// Syslog (RFC 5424)
    Syslog {
        host: String,
        port: u16,
        protocol: SyslogProtocol,
    },

    /// HTTP webhook
    Webhook {
        url: String,
        headers: HashMap<String, String>,
    },

    /// S3 bucket
    S3 {
        bucket: String,
        prefix: String,
        region: String,
    },

    /// Local file
    File { path: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyslogProtocol {
    Udp,
    Tcp,
    Tls,
}

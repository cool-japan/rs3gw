//! IP address filtering for ABAC

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

/// IP range for filtering (CIDR notation)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpRange {
    /// Single IP address
    Single(IpAddr),
    /// IPv4 CIDR range
    Ipv4Cidr { addr: Ipv4Addr, prefix_len: u8 },
    /// IPv6 CIDR range
    Ipv6Cidr { addr: Ipv6Addr, prefix_len: u8 },
}

impl IpRange {
    /// Parse IP range from string (supports single IPs and CIDR notation)
    pub fn parse(s: &str) -> Result<Self, String> {
        if let Some((addr_str, prefix_str)) = s.split_once('/') {
            // CIDR notation
            let prefix_len: u8 = prefix_str
                .parse()
                .map_err(|_| format!("Invalid prefix length: {}", prefix_str))?;

            if let Ok(ipv4) = Ipv4Addr::from_str(addr_str) {
                if prefix_len > 32 {
                    return Err(format!(
                        "IPv4 prefix length must be <= 32, got {}",
                        prefix_len
                    ));
                }
                Ok(IpRange::Ipv4Cidr {
                    addr: ipv4,
                    prefix_len,
                })
            } else if let Ok(ipv6) = Ipv6Addr::from_str(addr_str) {
                if prefix_len > 128 {
                    return Err(format!(
                        "IPv6 prefix length must be <= 128, got {}",
                        prefix_len
                    ));
                }
                Ok(IpRange::Ipv6Cidr {
                    addr: ipv6,
                    prefix_len,
                })
            } else {
                Err(format!("Invalid IP address: {}", addr_str))
            }
        } else {
            // Single IP
            IpAddr::from_str(s)
                .map(IpRange::Single)
                .map_err(|e| format!("Invalid IP address: {}", e))
        }
    }

    /// Check if an IP address matches this range
    pub fn contains(&self, ip: &IpAddr) -> bool {
        match (self, ip) {
            (IpRange::Single(range_ip), test_ip) => range_ip == test_ip,
            (IpRange::Ipv4Cidr { addr, prefix_len }, IpAddr::V4(test_ip)) => {
                ipv4_in_cidr(*test_ip, *addr, *prefix_len)
            }
            (IpRange::Ipv6Cidr { addr, prefix_len }, IpAddr::V6(test_ip)) => {
                ipv6_in_cidr(*test_ip, *addr, *prefix_len)
            }
            _ => false, // IPv4 range with IPv6 address or vice versa
        }
    }
}

/// IP filter for whitelist/blacklist logic
#[derive(Debug, Clone)]
pub struct IpFilter {
    whitelist: Vec<IpRange>,
    blacklist: Vec<IpRange>,
}

impl IpFilter {
    /// Create a new IP filter
    pub fn new() -> Self {
        Self {
            whitelist: Vec::new(),
            blacklist: Vec::new(),
        }
    }

    /// Create a filter with whitelist
    pub fn with_whitelist(whitelist: Vec<String>) -> Result<Self, String> {
        let ranges: Result<Vec<IpRange>, String> =
            whitelist.iter().map(|s| IpRange::parse(s)).collect();
        Ok(Self {
            whitelist: ranges?,
            blacklist: Vec::new(),
        })
    }

    /// Create a filter with blacklist
    pub fn with_blacklist(blacklist: Vec<String>) -> Result<Self, String> {
        let ranges: Result<Vec<IpRange>, String> =
            blacklist.iter().map(|s| IpRange::parse(s)).collect();
        Ok(Self {
            whitelist: Vec::new(),
            blacklist: ranges?,
        })
    }

    /// Check if an IP address is allowed
    pub fn is_allowed(&self, ip: &IpAddr) -> bool {
        // If blacklist is present and IP is in it, deny
        if !self.blacklist.is_empty() {
            for range in &self.blacklist {
                if range.contains(ip) {
                    return false;
                }
            }
        }

        // If whitelist is present, IP must be in it
        if !self.whitelist.is_empty() {
            for range in &self.whitelist {
                if range.contains(ip) {
                    return true;
                }
            }
            return false;
        }

        // No restrictions, allow
        true
    }
}

impl Default for IpFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if an IPv4 address is in a CIDR range
fn ipv4_in_cidr(ip: Ipv4Addr, network: Ipv4Addr, prefix_len: u8) -> bool {
    if prefix_len == 0 {
        return true; // 0.0.0.0/0 matches everything
    }
    if prefix_len > 32 {
        return false;
    }

    let ip_bits = u32::from(ip);
    let network_bits = u32::from(network);
    let mask = !0u32 << (32 - prefix_len);

    (ip_bits & mask) == (network_bits & mask)
}

/// Check if an IPv6 address is in a CIDR range
fn ipv6_in_cidr(ip: Ipv6Addr, network: Ipv6Addr, prefix_len: u8) -> bool {
    if prefix_len == 0 {
        return true; // ::/0 matches everything
    }
    if prefix_len > 128 {
        return false;
    }

    let ip_bits = u128::from(ip);
    let network_bits = u128::from(network);
    let mask = !0u128 << (128 - prefix_len);

    (ip_bits & mask) == (network_bits & mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipv4_single() {
        let range =
            IpRange::parse("192.168.1.1").expect("Failed to parse IPv4 single address range");
        let ip: IpAddr = "192.168.1.1"
            .parse()
            .expect("Failed to parse IP address 192.168.1.1");
        assert!(range.contains(&ip));

        let ip2: IpAddr = "192.168.1.2"
            .parse()
            .expect("Failed to parse IP address 192.168.1.2");
        assert!(!range.contains(&ip2));
    }

    #[test]
    fn test_ipv4_cidr() {
        let range = IpRange::parse("192.168.1.0/24")
            .expect("Failed to parse IPv4 CIDR range 192.168.1.0/24");

        let ip1: IpAddr = "192.168.1.1"
            .parse()
            .expect("Failed to parse IP address 192.168.1.1");
        assert!(range.contains(&ip1));

        let ip2: IpAddr = "192.168.1.255"
            .parse()
            .expect("Failed to parse IP address 192.168.1.255");
        assert!(range.contains(&ip2));

        let ip3: IpAddr = "192.168.2.1"
            .parse()
            .expect("Failed to parse IP address 192.168.2.1");
        assert!(!range.contains(&ip3));
    }

    #[test]
    fn test_ipv6_cidr() {
        let range =
            IpRange::parse("2001:db8::/32").expect("Failed to parse IPv6 CIDR range 2001:db8::/32");

        let ip1: IpAddr = "2001:db8::1"
            .parse()
            .expect("Failed to parse IPv6 address 2001:db8::1");
        assert!(range.contains(&ip1));

        let ip2: IpAddr = "2001:db9::1"
            .parse()
            .expect("Failed to parse IPv6 address 2001:db9::1");
        assert!(!range.contains(&ip2));
    }

    #[test]
    fn test_ip_filter_whitelist() {
        let filter =
            IpFilter::with_whitelist(vec!["192.168.1.0/24".to_string(), "10.0.0.1".to_string()])
                .expect("Failed to create IP filter whitelist");

        let ip1: IpAddr = "192.168.1.100"
            .parse()
            .expect("Failed to parse IP address 192.168.1.100");
        assert!(filter.is_allowed(&ip1));

        let ip2: IpAddr = "10.0.0.1"
            .parse()
            .expect("Failed to parse IP address 10.0.0.1");
        assert!(filter.is_allowed(&ip2));

        let ip3: IpAddr = "172.16.0.1"
            .parse()
            .expect("Failed to parse IP address 172.16.0.1");
        assert!(!filter.is_allowed(&ip3));
    }

    #[test]
    fn test_ip_filter_blacklist() {
        let filter = IpFilter::with_blacklist(vec!["192.168.1.100".to_string()])
            .expect("Failed to create IP filter blacklist");

        let ip1: IpAddr = "192.168.1.100"
            .parse()
            .expect("Failed to parse IP address 192.168.1.100 for blacklist test");
        assert!(!filter.is_allowed(&ip1));

        let ip2: IpAddr = "192.168.1.101"
            .parse()
            .expect("Failed to parse IP address 192.168.1.101 for blacklist test");
        assert!(filter.is_allowed(&ip2));
    }
}

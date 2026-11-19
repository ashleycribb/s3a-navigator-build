//! Egress deny list and allowlist (§5.2.4, AT-226, AT-183). Pure functions,
//! unit-tested with no network and no resolver override.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// AT-226: true iff `addr` is in the union of the startup resolution and
/// the connect-time resolution of the configured LRS host.
pub fn lrs_deny_union(
    startup_ips: &[IpAddr],
    connect_time_ips: &[IpAddr],
    addr: &IpAddr,
) -> bool {
    startup_ips.contains(addr) || connect_time_ips.contains(addr)
}

/// Is this hostname denied **by name**, applied to the URL host **before
/// any DNS query** (AT-226)? No lookup and no connect are made.
pub fn is_denied_hostname(host: &str) -> bool {
    host.eq_ignore_ascii_case("metadata.google.internal")
}

/// Is this address denied by the deny list (any address that is not global
/// unicast, plus the metadata address)? Covers loopback (IPv4 and `::1`),
/// IPv4-mapped forms of those ranges, `0.0.0.0/8`, `::`, RFC1918, RFC4193,
/// IPv6 site-local (`fec0::/10`), link-local, multicast, broadcast, CGNAT
/// `100.64.0.0/10`, NAT64 / 6to4 translated forms of the same ranges, and
/// the metadata address `169.254.169.254`.
pub fn is_denied_address(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => is_denied_v4(*v4),
        IpAddr::V6(v6) => is_denied_v6(*v6),
    }
}

fn is_denied_v4(v4: Ipv4Addr) -> bool {
    // The std `is_reserved` / `is_shared` / `is_benchmarking` helpers are
    // still behind the unstable `ip` feature in this toolchain, so the
    // ranges are checked by octets here (RFC 6890).
    let o = v4.octets();
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_multicast()
        || v4.is_broadcast()
        || v4.is_unspecified()
        || v4.is_documentation()
        || o[0] >= 240 // 240.0.0.0/4 reserved
        || (o[0] == 100 && (64..=127).contains(&o[1])) // 100.64.0.0/10 CGNAT
        || (o[0] == 198 && (18..=19).contains(&o[1])) // 198.18.0.0/15 benchmarking
        || v4 == Ipv4Addr::new(169, 254, 169, 254)
}

fn is_denied_v6(v6: Ipv6Addr) -> bool {
    if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() || v6.is_unique_local() {
        return true;
    }
    // IPv4-mapped: deny iff the embedded IPv4 is denied.
    if let Some(v4) = v6.to_ipv4_mapped() {
        return is_denied_v4(v4);
    }
    let seg = v6.segments();
    // IPv6 site-local fec0::/10 (deprecated but listed).
    if (seg[0] & 0xffc0) == 0xfec0 {
        return true;
    }
    // Link-local fe80::/10.
    if v6.is_unicast_link_local() {
        return true;
    }
    // NAT64 64:ff9b::/96: deny iff the embedded IPv4 is denied.
    if seg[0] == 0x0064 && seg[1] == 0xff9b && seg[2] == 0 && seg[3] == 0 && seg[4] == 0 {
        let v4 = Ipv4Addr::new(
            (seg[6] >> 8) as u8,
            (seg[6] & 0xff) as u8,
            (seg[7] >> 8) as u8,
            (seg[7] & 0xff) as u8,
        );
        return is_denied_v4(v4);
    }
    // 6to4 2002::/16: deny iff the embedded IPv4 is denied.
    if seg[0] == 0x2002 {
        let v4 = Ipv4Addr::new(
            (seg[1] >> 8) as u8,
            (seg[1] & 0xff) as u8,
            (seg[2] >> 8) as u8,
            (seg[2] & 0xff) as u8,
        );
        return is_denied_v4(v4);
    }
    false
}

/// One `--egress-allowlist` entry: an exact hostname or IP literal, with an
/// optional exact port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowEntry {
    pub host: String,
    pub ip: Option<IpAddr>,
    pub port: Option<u16>,
}

/// Parse `--egress-allowlist`: a comma-separated list of exact hostnames or
/// exact IP literals, each optionally followed by `:port`. Ports on IPv6
/// entries require brackets (`[2001:db8::1]:443`). An unbracketed entry is
/// parsed as an IPv6 address first: if it parses as a single IPv6 address
/// it is a portless exact address and is valid. An unbracketed entry that
/// is not a hostname, not an IPv4 literal, and does not parse as an IPv6
/// address is a startup error that names `--egress-allowlist` (AT-183).
pub fn parse_allowlist(flag: &str) -> Result<Vec<AllowEntry>, String> {
    let mut out = Vec::new();
    for raw in flag.split(',') {
        let entry = raw.trim();
        if entry.is_empty() {
            return Err("--egress-allowlist: empty entry".to_string());
        }
        out.push(parse_allow_entry(entry)?);
    }
    Ok(out)
}

fn parse_allow_entry(entry: &str) -> Result<AllowEntry, String> {
    let err = || format!("--egress-allowlist: unparseable entry {entry:?}");
    if let Some(rest) = entry.strip_prefix('[') {
        // [ipv6] or [ipv6]:port
        let (addr_str, port) = match rest.split_once(']') {
            Some((a, "")) => (a, None),
            Some((a, p)) => {
                let p = p.strip_prefix(':').ok_or_else(err)?;
                (a, Some(parse_port(p).ok_or_else(err)?))
            }
            None => return Err(err()),
        };
        let ip: IpAddr = addr_str.parse().map_err(|_| err())?;
        if !matches!(ip, IpAddr::V6(_)) {
            return Err(err());
        }
        return Ok(AllowEntry {
            host: addr_str.to_string(),
            ip: Some(ip),
            port,
        });
    }
    // Unbracketed: try the whole entry as an IPv6 address first.
    if let Ok(ip) = entry.parse::<IpAddr>() {
        return Ok(AllowEntry {
            host: entry.to_string(),
            ip: Some(ip),
            port: None,
        });
    }
    // host or host:port (split at the last colon).
    if let Some((host, port_str)) = entry.rsplit_once(':') {
        // A bare IPv6-looking left side without brackets is not a host:port.
        if host.contains(':') {
            return Err(err());
        }
        let port = parse_port(port_str).ok_or_else(err)?;
        validate_hostname(host).ok_or_else(err)?;
        let ip = host.parse::<IpAddr>().ok();
        return Ok(AllowEntry {
            host: host.to_string(),
            ip,
            port: Some(port),
        });
    }
    validate_hostname(entry).ok_or_else(err)?;
    let ip = entry.parse::<IpAddr>().ok();
    Ok(AllowEntry {
        host: entry.to_string(),
        ip,
        port: None,
    })
}

fn parse_port(s: &str) -> Option<u16> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u32 = s.parse().ok()?;
    if n == 0 || n > 65535 {
        return None;
    }
    Some(n as u16)
}

fn validate_hostname(host: &str) -> Option<()> {
    if host.is_empty() || host.len() > 253 {
        return None;
    }
    // Exact hostname: labels of alphanumerics and hyphens (underscores are
    // tolerated, as they appear in real hostnames); no leading/trailing dot.
    if host.starts_with('.') || host.ends_with('.') {
        return None;
    }
    for label in host.split('.') {
        if label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return None;
        }
    }
    Some(())
}

/// Does this (`scheme`, `host`, effective port) match the allowlist?
/// An entry without a port matches **only** port 443 with scheme `https`.
/// An entry `host:PORT` matches either scheme at that effective port.
/// Matching never overrides the deny list (the caller checks that first).
pub fn allowlist_matches(
    entries: &[AllowEntry],
    scheme: &str,
    host: &str,
    effective_port: u16,
) -> bool {
    let host_ip: Option<IpAddr> = host.parse().ok();
    entries.iter().any(|e| {
        let host_matches = match (e.ip, host_ip) {
            (Some(a), Some(b)) => a == b,
            _ => e.host.eq_ignore_ascii_case(host),
        };
        if !host_matches {
            return false;
        }
        match e.port {
            None => scheme.eq_ignore_ascii_case("https") && effective_port == 443,
            Some(p) => p == effective_port,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn deny_union() {
        let startup = vec![ip("93.184.216.34")];
        let connect = vec![ip("93.184.216.35")];
        assert!(lrs_deny_union(&startup, &connect, &ip("93.184.216.34")));
        assert!(lrs_deny_union(&startup, &connect, &ip("93.184.216.35")));
        assert!(!lrs_deny_union(&startup, &connect, &ip("8.8.8.8")));
    }

    #[test]
    fn denied_addresses() {
        // Loopback, private, link-local, multicast, CGNAT, metadata.
        for s in [
            "127.0.0.1",
            "127.0.0.2",
            "127.53.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.1.1",
            "169.254.169.254",
            "224.0.0.1",
            "255.255.255.255",
            "0.0.0.0",
            "100.64.0.1",
            "192.0.2.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fec0::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "64:ff9b::a9fe:a9fe",
            "2002:7f00:1::1",
        ] {
            assert!(is_denied_address(&ip(s)), "{s} should be denied");
        }
        // Global unicast passes.
        for s in ["8.8.8.8", "93.184.216.34", "2001:db8::1", "2606:4700:4700::1111"] {
            assert!(!is_denied_address(&ip(s)), "{s} should be allowed");
        }
        // 6to4 / NAT64 with a global embedded v4 are not denied by the
        // translated-form rule.
        assert!(!is_denied_address(&ip("2002:0808:0808::1")));
        assert!(!is_denied_address(&ip("64:ff9b::808:808")));
    }

    #[test]
    fn denied_hostname_by_name() {
        assert!(is_denied_hostname("metadata.google.internal"));
        assert!(is_denied_hostname("METADATA.GOOGLE.INTERNAL"));
        assert!(!is_denied_hostname("example.com"));
    }

    #[test]
    fn allowlist_parsing() {
        // Bare IPv6 is a portless exact address, not host:port.
        let e = parse_allowlist("2001:db8::1:443").unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].port, None);
        assert_eq!(e[0].ip, Some(ip("2001:db8::1:443")));
        // Bracketed IPv6 with a port.
        let e = parse_allowlist("[2001:db8::1]:443").unwrap();
        assert_eq!(e[0].port, Some(443));
        // The SPEC's invalid example.
        assert!(parse_allowlist("2001:db8::1::443").is_err());
        assert!(parse_allowlist("host:0").is_err());
        assert!(parse_allowlist("host:99999").is_err());
        assert!(parse_allowlist("").is_err());
        let e = parse_allowlist("example.com,example.org:8443").unwrap();
        assert_eq!(e.len(), 2);
    }

    #[test]
    fn allowlist_matching() {
        let entries = parse_allowlist("host,host2:443,host3:80").unwrap();
        // Portless entry: only https/443.
        assert!(allowlist_matches(&entries, "https", "host", 443));
        assert!(!allowlist_matches(&entries, "http", "host", 80));
        assert!(!allowlist_matches(&entries, "http", "host", 443));
        assert!(!allowlist_matches(&entries, "https", "host", 8443));
        // host2:443 matches either scheme at 443.
        assert!(allowlist_matches(&entries, "https", "host2", 443));
        assert!(allowlist_matches(&entries, "http", "host2", 443));
        assert!(!allowlist_matches(&entries, "https", "host2", 8443));
        // host3:80 matches http/80 and https/80, not https/443.
        assert!(allowlist_matches(&entries, "http", "host3", 80));
        assert!(allowlist_matches(&entries, "https", "host3", 80));
        assert!(!allowlist_matches(&entries, "https", "host3", 443));
    }
}

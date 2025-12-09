use crate::config::ProxyMode;
use axum::http::HeaderMap;
use std::net::{IpAddr, SocketAddr};
use tracing::debug;

/// Extracts the real client IP address from the request.
///
/// If proxy_mode is configured, it attempts to extract the IP from the appropriate
/// header. If the connecting IP is not in trusted_proxies (when trusted_proxies is
/// non-empty), the direct connection IP is returned instead.
pub fn extract_client_ip(
    headers: &HeaderMap,
    conn_addr: SocketAddr,
    proxy_mode: &ProxyMode,
    trusted_proxies: &[IpAddr],
) -> String {
    let direct_ip = conn_addr.ip();

    // If no proxy mode configured, return direct connection IP
    let header_name = match proxy_mode.header_name() {
        Some(name) => name,
        None => return direct_ip.to_string(),
    };

    // Check if the connecting IP is a trusted proxy
    if !trusted_proxies.is_empty() && !trusted_proxies.contains(&direct_ip) {
        debug!(
            direct_ip = %direct_ip,
            "Connection not from trusted proxy, using direct IP"
        );
        return direct_ip.to_string();
    }

    // Try to extract IP from the configured header
    let header_value = match headers.get(header_name).and_then(|v| v.to_str().ok()) {
        Some(value) => value,
        None => {
            debug!(header = header_name, "Proxy header not found, using direct IP");
            return direct_ip.to_string();
        }
    };

    // Parse the header value based on proxy mode
    let extracted_ip = match proxy_mode {
        ProxyMode::Forwarded => parse_forwarded_header(header_value),
        ProxyMode::XForwardedFor => parse_x_forwarded_for(header_value),
        _ => parse_simple_ip_header(header_value),
    };

    match extracted_ip {
        Some(ip) => {
            debug!(
                header = header_name,
                extracted_ip = %ip,
                direct_ip = %direct_ip,
                "Extracted client IP from proxy header"
            );
            ip
        }
        None => {
            debug!(
                header = header_name,
                value = header_value,
                "Failed to parse IP from proxy header, using direct IP"
            );
            direct_ip.to_string()
        }
    }
}

/// Parses X-Forwarded-For header which contains a comma-separated list of IPs.
/// The leftmost IP is the original client.
/// Format: "client, proxy1, proxy2"
fn parse_x_forwarded_for(value: &str) -> Option<String> {
    value
        .split(',')
        .next()
        .map(|ip| ip.trim().to_string())
        .filter(|ip| !ip.is_empty())
}

/// Parses the RFC 7239 Forwarded header.
/// Format: "for=192.0.2.60;proto=http;by=203.0.113.43" or "for="[2001:db8::1]""
fn parse_forwarded_header(value: &str) -> Option<String> {
    // Get the first forwarded element (original client)
    let first_element = value.split(',').next()?;

    // Find the "for=" directive
    for directive in first_element.split(';') {
        let directive = directive.trim();
        if directive.to_lowercase().starts_with("for=") {
            let ip_part = &directive[4..];
            return Some(clean_forwarded_ip(ip_part));
        }
    }

    None
}

/// Cleans up an IP from the Forwarded header (removes quotes and brackets for IPv6)
fn clean_forwarded_ip(ip: &str) -> String {
    let ip = ip.trim();

    // Remove surrounding quotes if present
    let ip = ip.trim_matches('"');

    // Handle IPv6 with brackets: [2001:db8::1] or [2001:db8::1]:port
    if ip.starts_with('[')
        && let Some(end_bracket) = ip.find(']')
    {
        return ip[1..end_bracket].to_string();
    }

    // Handle IPv4 with port: 192.168.1.1:12345
    if let Some(colon_pos) = ip.rfind(':') {
        // Check if it looks like IPv4:port (simple heuristic)
        let before_colon = &ip[..colon_pos];
        if before_colon.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return before_colon.to_string();
        }
    }

    ip.to_string()
}

/// Parses a simple header that contains just an IP address
/// (like X-Real-IP, CF-Connecting-IP, True-Client-IP)
fn parse_simple_ip_header(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_x_forwarded_for_single() {
        assert_eq!(
            parse_x_forwarded_for("192.168.1.1"),
            Some("192.168.1.1".to_string())
        );
    }

    #[test]
    fn test_parse_x_forwarded_for_multiple() {
        assert_eq!(
            parse_x_forwarded_for("203.0.113.50, 70.41.3.18, 150.172.238.178"),
            Some("203.0.113.50".to_string())
        );
    }

    #[test]
    fn test_parse_x_forwarded_for_with_spaces() {
        assert_eq!(
            parse_x_forwarded_for("  192.168.1.1  ,  10.0.0.1  "),
            Some("192.168.1.1".to_string())
        );
    }

    #[test]
    fn test_parse_forwarded_simple() {
        assert_eq!(
            parse_forwarded_header("for=192.0.2.60"),
            Some("192.0.2.60".to_string())
        );
    }

    #[test]
    fn test_parse_forwarded_with_proto() {
        assert_eq!(
            parse_forwarded_header("for=192.0.2.60;proto=http;by=203.0.113.43"),
            Some("192.0.2.60".to_string())
        );
    }

    #[test]
    fn test_parse_forwarded_ipv6() {
        assert_eq!(
            parse_forwarded_header("for=\"[2001:db8::1]\""),
            Some("2001:db8::1".to_string())
        );
    }

    #[test]
    fn test_parse_forwarded_multiple() {
        assert_eq!(
            parse_forwarded_header("for=192.0.2.60, for=198.51.100.178"),
            Some("192.0.2.60".to_string())
        );
    }

    #[test]
    fn test_parse_simple_ip() {
        assert_eq!(
            parse_simple_ip_header("  192.168.1.1  "),
            Some("192.168.1.1".to_string())
        );
    }

    #[test]
    fn test_clean_forwarded_ip_with_port() {
        assert_eq!(clean_forwarded_ip("192.168.1.1:12345"), "192.168.1.1");
    }

    #[test]
    fn test_clean_forwarded_ipv6_with_brackets() {
        assert_eq!(clean_forwarded_ip("[2001:db8::1]"), "2001:db8::1");
    }
}

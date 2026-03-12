//! Kiểm tra IP client có nằm trong danh sách từ chối (denylist) hay không.
//! Hỗ trợ pattern: IP đầy đủ (10.10.10.10), prefix với wildcard (10.10.*, 10.*). Chỉ áp dụng IPv4.
//! Pattern được biên dịch một lần khi khởi tạo để hot path không allocation/parse.

use std::net::IpAddr;

/// Pattern đã biên dịch: mỗi octet là Some(byte) hoặc None (wildcard từ vị trí đó đến hết).
/// Dùng trên hot path: chỉ so sánh octet, không allocation.
type CompiledPattern = [Option<u8>; 4];

/// Danh sách pattern đã biên dịch, dùng một lần trước accept loop để tránh parse/allocate mỗi kết nối.
#[derive(Clone, Default)]
pub struct CompiledIpDenylist {
    patterns: Vec<CompiledPattern>,
}

impl CompiledIpDenylist {
    /// Biên dịch danh sách pattern từ config. Gọi một lần trước loop; pattern không hợp lệ bỏ qua.
    pub fn compile(patterns: &[String]) -> Self {
        let mut out = Vec::with_capacity(patterns.len());
        for p in patterns {
            let p = p.trim();
            if p.is_empty() {
                continue;
            }
            if let Some(compiled) = parse_pattern_to_compiled(p) {
                out.push(compiled);
            }
        }
        Self { patterns: out }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Trả về true nếu IP khớp bất kỳ pattern nào. Chỉ so sánh octet, không allocation (hot path).
    #[inline]
    pub fn matches(&self, ip: IpAddr) -> bool {
        let IpAddr::V4(ipv4) = ip else {
            return false;
        };
        let octets = ipv4.octets();
        for pat in &self.patterns {
            if match_compiled(&octets, pat) {
                return true;
            }
        }
        false
    }
}

/// Parse pattern string thành CompiledPattern. Trả về None nếu không hợp lệ.
fn parse_pattern_to_compiled(pattern: &str) -> Option<CompiledPattern> {
    let parts: Vec<&str> = pattern.split('.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let mut out: CompiledPattern = [None, None, None, None];
    for (i, part) in parts.iter().enumerate() {
        let part = part.trim();
        if part == "*" {
            return Some(out);
        }
        let Ok(byte) = part.parse::<u8>() else {
            return None;
        };
        out[i] = Some(byte);
    }
    Some(out)
}

#[inline]
fn match_compiled(octets: &[u8; 4], pat: &CompiledPattern) -> bool {
    for (i, p) in pat.iter().enumerate() {
        match p {
            Some(b) if octets.get(i).copied().unwrap_or(0) != *b => return false,
            None => return true,
            _ => {}
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn exact_match() {
        let patterns = vec!["10.10.10.10".to_string()];
        let c = CompiledIpDenylist::compile(&patterns);
        assert!(c.matches(v4(10, 10, 10, 10)));
        assert!(!c.matches(v4(10, 10, 10, 11)));
    }

    #[test]
    fn wildcard_10_star() {
        let patterns = vec!["10.*".to_string()];
        let c = CompiledIpDenylist::compile(&patterns);
        assert!(c.matches(v4(10, 0, 0, 0)));
        assert!(c.matches(v4(10, 255, 255, 255)));
        assert!(!c.matches(v4(11, 0, 0, 0)));
    }

    #[test]
    fn wildcard_10_10_star() {
        let patterns = vec!["10.10.*".to_string()];
        let c = CompiledIpDenylist::compile(&patterns);
        assert!(c.matches(v4(10, 10, 0, 0)));
        assert!(c.matches(v4(10, 10, 1, 2)));
        assert!(!c.matches(v4(10, 11, 0, 0)));
    }

    #[test]
    fn multiple_patterns() {
        let patterns = vec![
            "10.10.*".to_string(),
            "192.168.1.1".to_string(),
            "10.*".to_string(),
        ];
        let c = CompiledIpDenylist::compile(&patterns);
        assert!(c.matches(v4(10, 10, 1, 1)));
        assert!(c.matches(v4(192, 168, 1, 1)));
        assert!(c.matches(v4(10, 1, 1, 1)));
    }

    #[test]
    fn empty_list_is_empty_and_never_matches() {
        let c = CompiledIpDenylist::compile(&[]);
        assert!(c.is_empty());
        assert!(!c.matches(v4(10, 0, 0, 0)));
    }

    #[test]
    fn prefix_without_star_three_parts() {
        let patterns = vec!["10.10.10".to_string()];
        let c = CompiledIpDenylist::compile(&patterns);
        assert!(c.matches(v4(10, 10, 10, 0)));
        assert!(c.matches(v4(10, 10, 10, 255)));
        assert!(!c.matches(v4(10, 10, 11, 0)));
    }

    #[test]
    fn invalid_patterns_skipped() {
        let patterns = vec![
            "10.256.0.0".to_string(),
            "10.10.10.10.1".to_string(),
            "10.10.10.10".to_string(),
        ];
        let c = CompiledIpDenylist::compile(&patterns);
        assert!(c.matches(v4(10, 10, 10, 10)));
        assert!(!c.matches(v4(10, 10, 10, 11)));
    }

    #[test]
    fn ipv6_not_matched() {
        let patterns = vec!["10.*".to_string()];
        let c = CompiledIpDenylist::compile(&patterns);
        let v6 = "::1".parse::<IpAddr>().unwrap();
        assert!(!c.matches(v6));
    }
}

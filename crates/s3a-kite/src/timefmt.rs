//! RFC3339 seconds UTC timestamps (consent file, logs). No chrono
//! dependency: the civil conversion is Howard Hinnant's algorithm.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Format a `SystemTime` as RFC3339 seconds UTC, e.g. `2026-11-18T21:00:00Z`.
pub fn rfc3339_secs(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Parse a strict RFC3339 seconds UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`).
/// Returns `None` for any other form.
pub fn parse_rfc3339_secs(s: &str) -> Option<SystemTime> {
    let b = s.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return None;
    }
    let num = |a: usize, z: usize| -> Option<u32> {
        let mut v: u32 = 0;
        for i in a..z {
            if !b[i].is_ascii_digit() {
                return None;
            }
            v = v * 10 + (b[i] - b'0') as u32;
        }
        Some(v)
    };
    let y = num(0, 4)? as i64;
    let m = num(5, 7)?;
    let d = num(8, 10)?;
    let hh = num(11, 13)?;
    let mm = num(14, 16)?;
    let ss = num(17, 19)?;
    if !(1..=12).contains(&m) || hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    if d == 0 || d > days_in_month(y, m) {
        return None;
    }
    let days = days_from_civil(y, m, d);
    let secs = days * 86_400 + (hh as i64) * 3600 + (mm as i64) * 60 + ss as i64;
    Some(UNIX_EPOCH + Duration::from_secs(secs as u64))
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Days since 1970-01-01 from a civil date (Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + (d as u64) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

/// Civil date from days since 1970-01-01 (Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for s in [
            "1970-01-01T00:00:00Z",
            "2026-11-18T21:00:00Z",
            "2024-02-29T23:59:59Z",
            "2000-02-29T12:00:00Z",
        ] {
            let t = parse_rfc3339_secs(s).unwrap_or_else(|| panic!("parse {s}"));
            assert_eq!(rfc3339_secs(t), s, "round trip {s}");
        }
        assert!(parse_rfc3339_secs("2023-02-29T00:00:00Z").is_none());
        assert!(parse_rfc3339_secs("2026-11-18T21:00:00").is_none());
        assert!(parse_rfc3339_secs("2026-11-18T21:00:00+00:00").is_none());
        assert!(parse_rfc3339_secs("2026-11-18T24:00:00Z").is_none());
        assert!(parse_rfc3339_secs("2026-13-01T00:00:00Z").is_none());
    }
}

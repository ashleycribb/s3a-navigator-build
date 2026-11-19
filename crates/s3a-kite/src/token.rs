//! UI token file (SPEC §5.2.2, AT-45/AT-81/AT-117/AT-142) and secret-file
//! mode/owner checks. The token bytes are never logged.

use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// A single line of at least 32 characters from `A-Z a-z 0-9 _ -`
/// (the unpadded base64url alphabet), optionally ending in one `\n` or
/// `\r\n`. Any other whitespace, any other byte, an empty file, or a
/// shorter secret is a startup error that names the file (AT-45, AT-81).
pub fn parse_token_bytes(bytes: &[u8], file_display: &str) -> Result<String, String> {
    let mut body = bytes;
    if body.ends_with(b"\r\n") {
        body = &body[..body.len() - 2];
    } else if body.ends_with(b"\n") {
        body = &body[..body.len() - 1];
    }
    if body.len() < 32 {
        return Err(format!(
            "{file_display}: token is shorter than 32 characters ({} bytes)",
            body.len()
        ));
    }
    if !body
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
    {
        return Err(format!(
            "{file_display}: token contains a character outside A-Z a-z 0-9 _ -"
        ));
    }
    // The byte check above already rejects non-ASCII; the secret is UTF-8.
    Ok(String::from_utf8(body.to_vec()).expect("token is ASCII"))
}

/// Check a secret file's ownership/mode (AT-117, AT-142): an existing file
/// must be owned by the process's effective user and have mode `0600`.
/// Returns Err naming the file.
pub fn check_secret_file(path: &Path, file_display: &str) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|e| format!("{file_display}: cannot stat: {e}"))?;
    if meta.file_type().is_symlink() {
        return Err(format!("{file_display}: is a symlink"));
    }
    if !meta.is_file() {
        return Err(format!("{file_display}: is not a regular file"));
    }
    let mode = meta.mode() & 0o777;
    if mode != 0o600 {
        return Err(format!(
            "{file_display}: mode is {mode:04o}, expected 0600"
        ));
    }
    if meta.uid() != unsafe { libc::geteuid() } {
        return Err(format!(
            "{file_display}: owned by uid {}, expected euid {}",
            meta.uid(),
            unsafe { libc::geteuid() }
        ));
    }
    Ok(())
}

/// Constant-time equality for two secrets of equal length. A length
/// mismatch is false (the secret length is not itself secret; the byte
/// comparison is constant-time once lengths match).
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Is this `Authorization` header value exactly `Bearer <secret>`?
/// The comparison is constant-time on the secret.
pub fn bearer_matches(header: Option<&str>, secret: &str) -> bool {
    let Some(value) = header else {
        return false;
    };
    let Some(presented) = value.strip_prefix("Bearer ") else {
        return false;
    };
    constant_time_eq(presented.as_bytes(), secret.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_charset_and_length() {
        let good = "a".repeat(32);
        assert_eq!(
            parse_token_bytes(good.as_bytes(), "tok").unwrap(),
            good
        );
        assert_eq!(
            parse_token_bytes(format!("{good}\n").as_bytes(), "tok").unwrap(),
            good
        );
        assert_eq!(
            parse_token_bytes(format!("{good}\r\n").as_bytes(), "tok").unwrap(),
            good
        );
        assert!(parse_token_bytes(b"short", "tok").is_err());
        assert!(parse_token_bytes(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n\n", "tok").is_err());
        assert!(parse_token_bytes(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ", "tok").is_err());
        assert!(parse_token_bytes(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa!", "tok").is_err());
        assert!(parse_token_bytes(b"", "tok").is_err());
    }

    #[test]
    fn bearer_matching_is_exact() {
        let secret = "x".repeat(32);
        assert!(bearer_matches(Some(&format!("Bearer {secret}")), &secret));
        assert!(!bearer_matches(Some(&format!("Bearer {secret}\n")), &secret));
        assert!(!bearer_matches(Some(&format!("bearer {secret}")), &secret));
        assert!(!bearer_matches(Some("Bearer short"), &secret));
        assert!(!bearer_matches(None, &secret));
    }
}

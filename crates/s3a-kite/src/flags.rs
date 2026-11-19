//! Host flags (§5.2.2 inventory, DD-106) and startup (DD-127 precedence,
//! §5.2.1 steps 1–3, AT-04/AT-07/AT-45/AT-51/AT-78/AT-79/AT-87/AT-113/
//! AT-114/AT-125/AT-151/AT-163/AT-164/AT-174/AT-179/AT-187/AT-190/AT-203/
//! AT-208/AT-223/AT-224/AT-226).
//!
//! Startup error precedence (DD-127): when more than one required flag is
//! missing or invalid, the host exits non-zero, binds nothing, and names
//! **one** of the failing flags (no prescribed order). An AT that asserts
//! the name of a particular flag supplies every other flag valid, so the
//! named flag is the only error.

use std::collections::HashMap;
use std::ffi::CString;
use std::net::{IpAddr, ToSocketAddrs};
use std::os::unix::io::{FromRawFd, RawFd};
use std::path::{Path, PathBuf};

use s3a_core::consent::{actor_id_to_subject_id, ConsentPurpose, ConsentRecord};
use s3a_core::{TileType, ADDON_EDU};
use s3a_engine::TileWriter;
use rustls::pki_types::pem::PemObject;
use s3a_pack_kite::KiteContext;
use s3a_pack_scholar::xapi::hash_iri;

use crate::consent::{parse_consent_file, random_suffix, write_consent_file, ConsentState};
use crate::egress::{parse_allowlist, AllowEntry};
use crate::fonts::required_ua_fonts;
use crate::hooks::Hooks;
use crate::paths::{
    are_disjoint, canonicalize, close_fd, flock_ex_nb, fstat_fd, fstatat_nofollow,
    is_within, lexical_normalize, lstat_path, open_dir_fd, openat_child, openat_child_rw,
    read_dir_entries, read_fd_bounded,
};
use crate::timefmt::rfc3339_secs;
use crate::token::{check_secret_file, parse_token_bytes};

/// A startup error: exits non-zero, binds nothing, names the flag/path.
#[derive(Debug, Clone)]
pub struct StartupError {
    pub message: String,
    pub flag: Option<String>,
}

impl StartupError {
    pub fn msg(message: impl Into<String>) -> Self {
        StartupError {
            message: message.into(),
            flag: None,
        }
    }
    pub fn flag(flag: &str, message: impl Into<String>) -> Self {
        StartupError {
            message: message.into(),
            flag: Some(flag.to_string()),
        }
    }
    pub fn flags(flag_a: &str, flag_b: &str, why: &str) -> Self {
        StartupError {
            message: format!("{flag_a} and {flag_b}: {why}"),
            flag: Some(flag_a.to_string()),
        }
    }
}

impl std::fmt::Display for StartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.flag {
            Some(flag) => write!(f, "{flag}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for StartupError {}

impl From<String> for StartupError {
    fn from(message: String) -> Self {
        StartupError::msg(message)
    }
}

/// `--bind` value: a TCP IP literal or a Unix-socket path.
#[derive(Debug, Clone)]
pub enum Bind {
    Tcp(IpAddr),
    Unix(PathBuf),
}

impl Bind {
    /// Loopback per F-09: IPv4 `127.0.0.0/8`, IPv6 `::1`, or the
    /// IPv4-mapped form of `127.0.0.0/8`. `0.0.0.0` and `::` are not
    /// loopback. A Unix socket is loopback-like for the health/token and
    /// `--headless` rules (it is not a TCP bind).
    pub fn is_loopback(&self) -> bool {
        match self {
            Bind::Unix(_) => true,
            Bind::Tcp(ip) => is_loopback_ip(*ip),
        }
    }
    pub fn is_unix(&self) -> bool {
        matches!(self, Bind::Unix(_))
    }
}

/// F-09 loopback definition for an IP literal.
pub fn is_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.octets()[0] == 127,
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                return true;
            }
            // IPv4-mapped form of 127.0.0.0/8.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return v4.octets()[0] == 127;
            }
            false
        }
    }
}

/// A parsed `--lrs-url`.
#[derive(Debug, Clone)]
pub struct LrsUrl {
    pub https: bool,
    pub host: String,
    pub port: Option<u16>,
    /// "" or "/xapi" (a single trailing "/" is stripped).
    pub path: String,
    /// IPs the hostname resolved to at startup (empty for IP literals).
    pub resolved: Vec<IpAddr>,
}

/// The validated host configuration (all paths as given on the command
/// line; canonicalized paths live in `Started`).
#[derive(Debug, Clone)]
pub struct Config {
    pub viewer: bool,
    pub bind: Bind,
    pub port: Option<u16>,
    pub auth_token_file: PathBuf,
    pub view_root: Option<PathBuf>,
    pub log_dir: PathBuf,
    pub max_open_handles: u64,
    pub max_frame_pixels: u64,
    pub handle_idle_secs: u64,
    pub max_request_bytes: u64,
    pub max_open_bytes: u64,
    pub headless: bool,
    pub wiki_root: Option<PathBuf>,
    pub corpus_root: Option<PathBuf>,
    pub working_archive: Option<PathBuf>,
    pub snapshot_dir: Option<PathBuf>,
    pub download_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub researcher_id: Option<String>,
    pub fetch_timeout_secs: Option<u64>,
    pub max_fetch_bytes: Option<u64>,
    pub max_redirects: Option<u64>,
    pub lrs_url: Option<LrsUrl>,
    pub consent_state_file: Option<PathBuf>,
    pub lrs_credentials_file: Option<PathBuf>,
    pub study_id: Option<String>,
    pub lrs_timeout_secs: Option<u64>,
    pub pinned_keys_file: Option<PathBuf>,
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
    pub behind_proxy: bool,
    pub egress_allowlist: Option<Vec<AllowEntry>>,
    pub egress_test_loopback_port: Option<u16>,
    /// Flag names as passed, in order (for the startup log line; names
    /// only, never values).
    pub flag_names: Vec<String>,
    /// Parsed test hooks (only meaningful in a `test_hooks` build).
    pub hooks: Hooks,
}

const SWITCHES: &[&str] = &["--viewer", "--headless", "--behind-proxy"];
const VALUE_FLAGS: &[&str] = &[
    "--bind",
    "--port",
    "--auth-token-file",
    "--view-root",
    "--log-dir",
    "--max-open-handles",
    "--max-frame-pixels",
    "--handle-idle-secs",
    "--max-request-bytes",
    "--max-open-bytes",
    "--wiki-root",
    "--corpus-root",
    "--working-archive",
    "--snapshot-dir",
    "--download-dir",
    "--data-dir",
    "--researcher-id",
    "--fetch-timeout-secs",
    "--max-fetch-bytes",
    "--max-redirects",
    "--lrs-url",
    "--consent-state-file",
    "--lrs-credentials-file",
    "--study-id",
    "--lrs-timeout-secs",
    "--pinned-keys-file",
    "--tls-cert",
    "--tls-key",
    "--egress-allowlist",
    "--egress-test-loopback-port",
];

/// Parse `s3a-kite host [flags...]` argv (after the `host` subcommand).
/// Accepts `--flag value` and `--flag=value`. Unknown flags, duplicate
/// flags, a value for a switch, and a missing value are startup errors
/// naming the flag.
pub fn parse_args(argv: &[String]) -> Result<HashMap<String, Option<String>>, StartupError> {
    let mut seen: HashMap<String, Option<String>> = HashMap::new();
    let mut i = 0;
    while i < argv.len() {
        let arg = &argv[i];
        let (name, inline_value) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        if !name.starts_with("--") {
            return Err(StartupError::flag(
                &name,
                "unexpected positional argument (expected a --flag)",
            ));
        }
        if !SWITCHES.contains(&name.as_str()) && !VALUE_FLAGS.contains(&name.as_str()) {
            return Err(StartupError::flag(&name, "unknown flag"));
        }
        if seen.contains_key(&name) {
            return Err(StartupError::flag(&name, "flag passed twice"));
        }
        if SWITCHES.contains(&name.as_str()) {
            if inline_value.is_some() {
                return Err(StartupError::flag(&name, "switch takes no value"));
            }
            seen.insert(name, None);
            i += 1;
            continue;
        }
        // Value flag.
        let value = match inline_value {
            Some(v) => v,
            None => {
                i += 1;
                if i >= argv.len() {
                    return Err(StartupError::flag(&name, "missing value"));
                }
                argv[i].clone()
            }
        };
        seen.insert(name, Some(value));
        i += 1;
    }
    Ok(seen)
}

fn present(flags: &HashMap<String, Option<String>>, name: &str) -> bool {
    flags.contains_key(name)
}

fn value(flags: &HashMap<String, Option<String>>, name: &str) -> Option<String> {
    flags.get(name).and_then(|v| v.clone())
}

fn required_value(
    flags: &HashMap<String, Option<String>>,
    name: &str,
) -> Result<String, StartupError> {
    value(flags, name).ok_or_else(|| StartupError::flag(name, "required flag is missing"))
}

fn parse_int(
    flags: &HashMap<String, Option<String>>,
    name: &str,
    min: u64,
    max: u64,
) -> Result<Option<u64>, StartupError> {
    let Some(raw) = value(flags, name) else {
        return Ok(None);
    };
    let n: u64 = raw.parse().map_err(|_| {
        StartupError::flag(name, &format!("not an integer: {raw:?}"))
    })?;
    if n < min || n > max {
        return Err(StartupError::flag(
            name,
            &format!("out of range [{min}..={max}]: {n}"),
        ));
    }
    Ok(Some(n))
}

fn parse_int_required(
    flags: &HashMap<String, Option<String>>,
    name: &str,
    min: u64,
    max: u64,
) -> Result<u64, StartupError> {
    parse_int(flags, name, min, max)?.ok_or_else(|| StartupError::flag(name, "required flag is missing"))
}

/// Validate the raw flags into a `Config` (mode rules, integers,
/// cross-flag rules, `--lrs-url`, allowlist, hooks). No filesystem access
/// here except the LRS hostname DNS resolution (AT-226 startup half).
pub fn validate_config(
    flags: &HashMap<String, Option<String>>,
) -> Result<Config, StartupError> {
    // The removed flag is a startup error that names it (v24.1, DD-157).
    if present(flags, "--lrs-actor-homepage") {
        return Err(StartupError::flag(
            "--lrs-actor-homepage",
            "removed in v24.1 (the host builds the actor homepage itself)",
        ));
    }
    let viewer = present(flags, "--viewer");
    // Forbidden flags per mode (DD-106 inventory).
    let scholar_only: &[&str] = &[
        "--wiki-root",
        "--corpus-root",
        "--working-archive",
        "--snapshot-dir",
        "--download-dir",
        "--data-dir",
        "--researcher-id",
        "--fetch-timeout-secs",
        "--max-fetch-bytes",
        "--max-redirects",
        "--lrs-url",
        "--consent-state-file",
        "--lrs-credentials-file",
        "--study-id",
        "--lrs-timeout-secs",
        "--egress-allowlist",
        "--egress-test-loopback-port",
    ];
    if viewer {
        for f in scholar_only {
            if present(flags, f) {
                return Err(StartupError::flag(
                    f,
                    "forbidden in viewer-only mode",
                ));
            }
        }
    }
    // Required in both modes.
    let bind_raw = required_value(flags, "--bind")?;
    let bind = parse_bind(&bind_raw)?;
    let is_unix = bind.is_unix();
    let port = if is_unix {
        if present(flags, "--port") {
            return Err(StartupError::flag(
                "--port",
                "forbidden with a unix: bind",
            ));
        }
        None
    } else {
        Some(parse_int_required(flags, "--port", 1, 65535)? as u16)
    };
    let auth_token_file = PathBuf::from(required_value(flags, "--auth-token-file")?);
    let log_dir = PathBuf::from(required_value(flags, "--log-dir")?);
    let max_open_handles = parse_int_required(flags, "--max-open-handles", 1, u64::MAX)?;
    let max_frame_pixels = parse_int_required(flags, "--max-frame-pixels", 1, u64::MAX)?;
    let handle_idle_secs = parse_int_required(flags, "--handle-idle-secs", 1, u64::MAX)?;
    let max_request_bytes = parse_int_required(flags, "--max-request-bytes", 1, u64::MAX)?;
    let max_open_bytes = parse_int_required(flags, "--max-open-bytes", 1, u64::MAX)?;
    let view_root = value(flags, "--view-root").map(PathBuf::from);
    if viewer && view_root.is_none() {
        return Err(StartupError::flag(
            "--view-root",
            "required in viewer-only mode",
        ));
    }
    // --headless: required unless the bind is loopback or unix:; required
    // in builds without the native-window feature (AT-125).
    let headless = present(flags, "--headless");
    if !cfg!(feature = "native-window") && !headless {
        return Err(StartupError::flag(
            "--headless",
            "required in builds without the native-window feature",
        ));
    }
    if !headless && !bind.is_loopback() {
        return Err(StartupError::flag(
            "--headless",
            "required unless the bind is loopback or unix:",
        ));
    }
    // In a native-window build, a window without --headless needs a display.
    if !headless && cfg!(feature = "native-window") {
        let display = std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty())
            || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
        if !display {
            return Err(StartupError::msg(
                "DISPLAY: no DISPLAY or WAYLAND_DISPLAY is set for the native window".to_string(),
            ));
        }
    }
    // TLS rules.
    let tls_cert = value(flags, "--tls-cert").map(PathBuf::from);
    let tls_key = value(flags, "--tls-key").map(PathBuf::from);
    let non_loopback_tcp = matches!(bind, Bind::Tcp(_)) && !bind.is_loopback();
    if non_loopback_tcp {
        if tls_cert.is_none() {
            return Err(StartupError::flag(
                "--tls-cert",
                "required with a non-loopback bind",
            ));
        }
        if tls_key.is_none() {
            return Err(StartupError::flag(
                "--tls-key",
                "required with a non-loopback bind",
            ));
        }
    } else if tls_cert.is_some() {
        return Err(StartupError::flag(
            "--tls-cert",
            "forbidden with a loopback or unix: bind (those binds never use TLS)",
        ));
    } else if tls_key.is_some() {
        return Err(StartupError::flag(
            "--tls-key",
            "forbidden with a loopback or unix: bind (those binds never use TLS)",
        ));
    }
    let behind_proxy = present(flags, "--behind-proxy");
    if behind_proxy && non_loopback_tcp {
        return Err(StartupError::flag(
            "--behind-proxy",
            "accepted only with a loopback or unix: bind",
        ));
    }
    // Scholar-only required flags.
    let (
        wiki_root,
        corpus_root,
        working_archive,
        snapshot_dir,
        download_dir,
        data_dir,
        researcher_id,
        fetch_timeout_secs,
        max_fetch_bytes,
        max_redirects,
    ) = if viewer {
        (None, None, None, None, None, None, None, None, None, None)
    } else {
        (
            Some(PathBuf::from(required_value(flags, "--wiki-root")?)),
            Some(PathBuf::from(required_value(flags, "--corpus-root")?)),
            Some(PathBuf::from(required_value(flags, "--working-archive")?)),
            Some(PathBuf::from(required_value(flags, "--snapshot-dir")?)),
            Some(PathBuf::from(required_value(flags, "--download-dir")?)),
            Some(PathBuf::from(required_value(flags, "--data-dir")?)),
            Some(required_value(flags, "--researcher-id")?),
            Some(parse_int_required(flags, "--fetch-timeout-secs", 1, u64::MAX)?),
            Some(parse_int_required(flags, "--max-fetch-bytes", 1, u64::MAX)?),
            Some(parse_int_required(flags, "--max-redirects", 0, u64::MAX)?),
        )
    };
    if let Some(rid) = &researcher_id {
        if rid.is_empty() {
            return Err(StartupError::flag(
                "--researcher-id",
                "must be a non-empty string",
            ));
        }
    }
    // --max-fetch-bytes > --max-open-bytes is a startup error naming both
    // flags (AT-223).
    if let (Some(fetch), Some(open)) = (max_fetch_bytes, Some(max_open_bytes)) {
        if fetch > open {
            return Err(StartupError::flags(
                "--max-fetch-bytes",
                "--max-open-bytes",
                "--max-fetch-bytes is greater than --max-open-bytes",
            ));
        }
    }
    // LRS URL (Scholar only, optional).
    let lrs_url_raw = value(flags, "--lrs-url");
    let lrs_url = match lrs_url_raw {
        Some(raw) => Some(parse_lrs_url(&raw)?),
        None => None,
    };
    let lrs_set = lrs_url.is_some();
    let consent_state_file = value(flags, "--consent-state-file").map(PathBuf::from);
    let lrs_credentials_file = value(flags, "--lrs-credentials-file").map(PathBuf::from);
    let lrs_timeout_secs = parse_int(flags, "--lrs-timeout-secs", 1, u64::MAX)?;
    if lrs_set {
        if consent_state_file.is_none() {
            return Err(StartupError::flag(
                "--consent-state-file",
                "required when --lrs-url is set",
            ));
        }
        if lrs_credentials_file.is_none() {
            return Err(StartupError::flag(
                "--lrs-credentials-file",
                "required when --lrs-url is set",
            ));
        }
        if lrs_timeout_secs.is_none() {
            return Err(StartupError::flag(
                "--lrs-timeout-secs",
                "required when --lrs-url is set",
            ));
        }
    } else {
        if consent_state_file.is_some() {
            return Err(StartupError::flag(
                "--consent-state-file",
                "forbidden when --lrs-url is absent",
            ));
        }
        if lrs_credentials_file.is_some() {
            return Err(StartupError::flag(
                "--lrs-credentials-file",
                "forbidden when --lrs-url is absent",
            ));
        }
        if lrs_timeout_secs.is_some() {
            return Err(StartupError::flag(
                "--lrs-timeout-secs",
                "forbidden when --lrs-url is absent",
            ));
        }
    }
    let study_id = value(flags, "--study-id");
    if let Some(s) = &study_id {
        if s.is_empty() {
            return Err(StartupError::flag(
                "--study-id",
                "must be a non-empty string",
            ));
        }
    }
    let pinned_keys_file = value(flags, "--pinned-keys-file").map(PathBuf::from);
    let egress_allowlist = match value(flags, "--egress-allowlist") {
        Some(raw) => Some(parse_allowlist(&raw).map_err(StartupError::msg)?),
        None => None,
    };
    let egress_test_loopback_port =
        parse_int(flags, "--egress-test-loopback-port", 1, 65535)?.map(|n| n as u16);
    if egress_test_loopback_port.is_some() && !cfg!(feature = "egress_test") {
        return Err(StartupError::flag(
            "--egress-test-loopback-port",
            "startup error unless the binary was built with Cargo feature egress_test",
        ));
    }
    // Test hooks (feature-gated; §5.2.7, AT-222).
    let hooks = Hooks::from_env().map_err(StartupError::msg)?;
    let mut flag_names: Vec<String> = flags.keys().cloned().collect();
    flag_names.sort();
    Ok(Config {
        viewer,
        bind,
        port,
        auth_token_file,
        view_root,
        log_dir,
        max_open_handles,
        max_frame_pixels,
        handle_idle_secs,
        max_request_bytes,
        max_open_bytes,
        headless,
        wiki_root,
        corpus_root,
        working_archive,
        snapshot_dir,
        download_dir,
        data_dir,
        researcher_id,
        fetch_timeout_secs,
        max_fetch_bytes,
        max_redirects,
        lrs_url,
        consent_state_file,
        lrs_credentials_file,
        study_id,
        lrs_timeout_secs,
        pinned_keys_file,
        tls_cert,
        tls_key,
        behind_proxy,
        egress_allowlist,
        egress_test_loopback_port,
        flag_names,
        hooks,
    })
}

fn parse_bind(raw: &str) -> Result<Bind, StartupError> {
    if let Some(rest) = raw.strip_prefix("unix:") {
        let path = PathBuf::from(rest);
        if !path.is_absolute() {
            return Err(StartupError::flag(
                "--bind",
                "unix: path must be absolute",
            ));
        }
        return Ok(Bind::Unix(path));
    }
    let ip: IpAddr = raw.parse().map_err(|_| {
        StartupError::flag("--bind", &format!("not an IP literal: {raw:?}"))
    })?;
    Ok(Bind::Tcp(ip))
}

/// Parse and validate `--lrs-url`: `scheme://host[:port]` with an optional
/// path of `/xapi` and an optional single trailing `/` stripped; `https`,
/// or `http` on a loopback IP literal. An `https` hostname that fails to
/// resolve at start is a startup error naming `--lrs-url` (AT-226).
fn parse_lrs_url(raw: &str) -> Result<LrsUrl, StartupError> {
    let err = |why: &str| StartupError::flag("--lrs-url", why);
    let (scheme, rest) = match raw.split_once("://") {
        Some((s, r)) => (s, r),
        None => return Err(err("expected scheme://host[:port][/xapi]")),
    };
    if scheme != "https" && scheme != "http" {
        return Err(err("scheme must be https, or http on a loopback IP literal"));
    }
    if rest.contains('@') || rest.contains('?') || rest.contains('#') {
        return Err(err("userinfo, query, and fragment are not allowed"));
    }
    let (authority, path) = match rest.split_once('/') {
        Some((a, p)) => (a, format!("/{p}")),
        None => (rest, String::new()),
    };
    let path = path.strip_suffix('/').unwrap_or(&path).to_string();
    if path != "" && path != "/xapi" {
        return Err(err("the only allowed path is empty, /, /xapi, or /xapi/"));
    }
    // Authority: [ipv6]:port | host:port | host | [ipv6].
    let (host, port): (String, Option<u16>) = if let Some(rest) = authority.strip_prefix('[') {
        let (h, p) = match rest.split_once(']') {
            Some((h, "")) => (h.to_string(), None),
            Some((h, p)) => {
                let p = p.strip_prefix(':').ok_or_else(|| err("bad [ipv6]:port"))?;
                (h.to_string(), Some(parse_port_str(p).ok_or_else(|| err("bad port"))?))
            }
            None => return Err(err("unclosed [ipv6]")),
        };
        (h, p)
    } else if let Some((h, p)) = authority.rsplit_once(':') {
        // A bare IPv6 literal without brackets has more than one colon.
        if h.contains(':') {
            (authority.to_string(), None)
        } else {
            (h.to_string(), Some(parse_port_str(p).ok_or_else(|| err("bad port"))?))
        }
    } else {
        (authority.to_string(), None)
    };
    if host.is_empty() {
        return Err(err("empty host"));
    }
    let https = scheme == "https";
    let mut resolved: Vec<IpAddr> = Vec::new();
    if let Ok(ip) = host.parse::<IpAddr>() {
        // An IP literal: http is allowed only on a loopback literal.
        if !https && !is_loopback_ip(ip) {
            return Err(err("http is allowed only on a loopback IP literal"));
        }
        resolved.push(ip);
    } else {
        // A hostname: http is never allowed (including localhost).
        if !https {
            return Err(err(
                "http with a hostname (including localhost) is not allowed",
            ));
        }
        let lookup_port = port.unwrap_or(443);
        let addrs: Vec<IpAddr> = (host.as_str(), lookup_port)
            .to_socket_addrs()
            .map_err(|_| err("https hostname failed to resolve at startup"))?
            .map(|s| s.ip())
            .collect();
        if addrs.is_empty() {
            return Err(err("https hostname failed to resolve at startup"));
        }
        resolved = addrs;
    }
    Ok(LrsUrl {
        https,
        host,
        port,
        path,
        resolved,
    })
}

fn parse_port_str(s: &str) -> Option<u16> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u32 = s.parse().ok()?;
    if n == 0 || n > 65535 {
        return None;
    }
    Some(n as u16)
}

/// Canonicalize a required directory flag (must exist as a directory).
pub fn canonical_dir(flag: &str, path: &Path) -> Result<PathBuf, StartupError> {
    let canon = canonicalize(path)
        .map_err(|e| StartupError::flag(flag, &format!("cannot canonicalize: {e}")))?;
    let meta = std::fs::metadata(&canon)
        .map_err(|e| StartupError::flag(flag, &format!("cannot stat: {e}")))?;
    if !meta.is_dir() {
        return Err(StartupError::flag(flag, "is not a directory"));
    }
    Ok(canon)
}

/// Canonicalize a required existing file flag (must exist as a regular
/// file, not a symlink).
pub fn canonical_file(flag: &str, path: &Path) -> Result<PathBuf, StartupError> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|e| StartupError::flag(flag, &format!("cannot stat: {e}")))?;
    if meta.file_type().is_symlink() {
        return Err(StartupError::flag(flag, "is a symlink"));
    }
    if !meta.is_file() {
        return Err(StartupError::flag(flag, "is not a regular file"));
    }
    canonicalize(path).map_err(|e| StartupError::flag(flag, &format!("cannot canonicalize: {e}")))
}

/// Split a file flag into (canonical parent dir, basename). The parent
/// must exist as a directory; the file itself may be missing.
pub fn canonical_parent(flag: &str, path: &Path) -> Result<(PathBuf, String), StartupError> {
    let parent = path.parent().ok_or_else(|| {
        StartupError::flag(flag, "has no parent directory")
    })?;
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let canon_parent = canonical_dir(flag, parent)?;
    let basename = path
        .file_name()
        .ok_or_else(|| StartupError::flag(flag, "has no file name"))?
        .to_string_lossy()
        .into_owned();
    if basename.is_empty() || basename.contains('\0') {
        return Err(StartupError::flag(flag, "bad file name"));
    }
    Ok((canon_parent, basename))
}

/// Disjointness helper: error naming both flags when the two paths are not
/// disjoint (one is equal to, or an ancestor of, the other).
pub fn require_disjoint(
    flag_a: &str,
    a: &Path,
    flag_b: &str,
    b: &Path,
) -> Result<(), StartupError> {
    if !are_disjoint(a, b) {
        return Err(StartupError::flags(
            flag_a,
            flag_b,
            "paths are not disjoint (one is equal to, or an ancestor of, the other)",
        ));
    }
    Ok(())
}

/// Not-inside helper: error naming both flags when `path` is inside `root`.
pub fn require_not_inside(
    flag: &str,
    path: &Path,
    root_flag: &str,
    root: &Path,
) -> Result<(), StartupError> {
    if is_within(root, path) {
        return Err(StartupError::flags(
            flag,
            root_flag,
            "path is inside an allowed request root",
        ));
    }
    Ok(())
}

/// Lexical-normalize a request path for the DD-89 root match (used by the
/// server; kept here so the rule is shared).
pub fn normalize_request_path(path: &Path) -> Option<PathBuf> {
    lexical_normalize(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(pairs: &[(&str, &str)]) -> HashMap<String, Option<String>> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Some(v.to_string())))
            .collect()
    }

    #[test]
    fn loopback_definition() {
        assert!(is_loopback_ip("127.0.0.1".parse().unwrap()));
        assert!(is_loopback_ip("127.0.0.2".parse().unwrap()));
        assert!(is_loopback_ip("127.53.0.1".parse().unwrap()));
        assert!(is_loopback_ip("::1".parse().unwrap()));
        assert!(is_loopback_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_loopback_ip("0.0.0.0".parse().unwrap()));
        assert!(!is_loopback_ip("::".parse().unwrap()));
        assert!(!is_loopback_ip("192.0.2.10".parse().unwrap()));
        assert!(!is_loopback_ip("::ffff:192.0.2.10".parse().unwrap()));
    }

    #[test]
    fn unknown_flag_is_error() {
        let f = flags(&[("--bogus", "1")]);
        assert!(parse_args(&["--bogus".to_string(), "1".to_string()]).is_err());
        let _ = f;
    }

    #[test]
    fn lrs_url_validation() {
        // http on a loopback literal is allowed.
        let u = parse_lrs_url("http://127.0.0.1:8080/xapi").unwrap();
        assert!(!u.https);
        assert_eq!(u.path, "/xapi");
        // http on a hostname is refused.
        assert!(parse_lrs_url("http://localhost:8080/xapi").is_err());
        assert!(parse_lrs_url("http://example.com/xapi").is_err());
        // http on a non-loopback IP is refused.
        assert!(parse_lrs_url("http://192.0.2.10/xapi").is_err());
        // Bad paths / userinfo / query.
        assert!(parse_lrs_url("https://example.com/other").is_err());
        assert!(parse_lrs_url("https://user@example.com/xapi").is_err());
        assert!(parse_lrs_url("https://example.com/xapi?x=1").is_err());
        // A trailing slash is stripped.
        let u = parse_lrs_url("https://example.com/xapi/").unwrap();
        assert_eq!(u.path, "/xapi");
        // An unresolvable https hostname is a startup error. This
        // sandbox's DNS is a wildcard (every name resolves to a
        // 198.18.x.x address), so the unresolvable case uses an
        // over-long hostname the resolver rejects.
        let long_host = "a".repeat(300);
        assert!(parse_lrs_url(&format!("https://{long_host}.invalid/xapi")).is_err());
    }
}

/// Canonicalized paths (all symlink-free, all under the startup dirfd's
/// view where relevant).
#[derive(Debug, Clone, Default)]
pub struct Canon {
    pub view_root: Option<PathBuf>,
    pub wiki_root: Option<PathBuf>,
    pub corpus_root: Option<PathBuf>,
    pub snapshot_dir: Option<PathBuf>,
    pub download_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub log_dir: PathBuf,
    pub auth_token_file: PathBuf,
    pub pinned_keys_file: Option<PathBuf>,
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
    pub lrs_credentials_file: Option<PathBuf>,
    pub consent_state_file: Option<PathBuf>,
    pub consent_basename: Option<String>,
    pub working_archive: Option<PathBuf>,
    pub working_archive_parent: Option<PathBuf>,
    pub working_archive_basename: Option<String>,
}

/// The dirfds the host holds for the process lifetime.
pub struct DirFds {
    pub view_root: Option<RawFd>,
    pub wiki_root: Option<RawFd>,
    pub corpus_root: Option<RawFd>,
    pub snapshot_dir: Option<RawFd>,
    pub download_dir: Option<RawFd>,
    pub data_dir: Option<RawFd>,
    pub log_dir: RawFd,
    pub working_archive_parent: Option<RawFd>,
    pub consent_parent: Option<RawFd>,
}

impl DirFds {
    fn close_all(&self) {
        for fd in [
            self.view_root,
            self.wiki_root,
            self.corpus_root,
            self.snapshot_dir,
            self.download_dir,
            self.data_dir,
            self.working_archive_parent,
            self.consent_parent,
        ]
        .into_iter()
        .flatten()
        {
            close_fd(fd);
        }
        close_fd(self.log_dir);
    }
}

/// The listener the host accepted connections on.
pub enum Listener {
    Tcp(std::net::TcpListener),
    Unix(std::os::unix::net::UnixListener),
}

/// Everything the serve loop needs. Built once by `startup`.
pub struct Started {
    pub config: Config,
    pub canon: Canon,
    pub dirfds: DirFds,
    pub lock_fds: Vec<RawFd>,
    pub ephemeral_marker_fd: RawFd,
    pub ephemeral_temp: PathBuf,
    pub token_secret: String,
    pub pinned_keys: Vec<[u8; 32]>,
    pub tls_config: Option<std::sync::Arc<rustls::ServerConfig>>,
    pub consent: ConsentState,
    pub working_archive_identity: Option<(u64, u64)>,
    pub kite: KiteContext,
    pub log_file: std::sync::Arc<std::sync::Mutex<std::fs::File>>,
    pub listener: Listener,
}

impl Drop for Started {
    fn drop(&mut self) {
        for fd in self.lock_fds.drain(..) {
            close_fd(fd);
        }
        close_fd(self.ephemeral_marker_fd);
        self.dirfds.close_all();
    }
}

/// Parse argv and run the full startup sequence (§5.2.1/§5.2.2):
/// (1) canonicalize + lstat every path + DD-93/DD-129 checks + lstat each
/// sidecar lock name — binds nothing, mutates nothing, creates nothing;
/// (2) take the sidecar locks; (3) other reads, cleanup, bootstrap, bind.
pub fn startup(argv: &[String]) -> Result<Started, StartupError> {
    let raw = parse_args(argv)?;
    let config = validate_config(&raw)?;
    startup_checked(config)
}

fn startup_checked(config: Config) -> Result<Started, StartupError> {
    // ---- Step 1: canonicalize + lstat + checks (mutates nothing). ----
    let canon = canonicalize_all(&config)?;
    check_disjointness(&config, &canon)?;
    check_reserved_and_distinct(&config, &canon)?;
    check_secrets_not_inside_roots(&config, &canon)?;
    check_sidecar_lock_names(&canon)?;
    check_unix_socket_parent(&config)?;
    // ---- Secret reads (still no mutation). ----
    let token_secret = load_token(&config, &canon)?;
    let pinned_keys = load_pinned_keys(&config, &canon)?;
    let tls_config = load_tls(&config, &canon)?;
    check_lrs_credentials(&config, &canon)?;
    check_consent_file_mode(&canon)?;
    // ---- Open the dirfds. ----
    let dirfds = open_dirfds(&config, &canon)?;
    // ---- Step 2: take the sidecar locks. ----
    let lock_fds = take_sidecar_locks(&config, &canon, &dirfds)?;
    // ---- Step 3: bootstrap. ----
    let consent = bootstrap_consent(&config, &canon, &dirfds)?;
    let working_archive_identity = bootstrap_working_archive(&config, &canon, &dirfds)?;
    bootstrap_inclusion_index(&config, &canon, &dirfds)?;
    cleanup_startup_temps(&config, &canon, &dirfds)?;
    let (ephemeral_temp, ephemeral_marker_fd) = setup_ephemeral_temp()?;
    let log_file = open_log(&config, &canon, &dirfds)?;
    let kite = KiteContext::new(required_ua_fonts())
        .map_err(|e| StartupError::msg(format!("KiteContext::new: {}: {}", e.code(), e.message())))?;
    log_startup(&log_file, &config);
    // ---- Bind. ----
    let listener = bind_listener(&config)?;
    Ok(Started {
        config,
        canon,
        dirfds,
        lock_fds,
        ephemeral_marker_fd,
        ephemeral_temp,
        token_secret,
        pinned_keys,
        tls_config,
        consent,
        working_archive_identity,
        kite,
        log_file,
        listener,
    })
}

fn canonicalize_all(config: &Config) -> Result<Canon, StartupError> {
    let mut canon = Canon {
        log_dir: canonical_dir("--log-dir", &config.log_dir)?,
        auth_token_file: canonical_file("--auth-token-file", &config.auth_token_file)?,
        ..Canon::default()
    };
    if let Some(v) = &config.view_root {
        canon.view_root = Some(canonical_dir("--view-root", v)?);
    }
    if let Some(w) = &config.wiki_root {
        canon.wiki_root = Some(canonical_dir("--wiki-root", w)?);
    }
    if let Some(c) = &config.corpus_root {
        canon.corpus_root = Some(canonical_dir("--corpus-root", c)?);
    }
    if let Some(s) = &config.snapshot_dir {
        canon.snapshot_dir = Some(canonical_dir("--snapshot-dir", s)?);
    }
    if let Some(d) = &config.download_dir {
        canon.download_dir = Some(canonical_dir("--download-dir", d)?);
    }
    if let Some(d) = &config.data_dir {
        canon.data_dir = Some(canonical_dir("--data-dir", d)?);
    }
    if let Some(p) = &config.pinned_keys_file {
        canon.pinned_keys_file = Some(canonical_file("--pinned-keys-file", p)?);
    }
    if let Some(c) = &config.tls_cert {
        canon.tls_cert = Some(canonical_file("--tls-cert", c)?);
    }
    if let Some(k) = &config.tls_key {
        canon.tls_key = Some(canonical_file("--tls-key", k)?);
    }
    if let Some(c) = &config.lrs_credentials_file {
        canon.lrs_credentials_file = Some(canonical_file("--lrs-credentials-file", c)?);
    }
    if let Some(f) = &config.consent_state_file {
        let (parent, basename) = canonical_parent("--consent-state-file", f)?;
        let joined = parent.join(&basename);
        if let Ok(st) = lstat_path(&joined) {
            if st.is_symlink {
                return Err(StartupError::flag(
                    "--consent-state-file",
                    format!("{} is a symlink", joined.display()),
                ));
            }
            if !st.is_regular {
                return Err(StartupError::flag(
                    "--consent-state-file",
                    format!("{} is not a regular file", joined.display()),
                ));
            }
        }
        canon.consent_state_file = Some(joined);
        canon.consent_basename = Some(basename);
    }
    if let Some(a) = &config.working_archive {
        let (parent, basename) = canonical_parent("--working-archive", a)?;
        let joined = parent.join(&basename);
        if let Ok(st) = lstat_path(&joined) {
            if st.is_symlink {
                return Err(StartupError::flag(
                    "--working-archive",
                    format!("{} is a symlink", joined.display()),
                ));
            }
            if !st.is_regular {
                return Err(StartupError::flag(
                    "--working-archive",
                    format!("{} is not a regular file", joined.display()),
                ));
            }
        }
        canon.working_archive = Some(joined);
        canon.working_archive_parent = Some(parent);
        canon.working_archive_basename = Some(basename);
    }
    Ok(canon)
}

/// DD-93/DD-129 disjointness: no write directory may overlap a read root
/// (or another write directory it must not contain).
fn check_disjointness(config: &Config, canon: &Canon) -> Result<(), StartupError> {
    let mut read_roots: Vec<(&str, &PathBuf)> = Vec::new();
    if let Some(v) = &canon.view_root {
        read_roots.push(("--view-root", v));
    }
    if let Some(w) = &canon.wiki_root {
        read_roots.push(("--wiki-root", w));
    }
    if let Some(c) = &canon.corpus_root {
        read_roots.push(("--corpus-root", c));
    }
    if let Some(s) = &canon.snapshot_dir {
        read_roots.push(("--snapshot-dir", s));
    }
    if let Some(d) = &canon.download_dir {
        read_roots.push(("--download-dir", d));
    }
    let mut write_dirs: Vec<(&str, &PathBuf)> = vec![("--log-dir", &canon.log_dir)];
    if let Some(d) = &canon.data_dir {
        write_dirs.push(("--data-dir", d));
    }
    if let Some(p) = &canon.working_archive_parent {
        write_dirs.push(("--working-archive", p));
    }
    // Each write dir must be disjoint from every read root.
    for (wf, wd) in &write_dirs {
        for (rf, rd) in &read_roots {
            require_disjoint(wf, wd, rf, rd)?;
        }
    }
    // Write dirs must be pairwise disjoint (except a dir with itself).
    for i in 0..write_dirs.len() {
        for j in (i + 1)..write_dirs.len() {
            let (wf, wd) = write_dirs[i];
            let (rf, rd) = write_dirs[j];
            require_disjoint(wf, wd, rf, rd)?;
        }
    }
    // Read roots must be pairwise disjoint (wiki vs corpus vs view vs
    // snapshot vs download).
    for i in 0..read_roots.len() {
        for j in (i + 1)..read_roots.len() {
            let (wf, wd) = read_roots[i];
            let (rf, rd) = read_roots[j];
            require_disjoint(wf, wd, rf, rd)?;
        }
    }
    let _ = config;
    Ok(())
}

/// Reserved names + pairwise-distinct paths (§5.2.2).
fn check_reserved_and_distinct(config: &Config, canon: &Canon) -> Result<(), StartupError> {
    // (canonical path, owning flag) of every reserved name.
    let mut reserved: Vec<(PathBuf, &str)> = Vec::new();
    if let Some(d) = &canon.data_dir {
        reserved.push((d.join(".lock"), "--data-dir"));
        reserved.push((d.join("inclusion-index.jsonl"), "--data-dir"));
        reserved.push((d.join("revoke-pending"), "--data-dir"));
        reserved.push((d.join("lrs-retry"), "--data-dir"));
    }
    if let Some(p) = &canon.working_archive_parent {
        if let Some(b) = &canon.working_archive_basename {
            reserved.push((p.join(format!("{b}.lock")), "--working-archive"));
        }
    }
    if let Some(s) = &canon.snapshot_dir {
        reserved.push((s.join(".lock"), "--snapshot-dir"));
    }
    if let Some(d) = &canon.download_dir {
        reserved.push((d.join(".lock"), "--download-dir"));
    }
    if let Some(f) = &canon.consent_state_file {
        if let Some(parent) = f.parent() {
            if let Some(b) = &canon.consent_basename {
                reserved.push((parent.join(format!("{b}.lock")), "--consent-state-file"));
            }
        }
    }
    // A configurable file must not be a reserved name, and its basename
    // must not match s3a-kite-tmp-*.
    let mut files: Vec<(&str, &PathBuf)> = vec![("--auth-token-file", &canon.auth_token_file)];
    if let Some(p) = &canon.pinned_keys_file {
        files.push(("--pinned-keys-file", p));
    }
    if let Some(c) = &canon.tls_cert {
        files.push(("--tls-cert", c));
    }
    if let Some(k) = &canon.tls_key {
        files.push(("--tls-key", k));
    }
    if let Some(c) = &canon.lrs_credentials_file {
        files.push(("--lrs-credentials-file", c));
    }
    if let Some(f) = &canon.consent_state_file {
        files.push(("--consent-state-file", f));
    }
    if let Some(a) = &canon.working_archive {
        files.push(("--working-archive", a));
    }
    for (flag, path) in &files {
        for (reserved_path, owner_flag) in &reserved {
            if *path == reserved_path || is_within(reserved_path, *path) {
                return Err(StartupError::flag(
                    flag,
                    format!(
                        "{flag} ({}) is a reserved name owned by {owner_flag} ({})",
                        path.display(),
                        reserved_path.display()
                    ),
                ));
            }
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("s3a-kite-tmp-") {
                return Err(StartupError::flag(
                    flag,
                    format!("{flag} basename {name:?} matches the reserved s3a-kite-tmp-* prefix"),
                ));
            }
        }
    }
    // Pairwise distinct: no two configurable paths may be the same file.
    for i in 0..files.len() {
        for j in (i + 1)..files.len() {
            if files[i].1 == files[j].1 {
                return Err(StartupError::flag(
                    files[i].0,
                    format!(
                        "{} and {} name the same file ({})",
                        files[i].0,
                        files[j].0,
                        files[i].1.display()
                    ),
                ));
            }
        }
    }
    let _ = config;
    Ok(())
}

/// DD-89: secret/state files must not be inside any allowed request root.
fn check_secrets_not_inside_roots(config: &Config, canon: &Canon) -> Result<(), StartupError> {
    let mut roots: Vec<(&str, &PathBuf)> = Vec::new();
    if let Some(v) = &canon.view_root {
        roots.push(("--view-root", v));
    }
    if let Some(w) = &canon.wiki_root {
        roots.push(("--wiki-root", w));
    }
    if let Some(c) = &canon.corpus_root {
        roots.push(("--corpus-root", c));
    }
    if let Some(s) = &canon.snapshot_dir {
        roots.push(("--snapshot-dir", s));
    }
    if let Some(d) = &canon.download_dir {
        roots.push(("--download-dir", d));
    }
    let mut files: Vec<(&str, &PathBuf)> = vec![("--auth-token-file", &canon.auth_token_file)];
    if let Some(p) = &canon.pinned_keys_file {
        files.push(("--pinned-keys-file", p));
    }
    if let Some(c) = &canon.tls_cert {
        files.push(("--tls-cert", c));
    }
    if let Some(k) = &canon.tls_key {
        files.push(("--tls-key", k));
    }
    if let Some(c) = &canon.lrs_credentials_file {
        files.push(("--lrs-credentials-file", c));
    }
    if let Some(f) = &canon.consent_state_file {
        files.push(("--consent-state-file", f));
    }
    for (flag, path) in &files {
        for (rflag, root) in &roots {
            if is_within(root, path) {
                return Err(StartupError::flag(
                    flag,
                    format!(
                        "{flag} ({}) is inside {rflag} ({})",
                        path.display(),
                        root.display()
                    ),
                ));
            }
        }
    }
    let _ = config;
    Ok(())
}

/// Step 1 lstat of each sidecar lock name: a present non-regular entry is
/// a startup error naming the path.
fn check_sidecar_lock_names(canon: &Canon) -> Result<(), StartupError> {
    let mut locks: Vec<PathBuf> = Vec::new();
    if let Some(d) = &canon.data_dir {
        locks.push(d.join(".lock"));
    }
    if let Some(p) = &canon.working_archive_parent {
        if let Some(b) = &canon.working_archive_basename {
            locks.push(p.join(format!("{b}.lock")));
        }
    }
    if let Some(s) = &canon.snapshot_dir {
        locks.push(s.join(".lock"));
    }
    if let Some(d) = &canon.download_dir {
        locks.push(d.join(".lock"));
    }
    if let Some(f) = &canon.consent_state_file {
        if let Some(parent) = f.parent() {
            if let Some(b) = &canon.consent_basename {
                locks.push(parent.join(format!("{b}.lock")));
            }
        }
    }
    for lock in &locks {
        if let Ok(st) = lstat_path(lock) {
            if !st.is_regular {
                return Err(StartupError::msg(format!(
                    "sidecar lock {} exists but is not a regular file",
                    lock.display()
                )));
            }
        }
    }
    Ok(())
}

/// AT-174/AT-163: the Unix-socket parent must exist, be euid-owned, and
/// not group/world-writable; no file may already exist at the socket path.
fn check_unix_socket_parent(config: &Config) -> Result<(), StartupError> {
    if let Bind::Unix(path) = &config.bind {
        let parent = path.parent().ok_or_else(|| {
            StartupError::flag("--bind", format!("unix socket path {} has no parent", path.display()))
        })?;
        let st = lstat_path(parent).map_err(|e| {
            StartupError::flag(
                "--bind",
                format!("unix socket parent {} cannot be stat'ed: {e}", parent.display()),
            )
        })?;
        if !st.is_dir {
            return Err(StartupError::flag(
                "--bind",
                format!("unix socket parent {} is not a directory", parent.display()),
            ));
        }
        if st.uid != unsafe { libc::geteuid() } {
            return Err(StartupError::flag(
                "--bind",
                format!("unix socket parent {} is not owned by the effective uid", parent.display()),
            ));
        }
        if st.mode & 0o022 != 0 {
            return Err(StartupError::flag(
                "--bind",
                format!(
                    "unix socket parent {} is group/world-writable (mode {:o})",
                    parent.display(),
                    st.mode & 0o777
                ),
            ));
        }
        if lstat_path(path).is_ok() {
            return Err(StartupError::flag(
                "--bind",
                format!("a file already exists at unix socket path {}", path.display()),
            ));
        }
    }
    Ok(())
}

/// Read a secret file through its parent dirfd, bounded by --max-open-bytes.
fn read_via_parent(dirfd: RawFd, basename: &str, cap: u64, flag: &str) -> Result<Vec<u8>, StartupError> {
    let fd = openat_child(dirfd, basename, false)
        .map_err(|e| StartupError::flag(flag, format!("cannot open {basename}: {e}")))?;
    let st = fstat_fd(fd)
        .map_err(|e| StartupError::flag(flag, format!("cannot fstat {basename}: {e}")))?;
    if !st.is_regular {
        close_fd(fd);
        return Err(StartupError::flag(flag, format!("{basename} is not a regular file")));
    }
    let bytes = read_fd_bounded(fd, cap)
        .map_err(|e| StartupError::flag(flag, format!("cannot read {basename}: {e}")))?;
    close_fd(fd);
    Ok(bytes)
}

/// Open the parent dirfd of a canonical file path.
fn parent_dirfd(path: &Path) -> Result<(RawFd, String), StartupError> {
    let parent = path.parent().ok_or_else(|| {
        StartupError::msg(format!("{} has no parent", path.display()))
    })?;
    let fd = open_dir_fd(parent)
        .map_err(|e| StartupError::msg(format!("cannot open dirfd for {}: {e}", parent.display())))?;
    let basename = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| StartupError::msg(format!("{} has no basename", path.display())))?
        .to_string();
    Ok((fd, basename))
}

fn load_token(config: &Config, canon: &Canon) -> Result<String, StartupError> {
    check_secret_file(&canon.auth_token_file, "--auth-token-file")?;
    let (parent_fd, basename) = parent_dirfd(&canon.auth_token_file)?;
    let bytes = read_via_parent(parent_fd, &basename, config.max_open_bytes, "--auth-token-file")?;
    close_fd(parent_fd);
    parse_token_bytes(&bytes, "--auth-token-file")
        .map_err(|e| StartupError::flag("--auth-token-file", e))
}

fn load_pinned_keys(config: &Config, canon: &Canon) -> Result<Vec<[u8; 32]>, StartupError> {
    let Some(path) = &canon.pinned_keys_file else {
        return Ok(Vec::new());
    };
    check_secret_file(path, "--pinned-keys-file")?;
    let (parent_fd, basename) = parent_dirfd(path)?;
    let bytes = read_via_parent(parent_fd, &basename, config.max_open_bytes, "--pinned-keys-file")?;
    close_fd(parent_fd);
    let text = String::from_utf8(bytes).map_err(|_| {
        StartupError::flag("--pinned-keys-file", "pinned-keys file is not UTF-8")
    })?;
    let mut keys = Vec::new();
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.len() != 64 || !line.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(StartupError::flag(
                "--pinned-keys-file",
                format!("pinned-keys line {line:?} is not exactly 64 hex characters"),
            ));
        }
        let mut key = [0u8; 32];
        for i in 0..32 {
            key[i] = u8::from_str_radix(&line[2 * i..2 * i + 2], 16).unwrap();
        }
        keys.push(key);
    }
    Ok(keys)
}

fn load_tls(config: &Config, canon: &Canon) -> Result<Option<std::sync::Arc<rustls::ServerConfig>>, StartupError> {
    let (Some(cert_path), Some(key_path)) = (&canon.tls_cert, &canon.tls_key) else {
        return Ok(None);
    };
    check_secret_file(cert_path, "--tls-cert")?;
    check_secret_file(key_path, "--tls-key")?;
    let (cert_parent, cert_base) = parent_dirfd(cert_path)?;
    let cert_bytes = read_via_parent(cert_parent, &cert_base, config.max_open_bytes, "--tls-cert")?;
    close_fd(cert_parent);
    let (key_parent, key_base) = parent_dirfd(key_path)?;
    let key_bytes = read_via_parent(key_parent, &key_base, config.max_open_bytes, "--tls-key")?;
    close_fd(key_parent);
    let certs: Vec<rustls::pki_types::CertificateDer<'_>> = rustls::pki_types::CertificateDer::pem_slice_iter(&cert_bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| StartupError::flag("--tls-cert", format!("cannot parse PEM certificates: {e}")))?;
    if certs.is_empty() {
        return Err(StartupError::flag("--tls-cert", "no certificates found in file"));
    }
    let key = rustls::pki_types::PrivateKeyDer::from_pem_slice(&key_bytes)
        .map_err(|e| StartupError::flag("--tls-key", format!("cannot parse PEM private key: {e}")))?;
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    let server_config = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
        .map_err(|e| StartupError::flag("--tls-cert", format!("TLS setup failed: {e}")))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| StartupError::flag("--tls-cert", format!("invalid certificate/key pair: {e}")))?;
    Ok(Some(std::sync::Arc::new(server_config)))
}

/// --lrs-credentials-file: only the mode/owner check at startup; the
/// credentials themselves are used by the Step 11 LRS client.
fn check_lrs_credentials(config: &Config, canon: &Canon) -> Result<(), StartupError> {
    if let Some(path) = &canon.lrs_credentials_file {
        check_secret_file(path, "--lrs-credentials-file")?;
    }
    let _ = config;
    Ok(())
}

/// A present consent file must be euid-owned and mode 0600 (AT-142).
fn check_consent_file_mode(canon: &Canon) -> Result<(), StartupError> {
    if let Some(path) = &canon.consent_state_file {
        if lstat_path(path).is_ok() {
            check_secret_file(path, "--consent-state-file")?;
        }
    }
    Ok(())
}

fn open_dirfds(config: &Config, canon: &Canon) -> Result<DirFds, StartupError> {
    let open_opt = |p: &Option<PathBuf>, flag: &str| -> Result<Option<RawFd>, StartupError> {
        match p {
            Some(path) => open_dir_fd(path)
                .map(Some)
                .map_err(|e| StartupError::flag(flag, format!("cannot open dirfd for {}: {e}", path.display()))),
            None => Ok(None),
        }
    };
    let view_root = open_opt(&canon.view_root, "--view-root")?;
    let wiki_root = open_opt(&canon.wiki_root, "--wiki-root")?;
    let corpus_root = open_opt(&canon.corpus_root, "--corpus-root")?;
    let snapshot_dir = open_opt(&canon.snapshot_dir, "--snapshot-dir")?;
    let download_dir = open_opt(&canon.download_dir, "--download-dir")?;
    let data_dir = open_opt(&canon.data_dir, "--data-dir")?;
    let log_dir = open_dir_fd(&canon.log_dir)
        .map_err(|e| StartupError::flag("--log-dir", format!("cannot open dirfd: {e}")))?;
    let working_archive_parent = open_opt(&canon.working_archive_parent, "--working-archive")?;
    let consent_parent = match &canon.consent_state_file {
        Some(f) => {
            let parent = f.parent().unwrap();
            Some(open_dir_fd(parent).map_err(|e| {
                StartupError::flag(
                    "--consent-state-file",
                    format!("cannot open dirfd for {}: {e}", parent.display()),
                )
            })?)
        }
        None => None,
    };
    let _ = config;
    Ok(DirFds {
        view_root,
        wiki_root,
        corpus_root,
        snapshot_dir,
        download_dir,
        data_dir,
        log_dir,
        working_archive_parent,
        consent_parent,
    })
}

/// Step 2: take every sidecar lock, non-blocking, exclusive (DD-114).
/// A lock that another host holds is a startup error naming the flag
/// (AT-164/AT-208). The fds are held for the process lifetime.
fn take_sidecar_locks(config: &Config, canon: &Canon, dirfds: &DirFds) -> Result<Vec<RawFd>, StartupError> {
    let mut fds: Vec<RawFd> = Vec::new();
    let mut take = |dirfd: RawFd, name: &str, flag: &str| -> Result<(), StartupError> {
        let fd = openat_child_rw(dirfd, name, true)
            .map_err(|e| StartupError::flag(flag, format!("cannot open sidecar lock {name}: {e}")))?;
        let st = fstat_fd(fd)
            .map_err(|e| StartupError::flag(flag, format!("cannot fstat sidecar lock {name}: {e}")))?;
        if !st.is_regular {
            close_fd(fd);
            return Err(StartupError::flag(flag, format!("sidecar lock {name} is not a regular file")));
        }
        if flock_ex_nb(fd).is_err() {
            close_fd(fd);
            return Err(StartupError::flag(
                flag,
                format!("another host holds the sidecar lock {name}"),
            ));
        }
        fds.push(fd);
        Ok(())
    };
    if let Some(dirfd) = dirfds.data_dir {
        take(dirfd, ".lock", "--data-dir")?;
    }
    if let Some(dirfd) = dirfds.working_archive_parent {
        if let Some(b) = &canon.working_archive_basename {
            take(dirfd, &format!("{b}.lock"), "--working-archive")?;
        }
    }
    if let Some(dirfd) = dirfds.snapshot_dir {
        take(dirfd, ".lock", "--snapshot-dir")?;
    }
    if let Some(dirfd) = dirfds.download_dir {
        take(dirfd, ".lock", "--download-dir")?;
    }
    if let Some(dirfd) = dirfds.consent_parent {
        if let Some(b) = &canon.consent_basename {
            take(dirfd, &format!("{b}.lock"), "--consent-state-file")?;
        }
    }
    let _ = config;
    Ok(fds)
}

/// AT-151: when --lrs-url is set, bootstrap the consent file (create as
/// `off\n` + RFC3339-seconds-UTC timestamp, mode 0600) or read and parse
/// the present file. Without --lrs-url there is no file and the state is
/// off/null.
fn bootstrap_consent(config: &Config, canon: &Canon, dirfds: &DirFds) -> Result<ConsentState, StartupError> {
    if config.lrs_url.is_none() {
        return Ok(ConsentState::off());
    }
    let (Some(parent_fd), Some(basename)) = (dirfds.consent_parent, &canon.consent_basename) else {
        return Err(StartupError::msg("internal: consent state file configured but no dirfd"));
    };
    let _ = canon;
    let existing = openat_child(parent_fd, basename, false);
    match existing {
        Ok(fd) => {
            let st = fstat_fd(fd).map_err(|e| {
                StartupError::flag("--consent-state-file", format!("cannot fstat: {e}"))
            })?;
            if !st.is_regular {
                close_fd(fd);
                return Err(StartupError::flag(
                    "--consent-state-file",
                    "consent file is not a regular file",
                ));
            }
            let bytes = read_fd_bounded(fd, config.max_open_bytes).map_err(|e| {
                StartupError::flag("--consent-state-file", format!("cannot read: {e}"))
            })?;
            close_fd(fd);
            parse_consent_file(&bytes, "--consent-state-file")
                .map_err(|e| StartupError::flag("--consent-state-file", e))
        }
        Err(_) => {
            let now = rfc3339_secs(std::time::SystemTime::now());
            let state = ConsentState {
                on: false,
                timestamp: Some(now),
            };
            write_consent_file(parent_fd, basename, state.clone()).map_err(|e| {
                StartupError::flag("--consent-state-file", e)
            })?;
            Ok(state)
        }
    }
}

/// AT-78/AT-79/AT-203: the working archive. A missing file with an
/// existing parent is created (TileWriter::create + ADDON_EDU +
/// create-time ConsentRecord); a missing parent or a create failure is a
/// startup error naming --working-archive. The (dev, ino) identity is
/// recorded for name-binding checks.
fn bootstrap_working_archive(config: &Config, canon: &Canon, dirfds: &DirFds) -> Result<Option<(u64, u64)>, StartupError> {
    if config.viewer {
        return Ok(None);
    }
    let (Some(parent_fd), Some(basename)) = (dirfds.working_archive_parent, &canon.working_archive_basename) else {
        return Err(StartupError::msg("internal: working archive configured but no dirfd"));
    };
    let _ = canon;
    match fstatat_nofollow(parent_fd, basename) {
        Ok(st) => Ok(Some((st.dev, st.ino))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let path = config.working_archive.as_ref().unwrap();
            let fd = open_exclusive(parent_fd, basename, true).map_err(|e| {
                StartupError::flag(
                    "--working-archive",
                    format!("cannot create {}: {e}", path.display()),
                )
            })?;
            let st = fstat_fd(fd).map_err(|e| {
                close_fd(fd);
                StartupError::flag("--working-archive", format!("cannot fstat new file: {e}"))
            })?;
            let identity = (st.dev, st.ino);
            let file = unsafe { std::fs::File::from_raw_fd(fd) };
            let mut writer = TileWriter::create_from_file(file).map_err(|e| {
                StartupError::flag("--working-archive", format!("TileWriter::create failed: {e}"))
            })?;
            writer.enable_addon(ADDON_EDU).map_err(|e| {
                StartupError::flag("--working-archive", format!("enable_addon(EDU) failed: {e}"))
            })?;
            let record = create_time_consent_record(config);
            writer
                .write_hyper_tile(TileType::CONSENT, &[record], None, None)
                .map_err(|e| {
                    StartupError::flag("--working-archive", format!("consent record write failed: {e}"))
                })?;
            Ok(Some(identity))
        }
        Err(e) => Err(StartupError::flag(
            "--working-archive",
            format!("cannot stat working archive: {e}"),
        )),
    }
}

/// Open with O_CREAT|O_EXCL|O_RDWR|O_NOFOLLOW|O_NONBLOCK|O_CLOEXEC, 0600.
fn open_exclusive(dirfd: RawFd, name: &str, _create: bool) -> std::io::Result<RawFd> {
    let c = CString::new(name)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in name"))?;
    let fd = unsafe {
        libc::openat(
            dirfd,
            c.as_ptr(),
            libc::O_CREAT | libc::O_EXCL | libc::O_RDWR | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(fd)
}

/// The create-time ConsentRecord (AT-79): subject_id from
/// hash_iri(--researcher-id), study_id 0, purpose Research, scope 0,
/// granted_at = create time, expires_at 0, withdrawn_at 0, legal_hold 0,
/// granted_by = hash_iri(--researcher-id), prev_consent_hash 32 zero
/// bytes, consent_hash over the preceding 104 bytes.
fn create_time_consent_record(config: &Config) -> ConsentRecord {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let hash = hash_iri(config.researcher_id.as_deref().expect("Scholar mode requires --researcher-id"));
    ConsentRecord::new(
        actor_id_to_subject_id(hash),
        0,
        ConsentPurpose::Research,
        0,
        now,
        0,
        false,
        hash,
        [0u8; 32],
    )
}

/// AT-224: the inclusion index. A missing file is created empty; a present
/// file must be a regular file of well-formed JSON lines.
fn bootstrap_inclusion_index(config: &Config, _canon: &Canon, dirfds: &DirFds) -> Result<(), StartupError> {
    if config.viewer {
        return Ok(());
    }
    let Some(data_fd) = dirfds.data_dir else {
        return Err(StartupError::msg("internal: Scholar mode but no data-dir dirfd"));
    };
    const NAME: &str = "inclusion-index.jsonl";
    match openat_child(data_fd, NAME, false) {
        Ok(fd) => {
            let st = fstat_fd(fd).map_err(|e| {
                StartupError::flag("--data-dir", format!("cannot fstat {NAME}: {e}"))
            })?;
            if !st.is_regular {
                close_fd(fd);
                return Err(StartupError::flag(
                    "--data-dir",
                    format!("{NAME} is not a regular file"),
                ));
            }
            let bytes = read_fd_bounded(fd, u64::MAX).map_err(|e| {
                StartupError::flag("--data-dir", format!("cannot read {NAME}: {e}"))
            })?;
            close_fd(fd);
            let text = String::from_utf8(bytes).map_err(|_| {
                StartupError::flag("--data-dir", format!("{NAME} is not UTF-8"))
            })?;
            for (i, line) in text.lines().enumerate() {
                if line.is_empty() {
                    continue;
                }
                if serde_json::from_str::<serde_json::Value>(line).is_err() {
                    return Err(StartupError::flag(
                        "--data-dir",
                        format!("{NAME} line {} is not well-formed JSON", i + 1),
                    ));
                }
            }
            Ok(())
        }
        Err(_) => {
            let fd = openat_child_rw(data_fd, NAME, true).map_err(|e| {
                StartupError::flag("--data-dir", format!("cannot create {NAME}: {e}"))
            })?;
            close_fd(fd);
            Ok(())
        }
    }
}

/// AT-187: startup temp cleanup in --snapshot-dir and --download-dir:
/// remove stale `s3a-kite-tmp-*` entries only when the same euid can take
/// an exclusive flock on their marker. Never on a pid check alone.
fn cleanup_startup_temps(config: &Config, _canon: &Canon, dirfds: &DirFds) -> Result<(), StartupError> {
    if config.viewer {
        return Ok(());
    }
    for (dirfd, flag) in [
        (dirfds.snapshot_dir, "--snapshot-dir"),
        (dirfds.download_dir, "--download-dir"),
    ] {
        let Some(dirfd) = dirfd else { continue };
        let entries = read_dir_entries(dirfd).map_err(|e| {
            StartupError::flag(flag, format!("cannot list directory: {e}"))
        })?;
        for entry in entries {
            if !entry.name.starts_with("s3a-kite-tmp-") {
                continue;
            }
            if entry.is_symlink || !entry.is_dir {
                continue;
            }
            // Same-euid + flockable marker only.
            let dir_fd = match openat_child(dirfd, &entry.name, false) {
                Ok(fd) => fd,
                Err(_) => continue,
            };
            let st = match fstat_fd(dir_fd) {
                Ok(st) => st,
                Err(_) => {
                    close_fd(dir_fd);
                    continue;
                }
            };
            if !st.is_dir {
                close_fd(dir_fd);
                continue;
            }
            let marker = match openat_child(dir_fd, "marker", false) {
                Ok(fd) => fd,
                Err(_) => {
                    close_fd(dir_fd);
                    continue;
                }
            };
            let st = match fstat_fd(marker) {
                Ok(st) => st,
                Err(_) => {
                    close_fd(marker);
                    close_fd(dir_fd);
                    continue;
                }
            };
            if st.uid != unsafe { libc::geteuid() } || !st.is_regular {
                close_fd(marker);
                close_fd(dir_fd);
                continue;
            }
            if flock_ex_nb(marker).is_ok() {
                let _ = std::fs::remove_dir_all(format!("/proc/self/fd/{dirfd}/{}", entry.name));
            }
            close_fd(marker);
            close_fd(dir_fd);
        }
    }
    Ok(())
}

/// DD-88: the ephemeral temp directory: `$XDG_RUNTIME_DIR` if it names an
/// existing directory, else `/tmp`. Same-euid + flockable-marker cleanup
/// of stale `s3a-kite-*` dirs first (never on a pid check alone), then
/// create `s3a-kite-{pid}-{random}` mode 0700 with a held `marker` lock.
fn setup_ephemeral_temp() -> Result<(PathBuf, RawFd), StartupError> {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    // Cleanup of stale dirs.
    if let Ok(rd) = std::fs::read_dir(&base) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("s3a-kite-") {
                continue;
            }
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            // Parse the pid: s3a-kite-{pid}-...
            let rest = &name["s3a-kite-".len()..];
            let Some(pid_str) = rest.split('-').next() else { continue };
            if pid_str.parse::<u32>().is_err() {
                continue;
            }
            let dir_path = base.join(&name);
            let marker_path = dir_path.join("marker");
            let marker = match std::fs::File::open(&marker_path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let fd = std::os::unix::io::AsRawFd::as_raw_fd(&marker);
            let st = match fstat_fd(fd) {
                Ok(st) => st,
                Err(_) => continue,
            };
            if st.uid != unsafe { libc::geteuid() } || !st.is_regular {
                continue;
            }
            if flock_ex_nb(fd).is_ok() {
                let _ = std::fs::remove_dir_all(&dir_path);
            }
        }
    }
    let name = format!("s3a-kite-{}-{}", std::process::id(), random_suffix());
    let path = base.join(&name);
    let c = CString::new(path.to_string_lossy().into_owned())
        .map_err(|_| StartupError::msg("ephemeral temp path contains NUL"))?;
    let rc = unsafe { libc::mkdir(c.as_ptr(), 0o700) };
    if rc != 0 {
        return Err(StartupError::msg(format!(
            "cannot create ephemeral temp {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        )));
    }
    let _ = unsafe { libc::chmod(c.as_ptr(), 0o700) };
    let dir_fd = open_dir_fd(&path)
        .map_err(|e| StartupError::msg(format!("cannot open ephemeral temp: {e}")))?;
    let marker_fd = openat_child_rw(dir_fd, "marker", true)
        .map_err(|e| StartupError::msg(format!("cannot create ephemeral marker: {e}")))?;
    close_fd(dir_fd);
    if flock_ex_nb(marker_fd).is_err() {
        close_fd(marker_fd);
        return Err(StartupError::msg("cannot lock ephemeral marker"));
    }
    Ok((path, marker_fd))
}

/// Open `s3a-kite.log` in --log-dir (append, 0600) and write the startup
/// line. The SPEC names no log file; `s3a-kite.log` is this host's choice.
/// Only startup/shutdown/5xx lines are logged, never per-request, so
/// AT-24/AT-29 byte-identity holds.
fn open_log(config: &Config, _canon: &Canon, dirfds: &DirFds) -> Result<std::sync::Arc<std::sync::Mutex<std::fs::File>>, StartupError> {
    let c = CString::new("s3a-kite.log").unwrap();
    let fd = unsafe {
        libc::openat(
            dirfds.log_dir,
            c.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(StartupError::flag(
            "--log-dir",
            format!("cannot open s3a-kite.log: {}", std::io::Error::last_os_error()),
        ));
    }
    let file = unsafe { std::fs::File::from_raw_fd(fd) };
    let shared = std::sync::Arc::new(std::sync::Mutex::new(file));
    let _ = config;
    Ok(shared)
}

fn log_startup(log_file: &std::sync::Arc<std::sync::Mutex<std::fs::File>>, config: &Config) {
    let mode = if config.viewer { "viewer" } else { "scholar" };
    let bind = match &config.bind {
        Bind::Tcp(ip) => {
            let port = config.port.unwrap_or(0);
            if is_loopback_ip(*ip) {
                format!("tcp loopback {ip}:{port}")
            } else {
                format!("tcp non-loopback {ip}:{port}")
            }
        }
        Bind::Unix(p) => format!("unix {}", p.display()),
    };
    let line = format!(
        "{} startup mode={} bind={} flags: {}\n",
        rfc3339_secs(std::time::SystemTime::now()),
        mode,
        bind,
        config.flag_names.join(" ")
    );
    if let Ok(mut f) = log_file.lock() {
        use std::io::Write;
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }
}

/// Bind the listener. TCP: the port is the one the user asked for (no
/// ephemeral-port fallback). Unix: re-check that no file exists at the
/// socket path (AT-163), then bind and chmod the socket 0600.
fn bind_listener(config: &Config) -> Result<Listener, StartupError> {
    match &config.bind {
        Bind::Tcp(ip) => {
            let port = config.port.ok_or_else(|| {
                StartupError::flag("--port", "a TCP bind requires --port")
            })?;
            let sock = std::net::SocketAddr::new(*ip, port);
            let listener = std::net::TcpListener::bind(sock).map_err(|e| {
                StartupError::flag("--bind", format!("cannot bind {sock}: {e}"))
            })?;
            listener
                .set_nonblocking(true)
                .map_err(|e| StartupError::flag("--bind", format!("cannot set non-blocking: {e}")))?;
            Ok(Listener::Tcp(listener))
        }
        Bind::Unix(path) => {
            if lstat_path(path).is_ok() {
                return Err(StartupError::flag(
                    "--bind",
                    format!("a file already exists at unix socket path {}", path.display()),
                ));
            }
            let listener = std::os::unix::net::UnixListener::bind(path).map_err(|e| {
                StartupError::flag("--bind", format!("cannot bind unix socket {}: {e}", path.display()))
            })?;
            let _ = std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600));
            listener
                .set_nonblocking(true)
                .map_err(|e| StartupError::flag("--bind", format!("cannot set non-blocking: {e}")))?;
            Ok(Listener::Unix(listener))
        }
    }
}

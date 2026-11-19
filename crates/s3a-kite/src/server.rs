//! Route dispatch (§5.2.1) and host state.
//!
//! Error precedence per request (after the transport-level header-block
//! 413): authentication, then 405, then 413/415, then 409 viewer_only,
//! then JSON/param errors, then route-specific errors. The frame/hit-test
//! tail is: invalid_view, then unknown_handle, then the §5.2.6 matrix,
//! then invalid_floor, then invalid_frame_size, then no_frame. The
//! /v1/open tail is: path_escape, not_found, unreadable,
//! not_regular_file, file_too_large, invalid_magic, too_many_handles.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use base64::Engine as _;
use s3a_pack_kite::{
    hit_test, open, open_path, proceed_to_warehouse, render_floor_detail,
    render_floor_transition, render_manifest, render_page, render_warehouse, Hit, KiteError,
    ScholarRoots,
};
use serde_json::json;

use crate::candidates::list_candidates;
use crate::consent::{write_consent_file, ConsentState};
use crate::flags::{Config, Started};
use crate::http::{Handler, Request, Response};
use crate::paths::{
    close_fd, fstat_fd, is_within, lexical_normalize, openat_child, read_fd_bounded,
};
use crate::png::encode_png;
use crate::timefmt::rfc3339_secs;
use crate::token::bearer_matches;

pub const KNOWN_VIEWS: &[&str] = &[
    "manifest",
    "header",
    "banner",
    "page",
    "warehouse",
    "floor-detail",
    "floor-transition",
];

/// One open handle: the pack handle (behind its own mutex — the pack
/// `Handle` is `Send` but not `Sync`, so the mutex is also the
/// per-handle serialization lock), its kind/first_view strings, and the
/// last-use instant for the idle rule (AT-149).
pub struct HandleEntry {
    pub handle: Mutex<s3a_pack_kite::Handle>,
    pub kind: String,
    pub first_view: String,
    pub last_used: Mutex<Instant>,
}

/// Host state shared by the connection threads.
pub struct Host {
    pub started: Arc<Started>,
    pub handles: RwLock<HashMap<String, Arc<HandleEntry>>>,
    pub next_handle: AtomicU64,
    /// In-memory selected paper identities (Step 9 approximation of the
    /// select record; the durable record is a Step 10/11 question).
    pub selected: Mutex<HashSet<String>>,
    /// In-memory consent state (read/written under the write mutex).
    pub consent: Mutex<ConsentState>,
    /// The write mutex (DD-102): select, download, and consent. Skip
    /// does not take it. Candidate membership is computed before it.
    pub write_mu: Mutex<()>,
    /// The in-flight step-2 fetch registry, keyed by
    /// `(file_uuid_hex, identity)` (Step 10, DD-104). A waiter never
    /// starts a GET and never appends an index line.
    pub inflight: Mutex<HashMap<(String, String), Arc<crate::write_path::InFlightFetch>>>,
    pub shutdown: Arc<AtomicBool>,
    /// Set when a fail-closed error (DD-138) requires a non-zero exit
    /// after the response is written.
    pub fail_closed: AtomicBool,
}

impl Host {
    pub fn new(started: Started, shutdown: Arc<AtomicBool>) -> Host {
        let consent = started.consent.clone();
        Host {
            started: Arc::new(started),
            handles: RwLock::new(HashMap::new()),
            next_handle: AtomicU64::new(1),
            selected: Mutex::new(HashSet::new()),
            consent: Mutex::new(consent),
            write_mu: Mutex::new(()),
            inflight: Mutex::new(HashMap::new()),
            shutdown,
            fail_closed: AtomicBool::new(false),
        }
    }

    pub fn config(&self) -> &Config {
        &self.started.config
    }

    pub fn is_scholar(&self) -> bool {
        !self.config().viewer
    }

    /// `select_enabled` for `render_page` (AT-144): true in Scholar mode,
    /// false on a viewer-only host.
    pub fn select_enabled(&self) -> bool {
        self.is_scholar()
    }

    fn log_5xx(&self, status: u16, method: &str, path: &str) {
        // Only startup/shutdown/5xx lines are logged, never per-request,
        // so AT-24/AT-29 byte-identity holds. Method + path carry no
        // identifiers (handle/paper ids are query/body fields).
        let line = format!(
            "{} 5xx {} {} {}\n",
            rfc3339_secs(std::time::SystemTime::now()),
            status,
            method,
            path
        );
        if let Ok(mut f) = self.started.log_file.lock() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }
    }

    /// Authenticate a request. `/health` is tokenless unless
    /// --behind-proxy is set (then it requires the token like any route).
    fn authenticate(&self, req: &Request) -> bool {
        if req.path == "/health" && !self.config().behind_proxy {
            return true;
        }
        bearer_matches(req.header("authorization"), &self.started.token_secret)
    }

    /// Look up a handle, evicting it (and returning None) when it has
    /// been idle longer than --handle-idle-secs (AT-149). A successful
    /// lookup refreshes the last-use instant.
    fn get_handle(&self, id: &str) -> Option<Arc<HandleEntry>> {
        let entry = {
            let table = self.handles.read().unwrap();
            table.get(id)?.clone()
        };
        let idle = Duration::from_secs(self.config().handle_idle_secs);
        let expired = {
            let last = entry.last_used.lock().unwrap();
            last.elapsed() > idle
        };
        if expired {
            let mut table = self.handles.write().unwrap();
            table.remove(id);
            return None;
        }
        {
            let mut last = entry.last_used.lock().unwrap();
            *last = Instant::now();
        }
        Some(entry)
    }

    /// Evict idle-expired handles (so the max-open-handles cap counts
    /// only live handles — an expired handle is closed, AT-149).
    fn evict_expired_handles(&self) {
        let idle = Duration::from_secs(self.config().handle_idle_secs);
        let mut table = self.handles.write().unwrap();
        table.retain(|_, entry| {
            let last = entry.last_used.lock().unwrap();
            last.elapsed() <= idle
        });
    }
}

impl Handler for Host {
    fn handle(&self, req: &Request) -> Response {
        // Health is tokenless unless --behind-proxy; every other route
        // requires the token (auth first, including on unknown paths).
        if !self.authenticate(req) {
            return err(401, "unauthenticated", "missing or invalid token");
        }
        match (req.method.as_str(), req.path.as_str()) {
            ("GET", "/health") => {
                return Response::text(200, "ok\n");
            }
            ("GET", "/v1/frame") => return self.route_frame(req),
            ("GET", "/v1/candidates") => return self.route_candidates(req),
            ("GET", "/v1/consent") => return self.route_consent_get(req),
            ("POST", "/v1/open") => return self.route_open(req),
            ("POST", "/v1/close") => return self.route_close(req),
            ("POST", "/v1/proceed") => return self.route_proceed(req),
            ("POST", "/v1/hit-test") => return self.route_hit_test(req),
            ("POST", "/v1/select") => return self.route_select(req),
            ("POST", "/v1/download") => return self.route_download(req),
            ("POST", "/v1/skip") => return self.route_skip(req),
            ("POST", "/v1/deselect") => return self.route_deselect(req),
            ("POST", "/v1/consent") => return self.route_consent_post(req),
            _ => {}
        }
        // Known path, wrong method → 405; unknown path → 404 unknown_path.
        const KNOWN_PATHS: &[&str] = &[
            "/health",
            "/v1/open",
            "/v1/close",
            "/v1/proceed",
            "/v1/frame",
            "/v1/hit-test",
            "/v1/candidates",
            "/v1/select",
            "/v1/download",
            "/v1/skip",
            "/v1/deselect",
            "/v1/consent",
        ];
        if KNOWN_PATHS.contains(&req.path.as_str()) {
            err(
                405,
                "method_not_allowed",
                &format!("{} is not allowed on {}", req.method, req.path),
            )
        } else {
            err(404, "unknown_path", &format!("unknown path {}", req.path))
        }
    }

    fn on_5xx(&self, status: u16, method: &str, path: &str) {
        self.log_5xx(status, method, path);
    }
}

/// `{"error": code, "message": text}` (DD-83).
pub fn err(status: u16, code: &str, message: &str) -> Response {
    Response::error(status, code, message)
}

/// Map a pack `KiteError` to the HTTP error the SPEC assigns its code.
pub fn kite_err(e: &KiteError) -> Response {
    let status = match e.code() {
        "invalid_param" | "missing_param" | "invalid_view" | "invalid_frame_size"
        | "invalid_magic" => 400,
        "unreadable" => 403,
        "unknown_handle" | "unknown_paper" | "not_found" => 404,
        "viewer_only" | "view_mismatch" | "not_archive" | "unsupported" | "no_frame"
        | "invalid_floor" | "file_too_large" | "not_regular_file" | "path_escape"
        | "already_included" | "addon_missing" | "download_conflict" | "snapshot_conflict" => 409,
        "request_too_large" => 413,
        "unsupported_media_type" => 415,
        "too_many_handles" => 429,
        _ => 500,
    };
    let code = if status == 500 && e.code() != "render_failed" && e.code() != "internal" {
        // An unmapped pack code is an unexpected host failure.
        "internal"
    } else {
        e.code()
    };
    let mut body = json!({"error": code, "message": e.message()});
    if e.code() == "render_failed" {
        body["kite_code"] = json!(e.code());
    }
    Response::json(status, serde_json::to_vec(&body).unwrap())
}

/// 415: a JSON route whose Content-Type is present and is not
/// `application/json`, or is missing on a route that requires a JSON
/// body (case-insensitive, parameters ignored, AT-171).
fn check_content_type(req: &Request) -> Option<Response> {
    match req.content_type() {
        None => Some(err(
            415,
            "unsupported_media_type",
            "a JSON body requires Content-Type: application/json",
        )),
        Some(ct) => {
            let media = ct.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
            if media != "application/json" {
                return Some(err(
                    415,
                    "unsupported_media_type",
                    "Content-Type is not application/json",
                ));
            }
            None
        }
    }
}

/// 413 for a body over --max-request-bytes (after auth, 405, 415).
fn check_body_over_cap(req: &Request) -> Option<Response> {
    if req.body_over_cap {
        Some(err(
            413,
            "request_too_large",
            "request body exceeds --max-request-bytes",
        ))
    } else {
        None
    }
}

/// Parse the request body as a JSON object. A body that is not a JSON
/// object (truncated, not JSON, an array/scalar) is `malformed_json`.
fn parse_json_body(req: &Request) -> Result<serde_json::Value, Response> {
    match serde_json::from_slice::<serde_json::Value>(&req.body) {
        Ok(v) if v.is_object() => Ok(v),
        _ => Err(err(400, "malformed_json", "request body is not a JSON object")),
    }
}

/// A required string field: absent or JSON null → missing_param; wrong
/// type → invalid_param.
fn req_string(body: &serde_json::Value, name: &str) -> Result<String, Response> {
    match body.get(name) {
        None | Some(serde_json::Value::Null) => Err(err(
            400,
            "missing_param",
            &format!("{name} is required"),
        )),
        Some(serde_json::Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(err(
            400,
            "invalid_param",
            &format!("{name} must be a string"),
        )),
    }
}

/// An optional string field: absent or JSON null → None; wrong type →
/// invalid_param.
fn opt_string(body: &serde_json::Value, name: &str) -> Result<Option<String>, Response> {
    match body.get(name) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(err(
            400,
            "invalid_param",
            &format!("{name} must be a string"),
        )),
    }
}

/// A required finite f64 field: absent/null → missing_param; wrong type
/// or non-finite → invalid_param.
fn req_f64(body: &serde_json::Value, name: &str) -> Result<f64, Response> {
    match body.get(name) {
        None | Some(serde_json::Value::Null) => Err(err(
            400,
            "missing_param",
            &format!("{name} is required"),
        )),
        Some(serde_json::Value::Number(n)) => {
            let v = n.as_f64().ok_or_else(|| {
                err(400, "invalid_param", &format!("{name} must be a number"))
            })?;
            if !v.is_finite() {
                return Err(err(400, "invalid_param", &format!("{name} must be finite")));
            }
            Ok(v)
        }
        Some(_) => Err(err(400, "invalid_param", &format!("{name} must be a number"))),
    }
}

/// A required non-negative integer field: absent/null → missing_param;
/// non-integer or negative → invalid_param.
fn req_u64(body: &serde_json::Value, name: &str) -> Result<u64, Response> {
    match body.get(name) {
        None | Some(serde_json::Value::Null) => Err(err(
            400,
            "missing_param",
            &format!("{name} is required"),
        )),
        Some(serde_json::Value::Number(n)) => {
            if let Some(v) = n.as_u64() {
                Ok(v)
            } else {
                Err(err(
                    400,
                    "invalid_param",
                    &format!("{name} must be a non-negative integer"),
                ))
            }
        }
        Some(_) => Err(err(
            400,
            "invalid_param",
            &format!("{name} must be a non-negative integer"),
        )),
    }
}

/// Parse a query integer (w, h, floor): all digits, fits u64. Missing →
/// missing_param; non-integer or negative → invalid_param.
fn query_u64(req: &Request, name: &str) -> Result<u64, Response> {
    match req.query_param(name) {
        None => Err(err(400, "missing_param", &format!("{name} is required"))),
        Some(s) => parse_u64_param(s, name),
    }
}

fn parse_u64_param(s: &str, name: &str) -> Result<u64, Response> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(err(
            400,
            "invalid_param",
            &format!("{name} must be a non-negative integer"),
        ));
    }
    s.parse::<u64>().map_err(|_| {
        err(400, "invalid_param", &format!("{name} is out of range"))
    })
}

/// Parse an optional query f64: missing → default; non-numeric or
/// non-finite → invalid_param.
fn query_f64_opt(req: &Request, name: &str, default: f64) -> Result<f64, Response> {
    match req.query_param(name) {
        None => Ok(default),
        Some(s) => match s.parse::<f64>() {
            Ok(v) if v.is_finite() => Ok(v),
            _ => Err(err(
                400,
                "invalid_param",
                &format!("{name} must be a finite number"),
            )),
        },
    }
}

/// Parse the `instant` query literal: `true` / `false` only; anything
/// else → invalid_param.
fn query_instant(req: &Request) -> Result<bool, Response> {
    match req.query_param("instant") {
        None => Ok(false),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(_) => Err(err(
            400,
            "invalid_param",
            "instant must be the literal true or false",
        )),
    }
}

impl Host {
    // ---- GET /v1/frame ----

    fn route_frame(&self, req: &Request) -> Response {
        if let Some(r) = check_body_over_cap(req) {
            return r;
        }
        // Param phase. `view` first: an unknown view beats every other
        // field check (AT-100/AT-196).
        let view = match req.query_param("view") {
            None => {
                return err(400, "missing_param", "view is required");
            }
            Some(v) => v.to_string(),
        };
        if !KNOWN_VIEWS.contains(&view.as_str()) {
            return err(400, "invalid_view", &format!("unknown view {view:?}"));
        }
        let is_floor_view = view == "floor-detail" || view == "floor-transition";
        let handle_id = match req.query_param("handle") {
            None => {
                return err(400, "missing_param", "handle is required");
            }
            Some(h) => h.to_string(),
        };
        let w = match query_u64(req, "w") {
            Ok(v) => v,
            Err(r) => return r,
        };
        let h = match query_u64(req, "h") {
            Ok(v) => v,
            Err(r) => return r,
        };
        let floor: Option<u32> = if is_floor_view {
            match req.query_param("floor") {
                None => {
                    return err(400, "missing_param", "floor is required for this view");
                }
                Some(s) => match parse_u64_param(s, "floor") {
                    Ok(v) => match u32::try_from(v) {
                        Ok(v) => Some(v),
                        Err(_) => {
                            return err(400, "invalid_param", "floor is out of range");
                        }
                    },
                    Err(r) => return r,
                },
            }
        } else {
            if req.query_param("floor").is_some() {
                return err(
                    400,
                    "invalid_param",
                    "floor must be omitted on a non-floor view",
                );
            }
            None
        };
        let instant = match query_instant(req) {
            Ok(v) => v,
            Err(r) => return r,
        };
        // `t` is required for floor-transition when instant is false; a
        // finite value outside [0, 0.45] is clamped, not an error.
        let t = match req.query_param("t") {
            Some(s) => match s.parse::<f64>() {
                Ok(v) if v.is_finite() => v.clamp(0.0, 0.45),
                _ => {
                    return err(400, "invalid_param", "t must be a finite number");
                }
            },
            None => {
                if view == "floor-transition" && !instant {
                    return err(
                        400,
                        "missing_param",
                        "t is required for floor-transition when instant is false",
                    );
                }
                0.0
            }
        };
        let scroll_x = match query_f64_opt(req, "scroll_x", 0.0) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let scroll_y = match query_f64_opt(req, "scroll_y", 0.0) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let dpr = match query_f64_opt(req, "device_pixel_ratio", 1.0) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let entry = match self.get_handle(&handle_id) {
            Some(e) => e,
            None => {
                return err(404, "unknown_handle", "handle is not open");
            }
        };
        let frame = {
            let handle = entry.handle.lock().unwrap();
            let (w, h) = (w as f64, h as f64);
            match view.as_str() {
                "manifest" | "header" | "banner" => {
                    render_manifest(&handle, w, h, scroll_y, dpr)
                }
                "page" => render_page(&handle, w, h, scroll_y, dpr, self.select_enabled()),
                "warehouse" => render_warehouse(&handle, w, h, scroll_x, scroll_y, dpr),
                "floor-detail" => {
                    render_floor_detail(&handle, floor.unwrap(), w, h, scroll_y, dpr)
                }
                "floor-transition" => render_floor_transition(
                    &handle,
                    floor.unwrap(),
                    t,
                    w,
                    h,
                    scroll_x,
                    scroll_y,
                    instant,
                    dpr,
                ),
                _ => unreachable!("view checked against KNOWN_VIEWS"),
            }
        };
        let frame = match frame {
            Ok(f) => f,
            Err(e) => return kite_err(&e),
        };
        let png = match encode_png(&frame.rgba, frame.width, frame.height) {
            Ok(p) => p,
            Err(e) => {
                return err(500, "internal", &format!("PNG encode failed: {e}"));
            }
        };
        let png_b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        let text_runs: Vec<serde_json::Value> = frame
            .text_runs
            .iter()
            .map(|r| {
                json!({"text": r.text, "x": r.x, "y": r.y, "width": r.width, "height": r.height})
            })
            .collect();
        let semantics: Vec<serde_json::Value> = frame
            .semantics
            .iter()
            .map(|n| {
                json!({"role": n.role, "label": n.label, "x": n.x, "y": n.y, "width": n.width, "height": n.height, "href": n.href, "paper_id": n.paper_id, "floor": n.floor, "opacity": n.opacity})
            })
            .collect();
        Response::json(
            200,
            serde_json::to_vec(&json!({
                "png_base64": png_b64,
                "width": frame.width,
                "height": frame.height,
                "text_runs": text_runs,
                "semantics": semantics,
            }))
            .unwrap(),
        )
    }

    // ---- POST /v1/hit-test ----

    fn route_hit_test(&self, req: &Request) -> Response {
        if let Some(r) = check_content_type(req) {
            return r;
        }
        if let Some(r) = check_body_over_cap(req) {
            return r;
        }
        let body = match parse_json_body(req) {
            Ok(b) => b,
            Err(r) => return r,
        };
        // `view` first (malformed_json already passed): an unknown view
        // beats every other field check.
        let view = match req_string(&body, "view") {
            Ok(v) => v,
            Err(r) => return r,
        };
        if !KNOWN_VIEWS.contains(&view.as_str()) {
            return err(400, "invalid_view", &format!("unknown view {view:?}"));
        }
        let is_floor_view = view == "floor-detail" || view == "floor-transition";
        let handle_id = match req_string(&body, "handle") {
            Ok(v) => v,
            Err(r) => return r,
        };
        let x = match req_f64(&body, "x") {
            Ok(v) => v,
            Err(r) => return r,
        };
        let y = match req_f64(&body, "y") {
            Ok(v) => v,
            Err(r) => return r,
        };
        let floor: Option<u32> = if is_floor_view {
            match req_u64(&body, "floor") {
                Ok(v) => match u32::try_from(v) {
                    Ok(v) => Some(v),
                    Err(_) => {
                        return err(400, "invalid_param", "floor is out of range");
                    }
                },
                Err(r) => return r,
            }
        } else {
            match body.get("floor") {
                None | Some(serde_json::Value::Null) => None,
                Some(_) => {
                    return err(
                        400,
                        "invalid_param",
                        "floor must be omitted on a non-floor view",
                    );
                }
            }
        };
        let entry = match self.get_handle(&handle_id) {
            Some(e) => e,
            None => {
                return err(404, "unknown_handle", "handle is not open");
            }
        };
        let hit = {
            let handle = entry.handle.lock().unwrap();
            hit_test(&handle, &view, floor, x, y)
        };
        let hit = match hit {
            Ok(h) => h,
            Err(e) => return kite_err(&e),
        };
        let (hit_str, floor_out, paper_id, href) = match hit {
            Hit::Continue => ("continue", None, None, None),
            Hit::Floor(n) => ("floor", Some(n), None, None),
            Hit::Select { paper_id } => ("select", None, Some(paper_id), None),
            Hit::Link { href } => ("link", None, None, Some(href)),
            Hit::None => ("none", None, None, None),
        };
        Response::json(
            200,
            serde_json::to_vec(&json!({
                "hit": hit_str,
                "floor": floor_out,
                "paper_id": paper_id,
                "href": href,
            }))
            .unwrap(),
        )
    }

    // ---- POST /v1/open ----

    fn route_open(&self, req: &Request) -> Response {
        if let Some(r) = check_content_type(req) {
            return r;
        }
        if let Some(r) = check_body_over_cap(req) {
            return r;
        }
        let body = match parse_json_body(req) {
            Ok(b) => b,
            Err(r) => return r,
        };
        let path = match req_string(&body, "path") {
            Ok(p) => p,
            Err(r) => return r,
        };
        let name = match opt_string(&body, "name") {
            Ok(n) => n,
            Err(r) => return r,
        };
        if path.is_empty() || path.contains('\0') {
            return err(
                400,
                "invalid_param",
                "path must be a non-empty absolute path",
            );
        }
        let abs_path = match parse_open_path(&path) {
            Ok(p) => p,
            Err(r) => return r,
        };
        let (root_flag, root_path, root_fd, rel) = match self.root_for(&abs_path) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let _ = root_flag;
        // Walk the relative components from the root dirfd (O_NOFOLLOW).
        let fd = match self.walk_open(root_fd, &rel) {
            Ok(fd) => fd,
            Err(r) => return r,
        };
        let st = match fstat_fd(fd) {
            Ok(st) => st,
            Err(_) => {
                close_fd(fd);
                return err(403, "unreadable", "cannot stat the file");
            }
        };
        if !st.is_regular {
            close_fd(fd);
            return err(409, "not_regular_file", "path is not a regular file");
        }
        // A basename matching s3a-kite-tmp-* directly inside --snapshot-dir
        // or --download-dir is not_regular_file (AT-171).
        if rel.components().count() == 1 {
            if let Some(base) = rel.file_name().and_then(|n| n.to_str()) {
                if base.starts_with("s3a-kite-tmp-") {
                    let is_snap_or_dl = matches!(
                        (&self.started.canon.snapshot_dir, &self.started.canon.download_dir),
                        (Some(s), _) if *s == root_path
                    ) || matches!(
                        (&self.started.canon.snapshot_dir, &self.started.canon.download_dir),
                        (_, Some(d)) if *d == root_path
                    );
                    if is_snap_or_dl {
                        close_fd(fd);
                        return err(
                            409,
                            "not_regular_file",
                            "s3a-kite-tmp-* files inside --snapshot-dir/--download-dir cannot be opened",
                        );
                    }
                }
            }
        }
        // AT-217: a working-archive open whose (dev, ino) differs from
        // the recorded startup/create identity is path_escape.
        if self.is_scholar() {
            if let Some(wa) = &self.started.canon.working_archive {
                if abs_path == *wa {
                    if let Some((dev, ino)) = self.started.working_archive_identity {
                        if (st.dev, st.ino) != (dev, ino) {
                            close_fd(fd);
                            return err(
                                409,
                                "path_escape",
                                "working archive identity changed since startup",
                            );
                        }
                    }
                }
            }
        }
        let under_scholar_root = self.is_scholar()
            && (self
                .started
                .canon
                .wiki_root
                .as_ref()
                .map(|w| *w == root_path)
                .unwrap_or(false)
                || self
                    .started
                    .canon
                    .corpus_root
                    .as_ref()
                    .map(|c| *c == root_path)
                    .unwrap_or(false));
        // `open_path` reads from the already-open fd (pread, no close,
        // no reopen by name); `open` reads bytes the host read itself.
        // The pack classifies a non-archive by the name hint's extension
        // (§5.2.5), so when the client sends no `name` the host uses the
        // file's basename as the hint; a client-supplied name still wins.
        let name_hint = name.clone().or_else(|| {
            rel.file_name()
                .map(|n| n.to_string_lossy().into_owned())
        });
        let handle = if under_scholar_root {
            let is_corpus = self
                .started
                .canon
                .corpus_root
                .as_ref()
                .map(|c| *c == root_path)
                .unwrap_or(false);
            let scholar_roots = ScholarRoots {
                wiki_root: self.started.canon.wiki_root.clone().unwrap(),
                corpus_root: self.started.canon.corpus_root.clone().unwrap(),
                // Only a corpus dossier can be shadowed by the wiki
                // version at the same relative path; a wiki dossier is
                // the winner and is never shadowed.
                shadowed_by_wiki: if is_corpus {
                    self.shadowed_by_wiki(&rel)
                } else {
                    false
                },
            };
            let result = open_path(
                &self.started.kite,
                fd,
                &abs_path.to_string_lossy(),
                &root_path,
                self.config().max_frame_pixels,
                self.config().max_open_bytes,
                self.started.pinned_keys.clone(),
                Some(&scholar_roots),
                name_hint.clone(),
            );
            close_fd(fd);
            result
        } else {
            let bytes = match read_fd_bounded(fd, self.config().max_open_bytes) {
                Ok(b) => b,
                Err(_) => {
                    close_fd(fd);
                    return err(403, "unreadable", "cannot read the file");
                }
            };
            close_fd(fd);
            open(
                &self.started.kite,
                &bytes,
                name_hint.clone(),
                self.config().max_frame_pixels,
                self.config().max_open_bytes,
                self.started.pinned_keys.clone(),
            )
        };
        let handle = match handle {
            Ok(h) => h,
            Err(e) => return kite_err(&e),
        };
        self.evict_expired_handles();
        {
            let table = self.handles.read().unwrap();
            if table.len() >= self.config().max_open_handles as usize {
                return err(
                    429,
                    "too_many_handles",
                    "too many open handles",
                );
            }
        }
        let id = format!("h-{:016x}", self.next_handle.fetch_add(1, Ordering::SeqCst));
        let entry = Arc::new(HandleEntry {
            kind: handle.kind().to_string(),
            first_view: handle.first_view().to_string(),
            handle: Mutex::new(handle),
            last_used: Mutex::new(Instant::now()),
        });
        let (kind, first_view) = (entry.kind.clone(), entry.first_view.clone());
        {
            let mut table = self.handles.write().unwrap();
            table.insert(id.clone(), entry);
        }
        Response::json(
            200,
            serde_json::to_vec(&json!({
                "handle": id,
                "kind": kind,
                "first_view": first_view,
            }))
            .unwrap(),
        )
    }

    /// Find the allowed root containing `abs_path` (already lexically
    /// normalized). Returns (flag, root, root dirfd, relative path).
    fn root_for(
        &self,
        abs_path: &std::path::Path,
    ) -> Result<(&'static str, std::path::PathBuf, std::os::unix::io::RawFd, std::path::PathBuf), Response>
    {
        let canon = &self.started.canon;
        let dirfds = &self.started.dirfds;
        let mut roots: Vec<(&'static str, &std::path::PathBuf, std::os::unix::io::RawFd)> = Vec::new();
        if let (Some(v), Some(fd)) = (&canon.view_root, dirfds.view_root) {
            roots.push(("--view-root", v, fd));
        }
        if let (Some(w), Some(fd)) = (&canon.wiki_root, dirfds.wiki_root) {
            roots.push(("--wiki-root", w, fd));
        }
        if let (Some(c), Some(fd)) = (&canon.corpus_root, dirfds.corpus_root) {
            roots.push(("--corpus-root", c, fd));
        }
        if let (Some(s), Some(fd)) = (&canon.snapshot_dir, dirfds.snapshot_dir) {
            roots.push(("--snapshot-dir", s, fd));
        }
        if let (Some(d), Some(fd)) = (&canon.download_dir, dirfds.download_dir) {
            roots.push(("--download-dir", d, fd));
        }
        for (flag, root, fd) in roots {
            if abs_path == root.as_path() {
                // The path IS the root directory: not a regular file.
                return Err(err(409, "not_regular_file", "path is a directory"));
            }
            if is_within(root, abs_path) {
                let rel = abs_path.strip_prefix(root).unwrap().to_path_buf();
                return Ok((flag, root.clone(), fd, rel));
            }
        }
        Err(err(
            409,
            "path_escape",
            "path is outside the allowed roots",
        ))
    }

    /// Walk `rel` from `root_fd` component by component (O_NOFOLLOW).
    /// Returns the fd of the final component.
    fn walk_open(
        &self,
        root_fd: std::os::unix::io::RawFd,
        rel: &std::path::Path,
    ) -> Result<std::os::unix::io::RawFd, Response> {
        let mut fd = root_fd;
        let mut owned: Vec<std::os::unix::io::RawFd> = Vec::new();
        let components: Vec<_> = rel.components().collect();
        for (i, component) in components.iter().enumerate() {
            let name = component.as_os_str().to_string_lossy().into_owned();
            let next = match openat_child(fd, &name, false) {
                Ok(f) => f,
                Err(e) => {
                    for old in owned {
                        close_fd(old);
                    }
                    return Err(walk_io_error(&e));
                }
            };
            if fd != root_fd {
                owned.push(fd);
            }
            // Intermediate components must be directories.
            if i + 1 < components.len() {
                match fstat_fd(next) {
                    Ok(st) if st.is_dir => {}
                    Ok(_) => {
                        close_fd(next);
                        for old in owned {
                            close_fd(old);
                        }
                        return Err(err(404, "not_found", "path does not exist under the root"));
                    }
                    Err(e) => {
                        close_fd(next);
                        for old in owned {
                            close_fd(old);
                        }
                        return Err(walk_io_error(&e));
                    }
                }
            }
            fd = next;
        }
        for old in owned {
            close_fd(old);
        }
        Ok(fd)
    }

    /// Is this corpus-relative path shadowed by a qualifying wiki dossier
    /// at the same relative path (only for `papers/SRC-*.md`)?
    fn shadowed_by_wiki(&self, rel: &std::path::Path) -> bool {
        let Some(name) = rel.file_name().and_then(|n| n.to_str()) else {
            return false;
        };
        if !name.starts_with("SRC-") || !name.ends_with(".md") {
            return false;
        }
        if rel.parent().map(|p| p.as_os_str() != "papers").unwrap_or(true) {
            return false;
        }
        let Some(wiki_fd) = self.started.dirfds.wiki_root else {
            return false;
        };
        let Some(fm) = crate::candidates::dossier_front_matter_at(
            wiki_fd,
            rel,
            self.config().max_open_bytes,
        ) else {
            return false;
        };
        let stem = name.trim_end_matches(".md");
        match fm.source_id.as_deref() {
            Some(id) => id == stem,
            None => true,
        }
    }

    // ---- POST /v1/close ----

    fn route_close(&self, req: &Request) -> Response {
        if let Some(r) = check_content_type(req) {
            return r;
        }
        if let Some(r) = check_body_over_cap(req) {
            return r;
        }
        let body = match parse_json_body(req) {
            Ok(b) => b,
            Err(r) => return r,
        };
        let handle_id = match req_string(&body, "handle") {
            Ok(v) => v,
            Err(r) => return r,
        };
        // An idle-expired handle is already closed → unknown_handle.
        if self.get_handle(&handle_id).is_none() {
            return err(404, "unknown_handle", "handle is not open");
        }
        {
            let mut table = self.handles.write().unwrap();
            table.remove(&handle_id);
        }
        Response::json(200, serde_json::to_vec(&json!({"ok": true})).unwrap())
    }

    // ---- POST /v1/proceed ----

    fn route_proceed(&self, req: &Request) -> Response {
        if let Some(r) = check_content_type(req) {
            return r;
        }
        if let Some(r) = check_body_over_cap(req) {
            return r;
        }
        let body = match parse_json_body(req) {
            Ok(b) => b,
            Err(r) => return r,
        };
        let handle_id = match req_string(&body, "handle") {
            Ok(v) => v,
            Err(r) => return r,
        };
        let entry = match self.get_handle(&handle_id) {
            Some(e) => e,
            None => {
                return err(404, "unknown_handle", "handle is not open");
            }
        };
        let result = {
            let handle = entry.handle.lock().unwrap();
            proceed_to_warehouse(&handle)
        };
        if let Err(e) = result {
            return kite_err(&e);
        }
        Response::json(200, serde_json::to_vec(&json!({"ok": true})).unwrap())
    }
}

/// Parse a `/v1/open` path: a `file://` URL (`file:///path` or
/// `file://localhost/path`; any other host is `path_escape`) or an
/// absolute filesystem path (a relative path is `invalid_param`). The
/// result is lexically normalized.
fn parse_open_path(path: &str) -> Result<std::path::PathBuf, Response> {
    let raw: &str = if let Some(rest) = path.strip_prefix("file://") {
        if rest.starts_with('/') {
            // file:///abs/path -> /abs/path (the leading slash is kept).
            rest
        } else if let Some(p) = rest.strip_prefix("localhost") {
            // file://localhost/abs/path -> /abs/path.
            p
        } else {
            return Err(err(
                409,
                "path_escape",
                "file:// URLs must use an empty or localhost host",
            ));
        }
    } else {
        path
    };
    if !raw.starts_with('/') {
        return Err(err(
            400,
            "invalid_param",
            "path must be absolute (or a file:// URL)",
        ));
    }
    lexical_normalize(std::path::Path::new(raw)).ok_or_else(|| {
        err(400, "invalid_param", "path must be absolute")
    })
}

/// Map a walk/open io error to the /v1/open tail code: ELOOP →
/// path_escape, ENOENT/ENOTDIR → not_found, EACCES → unreadable.
fn walk_io_error(e: &std::io::Error) -> Response {
    match e.raw_os_error() {
        Some(code) if code == libc::ELOOP => err(
            409,
            "path_escape",
            "a symlink in the path escapes the allowed roots",
        ),
        Some(code) if code == libc::ENOENT || code == libc::ENOTDIR => {
            err(404, "not_found", "path does not exist under the root")
        }
        Some(code) if code == libc::EACCES => {
            err(403, "unreadable", "the process cannot read the path")
        }
        _ => err(403, "unreadable", "the process cannot read the path"),
    }
}

impl Host {
    // ---- GET /v1/candidates ----

    fn route_candidates(&self, req: &Request) -> Response {
        if let Some(r) = check_body_over_cap(req) {
            return r;
        }
        if !self.is_scholar() {
            return err(409, "viewer_only", "candidates is a Scholar route");
        }
        let papers = list_candidates(
            &self.started.canon,
            &self.started.dirfds,
            &self.started.kite,
            self.config().max_frame_pixels,
            self.config().max_open_bytes,
            &self.started.pinned_keys,
        );
        let papers_json: Vec<serde_json::Value> = papers
            .iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "title": c.title.clone().unwrap_or_default(),
                    "source_url": c.source_url,
                    "on_disk": c.on_disk,
                    "page_path": c.page_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                    "archive_path": c.archive_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                })
            })
            .collect();
        Response::json(
            200,
            serde_json::to_vec(&json!({"papers": papers_json})).unwrap(),
        )
    }

    /// The candidate for `paper_id`, recomputed at this request
    /// (membership is computed BEFORE the write mutex).
    fn find_candidate(&self, paper_id: &str) -> Option<crate::candidates::Candidate> {
        let papers = list_candidates(
            &self.started.canon,
            &self.started.dirfds,
            &self.started.kite,
            self.config().max_frame_pixels,
            self.config().max_open_bytes,
            &self.started.pinned_keys,
        );
        papers.into_iter().find(|c| c.id == paper_id)
    }

    /// Is `paper_id` in the candidate set recomputed at this request?
    fn is_candidate(&self, paper_id: &str) -> bool {
        self.find_candidate(paper_id).is_some()
    }

    /// The shared Scholar write-route preamble: 415 → 413 → viewer_only
    /// → JSON object → `paper_id` field. Returns the paper id, or the
    /// error response.
    fn paper_route_preamble(&self, req: &Request) -> Result<String, Response> {
        if let Some(r) = check_content_type(req) {
            return Err(r);
        }
        if let Some(r) = check_body_over_cap(req) {
            return Err(r);
        }
        if !self.is_scholar() {
            return Err(err(409, "viewer_only", "this is a Scholar route"));
        }
        let body = parse_json_body(req)?;
        req_string(&body, "paper_id")
    }

    // ---- POST /v1/select ----

    fn route_select(&self, req: &Request) -> Response {
        let paper_id = match self.paper_route_preamble(req) {
            Ok(id) => id,
            Err(r) => return r,
        };
        // Candidate membership is computed BEFORE the write mutex.
        let Some(cand) = self.find_candidate(&paper_id) else {
            return err(404, "unknown_paper", "paper is not in the candidate set");
        };
        // Step 10: the Scholar write path (steps 0/1/3; select writes
        // nothing durable).
        crate::write_path::run_select(self, &cand)
    }

    // ---- POST /v1/download ----

    fn route_download(&self, req: &Request) -> Response {
        let paper_id = match self.paper_route_preamble(req) {
            Ok(id) => id,
            Err(r) => return r,
        };
        let Some(cand) = self.find_candidate(&paper_id) else {
            return err(404, "unknown_paper", "paper is not in the candidate set");
        };
        // Step 10: the Scholar write path (steps 0/1/2/2b/3).
        crate::write_path::run_download(self, &cand)
    }

    // ---- POST /v1/skip ----

    fn route_skip(&self, req: &Request) -> Response {
        let paper_id = match self.paper_route_preamble(req) {
            Ok(id) => id,
            Err(r) => return r,
        };
        if !self.is_candidate(&paper_id) {
            return err(404, "unknown_paper", "paper is not in the candidate set");
        }
        // Skip writes nothing and does not take the write mutex.
        Response::json(200, serde_json::to_vec(&json!({"ok": true})).unwrap())
    }

    // ---- POST /v1/deselect ----

    fn route_deselect(&self, req: &Request) -> Response {
        if let Some(r) = check_content_type(req) {
            return r;
        }
        if let Some(r) = check_body_over_cap(req) {
            return r;
        }
        if !self.is_scholar() {
            return err(409, "viewer_only", "deselect is a Scholar route");
        }
        let body = match parse_json_body(req) {
            Ok(b) => b,
            Err(r) => return r,
        };
        // A `reason` property is rejected if present.
        if body.get("reason").is_some() {
            return err(
                400,
                "invalid_request",
                "deselect takes no reason field",
            );
        }
        let paper_id = match req_string(&body, "paper_id") {
            Ok(v) => v,
            Err(r) => return r,
        };
        // Candidate membership is computed BEFORE the write mutex.
        let Some(cand) = self.find_candidate(&paper_id) else {
            return err(404, "unknown_paper", "paper is not in the candidate set");
        };
        // Step 10: the Scholar write path (`already_included` →
        // `not_selected` → remove → step 3). A successful deselect
        // writes no tile-10 and no inclusion-index line.
        crate::write_path::run_deselect(self, &cand)
    }

    // ---- GET /v1/consent ----

    fn route_consent_get(&self, _req: &Request) -> Response {
        if !self.is_scholar() {
            return err(409, "viewer_only", "consent is a Scholar route");
        }
        let _guard = self.write_mu.lock().unwrap();
        let state = self.consent.lock().unwrap().clone();
        Response::json(
            200,
            serde_json::to_vec(&json!({
                "state": if state.on { "on" } else { "off" },
                "timestamp": state.timestamp,
            }))
            .unwrap(),
        )
    }

    // ---- POST /v1/consent ----

    fn route_consent_post(&self, req: &Request) -> Response {
        if let Some(r) = check_content_type(req) {
            return r;
        }
        if let Some(r) = check_body_over_cap(req) {
            return r;
        }
        if !self.is_scholar() {
            return err(409, "viewer_only", "consent is a Scholar route");
        }
        let body = match parse_json_body(req) {
            Ok(b) => b,
            Err(r) => return r,
        };
        let state_str = match req_string(&body, "state") {
            Ok(v) => v,
            Err(r) => return r,
        };
        let want_on = match state_str.as_str() {
            "on" => true,
            "off" => false,
            _ => {
                return err(400, "invalid_param", "state must be on or off");
            }
        };
        if self.config().lrs_url.is_none() {
            // AT-116: `on` without --lrs-url is 409 lrs_not_configured
            // (no file is created); `off` is a 200 off/null no-op.
            if want_on {
                return err(
                    409,
                    "lrs_not_configured",
                    "consent on requires --lrs-url",
                );
            }
            return Response::json(
                200,
                serde_json::to_vec(&json!({"state": "off", "timestamp": serde_json::Value::Null})).unwrap(),
            );
        }
        let _guard = self.write_mu.lock().unwrap();
        let current = self.consent.lock().unwrap().clone();
        if current.on == want_on {
            // Same-state POST is a no-op that keeps the timestamp (the
            // DD-144 latch actions on a same-state off are Step 11).
            return Response::json(
                200,
                serde_json::to_vec(&json!({
                    "state": if current.on { "on" } else { "off" },
                    "timestamp": current.timestamp,
                }))
                .unwrap(),
            );
        }
        let now = rfc3339_secs(std::time::SystemTime::now());
        let new_state = ConsentState {
            on: want_on,
            timestamp: Some(now.clone()),
        };
        let (Some(parent_fd), Some(basename)) = (
            self.started.dirfds.consent_parent,
            self.started.canon.consent_basename.clone(),
        ) else {
            return err(500, "internal", "consent file is not configured");
        };
        if let Err(e) = write_consent_file(parent_fd, &basename, new_state.clone()) {
            // DD-138 fail-closed: this is the last response the process
            // may write; it then stops all LRS sending and exits non-zero.
            self.fail_closed.store(true, Ordering::SeqCst);
            self.shutdown.store(true, Ordering::SeqCst);
            return err(500, "consent_write_failed", &format!("consent file write failed: {e}"));
        }
        {
            let mut consent = self.consent.lock().unwrap();
            *consent = new_state;
        }
        Response::json(
            200,
            serde_json::to_vec(&json!({
                "state": if want_on { "on" } else { "off" },
                "timestamp": now,
            }))
            .unwrap(),
        )
    }
}

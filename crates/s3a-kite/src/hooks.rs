//! Test hooks (§5.2.7, DD-151). Configured only by environment variables,
//! only in a `test_hooks` build. Release builds and builds without the
//! feature reject the variables at startup (AT-222).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

/// A parsed hook action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookAction {
    PauseUntilSignal,
    FailWithErrno(i32),
    FailOnceWithErrno(i32),
    Grow,
    Issue,
}

/// One `POINT=ACTION` entry.
#[derive(Debug, Clone)]
pub struct HookPoint {
    pub point: &'static str,
    pub action: HookAction,
}

/// The parsed hook configuration for this process.
#[derive(Debug, Clone, Default)]
pub struct Hooks {
    pub points: Vec<HookPoint>,
    pub ready_path: Option<PathBuf>,
    pub continue_path: Option<PathBuf>,
    once_fired: std::sync::Arc<Mutex<HashSet<&'static str>>>,
}

/// The closed hook-point list with accepted actions (§5.2.7).
pub const HOOK_POINTS: &[(&str, &[&str])] = &[
    ("pause_before_snapshot_rename", &["pause_until_signal"]),
    ("pause_before_name_binding_check", &["pause_until_signal"]),
    ("fail_revoke_pending_write", &["fail_with_errno", "fail_once_with_errno"]),
    ("fail_lrs_retry_delete", &["fail_with_errno", "fail_once_with_errno"]),
    ("fail_revoke_pending_remove", &["fail_with_errno", "fail_once_with_errno"]),
    ("fail_index_append", &["fail_with_errno", "fail_once_with_errno"]),
    ("fail_snapshot_write_temp", &["fail_with_errno", "fail_once_with_errno"]),
    ("fail_snapshot_write_fsync", &["fail_with_errno", "fail_once_with_errno"]),
    ("fail_snapshot_write_rename", &["fail_with_errno", "fail_once_with_errno"]),
    ("fail_snapshot_index_append", &["fail_with_errno", "fail_once_with_errno"]),
    ("fail_download_write", &["fail_with_errno", "fail_once_with_errno"]),
    ("grow_drive_path_during_copy", &["grow"]),
    ("window_issue_select", &["issue"]),
    ("pause_before_lrs_retry_write", &["pause_until_signal"]),
    ("pause_before_consent_rename", &["pause_until_signal"]),
    ("pause_after_consent_receipt", &["pause_until_signal"]),
    ("fail_lrs_retry_write", &["fail_with_errno", "fail_once_with_errno"]),
];

fn hook_point(name: &str) -> Option<(&'static str, &'static [&'static str])> {
    HOOK_POINTS.iter().find(|(n, _)| *n == name).copied()
}

impl Hooks {
    /// Read the hook configuration from the environment. In a build without
    /// the `test_hooks` feature, any of the three variables set (even to an
    /// empty value) is a startup error naming the variable (AT-222).
    pub fn from_env() -> Result<Hooks, String> {
        if !cfg!(feature = "test_hooks") {
            for var in [
                "S3A_KITE_TEST_HOOKS",
                "S3A_KITE_TEST_HOOK_READY",
                "S3A_KITE_TEST_HOOK_CONTINUE",
            ] {
                if std::env::var_os(var).is_some() {
                    return Err(format!(
                        "{var}: test hooks are not compiled into this build (no test_hooks feature)"
                    ));
                }
            }
            return Ok(Hooks::default());
        }
        let raw = match std::env::var("S3A_KITE_TEST_HOOKS") {
            Ok(v) => v,
            Err(_) => return Ok(Hooks::default()),
        };
        let mut hooks = Hooks::default();
        if raw.is_empty() {
            return Ok(hooks);
        }
        let mut needs_signals = false;
        for entry in raw.split(',') {
            let (point, action) = entry.split_once('=').ok_or_else(|| {
                format!("S3A_KITE_TEST_HOOKS: malformed entry {entry:?} (expected POINT=ACTION)")
            })?;
            if point.is_empty() || action.is_empty() {
                return Err(format!(
                    "S3A_KITE_TEST_HOOKS: malformed entry {entry:?}"
                ));
            }
            let (name, accepted) = hook_point(point).ok_or_else(|| {
                format!("S3A_KITE_TEST_HOOKS: unknown hook point {point:?}")
            })?;
            let parsed = parse_action(action, accepted).map_err(|e| {
                format!("S3A_KITE_TEST_HOOKS: {e}")
            })?;
            if parsed == HookAction::PauseUntilSignal {
                needs_signals = true;
            }
            hooks.points.push(HookPoint {
                point: name,
                action: parsed,
            });
        }
        if needs_signals {
            let ready = std::env::var("S3A_KITE_TEST_HOOK_READY").map_err(|_| {
                "S3A_KITE_TEST_HOOKS: pause_until_signal requires S3A_KITE_TEST_HOOK_READY".to_string()
            })?;
            let cont = std::env::var("S3A_KITE_TEST_HOOK_CONTINUE").map_err(|_| {
                "S3A_KITE_TEST_HOOKS: pause_until_signal requires S3A_KITE_TEST_HOOK_CONTINUE".to_string()
            })?;
            if ready.is_empty() || cont.is_empty() || ready == cont {
                return Err(
                    "S3A_KITE_TEST_HOOKS: READY and CONTINUE must be two distinct absolute paths"
                        .to_string(),
                );
            }
            if !PathBuf::from(&ready).is_absolute() || !PathBuf::from(&cont).is_absolute() {
                return Err(
                    "S3A_KITE_TEST_HOOKS: READY and CONTINUE must be two distinct absolute paths"
                        .to_string(),
                );
            }
            hooks.ready_path = Some(PathBuf::from(ready));
            hooks.continue_path = Some(PathBuf::from(cont));
        }
        Ok(hooks)
    }

    /// Is `window_issue_select` configured (the Step 9 firing point)?
    pub fn window_issue_select(&self) -> bool {
        self.points
            .iter()
            .any(|p| p.point == "window_issue_select")
    }

    /// The errno to fail with at `point`, if a fail hook is configured
    /// there. `fail_once_with_errno` fires only the first time.
    pub fn fail_errno(&self, point: &str) -> Option<i32> {
        for p in &self.points {
            if p.point != point {
                continue;
            }
            match p.action {
                HookAction::FailWithErrno(n) => return Some(n),
                HookAction::FailOnceWithErrno(n) => {
                    let mut fired = self.once_fired.lock().unwrap();
                    if fired.insert(p.point) {
                        return Some(n);
                    }
                    return None;
                }
                _ => {}
            }
        }
        None
    }

    /// Is a pause hook configured at `point`?
    pub fn pause_at(&self, point: &str) -> bool {
        self.points
            .iter()
            .any(|p| p.point == point && p.action == HookAction::PauseUntilSignal)
    }

    /// Is a `grow` action configured at `point`? SPEC §5.2.7 (line
    /// 523): `grow` — valid only on `grow_drive_path_during_copy`.
    /// During the step-2b copy the HOST extends the already-opened
    /// `drive_path` so that more than `--max-open-bytes` arrive. It is
    /// NOT a pause: no READY/CONTINUE signal is involved (the host
    /// grows the file itself), and `from_env` does not set
    /// `needs_signals` for a `Grow` action.
    pub fn grow_at(&self, point: &str) -> bool {
        self.points
            .iter()
            .any(|p| p.point == point && p.action == HookAction::Grow)
    }

    /// Fire a `pause_until_signal` hook: create/replace READY with the
    /// hook-point name + newline, wait until CONTINUE exists as a
    /// non-empty regular file, then delete CONTINUE.
    pub fn wait_pause(&self, point: &str) {
        let (Some(ready), Some(cont)) = (&self.ready_path, &self.continue_path) else {
            return;
        };
        let _ = std::fs::write(ready, format!("{point}\n"));
        loop {
            if let Ok(meta) = std::fs::metadata(cont) {
                if meta.is_file() && meta.len() > 0 {
                    let _ = std::fs::remove_file(cont);
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

fn parse_action(action: &str, accepted: &[&str]) -> Result<HookAction, String> {
    let base = if action == "pause_until_signal" || action == "grow" || action == "issue" {
        action
    } else if action.starts_with("fail_with_errno=") {
        "fail_with_errno"
    } else if action.starts_with("fail_once_with_errno=") {
        "fail_once_with_errno"
    } else {
        return Err(format!("unknown action {action:?}"));
    };
    if !accepted.contains(&base) {
        return Err(format!("action {base:?} is not accepted on this point"));
    }
    if action == "pause_until_signal" {
        return Ok(HookAction::PauseUntilSignal);
    }
    if action == "grow" {
        return Ok(HookAction::Grow);
    }
    if action == "issue" {
        return Ok(HookAction::Issue);
    }
    if let Some(n) = action.strip_prefix("fail_with_errno=") {
        return Ok(HookAction::FailWithErrno(parse_errno(n)?));
    }
    if let Some(n) = action.strip_prefix("fail_once_with_errno=") {
        return Ok(HookAction::FailOnceWithErrno(parse_errno(n)?));
    }
    Err(format!("unknown action {action:?}"))
}

fn parse_errno(s: &str) -> Result<i32, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("errno must be a positive decimal, got {s:?}"));
    }
    let n: i64 = s.parse().map_err(|_| format!("errno out of range: {s:?}"))?;
    if n <= 0 || n > i32::MAX as i64 {
        return Err(format!("errno must be positive, got {s:?}"));
    }
    Ok(n as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_list_rejects_unknown_point() {
        // parse_action is unit-tested through the point table, not the env.
        assert!(hook_point("no_such_point").is_none());
        assert!(hook_point("window_issue_select").is_some());
        assert_eq!(
            parse_action("issue", &["issue"]).unwrap(),
            HookAction::Issue
        );
        assert!(parse_action("pause_until_signal", &["issue"]).is_err());
        assert_eq!(
            parse_action("fail_with_errno=28", &["fail_with_errno"]).unwrap(),
            HookAction::FailWithErrno(28)
        );
        assert!(parse_action("fail_with_errno=0", &["fail_with_errno"]).is_err());
        assert!(parse_action("fail_with_errno=x", &["fail_with_errno"]).is_err());
    }
}

//! `s3a-kite host` — the S3A Navigator host binary (SPEC §5.2.1/§5.2.2).

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use s3a_kite::flags::{startup, Listener};
use s3a_kite::http::{serve_tcp, serve_unix, ServerConfig};
use s3a_kite::server::Host;
use s3a_kite::timefmt::rfc3339_secs;

/// The shutdown flag, shared with the signal handler.
static SHUTDOWN_ARC: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

extern "C" fn on_signal(_sig: libc::c_int) {
    if let Some(flag) = SHUTDOWN_ARC.get() {
        flag.store(true, Ordering::SeqCst);
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let shutdown = Arc::new(AtomicBool::new(false));
    let _ = SHUTDOWN_ARC.set(shutdown.clone());
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as usize);
        libc::signal(libc::SIGINT, on_signal as *const () as usize);
    }
    let started = match startup(&argv) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("s3a-kite: startup error: {e}");
            std::process::exit(2);
        }
    };
    let host = Arc::new(Host::new(started, shutdown.clone()));
    // Step 11: the background LRS retry worker (DD-51, AT-236) — one
    // detached thread per Scholar host with --lrs-url. It exits when
    // the host shuts down (or the process exits).
    let _retry_worker = if host.config().lrs_url.is_some() {
        Some(s3a_kite::retry::spawn_retry_worker(&host))
    } else {
        None
    };
    // The native-window client (native-window builds without --headless).
    // It only performs the window_issue_select hook action; the window
    // rendering itself is not implemented in this build.
    #[cfg(feature = "native-window")]
    if !host.config().headless {
        s3a_kite::window::start_window_client(
            host.config(),
            &host.started.token_secret,
            host.config().hooks.window_issue_select(),
            shutdown.clone(),
        );
    }
    let server_config = Arc::new(ServerConfig {
        tls: host.started.tls_config.clone(),
        max_request_bytes: host.config().max_request_bytes,
        handle_idle: Duration::from_secs(host.config().handle_idle_secs),
        shutdown: shutdown.clone(),
    });
    match &host.started.listener {
        Listener::Tcp(l) => {
            let cloned = l.try_clone().expect("TcpListener::try_clone");
            serve_tcp(cloned, server_config, host.clone());
        }
        Listener::Unix(l) => {
            use std::os::unix::io::{AsRawFd, FromRawFd};
            let fd = unsafe { libc::dup(l.as_raw_fd()) };
            assert!(fd >= 0, "UnixListener dup failed");
            let cloned = unsafe { std::os::unix::net::UnixListener::from_raw_fd(fd) };
            let _ = cloned.set_nonblocking(true);
            serve_unix(cloned, server_config, host.clone());
        }
    }
    finish(&host);
}

fn finish(host: &Arc<Host>) -> ! {
    // The shutdown log line (startup/shutdown/5xx only, never per-request).
    let line = format!("{} shutdown\n", rfc3339_secs(std::time::SystemTime::now()));
    if let Ok(mut f) = host.started.log_file.lock() {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }
    if host.fail_closed.load(Ordering::SeqCst) {
        std::process::exit(1);
    }
    std::process::exit(0);
}

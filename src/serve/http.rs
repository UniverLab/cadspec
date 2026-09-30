//! HTTP layer — one connection per request: parse the request line, dispatch
//! to a per-route handler, write the response.

use super::inspect::{
    entity_block_json, html_escape, query_param, read_request_body, safe_cf_name,
};
use super::page::{index_html, FAVICON_SVG};
use super::{rebuild, render_named_plano, scene_gltf, Shared, SSE_KEEPALIVE};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

pub(super) fn handle_connection(stream: TcpStream, state: &Shared) -> std::io::Result<()> {
    // Small localhost responses: Nagle's algorithm only adds latency here.
    let _ = stream.set_nodelay(true);
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    let target = request_line.split_whitespace().nth(1).unwrap_or("/");
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };

    match path {
        "/" => serve_index(stream, state),
        "/preview.svg" => serve_preview_svg(stream, state),
        "/preview3d.svg" => serve_preview3d_svg(stream, state),
        "/scene.gltf" => serve_scene_gltf(stream, state),
        "/plano.svg" => serve_plano_svg(stream, state, query),
        "/state" => serve_state(stream, state),
        "/entity" => serve_entity(stream, state, query),
        "/files" => serve_files_list(stream, state),
        "/file" => serve_file_body(stream, state, query),
        "/save" => serve_save_body(stream, state, query, &mut reader),
        "/events" => serve_events(stream, state),
        "/favicon.svg" => serve_favicon(stream),
        _ => respond(stream, "404 Not Found", "text/plain", b"not found"),
    }
}

fn serve_index(stream: TcpStream, state: &Shared) -> std::io::Result<()> {
    let html = index_html(&state.state.lock().unwrap().project_name);
    respond(
        stream,
        "200 OK",
        "text/html; charset=utf-8",
        html.as_bytes(),
    )
}

fn serve_preview_svg(stream: TcpStream, state: &Shared) -> std::io::Result<()> {
    let svg = Arc::clone(&state.state.lock().unwrap().svg);
    respond(stream, "200 OK", "image/svg+xml", svg.as_bytes())
}

fn serve_preview3d_svg(stream: TcpStream, state: &Shared) -> std::io::Result<()> {
    let svg = Arc::clone(&state.state.lock().unwrap().svg3d);
    respond(stream, "200 OK", "image/svg+xml", svg.as_bytes())
}

fn serve_scene_gltf(stream: TcpStream, state: &Shared) -> std::io::Result<()> {
    let body = scene_gltf(&state.project_dir).unwrap_or_else(|_| crate::gltf::scene_to_gltf(&[]));
    respond(stream, "200 OK", "model/gltf+json", body.as_bytes())
}

fn serve_plano_svg(stream: TcpStream, state: &Shared, query: &str) -> std::io::Result<()> {
    // Rendered on demand (sections run CSG, so we don't precompute all).
    let name = query_param(query, "name").unwrap_or_default();
    let dir = &state.project_dir;
    let svg = render_named_plano(dir, &name).unwrap_or_else(|e| {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="200"><rect width="100%" height="100%" fill="#0d0d0d"/><text x="20" y="40" fill="#ff9f9a" font-family="monospace" font-size="14">plano error: {}</text></svg>"##,
            html_escape(&format!("{e:#}"))
        )
    });
    respond(stream, "200 OK", "image/svg+xml", svg.as_bytes())
}

fn serve_state(stream: TcpStream, state: &Shared) -> std::io::Result<()> {
    let st = state.state.lock().unwrap();
    let layers: Vec<_> = st
        .layers
        .iter()
        .map(|(name, color)| serde_json::json!({"name": name, "color": color}))
        .collect();
    let planos: Vec<_> = st
        .planos
        .iter()
        .map(|(name, view, title)| serde_json::json!({"name": name, "view": view, "title": title}))
        .collect();
    let body = serde_json::json!({
        "version": st.version,
        "project": st.project_name,
        "error": st.error,
        "layers": layers,
        "planos": planos,
    })
    .to_string();
    drop(st);
    respond(stream, "200 OK", "application/json", body.as_bytes())
}

fn serve_entity(stream: TcpStream, state: &Shared, query: &str) -> std::io::Result<()> {
    let id = query_param(query, "id").unwrap_or_default();
    let body = entity_block_json(&state.project_dir, &id);
    respond(stream, "200 OK", "application/json", body.as_bytes())
}

// ── Built-in .cf editor ────────────────────────────────────────────────
fn serve_files_list(stream: TcpStream, state: &Shared) -> std::io::Result<()> {
    let mut names: Vec<String> = std::fs::read_dir(&state.project_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".cf"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let body = serde_json::json!({ "files": names }).to_string();
    respond(stream, "200 OK", "application/json", body.as_bytes())
}

fn serve_file_body(stream: TcpStream, state: &Shared, query: &str) -> std::io::Result<()> {
    let name = query_param(query, "name").unwrap_or_default();
    match safe_cf_name(&name) {
        Some(n) => match std::fs::read_to_string(state.project_dir.join(&n)) {
            Ok(s) => respond(stream, "200 OK", "text/plain; charset=utf-8", s.as_bytes()),
            Err(_) => respond(stream, "404 Not Found", "text/plain", b"not found"),
        },
        None => respond(stream, "400 Bad Request", "text/plain", b"bad name"),
    }
}

fn serve_save_body(
    stream: TcpStream,
    state: &Shared,
    query: &str,
    reader: &mut BufReader<TcpStream>,
) -> std::io::Result<()> {
    let name = query_param(query, "name").unwrap_or_default();
    let body = read_request_body(reader);
    match safe_cf_name(&name) {
        Some(n) => match std::fs::write(state.project_dir.join(&n), &body) {
            Ok(()) => {
                rebuild(&state.project_dir, state);
                let st = state.state.lock().unwrap();
                let resp = serde_json::json!({
                    "ok": st.error.is_none(),
                    "version": st.version,
                    "error": st.error,
                })
                .to_string();
                drop(st);
                respond(stream, "200 OK", "application/json", resp.as_bytes())
            }
            Err(e) => respond(
                stream,
                "500 Internal Server Error",
                "text/plain",
                format!("write error: {e}").as_bytes(),
            ),
        },
        None => respond(stream, "400 Bad Request", "text/plain", b"bad name"),
    }
}

fn serve_favicon(stream: TcpStream) -> std::io::Result<()> {
    respond(stream, "200 OK", "image/svg+xml", FAVICON_SVG.as_bytes())
}

fn respond(
    mut stream: TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

/// Server-sent events: the condvar wakes us the instant a rebuild lands, so
/// the browser is notified with sub-millisecond latency instead of polling.
fn serve_events(stream: TcpStream, state: &Shared) -> std::io::Result<()> {
    serve_events_with_keepalive(stream, state, SSE_KEEPALIVE)
}

/// Same SSE stream with the keep-alive interval injected, so tests can drive
/// the timeout branch without waiting out the production 15 s window.
fn serve_events_with_keepalive(
    mut stream: TcpStream,
    state: &Shared,
    keepalive: Duration,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\n"
    )?;
    stream.flush()?;

    let mut last = 0u64;
    loop {
        let current = {
            let mut st = state.state.lock().unwrap();
            while st.version == last {
                let (guard, timeout) = state
                    .changed
                    .wait_timeout(st, keepalive)
                    .map_err(|_| std::io::Error::other("state poisoned"))?;
                st = guard;
                if timeout.timed_out() && st.version == last {
                    drop(st);
                    // Keep-alive comment so dead clients are detected.
                    write!(stream, ": ping\n\n")?;
                    stream.flush()?;
                    st = state.state.lock().unwrap();
                }
            }
            st.version
        };
        last = current;
        write!(stream, "data: {}\n\n", current)?;
        stream.flush()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::{Live, LiveState};
    use std::io::Read;
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Condvar, Mutex};
    use std::time::{Duration, Instant};

    // ── Fixture: a minimal project that actually builds ─────────────────

    const PROJECT_TOML: &str = r#"[project]
name = "Test"
units = "m"

[layers]
muros = { file = "muros.cf" }
"#;

    const MUROS_CF: &str = r#"[layer]
name = "muros"

[[line]]
id = "ln-1"
from = [0.0, 0.0]
to = [4.0, 0.0]
"#;

    /// Unique temp project per test; removed on drop even when an assert
    /// panics, so failures never leave `/tmp/cadspec_http_*` behind.
    struct TempProject(PathBuf);

    impl TempProject {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "cadspec_http_{}_{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("project.toml"), PROJECT_TOML).unwrap();
            std::fs::write(dir.join("muros.cf"), MUROS_CF).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn test_state(dir: &Path) -> Shared {
        Arc::new(Live {
            state: Mutex::new(LiveState {
                svg: Arc::new("<svg>ok</svg>".to_string()),
                svg3d: Arc::new("<svg>3d</svg>".to_string()),
                error: None,
                version: 1,
                project_name: "Test".to_string(),
                layers: vec![("muros".to_string(), "#FF0000".to_string())],
                planos: Vec::new(),
            }),
            changed: Condvar::new(),
            project_dir: dir.to_path_buf(),
        })
    }

    // ── Raw-HTTP loopback harness ──────────────────────────────────────

    /// Per-recv timeout; every read below is additionally capped by an
    /// absolute deadline, so a handler that writes nothing (or hangs) can
    /// never stall the suite — the read returns partial/empty instead.
    const READ_CHUNK: Duration = Duration::from_millis(300);
    const READ_DEADLINE: Duration = Duration::from_secs(2);
    /// Stop capturing runaway responses (spin-loop mutants) before they
    /// exhaust memory; well above any legitimate body here.
    const MAX_CAPTURE: usize = 1024 * 1024;

    /// Bind an ephemeral loopback port, spawn the handler for a single
    /// connection, and return the client end of the socket.
    fn connect(state: &Shared) -> TcpStream {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::clone(state);
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let _ = handle_connection(stream, &state);
            }
        });
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(READ_CHUNK)).unwrap();
        stream
    }

    /// Like `connect`, but serves the SSE handler directly with a test-sized
    /// keep-alive interval.
    fn connect_events(state: &Shared, keepalive: Duration) -> TcpStream {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::clone(state);
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let _ = serve_events_with_keepalive(stream, &state, keepalive);
            }
        });
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(READ_CHUNK)).unwrap();
        stream
    }

    /// Read until `needle` shows up, or EOF, or `deadline`, or the capture
    /// cap. Empty `needle` means "read to EOF/deadline".
    fn read_until(stream: &mut TcpStream, needle: &str, deadline: Instant) -> String {
        let mut out = String::new();
        let mut buf = [0u8; 4096];
        while Instant::now() < deadline && out.len() < MAX_CAPTURE {
            match stream.read(&mut buf) {
                Ok(0) => break, // EOF: handler finished (or wrote nothing)
                Ok(n) => {
                    out.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if !needle.is_empty() && out.contains(needle) {
                        break;
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    // Bounded recv timeout: poll again until the deadline.
                }
                Err(_) => break, // reset: keep what was already received
            }
        }
        out
    }

    fn parse_response(raw: &str) -> (u16, String) {
        let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
        let status = head
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);
        (status, body.to_string())
    }

    fn get(state: &Shared, path: &str) -> (u16, String) {
        let mut stream = connect(state);
        let _ = stream.write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        );
        let raw = read_until(&mut stream, "", Instant::now() + READ_DEADLINE);
        parse_response(&raw)
    }

    fn post(state: &Shared, path: &str, body: &str) -> (u16, String) {
        let mut stream = connect(state);
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(request.as_bytes());
        let raw = read_until(&mut stream, "", Instant::now() + READ_DEADLINE);
        parse_response(&raw)
    }

    // ── Route tests: each one covers the match arm AND the handler body ─

    #[test]
    fn index_route_serves_the_viewer_page() {
        let dir = TempProject::new();
        let state = test_state(dir.path());
        let (status, body) = get(&state, "/");
        assert_eq!(status, 200, "GET / failed: {body}");
        assert!(body.contains("<html"), "expected HTML, got: {body:?}");
        assert!(
            body.contains("Test"),
            "expected the project title, got: {body:?}"
        );
    }

    #[test]
    fn preview3d_route_serves_the_3d_svg() {
        let dir = TempProject::new();
        let state = test_state(dir.path());
        let (status, body) = get(&state, "/preview3d.svg");
        assert_eq!(status, 200, "GET /preview3d.svg failed: {body}");
        assert!(
            body.contains("3d"),
            "expected the svg3d payload, got: {body:?}"
        );
    }

    #[test]
    fn scene_gltf_route_serves_a_gltf_document() {
        let dir = TempProject::new();
        let state = test_state(dir.path());
        let (status, body) = get(&state, "/scene.gltf");
        assert_eq!(status, 200, "GET /scene.gltf failed: {body}");
        assert!(body.contains("asset"), "expected glTF JSON, got: {body:?}");
    }

    #[test]
    fn plano_route_renders_or_explains_in_svg() {
        let dir = TempProject::new();
        let state = test_state(dir.path());
        let (status, body) = get(&state, "/plano.svg?name=nope");
        assert_eq!(status, 200, "GET /plano.svg failed: {body}");
        assert!(
            body.contains("<svg"),
            "expected an SVG (even on error), got: {body:?}"
        );
    }

    #[test]
    fn entity_route_returns_the_source_block_as_json() {
        let dir = TempProject::new();
        let state = test_state(dir.path());
        let (status, body) = get(&state, "/entity?id=ln-1");
        assert_eq!(status, 200, "GET /entity failed: {body}");
        let json: serde_json::Value = serde_json::from_str(&body)
            .unwrap_or_else(|e| panic!("entity response must be JSON ({e}): {body}"));
        assert_eq!(json["layer"].as_str(), Some("muros"), "body: {body}");
        assert_eq!(json["base_id"].as_str(), Some("ln-1"), "body: {body}");
        assert!(
            json["block"]
                .as_str()
                .unwrap_or_default()
                .contains("[[line]]"),
            "expected the raw TOML block, body: {body}"
        );
    }

    #[test]
    fn files_route_lists_the_cf_files() {
        let dir = TempProject::new();
        let state = test_state(dir.path());
        let (status, body) = get(&state, "/files");
        assert_eq!(status, 200, "GET /files failed: {body}");
        assert!(
            body.contains("muros.cf"),
            "expected the layer file listed, got: {body:?}"
        );
    }

    #[test]
    fn file_route_serves_contents_and_rejects_bad_names() {
        let dir = TempProject::new();
        let state = test_state(dir.path());
        let (status, body) = get(&state, "/file?name=muros.cf");
        assert_eq!(status, 200, "GET /file failed: {body}");
        assert!(
            body.contains("[layer]"),
            "expected the file contents, got: {body:?}"
        );

        let (bad_status, bad_body) = get(&state, "/file?name=bad%2Fname.cf");
        assert_eq!(
            bad_status, 400,
            "path-ish names must be rejected: {bad_body}"
        );
    }

    #[test]
    fn save_route_writes_the_body_and_reports_ok() {
        let dir = TempProject::new();
        let state = test_state(dir.path());
        let new_content = "[layer]\nname = \"muros\"\n\n[[line]]\nid = \"ln-1\"\nfrom = [0.0, 0.0]\nto = [5.5, 0.0]\n";
        let (status, body) = post(&state, "/save?name=muros.cf", new_content);
        assert_eq!(status, 200, "POST /save failed: {body}");
        let json: serde_json::Value = serde_json::from_str(&body)
            .unwrap_or_else(|e| panic!("save response must be JSON ({e}): {body}"));
        assert_eq!(
            json["ok"].as_bool(),
            Some(true),
            "rebuild must succeed: {body}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("muros.cf")).unwrap(),
            new_content,
            "POST /save must persist the body to disk"
        );
    }

    // ── SSE /events ────────────────────────────────────────────────────

    #[test]
    fn events_route_streams_headers_and_the_current_version() {
        let dir = TempProject::new();
        let state = test_state(dir.path()); // version = 1 vs. `last = 0`
        let mut stream = connect(&state);
        let _ = stream
            .write_all(b"GET /events HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        let got = read_until(&mut stream, "data: 1", Instant::now() + READ_DEADLINE);
        assert!(
            got.contains("text/event-stream"),
            "GET /events must open an SSE stream, got: {got:?}"
        );
        assert!(
            got.contains("data: 1"),
            "GET /events must immediately push the current version (1), got: {got:?}"
        );
    }

    #[test]
    fn events_keepalive_pings_an_idle_client() {
        let dir = TempProject::new();
        let state = test_state(dir.path());
        state.state.lock().unwrap().version = 0; // park the loop on `last = 0`
        let mut stream = connect_events(&state, Duration::from_millis(100));
        let got = read_until(
            &mut stream,
            ": ping",
            Instant::now() + Duration::from_millis(1500),
        );
        assert!(
            got.contains(": ping"),
            "an idle SSE client must get a keep-alive ping within 1.5s, got: {got:?}"
        );
    }

    #[test]
    fn events_wake_without_version_change_writes_nothing() {
        let dir = TempProject::new();
        let state = test_state(dir.path());
        state.state.lock().unwrap().version = 0;
        let mut stream = connect_events(&state, Duration::from_secs(10));

        // Drain the opening headers first, so the observation window below
        // only contains loop writes.
        let headers = read_until(&mut stream, "\r\n\r\n", Instant::now() + READ_DEADLINE);
        assert!(
            headers.contains("text/event-stream"),
            "SSE headers missing: {headers:?}"
        );

        // Hammer the condvar WITHOUT bumping the version. Repeated notifies
        // make the wake deterministic even if one lands before the wait.
        let notifier = {
            let state = Arc::clone(&state);
            std::thread::spawn(move || {
                for _ in 0..14 {
                    state.changed.notify_all();
                    std::thread::sleep(Duration::from_millis(50));
                }
            })
        };

        let got = read_until(
            &mut stream,
            ": ping",
            Instant::now() + Duration::from_millis(700),
        );
        assert!(
            got.is_empty(),
            "a condvar wake without a version change must not write anything, got: {got:?}"
        );
        let _ = notifier.join();
    }
}

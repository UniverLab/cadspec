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
fn serve_events(mut stream: TcpStream, state: &Shared) -> std::io::Result<()> {
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
                    .wait_timeout(st, SSE_KEEPALIVE)
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

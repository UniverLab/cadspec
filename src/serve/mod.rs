//! Live preview server — `cadspec serve`.
//!
//! Watches the project files and serves an auto-reloading SVG preview in the
//! browser. The vibecoding loop: an agent (or human) edits `.cf` files, the
//! browser refreshes instantly, build errors show as an overlay.
//!
//! Viewer features: pan/zoom, click-to-inspect any entity (shows its source
//! TOML block, copyable for targeted agent edits), per-layer visibility with
//! a ghost mode for tracing over other floors, and an extruded 3D view.
//!
//! Plain `std::net` HTTP — this is a localhost dev server, no framework needed.

use crate::parser::parse_project;
use crate::render3d::render_scene_3d;
use crate::svg::{layer_display_color, load_project_layers, render_scene_from};
use anyhow::{Context, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

mod daemon;
mod http;
mod inspect;
mod page;

pub use daemon::{serve_daemon, serve_stop};

use http::handle_connection;
use inspect::open_browser;

const SVG_WIDTH: u32 = 1600;
const DEBOUNCE: Duration = Duration::from_millis(80);
const SSE_KEEPALIVE: Duration = Duration::from_secs(15);

struct LiveState {
    /// Arc so request handlers serve the SVG without copying it.
    svg: Arc<String>,
    /// Extruded axonometric 3D render of the same scene.
    svg3d: Arc<String>,
    error: Option<String>,
    version: u64,
    project_name: String,
    /// (name, color) per layer, for the layer panel.
    layers: Vec<(String, String)>,
    /// (name, view, title) per plano, for the planos panel.
    planos: Vec<(String, String, String)>,
}

/// Shared state plus a condvar so SSE clients are woken the instant a rebuild
/// lands, instead of polling.
struct Live {
    state: Mutex<LiveState>,
    changed: Condvar,
    project_dir: PathBuf,
}

type Shared = Arc<Live>;

/// Start the live preview server (blocks until killed).
pub fn serve_project(project_dir: &Path, port: u16, open: bool) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let project_dir = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());

    let state: Shared = Arc::new(Live {
        state: Mutex::new(LiveState {
            svg: Arc::new(String::new()),
            svg3d: Arc::new(String::new()),
            error: None,
            version: 0,
            project_name: project.project.name.clone(),
            layers: Vec::new(),
            planos: Vec::new(),
        }),
        changed: Condvar::new(),
        project_dir: project_dir.clone(),
    });

    rebuild(&project_dir, &state);

    let listener = TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("Cannot bind 127.0.0.1:{} (port in use?)", port))?;
    let url = format!("http://127.0.0.1:{}", port);

    println!("◉ cadspec serve — {}", project.project.name);
    println!("  Preview: {}", url);
    println!("  Watching: {}", project_dir.display());
    println!();
    println!("  Edit .cf files — the browser updates automatically.");
    println!("  Click an entity in the viewer to inspect/copy its TOML.");
    println!("  Press Ctrl+C to stop.");

    spawn_watcher(project_dir.clone(), Arc::clone(&state))?;

    if open {
        open_browser(&url);
    }

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            let _ = handle_connection(stream, &state);
        });
    }
    Ok(())
}

fn rebuild(project_dir: &Path, state: &Shared) {
    type PlanoInfo = Vec<(String, String, String)>;
    type Built = (String, String, Vec<(String, String)>, PlanoInfo);
    let result = (|| -> Result<Built> {
        let (project, layers) = load_project_layers(project_dir, None)?;
        let scene = render_scene_from(
            &project.project.name,
            &project.project.units,
            &layers,
            SVG_WIDTH,
            &[],
        );
        let scene3d = render_scene_3d(&layers, SVG_WIDTH);
        let layer_info = layers
            .iter()
            .enumerate()
            .map(|(i, (name, cf))| (name.clone(), layer_display_color(cf, i)))
            .collect();
        let plano_info = project
            .planos
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    p.view.clone(),
                    p.title.clone().unwrap_or_else(|| p.name.clone()),
                )
            })
            .collect();
        Ok((scene.svg, scene3d.svg, layer_info, plano_info))
    })();

    let mut st = state.state.lock().unwrap();
    match result {
        Ok((svg, svg3d, layers, planos)) => {
            st.svg = Arc::new(svg);
            st.svg3d = Arc::new(svg3d);
            st.layers = layers;
            st.planos = planos;
            st.error = None;
        }
        Err(e) => {
            st.error = Some(format!("{:#}", e));
        }
    }
    st.version += 1;
    drop(st);
    state.changed.notify_all();
}

/// Build the scene's 3D solids as a glTF document (for the WebGL viewer).
fn scene_gltf(project_dir: &Path) -> Result<String> {
    let (_project, layers) = load_project_layers(project_dir, None)?;
    let meshes = crate::render3d::scene_meshes(&layers);
    Ok(crate::gltf::scene_to_gltf(&meshes))
}

/// Render a plano by name to SVG (on demand, for the `/plano.svg` endpoint).
fn render_named_plano(project_dir: &Path, name: &str) -> Result<String> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let plano = project
        .planos
        .iter()
        .find(|p| p.name == name)
        .with_context(|| format!("no plano named '{name}'"))?;
    Ok(crate::planos::render_plano(project_dir, plano, SVG_WIDTH)?.svg)
}

fn spawn_watcher(project_dir: PathBuf, state: Shared) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        notify::Config::default(),
    )?;
    watcher.watch(&project_dir, RecursiveMode::NonRecursive)?;

    std::thread::spawn(move || {
        // Keep the watcher alive inside the thread.
        let _watcher = watcher;
        while let Ok(event) = rx.recv() {
            if !is_relevant(&event) {
                continue;
            }
            // Debounce: absorb the burst of events an editor save produces.
            std::thread::sleep(DEBOUNCE);
            while rx.try_recv().is_ok() {}

            rebuild(&project_dir, &state);
            let st = state.state.lock().unwrap();
            match &st.error {
                None => println!("⟳ rebuilt (v{})", st.version),
                Some(e) => println!("✗ build error (v{}): {}", st.version, e),
            }
        }
    });
    Ok(())
}

fn is_relevant(event: &Event) -> bool {
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {}
        _ => return false,
    }
    event.paths.iter().any(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "cf" || e == "toml")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevant_events_filter_by_extension() {
        use notify::event::{CreateKind, EventAttributes};
        let mut event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from("/p/muros.cf")],
            attrs: EventAttributes::new(),
        };
        assert!(is_relevant(&event));
        event.paths = vec![PathBuf::from("/p/output.dxf")];
        assert!(!is_relevant(&event));
        event.paths = vec![PathBuf::from("/p/preview.svg")];
        assert!(!is_relevant(&event));
    }
}

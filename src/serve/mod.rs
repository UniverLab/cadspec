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
    use std::time::Instant;

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

    #[test]
    fn scene_gltf_returns_a_nonempty_gltf_document() {
        let gltf =
            scene_gltf(Path::new("examples/vivienda")).expect("vivienda should render to glTF");
        // Kills a `Ok(String::new())` rewrite: an empty document has neither
        // the JSON payload nor the glTF "asset" marker.
        assert!(!gltf.is_empty(), "gltf: {}", gltf);
        assert!(gltf.contains("\"asset\""), "gltf head: {}", gltf);
    }

    #[test]
    fn render_named_plano_renders_only_declared_sheets() {
        let dir = std::env::temp_dir().join(format!("cadspec_serve_plano_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Copy examples/vivienda's sources (project.toml + *.cf) only.
        for entry in std::fs::read_dir("examples/vivienda").unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_os_string();
            let is_source = name == "project.toml" || path.extension().is_some_and(|e| e == "cf");
            if path.is_file() && is_source {
                std::fs::copy(&path, dir.join(&name)).unwrap();
            }
        }
        // examples/vivienda declares no [[plano]]; append one so the name
        // lookup has a sheet to find.
        let mut project_toml = std::fs::read_to_string(dir.join("project.toml")).unwrap();
        project_toml.push_str(
            r#"
[[plano]]
name = "P-01"
view = "plan"
size = [420.0, 297.0]
scale = "1:50"
title = "X"
"#,
        );
        std::fs::write(dir.join("project.toml"), project_toml).unwrap();

        let svg = render_named_plano(&dir, "P-01").expect("declared plano renders");
        assert!(svg.contains("<svg"), "svg len: {}", svg.len());

        // Unknown names are an error (an `Ok(String::new())` rewrite would
        // turn this into `Ok("")`, and an inverted name comparison would
        // render P-01 instead of failing).
        assert!(render_named_plano(&dir, "nope").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spawn_watcher_bumps_version_only_on_relevant_changes() {
        let dir = std::env::temp_dir().join(format!("cadspec_serve_watch_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("project.toml"),
            "[project]\nname = \"Watch Test\"\n\n[layers]\nmuros = { file = \"muros.cf\" }\n",
        )
        .unwrap();
        let cf_path = dir.join("muros.cf");
        std::fs::write(
            &cf_path,
            "[layer]\nname = \"muros\"\n\n[[line]]\nid = \"ln-1\"\nfrom = [0.0, 0.0]\nto = [1.0, 0.0]\n",
        )
        .unwrap();

        // Shared state built exactly like `serve_project` does.
        let state: Shared = Arc::new(Live {
            state: Mutex::new(LiveState {
                svg: Arc::new(String::new()),
                svg3d: Arc::new(String::new()),
                error: None,
                version: 0,
                project_name: "Watch Test".to_string(),
                layers: Vec::new(),
                planos: Vec::new(),
            }),
            changed: Condvar::new(),
            project_dir: dir.clone(),
        });

        spawn_watcher(dir.clone(), Arc::clone(&state)).unwrap();
        // Give the watcher a moment to register before touching files.
        std::thread::sleep(Duration::from_millis(100));

        // Phase 1 — an irrelevant file (*.txt) must NOT trigger a rebuild.
        // If the relevance guard is inverted, this write bumps the version
        // and the assertion below fails after a fixed, bounded sleep.
        std::fs::write(dir.join("notes.txt"), "scratch").unwrap();
        std::thread::sleep(Duration::from_millis(700));
        let version_after_irrelevant = state.state.lock().unwrap().version;
        assert_eq!(
            version_after_irrelevant, 0,
            "writing a non-.cf/.toml file must not rebuild, version is {}",
            version_after_irrelevant
        );

        // Phase 2 — a real .cf change must bump the version, within a
        // bounded poll: if the watcher never fires, the loop ends at the
        // deadline and the assertion FAILS instead of hanging.
        std::fs::write(
            &cf_path,
            "[layer]\nname = \"muros\"\n\n[[line]]\nid = \"ln-1\"\nfrom = [0.0, 0.0]\nto = [2.0, 0.0]\n",
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_millis(4000);
        let mut version = version_after_irrelevant;
        while Instant::now() < deadline {
            version = state.state.lock().unwrap().version;
            if version > version_after_irrelevant {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            version > version_after_irrelevant,
            "watcher never rebuilt on a .cf change, version stayed {}",
            version
        );
    }
}

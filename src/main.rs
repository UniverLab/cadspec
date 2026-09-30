use anyhow::{bail, Result};
use cadspec::compiler::{check_project, compile_project, list_layers, project_report};
use cadspec::config::{config_set, config_show};
use cadspec::fmt::format_project;
use cadspec::importer::import_dxf;
use cadspec::preview::{
    generate_gltf, generate_plano, generate_preview, PreviewOutputs, PreviewView,
};
use cadspec::scaffold::{create_project, init_project};
use cadspec::schema::print_schema;
use cadspec::serve::{serve_daemon, serve_project, serve_stop};
use cadspec::viewer::view_project;
use cadspec::watch::watch_project;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "cadspec",
    version,
    about = "CAD as code — declarative geometry → DXF"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new CADspec project
    New {
        /// Project name (creates a directory with this name)
        name: String,
    },
    /// Initialize CADspec in the current directory
    Init,
    /// Compile project (.cf files) → DXF output
    Build {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Compile only a specific layer
        #[arg(short, long)]
        layer: Option<String>,
        /// Output file path (defaults to output.dxf in project dir)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Validate constraints and geometry without generating DXF
        #[arg(long)]
        check: bool,
    },
    /// Validate project without generating DXF
    Check {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Emit a machine-readable JSON report
        #[arg(long)]
        json: bool,
    },
    /// List project layers with status
    Layers {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Emit a machine-readable JSON report
        #[arg(long)]
        json: bool,
    },
    /// Generate preview (PNG + metadata JSON, or SVG) for AI agents
    Preview {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Image width in pixels
        #[arg(short, long, default_value = "1600")]
        width: u32,
        /// Image height in pixels (PNG only; SVG derives it from content)
        #[arg(short = 'H', long, default_value = "1200")]
        height: u32,
        /// Render only a specific layer
        #[arg(short, long)]
        layer: Option<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value_t = PreviewFormat::Png)]
        format: PreviewFormat,
        /// Highlight entities by id (comma-separated) with labeled markers
        #[arg(long, value_delimiter = ',')]
        highlight: Vec<String>,
        /// Render the extruded 3D view instead of the flat plan
        #[arg(long = "3d")]
        three_d: bool,
        /// Render a named plano (drawing sheet) defined in project.toml
        #[arg(long)]
        plano: Option<String>,
    },
    /// Live preview server — browser auto-reloads when .cf files change.
    /// Runs detached in the background by default; use --foreground to stay attached.
    Serve {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Port to listen on
        #[arg(long, default_value = "4377")]
        port: u16,
        /// Open the browser automatically
        #[arg(long)]
        open: bool,
        /// Stay in the foreground (stream logs, stop with Ctrl+C) instead of daemonizing
        #[arg(short = 'f', long)]
        foreground: bool,
        /// Stop the background server running for this project
        #[arg(long)]
        stop: bool,
    },
    /// Print the .cf language reference (markdown, for humans and AI agents)
    Schema,
    /// Format .cf files (sort keys, normalize whitespace)
    Fmt {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Check formatting without modifying files
        #[arg(long)]
        check: bool,
    },
    /// Watch project files and auto-rebuild on changes
    Watch {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },
    /// Import a DXF file into CADspec project files
    Import {
        /// Input DXF file
        input: PathBuf,
        /// Output directory for generated project (defaults to current dir)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Import only one DXF layer
        #[arg(short, long)]
        layer: Option<String>,
    },
    /// Open project output in external viewer
    View {
        /// Project directory (defaults to current dir)
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// View only one layer
        #[arg(short, long)]
        layer: Option<String>,
    },
    /// Global cadspec configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum PreviewFormat {
    /// Raster PNG + preview.meta.json
    Png,
    /// Vector SVG (real text, dimensions, hatches)
    Svg,
    /// Both PNG and SVG
    All,
    /// Self-contained glTF of the 3D solids (`scene.gltf`)
    Gltf,
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Set global default value
    Set {
        /// Config key (author | units)
        key: String,
        /// Value to store
        value: String,
    },
    /// Show global configuration
    Show,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { name } => create_project(&name, &PathBuf::from(".")),
        Commands::Init => init_project(&PathBuf::from(".")),
        Commands::Build {
            path,
            layer,
            output,
            check,
        } => run_build(path, layer, output, check),
        Commands::Check { path, json } => run_check(path, json),
        Commands::Layers { path, json } => run_layers(path, json),
        Commands::Preview {
            path,
            width,
            height,
            layer,
            format,
            highlight,
            three_d,
            plano,
        } => {
            let dir = resolve_project_dir(path)?;
            if matches!(format, PreviewFormat::Gltf) {
                generate_gltf(&dir, layer.as_deref())
            } else {
                let outputs = PreviewOutputs {
                    png: matches!(format, PreviewFormat::Png | PreviewFormat::All),
                    svg: matches!(format, PreviewFormat::Svg | PreviewFormat::All),
                };
                if let Some(name) = plano {
                    generate_plano(&dir, &name, width, height, outputs)
                } else {
                    let view = if three_d {
                        PreviewView::ThreeD
                    } else {
                        PreviewView::Plan
                    };
                    generate_preview(
                        &dir,
                        width,
                        height,
                        layer.as_deref(),
                        &highlight,
                        outputs,
                        view,
                    )
                }
            }
        }
        Commands::Serve {
            path,
            port,
            open,
            foreground,
            stop,
        } => run_serve(path, port, open, foreground, stop),
        Commands::Schema => {
            print_schema();
            Ok(())
        }
        Commands::Fmt { path, check } => {
            let dir = resolve_project_dir(path)?;
            format_project(&dir, check)
        }
        Commands::Watch { path } => {
            let dir = resolve_project_dir(path)?;
            watch_project(&dir)
        }
        Commands::Import {
            input,
            output,
            layer,
        } => {
            let out_dir = output.unwrap_or_else(|| PathBuf::from("."));
            import_dxf(&input, &out_dir, layer.as_deref())
        }
        Commands::View { path, layer } => {
            let dir = resolve_project_dir(path)?;
            view_project(&dir, layer.as_deref())
        }
        Commands::Config { command } => match command {
            ConfigCommands::Set { key, value } => config_set(&key, &value),
            ConfigCommands::Show => config_show(),
        },
    }
}

fn run_build(
    path: Option<PathBuf>,
    layer: Option<String>,
    output: Option<PathBuf>,
    check: bool,
) -> Result<()> {
    let dir = resolve_project_dir(path)?;
    if check {
        check_project(&dir)?;
        Ok(())
    } else {
        compile_project(&dir, layer.as_deref(), output.as_deref())
    }
}

fn run_check(path: Option<PathBuf>, json: bool) -> Result<()> {
    let dir = resolve_project_dir(path)?;
    if json {
        let report = project_report(&dir)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        if report.strict && !report.issues.is_empty() {
            bail!(
                "Check failed: {} constraint violation(s) with strict = true",
                report.issues.len()
            );
        }
        Ok(())
    } else {
        check_project(&dir)?;
        Ok(())
    }
}

fn run_layers(path: Option<PathBuf>, json: bool) -> Result<()> {
    let dir = resolve_project_dir(path)?;
    if json {
        let report = project_report(&dir)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        Ok(())
    } else {
        list_layers(&dir)
    }
}

fn run_serve(
    path: Option<PathBuf>,
    port: u16,
    open: bool,
    foreground: bool,
    stop: bool,
) -> Result<()> {
    let dir = resolve_project_dir(path)?;
    if stop {
        serve_stop(&dir, port)
    } else if foreground {
        serve_project(&dir, port, open)
    } else {
        serve_daemon(&dir, port, open)
    }
}

fn resolve_project_dir(path: Option<PathBuf>) -> Result<PathBuf> {
    let dir = path.unwrap_or_else(|| PathBuf::from("."));
    if !dir.join("project.toml").exists() {
        bail!(
            "No project.toml found in '{}'. Run `cadspec new` to create a project.",
            dir.display()
        );
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    static DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    /// Unique, empty temp directory: `/tmp/<prefix>_<pid>_<seq>`.
    fn unique_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{}_{}_{}",
            prefix,
            std::process::id(),
            DIR_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// A path that is guaranteed not to contain a project.toml.
    fn missing_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(prefix);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// Minimal valid project: one layer with a single line.
    fn write_simple_project(dir: &Path, strict: bool) {
        std::fs::write(
            dir.join("project.toml"),
            format!(
                r#"[project]
name = "t"
units = "m"
strict = {strict}

[layers]
main = {{ file = "main.cf", locked = false }}
"#
            ),
        )
        .expect("write project.toml");
        std::fs::write(
            dir.join("main.cf"),
            r#"[layer]
name = "main"

[[line]]
id = "l1"
from = [0.0, 0.0]
to = [1.0, 1.0]
"#,
        )
        .expect("write main.cf");
    }

    /// Project with exactly one constraint violation: the child layer's bbox
    /// (3,3 → 4,4) lies outside the parent layer's bbox (0,0 → 2,2).
    fn write_violating_project(dir: &Path, strict: bool) {
        std::fs::write(
            dir.join("project.toml"),
            format!(
                r#"[project]
name = "t"
units = "m"
strict = {strict}

[layers]
parent = {{ file = "parent.cf", locked = false }}
child = {{ file = "child.cf", locked = false }}

[constraints]
child.parent = "parent"
"#
            ),
        )
        .expect("write project.toml");
        std::fs::write(
            dir.join("parent.cf"),
            r#"[layer]
name = "parent"

[[rect]]
id = "room-1"
origin = [0.0, 0.0]
width = 2.0
height = 2.0
"#,
        )
        .expect("write parent.cf");
        std::fs::write(
            dir.join("child.cf"),
            r#"[layer]
name = "child"

[[rect]]
id = "furn-1"
origin = [3.0, 3.0]
width = 1.0
height = 1.0
"#,
        )
        .expect("write child.cf");
    }

    #[test]
    fn run_build_writes_dxf_output() {
        let dir = unique_dir("cadspec_main_build");
        write_simple_project(&dir, false);
        let out = dir.join("out.dxf");

        let result = run_build(Some(dir.clone()), None, Some(out.clone()), false);
        assert!(result.is_ok(), "build should succeed: {:?}", result.err());
        assert!(out.exists(), "output DXF file must be created");
        let content = std::fs::read_to_string(&out).expect("read output DXF");
        assert!(!content.is_empty(), "output DXF must not be empty");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_build_errors_on_missing_project_dir() {
        let missing = missing_dir("cadspec_main_missing_build");
        let result = run_build(Some(missing), None, None, false);
        assert!(result.is_err(), "missing project dir must fail the build");
    }

    #[test]
    fn run_check_strict_violation_fails_in_json_mode() {
        let dir = unique_dir("cadspec_main_check_strict");
        write_violating_project(&dir, true);

        let result = run_check(Some(dir.clone()), true);
        assert!(
            result.is_err(),
            "strict + violation must bail in json mode, got: {:?}",
            result
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_check_non_strict_violation_passes_in_json_mode() {
        let dir = unique_dir("cadspec_main_check_relaxed");
        write_violating_project(&dir, false);

        let result = run_check(Some(dir.clone()), true);
        assert!(
            result.is_ok(),
            "non-strict violation must not bail in json mode: {:?}",
            result.err()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_check_strict_without_violations_passes_in_json_mode() {
        let dir = unique_dir("cadspec_main_check_clean");
        write_simple_project(&dir, true);

        let result = run_check(Some(dir.clone()), true);
        assert!(
            result.is_ok(),
            "strict project with no violations must pass: {:?}",
            result.err()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_check_errors_on_missing_project_dir() {
        let missing = missing_dir("cadspec_main_missing_check");
        assert!(
            run_check(Some(missing.clone()), false).is_err(),
            "missing dir must fail check (non-json)"
        );
        assert!(
            run_check(Some(missing), true).is_err(),
            "missing dir must fail check (json)"
        );
    }

    #[test]
    fn run_layers_errors_on_missing_project_dir() {
        let missing = missing_dir("cadspec_main_missing_layers");
        let result = run_layers(Some(missing), false);
        assert!(result.is_err(), "missing dir must fail layers listing");
    }

    #[test]
    fn run_layers_lists_project_layers() {
        let dir = unique_dir("cadspec_main_layers");
        write_simple_project(&dir, false);

        assert!(
            run_layers(Some(dir.clone()), false).is_ok(),
            "listing layers of a valid project must succeed"
        );
        assert!(
            run_layers(Some(dir.clone()), true).is_ok(),
            "json layers report of a valid project must succeed"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_serve_errors_on_missing_project_dir_without_starting_server() {
        // stop = true and a missing dir: resolve_project_dir fails first, so
        // no server (foreground or daemon) can ever be spawned here.
        let missing = missing_dir("cadspec_main_missing_serve");
        let result = run_serve(Some(missing), 43999, false, false, true);
        assert!(result.is_err(), "missing dir must fail serve");
    }
}

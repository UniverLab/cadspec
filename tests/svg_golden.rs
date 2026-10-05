//! Golden SVG tests — byte-exact regression proof for the SVG renderer.
//!
//! Each case renders a fixture project to SVG through the same public path
//! `cadspec preview --svg` uses (`load_project_layers` + `render_scene_from`)
//! and compares the output string exactly with the committed file under
//! `tests/golden/`.
//!
//! The golden files were generated with the `origin/develop` binary (see the
//! spec), so this suite proves the quality refactor did not change rendering.
//! `CADSPEC_BLESS=1 cargo test --test svg_golden` rewrites the golden files;
//! without it a mismatch fails and prints the first differing region. Never
//! bless to fix a mismatch on this branch — a mismatch is a regression.

use cadspec::svg::{load_project_layers, render_scene_from};
use std::path::{Path, PathBuf};

const WIDTH: u32 = 1600;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn render(dir: &Path, highlight: &[String]) -> String {
    let (project, layers) = load_project_layers(dir, None).unwrap();
    render_scene_from(
        &project.project.name,
        &project.project.units,
        &layers,
        WIDTH,
        highlight,
    )
    .svg
}

fn check_case(name: &str, dir: &Path, highlight: &[String]) {
    let produced = render(dir, highlight);
    let golden_path = workspace_root()
        .join("tests/golden")
        .join(format!("{name}.svg"));
    let bless = std::env::var("CADSPEC_BLESS").as_deref() == Ok("1");
    if bless {
        if let Some(parent) = golden_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&golden_path, &produced).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&golden_path).unwrap_or_else(|_| {
        panic!(
            "missing golden file {} — run with CADSPEC_BLESS=1 to generate it",
            golden_path.display()
        )
    });
    if expected != produced {
        let (offset, e_ctx, p_ctx) = first_diff(&expected, &produced);
        panic!(
            "golden mismatch for '{name}' at byte {offset}:\n  expected: ...{e_ctx}...\n  actual:   ...{p_ctx}..."
        );
    }
}

fn first_diff(expected: &str, produced: &str) -> (usize, String, String) {
    let e = expected.as_bytes();
    let p = produced.as_bytes();
    let mut offset = 0;
    while offset < e.len() && offset < p.len() && e[offset] == p[offset] {
        offset += 1;
    }
    let lo = offset.saturating_sub(60);
    let e_hi = (offset + 60).min(e.len());
    let p_hi = (offset + 60).min(p.len());
    (
        offset,
        String::from_utf8_lossy(&e[lo..e_hi]).into_owned(),
        String::from_utf8_lossy(&p[lo..p_hi]).into_owned(),
    )
}

fn fixture(name: &str) -> PathBuf {
    workspace_root().join("tests/fixtures/svg").join(name)
}

fn example(name: &str) -> PathBuf {
    workspace_root().join("examples").join(name)
}

fn case(name: &str, dir: PathBuf) {
    check_case(name, &dir, &[]);
}

#[test]
fn golden_taller() {
    case("taller", example("taller"));
}

#[test]
fn golden_vivienda() {
    case("vivienda", example("vivienda"));
}

#[test]
fn golden_entities() {
    case("entities", fixture("entities"));
}

#[test]
fn golden_dims() {
    case("dims", fixture("dims"));
}

#[test]
fn golden_layers() {
    case("layers", fixture("layers"));
}

#[test]
fn golden_grid_origin() {
    case("grid-origin", fixture("grid-origin"));
}

#[test]
fn golden_grid_wide() {
    case("grid-wide", fixture("grid-wide"));
}

#[test]
fn golden_grid_large() {
    case("grid-large", fixture("grid-large"));
}

#[test]
fn golden_grid_straddle() {
    case("grid-straddle", fixture("grid-straddle"));
}

#[test]
fn golden_empty() {
    case("empty", fixture("empty"));
}

#[test]
fn golden_arc_bounds() {
    case("arc-bounds", fixture("arc-bounds"));
}

#[test]
fn golden_entities_highlight() {
    let ids = [
        "ln-dashed",
        "ln-dotted",
        "ln-dashdot",
        "rc-1",
        "rc-hidden",
        "ci-1",
        "ar-1",
        "pl-room",
        "pl-open",
        "pt-1",
        "tx-center",
        "tx-right",
        "tx-left",
        "tx-plain",
        "pl-hatch-boundary",
        "ht-ansi31",
        "ht-solid",
        "ht-ansi37",
        "ht-inline",
        "fl-boundary",
        "fl-inline",
        "ln-marca",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect::<Vec<_>>();
    check_case("entities.highlight", &fixture("entities"), &ids);
}

/// The file-level public path (`generate_preview` writing `preview.svg`)
/// produces the same bytes as the golden for the entities fixture.
#[test]
fn golden_public_path_identity() {
    use cadspec::preview::{generate_preview, PreviewOutputs, PreviewView};

    let tmp = std::env::temp_dir().join(format!("cs-svg-golden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    copy_dir(&fixture("entities"), &tmp);
    generate_preview(
        &tmp,
        WIDTH,
        1200,
        None,
        &[],
        PreviewOutputs {
            png: false,
            svg: true,
        },
        PreviewView::Plan,
    )
    .unwrap();
    let written = std::fs::read_to_string(tmp.join("preview.svg")).unwrap();
    let _ = std::fs::remove_dir_all(&tmp);
    let golden =
        std::fs::read_to_string(workspace_root().join("tests/golden/entities.svg")).unwrap();
    assert_eq!(written, golden, "generate_preview SVG differs from golden");
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), to).unwrap();
        }
    }
}

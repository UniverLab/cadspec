//! Scaffold — generates a new CADspec project structure.

use anyhow::{bail, Result};
use std::fs;
use std::path::Path;

/// Create a new CADspec project in the given directory.
pub fn create_project(name: &str, parent: &Path) -> Result<()> {
    let project_dir = parent.join(name);
    if project_dir.exists() {
        bail!("Directory '{}' already exists", project_dir.display());
    }

    fs::create_dir_all(&project_dir)?;
    write_project_files(&project_dir, name)?;

    println!("✓ Project '{}' created at {}", name, project_dir.display());
    println!("  → project.toml");
    println!("  → shapes.cf");
    println!("  → curves.cf");
    println!("  → annotations.cf");
    println!("  → .gitignore");
    println!(
        "\n  Run `cadspec serve --path {}` for a live preview,",
        name
    );
    println!("  or `cadspec build --path {}` to compile to DXF.", name);
    println!("  `cadspec schema` prints the .cf language reference.");
    Ok(())
}

/// Initialize a CADspec project in the current directory.
pub fn init_project(dir: &Path) -> Result<()> {
    if dir.join("project.toml").exists() {
        bail!("project.toml already exists in '{}'", dir.display());
    }

    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project");
    write_project_files(dir, name)?;

    println!("✓ Initialized CADspec project in {}", dir.display());
    println!("  → project.toml");
    println!("  → shapes.cf");
    println!("  → curves.cf");
    println!("  → annotations.cf");
    println!("  → .gitignore");
    println!("\n  Run `cadspec serve` for a live preview.");
    println!("  `cadspec schema` prints the .cf language reference.");
    Ok(())
}

fn write_project_files(project_dir: &Path, name: &str) -> Result<()> {
    fs::write(project_dir.join("project.toml"), project_toml_text(name))?;
    fs::write(project_dir.join(".gitignore"), GITIGNORE_TEXT)?;
    fs::write(project_dir.join("shapes.cf"), SHAPES_CF_TEXT)?;
    fs::write(project_dir.join("curves.cf"), CURVES_CF_TEXT)?;
    fs::write(project_dir.join("annotations.cf"), ANNOTATIONS_CF_TEXT)?;
    Ok(())
}

/// The `project.toml` of a fresh project, with its name filled in.
fn project_toml_text(name: &str) -> String {
    format!(
        r#"[project]
name = "{name}"
scale = "1:100"
units = "m"

[layers]
shapes = {{ file = "shapes.cf", locked = false }}
curves = {{ file = "curves.cf", locked = false }}
annotations = {{ file = "annotations.cf", locked = false }}
"#
    )
}

const GITIGNORE_TEXT: &str = "# CADspec output\noutput.dxf\npreview.png\npreview.svg\npreview.meta.json\n\n# CADspec serve daemon (pid + logs)\n.cadspec/\n\n# Rust build artifacts\ntarget/\n";

const SHAPES_CF_TEXT: &str = r##"[layer]
name = "shapes"
color = "#FFFFFF"
line_weight = 0.35

# Rectangle from its bottom-left origin.
# `extrude` gives it height in the 3D view (the `3D` button) — here, a box.
[[rect]]
id = "rc-001"
origin = [0.0, 0.0]
width = 4.0
height = 4.0
extrude = 3.0

# Closed polyline (a polygon) — also used as a hatch boundary
[[polyline]]
id = "pl-001"
points = [[6.0, 0.0], [10.0, 0.0], [11.0, 2.0], [8.0, 4.0], [6.0, 2.0]]
closed = true

# Reference marker (drawn as a cross)
[[point]]
id = "pt-001"
position = [2.0, 2.0]

# Pattern fill inside the polygon above
[[hatch]]
id = "ht-001"
boundary = "pl-001"
pattern = "ansi31"
angle = 45.0
"##;

const CURVES_CF_TEXT: &str = r##"[layer]
name = "curves"
color = "#4488FF"
line_weight = 0.25

# Full circle — extruded into a cylinder in the 3D view
[[circle]]
id = "ci-001"
center = [2.0, 8.0]
radius = 1.5
extrude = 2.0

# Half arc (angles in degrees, counterclockwise from +X)
[[arc]]
id = "ar-001"
center = [6.0, 8.0]
radius = 1.5
from_angle = 0.0
to_angle = 180.0

# A small circle repeated around a center (polar array).
# The array expands at build time into ci-sat, ci-sat@1, ci-sat@2, …
[[circle]]
id = "ci-sat"
center = [11.5, 8.0]
radius = 0.4

[[array]]
target = "ci-sat"
mode = "polar"
count = 6
center = [10.0, 8.0]
step_angle = 60.0
"##;

const ANNOTATIONS_CF_TEXT: &str = r##"[layer]
name = "annotations"
color = "#FF4444"
line_weight = 0.18

# Linear dimension — the measured distance is labeled automatically
[[dim]]
id = "dm-001"
type = "linear"
from = [0.0, 0.0]
to = [4.0, 0.0]
offset = -0.8

# Text labels (size is in world units, not points)
[[text]]
id = "tx-shapes"
position = [2.0, 4.4]
content = "shapes"
size = 0.4
align = "center"

[[text]]
id = "tx-curves"
position = [4.0, 10.2]
content = "curves"
size = 0.4
align = "center"
"##;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn creates_project_structure() {
        let tmp = PathBuf::from("/tmp/cadspec_test_new");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        create_project("mi-proyecto", &tmp).unwrap();

        let project_dir = tmp.join("mi-proyecto");
        assert!(project_dir.join("project.toml").exists());
        assert!(project_dir.join("shapes.cf").exists());
        assert!(project_dir.join("curves.cf").exists());
        assert!(project_dir.join("annotations.cf").exists());
        assert!(!project_dir.join("AGENTS.md").exists());
        assert!(project_dir.join(".gitignore").exists());

        let content = fs::read_to_string(project_dir.join("project.toml")).unwrap();
        assert!(content.contains("mi-proyecto"));
        assert!(content.contains("shapes.cf"));

        let gitignore = fs::read_to_string(project_dir.join(".gitignore")).unwrap();
        assert!(gitignore.contains("output.dxf"));
        assert!(gitignore.contains("preview.svg"));
        assert!(gitignore.contains("target/"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn fails_if_dir_exists() {
        let tmp = PathBuf::from("/tmp/cadspec_test_exists");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("existing")).unwrap();

        let result = create_project("existing", &tmp);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn init_in_existing_dir() {
        let tmp = PathBuf::from("/tmp/cadspec_test_init");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        init_project(&tmp).unwrap();

        assert!(tmp.join("project.toml").exists());
        assert!(tmp.join("shapes.cf").exists());
        assert!(tmp.join("curves.cf").exists());
        assert!(tmp.join("annotations.cf").exists());
        assert!(tmp.join(".gitignore").exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn init_fails_if_project_exists() {
        let tmp = PathBuf::from("/tmp/cadspec_test_init_exists");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("project.toml"), "").unwrap();

        let result = init_project(&tmp);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&tmp);
    }
}

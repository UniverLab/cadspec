//! SVG renderer — full-fidelity vector rendering of a project.
//!
//! Renders real text, dimension lines with measured values, hatch patterns
//! clipped to their boundary, line styles, and optional highlight markers.
//! It is the single rendering backend: `cadspec serve` displays the SVG
//! directly and the PNG preview rasterizes it.

use crate::compiler::resolve_boundary;
use crate::model::{CfFile, CommonAttrs, LineStyle};
use crate::parser::{parse_cf, parse_project};
use crate::transform::expand_cf;
use anyhow::{Context, Result};
use std::fmt::Write as _;
use std::path::Path;

const PADDING: f64 = 1.0; // world units around content
const MAX_HEIGHT_PX: f64 = 4096.0;
const BG_COLOR: &str = "#141414";
const GRID_COLOR: &str = "#232323";
const AXIS_COLOR: &str = "#333333";

const LAYER_PALETTE: &[&str] = &[
    "#FFFFFF", "#FF5050", "#50FF50", "#50C8FF", "#FFC850", "#C878FF", "#FF9632",
];

// ── Public API ──────────────────────────────────────────────────────────

/// A rendered SVG plus the world→pixel transform used to produce it, so
/// consumers (PNG rasterizer, metadata) can map world coordinates to pixels.
pub struct Scene {
    pub svg: String,
    /// Pixels per world unit.
    pub px_per_unit: f64,
    /// World X of the left canvas edge.
    pub offset_x: f64,
    /// World Y of the bottom canvas edge.
    pub offset_y: f64,
    /// Canvas height in world units (used for the Y flip).
    pub world_h: f64,
    pub width_px: f64,
    pub height_px: f64,
    /// Content bounds (without padding): [min_x, min_y, max_x, max_y].
    pub world_bounds: [f64; 4],
}

impl Scene {
    /// Map a world coordinate to image pixels.
    pub fn world_to_px(&self, x: f64, y: f64) -> (f64, f64) {
        (
            (x - self.offset_x) * self.px_per_unit,
            (self.world_h - (y - self.offset_y)) * self.px_per_unit,
        )
    }
}

/// Parse `project.toml` and its (filtered) layer files in one pass, with
/// `[[array]]`/`[[mirror]]` constructions expanded into concrete primitives.
pub fn load_project_layers(
    project_dir: &Path,
    layer_filter: Option<&str>,
) -> Result<(crate::parser::ProjectFile, Vec<(String, CfFile)>)> {
    let project = parse_project(&project_dir.join("project.toml"))?;

    let layers: Vec<(String, CfFile)> = project
        .layers
        .iter()
        .filter(|(name, _)| layer_filter.is_none_or(|f| f == *name))
        .map(|(name, entry)| {
            let cf = parse_cf(&project_dir.join(&entry.file))
                .with_context(|| format!("Failed to parse layer '{}'", name))?;
            Ok((name.clone(), expand_cf(&cf)))
        })
        .collect::<Result<_>>()?;

    Ok((project, layers))
}

/// Render already-parsed layers to a [`Scene`], optionally highlighting ids.
pub fn render_scene_from(
    project_name: &str,
    units: &str,
    layers: &[(String, CfFile)],
    width: u32,
    highlight: &[String],
) -> Scene {
    render_layers(project_name, units, layers, width, highlight)
}

/// Render the project to a [`Scene`], optionally highlighting entities by id.
pub fn render_scene(
    project_dir: &Path,
    layer_filter: Option<&str>,
    width: u32,
    highlight: &[String],
) -> Result<Scene> {
    let (project, layers) = load_project_layers(project_dir, layer_filter)?;
    Ok(render_layers(
        &project.project.name,
        &project.project.units,
        &layers,
        width,
        highlight,
    ))
}

/// Render the project to an SVG string.
pub fn render_svg(project_dir: &Path, layer_filter: Option<&str>, width: u32) -> Result<String> {
    Ok(render_scene(project_dir, layer_filter, width, &[])?.svg)
}

/// Display color of a layer: its declared color, or a palette color by index.
pub fn layer_display_color(cf: &CfFile, index: usize) -> String {
    cf.layer_meta
        .as_ref()
        .and_then(|m| m.color.clone())
        .unwrap_or_else(|| LAYER_PALETTE[index % LAYER_PALETTE.len()].to_string())
}

// ── Canvas ──────────────────────────────────────────────────────────────

struct Canvas {
    out: String,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    world_h: f64,
    width_px: f64,
    height_px: f64,
    clip_seq: usize,
}

impl Canvas {
    fn world_to_px(&self, x: f64, y: f64) -> (f64, f64) {
        let px = (x - self.offset_x) * self.scale;
        let py = (self.world_h - (y - self.offset_y)) * self.scale;
        (px, py)
    }

    fn points_attr(&self, points: &[(f64, f64)]) -> String {
        let mut s = String::with_capacity(points.len() * 16);
        for (i, &(x, y)) in points.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            let (px, py) = self.world_to_px(x, y);
            let _ = write!(s, "{:.2},{:.2}", px, py);
        }
        s
    }
}

#[derive(Clone)]
struct Style {
    color: String,
    width_px: f64,
    dash: Option<&'static str>,
}

fn resolve_style(common: &CommonAttrs, layer_color: &str, default_weight: f64) -> Style {
    let color = common
        .color
        .clone()
        .unwrap_or_else(|| layer_color.to_string());
    let weight = common.weight.unwrap_or(default_weight);
    let width_px = ((weight / 0.35) * 1.4).clamp(0.6, 6.0);
    let dash = match common.style {
        Some(LineStyle::Dashed) => Some("8,6"),
        Some(LineStyle::Dotted) => Some("1.5,5"),
        Some(LineStyle::Dashdot) => Some("10,4,1.5,4"),
        _ => None,
    };
    Style {
        color,
        width_px,
        dash,
    }
}

fn stroke_attrs(s: &Style) -> String {
    let mut a = format!(
        r#"stroke="{}" stroke-width="{:.2}" fill="none""#,
        s.color, s.width_px
    );
    if let Some(dash) = s.dash {
        let _ = write!(a, r#" stroke-dasharray="{}""#, dash);
    }
    a
}

fn id_attr(common: &CommonAttrs) -> String {
    match &common.id {
        Some(id) => format!(r#" data-id="{}""#, xml_escape(id)),
        None => String::new(),
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ── Bounds ──────────────────────────────────────────────────────────────

struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min_x: f64::MAX,
            min_y: f64::MAX,
            max_x: f64::MIN,
            max_y: f64::MIN,
        }
    }

    fn add(&mut self, x: f64, y: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    fn is_empty(&self) -> bool {
        self.min_x > self.max_x
    }
}

fn compute_bounds(layers: &[(String, CfFile)]) -> Bounds {
    let mut b = Bounds::empty();
    for (_, cf) in layers {
        for e in &cf.lines {
            b.add(e.from[0], e.from[1]);
            b.add(e.to[0], e.to[1]);
        }
        for e in &cf.polylines {
            for p in &e.points {
                b.add(p[0], p[1]);
            }
        }
        for e in &cf.rects {
            b.add(e.origin[0], e.origin[1]);
            b.add(e.origin[0] + e.width, e.origin[1] + e.height);
        }
        for e in &cf.circles {
            b.add(e.center[0] - e.radius, e.center[1] - e.radius);
            b.add(e.center[0] + e.radius, e.center[1] + e.radius);
        }
        for e in &cf.arcs {
            b.add(e.center[0] - e.radius, e.center[1] - e.radius);
            b.add(e.center[0] + e.radius, e.center[1] + e.radius);
        }
        for e in &cf.texts {
            b.add(e.position[0], e.position[1]);
        }
        for e in &cf.points {
            b.add(e.position[0], e.position[1]);
        }
        for e in &cf.dims {
            b.add(e.from[0], e.from[1]);
            b.add(e.to[0], e.to[1]);
        }
        for e in &cf.fills {
            if let Some(points) = &e.points {
                for p in points {
                    b.add(p[0], p[1]);
                }
            }
        }
    }
    if b.is_empty() {
        Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 10.0,
            max_y: 10.0,
        }
    } else {
        b
    }
}

// ── Rendering ───────────────────────────────────────────────────────────

fn render_layers(
    project_name: &str,
    units: &str,
    layers: &[(String, CfFile)],
    width: u32,
    highlight: &[String],
) -> Scene {
    let bounds = compute_bounds(layers);
    let world_w = bounds.max_x - bounds.min_x + 2.0 * PADDING;
    let world_h = bounds.max_y - bounds.min_y + 2.0 * PADDING;

    let width_px = width as f64;
    let height_px = (width_px * world_h / world_w).min(MAX_HEIGHT_PX);
    let scale = (width_px / world_w).min(height_px / world_h);

    let mut canvas = Canvas {
        out: String::with_capacity(16 * 1024),
        scale,
        offset_x: bounds.min_x - PADDING,
        offset_y: bounds.min_y - PADDING,
        world_h,
        width_px,
        height_px,
        clip_seq: 0,
    };

    let _ = write!(
        canvas.out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}" data-project="{name}">"#,
        w = canvas.width_px,
        h = canvas.height_px,
        name = xml_escape(project_name),
    );
    let _ = write!(
        canvas.out,
        r#"<rect width="100%" height="100%" fill="{}"/>"#,
        BG_COLOR
    );

    draw_grid(&mut canvas, &bounds);

    for (idx, (layer_name, cf)) in layers.iter().enumerate() {
        let layer_color = layer_display_color(cf, idx);
        let default_weight = cf
            .layer_meta
            .as_ref()
            .and_then(|m| m.line_weight)
            .unwrap_or(0.35);
        let visible = cf.layer_meta.as_ref().map(|m| m.visible).unwrap_or(true);
        if !visible {
            continue;
        }
        let _ = write!(canvas.out, r#"<g data-layer="{}">"#, xml_escape(layer_name));
        render_layer(
            &mut canvas,
            cf,
            layer_name,
            &layer_color,
            default_weight,
            units,
        );
        canvas.out.push_str("</g>");
    }

    if !highlight.is_empty() {
        draw_highlights(&mut canvas, layers, highlight);
    }

    canvas.out.push_str("</svg>");
    Scene {
        px_per_unit: canvas.scale,
        offset_x: canvas.offset_x,
        offset_y: canvas.offset_y,
        world_h: canvas.world_h,
        width_px: canvas.width_px,
        height_px: canvas.height_px,
        world_bounds: [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y],
        svg: canvas.out,
    }
}
/// Resolve a region's points from an optional boundary id (same-layer lookup)
/// or its explicit point list — shared by enumeration and drawing.
fn region_points(
    boundary_id: Option<&str>,
    points: &Option<Vec<[f64; 2]>>,
    cf: &CfFile,
) -> Option<Vec<(f64, f64)>> {
    match boundary_id {
        Some(id) => resolve_boundary(id, cf),
        None => points
            .as_ref()
            .map(|p| p.iter().map(|v| (v[0], v[1])).collect()),
    }
}

mod entities;
mod layer;

pub use self::entities::{enumerate_entities, EntityRecord};
pub use self::layer::format_dim_label;
use self::layer::{draw_grid, draw_highlights, render_layer};

#[cfg(test)]
mod fixtures {
    use crate::model::CfFile;

    pub(super) fn sample_layers() -> Vec<(String, CfFile)> {
        let toml = r##"
[layer]
name = "test"
color = "#FFFFFF"

[[line]]
id = "ln-1"
from = [0.0, 0.0]
to = [10.0, 0.0]
style = "dashed"

[[rect]]
id = "rc-1"
origin = [1.0, 1.0]
width = 3.0
height = 2.0

[[circle]]
center = [5.0, 5.0]
radius = 1.5

[[arc]]
center = [2.0, 2.0]
radius = 1.0
from_angle = 0.0
to_angle = 90.0

[[text]]
position = [5.0, 5.0]
content = "SALA <principal>"
size = 0.3
align = "center"

[[dim]]
from = [0.0, 0.0]
to = [10.0, 0.0]
offset = -0.8

[[polyline]]
id = "pl-room"
points = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]
closed = true

[[hatch]]
boundary = "pl-room"
pattern = "ansi31"
"##;
        let cf: CfFile = toml::from_str(toml).unwrap();
        vec![("test".to_string(), cf)]
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::sample_layers;
    use super::*;

    #[test]
    fn renders_all_primitives() {
        let svg = render_layers("demo", "m", &sample_layers(), 1200, &[]).svg;
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains("<line"));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("<circle"));
        assert!(svg.contains("<polyline")); // arc
        assert!(svg.contains("<polygon")); // closed polyline
        assert!(svg.contains("clipPath")); // hatch
        assert!(svg.contains("stroke-dasharray")); // dashed line
        assert!(svg.contains(r#"data-id="ln-1""#));
    }

    #[test]
    fn escapes_text_content() {
        let svg = render_layers("demo", "m", &sample_layers(), 1200, &[]).svg;
        assert!(svg.contains("SALA &lt;principal&gt;"));
        assert!(!svg.contains("SALA <principal>"));
    }

    #[test]
    fn dim_label_shows_measured_length() {
        let svg = render_layers("demo", "m", &sample_layers(), 1200, &[]).svg;
        assert!(svg.contains("10.00 m"));
    }

    #[test]
    fn empty_project_renders_default_viewport() {
        let svg = render_layers("empty", "m", &[], 800, &[]).svg;
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains(r#"width="800""#));
    }

    #[test]
    fn highlight_draws_marker_for_matching_id() {
        let scene = render_layers(
            "demo",
            "m",
            &sample_layers(),
            1200,
            &["rc-1".to_string(), "missing-id".to_string()],
        );
        assert!(scene.svg.contains(r#"data-highlight="rc-1""#));
        assert!(scene.svg.contains(">rc-1</text>"));
        assert!(!scene.svg.contains(r#"data-highlight="missing-id""#));
    }
    #[test]
    fn scene_world_to_px_is_consistent_with_canvas() {
        let scene = render_layers("demo", "m", &sample_layers(), 1200, &[]);
        // Bottom-left content corner with padding maps inside the canvas
        let (px, py) = scene.world_to_px(scene.world_bounds[0], scene.world_bounds[1]);
        assert!(px > 0.0 && px < scene.width_px);
        assert!(py > 0.0 && py <= scene.height_px);
    }
}

//! DXF emission — compiles a parsed `.cf` layer into `DxfWriter` entities.

use super::{resolve_boundary, resolve_layer, resolve_style, warn_unresolved_boundary};
use crate::color::hex_to_aci;
use crate::dxf_writer::DxfWriter;
use crate::model::CfFile;

/// Compile a single .cf file into the DxfWriter (public for integration tests).
pub fn compile_cf_public(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    compile_cf(writer, cf, default_layer);
}

/// Compile a single .cf file into the DxfWriter.
pub(super) fn compile_cf(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    if let Some(meta) = &cf.layer_meta {
        if let Some(color) = &meta.color {
            writer.add_layer(default_layer, hex_to_aci(color));
        }
    }

    compile_lines(writer, cf, default_layer);
    compile_polylines(writer, cf, default_layer);
    compile_rects(writer, cf, default_layer);
    compile_circles(writer, cf, default_layer);
    compile_arcs(writer, cf, default_layer);
    compile_texts(writer, cf, default_layer);
    compile_points(writer, cf, default_layer);
    compile_dims(writer, cf, default_layer);
    compile_hatches(writer, cf, default_layer);
    compile_fills(writer, cf, default_layer);
}

fn compile_lines(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    for e in &cf.lines {
        let style = resolve_style(&e.common);
        writer.line(
            e.from[0],
            e.from[1],
            e.to[0],
            e.to[1],
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }
}

fn compile_polylines(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    for e in &cf.polylines {
        let style = resolve_style(&e.common);
        let pts: Vec<(f64, f64)> = e.points.iter().map(|p| (p[0], p[1])).collect();
        writer.polyline(
            &pts,
            e.closed,
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }
}

fn compile_rects(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    for e in &cf.rects {
        let style = resolve_style(&e.common);
        writer.rect(
            e.origin[0],
            e.origin[1],
            e.width,
            e.height,
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }
}

fn compile_circles(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    for e in &cf.circles {
        let style = resolve_style(&e.common);
        writer.circle(
            e.center[0],
            e.center[1],
            e.radius,
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }
}

fn compile_arcs(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    for e in &cf.arcs {
        let style = resolve_style(&e.common);
        writer.arc(
            e.center[0],
            e.center[1],
            e.radius,
            e.from_angle,
            e.to_angle,
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }
}

fn compile_texts(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    for e in &cf.texts {
        let style = resolve_style(&e.common);
        writer.text(
            e.position[0],
            e.position[1],
            e.size,
            &e.content,
            e.rotation.unwrap_or(0.0),
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }
}

fn compile_points(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    for e in &cf.points {
        let style = resolve_style(&e.common);
        writer.point(
            e.position[0],
            e.position[1],
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }
}

fn compile_dims(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    for e in &cf.dims {
        let style = resolve_style(&e.common);
        let dist = ((e.to[0] - e.from[0]).powi(2) + (e.to[1] - e.from[1]).powi(2)).sqrt();
        let label =
            crate::svg::format_dim_label(dist, e.precision.unwrap_or(2) as usize, e.show_units, "")
                .trim_end()
                .to_string();
        writer.dim_linear(
            e.from[0],
            e.from[1],
            e.to[0],
            e.to[1],
            e.offset,
            &label,
            e.text_size.unwrap_or(0.25),
            resolve_layer(&e.common, default_layer),
            &style,
        );
    }
}

// Hatches: resolve boundary by id, generate pattern lines
fn compile_hatches(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    for e in &cf.hatches {
        let layer = resolve_layer(&e.common, default_layer);
        let style = resolve_style(&e.common);
        let spacing = 0.1 * e.scale; // base spacing scaled

        let boundary = resolve_region_points(
            "hatch",
            &e.boundary,
            &e.points,
            cf,
            e.common.id.as_deref(),
            default_layer,
        );
        if let Some(boundary) = boundary {
            writer.hatch(
                &boundary, e.angle, spacing, e.scale, &e.pattern, layer, &style,
            );
        }
    }
}

// Solid fills
fn compile_fills(writer: &mut DxfWriter, cf: &CfFile, default_layer: &str) {
    for e in &cf.fills {
        let layer = resolve_layer(&e.common, default_layer);
        let style = resolve_style(&e.common);

        let pts = resolve_region_points(
            "fill",
            &e.boundary,
            &e.points,
            cf,
            e.common.id.as_deref(),
            default_layer,
        );
        if let Some(pts) = pts {
            writer.solid_fill(&pts, layer, &style);
        }
    }
}

/// Resolve a hatch/fill region's points: by boundary id (warning, without
/// failing the build, when the id does not resolve in this layer) or from the
/// explicit point list. The `layer` reported in warnings is the file's default
/// layer, as `build` has per-file context only.
fn resolve_region_points(
    kind: &str,
    boundary: &Option<String>,
    points: &Option<Vec<[f64; 2]>>,
    cf: &CfFile,
    entity_id: Option<&str>,
    layer: &str,
) -> Option<Vec<(f64, f64)>> {
    let Some(boundary_id) = boundary else {
        return points
            .as_ref()
            .map(|p| p.iter().map(|v| (v[0], v[1])).collect());
    };
    let resolved = resolve_boundary(boundary_id, cf);
    if resolved.is_none() {
        warn_unresolved_boundary(kind, entity_id, boundary_id, layer);
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(toml: &str) -> CfFile {
        toml::from_str(toml).unwrap()
    }

    #[test]
    fn region_points_come_from_the_explicit_list_without_boundary() {
        let cf = layer(
            r#"[layer]
name = "l"

[[fill]]
points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]
"#,
        );
        let pts = resolve_region_points("fill", &None, &cf.fills[0].points, &cf, None, "l");
        assert_eq!(pts.unwrap(), vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]);
    }

    #[test]
    fn region_points_resolve_a_closed_polyline_boundary() {
        let cf = layer(
            r#"[layer]
name = "l"

[[polyline]]
id = "zone"
points = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]
closed = true

[[fill]]
boundary = "zone"
"#,
        );
        let pts = resolve_region_points(
            "fill",
            &cf.fills[0].boundary,
            &cf.fills[0].points,
            &cf,
            None,
            "l",
        );
        assert_eq!(pts.unwrap().len(), 4);
    }

    #[test]
    fn region_points_are_none_when_the_boundary_does_not_resolve() {
        let cf = layer(
            r#"[layer]
name = "l"

[[hatch]]
boundary = "missing"
"#,
        );
        let pts = resolve_region_points(
            "hatch",
            &cf.hatches[0].boundary,
            &cf.hatches[0].points,
            &cf,
            None,
            "l",
        );
        assert!(pts.is_none());
    }

    // ── Emission: what actually lands in the DXF ──────────────────────────

    use dxf::entities::EntityType;

    /// Compile a `.cf` snippet, write the DXF to a temp file and read it back.
    fn compile_drawing(tag: &str, toml: &str) -> dxf::Drawing {
        let cf = layer(toml);
        let mut writer = DxfWriter::new();
        writer.add_layer("l", 7);
        compile_cf(&mut writer, &cf, "l");
        let path =
            std::env::temp_dir().join(format!("cadspec_emit_{}_{}.dxf", tag, std::process::id()));
        writer.save(&path).unwrap();
        let drawing = dxf::Drawing::load_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        drawing
    }

    fn count_entities(drawing: &dxf::Drawing, matches: impl Fn(&EntityType) -> bool) -> usize {
        drawing.entities().filter(|e| matches(&e.specific)).count()
    }

    #[test]
    fn compile_emits_a_point_entity_per_point() {
        let drawing = compile_drawing(
            "points",
            r#"[layer]
name = "l"

[[point]]
position = [1.0, 2.0]

[[point]]
position = [-3.0, 4.5]
"#,
        );
        assert_eq!(
            count_entities(&drawing, |t| matches!(t, EntityType::ModelPoint(_))),
            2
        );
    }

    #[test]
    fn compile_emits_a_polyline_entity_per_polyline() {
        let drawing = compile_drawing(
            "polylines",
            r#"[layer]
name = "l"

[[polyline]]
id = "zone"
points = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]
closed = true
"#,
        );
        assert_eq!(
            count_entities(&drawing, |t| matches!(t, EntityType::LwPolyline(_))),
            1
        );
    }

    #[test]
    fn compile_labels_dims_with_the_measured_distance() {
        // from (2,2) → to (5,6): dx=3, dy=4 → distance 5 → label "5.00".
        let drawing = compile_drawing(
            "dims",
            r#"[layer]
name = "l"

[[dim]]
from = [2.0, 2.0]
to = [5.0, 6.0]
offset = 0.5
"#,
        );
        let texts: Vec<&str> = drawing
            .entities()
            .filter_map(|e| match &e.specific {
                EntityType::Text(t) => Some(t.value.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts.contains(&"5.00"),
            "expected the dimension label 5.00, got {texts:?}"
        );
    }

    #[test]
    fn compile_scales_the_hatch_line_spacing() {
        // 2×2 square, angle 0, scale 2 → spacing 0.2 → a fixed, known number
        // of pattern lines inside the boundary (spacing drives the line count).
        let drawing = compile_drawing(
            "hatch",
            r#"[layer]
name = "l"

[[hatch]]
points = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]
angle = 0.0
scale = 2.0
pattern = "ansi31"
"#,
        );
        let lines = count_entities(&drawing, |t| matches!(t, EntityType::Line(_)));
        assert_eq!(
            lines, 11,
            "spacing = 0.1 * scale = 0.2 must yield 11 clipped pattern lines"
        );
    }
}

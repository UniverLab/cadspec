//! Layer drawing — one `draw_*` pass per entity kind, plus grid, highlights,
//! dimensions and hatch patterns.

use super::entities::enumerate_entities;
use super::{
    id_attr, region_points, resolve_style, stroke_attrs, xml_escape, Bounds, Canvas, Style,
    AXIS_COLOR, GRID_COLOR, PADDING,
};
use crate::compiler::resolve_boundary;
use crate::model::{CfFile, CommonAttrs, TextAlign};
use std::fmt::Write as _;

// ── Highlights (visual verification markers for agents) ────────────────

const HIGHLIGHT_COLOR: &str = "#FFB300";

pub(super) fn draw_highlights(c: &mut Canvas, layers: &[(String, CfFile)], highlight: &[String]) {
    c.out.push_str(r#"<g data-highlights="true">"#);
    for (_, cf) in layers {
        for rec in enumerate_entities(cf) {
            let Some(id) = &rec.id else { continue };
            if !highlight.iter().any(|h| h == id) {
                continue;
            }
            let (x1, y1) = c.world_to_px(rec.bbox[0], rec.bbox[3]); // top-left
            let (x2, y2) = c.world_to_px(rec.bbox[2], rec.bbox[1]); // bottom-right
            let margin = 8.0;
            let _ = write!(
                c.out,
                r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" stroke="{}" stroke-width="1.8" stroke-dasharray="6,4" fill="none" data-highlight="{}"/>"#,
                x1 - margin,
                y1 - margin,
                (x2 - x1) + 2.0 * margin,
                (y2 - y1) + 2.0 * margin,
                HIGHLIGHT_COLOR,
                xml_escape(id),
            );
            let _ = write!(
                c.out,
                r#"<text x="{:.2}" y="{:.2}" font-size="14" font-family="monospace" fill="{}">{}</text>"#,
                x1 - margin,
                y1 - margin - 6.0,
                HIGHLIGHT_COLOR,
                xml_escape(id),
            );
        }
    }
    c.out.push_str("</g>");
}

fn grid_step(world_w: f64) -> f64 {
    const STEPS: &[f64] = &[0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 500.0];
    for &s in STEPS {
        if world_w / s <= 40.0 {
            return s;
        }
    }
    1000.0
}

pub(super) fn draw_grid(c: &mut Canvas, bounds: &Bounds) {
    let step = grid_step(bounds.max_x - bounds.min_x + 2.0 * PADDING);
    let x0 = ((bounds.min_x - PADDING) / step).floor() * step;
    let x1 = bounds.max_x + PADDING;
    let y0 = ((bounds.min_y - PADDING) / step).floor() * step;
    let y1 = bounds.max_y + PADDING;

    c.out.push_str(r#"<g data-grid="true">"#);
    let mut x = x0;
    while x <= x1 {
        let (px, _) = c.world_to_px(x, 0.0);
        let color = if x.abs() < 1e-9 {
            AXIS_COLOR
        } else {
            GRID_COLOR
        };
        let _ = write!(
            c.out,
            r#"<line x1="{px:.2}" y1="0" x2="{px:.2}" y2="{h:.2}" stroke="{color}" stroke-width="1"/>"#,
            h = c.height_px,
        );
        x += step;
    }
    let mut y = y0;
    while y <= y1 {
        let (_, py) = c.world_to_px(0.0, y);
        let color = if y.abs() < 1e-9 {
            AXIS_COLOR
        } else {
            GRID_COLOR
        };
        let _ = write!(
            c.out,
            r#"<line x1="0" y1="{py:.2}" x2="{w:.2}" y2="{py:.2}" stroke="{color}" stroke-width="1"/>"#,
            w = c.width_px,
        );
        y += step;
    }
    c.out.push_str("</g>");
}

pub(super) fn render_layer(
    c: &mut Canvas,
    cf: &CfFile,
    layer_name: &str,
    layer_color: &str,
    default_weight: f64,
    units: &str,
) {
    draw_lines(c, cf, layer_color, default_weight);
    draw_polylines(c, cf, layer_color, default_weight);
    draw_rects(c, cf, layer_color, default_weight);
    draw_circles(c, cf, layer_color, default_weight);
    draw_arcs(c, cf, layer_color, default_weight);
    // Fills and hatches go before text so labels stay readable on top.
    draw_fills(c, cf, layer_color, default_weight, layer_name);
    draw_hatches(c, cf, layer_color, default_weight, layer_name);
    draw_dims(c, cf, layer_color, default_weight, units);
    draw_points(c, cf, layer_color, default_weight);
    draw_texts(c, cf, layer_color, default_weight);
}

/// Resolve a region's points, warning when a boundary id does not resolve
/// in this layer (the region is then skipped, as in `build`).
fn region_points_with_warn(
    kind: &str,
    common: &CommonAttrs,
    boundary: &Option<String>,
    points: &Option<Vec<[f64; 2]>>,
    cf: &CfFile,
    layer_name: &str,
) -> Option<Vec<(f64, f64)>> {
    let Some(boundary_id) = boundary else {
        return region_points(None, points, cf);
    };
    let resolved = resolve_boundary(boundary_id, cf);
    if resolved.is_none() {
        crate::compiler::warn_unresolved_boundary(
            kind,
            common.id.as_deref(),
            boundary_id,
            layer_name,
        );
    }
    resolved
}

fn draw_lines(c: &mut Canvas, cf: &CfFile, layer_color: &str, default_weight: f64) {
    for e in cf.lines.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let (x1, y1) = c.world_to_px(e.from[0], e.from[1]);
        let (x2, y2) = c.world_to_px(e.to[0], e.to[1]);
        let _ = write!(
            c.out,
            r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" {}{}/>"#,
            x1,
            y1,
            x2,
            y2,
            stroke_attrs(&s),
            id_attr(&e.common)
        );
    }
}

fn draw_polylines(c: &mut Canvas, cf: &CfFile, layer_color: &str, default_weight: f64) {
    for e in cf.polylines.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let pts: Vec<(f64, f64)> = e.points.iter().map(|p| (p[0], p[1])).collect();
        let tag = if e.closed { "polygon" } else { "polyline" };
        let _ = write!(
            c.out,
            r#"<{} points="{}" {}{}/>"#,
            tag,
            c.points_attr(&pts),
            stroke_attrs(&s),
            id_attr(&e.common)
        );
    }
}

fn draw_rects(c: &mut Canvas, cf: &CfFile, layer_color: &str, default_weight: f64) {
    for e in cf.rects.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let (px, py) = c.world_to_px(e.origin[0], e.origin[1] + e.height);
        let _ = write!(
            c.out,
            r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" {}{}/>"#,
            px,
            py,
            e.width * c.scale,
            e.height * c.scale,
            stroke_attrs(&s),
            id_attr(&e.common)
        );
    }
}

fn draw_circles(c: &mut Canvas, cf: &CfFile, layer_color: &str, default_weight: f64) {
    for e in cf.circles.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let (px, py) = c.world_to_px(e.center[0], e.center[1]);
        let _ = write!(
            c.out,
            r#"<circle cx="{:.2}" cy="{:.2}" r="{:.2}" {}{}/>"#,
            px,
            py,
            e.radius * c.scale,
            stroke_attrs(&s),
            id_attr(&e.common)
        );
    }
}

fn draw_arcs(c: &mut Canvas, cf: &CfFile, layer_color: &str, default_weight: f64) {
    for e in cf.arcs.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let pts = arc_points(e.center[0], e.center[1], e.radius, e.from_angle, e.to_angle);
        let _ = write!(
            c.out,
            r#"<polyline points="{}" {}{}/>"#,
            c.points_attr(&pts),
            stroke_attrs(&s),
            id_attr(&e.common)
        );
    }
}

fn draw_fills(
    c: &mut Canvas,
    cf: &CfFile,
    layer_color: &str,
    default_weight: f64,
    layer_name: &str,
) {
    for e in cf.fills.iter().filter(|e| e.common.visible) {
        let pts =
            region_points_with_warn("fill", &e.common, &e.boundary, &e.points, cf, layer_name);
        if let Some(pts) = pts {
            let s = resolve_style(&e.common, layer_color, default_weight);
            let _ = write!(
                c.out,
                r#"<polygon points="{}" fill="{}" fill-opacity="0.35" stroke="none"{}/>"#,
                c.points_attr(&pts),
                s.color,
                id_attr(&e.common)
            );
        }
    }
}

fn draw_hatches(
    c: &mut Canvas,
    cf: &CfFile,
    layer_color: &str,
    default_weight: f64,
    layer_name: &str,
) {
    for e in cf.hatches.iter().filter(|e| e.common.visible) {
        let boundary =
            region_points_with_warn("hatch", &e.common, &e.boundary, &e.points, cf, layer_name);
        if let Some(boundary) = boundary {
            let s = resolve_style(&e.common, layer_color, default_weight);
            draw_hatch(
                c,
                &boundary,
                e.pattern.as_str(),
                e.angle,
                0.1 * e.scale,
                &s,
                &e.common,
            );
        }
    }
}

fn draw_dims(c: &mut Canvas, cf: &CfFile, layer_color: &str, default_weight: f64, units: &str) {
    for e in cf.dims.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        draw_dim(c, e, &s, units);
    }
}

fn draw_points(c: &mut Canvas, cf: &CfFile, layer_color: &str, default_weight: f64) {
    for e in cf.points.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let (px, py) = c.world_to_px(e.position[0], e.position[1]);
        let _ = write!(
            c.out,
            r#"<path d="M {x0:.2} {y:.2} H {x1:.2} M {x:.2} {y0:.2} V {y1:.2}" {attrs}{id}/>"#,
            x0 = px - 4.0,
            x1 = px + 4.0,
            y0 = py - 4.0,
            y1 = py + 4.0,
            x = px,
            y = py,
            attrs = stroke_attrs(&s),
            id = id_attr(&e.common)
        );
    }
}

fn draw_texts(c: &mut Canvas, cf: &CfFile, layer_color: &str, default_weight: f64) {
    for e in cf.texts.iter().filter(|e| e.common.visible) {
        let s = resolve_style(&e.common, layer_color, default_weight);
        let (px, py) = c.world_to_px(e.position[0], e.position[1]);
        let anchor = match e.align {
            Some(TextAlign::Center) => "middle",
            Some(TextAlign::Right) => "end",
            _ => "start",
        };
        let font_px = (e.size * c.scale).max(1.0);
        let font_family = xml_escape(e.font.as_deref().unwrap_or("monospace"));
        let mut extra_attrs = String::new();
        if e.bold == Some(true) {
            extra_attrs.push_str(r#" font-weight="bold""#);
        }
        if e.italic == Some(true) {
            extra_attrs.push_str(r#" font-style="italic""#);
        }
        if let Some(angle) = e.rotation {
            if angle != 0.0 {
                let _ = write!(
                    extra_attrs,
                    r#" transform="rotate({:.2}, {:.2}, {:.2})""#,
                    -angle, px, py
                );
            }
        }
        let _ = write!(
            c.out,
            r#"<text x="{:.2}" y="{:.2}" font-size="{:.2}" font-family="{}" text-anchor="{}" fill="{}"{}{}>{}</text>"#,
            px,
            py,
            font_px,
            font_family,
            anchor,
            s.color,
            extra_attrs,
            id_attr(&e.common),
            xml_escape(&e.content)
        );
    }
}

pub(super) fn arc_points(
    cx: f64,
    cy: f64,
    radius: f64,
    from_deg: f64,
    to_deg: f64,
) -> Vec<(f64, f64)> {
    const STEPS: usize = 48;
    let start = from_deg.to_radians();
    let delta = (to_deg.to_radians() - start) / STEPS as f64;
    (0..=STEPS)
        .map(|i| {
            let a = start + delta * i as f64;
            (cx + radius * a.cos(), cy + radius * a.sin())
        })
        .collect()
}

fn draw_dim(c: &mut Canvas, dim: &crate::model::CfDim, s: &Style, units: &str) {
    let (from, to, offset) = (dim.from, dim.to, dim.offset);
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let (nx, ny) = (-uy, ux);

    // Dimension line endpoints, offset along the normal
    let a = (from[0] + nx * offset, from[1] + ny * offset);
    let b = (to[0] + nx * offset, to[1] + ny * offset);

    let (ax, ay) = c.world_to_px(a.0, a.1);
    let (bx, by) = c.world_to_px(b.0, b.1);
    let (fx, fy) = c.world_to_px(from[0], from[1]);
    let (tx, ty) = c.world_to_px(to[0], to[1]);

    let _ = write!(c.out, r#"<g data-dim="true"{}>"#, id_attr(&dim.common));
    // Extension lines + dimension line
    let _ = write!(
        c.out,
        r#"<path d="M {fx:.2} {fy:.2} L {ax:.2} {ay:.2} M {tx:.2} {ty:.2} L {bx:.2} {by:.2} M {ax:.2} {ay:.2} L {bx:.2} {by:.2}" stroke="{color}" stroke-width="{w:.2}" fill="none"/>"#,
        color = s.color,
        w = (s.width_px * 0.8).max(0.6),
    );
    // Tick marks (45° slashes) at both ends
    let tick = 5.0;
    for &(px, py) in &[(ax, ay), (bx, by)] {
        let _ = write!(
            c.out,
            r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{}" stroke-width="{:.2}"/>"#,
            px - tick,
            py + tick,
            px + tick,
            py - tick,
            s.color,
            (s.width_px * 0.8).max(0.6),
        );
    }
    // Measured value label at the midpoint
    let text_size = dim.text_size.unwrap_or(0.0);
    let label_gap = if text_size > 0.0 {
        text_size * 0.6
    } else {
        0.15
    };
    let mid = (
        (a.0 + b.0) / 2.0 + nx * label_gap,
        (a.1 + b.1) / 2.0 + ny * label_gap,
    );
    let (mx, my) = c.world_to_px(mid.0, mid.1);
    let font_px = if text_size > 0.0 {
        (text_size * c.scale).max(1.0)
    } else {
        (0.22 * c.scale).clamp(9.0, 28.0)
    };
    let label = format_dim_label(
        len,
        dim.precision.unwrap_or(2) as usize,
        dim.show_units,
        units,
    );
    let _ = write!(
        c.out,
        r#"<text x="{:.2}" y="{:.2}" font-size="{:.2}" font-family="monospace" text-anchor="middle" fill="{}">{}</text>"#,
        mx,
        my,
        font_px,
        s.color,
        xml_escape(&label),
    );
    c.out.push_str("</g>");
}

/// Format a dimension label: measured value with the configured precision,
/// optionally followed by the project units.
pub fn format_dim_label(
    len: f64,
    precision: usize,
    show_units: Option<bool>,
    units: &str,
) -> String {
    if show_units.unwrap_or(true) {
        format!("{:.prec$} {}", len, units, prec = precision)
    } else {
        format!("{:.prec$}", len, prec = precision)
    }
}

fn draw_hatch(
    c: &mut Canvas,
    boundary: &[(f64, f64)],
    pattern: &str,
    angle_deg: f64,
    spacing: f64,
    s: &Style,
    common: &CommonAttrs,
) {
    if boundary.is_empty() || spacing <= 0.0 {
        return;
    }

    if pattern == "solid" {
        let _ = write!(
            c.out,
            r#"<polygon points="{}" fill="{}" fill-opacity="0.5" stroke="none"{}/>"#,
            c.points_attr(boundary),
            s.color,
            id_attr(common)
        );
        return;
    }

    // Bounding box of the boundary
    let mut b = Bounds::empty();
    for &(x, y) in boundary {
        b.add(x, y);
    }
    let cx = (b.min_x + b.max_x) / 2.0;
    let cy = (b.min_y + b.max_y) / 2.0;
    let half_diag = (((b.max_x - b.min_x).powi(2) + (b.max_y - b.min_y).powi(2)).sqrt()) / 2.0;

    let theta = angle_deg.to_radians();
    let (dx, dy) = (theta.cos(), theta.sin());
    let (nx, ny) = (-dy, dx);

    let n = ((half_diag / spacing).ceil() as i64).min(2000);

    c.clip_seq += 1;
    let clip_id = format!("hatch-clip-{}", c.clip_seq);
    let _ = write!(
        c.out,
        r#"<clipPath id="{}"><polygon points="{}"/></clipPath>"#,
        clip_id,
        c.points_attr(boundary)
    );
    let _ = write!(
        c.out,
        r#"<g clip-path="url(#{})"{}>"#,
        clip_id,
        id_attr(common)
    );
    let mut path = String::new();
    for k in -n..=n {
        let ox = cx + nx * spacing * k as f64;
        let oy = cy + ny * spacing * k as f64;
        let (x1, y1) = c.world_to_px(ox - dx * half_diag, oy - dy * half_diag);
        let (x2, y2) = c.world_to_px(ox + dx * half_diag, oy + dy * half_diag);
        let _ = write!(path, "M {:.2} {:.2} L {:.2} {:.2} ", x1, y1, x2, y2);
    }
    let _ = write!(
        c.out,
        r#"<path d="{}" stroke="{}" stroke-width="0.8" fill="none"/>"#,
        path.trim_end(),
        s.color
    );
    c.out.push_str("</g>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::render_layers;

    #[test]
    fn grid_step_scales_with_world_size() {
        assert_eq!(grid_step(10.0), 0.5);
        assert_eq!(grid_step(30.0), 1.0);
        assert_eq!(grid_step(300.0), 10.0);
    }

    fn text_layer(extra_fields: &str) -> Vec<(String, CfFile)> {
        let toml = format!(
            r##"
[layer]
name = "test"
color = "#FFFFFF"

[[text]]
id = "tx-1"
position = [5.0, 5.0]
content = "HOLA"
size = 0.3
{extra_fields}
"##
        );
        let cf: CfFile = toml::from_str(&toml).unwrap();
        vec![("test".to_string(), cf)]
    }

    #[test]
    fn text_font_is_emitted_in_font_family() {
        let svg = render_layers("demo", "m", &text_layer(r#"font = "serif""#), 1200, &[]).svg;
        assert!(svg.contains(r#"font-family="serif""#));
    }

    #[test]
    fn text_without_font_defaults_to_monospace() {
        let svg = render_layers("demo", "m", &text_layer(""), 1200, &[]).svg;
        assert!(svg.contains(r#"font-family="monospace""#));
    }

    #[test]
    fn text_rotation_emits_negated_svg_transform() {
        let svg = render_layers("demo", "m", &text_layer("rotation = 45.0"), 1200, &[]).svg;
        assert!(svg.contains("transform=\"rotate(-45.00,"));
    }

    #[test]
    fn text_bold_and_italic_emit_style_attrs() {
        let svg = render_layers(
            "demo",
            "m",
            &text_layer("bold = true\nitalic = true"),
            1200,
            &[],
        )
        .svg;
        assert!(svg.contains(r#"font-weight="bold""#));
        assert!(svg.contains(r#"font-style="italic""#));
    }
}

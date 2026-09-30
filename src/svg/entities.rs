//! Entity enumeration — drawable primitives with world bounds, shared with
//! the PNG preview metadata and the highlight overlay.

use super::layer::arc_points;
use super::{region_points, Bounds};
use crate::model::CfFile;

// ── Entity enumeration (shared with the PNG preview metadata) ───────────

/// A primitive with its world-space bounding box, for metadata and highlights.
pub struct EntityRecord {
    pub id: Option<String>,
    pub kind: &'static str,
    /// [min_x, min_y, max_x, max_y] in world units.
    pub bbox: [f64; 4],
    /// Text content, for `text` entities.
    pub content: Option<String>,
}

/// Enumerate the drawable primitives of a layer with their world bounds.
pub fn enumerate_entities(cf: &CfFile) -> Vec<EntityRecord> {
    let mut out = Vec::new();
    push_line_records(&mut out, cf);
    push_polyline_records(&mut out, cf);
    push_rect_records(&mut out, cf);
    push_circle_records(&mut out, cf);
    push_arc_records(&mut out, cf);
    push_text_records(&mut out, cf);
    push_point_records(&mut out, cf);
    push_dim_records(&mut out, cf);
    push_hatch_records(&mut out, cf);
    push_fill_records(&mut out, cf);
    out
}

fn bbox_of(points: &[(f64, f64)]) -> [f64; 4] {
    let mut b = Bounds::empty();
    for &(x, y) in points {
        b.add(x, y);
    }
    [b.min_x, b.min_y, b.max_x, b.max_y]
}

fn push_line_records(out: &mut Vec<EntityRecord>, cf: &CfFile) {
    for e in &cf.lines {
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "line",
            bbox: bbox_of(&[(e.from[0], e.from[1]), (e.to[0], e.to[1])]),
            content: None,
        });
    }
}

fn push_polyline_records(out: &mut Vec<EntityRecord>, cf: &CfFile) {
    for e in &cf.polylines {
        let pts: Vec<(f64, f64)> = e.points.iter().map(|p| (p[0], p[1])).collect();
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "polyline",
            bbox: bbox_of(&pts),
            content: None,
        });
    }
}

fn push_rect_records(out: &mut Vec<EntityRecord>, cf: &CfFile) {
    for e in &cf.rects {
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "rect",
            bbox: [
                e.origin[0],
                e.origin[1],
                e.origin[0] + e.width,
                e.origin[1] + e.height,
            ],
            content: None,
        });
    }
}

fn push_circle_records(out: &mut Vec<EntityRecord>, cf: &CfFile) {
    for e in &cf.circles {
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "circle",
            bbox: [
                e.center[0] - e.radius,
                e.center[1] - e.radius,
                e.center[0] + e.radius,
                e.center[1] + e.radius,
            ],
            content: None,
        });
    }
}

fn push_arc_records(out: &mut Vec<EntityRecord>, cf: &CfFile) {
    for e in &cf.arcs {
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "arc",
            bbox: bbox_of(&arc_points(
                e.center[0],
                e.center[1],
                e.radius,
                e.from_angle,
                e.to_angle,
            )),
            content: None,
        });
    }
}

fn push_text_records(out: &mut Vec<EntityRecord>, cf: &CfFile) {
    for e in &cf.texts {
        // Approximate extent from monospace glyph proportions. This is a rough
        // estimate even for the default monospace font, and it is only more
        // wrong when `font` names a non-monospace family or `rotation` is set
        // (the bbox below is axis-aligned and ignores rotation entirely), so
        // `align` anchoring and this bbox are approximate in those cases.
        let w = 0.6 * e.size * e.content.chars().count() as f64;
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "text",
            bbox: [
                e.position[0],
                e.position[1],
                e.position[0] + w,
                e.position[1] + e.size,
            ],
            content: Some(e.content.clone()),
        });
    }
}

fn push_point_records(out: &mut Vec<EntityRecord>, cf: &CfFile) {
    for e in &cf.points {
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "point",
            bbox: [
                e.position[0] - 0.05,
                e.position[1] - 0.05,
                e.position[0] + 0.05,
                e.position[1] + 0.05,
            ],
            content: None,
        });
    }
}

fn push_dim_records(out: &mut Vec<EntityRecord>, cf: &CfFile) {
    for e in &cf.dims {
        let dx = e.to[0] - e.from[0];
        let dy = e.to[1] - e.from[1];
        let len = (dx * dx + dy * dy).sqrt().max(1e-9);
        let (nx, ny) = (-dy / len, dx / len);
        out.push(EntityRecord {
            id: e.common.id.clone(),
            kind: "dim",
            bbox: bbox_of(&[
                (e.from[0], e.from[1]),
                (e.to[0], e.to[1]),
                (e.from[0] + nx * e.offset, e.from[1] + ny * e.offset),
                (e.to[0] + nx * e.offset, e.to[1] + ny * e.offset),
            ]),
            content: None,
        });
    }
}

fn push_hatch_records(out: &mut Vec<EntityRecord>, cf: &CfFile) {
    for e in &cf.hatches {
        let pts = region_points(e.boundary.as_deref(), &e.points, cf);
        if let Some(pts) = pts {
            out.push(EntityRecord {
                id: e.common.id.clone(),
                kind: "hatch",
                bbox: bbox_of(&pts),
                content: None,
            });
        }
    }
}

fn push_fill_records(out: &mut Vec<EntityRecord>, cf: &CfFile) {
    for e in &cf.fills {
        let pts = region_points(e.boundary.as_deref(), &e.points, cf);
        if let Some(pts) = pts {
            out.push(EntityRecord {
                id: e.common.id.clone(),
                kind: "fill",
                bbox: bbox_of(&pts),
                content: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::fixtures::sample_layers;

    #[test]
    fn enumerate_entities_covers_bounds_and_content() {
        let layers = sample_layers();
        let records = enumerate_entities(&layers[0].1);
        // line, rect, circle, arc, text, dim, polyline, hatch
        assert_eq!(records.len(), 8);
        let text = records.iter().find(|r| r.kind == "text").unwrap();
        assert_eq!(text.content.as_deref(), Some("SALA <principal>"));
        let rect = records.iter().find(|r| r.kind == "rect").unwrap();
        assert_eq!(rect.bbox, [1.0, 1.0, 4.0, 3.0]);
    }

    fn kinds_fixture() -> CfFile {
        let toml = r##"
[layer]
name = "kinds"

[[line]]
id = "k-line"
from = [2.0, 3.0]
to = [5.0, 7.0]

[[polyline]]
id = "k-pl"
points = [[0.0, 0.0], [3.0, 1.0], [1.0, 4.0]]
closed = false

[[rect]]
id = "k-rect"
origin = [1.0, 1.0]
width = 3.0
height = 2.0

[[circle]]
id = "k-circle"
center = [1.0, 1.0]
radius = 2.0

[[arc]]
id = "k-arc"
center = [0.0, 0.0]
radius = 1.0
from_angle = 30.0
to_angle = 120.0

[[text]]
id = "k-text"
position = [1.0, 2.0]
content = "AB"
size = 0.5

[[point]]
id = "k-point"
position = [4.0, 4.0]

[[dim]]
id = "k-dim-a"
from = [1.0, 1.0]
to = [5.0, 4.0]

[[dim]]
id = "k-dim-b"
from = [5.0, 1.0]
to = [1.0, 4.0]

[[polyline]]
id = "k-boundary"
points = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]
closed = true

[[hatch]]
id = "k-hatch"
boundary = "k-boundary"
pattern = "ansi31"

[[fill]]
id = "k-fill"
boundary = "k-boundary"
"##;
        toml::from_str(toml).unwrap()
    }

    fn bbox_of_kind(cf: &CfFile, kind: &str) -> [f64; 4] {
        enumerate_entities(cf)
            .into_iter()
            .find(|r| r.kind == kind)
            .unwrap()
            .bbox
    }

    fn bboxes_of_kind(cf: &CfFile, kind: &str) -> Vec<[f64; 4]> {
        enumerate_entities(cf)
            .into_iter()
            .filter(|r| r.kind == kind)
            .map(|r| r.bbox)
            .collect()
    }

    fn assert_close(actual: [f64; 4], expected: [f64; 4]) {
        for (a, b) in actual.iter().zip(expected) {
            assert!(
                (a - b).abs() < 1e-9,
                "bbox {actual:?} differs from {expected:?}"
            );
        }
    }

    #[test]
    fn bboxes_are_exact_per_kind() {
        let cf = kinds_fixture();
        assert_eq!(bbox_of_kind(&cf, "line"), [2.0, 3.0, 5.0, 7.0]);
        assert_eq!(bbox_of_kind(&cf, "polyline"), [0.0, 0.0, 3.0, 4.0]);
        assert_eq!(bbox_of_kind(&cf, "rect"), [1.0, 1.0, 4.0, 3.0]);
        assert_eq!(bbox_of_kind(&cf, "circle"), [-1.0, -1.0, 3.0, 3.0]);
        assert_eq!(bbox_of_kind(&cf, "point"), [3.95, 3.95, 4.05, 4.05]);
        assert_eq!(bbox_of_kind(&cf, "text"), [1.0, 2.0, 1.6, 2.5]);
        // 3-4-5 diagonals in opposite orientations: every offset corner is an
        // extreme of its bbox, so each offset arithmetic mutant moves an edge.
        let mut dims = bboxes_of_kind(&cf, "dim");
        dims.sort_by(|a, b| a[1].partial_cmp(&b[1]).unwrap());
        assert_eq!(dims.len(), 2);
        assert_close(dims[0], [0.7, 0.6, 5.0, 4.0]);
        assert_close(dims[1], [0.7, 1.0, 5.0, 4.4]);
        assert_eq!(bbox_of_kind(&cf, "hatch"), [0.0, 0.0, 2.0, 2.0]);
        assert_eq!(bbox_of_kind(&cf, "fill"), [0.0, 0.0, 2.0, 2.0]);
        let arc = bbox_of_kind(&cf, "arc");
        assert!(arc[0] < 0.0 && arc[2] <= 1.0 && arc[1] > 0.0 && arc[3] <= 1.0);
    }
}

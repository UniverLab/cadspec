//! Transform pass — expands `[[array]]` and `[[mirror]]` constructions into
//! concrete primitives before compilation.
//!
//! A polar array of a tread polyline is a spiral staircase plan; a polar array
//! of a tooth profile is a gear; a mirror duplicates a wing of a building.
//! Expansion happens at load time, so DXF output, SVG/PNG previews, and entity
//! metadata all see the generated geometry with no special cases.
//!
//! Generated copies get derived ids: `tread@1`, `tread@2`, … for arrays and
//! `tread@m` for mirrors, so they can still be referenced (e.g. by hatches)
//! and highlighted.

use crate::model::{ArrayMode, CfArray, CfFile, CfMirror};
use std::collections::HashSet;

// ── Point operations ────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum PointOp {
    Translate {
        dx: f64,
        dy: f64,
    },
    Rotate {
        cx: f64,
        cy: f64,
        angle_rad: f64,
    },
    /// Mirror across the line through (x0, y0) with direction angle `axis_rad`.
    Mirror {
        x0: f64,
        y0: f64,
        axis_rad: f64,
    },
}

impl PointOp {
    fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        match *self {
            PointOp::Translate { dx, dy } => [p[0] + dx, p[1] + dy],
            PointOp::Rotate { cx, cy, angle_rad } => {
                let (s, c) = angle_rad.sin_cos();
                let (x, y) = (p[0] - cx, p[1] - cy);
                [cx + x * c - y * s, cy + x * s + y * c]
            }
            PointOp::Mirror { x0, y0, axis_rad } => {
                let (s, c) = (2.0 * axis_rad).sin_cos();
                let (x, y) = (p[0] - x0, p[1] - y0);
                [x0 + x * c + y * s, y0 + x * s - y * c]
            }
        }
    }

    /// How a direction angle (degrees) maps under this op.
    fn apply_angle_deg(&self, deg: f64) -> f64 {
        match *self {
            PointOp::Translate { .. } => deg,
            PointOp::Rotate { angle_rad, .. } => deg + angle_rad.to_degrees(),
            PointOp::Mirror { axis_rad, .. } => 2.0 * axis_rad.to_degrees() - deg,
        }
    }

    fn flips_orientation(&self) -> bool {
        matches!(self, PointOp::Mirror { .. })
    }
}

// ── Expansion ───────────────────────────────────────────────────────────

/// Expand all `[[array]]` and `[[mirror]]` entries of a layer into concrete
/// primitives. The original constructions are consumed.
pub fn expand_cf(cf: &CfFile) -> CfFile {
    let mut out = cf.clone();
    let arrays = std::mem::take(&mut out.arrays);
    let mirrors = std::mem::take(&mut out.mirrors);

    for array in &arrays {
        expand_array(&mut out, array);
    }
    for mirror in &mirrors {
        expand_mirror(&mut out, mirror);
    }
    out
}

fn target_set(target: &Option<String>, targets: &Option<Vec<String>>) -> HashSet<String> {
    let mut set = HashSet::new();
    if let Some(t) = target {
        set.insert(t.clone());
    }
    if let Some(ts) = targets {
        set.extend(ts.iter().cloned());
    }
    set
}

fn expand_array(out: &mut CfFile, array: &CfArray) {
    let targets = target_set(&array.target, &array.targets);
    if targets.is_empty() || array.count < 2 {
        return;
    }

    for k in 1..array.count {
        let op = match array.mode {
            ArrayMode::Linear => {
                let [dx, dy] = array.offset.unwrap_or([0.0, 0.0]);
                PointOp::Translate {
                    dx: dx * k as f64,
                    dy: dy * k as f64,
                }
            }
            ArrayMode::Polar => {
                let [cx, cy] = array.center.unwrap_or([0.0, 0.0]);
                PointOp::Rotate {
                    cx,
                    cy,
                    angle_rad: (array.step_angle.unwrap_or(0.0) * k as f64).to_radians(),
                }
            }
        };
        let orbit_only = array.mode == ArrayMode::Polar && !array.rotate_items;
        copy_targets(out, &targets, op, orbit_only, &format!("@{}", k));
    }
}

fn expand_mirror(out: &mut CfFile, mirror: &CfMirror) {
    let targets = target_set(&mirror.target, &mirror.targets);
    if targets.is_empty() {
        return;
    }
    let [a, b] = mirror.axis;
    let axis_rad = (b[1] - a[1]).atan2(b[0] - a[0]);
    let op = PointOp::Mirror {
        x0: a[0],
        y0: a[1],
        axis_rad,
    };
    copy_targets(out, &targets, op, false, "@m");
}

/// Clone every targeted primitive, transform it, suffix its id, and append it.
fn copy_targets(
    out: &mut CfFile,
    targets: &HashSet<String>,
    op: PointOp,
    orbit_only: bool,
    suffix: &str,
) {
    let ctx = CopyCtx {
        targets,
        op,
        orbit_only,
        suffix,
    };
    copy_lines(out, &ctx);
    copy_polylines(out, &ctx);
    copy_rects(out, &ctx);
    copy_circles(out, &ctx);
    copy_arcs(out, &ctx);
    copy_texts(out, &ctx);
    copy_points(out, &ctx);
    copy_dims(out, &ctx);
    copy_hatches(out, &ctx);
    copy_fills(out, &ctx);
}

/// The shared inputs of one copy pass over every entity vector.
struct CopyCtx<'a> {
    targets: &'a HashSet<String>,
    op: PointOp,
    orbit_only: bool,
    suffix: &'a str,
}

impl CopyCtx<'_> {
    /// Is this entity's id one of the copy targets?
    fn hit(&self, id: &Option<String>) -> bool {
        id.as_deref().is_some_and(|i| self.targets.contains(i))
    }

    /// The copy's derived id (`base@1`, `base@m`, …).
    fn suffixed(&self, id: &Option<String>) -> Option<String> {
        id.as_ref().map(|i| format!("{}{}", i, self.suffix))
    }
}

fn copy_lines(out: &mut CfFile, ctx: &CopyCtx) {
    let mut new_lines = Vec::new();
    for e in out.lines.iter().filter(|e| ctx.hit(&e.common.id)) {
        let mut c = e.clone();
        c.from = ctx.op.apply(c.from);
        c.to = ctx.op.apply(c.to);
        c.common.id = ctx.suffixed(&e.common.id);
        new_lines.push(c);
    }
    out.lines.extend(new_lines);
}

fn copy_polylines(out: &mut CfFile, ctx: &CopyCtx) {
    let mut new_polys = Vec::new();
    for e in out.polylines.iter().filter(|e| ctx.hit(&e.common.id)) {
        let mut c = e.clone();
        for p in &mut c.points {
            *p = ctx.op.apply(*p);
        }
        c.common.id = ctx.suffixed(&e.common.id);
        new_polys.push(c);
    }
    out.polylines.extend(new_polys);
}

fn copy_rects(out: &mut CfFile, ctx: &CopyCtx) {
    // Rects: a translated copy stays a rect; a rotated or mirrored copy
    // becomes a closed polyline (rects are axis-aligned by definition).
    let mut rect_polys = Vec::new();
    let mut new_rects = Vec::new();
    for e in out.rects.iter().filter(|e| ctx.hit(&e.common.id)) {
        let corners = [
            e.origin,
            [e.origin[0] + e.width, e.origin[1]],
            [e.origin[0] + e.width, e.origin[1] + e.height],
            [e.origin[0], e.origin[1] + e.height],
        ];
        let keeps_shape = matches!(ctx.op, PointOp::Translate { .. }) || ctx.orbit_only;
        if keeps_shape {
            let mut c = e.clone();
            if ctx.orbit_only {
                // Orbit the rect center, keep the rect axis-aligned.
                let center = [e.origin[0] + e.width / 2.0, e.origin[1] + e.height / 2.0];
                let moved = ctx.op.apply(center);
                c.origin = [moved[0] - e.width / 2.0, moved[1] - e.height / 2.0];
            } else {
                c.origin = ctx.op.apply(c.origin);
            }
            c.common.id = ctx.suffixed(&e.common.id);
            new_rects.push(c);
        } else {
            rect_polys.push(crate::model::CfPolyline {
                points: corners.iter().map(|&p| ctx.op.apply(p)).collect(),
                closed: true,
                common: crate::model::CommonAttrs {
                    id: ctx.suffixed(&e.common.id),
                    ..e.common.clone()
                },
            });
        }
    }
    out.rects.extend(new_rects);
    out.polylines.extend(rect_polys);
}

fn copy_circles(out: &mut CfFile, ctx: &CopyCtx) {
    let mut new_circles = Vec::new();
    for e in out.circles.iter().filter(|e| ctx.hit(&e.common.id)) {
        let mut c = e.clone();
        c.center = ctx.op.apply(c.center);
        c.common.id = ctx.suffixed(&e.common.id);
        new_circles.push(c);
    }
    out.circles.extend(new_circles);
}

fn copy_arcs(out: &mut CfFile, ctx: &CopyCtx) {
    let mut new_arcs = Vec::new();
    for e in out.arcs.iter().filter(|e| ctx.hit(&e.common.id)) {
        let mut c = e.clone();
        c.center = ctx.op.apply(c.center);
        if !ctx.orbit_only {
            if ctx.op.flips_orientation() {
                // Reflected sweep: endpoints swap to keep the arc CCW.
                let from = ctx.op.apply_angle_deg(e.to_angle);
                let to = ctx.op.apply_angle_deg(e.from_angle);
                c.from_angle = from;
                c.to_angle = to;
            } else {
                c.from_angle = ctx.op.apply_angle_deg(e.from_angle);
                c.to_angle = ctx.op.apply_angle_deg(e.to_angle);
            }
        }
        c.common.id = ctx.suffixed(&e.common.id);
        new_arcs.push(c);
    }
    out.arcs.extend(new_arcs);
}

fn copy_texts(out: &mut CfFile, ctx: &CopyCtx) {
    // Texts stay upright: only the anchor moves.
    let mut new_texts = Vec::new();
    for e in out.texts.iter().filter(|e| ctx.hit(&e.common.id)) {
        let mut c = e.clone();
        c.position = ctx.op.apply(c.position);
        c.common.id = ctx.suffixed(&e.common.id);
        new_texts.push(c);
    }
    out.texts.extend(new_texts);
}

fn copy_points(out: &mut CfFile, ctx: &CopyCtx) {
    let mut new_points = Vec::new();
    for e in out.points.iter().filter(|e| ctx.hit(&e.common.id)) {
        let mut c = e.clone();
        c.position = ctx.op.apply(c.position);
        c.common.id = ctx.suffixed(&e.common.id);
        new_points.push(c);
    }
    out.points.extend(new_points);
}

fn copy_dims(out: &mut CfFile, ctx: &CopyCtx) {
    let mut new_dims = Vec::new();
    for e in out.dims.iter().filter(|e| ctx.hit(&e.common.id)) {
        let mut c = e.clone();
        c.from = ctx.op.apply(c.from);
        c.to = ctx.op.apply(c.to);
        if ctx.op.flips_orientation() {
            c.offset = -c.offset;
        }
        c.common.id = ctx.suffixed(&e.common.id);
        new_dims.push(c);
    }
    out.dims.extend(new_dims);
}

// Hatches/fills follow their boundary: if the boundary was copied too,
// the copy references the copied boundary id.
fn copy_hatches(out: &mut CfFile, ctx: &CopyCtx) {
    let mut new_hatches = Vec::new();
    for e in out.hatches.iter().filter(|e| ctx.hit(&e.common.id)) {
        let mut c = e.clone();
        c.boundary = retarget_boundary(&e.boundary, ctx);
        transform_points(&mut c.points, &ctx.op);
        c.common.id = ctx.suffixed(&e.common.id);
        new_hatches.push(c);
    }
    out.hatches.extend(new_hatches);
}

fn copy_fills(out: &mut CfFile, ctx: &CopyCtx) {
    let mut new_fills = Vec::new();
    for e in out.fills.iter().filter(|e| ctx.hit(&e.common.id)) {
        let mut c = e.clone();
        c.boundary = retarget_boundary(&e.boundary, ctx);
        transform_points(&mut c.points, &ctx.op);
        c.common.id = ctx.suffixed(&e.common.id);
        new_fills.push(c);
    }
    out.fills.extend(new_fills);
}

/// Keep a region's boundary id unless the boundary itself was copied too —
/// then the copy must reference the copied boundary.
fn retarget_boundary(boundary: &Option<String>, ctx: &CopyCtx) -> Option<String> {
    match boundary {
        Some(b) if ctx.targets.contains(b) => Some(format!("{}{}", b, ctx.suffix)),
        other => other.clone(),
    }
}

fn transform_points(points: &mut Option<Vec<[f64; 2]>>, op: &PointOp) {
    if let Some(points) = points {
        for p in points {
            *p = op.apply(*p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml: &str) -> CfFile {
        toml::from_str(toml).unwrap()
    }

    fn assert_close(actual: [f64; 2], expected: [f64; 2]) {
        assert!(
            (actual[0] - expected[0]).abs() < 1e-9 && (actual[1] - expected[1]).abs() < 1e-9,
            "expected {:?}, got {:?}",
            expected,
            actual
        );
    }

    #[test]
    fn linear_array_translates_copies() {
        let cf = parse(
            r#"
[[rect]]
id = "col"
origin = [0.0, 0.0]
width = 0.3
height = 0.3

[[array]]
target = "col"
mode = "linear"
count = 4
offset = [2.0, 0.0]
"#,
        );
        let out = expand_cf(&cf);
        assert_eq!(out.rects.len(), 4);
        assert!(out.arrays.is_empty());
        assert_eq!(out.rects[3].origin, [6.0, 0.0]);
        assert_eq!(out.rects[3].common.id.as_deref(), Some("col@3"));
    }

    #[test]
    fn polar_array_rotates_polyline_like_a_gear() {
        let cf = parse(
            r#"
[[polyline]]
id = "tooth"
points = [[10.0, 0.0], [11.0, 0.5], [11.0, -0.5]]
closed = true

[[array]]
target = "tooth"
mode = "polar"
count = 12
center = [0.0, 0.0]
step_angle = 30.0
"#,
        );
        let out = expand_cf(&cf);
        assert_eq!(out.polylines.len(), 12);
        // Copy 3 is rotated 90°: (10, 0) → (0, 10)
        let p = out.polylines[3].points[0];
        assert!((p[0]).abs() < 1e-9 && (p[1] - 10.0).abs() < 1e-9);
    }

    #[test]
    fn polar_orbit_keeps_rect_axis_aligned() {
        let cf = parse(
            r#"
[[rect]]
id = "silla"
origin = [4.0, -0.5]
width = 1.0
height = 1.0

[[array]]
target = "silla"
mode = "polar"
count = 4
center = [0.0, 0.0]
step_angle = 90.0
rotate_items = false
"#,
        );
        let out = expand_cf(&cf);
        // Rect stays a rect when orbiting
        assert_eq!(out.rects.len(), 4);
        // Center (4.5, 0) rotated 90° → (0, 4.5); origin = center - half size
        assert_close(out.rects[1].origin, [-0.5, 4.0]);
    }

    #[test]
    fn polar_rotation_converts_rect_to_polyline() {
        let cf = parse(
            r#"
[[rect]]
id = "huella"
origin = [1.0, 0.0]
width = 1.2
height = 0.3

[[array]]
target = "huella"
mode = "polar"
count = 3
center = [0.0, 0.0]
step_angle = 20.0
"#,
        );
        let out = expand_cf(&cf);
        assert_eq!(out.rects.len(), 1, "original rect stays");
        assert_eq!(out.polylines.len(), 2, "rotated copies become polylines");
        assert!(out.polylines.iter().all(|p| p.closed));
    }

    #[test]
    fn mirror_reflects_and_inverts_arc_sweep() {
        let cf = parse(
            r#"
[[line]]
id = "muro"
from = [1.0, 0.0]
to = [1.0, 5.0]

[[arc]]
id = "puerta"
center = [1.0, 2.0]
radius = 0.9
from_angle = 0.0
to_angle = 90.0

[[mirror]]
targets = ["muro", "puerta"]
axis = [[3.0, 0.0], [3.0, 1.0]]
"#,
        );
        let out = expand_cf(&cf);
        assert_eq!(out.lines.len(), 2);
        assert_eq!(out.arcs.len(), 2);
        // Vertical axis at x=3: x=1 → x=5
        assert_close(out.lines[1].from, [5.0, 0.0]);
        let m = &out.arcs[1];
        assert_close(m.center, [5.0, 2.0]);
        // Vertical-axis mirror maps θ → 180−θ, endpoints swapped: [90°,180°]
        assert!((m.from_angle - 90.0).abs() < 1e-9);
        assert!((m.to_angle - 180.0).abs() < 1e-9);
        assert_eq!(m.common.id.as_deref(), Some("puerta@m"));
    }

    #[test]
    fn array_remaps_hatch_boundary_to_copied_polyline() {
        let cf = parse(
            r#"
[[polyline]]
id = "zona"
points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
closed = true

[[hatch]]
id = "zona-hatch"
boundary = "zona"

[[array]]
targets = ["zona", "zona-hatch"]
mode = "linear"
count = 2
offset = [3.0, 0.0]
"#,
        );
        let out = expand_cf(&cf);
        assert_eq!(out.hatches.len(), 2);
        assert_eq!(out.hatches[1].boundary.as_deref(), Some("zona@1"));
        assert_eq!(out.polylines[1].points[0], [3.0, 0.0]);
    }

    #[test]
    fn linear_array_copies_circles_with_shifted_centers() {
        let cf = parse(
            r#"
[[circle]]
id = "c"
center = [1.0, 1.0]
radius = 0.5

[[array]]
target = "c"
mode = "linear"
count = 3
offset = [2.0, 0.0]
"#,
        );
        let out = expand_cf(&cf);
        assert_eq!(out.circles.len(), 3, "both circle copies must be created");
        assert_eq!(out.circles[1].common.id.as_deref(), Some("c@1"));
        assert_eq!(out.circles[2].common.id.as_deref(), Some("c@2"));
        assert_eq!(out.circles[1].center, [3.0, 1.0]);
        assert_eq!(out.circles[2].center, [5.0, 1.0]);
        assert_eq!(out.circles[2].radius, 0.5);
    }

    #[test]
    fn linear_array_copies_texts_moving_anchor() {
        let cf = parse(
            r#"
[[text]]
id = "t"
position = [0.0, 0.0]
content = "X"
size = 0.3

[[array]]
target = "t"
mode = "linear"
count = 3
offset = [2.0, 1.0]
"#,
        );
        let out = expand_cf(&cf);
        assert_eq!(out.texts.len(), 3, "both text copies must be created");
        assert_eq!(out.texts[2].common.id.as_deref(), Some("t@2"));
        assert_eq!(out.texts[2].position, [4.0, 2.0]);
        assert_eq!(out.texts[2].content, "X");
    }

    #[test]
    fn linear_array_copies_dims_moving_endpoints() {
        let cf = parse(
            r#"
[[dim]]
id = "d"
type = "linear"
from = [0.0, 0.0]
to = [2.0, 0.0]
offset = 0.5

[[array]]
target = "d"
mode = "linear"
count = 3
offset = [2.0, 0.0]
"#,
        );
        let out = expand_cf(&cf);
        assert_eq!(out.dims.len(), 3, "both dim copies must be created");
        assert_eq!(out.dims[2].common.id.as_deref(), Some("d@2"));
        assert_eq!(out.dims[2].from, [4.0, 0.0]);
        assert_eq!(out.dims[2].to, [6.0, 0.0]);
        assert_eq!(out.dims[2].offset, 0.5);
    }

    #[test]
    fn linear_array_copies_fill_points_exactly() {
        let cf = parse(
            r#"
[[fill]]
id = "f"
points = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]

[[array]]
target = "f"
mode = "linear"
count = 3
offset = [2.0, 0.0]
"#,
        );
        let out = expand_cf(&cf);
        assert_eq!(out.fills.len(), 3, "both fill copies must be created");
        let copy = &out.fills[2];
        assert_eq!(copy.common.id.as_deref(), Some("f@2"));
        let points = copy.points.as_ref().expect("fill keeps its points");
        assert_eq!(points.len(), 3);
        assert_eq!(points[0], [4.0, 0.0]);
        assert_eq!(points[1], [5.0, 0.0]);
        assert_eq!(points[2], [4.0, 1.0]);
    }

    #[test]
    fn fill_copy_keeps_boundary_when_boundary_not_targeted() {
        // The array targets only the fill; the boundary polyline "zone" is
        // NOT copied, so the fill copy must keep referencing "zone" itself.
        let cf = parse(
            r#"
[[polyline]]
id = "zone"
points = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]
closed = true

[[fill]]
id = "fl"
boundary = "zone"
points = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0]]

[[array]]
target = "fl"
mode = "linear"
count = 2
offset = [3.0, 0.0]
"#,
        );
        let out = expand_cf(&cf);
        assert_eq!(out.fills.len(), 2);
        assert_eq!(out.fills[1].common.id.as_deref(), Some("fl@1"));
        assert_eq!(
            out.fills[1].boundary.as_deref(),
            Some("zone"),
            "boundary not in targets must stay unsuffixed"
        );
        assert_eq!(out.polylines.len(), 1, "untargeted boundary is not copied");
    }

    #[test]
    fn fill_copy_retargets_boundary_when_boundary_copied_too() {
        // Both the fill and its boundary are targeted: the copy must point
        // at the copied boundary id.
        let cf = parse(
            r#"
[[polyline]]
id = "zone"
points = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]
closed = true

[[fill]]
id = "fl"
boundary = "zone"
points = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0]]

[[array]]
targets = ["fl", "zone"]
mode = "linear"
count = 2
offset = [3.0, 0.0]
"#,
        );
        let out = expand_cf(&cf);
        assert_eq!(out.fills.len(), 2);
        assert_eq!(out.polylines.len(), 2);
        assert_eq!(out.polylines[1].common.id.as_deref(), Some("zone@1"));
        assert_eq!(out.fills[1].boundary.as_deref(), Some("zone@1"));
    }

    #[test]
    fn linear_array_copies_points_moving_position() {
        let cf = parse(
            r#"
[[point]]
id = "p"
position = [1.0, 2.0]

[[array]]
target = "p"
mode = "linear"
count = 3
offset = [2.0, 0.0]
"#,
        );
        let out = expand_cf(&cf);
        assert_eq!(out.points.len(), 3, "both point copies must be created");
        assert_eq!(out.points[1].common.id.as_deref(), Some("p@1"));
        assert_eq!(out.points[2].common.id.as_deref(), Some("p@2"));
        assert_eq!(out.points[1].position, [3.0, 2.0]);
        assert_eq!(out.points[2].position, [5.0, 2.0]);
    }

    #[test]
    fn transform_points_translates_some_and_keeps_none() {
        let mut pts = Some(vec![[0.0, 0.0], [1.0, 2.0]]);
        transform_points(&mut pts, &PointOp::Translate { dx: 3.0, dy: -1.0 });
        assert_eq!(pts, Some(vec![[3.0, -1.0], [4.0, 1.0]]));

        let mut none: Option<Vec<[f64; 2]>> = None;
        transform_points(&mut none, &PointOp::Translate { dx: 3.0, dy: -1.0 });
        assert_eq!(none, None, "None points stay None");
    }
}

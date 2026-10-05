//! Intermediate model — structs that represent `.cf` file contents.

use serde::Deserialize;

/// Common visual attributes shared by all primitives.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CommonAttrs {
    pub id: Option<String>,
    pub color: Option<String>,
    pub weight: Option<f64>,
    pub style: Option<LineStyle>,
    pub layer: Option<String>,
    pub belongs_to: Option<String>,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    /// Extrusion height in world units for the 3D view. `None`/0 stays flat;
    /// a closed shape becomes a solid, a line/open polyline becomes a wall.
    pub extrude: Option<f64>,
    /// Base elevation (Z) for the 3D view. Defaults to 0 (the ground plane).
    pub elevation: Option<f64>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
    Dashdot,
}

// ── Primitives ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct CfLine {
    pub from: [f64; 2],
    pub to: [f64; 2],
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfPolyline {
    pub points: Vec<[f64; 2]>,
    #[serde(default)]
    pub closed: bool,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfRect {
    pub origin: [f64; 2],
    pub width: f64,
    pub height: f64,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfCircle {
    pub center: [f64; 2],
    pub radius: f64,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfArc {
    pub center: [f64; 2],
    pub radius: f64,
    pub from_angle: f64,
    pub to_angle: f64,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfText {
    pub position: [f64; 2],
    pub content: String,
    #[serde(default = "default_text_size")]
    pub size: f64,
    pub align: Option<TextAlign>,
    pub font: Option<String>,
    pub rotation: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

fn default_text_size() -> f64 {
    2.5
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfPoint {
    pub position: [f64; 2],
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfDim {
    #[serde(rename = "type")]
    pub dim_type: Option<DimType>,
    pub from: [f64; 2],
    pub to: [f64; 2],
    #[serde(default = "default_offset")]
    pub offset: f64,
    /// Label height in world units (default 0.25).
    pub text_size: Option<f64>,
    /// Decimal places for the measured value (default 2).
    pub precision: Option<u32>,
    /// Append the project units to the label (default true).
    pub show_units: Option<bool>,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

fn default_offset() -> f64 {
    0.5
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DimType {
    Linear,
    Angular,
    Radial,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfHatch {
    /// Reference to a closed polyline or rect id, or inline points.
    #[serde(default)]
    pub boundary: Option<String>,
    /// Inline points (alternative to boundary reference). Used by DXF import to
    /// round-trip a hatch's region without depending on a separate boundary id.
    pub points: Option<Vec<[f64; 2]>>,
    #[serde(default = "default_pattern")]
    pub pattern: String,
    #[serde(default = "default_scale")]
    pub scale: f64,
    #[serde(default = "default_angle")]
    pub angle: f64,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

fn default_pattern() -> String {
    "ansi31".to_string()
}
fn default_scale() -> f64 {
    1.0
}
fn default_angle() -> f64 {
    45.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfGroup {
    pub members: Vec<String>,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ArrayMode {
    Linear,
    Polar,
}

/// Repeats target primitives: linear (offset per copy) or polar (rotation
/// around a center — spiral stairs, gear teeth, radial columns).
#[derive(Debug, Clone, Deserialize)]
pub struct CfArray {
    /// Single target id (alternative to `targets`).
    pub target: Option<String>,
    /// Multiple target ids.
    pub targets: Option<Vec<String>>,
    pub mode: ArrayMode,
    /// Total number of instances, including the original.
    pub count: usize,
    /// Linear: displacement per copy.
    pub offset: Option<[f64; 2]>,
    /// Polar: rotation center.
    pub center: Option<[f64; 2]>,
    /// Polar: degrees per copy (counterclockwise).
    pub step_angle: Option<f64>,
    /// Polar: rotate each copy's geometry (true) or only orbit it (false).
    #[serde(default = "default_true")]
    pub rotate_items: bool,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

/// Mirrors target primitives across an axis defined by two points.
#[derive(Debug, Clone, Deserialize)]
pub struct CfMirror {
    /// Single target id (alternative to `targets`).
    pub target: Option<String>,
    /// Multiple target ids.
    pub targets: Option<Vec<String>>,
    /// Mirror axis: two points [[x1, y1], [x2, y2]].
    pub axis: [[f64; 2]; 2],
    #[serde(flatten)]
    pub common: CommonAttrs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfFill {
    /// Reference to a closed polyline or rect id, or inline points.
    pub boundary: Option<String>,
    /// Inline points (alternative to boundary reference).
    pub points: Option<Vec<[f64; 2]>>,
    #[serde(flatten)]
    pub common: CommonAttrs,
}

// ── 3D solids (CSG) ────────────────────────────────────────────────────

/// A named 3D primitive solid for the extruded/CSG view. Referenced by id from
/// `[[boolean]]`. 3D-only — solids do not appear in the 2D plan or DXF.
#[derive(Debug, Clone, Deserialize)]
pub struct CfSolid {
    pub id: String,
    /// `box` | `cylinder`.
    pub shape: String,
    /// Placement: box minimum corner, or cylinder base-circle center. Default origin.
    pub at: Option<[f64; 3]>,
    /// Box dimensions [sx, sy, sz].
    pub size: Option<[f64; 3]>,
    /// Cylinder radius.
    pub radius: Option<f64>,
    /// Cylinder height.
    pub height: Option<f64>,
    /// Cylinder facet count (default 40).
    pub segments: Option<usize>,
    pub color: Option<String>,
}

/// A CSG operation combining named solids. The result is rendered; the solids
/// it consumes (`base` + `tools`) are not drawn on their own.
#[derive(Debug, Clone, Deserialize)]
pub struct CfBoolean {
    pub id: Option<String>,
    /// `difference` | `union` | `intersection`.
    pub op: String,
    pub base: String,
    #[serde(default)]
    pub tools: Vec<String>,
    pub color: Option<String>,
}

// ── Layer-level metadata ───────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LayerMeta {
    pub name: Option<String>,
    pub color: Option<String>,
    pub line_weight: Option<f64>,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
}

// ── Top-level .cf file ─────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CfFile {
    #[serde(rename = "layer")]
    pub layer_meta: Option<LayerMeta>,
    #[serde(default, rename = "line")]
    pub lines: Vec<CfLine>,
    #[serde(default, rename = "polyline")]
    pub polylines: Vec<CfPolyline>,
    #[serde(default, rename = "rect")]
    pub rects: Vec<CfRect>,
    #[serde(default, rename = "circle")]
    pub circles: Vec<CfCircle>,
    #[serde(default, rename = "arc")]
    pub arcs: Vec<CfArc>,
    #[serde(default, rename = "text")]
    pub texts: Vec<CfText>,
    #[serde(default, rename = "point")]
    pub points: Vec<CfPoint>,
    #[serde(default, rename = "dim")]
    pub dims: Vec<CfDim>,
    #[serde(default, rename = "hatch")]
    pub hatches: Vec<CfHatch>,
    #[serde(default, rename = "fill")]
    pub fills: Vec<CfFill>,
    #[serde(default, rename = "group")]
    pub groups: Vec<CfGroup>,
    #[serde(default, rename = "array")]
    pub arrays: Vec<CfArray>,
    #[serde(default, rename = "mirror")]
    pub mirrors: Vec<CfMirror>,
    #[serde(default, rename = "solid")]
    pub solids: Vec<CfSolid>,
    #[serde(default, rename = "boolean")]
    pub booleans: Vec<CfBoolean>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Default values ────────────────────────────────────────────────

    #[test]
    fn default_common_attrs() {
        let c = CommonAttrs::default();
        assert!(c.id.is_none());
        assert!(c.color.is_none());
        assert!(c.weight.is_none());
        assert!(c.style.is_none());
        assert!(c.layer.is_none());
        assert!(c.belongs_to.is_none());
        assert!(!c.visible); // Default derive for bool = false; serde default_true applies at parse time
        assert!(!c.locked);
        assert!(c.extrude.is_none());
        assert!(c.elevation.is_none());
    }

    #[test]
    fn default_cf_file() {
        let f = CfFile::default();
        assert!(f.layer_meta.is_none());
        assert!(f.lines.is_empty());
        assert!(f.polylines.is_empty());
        assert!(f.rects.is_empty());
        assert!(f.circles.is_empty());
        assert!(f.arcs.is_empty());
        assert!(f.texts.is_empty());
        assert!(f.points.is_empty());
        assert!(f.dims.is_empty());
        assert!(f.hatches.is_empty());
        assert!(f.fills.is_empty());
        assert!(f.groups.is_empty());
        assert!(f.arrays.is_empty());
        assert!(f.mirrors.is_empty());
        assert!(f.solids.is_empty());
        assert!(f.booleans.is_empty());
    }

    #[test]
    fn default_layer_meta() {
        let l = LayerMeta::default();
        assert!(l.name.is_none());
        assert!(l.color.is_none());
        assert!(l.line_weight.is_none());
        assert!(!l.visible); // same as CommonAttrs
        assert!(!l.locked);
    }

    // ── Deserialization roundtrips (JSON → struct) ────────────────────

    #[test]
    fn deserialize_line() {
        let json = r#"{"from":[0.0,0.0],"to":[10.0,5.0],"color":"red"}"#;
        let line: CfLine = serde_json::from_str(json).unwrap();
        assert_eq!(line.from, [0.0, 0.0]);
        assert_eq!(line.to, [10.0, 5.0]);
        assert_eq!(line.common.color.as_deref(), Some("red"));
    }

    #[test]
    fn deserialize_polyline() {
        let json = r#"{"points":[[0,0],[1,2],[3,4]],"closed":true,"layer":"walls"}"#;
        let pl: CfPolyline = serde_json::from_str(json).unwrap();
        assert_eq!(pl.points.len(), 3);
        assert!(pl.closed);
        assert_eq!(pl.common.layer.as_deref(), Some("walls"));
    }

    #[test]
    fn deserialize_polyline_closed_default() {
        let json = r#"{"points":[[0,0],[1,1]]}"#;
        let pl: CfPolyline = serde_json::from_str(json).unwrap();
        assert!(!pl.closed);
    }

    #[test]
    fn deserialize_rect() {
        let json = r#"{"origin":[1.0,2.0],"width":10.0,"height":5.0,"visible":false}"#;
        let r: CfRect = serde_json::from_str(json).unwrap();
        assert_eq!(r.origin, [1.0, 2.0]);
        assert_eq!(r.width, 10.0);
        assert_eq!(r.height, 5.0);
        assert!(!r.common.visible);
    }

    #[test]
    fn deserialize_circle() {
        let json = r#"{"center":[5.0,5.0],"radius":3.0,"color":"blue"}"#;
        let c: CfCircle = serde_json::from_str(json).unwrap();
        assert_eq!(c.center, [5.0, 5.0]);
        assert_eq!(c.radius, 3.0);
    }

    #[test]
    fn deserialize_arc() {
        let json = r#"{"center":[0,0],"radius":5.0,"from_angle":0.0,"to_angle":90.0}"#;
        let a: CfArc = serde_json::from_str(json).unwrap();
        assert_eq!(a.radius, 5.0);
        assert_eq!(a.from_angle, 0.0);
        assert_eq!(a.to_angle, 90.0);
    }

    #[test]
    fn deserialize_text_defaults() {
        let json = r#"{"position":[1.0,1.0],"content":"hello"}"#;
        let t: CfText = serde_json::from_str(json).unwrap();
        assert_eq!(t.content, "hello");
        assert_eq!(t.size, 2.5); // default_text_size
        assert!(t.align.is_none());
        assert!(t.font.is_none());
        assert!(t.rotation.is_none());
        assert!(t.bold.is_none());
        assert!(t.italic.is_none());
    }

    #[test]
    fn deserialize_text_full() {
        let json = r#"{"position":[0,0],"content":"x","size":12.0,"align":"center","font":"mono","rotation":45.0,"bold":true,"italic":true}"#;
        let t: CfText = serde_json::from_str(json).unwrap();
        assert_eq!(t.size, 12.0);
        assert!(matches!(t.align, Some(TextAlign::Center)));
        assert_eq!(t.font.as_deref(), Some("mono"));
        assert_eq!(t.rotation, Some(45.0));
        assert_eq!(t.bold, Some(true));
        assert_eq!(t.italic, Some(true));
    }

    #[test]
    fn deserialize_point() {
        let json = r#"{"position":[7.0,8.0],"id":"p1"}"#;
        let p: CfPoint = serde_json::from_str(json).unwrap();
        assert_eq!(p.position, [7.0, 8.0]);
        assert_eq!(p.common.id.as_deref(), Some("p1"));
    }

    #[test]
    fn deserialize_dim_defaults() {
        let json = r#"{"from":[0,0],"to":[10,0]}"#;
        let d: CfDim = serde_json::from_str(json).unwrap();
        assert_eq!(d.offset, 0.5); // default_offset
        assert!(d.dim_type.is_none());
        assert!(d.text_size.is_none());
        assert!(d.precision.is_none());
        assert!(d.show_units.is_none());
    }

    #[test]
    fn deserialize_dim_full() {
        let json = r#"{"type":"linear","from":[0,0],"to":[10,0],"offset":1.0,"text_size":0.5,"precision":3,"show_units":false}"#;
        let d: CfDim = serde_json::from_str(json).unwrap();
        assert!(matches!(d.dim_type, Some(DimType::Linear)));
        assert_eq!(d.offset, 1.0);
        assert_eq!(d.text_size, Some(0.5));
        assert_eq!(d.precision, Some(3));
        assert_eq!(d.show_units, Some(false));
    }

    #[test]
    fn deserialize_hatch_defaults() {
        let json = r#"{}"#;
        let h: CfHatch = serde_json::from_str(json).unwrap();
        assert_eq!(h.pattern, "ansi31");
        assert_eq!(h.scale, 1.0);
        assert_eq!(h.angle, 45.0);
        assert!(h.boundary.is_none());
        assert!(h.points.is_none());
    }

    #[test]
    fn deserialize_hatch_with_boundary_ref() {
        let json = r#"{"boundary":"polyline1","pattern":"solid","scale":2.0,"angle":0.0}"#;
        let h: CfHatch = serde_json::from_str(json).unwrap();
        assert_eq!(h.boundary.as_deref(), Some("polyline1"));
        assert_eq!(h.pattern, "solid");
        assert_eq!(h.scale, 2.0);
        assert_eq!(h.angle, 0.0);
    }

    #[test]
    fn deserialize_hatch_inline_points() {
        let json = r#"{"points":[[0,0],[10,0],[10,10],[0,10]]}"#;
        let h: CfHatch = serde_json::from_str(json).unwrap();
        assert!(h.points.is_some());
        assert_eq!(h.points.unwrap().len(), 4);
    }

    #[test]
    fn deserialize_group() {
        let json = r#"{"members":["line1","rect1"],"id":"g1"}"#;
        let g: CfGroup = serde_json::from_str(json).unwrap();
        assert_eq!(g.members, vec!["line1".to_string(), "rect1".to_string()]);
        assert_eq!(g.common.id.as_deref(), Some("g1"));
    }

    #[test]
    fn deserialize_array_linear() {
        let json = r#"{"target":"rect1","mode":"linear","count":4,"offset":[2.0,0.0],"rotate_items":false}"#;
        let a: CfArray = serde_json::from_str(json).unwrap();
        assert_eq!(a.target.as_deref(), Some("rect1"));
        assert_eq!(a.mode, ArrayMode::Linear);
        assert_eq!(a.count, 4);
        assert_eq!(a.offset, Some([2.0, 0.0]));
        assert!(!a.rotate_items);
    }

    #[test]
    fn deserialize_array_polar() {
        let json = r#"{"targets":["col1","col2"],"mode":"polar","count":8,"center":[0,0],"step_angle":45.0}"#;
        let a: CfArray = serde_json::from_str(json).unwrap();
        assert_eq!(
            a.targets,
            Some(vec!["col1".to_string(), "col2".to_string()])
        );
        assert_eq!(a.mode, ArrayMode::Polar);
        assert_eq!(a.center, Some([0.0, 0.0]));
        assert_eq!(a.step_angle, Some(45.0));
        assert!(a.rotate_items); // default_true
    }

    #[test]
    fn deserialize_mirror() {
        let json = r#"{"target":"line1","axis":[[0,0],[0,1]]}"#;
        let m: CfMirror = serde_json::from_str(json).unwrap();
        assert_eq!(m.target.as_deref(), Some("line1"));
        assert_eq!(m.axis, [[0.0, 0.0], [0.0, 1.0]]);
    }

    #[test]
    fn deserialize_fill() {
        let json = r#"{"boundary":"rect1"}"#;
        let f: CfFill = serde_json::from_str(json).unwrap();
        assert_eq!(f.boundary.as_deref(), Some("rect1"));
        assert!(f.points.is_none());
    }

    #[test]
    fn deserialize_fill_inline() {
        let json = r#"{"points":[[0,0],[5,0],[5,5],[0,5]]}"#;
        let f: CfFill = serde_json::from_str(json).unwrap();
        assert!(f.boundary.is_none());
        assert_eq!(f.points.as_ref().unwrap().len(), 4);
    }

    #[test]
    fn deserialize_solid_box() {
        let json = r#"{"id":"box1","shape":"box","at":[0,0,0],"size":[10,20,5],"color":"gray"}"#;
        let s: CfSolid = serde_json::from_str(json).unwrap();
        assert_eq!(s.id, "box1");
        assert_eq!(s.shape, "box");
        assert_eq!(s.at, Some([0.0, 0.0, 0.0]));
        assert_eq!(s.size, Some([10.0, 20.0, 5.0]));
        assert_eq!(s.color.as_deref(), Some("gray"));
    }

    #[test]
    fn deserialize_solid_cylinder() {
        let json = r#"{"id":"cyl1","shape":"cylinder","radius":3.0,"height":10.0,"segments":60}"#;
        let s: CfSolid = serde_json::from_str(json).unwrap();
        assert_eq!(s.radius, Some(3.0));
        assert_eq!(s.height, Some(10.0));
        assert_eq!(s.segments, Some(60));
    }

    #[test]
    fn deserialize_boolean() {
        let json = r#"{"op":"difference","base":"box1","tools":["cyl1","cyl2"],"color":"red"}"#;
        let b: CfBoolean = serde_json::from_str(json).unwrap();
        assert_eq!(b.op, "difference");
        assert_eq!(b.base, "box1");
        assert_eq!(b.tools, vec!["cyl1".to_string(), "cyl2".to_string()]);
        assert_eq!(b.color.as_deref(), Some("red"));
    }

    #[test]
    fn deserialize_boolean_default_tools() {
        let json = r#"{"op":"union","base":"a"}"#;
        let b: CfBoolean = serde_json::from_str(json).unwrap();
        assert!(b.tools.is_empty());
        assert!(b.id.is_none());
    }

    // ── LineStyle / enum variants ─────────────────────────────────────

    #[test]
    fn deserialize_line_style_variants() {
        let variants = ["solid", "dashed", "dotted", "dashdot"];
        for v in variants {
            let json = format!(r#"{{"style":"{}"}}"#, v);
            let c: CommonAttrs = serde_json::from_str(&json).unwrap();
            assert!(c.style.is_some(), "failed for {}", v);
        }
    }

    #[test]
    fn deserialize_dim_type_variants() {
        let variants = ["linear", "angular", "radial"];
        for v in variants {
            let json = format!(r#"{{"type":"{}","from":[0,0],"to":[1,1]}}"#, v);
            let d: CfDim = serde_json::from_str(&json).unwrap();
            assert!(d.dim_type.is_some(), "failed for {}", v);
        }
    }

    #[test]
    fn deserialize_text_align_variants() {
        let variants = ["left", "center", "right"];
        for v in variants {
            let json = format!(r#"{{"position":[0,0],"content":"x","align":"{}"}}"#, v);
            let t: CfText = serde_json::from_str(&json).unwrap();
            assert!(t.align.is_some(), "failed for {}", v);
        }
    }

    // ── CfFile full deserialization ───────────────────────────────────

    #[test]
    fn deserialize_cf_file_empty() {
        let json = "{}";
        let f: CfFile = serde_json::from_str(json).unwrap();
        assert!(f.lines.is_empty());
        assert!(f.layer_meta.is_none());
    }

    #[test]
    fn deserialize_cf_file_mixed() {
        let json = r#"{
            "layer": {"name": "main", "color": "ff0000"},
            "line": [{"from":[0,0],"to":[1,1]}],
            "rect": [{"origin":[2,2],"width":5.0,"height":3.0}],
            "circle": [{"center":[0,0],"radius":1.0}],
            "text": [{"position":[0,0],"content":"label"}],
            "solid": [{"id":"s1","shape":"box","size":[1,1,1]}]
        }"#;
        let f: CfFile = serde_json::from_str(json).unwrap();
        assert!(f.layer_meta.is_some());
        assert_eq!(f.lines.len(), 1);
        assert_eq!(f.rects.len(), 1);
        assert_eq!(f.circles.len(), 1);
        assert_eq!(f.texts.len(), 1);
        assert_eq!(f.solids.len(), 1);
        assert_eq!(f.polylines.len(), 0);
        assert_eq!(f.booleans.len(), 0);
    }

    // ── Clone & Debug ─────────────────────────────────────────────────

    #[test]
    fn clone_common_attrs() {
        let c = CommonAttrs {
            id: Some("x".into()),
            ..Default::default()
        };
        let c2 = c.clone();
        assert_eq!(c.id, c2.id);
    }

    #[test]
    fn debug_common_attrs() {
        let c = CommonAttrs::default();
        let dbg = format!("{:?}", c);
        assert!(dbg.contains("CommonAttrs"));
    }
}

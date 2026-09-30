//! DXF shapes — the entity shapes this importer understands, their style
//! attributes, and the `.cf` body text each shape renders to.

use crate::color::aci_to_hex;
use crate::dxf_writer::HATCH_XDATA_APP;
use dxf::{XData, XDataItem};

use super::{escape_string, n};

#[derive(Default, Clone)]
pub(super) struct StyleAttrs {
    color: Option<String>,
    weight: Option<f64>,
    line_style: Option<&'static str>,
}

impl StyleAttrs {
    pub(super) fn from_common(common: &dxf::entities::EntityCommon) -> Self {
        let color = if common.color_24_bit > 0 {
            Some(format!("#{:06X}", common.color_24_bit))
        } else {
            // Spelled `!= 0`, not `> 0`: for a `u8` the two are the same
            // predicate, but `> 0` also has the always-true mutant `>= 0`,
            // which no test can distinguish. `!= 0` has only the killable
            // `== 0` and `!`-deleted forms.
            common.color.index().filter(|i| *i != 0).map(aci_to_hex)
        };
        let weight = (common.lineweight_enum_value > 0)
            .then(|| f64::from(common.lineweight_enum_value) / 100.0);
        let line_style = match common.line_type_name.to_ascii_uppercase().as_str() {
            "DASHED" => Some("dashed"),
            "DOTTED" => Some("dotted"),
            "DASHDOT" => Some("dashdot"),
            _ => None,
        };
        StyleAttrs {
            color,
            weight,
            line_style,
        }
    }

    pub(super) fn emit(&self, out: &mut String) {
        if let Some(c) = &self.color {
            out.push_str(&format!("color = \"{}\"\n", c));
        }
        if let Some(w) = self.weight {
            out.push_str(&format!("weight = {}\n", w));
        }
        if let Some(s) = self.line_style {
            out.push_str(&format!("style = \"{}\"\n", s));
        }
    }
}

pub(super) enum Shape {
    Line {
        from: [f64; 2],
        to: [f64; 2],
    },
    Polyline {
        points: Vec<[f64; 2]>,
        closed: bool,
    },
    Circle {
        center: [f64; 2],
        radius: f64,
    },
    Arc {
        center: [f64; 2],
        radius: f64,
        from_angle: f64,
        to_angle: f64,
    },
    Text {
        position: [f64; 2],
        content: String,
        size: f64,
        rotation: f64,
    },
    Point {
        position: [f64; 2],
    },
    Dim {
        from: [f64; 2],
        to: [f64; 2],
        offset: f64,
    },
    /// A filled region. Imported from one or more SOLID entities; adjacent
    /// SOLID triangles/quads on the same layer are fused into a single fill.
    Fill {
        points: Vec<[f64; 2]>,
    },
    /// A hatched region, recovered from the pattern lines it expanded into by
    /// reading the `CADSPEC_HATCH` XDATA those lines carry.
    Hatch {
        points: Vec<[f64; 2]>,
        angle: f64,
        scale: f64,
        pattern: String,
    },
}

/// Hatch parameters recovered from one pattern line's `CADSPEC_HATCH` XDATA.
pub(super) struct HatchMeta {
    pub(super) group: i32,
    pub(super) angle: f64,
    pub(super) scale: f64,
    pub(super) pattern: String,
    pub(super) points: Vec<[f64; 2]>,
}

/// Read the `CADSPEC_HATCH` XDATA payload (group id, angle, scale, pattern, then
/// the boundary polygon vertices) off a line entity. Returns `None` for any line
/// that this tool did not emit as part of a hatch, so foreign DXF lines are never
/// mistaken for hatch pattern.
pub(super) fn parse_hatch_xdata(xdata: &[XData]) -> Option<HatchMeta> {
    let x = xdata
        .iter()
        .find(|x| x.application_name == HATCH_XDATA_APP)?;
    let mut group = None;
    let mut angle = None;
    let mut scale = None;
    let mut pattern = None;
    let mut points = Vec::new();
    for item in &x.items {
        match item {
            XDataItem::Long(v) if group.is_none() => group = Some(*v),
            XDataItem::Real(v) if angle.is_none() => angle = Some(*v),
            XDataItem::Real(v) if scale.is_none() => scale = Some(*v),
            XDataItem::Str(s) if pattern.is_none() => pattern = Some(s.clone()),
            XDataItem::WorldSpacePosition(p) => points.push([p.x, p.y]),
            _ => {}
        }
    }
    // A hatch needs at least a triangle of boundary to be reconstructable.
    if points.len() < 3 {
        return None;
    }
    Some(HatchMeta {
        group: group?,
        angle: angle?,
        scale: scale?,
        pattern: pattern.unwrap_or_else(|| "ansi31".to_string()),
        points,
    })
}

/// Render a shape body (without `[[header]]`/`id`); returns (id prefix, body).
pub(super) fn emit_shape(shape: &Shape) -> (&'static str, String) {
    match shape {
        Shape::Line { from, to } => ("ln", emit_line_body(from, to)),
        Shape::Polyline { points, closed } => ("pl", emit_polyline_body(points, *closed)),
        Shape::Circle { center, radius } => ("ci", emit_circle_body(center, *radius)),
        Shape::Arc {
            center,
            radius,
            from_angle,
            to_angle,
        } => ("ar", emit_arc_body(center, *radius, *from_angle, *to_angle)),
        Shape::Text {
            position,
            content,
            size,
            rotation,
        } => ("tx", emit_text_body(position, content, *size, *rotation)),
        Shape::Point { position } => ("pt", emit_point_body(position)),
        Shape::Dim { from, to, offset } => ("dm", emit_dim_body(from, to, *offset)),
        Shape::Fill { points } => ("fl", emit_fill_body(points)),
        Shape::Hatch {
            points,
            angle,
            scale,
            pattern,
        } => ("ht", emit_hatch_body(points, *angle, *scale, pattern)),
    }
}

fn emit_line_body(from: &[f64; 2], to: &[f64; 2]) -> String {
    format!(
        "from = [{}, {}]\nto = [{}, {}]\n",
        n(from[0]),
        n(from[1]),
        n(to[0]),
        n(to[1])
    )
}

fn emit_polyline_body(points: &[[f64; 2]], closed: bool) -> String {
    format!("points = [{}]\nclosed = {}\n", points_list(points), closed)
}

fn emit_circle_body(center: &[f64; 2], radius: f64) -> String {
    format!(
        "center = [{}, {}]\nradius = {}\n",
        n(center[0]),
        n(center[1]),
        n(radius)
    )
}

fn emit_arc_body(center: &[f64; 2], radius: f64, from_angle: f64, to_angle: f64) -> String {
    format!(
        "center = [{}, {}]\nradius = {}\nfrom_angle = {}\nto_angle = {}\n",
        n(center[0]),
        n(center[1]),
        n(radius),
        n(from_angle),
        n(to_angle)
    )
}

fn emit_text_body(position: &[f64; 2], content: &str, size: f64, rotation: f64) -> String {
    let mut body = format!(
        "position = [{}, {}]\ncontent = \"{}\"\nsize = {}\n",
        n(position[0]),
        n(position[1]),
        escape_string(content),
        n(size)
    );
    if rotation != 0.0 {
        body.push_str(&format!("rotation = {}\n", n(rotation)));
    }
    body
}

fn emit_point_body(position: &[f64; 2]) -> String {
    format!("position = [{}, {}]\n", n(position[0]), n(position[1]))
}

fn emit_dim_body(from: &[f64; 2], to: &[f64; 2], offset: f64) -> String {
    format!(
        "type = \"linear\"\nfrom = [{}, {}]\nto = [{}, {}]\noffset = {}\n",
        n(from[0]),
        n(from[1]),
        n(to[0]),
        n(to[1]),
        n(offset)
    )
}

fn emit_fill_body(points: &[[f64; 2]]) -> String {
    format!("points = [{}]\n", points_list(points))
}

fn emit_hatch_body(points: &[[f64; 2]], angle: f64, scale: f64, pattern: &str) -> String {
    format!(
        "points = [{}]\npattern = \"{}\"\nangle = {}\nscale = {}\n",
        points_list(points),
        escape_string(pattern),
        n(angle),
        n(scale)
    )
}

/// Format points as `[[x, y], …]` with the crate's number precision.
fn points_list(points: &[[f64; 2]]) -> String {
    points
        .iter()
        .map(|p| format!("[{}, {}]", n(p[0]), n(p[1])))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn header_for(prefix: &str) -> &'static str {
    match prefix {
        "pl" => "polyline",
        "ci" => "circle",
        "ar" => "arc",
        "tx" => "text",
        "pt" => "point",
        "dm" => "dim",
        "fl" => "fill",
        "ht" => "hatch",
        _ => "line",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_shape_renders_a_line_body() {
        let (prefix, body) = emit_shape(&Shape::Line {
            from: [0.0, 1.5],
            to: [2.0, 3.25],
        });
        assert_eq!(prefix, "ln");
        assert_eq!(body, "from = [0.0000, 1.5000]\nto = [2.0000, 3.2500]\n");
    }

    #[test]
    fn emit_shape_renders_a_polyline_body() {
        let (prefix, body) = emit_shape(&Shape::Polyline {
            points: vec![[0.0, 0.0], [1.0, 2.5]],
            closed: true,
        });
        assert_eq!(prefix, "pl");
        assert_eq!(
            body,
            "points = [[0.0000, 0.0000], [1.0000, 2.5000]]\nclosed = true\n"
        );
    }

    #[test]
    fn emit_shape_renders_text_body_rotation_only_when_set() {
        let (_, plain) = emit_shape(&Shape::Text {
            position: [1.0, 2.0],
            content: "SALA A".to_string(),
            size: 0.3,
            rotation: 0.0,
        });
        assert_eq!(
            plain,
            "position = [1.0000, 2.0000]\ncontent = \"SALA A\"\nsize = 0.3000\n"
        );

        let (_, rotated) = emit_shape(&Shape::Text {
            position: [1.0, 2.0],
            content: "x".to_string(),
            size: 0.3,
            rotation: 45.0,
        });
        assert!(rotated.ends_with("rotation = 45.0000\n"), "{}", rotated);
    }

    #[test]
    fn emit_shape_renders_a_hatch_body() {
        let (prefix, body) = emit_shape(&Shape::Hatch {
            points: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            angle: 45.0,
            scale: 1.0,
            pattern: "ansi31".to_string(),
        });
        assert_eq!(prefix, "ht");
        assert_eq!(
            body,
            "points = [[0.0000, 0.0000], [1.0000, 0.0000], [0.0000, 1.0000]]\npattern = \"ansi31\"\nangle = 45.0000\nscale = 1.0000\n"
        );
    }

    #[test]
    fn header_for_maps_id_prefixes_to_table_names() {
        assert_eq!(header_for("ln"), "line");
        assert_eq!(header_for("ht"), "hatch");
        assert_eq!(header_for("unknown"), "line");
    }

    #[test]
    fn header_for_maps_the_point_prefix_to_the_point_table() {
        assert_eq!(header_for("pt"), "point");
        // "ln" has no dedicated arm: it falls through to the default mapping.
        assert_eq!(header_for("ln"), "line");
    }

    #[test]
    fn emit_shape_renders_a_point_body() {
        let (prefix, body) = emit_shape(&Shape::Point {
            position: [1.5, 2.5],
        });
        assert_eq!(prefix, "pt");
        assert_eq!(body, "position = [1.5000, 2.5000]\n");
    }

    #[test]
    fn emit_shape_renders_a_fill_body() {
        let (prefix, body) = emit_shape(&Shape::Fill {
            points: vec![[0.0, 0.0], [1.0, 2.0]],
        });
        assert_eq!(prefix, "fl");
        assert_eq!(body, "points = [[0.0000, 0.0000], [1.0000, 2.0000]]\n");
    }

    // ── StyleAttrs::from_common ───────────────────────────────────────────

    fn common_with(
        color_24_bit: i32,
        color: dxf::Color,
        lineweight_enum_value: i16,
        line_type_name: &str,
    ) -> dxf::entities::EntityCommon {
        dxf::entities::EntityCommon {
            color_24_bit,
            color,
            lineweight_enum_value,
            line_type_name: line_type_name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn from_common_prefers_the_24bit_true_color() {
        let common = common_with(5, dxf::Color::by_layer(), 0, "BYLAYER");
        assert_eq!(
            StyleAttrs::from_common(&common).color,
            Some("#000005".to_string())
        );
    }

    #[test]
    fn from_common_falls_back_to_the_aci_index_color() {
        // No 24-bit true color → the ACI index (7 = white) decides.
        let common = common_with(0, dxf::Color::from_index(7), 0, "BYLAYER");
        assert_eq!(
            StyleAttrs::from_common(&common).color,
            Some("#FFFFFF".to_string())
        );
    }

    #[test]
    fn from_common_keeps_no_color_for_a_zero_aci_index() {
        // ACI index 0 is BYBLOCK, not a palette entry, so no color is set.
        let common = common_with(0, dxf::Color::from_index(0), 0, "BYLAYER");
        assert_eq!(StyleAttrs::from_common(&common).color, None);
    }

    #[test]
    fn from_common_maps_lineweight_enum_values_to_millimetres() {
        let auto = common_with(0, dxf::Color::by_layer(), 0, "BYLAYER");
        assert_eq!(StyleAttrs::from_common(&auto).weight, None);
        let half_mm = common_with(0, dxf::Color::by_layer(), 50, "BYLAYER");
        assert_eq!(StyleAttrs::from_common(&half_mm).weight, Some(0.5));
    }

    #[test]
    fn from_common_maps_line_type_names_case_insensitively() {
        let style_for = |name: &str| {
            StyleAttrs::from_common(&common_with(0, dxf::Color::by_layer(), 0, name)).line_style
        };
        assert_eq!(style_for("dotted"), Some("dotted"));
        assert_eq!(style_for("DOTTED"), Some("dotted"));
        assert_eq!(style_for("dashdot"), Some("dashdot"));
        assert_eq!(style_for("DASHDOT"), Some("dashdot"));
        assert_eq!(style_for("DASHED"), Some("dashed"));
        assert_eq!(style_for("BYLAYER"), None);
    }

    // ── parse_hatch_xdata ─────────────────────────────────────────────────

    fn hatch_app(items: Vec<XDataItem>) -> Vec<XData> {
        vec![XData {
            application_name: HATCH_XDATA_APP.to_string(),
            items,
        }]
    }

    fn wsp(x: f64, y: f64) -> XDataItem {
        XDataItem::WorldSpacePosition(dxf::Point::new(x, y, 0.0))
    }

    #[test]
    fn parse_hatch_xdata_keeps_the_first_group_scale_and_pattern() {
        let items = vec![
            XDataItem::Long(11),
            XDataItem::Long(99),   // later group id must not overwrite the first
            XDataItem::Real(45.0), // angle
            XDataItem::Real(2.0),  // scale
            XDataItem::Real(7.5),  // later real must not overwrite scale
            XDataItem::Str("hatch45".to_string()),
            XDataItem::Str("decoy".to_string()), // later string must not overwrite the pattern
            wsp(0.0, 0.0),
            wsp(1.0, 0.0),
            wsp(0.0, 1.0),
        ];
        let meta =
            parse_hatch_xdata(&hatch_app(items)).expect("a full CADSPEC_HATCH payload must parse");
        assert_eq!(meta.group, 11);
        assert_eq!(meta.angle, 45.0);
        assert_eq!(meta.scale, 2.0);
        assert_eq!(meta.pattern, "hatch45");
        assert_eq!(meta.points, vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    }

    #[test]
    fn parse_hatch_xdata_needs_at_least_three_boundary_points() {
        let payload = |extra: Vec<XDataItem>| {
            let mut items = vec![
                XDataItem::Long(1),
                XDataItem::Real(0.0),
                XDataItem::Real(1.0),
                XDataItem::Str("ansi31".to_string()),
                wsp(0.0, 0.0),
                wsp(1.0, 0.0),
            ];
            items.extend(extra);
            hatch_app(items)
        };

        let two_points = payload(vec![]);
        assert!(
            parse_hatch_xdata(&two_points).is_none(),
            "two boundary points are not a polygon"
        );

        let three_points = payload(vec![wsp(0.0, 1.0)]);
        let meta = parse_hatch_xdata(&three_points).expect("three boundary points must parse");
        assert_eq!(meta.points.len(), 3);
    }

    #[test]
    fn parse_hatch_xdata_ignores_foreign_xdata() {
        let foreign = vec![XData {
            application_name: "NOT_CADSPEC".to_string(),
            items: vec![XDataItem::Long(1), XDataItem::Real(0.0)],
        }];
        assert!(parse_hatch_xdata(&foreign).is_none());
        assert!(parse_hatch_xdata(&[]).is_none());
    }
}

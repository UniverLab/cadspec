//! DXF importer — converts DXF layers/entities into CADspec `.cf` + `project.toml`.

use crate::color::aci_to_hex;
use anyhow::{anyhow, Context, Result};
use dxf::entities::EntityType;
use dxf::Drawing;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

mod dims;
mod fuse;
mod shapes;

use self::dims::{dim_offset, remove_dim_companions};
use self::fuse::{fuse_solids, solid_ring};
use self::shapes::{emit_shape, header_for, parse_hatch_xdata, Shape, StyleAttrs};

struct Imported {
    shape: Shape,
    style: StyleAttrs,
}

#[derive(Default)]
struct LayerFile {
    entities: Vec<Imported>,
    counters: BTreeMap<&'static str, usize>,
}

pub fn import_dxf(input: &Path, output_dir: &Path, layer_filter: Option<&str>) -> Result<()> {
    if !input.exists() {
        return Err(anyhow!(
            "Input DXF file does not exist: {}",
            input.display()
        ));
    }

    fs::create_dir_all(output_dir)
        .with_context(|| format!("Cannot create output dir {}", output_dir.display()))?;

    let mut layers: BTreeMap<String, LayerFile> = BTreeMap::new();
    let mut layer_colors: BTreeMap<String, String> = BTreeMap::new();
    let mut unsupported = 0usize;

    match Drawing::load_file(input) {
        Ok(drawing) => collect_drawing_layers(
            &drawing,
            layer_filter,
            &mut layers,
            &mut layer_colors,
            &mut unsupported,
        ),
        Err(_) => fallback_layers_from_text(input, layer_filter, &mut layers)?,
    }

    if layers.is_empty() {
        fallback_layers_from_text(input, layer_filter, &mut layers)?;
    }
    if layers.is_empty() {
        let content = read_dxf_text(input)?;
        insert_layer_names(
            collect_layer_names_from_layer_table(&content),
            layer_filter,
            &mut layers,
        );
    }

    if layers.is_empty() {
        return Err(anyhow!(
            "No importable entities found in DXF (filter: {})",
            layer_filter.unwrap_or("<none>")
        ));
    }

    let project_name = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported-project");
    let (imported_layers, project_toml) =
        write_layer_files(output_dir, &mut layers, &layer_colors, project_name)?;

    let project_path: PathBuf = output_dir.join("project.toml");
    fs::write(&project_path, project_toml)
        .with_context(|| format!("Cannot write {}", project_path.display()))?;

    println!("✓ Imported DXF: {}", input.display());
    println!("  Layers: {}", imported_layers);
    println!(
        "  Unsupported entities skipped: {} (kept import resilient)",
        unsupported
    );
    println!("  Project: {}", project_path.display());
    Ok(())
}

/// Read every layer of a loaded drawing: layer colors, one entity pass that
/// classifies shapes and collects hatch pattern lines, then the per-layer
/// dimension cleanup and SOLID fusing.
fn collect_drawing_layers(
    drawing: &Drawing,
    layer_filter: Option<&str>,
    layers: &mut BTreeMap<String, LayerFile>,
    layer_colors: &mut BTreeMap<String, String>,
    unsupported: &mut usize,
) {
    for layer in drawing.layers() {
        if let Some(index) = layer.color.index() {
            layer_colors.insert(normalize_layer_name(&layer.name), aci_to_hex(index));
        }
    }

    // Pattern lines of the same hatch (same XDATA group id) are collected here
    // and re-fused into one `[[hatch]]` after the entity pass, keyed by group id.
    let mut hatch_groups: BTreeMap<i32, (String, Imported)> = BTreeMap::new();

    for entity in drawing.entities() {
        let layer_name = normalize_layer_name(&entity.common.layer);
        if layer_filter.is_some_and(|filter| filter != layer_name) {
            continue;
        }

        let style = StyleAttrs::from_common(&entity.common);

        // A line stamped with CADSPEC_HATCH XDATA is one of a hatch's
        // pattern lines: register its group (once) and drop the line, so
        // the round-trip yields one `[[hatch]]` instead of N `[[line]]`.
        if matches!(&entity.specific, EntityType::Line(_))
            && register_hatch_line(entity, &layer_name, &style, &mut hatch_groups)
        {
            continue;
        }

        let Some(shape) = classify_entity(&entity.specific, unsupported) else {
            continue;
        };
        layers
            .entry(layer_name)
            .or_default()
            .entities
            .push(Imported { shape, style });
    }

    // Emit one hatch per collected group into its layer (ordered by
    // group id for deterministic output).
    for (_group, (layer_name, imported)) in hatch_groups {
        layers
            .entry(layer_name)
            .or_default()
            .entities
            .push(imported);
    }

    for layer in layers.values_mut() {
        remove_dim_companions(&mut layer.entities);
        fuse_solids(&mut layer.entities);
    }
}

/// Register a hatch pattern line's group (the first line wins) and consume
/// it. Returns `false` for any line this tool did not emit as part of a
/// hatch, so foreign DXF lines are never mistaken for hatch pattern.
fn register_hatch_line(
    entity: &dxf::entities::Entity,
    layer_name: &str,
    style: &StyleAttrs,
    hatch_groups: &mut BTreeMap<i32, (String, Imported)>,
) -> bool {
    let Some(meta) = parse_hatch_xdata(&entity.common.x_data) else {
        return false;
    };
    hatch_groups.entry(meta.group).or_insert_with(|| {
        (
            layer_name.to_string(),
            Imported {
                shape: Shape::Hatch {
                    points: meta.points,
                    angle: meta.angle,
                    scale: meta.scale,
                    pattern: meta.pattern,
                },
                style: style.clone(),
            },
        )
    });
    true
}

/// Map one DXF entity to the shape it imports as. Unsupported entity types
/// are counted (reported after the import) and skipped.
fn classify_entity(specific: &EntityType, unsupported: &mut usize) -> Option<Shape> {
    match specific {
        EntityType::Line(e) => Some(Shape::Line {
            from: [e.p1.x, e.p1.y],
            to: [e.p2.x, e.p2.y],
        }),
        EntityType::LwPolyline(e) => (e.vertices.len() >= 2).then(|| Shape::Polyline {
            points: e.vertices.iter().map(|v| [v.x, v.y]).collect(),
            closed: e.is_closed(),
        }),
        EntityType::Circle(e) => Some(Shape::Circle {
            center: [e.center.x, e.center.y],
            radius: e.radius,
        }),
        EntityType::Arc(e) => Some(Shape::Arc {
            center: [e.center.x, e.center.y],
            radius: e.radius,
            from_angle: e.start_angle,
            to_angle: e.end_angle,
        }),
        EntityType::Text(e) => Some(Shape::Text {
            position: [e.location.x, e.location.y],
            content: e.value.clone(),
            size: e.text_height.max(0.1),
            rotation: e.rotation,
        }),
        EntityType::ModelPoint(e) => Some(Shape::Point {
            position: [e.location.x, e.location.y],
        }),
        EntityType::RotatedDimension(e) => {
            let from = [e.definition_point_2.x, e.definition_point_2.y];
            let to = [e.definition_point_3.x, e.definition_point_3.y];
            dim_offset(from, to, [e.insertion_point.x, e.insertion_point.y])
                .map(|offset| Shape::Dim { from, to, offset })
        }
        EntityType::Solid(e) => {
            let ring = solid_ring(e);
            (ring.len() >= 3).then_some(Shape::Fill { points: ring })
        }
        _ => {
            *unsupported += 1;
            None
        }
    }
}

/// Recover layer names from the raw DXF text when the drawing will not load.
fn fallback_layers_from_text(
    input: &Path,
    layer_filter: Option<&str>,
    layers: &mut BTreeMap<String, LayerFile>,
) -> Result<()> {
    let content = read_dxf_text(input)?;
    insert_layer_names(
        collect_layer_names_from_text(&content),
        layer_filter,
        layers,
    );
    Ok(())
}

fn read_dxf_text(input: &Path) -> Result<String> {
    fs::read_to_string(input).with_context(|| format!("Cannot read DXF text: {}", input.display()))
}

/// Register each collected layer name (optionally filtered) as an empty layer.
fn insert_layer_names(
    names: Vec<String>,
    layer_filter: Option<&str>,
    layers: &mut BTreeMap<String, LayerFile>,
) {
    for layer_name in names {
        if layer_filter.is_some_and(|filter| filter != layer_name) {
            continue;
        }
        layers.entry(layer_name).or_default();
    }
}

/// Write one `.cf` per layer plus the `project.toml` index; returns
/// (layer count, `project.toml` contents).
fn write_layer_files(
    output_dir: &Path,
    layers: &mut BTreeMap<String, LayerFile>,
    layer_colors: &BTreeMap<String, String>,
    project_name: &str,
) -> Result<(usize, String)> {
    let mut project_toml = format!(
        "[project]\nname = \"{}\"\nscale = \"1:100\"\nunits = \"m\"\n\n[layers]\n",
        escape_string(project_name)
    );

    let mut imported_layers = 0usize;
    for (layer_name, layer_file) in layers.iter_mut() {
        imported_layers += 1;
        let file_name = format!("{}.cf", sanitize_for_filename(layer_name));
        let color = layer_colors
            .get(layer_name)
            .map(String::as_str)
            .unwrap_or("#FFFFFF");
        let mut cf = format!(
            "[layer]\nname = \"{}\"\ncolor = \"{}\"\n\n",
            escape_string(layer_name),
            color
        );
        if layer_file.entities.is_empty() {
            cf.push_str("[[line]]\nfrom = [0.0, 0.0]\nto = [1.0, 0.0]\n");
        } else {
            for entity in &layer_file.entities {
                let (prefix, body) = emit_shape(&entity.shape);
                let count = layer_file
                    .counters
                    .entry(prefix)
                    .and_modify(|v| *v += 1)
                    .or_insert(1);
                cf.push_str(&format!(
                    "[[{}]]\nid = \"{prefix}-{count:03}\"\n",
                    header_for(prefix)
                ));
                cf.push_str(&body);
                entity.style.emit(&mut cf);
                cf.push('\n');
            }
        }
        fs::write(
            output_dir.join(&file_name),
            cf.trim_end().to_string() + "\n",
        )
        .with_context(|| format!("Cannot write layer file {}", file_name))?;
        project_toml.push_str(&format!(
            "\"{}\" = {{ file = \"{}\", locked = false }}\n",
            escape_string(layer_name),
            file_name
        ));
    }
    Ok((imported_layers, project_toml))
}

fn n(v: f64) -> String {
    format!("{:.4}", v)
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn sanitize_for_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "layer".to_string()
    } else {
        out
    }
}

fn normalize_layer_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_string()
    }
}

fn collect_layer_names_from_text(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut lines = content.lines();
    while let Some(code) = lines.next() {
        let Some(value) = lines.next() else {
            break;
        };
        if code.trim() == "8" {
            let layer = normalize_layer_name(value);
            if layer != "0" && !names.iter().any(|existing| existing == &layer) {
                names.push(layer);
            }
        }
    }
    names
}

fn collect_layer_names_from_layer_table(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut lines = content.lines();
    let mut in_layer_record = false;
    while let Some(code) = lines.next() {
        let Some(value) = lines.next() else {
            break;
        };
        let code = code.trim();
        let value = value.trim();
        if code == "100" && value == "AcDbLayerTableRecord" {
            in_layer_record = true;
            continue;
        }
        if in_layer_record && code == "2" {
            let layer = normalize_layer_name(value);
            if layer != "0" && !names.iter().any(|existing| existing == &layer) {
                names.push(layer);
            }
            in_layer_record = false;
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use dxf::entities::{Arc, Circle, Entity, Insert, Line, LwPolyline, ModelPoint, Solid};
    use dxf::{LwPolylineVertex, Point};

    /// Unique-per-run temp directory (under `/tmp/cadspec_importer_*`) that
    /// removes itself on drop, so no test leaves files behind.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "cadspec_importer_{}_{}_{}",
                tag,
                std::process::id(),
                stamp
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("temp dir must be creatable");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    impl AsRef<Path> for TempDir {
        fn as_ref(&self) -> &Path {
            &self.0
        }
    }

    fn line_entity(layer: &str, p1: (f64, f64), p2: (f64, f64)) -> Entity {
        let mut entity = Entity::new(EntityType::Line(Line::new(
            Point::new(p1.0, p1.1, 0.0),
            Point::new(p2.0, p2.1, 0.0),
        )));
        entity.common.layer = layer.to_string();
        entity
    }

    fn corner_present(points: &[[f64; 2]], corner: [f64; 2]) -> bool {
        points
            .iter()
            .any(|p| (p[0] - corner[0]).abs() < 1e-9 && (p[1] - corner[1]).abs() < 1e-9)
    }

    #[test]
    fn collect_drawing_layers_keeps_only_the_filtered_layer() {
        let mut drawing = Drawing::new();
        drawing.add_entity(line_entity("Wall", (0.0, 0.0), (1.0, 1.0)));
        drawing.add_entity(line_entity("Door", (2.0, 2.0), (3.0, 3.0)));

        let mut layers: BTreeMap<String, LayerFile> = BTreeMap::new();
        let mut layer_colors: BTreeMap<String, String> = BTreeMap::new();
        let mut unsupported = 0usize;
        collect_drawing_layers(
            &drawing,
            Some("Wall"),
            &mut layers,
            &mut layer_colors,
            &mut unsupported,
        );

        let keys: Vec<&str> = layers.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec!["Wall"],
            "only entities on the filtered layer are collected"
        );
        assert_eq!(layers["Wall"].entities.len(), 1);
        assert_eq!(unsupported, 0);
    }

    #[test]
    fn classify_entity_converts_each_supported_type() {
        let mut unsupported = 0usize;

        let poly = LwPolyline {
            flags: 1,
            vertices: vec![
                LwPolylineVertex {
                    x: 0.0,
                    y: 0.0,
                    ..Default::default()
                },
                LwPolylineVertex {
                    x: 2.0,
                    y: 4.0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        match classify_entity(&EntityType::LwPolyline(poly), &mut unsupported)
            .expect("lwpolyline must classify")
        {
            Shape::Polyline { points, closed } => {
                assert_eq!(points, vec![[0.0, 0.0], [2.0, 4.0]]);
                assert!(closed, "flags bit 0 marks the polyline closed");
            }
            _ => panic!("expected Shape::Polyline"),
        }

        match classify_entity(
            &EntityType::Circle(Circle::new(Point::new(1.0, 2.0, 0.0), 3.5)),
            &mut unsupported,
        )
        .expect("circle must classify")
        {
            Shape::Circle { center, radius } => {
                assert_eq!(center, [1.0, 2.0]);
                assert_eq!(radius, 3.5);
            }
            _ => panic!("expected Shape::Circle"),
        }

        match classify_entity(
            &EntityType::Arc(Arc::new(Point::new(4.0, 5.0, 0.0), 2.0, 30.0, 90.0)),
            &mut unsupported,
        )
        .expect("arc must classify")
        {
            Shape::Arc {
                center,
                radius,
                from_angle,
                to_angle,
            } => {
                assert_eq!(center, [4.0, 5.0]);
                assert_eq!(radius, 2.0);
                assert_eq!(from_angle, 30.0);
                assert_eq!(to_angle, 90.0);
            }
            _ => panic!("expected Shape::Arc"),
        }

        match classify_entity(
            &EntityType::ModelPoint(ModelPoint::new(Point::new(7.0, -8.0, 0.0))),
            &mut unsupported,
        )
        .expect("model point must classify")
        {
            Shape::Point { position } => assert_eq!(position, [7.0, -8.0]),
            _ => panic!("expected Shape::Point"),
        }

        // Four distinct corners: the ring survives the duplicate collapse
        // with all four rectangle corners, so this is a real fill.
        let solid = Solid::new(
            Point::new(0.0, 0.0, 0.0),
            Point::new(2.0, 0.0, 0.0),
            Point::new(2.0, 3.0, 0.0),
            Point::new(0.0, 3.0, 0.0),
        );
        match classify_entity(&EntityType::Solid(solid), &mut unsupported)
            .expect("solid with distinct corners must classify")
        {
            Shape::Fill { points } => {
                assert_eq!(points.len(), 4, "a quad ring keeps its four corners");
                for corner in [[0.0, 0.0], [2.0, 0.0], [2.0, 3.0], [0.0, 3.0]] {
                    assert!(corner_present(&points, corner), "missing {corner:?}");
                }
            }
            _ => panic!("expected Shape::Fill"),
        }

        assert_eq!(
            unsupported, 0,
            "supported entity types are never counted as unsupported"
        );
    }

    #[test]
    fn classify_entity_skips_a_degenerate_solid() {
        let mut unsupported = 0usize;
        // All four corners equal: the ring collapses to a single point,
        // which is below the three-point minimum for a fill.
        let solid = Solid::new(
            Point::new(5.0, 5.0, 0.0),
            Point::new(5.0, 5.0, 0.0),
            Point::new(5.0, 5.0, 0.0),
            Point::new(5.0, 5.0, 0.0),
        );
        assert!(
            classify_entity(&EntityType::Solid(solid), &mut unsupported).is_none(),
            "a solid that collapses below three points is not a fill"
        );
        assert_eq!(
            unsupported, 0,
            "a degenerate solid is skipped without being counted"
        );
    }

    #[test]
    fn classify_entity_counts_each_unsupported_entity() {
        // Inserts are not importable: each one must bump the counter by one.
        // `+= 1` rewritten as `*= 1` would leave the count at 0, and as
        // `-= 1` it would underflow — both must fail this assert.
        let mut unsupported = 0usize;
        assert!(
            classify_entity(&EntityType::Insert(Insert::default()), &mut unsupported).is_none()
        );
        assert_eq!(unsupported, 1);
        assert!(
            classify_entity(&EntityType::Insert(Insert::default()), &mut unsupported).is_none()
        );
        assert_eq!(unsupported, 2, "each unsupported entity increments once");
    }

    #[test]
    fn fallback_layers_from_text_registers_layers_from_dxf_text() {
        // An `Ok(())` stub would leave `layers` empty; the real fallback
        // must register every layer named by group-8 codes in the text.
        let dir = TempDir::new("fallback_layers");
        let file = dir.as_ref().join("broken.dxf");
        fs::write(&file, "0\nSECTION\n8\nWall\n8\nDoor\n").expect("temp DXF must be writable");
        let mut layers: BTreeMap<String, LayerFile> = BTreeMap::new();
        fallback_layers_from_text(&file, None, &mut layers).expect("fallback must read text");
        let keys: Vec<&str> = layers.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["Door", "Wall"]);
    }

    #[test]
    fn read_dxf_text_returns_contents_and_errors_on_a_missing_file() {
        let dir = TempDir::new("read_dxf_text");
        let file = dir.as_ref().join("sample.dxf");
        let content = "0\nSECTION\n2\nHEADER\n0\nENDSEC\n";
        fs::write(&file, content).expect("temp DXF must be writable");
        assert_eq!(
            read_dxf_text(&file).expect("existing file must read"),
            content
        );

        let missing = dir.as_ref().join("missing.dxf");
        assert!(
            read_dxf_text(&missing).is_err(),
            "a missing path must surface an error"
        );
    }

    #[test]
    fn write_layer_files_counts_layers_and_assigns_unique_ids() {
        let dir = TempDir::new("write_layer_files");
        let mut layers: BTreeMap<String, LayerFile> = BTreeMap::new();
        layers.insert(
            "Wall".to_string(),
            LayerFile {
                entities: vec![
                    Imported {
                        shape: Shape::Line {
                            from: [0.0, 0.0],
                            to: [1.0, 0.0],
                        },
                        style: StyleAttrs::default(),
                    },
                    Imported {
                        shape: Shape::Line {
                            from: [1.0, 0.0],
                            to: [1.0, 1.0],
                        },
                        style: StyleAttrs::default(),
                    },
                ],
                counters: BTreeMap::new(),
            },
        );
        layers.insert(
            "Door".to_string(),
            LayerFile {
                entities: Vec::new(),
                counters: BTreeMap::new(),
            },
        );
        let layer_colors: BTreeMap<String, String> = BTreeMap::new();

        let (imported_layers, project_toml) =
            write_layer_files(dir.as_ref(), &mut layers, &layer_colors, "demo")
                .expect("layer files must be written");

        assert_eq!(imported_layers, 2, "one count per written layer");
        assert!(project_toml.contains("\"Wall\" = { file = \"wall.cf\", locked = false }"));
        assert!(project_toml.contains("\"Door\" = { file = \"door.cf\", locked = false }"));

        let wall = fs::read_to_string(dir.as_ref().join("wall.cf")).expect("wall.cf must exist");
        assert!(wall.contains("id = \"ln-001\""), "{}", wall);
        assert!(
            wall.contains("id = \"ln-002\""),
            "second entity gets the next id: {}",
            wall
        );

        let door = fs::read_to_string(dir.as_ref().join("door.cf")).expect("door.cf must exist");
        assert!(
            door.contains("from = [0.0, 0.0]"),
            "empty layers still emit a placeholder line: {}",
            door
        );
    }

    #[test]
    fn sanitize_for_filename_lowercases_and_replaces_separators() {
        assert_eq!(sanitize_for_filename("A-b_c"), "a-b_c");
        assert_eq!(sanitize_for_filename("abc"), "abc");
        assert_eq!(sanitize_for_filename("a b"), "a_b");
        assert_eq!(sanitize_for_filename("a-b"), "a-b");
        assert_eq!(sanitize_for_filename(""), "layer");
    }

    #[test]
    fn collect_layer_names_from_text_pairs_codes_with_values() {
        let content = "0\nSECTION\n8\nWall\n8\nWall\n8\nDoor\n10\n0\n8\n0\n";
        assert_eq!(
            collect_layer_names_from_text(content),
            vec!["Wall", "Door"],
            "group 8 introduces a layer; duplicates and layer 0 are dropped"
        );
        assert_eq!(
            collect_layer_names_from_text("8\nCeiling\n"),
            vec!["Ceiling"]
        );
    }

    #[test]
    fn collect_layer_names_from_layer_table_reads_layer_records() {
        let content = "0\nTABLE\n2\nLAYER\n100\nAcDbLayerTableRecord\n2\nWall\n100\nAcDbLayerTableRecord\n2\nDoor\n0\nENDTAB\n";
        assert_eq!(
            collect_layer_names_from_layer_table(content),
            vec!["Wall", "Door"]
        );
    }

    #[test]
    fn collect_layer_names_from_layer_table_requires_the_layer_table_record() {
        let content = "100\nNotTheRecord\n2\nWall\n";
        assert_eq!(
            collect_layer_names_from_layer_table(content),
            Vec::<String>::new()
        );
    }

    #[test]
    fn collect_layer_names_from_layer_table_requires_a_record_before_the_name() {
        let content = "2\nWall\n";
        assert_eq!(
            collect_layer_names_from_layer_table(content),
            Vec::<String>::new()
        );
    }

    #[test]
    fn collect_layer_names_from_layer_table_skips_the_zero_layer() {
        let content = "100\nAcDbLayerTableRecord\n2\n0\n";
        assert_eq!(
            collect_layer_names_from_layer_table(content),
            Vec::<String>::new()
        );
    }

    #[test]
    fn collect_layer_names_from_layer_table_deduplicates_repeated_records() {
        let content = "100\nAcDbLayerTableRecord\n2\nWall\n100\nAcDbLayerTableRecord\n2\nWall\n";
        assert_eq!(collect_layer_names_from_layer_table(content), vec!["Wall"]);
    }

    #[test]
    fn insert_layer_names_applies_the_filter() {
        let mut layers: BTreeMap<String, LayerFile> = BTreeMap::new();
        insert_layer_names(
            vec!["a".to_string(), "b".to_string(), "a".to_string()],
            Some("b"),
            &mut layers,
        );
        let keys: Vec<&str> = layers.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["b"]);
    }

    #[test]
    fn insert_layer_names_keeps_every_name_without_filter() {
        let mut layers: BTreeMap<String, LayerFile> = BTreeMap::new();
        insert_layer_names(vec!["a".to_string(), "b".to_string()], None, &mut layers);
        let keys: Vec<&str> = layers.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["a", "b"]);
    }
}

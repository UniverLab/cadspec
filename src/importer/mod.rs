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

//! Preview — rasterizes the SVG scene to PNG + metadata JSON for multimodal AI agents.
//!
//! The PNG is a faithful raster of the SVG renderer (real text, measured
//! dimensions, hatches, line styles), so what an agent *sees* in the image is
//! exactly what `cadspec serve` shows a human. The metadata JSON maps every
//! entity to world and pixel bounding boxes so agents can locate geometry in
//! the image.

use crate::model::CfFile;
use crate::parser::parse_project;
use crate::planos::render_plano;
use crate::render3d::render_scene_3d;
use crate::svg::{
    enumerate_entities, layer_display_color, load_project_layers, render_scene_from, Scene,
};
use anyhow::{Context, Result};
use resvg::{tiny_skia, usvg};
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, OnceLock};

// ── Metadata structures (for the agent) ─────────────────────────────────

#[derive(Serialize, Clone, Copy)]
pub struct WorldBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

#[derive(Serialize)]
pub struct PreviewMeta {
    pub project_name: String,
    pub image_file: String,
    pub width_px: u32,
    pub height_px: u32,
    pub world_bounds: WorldBounds,
    /// Pixels per world unit in the output image.
    pub scale: f64,
    pub units: String,
    pub layers: Vec<LayerInfo>,
    pub entities: Vec<EntityInfo>,
    pub highlighted: Vec<String>,
}

#[derive(Serialize)]
pub struct LayerInfo {
    pub name: String,
    pub entity_count: usize,
    pub color: String,
}

#[derive(Serialize)]
pub struct EntityInfo {
    pub id: Option<String>,
    pub entity_type: String,
    pub layer: String,
    /// Text content (only for `text` entities).
    pub content: Option<String>,
    pub bbox: [f64; 4],       // [min_x, min_y, max_x, max_y] in world coords
    pub pixel_bbox: [u32; 4], // [x, y, w, h] in image coords
}

// ── Public API ──────────────────────────────────────────────────────────

/// Which preview artifacts to write.
#[derive(Clone, Copy)]
pub struct PreviewOutputs {
    /// Write `preview.png` + `preview.meta.json`.
    pub png: bool,
    /// Write `preview.svg`.
    pub svg: bool,
}

/// Which projection to render.
#[derive(Clone, Copy, PartialEq)]
pub enum PreviewView {
    /// Flat 2D plan (the default).
    Plan,
    /// Extruded axonometric 3D view.
    ThreeD,
}

/// Generate preview artifacts (PNG + metadata JSON, and/or SVG) for the project.
///
/// Everything is produced from a single parse + render pass.
/// `width`/`height` are treated as a bounding box: the image keeps the
/// project's aspect ratio and fits inside it.
pub fn generate_preview(
    project_dir: &Path,
    width: u32,
    height: u32,
    layer_filter: Option<&str>,
    highlight: &[String],
    outputs: PreviewOutputs,
    view: PreviewView,
) -> Result<()> {
    if view == PreviewView::ThreeD {
        return generate_preview_3d(project_dir, width, height, layer_filter, outputs);
    }
    let (project, layers) = load_project_layers(project_dir, layer_filter)?;
    let scene = render_scene_from(
        &project.project.name,
        &project.project.units,
        &layers,
        width,
        highlight,
    );

    if outputs.svg {
        let svg_path = project_dir.join("preview.svg");
        std::fs::write(&svg_path, &scene.svg)
            .with_context(|| format!("Cannot write {}", svg_path.display()))?;
        println!("✓ SVG: {}", svg_path.display());
    }

    if !outputs.png {
        return Ok(());
    }

    // Fit the scene inside the requested width × height box.
    let fit = (height as f64 / scene.height_px).min(1.0);
    let pixmap = rasterize(&scene.svg, fit as f32)?;

    let png_path = project_dir.join("preview.png");
    pixmap
        .save_png(&png_path)
        .map_err(|e| anyhow::anyhow!("Failed to save PNG: {}", e))?;

    let meta = build_meta(
        &project.project.name,
        &project.project.units,
        &layers,
        &scene,
        fit,
        highlight,
        &pixmap,
    );
    let json_path = project_dir.join("preview.meta.json");
    std::fs::write(&json_path, serde_json::to_string_pretty(&meta)?)?;

    println!(
        "✓ Preview: {} ({}x{})",
        png_path.display(),
        pixmap.width(),
        pixmap.height()
    );
    println!("✓ Metadata: {}", json_path.display());
    Ok(())
}

/// Render the extruded 3D view. Geometry is projected axonometrically, so there
/// is no world→pixel mapping to emit — we write the PNG (and optional SVG) only.
fn generate_preview_3d(
    project_dir: &Path,
    width: u32,
    height: u32,
    layer_filter: Option<&str>,
    outputs: PreviewOutputs,
) -> Result<()> {
    let (_project, layers) = load_project_layers(project_dir, layer_filter)?;
    let scene = render_scene_3d(&layers, width);

    if outputs.svg {
        let svg_path = project_dir.join("preview.svg");
        std::fs::write(&svg_path, &scene.svg)
            .with_context(|| format!("Cannot write {}", svg_path.display()))?;
        println!("✓ SVG (3D): {}", svg_path.display());
    }

    if !outputs.png {
        return Ok(());
    }

    let fit = (height as f64 / scene.height_px).min(1.0);
    let pixmap = rasterize(&scene.svg, fit as f32)?;
    let png_path = project_dir.join("preview.png");
    pixmap
        .save_png(&png_path)
        .map_err(|e| anyhow::anyhow!("Failed to save PNG: {}", e))?;
    println!(
        "✓ Preview (3D): {} ({}x{})",
        png_path.display(),
        pixmap.width(),
        pixmap.height()
    );
    Ok(())
}

/// Export the scene's 3D solids to a self-contained glTF (`scene.gltf`) — the
/// same meshes the 3D view renders, for an interactive viewer or interchange.
pub fn generate_gltf(project_dir: &Path, layer_filter: Option<&str>) -> Result<()> {
    let (_project, layers) = load_project_layers(project_dir, layer_filter)?;
    let meshes = crate::render3d::scene_meshes(&layers);
    if meshes.is_empty() {
        anyhow::bail!(
            "no 3D geometry to export — declare [[solid]]/[[boolean]] or set `extrude` on a primitive"
        );
    }
    let tris: usize = meshes.iter().map(|(m, _)| m.tris.len()).sum();
    let doc = crate::gltf::scene_to_gltf(&meshes);
    let out = project_dir.join("scene.gltf");
    std::fs::write(&out, &doc).with_context(|| format!("Cannot write {}", out.display()))?;
    println!(
        "✓ glTF: {} ({} meshes, {} triangles)",
        out.display(),
        meshes.len(),
        tris
    );
    Ok(())
}

/// Render a named plano (drawing sheet) to `preview.png` (and/or `preview.svg`).
pub fn generate_plano(
    project_dir: &Path,
    name: &str,
    width: u32,
    height: u32,
    outputs: PreviewOutputs,
) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let plano = project
        .planos
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| {
            let names: Vec<&str> = project.planos.iter().map(|p| p.name.as_str()).collect();
            anyhow::anyhow!("no plano named '{}' (have: {})", name, names.join(", "))
        })?;
    let sheet = render_plano(project_dir, plano, width)?;

    if outputs.svg {
        let svg_path = project_dir.join("preview.svg");
        std::fs::write(&svg_path, &sheet.svg)
            .with_context(|| format!("Cannot write {}", svg_path.display()))?;
        println!("✓ SVG (plano '{}'): {}", name, svg_path.display());
    }
    if !outputs.png {
        return Ok(());
    }
    let fit = (height as f64 / sheet.height_px).min(1.0);
    let pixmap = rasterize(&sheet.svg, fit as f32)?;
    let png_path = project_dir.join("preview.png");
    pixmap
        .save_png(&png_path)
        .map_err(|e| anyhow::anyhow!("Failed to save PNG: {}", e))?;
    println!(
        "✓ Preview (plano '{}'): {} ({}x{})",
        name,
        png_path.display(),
        pixmap.width(),
        pixmap.height()
    );
    Ok(())
}

// ── Internal ────────────────────────────────────────────────────────────

fn build_meta(
    project_name: &str,
    units: &str,
    layers: &[(String, CfFile)],
    scene: &Scene,
    fit: f64,
    highlight: &[String],
    pixmap: &tiny_skia::Pixmap,
) -> PreviewMeta {
    let mut entities = Vec::new();
    let mut layer_infos = Vec::new();

    for (idx, (layer_name, cf)) in layers.iter().enumerate() {
        let records = enumerate_entities(cf);
        layer_infos.push(LayerInfo {
            name: layer_name.clone(),
            entity_count: records.len(),
            color: layer_display_color(cf, idx),
        });
        for rec in records {
            let (x1, y1) = scene.world_to_px(rec.bbox[0], rec.bbox[3]); // top-left
            let (x2, y2) = scene.world_to_px(rec.bbox[2], rec.bbox[1]); // bottom-right
            entities.push(EntityInfo {
                id: rec.id,
                entity_type: rec.kind.to_string(),
                layer: layer_name.clone(),
                content: rec.content,
                bbox: rec.bbox,
                pixel_bbox: [
                    (x1 * fit) as u32,
                    (y1 * fit) as u32,
                    ((x2 - x1) * fit) as u32,
                    ((y2 - y1) * fit) as u32,
                ],
            });
        }
    }

    PreviewMeta {
        project_name: project_name.to_string(),
        image_file: "preview.png".to_string(),
        width_px: pixmap.width(),
        height_px: pixmap.height(),
        world_bounds: WorldBounds {
            min_x: scene.world_bounds[0],
            min_y: scene.world_bounds[1],
            max_x: scene.world_bounds[2],
            max_y: scene.world_bounds[3],
        },
        scale: scene.px_per_unit * fit,
        units: units.to_string(),
        layers: layer_infos,
        entities,
        highlighted: highlight.to_vec(),
    }
}

/// Embedded monospace font: zero font-scan latency and identical, deterministic
/// text rendering on any machine — including containers with no fonts at all.
const EMBEDDED_FONT: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");

fn fontdb() -> Arc<usvg::fontdb::Database> {
    static FONTDB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTDB
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_font_data(EMBEDDED_FONT.to_vec());
            db.set_monospace_family("DejaVu Sans Mono");
            Arc::new(db)
        })
        .clone()
}

/// Rasterize an SVG string at the given scale factor.
fn rasterize(svg: &str, scale: f32) -> Result<tiny_skia::Pixmap> {
    let opt = usvg::Options {
        fontdb: fontdb(),
        ..Default::default()
    };

    let tree = usvg::Tree::from_str(svg, &opt).context("Failed to parse generated SVG")?;
    let size = tree.size();
    let w = ((size.width() * scale).ceil() as u32).max(1);
    let h = ((size.height() * scale).ceil() as u32).max(1);

    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| anyhow::anyhow!("Invalid image dimensions {}x{}", w, h))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterize_produces_scaled_pixmap() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect width="100%" height="100%" fill="#141414"/><line x1="0" y1="0" x2="200" y2="100" stroke="#FFFFFF" stroke-width="2"/></svg>"##;
        let pixmap = rasterize(svg, 1.0).unwrap();
        assert_eq!((pixmap.width(), pixmap.height()), (200, 100));

        let half = rasterize(svg, 0.5).unwrap();
        assert_eq!((half.width(), half.height()), (100, 50));

        // Background must be painted (not transparent)
        let px = pixmap.pixel(5, 50).unwrap();
        assert!(px.alpha() == 255);
    }

    #[test]
    fn rasterize_rejects_invalid_svg() {
        assert!(rasterize("not an svg", 1.0).is_err());
    }

    #[test]
    fn fontdb_loads_embedded_monospace() {
        // The rasterizer's text path depends on the embedded DejaVu Sans Mono
        // being registered in the (lazily built) font database.
        let db = fontdb();
        let query = usvg::fontdb::Query {
            families: &[usvg::fontdb::Family::Name("DejaVu Sans Mono")],
            ..Default::default()
        };
        assert!(
            db.query(&query).is_some(),
            "embedded DejaVu Sans Mono not found in fontdb"
        );
    }
}

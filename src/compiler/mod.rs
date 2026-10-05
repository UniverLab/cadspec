//! Compiler — transforms the intermediate model into a DXF file via DxfWriter.

use crate::color::{hex_to_24bit, hex_to_aci, weight_to_dxf};
use crate::dxf_writer::{DxfWriter, EntityStyle};
use crate::model::{CfFile, CommonAttrs, LineStyle};
use crate::parser::{parse_cf, parse_project, LayerEntry};
use crate::transform::expand_cf;
use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

mod emit;
mod rules;

use self::emit::compile_cf;
pub use self::emit::compile_cf_public;
use self::rules::{is_strict, validate_constraints};

#[derive(Debug, Clone, Copy)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn new(x: f64, y: f64) -> Self {
        Self {
            min_x: x,
            min_y: y,
            max_x: x,
            max_y: y,
        }
    }

    fn include_point(&mut self, x: f64, y: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    fn contains(&self, other: &Bounds) -> bool {
        other.min_x >= self.min_x
            && other.min_y >= self.min_y
            && other.max_x <= self.max_x
            && other.max_y <= self.max_y
    }
}

// ── Style resolution (DRY: one place to convert CommonAttrs → EntityStyle) ──

fn resolve_style(common: &CommonAttrs) -> EntityStyle {
    EntityStyle {
        color_24bit: common.color.as_deref().map(hex_to_24bit),
        lineweight: common.weight.map(weight_to_dxf),
        line_type: common.style.as_ref().map(line_style_to_dxf_name),
    }
}

fn line_style_to_dxf_name(style: &LineStyle) -> String {
    match style {
        LineStyle::Solid => "CONTINUOUS".to_string(),
        LineStyle::Dashed => "DASHED".to_string(),
        LineStyle::Dotted => "DOTTED".to_string(),
        LineStyle::Dashdot => "DASHDOT".to_string(),
    }
}

fn resolve_layer<'a>(common: &'a CommonAttrs, default: &'a str) -> &'a str {
    common.layer.as_deref().unwrap_or(default)
}

fn load_layers(
    project_dir: &Path,
    layers: &IndexMap<String, LayerEntry>,
) -> Result<IndexMap<String, CfFile>> {
    let mut loaded = IndexMap::with_capacity(layers.len());
    for (name, entry) in layers {
        let cf_path = project_dir.join(&entry.file);
        let cf = parse_cf(&cf_path).with_context(|| format!("Failed to parse layer '{}'", name))?;
        loaded.insert(name.clone(), expand_cf(&cf));
    }
    Ok(loaded)
}

fn layer_bbox(cf: &CfFile) -> Option<Bounds> {
    let mut bounds: Option<Bounds> = None;
    let mut include = |x: f64, y: f64| {
        if let Some(b) = bounds.as_mut() {
            b.include_point(x, y);
        } else {
            bounds = Some(Bounds::new(x, y));
        }
    };

    for e in &cf.lines {
        include(e.from[0], e.from[1]);
        include(e.to[0], e.to[1]);
    }
    for e in &cf.polylines {
        for p in &e.points {
            include(p[0], p[1]);
        }
    }
    for e in &cf.rects {
        include(e.origin[0], e.origin[1]);
        include(e.origin[0] + e.width, e.origin[1] + e.height);
    }
    for e in &cf.circles {
        include(e.center[0] - e.radius, e.center[1] - e.radius);
        include(e.center[0] + e.radius, e.center[1] + e.radius);
    }
    for e in &cf.arcs {
        include(e.center[0] - e.radius, e.center[1] - e.radius);
        include(e.center[0] + e.radius, e.center[1] + e.radius);
    }
    for e in &cf.texts {
        include(e.position[0], e.position[1]);
    }
    for e in &cf.points {
        include(e.position[0], e.position[1]);
    }
    for e in &cf.dims {
        include(e.from[0], e.from[1]);
        include(e.to[0], e.to[1]);
    }
    for e in &cf.fills {
        if let Some(points) = &e.points {
            for p in points {
                include(p[0], p[1]);
            }
        }
    }

    bounds
}

fn collect_layer_ids(cf: &CfFile) -> HashSet<String> {
    let mut ids = HashSet::new();
    for_each_common(cf, |common| {
        if let Some(id) = &common.id {
            ids.insert(id.clone());
        }
    });
    ids
}

fn for_each_common(cf: &CfFile, mut f: impl FnMut(&CommonAttrs)) {
    for e in &cf.lines {
        f(&e.common);
    }
    for e in &cf.polylines {
        f(&e.common);
    }
    for e in &cf.rects {
        f(&e.common);
    }
    for e in &cf.circles {
        f(&e.common);
    }
    for e in &cf.arcs {
        f(&e.common);
    }
    for e in &cf.texts {
        f(&e.common);
    }
    for e in &cf.points {
        f(&e.common);
    }
    for e in &cf.dims {
        f(&e.common);
    }
    for e in &cf.hatches {
        f(&e.common);
    }
    for e in &cf.fills {
        f(&e.common);
    }
    for e in &cf.groups {
        f(&e.common);
    }
}

fn print_constraint_issues(issues: &[String]) {
    for issue in issues {
        println!("warning CONSTRAINT VIOLATION");
        println!("  Detail: {}", issue);
        println!("  Action: build continues with warning (set strict = true to fail)");
        println!();
    }
}

// ── Public API ──────────────────────────────────────────────────────────

/// Compile a full project (project.toml + .cf files) into a single DXF.
pub fn compile_project(
    project_dir: &Path,
    layer_filter: Option<&str>,
    output: Option<&Path>,
) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let mut writer = DxfWriter::new();

    let loaded_layers = load_layers(project_dir, &project.layers)?;
    for name in project.layers.keys() {
        let color = loaded_layers
            .get(name)
            .and_then(|cf| cf.layer_meta.as_ref())
            .and_then(|meta| meta.color.as_deref())
            .unwrap_or("#FFFFFF");
        writer.add_layer(name, hex_to_aci(color));
    }
    let issues = validate_constraints(&project, &loaded_layers);
    let strict = is_strict(&project);
    if !issues.is_empty() {
        print_constraint_issues(&issues);
        if strict {
            bail!(
                "Build blocked: {} constraint violation(s) with strict = true",
                issues.len()
            );
        }
    }

    let mut total_entities = 0usize;
    let mut layer_stats: Vec<(String, usize)> = Vec::new();
    for name in project.layers.keys() {
        if layer_filter.is_none_or(|f| f == name) {
            let cf = loaded_layers
                .get(name)
                .with_context(|| format!("Failed to load layer '{}'", name))?;
            let count = entity_count(cf);
            compile_cf(&mut writer, cf, name);
            total_entities += count;
            layer_stats.push((name.to_string(), count));
        }
    }

    let output_path = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| project_dir.join("output.dxf"));
    writer.save(&output_path)?;

    println!("✓ DXF generated: {}", output_path.display());
    println!(
        "  {} entities in {} layers",
        total_entities,
        layer_stats.len()
    );
    for (name, count) in &layer_stats {
        println!("    {}: {} entities", name, count);
    }
    Ok(())
}

/// Validate a project without generating DXF output.
pub fn check_project(project_dir: &Path) -> Result<usize> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let loaded_layers = load_layers(project_dir, &project.layers)?;
    let issues = validate_constraints(&project, &loaded_layers);
    let strict = is_strict(&project);
    let mut total = 0usize;

    println!("Project: {}", project.project.name);
    println!(
        "Scale: {}  Units: {}",
        project.project.scale, project.project.units
    );
    println!();

    for (name, entry) in &project.layers {
        let cf = loaded_layers
            .get(name)
            .with_context(|| format!("Failed to load layer '{}'", name))?;
        let count = entity_count(cf);
        let color = cf
            .layer_meta
            .as_ref()
            .and_then(|m| m.color.as_deref())
            .unwrap_or("#FFFFFF");
        println!("  ✓ {} — {} entities [{}]", entry.file, count, color);
        total += count;
    }

    if !issues.is_empty() {
        println!();
        print_constraint_issues(&issues);
        if strict {
            bail!(
                "Check failed: {} constraint violation(s) with strict = true",
                issues.len()
            );
        }
    }

    println!();
    println!(
        "✓ Valid: {} layers, {} total entities",
        project.layers.len(),
        total
    );
    Ok(total)
}

/// List layers in a project with their status.
pub fn list_layers(project_dir: &Path) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;

    println!("Project: {}", project.project.name);
    println!(
        "Scale: {}  Units: {}",
        project.project.scale, project.project.units
    );
    println!();
    println!("{:<20} {:<25} {:<10} Color", "Layer", "File", "Entities");
    println!("{}", "-".repeat(65));

    for (name, entry) in &project.layers {
        let cf_path = project_dir.join(&entry.file);
        let (status, color) = if cf_path.exists() {
            let cf = expand_cf(&parse_cf(&cf_path)?);
            let count = entity_count(&cf);
            let col = cf
                .layer_meta
                .as_ref()
                .and_then(|m| m.color.as_deref())
                .unwrap_or("#FFFFFF");
            (format!("{}", count), col.to_string())
        } else {
            ("⚠ missing".to_string(), "-".to_string())
        };
        let lock = if entry.locked { " [locked]" } else { "" };
        println!(
            "{:<20} {:<25} {:<10} {}{}",
            name, entry.file, status, color, lock
        );
    }
    Ok(())
}

// ── Machine-readable report (for AI agents / tooling) ──────────────────

#[derive(Serialize)]
pub struct LayerReport {
    pub name: String,
    pub file: String,
    pub entities: Option<usize>,
    pub color: Option<String>,
    pub locked: bool,
    pub missing: bool,
}

#[derive(Serialize)]
pub struct ProjectReport {
    pub name: String,
    pub scale: String,
    pub units: String,
    pub strict: bool,
    pub total_entities: usize,
    pub layers: Vec<LayerReport>,
    pub issues: Vec<String>,
}

/// Build a structured validation report of the project (used by `--json` flags).
pub fn project_report(project_dir: &Path) -> Result<ProjectReport> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let mut loaded: IndexMap<String, CfFile> = IndexMap::new();
    let mut layers = Vec::with_capacity(project.layers.len());
    let mut total = 0usize;

    for (name, entry) in &project.layers {
        let cf_path = project_dir.join(&entry.file);
        if cf_path.exists() {
            let cf = expand_cf(
                &parse_cf(&cf_path).with_context(|| format!("Failed to parse layer '{}'", name))?,
            );
            let count = entity_count(&cf);
            total += count;
            layers.push(LayerReport {
                name: name.clone(),
                file: entry.file.clone(),
                entities: Some(count),
                color: cf.layer_meta.as_ref().and_then(|m| m.color.clone()),
                locked: entry.locked,
                missing: false,
            });
            loaded.insert(name.clone(), cf);
        } else {
            layers.push(LayerReport {
                name: name.clone(),
                file: entry.file.clone(),
                entities: None,
                color: None,
                locked: entry.locked,
                missing: true,
            });
        }
    }

    let issues = validate_constraints(&project, &loaded);
    Ok(ProjectReport {
        name: project.project.name.clone(),
        scale: project.project.scale.clone(),
        units: project.project.units.clone(),
        strict: is_strict(&project),
        total_entities: total,
        layers,
        issues,
    })
}

// ── Internal ────────────────────────────────────────────────────────────

fn entity_count(cf: &CfFile) -> usize {
    cf.lines.len()
        + cf.polylines.len()
        + cf.rects.len()
        + cf.circles.len()
        + cf.arcs.len()
        + cf.texts.len()
        + cf.points.len()
        + cf.dims.len()
        + cf.hatches.len()
        + cf.fills.len()
        + cf.groups.len()
}

/// Warn (without failing the build) when a hatch/fill references a boundary id
/// that does not resolve to any closed polyline or rect in the same layer file.
/// Boundaries are resolved per layer file; a reference to an id defined in a
/// different layer will not resolve and the region is skipped. Shared with
/// `svg.rs` so preview/SVG rendering warns on the same condition as `build`.
pub(crate) fn warn_unresolved_boundary(
    kind: &str,
    entity_id: Option<&str>,
    boundary: &str,
    layer: &str,
) {
    let who = entity_id
        .map(|id| format!("'{}'", id))
        .unwrap_or_else(|| "<unnamed>".to_string());
    eprintln!(
        "warning: {kind} {who} in layer '{layer}' references boundary '{boundary}', \
         which is not a closed polyline or rect in this layer — region skipped"
    );
}

/// Resolve a boundary id to a list of (x,y) points from polylines or rects in the file.
pub fn resolve_boundary(id: &str, cf: &CfFile) -> Option<Vec<(f64, f64)>> {
    // Search polylines
    for poly in &cf.polylines {
        if poly.common.id.as_deref() == Some(id) && poly.closed {
            return Some(poly.points.iter().map(|p| (p[0], p[1])).collect());
        }
    }
    // Search rects
    for rect in &cf.rects {
        if rect.common.id.as_deref() == Some(id) {
            let (x, y) = (rect.origin[0], rect.origin[1]);
            return Some(vec![
                (x, y),
                (x + rect.width, y),
                (x + rect.width, y + rect.height),
                (x, y + rect.height),
            ]);
        }
    }
    None
}

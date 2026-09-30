//! Constraint rules — extraction of `[[constraints]]` pairs and validation
//! of parent / belongs_to / spatial_dependency across layers.

use super::{collect_layer_ids, for_each_common, layer_bbox};
use crate::model::CfFile;
use crate::parser::ProjectFile;
use indexmap::IndexMap;

#[derive(Default)]
struct ConstraintRules {
    parent: Vec<(String, String)>,
    belongs_to: Vec<(String, String)>,
    spatial_dependency: Vec<(String, String)>,
    strict: bool,
}

fn extract_constraint_rules(project: &ProjectFile) -> ConstraintRules {
    let mut rules = ConstraintRules::default();
    let Some(toml::Value::Table(table)) = project.constraints.as_ref() else {
        return rules;
    };

    for (key, value) in table {
        if key == "strict" {
            if let toml::Value::Boolean(strict) = value {
                rules.strict = *strict;
            }
            continue;
        }

        if key.contains('→') {
            if let Some((from, to)) = parse_spatial_rule(key, value) {
                rules.spatial_dependency.push((from, to));
            }
            continue;
        }

        if let toml::Value::Table(child_rules) = value {
            if let Some(toml::Value::String(parent)) = child_rules.get("parent") {
                rules.parent.push((key.clone(), parent.clone()));
            }
            if let Some(toml::Value::String(parent)) = child_rules.get("belongs_to") {
                rules.belongs_to.push((key.clone(), parent.clone()));
            }
        }
    }

    rules
}

/// Parse a `from → to = "spatial_dependency"` constraint pair.
fn parse_spatial_rule(key: &str, value: &toml::Value) -> Option<(String, String)> {
    let toml::Value::String(kind) = value else {
        return None;
    };
    if kind != "spatial_dependency" {
        return None;
    }
    let mut parts = key.split('→').map(|s| s.trim().to_string());
    let from = parts.next()?;
    let to = parts.next()?;
    Some((from, to))
}

pub(super) fn validate_constraints(
    project: &ProjectFile,
    layers: &IndexMap<String, CfFile>,
) -> Vec<String> {
    let rules = extract_constraint_rules(project);
    let mut issues = Vec::new();

    for (child, parent) in &rules.parent {
        match (layers.get(child), layers.get(parent)) {
            (Some(child_cf), Some(parent_cf)) => {
                let child_bbox = layer_bbox(child_cf);
                let parent_bbox = layer_bbox(parent_cf);
                match (child_bbox, parent_bbox) {
                    (Some(c), Some(p)) => {
                        if !p.contains(&c) {
                            issues.push(format!(
                                "Layer '{}' violates parent='{}': child bbox [{:.2}, {:.2}]->[{:.2}, {:.2}] is outside parent bbox [{:.2}, {:.2}]->[{:.2}, {:.2}]",
                                child, parent, c.min_x, c.min_y, c.max_x, c.max_y, p.min_x, p.min_y, p.max_x, p.max_y
                            ));
                        }
                    }
                    _ => {
                        issues.push(format!(
                            "Layer '{}' parent='{}' cannot be validated because one layer has no measurable geometry",
                            child, parent
                        ));
                    }
                }
            }
            _ => issues.push(format!(
                "Invalid parent constraint: '{}' or '{}' layer does not exist",
                child, parent
            )),
        }
    }

    for (child, parent) in &rules.belongs_to {
        match (layers.get(child), layers.get(parent)) {
            (Some(child_cf), Some(parent_cf)) => {
                check_belongs_to(child, parent, child_cf, parent_cf, &mut issues);
            }
            _ => issues.push(format!(
                "Invalid belongs_to constraint: '{}' or '{}' layer does not exist",
                child, parent
            )),
        }
    }

    for (from, to) in &rules.spatial_dependency {
        if layers.contains_key(from) && layers.contains_key(to) {
            issues.push(format!(
                "spatial_dependency '{}' -> '{}' registered; dynamic movement tracking is not implemented yet (warning only)",
                from, to
            ));
        } else {
            issues.push(format!(
                "Invalid spatial_dependency '{}' -> '{}': one layer does not exist",
                from, to
            ));
        }
    }

    issues
}

/// Check one `belongs_to` constraint: every child primitive that references a
/// parent id must find it in the parent layer, and at least one primitive must
/// reference at all when the layer has primitives.
fn check_belongs_to(
    child: &str,
    parent: &str,
    child_cf: &CfFile,
    parent_cf: &CfFile,
    issues: &mut Vec<String>,
) {
    let parent_ids = collect_layer_ids(parent_cf);
    let mut total = 0usize;
    let mut referenced = 0usize;

    for_each_common(child_cf, |common| {
        total += 1;
        let Some(reference) = &common.belongs_to else {
            return;
        };
        referenced += 1;
        if !parent_ids.contains(reference) {
            issues.push(format!(
                "Layer '{}' has belongs_to='{}' but id does not exist in parent layer '{}'",
                child, reference, parent
            ));
        }
    });

    if total > 0 && referenced == 0 {
        issues.push(format!(
            "Layer '{}' has belongs_to='{}' constraint but no primitives define belongs_to references",
            child, parent
        ));
    }
}

pub(super) fn is_strict(project: &ProjectFile) -> bool {
    project.project.strict || extract_constraint_rules(project).strict
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(toml: &str) -> CfFile {
        toml::from_str(toml).unwrap()
    }

    #[test]
    fn spatial_rule_pairs_both_sides_trimmed() {
        let value = toml::Value::String("spatial_dependency".to_string());
        assert_eq!(
            parse_spatial_rule("pisos \u{2192} azotea", &value),
            Some(("pisos".to_string(), "azotea".to_string()))
        );
        assert_eq!(parse_spatial_rule("no-arrow", &value), None);
        // Both halves are taken as split (the original rule did the same).
        assert_eq!(
            parse_spatial_rule("\u{2192} only-one", &value),
            Some(("".to_string(), "only-one".to_string()))
        );
        let other = toml::Value::String("not_spatial".to_string());
        assert_eq!(parse_spatial_rule("a \u{2192} b", &other), None);
        let boolean = toml::Value::Boolean(true);
        assert_eq!(parse_spatial_rule("a \u{2192} b", &boolean), None);
    }

    #[test]
    fn belongs_to_flags_ids_missing_in_parent() {
        let child = layer(
            r#"[layer]
name = "child"

[[line]]
id = "ln-1"
from = [0.0, 0.0]
to = [1.0, 0.0]
belongs_to = "missing"
"#,
        );
        let parent = layer(
            r#"[layer]
name = "parent"

[[line]]
id = "other"
from = [0.0, 0.0]
to = [1.0, 0.0]
"#,
        );
        let mut issues = Vec::new();
        check_belongs_to("child", "parent", &child, &parent, &mut issues);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("belongs_to='missing'"), "{}", issues[0]);
        assert!(
            issues[0].contains("does not exist in parent layer 'parent'"),
            "{}",
            issues[0]
        );
    }

    #[test]
    fn belongs_to_accepts_references_defined_in_parent() {
        let child = layer(
            r#"[layer]
name = "child"

[[line]]
id = "ln-1"
from = [0.0, 0.0]
to = [1.0, 0.0]
belongs_to = "pid"
"#,
        );
        let parent = layer(
            r#"[layer]
name = "parent"

[[line]]
id = "pid"
from = [0.0, 0.0]
to = [1.0, 0.0]
"#,
        );
        let mut issues = Vec::new();
        check_belongs_to("child", "parent", &child, &parent, &mut issues);
        assert!(issues.is_empty(), "{:?}", issues);
    }

    #[test]
    fn belongs_to_requires_at_least_one_reference() {
        let child = layer(
            r#"[layer]
name = "child"

[[line]]
id = "ln-1"
from = [0.0, 0.0]
to = [1.0, 0.0]
"#,
        );
        let parent = layer(
            r#"[layer]
name = "parent"

[[line]]
id = "pid"
from = [0.0, 0.0]
to = [1.0, 0.0]
"#,
        );
        let mut issues = Vec::new();
        check_belongs_to("child", "parent", &child, &parent, &mut issues);
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0].contains("no primitives define belongs_to references"),
            "{}",
            issues[0]
        );
    }

    // ── validate_constraints (whole-rule entry point) ─────────────────────

    fn project(toml: &str) -> ProjectFile {
        toml::from_str(toml).unwrap()
    }

    fn child_inside_parent_project(constraint: &str) -> ProjectFile {
        project(&format!(
            r#"[project]
name = "t"

[layers]
child = {{ file = "child.cf" }}
parent = {{ file = "parent.cf" }}

[constraints]
{constraint}
"#
        ))
    }

    fn inside_layers() -> IndexMap<String, CfFile> {
        let mut layers: IndexMap<String, CfFile> = IndexMap::new();
        layers.insert(
            "child".to_string(),
            layer(
                r#"[layer]
name = "child"

[[line]]
from = [0.0, 0.0]
to = [1.0, 1.0]
"#,
            ),
        );
        layers.insert(
            "parent".to_string(),
            layer(
                r#"[layer]
name = "parent"

[[line]]
from = [-1.0, -1.0]
to = [3.0, 3.0]
"#,
            ),
        );
        layers
    }

    #[test]
    fn parent_constraint_passes_when_the_child_bbox_fits() {
        let proj = child_inside_parent_project(r#"child = { parent = "parent" }"#);
        let issues = validate_constraints(&proj, &inside_layers());
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn parent_constraint_reports_a_child_outside_its_parent() {
        let proj = child_inside_parent_project(r#"child = { parent = "parent" }"#);
        let mut layers = inside_layers();
        layers.insert(
            "parent".to_string(),
            layer(
                r#"[layer]
name = "parent"

[[line]]
from = [5.0, 5.0]
to = [6.0, 6.0]
"#,
            ),
        );
        let issues = validate_constraints(&proj, &layers);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0].contains("violates parent='parent'"),
            "unexpected issue: {}",
            issues[0]
        );
    }

    #[test]
    fn belongs_to_constraint_is_validated_when_both_layers_exist() {
        let proj = child_inside_parent_project(r#"child = { belongs_to = "parent" }"#);
        let mut layers: IndexMap<String, CfFile> = IndexMap::new();
        layers.insert(
            "child".to_string(),
            layer(
                r#"[layer]
name = "child"

[[line]]
id = "ln-1"
from = [0.0, 0.0]
to = [1.0, 0.0]
belongs_to = "missing"
"#,
            ),
        );
        layers.insert(
            "parent".to_string(),
            layer(
                r#"[layer]
name = "parent"

[[line]]
id = "pid"
from = [0.0, 0.0]
to = [1.0, 0.0]
"#,
            ),
        );
        let issues = validate_constraints(&proj, &layers);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0].contains("does not exist in parent layer 'parent'"),
            "unexpected issue: {}",
            issues[0]
        );
    }

    #[test]
    fn belongs_to_constraint_passes_with_a_reference_defined_in_the_parent() {
        let proj = child_inside_parent_project(r#"child = { belongs_to = "parent" }"#);
        let mut layers: IndexMap<String, CfFile> = IndexMap::new();
        layers.insert(
            "child".to_string(),
            layer(
                r#"[layer]
name = "child"

[[line]]
id = "ln-1"
from = [0.0, 0.0]
to = [1.0, 0.0]
belongs_to = "pid"
"#,
            ),
        );
        layers.insert(
            "parent".to_string(),
            layer(
                r#"[layer]
name = "parent"

[[line]]
id = "pid"
from = [0.0, 0.0]
to = [1.0, 0.0]
"#,
            ),
        );
        let issues = validate_constraints(&proj, &layers);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn belongs_to_constraint_stays_silent_for_a_layer_without_primitives() {
        let proj = child_inside_parent_project(r#"child = { belongs_to = "parent" }"#);
        let mut layers: IndexMap<String, CfFile> = IndexMap::new();
        layers.insert(
            "child".to_string(),
            layer(
                r#"[layer]
name = "child"
"#,
            ),
        );
        layers.insert(
            "parent".to_string(),
            layer(
                r#"[layer]
name = "parent"

[[line]]
id = "pid"
from = [0.0, 0.0]
to = [1.0, 0.0]
"#,
            ),
        );
        let issues = validate_constraints(&proj, &layers);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn spatial_dependency_needs_both_layers_to_exist() {
        let proj =
            child_inside_parent_project("\"pisos \u{2192} azotea\" = \"spatial_dependency\"");
        // Both layers registered → warning-only registration message.
        let mut layers: IndexMap<String, CfFile> = IndexMap::new();
        layers.insert(
            "pisos".to_string(),
            layer(
                r#"[layer]
name = "pisos"
"#,
            ),
        );
        layers.insert(
            "azotea".to_string(),
            layer(
                r#"[layer]
name = "azotea"
"#,
            ),
        );
        let issues = validate_constraints(&proj, &layers);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0].contains("registered; dynamic movement tracking"),
            "unexpected issue: {}",
            issues[0]
        );

        // One side missing → invalid, never the registration message.
        layers.shift_remove("azotea");
        let issues = validate_constraints(&proj, &layers);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(
            issues[0].contains("Invalid spatial_dependency 'pisos' -> 'azotea'"),
            "unexpected issue: {}",
            issues[0]
        );
    }
}

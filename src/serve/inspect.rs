//! Request inspection helpers: safe file names, query parsing, body
//! reading, entity→TOML lookup, HTML escaping, browser launch.

use crate::parser::parse_project;
use std::io::BufRead;
use std::path::Path;

/// Accept only a bare `*.cf` filename (no path traversal) for the editor.
pub(super) fn safe_cf_name(name: &str) -> Option<String> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || !name.ends_with(".cf")
    {
        return None;
    }
    Some(name.to_string())
}

/// Read the remaining request headers, then the body of `Content-Length` bytes.
///
/// Generic over `BufRead` so tests can drive it with an in-memory cursor; the
/// production caller passes `&mut BufReader<TcpStream>` unchanged.
pub(super) fn read_request_body<R: BufRead>(reader: &mut R) -> Vec<u8> {
    let mut len = 0usize;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let t = line.trim_end();
        if t.is_empty() {
            break;
        }
        if let Some(v) = t.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; len];
    let _ = std::io::Read::read_exact(reader, &mut body);
    body
}

pub(super) fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| percent_decode(v))
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    // Every arm consumes at least one byte, so `len` iterations suffice.
    // The explicit bound keeps a (mutated) step operator from spinning
    // forever: a stuck `i` simply yields wrong output after `len` passes.
    for _ in 0..bytes.len() {
        if i >= bytes.len() {
            break;
        }
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                    out.push(h * 16 + l);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Find the raw TOML block that defines `id` and return it as JSON, so the
/// viewer can hand an agent the exact source to edit.
pub(super) fn entity_block_json(project_dir: &Path, id: &str) -> String {
    // Generated copies (array/mirror) carry an @ suffix; their source is the base id.
    let base = id.split('@').next().unwrap_or(id);

    let lookup = || -> Option<(String, String, String)> {
        let project = parse_project(&project_dir.join("project.toml")).ok()?;
        for (layer, entry) in &project.layers {
            let path = project_dir.join(&entry.file);
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(block) = find_block(&text, base) {
                return Some((layer.clone(), entry.file.clone(), block));
            }
        }
        None
    };

    match lookup() {
        Some((layer, file, block)) => serde_json::json!({
            "id": id,
            "base_id": base,
            "generated": id != base,
            "layer": layer,
            "file": file,
            "block": block,
        })
        .to_string(),
        None => serde_json::json!({ "id": id, "error": "not found" }).to_string(),
    }
}

/// Extract the `[[...]]` block (with leading comments) that contains `id = "<id>"`.
///
/// Uses toml_edit spans, so multi-line values (e.g. `points = [` …) are kept
/// intact instead of being cut at lines that merely look like TOML headers.
fn find_block(text: &str, id: &str) -> Option<String> {
    let doc = toml_edit::ImDocument::parse(text).ok()?;
    let mut span: Option<std::ops::Range<usize>> = None;
    for (_key, item) in doc.iter() {
        if let toml_edit::Item::ArrayOfTables(tables) = item {
            for table in tables.iter() {
                if table.get("id").and_then(|v| v.as_str()) == Some(id) {
                    span = table.span();
                }
            }
        }
    }
    let span = span?;

    let lines: Vec<&str> = text.lines().collect();
    let span_start_line = text[..span.start.min(text.len())].matches('\n').count();
    let span_end_line = text[..span.end.min(text.len())]
        .matches('\n')
        .count()
        .min(lines.len().saturating_sub(1));

    // The span covers the key/value pairs; step back to the [[header]] line
    // and pull in any comment lines directly above it.
    let header = lines[..=span_start_line.min(lines.len().saturating_sub(1))]
        .iter()
        .rposition(|l| l.trim_start().starts_with("[["))?;
    // Bounded form of "walk up while the line above is a comment": each
    // candidate is inspected at most once, so no step operator can hang here.
    let comments = lines[..header]
        .iter()
        .rev()
        .take_while(|l| l.trim_start().starts_with('#'))
        .count();
    let start = header - comments;

    Some(
        lines[start..=span_end_line]
            .join("\n")
            .trim_end()
            .to_string(),
    )
}

pub(super) fn open_browser(url: &str) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if result.is_err() {
        println!("  (could not open browser automatically)");
    }
}

pub(super) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn safe_cf_name_accepts_bare_cf_names() {
        // A plain `*.cf` filename must pass through untouched: this pins the
        // `Some(...)` return, so a `-> None` rewrite or a dropped `!` on the
        // `ends_with(".cf")` check cannot land unnoticed.
        assert_eq!(safe_cf_name("plano.cf").as_deref(), Some("plano.cf"));
        assert_eq!(safe_cf_name("muros-v2.cf").as_deref(), Some("muros-v2.cf"));
    }

    #[test]
    fn safe_cf_name_rejects_paths_and_foreign_extensions() {
        // Each rejection pins one of the `||` guards in the condition: with a
        // `&&` substitution the matching input would wrongly return `Some`.
        assert_eq!(safe_cf_name("a/b.cf"), None); // contains '/'
        assert_eq!(safe_cf_name("a\\b.cf"), None); // contains '\\'
        assert_eq!(safe_cf_name("a..b.cf"), None); // contains ".."
        assert_eq!(safe_cf_name("../plano.cf"), None); // traversal
        assert_eq!(safe_cf_name(""), None); // empty
        assert_eq!(safe_cf_name("x.txt"), None); // needs `!ends_with(".cf")`
    }

    #[test]
    fn read_request_body_reads_content_length_body() {
        let raw = b"Content-Length: 5\r\nX-Other: y\r\n\r\nhello".to_vec();
        let mut reader = BufReader::new(Cursor::new(raw));
        let body = read_request_body(&mut reader);
        assert_eq!(body, b"hello".to_vec());
    }

    #[test]
    fn read_request_body_without_content_length_is_empty() {
        let raw = b"X-Foo: bar\r\n\r\nleftover".to_vec();
        let mut reader = BufReader::new(Cursor::new(raw));
        let body = read_request_body(&mut reader);
        assert!(body.is_empty(), "body: {:?}", body);
    }

    #[test]
    fn percent_decode_translates_hex_escapes() {
        // Uppercase, lowercase and digit hex — plus arithmetic-sensitive arms.
        assert_eq!(percent_decode("%41"), "A");
        assert_eq!(percent_decode("%4a"), "J"); // lowercase `b'a'..=b'f'` arm
        assert_eq!(percent_decode("%4A"), "J"); // uppercase `b'A'..=b'F'` arm
        assert_eq!(percent_decode("%30"), "0"); // digit `b'0'..=b'9'` arm
        assert_eq!(percent_decode("%2B"), "+"); // encoded '+'
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("ln%2D001"), "ln-001");
    }

    #[test]
    fn percent_decode_handles_truncated_and_invalid_sequences() {
        // "%" / "%4" pin the `i + 2 < len` guard and its arithmetic: a
        // rewritten guard makes these index out of bounds and panic.
        assert_eq!(percent_decode("%"), "%");
        assert_eq!(percent_decode("%4"), "%4");
        // Invalid hex takes the `i += 1` fallback arm.
        assert_eq!(percent_decode("%zz"), "%zz");
        // The '+' arm and the plain-byte arm.
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("abc"), "abc");
        assert_eq!(percent_decode(""), "");
    }

    #[test]
    fn entity_block_json_reports_block_and_missing_ids() {
        let dir = std::env::temp_dir().join(format!("cadspec_serve_entity_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("project.toml"),
            "[project]\nname = \"Inspect Test\"\n\n[layers]\nmuros = { file = \"muros.cf\" }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("muros.cf"),
            "[layer]\nname = \"muros\"\n\n# Puerta principal\n[[line]]\nid = \"ln-1\"\nfrom = [0.0, 0.0]\nto = [1.0, 0.0]\n",
        )
        .unwrap();

        let found: serde_json::Value =
            serde_json::from_str(&entity_block_json(&dir, "ln-1")).unwrap();
        assert_eq!(found["id"], "ln-1");
        assert_eq!(found["base_id"], "ln-1");
        assert_eq!(found["generated"], false);
        assert_eq!(found["layer"], "muros");
        assert_eq!(found["file"], "muros.cf");
        let block = found["block"].as_str().unwrap();
        assert!(block.contains("[[line]]"), "block: {}", block);
        assert!(block.contains("id = \"ln-1\""), "block: {}", block);

        // A generated copy (@ suffix) reports its base id and `generated`.
        let gen: serde_json::Value =
            serde_json::from_str(&entity_block_json(&dir, "ln-1@2")).unwrap();
        assert_eq!(gen["base_id"], "ln-1");
        assert_eq!(gen["generated"], true);
        assert!(gen["block"].as_str().unwrap().contains("[[line]]"));

        // Unknown ids are an error payload, not a panic.
        let missing: serde_json::Value =
            serde_json::from_str(&entity_block_json(&dir, "nope")).unwrap();
        assert_eq!(missing["id"], "nope");
        assert_eq!(missing["error"], "not found");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_block_extracts_entity_with_comments() {
        let text = r#"[layer]
name = "muros"

# Puerta principal
[[arc]]
id = "ar-puerta"
center = [0.0, 2.5]
radius = 0.9

[[line]]
id = "ln-otro"
from = [0.0, 0.0]
to = [1.0, 0.0]
"#;
        let block = find_block(text, "ar-puerta").unwrap();
        assert!(block.starts_with("# Puerta principal"));
        assert!(block.contains("[[arc]]"));
        assert!(block.contains("radius = 0.9"));
        assert!(!block.contains("ln-otro"));

        let last = find_block(text, "ln-otro").unwrap();
        assert!(last.contains("[[line]]"));
        assert!(last.contains("to = [1.0, 0.0]"));

        assert!(find_block(text, "missing").is_none());
    }

    #[test]
    fn find_block_keeps_multiline_arrays_intact() {
        let text = r#"[[polyline]]
id = "pl-huella"
points = [
    [0.30, 0.0],
    [1.55, 0.0],
]
closed = true

[[circle]]
id = "ci-otro"
center = [0.0, 0.0]
radius = 1.0
"#;
        let block = find_block(text, "pl-huella").unwrap();
        assert!(block.contains("[1.55, 0.0],"), "block: {}", block);
        assert!(block.contains("closed = true"), "block: {}", block);
        assert!(!block.contains("ci-otro"));
    }

    #[test]
    fn find_block_does_not_match_belongs_to() {
        let text = r#"[[rect]]
id = "real"
belongs_to = "fake"
width = 1.0
"#;
        assert!(find_block(text, "fake").is_none());
        assert!(find_block(text, "real").is_some());
    }

    #[test]
    fn query_param_decodes_percent_encoding() {
        assert_eq!(query_param("id=ln%2D001", "id").as_deref(), Some("ln-001"));
        assert_eq!(query_param("a=1&id=tx+1", "id").as_deref(), Some("tx 1"));
        assert_eq!(query_param("a=1", "id"), None);
    }
}

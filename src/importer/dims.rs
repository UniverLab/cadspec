//! Dimension cleanup — drop the extension/dimension lines and label text
//! that `cadspec build` emits alongside each DIMENSION entity (the
//! re-created `[[dim]]` regenerates all of them).

use super::shapes::Shape;
use super::Imported;

/// Recover the perpendicular dimension offset from the insertion point.
pub(super) fn dim_offset(from: [f64; 2], to: [f64; 2], insertion: [f64; 2]) -> Option<f64> {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return None;
    }
    let (nx, ny) = (-dy / len, dx / len);
    let mid = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
    Some((insertion[0] - mid[0]) * nx + (insertion[1] - mid[1]) * ny)
}

/// Drop the extension/dimension lines and label text that `cadspec build`
/// emits alongside each DIMENSION entity for viewer compatibility; the
/// re-created `[[dim]]` regenerates all of them. Foreign DXFs are unaffected
/// (their dimension graphics live in blocks, not loose entities).
pub(super) fn remove_dim_companions(entities: &mut Vec<Imported>) {
    const TOL: f64 = 1e-6;
    let close = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < TOL && (a[1] - b[1]).abs() < TOL;

    let dims: Vec<([f64; 2], [f64; 2], f64)> = entities
        .iter()
        .filter_map(|e| match e.shape {
            Shape::Dim { from, to, offset } => Some((from, to, offset)),
            _ => None,
        })
        .collect();

    let mut keep = vec![true; entities.len()];
    for (from, to, offset) in dims {
        let dx = to[0] - from[0];
        let dy = to[1] - from[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            continue;
        }
        let (nx, ny) = (-dy / len, dx / len);
        let a = [from[0] + nx * offset, from[1] + ny * offset];
        let b = [to[0] + nx * offset, to[1] + ny * offset];
        let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];

        for (cf, ct) in [(from, a), (to, b), (a, b)] {
            mark_line_companion(entities, &mut keep, cf, ct, close);
        }
        mark_text_companion(entities, &mut keep, mid, nx, ny, close);
    }

    let mut it = keep.into_iter();
    entities.retain(|_| it.next().unwrap_or(true));
}

fn mark_line_companion(
    entities: &[Imported],
    keep: &mut [bool],
    from: [f64; 2],
    to: [f64; 2],
    close: impl Fn([f64; 2], [f64; 2]) -> bool,
) {
    if let Some(i) = (0..entities.len()).find(|&i| {
        keep[i]
            && matches!(entities[i].shape, Shape::Line { from: lf, to: lt }
                if (close(lf, from) && close(lt, to)) || (close(lf, to) && close(lt, from)))
    }) {
        keep[i] = false;
    }
}

fn mark_text_companion(
    entities: &[Imported],
    keep: &mut [bool],
    mid: [f64; 2],
    nx: f64,
    ny: f64,
    close: impl Fn([f64; 2], [f64; 2]) -> bool,
) {
    if let Some(i) = (0..entities.len()).find(|&i| {
        keep[i]
            && matches!(entities[i].shape, Shape::Text { position, size, .. }
                if close(position, [mid[0] + nx * size * 0.5, mid[1] + ny * size * 0.5]))
    }) {
        keep[i] = false;
    }
}

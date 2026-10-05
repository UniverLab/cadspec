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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::shapes::StyleAttrs;

    fn dim(from: [f64; 2], to: [f64; 2], offset: f64) -> Imported {
        Imported {
            shape: Shape::Dim { from, to, offset },
            style: StyleAttrs::default(),
        }
    }

    fn line(from: [f64; 2], to: [f64; 2]) -> Imported {
        Imported {
            shape: Shape::Line { from, to },
            style: StyleAttrs::default(),
        }
    }

    #[test]
    fn dim_offset_projects_the_insertion_onto_the_meridian() {
        // dx=3, dy=3, len=sqrt(18), normal=(-√2/2, √2/2), mid=(2.5, 3.5):
        // (7-2.5)*nx + (3-3.5)*ny = -3.535533905932738
        let value = dim_offset([1.0, 2.0], [4.0, 5.0], [7.0, 3.0]).unwrap();
        assert!(
            (value - (-3.535533905932738)).abs() < 1e-12,
            "unexpected offset {value}"
        );
        // Plain horizontal case: normal (0, 1), mid (1, 0) → offset 1.
        assert_eq!(dim_offset([0.0, 0.0], [2.0, 0.0], [0.0, 1.0]), Some(1.0));
    }

    #[test]
    fn dim_offset_rejects_a_zero_length_dimension() {
        assert_eq!(dim_offset([0.0, 0.0], [0.0, 0.0], [1.0, 1.0]), None);
    }

    #[test]
    fn dim_offset_accepts_a_dimension_of_exactly_one_nanometre() {
        // len computes to exactly 1e-9 here: still a measurable dimension, so
        // the length guard must not swallow it.
        let value = dim_offset([0.0, 0.0], [1e-9, 0.0], [0.0, 1.0]);
        assert_eq!(value, Some(1.0));
    }

    #[test]
    fn remove_dim_companions_drops_the_generated_graphics() {
        // Dim from (1,2) to (4,6) offset 1: normal = (-0.8, 0.6),
        // a = (0.2, 2.6), b = (3.2, 6.6).
        let mut entities = vec![
            dim([1.0, 2.0], [4.0, 6.0], 1.0),
            line([1.0, 2.0], [0.2, 2.6]),
            line([4.0, 6.0], [3.2, 6.6]),
            line([0.2, 2.6], [3.2, 6.6]),
        ];
        remove_dim_companions(&mut entities);
        assert_eq!(entities.len(), 1, "only the [[dim]] entity must remain");
        assert!(matches!(entities[0].shape, Shape::Dim { .. }));
    }

    #[test]
    fn remove_dim_companions_keeps_lines_outside_the_tolerance_on_x() {
        // Dim (0,0)→(2,0) offset 1 has companions at (0,0)→(0,1); this line's
        // start is exactly TOL (1e-6) away on x, so it must NOT be swallowed.
        let mut entities = vec![
            dim([0.0, 0.0], [2.0, 0.0], 1.0),
            line([1e-6, 0.0], [0.0, 1.0]),
        ];
        remove_dim_companions(&mut entities);
        assert_eq!(entities.len(), 2, "a line 1e-6 off on x is not a companion");
    }

    #[test]
    fn remove_dim_companions_keeps_lines_outside_the_tolerance_on_y() {
        let mut entities = vec![
            dim([0.0, 0.0], [2.0, 0.0], 1.0),
            line([0.0, 1e-6], [0.0, 1.0]),
        ];
        remove_dim_companions(&mut entities);
        assert_eq!(entities.len(), 2, "a line 1e-6 off on y is not a companion");
    }

    #[test]
    fn remove_dim_companions_cleans_a_dimension_of_exactly_one_nanometre() {
        // len == 1e-9 exactly: still measurable, so its graphics (a=(0,1),
        // b=(1e-9,1)) must be removed like any other dimension's.
        let mut entities = vec![
            dim([0.0, 0.0], [1e-9, 0.0], 1.0),
            line([0.0, 0.0], [0.0, 1.0]),
            line([1e-9, 0.0], [1e-9, 1.0]),
            line([0.0, 1.0], [1e-9, 1.0]),
        ];
        remove_dim_companions(&mut entities);
        assert_eq!(
            entities.len(),
            1,
            "the 1nm dimension's companion graphics must be removed"
        );
    }
}

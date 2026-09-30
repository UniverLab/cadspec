//! SOLID fill fusing — adjacent DXF SOLIDs that tile a region are stitched
//! back into a single `[[fill]]` polygon.

use super::shapes::Shape;
use super::Imported;
use std::collections::BTreeMap;

/// Recover the polygon ring of a DXF SOLID entity. SOLID vertices render in
/// the order first → second → fourth → third; adjacent duplicates (the writer
/// sets the fourth corner equal to the third for triangles) are collapsed.
pub(super) fn solid_ring(e: &dxf::entities::Solid) -> Vec<[f64; 2]> {
    let raw = [
        [e.first_corner.x, e.first_corner.y],
        [e.second_corner.x, e.second_corner.y],
        [e.fourth_corner.x, e.fourth_corner.y],
        [e.third_corner.x, e.third_corner.y],
    ];
    let mut ring: Vec<[f64; 2]> = Vec::with_capacity(4);
    for p in raw {
        if ring.last().is_none_or(|q| !pts_eq(*q, p)) {
            ring.push(p);
        }
    }
    // Drop a closing duplicate if the last equals the first.
    if ring.len() > 1 && pts_eq(ring[0], ring[ring.len() - 1]) {
        ring.pop();
    }
    ring
}

/// Quantized key for a point so that geometrically equal vertices from
/// different SOLID entities hash together despite float round-trip noise.
type PtKey = (i64, i64);

fn pt_key(p: [f64; 2]) -> PtKey {
    ((p[0] * 1e6).round() as i64, (p[1] * 1e6).round() as i64)
}

fn pts_eq(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
}

/// Undirected edge key with a canonical (min, max) endpoint order.
fn edge_key(a: PtKey, b: PtKey) -> (PtKey, PtKey) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Fuse adjacent SOLID-derived fills into single `[[fill]]` regions.
///
/// `cadspec build` fan-triangulates a solid fill into several SOLID entities;
/// importing each one separately would inflate the entity count and lose the
/// `[[fill]]` semantics. Fills that share an edge belong to the same region:
/// group them, drop the shared (internal) edges, and stitch the remaining
/// boundary edges back into one polygon. Foreign SOLIDs that don't tile a
/// region are left as individual fills.
pub(super) fn fuse_solids(entities: &mut Vec<Imported>) {
    let fill_positions: Vec<usize> = entities
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.shape, Shape::Fill { .. }))
        .map(|(i, _)| i)
        .collect();
    if fill_positions.len() < 2 {
        return; // nothing to merge
    }

    let ring_of = |pos: usize| -> &[[f64; 2]] {
        match &entities[pos].shape {
            Shape::Fill { points } => points,
            _ => unreachable!("filtered to fills"),
        }
    };

    // Union-find over fills, joined when they share an edge.
    let mut parent: Vec<usize> = (0..fill_positions.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        // Bounded walk: the forest has parent.len() nodes and no cycles, so the
        // root is always reached within parent.len() steps (same result as a
        // `while parent[x] != x` walk, whose mutated operator could otherwise
        // spin forever).
        for _ in 0..parent.len() {
            if parent[x] == x {
                break;
            }
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }

    let mut edge_owner: std::collections::HashMap<(PtKey, PtKey), usize> =
        std::collections::HashMap::new();
    for (idx, &pos) in fill_positions.iter().enumerate() {
        for (a, b) in ring_edges(ring_of(pos)) {
            let key = edge_key(pt_key(a), pt_key(b));
            if let Some(&other) = edge_owner.get(&key) {
                let (ra, rb) = (find(&mut parent, idx), find(&mut parent, other));
                parent[ra] = rb;
            } else {
                edge_owner.insert(key, idx);
            }
        }
    }

    // Group fill indices by component root.
    let mut components: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for idx in 0..fill_positions.len() {
        let root = find(&mut parent, idx);
        components.entry(root).or_default().push(idx);
    }

    // Build replacement fills and track which original positions to drop.
    let mut fused: Vec<Imported> = Vec::new();
    let mut drop: Vec<bool> = vec![false; entities.len()];
    for members in components.values() {
        for &idx in members {
            drop[fill_positions[idx]] = true;
        }
        let style_pos = fill_positions[members[0]];
        let style = entities[style_pos].style.clone();

        if members.len() == 1 {
            // A lone fill: keep its own ring.
            let points = ring_of(fill_positions[members[0]]).to_vec();
            fused.push(Imported {
                shape: Shape::Fill { points },
                style,
            });
            continue;
        }

        let rings: Vec<&[[f64; 2]]> = members
            .iter()
            .map(|&idx| ring_of(fill_positions[idx]))
            .collect();
        match merge_rings(&rings) {
            Some(points) => fused.push(Imported {
                shape: Shape::Fill { points },
                style,
            }),
            None => {
                // Couldn't stitch a clean boundary: keep each fill as-is.
                for &idx in members {
                    fused.push(Imported {
                        shape: Shape::Fill {
                            points: ring_of(fill_positions[idx]).to_vec(),
                        },
                        style: entities[fill_positions[idx]].style.clone(),
                    });
                }
            }
        }
    }

    let mut it = drop.into_iter();
    entities.retain(|_| !it.next().unwrap_or(false));
    entities.extend(fused);
}

/// Iterate the closed ring's undirected edges as consecutive point pairs.
fn ring_edges(ring: &[[f64; 2]]) -> Vec<([f64; 2], [f64; 2])> {
    let n = ring.len();
    (0..n).map(|i| (ring[i], ring[(i + 1) % n])).collect()
}

/// Merge a set of triangle/quad rings that tile one region into a single
/// boundary polygon: edges shared by two rings are interior and dropped; the
/// remaining boundary edges are chained into one ring. Returns `None` if the
/// boundary is not a single simple loop.
fn merge_rings(rings: &[&[[f64; 2]]]) -> Option<Vec<[f64; 2]>> {
    let mut edge_count: std::collections::HashMap<(PtKey, PtKey), usize> =
        std::collections::HashMap::new();
    let mut coord: std::collections::HashMap<PtKey, [f64; 2]> = std::collections::HashMap::new();
    for ring in rings {
        for (a, b) in ring_edges(ring) {
            coord.entry(pt_key(a)).or_insert(a);
            coord.entry(pt_key(b)).or_insert(b);
            *edge_count
                .entry(edge_key(pt_key(a), pt_key(b)))
                .or_insert(0) += 1;
        }
    }

    // Boundary edges appear exactly once.
    let boundary: Vec<(PtKey, PtKey)> = edge_count
        .iter()
        .filter(|(_, &c)| c == 1)
        .map(|(&e, _)| e)
        .collect();
    if boundary.len() < 3 {
        return None;
    }

    // Adjacency; a clean loop has every vertex at degree 2.
    let mut adj: std::collections::HashMap<PtKey, Vec<PtKey>> = std::collections::HashMap::new();
    for (a, b) in &boundary {
        adj.entry(*a).or_default().push(*b);
        adj.entry(*b).or_default().push(*a);
    }
    if adj.values().any(|v| v.len() != 2) {
        return None;
    }

    // Walk the loop from a deterministic start.
    let start = *adj.keys().min()?;
    let mut ring_keys = vec![start];
    let mut prev = start;
    let mut cur = adj[&start][0];
    while cur != start {
        ring_keys.push(cur);
        let nbrs = &adj[&cur];
        let next = if nbrs[0] == prev { nbrs[1] } else { nbrs[0] };
        prev = cur;
        cur = next;
        if ring_keys.len() > boundary.len() {
            return None; // did not close cleanly
        }
    }
    if ring_keys.len() != boundary.len() {
        return None; // disconnected boundary (e.g. a hole)
    }

    Some(ring_keys.iter().map(|k| coord[k]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::shapes::StyleAttrs;

    fn fill(points: Vec<[f64; 2]>) -> Imported {
        Imported {
            shape: Shape::Fill { points },
            style: StyleAttrs::default(),
        }
    }

    fn ring_of(e: &Imported) -> &[[f64; 2]] {
        match &e.shape {
            Shape::Fill { points } => points,
            _ => panic!("expected fill"),
        }
    }

    #[test]
    fn merge_rings_stitches_fan_triangles_into_rectangle() {
        // Fan triangulation of rect [0,0]-[2,2] from vertex [0,0].
        let t1: &[[f64; 2]] = &[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]];
        let t2: &[[f64; 2]] = &[[0.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        let ring = merge_rings(&[t1, t2]).expect("should stitch a clean loop");
        assert_eq!(ring.len(), 4, "shared diagonal edge is dropped");
        // All four rectangle corners present regardless of winding/start.
        for corner in [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]] {
            assert!(
                ring.iter().any(|p| pts_eq(*p, corner)),
                "missing corner {corner:?}"
            );
        }
    }

    #[test]
    fn fuse_solids_merges_adjacent_and_keeps_disjoint_separate() {
        let mut entities = vec![
            // Region A: two triangles tiling rect [0,0]-[2,2].
            fill(vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]]),
            fill(vec![[0.0, 0.0], [2.0, 2.0], [0.0, 2.0]]),
            // Region B: a lone triangle far away.
            fill(vec![[10.0, 10.0], [11.0, 10.0], [10.5, 11.0]]),
        ];
        fuse_solids(&mut entities);
        let fills: Vec<&Imported> = entities
            .iter()
            .filter(|e| matches!(e.shape, Shape::Fill { .. }))
            .collect();
        assert_eq!(fills.len(), 2, "adjacent pair fuses, disjoint stays");
        let mut lens: Vec<usize> = fills.iter().map(|e| ring_of(e).len()).collect();
        lens.sort_unstable();
        assert_eq!(lens, vec![3, 4], "one rectangle (4) and one triangle (3)");
    }

    #[test]
    fn fuse_solids_leaves_single_fill_untouched() {
        let mut entities = vec![fill(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]])];
        fuse_solids(&mut entities);
        assert_eq!(entities.len(), 1);
        assert_eq!(ring_of(&entities[0]).len(), 3);
    }

    // ── solid_ring ────────────────────────────────────────────────────────

    fn solid(
        first: (f64, f64),
        second: (f64, f64),
        third: (f64, f64),
        fourth: (f64, f64),
    ) -> dxf::entities::Solid {
        dxf::entities::Solid::new(
            dxf::Point::new(first.0, first.1, 0.0),
            dxf::Point::new(second.0, second.1, 0.0),
            dxf::Point::new(third.0, third.1, 0.0),
            dxf::Point::new(fourth.0, fourth.1, 0.0),
        )
    }

    #[test]
    fn solid_ring_recovers_the_four_corners_in_render_order() {
        // Render order is first → second → fourth → third; every corner is
        // distinct, so all four survive in exactly that order.
        let e = solid((0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0));
        assert_eq!(
            solid_ring(&e),
            vec![[0.0, 0.0], [2.0, 0.0], [0.0, 2.0], [2.0, 2.0]]
        );
    }

    #[test]
    fn solid_ring_collapses_the_repeated_triangle_corner() {
        // The writer emits triangles with fourth == third, which must collapse
        // to the three real corners.
        let e = solid((0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (2.0, 2.0));
        assert_eq!(solid_ring(&e), vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]]);
    }

    #[test]
    fn solid_ring_drops_a_closing_duplicate_of_the_first_corner() {
        // Raw order is [first, second, fourth, third]; a third corner equal to
        // the first repeats the ring's start and must be popped by the closing
        // rule. The neighbours stay pairwise distinct so only that rule
        // applies (no in-loop collapse can hide the popped point).
        let e = solid((0.0, 0.0), (3.0, 0.0), (0.0, 0.0), (1.0, 4.0));
        assert_eq!(solid_ring(&e), vec![[0.0, 0.0], [3.0, 0.0], [1.0, 4.0]]);
    }

    #[test]
    fn solid_ring_keeps_a_single_point_ring_from_collapsing() {
        // All four raw points equal: the ring is one point, and the closing
        // duplicate pop must not fire (that would empty the ring).
        let e = solid((1.0, 1.0), (1.0, 1.0), (1.0, 1.0), (1.0, 1.0));
        assert_eq!(solid_ring(&e), vec![[1.0, 1.0]]);
    }

    // ── pt_key / pts_eq / edge_key ────────────────────────────────────────

    #[test]
    fn pt_key_quantizes_coordinates_to_micro_units() {
        assert_eq!(pt_key([1.0, 2.0]), (1_000_000, 2_000_000));
        assert_eq!(pt_key([3.5, -4.25]), (3_500_000, -4_250_000));
    }

    #[test]
    fn pts_eq_treats_points_within_the_tolerance_as_equal() {
        assert!(pts_eq([0.0, 0.0], [0.0, 0.0]));
        assert!(pts_eq([1.5, 2.5], [1.5, 2.5]));
        // 1e-7 is an order of magnitude below the 1e-6 tolerance.
        assert!(pts_eq([1.5, 2.5], [1.5 + 1e-7, 2.5]));
    }

    #[test]
    fn pts_eq_rejects_points_beyond_the_tolerance() {
        // Differs only in y, so both axis checks must hold: a `&&` relaxed to
        // `||`, or an unconditional `true`, would call these equal.
        assert!(!pts_eq([0.0, 0.0], [0.0, 1.0]));
        assert!(!pts_eq([1.0, 1.0], [2.0, 1.0]));
    }

    #[test]
    fn pts_eq_rejects_a_difference_exactly_at_the_tolerance() {
        // 0.0 - 1e-6 is exact in f64, so the compared magnitude is exactly the
        // 1e-6 tolerance and must stay strictly below it.
        assert!(!pts_eq([0.0, 0.0], [0.0, 1e-6]));
    }

    #[test]
    fn edge_key_canonicalizes_the_endpoint_order() {
        assert_eq!(edge_key((1, 2), (3, 4)), ((1, 2), (3, 4)));
        assert_eq!(edge_key((3, 4), (1, 2)), ((1, 2), (3, 4)));
    }

    // ── merge_rings / fuse_solids ─────────────────────────────────────────

    #[test]
    fn merge_rings_stitches_a_single_triangle_ring() {
        // Three boundary edges is the smallest acceptable loop.
        let t: &[[f64; 2]] = &[[0.0, 0.0], [2.0, 0.0], [0.0, 2.0]];
        let ring = merge_rings(&[t]).expect("a lone triangle is a clean loop");
        assert_eq!(ring.len(), 3);
    }

    #[test]
    fn fuse_solids_merges_exactly_two_adjacent_fills() {
        let mut entities = vec![
            fill(vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]]),
            fill(vec![[0.0, 0.0], [2.0, 2.0], [0.0, 2.0]]),
        ];
        fuse_solids(&mut entities);
        let fills: Vec<&Imported> = entities
            .iter()
            .filter(|e| matches!(e.shape, Shape::Fill { .. }))
            .collect();
        assert_eq!(
            fills.len(),
            1,
            "exactly two adjacent fills must fuse into one region"
        );
        assert_eq!(
            ring_of(fills[0]).len(),
            4,
            "the shared diagonal is dropped, leaving the 4-gon"
        );
    }

    #[test]
    fn fuse_solids_fuses_each_adjacent_pair_independently() {
        let mut entities = vec![
            // Region A: two triangles tiling rect [0,0]-[2,2].
            fill(vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]]),
            fill(vec![[0.0, 0.0], [2.0, 2.0], [0.0, 2.0]]),
            // Region B: two triangles tiling rect [10,10]-[12,12].
            fill(vec![[10.0, 10.0], [12.0, 10.0], [12.0, 12.0]]),
            fill(vec![[10.0, 10.0], [12.0, 12.0], [10.0, 12.0]]),
        ];
        fuse_solids(&mut entities);
        let fills: Vec<&Imported> = entities
            .iter()
            .filter(|e| matches!(e.shape, Shape::Fill { .. }))
            .collect();
        assert_eq!(
            fills.len(),
            2,
            "each adjacent pair fuses on its own; the two regions stay apart"
        );
        let mut lens: Vec<usize> = fills.iter().map(|e| ring_of(e).len()).collect();
        lens.sort_unstable();
        assert_eq!(lens, vec![4, 4], "both fused regions are 4-gons");
    }
}

//! Seeded generation of one act's map.

use std::collections::{BTreeMap, BTreeSet};

use crate::balance::{MapBalance, MapRowBalance};
use crate::duel::DuelRng;

use super::state::{MapNode, NodeId, NodeKind, RunMap};

const KINDS: [NodeKind; 6] = [
    NodeKind::Duel,
    NodeKind::Elite,
    NodeKind::Rest,
    NodeKind::Shop,
    NodeKind::Draft,
    NodeKind::Treasure,
];

fn weight(row: &MapRowBalance, kind: NodeKind) -> u32 {
    match kind {
        NodeKind::Duel => row.duel,
        NodeKind::Elite => row.elite,
        NodeKind::Rest => row.rest,
        NodeKind::Shop => row.shop,
        NodeKind::Draft => row.draft,
        NodeKind::Treasure => row.treasure,
        NodeKind::Boss => 0,
    }
}

/// Picks a node kind by the row's weights, leaving `except` out.
fn pick_kind(rng: &mut DuelRng, row: &MapRowBalance, except: Option<NodeKind>) -> NodeKind {
    let allowed = |kind: &NodeKind| Some(*kind) != except && weight(row, *kind) > 0;
    let total: u32 = KINDS
        .iter()
        .filter(|kind| allowed(kind))
        .map(|&kind| weight(row, kind))
        .sum();
    let mut roll = rng.below(total);
    for kind in KINDS.into_iter().filter(allowed) {
        if roll < weight(row, kind) {
            return kind;
        }
        roll -= weight(row, kind);
    }
    // Only reached if nothing is allowed; balance validation gives every
    // row some weight.
    NodeKind::Duel
}

/// Builds a map: `columns` paths start in the first row, one per column, and
/// each wanders at most one column sideways per row without crossing
/// another. The cells the paths visit are the nodes, and the boss follows
/// the last row.
///
/// Each node's kind is drawn from its row's weights. A row with more than
/// one node and more than one possible kind always shows at least two
/// kinds, and a row with a single node is never an elite, so the map
/// always has a way past each row of elites.
pub fn generate_map(rng: &mut DuelRng, balance: &MapBalance) -> RunMap {
    let columns = balance.columns.max(1);
    let rows = balance.rows.len();

    // The cells in use, and the edges between consecutive rows.
    let mut cells: Vec<BTreeSet<u8>> = vec![BTreeSet::new(); rows];
    let mut edges: Vec<BTreeSet<(u8, u8)>> = vec![BTreeSet::new(); rows.saturating_sub(1)];
    for start in 0..columns {
        let mut column = start;
        for row in 0..rows {
            cells[row].insert(column);
            if row + 1 == rows {
                break;
            }
            let mut next = match rng.below(3) {
                0 => column.saturating_sub(1),
                1 => column,
                _ => (column + 1).min(columns - 1),
            };
            // Two edges cross if they swap columns; go straight instead.
            if next != column && edges[row].contains(&(next, column)) {
                next = column;
            }
            edges[row].insert((column, next));
            column = next;
        }
    }

    let mut ids: BTreeMap<(usize, u8), NodeId> = BTreeMap::new();
    let mut nodes = Vec::new();
    for (row, used) in cells.iter().enumerate() {
        let row_balance = &balance.rows[row];
        let mut kinds: Vec<NodeKind> = used
            .iter()
            .map(|_| pick_kind(rng, row_balance, None))
            .collect();
        let possible = KINDS
            .iter()
            .filter(|&&kind| weight(row_balance, kind) > 0)
            .count();
        let all_same = kinds.iter().all(|kind| *kind == kinds[0]);
        if kinds.len() > 1 && possible > 1 && all_same {
            let last = kinds.len() - 1;
            kinds[last] = pick_kind(rng, row_balance, Some(kinds[0]));
        }
        // Where every path meets in one node, it is never an elite.
        if kinds.len() == 1 && kinds[0] == NodeKind::Elite && possible > 1 {
            kinds[0] = pick_kind(rng, row_balance, Some(NodeKind::Elite));
        }
        for (&column, kind) in used.iter().zip(kinds) {
            let id = NodeId(u32::try_from(nodes.len()).unwrap_or(u32::MAX));
            ids.insert((row, column), id);
            nodes.push(MapNode {
                id,
                row: u8::try_from(row).unwrap_or(u8::MAX),
                column,
                kind,
                next: Vec::new(),
            });
        }
    }
    let boss = NodeId(u32::try_from(nodes.len()).unwrap_or(u32::MAX));
    for node in &mut nodes {
        let row = usize::from(node.row);
        if row + 1 == rows {
            node.next.push(boss);
            continue;
        }
        for &(from, to) in &edges[row] {
            if from == node.column
                && let Some(&id) = ids.get(&(row + 1, to))
            {
                node.next.push(id);
            }
        }
    }
    nodes.push(MapNode {
        id: boss,
        row: u8::try_from(rows).unwrap_or(u8::MAX),
        column: columns / 2,
        kind: NodeKind::Boss,
        next: Vec::new(),
    });
    RunMap {
        columns,
        rows: u8::try_from(rows + 1).unwrap_or(u8::MAX),
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data;

    fn map(seed: u64) -> RunMap {
        let balance = &data::embedded().unwrap().balance.run.map;
        generate_map(&mut DuelRng::from_seed(seed), balance)
    }

    #[test]
    fn every_map_is_a_layered_graph_ending_in_the_boss() {
        let balance = &data::embedded().unwrap().balance.run.map;
        let rows = balance.rows.len();
        for seed in 0..300 {
            let map = map(seed);
            assert_eq!(usize::from(map.rows), rows + 1);
            let boss = map.nodes.last().unwrap();
            assert_eq!(boss.kind, NodeKind::Boss);
            assert!(boss.next.is_empty());
            assert_eq!(
                map.nodes
                    .iter()
                    .filter(|node| node.kind == NodeKind::Boss)
                    .count(),
                1
            );

            let mut has_parent = BTreeSet::new();
            for (index, node) in map.nodes.iter().enumerate() {
                assert_eq!(node.id.0 as usize, index);
                assert!(node.column < map.columns);
                if node.kind != NodeKind::Boss {
                    assert!(!node.next.is_empty(), "seed {seed}: dead end at {node:?}");
                    assert!(weight(&balance.rows[usize::from(node.row)], node.kind) > 0);
                }
                for &next in &node.next {
                    let target = map.node(next).unwrap();
                    assert_eq!(target.row, node.row + 1, "seed {seed}");
                    if target.kind != NodeKind::Boss {
                        assert!(target.column.abs_diff(node.column) <= 1, "seed {seed}");
                    }
                    has_parent.insert(next);
                }
            }
            for node in &map.nodes {
                assert!(
                    node.row == 0 || has_parent.contains(&node.id),
                    "seed {seed}: {node:?} can't be reached"
                );
            }
            // The first row uses every column.
            let first_row = map.nodes.iter().filter(|node| node.row == 0).count();
            assert_eq!(first_row, usize::from(map.columns));
        }
    }

    #[test]
    fn paths_never_cross() {
        for seed in 0..300 {
            let map = map(seed);
            for a in &map.nodes {
                for b in &map.nodes {
                    if a.row != b.row || a.column >= b.column {
                        continue;
                    }
                    for &to_a in &a.next {
                        for &to_b in &b.next {
                            let (to_a, to_b) = (map.node(to_a).unwrap(), map.node(to_b).unwrap());
                            assert!(to_a.column <= to_b.column, "seed {seed}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_row_with_a_choice_of_kinds_never_forces_one() {
        let balance = &data::embedded().unwrap().balance.run.map;
        let mut elites = 0;
        for seed in 0..300 {
            let map = map(seed);
            for row in 0..balance.rows.len() {
                let kinds: BTreeSet<NodeKind> = map
                    .nodes
                    .iter()
                    .filter(|node| usize::from(node.row) == row)
                    .map(|node| node.kind)
                    .collect();
                let nodes = map
                    .nodes
                    .iter()
                    .filter(|node| usize::from(node.row) == row)
                    .count();
                let possible = KINDS
                    .iter()
                    .filter(|&&kind| weight(&balance.rows[row], kind) > 0)
                    .count();
                if nodes > 1 && possible > 1 {
                    assert!(kinds.len() > 1, "seed {seed} row {row}: only {kinds:?}");
                }
                if nodes == 1 {
                    assert!(!kinds.contains(&NodeKind::Elite), "seed {seed} row {row}");
                }
                elites += usize::from(kinds.contains(&NodeKind::Elite));
            }
        }
        assert!(elites > 100, "elites on only {elites} rows of 300 maps");
    }

    #[test]
    fn the_same_seed_gives_the_same_map() {
        assert_eq!(map(7), map(7));
        assert_ne!(map(7), map(8));
    }
}

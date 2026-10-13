//! Seeded generation of one act's map.

use std::collections::{BTreeMap, BTreeSet};

use crate::balance::{MapBalance, MapRowBalance};
use crate::duel::DuelRng;

use super::defs::EnemyId;
use super::state::{MapNode, NodeId, NodeKind, RunMap};

const KINDS: [NodeKind; 7] = [
    NodeKind::Duel,
    NodeKind::Elite,
    NodeKind::Rest,
    NodeKind::Shop,
    NodeKind::Draft,
    NodeKind::Treasure,
    NodeKind::Event,
];

fn weight(row: &MapRowBalance, kind: NodeKind) -> u32 {
    match kind {
        NodeKind::Duel => row.duel,
        NodeKind::Elite => row.elite,
        NodeKind::Rest => row.rest,
        NodeKind::Shop => row.shop,
        NodeKind::Draft => row.draft,
        NodeKind::Treasure => row.treasure,
        NodeKind::Event => row.event,
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
                enemy: None,
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
        enemy: None,
    });
    RunMap {
        columns,
        rows: u8::try_from(rows + 1).unwrap_or(u8::MAX),
        nodes,
    }
}

fn pick_from<'a>(rng: &mut DuelRng, pool: &[&'a EnemyId]) -> Option<&'a EnemyId> {
    if pool.is_empty() {
        return None;
    }
    let pick = rng.below(u32::try_from(pool.len()).unwrap_or(u32::MAX)) as usize;
    pool.get(pick).copied()
}

/// Says who waits at the boss node and at each elite node: one of `bosses`,
/// and for each elite node, in node order, one of `elites`.
///
/// An elite node gets an elite that is nowhere on the map yet while there
/// is one. When there are more elite nodes than elites, it gets one that is
/// on no elite node leading to it, so that no path meets the same elite
/// twice; failing that (the nodes leading to it hold every elite between
/// them), any.
pub fn place_enemies(rng: &mut DuelRng, map: &mut RunMap, bosses: &[EnemyId], elites: &[EnemyId]) {
    let boss = pick_from(rng, &bosses.iter().collect::<Vec<_>>()).cloned();
    // The nodes each node can be reached from. Every edge goes forward in
    // `nodes`, so one pass in order has a node's own set whole before it
    // is handed on.
    let mut behind: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); map.nodes.len()];
    for index in 0..map.nodes.len() {
        let mut passed = behind[index].clone();
        passed.insert(index);
        for next in &map.nodes[index].next {
            if let Some(set) = behind.get_mut(next.0 as usize) {
                set.extend(passed.iter().copied());
            }
        }
    }
    for (index, reached_from) in behind.iter().enumerate() {
        match map.nodes[index].kind {
            NodeKind::Boss => map.nodes[index].enemy = boss.clone(),
            NodeKind::Elite => {
                let placed = |only_behind: bool| -> Vec<&EnemyId> {
                    map.nodes
                        .iter()
                        .enumerate()
                        .filter(|(other, _)| !only_behind || reached_from.contains(other))
                        .filter_map(|(_, node)| node.enemy.as_ref())
                        .collect()
                };
                let without = |taken: Vec<&EnemyId>| -> Vec<&EnemyId> {
                    elites.iter().filter(|id| !taken.contains(id)).collect()
                };
                let mut pool = without(placed(false));
                if pool.is_empty() {
                    pool = without(placed(true));
                }
                if pool.is_empty() {
                    pool = elites.iter().collect();
                }
                let elite = pick_from(rng, &pool).cloned();
                map.nodes[index].enemy = elite;
            }
            _ => {}
        }
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

    fn ids(names: &[&str]) -> Vec<EnemyId> {
        names.iter().map(|name| EnemyId(name.to_string())).collect()
    }

    /// A map with its enemies placed from these pools.
    fn peopled(seed: u64, bosses: &[&str], elites: &[&str]) -> RunMap {
        let balance = &data::embedded().unwrap().balance.run.map;
        let mut rng = DuelRng::from_seed(seed);
        let mut map = generate_map(&mut rng, balance);
        place_enemies(&mut rng, &mut map, &ids(bosses), &ids(elites));
        map
    }

    /// The elite nodes of the map, each with the elite nodes on the paths
    /// that lead on from it.
    fn elites_ahead(map: &RunMap) -> Vec<(&MapNode, Vec<&MapNode>)> {
        let elite = |node: &&MapNode| node.kind == NodeKind::Elite;
        map.nodes
            .iter()
            .filter(elite)
            .map(|from| {
                let mut ahead = Vec::new();
                let mut todo = from.next.clone();
                while let Some(id) = todo.pop() {
                    let node = map.node(id).unwrap();
                    todo.extend(node.next.iter().copied());
                    if elite(&node) && !ahead.contains(&node) {
                        ahead.push(node);
                    }
                }
                (from, ahead)
            })
            .collect()
    }

    #[test]
    fn the_boss_and_every_elite_are_on_the_map_and_nobody_else() {
        for seed in 0..300 {
            let map = peopled(seed, &["tyrant"], &["a", "b", "c", "d"]);
            for node in &map.nodes {
                match node.kind {
                    NodeKind::Boss => assert_eq!(node.enemy, Some(EnemyId("tyrant".to_string()))),
                    NodeKind::Elite => assert!(node.enemy.is_some(), "seed {seed}"),
                    _ => assert_eq!(node.enemy, None, "seed {seed}: {node:?}"),
                }
            }
            // Placing them doesn't touch the map itself.
            let mut bare = map.clone();
            for node in &mut bare.nodes {
                node.enemy = None;
            }
            let balance = &data::embedded().unwrap().balance.run.map;
            assert_eq!(bare, generate_map(&mut DuelRng::from_seed(seed), balance));
        }
    }

    #[test]
    fn with_enough_elites_every_elite_node_of_a_map_is_a_different_one() {
        let mut most = 0;
        for seed in 0..300 {
            let map = peopled(seed, &["tyrant"], &["a", "b", "c", "d"]);
            let met: Vec<&EnemyId> = map
                .nodes
                .iter()
                .filter(|node| node.kind == NodeKind::Elite)
                .filter_map(|node| node.enemy.as_ref())
                .collect();
            let different: BTreeSet<&EnemyId> = met.iter().copied().collect();
            assert_eq!(different.len(), met.len(), "seed {seed}: {met:?}");
            most = most.max(met.len());
        }
        // The real pool of four is enough for any map: this is what the
        // test above leans on.
        assert!((2..=4).contains(&most), "{most} elite nodes on one map");
    }

    #[test]
    fn with_too_few_elites_a_path_meets_one_twice_only_when_it_must() {
        let mut shared = 0;
        let mut pairs = 0;
        for seed in 0..300 {
            let map = peopled(seed, &["tyrant"], &["a", "b"]);
            let nodes = map.nodes.iter().filter(|node| node.kind == NodeKind::Elite);
            let different: BTreeSet<&EnemyId> = nodes
                .clone()
                .filter_map(|node| node.enemy.as_ref())
                .collect();
            shared += usize::from(different.len() < nodes.count());
            let ahead = elites_ahead(&map);
            for (node, _) in &ahead {
                // The elites on the elite nodes that lead here.
                let behind: BTreeSet<&EnemyId> = ahead
                    .iter()
                    .filter(|(_, later)| later.contains(node))
                    .filter_map(|(from, _)| from.enemy.as_ref())
                    .collect();
                pairs += behind.len();
                // Only when both ways here have used up the pool between
                // them is there no helping it.
                if behind.len() < 2 {
                    let elite = node.enemy.as_ref().unwrap();
                    assert!(!behind.contains(elite), "seed {seed}: {node:?}");
                }
            }
        }
        // Some maps did have to use an elite twice, and some paths do hold
        // two elite nodes, or the test shows nothing.
        assert!(shared > 0 && pairs > 0, "{shared} maps, {pairs} pairs");
    }

    #[test]
    fn one_elite_or_none_is_placed_without_fuss() {
        for seed in 0..50 {
            let alone = peopled(seed, &["tyrant"], &["a"]);
            let nobody = peopled(seed, &[], &[]);
            for (one, none) in alone.nodes.iter().zip(&nobody.nodes) {
                assert_eq!(none.enemy, None);
                if one.kind == NodeKind::Elite {
                    assert_eq!(one.enemy, Some(EnemyId("a".to_string())));
                }
            }
        }
    }

    #[test]
    fn the_same_seed_places_the_same_enemies() {
        let elites = ["a", "b", "c", "d"];
        let bosses = ["tyrant", "other"];
        assert_eq!(peopled(7, &bosses, &elites), peopled(7, &bosses, &elites));
        let placed = |seed| -> Vec<Option<EnemyId>> {
            let map = peopled(seed, &bosses, &elites);
            map.nodes.into_iter().map(|node| node.enemy).collect()
        };
        assert!((0..20).any(|seed| placed(seed) != placed(seed + 100)));
        // With several bosses, each gets its turn.
        let met: BTreeSet<Option<EnemyId>> =
            (0..40).filter_map(|seed| placed(seed).pop()).collect();
        assert_eq!(met.len(), 2);
    }
}

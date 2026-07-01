//! Recherche de chemin A* sur la grille (déplacements orthogonaux).

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::map::Map;
use crate::types::Coord;

#[inline]
fn manhattan(a: Coord, b: Coord) -> u32 {
    let dx = (a.0 as i32 - b.0 as i32).unsigned_abs();
    let dy = (a.1 as i32 - b.1 as i32).unsigned_abs();
    dx + dy
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct Node {
    est: u32,
    cost: u32,
    pos: Coord,
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        // Tas-min : on inverse l'ordre pour que le plus petit `est` sorte en premier.
        other
            .est
            .cmp(&self.est)
            .then_with(|| other.cost.cmp(&self.cost))
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Calcule un chemin A* de `start` à `goal` en évitant les obstacles, puis
/// renvoie uniquement la **première case** à emprunter (`None` si l'on est déjà
/// arrivé ou si aucun chemin n'existe). Recalculer le premier pas à chaque tick
/// permet de réagir aux changements (ressources épuisées, robots mobiles).
pub fn next_step(map: &Map, start: Coord, goal: Coord) -> Option<Coord> {
    if start == goal {
        return None;
    }

    let mut open = BinaryHeap::new();
    let mut came: HashMap<Coord, Coord> = HashMap::new();
    let mut gscore: HashMap<Coord, u32> = HashMap::new();

    gscore.insert(start, 0);
    open.push(Node {
        est: manhattan(start, goal),
        cost: 0,
        pos: start,
    });

    while let Some(Node { cost, pos, .. }) = open.pop() {
        if pos == goal {
            // Remonte la chaîne de prédécesseurs jusqu'au premier pas après `start`.
            let mut cur = goal;
            loop {
                match came.get(&cur) {
                    Some(&prev) if prev == start => return Some(cur),
                    Some(&prev) => cur = prev,
                    None => return None,
                }
            }
        }
        if cost > *gscore.get(&pos).unwrap_or(&u32::MAX) {
            continue;
        }
        for n in map.walkable_neighbors(pos) {
            let ng = cost + 1;
            if ng < *gscore.get(&n).unwrap_or(&u32::MAX) {
                came.insert(n, pos);
                gscore.insert(n, ng);
                open.push(Node {
                    est: ng + manhattan(n, goal),
                    cost: ng,
                    pos: n,
                });
            }
        }
    }
    None
}

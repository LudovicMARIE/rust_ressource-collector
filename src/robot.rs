use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::time::Duration;

use rand::{Rng, RngExt};

use crate::path::next_step;
use crate::state::SharedState;
use crate::types::{Coord, ResourceKind, RobotKind, ToBase};

const CARRY_CAPACITY: u32 = 15;
const SENSE_RADIUS: i32 = 3;

pub fn run(id: usize, kind: RobotKind, shared: Arc<SharedState>, tx: Sender<ToBase>) {
    let base = shared.map.base;
    let mut pos = base;
    let mut rng = rand::rng();

    let mut reported_res: HashSet<Coord> = HashSet::new();
    let mut reported_obs: HashSet<Coord> = HashSet::new();

    let mut carry_energy: u32 = 0;
    let mut carry_crystal: u32 = 0;
    let mut returning = false;

    let mut prev_pos = pos;
    let mut stuck = 0u32;

    let mut visits: std::collections::HashMap<Coord, u32> = std::collections::HashMap::new();
    visits.insert(pos, 1);

    let base_delay = match kind {
        RobotKind::Scout => 85,
        RobotKind::Collector => 110,
    };

    while shared.running.load(Ordering::Relaxed) {
        sense(&shared, pos, &tx, &mut reported_res, &mut reported_obs);

        match kind {
            RobotKind::Scout => {
                pos = scout_step(&shared, pos, &mut visits, &mut rng);
            }
            RobotKind::Collector => {
                pos = collector_step(
                    &shared,
                    pos,
                    &tx,
                    &mut carry_energy,
                    &mut carry_crystal,
                    &mut returning,
                    &mut visits,
                    &mut rng,
                );
                if pos == prev_pos {
                    stuck += 1;
                    if stuck >= 6 {
                        pos = scout_step(&shared, pos, &mut visits, &mut rng);
                        stuck = 0;
                    }
                } else {
                    stuck = 0;
                }
                prev_pos = pos;
            }
        }

        {
            let mut robots = shared.robots.lock().unwrap();
            robots[id].pos = pos;
            robots[id].carrying = carry_energy + carry_crystal;
        }

        let jitter = rng.random_range(0..40);
        std::thread::sleep(Duration::from_millis(base_delay + jitter));
    }
}

fn sense(
    shared: &SharedState,
    pos: Coord,
    tx: &Sender<ToBase>,
    reported_res: &mut HashSet<Coord>,
    reported_obs: &mut HashSet<Coord>,
) {
    for dy in -SENSE_RADIUS..=SENSE_RADIUS {
        for dx in -SENSE_RADIUS..=SENSE_RADIUS {
            let nx = pos.0 as i32 + dx;
            let ny = pos.1 as i32 + dy;
            if !shared.map.in_bounds(nx, ny) {
                continue;
            }
            let cell = (nx as u16, ny as u16);

            if shared.map.is_obstacle(cell.0, cell.1) {
                if reported_obs.insert(cell) {
                    let _ = tx.send(ToBase::ObstacleFound { pos: cell });
                }
            } else if let Some(&(kind, _)) = shared.map.resources.get(&cell) {
                if reported_res.insert(cell) {
                    let _ = tx.send(ToBase::ResourceFound { pos: cell, kind });
                }
            }
        }
    }
}

fn occupied_cells(shared: &SharedState, me: Coord) -> HashSet<Coord> {
    let robots = shared.robots.lock().unwrap();
    robots.iter().map(|r| r.pos).filter(|&p| p != me).collect()
}

fn move_toward(shared: &SharedState, pos: Coord, goal: Coord) -> Coord {
    let occ = occupied_cells(shared, pos);
    if let Some(step) = next_step(&shared.map, pos, goal) {
        if !occ.contains(&step) {
            return step;
        }
        let cur_dist = manhattan(pos, goal);
        let mut best: Option<(u32, Coord)> = None;
        for n in shared.map.walkable_neighbors(pos) {
            if occ.contains(&n) {
                continue;
            }
            let d = manhattan(n, goal);
            if d <= cur_dist && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                best = Some((d, n));
            }
        }
        if let Some((_, n)) = best {
            return n;
        }
    }
    pos
}

#[inline]
fn manhattan(a: Coord, b: Coord) -> u32 {
    let dx = (a.0 as i32 - b.0 as i32).unsigned_abs();
    let dy = (a.1 as i32 - b.1 as i32).unsigned_abs();
    dx + dy
}

fn scout_step(
    shared: &SharedState,
    pos: Coord,
    visits: &mut std::collections::HashMap<Coord, u32>,
    rng: &mut impl Rng,
) -> Coord {
    let occ = occupied_cells(shared, pos);
    let candidates = shared.map.walkable_neighbors(pos);
    if candidates.is_empty() {
        return pos;
    }
    let mut pool: Vec<Coord> = candidates
        .iter()
        .copied()
        .filter(|c| !occ.contains(c))
        .collect();
    if pool.is_empty() {
        pool = candidates;
    }
    let min_v = pool
        .iter()
        .map(|c| *visits.get(c).unwrap_or(&0))
        .min()
        .unwrap_or(0);
    let best: Vec<Coord> = pool
        .into_iter()
        .filter(|c| *visits.get(c).unwrap_or(&0) == min_v)
        .collect();
    let next = best[rng.random_range(0..best.len())];
    *visits.entry(next).or_insert(0) += 1;
    next
}

fn collector_step(
    shared: &SharedState,
    pos: Coord,
    tx: &Sender<ToBase>,
    carry_energy: &mut u32,
    carry_crystal: &mut u32,
    returning: &mut bool,
    visits: &mut std::collections::HashMap<Coord, u32>,
    rng: &mut impl Rng,
) -> Coord {
    let base = shared.map.base;
    let carrying = *carry_energy + *carry_crystal;

    if carrying >= CARRY_CAPACITY {
        *returning = true;
    }
    if *returning {
        if pos == base {
            if carrying > 0 {
                let _ = tx.send(ToBase::Unload {
                    energy: *carry_energy,
                    crystal: *carry_crystal,
                });
            }
            *carry_energy = 0;
            *carry_crystal = 0;
            *returning = false;
            return pos;
        }
        return move_toward(shared, pos, base);
    }

    if let Some((target, kind)) = nearest_known_resource(shared, pos) {
        if pos == target {
            let available = {
                let ledger = shared.ledger.lock().unwrap();
                ledger.get(&target).map(|c| c.remaining).unwrap_or(0)
            };
            if available > 0 {
                let capacity_left = CARRY_CAPACITY - carrying;
                let to_collect = available.min(capacity_left).max(1);
                let _ = tx.send(ToBase::Collected { pos: target, kind, quantity: to_collect });
                match kind {
                    ResourceKind::Energy => *carry_energy += to_collect,
                    ResourceKind::Crystal => *carry_crystal += to_collect,
                }
            }
            return pos;
        }
        return move_toward(shared, pos, target);
    }

    if carrying > 0 {
        *returning = true;
        return move_toward(shared, pos, base);
    }
    scout_step(shared, pos, visits, rng)
}

fn nearest_known_resource(shared: &SharedState, pos: Coord) -> Option<(Coord, ResourceKind)> {
    let discovered = shared.discovered.read().unwrap();
    if discovered.is_empty() {
        return None;
    }
    let ledger = shared.ledger.lock().unwrap();
    discovered
        .iter()
        .filter_map(|&p| {
            ledger.get(&p).and_then(|cell| {
                if cell.remaining > 0 {
                    Some((p, cell.kind))
                } else {
                    None
                }
            })
        })
        .min_by_key(|&(p, _)| manhattan(pos, p))
}

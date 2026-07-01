//! Comportement des robots autonomes. Chaque robot s'exécute dans son propre
//! thread (entité indépendante) et communique avec la base par messages.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::time::Duration;

use rand::{Rng, RngExt};

use crate::path::next_step;
use crate::state::SharedState;
use crate::types::{Coord, ResourceKind, RobotKind, ToBase};

/// Capacité de transport d'un collecteur avant retour obligatoire à la base.
const CARRY_CAPACITY: u32 = 15;
/// Rayon de perception (en cases) autour du robot.
const SENSE_RADIUS: i32 = 3;

/// Point d'entrée d'un thread robot.
pub fn run(id: usize, kind: RobotKind, shared: Arc<SharedState>, tx: Sender<ToBase>) {
    let base = shared.map.base;
    let mut pos = base;
    let mut rng = rand::rng();

    // Connaissance locale déjà signalée (évite de spammer la base).
    let mut reported_res: HashSet<Coord> = HashSet::new();
    let mut reported_obs: HashSet<Coord> = HashSet::new();

    // État propre au collecteur.
    let mut carry_energy: u32 = 0;
    let mut carry_crystal: u32 = 0;
    let mut returning = false;

    // Détection de blocage (ex. deux collecteurs se gênant mutuellement).
    let mut prev_pos = pos;
    let mut stuck = 0u32;

    // Mémoire locale des cases visitées : sert à l'exploration par
    // « case la moins visitée », bien plus couvrante qu'une marche aléatoire.
    let mut visits: std::collections::HashMap<Coord, u32> = std::collections::HashMap::new();
    visits.insert(pos, 1);

    // Cadence légèrement aléatoire : les robots n'avancent pas en lock-step.
    let base_delay = match kind {
        RobotKind::Scout => 85,
        RobotKind::Collector => 110,
    };

    while shared.running.load(Ordering::Relaxed) {
        // 1. Perception locale : signaler ressources et obstacles inédits.
        sense(&shared, pos, &tx, &mut reported_res, &mut reported_obs);

        // 2. Décision de déplacement selon le type de robot.
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
                // Si le collecteur n'a pas bougé pendant plusieurs ticks alors
                // qu'il devrait, on le débloque par un pas aléatoire.
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

        // 3. Publier sa position pour le rendu.
        {
            let mut robots = shared.robots.lock().unwrap();
            robots[id].pos = pos;
            robots[id].carrying = carry_energy + carry_crystal;
        }

        let jitter = rng.random_range(0..40);
        std::thread::sleep(Duration::from_millis(base_delay + jitter));
    }
}

/// Perçoit les cases voisines et signale à la base ce qui est inédit.
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

/// Ensemble des cases occupées par les autres robots (évitement de collisions).
fn occupied_cells(shared: &SharedState, me: Coord) -> HashSet<Coord> {
    let robots = shared.robots.lock().unwrap();
    robots.iter().map(|r| r.pos).filter(|&p| p != me).collect()
}

/// Avance d'une case vers `goal` via A*, en évitant les cases occupées si possible.
fn move_toward(shared: &SharedState, pos: Coord, goal: Coord) -> Coord {
    let occ = occupied_cells(shared, pos);
    if let Some(step) = next_step(&shared.map, pos, goal) {
        if !occ.contains(&step) {
            return step;
        }
        // Case bloquée par un autre robot : tenter un contournement qui ne
        // nous éloigne pas davantage de l'objectif.
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
    pos // immobile ce tick
}

#[inline]
fn manhattan(a: Coord, b: Coord) -> u32 {
    let dx = (a.0 as i32 - b.0 as i32).unsigned_abs();
    let dy = (a.1 as i32 - b.1 as i32).unsigned_abs();
    dx + dy
}

/// Déplacement d'un éclaireur : se dirige vers la case voisine la moins
/// visitée (mémoire locale), en évitant obstacles et cases occupées. Cette
/// heuristique couvre la carte bien plus vite qu'une marche purement aléatoire.
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
    // Préfère les voisins libres ; si tous occupés, accepte les occupés.
    let mut pool: Vec<Coord> = candidates
        .iter()
        .copied()
        .filter(|c| !occ.contains(c))
        .collect();
    if pool.is_empty() {
        pool = candidates;
    }
    // Choisit le minimum de visites, départage aléatoirement.
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

/// Déplacement d'un collecteur : navigue vers la ressource connue la plus
/// proche, collecte une unité, puis revient décharger à la base.
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

    // Cargaison pleine -> retour à la base pour décharger.
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

    // Cherche la ressource découverte la plus proche encore disponible.
    if let Some((target, kind)) = nearest_known_resource(shared, pos) {
        if pos == target {
            // Sur la ressource : collecte tout ce que la cargaison permet en un seul passage.
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

    // Aucune ressource connue : exploration légère pour en trouver.
    if carrying > 0 {
        // On porte quelque chose mais plus de cible : on rentre.
        *returning = true;
        return move_toward(shared, pos, base);
    }
    scout_step(shared, pos, visits, rng)
}

/// Ressource découverte la plus proche (distance de Manhattan) ayant encore du stock.
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

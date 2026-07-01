//! Génération procédurale de la carte : obstacles via bruit de Perlin et
//! placement aléatoire des ressources.

use std::collections::{HashMap, HashSet, VecDeque};

use noise::{NoiseFn, Perlin};
use rand::{Rng, RngExt};

use crate::types::{Coord, ResourceKind};

/// Carte immuable après génération (partagée en lecture seule via `Arc`).
pub struct Map {
    pub width: u16,
    pub height: u16,
    /// Grille des obstacles, indexée par `y * width + x`.
    obstacles: Vec<bool>,
    /// Emplacements d'origine des ressources et leur quantité initiale.
    pub resources: HashMap<Coord, (ResourceKind, u32)>,
    /// Position de la base centrale.
    pub base: Coord,
}

impl Map {
    /// Génère une carte de dimensions données.
    ///
    /// * Les obstacles sont produits par un bruit de Perlin seuillé.
    /// * Les ressources (énergie / cristaux) sont dispersées aléatoirement sur
    ///   les cases libres, avec une quantité de 50 à 200 unités.
    /// * Une zone dégagée est garantie autour de la base.
    pub fn generate(width: u16, height: u16, resource_count: usize) -> Self {
        let mut rng = rand::rng();
        let seed: u32 = rng.next_u32();
        let perlin = Perlin::new(seed);

        let base = (width / 2, height / 2);
        let mut obstacles = vec![false; width as usize * height as usize];

        // --- Obstacles via bruit de Perlin ---
        const SCALE: f64 = 0.085;
        const THRESHOLD: f64 = 0.40;
        for y in 0..height {
            for x in 0..width {
                let v = perlin.get([x as f64 * SCALE, y as f64 * SCALE]);
                if v > THRESHOLD {
                    obstacles[y as usize * width as usize + x as usize] = true;
                }
            }
        }

        let mut map = Map {
            width,
            height,
            obstacles,
            resources: HashMap::new(),
            base,
        };

        // Dégage une zone 3x3 autour de la base : point de départ sûr.
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let nx = base.0 as i32 + dx;
                let ny = base.1 as i32 + dy;
                if map.in_bounds(nx, ny) {
                    let idx = ny as usize * width as usize + nx as usize;
                    map.obstacles[idx] = false;
                }
            }
        }

        // Ensemble des cases accessibles depuis la base (parcours en largeur).
        // Les ressources ne sont placées que là, garantissant qu'un chemin
        // existe toujours : aucun collecteur ne peut se retrouver bloqué.
        let reachable = map.reachable_from(base);

        // --- Placement des ressources sur des cases libres et accessibles ---
        let reachable_vec: Vec<Coord> = reachable.iter().copied().collect();
        let mut placed = 0;
        let mut attempts = 0;
        let max_attempts = resource_count * 50;
        while placed < resource_count && attempts < max_attempts && !reachable_vec.is_empty() {
            attempts += 1;
            let pos = reachable_vec[rng.random_range(0..reachable_vec.len())];
            if pos == base || map.resources.contains_key(&pos) {
                continue;
            }
            let kind = if rng.random_bool(0.5) {
                ResourceKind::Energy
            } else {
                ResourceKind::Crystal
            };
            let quantity = rng.random_range(50..=200);
            map.resources.insert(pos, (kind, quantity));
            placed += 1;
        }

        map
    }

    #[inline]
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32
    }

    /// Indique si la case `(x, y)` est un obstacle (hors-limites = obstacle).
    #[inline]
    pub fn is_obstacle(&self, x: u16, y: u16) -> bool {
        if x >= self.width || y >= self.height {
            return true;
        }
        self.obstacles[y as usize * self.width as usize + x as usize]
    }

    /// Voisins orthogonaux franchissables (dans les limites et sans obstacle).
    pub fn walkable_neighbors(&self, pos: Coord) -> Vec<Coord> {
        const DIRS: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        let mut out = Vec::with_capacity(4);
        for (dx, dy) in DIRS {
            let nx = pos.0 as i32 + dx;
            let ny = pos.1 as i32 + dy;
            if self.in_bounds(nx, ny) && !self.is_obstacle(nx as u16, ny as u16) {
                out.push((nx as u16, ny as u16));
            }
        }
        out
    }

    /// Renvoie l'ensemble des cases franchissables accessibles depuis `start`
    /// (parcours en largeur sur les voisins orthogonaux).
    pub fn reachable_from(&self, start: Coord) -> HashSet<Coord> {
        let mut seen = HashSet::new();
        let mut queue = VecDeque::new();
        seen.insert(start);
        queue.push_back(start);
        while let Some(p) = queue.pop_front() {
            for n in self.walkable_neighbors(p) {
                if seen.insert(n) {
                    queue.push_back(n);
                }
            }
        }
        seen
    }
}

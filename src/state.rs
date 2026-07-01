//! État partagé entre les threads.
//!
//! Plusieurs verrous indépendants à granularité fine plutôt qu'un verrou
//! global : chaque section critique est très courte et l'on ne détient jamais
//! deux verrous à la fois, ce qui évite tout interblocage et garde les
//! opérations non bloquantes les unes vis-à-vis des autres.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use crate::map::Map;
use crate::types::{Coord, ResourceKind, RobotView};

/// Quantité restante pour une ressource (registre faisant autorité, tenu par la base).
#[derive(Clone, Copy)]
pub struct ResourceCell {
    pub kind: ResourceKind,
    pub remaining: u32,
}

/// Totaux collectés et déchargés à la base.
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub energy: u32,
    pub crystal: u32,
}

/// État global accessible (en lecture seule pour la carte) par tous les threads.
pub struct SharedState {
    /// Carte immuable (lecture seule).
    pub map: Arc<Map>,
    /// Registre faisant autorité des quantités restantes — écrit uniquement par la base.
    pub ledger: Mutex<HashMap<Coord, ResourceCell>>,
    /// Connaissance des robots : ressources découvertes — écrit uniquement par la base.
    pub discovered: RwLock<HashSet<Coord>>,
    /// Connaissance des robots : obstacles découverts — écrit uniquement par la base.
    pub known_obstacles: RwLock<HashSet<Coord>>,
    /// Position et état de chaque robot — chaque robot écrit son propre emplacement.
    pub robots: Mutex<Vec<RobotView>>,
    /// Totaux collectés — écrit uniquement par la base.
    pub stats: Mutex<Stats>,
    /// Drapeau d'arrêt coopératif lu par tous les threads.
    pub running: AtomicBool,
}

impl SharedState {
    /// Construit l'état initial : le registre contient toutes les ressources de
    /// la carte (la base est le « hub de stockage »), mais les robots ne
    /// connaissent encore rien.
    pub fn new(map: Arc<Map>, robots: Vec<RobotView>) -> Arc<Self> {
        let mut ledger = HashMap::new();
        for (&pos, &(kind, qty)) in map.resources.iter() {
            ledger.insert(pos, ResourceCell { kind, remaining: qty });
        }
        Arc::new(SharedState {
            map,
            ledger: Mutex::new(ledger),
            discovered: RwLock::new(HashSet::new()),
            known_obstacles: RwLock::new(HashSet::new()),
            robots: Mutex::new(robots),
            stats: Mutex::new(Stats::default()),
            running: AtomicBool::new(true),
        })
    }
}

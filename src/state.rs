use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use crate::map::Map;
use crate::types::{Coord, ResourceKind, RobotView};

#[derive(Clone, Copy)]
pub struct ResourceCell {
    pub kind: ResourceKind,
    pub remaining: u32,
}

#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub energy: u32,
    pub crystal: u32,
}

pub struct SharedState {
    pub map: Arc<Map>,
    pub ledger: Mutex<HashMap<Coord, ResourceCell>>,
    pub discovered: RwLock<HashSet<Coord>>,
    pub known_obstacles: RwLock<HashSet<Coord>>,
    pub robots: Mutex<Vec<RobotView>>,
    pub stats: Mutex<Stats>,
    pub running: AtomicBool,
}

impl SharedState {
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

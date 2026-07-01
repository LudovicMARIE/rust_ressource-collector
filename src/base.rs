//! Thread de la base centrale : agrège les connaissances et fait autorité sur
//! l'état du monde. C'est le seul écrivain du registre, des ensembles de
//! découvertes et des totaux collectés.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use crate::state::SharedState;
use crate::types::ToBase;

/// Boucle principale de la base : consomme les messages des robots et met à
/// jour l'état partagé. Se termine lorsque `running` passe à `false` ou que
/// tous les émetteurs ont été détruits.
pub fn run(shared: Arc<SharedState>, rx: Receiver<ToBase>) {
    while shared.running.load(Ordering::Relaxed) {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(msg) => handle(&shared, msg),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn handle(shared: &SharedState, msg: ToBase) {
    match msg {
        ToBase::ResourceFound { pos, kind: _ } => {
            // N'enregistre la découverte que si la ressource existe encore.
            let exists = {
                let ledger = shared.ledger.lock().unwrap();
                ledger.get(&pos).map(|c| c.remaining > 0).unwrap_or(false)
            };
            if exists {
                shared.discovered.write().unwrap().insert(pos);
            }
        }
        ToBase::ObstacleFound { pos } => {
            shared.known_obstacles.write().unwrap().insert(pos);
        }
        ToBase::Collected { pos, kind: _, quantity } => {
            // Décrémente la quantité restante (collecte en lot).
            let mut depleted = false;
            {
                let mut ledger = shared.ledger.lock().unwrap();
                if let Some(cell) = ledger.get_mut(&pos) {
                    cell.remaining = cell.remaining.saturating_sub(quantity);
                    if cell.remaining == 0 {
                        depleted = true;
                    }
                }
            }
            // Une ressource épuisée sort de l'ensemble des cibles connues.
            if depleted {
                shared.discovered.write().unwrap().remove(&pos);
            }
        }
        ToBase::Unload { energy, crystal } => {
            let mut stats = shared.stats.lock().unwrap();
            stats.energy += energy;
            stats.crystal += crystal;
        }
    }
}

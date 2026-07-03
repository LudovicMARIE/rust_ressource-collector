use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use crate::state::SharedState;
use crate::types::ToBase;

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

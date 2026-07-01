//! Simulation de Collecte de Ressources
//! ------------------------------------
//! Des robots autonomes (éclaireurs et collecteurs) explorent une carte générée
//! procéduralement, partagent leurs découvertes via une base centrale et
//! rapportent les ressources. Rendu temps réel avec Ratatui ; chaque robot et
//! la base s'exécutent dans leur propre thread et communiquent par messages.

mod base;
mod map;
mod path;
mod robot;
mod state;
mod types;
mod ui;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::terminal as crossterm_terminal;

use crate::map::Map;
use crate::state::SharedState;
use crate::types::{RobotKind, RobotView};

// Paramètres de la simulation.
const RESOURCE_COUNT: usize = 40;
const NUM_SCOUTS: usize = 3;
const NUM_COLLECTORS: usize = 4;

// Largeur du panneau latéral + ses bordures (pour calculer la largeur de carte).
const SIDEBAR_COLS: u16 = 32; // 30 contenu + 2 bordures
const MAP_BORDER_ROWS: u16 = 2; // bordure haute + basse de la carte

fn main() -> std::io::Result<()> {
    // Détermine la taille de la carte d'après le terminal courant.
    let (term_cols, term_rows) = crossterm_terminal::size()?;
    let map_width = term_cols.saturating_sub(SIDEBAR_COLS).max(20);
    let map_height = term_rows.saturating_sub(MAP_BORDER_ROWS).max(10);

    // --- Génération de la carte et de l'état partagé ---
    let map = Arc::new(Map::generate(map_width, map_height, RESOURCE_COUNT));
    let base = map.base;

    let mut robot_views = Vec::new();
    let mut kinds = Vec::new();
    for _ in 0..NUM_SCOUTS {
        kinds.push(RobotKind::Scout);
        robot_views.push(RobotView {
            kind: RobotKind::Scout,
            pos: base,
            carrying: 0,
        });
    }
    for _ in 0..NUM_COLLECTORS {
        kinds.push(RobotKind::Collector);
        robot_views.push(RobotView {
            kind: RobotKind::Collector,
            pos: base,
            carrying: 0,
        });
    }

    let shared = SharedState::new(map, robot_views);

    // --- Canal de communication robots -> base ---
    let (tx, rx) = mpsc::channel();

    // --- Lancement des threads (base + un thread par robot) ---
    let mut handles = Vec::new();

    {
        let shared = Arc::clone(&shared);
        handles.push(thread::spawn(move || base::run(shared, rx)));
    }

    for (id, kind) in kinds.into_iter().enumerate() {
        let shared = Arc::clone(&shared);
        let tx = tx.clone();
        handles.push(thread::spawn(move || robot::run(id, kind, shared, tx)));
    }
    // On ne garde aucun émetteur dans le thread principal : ainsi, lorsque tous
    // les robots s'arrêtent, la base reçoit `Disconnected` et se termine.
    drop(tx);

    // --- Boucle de rendu / entrées (thread principal) ---
    let mut terminal = ratatui::init();
    let result = run_ui(&mut terminal, &shared);
    ratatui::restore();

    // Arrêt coopératif et attente de tous les threads.
    shared.running.store(false, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }

    // Bilan final dans le terminal restauré.
    let stats = *shared.stats.lock().unwrap();
    println!(
        "Simulation terminée — Énergie collectée : {}, Cristaux collectés : {}, Total : {}",
        stats.energy,
        stats.crystal,
        stats.energy + stats.crystal
    );

    result
}

fn run_ui(
    terminal: &mut ratatui::DefaultTerminal,
    shared: &Arc<SharedState>,
) -> std::io::Result<()> {
    while shared.running.load(Ordering::Relaxed) {
        terminal.draw(|frame| ui::render(frame, shared))?;

        // Interroge les entrées sans bloquer le rendu.
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => break,
                _ => {}
            }
        }
    }
    Ok(())
}

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

const RESOURCE_COUNT: usize = 40;
const NUM_SCOUTS: usize = 3;
const NUM_COLLECTORS: usize = 4;

const SIDEBAR_COLS: u16 = 32;
const MAP_BORDER_ROWS: u16 = 2;

fn main() -> std::io::Result<()> {
    let (term_cols, term_rows) = crossterm_terminal::size()?;
    let map_width = term_cols.saturating_sub(SIDEBAR_COLS).max(20);
    let map_height = term_rows.saturating_sub(MAP_BORDER_ROWS).max(10);

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

    let (tx, rx) = mpsc::channel();

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
    drop(tx);

    let mut terminal = ratatui::init();
    let result = run_ui(&mut terminal, &shared);
    ratatui::restore();

    shared.running.store(false, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }

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

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => break,
                _ => {}
            }
        }
    }
    Ok(())
}

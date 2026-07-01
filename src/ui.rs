//! Rendu de l'interface terminal avec Ratatui.

use std::sync::Arc;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::state::SharedState;
use crate::types::{ResourceKind, RobotKind};

// Codage couleur (conforme à la disposition visuelle demandée).
const C_OBSTACLE: Color = Color::LightCyan;
const C_ENERGY: Color = Color::Green;
const C_CRYSTAL: Color = Color::LightMagenta;
const C_BASE: Color = Color::LightGreen;
const C_SCOUT: Color = Color::Red;
const C_COLLECTOR: Color = Color::Magenta;

/// Dessine la frame complète : panneau latéral de statistiques + carte.
pub fn render(frame: &mut Frame, shared: &Arc<SharedState>) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(0)])
        .split(frame.area());

    render_sidebar(frame, chunks[0], shared);
    render_map(frame, chunks[1], shared);
}

fn render_sidebar(frame: &mut Frame, area: Rect, shared: &Arc<SharedState>) {
    let stats = *shared.stats.lock().unwrap();
    let discovered = shared.discovered.read().unwrap().len();
    let remaining: u32 = shared
        .ledger
        .lock()
        .unwrap()
        .values()
        .map(|c| c.remaining)
        .sum();

    let (scouts, collectors) = {
        let robots = shared.robots.lock().unwrap();
        let s = robots.iter().filter(|r| r.kind == RobotKind::Scout).count();
        let c = robots
            .iter()
            .filter(|r| r.kind == RobotKind::Collector)
            .count();
        (s, c)
    };

    let label = Style::default().add_modifier(Modifier::BOLD);
    let lines = vec![
        Line::from(Span::styled(
            "RESSOURCES COLLECTÉES",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("Énergie  : ", label),
            Span::styled(stats.energy.to_string(), Style::default().fg(C_ENERGY)),
        ]),
        Line::from(vec![
            Span::styled("Cristaux : ", label),
            Span::styled(stats.crystal.to_string(), Style::default().fg(C_CRYSTAL)),
        ]),
        Line::from(vec![
            Span::styled("Total    : ", label),
            Span::raw((stats.energy + stats.crystal).to_string()),
        ]),
        Line::from(""),
        Line::from(Span::styled("ÉTAT DU MONDE", label)),
        Line::from(format!("Découvertes : {discovered}")),
        Line::from(format!("Restant carte: {remaining}")),
        Line::from(format!("Éclaireurs  : {scouts}")),
        Line::from(format!("Collecteurs : {collectors}")),
        Line::from(""),
        Line::from(Span::styled("LÉGENDE", label)),
        Line::from(vec![Span::styled("#", Style::default().fg(C_BASE)), Span::raw(" base")]),
        Line::from(vec![Span::styled("x", Style::default().fg(C_SCOUT)), Span::raw(" éclaireur")]),
        Line::from(vec![Span::styled("o", Style::default().fg(C_COLLECTOR)), Span::raw(" collecteur")]),
        Line::from(vec![Span::styled("E", Style::default().fg(C_ENERGY)), Span::raw(" énergie")]),
        Line::from(vec![Span::styled("C", Style::default().fg(C_CRYSTAL)), Span::raw(" cristal")]),
        Line::from(vec![Span::styled("O", Style::default().fg(C_OBSTACLE)), Span::raw(" obstacle")]),
        Line::from(""),
        Line::from(Span::styled(
            "Appuyez sur une touche pour quitter",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Simulation de Collecte ");
    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

fn render_map(frame: &mut Frame, area: Rect, shared: &Arc<SharedState>) {
    let block = Block::default().borders(Borders::ALL).title(" Carte ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let map = &shared.map;
    let view_w = inner.width.min(map.width);
    let view_h = inner.height.min(map.height);
    if view_w == 0 || view_h == 0 {
        return;
    }

    // Instantanés courts des états mutables.
    let ledger = shared.ledger.lock().unwrap().clone();
    let robot_views: Vec<_> = shared.robots.lock().unwrap().clone();

    let mut lines: Vec<Line> = Vec::with_capacity(view_h as usize);
    for y in 0..view_h {
        let mut spans: Vec<Span> = Vec::with_capacity(view_w as usize);
        for x in 0..view_w {
            let cell = (x, y);
            let (glyph, color) = cell_glyph(map, &ledger, &robot_views, cell);
            spans.push(Span::styled(
                glyph.to_string(),
                Style::default().fg(color),
            ));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Détermine le caractère et la couleur d'une case selon une priorité :
/// robot > ressource > base > obstacle > vide.
fn cell_glyph(
    map: &crate::map::Map,
    ledger: &std::collections::HashMap<crate::types::Coord, crate::state::ResourceCell>,
    robots: &[crate::types::RobotView],
    cell: crate::types::Coord,
) -> (char, Color) {
    if let Some(r) = robots.iter().find(|r| r.pos == cell) {
        return match r.kind {
            RobotKind::Scout => ('x', C_SCOUT),
            RobotKind::Collector => ('o', C_COLLECTOR),
        };
    }
    if let Some(c) = ledger.get(&cell) {
        if c.remaining > 0 {
            return match c.kind {
                ResourceKind::Energy => ('E', C_ENERGY),
                ResourceKind::Crystal => ('C', C_CRYSTAL),
            };
        }
    }
    if cell == map.base {
        return ('#', C_BASE);
    }
    if map.is_obstacle(cell.0, cell.1) {
        return ('O', C_OBSTACLE);
    }
    (' ', Color::Reset)
}

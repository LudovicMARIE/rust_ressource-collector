# rust_ressource-collector

A real-time terminal simulation of autonomous robots exploring a procedurally-generated map, discovering resources, and ferrying them back to a central base. Built in Rust with Ratatui; every robot and the base run in their own OS thread and communicate exclusively through `mpsc` channels.

---

## Demo

```
┌ Simulation de Collecte ─────────────┬ Carte ────────────────────────────────────────────────────────┐
│ RESSOURCES COLLECTÉES               │      O  O  O  O  O                               O  O  O      │
│ Énergie  : 142                      │   O  O        C           O  O  O  O                          │
│ Cristaux : 87                       │         O                          O  E  O  O  O  O           │
│ Total    : 229                      │   O        O  O       x                  O                    │
│                                     │      O  O  O                    O  O  O  O  O  O              │
│ ÉTAT DU MONDE                       │   O  O  O  O  O  O  O  O  O  O  O  O                          │
│ Découvertes : 8                     │         E              O  O  o     O  O  O  O  O              │
│ Restant carte: 2140                 │   O              O  O     O  O  O  O  O  O  O  O  O           │
│ Éclaireurs  : 3                     │      O        O     O  O     O        O  O  O  O  O           │
│ Collecteurs : 4                     │   O  O  O  O  O     O  O  O        O  #  O  O  O  O           │
│                                     │      O  O  O  O  O  O     O  O  O        O  O                 │
│ LÉGENDE                             │   O     O        O     x  O  O        O  O  O  O  O           │
│ # base                              │      O  O     O  O  O  O     O  o  O  O     O                 │
│ x éclaireur                         │            O  O  O  O  O  O  O  O  O  O  O  O  O  O           │
│ o collecteur                        │   O  O  O  O  O  C  O  O  O     O  O     O  O  O  O  O        │
│ E énergie                           │      O  O  O  O  O     O  O  O  O        O                    │
│ C cristal                           │   O  O  O  O  O  O  O  O  O  O  O  O  O  O  O  O  O           │
│ O obstacle                          │                                                               │
│                                     │                                                               │
│ Appuyez sur une touche pour quitter │                                                               │
└─────────────────────────────────────┴───────────────────────────────────────────────────────────────┘
```

**Cell rendering priority:** robot > resource > base > obstacle > empty

| Symbol | Color   | Meaning         |
| ------ | ------- | --------------- |
| `#`    | Green   | Central base    |
| `x`    | Red     | Scout robot     |
| `o`    | Magenta | Collector robot |
| `E`    | Green   | Energy deposit  |
| `C`    | Magenta | Crystal deposit |
| `O`    | Cyan    | Obstacle        |

---

## Quick Start

```bash
cargo run --release   # recommended — smoother rendering
cargo run             # debug build
```

Run inside a **real terminal** (not an IDE embedded console). Press **any key** to quit; a final tally prints to stdout.

---

## Tech Stack

| Crate     | Version | Role                                                |
| --------- | ------- | --------------------------------------------------- |
| `ratatui` | 0.30    | TUI rendering (crossterm backend)                   |
| `noise`   | 0.9     | Perlin noise for procedural obstacle generation     |
| `rand`    | 0.10    | RNG throughout — seeds, resource quantities, jitter |

---

## Architecture Overview

```
                        ┌─────────────────────────────────────────┐
                        │              SharedState (Arc)          │
                        │                                         │
                        │  map: Arc<Map>          (immutable)     │
                        │  ledger: Mutex<HashMap>  ← base only    │
                        │  discovered: RwLock      ← base only    │
                        │  known_obstacles: RwLock ← base only    │
                        │  robots: Mutex<Vec>      ← each robot   │
                        │  stats: Mutex<Stats>     ← base only    │
                        │  running: AtomicBool     ← main thread  │
                        └────────────┬────────────────────────────┘
                                     │ Arc::clone  (all threads share this)
            ┌────────────────────────┼────────────────────────────┐
            │                        │                            │
    ┌───────▼──────┐        ┌────────▼───────┐          ┌────────▼───────┐
    │  Robot #0    │  mpsc  │  Robot #1 …#6  │          │  Main thread   │
    │  (scout)     ├───────>│  (collectors)  │          │  Ratatui loop  │
    └──────────────┘        └───────┬────────┘          │  50 ms poll    │
                                    │ ToBase msgs       └────────────────┘
                                    ▼
                            ┌────────────────┐
                            │  Base thread   │
                            │  sole writer   │
                            │  of world state│
                            └────────────────┘
```

### Threading Model

- **One thread per robot** — each calls `robot::run()` in a loop
- **One base thread** — the sole authority that mutates shared world knowledge
- **Main thread** — owns the Ratatui render loop, polls input every 50 ms
- **`AtomicBool shared.running`** — cooperative stop flag; all threads check it each iteration
- **`mpsc` channel (robots → base)** — robots never mutate global state directly; they send typed messages and let the base decide

When all robot senders are dropped, the base receives `RecvTimeoutError::Disconnected` and exits cleanly.

---

## Module Map

### `src/types.rs` — Shared Types

All cross-module types live here: coordinates, resource/robot kinds, the message enum, and the UI view struct.

```rust
pub type Coord = (u16, u16);

pub enum ResourceKind { Energy, Crystal }
pub enum RobotKind    { Scout, Collector }

/// Every mutation of world state goes through one of these messages.
pub enum ToBase {
    ResourceFound { pos: Coord, kind: ResourceKind },
    ObstacleFound { pos: Coord },
    Collected     { pos: Coord, kind: ResourceKind, quantity: u32 },
    Unload        { energy: u32, crystal: u32 },
}

/// Written by each robot thread, read by the UI thread.
pub struct RobotView {
    pub kind:     RobotKind,
    pub pos:      Coord,
    pub carrying: u32,
}
```

### `src/map.rs` — Procedural Map Generation

The `Map` is **immutable after construction** and shared read-only across all threads via `Arc<Map>`.

Generation pipeline:

1. **Perlin noise** threshold → obstacle grid
2. Clear a 3×3 zone around the base
3. **BFS flood-fill** from base → reachable cell set
4. Place resources **only on reachable cells** (guarantees collectors can never get permanently stuck)

```rust
pub struct Map {
    pub width:     u16,
    pub height:    u16,
    obstacles:     Vec<bool>,               // y * width + x
    pub resources: HashMap<Coord, (ResourceKind, u32)>,
    pub base:      Coord,
}
```

Key parameters baked into generation:

| Constant          | Value   | Effect                                   |
| ----------------- | ------- | ---------------------------------------- |
| `SCALE`           | `0.085` | Perlin zoom level — lower = larger blobs |
| `THRESHOLD`       | `0.40`  | Obstacle density (~40 % of map)          |
| Resource quantity | 50–200  | Random per deposit                       |

### `src/state.rs` — Fine-Grained Shared State

Uses **separate locks per concern** rather than one global mutex, so threads rarely contend:

```rust
pub struct SharedState {
    pub map:             Arc<Map>,
    pub ledger:          Mutex<HashMap<Coord, ResourceCell>>,   // remaining quantities
    pub discovered:      RwLock<HashSet<Coord>>,                // robot knowledge
    pub known_obstacles: RwLock<HashSet<Coord>>,
    pub robots:          Mutex<Vec<RobotView>>,
    pub stats:           Mutex<Stats>,
    pub running:         AtomicBool,
}
```

The rule: **no thread ever holds two locks simultaneously** → zero deadlock risk.

### `src/path.rs` — A\* Pathfinding

Single-step A\* with Manhattan distance heuristic. Returns **only the first move**, recomputed every tick so robots react to dynamic state (another robot blocking, a resource being depleted mid-route).

```rust
pub fn next_step(map: &Map, start: Coord, goal: Coord) -> Option<Coord> {
    // Returns the first cell on the optimal path from start to goal,
    // or None if already there / no path exists.
}
```

Backtracking: once A\* finds `goal`, the function walks `came[]` predecessors back to the step immediately after `start` and returns that single coordinate.

### `src/robot.rs` — Robot Behaviour

Each robot thread runs this loop:

```
while running {
    1. sense()          — scan 7×7 neighbourhood, report new finds to base
    2. scout_step()     — least-visited-neighbour heuristic
       OR
       collector_step() — navigate → collect → return to base
    3. update robots[]  — publish position for the UI
    4. sleep(base_delay + jitter)
}
```

**Scout exploration heuristic** — covers the map far faster than random walk:

```rust
fn scout_step(shared, pos, visits, rng) -> Coord {
    // Pick the walkable neighbour with the fewest prior visits.
    // Break ties randomly. Unvisited cells always win.
    let min_v = candidates.iter().map(|c| visits[c]).min();
    let best  = candidates.into_iter().filter(|c| visits[c] == min_v);
    let next  = best[rng.random_range(0..best.len())];
    visits[next] += 1;
    next
}
```

**Collector state machine:**

```
           ┌─────────────────────────────────────────────────────┐
           │                                                     │
    full? ──►  returning = true ──► move_toward(base)           │
    arrived? ► send Unload ──────► returning = false            │
           │                                                     │
    has target? ► move_toward(resource)                         │
    on target?  ► send Collected(batch) ──► update carry        │
           │                                                     │
    no target, carrying? ► returning = true                     │
    no target, empty?    ► scout_step() (explore)               │
           └─────────────────────────────────────────────────────┘
```

**Batch collection**: on arrival at a resource, the collector takes `min(remaining, capacity_left)` in a single tick — the deposit disappears immediately rather than requiring many revisits.

**Stuck detection**: if a collector doesn't move for 6 consecutive ticks (blocked by another robot), it falls back to a scout step to break the deadlock.

**Collision avoidance** in `move_toward()`:

```rust
fn move_toward(shared, pos, goal) -> Coord {
    let step = next_step(map, pos, goal)?;  // A* ideal step
    if !occupied.contains(step) {
        return step;                         // free — take it
    }
    // Blocked: find a walkable neighbour no farther from goal
    best_detour(pos, goal, occupied)
        .unwrap_or(pos)                      // stay put this tick
}
```

### `src/base.rs` — The World Authority

The base is the **only** entity that writes to `ledger`, `discovered`, `known_obstacles`, and `stats`. All robots send messages; the base serialises all mutations.

```rust
fn handle(shared: &SharedState, msg: ToBase) {
    match msg {
        ResourceFound { pos, .. } => {
            // Only register if the ledger still shows remaining > 0.
            if ledger[pos].remaining > 0 { discovered.insert(pos); }
        }
        ObstacleFound { pos } => { known_obstacles.insert(pos); }
        Collected { pos, quantity, .. } => {
            ledger[pos].remaining -= quantity;
            if ledger[pos].remaining == 0 { discovered.remove(pos); }
        }
        Unload { energy, crystal } => { stats += (energy, crystal); }
    }
}
```

### `src/ui.rs` — Ratatui Rendering

The UI is a **horizontal split**: 30-column sidebar + full-width map panel.

```
Layout::horizontal([Constraint::Length(30), Constraint::Min(0)])
```

The map is rendered cell by cell. Priority per cell (first match wins):

```rust
fn cell_glyph(map, ledger, robots, cell) -> (char, Color) {
    if robot at cell  → ('x' / 'o', Red / Magenta)
    if resource > 0   → ('E' / 'C', Green / Magenta)
    if cell == base   → ('#',        LightGreen)
    if obstacle       → ('O',        LightCyan)
    else              → (' ',        Reset)
}
```

### `src/main.rs` — Bootstrap

Map dimensions are computed from the **actual terminal size** at startup:

```rust
let map_width  = term_cols.saturating_sub(SIDEBAR_COLS).max(20);
//                         └── 32 (30 content + 2 borders)
let map_height = term_rows.saturating_sub(MAP_BORDER_ROWS).max(10);
//                         └── 2 (top + bottom borders)
```

Thread launch order:

```rust
// 1. base thread (consumer)
thread::spawn(|| base::run(shared, rx));

// 2. one thread per robot (producers)
for (id, kind) in kinds { thread::spawn(|| robot::run(id, kind, shared, tx)); }

// 3. drop the main-thread tx clone → base gets Disconnected when all robots stop
drop(tx);

// 4. Ratatui loop on main thread
run_ui(&mut terminal, &shared);

// 5. signal stop, join all threads
shared.running.store(false, Ordering::Relaxed);
for h in handles { h.join(); }
```

---

## Simulation Parameters

Defined at the top of `src/main.rs`:

```rust
const RESOURCE_COUNT: usize = 40;   // deposits placed on the map
const NUM_SCOUTS:     usize = 3;    // explorer robots
const NUM_COLLECTORS: usize = 4;    // harvester robots
```

And in `src/robot.rs`:

```rust
const CARRY_CAPACITY: u32 = 15;   // units before forced return to base
const SENSE_RADIUS:   i32 = 3;    // 7×7 perception square around each robot
```

Robot speeds (milliseconds per tick, plus 0–40 ms random jitter):

| Robot type | Base delay |
| ---------- | ---------- |
| Scout      | 85 ms      |
| Collector  | 110 ms     |

---

## Data Flow Summary

```
Map::generate()
    └─► SharedState::new()              (ledger pre-filled, discovered empty)

Robot threads (tick loop)
    ├─► sense() ──────────────────────► tx.send(ResourceFound / ObstacleFound)
    ├─► scout_step() / collector_step()
    │       └─► move_toward() ────────► A* next_step()
    │       └─► on resource ──────────► tx.send(Collected)
    │       └─► at base ──────────────► tx.send(Unload)
    └─► robots[id].pos = pos           (UI reads this)

Base thread
    └─► rx.recv() ────────────────────► handle()
            ├─► ledger   (Mutex)        sole writer
            ├─► discovered (RwLock)     sole writer
            ├─► known_obstacles (RwLock)sole writer
            └─► stats    (Mutex)        sole writer

Main thread (Ratatui)
    └─► ui::render() every 50 ms
            ├─► reads ledger, robots, stats, discovered (short-lived locks)
            └─► draws sidebar + map
```

---

## Project Layout

```
.
├── Cargo.toml
└── src/
    ├── main.rs       entry point, thread launch, render loop
    ├── types.rs      Coord, ResourceKind, RobotKind, ToBase, RobotView
    ├── map.rs        Map::generate() — Perlin + BFS reachability
    ├── state.rs      SharedState — fine-grained locks
    ├── path.rs       A* next_step()
    ├── robot.rs      robot thread: sense, scout_step, collector_step
    ├── base.rs       base thread: message handler, sole state writer
    └── ui.rs         Ratatui render: sidebar + map panel
```

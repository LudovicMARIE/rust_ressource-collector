pub type Coord = (u16, u16);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResourceKind {
    Energy,
    Crystal,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RobotKind {
    Scout,
    Collector,
}

#[derive(Clone, Debug)]
pub enum ToBase {
    ResourceFound { pos: Coord, kind: ResourceKind },
    ObstacleFound { pos: Coord },
    Collected { pos: Coord, kind: ResourceKind, quantity: u32 },
    Unload { energy: u32, crystal: u32 },
}

#[derive(Clone, Copy)]
pub struct RobotView {
    pub kind: RobotKind,
    pub pos: Coord,
    pub carrying: u32,
}

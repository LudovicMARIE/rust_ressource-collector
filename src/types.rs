//! Types partagés entre les modules : coordonnées, ressources, robots et
//! messages échangés avec la base via les canaux de communication.

/// Coordonnée sur la carte : `(x, y)`.
pub type Coord = (u16, u16);

/// Les deux types de ressources collectables.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResourceKind {
    Energy,
    Crystal,
}

/// Les deux types de robots autonomes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RobotKind {
    Scout,
    Collector,
}

/// Message asynchrone envoyé par un robot vers la base.
///
/// Toute modification de la connaissance globale (ressources découvertes,
/// obstacles découverts, dépôt de ressources) passe exclusivement par ces
/// messages : la base est la seule entité autorisée à muter l'état du monde.
#[derive(Clone, Debug)]
pub enum ToBase {
    /// Un éclaireur a découvert une ressource à `pos`.
    ResourceFound { pos: Coord, kind: ResourceKind },
    /// Un robot a découvert un obstacle à `pos`.
    ObstacleFound { pos: Coord },
    /// Un collecteur a collecté `quantity` unités de ressource à `pos`.
    Collected { pos: Coord, kind: ResourceKind, quantity: u32 },
    /// Un collecteur décharge sa cargaison à la base.
    Unload { energy: u32, crystal: u32 },
}

/// Vue d'un robot pour le rendu (écrite par le robot, lue par l'UI).
#[derive(Clone, Copy)]
pub struct RobotView {
    pub kind: RobotKind,
    pub pos: Coord,
    pub carrying: u32,
}

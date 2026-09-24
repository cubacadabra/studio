use super::*;

#[derive(Clone, Debug)]
pub(crate) enum SceneEditRequest {
    AddObject {
        world_id: Option<String>,
        kind: SceneObjectKind,
    },
    UseImageAsFloor {
        asset_path: PathBuf,
    },
    SetTransform {
        target: String,
        position: [f32; 3],
        rotation: Option<[f32; 3]>,
        scale: Option<[f32; 3]>,
    },
    SetPrimitiveSize {
        target: String,
        position: [f32; 3],
        size: [f32; 3],
    },
    UpdateSignText {
        target: String,
        text: String,
    },
    UpdateProperty {
        target: String,
        key: String,
        value: Value,
    },
    RemoveProperty {
        target: String,
        key: String,
    },
    RenameObject {
        target: String,
        name: String,
    },
    ReparentObjects {
        targets: Vec<String>,
        parent_id: String,
    },
    GroupObjects {
        targets: Vec<String>,
    },
    DuplicateObject {
        target: String,
    },
    DeleteObject {
        target: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SceneTool {
    #[default]
    Choose,
    Place,
    Shape,
    Turn,
    Craft,
}

impl SceneTool {
    pub(crate) const ALL: [Self; 5] = [
        Self::Choose,
        Self::Place,
        Self::Shape,
        Self::Turn,
        Self::Craft,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Choose => "Choose",
            Self::Place => "Place",
            Self::Shape => "Shape",
            Self::Turn => "Turn",
            Self::Craft => "Craft",
        }
    }

    pub(crate) const fn shortcut(self) -> &'static str {
        match self {
            Self::Choose => "Q",
            Self::Place => "W",
            Self::Shape => "E",
            Self::Turn => "R",
            Self::Craft => "T",
        }
    }

    pub(crate) const fn hint(self, can_resize: bool) -> &'static str {
        match self {
            Self::Choose => "Click objects to choose · double-click to frame",
            Self::Place => "Drag the object to place · drag the upper handle to lift",
            Self::Shape if can_resize => "Drag corner and height handles to reshape",
            Self::Shape => "This object has no editable shape",
            Self::Turn => "Drag the ring handle to turn in 15° steps",
            Self::Craft if can_resize => "Drag to place · handles reshape, lift, or turn",
            Self::Craft => "Drag to place · upper handles lift or turn",
        }
    }

    pub(crate) const fn moves(self) -> bool {
        matches!(self, Self::Place | Self::Craft)
    }

    pub(crate) const fn resizes(self) -> bool {
        matches!(self, Self::Shape | Self::Craft)
    }

    pub(crate) const fn rotates(self) -> bool {
        matches!(self, Self::Turn | Self::Craft)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SceneObjectKind {
    Block,
    Group,
    Sign,
    Ladder,
    Actor,
    Interaction,
    Checkpoint,
    Hazard,
    SafeZone,
}

impl SceneObjectKind {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 9] = [
        Self::Block,
        Self::Group,
        Self::Sign,
        Self::Ladder,
        Self::Actor,
        Self::Interaction,
        Self::Checkpoint,
        Self::Hazard,
        Self::SafeZone,
    ];

    pub(crate) const MANIFEST_KINDS: [Self; 8] = [
        Self::Block,
        Self::Sign,
        Self::Ladder,
        Self::Actor,
        Self::Interaction,
        Self::Checkpoint,
        Self::Hazard,
        Self::SafeZone,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Block => "Block",
            Self::Group => "Group",
            Self::Sign => "Sign",
            Self::Ladder => "Ladder",
            Self::Actor => "Actor",
            Self::Interaction => "Interaction",
            Self::Checkpoint => "Checkpoint",
            Self::Hazard => "Hazard",
            Self::SafeZone => "Safe zone",
        }
    }

    pub(crate) fn collection(self) -> Option<&'static str> {
        Some(match self {
            Self::Block => "blocks",
            Self::Group => return None,
            Self::Sign => "signs",
            Self::Ladder => "ladders",
            Self::Actor => "actors",
            Self::Interaction => "interactions",
            Self::Checkpoint => "checkpoints",
            Self::Hazard => "hazards",
            Self::SafeZone => "safeZones",
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SceneObjectGeometry {
    pub(crate) id: String,
    pub(crate) position: [f32; 3],
    pub(crate) rotation: [f32; 3],
    pub(crate) local_rotation: [f32; 3],
    pub(crate) size: Option<[f32; 3]>,
    pub(crate) scale: Option<[f32; 3]>,
    pub(crate) primitive_size: bool,
    pub(crate) editable: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneObjectProjection {
    pub(crate) id: String,
    pub(crate) position: [f32; 3],
    pub(crate) rotation: [f32; 3],
    pub(crate) local_rotation: [f32; 3],
    pub(crate) size: Option<[f32; 3]>,
    pub(crate) scale: Option<[f32; 3]>,
    pub(crate) base_size: Option<[f32; 3]>,
    pub(crate) primitive_size: bool,
    pub(crate) editable: bool,
    pub(crate) center_screen: Pos2,
    pub(crate) world_corners: Option<[[f32; 3]; 4]>,
    pub(crate) screen_corners: Option<[Pos2; 4]>,
    pub(crate) bottom_screen_corners: Option<[Pos2; 4]>,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneTreeDragPayload {
    pub(crate) targets: Vec<String>,
}

impl SceneObjectProjection {
    pub(crate) fn bounds(&self) -> Rect {
        self.screen_corners.map_or_else(
            || Rect::from_center_size(self.center_screen, Vec2::splat(20.0)),
            |top| {
                let mut bounds = Rect::from_points(&top);
                if let Some(bottom) = self.bottom_screen_corners {
                    bounds = bounds.union(Rect::from_points(&bottom));
                }
                bounds.expand(4.0)
            },
        )
    }

    pub(crate) fn contains(&self, point: Pos2) -> bool {
        let Some(top) = self.screen_corners else {
            return self.center_screen.distance(point) <= 10.0;
        };
        if point_in_scene_quad(point, top) {
            return true;
        }
        let Some(bottom) = self.bottom_screen_corners else {
            return false;
        };
        if point_in_scene_quad(point, bottom) {
            return true;
        }
        (0..4).any(|index| {
            point_in_scene_quad(
                point,
                [
                    top[index],
                    top[(index + 1) % 4],
                    bottom[(index + 1) % 4],
                    bottom[index],
                ],
            )
        })
    }
}

fn point_in_scene_quad(point: Pos2, corners: [Pos2; 4]) -> bool {
    let mut sign = 0.0_f32;
    for index in 0..4 {
        let a = corners[index];
        let b = corners[(index + 1) % 4];
        let cross = (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x);
        if cross.abs() <= f32::EPSILON {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if sign != cross.signum() {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SceneViewportEditPhase {
    Begin,
    Update,
    Commit,
}

#[derive(Clone, Debug)]
pub(crate) enum SceneViewportEditRequest {
    Move {
        phase: SceneViewportEditPhase,
        target: String,
        origin_screen: Pos2,
        current_screen: Pos2,
        origin_position: [f32; 3],
    },
    MoveHeight {
        phase: SceneViewportEditPhase,
        target: String,
        origin_screen: Pos2,
        current_screen: Pos2,
        origin_position: [f32; 3],
    },
    Resize {
        phase: SceneViewportEditPhase,
        target: String,
        current_screen: Pos2,
        fixed_corner: [f32; 3],
        origin_position: [f32; 3],
        origin_rotation: [f32; 3],
        origin_size: [f32; 3],
        origin_scale: Option<[f32; 3]>,
        base_size: Option<[f32; 3]>,
        primitive_size: bool,
    },
    ResizeHeight {
        phase: SceneViewportEditPhase,
        target: String,
        origin_screen: Pos2,
        current_screen: Pos2,
        origin_position: [f32; 3],
        origin_scale: Option<[f32; 3]>,
        base_size: [f32; 3],
        primitive_size: bool,
    },
    RotateYaw {
        phase: SceneViewportEditPhase,
        target: String,
        origin_screen: Pos2,
        current_screen: Pos2,
        origin_position: [f32; 3],
        origin_rotation: [f32; 3],
    },
}

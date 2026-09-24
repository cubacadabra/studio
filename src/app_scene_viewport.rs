use super::*;

impl StudioApp {
    pub(crate) fn authoring_local_position_for_world(
        &self,
        id: &str,
        world_position: [f32; 3],
    ) -> Result<Option<[f32; 3]>, String> {
        let Some(scene) = self.authoring_scene.as_ref() else {
            return Ok(None);
        };
        if !self.authoring_scene_indices.contains_key(id) {
            return Ok(None);
        }
        scene.local_position_for_world(id, world_position).map(Some)
    }

    pub(crate) fn authoring_local_transform_for_world(
        &self,
        id: &str,
        world_position: [f32; 3],
        world_scale: [f32; 3],
    ) -> Result<([f32; 3], [f32; 3]), String> {
        let Some(scene) = self.authoring_scene.as_ref() else {
            return Ok((world_position, world_scale));
        };
        if !self.authoring_scene_indices.contains_key(id) {
            return Ok((world_position, world_scale));
        }
        scene.local_transform_for_world(id, world_position, world_scale)
    }

    pub(crate) fn update_scene_object_projections(&mut self) {
        let Some(window) = &self.window else { return };
        let scale = window.scale_factor() as f32;
        let active_world = self.client.engine().active_world_id();
        let geometries = self
            .shell
            .as_ref()
            .map(StudioShell::scene_object_geometries)
            .map(|geometries| {
                geometries
                    .iter()
                    .filter(|geometry| {
                        active_world.is_none_or(|world| {
                            scene_world_id(&geometry.id).is_none_or(|candidate| candidate == world)
                        })
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let projections = self.renderer.as_ref().map_or_else(Vec::new, |renderer| {
            let mut world_points = Vec::new();
            for geometry in &geometries {
                let [x, y, z] = geometry.position;
                world_points.push([x, y, z]);
                let transform_scale = geometry.scale.unwrap_or([1.0; 3]);
                if let Some([width, height, depth]) = geometry.size.map(|size| {
                    [
                        size[0] * transform_scale[0],
                        size[1] * transform_scale[1],
                        size[2] * transform_scale[2],
                    ]
                }) {
                    let (top, bottom) =
                        scene_box_corners([x, y, z], [width, height, depth], geometry.rotation);
                    world_points.extend(top);
                    world_points.extend(bottom);
                }
            }
            let projected = renderer.studio_project_world_points(&world_points);
            let mut point_index = 0;
            geometries
                .into_iter()
                .filter_map(|geometry| {
                    let center = projected.get(point_index).copied().flatten();
                    point_index += 1;
                    let visual_size = geometry.size.map(|size| {
                        let transform_scale = geometry.scale.unwrap_or([1.0; 3]);
                        [
                            size[0] * transform_scale[0],
                            size[1] * transform_scale[1],
                            size[2] * transform_scale[2],
                        ]
                    });
                    let (world_corners, screen_corners, bottom_screen_corners) =
                        if let Some([width, height, depth]) = visual_size {
                            let [x, y, z] = geometry.position;
                            let (top_corners, _) = scene_box_corners(
                                [x, y, z],
                                [width, height, depth],
                                geometry.rotation,
                            );
                            let top_screen = projected
                                .get(point_index..point_index + 4)?
                                .iter()
                                .copied()
                                .collect::<Option<Vec<_>>>()
                                .and_then(|points| points.try_into().ok())
                                .map(|points: [[f32; 2]; 4]| {
                                    points.map(|[x, y]| egui::pos2(x / scale, y / scale))
                                });
                            point_index += 4;
                            let bottom_screen = projected
                                .get(point_index..point_index + 4)?
                                .iter()
                                .copied()
                                .collect::<Option<Vec<_>>>()
                                .and_then(|points| points.try_into().ok())
                                .map(|points: [[f32; 2]; 4]| {
                                    points.map(|[x, y]| egui::pos2(x / scale, y / scale))
                                });
                            point_index += 4;
                            (Some(top_corners), top_screen, bottom_screen)
                        } else {
                            (None, None, None)
                        };
                    let center = center?;
                    Some(SceneObjectProjection {
                        id: geometry.id,
                        position: geometry.position,
                        rotation: geometry.rotation,
                        local_rotation: geometry.local_rotation,
                        size: visual_size,
                        scale: geometry.scale,
                        base_size: geometry.size,
                        primitive_size: geometry.primitive_size,
                        editable: geometry.editable,
                        center_screen: egui::pos2(center[0] / scale, center[1] / scale),
                        world_corners,
                        screen_corners,
                        bottom_screen_corners,
                    })
                })
                .collect()
        });
        if let Some(shell) = &mut self.shell {
            shell.set_scene_object_projections(projections);
        }
    }

    pub(crate) fn resolve_scene_viewport_edit(
        &self,
        request: SceneViewportEditRequest,
    ) -> Result<Option<(SceneEditRequest, SceneViewportEditPhase)>, String> {
        let Some(renderer) = self.renderer.as_ref() else {
            return Ok(None);
        };
        let Some(window) = self.window.as_ref() else {
            return Ok(None);
        };
        let scale = window.scale_factor() as f32;
        let world_point = |point: egui::Pos2, plane_y: f32| {
            renderer
                .studio_world_point_on_horizontal_plane([point.x * scale, point.y * scale], plane_y)
        };
        match request {
            SceneViewportEditRequest::Move {
                phase,
                target,
                origin_screen,
                current_screen,
                origin_position,
            } => {
                let Some(origin) = world_point(origin_screen, origin_position[1]) else {
                    return Ok(None);
                };
                let Some(current) = world_point(current_screen, origin_position[1]) else {
                    return Ok(None);
                };
                let world_position = [
                    snap_scene_value(origin_position[0] + current[0] - origin[0]),
                    origin_position[1],
                    snap_scene_value(origin_position[2] + current[2] - origin[2]),
                ];
                let position = self
                    .authoring_local_position_for_world(&target, world_position)?
                    .unwrap_or(world_position);
                Ok(Some((
                    SceneEditRequest::SetTransform {
                        target,
                        position,
                        rotation: None,
                        scale: None,
                    },
                    phase,
                )))
            }
            SceneViewportEditRequest::MoveHeight {
                phase,
                target,
                origin_screen,
                current_screen,
                origin_position,
            } => {
                let world_position =
                    scene_vertical_drag_position(origin_screen, current_screen, origin_position);
                let position = self
                    .authoring_local_position_for_world(&target, world_position)?
                    .unwrap_or(world_position);
                Ok(Some((
                    SceneEditRequest::SetTransform {
                        target,
                        position,
                        rotation: None,
                        scale: None,
                    },
                    phase,
                )))
            }
            SceneViewportEditRequest::Resize {
                phase,
                target,
                current_screen,
                fixed_corner,
                origin_position,
                origin_rotation,
                origin_size,
                origin_scale,
                base_size,
                primitive_size,
            } => {
                let Some(moving) = world_point(current_screen, fixed_corner[1]) else {
                    return Ok(None);
                };
                let yaw = origin_rotation[1];
                let (sin, cos) = yaw.sin_cos();
                let to_local = |point: [f32; 3]| {
                    let dx = point[0] - origin_position[0];
                    let dz = point[2] - origin_position[2];
                    [dx * cos - dz * sin, point[1], dx * sin + dz * cos]
                };
                let to_world = |point: [f32; 3]| {
                    [
                        origin_position[0] + point[0] * cos + point[2] * sin,
                        point[1],
                        origin_position[2] - point[0] * sin + point[2] * cos,
                    ]
                };
                let fixed_local = to_local(fixed_corner);
                let mut moving_local = to_local(moving);
                moving_local[0] = snap_scene_value(moving_local[0]);
                moving_local[2] = snap_scene_value(moving_local[2]);
                let x_direction = if fixed_local[0] <= 0.0 { 1.0 } else { -1.0 };
                let z_direction = if fixed_local[2] <= 0.0 { 1.0 } else { -1.0 };
                if (moving_local[0] - fixed_local[0]).abs() < 0.25 {
                    moving_local[0] = fixed_local[0] + 0.25 * x_direction;
                }
                if (moving_local[2] - fixed_local[2]).abs() < 0.25 {
                    moving_local[2] = fixed_local[2] + 0.25 * z_direction;
                }
                let size = [
                    (moving_local[0] - fixed_local[0]).abs(),
                    origin_size[1],
                    (moving_local[2] - fixed_local[2]).abs(),
                ];
                let desired_scale = match (origin_scale, base_size, primitive_size) {
                    (_, _, true) => None,
                    (Some(origin_scale), Some(base_size), false) => Some([
                        (size[0] / base_size[0]).max(0.05),
                        origin_scale[1],
                        (size[2] / base_size[2]).max(0.05),
                    ]),
                    _ => None,
                };
                let world_position = to_world([
                    (moving_local[0] + fixed_local[0]) * 0.5,
                    origin_position[1],
                    (moving_local[2] + fixed_local[2]) * 0.5,
                ]);
                let (position, scale) = if let Some(desired_scale) = desired_scale {
                    let (position, scale) = self.authoring_local_transform_for_world(
                        &target,
                        world_position,
                        desired_scale,
                    )?;
                    (position, Some(scale))
                } else {
                    (world_position, None)
                };
                Ok(Some((
                    if primitive_size {
                        SceneEditRequest::SetPrimitiveSize {
                            target,
                            position,
                            size,
                        }
                    } else {
                        SceneEditRequest::SetTransform {
                            target,
                            position,
                            rotation: None,
                            scale,
                        }
                    },
                    phase,
                )))
            }
            SceneViewportEditRequest::ResizeHeight {
                phase,
                target,
                origin_screen,
                current_screen,
                origin_position,
                origin_scale,
                base_size,
                primitive_size,
            } => {
                let delta = origin_screen.y - current_screen.y;
                let original_height = base_size[1] * origin_scale.map_or(1.0, |scale| scale[1]);
                let height = (original_height + delta * 0.05).max(0.25);
                let scale_y = (height / base_size[1]).max(0.05);
                let world_position = [
                    origin_position[0],
                    origin_position[1] + (height - original_height) * 0.5,
                    origin_position[2],
                ];
                let (position, size, scale) = if primitive_size {
                    let position = self
                        .authoring_local_position_for_world(&target, world_position)?
                        .unwrap_or(world_position);
                    (position, Some([base_size[0], height, base_size[2]]), None)
                } else if let Some(origin_scale) = origin_scale {
                    let desired_scale = [origin_scale[0], scale_y, origin_scale[2]];
                    let (position, scale) = self.authoring_local_transform_for_world(
                        &target,
                        world_position,
                        desired_scale,
                    )?;
                    (position, None, Some(scale))
                } else {
                    let position = self
                        .authoring_local_position_for_world(&target, world_position)?
                        .unwrap_or(world_position);
                    (position, Some([base_size[0], height, base_size[2]]), None)
                };
                Ok(Some((
                    if primitive_size {
                        SceneEditRequest::SetPrimitiveSize {
                            target,
                            position,
                            size: size.expect("primitive resize has a size"),
                        }
                    } else {
                        SceneEditRequest::SetTransform {
                            target,
                            position,
                            rotation: None,
                            scale,
                        }
                    },
                    phase,
                )))
            }
            SceneViewportEditRequest::RotateYaw {
                phase,
                target,
                origin_screen,
                current_screen,
                origin_position,
                origin_rotation,
            } => {
                let delta = current_screen.x - origin_screen.x;
                let mut rotation = origin_rotation;
                rotation[1] = snap_scene_angle(origin_rotation[1] + delta * 0.01);
                let position = self
                    .authoring_local_position_for_world(&target, origin_position)?
                    .unwrap_or(origin_position);
                Ok(Some((
                    SceneEditRequest::SetTransform {
                        target,
                        position,
                        rotation: Some(rotation),
                        scale: None,
                    },
                    phase,
                )))
            }
        }
    }
}

fn scene_box_corners(
    [x, y, z]: [f32; 3],
    [width, height, depth]: [f32; 3],
    rotation: [f32; 3],
) -> ([[f32; 3]; 4], [[f32; 3]; 4]) {
    let (sin_x, cos_x) = rotation[0].sin_cos();
    let (sin_y, cos_y) = rotation[1].sin_cos();
    let (sin_z, cos_z) = rotation[2].sin_cos();
    let rotate = |local_x: f32, local_y: f32, local_z: f32| {
        // Match the authoring/runtime XYZ Euler order without introducing a
        // second transform dependency into Studio's viewport projection.
        let rotated_x = cos_y * cos_z * local_x + (-cos_y * sin_z) * local_y + sin_y * local_z;
        let rotated_y = (sin_x * sin_y * cos_z + cos_x * sin_z) * local_x
            + (-sin_x * sin_y * sin_z + cos_x * cos_z) * local_y
            + (-sin_x * cos_y) * local_z;
        let rotated_z = (-cos_x * sin_y * cos_z + sin_x * sin_z) * local_x
            + (cos_x * sin_y * sin_z + sin_x * cos_z) * local_y
            + cos_x * cos_y * local_z;
        [x + rotated_x, y + rotated_y, z + rotated_z]
    };
    let local = [
        [-width * 0.5, -height * 0.5, -depth * 0.5],
        [width * 0.5, -height * 0.5, -depth * 0.5],
        [width * 0.5, -height * 0.5, depth * 0.5],
        [-width * 0.5, -height * 0.5, depth * 0.5],
    ];
    (
        local.map(|[local_x, _, local_z]| rotate(local_x, height * 0.5, local_z)),
        local.map(|[local_x, _, local_z]| rotate(local_x, -height * 0.5, local_z)),
    )
}

pub(crate) fn snap_scene_value(value: f32) -> f32 {
    (value * 4.0).round() * 0.25
}

pub(crate) fn snap_scene_angle(value: f32) -> f32 {
    const STEP: f32 = std::f32::consts::PI / 12.0;
    (value / STEP).round() * STEP
}

fn scene_vertical_drag_position(
    origin_screen: egui::Pos2,
    current_screen: egui::Pos2,
    origin_position: [f32; 3],
) -> [f32; 3] {
    [
        origin_position[0],
        snap_scene_value(origin_position[1] + (origin_screen.y - current_screen.y) * 0.05),
        origin_position[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_angles_snap_to_fifteen_degree_steps() {
        assert!((snap_scene_angle(8.0_f32.to_radians()) - 15.0_f32.to_radians()).abs() < 0.0001);
        assert!((snap_scene_angle(22.0_f32.to_radians()) - 15.0_f32.to_radians()).abs() < 0.0001);
        assert!((snap_scene_angle(-25.0_f32.to_radians()) + 30.0_f32.to_radians()).abs() < 0.0001);
    }

    #[test]
    fn projected_box_corners_follow_yaw() {
        let (top, bottom) = scene_box_corners(
            [4.0, 3.0, 8.0],
            [4.0, 2.0, 2.0],
            [0.0, std::f32::consts::FRAC_PI_2, 0.0],
        );
        assert!((top[0][0] - 3.0).abs() < 0.0001);
        assert!((top[0][1] - 4.0).abs() < 0.0001);
        assert!((top[0][2] - 10.0).abs() < 0.0001);
        assert!((bottom[0][1] - 2.0).abs() < 0.0001);
    }

    #[test]
    fn vertical_drag_raises_and_lowers_without_changing_the_ground_plane_axes() {
        assert_eq!(
            scene_vertical_drag_position(
                egui::pos2(100.0, 100.0),
                egui::pos2(140.0, 60.0),
                [3.0, 1.0, -2.0],
            ),
            [3.0, 3.0, -2.0]
        );
        assert_eq!(
            scene_vertical_drag_position(
                egui::pos2(100.0, 100.0),
                egui::pos2(100.0, 130.0),
                [3.0, 1.0, -2.0],
            ),
            [3.0, -0.5, -2.0]
        );
    }
}

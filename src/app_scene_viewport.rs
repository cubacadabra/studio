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
                    let top = y + height * 0.5;
                    world_points.extend([
                        [x - width * 0.5, top, z - depth * 0.5],
                        [x + width * 0.5, top, z - depth * 0.5],
                        [x + width * 0.5, top, z + depth * 0.5],
                        [x - width * 0.5, top, z + depth * 0.5],
                    ]);
                    let bottom = y - height * 0.5;
                    world_points.extend([
                        [x - width * 0.5, bottom, z - depth * 0.5],
                        [x + width * 0.5, bottom, z - depth * 0.5],
                        [x + width * 0.5, bottom, z + depth * 0.5],
                        [x - width * 0.5, bottom, z + depth * 0.5],
                    ]);
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
                            let top = y + height * 0.5;
                            let top_corners = [
                                [x - width * 0.5, top, z - depth * 0.5],
                                [x + width * 0.5, top, z - depth * 0.5],
                                [x + width * 0.5, top, z + depth * 0.5],
                                [x - width * 0.5, top, z + depth * 0.5],
                            ];
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
                origin_size,
                origin_scale,
                base_size,
                primitive_size,
            } => {
                let Some(mut moving) = world_point(current_screen, fixed_corner[1]) else {
                    return Ok(None);
                };
                moving[0] = snap_scene_value(moving[0]);
                moving[2] = snap_scene_value(moving[2]);
                let x_direction = if fixed_corner[0] <= origin_position[0] {
                    1.0
                } else {
                    -1.0
                };
                let z_direction = if fixed_corner[2] <= origin_position[2] {
                    1.0
                } else {
                    -1.0
                };
                if (moving[0] - fixed_corner[0]).abs() < 0.25 {
                    moving[0] = fixed_corner[0] + 0.25 * x_direction;
                }
                if (moving[2] - fixed_corner[2]).abs() < 0.25 {
                    moving[2] = fixed_corner[2] + 0.25 * z_direction;
                }
                let size = [
                    (moving[0] - fixed_corner[0]).abs(),
                    origin_size[1],
                    (moving[2] - fixed_corner[2]).abs(),
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
                let world_position = [
                    (moving[0] + fixed_corner[0]) * 0.5,
                    origin_position[1],
                    (moving[2] + fixed_corner[2]) * 0.5,
                ];
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
                            scale,
                        }
                    },
                    phase,
                )))
            }
        }
    }
}

pub(crate) fn snap_scene_value(value: f32) -> f32 {
    (value * 4.0).round() * 0.25
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

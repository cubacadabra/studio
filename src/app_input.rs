use super::*;
impl StudioApp {
    pub(crate) fn review_navigation_active(&self) -> bool {
        self.shell.as_ref().is_some_and(|shell| {
            !shell.is_morphs_workspace()
                && shell.review_camera() != crate::shell::ReviewCameraPreset::Gameplay
                && !shell.is_project_loading()
        })
    }

    pub(crate) fn clear_pointer_controls(&mut self) {
        let cancel_ui_pointer = self.ui_pointer_active;
        self.pressed_keys.clear();
        self.jump_queued = false;
        self.climb = false;
        self.pointer_active = false;
        self.camera_pointer_active = false;
        self.pan_pointer_active = false;
        self.scene_pointer_active = false;
        self.movement_pointer_active = false;
        self.movement_pointer_origin = None;
        self.ui_pointer_active = false;
        self.joystick_input = (0.0, 0.0);
        self.look_delta = (0.0, 0.0);
        self.pan_delta = (0.0, 0.0);
        self.zoom_delta = 0.0;
        if cancel_ui_pointer {
            self.pointer_event(3, 0.0, 0.0);
        }
    }

    pub(crate) fn pointer_event(&mut self, phase: u8, x: f32, y: f32) -> bool {
        self.client.ui_pointer_event(1, phase, x, y)
    }

    pub(crate) fn drain_ui_events(&mut self) {
        while let Some(source) = self.client.poll_ui_event_json() {
            let Ok(event) = serde_json::from_slice::<Value>(&source) else {
                continue;
            };
            let action = event
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let phase = event
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match action {
                "player.move" => {
                    let x = event.get("x").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    let y = event.get("y").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    self.joystick_input = (x, y);
                }
                "player.jump" if phase == "activate" => self.jump_queued = true,
                "player.run" if phase == "activate" => self.mobile_sprint = !self.mobile_sprint,
                "player.climb" if phase == "activate" => self.climb = !self.climb,
                _ => {}
            }
        }
    }

    pub(crate) fn handle_key(&mut self, event: &KeyEvent, _event_loop: &ActiveEventLoop) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        match event.state {
            ElementState::Pressed => {
                if code == KeyCode::Escape && !event.repeat {
                    self.cancel_scene_viewport_edit();
                }
                if code == KeyCode::Digit3
                    && !event.repeat
                    && let Some(shell) = &mut self.shell
                {
                    shell.activate_scene_resize_tool();
                }
                if code == KeyCode::Space && !event.repeat {
                    self.jump_queued = true;
                }
                self.pressed_keys.insert(code);
            }
            ElementState::Released => {
                self.pressed_keys.remove(&code);
            }
        }
    }

    pub(crate) fn handle_cursor_move(&mut self, x: f64, y: f64) {
        let scale = self.window.as_ref().map_or(1.0, Window::scale_factor) as f32;
        let logical = (x as f32 / scale, y as f32 / scale);
        if self.scene_pointer_active {
            self.pointer_position = Some(logical);
            return;
        }
        if let Some(previous) = self.pointer_position {
            if self.pan_pointer_active {
                self.pan_delta.0 += logical.0 - previous.0;
                self.pan_delta.1 += logical.1 - previous.1;
            } else if (self.pointer_active || self.camera_pointer_active) && !self.ui_pointer_active
            {
                self.look_delta.0 += logical.0 - previous.0;
                self.look_delta.1 += logical.1 - previous.1;
            }
        }
        if self.movement_pointer_active
            && let Some(origin) = self.movement_pointer_origin
        {
            const JOYSTICK_RADIUS: f32 = 72.0;
            self.joystick_input = (
                ((logical.0 - origin.0) / JOYSTICK_RADIUS).clamp(-1.0, 1.0),
                ((logical.1 - origin.1) / JOYSTICK_RADIUS).clamp(-1.0, 1.0),
            );
        }
        self.pointer_position = Some(logical);
        if self.ui_pointer_active {
            if let Some((local_x, local_y)) = self.runtime_pointer(logical.0, logical.1, false) {
                self.pointer_event(1, local_x, local_y);
            }
        }
    }

    pub(crate) fn runtime_pointer(
        &self,
        x: f32,
        y: f32,
        require_inside: bool,
    ) -> Option<(f32, f32)> {
        let viewport = self.shell.as_ref()?.runtime_viewport();
        if !viewport.is_positive() || (require_inside && !viewport.contains(egui::pos2(x, y))) {
            return None;
        }
        Some((x - viewport.min.x, y - viewport.min.y))
    }

    pub(crate) fn handle_mouse_button(&mut self, state: ElementState, button: MouseButton) {
        let Some((x, y)) = self.pointer_position else {
            return;
        };
        if self.review_navigation_active()
            && matches!(button, MouseButton::Right | MouseButton::Middle)
        {
            self.pan_pointer_active =
                state == ElementState::Pressed && self.runtime_pointer(x, y, true).is_some();
            return;
        }
        if state == ElementState::Released
            && matches!(button, MouseButton::Right | MouseButton::Middle)
        {
            self.pan_pointer_active = false;
        }
        if matches!(button, MouseButton::Left | MouseButton::Right) {
            if state == ElementState::Pressed
                && self
                    .shell
                    .as_ref()
                    .is_some_and(|shell| shell.scene_editor_hit_test(egui::pos2(x, y)))
            {
                self.scene_pointer_active = true;
                self.pointer_active = false;
                self.camera_pointer_active = false;
                self.ui_pointer_active = false;
                return;
            }
            if state == ElementState::Released && self.scene_pointer_active {
                self.scene_pointer_active = false;
                return;
            }
        }
        let morph_preview = self
            .shell
            .as_ref()
            .is_some_and(StudioShell::is_morphs_workspace);
        match state {
            ElementState::Pressed => {
                let Some((local_x, local_y)) = self.runtime_pointer(x, y, true) else {
                    return;
                };
                let camera_side = morph_preview
                    && self
                        .shell
                        .as_ref()
                        .is_some_and(|shell| local_x >= shell.runtime_viewport().width() * 0.5);
                match button {
                    MouseButton::Left if self.review_navigation_active() => {
                        // Game HUDs may claim the entire left viewport for a
                        // joystick. Review drags belong to the editor camera,
                        // including while the simulation is stopped.
                        self.pointer_active = true;
                        self.camera_pointer_active = false;
                        self.ui_pointer_active = false;
                    }
                    MouseButton::Left if morph_preview && camera_side => {
                        self.camera_pointer_active = true;
                        self.pointer_active = false;
                        self.movement_pointer_active = false;
                        self.movement_pointer_origin = None;
                        self.joystick_input = (0.0, 0.0);
                        self.ui_pointer_active = false;
                    }
                    MouseButton::Left if morph_preview => {
                        self.movement_pointer_active = true;
                        self.movement_pointer_origin = Some((x, y));
                        self.joystick_input = (0.0, 0.0);
                        self.pointer_active = false;
                        self.ui_pointer_active = false;
                    }
                    MouseButton::Left => {
                        self.ui_pointer_active = self.pointer_event(0, local_x, local_y);
                        self.pointer_active = !self.ui_pointer_active;
                    }
                    MouseButton::Right => {
                        self.camera_pointer_active = true;
                        self.pointer_active = false;
                    }
                    _ => {}
                }
            }
            ElementState::Released => match button {
                MouseButton::Left if self.movement_pointer_active => {
                    self.movement_pointer_active = false;
                    self.movement_pointer_origin = None;
                    self.joystick_input = (0.0, 0.0);
                }
                MouseButton::Left if morph_preview && self.camera_pointer_active => {
                    self.camera_pointer_active = false;
                }
                MouseButton::Left => {
                    if self.ui_pointer_active {
                        if let Some((local_x, local_y)) = self.runtime_pointer(x, y, false) {
                            self.pointer_event(2, local_x, local_y);
                        }
                    }
                    self.ui_pointer_active = false;
                    self.pointer_active = false;
                }
                MouseButton::Right => self.camera_pointer_active = false,
                _ => {}
            },
        }
    }
}

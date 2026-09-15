use super::*;

impl ApplicationHandler for StudioApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        #[cfg(target_os = "macos")]
        macos::install_native_menu();
        if let Err(error) = self.create_window(event_loop) {
            eprintln!("Cubacadabra Studio: {error}");
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let shell_consumed = match (&mut self.shell, &self.window) {
            (Some(shell), Some(window)) => shell.on_window_event(window, &event),
            _ => false,
        };
        // Once Play is active, the game owns its keyboard controls even if
        // egui still reports that it wants keyboard input. This can happen
        // after the editor's search field or another shell control had focus;
        // letting that stale focus consume W/A/S/D, arrows, or Shift makes the
        // running game appear completely unresponsive. Enter and Space stay
        // with a focused editor control so typing cannot trigger gameplay.
        let playing = self
            .shell
            .as_ref()
            .is_some_and(|shell| shell.is_playing() && !shell.is_project_loading());
        let runtime_hovered = self
            .pointer_position
            .is_some_and(|(x, y)| self.runtime_pointer(x, y, true).is_some());
        match event {
            WindowEvent::CloseRequested => {
                if let Some(shell) = &mut self.shell {
                    if shell.project_is_dirty() {
                        shell.request_close();
                        self.request_redraw();
                    } else {
                        event_loop.exit();
                    }
                } else {
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(size) => {
                self.clear_pointer_controls();
                self.resize(size);
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.clear_pointer_controls();
                self.resize(
                    self.window
                        .as_ref()
                        .map_or(PhysicalSize::new(0, 0), Window::inner_size),
                );
            }
            WindowEvent::RedrawRequested => {
                self.render();
                if self
                    .shell
                    .as_mut()
                    .is_some_and(StudioShell::take_exit_requested)
                {
                    event_loop.exit();
                    return;
                }
                self.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. }
                if matches!(
                    event.physical_key,
                    PhysicalKey::Code(code)
                        if should_forward_gameplay_key(playing, shell_consumed, code)
                ) =>
            {
                self.handle_key(&event, event_loop)
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_move(position.x, position.y)
            }
            WindowEvent::MouseInput { state, button, .. }
                if playing
                    || runtime_hovered
                    || !shell_consumed
                    || state == ElementState::Released =>
            {
                self.handle_mouse_button(state, button)
            }
            WindowEvent::MouseWheel { delta, .. } if runtime_hovered => {
                self.zoom_delta += match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 0.9,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / 100.0,
                };
            }
            WindowEvent::Focused(false) => {
                self.pressed_keys.clear();
                self.clear_pointer_controls();
            }
            _ => {}
        }
    }
}

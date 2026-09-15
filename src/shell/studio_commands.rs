use super::*;
use egui_wgpu::wgpu;
impl StudioShell {
    pub(crate) fn execute_command(&mut self, command: StudioCommand) {
        if self.project_loading.is_some() {
            return;
        }
        match command {
            StudioCommand::NewProject => {
                self.new_project_dialog_open = true;
                self.new_project_title.clear();
                self.new_project_error = None;
                #[cfg(not(target_os = "macos"))]
                {
                    self.new_project_title_focus_requested = true;
                }
            }
            StudioCommand::OpenProject => {
                self.open_project_requested = true;
                self.notice = "Choose a project folder…".to_owned();
            }
            StudioCommand::Save => {
                if self.project_editable {
                    self.save_requested = true;
                    self.notice = "Saving project…".to_owned();
                } else {
                    self.notice = "This preview is read-only".to_owned();
                }
            }
            StudioCommand::RevealProject => {
                self.notice = "Reveal Project is not connected yet".to_owned();
            }
            StudioCommand::Copy => {
                self.state.egui_input_mut().events.push(egui::Event::Copy);
            }
            StudioCommand::Preferences => {
                self.notice = "Preferences are coming later".to_owned();
            }
            StudioCommand::MaximizeViewport => {
                self.notice = "Viewport maximize is coming later".to_owned();
            }
            StudioCommand::ResetLayout => {
                self.notice = "Layout reset".to_owned();
            }
            StudioCommand::ShowWorld => self.select_workspace(Workspace::World),
            StudioCommand::ShowScripts => self.select_workspace(Workspace::Scripts),
            StudioCommand::ShowAssets => self.select_workspace(Workspace::Assets),
            StudioCommand::ShowMaterials => self.select_workspace(Workspace::Materials),
            StudioCommand::ShowMorphs => self.select_workspace(Workspace::Morphs),
            StudioCommand::ShowTest => self.select_workspace(Workspace::Test),
        }
    }

    pub(crate) fn select_workspace(&mut self, workspace: Workspace) {
        self.workspace = workspace;
        self.notice = format!("{} workspace", workspace.label());
    }

    pub(crate) fn prepare(&mut self, window: &Window, project_name: &str) -> PreparedShell {
        let input = self.state.take_egui_input(window);
        let context = self.context.clone();
        let output = context.run_ui(input, |ui| self.show(ui, project_name));
        self.state
            .handle_platform_output(window, output.platform_output);
        let pixels_per_point = context.pixels_per_point();
        let paint_jobs = context.tessellate(output.shapes, pixels_per_point);
        let size = window.inner_size();
        self.pending_textures_delta.append(output.textures_delta);
        PreparedShell {
            paint_jobs,
            screen: ScreenDescriptor {
                size_in_pixels: [size.width, size.height],
                pixels_per_point,
            },
        }
    }

    pub(crate) fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        destination: &wgpu::TextureView,
        prepared: PreparedShell,
    ) {
        let textures_delta = std::mem::take(&mut self.pending_textures_delta);
        for (id, image_delta) in &textures_delta.set {
            self.renderer
                .update_texture(device, queue, *id, image_delta);
        }
        let command_buffers = self.renderer.update_buffers(
            device,
            queue,
            encoder,
            &prepared.paint_jobs,
            &prepared.screen,
        );
        debug_assert!(command_buffers.is_empty());
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Cubacadabra Studio shell"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: destination,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.renderer.render(
            &mut pass.forget_lifetime(),
            &prepared.paint_jobs,
            &prepared.screen,
        );
        for id in &textures_delta.free {
            self.renderer.free_texture(id);
        }
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui, project_name: &str) {
        self.runtime_viewport = Rect::NOTHING;
        self.show_top_bar(ui, project_name);
        if self.start_screen {
            self.show_start_screen(ui);
        } else {
            self.show_status_bar(ui);
            if self.codex_chat_open {
                self.show_codex_chat(ui);
            }
            match self.workspace {
                Workspace::World => self.show_world(ui),
                Workspace::Scripts => self.show_scripts(ui),
                Workspace::Assets => self.show_assets(ui),
                Workspace::Materials => self.show_materials(ui),
                Workspace::Morphs => self.show_morphs(ui),
                Workspace::Test => self.show_test(ui),
            }
        }
        #[cfg(not(target_os = "macos"))]
        self.show_new_project_dialog(ui.ctx());
        self.show_project_loading(ui.ctx());
        self.show_project_error(ui.ctx());
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    pub(crate) fn show_project_loading(&self, context: &egui::Context) {
        let Some(loading) = &self.project_loading else {
            return;
        };
        let colors = if context.style_of(context.theme()).visuals.dark_mode {
            DARK_PALETTE
        } else {
            LIGHT_PALETTE
        };
        egui::Modal::new(egui::Id::new("project_loading"))
            .backdrop_color(Color32::from_black_alpha(120))
            .frame(
                Frame::NONE
                    .fill(colors.panel_raised)
                    .stroke(Stroke::new(1.0, colors.border_strong))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(20)),
            )
            .show(context, |ui| {
                ui.set_width(280.0);
                ui.label(
                    RichText::new(if self.is_rebuilding_project() {
                        "Rebuilding preview..."
                    } else {
                        "Loading project..."
                    })
                    .font(semibold_font(TYPE.primary))
                    .color(colors.text),
                );
                ui.add_space(10.0);
                ui.add(
                    egui::ProgressBar::new(loading.progress)
                        .desired_width(ui.available_width())
                        .show_percentage(),
                );
            });
    }

    pub(crate) fn show_project_error(&mut self, context: &egui::Context) {
        let Some(mut error) = self.project_error.clone() else {
            return;
        };
        let colors = if context.style_of(context.theme()).visuals.dark_mode {
            DARK_PALETTE
        } else {
            LIGHT_PALETTE
        };
        egui::Window::new("Preview build failed")
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .frame(
                Frame::NONE
                    .fill(colors.panel_raised)
                    .stroke(Stroke::new(1.0, colors.axis_x))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(14)),
            )
            .show(context, |ui| {
                ui.label(
                    RichText::new("The last working preview is still running.")
                        .size(TYPE.secondary)
                        .color(colors.text),
                );
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::multiline(&mut error)
                        .desired_rows(7)
                        .interactive(false)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);
                if ui.button("Dismiss").clicked() {
                    self.project_error = None;
                }
            });
    }
}

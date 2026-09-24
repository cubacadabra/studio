use super::*;
use egui_wgpu::wgpu;
impl StudioShell {
    pub(crate) fn execute_command(&mut self, command: StudioCommand) {
        if self.project_loading.is_some() {
            return;
        }
        match command {
            StudioCommand::ShowAbout => {
                self.about_open = true;
            }
            StudioCommand::NewProject => {
                if self.project_dirty {
                    self.pending_project_action = Some(PendingProjectAction::NewProject);
                    self.notice =
                        "Unsaved changes — save or discard them before continuing".to_owned();
                    return;
                }
                self.new_project_dialog_open = true;
                self.new_project_title.clear();
                self.new_project_parent = crate::default_new_project_parent();
                self.new_project_error = None;
                #[cfg(not(target_os = "macos"))]
                {
                    self.new_project_title_focus_requested = true;
                }
            }
            StudioCommand::OpenProject => {
                if self.project_dirty {
                    self.pending_project_action = Some(PendingProjectAction::OpenProject);
                    self.notice =
                        "Unsaved changes — save or discard them before continuing".to_owned();
                    return;
                }
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
            StudioCommand::ImportRobloxPlace => {
                if self.project_editable {
                    self.roblox_import_requested = true;
                    self.notice = "Choose a Roblox XML place to import…".to_owned();
                } else {
                    self.notice =
                        "Open a source project before importing a Roblox place".to_owned();
                }
            }
            StudioCommand::ExportRobloxPlace => {
                if self.authoring_scene_source.is_some() {
                    self.roblox_export_requested = true;
                    self.notice = "Choose where to export the Roblox place…".to_owned();
                } else {
                    self.notice = "This project has no authoring scene to export".to_owned();
                }
            }
            StudioCommand::Undo => self.undo_requested = true,
            StudioCommand::Redo => self.redo_requested = true,
            StudioCommand::Duplicate => {
                if self.editor_shortcuts_active()
                    && let Some(selected) = self.selected_scene_object_geometry()
                {
                    self.scene_edit_requested = Some(SceneEditRequest::DuplicateObject {
                        target: selected.id,
                    });
                    self.notice = "Duplicating selection…".to_owned();
                }
            }
            StudioCommand::CloseWindow => self.request_close(),
            StudioCommand::Copy => {
                self.state.egui_input_mut().events.push(egui::Event::Copy);
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

    pub(crate) fn set_about_preview_texture(
        &mut self,
        device: &wgpu::Device,
        texture: &wgpu::TextureView,
    ) {
        self.about_texture = Some(self.renderer.register_native_texture(
            device,
            texture,
            wgpu::FilterMode::Linear,
        ));
    }

    pub(crate) const fn about_is_open(&self) -> bool {
        self.about_open
    }

    pub(crate) fn prepare(&mut self, window: &Window, project_name: &str) -> PreparedShell {
        let input = self.state.take_egui_input(window);
        let context = self.context.clone();
        let ui_started = Instant::now();
        let output = context.run_ui(input, |ui| self.show(ui, project_name));
        let ui_build_ms = ui_started.elapsed().as_secs_f32() * 1_000.0;
        self.state
            .handle_platform_output(window, output.platform_output);
        let pixels_per_point = context.pixels_per_point();
        let tessellate_started = Instant::now();
        let paint_jobs = context.tessellate(output.shapes, pixels_per_point);
        let ui_tessellate_ms = tessellate_started.elapsed().as_secs_f32() * 1_000.0;
        let size = window.inner_size();
        self.pending_textures_delta.append(output.textures_delta);
        PreparedShell {
            performance: PerformanceSample {
                ui_build_ms,
                ui_tessellate_ms,
                tree_rows: self.scene_tree_rows.len(),
                scene_objects: self.scene_outline.placeable_object_geometries().len(),
                egui_primitives: paint_jobs.len(),
                playing: self.playing,
                workspace: self.workspace,
                ..PerformanceSample::default()
            },
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
        let paint_started = Instant::now();
        let mut performance = prepared.performance;
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
        performance.overlay_paint_ms = paint_started.elapsed().as_secs_f32() * 1_000.0;
        self.performance_pending = Some(performance);
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
        self.show_unsaved_changes(ui.ctx());
        self.show_about(ui.ctx());
        self.show_add_palette(ui.ctx());
        self.show_performance_monitor(ui.ctx());
        ui.ctx().request_repaint_after(if self.about_open {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(16)
        });
    }

    fn show_about(&mut self, context: &egui::Context) {
        if !self.about_open {
            return;
        }
        let colors = if context.style_of(context.theme()).visuals.dark_mode {
            DARK_PALETTE
        } else {
            LIGHT_PALETTE
        };
        let about_texture = self.about_texture;
        let logo_texture = self.logo_texture.clone();
        let mut close_requested = false;
        let response = egui::Modal::new(egui::Id::new("about_cubacadabra"))
            .backdrop_color(Color32::from_black_alpha(150))
            .frame(
                Frame::NONE
                    .fill(colors.panel_raised)
                    .stroke(Stroke::new(1.0, colors.border_strong))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(20)),
            )
            .show(context, |ui| {
                ui.set_width(520.0_f32.min(ui.available_width()));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Image::from_texture(&logo_texture)
                            .fit_to_exact_size(egui::vec2(28.0, 28.0)),
                    );
                    ui.label(
                        RichText::new("About Cubacadabra")
                            .font(semibold_font(20.0))
                            .color(colors.text),
                    );
                });
                ui.add_space(14.0);
                if let Some(texture) = about_texture {
                    let width = ui.available_width().min(512.0);
                    ui.add(
                        egui::Image::from_texture((texture, egui::vec2(width, width * 9.0 / 16.0)))
                            .fit_to_exact_size(egui::vec2(width, width * 9.0 / 16.0)),
                    );
                }
                ui.add_space(12.0);
                ui.label(
                    RichText::new(format!(
                        "Cubacadabra Studio {} ({})",
                        env!("CARGO_PKG_VERSION"),
                        env!("CUBACADABRA_GIT_SHA")
                    ))
                    .font(medium_font(TYPE.secondary))
                    .color(colors.secondary_text),
                );
                ui.label(
                    RichText::new("An open-source creator tool for building worlds.")
                        .size(TYPE.secondary)
                        .color(colors.muted),
                );
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        close_requested = true;
                    }
                });
            });
        if close_requested || response.should_close() {
            self.about_open = false;
        }
    }

    fn show_unsaved_changes(&mut self, context: &egui::Context) {
        if self.pending_project_action.is_none() {
            return;
        }
        let colors = if context.style_of(context.theme()).visuals.dark_mode {
            DARK_PALETTE
        } else {
            LIGHT_PALETTE
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::new("unsaved_project_changes"))
            .backdrop_color(Color32::from_black_alpha(128))
            .frame(
                Frame::NONE
                    .fill(colors.panel_raised)
                    .stroke(Stroke::new(1.0, colors.border_strong))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(20)),
            )
            .show(context, |ui| {
                ui.set_width(360.0);
                ui.label(
                    RichText::new("Unsaved changes")
                        .font(semibold_font(18.0))
                        .color(colors.text),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Save your project before continuing?")
                        .size(TYPE.secondary)
                        .color(colors.secondary_text),
                );
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        choice = Some(0);
                    }
                    if ui.button("Discard").clicked() {
                        choice = Some(1);
                    }
                    if ui
                        .add_enabled(self.project_editable, egui::Button::new("Save"))
                        .clicked()
                    {
                        choice = Some(2);
                    }
                });
            });
        match choice {
            Some(0) => self.pending_project_action = None,
            Some(1) => {
                if let Some(action) = self.discard_pending_project_action() {
                    for path in self.take_imported_assets_for_discard() {
                        if let Err(error) = fs::remove_file(&path)
                            && error.kind() != std::io::ErrorKind::NotFound
                        {
                            log::warn!(
                                "could not remove discarded imported asset {}: {error}",
                                path.display()
                            );
                        }
                    }
                    self.apply_pending_project_action(action);
                }
            }
            Some(2) => {
                self.save_requested = true;
                self.notice = "Saving project…".to_owned();
            }
            _ => {}
        }
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

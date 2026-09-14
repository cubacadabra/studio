use super::*;
impl StudioShell {
    pub(crate) fn scene_tree(&mut self, ui: &mut egui::Ui) {
        if self.project_loading.is_some() {
            return;
        }
        panel_header(ui, Icon::World, "Scene", |ui| {
            icon_button(ui, Icon::Filter, "Filter scene", false);
        });
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::NONE
                    .inner_margin(Margin::symmetric(0, 4))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        show_scene_node(
                            ui,
                            &self.scene_outline.root,
                            0,
                            &mut self.expanded_scene,
                            &mut self.selected_scene,
                        );
                    });
            });
    }

    pub(crate) fn inspector(&mut self, ui: &mut egui::Ui) {
        panel_header(ui, Icon::Sliders, "Inspector", |ui| {
            icon_button(ui, Icon::Lock, "Lock inspector", false);
            icon_button(ui, Icon::More, "Inspector options", false);
        });
        let selected = self
            .scene_outline
            .root
            .find(&self.selected_scene)
            .cloned()
            .unwrap_or_else(|| self.scene_outline.root.clone());
        content_frame().show(ui, |ui| {
            selected_object_header(ui, &selected.label, selected.kind);
            ui.add_space(2.0);
            if selected.kind == "Block" {
                self.block_inspector(ui, &selected);
            } else if selected.kind == "Sign" {
                self.sign_inspector(ui, &selected);
            } else if selected.properties.is_empty() {
                ui.label(
                    RichText::new("No properties")
                        .size(TYPE.secondary)
                        .color(palette(ui).muted),
                );
            } else {
                property_section(ui, "Manifest", |ui| {
                    for (label, value) in &selected.properties {
                        property_row(ui, label, value);
                    }
                });
            }
        });
    }

    pub(crate) fn block_inspector(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        let colors = palette(ui);
        if self.scene_editor_target != selected.id {
            self.scene_editor_target = selected.id.clone();
            self.scene_editor_position = vector_property(selected, "Position").unwrap_or([0.0; 3]);
            self.scene_editor_size = vector_property(selected, "Size").unwrap_or([1.0; 3]);
        }

        property_section(ui, "Transform", |ui| {
            let position_changed =
                vector_editor(ui, "Position", &mut self.scene_editor_position, 0.1);
            let size_changed = vector_editor(ui, "Size", &mut self.scene_editor_size, 0.1);
            if position_changed || size_changed {
                self.project_dirty = true;
                self.project_error = None;
                self.scene_edit_requested = Some(SceneEditRequest::UpdateBlock {
                    target: selected.id.clone(),
                    position: self.scene_editor_position,
                    size: self.scene_editor_size,
                });
                self.notice = "Platform changed — save to keep it".to_owned();
            }
        });
        property_section(ui, "Actions", |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.project_editable, egui::Button::new("Duplicate"))
                    .on_disabled_hover_text("Open a raw source project to edit the scene")
                    .clicked()
                {
                    self.scene_edit_requested = Some(SceneEditRequest::DuplicateBlock {
                        target: selected.id.clone(),
                    });
                    self.notice = "Duplicating platform…".to_owned();
                }
                if ui
                    .add_enabled(self.project_editable, egui::Button::new("Delete"))
                    .on_disabled_hover_text("Open a raw source project to edit the scene")
                    .clicked()
                {
                    self.scene_edit_requested = Some(SceneEditRequest::DeleteBlock {
                        target: selected.id.clone(),
                    });
                    self.notice = "Deleting platform…".to_owned();
                }
            });
        });
        property_section(ui, "Manifest", |ui| {
            for (label, value) in &selected.properties {
                if label != "Position" && label != "Size" {
                    property_row(ui, label, value);
                }
            }
        });
        if !self.project_editable {
            ui.label(
                RichText::new("Open a raw source project to edit scene objects.")
                    .size(TYPE.meta)
                    .color(colors.muted),
            );
        }
    }

    pub(crate) fn sign_inspector(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        let colors = palette(ui);
        if self.scene_editor_target != selected.id {
            self.scene_editor_target = selected.id.clone();
            self.scene_editor_text = selected
                .properties
                .iter()
                .find(|(label, _)| label == "Text")
                .map(|(_, value)| value.clone())
                .unwrap_or_default();
        }

        property_section(ui, "Content", |ui| {
            ui.label(
                RichText::new("Text")
                    .size(TYPE.secondary)
                    .color(colors.secondary_text),
            );
            if ui
                .add(
                    egui::TextEdit::multiline(&mut self.scene_editor_text)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .hint_text("Sign text"),
                )
                .changed()
            {
                self.project_dirty = true;
                self.project_error = None;
                self.scene_edit_requested = Some(SceneEditRequest::UpdateSignText {
                    target: selected.id.clone(),
                    text: self.scene_editor_text.clone(),
                });
                self.notice = "Sign text changed — save to keep it".to_owned();
            }
        });
        ui.label(
            RichText::new("Save, then Rebuild & Play to see the updated sign in the game.")
                .size(TYPE.meta)
                .color(colors.muted),
        );
    }

    pub(crate) fn asset_shelf(&mut self, ui: &mut egui::Ui) {
        let colors = palette(ui);
        editor_header(ui, |ui| {
            inline_icon(ui, Icon::Assets, colors.muted);
            ui.label(
                RichText::new("Assets")
                    .font(semibold_font(TYPE.primary))
                    .color(colors.text),
            );
            vertical_separator(ui, 12.0);
            ui.spacing_mut().item_spacing.x = 0.0;
            for filter in ["All", "Images", "Materials", "Characters"] {
                if compact_tab(ui, filter, self.asset_filter == filter).clicked() {
                    self.asset_filter = filter;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                icon_button(ui, Icon::Grid, "Grid view", true);
                search_field(ui, &mut self.search_query, 190.0);
            });
        });
        Frame::NONE
            .inner_margin(Margin::symmetric(8, 8))
            .show(ui, |ui| {
                let query = self.search_query.trim().to_lowercase();
                let assets = self
                    .scene_outline
                    .assets
                    .iter()
                    .filter(|asset| match self.asset_filter {
                        "Images" => asset.kind == "IMAGE",
                        "Materials" => asset.kind == "MATERIAL",
                        "Characters" => asset.kind == "CHARACTER",
                        _ => true,
                    })
                    .filter(|asset| query.is_empty() || asset.name.to_lowercase().contains(&query))
                    .cloned()
                    .collect::<Vec<_>>();
                ui.horizontal_wrapped(|ui| {
                    if assets.is_empty() {
                        ui.label(
                            RichText::new(if query.is_empty() {
                                "No assets in this game."
                            } else {
                                "No matching assets."
                            })
                            .size(TYPE.secondary)
                            .color(colors.muted),
                        );
                    }
                    for asset in assets {
                        if asset_tile(
                            ui,
                            &asset.name,
                            asset.icon,
                            asset.kind,
                            self.selected_world_asset == asset.name,
                        )
                        .clicked()
                        {
                            self.selected_world_asset = asset.name;
                        }
                    }
                });
            });
    }
}

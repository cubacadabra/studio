use super::*;
impl StudioShell {
    pub(crate) fn scene_tree(&mut self, ui: &mut egui::Ui) {
        if self.project_loading.is_some() {
            return;
        }
        let previous_selection = self.selected_scene.clone();
        let previous_selections = self.selected_scenes.clone();
        let mut add_requested = false;
        panel_header(ui, Icon::World, "Scene", |ui| {
            if self.project_editable {
                add_requested = ui
                    .add_enabled(!self.playing, egui::Button::new("Add"))
                    .on_hover_text(if self.playing {
                        "Stop Preview to add objects"
                    } else {
                        "Add object (Shift+A)"
                    })
                    .clicked();
            } else {
                ui.label(
                    RichText::new("Read-only")
                        .size(TYPE.meta)
                        .color(palette(ui).muted),
                )
                .on_hover_text("Open a raw source project to edit the scene");
            }
        });
        if add_requested {
            let world_id = scene_world_id(&self.selected_scene).map(str::to_owned);
            self.open_add_palette(world_id);
        }
        search_field_with_hint(
            ui,
            &mut self.scene_search_query,
            ui.available_width(),
            "Filter scene…",
            "Filter scene tree",
        );
        ui.add_space(4.0);
        if self.scene_search_matches_query != self.scene_search_query {
            self.scene_search_matches_query = self.scene_search_query.clone();
            self.scene_search_matches = self.scene_outline.search_matches(&self.scene_search_query);
            self.scene_search_result_count = self
                .scene_outline
                .search_result_count(&self.scene_search_query);
            self.scene_tree_rows_dirty = true;
        }
        let scene_filter = self.scene_search_query.trim();
        if !scene_filter.is_empty() {
            let result_count = self.scene_search_result_count;
            let label = if result_count == 1 {
                "1 matching item"
            } else {
                "matching items"
            };
            ui.label(
                RichText::new(if result_count == 1 {
                    label.to_owned()
                } else {
                    format!("{result_count} {label}")
                })
                .size(TYPE.meta)
                .color(palette(ui).muted),
            );
            ui.add_space(2.0);
        }
        if self.scene_tree_rows_dirty {
            self.scene_tree_rows.clear();
            flatten_scene_rows(
                &self.scene_outline.root,
                0,
                &self.expanded_scene,
                (!scene_filter.is_empty()).then_some(&self.scene_search_matches),
                &mut self.scene_tree_rows,
            );
            self.scene_tree_rows_dirty = false;
        }
        let expanded_before = self.expanded_scene.clone();
        if self.scene_tree_rows.is_empty() {
            ui.add_space(12.0);
            ui.label(
                RichText::new("No scene items match this filter.")
                    .size(TYPE.secondary)
                    .color(palette(ui).muted),
            );
        } else {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, UI.row, self.scene_tree_rows.len(), |ui, row_range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.spacing_mut().interact_size.y = UI.row;
                    for index in row_range {
                        let row = self.scene_tree_rows[index].clone();
                        show_scene_row(
                            ui,
                            &row,
                            &mut self.expanded_scene,
                            &mut self.selected_scene,
                            &mut self.selected_scenes,
                            self.project_editable && !self.playing,
                            &mut self.scene_edit_requested,
                            &mut self.add_palette,
                        );
                    }
                });
        }
        if self.expanded_scene != expanded_before {
            self.scene_tree_rows_dirty = true;
        }
        if self.selected_scenes != previous_selections {
            self.scene_editor_target.clear();
        }
        if self.selected_scenes.len() > 1 {
            self.notice = format!("{} objects selected", self.selected_scenes.len());
        } else if self.selected_scene != previous_selection
            && let Some(selected) = self.scene_outline.root.find(&self.selected_scene)
        {
            self.notice = format!("Selected {}", selected.label);
        }
    }

    pub(crate) fn inspector(&mut self, ui: &mut egui::Ui) {
        panel_header(ui, Icon::Sliders, "Inspector", |_| {});
        if self.selected_scenes.len() > 1 {
            self.multi_selection_inspector(ui);
            return;
        }
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
            } else if self.project_editable
                && (is_scene_object(&selected.id) || is_authoring_node(&selected))
                && vector_property(&selected, "Position").is_some()
            {
                self.scene_object_inspector(ui, &selected);
            } else if selected.properties.is_empty() {
                ui.label(
                    RichText::new(if selected.kind == "Collection" {
                        "Select an item in this group to inspect it."
                    } else {
                        "No editable properties"
                    })
                    .size(TYPE.secondary)
                    .color(palette(ui).muted),
                );
            } else {
                property_section(ui, "Manifest", |ui| {
                    for (label, value) in &selected.properties {
                        property_row(ui, label, value);
                    }
                });
                ui.label(
                    RichText::new("Manifest data is read-only in Studio.")
                        .size(TYPE.meta)
                        .color(palette(ui).muted),
                );
            }
            if self.project_editable
                && (is_scene_object(&selected.id)
                    || (is_authoring_node(&selected) && scene_parent_id(&selected).is_some()))
            {
                self.scene_object_actions(ui, &selected);
            }
        });
    }

    fn multi_selection_inspector(&mut self, ui: &mut egui::Ui) {
        let selected = self
            .selected_scenes
            .iter()
            .filter_map(|id| self.scene_outline.root.find(id))
            .cloned()
            .collect::<Vec<_>>();
        content_frame().show(ui, |ui| {
            selected_object_header(ui, &format!("{} objects", selected.len()), "Selection");
            property_section(ui, "Selected", |ui| {
                for node in selected.iter().take(8) {
                    ui.horizontal(|ui| {
                        inline_icon(ui, node.icon, palette(ui).faint);
                        ui.label(
                            RichText::new(&node.label)
                                .size(TYPE.secondary)
                                .color(palette(ui).secondary_text),
                        );
                    });
                }
                if selected.len() > 8 {
                    ui.label(
                        RichText::new(format!("and {} more", selected.len() - 8))
                            .size(TYPE.meta)
                            .color(palette(ui).muted),
                    );
                }
            });
            let targets = selected
                .iter()
                .filter(|node| is_authoring_node(node) && scene_parent_id(node).is_some())
                .map(|node| node.id.clone())
                .collect::<Vec<_>>();
            if self.project_editable && !self.playing && !targets.is_empty() {
                property_section(ui, "Actions", |ui| {
                    if ui.button("Group selection").clicked() {
                        self.scene_edit_requested =
                            Some(SceneEditRequest::GroupObjects { targets });
                        self.notice = "Grouping selection…".to_owned();
                    }
                    ui.label(
                        RichText::new("Drag any selected row onto a Group to move the selection.")
                            .size(TYPE.meta)
                            .color(palette(ui).muted),
                    );
                });
            }
        });
    }

    pub(crate) fn block_inspector(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        let colors = palette(ui);
        self.sync_scene_editor(selected);
        self.scene_identity_editor(ui, selected);
        self.scene_transform_editor(ui, selected, true, false, false, false);
        self.block_appearance_editor(ui, selected);
        self.scene_manifest_properties(
            ui,
            selected,
            &[
                "Name", "Position", "Rotation", "Size", "Scale", "Color", "Material",
            ],
        );
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
        self.sync_scene_editor(selected);
        self.scene_identity_editor(ui, selected);
        self.scene_transform_editor(ui, selected, false, false, false, false);

        property_section(ui, "Content", |ui| {
            ui.label(
                RichText::new("Text")
                    .size(TYPE.secondary)
                    .color(colors.secondary_text),
            );
            if ui
                .add_enabled(
                    self.project_editable,
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
        self.scene_manifest_properties(ui, selected, &["Name", "Position", "Rotation", "Text"]);
        ui.label(
            RichText::new(if self.project_editable {
                "Press Play to see the updated sign in the game."
            } else {
                "Open a raw source project to edit this sign."
            })
            .size(TYPE.meta)
            .color(colors.muted),
        );
    }

    fn scene_object_inspector(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        self.sync_scene_editor(selected);
        self.scene_identity_editor(ui, selected);
        let authoring_node = is_authoring_node(selected);
        let has_primitive_size = selected.kind == "Block" && authoring_node;
        let has_size =
            vector_property(selected, "Size").is_some() && (!authoring_node || has_primitive_size);
        let actor = selected.kind == "Actor";
        let has_scale = authoring_node && !has_primitive_size && !actor;
        self.scene_transform_editor(ui, selected, has_size, has_scale, has_primitive_size, actor);
        self.scene_manifest_properties(ui, selected, &["Name", "Position", "Rotation", "Scale"]);
    }

    fn sync_scene_editor(&mut self, selected: &SceneNode) {
        if self.scene_editor_target == selected.id {
            return;
        }
        self.scene_editor_target = selected.id.clone();
        self.scene_editor_name = selected.label.clone();
        self.scene_editor_position = vector_property(selected, "Position").unwrap_or([0.0; 3]);
        self.scene_editor_rotation = vector_property(selected, "Rotation")
            .unwrap_or([0.0; 3])
            .map(f32::to_degrees);
        self.scene_editor_size = vector_property(selected, "Size").unwrap_or([1.0; 3]);
        self.scene_editor_scale = vector_property(selected, "Scale").unwrap_or([1.0; 3]);
        self.scene_editor_position_text = scene_vector_text(self.scene_editor_position);
        self.scene_editor_rotation_text = scene_vector_text(self.scene_editor_rotation);
        self.scene_editor_size_text = scene_vector_text(self.scene_editor_size);
        self.scene_editor_scale_text = scene_vector_text(self.scene_editor_scale);
        self.scene_editor_text = selected
            .properties
            .iter()
            .find(|(label, _)| label == "Text")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        self.scene_editor_properties = selected.properties.iter().cloned().collect();
    }

    fn scene_identity_editor(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        if !is_authoring_node(selected) {
            return;
        }
        property_section(ui, "Identity", |ui| {
            ui.label(
                RichText::new("Name")
                    .size(TYPE.secondary)
                    .color(palette(ui).secondary_text),
            );
            let response = ui.add_enabled(
                self.project_editable && !self.playing && !scene_node_locked(selected),
                egui::TextEdit::singleline(&mut self.scene_editor_name)
                    .desired_width(f32::INFINITY)
                    .hint_text("Scene object name"),
            );
            if response.lost_focus()
                && self.scene_editor_name.trim() != selected.label
                && !self.scene_editor_name.trim().is_empty()
            {
                self.scene_edit_requested = Some(SceneEditRequest::RenameObject {
                    target: selected.id.clone(),
                    name: self.scene_editor_name.trim().to_owned(),
                });
                self.notice = "Renaming scene object…".to_owned();
            }
            property_row(ui, "Stable ID", &selected.id);
        });
    }

    fn block_appearance_editor(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        const COLORS: [(&str, &str, Color32); 8] = [
            ("Cloud", "#E8EEF8", Color32::from_rgb(232, 238, 248)),
            ("Stone", "#767F91", Color32::from_rgb(118, 127, 145)),
            ("Charcoal", "#242A36", Color32::from_rgb(36, 42, 54)),
            ("Grass", "#62A85A", Color32::from_rgb(98, 168, 90)),
            ("Wood", "#9B6947", Color32::from_rgb(155, 105, 71)),
            ("Signal", "#F05252", Color32::from_rgb(240, 82, 82)),
            ("Magic", "#B04CFF", Color32::from_rgb(176, 76, 255)),
            ("Gold", "#F4C95D", Color32::from_rgb(244, 201, 93)),
        ];
        const MATERIALS: [(&str, &str); 6] = [
            ("Grass", "builtin:grass"),
            ("Ground", "builtin:ground"),
            ("Stone", "builtin:rock"),
            ("Sand", "builtin:sand"),
            ("Snow", "builtin:snow"),
            ("Leafy grass", "builtin:leafygrass"),
        ];
        let current_color = scene_property(selected, "Color").unwrap_or("signal");
        let current_material = scene_property(selected, "Material");
        let enabled = self.project_editable && !self.playing && !scene_node_locked(selected);
        property_section(ui, "Appearance", |ui| {
            ui.label(
                RichText::new("Color")
                    .size(TYPE.secondary)
                    .color(palette(ui).secondary_text),
            );
            ui.add_enabled_ui(enabled, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
                    for (label, value, color) in COLORS {
                        let selected_color = current_color.eq_ignore_ascii_case(value);
                        let response = ui
                            .add_sized(
                                [28.0, 28.0],
                                egui::Button::new(if selected_color { "✓" } else { "" })
                                    .fill(color)
                                    .stroke(Stroke::new(
                                        if selected_color { 2.0 } else { 1.0 },
                                        if selected_color {
                                            palette(ui).text
                                        } else {
                                            palette(ui).border_strong
                                        },
                                    )),
                            )
                            .on_hover_text(label);
                        if response.clicked() {
                            self.scene_edit_requested = Some(SceneEditRequest::UpdateProperty {
                                target: selected.id.clone(),
                                key: "primitive.color".to_owned(),
                                value: Value::String(value.to_owned()),
                            });
                            self.notice = format!("{label} color applied — save to keep it");
                        }
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Material")
                    .size(TYPE.secondary)
                    .color(palette(ui).secondary_text),
            );
            let material_label = current_material
                .and_then(|value| {
                    MATERIALS
                        .iter()
                        .find(|(_, material)| value == *material)
                        .map(|(label, _)| *label)
                })
                .unwrap_or_else(|| current_material.unwrap_or("Color only"));
            ui.add_enabled_ui(enabled, |ui| {
                egui::ComboBox::from_id_salt(("block-material", &selected.id))
                    .selected_text(material_label)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(current_material.is_none(), "Color only")
                            .clicked()
                            && current_material.is_some()
                        {
                            self.scene_edit_requested = Some(SceneEditRequest::RemoveProperty {
                                target: selected.id.clone(),
                                key: "primitive.material".to_owned(),
                            });
                        }
                        for (label, value) in MATERIALS {
                            if ui
                                .selectable_label(current_material == Some(value), label)
                                .clicked()
                            {
                                self.scene_edit_requested =
                                    Some(SceneEditRequest::UpdateProperty {
                                        target: selected.id.clone(),
                                        key: "primitive.material".to_owned(),
                                        value: Value::String(value.to_owned()),
                                    });
                            }
                        }
                    });
            });
        });
    }

    fn scene_transform_editor(
        &mut self,
        ui: &mut egui::Ui,
        selected: &SceneNode,
        has_size: bool,
        has_scale: bool,
        has_primitive_size: bool,
        yaw_only: bool,
    ) {
        property_section(ui, "Transform", |ui| {
            let mut position_changed = false;
            let mut rotation_changed = false;
            let mut size_changed = false;
            let mut scale_changed = false;
            ui.add_enabled_ui(
                self.project_editable && !self.playing && !scene_node_locked(selected),
                |ui| {
                    position_changed |= vector_editor(
                        ui,
                        "Position",
                        &mut self.scene_editor_position,
                        &mut self.scene_editor_position_text,
                    );
                    if yaw_only {
                        ui.label(
                            RichText::new("Yaw °")
                                .size(TYPE.secondary)
                                .color(palette(ui).secondary_text),
                        );
                        let response = ui.add_sized(
                            [ui.available_width(), CONTROL_HEIGHT],
                            egui::TextEdit::singleline(&mut self.scene_editor_rotation_text[1])
                                .id_salt(("scene-yaw", &selected.id))
                                .horizontal_align(Align::RIGHT),
                        );
                        if response.lost_focus()
                            && let Ok(value) =
                                self.scene_editor_rotation_text[1].trim().parse::<f32>()
                            && value.is_finite()
                            && self.scene_editor_rotation[1] != value
                        {
                            self.scene_editor_rotation = [0.0, value, 0.0];
                            rotation_changed = true;
                        }
                        if response.lost_focus() {
                            self.scene_editor_rotation_text[1] =
                                format_scene_number(self.scene_editor_rotation[1]);
                        }
                    } else {
                        rotation_changed |= vector_editor(
                            ui,
                            "Orientation °",
                            &mut self.scene_editor_rotation,
                            &mut self.scene_editor_rotation_text,
                        );
                    }
                    if has_size {
                        size_changed |= vector_editor(
                            ui,
                            "Size",
                            &mut self.scene_editor_size,
                            &mut self.scene_editor_size_text,
                        );
                    }
                    if has_scale {
                        scale_changed |= vector_editor(
                            ui,
                            "Scale",
                            &mut self.scene_editor_scale,
                            &mut self.scene_editor_scale_text,
                        );
                    }
                },
            );
            if scene_node_locked(selected) {
                ui.label(
                    RichText::new(
                        selected
                            .properties
                            .iter()
                            .find(|(label, _)| label == "Lock reason")
                            .map(|(_, reason)| reason.as_str())
                            .unwrap_or("This imported node is read-only."),
                    )
                    .size(TYPE.meta)
                    .color(palette(ui).muted),
                );
            } else if self.playing {
                ui.label(
                    RichText::new("Stop Play to edit the authoring scene.")
                        .size(TYPE.meta)
                        .color(palette(ui).muted),
                );
            }
            if position_changed || rotation_changed || size_changed || scale_changed {
                if has_size {
                    self.scene_editor_size = self.scene_editor_size.map(|value| value.max(0.05));
                    self.scene_editor_size_text = scene_vector_text(self.scene_editor_size);
                }
                if has_scale {
                    self.scene_editor_scale = self.scene_editor_scale.map(|value| value.max(0.05));
                    self.scene_editor_scale_text = scene_vector_text(self.scene_editor_scale);
                }
                self.project_dirty = true;
                self.project_error = None;
                self.scene_edit_requested = if has_primitive_size && size_changed {
                    Some(SceneEditRequest::SetPrimitiveSize {
                        target: selected.id.clone(),
                        position: self.scene_editor_position,
                        size: self.scene_editor_size,
                    })
                } else {
                    Some(SceneEditRequest::SetTransform {
                        target: selected.id.clone(),
                        position: self.scene_editor_position,
                        rotation: rotation_changed
                            .then(|| self.scene_editor_rotation.map(f32::to_radians)),
                        scale: has_scale.then_some(self.scene_editor_scale),
                    })
                };
                self.notice = if rotation_changed {
                    "Orientation changed — save to keep it".to_owned()
                } else {
                    "Scene object changed — save to keep it".to_owned()
                };
            }
        });
    }

    fn scene_object_actions(&mut self, ui: &mut egui::Ui, selected: &SceneNode) {
        let current_parent = scene_parent_id(selected);
        let selected_subtree = self.scene_outline.root.find(&selected.id);
        let mut groups = Vec::new();
        self.scene_outline
            .root
            .collect_group_options(selected_subtree, &mut groups);
        property_section(ui, "Actions", |ui| {
            if let Some(current_parent) = current_parent {
                ui.label(
                    RichText::new("Parent")
                        .size(TYPE.secondary)
                        .color(palette(ui).secondary_text),
                );
                let parent_label = groups
                    .iter()
                    .find(|(id, _)| id == current_parent)
                    .map(|(_, label)| label.as_str())
                    .unwrap_or(current_parent);
                egui::ComboBox::from_id_salt(("scene-parent", &selected.id))
                    .selected_text(parent_label)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for (id, label) in &groups {
                            if ui.selectable_label(id == current_parent, label).clicked()
                                && id != current_parent
                            {
                                self.scene_edit_requested =
                                    Some(SceneEditRequest::ReparentObjects {
                                        targets: vec![selected.id.clone()],
                                        parent_id: id.clone(),
                                    });
                                self.notice = format!("Moving {} into {label}…", selected.label);
                            }
                        }
                    });
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                if is_authoring_transformable(selected) && ui.button("Duplicate").clicked() {
                    self.scene_edit_requested = Some(SceneEditRequest::DuplicateObject {
                        target: selected.id.clone(),
                    });
                    self.notice = "Duplicating scene object…".to_owned();
                }
                let can_delete =
                    !scene_node_roblox_linked(selected) && !scene_node_locked(selected);
                if ui
                    .add_enabled(can_delete, egui::Button::new("Delete"))
                    .on_hover_text(if can_delete {
                        "Delete scene object"
                    } else {
                        "Deleting imported Roblox objects is not supported yet"
                    })
                    .clicked()
                {
                    self.scene_edit_requested = Some(SceneEditRequest::DeleteObject {
                        target: selected.id.clone(),
                    });
                    self.notice = "Deleting scene object…".to_owned();
                }
            });
        });
    }

    fn scene_manifest_properties(
        &mut self,
        ui: &mut egui::Ui,
        selected: &SceneNode,
        excluded: &[&str],
    ) {
        let properties = selected
            .properties
            .iter()
            .filter(|(label, _)| {
                !excluded.contains(&label.as_str())
                    && (!is_authoring_node(selected)
                        || !matches!(
                            label.as_str(),
                            "Name" | "Authoring ID" | "Kind" | "Parent" | "Visible" | "Locked"
                        ))
            })
            .cloned()
            .collect::<Vec<_>>();
        if properties.is_empty() {
            return;
        }
        property_section(ui, "Properties", |ui| {
            for (index, (label, value)) in properties.into_iter().enumerate() {
                if self.project_editable
                    && let Some((key, kind)) = editable_scene_property(&label)
                {
                    self.scene_property_editor(ui, selected, &label, &value, key, kind, index);
                } else {
                    property_row(ui, &label, &value);
                }
            }
        });
    }

    fn scene_property_editor(
        &mut self,
        ui: &mut egui::Ui,
        selected: &SceneNode,
        label: &str,
        current: &str,
        key: &str,
        kind: ScenePropertyKind,
        index: usize,
    ) {
        let field = property_field(ui, label);
        let mut text = self
            .scene_editor_properties
            .remove(label)
            .unwrap_or_else(|| current.to_owned());
        let response = ui.put(
            field,
            egui::TextEdit::singleline(&mut text)
                .id_salt(("scene-property", &selected.id, key, index))
                .horizontal_align(Align::RIGHT),
        );
        let commit = response.lost_focus() && text.trim() != current;
        self.scene_editor_properties
            .insert(label.to_owned(), text.clone());
        if !commit {
            return;
        }
        let value = match kind.parse(&text) {
            Ok(value) => value,
            Err(message) => {
                self.notice = format!("{label}: {message}");
                return;
            }
        };
        self.project_dirty = true;
        self.project_error = None;
        self.scene_edit_requested = Some(SceneEditRequest::UpdateProperty {
            target: selected.id.clone(),
            key: key.to_owned(),
            value,
        });
        self.notice = format!("{label} changed — save to keep it");
    }
}

fn scene_property<'a>(node: &'a SceneNode, label: &str) -> Option<&'a str> {
    node.properties
        .iter()
        .find(|(candidate, _)| candidate == label)
        .map(|(_, value)| value.as_str())
}

pub(crate) fn scene_parent_id(node: &SceneNode) -> Option<&str> {
    scene_property(node, "Parent").filter(|parent| *parent != "—")
}

#[derive(Clone, Copy)]
enum ScenePropertyKind {
    Text,
    Number,
    Boolean,
}

impl ScenePropertyKind {
    fn parse(self, source: &str) -> Result<Value, &'static str> {
        let source = source.trim();
        match self {
            Self::Text if !source.is_empty() => Ok(Value::String(source.to_owned())),
            Self::Text => Err("enter a value"),
            Self::Number => source
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(Value::from)
                .ok_or("enter a valid number"),
            Self::Boolean
                if source.eq_ignore_ascii_case("yes") || source.eq_ignore_ascii_case("true") =>
            {
                Ok(Value::Bool(true))
            }
            Self::Boolean
                if source.eq_ignore_ascii_case("no") || source.eq_ignore_ascii_case("false") =>
            {
                Ok(Value::Bool(false))
            }
            Self::Boolean => Err("enter Yes or No"),
        }
    }
}

fn editable_scene_property(label: &str) -> Option<(&'static str, ScenePropertyKind)> {
    Some(match label {
        "Id" => ("id", ScenePropertyKind::Text),
        "Name" => ("name", ScenePropertyKind::Text),
        "Label" => ("label", ScenePropertyKind::Text),
        "Skin" => ("actor.skin", ScenePropertyKind::Text),
        "Shirt" => ("actor.shirt", ScenePropertyKind::Text),
        "Pants" => ("actor.pants", ScenePropertyKind::Text),
        "Shoes" => ("actor.shoes", ScenePropertyKind::Text),
        "Kind" => ("kind", ScenePropertyKind::Text),
        "Color" => ("color", ScenePropertyKind::Text),
        "Material" => ("material", ScenePropertyKind::Text),
        "Visual" => ("visual", ScenePropertyKind::Text),
        "Climb Axis" => ("climbAxis", ScenePropertyKind::Text),
        "Radius" => ("radius", ScenePropertyKind::Number),
        "Yaw" => ("yaw", ScenePropertyKind::Number),
        "Max Width" => ("maxWidth", ScenePropertyKind::Number),
        "Climb Speed" => ("climbSpeed", ScenePropertyKind::Number),
        "Damage Per Second" => ("damagePerSecond", ScenePropertyKind::Number),
        "Heal Per Second" => ("healPerSecond", ScenePropertyKind::Number),
        "Outline" => ("outline", ScenePropertyKind::Boolean),
        "Framed" => ("framed", ScenePropertyKind::Boolean),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspector_property_values_keep_their_manifest_types() {
        assert_eq!(
            ScenePropertyKind::Number.parse("3.5").unwrap(),
            serde_json::json!(3.5)
        );
        assert_eq!(
            ScenePropertyKind::Boolean.parse("Yes").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            ScenePropertyKind::Text.parse("signal").unwrap(),
            Value::String("signal".to_owned())
        );
        assert!(ScenePropertyKind::Number.parse("wide").is_err());
    }
}

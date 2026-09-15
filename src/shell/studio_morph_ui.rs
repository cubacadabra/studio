use super::*;
impl StudioShell {
    pub(crate) fn show_morphs(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        self.show_morph_library(root);

        if root.available_width() >= 650.0 {
            egui::Panel::right("morph_inspector")
            .resizable(true)
            .default_size(280.0)
            .size_range(236.0..=360.0)
            .frame(editor_frame(colors.panel_raised))
            .show(root, |ui| {
                panel_header(ui, Icon::Sliders, "Morph inspector", |ui| {
                    let _ = ui;
                });
                content_frame().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                        if ui.button("Import character asset…").clicked() {
                            self.morph_import_requested = true;
                            self.morph_import_error = None;
                        }
                        if ui.button("Open .morph.json").clicked() {
                            self.morph_sidecar_import_requested = true;
                            self.morph_import_error = None;
                        }
                        if ui.button("Load .morphpack").clicked() {
                            self.morph_pack_import_requested = true;
                            self.morph_import_error = None;
                        }
                    });
                    if let Some(path) = &self.morph_preview_path {
                        property_section(ui, "Imported source", |ui| {
                            let filename = std::path::Path::new(path)
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or(path);
                            property_row(ui, "File", filename);
                            if let Some(preview) = &self.morph_preview {
                                property_row(ui, "Vertices", &preview.vertices.len().to_string());
                                property_row(ui, "Indices", &preview.indices.len().to_string());
                                property_row(
                                    ui,
                                    "Triangles",
                                    &(preview.indices.len() / 3).to_string(),
                                );
                                if let Some((minimum, maximum)) = morph_mesh_bounds(preview) {
                                    let size: [f32; 3] = std::array::from_fn(|axis| {
                                        maximum[axis] - minimum[axis]
                                    });
                                    property_row(
                                        ui,
                                        "Source size",
                                        &format!(
                                            "{:.2} × {:.2} × {:.2}",
                                            size[0], size[1], size[2]
                                        ),
                                    );
                                    property_row(
                                        ui,
                                        "Runtime size",
                                        &format!(
                                            "{:.2} × {:.2} × {:.2}",
                                            size[0] * self.morph_attachment_scale[0],
                                            size[1] * self.morph_attachment_scale[1],
                                            size[2] * self.morph_attachment_scale[2]
                                        ),
                                    );
                                }
                            }
                            if let Some(asset) = &self.morph_draft_asset {
                                property_row(ui, "Draft", &asset.display_name);
                                property_row(ui, "Asset ID", asset.id.as_str());
                            }
                        });
                        if let Some(summary) = self.morph_source_summary.clone() {
                            property_section(ui, "Source contract", |ui| {
                                property_row(ui, "Nodes", &summary.node_names.len().to_string());
                                property_row(ui, "Meshes", &summary.mesh_names.len().to_string());
                                property_row(
                                    ui,
                                    "Materials",
                                    &summary.material_names.len().to_string(),
                                );
                                property_row(
                                    ui,
                                    "Source triangles",
                                    &summary.triangle_count.to_string(),
                                );
                                for level in ["near", "mid", "far"] {
                                    let status = source_lod_status(&summary, level);
                                    property_row(ui, &format!("{} LOD", title_case(level)), &status);
                                }
                                if ["near", "mid", "far"]
                                    .into_iter()
                                    .any(|level| summary.lod_candidates[level].is_empty())
                                {
                                    ui.label(
                                        RichText::new(
                                            "Preview only: map distinct Near / Mid / Far nodes before publishing.",
                                        )
                                        .size(TYPE.meta)
                                        .color(colors.axis_x),
                                    );
                                }
                            });
                            property_section(ui, "Draft mapping", |ui| {
                                ui.label(
                                    RichText::new("Attachment joint")
                                        .size(TYPE.meta)
                                        .color(colors.secondary_text),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.morph_attachment_joint)
                                        .hint_text("head")
                                        .desired_width(ui.available_width()),
                                );
                                ui.label(
                                    RichText::new("Attachment offset X / Y / Z")
                                        .size(TYPE.meta)
                                        .color(colors.secondary_text),
                                );
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    for (axis, value) in
                                        self.morph_attachment_translation.iter_mut().enumerate()
                                    {
                                        ui.add(
                                            egui::DragValue::new(value)
                                                .speed(0.01)
                                                .range(-10.0..=10.0)
                                                .prefix(["X ", "Y ", "Z "][axis]),
                                        );
                                    }
                                });
                                ui.label(
                                    RichText::new("Attachment scale X / Y / Z")
                                        .size(TYPE.meta)
                                        .color(colors.secondary_text),
                                );
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    for (axis, value) in
                                        self.morph_attachment_scale.iter_mut().enumerate()
                                    {
                                        ui.add(
                                            egui::DragValue::new(value)
                                                .speed(0.01)
                                                .range(0.01..=100.0)
                                                .prefix(["X ", "Y ", "Z "][axis]),
                                        );
                                    }
                                });
                                if ui.button("Fit to person head").clicked() {
                                    self.fit_current_morph_to_person();
                                }
                                for (index, level) in ["Near", "Mid", "Far"].into_iter().enumerate()
                                {
                                    ui.label(
                                        RichText::new(format!("{level} LOD node"))
                                            .size(TYPE.meta)
                                            .color(colors.secondary_text),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.morph_lod_nodes[index])
                                            .hint_text("GLB node name")
                                            .desired_width(ui.available_width()),
                                    );
                                }
                                if ui.button("Validate mapping").clicked() {
                                    self.validate_morph_draft();
                                }
                                if ui.button("Save draft").clicked() {
                                    self.morph_draft_export_requested = true;
                                }
                                if ui.button("Export .morph.json").clicked() {
                                    self.validate_morph_draft();
                                    if self
                                        .morph_draft_status
                                        .as_ref()
                                        .is_some_and(|(valid, _)| *valid)
                                    {
                                        self.morph_sidecar_export_requested = true;
                                    }
                                }
                                let add_to_game = ui
                                    .add_enabled(
                                        self.project_asset_available,
                                        egui::Button::new("Add to this game"),
                                    )
                                    .on_disabled_hover_text(
                                        "Open a game project before adding an asset.",
                                    );
                                if add_to_game.clicked() {
                                    self.validate_morph_draft();
                                    if self
                                        .morph_draft_status
                                        .as_ref()
                                        .is_some_and(|(valid, _)| *valid)
                                    {
                                        self.morph_project_add_requested = true;
                                    }
                                }
                                if ui.button("Export .morphpack").clicked() {
                                    self.validate_morph_draft();
                                    if self
                                        .morph_draft_status
                                        .as_ref()
                                        .is_some_and(|(valid, _)| *valid)
                                    {
                                        self.morph_publish_requested = true;
                                    }
                                }
                                if ui.button("Generate thumbnail PNG").clicked() {
                                    self.morph_thumbnail_requested = true;
                                }
                                if let Some((valid, status)) = &self.morph_draft_status {
                                    ui.label(
                                        RichText::new(status)
                                            .size(TYPE.meta)
                                            .color(if *valid { colors.live } else { colors.axis_x }),
                                    );
                                }
                            });
                        }
                    }
                    if let Some(error) = &self.morph_import_error {
                        ui.label(RichText::new(error).size(TYPE.meta).color(colors.axis_x));
                        ui.add_space(4.0);
                    }
                    if let Some(preset) = self.morph_catalog.presets.iter().find(|preset| preset.id == self.selected_morph) {
                        selected_object_header(ui, &preset.display_name, "Starter");
                        property_section(ui, "Appearance", |ui| {
                            for id in std::iter::once(&preset.base).chain(preset.parts.iter()) {
                                if let Some(asset) = self.morph_catalog.asset(id) {
                                    property_row(ui, morph_kind_label(asset.kind), &asset.display_name);
                                }
                            }
                            if let Some(cubacadabra_morphs::MorphParameterValue::Text(skin)) = preset.parameters.get("skin") {
                                property_row(ui, "Skin", skin);
                            }
                        });
                    } else if let Some(asset) = self.morph_catalog.asset(&self.selected_morph) {
                        selected_object_header(
                            ui,
                            &asset.display_name,
                            morph_kind_label(asset.kind),
                        );
                        ui.add_space(4.0);
                        property_section(ui, "Identity", |ui| {
                            let id = asset.id.to_string();
                            property_row(ui, "ID", &id);
                            property_row(ui, "Kind", morph_kind_label(asset.kind));
                            if let Some(rig) = &asset.rig_profile {
                                let rig = rig.to_string();
                                property_row(ui, "Rig", &rig);
                            }
                        });
                        property_section(ui, "Compatibility", |ui| {
                            let fits = asset
                                .fit_profiles
                                .iter()
                                .map(MorphAssetId::as_str)
                                .collect::<Vec<_>>()
                                .join(", ");
                            property_row(ui, "Fits", &fits);
                            let bases = asset
                                .supported_bases
                                .iter()
                                .map(MorphAssetId::as_str)
                                .collect::<Vec<_>>()
                                .join(", ");
                            property_row(
                                ui,
                                "Bases",
                                if bases.is_empty() { "Any" } else { &bases },
                            );
                        });
                        property_section(ui, "Runtime", |ui| {
                            let capabilities = asset
                                .required_capabilities
                                .iter()
                                .map(|capability| capability.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                            property_row(ui, "Needs", &capabilities);
                            property_row(ui, "LOD", "Near / Mid / Far");
                        });
                    } else {
                        ui.label(
                            RichText::new("Select a morph to inspect its contract.")
                                .size(TYPE.secondary)
                                .color(colors.muted),
                        );
                    }
                });
            });
        }
        self.morph_preview_panel(root);
    }

    pub(crate) fn morph_preview_panel(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(root, |ui| {
                let available = ui.available_rect_before_wrap();
                editor_header(ui, |ui| {
                    inline_icon(ui, Icon::Camera, colors.muted);
                    ui.label(
                        RichText::new("Morph preview")
                            .font(semibold_font(TYPE.primary))
                            .color(colors.text),
                    );
                    vertical_separator(ui, 12.0);
                    ui.label(
                        RichText::new(if self.morph_preview.is_some() {
                            "Imported GLB"
                        } else {
                            self.morph_catalog
                                .presets
                                .iter()
                                .find(|preset| {
                                    crate::wardrobe::matches_preset(&self.active_loadout, preset)
                                })
                                .map_or("Custom appearance", |preset| preset.display_name.as_str())
                        })
                        .size(TYPE.secondary)
                        .color(colors.secondary_text),
                    );
                });
                let preview_rect = Rect::from_min_max(
                    egui::pos2(
                        available.min.x + 1.0,
                        available.min.y + EDITOR_HEADER_HEIGHT + 2.0,
                    ),
                    egui::pos2(available.max.x - 1.0, available.max.y - 1.0),
                );
                self.runtime_viewport = preview_rect;
                ui.allocate_rect(preview_rect, Sense::hover());
                if !self.morph_catalog_ready || self.morph_loading {
                    // The engine starts with its bundled appearance while the
                    // Studio catalog and initial loadout are arriving. Keep
                    // that implementation fallback out of the preview so it
                    // cannot flash before the requested appearance is ready.
                    ui.painter().rect_filled(preview_rect, 0.0, colors.surface);
                    ui.painter().text(
                        preview_rect.center(),
                        Align2::CENTER_CENTER,
                        "Loading appearance…",
                        FontId::proportional(TYPE.secondary),
                        colors.muted,
                    );
                }
                ui.painter().rect_stroke(
                    available,
                    0.0,
                    Stroke::new(1.0, colors.border),
                    StrokeKind::Inside,
                );
            });
    }
}

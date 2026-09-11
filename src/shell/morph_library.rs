use super::*;
use crate::wardrobe::{self, Request};

impl StudioShell {
    fn queue_morph(&mut self, request: Request) {
        self.morph_preview = None;
        self.morph_preview_path = None;
        self.morph_draft_asset = None;
        self.morph_source_summary = None;
        self.morph_lod_previews = [None, None, None];
        self.morph_import_error = None;
        self.morph_request = Some(request);
    }

    pub(super) fn show_morph_library(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        let narrow = root.available_width() < 600.0;
        let panel = if narrow {
            egui::Panel::top("morph_library_compact")
                .default_size(300.0)
                .size_range(180.0..=400.0)
        } else {
            egui::Panel::left("morph_library")
                .default_size(252.0)
                .size_range(200.0..=340.0)
        };
        panel
            .resizable(true)
            .frame(editor_frame(colors.panel))
            .show(root, |ui| {
                panel_header(ui, Icon::Character, "Morph library", |_| {});
                content_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.morph_starters, true, "Starters");
                        ui.selectable_value(&mut self.morph_starters, false, "Customize");
                    });
                    search_field(ui, &mut self.morph_query, ui.available_width());
                    if self.morph_loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Loading appearance…");
                        });
                    }
                    if let Some(error) = self.morph_catalog_error.clone() {
                        ui.label(RichText::new("Catalog unavailable").color(colors.muted))
                            .on_hover_text(error);
                        if ui.button("Retry").clicked() {
                            self.morph_request = Some(Request::RetryCatalog);
                        }
                    } else if !self.morph_catalog_ready {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Loading starters…");
                        });
                    }
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .id_salt("morph_library_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.morph_starters {
                                self.show_starters(ui);
                            } else {
                                ui.add_enabled_ui(!self.morph_loading, |ui| {
                                    self.show_customization(ui);
                                });
                            }
                        });
                });
            });
    }

    fn show_starters(&mut self, ui: &mut egui::Ui) {
        if !self.morph_catalog_ready {
            return;
        }
        let query = self.morph_query.trim().to_ascii_lowercase();
        let presets: Vec<_> = self
            .morph_catalog
            .presets
            .iter()
            .filter(|preset| {
                query.is_empty()
                    || preset.display_name.to_ascii_lowercase().contains(&query)
                    || preset.parts.iter().any(|id| {
                        self.morph_catalog.asset(id).is_some_and(|asset| {
                            asset.display_name.to_ascii_lowercase().contains(&query)
                        })
                    })
            })
            .cloned()
            .collect();
        if presets.is_empty() {
            ui.label(if query.is_empty() {
                "No starters in this catalog."
            } else {
                "No starters match this search."
            });
            return;
        }
        let width = ((ui.available_width() - 8.0) / 2.0).floor();
        for row in presets.chunks(2) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for preset in row {
                    ui.push_id(preset.id.as_str(), |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            let selected = wardrobe::matches_preset(&self.active_loadout, preset);
                            let image = preset
                                .thumbnail
                                .as_ref()
                                .and_then(|url| self.morph_thumbnails.get(url));
                            let response = if let Some(image) = image {
                                ui.add(
                                    egui::Button::image(egui::Image::new((
                                        image.id(),
                                        egui::vec2(width - 8.0, (width - 8.0) * 1.25),
                                    )))
                                    .selected(selected)
                                    .min_size(egui::vec2(width, width * 1.25)),
                                )
                            } else {
                                ui.add_sized(
                                    [width, width * 1.25],
                                    egui::Button::new(&preset.display_name).selected(selected),
                                )
                            }
                            .on_hover_text(format!(
                                "Use {} · replaces the complete appearance",
                                preset.display_name
                            ));
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Button,
                                    true,
                                    selected,
                                    &preset.display_name,
                                )
                            });
                            if response.clicked() {
                                self.selected_morph = preset.id.clone();
                                self.queue_morph(Request::Preset(preset.id.clone()));
                            }
                            ui.label(RichText::new(&preset.display_name).size(TYPE.secondary));
                        });
                    });
                }
            });
            ui.add_space(6.0);
        }
    }

    fn show_customization(&mut self, ui: &mut egui::Ui) {
        let query = self.morph_query.trim().to_ascii_lowercase();
        if query.is_empty() || "skin".contains(&query) {
            egui::CollapsingHeader::new("Skin")
                .default_open(true)
                .show(ui, |ui| {
                    let selected = self.active_loadout.parameters.get("skin");
                    let mut choice = None;
                    for row in wardrobe::SKIN_TONES.chunks(6) {
                        ui.horizontal(|ui| {
                            for color in row {
                                let value = u32::from_str_radix(&color[1..], 16).unwrap();
                                let fill = Color32::from_rgb(
                                    (value >> 16) as u8,
                                    (value >> 8) as u8,
                                    value as u8,
                                );
                                let active = selected
                                    == Some(&cubacadabra_morphs::MorphParameterValue::Text(
                                        (*color).into(),
                                    ));
                                let ink = if value >> 16 > 150 {
                                    Color32::BLACK
                                } else {
                                    Color32::WHITE
                                };
                                let response = ui.add_sized(
                                    [25.0, 25.0],
                                    egui::Button::new(
                                        RichText::new(if active { "✓" } else { " " }).color(ink),
                                    )
                                    .fill(fill),
                                );
                                response.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::Button,
                                        ui.is_enabled(),
                                        active,
                                        format!("Skin tone {color}"),
                                    )
                                });
                                if response
                                    .on_hover_text(format!("Skin tone {color}"))
                                    .clicked()
                                {
                                    choice = Some((*color).to_owned());
                                }
                            }
                        });
                    }
                    if let Some(color) = choice {
                        self.queue_morph(Request::Color("skin".into(), color));
                    }
                });
        }
        let mut groups: BTreeMap<(usize, &'static str), Vec<_>> = BTreeMap::new();
        for asset in &self.morph_catalog.assets {
            if asset.kind == MorphAssetKind::Outfit {
                continue;
            }
            let group = library_group(asset);
            if !query.is_empty()
                && !asset.display_name.to_ascii_lowercase().contains(&query)
                && !group.1.to_ascii_lowercase().contains(&query)
            {
                continue;
            }
            groups.entry(group).or_default().push(asset.clone());
        }
        if groups.is_empty() && !query.is_empty() && !"skin".contains(&query) {
            ui.label("No parts match this search.");
        }
        for ((_, label), mut assets) in groups {
            assets.sort_by(|a, b| a.display_name.cmp(&b.display_name));
            egui::CollapsingHeader::new(label)
                .default_open(!query.is_empty() || label == "Hair")
                .show(ui, |ui| {
                    let optional = assets.iter().all(wardrobe::optional);
                    // None removes every currently equipped item in this category,
                    // without touching other categories sharing a parent kind.
                    if optional {
                        let active: Vec<_> = assets
                            .iter()
                            .filter(|asset| self.active_morphs.contains(asset.id.as_str()))
                            .collect();
                        if ui.selectable_label(active.is_empty(), "None").clicked() {
                            self.queue_morph(Request::Clear(
                                active.iter().map(|asset| asset.id.clone()).collect(),
                            ));
                        }
                    }
                    for asset in assets {
                        let active = self.active_morphs.contains(asset.id.as_str());
                        let compatible = asset.kind == MorphAssetKind::Base
                            || asset.supported_bases.contains(&self.active_loadout.base);
                        let response = ui.add_enabled(
                            compatible && !self.morph_loading,
                            egui::Button::new(format!(
                                "{}{}",
                                if active { "✓ " } else { "" },
                                asset.display_name
                            ))
                            .selected(active)
                            .frame(false),
                        );
                        if response
                            .on_disabled_hover_text("Does not fit the current body")
                            .clicked()
                        {
                            self.selected_morph = asset.id.clone();
                            self.queue_morph(if active && wardrobe::optional(&asset) {
                                Request::Remove(asset.id)
                            } else {
                                Request::Equip(asset.id)
                            });
                        }
                    }
                });
        }
    }
}

fn library_group(asset: &cubacadabra_morphs::MorphAssetDefinition) -> (usize, &'static str) {
    if asset.occupied_slots.iter().any(|slot| slot == "ear-device") {
        return (13, "Hearing devices");
    }
    if asset.kind == MorphAssetKind::Headwear {
        return if asset
            .occupied_slots
            .iter()
            .any(|slot| slot == "ear-accessory")
        {
            (12, "Headphones")
        } else {
            (11, "Hats")
        };
    }
    let index = MORPH_LIBRARY_KINDS
        .iter()
        .position(|kind| *kind == asset.kind)
        .unwrap_or(99);
    (
        index,
        match asset.kind {
            MorphAssetKind::Base => "Body & facial shape",
            MorphAssetKind::Face => "Expression",
            MorphAssetKind::Facewear => "Glasses",
            _ => morph_kind_label(asset.kind),
        },
    )
}

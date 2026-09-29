use super::*;

impl StudioShell {
    pub(crate) fn performance_shadows_enabled(&self) -> bool {
        self.performance_shadows_enabled
    }

    pub(crate) fn performance_static_translucent_sort_enabled(&self) -> bool {
        self.performance_static_translucent_sort_enabled
    }

    pub(crate) fn finish_performance_frame(
        &mut self,
        frame_ms: f32,
        logic_ms: f32,
        client_step_ms: f32,
        scene_projection_ms: f32,
        renderer_sync_ms: f32,
        renderer_draw_ms: f32,
        renderer_timings_ms: [f32; 4],
    ) {
        let Some(mut sample) = self.performance_pending.take() else {
            return;
        };
        sample.frame_ms = frame_ms;
        sample.logic_ms = logic_ms;
        sample.client_step_ms = client_step_ms;
        sample.scene_projection_ms = scene_projection_ms;
        sample.renderer_sync_ms = renderer_sync_ms;
        sample.renderer_draw_ms = renderer_draw_ms;
        sample.renderer_encode_ms = renderer_timings_ms[0];
        sample.renderer_presenter_ms = renderer_timings_ms[1];
        sample.renderer_submit_ms = renderer_timings_ms[2];
        sample.renderer_present_ms = renderer_timings_ms[3];
        self.performance_latest = sample;
        self.performance_history.push_back(sample);
        if self.performance_history.len() > PERFORMANCE_HISTORY_LIMIT {
            self.performance_history.pop_front();
        }

        if self.performance_log_slow_frames
            && sample.frame_ms >= 50.0
            && self
                .performance_last_slow_log
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(1))
        {
            log::warn!("studio performance slow frame: {}", sample.log_line());
            self.performance_last_slow_log = Some(Instant::now());
        }
    }

    pub(crate) fn show_performance_monitor(&mut self, context: &egui::Context) {
        if !self.performance_open {
            return;
        }

        let sample = self.performance_latest;
        let history = self.performance_history.iter().copied().collect::<Vec<_>>();
        let mut open = self.performance_open;
        let mut log_snapshot = false;
        let mut clear_history = false;
        let colors = if context.style_of(context.theme()).visuals.dark_mode {
            DARK_PALETTE
        } else {
            LIGHT_PALETTE
        };

        egui::Window::new("Performance")
            .open(&mut open)
            .default_width(360.0)
            .resizable(true)
            .frame(
                Frame::NONE
                    .fill(colors.panel_raised)
                    .stroke(Stroke::new(1.0, colors.border_strong))
                    .corner_radius(6.0)
                    .inner_margin(Margin::same(12)),
            )
            .show(context, |ui| {
                let fps = if sample.frame_ms > 0.0 {
                    1_000.0 / sample.frame_ms
                } else {
                    0.0
                };
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{:.1} ms", sample.frame_ms))
                            .font(semibold_font(18.0))
                            .color(performance_color(colors, sample.frame_ms)),
                    );
                    ui.label(
                        RichText::new(format!("{fps:.0} FPS · {}", sample.workspace.label()))
                            .size(TYPE.secondary)
                            .color(colors.secondary_text),
                    );
                });
                ui.label(
                    RichText::new("CPU timings for the last completed Studio frame")
                        .size(TYPE.meta)
                        .color(colors.muted),
                );
                ui.add_space(8.0);

                performance_chart(ui, &history, colors);

                ui.add_space(8.0);
                metric_row(ui, "Frame total", sample.frame_ms, colors);
                metric_row(ui, "App logic", sample.logic_ms, colors);
                metric_row(ui, "Client step", sample.client_step_ms, colors);
                metric_row(ui, "Scene projections", sample.scene_projection_ms, colors);
                metric_row(ui, "UI build", sample.ui_build_ms, colors);
                metric_row(ui, "UI tessellate", sample.ui_tessellate_ms, colors);
                metric_row(ui, "Renderer sync", sample.renderer_sync_ms, colors);
                metric_row(ui, "Renderer draw", sample.renderer_draw_ms, colors);
                metric_row(ui, "Renderer encode", sample.renderer_encode_ms, colors);
                metric_row(ui, "Presenter draw", sample.renderer_presenter_ms, colors);
                metric_row(ui, "Queue submit", sample.renderer_submit_ms, colors);
                metric_row(ui, "Frame present", sample.renderer_present_ms, colors);
                metric_row(ui, "Overlay paint", sample.overlay_paint_ms, colors);

                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    diagnostic_count(ui, "tree rows", sample.tree_rows, colors);
                    diagnostic_count(ui, "scene objects", sample.scene_objects, colors);
                    diagnostic_count(ui, "egui primitives", sample.egui_primitives, colors);
                });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.checkbox(&mut self.performance_shadows_enabled, "Static shadows")
                    .on_hover_text(
                        "Toggle Studio's directional shadow pass for an A/B performance check.",
                    );
                ui.checkbox(
                    &mut self.performance_static_translucent_sort_enabled,
                    "Sort static translucent (slow)",
                )
                .on_hover_text(
                    "Diagnostic fallback: per-frame depth sorting of static translucent geometry. The normal Studio path keeps static geometry in authored order for performance; blending order may differ.",
                );
                ui.checkbox(
                    &mut self.performance_log_slow_frames,
                    "Log frames over 50 ms (once per second)",
                )
                .on_hover_text("Writes a compact timing line to the Studio log.");
                ui.horizontal(|ui| {
                    if ui.button("Log current sample").clicked() {
                        log_snapshot = true;
                    }
                    if ui.button("Clear history").clicked() {
                        clear_history = true;
                    }
                });
            });

        self.performance_open = open;
        if clear_history {
            self.performance_history.clear();
        }
        if log_snapshot {
            log::info!("studio performance sample: {}", sample.log_line());
        }
    }
}

fn performance_chart(ui: &mut egui::Ui, history: &[PerformanceSample], colors: Palette) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 82.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, colors.surface);
    ui.painter().rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0, colors.border),
        StrokeKind::Inside,
    );
    if history.len() < 2 {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Collecting samples…",
            FontId::proportional(TYPE.meta),
            colors.muted,
        );
        return;
    }

    let max_ms = history
        .iter()
        .map(|sample| sample.frame_ms)
        .fold(33.0_f32, f32::max);
    let inner = rect.shrink2(egui::vec2(8.0, 12.0));
    let points = history
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            let x = if history.len() == 1 {
                inner.center().x
            } else {
                inner.left() + inner.width() * index as f32 / (history.len() - 1) as f32
            };
            let y = inner.bottom() - (sample.frame_ms / max_ms).clamp(0.0, 1.0) * inner.height();
            egui::pos2(x, y)
        })
        .collect::<Vec<_>>();
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(1.5, colors.accent)));
    ui.painter().text(
        rect.left_top() + egui::vec2(8.0, 5.0),
        Align2::LEFT_TOP,
        format!("max {max_ms:.0} ms"),
        FontId::proportional(TYPE.meta - 1.0),
        colors.muted,
    );
    ui.painter().text(
        rect.right_bottom() - egui::vec2(8.0, 5.0),
        Align2::RIGHT_BOTTOM,
        "older  →  newer",
        FontId::proportional(TYPE.meta - 1.0),
        colors.muted,
    );
}

fn metric_row(ui: &mut egui::Ui, label: &str, value: f32, colors: Palette) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [ui.available_width() - 58.0, 18.0],
            egui::Label::new(
                RichText::new(label)
                    .size(TYPE.secondary)
                    .color(colors.secondary_text),
            ),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{value:>6.1} ms"))
                    .size(TYPE.secondary)
                    .color(performance_color(colors, value)),
            );
        });
    });
}

fn diagnostic_count(ui: &mut egui::Ui, label: &str, value: usize, colors: Palette) {
    ui.label(
        RichText::new(format!("{label} {value}"))
            .size(TYPE.meta)
            .color(colors.muted),
    );
}

fn performance_color(colors: Palette, millis: f32) -> Color32 {
    if millis >= 50.0 {
        colors.axis_x
    } else if millis >= 16.7 {
        colors.accent
    } else {
        colors.live
    }
}

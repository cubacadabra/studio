use super::*;
impl StudioShell {
    pub(crate) fn show_world(&mut self, root: &mut egui::Ui) {
        let colors = palette(root);
        if !self.playing {
            egui::Panel::left("world_scene")
                .resizable(true)
                .default_size(208.0)
                .size_range(180.0..=340.0)
                .frame(editor_frame(colors.panel))
                .show(root, |ui| self.scene_tree(ui));

            egui::Panel::right("world_inspector")
                .resizable(true)
                .default_size(256.0)
                .size_range(224.0..=340.0)
                .frame(editor_frame(colors.panel_raised))
                .show(root, |ui| self.inspector(ui));
        }

        self.viewport_panel(root, "Perspective", "Viewport");
    }
}

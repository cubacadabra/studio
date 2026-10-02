use super::*;
use cubacadabra_room_capture::{
    alignment::{Alignment, create_alignment, read_alignment, save_alignment, subtract},
    measurements::{CaptureMeasurements, read_measurements},
    reconstruction::{
        Reconstruction, ReconstructionOptions, ReconstructionProgress, SparsePoint,
        read_component_points, read_reconstruction, recover_cameras,
    },
};

enum Message {
    Progress(ReconstructionProgress),
    Finished(Result<Reconstruction, String>),
}

struct Worker {
    messages: Receiver<Message>,
    cancelled: Arc<AtomicBool>,
    started: Instant,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub(super) struct CameraReviewState {
    pub(super) active: bool,
    worker: Option<Worker>,
    progress: Option<ReconstructionProgress>,
    error: Option<String>,
    manifest: Option<PathBuf>,
    capture: Option<PathBuf>,
    result: Option<Reconstruction>,
    component: usize,
    points: Vec<SparsePoint>,
    selected_point: Option<u64>,
    inspect_id: String,
    selected_frame: Option<String>,
    orbit: Orbit,
    alignment: Option<Alignment>,
    measurements: Option<CaptureMeasurements>,
    distance_ids: [Option<u64>; 2],
    floor_ids: [Option<u64>; 3],
    distance_meters: f64,
    pick_target: Option<usize>,
    evidence: Option<(String, egui::TextureHandle)>,
    evidence_loaded: Option<String>,
    evidence_error: Option<String>,
    notice: Option<String>,
}

impl CameraReviewState {
    #[cfg(debug_assertions)]
    pub(super) fn assert_review_visible(&self) {
        assert!(
            self.active && !self.points.is_empty(),
            "camera review needs sparse points"
        );
        assert!(
            self.evidence.is_some(),
            "camera review source view must decode: {:?}",
            self.evidence_error
        );
        assert!(
            self.error.is_none(),
            "camera review failed: {:?}",
            self.error
        );
    }
    pub(super) fn is_busy(&self) -> bool {
        self.worker.is_some()
    }
    pub(super) fn has_result(&self) -> bool {
        self.result.is_some()
    }
    pub(super) fn cancel(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancelled.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn start(&mut self, capture: &Path) {
        if self.is_busy() {
            return;
        }
        let Some(folder) = capture.parent() else {
            return;
        };
        let Some(parent) = folder.parent() else {
            return;
        };
        let output = match new_capture_path(folder, parent) {
            Ok(path) => path.with_file_name(
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .replace("-capture-", "-cameras-"),
            ),
            Err(error) => {
                self.active = true;
                self.error = Some(error);
                return;
            }
        };
        *self = Self::default();
        self.active = true;
        self.capture = Some(capture.to_owned());
        self.manifest = Some(output.join("reconstruction.json"));
        let capture = capture.to_owned();
        let (send, messages) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let thread_cancelled = cancelled.clone();
        let spawned = std::thread::Builder::new()
            .name("room-camera-recovery".into())
            .spawn(move || {
                let mut logged = None;
                let result = recover_cameras(
                    &capture,
                    &output,
                    ReconstructionOptions::default(),
                    &thread_cancelled,
                    |progress| {
                        if logged != Some(progress.stage) {
                            log::info!(
                                "Room camera recovery: {} · {} elapsed",
                                progress.stage.label(),
                                capture_duration(progress.elapsed)
                            );
                            logged = Some(progress.stage);
                        }
                        let _ = send.send(Message::Progress(progress));
                    },
                );
                if let Err(error) = &result {
                    log::info!("Room camera recovery stopped: {error}");
                }
                let _ = send.send(Message::Finished(result));
            });
        match spawned {
            Ok(_) => {
                self.worker = Some(Worker {
                    messages,
                    cancelled,
                    started: Instant::now(),
                })
            }
            Err(_) => self.error = Some("Could not start camera recovery. Try again.".into()),
        }
    }

    pub(super) fn poll(&mut self) {
        let mut finished = None;
        if let Some(worker) = &self.worker {
            loop {
                match worker.messages.try_recv() {
                    Ok(Message::Progress(progress)) => self.progress = Some(progress),
                    Ok(Message::Finished(result)) => {
                        finished = Some(result);
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        finished =
                            Some(Err("Camera worker stopped unexpectedly. Try again.".into()));
                        break;
                    }
                }
            }
        }
        if let Some(result) = finished {
            self.worker = None;
            match result {
                Ok(result) => {
                    self.result = Some(result);
                    self.set_component(0);
                }
                Err(error) => self.error = Some(error),
            }
        }
    }

    pub(super) fn load(&mut self, manifest: &Path) {
        if self.is_busy() {
            return;
        }
        match read_reconstruction(manifest) {
            Ok(result) => {
                *self = Self::default();
                self.active = true;
                self.manifest = Some(manifest.to_owned());
                self.result = Some(result);
                self.set_component(0);
            }
            Err(error) => {
                self.active = true;
                self.error = Some(error);
            }
        }
    }

    fn set_component(&mut self, index: usize) {
        self.component = index;
        self.points.clear();
        self.selected_point = None;
        self.inspect_id.clear();
        self.pick_target = None;
        self.alignment = None;
        self.distance_ids = [None; 2];
        self.floor_ids = [None; 3];
        self.selected_frame = None;
        self.evidence = None;
        self.evidence_loaded = None;
        self.evidence_error = None;
        self.notice = None;
        let (Some(result), Some(manifest)) = (&self.result, &self.manifest) else {
            return;
        };
        let Some(component) = result.components.get(index) else {
            return;
        };
        match read_component_points(manifest.parent().unwrap(), component) {
            Ok(points) => self.points = points,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        }
        self.selected_frame = component.frames.first().map(|p| p.frame_id.clone());
        match read_measurements(manifest) {
            Ok(measurements) => self.measurements = measurements,
            Err(error) => self.error = Some(error),
        }
        match read_alignment(manifest, index) {
            Ok(Some(alignment)) => {
                self.distance_ids = alignment.distance_point_ids.map(Some);
                self.floor_ids = alignment.floor_point_ids.map(Some);
                self.distance_meters = alignment.distance_meters;
                self.alignment = Some(alignment);
            }
            Ok(None) => {}
            Err(error) => self.error = Some(error),
        }
        self.reframe();
    }

    fn reframe(&mut self) {
        let transform = |p| {
            self.alignment
                .as_ref()
                .map(|a| a.transform_point(p))
                .unwrap_or(p)
        };
        let cameras = self
            .result
            .as_ref()
            .and_then(|r| r.components.get(self.component))
            .map(|c| {
                c.frames
                    .iter()
                    .map(|p| transform(p.center()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.orbit = Orbit::fit(
            self.points
                .iter()
                .map(|p| transform(p.position))
                .chain(cameras)
                .collect(),
        );
    }

    fn pick(&mut self, id: u64) {
        self.selected_point = Some(id);
        self.inspect_id = id.to_string();
        self.notice = None;
        if let Some(target) = self.pick_target.take() {
            if target < 2 {
                self.distance_ids[target] = Some(id);
            } else {
                self.floor_ids[target - 2] = Some(id);
            }
        }
        if let Some(point) = self.points.iter().find(|p| p.id == id) {
            if !point
                .observations
                .iter()
                .any(|o| Some(&o.frame_id) == self.selected_frame.as_ref())
            {
                self.selected_frame = point.observations.first().map(|o| o.frame_id.clone());
            }
        }
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        let colors = palette(ui);
        if let Some(worker) = &self.worker {
            let cancelling = worker.cancelled.load(Ordering::Relaxed);
            ui.label(
                RichText::new(if cancelling {
                    "Cancelling camera recovery…"
                } else {
                    self.progress
                        .map(|p| p.stage.label())
                        .unwrap_or("Starting camera recovery…")
                })
                .strong(),
            );
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!(
                    "{} elapsed",
                    capture_duration(worker.started.elapsed())
                ));
            });
            ui.label("Reconstruction time depends on image detail and overlap.");
            if ui
                .add_enabled(
                    !cancelling,
                    egui::Button::new("Cancel recovery").min_size(egui::vec2(120.0, 44.0)),
                )
                .clicked()
            {
                self.cancel();
            }
            ui.ctx().request_repaint_after(Duration::from_millis(250));
            return;
        }
        if let Some(error) = &self.error {
            ui.label(RichText::new(error).color(colors.axis_x));
        }
        let Some(result) = &self.result else {
            if let Some(capture) = self.capture.clone() {
                if ui
                    .add_sized([120.0, 44.0], egui::Button::new("Retry recovery"))
                    .clicked()
                {
                    self.start(&capture);
                }
            }
            return;
        };
        ui.label(
            RichText::new(format!(
                "{} / {} frames registered · {} component(s)",
                result.inputs.len() - result.unregistered_frame_ids.len(),
                result.inputs.len(),
                result.components.len()
            ))
            .strong(),
        );
        if let Some(manifest) = &self.manifest {
            ui.add(egui::Label::new(manifest.display().to_string()).truncate())
                .on_hover_text(manifest.display().to_string());
        }
        if result.components.is_empty() {
            for diagnostic in &result.diagnostics {
                ui.label(diagnostic);
            }
            if let Some(capture) = self.capture.clone() {
                if ui
                    .add_sized([120.0, 44.0], egui::Button::new("Retry recovery"))
                    .clicked()
                {
                    self.start(&capture);
                }
            }
            return;
        }
        let mut selected = self.component;
        if result.components.len() > 1 {
            ui.horizontal_wrapped(|ui| {
                ui.label("Component");
                egui::ComboBox::from_id_salt("room_camera_component")
                    .selected_text(&result.components[self.component].id)
                    .show_ui(ui, |ui| {
                        for (index, c) in result.components.iter().enumerate() {
                            ui.selectable_value(
                                &mut selected,
                                index,
                                format!("{} · {} frames", c.id, c.frames.len()),
                            );
                        }
                    });
            });
        }
        if selected != self.component {
            self.set_component(selected);
        }
        let component = &self.result.as_ref().unwrap().components[self.component];
        ui.label(format!(
            "{} points · reprojection error mean {:.2}px / median {:.2}px",
            component.point_count,
            component.mean_point_reprojection_error_pixels,
            component.median_point_reprojection_error_pixels
        ));
        ui.label(format!(
            "Median track angle {:.2}° · review depth against the source views",
            component.median_track_angle_degrees
        ));
        if self.alignment.is_some() {
            ui.label("Reviewed alignment · meters · +Y up");
        } else {
            ui.label("Unaligned · arbitrary units and axes");
        }
        self.show_cloud(ui);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_sized([100.0, 44.0], egui::Button::new("Reset view"))
                .clicked()
            {
                self.reframe();
            }
            ui.add(
                egui::Label::new("Drag to orbit · scroll to zoom · click a point or camera").wrap(),
            );
        });
        // Sliders make the spatial review operable without pointer gestures.
        ui.add(
            egui::Slider::new(
                &mut self.orbit.yaw,
                -std::f64::consts::PI..=std::f64::consts::PI,
            )
            .text("Orbit"),
        );
        ui.add(egui::Slider::new(&mut self.orbit.pitch, -1.5..=1.5).text("Tilt"));
        ui.add(
            egui::Slider::new(&mut self.orbit.zoom, 0.2..=10.0)
                .logarithmic(true)
                .text("Zoom"),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("Point ID");
            ui.add(
                egui::TextEdit::singleline(&mut self.inspect_id)
                    .desired_width(90.0)
                    .id_salt("inspect_point_id"),
            );
            if ui
                .add_sized([100.0, 44.0], egui::Button::new("Show point"))
                .clicked()
            {
                match self
                    .inspect_id
                    .parse::<u64>()
                    .ok()
                    .filter(|id| self.points.iter().any(|p| p.id == *id))
                {
                    Some(id) => {
                        self.pick(id);
                        self.error = None;
                    }
                    None => self.error = Some("Enter a point ID from this component.".into()),
                }
            }
        });
        self.show_evidence(ui);
        self.show_alignment(ui);
        egui::CollapsingHeader::new("Diagnostics and failed frames").show(ui, |ui| {
            let result = self.result.as_ref().unwrap();
            for diagnostic in &result.diagnostics {
                ui.label(diagnostic);
            }
            ui.label(format!(
                "{} evaluation frames withheld",
                result.evaluation_frame_ids.len()
            ));
            for frame in &result.unregistered_frame_ids {
                ui.label(format!("{frame} · unregistered"));
            }
            ui.label(&result.backend.version);
        });
    }

    fn show_cloud(&mut self, ui: &mut egui::Ui) {
        let colors = palette(ui);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 240.0),
            Sense::click_and_drag(),
        );
        response.clone().widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Other,ui.is_enabled(),"Sparse reconstruction. Use orbit, tilt, and zoom sliders; point IDs and frame menu are available below."));
        if response.dragged() {
            let delta = ui.input(|i| i.pointer.delta());
            self.orbit.yaw = (self.orbit.yaw + f64::from(delta.x) * 0.01 + std::f64::consts::PI)
                .rem_euclid(std::f64::consts::TAU)
                - std::f64::consts::PI;
            self.orbit.pitch = (self.orbit.pitch + f64::from(delta.y) * 0.01).clamp(-1.5, 1.5);
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.orbit.zoom =
                    (self.orbit.zoom * (f64::from(scroll) * 0.005).exp()).clamp(0.2, 10.0);
                ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
            }
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::from_gray(18));
        let transform = |p| {
            self.alignment
                .as_ref()
                .map(|a| a.transform_point(p))
                .unwrap_or(p)
        };
        let component = &self.result.as_ref().unwrap().components[self.component];
        let stride = self.points.len().div_ceil(20_000).max(1);
        let mut projected: Vec<_> = self
            .points
            .iter()
            .enumerate()
            .filter(|(index, p)| {
                index % stride == 0
                    || Some(p.id) == self.selected_point
                    || self.distance_ids.contains(&Some(p.id))
                    || self.floor_ids.contains(&Some(p.id))
            })
            .map(|(index, p)| {
                let (screen, depth) = self.orbit.project(transform(p.position), rect);
                (index, screen, depth)
            })
            .collect();
        projected.sort_by(|a, b| a.2.total_cmp(&b.2));
        let pointer = response.interact_pointer_pos();
        let mut nearest_point = None;
        let mut nearest_distance = 10.0f32;
        // Very large clouds use a deterministic sample; point IDs still access every anchor.
        for (index, screen, _) in &projected {
            if !rect.contains(*screen) {
                continue;
            }
            let point = &self.points[*index];
            let selected = Some(point.id) == self.selected_point
                || self.distance_ids.contains(&Some(point.id))
                || self.floor_ids.contains(&Some(point.id));
            painter.circle_filled(
                *screen,
                if selected { 3.5 } else { 1.5 },
                if selected {
                    colors.accent
                } else {
                    Color32::from_rgb(point.color[0], point.color[1], point.color[2])
                },
            );
            if let Some(pointer) = pointer {
                let distance = screen.distance(pointer);
                if distance <= nearest_distance {
                    nearest_point = Some(point.id);
                    nearest_distance = distance;
                }
            }
        }
        // Camera frusta use recovered orientation. Cyan markers distinguish cameras from points.
        let camera_color = Color32::from_rgb(35, 153, 190);
        let mut nearest_frame = None;
        let mut camera_distance = 12.0f32;
        for pose in &component.frames {
            let center = pose.center();
            let screen = self.orbit.project(transform(center), rect).0;
            let selected = Some(&pose.frame_id) == self.selected_frame.as_ref();
            if rect.contains(screen) {
                painter.circle_stroke(
                    screen,
                    if selected { 5.0 } else { 3.0 },
                    Stroke::new(1.5, camera_color),
                );
                if let Some(pointer) = pointer {
                    let d = screen.distance(pointer);
                    if d < camera_distance {
                        nearest_frame = Some(pose.frame_id.clone());
                        camera_distance = d;
                    }
                }
            }
            if selected {
                let camera = component
                    .cameras
                    .iter()
                    .find(|c| c.id == pose.camera_id)
                    .unwrap();
                let depth = self.orbit.radius * 0.1
                    / self
                        .alignment
                        .as_ref()
                        .map(|a| a.meters_per_unit)
                        .unwrap_or(1.0);
                let mut corners = Vec::new();
                for [u, v] in [
                    [0.0, 0.0],
                    [camera.width as f64, 0.0],
                    [camera.width as f64, camera.height as f64],
                    [0.0, camera.height as f64],
                ] {
                    let direction = pose.camera_to_reconstruction([
                        (u - camera.principal_point_pixels[0]) / camera.focal_length_pixels,
                        (v - camera.principal_point_pixels[1]) / camera.focal_length_pixels,
                        1.0,
                    ]);
                    let corner = std::array::from_fn(|i| center[i] + direction[i] * depth);
                    let corner = self.orbit.project(transform(corner), rect).0;
                    painter.line_segment([screen, corner], Stroke::new(1.0, camera_color));
                    corners.push(corner);
                }
                for i in 0..4 {
                    painter.line_segment(
                        [corners[i], corners[(i + 1) % 4]],
                        Stroke::new(1.0, camera_color),
                    );
                }
            }
        }
        if self.alignment.is_some() {
            let origin = self.orbit.project([0.0; 3], rect).0;
            for (axis, color, label) in [
                ([self.orbit.radius * 0.2, 0.0, 0.0], colors.axis_x, "X"),
                (
                    [0.0, self.orbit.radius * 0.2, 0.0],
                    Color32::from_rgb(65, 165, 85),
                    "Y",
                ),
                ([0.0, 0.0, self.orbit.radius * 0.2], camera_color, "Z"),
            ] {
                let end = self.orbit.project(axis, rect).0;
                painter.line_segment([origin, end], Stroke::new(2.0, color));
                painter.text(
                    end,
                    Align2::LEFT_BOTTOM,
                    label,
                    FontId::proportional(12.0),
                    color,
                );
            }
        }
        if response.clicked() {
            if self.pick_target.is_some()
                || nearest_frame.is_none()
                || nearest_distance < camera_distance
            {
                if let Some(id) = nearest_point {
                    self.pick(id);
                }
            } else {
                self.selected_frame = nearest_frame;
            }
        }
        if let Some(target) = self.pick_target {
            painter.text(
                rect.left_top() + egui::vec2(8.0, 8.0),
                Align2::LEFT_TOP,
                if target < 2 {
                    "Pick a measured-distance endpoint"
                } else {
                    "Pick a point on the floor"
                },
                FontId::proportional(13.0),
                Color32::WHITE,
            );
        }
    }

    fn show_evidence(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        let result = self.result.as_ref().unwrap();
        let component = &result.components[self.component];
        ui.horizontal_wrapped(|ui| {
            ui.label("Source view");
            egui::ComboBox::from_id_salt("room_camera_frame")
                .selected_text(self.selected_frame.as_deref().unwrap_or("Choose frame"))
                .show_ui(ui, |ui| {
                    for pose in &component.frames {
                        ui.selectable_value(
                            &mut self.selected_frame,
                            Some(pose.frame_id.clone()),
                            &pose.frame_id,
                        );
                    }
                });
        });
        let Some(id) = &self.selected_frame else {
            return;
        };
        if self.evidence_loaded.as_ref() != Some(id) {
            self.evidence_loaded = Some(id.clone());
            self.evidence = None;
            self.evidence_error = None;
            let input = result.inputs.iter().find(|f| &f.id == id).unwrap();
            let root = self.manifest.as_ref().unwrap().parent().unwrap();
            match capture_preview_path(root, &input.file).and_then(|p| {
                image::open(p).map_err(|_| "Source frame cannot be decoded.".to_owned())
            }) {
                Ok(image) => {
                    let image = image.thumbnail(960, 540).to_rgba8();
                    let size = [image.width() as usize, image.height() as usize];
                    let texture = ui.ctx().load_texture(
                        "reconstruction_source_view",
                        egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
                        egui::TextureOptions::LINEAR,
                    );
                    self.evidence = Some((id.clone(), texture));
                    self.evidence_error = None;
                }
                Err(error) => self.evidence_error = Some(error),
            }
        }
        if let Some(error) = &self.evidence_error {
            ui.label(error);
        }
        if let Some((_, texture)) = &self.evidence {
            let input = result.inputs.iter().find(|f| &f.id == id).unwrap();
            let size = texture.size_vec2()
                * (ui.available_width() / texture.size_vec2().x)
                    .min(160.0 / texture.size_vec2().y)
                    .min(1.0);
            let response = ui.add(
                egui::Image::from_texture(texture)
                    .fit_to_exact_size(size)
                    .sense(Sense::click())
                    .alt_text(format!("Recovered source camera {id}")),
            );
            if response.clicked()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let mut nearest = 10.0f32;
                for point in &self.points {
                    for observation in point.observations.iter().filter(|o| &o.frame_id == id) {
                        let pixel = response.rect.left_top()
                            + egui::vec2(
                                (observation.pixel[0] / input.width as f64) as f32
                                    * response.rect.width(),
                                (observation.pixel[1] / input.height as f64) as f32
                                    * response.rect.height(),
                            );
                        let distance = pixel.distance(pointer);
                        if distance < nearest {
                            nearest = distance;
                            picked = Some(point.id);
                        }
                    }
                }
            }
            if let Some(point) = self
                .selected_point
                .and_then(|id| self.points.iter().find(|p| p.id == id))
            {
                if let Some(observation) = point
                    .observations
                    .iter()
                    .find(|o| Some(&o.frame_id) == self.selected_frame.as_ref())
                {
                    let marker = response.rect.left_top()
                        + egui::vec2(
                            (observation.pixel[0] / input.width as f64) as f32
                                * response.rect.width(),
                            (observation.pixel[1] / input.height as f64) as f32
                                * response.rect.height(),
                        );
                    ui.painter()
                        .circle_stroke(marker, 6.0, Stroke::new(2.0, Color32::YELLOW));
                }
                ui.label(format!(
                    "Point {} · {} source views · {:.2}px reprojection error",
                    point.id,
                    point
                        .observations
                        .iter()
                        .map(|o| &o.frame_id)
                        .collect::<BTreeSet<_>>()
                        .len(),
                    point.reprojection_error_pixels
                ));
            }
        }
        let camera = component
            .cameras
            .iter()
            .find(|c| {
                component
                    .frames
                    .iter()
                    .any(|p| &p.frame_id == id && p.camera_id == c.id)
            })
            .unwrap();
        ui.label(format!(
            "Estimated focal length {:.1}px · radial distortion {:.4}",
            camera.focal_length_pixels, camera.radial_distortion
        ));
        ui.label("Click a feature in the photo to select its 3D point.");
        if let Some(id) = picked {
            self.pick(id);
        }
    }

    fn show_alignment(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Scale and floor alignment").default_open(self.alignment.is_some()).show(ui,|ui|{
            if let Some(measurements)=&self.measurements {
                for object in &measurements.objects {
                    ui.label(format!("{} · {:.4}m long × {:.4}m deep × {:.4}m high",object.label,object.length_meters,object.depth_meters,object.height_meters));
                    ui.horizontal_wrapped(|ui|{
                        for (label,value) in [("Use length",object.length_meters),("Use depth",object.depth_meters),("Use height",object.height_meters)]{
                            if ui.add_sized([100.0,44.0],egui::Button::new(label)).clicked(){self.distance_meters=value;self.notice=None;}
                        }
                    });
                }
            }
            ui.label("Pick two points with a measured real distance. Pick three floor points: origin, +X direction, and a third point defining the upward normal. Check the Y arrow before saving.");
            ui.label("The source view marks the selected point; confirm each anchor against the photograph.");
            for (index,label) in ["Distance start","Distance end","Floor origin","Floor +X","Floor third point"].iter().enumerate() {
                let id=if index<2{&mut self.distance_ids[index]}else{&mut self.floor_ids[index-2]};
                ui.horizontal_wrapped(|ui|{
                    ui.label(*label);
                    let mut value=id.map(|v|v.to_string()).unwrap_or_default();
                    let response=ui.add(egui::TextEdit::singleline(&mut value).desired_width(90.0).hint_text("Point ID").id_salt(("anchor",index)));
                    response.clone().widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::TextEdit,ui.is_enabled(),format!("{label} point ID")));
                    if response.changed(){*id=value.parse().ok();self.notice=None;}
                    if ui.add_sized([76.0,44.0],egui::Button::new(if self.pick_target==Some(index){"Picking…"}else{"Pick"})).clicked(){self.pick_target=if self.pick_target==Some(index){None}else{Some(index)};}
                    if ui.add_enabled(self.selected_point.is_some(),egui::Button::new("Use selected").min_size(egui::vec2(100.0,44.0))).clicked(){*id=self.selected_point;self.notice=None;}
                });
            }
            ui.horizontal_wrapped(|ui|{
                ui.label("Measured distance (meters)");
                ui.add(egui::DragValue::new(&mut self.distance_meters).speed(0.01).range(0.0..=10000.0));
                if ui.add_sized([100.0,44.0],egui::Button::new("Flip floor up")).clicked(){self.floor_ids.swap(1,2);self.notice=None;}
            });
            let anchors=self.distance_ids.iter().chain(self.floor_ids.iter()).all(Option::is_some);
            if ui.add_enabled(anchors && self.distance_meters>0.0,egui::Button::new("Preview alignment").min_size(egui::vec2(140.0,44.0))).clicked() {
                let component=&self.result.as_ref().unwrap().components[self.component];
                match create_alignment(self.manifest.as_ref().unwrap(),&component.id,&self.points,self.distance_ids.map(Option::unwrap),self.distance_meters,self.floor_ids.map(Option::unwrap)) {
                    Ok(alignment)=>{self.alignment=Some(alignment);self.error=None;self.notice=Some("Alignment previewed. Check scale and the upward Y arrow, then save.".into());self.reframe();}
                    Err(error)=>self.error=Some(error),
                }
            }
            if let Some(alignment)=&self.alignment {
                ui.label(format!("{:.6} meters per reconstruction unit",alignment.meters_per_unit));
                // Changed anchor inputs must be previewed before they can be saved.
                let matches=self.distance_ids==alignment.distance_point_ids.map(Some) && self.floor_ids==alignment.floor_point_ids.map(Some) && self.distance_meters==alignment.distance_meters;
                if ui.add_enabled(matches,egui::Button::new("Save reviewed alignment").min_size(egui::vec2(180.0,44.0))).clicked(){
                    match save_alignment(self.manifest.as_ref().unwrap(),alignment){
                        Ok(())=>{self.error=None;self.notice=Some("Alignment saved.".into());}
                        Err(error)=>self.error=Some(error),
                    }
                }
            }
            if let Some(notice)=&self.notice{ui.label(notice);}
        });
    }
}

struct Orbit {
    center: [f64; 3],
    radius: f64,
    yaw: f64,
    pitch: f64,
    zoom: f64,
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            center: [0.0; 3],
            radius: 1.0,
            yaw: 0.4,
            pitch: 0.2,
            zoom: 1.0,
        }
    }
}

impl Orbit {
    fn fit(points: Vec<[f64; 3]>) -> Self {
        if points.is_empty() {
            return Self::default();
        }
        // Robust framing keeps a few distant floaters from hiding the useful cloud.
        let mut min = [0.0; 3];
        let mut max = [0.0; 3];
        for i in 0..3 {
            let mut axis: Vec<_> = points.iter().map(|p| p[i]).collect();
            axis.sort_by(f64::total_cmp);
            min[i] = axis[axis.len() / 100];
            max[i] = axis[(axis.len() - 1) * 99 / 100];
        }
        Self {
            center: std::array::from_fn(|i| (min[i] + max[i]) * 0.5),
            radius: (0..3).map(|i| (max[i] - min[i]) * 0.5).fold(0.01, f64::max),
            ..Default::default()
        }
    }

    fn project(&self, p: [f64; 3], rect: Rect) -> (Pos2, f64) {
        let p = subtract(p, self.center);
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let x = cy * p[0] + sy * p[2];
        let z = -sy * p[0] + cy * p[2];
        let y = cp * p[1] - sp * z;
        let depth = sp * p[1] + cp * z;
        let scale = f64::from(rect.width().min(rect.height())) * 0.4 * self.zoom / self.radius;
        (
            rect.center() + egui::vec2((x * scale) as f32, (-y * scale) as f32),
            depth,
        )
    }
}

#[cfg(test)]
pub(super) fn test_review(mode: &str) -> CameraReviewState {
    use cubacadabra_room_capture::reconstruction::*;
    let poses = vec![
        CameraPose {
            frame_id: "frame-000001".into(),
            camera_id: 1,
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            translation: [0.0; 3],
        },
        CameraPose {
            frame_id: "frame-000002".into(),
            camera_id: 1,
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            translation: [-1.0, 0.0, 0.0],
        },
    ];
    let points = vec![SparsePoint {
        id: 1,
        position: [0.0, 0.0, 3.0],
        color: [100, 150, 200],
        reprojection_error_pixels: 0.5,
        observations: vec![
            PointObservation {
                frame_id: "frame-000001".into(),
                point_index: 1,
                pixel: [800.0, 450.0],
            },
            PointObservation {
                frame_id: "frame-000002".into(),
                point_index: 1,
                pixel: [400.0, 450.0],
            },
        ],
    }];
    let component = ReconstructionComponent {
        id: "component-000".into(),
        cameras: vec![CameraIntrinsics {
            id: 1,
            model: CameraModel::SimpleRadial,
            width: 1600,
            height: 900,
            focal_length_pixels: 1200.0,
            principal_point_pixels: [800.0, 450.0],
            radial_distortion: 0.0,
        }],
        frames: poses,
        point_shards: Vec::new(),
        point_count: 1,
        mean_point_reprojection_error_pixels: 0.5,
        median_point_reprojection_error_pixels: 0.5,
        median_track_angle_degrees: 3.0,
    };
    let result = Reconstruction {
        format_version: 1,
        capture_sha256: "0".repeat(64),
        source_video_sha256: "0".repeat(64),
        backend: ReconstructionBackend {
            name: "COLMAP".into(),
            version: "fixture".into(),
            adapter: "colmap-sparse-v1".into(),
            threads: 4,
            camera_model: "SIMPLE_RADIAL".into(),
            shared_intrinsics: true,
            use_gpu: false,
            sequential_overlap: 20,
            random_seed: 0,
        },
        inputs: vec![FrameInput {
            id: "frame-000001".into(),
            file: "frames/frame-000001.jpg".into(),
            sha256: "0".repeat(64),
            width: 1600,
            height: 900,
        }],
        evaluation_frame_ids: Vec::new(),
        unregistered_frame_ids: Vec::new(),
        components: vec![component],
        diagnostics: vec!["Depth needs wider camera movement and creator review.".into()],
    };
    let mut state = CameraReviewState {
        active: true,
        result: Some(result),
        points,
        manifest: Some(PathBuf::from(format!(
            "/missing/{}/reconstruction.json",
            "long name ".repeat(50)
        ))),
        selected_frame: Some("frame-000001".into()),
        ..Default::default()
    };
    state.reframe();
    match mode {
        "camera-alignment" => {
            state.measurements = Some(CaptureMeasurements {
                format_version: 1,
                capture_sha256: "0".repeat(64),
                objects: vec![cubacadabra_room_capture::measurements::MeasuredObject {
                    id: "desk".into(),
                    label: "Desk".into(),
                    length_meters: 1.8288,
                    depth_meters: 0.9144,
                    height_meters: 0.9144,
                }],
            });
            state.alignment = Some(Alignment {
                format_version: 1,
                reconstruction_sha256: "0".repeat(64),
                component_id: "component-000".into(),
                distance_point_ids: [1, 2],
                distance_meters: 1.2,
                floor_point_ids: [3, 4, 5],
                meters_per_unit: 0.5,
                rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                translation_meters: [0.0; 3],
            });
            state.distance_ids = [Some(1), Some(2)];
            state.floor_ids = [Some(3), Some(4), Some(5)];
            state.distance_meters = 1.2;
        }
        "camera-empty" => {
            state.result.as_mut().unwrap().components.clear();
        }
        "camera-error" => {
            state.result = None;
            state.error = Some("Install COLMAP on PATH, then retry camera recovery.".into());
        }
        _ => {}
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dropped_worker_cancels_and_disconnect_allows_retry() {
        let (send, messages) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut state = CameraReviewState {
            worker: Some(Worker {
                messages,
                cancelled: cancelled.clone(),
                started: Instant::now(),
            }),
            ..Default::default()
        };
        drop(send);
        state.poll();
        assert!(!state.is_busy());
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(state.error.unwrap().contains("Try again"));
    }
    #[test]
    fn framing_and_projection_handle_small_cloud_and_outliers() {
        let orbit = Orbit::fit(vec![[0.0; 3], [1.0; 3]]);
        assert!(orbit.radius > 0.0);
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(300.0, 200.0));
        assert_eq!(orbit.project(orbit.center, rect).0, rect.center());
        let mut cloud = vec![[1.0, 2.0, 3.0]; 1000];
        cloud.push([1e9; 3]);
        let orbit = Orbit::fit(cloud);
        assert_eq!(orbit.center, [1.0, 2.0, 3.0]);
    }
    #[test]
    fn point_selection_links_evidence_and_assigns_only_the_requested_anchor() {
        let mut state = test_review("camera-review");
        state.pick_target = Some(3);
        state.selected_frame = None;
        state.pick(1);
        assert_eq!(state.selected_point, Some(1));
        assert_eq!(state.selected_frame.as_deref(), Some("frame-000001"));
        assert_eq!(state.floor_ids, [None, Some(1), None]);
        assert_eq!(state.distance_ids, [None, None]);
        assert!(state.pick_target.is_none());
    }
}

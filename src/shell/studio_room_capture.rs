use super::*;
use cubacadabra_room_capture::{
    CaptureDataset, CaptureOptions, CaptureProgress, CaptureStage, capture_video_with_progress,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};

#[path = "studio_room_reconstruction.rs"]
mod camera_review;

enum CaptureMessage {
    Progress(CaptureProgress),
    Finished(Result<CaptureDataset, String>),
}

struct CaptureWorker {
    messages: Receiver<CaptureMessage>,
    cancelled: Arc<AtomicBool>,
    started: Instant,
}

impl Drop for CaptureWorker {
    fn drop(&mut self) {
        // Dropping the shell, including when opening a project, stops extraction.
        // The UI never joins a decoder thread during window shutdown.
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub(super) struct RoomCaptureState {
    pub(super) open: bool,
    source: Option<PathBuf>,
    parent: Option<PathBuf>,
    choose_source: bool,
    choose_parent: bool,
    choose_capture: bool,
    choose_reconstruction: bool,
    cameras: camera_review::CameraReviewState,
    worker: Option<CaptureWorker>,
    status: String,
    progress: Option<CaptureProgress>,
    error: Option<String>,
    dataset: Option<CaptureDataset>,
    output: Option<PathBuf>,
    selected: usize,
    preview: Option<(usize, egui::TextureHandle)>,
    preview_loaded: Option<usize>,
    preview_error: Option<String>,
}

impl StudioShell {
    #[cfg(debug_assertions)]
    pub(crate) fn probe_room_capture_start(&mut self, source: &Path, parent: &Path) {
        self.room_capture.source = Some(source.to_owned());
        self.room_capture.parent = Some(parent.to_owned());
        self.room_capture.start();
    }

    #[cfg(debug_assertions)]
    pub(crate) fn probe_room_capture_progress(&self) -> (Option<CaptureProgress>, bool) {
        assert!(
            self.room_capture.error.is_none(),
            "capture probe failed: {:?}",
            self.room_capture.error
        );
        (
            self.room_capture.progress,
            self.room_capture.dataset.is_some(),
        )
    }

    #[cfg(debug_assertions)]
    pub(crate) fn probe_room_capture_review(&mut self, manifest: &Path) {
        let dataset: CaptureDataset =
            serde_json::from_slice(&fs::read(manifest).expect("read capture probe dataset"))
                .expect("parse capture probe dataset");
        assert_eq!(
            dataset.format_version,
            cubacadabra_room_capture::CAPTURE_FORMAT_VERSION,
            "unsupported capture dataset version"
        );
        assert!(!dataset.frames.is_empty(), "capture probe needs frames");
        let output = manifest.parent().unwrap();
        for frame in &dataset.frames {
            capture_preview_path(output, &frame.file).expect("safe capture probe frame path");
        }
        self.room_capture.status = format!("Captured {} selected frames", dataset.frames.len());
        self.room_capture.dataset = Some(dataset);
        self.room_capture.output = Some(manifest.parent().unwrap().to_owned());
    }

    #[cfg(debug_assertions)]
    pub(crate) fn probe_room_capture_visible(&self, require_preview: bool) {
        assert!(self.room_capture.open, "Room Video modal must be open");
        if require_preview {
            assert!(
                self.room_capture.preview.is_some(),
                "real captured frame must decode: {:?}",
                self.room_capture.preview_error
            );
        }
    }

    #[cfg(debug_assertions)]
    pub(crate) fn probe_room_camera_review(&mut self, manifest: &Path) {
        self.room_capture.cameras.load(manifest);
        assert!(
            self.room_capture.cameras.has_result(),
            "camera reconstruction must load"
        );
    }

    #[cfg(debug_assertions)]
    pub(crate) fn probe_room_camera_visible(&self) {
        self.room_capture.cameras.assert_review_visible();
    }

    pub(crate) fn handle_room_capture_dialogs(&mut self, window: Option<&Window>) {
        if std::mem::take(&mut self.room_capture.choose_capture) {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Open room capture.json")
                .add_filter("Capture dataset", &["json"]);
            if let Some(window) = window {
                dialog = dialog.set_parent(window);
            }
            if let Some(path) = dialog.pick_file() {
                match cubacadabra_room_capture::reconstruction::read_capture(&path) {
                    Ok(dataset) => {
                        self.room_capture.clear_review();
                        self.room_capture.status =
                            format!("Captured {} selected frames", dataset.frames.len());
                        self.room_capture.dataset = Some(dataset);
                        self.room_capture.output = path.parent().map(Path::to_owned);
                        self.room_capture.error = None;
                    }
                    Err(error) => self.room_capture.error = Some(error),
                }
            }
        }
        if std::mem::take(&mut self.room_capture.choose_reconstruction) {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Open reconstruction.json")
                .add_filter("Camera reconstruction", &["json"]);
            if let Some(window) = window {
                dialog = dialog.set_parent(window);
            }
            if let Some(path) = dialog.pick_file() {
                self.room_capture.cameras.load(&path);
            }
        }
        if std::mem::take(&mut self.room_capture.choose_source) {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Choose a room video")
                .add_filter("Video", &["mov", "mp4", "m4v", "mkv", "avi", "webm"]);
            if let Some(window) = window {
                dialog = dialog.set_parent(window);
            }
            if let Some(path) = dialog.pick_file() {
                self.room_capture.clear_review();
                self.room_capture.source = Some(path);
                self.room_capture.error = None;
            }
        }
        if std::mem::take(&mut self.room_capture.choose_parent) {
            let mut dialog =
                rfd::FileDialog::new().set_title("Choose where to save capture source");
            if let Some(parent) = &self.room_capture.parent {
                dialog = dialog.set_directory(parent);
            }
            if let Some(window) = window {
                dialog = dialog.set_parent(window);
            }
            if let Some(path) = dialog.pick_folder() {
                self.room_capture.clear_review();
                self.room_capture.parent = Some(path);
                self.room_capture.error = None;
            }
        }
    }
}

impl RoomCaptureState {
    fn clear_review(&mut self) {
        self.cameras = camera_review::CameraReviewState::default();
        self.dataset = None;
        self.output = None;
        self.preview = None;
        self.preview_loaded = None;
        self.preview_error = None;
        self.selected = 0;
        self.status.clear();
        self.progress = None;
    }

    fn start(&mut self) {
        let (Some(source), Some(parent)) = (&self.source, &self.parent) else {
            return;
        };
        self.error = None;
        let output = match new_capture_path(source, parent) {
            Ok(path) => path,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let source = source.clone();
        let worker_output = output.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let (send, messages) = mpsc::channel();
        let started = Instant::now();
        let spawned = std::thread::Builder::new()
            .name("room-video-capture".to_owned())
            .spawn(move || {
                log::info!("Room video capture started: source={} output={}", source.display(), worker_output.display());
                let mut last_logged = None;
                let mut last_stage = None;
                let result = capture_video_with_progress(
                    &source,
                    &worker_output,
                    CaptureOptions::default(),
                    &worker_cancelled,
                    |progress| {
                        let now = Instant::now();
                        if last_stage != Some(progress.stage)
                            || last_logged.is_none_or(|last: Instant| now.duration_since(last) >= Duration::from_secs(5))
                        {
                            log::info!(
                                "Room video capture progress: step={}/5 stage={:?} {} elapsed={} step_elapsed={} step_remaining={}",
                                progress.stage.step(), progress.stage, capture_progress_detail(progress),
                                capture_duration(progress.elapsed), capture_duration(progress.stage_elapsed),
                                progress.estimated_remaining().map(capture_duration).unwrap_or_else(|| "estimating".to_owned()),
                            );
                            last_logged = Some(now);
                            last_stage = Some(progress.stage);
                        }
                        let _ = send.send(CaptureMessage::Progress(progress));
                    },
                );
                match &result {
                    Ok(dataset) => log::info!("Room video capture finished: candidates={} selected={} elapsed={} output={}",
                        dataset.candidate_count, dataset.frames.len(), capture_duration(started.elapsed()), worker_output.display()),
                    Err(error) => log::info!("Room video capture stopped: elapsed={} reason={error}", capture_duration(started.elapsed())),
                }
                let _ = send.send(CaptureMessage::Finished(result));
            });
        match spawned {
            Ok(_) => {
                self.worker = Some(CaptureWorker {
                    messages,
                    cancelled,
                    started,
                });
                self.output = Some(output);
                self.dataset = None;
                self.preview = None;
                self.preview_loaded = None;
                self.preview_error = None;
                self.selected = 0;
                self.status = "Reading video…".to_owned();
                self.progress = None;
            }
            Err(error) => self.error = Some(format!("Could not start capture: {error}")),
        }
    }

    fn poll(&mut self) {
        let mut finished = None;
        if let Some(worker) = &self.worker {
            loop {
                match worker.messages.try_recv() {
                    Ok(CaptureMessage::Progress(progress)) => self.progress = Some(progress),
                    Ok(CaptureMessage::Finished(result)) => {
                        finished = Some(result);
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        finished = Some(Err(
                            "Capture worker stopped unexpectedly. Try again.".to_owned()
                        ));
                        break;
                    }
                }
            }
        }
        if let Some(result) = finished {
            self.worker = None;
            match result {
                Ok(dataset) => {
                    self.status = format!("Captured {} selected frames", dataset.frames.len());
                    self.dataset = Some(dataset);
                }
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn cancel(&mut self) {
        self.cameras.cancel();
        if let Some(worker) = &self.worker {
            if !worker.cancelled.swap(true, Ordering::Relaxed) {
                log::info!(
                    "Room video capture cancellation requested: elapsed={}",
                    capture_duration(worker.started.elapsed())
                );
            }
            self.status = "Cancelling capture…".to_owned();
        }
    }

    fn show_progress(&self, ui: &mut egui::Ui) {
        let worker = self.worker.as_ref().unwrap();
        let cancelling = worker.cancelled.load(Ordering::Relaxed);
        let colors = palette(ui);
        if cancelling {
            ui.label("Cancelling capture…");
        } else if let Some(progress) = self.progress {
            ui.label(format!(
                "Step {} of 5 · {}",
                progress.stage.step(),
                progress.stage.label()
            ));
            ui.add(
                egui::ProgressBar::new(progress.fraction().unwrap_or(0.0))
                    .desired_width(ui.available_width())
                    .text(capture_progress_detail(progress)),
            );
        } else {
            ui.label(&self.status);
        }
        let remaining = if cancelling {
            "Waiting for capture to stop".to_owned()
        } else {
            self.progress
                .and_then(CaptureProgress::estimated_remaining)
                .map(|remaining| format!("About {} left in this step", capture_duration(remaining)))
                .unwrap_or_else(|| "Estimating time left in this step…".to_owned())
        };
        ui.label(
            RichText::new(format!(
                "{} elapsed · {remaining}",
                capture_duration(worker.started.elapsed())
            ))
            .color(colors.secondary_text),
        );
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }

    pub(super) fn show(&mut self, context: &egui::Context) -> Option<Rect> {
        self.poll();
        self.cameras.poll();
        if !self.open {
            return None;
        }
        let viewport = context.content_rect();
        let width = (viewport.width() - 64.0).clamp(120.0, 560.0);
        let mut close = false;
        let response = egui::Modal::new(egui::Id::new("room_video_capture"))
            .frame(Frame::window(&context.style_of(context.theme())).inner_margin(Margin::same(16)))
            .show(context, |ui| {
                ui.set_width(width);
                let colors = palette(ui);
                ui.label(RichText::new("Room Video").font(semibold_font(18.0)));
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .max_height((viewport.height() - 140.0).max(80.0))
                    .show(ui, |ui| {
                        if self.cameras.active {
                            self.cameras.show(ui);
                            if !self.cameras.is_busy() {
                                if self.dataset.is_some() && ui.add_sized([140.0, 44.0], egui::Button::new("Back to frames")).clicked() {
                                    self.cameras.active = false;
                                }
                                if ui.add_sized([140.0, 44.0], egui::Button::new("Start another capture")).clicked() { self.clear_review(); }
                            }
                            return;
                        }
                        if self.dataset.is_some() {
                            self.show_review(ui);
                            ui.horizontal_wrapped(|ui| {
                                if ui.add_sized([140.0, 44.0], egui::Button::new("Recover cameras")).clicked() {
                                    if let Some(output) = &self.output { self.cameras.start(&output.join("capture.json")); }
                                }
                                if self.cameras.has_result() && ui.add_sized([140.0, 44.0], egui::Button::new("Review cameras")).clicked() { self.cameras.active = true; }
                                if ui.add_sized([140.0, 44.0], egui::Button::new("Open reconstruction…")).clicked() { self.choose_reconstruction = true; }
                            });
                            if let Some(error) = &self.error { ui.label(RichText::new(error).color(colors.axis_x)); }
                            if ui.add_sized([140.0, 44.0], egui::Button::new("Start another capture")).clicked() {
                                self.clear_review();
                            }
                            return;
                        }
                        ui.label("Extract sharp frames for camera recovery and room reconstruction.");
                        ui.add_enabled_ui(self.worker.is_none(), |ui| {
                            ui.horizontal_wrapped(|ui| {
                                if ui.add_sized([140.0, 44.0], egui::Button::new("Open capture…")).clicked() { self.choose_capture = true; }
                                if ui.add_sized([140.0, 44.0], egui::Button::new("Open reconstruction…")).clicked() { self.choose_reconstruction = true; }
                            });
                        });
                        ui.add_space(8.0);
                        let busy = self.worker.is_some();
                        ui.add_enabled_ui(!busy, |ui| {
                            capture_path_row(ui, "Video", self.source.as_deref(), &mut self.choose_source);
                            capture_path_row(ui, "Save in", self.parent.as_deref(), &mut self.choose_parent);
                        });
                        ui.label(RichText::new("Creates a new capture folder with source metadata and selected frames.")
                            .color(colors.secondary_text));
                        let options = CaptureOptions::default();
                        ui.label(RichText::new(format!("Up to {} frames · {} px maximum dimension",
                            options.max_frames, options.max_dimension)).color(colors.secondary_text));
                        ui.add_space(8.0);
                        if busy {
                            self.show_progress(ui);
                            let cancelling = self.worker.as_ref().unwrap().cancelled.load(Ordering::Relaxed);
                            if ui.add_enabled(!cancelling, egui::Button::new("Cancel capture")
                                .min_size(egui::vec2(100.0, 44.0))).clicked() {
                                self.cancel();
                            }
                        } else if ui.add_enabled(
                            self.source.is_some() && self.parent.is_some(),
                            egui::Button::new("Extract frames")
                                .min_size(egui::vec2(120.0, 44.0)),
                        ).clicked() {
                            self.start();
                        }
                        if let Some(error) = &self.error {
                            ui.label(RichText::new(error).color(colors.axis_x));
                        }
                    });
                ui.add_space(8.0);
                if ui.add_sized([80.0, 44.0], egui::Button::new("Close")).clicked() {
                    close = true;
                }
            });
        if close || response.should_close() {
            self.cancel();
            self.open = false;
        }
        Some(response.response.rect)
    }

    fn show_review(&mut self, ui: &mut egui::Ui) {
        let dataset = self.dataset.as_ref().unwrap();
        ui.add_space(8.0);
        ui.label(RichText::new(&self.status).strong());
        ui.label(format!(
            "{} · {} × {} · {:.1} seconds · {}",
            dataset.source.filename,
            dataset.source.video.width,
            dataset.source.video.height,
            dataset.source.video.duration_seconds,
            dataset.source.video.codec
        ));
        if let Some(output) = &self.output {
            ui.label(format!("Saved to {}", output.display()));
        }
        if !dataset.frames.is_empty() {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.selected > 0,
                        egui::Button::new("Previous").min_size(egui::vec2(76.0, 44.0)),
                    )
                    .clicked()
                {
                    self.selected -= 1;
                }
                if ui
                    .add_enabled(
                        self.selected + 1 < dataset.frames.len(),
                        egui::Button::new("Next").min_size(egui::vec2(76.0, 44.0)),
                    )
                    .clicked()
                {
                    self.selected += 1;
                }
                ui.label(format!("{} / {}", self.selected + 1, dataset.frames.len()));
            });
            // Navigation can change selection in this frame.
            let frame = &dataset.frames[self.selected];
            ui.label(format!(
                "{:.2}s · {} · sharpness {:.1}",
                frame.timestamp_seconds,
                if frame.evaluation {
                    "Evaluation"
                } else {
                    "Reconstruction"
                },
                frame.sharpness
            ));
            if self.preview_loaded != Some(self.selected) {
                self.preview = None;
                self.preview_loaded = Some(self.selected);
                self.preview_error = None;
                if let Some(output) = &self.output {
                    match capture_preview_path(output, &frame.file)
                        .and_then(|path| image::open(path).map_err(|error| error.to_string()))
                    {
                        Ok(image) => {
                            let image = image.thumbnail(960, 540).to_rgba8();
                            let size = [image.width() as usize, image.height() as usize];
                            let texture = ui.ctx().load_texture(
                                "room_capture_frame",
                                egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
                                egui::TextureOptions::LINEAR,
                            );
                            self.preview = Some((self.selected, texture));
                        }
                        Err(error) => {
                            self.preview_error = Some(format!("Could not preview frame: {error}"))
                        }
                    }
                }
            }
            if let Some((_, texture)) = &self.preview {
                ui.add(
                    egui::Image::from_texture(texture)
                        .fit_to_exact_size(
                            texture.size_vec2()
                                * (ui.available_width() / texture.size_vec2().x)
                                    .min(200.0 / texture.size_vec2().y)
                                    .min(1.0),
                        )
                        .alt_text(format!(
                            "Selected room frame at {:.2} seconds",
                            frame.timestamp_seconds
                        )),
                );
            }
            if let Some(error) = &self.preview_error {
                ui.label(error);
            }
        }
        for diagnostic in &dataset.diagnostics {
            ui.label(diagnostic);
        }
        ui.add_space(8.0);
        ui.label("Next: recover cameras and set a measured distance.");
    }
}

fn capture_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 3600 {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

fn capture_progress_detail(progress: CaptureProgress) -> String {
    let Some(total) = progress.total else {
        return "Reading video metadata…".to_owned();
    };
    let completed = progress.completed.min(total);
    let percent = progress.fraction().unwrap_or(0.0) * 100.0;
    match progress.stage {
        CaptureStage::Hashing => format!(
            "{:.1} / {:.1} MB hashed · {percent:.0}%",
            completed as f64 / 1_000_000.0,
            total as f64 / 1_000_000.0
        ),
        CaptureStage::Decoding => format!(
            "{} / {} of video decoded · {percent:.0}%",
            capture_duration(Duration::from_millis(completed)),
            capture_duration(Duration::from_millis(total))
        ),
        CaptureStage::Selecting => format!("{completed} / {total} frames checked · {percent:.0}%"),
        CaptureStage::Saving => format!(
            "{} / {} frames saved · {percent:.0}%",
            completed.min(total.saturating_sub(1)),
            total.saturating_sub(1)
        ),
        CaptureStage::Complete => "Capture complete · 100%".to_owned(),
        CaptureStage::Inspecting => "Reading video metadata…".to_owned(),
    }
}

fn capture_path_row(ui: &mut egui::Ui, label: &str, path: Option<&Path>, requested: &mut bool) {
    ui.label(RichText::new(label).strong());
    ui.horizontal(|ui| {
        let choose = ui
            .add_sized([84.0, 44.0], egui::Button::new("Choose…"))
            .on_hover_text(format!("Choose {label}"));
        choose.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                ui.is_enabled(),
                format!("Choose {label}"),
            )
        });
        if choose.clicked() {
            *requested = true;
        }
        let text = path
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Choose a location".to_owned());
        ui.add_sized(
            [ui.available_width().max(0.0), 44.0],
            egui::Label::new(&text).truncate(),
        )
        .on_hover_text(text);
    });
}

fn capture_preview_path(output: &Path, file: &str) -> Result<PathBuf, String> {
    let digits = file
        .strip_prefix("frames/frame-")
        .and_then(|name| name.strip_suffix(".jpg"))
        .ok_or("Invalid capture frame path")?;
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Invalid capture frame path".to_owned());
    }
    let root = output.canonicalize().map_err(|error| error.to_string())?;
    let path = root
        .join(file)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !path.starts_with(&root) {
        return Err("Capture frame path points outside the dataset".to_owned());
    }
    Ok(path)
}

fn new_capture_path(source: &Path, parent: &Path) -> Result<PathBuf, String> {
    let parent = parent
        .canonicalize()
        .map_err(|error| format!("Cannot use capture location: {error}"))?;
    if !parent.is_dir() {
        return Err("Choose a folder for the capture source.".to_owned());
    }
    if parent.components().any(|part| {
        part.as_os_str().eq_ignore_ascii_case("runtime")
            || part.as_os_str().eq_ignore_ascii_case("assets")
    }) {
        return Err(
            "Capture data is authoring source. Choose a folder outside runtime and assets directories.".to_owned(),
        );
    }
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    let stem: String = stem
        .chars()
        .take(64)
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect();
    let stem = if stem.is_empty() { "room" } else { &stem };
    let mut unique = [0u8; 8];
    getrandom::fill(&mut unique)
        .map_err(|error| format!("Cannot create a unique capture name: {error}"))?;
    Ok(parent.join(format!(
        "{stem}-capture-{:016x}",
        u64::from_le_bytes(unique)
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_or_dropped_capture_cancels_without_waiting() {
        let (_send, messages) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut state = RoomCaptureState {
            worker: Some(CaptureWorker {
                messages,
                cancelled: cancelled.clone(),
                started: Instant::now(),
            }),
            ..Default::default()
        };
        state.cancel();
        assert!(cancelled.load(Ordering::Relaxed));
        cancelled.store(false, Ordering::Relaxed);
        drop(state);
        assert!(cancelled.load(Ordering::Relaxed));
    }

    #[test]
    fn disconnected_worker_exits_busy_state_and_allows_retry() {
        let (send, messages) = mpsc::channel();
        let mut state = RoomCaptureState {
            worker: Some(CaptureWorker {
                messages,
                cancelled: Arc::new(AtomicBool::new(false)),
                started: Instant::now(),
            }),
            ..Default::default()
        };
        drop(send);
        state.poll();
        assert!(state.worker.is_none());
        assert!(state.error.as_deref().unwrap().contains("Try again"));
    }

    #[test]
    fn queued_progress_does_not_hide_cancellation_and_retry_clears_it() {
        let (send, messages) = mpsc::channel();
        let mut state = RoomCaptureState {
            worker: Some(CaptureWorker {
                messages,
                cancelled: Arc::new(AtomicBool::new(false)),
                started: Instant::now(),
            }),
            ..Default::default()
        };
        state.cancel();
        send.send(CaptureMessage::Progress(CaptureProgress {
            stage: CaptureStage::Selecting,
            completed: 120,
            total: Some(540),
            elapsed: Duration::from_secs(60),
            stage_elapsed: Duration::from_secs(20),
        }))
        .unwrap();
        state.poll();
        assert_eq!(state.status, "Cancelling capture…");
        assert_eq!(state.progress.unwrap().completed, 120);
        send.send(CaptureMessage::Finished(Err(
            "Capture cancelled.".to_owned()
        )))
        .unwrap();
        state.poll();
        assert!(state.worker.is_none());
        assert!(state.error.as_deref().unwrap().contains("cancelled"));
        state.clear_review();
        assert!(state.progress.is_none());
    }

    #[test]
    fn capture_output_is_unique_and_outside_published_asset_trees() {
        let parent = std::env::temp_dir();
        let first = new_capture_path(Path::new("a room.mov"), &parent).unwrap();
        let second = new_capture_path(Path::new("a room.mov"), &parent).unwrap();
        assert_ne!(first, second);
        assert!(
            first
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("a-room-capture-")
        );
        assert!(!first.exists());
        let root = parent.join(format!("studio-capture-test-{}", std::process::id()));
        for name in ["runtime", "assets"] {
            let folder = root.join(name);
            fs::create_dir_all(&folder).unwrap();
            assert!(new_capture_path(Path::new("room.mov"), &folder).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn capture_preview_rejects_paths_outside_selected_frames() {
        for file in [
            "../frame.jpg",
            "/tmp/frame.jpg",
            "frames/../../frame.jpg",
            "frames/frame-000001.png",
            "frames/frame-a00001.jpg",
        ] {
            assert!(capture_preview_path(Path::new("/missing-dataset"), file).is_err());
        }
    }

    #[test]
    fn room_video_modal_fits_mobile_tablet_and_desktop_with_long_paths() {
        for size in [
            [390.0, 844.0],
            [768.0, 1024.0],
            [1280.0, 800.0],
            [1440.0, 900.0],
        ] {
            for mode in [
                "ready",
                "review",
                "inspecting",
                "hashing",
                "decoding",
                "selecting",
                "saving",
                "cancelling",
                "camera-review",
                "camera-alignment",
                "camera-empty",
                "camera-error",
            ] {
                let context = egui::Context::default();
                configure_context(&context);
                let mut state = RoomCaptureState {
                    open: true,
                    source: Some(PathBuf::from(format!("/Volumes/{}/room.mov", "Long folder ".repeat(50)))),
                    parent: Some(PathBuf::from(format!("/Volumes/{}/captures", "Long folder ".repeat(50)))),
                    error: Some("A useful error that may wrap on narrow screens. Choose a valid video and try extraction again.".to_owned()),
                    ..Default::default()
                };
                let (_send, messages) = mpsc::channel();
                if !matches!(mode, "ready" | "review") && !mode.starts_with("camera-") {
                    let stage = match mode {
                        "inspecting" => CaptureStage::Inspecting,
                        "hashing" => CaptureStage::Hashing,
                        "decoding" => CaptureStage::Decoding,
                        "saving" => CaptureStage::Saving,
                        _ => CaptureStage::Selecting,
                    };
                    state.worker = Some(CaptureWorker {
                        messages,
                        cancelled: Arc::new(AtomicBool::new(mode == "cancelling")),
                        started: Instant::now() - Duration::from_secs(70),
                    });
                    state.progress = Some(CaptureProgress {
                        stage,
                        completed: 123,
                        total: if mode == "inspecting" {
                            None
                        } else {
                            Some(540)
                        },
                        elapsed: Duration::from_secs(70),
                        stage_elapsed: Duration::from_secs(20),
                    });
                    state.error = None;
                }
                if mode == "review" {
                    state.dataset = Some(serde_json::from_value(serde_json::json!({
                        "formatVersion": 1,
                        "source": {"filename": "room.mov", "sha256": "00", "bytes": 100,
                            "video": {"width": 1920, "height": 1080, "durationSeconds": 20.0, "codec": "h264", "rotationDegrees": 0,
                                "frameRate": "unknown", "pixelFormat": "unknown", "colorTransfer": "unknown", "colorPrimaries": "unknown", "colorRange": "unknown", "sampleAspectRatio": "unknown"}},
                        "settings": {"maxFrames": 180, "maxDimension": 1600},
                        "decoder": "ffmpeg", "selector": "sharpness", "scaleMetersPerUnit": null,
                        "candidateCount": 1,
                        "frames": [{"id": "frame-1", "file": "missing.png", "timestampSeconds": 1.2,
                            "sharpness": 40.0, "evaluation": false, "width": 1600, "height": 900}],
                        "diagnostics": ["Metric scale needs a measured distance."]
                    })).unwrap());
                    state.output = state.parent.clone();
                }
                if mode.starts_with("camera-") {
                    state.cameras = camera_review::test_review(mode);
                }
                let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(size[0], size[1]));
                let mut modal = Rect::NOTHING;
                // Egui resolves centered modal placement after the first pass.
                for _ in 0..3 {
                    let _ = context.run_ui(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |_| {
                            modal = state.show(&context).unwrap();
                        },
                    );
                }
                assert!(
                    screen.contains_rect(modal),
                    "{size:?}, mode={mode}: {modal:?}"
                );
                assert!(modal.width() <= 600.0);
            }
        }
    }
}

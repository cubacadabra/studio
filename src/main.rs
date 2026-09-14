use cubacadabra_client::{ClientAction, ClientSession, native::Renderer};
use image::{GenericImage, RgbaImage, imageops::FilterType};
use log::{debug, error, info, warn};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    env,
    error::Error,
    fmt, fs,
    path::{Component, Path, PathBuf},
    sync::mpsc,
    thread,
    time::Instant,
};
mod app;
mod assets;
mod codex;
mod game_creator;
mod input;
mod lifecycle;
#[cfg(target_os = "macos")]
mod macos;
mod morph_application;
mod morphs;
mod network;
mod options;
mod project;
mod shell;
mod wardrobe;
#[cfg(test)]
mod wardrobe_tests;
use assets::*;
use codex::{CodexClient, CodexEvent};
use cubacadabra_morphs::decode_morph_pack;
use input::*;
use morphs::{
    MorphGlbPreviewMesh, compile_source_morph_pack, decode_source_glb_preview,
    decode_source_glb_preview_node, encode_morph_thumbnail_png, inspect_source_glb_structure,
    inspect_source_sidecar, is_morph_draft_json, parse_morph_draft_json, source_manifest_asset,
    source_manifest_geometry_file,
};
use network::{BackendClient, BackendEvent};
use options::*;
use project::*;
use shell::{PreparedShell, SceneEditRequest, StudioShell};
#[cfg(target_os = "macos")]
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::{Window, WindowAttributes},
};

const DEFAULT_WINDOW_WIDTH: f64 = 1280.0;
const DEFAULT_WINDOW_HEIGHT: f64 = 800.0;
const MAX_ATLAS_DIMENSION: u32 = 2048;
const MAX_ATLAS_IMAGE_DIMENSION: u32 = 1020;
const ATLAS_PADDING: u32 = 2;
const STANDALONE_PREVIEW_ROOT: &str = "Morph Preview";
const STANDALONE_PREVIEW_MANIFEST: &str = r#"{
  "id": "studio-morph-preview",
  "version": "0.0.0",
  "sdkVersion": "0.3.0",
  "package": {
    "formatVersion": 3,
    "entry": "game.luau"
  },
  "displayName": "Morph Preview",
  "lobby": false,
  "startWorld": "lobby",
  "launch": {
    "destinationWorld": "lobby",
    "authoritative": false
  },
  "world": {
    "groundSize": 12,
    "gridSize": 0,
    "gridDivisions": 0,
    "spawn": [0, 0, 0],
    "showSpawnPad": false
  }
}"#;
const STANDALONE_PREVIEW_SCRIPT: &str = "return {}";

#[derive(Debug)]
struct StudioError(String);

impl fmt::Display for StudioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for StudioError {}

#[derive(Debug)]
struct ImageAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    regions: std::collections::BTreeMap<String, [f32; 4]>,
}

struct GameSources {
    project_root: PathBuf,
    root: PathBuf,
    authored_manifest_source: String,
    manifest_source: String,
    script_source: String,
    standalone_preview: bool,
    temporary_package: Option<PathBuf>,
}

struct BackgroundProjectLoad {
    sources: GameSources,
    image_atlas: Option<ImageAtlas>,
    local_morph_catalog: Option<LocalMorphCatalog>,
}

struct PreparedProjectLoad {
    background: BackgroundProjectLoad,
    client: ClientSession,
    network: BackendClient,
}

enum ProjectLoadEvent {
    Progress(f32),
    Finished(Result<BackgroundProjectLoad, String>),
}

struct PendingProjectLoad {
    project: PathBuf,
    preserve_editor: bool,
    codex_rebuild: bool,
    receiver: mpsc::Receiver<ProjectLoadEvent>,
}

type ProjectFileSnapshot = BTreeMap<PathBuf, Vec<u8>>;

#[derive(Clone)]
struct CodexFileChange {
    relative_path: PathBuf,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
}

struct LocalMorphCatalog {
    catalog: cubacadabra_morphs::MorphCatalog,
    packs: BTreeMap<cubacadabra_morphs::MorphAssetId, Vec<u8>>,
    thumbnails: BTreeMap<String, Vec<u8>>,
    initial_preset: Option<cubacadabra_morphs::MorphAssetId>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalMorphCatalogFile {
    #[allow(dead_code)]
    schema_version: u16,
    #[serde(default)]
    assets: Vec<LocalMorphAsset>,
    #[serde(default)]
    builtins: Option<String>,
    #[serde(default)]
    exclude_builtin_kinds: Vec<cubacadabra_morphs::MorphAssetKind>,
    #[serde(default)]
    presets: Vec<LocalMorphPreset>,
}

#[derive(Debug, serde::Deserialize)]
struct LocalMorphAsset {
    id: cubacadabra_morphs::MorphAssetId,
    source: String,
}

#[derive(Debug, serde::Deserialize)]
struct LocalMorphPreset {
    source: String,
}

struct StudioApp {
    project_root: PathBuf,
    game_root: PathBuf,
    authored_manifest_source: String,
    manifest_source: String,
    standalone_preview: bool,
    temporary_package: Option<PathBuf>,
    codex: CodexClient,
    network: BackendClient,
    client: ClientSession,
    image_atlas: Option<ImageAtlas>,
    window: Option<Window>,
    renderer: Option<Renderer>,
    shell: Option<StudioShell>,
    pending_project_load: Option<PendingProjectLoad>,
    background_project_ready: Option<(PathBuf, bool, bool, Result<BackgroundProjectLoad, String>)>,
    prepared_project_ready: Option<(PathBuf, bool, bool, Result<PreparedProjectLoad, String>)>,
    renderer_uses_base_package_generation: bool,
    codex_checkpoint: Option<ProjectFileSnapshot>,
    codex_changes: Option<Vec<CodexFileChange>>,
    local_morph_catalog: Option<LocalMorphCatalog>,
    pressed_keys: HashSet<KeyCode>,
    jump_queued: bool,
    mobile_sprint: bool,
    morph_loadout: cubacadabra_morphs::MorphLoadout,
    morph_request_serial: u64,
    pending_morph: Option<(u64, cubacadabra_morphs::MorphLoadout)>,
    registered_morphs: HashSet<String>,
    climb: bool,
    joystick_input: (f32, f32),
    pointer_position: Option<(f32, f32)>,
    pointer_active: bool,
    camera_pointer_active: bool,
    movement_pointer_active: bool,
    movement_pointer_origin: Option<(f32, f32)>,
    ui_pointer_active: bool,
    look_delta: (f32, f32),
    zoom_delta: f32,
    last_frame: Instant,
}

#[cfg(test)]
mod tests {
    use super::{
        ProjectFileSnapshot, STANDALONE_PREVIEW_MANIFEST, diff_project_files, joystick_movement,
        load_game_sources, load_local_morph_catalog, load_project_in_background,
        project_asset_slug, project_manifest, should_forward_gameplay_keyboard,
        update_manifest_sign_text, update_project_morph_catalog,
    };
    use crate::game_creator;
    use std::{fs, path::Path};

    #[test]
    fn standalone_sources_load_as_a_default_person_preview() {
        let sources = load_game_sources(None).expect("standalone sources");
        assert!(sources.standalone_preview);
        let client = cubacadabra_client::ClientSession::load(
            &sources.manifest_source,
            &sources.script_source,
        )
        .expect("standalone client");
        assert_eq!(client.game_id(), "studio-morph-preview");
        assert_eq!(client.engine().active_world_id(), Some("lobby"));
    }

    #[test]
    fn raw_game_projects_load_through_the_shared_builder() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../first-game");
        let sources = load_game_sources(Some(path)).expect("raw game project");
        let client = cubacadabra_client::ClientSession::load(
            &sources.manifest_source,
            &sources.script_source,
        )
        .expect("built raw project");
        assert_eq!(client.game_id(), "first-game");
        assert!(sources.script_source.contains("begin module: round.luau"));
        assert!(!sources.script_source.contains("@include"));
    }

    #[test]
    fn generated_starter_builds_through_the_shared_builder() {
        let root = std::env::temp_dir().join(format!(
            "cubacadabra-studio-starter-build-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let created = game_creator::create_game("Jump Course", &root).expect("starter project");
        let sources = load_game_sources(Some(created.project.clone())).expect("built starter");
        let client = cubacadabra_client::ClientSession::load(
            &sources.manifest_source,
            &sources.script_source,
        )
        .expect("starter package should compile");
        assert_eq!(client.game_id(), "jump-course");
        if let Some(package) = sources.temporary_package {
            let _ = fs::remove_dir_all(package);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sign_text_edit_preserves_the_authored_sign_shape() {
        let mut manifest = serde_json::json!({
            "worlds": {
                "course": {
                    "signs": [{
                        "text": "Before",
                        "position": [1, 2, 3],
                        "yaw": 0.5,
                        "maxWidth": 4.0,
                        "color": "paper"
                    }]
                }
            }
        });
        update_manifest_sign_text(&mut manifest, "world/course/signs/0", "After".to_owned())
            .expect("sign should be editable");
        let sign = &manifest["worlds"]["course"]["signs"][0];
        assert_eq!(sign["text"], "After");
        assert_eq!(sign["position"], serde_json::json!([1, 2, 3]));
        assert_eq!(sign["yaw"], 0.5);
        assert_eq!(sign["maxWidth"], 4.0);
        assert_eq!(sign["color"], "paper");
    }

    #[test]
    fn codex_change_diff_tracks_created_deleted_and_updated_files() {
        let mut before = ProjectFileSnapshot::new();
        before.insert("src/main.luau".into(), b"old".to_vec());
        before.insert("manifest.json".into(), b"same".to_vec());
        before.insert("src/old-ui.luau".into(), b"deleted".to_vec());
        let mut after = ProjectFileSnapshot::new();
        after.insert("src/main.luau".into(), b"new".to_vec());
        after.insert("manifest.json".into(), b"same".to_vec());
        after.insert("src/ui.luau".into(), b"created".to_vec());
        let changes = diff_project_files(before, after);
        assert_eq!(
            changes
                .iter()
                .map(|change| change.relative_path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["src/main.luau", "src/old-ui.luau", "src/ui.luau"]
        );
    }

    #[test]
    fn project_folders_require_a_manifest() {
        let valid = Path::new(env!("CARGO_MANIFEST_DIR")).join("../first-game");
        assert_eq!(
            project_manifest(&valid).unwrap(),
            valid.join("manifest.json")
        );

        let missing = std::env::temp_dir().join(format!(
            "cubacadabra-studio-no-manifest-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&missing);
        fs::create_dir(&missing).unwrap();
        let error = project_manifest(&missing).unwrap_err();
        assert!(error.contains("containing manifest.json"));
        let _ = fs::remove_dir(missing);
    }

    #[test]
    fn background_project_loader_reports_monotonic_real_phases() {
        let project = std::env::temp_dir().join(format!(
            "cubacadabra-studio-progress-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&project);
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("manifest.json"), STANDALONE_PREVIEW_MANIFEST).unwrap();
        fs::write(project.join("game.luau"), "return {}").unwrap();

        let mut progress = Vec::new();
        let result = load_project_in_background(&project, |value| progress.push(value));
        assert!(result.is_ok());
        assert!(progress.windows(2).all(|values| values[0] <= values[1]));
        assert_eq!(progress.first().copied(), Some(0.04));
        assert_eq!(progress.last().copied(), Some(0.88));
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn local_study_catalog_compiles_current_source_assets() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tools/starter-set/studies/mockup-person/catalog.json");
        let local = load_local_morph_catalog(&path).expect("mockup-person local catalog");
        assert_eq!(
            local.initial_preset.as_ref().unwrap().as_str(),
            "cuba:preset/mockup-person.v1"
        );
        assert_eq!(local.packs.len(), 5);
        assert_eq!(
            local
                .catalog
                .asset(
                    &cubacadabra_morphs::MorphAssetId::parse("cuba:base/study-person.v1").unwrap()
                )
                .unwrap()
                .display_name,
            "Studio study base"
        );
    }

    #[test]
    fn local_starter_catalog_loads_runtime_thumbnails() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/starter-set/catalog.json");
        let local = load_local_morph_catalog(&path).expect("starter catalog");
        assert_eq!(local.catalog.presets.len(), 24);
        assert_eq!(local.thumbnails.len(), 24);
        assert!(
            local
                .thumbnails
                .get("thumbnails/person-05.png")
                .is_some_and(|bytes| bytes.starts_with(b"\x89PNG\r\n\x1a\n"))
        );
    }

    #[test]
    fn project_catalog_upserts_a_stable_source_reference() {
        let root = std::env::temp_dir().join(format!(
            "cubacadabra-studio-catalog-test-{}",
            std::process::id()
        ));
        let path = root.join("assets/characters/catalog.json");
        let _ = fs::remove_dir_all(&root);
        update_project_morph_catalog(
            &path,
            "cuba:headwear/test-hat.v1",
            "test-hat/source.morph.json",
        )
        .expect("project catalog should be written");
        update_project_morph_catalog(
            &path,
            "cuba:headwear/test-hat.v1",
            "test-hat/source.morph.json",
        )
        .expect("project catalog should upsert");
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["assets"].as_array().unwrap().len(), 1);
        assert_eq!(value["assets"][0]["source"], "test-hat/source.morph.json");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn project_asset_slugs_are_safe_and_deterministic() {
        assert_eq!(
            project_asset_slug("cuba:headwear/test-hat.v1").unwrap(),
            "cuba-headwear-test-hat-v1"
        );
    }

    #[test]
    fn play_mode_bypasses_stale_shell_keyboard_capture() {
        assert!(should_forward_gameplay_keyboard(true, true));
    }

    #[test]
    fn paused_editor_still_honors_shell_keyboard_capture() {
        assert!(!should_forward_gameplay_keyboard(false, true));
        assert!(should_forward_gameplay_keyboard(false, false));
    }

    #[test]
    fn preview_drag_preserves_screen_space_movement_directions() {
        assert_eq!(joystick_movement((1.0, 0.0)), (0.0, 1.0));
        assert_eq!(joystick_movement((-1.0, 0.0)), (0.0, -1.0));
        assert_eq!(joystick_movement((0.0, -1.0)), (1.0, 0.0));
        assert_eq!(joystick_movement((0.0, 1.0)), (-1.0, 0.0));
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    debug!("Studio debug logging initialized");
    let options = parse_options()?;
    if let Some(path) = options.validate_project_path {
        return validate_project(path);
    }
    let mut app = StudioApp::load(options.game_path, options.morph_catalog_path)?;
    let mut event_loop_builder = EventLoop::builder();
    #[cfg(target_os = "macos")]
    event_loop_builder.with_activation_policy(ActivationPolicy::Regular);
    let event_loop = event_loop_builder.build()?;
    #[cfg(target_os = "macos")]
    macos::set_application_icon();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app)?;
    Ok(())
}

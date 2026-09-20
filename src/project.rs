use super::*;
use crate::shell::ReviewCameraPreset;
use std::collections::BTreeSet;

const MAX_RECENT_PROJECTS: usize = 8;
const MAX_PROJECT_IMAGE_ASSETS: usize = 16;
const MAX_SOURCE_FILE_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) fn load_recent_projects() -> Vec<PathBuf> {
    let Some(path) = recent_projects_file() else {
        warn!("recent projects: no platform config directory is available");
        return Vec::new();
    };
    info!("recent projects: reading {}", path.display());
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            info!("recent projects: no saved list at {}", path.display());
            return Vec::new();
        }
        Err(error) => {
            warn!(
                "recent projects: could not read {}: {error}",
                path.display()
            );
            return Vec::new();
        }
    };
    let projects = match serde_json::from_str::<Vec<String>>(&source) {
        Ok(projects) => projects,
        Err(error) => {
            warn!(
                "recent projects: could not parse {}: {error}",
                path.display()
            );
            return Vec::new();
        }
    };
    let mut recent = Vec::new();
    for project in projects {
        let path = PathBuf::from(project);
        let path = match path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                warn!(
                    "recent projects: skipping {} because it could not be resolved: {error}",
                    path.display()
                );
                continue;
            }
        };
        if !path.is_dir() {
            warn!(
                "recent projects: skipping {} because it is not a directory",
                path.display()
            );
        } else if !path.join("manifest.json").is_file() {
            warn!(
                "recent projects: skipping {} because manifest.json is missing",
                path.display()
            );
        } else if recent.contains(&path) {
            info!("recent projects: skipping duplicate {}", path.display());
        } else {
            recent.push(path);
        }
        if recent.len() == MAX_RECENT_PROJECTS {
            break;
        }
    }
    info!(
        "recent projects: loaded {} item(s): {}",
        recent.len(),
        recent_project_log_list(&recent)
    );
    recent
}

pub(crate) fn remember_recent_project(project: &Path) -> Vec<PathBuf> {
    let project = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf());
    let mut recent = load_recent_projects();
    recent.retain(|path| path != &project);
    recent.insert(0, project);
    recent.truncate(MAX_RECENT_PROJECTS);
    if let Some(file) = recent_projects_file() {
        if let Some(parent) = file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let serialized = recent
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        match serde_json::to_string_pretty(&serialized) {
            Ok(source) => match fs::write(&file, source) {
                Ok(()) => info!(
                    "recent projects: saved {} item(s) to {}",
                    recent.len(),
                    file.display()
                ),
                Err(error) => warn!(
                    "recent projects: could not write {}: {error}",
                    file.display()
                ),
            },
            Err(error) => warn!("recent projects: could not serialize list: {error}"),
        }
    }
    recent
}

pub(crate) fn recent_project_log_list(projects: &[PathBuf]) -> String {
    if projects.is_empty() {
        return "<empty>".to_owned();
    }
    projects
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn recent_projects_file() -> Option<PathBuf> {
    let config_directory = if let Some(directory) = env::var_os("CUBACADABRA_STUDIO_CONFIG_DIR") {
        PathBuf::from(directory)
    } else {
        #[cfg(target_os = "macos")]
        {
            PathBuf::from(env::var_os("HOME")?)
                .join("Library/Application Support/Cubacadabra Studio")
        }
        #[cfg(target_os = "windows")]
        {
            PathBuf::from(env::var_os("APPDATA")?).join("Cubacadabra Studio")
        }
        #[cfg(target_os = "linux")]
        {
            env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config")
                })
                .join("cubacadabra-studio")
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            return None;
        }
    };
    Some(config_directory.join("recent-projects.json"))
}

pub(crate) fn load_project_in_background(
    project: &Path,
    mut progress: impl FnMut(f32),
) -> Result<BackgroundProjectLoad, String> {
    project_manifest(project)?;
    progress(0.04);
    let sources =
        load_game_sources(Some(project.to_path_buf())).map_err(|error| error.to_string())?;
    progress(0.44);

    let prepared = (|| {
        let image_atlas = load_image_atlas_with_progress(
            &sources.root,
            &sources.manifest_source,
            |image_progress| progress(0.44 + image_progress * 0.30),
        )
        .map_err(|error| error.to_string())?;
        let world_models = load_world_models(&sources.root, &sources.manifest_source)
            .map_err(|error| error.to_string())?;
        progress(0.76);
        let morph_catalog_path = discover_project_morph_catalog(&sources.project_root);
        let local_morph_catalog = morph_catalog_path
            .as_deref()
            .map(load_local_morph_catalog)
            .transpose()
            .map_err(|error| error.to_string())?;
        progress(0.88);
        Ok::<_, String>((image_atlas, world_models, local_morph_catalog))
    })();

    match prepared {
        Ok((image_atlas, world_models, local_morph_catalog)) => Ok(BackgroundProjectLoad {
            sources,
            image_atlas,
            world_models,
            local_morph_catalog,
        }),
        Err(error) => {
            if let Some(package) = &sources.temporary_package {
                let _ = fs::remove_dir_all(package);
            }
            Err(error)
        }
    }
}

pub(crate) fn remove_temporary_package(sources: &GameSources) {
    if let Some(package) = &sources.temporary_package {
        let _ = fs::remove_dir_all(package);
    }
}

pub(crate) fn load_game_sources(game_root: Option<PathBuf>) -> Result<GameSources, Box<dyn Error>> {
    let Some(game_root) = game_root else {
        return Ok(GameSources {
            project_root: PathBuf::from(STANDALONE_PREVIEW_ROOT),
            root: PathBuf::from(STANDALONE_PREVIEW_ROOT),
            authored_manifest_source: STANDALONE_PREVIEW_MANIFEST.to_owned(),
            authored_scene_source: None,
            manifest_source: STANDALONE_PREVIEW_MANIFEST.to_owned(),
            script_source: STANDALONE_PREVIEW_SCRIPT.to_owned(),
            review_camera: ReviewCameraPreset::Gameplay,
            standalone_preview: true,
            temporary_package: None,
        });
    };
    let authored_manifest_source = read_utf8_file(&game_root.join("manifest.json"), "manifest")?;
    let authored_scene_source = game_root
        .join("scene.json")
        .is_file()
        .then(|| read_utf8_file(&game_root.join("scene.json"), "scene"))
        .transpose()?;
    let (package_root, temporary_package) = if game_root.join("game.luau").is_file() {
        (game_root.clone(), None)
    } else if game_root.join("src/main.luau").is_file() {
        let package = build_raw_game_package(&game_root)?;
        (package.clone(), Some(package))
    } else {
        return Err(Box::new(StudioError(format!(
            "{} is neither a built package nor a raw game project (expected game.luau or src/main.luau)",
            game_root.display()
        ))));
    };

    let manifest_source = read_utf8_file(&package_root.join("manifest.json"), "manifest")?;
    let (manifest_source, review_camera) =
        apply_studio_project_config(&game_root, manifest_source)?;
    Ok(GameSources {
        project_root: game_root,
        root: package_root.clone(),
        authored_manifest_source,
        authored_scene_source,
        manifest_source,
        script_source: read_utf8_file(&package_root.join("game.luau"), "script")?,
        review_camera,
        standalone_preview: false,
        temporary_package,
    })
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StudioProjectConfig {
    #[serde(default)]
    preview_world: Option<String>,
    #[serde(default)]
    review_camera: Option<String>,
}

fn apply_studio_project_config(
    project_root: &Path,
    manifest_source: String,
) -> Result<(String, ReviewCameraPreset), Box<dyn Error>> {
    let path = project_root.join("studio.json");
    if !path.is_file() {
        return Ok((manifest_source, ReviewCameraPreset::Gameplay));
    }
    let source = read_utf8_file(&path, "Studio project configuration")?;
    let config: StudioProjectConfig = serde_json::from_str(&source).map_err(|error| {
        Box::new(StudioError(format!(
            "could not parse Studio project configuration {}: {error}",
            path.display()
        ))) as Box<dyn Error>
    })?;
    let review_camera = match config.review_camera.as_deref().unwrap_or("gameplay") {
        "gameplay" => ReviewCameraPreset::Gameplay,
        "overview" => ReviewCameraPreset::Overview,
        "showcase" => ReviewCameraPreset::Showcase,
        value => {
            return Err(Box::new(StudioError(format!(
                "studio.json reviewCamera must be gameplay, overview, or showcase; found {value:?}"
            ))));
        }
    };
    let Some(preview_world) = config.preview_world else {
        return Ok((manifest_source, review_camera));
    };
    let mut manifest: Value = serde_json::from_str(&manifest_source).map_err(|error| {
        Box::new(StudioError(format!(
            "could not apply studio.json to the built manifest: {error}"
        ))) as Box<dyn Error>
    })?;
    let exists = preview_world == "lobby"
        || manifest
            .get("worlds")
            .and_then(Value::as_object)
            .is_some_and(|worlds| worlds.contains_key(&preview_world));
    if !exists {
        return Err(Box::new(StudioError(format!(
            "studio.json previewWorld {preview_world:?} does not exist in the project manifest"
        ))));
    }
    let root = manifest
        .as_object_mut()
        .ok_or_else(|| Box::new(StudioError("manifest must be a JSON object".to_owned())))?;
    let launch = root
        .entry("launch")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| Box::new(StudioError("manifest.launch must be an object".to_owned())))?;
    launch.insert("destinationWorld".to_owned(), Value::String(preview_world));
    Ok((serde_json::to_string(&manifest)?, review_camera))
}

pub(crate) fn build_raw_game_package(game_root: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let package = std::env::temp_dir().join(format!(
        "cubacadabra-studio-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    cubacadabra_builder::build_game(&cubacadabra_builder::BuildOptions {
        source_root: game_root.join("src"),
        manifest_path: game_root.join("manifest.json"),
        output: package.clone(),
        zip_path: None,
    })
    .map_err(|error| {
        Box::new(StudioError(format!(
            "could not build raw game project with cubacadabra: {error}"
        ))) as Box<dyn Error>
    })?;
    Ok(package)
}

pub(crate) fn read_utf8_file(path: &Path, kind: &str) -> Result<String, Box<dyn Error>> {
    fs::read_to_string(path).map_err(|error| {
        Box::new(StudioError(format!(
            "could not read {kind} file {}: {error}",
            path.display()
        ))) as Box<dyn Error>
    })
}

pub(crate) fn load_source_files(root: &Path) -> BTreeMap<PathBuf, String> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, String>) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if matches!(name.to_str(), Some(".git" | "target" | "build"))
                || name.to_string_lossy().starts_with('.')
            {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                visit(root, &path, files);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            if relative.starts_with("assets") {
                continue;
            }
            if fs::metadata(&path)
                .map(|metadata| metadata.len() > MAX_SOURCE_FILE_BYTES)
                .unwrap_or(true)
            {
                continue;
            }
            if let Ok(source) = fs::read_to_string(&path) {
                files.insert(relative.to_path_buf(), source);
            }
        }
    }

    let mut files = BTreeMap::new();
    if root.is_dir() {
        visit(root, root, &mut files);
    }
    files
}

pub(crate) fn load_source_directories(root: &Path) -> BTreeSet<PathBuf> {
    fn visit(root: &Path, directory: &Path, directories: &mut BTreeSet<PathBuf>) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if matches!(name.to_str(), Some(".git" | "target" | "build"))
                || name.to_string_lossy().starts_with('.')
            {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            directories.insert(relative.to_path_buf());
            visit(root, &path, directories);
        }
    }

    let mut directories = BTreeSet::new();
    if root.is_dir() {
        visit(root, root, &mut directories);
        if root.join("manifest.json").is_file() {
            directories.insert(PathBuf::from("assets"));
            directories.insert(PathBuf::from("assets/images"));
        }
    }
    directories
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceAssetKind {
    Image,
    Audio,
    Other,
}

#[derive(Clone, Debug)]
pub(crate) struct SourceAsset {
    pub(crate) path: PathBuf,
    pub(crate) bytes: usize,
    pub(crate) kind: SourceAssetKind,
}

pub(crate) fn load_source_assets(root: &Path) -> BTreeMap<PathBuf, SourceAsset> {
    fn asset_kind(path: &Path) -> SourceAssetKind {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp") => SourceAssetKind::Image,
            Some("wav" | "mp3" | "ogg" | "flac" | "m4a" | "aac") => SourceAssetKind::Audio,
            _ => SourceAssetKind::Other,
        }
    }

    fn visit(root: &Path, directory: &Path, assets: &mut BTreeMap<PathBuf, SourceAsset>) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if matches!(name.to_str(), Some(".git" | "target" | "build"))
                || name.to_string_lossy().starts_with('.')
            {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                visit(root, &path, assets);
                continue;
            }
            if !file_type.is_file() || !path.starts_with(root.join("assets")) {
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let Ok(bytes) = fs::metadata(&path).map(|metadata| metadata.len() as usize) else {
                continue;
            };
            let kind = asset_kind(&path);
            assets.insert(relative.to_path_buf(), SourceAsset { path, bytes, kind });
        }
    }

    let mut assets = BTreeMap::new();
    let assets_root = root.join("assets");
    if assets_root.is_dir() {
        visit(root, &assets_root, &mut assets);
    }
    assets
}

pub(crate) fn add_image_asset(manifest: &mut Value, path: &Path) -> Result<String, String> {
    let path = path.to_string_lossy().replace('\\', "/");
    if !path.starts_with("assets/") {
        return Err("image assets must live inside assets/".to_owned());
    }
    let stem = Path::new(&path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| "the image filename is not valid UTF-8".to_owned())?;
    let base_id = image_asset_id(stem);
    if base_id.is_empty() {
        return Err("the image filename must contain a letter or number".to_owned());
    }

    let assets = manifest
        .as_object_mut()
        .ok_or_else(|| "manifest must be a JSON object".to_owned())?
        .entry("assets")
        .or_insert_with(|| serde_json::json!({}));
    let assets = assets
        .as_object_mut()
        .ok_or_else(|| "manifest.assets must be an object".to_owned())?;
    let images = assets
        .entry("images")
        .or_insert_with(|| serde_json::json!({}));
    let images = images
        .as_object_mut()
        .ok_or_else(|| "manifest.assets.images must be an object".to_owned())?;

    if let Some((id, _)) = images.iter().find(|(_, definition)| {
        definition.get("path").and_then(Value::as_str) == Some(path.as_str())
    }) {
        return Ok(id.clone());
    }
    if images.len() >= MAX_PROJECT_IMAGE_ASSETS {
        return Err(format!(
            "a project can contain at most {MAX_PROJECT_IMAGE_ASSETS} image assets"
        ));
    }

    let mut id = base_id.clone();
    let mut suffix = 2;
    while images.contains_key(&id) {
        id = format!("{base_id}-{suffix}");
        suffix += 1;
    }
    images.insert(id.clone(), serde_json::json!({ "path": path }));
    Ok(id)
}

pub(crate) fn set_image_as_ground_material(
    manifest: &mut Value,
    image_path: &Path,
) -> Result<(String, String), String> {
    let image_path = image_path.to_string_lossy().replace('\\', "/");
    let image_id = manifest
        .get("assets")
        .and_then(|assets| assets.get("images"))
        .and_then(Value::as_object)
        .and_then(|images| {
            images.iter().find_map(|(id, definition)| {
                (definition.get("path").and_then(Value::as_str) == Some(image_path.as_str()))
                    .then_some(id.clone())
            })
        })
        .ok_or_else(|| "the image is not registered in manifest.assets.images".to_owned())?;
    let world_id = manifest
        .get("launch")
        .and_then(|launch| launch.get("destinationWorld"))
        .and_then(Value::as_str)
        .or_else(|| manifest.get("startWorld").and_then(Value::as_str))
        .unwrap_or("lobby")
        .to_owned();
    let world = if world_id == "lobby" {
        manifest
    } else {
        manifest
            .get_mut("worlds")
            .and_then(Value::as_object_mut)
            .and_then(|worlds| worlds.get_mut(&world_id))
            .ok_or_else(|| format!("world `{world_id}` was not found"))?
    };
    let world = world
        .as_object_mut()
        .ok_or_else(|| format!("world `{world_id}` must be an object"))?;
    let materials = world
        .entry("materials")
        .or_insert_with(|| serde_json::json!({}));
    let materials = materials
        .as_object_mut()
        .ok_or_else(|| format!("world `{world_id}` materials must be an object"))?;
    materials.insert(
        "floor".to_owned(),
        serde_json::json!({ "image": image_id, "tileU": 8, "tileV": 8 }),
    );
    world.insert(
        "groundMaterial".to_owned(),
        Value::String("floor".to_owned()),
    );
    Ok((image_id, world_id))
}

fn image_asset_id(stem: &str) -> String {
    let mut id = String::new();
    for character in stem.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
            id.push(character.to_ascii_lowercase());
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    id.trim_matches('-').chars().take(64).collect()
}

pub(crate) fn project_manifest(project: &Path) -> Result<PathBuf, String> {
    if !project.is_dir() {
        return Err(format!("{} is not a directory.", project.display()));
    }
    let manifest = project.join("manifest.json");
    if !manifest.is_file() {
        return Err(format!(
            "Choose a project folder containing manifest.json. No manifest was found in {}.",
            project.display()
        ));
    }
    Ok(manifest)
}

pub(crate) fn discover_project_morph_catalog(project_root: &Path) -> Option<PathBuf> {
    let path = project_root.join("assets/characters/catalog.json");
    path.is_file().then_some(path)
}

pub(crate) fn rewrite_manifest_geometry_file(
    source: &str,
    geometry_file: &str,
) -> Result<String, String> {
    let mut root: Value = serde_json::from_str(source)
        .map_err(|error| format!("the generated morph sidecar is invalid JSON: {error}"))?;
    root.get_mut("geometry")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "the generated morph sidecar has no geometry object".to_owned())?
        .insert("file".to_owned(), Value::String(geometry_file.to_owned()));
    root.get_mut("asset")
        .and_then(Value::as_object_mut)
        .and_then(|asset| asset.get_mut("source"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "the generated morph sidecar has no asset source object".to_owned())?
        .insert(
            "geometry".to_owned(),
            Value::String(geometry_file.to_owned()),
        );
    serde_json::to_string_pretty(&root)
        .map_err(|error| format!("could not serialize the morph sidecar: {error}"))
}

pub(crate) fn project_asset_slug(asset_id: &str) -> Result<String, String> {
    let mut slug = String::new();
    for byte in asset_id.bytes() {
        if byte.is_ascii_alphanumeric() {
            slug.push(char::from(byte).to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() || slug.len() > 96 {
        return Err("the character asset ID cannot become a safe project folder name".to_owned());
    }
    Ok(slug)
}

pub(crate) enum SceneEditOperation {
    SetTransform { position: [f32; 3] },
    SetPrimitiveSize { position: [f32; 3], size: [f32; 3] },
    UpdateProperty { key: String, value: Value },
    Duplicate,
    Delete,
}

pub(crate) fn parse_scene_object_target(target: &str) -> Result<(String, String, usize), String> {
    let mut parts = target.split('/');
    let kind = parts.next();
    let world = parts.next();
    let collection = parts.next();
    let index = parts.next();
    if kind != Some("world") || collection.is_none() || parts.next().is_some() {
        return Err(format!("`{target}` is not an editable scene object"));
    }
    let world = world
        .filter(|world| !world.is_empty())
        .ok_or_else(|| format!("`{target}` has no world"))?;
    let index = index
        .ok_or_else(|| format!("`{target}` has no object index"))?
        .parse::<usize>()
        .map_err(|_| format!("`{target}` has an invalid object index"))?;
    Ok((
        world.to_owned(),
        collection.unwrap_or_default().to_owned(),
        index,
    ))
}

pub(crate) fn update_manifest_sign_text(
    manifest: &mut Value,
    target: &str,
    text: String,
) -> Result<(), String> {
    let mut parts = target.split('/');
    let kind = parts.next();
    let world = parts.next();
    let collection = parts.next();
    let index = parts.next();
    if kind != Some("world") || collection != Some("signs") || parts.next().is_some() {
        return Err(format!("`{target}` is not an editable sign"));
    }
    let world = world
        .filter(|world| !world.is_empty())
        .ok_or_else(|| format!("`{target}` has no world"))?;
    let index = index
        .ok_or_else(|| format!("`{target}` has no sign index"))?
        .parse::<usize>()
        .map_err(|_| format!("`{target}` has an invalid sign index"))?;
    let world_definition = if world == "lobby" {
        manifest
    } else {
        manifest
            .get_mut("worlds")
            .and_then(Value::as_object_mut)
            .and_then(|worlds| worlds.get_mut(world))
            .ok_or_else(|| format!("scene world `{world}` was not found"))?
    };
    let signs = world_definition
        .get_mut("signs")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("scene world `{world}` has no signs"))?;
    let sign = signs
        .get_mut(index)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("scene sign `{target}` was not found"))?;
    sign.insert("text".to_owned(), Value::String(text));
    Ok(())
}

pub(crate) fn snapshot_project_files(root: &Path) -> Result<ProjectFileSnapshot, String> {
    fn visit(root: &Path, directory: &Path, files: &mut ProjectFileSnapshot) -> Result<(), String> {
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("could not read {}: {error}", directory.display()))?
        {
            let entry =
                entry.map_err(|error| format!("could not inspect project file: {error}"))?;
            let path = entry.path();
            let file_name = entry.file_name();
            if matches!(file_name.to_str(), Some(".git" | "target")) {
                continue;
            }
            let file_type = entry
                .file_type()
                .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
            if file_type.is_dir() {
                visit(root, &path, files)?;
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|error| format!("could not relativize {}: {error}", path.display()))?
                    .to_path_buf();
                let bytes = fs::read(&path)
                    .map_err(|error| format!("could not read {}: {error}", path.display()))?;
                files.insert(relative, bytes);
            }
        }
        Ok(())
    }

    let mut files = ProjectFileSnapshot::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

pub(crate) fn diff_project_files(
    before: ProjectFileSnapshot,
    after: ProjectFileSnapshot,
) -> Vec<CodexFileChange> {
    let paths = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    paths
        .into_iter()
        .filter_map(|relative_path| {
            let before_bytes = before.get(&relative_path).cloned();
            let after_bytes = after.get(&relative_path).cloned();
            (before_bytes != after_bytes).then_some(CodexFileChange {
                relative_path,
                before: before_bytes,
                after: after_bytes,
            })
        })
        .collect()
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("file has no parent directory: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("file has no safe name: {}", path.display()))?;
    let temporary = parent.join(format!(".{file_name}.studio-{}", std::process::id()));
    fs::write(&temporary, bytes)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            fs::remove_file(path).map_err(|remove_error| {
                format!("could not replace {}: {remove_error}", path.display())
            })?;
            fs::rename(&temporary, path).map_err(|rename_error| {
                format!("could not replace {}: {rename_error}", path.display())
            })
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(format!("could not finalize {}: {error}", path.display()))
        }
    }
}

pub(crate) fn update_project_morph_catalog(
    path: &Path,
    asset_id: &str,
    source: &str,
) -> Result<(), String> {
    let mut catalog: Value = if path.is_file() {
        let source = fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        serde_json::from_str(&source)
            .map_err(|error| format!("{} is not valid JSON: {error}", path.display()))?
    } else {
        serde_json::json!({ "schemaVersion": 1, "assets": [] })
    };
    if catalog.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err(format!(
            "{} must use morph catalog schema 1",
            path.display()
        ));
    }
    let assets = catalog
        .get_mut("assets")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("{} must contain an assets array", path.display()))?;
    assets.retain(|entry| entry.get("id").and_then(Value::as_str) != Some(asset_id));
    assets.push(serde_json::json!({ "id": asset_id, "source": source }));
    let serialized = serde_json::to_string_pretty(&catalog)
        .map_err(|error| format!("could not serialize {}: {error}", path.display()))?;
    write_atomic(path, serialized.as_bytes())
}

pub(crate) fn load_local_morph_catalog(path: &Path) -> Result<LocalMorphCatalog, Box<dyn Error>> {
    let catalog_source = read_utf8_file(path, "morph catalog")?;
    let definition: LocalMorphCatalogFile =
        serde_json::from_str(&catalog_source).map_err(|error| {
            Box::new(StudioError(format!(
                "local morph catalog {} is not valid JSON: {error}",
                path.display()
            ))) as Box<dyn Error>
        })?;
    if definition.schema_version != 1 {
        return Err(Box::new(StudioError(format!(
            "unsupported local morph catalog schema {}; expected 1",
            definition.schema_version
        ))));
    }

    let catalog_root = path.parent().ok_or_else(|| {
        Box::new(StudioError(format!(
            "local morph catalog has no parent directory: {}",
            path.display()
        ))) as Box<dyn Error>
    })?;
    let mut assets = Vec::new();
    let builtins_source = if let Some(builtins) = definition.builtins.as_deref() {
        let builtins_path = local_catalog_file(catalog_root, builtins, "built-in catalog")?;
        read_utf8_file(&builtins_path, "built-in morph catalog")?
    } else {
        include_str!("../../rust/assets/characters/morph_catalog.json").to_owned()
    };
    let builtins = cubacadabra_morphs::parse_catalog(&builtins_source).map_err(|diagnostics| {
        Box::new(StudioError(format!(
            "built-in morph catalog is invalid: {}",
            StudioApp::format_morph_diagnostics(&diagnostics)
        ))) as Box<dyn Error>
    })?;
    assets.extend(
        builtins
            .assets
            .into_iter()
            .filter(|asset| !definition.exclude_builtin_kinds.contains(&asset.kind)),
    );

    let mut packs = BTreeMap::new();
    for local_asset in definition.assets {
        let sidecar_path = local_catalog_file(catalog_root, &local_asset.source, "asset sidecar")?;
        let sidecar_source = read_utf8_file(&sidecar_path, "morph sidecar")?;
        let asset = source_manifest_asset(&sidecar_source).map_err(|diagnostics| {
            Box::new(StudioError(format!(
                "morph sidecar {} is invalid: {}",
                sidecar_path.display(),
                StudioApp::format_morph_diagnostics(&diagnostics)
            ))) as Box<dyn Error>
        })?;
        if asset.id != local_asset.id {
            return Err(Box::new(StudioError(format!(
                "local morph catalog asset {} points to {}, which declares {}",
                local_asset.id,
                sidecar_path.display(),
                asset.id
            ))));
        }
        let geometry_file =
            source_manifest_geometry_file(&sidecar_source).map_err(|diagnostics| {
                Box::new(StudioError(format!(
                    "morph sidecar {} has invalid geometry: {}",
                    sidecar_path.display(),
                    StudioApp::format_morph_diagnostics(&diagnostics)
                ))) as Box<dyn Error>
            })?;
        let geometry_path = local_catalog_file(
            sidecar_path.parent().ok_or_else(|| {
                Box::new(StudioError(format!(
                    "morph sidecar has no parent directory: {}",
                    sidecar_path.display()
                ))) as Box<dyn Error>
            })?,
            &geometry_file,
            "morph geometry",
        )?;
        let geometry = fs::read(&geometry_path).map_err(|error| {
            Box::new(StudioError(format!(
                "could not read morph geometry {}: {error}",
                geometry_path.display()
            ))) as Box<dyn Error>
        })?;
        let (pack, summary) =
            compile_source_morph_pack(&sidecar_source, &geometry).map_err(|diagnostics| {
                Box::new(StudioError(format!(
                    "could not compile local morph {}: {}",
                    local_asset.id,
                    StudioApp::format_morph_diagnostics(&diagnostics)
                ))) as Box<dyn Error>
            })?;
        if summary.asset_id != local_asset.id.as_str() {
            return Err(Box::new(StudioError(format!(
                "compiled local morph has unexpected ID {} (expected {})",
                summary.asset_id, local_asset.id
            ))));
        }
        assets.retain(|existing| existing.id != asset.id);
        assets.push(asset);
        packs.insert(local_asset.id, pack);
    }

    let mut presets = Vec::new();
    let mut thumbnails = BTreeMap::new();
    for local_preset in definition.presets {
        let preset_path = local_catalog_file(catalog_root, &local_preset.source, "preset")?;
        let preset_source = read_utf8_file(&preset_path, "morph preset")?;
        let preset: cubacadabra_morphs::MorphPreset = serde_json::from_str(&preset_source)
            .map_err(|error| {
                Box::new(StudioError(format!(
                    "morph preset {} is not valid: {error}",
                    preset_path.display()
                ))) as Box<dyn Error>
            })?;
        if let Some(thumbnail) = preset.thumbnail.as_deref() {
            let thumbnail_path = preset_path
                .parent()
                .ok_or_else(|| {
                    Box::new(StudioError(
                        "local preset has no parent directory".to_owned(),
                    )) as Box<dyn Error>
                })?
                .join(thumbnail);
            if Path::new(thumbnail).is_absolute() || !thumbnail_path.is_file() {
                return Err(Box::new(StudioError(format!(
                    "local morph thumbnail does not exist: {}",
                    thumbnail_path.display()
                ))));
            }
            thumbnails.insert(
                thumbnail.to_owned(),
                fs::read(&thumbnail_path).map_err(|error| {
                    Box::new(StudioError(format!(
                        "could not read local morph thumbnail {}: {error}",
                        thumbnail_path.display()
                    ))) as Box<dyn Error>
                })?,
            );
        }
        presets.push(preset);
    }

    let catalog = cubacadabra_morphs::MorphCatalog {
        schema_version: cubacadabra_morphs::MORPH_CATALOG_SCHEMA_VERSION,
        content_version: "local-development".to_owned(),
        assets,
        presets,
    };
    let diagnostics = catalog.validate();
    if !diagnostics.is_empty() {
        return Err(Box::new(StudioError(format!(
            "local morph catalog {} is invalid: {}",
            path.display(),
            StudioApp::format_morph_diagnostics(&diagnostics)
        ))));
    }
    let initial_preset = catalog.presets.first().map(|preset| preset.id.clone());
    log::info!(
        "loaded local morph catalog: path={} assets={} packs={} presets={}",
        path.display(),
        catalog.assets.len(),
        packs.len(),
        catalog.presets.len()
    );
    Ok(LocalMorphCatalog {
        catalog,
        packs,
        thumbnails,
        initial_preset,
    })
}

pub(crate) fn local_catalog_file(
    root: &Path,
    reference: &str,
    kind: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    let path = root.join(reference);
    if reference.is_empty() || Path::new(reference).is_absolute() || !path.is_file() {
        return Err(Box::new(StudioError(format!(
            "local {kind} does not exist: {}",
            path.display()
        ))));
    }
    path.canonicalize().map_err(|error| {
        Box::new(StudioError(format!(
            "could not resolve local {kind} {}: {error}",
            path.display()
        ))) as Box<dyn Error>
    })
}

#[cfg(test)]
mod studio_project_config_tests {
    use super::*;

    #[test]
    fn preview_world_changes_only_the_runtime_manifest() {
        let root = std::env::temp_dir().join(format!(
            "cubacadabra-studio-config-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("studio.json"),
            r#"{"previewWorld":"reference","reviewCamera":"showcase"}"#,
        )
        .unwrap();
        let authored = r#"{"launch":{"destinationWorld":"easy-room"},"worlds":{"easy-room":{},"reference":{}}}"#;
        let (runtime, camera) = apply_studio_project_config(&root, authored.to_owned()).unwrap();
        let runtime: Value = serde_json::from_str(&runtime).unwrap();
        assert_eq!(
            runtime.pointer("/launch/destinationWorld"),
            Some(&Value::String("reference".to_owned()))
        );
        assert_eq!(camera, ReviewCameraPreset::Showcase);
        assert!(authored.contains("easy-room"));
        let _ = fs::remove_dir_all(root);
    }
}

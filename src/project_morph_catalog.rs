use super::*;

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

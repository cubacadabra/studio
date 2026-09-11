//! Validate a complete publication, including preset dependencies and fit.
use cubacadabra_morphs::{CapabilitySet, MorphCatalog, resolve_preset};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let path = env::args()
        .nth(1)
        .ok_or("usage: morph_catalog_validate <catalog.lock.json>")?;
    let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let lock: serde_json::Value =
        serde_json::from_str(&source).map_err(|error| error.to_string())?;
    let catalog: MorphCatalog = serde_json::from_value(serde_json::json!({
        "schemaVersion": cubacadabra_morphs::MORPH_CATALOG_SCHEMA_VERSION,
        "contentVersion": "publication",
        "assets": lock["assets"].as_array().ok_or("missing assets")?.iter()
            .map(|asset| &asset["definition"]).collect::<Vec<_>>(),
        "presets": lock["presets"].as_array().cloned().unwrap_or_default(),
    }))
    .map_err(|error| error.to_string())?;
    let capabilities = CapabilitySet::new(
        catalog
            .assets
            .iter()
            .flat_map(|asset| asset.required_capabilities.iter().cloned()),
    );
    let mut diagnostics = catalog.validate();
    if diagnostics.is_empty() {
        for preset in &catalog.presets {
            if let Err(errors) = resolve_preset(&catalog, &preset.id, &capabilities) {
                diagnostics.extend(errors);
            }
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics
            .iter()
            .map(|item| format!("{} {}: {}", item.code, item.path, item.message))
            .collect::<Vec<_>>()
            .join("\n"));
    }
    println!(
        "Validated {} assets and {} complete presets",
        catalog.assets.len(),
        catalog.presets.len()
    );
    Ok(())
}

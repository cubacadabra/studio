//! Studio-facing access to the shared morph catalog contract.
//!
//! Keeping this adapter independent from the engine makes it possible for the
//! future Morphs workspace to inspect drafts and diagnostics before anything
//! is uploaded to the renderer.

use cubacadabra_morphs::{MorphCatalog, MorphDiagnostic, parse_catalog};

#[allow(dead_code)]
pub(crate) fn inspect_catalog(source: &str) -> Result<MorphCatalog, Vec<MorphDiagnostic>> {
    parse_catalog(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPATIBILITY_CATALOG: &str =
        include_str!("../../rust/assets/characters/morph_catalog.json");

    #[test]
    fn studio_can_inspect_the_shared_catalog_without_engine_internals() {
        let catalog = inspect_catalog(COMPATIBILITY_CATALOG).expect("compatibility catalog");
        assert_eq!(catalog.presets.len(), 3);
        assert_eq!(catalog.assets.len(), 13);
    }

    #[test]
    fn studio_receives_structured_catalog_diagnostics() {
        let diagnostics =
            inspect_catalog(r#"{"schemaVersion":1,"contentVersion":"","assets":[],"presets":[]}"#)
                .unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "MORPH_CATALOG_INVALID_CONTENT_VERSION" })
        );
    }
}

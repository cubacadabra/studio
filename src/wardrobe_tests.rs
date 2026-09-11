use crate::wardrobe::*;
use cubacadabra_morphs::{
    MorphAssetDefinition, MorphAssetId, MorphAssetKind, MorphCatalog, MorphParameterValue,
    parse_catalog, resolve_loadout,
};

pub(super) fn catalog() -> MorphCatalog {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/starter-set");
    let source: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("catalog.json")).unwrap()).unwrap();
    let mut catalog = parse_catalog(include_str!(
        "../../rust/assets/characters/morph_catalog.json"
    ))
    .unwrap();
    catalog
        .assets
        .retain(|asset| asset.kind != MorphAssetKind::Outfit);
    for spec in source["assets"].as_array().unwrap() {
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join(spec["source"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        let asset: MorphAssetDefinition =
            serde_json::from_value(manifest["asset"].clone()).unwrap();
        catalog.assets.retain(|other| other.id != asset.id);
        catalog.assets.push(asset);
    }
    catalog.presets = source["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|spec| {
            serde_json::from_str(
                &std::fs::read_to_string(root.join(spec["source"].as_str().unwrap())).unwrap(),
            )
            .unwrap()
        })
        .collect();
    catalog
}

fn id(value: &str) -> MorphAssetId {
    MorphAssetId::parse(format!("cuba:{value}.v1")).unwrap()
}

#[test]
fn all_24_starters_resolve_with_independent_skin_and_complete_clothing() {
    let catalog = catalog();
    assert_eq!(catalog.presets.len(), 24);
    let mut skins = std::collections::BTreeMap::new();
    for preset in &catalog.presets {
        let loadout = preset.loadout();
        resolve_loadout(
            &catalog,
            &loadout,
            &crate::morph_application::capabilities(),
        )
        .unwrap();
        cubacadabra_morphs::project_v2_to_v1(&catalog, &loadout).unwrap();
        for slot in ["shirt", "pants", "shoes"] {
            assert_eq!(
                loadout
                    .parts
                    .iter()
                    .filter(|id| catalog
                        .asset(id)
                        .unwrap()
                        .occupied_slots
                        .contains(&slot.to_owned()))
                    .count(),
                1
            );
        }
        assert!(matches_preset(&loadout, preset));
        assert!(preset.thumbnail.is_some());
        let MorphParameterValue::Text(skin) = &loadout.parameters["skin"] else {
            panic!("missing skin");
        };
        *skins.entry(skin.clone()).or_insert(0) += 1;
    }
    assert_eq!(skins.len(), 12);
    assert!(skins.values().all(|count| *count == 2));
}

#[test]
fn person_17_is_a_recipe_and_glasses_do_not_change_its_clothing() {
    let catalog = catalog();
    let preset = &catalog.presets[16];
    let original = preset.loadout();
    for part in [
        "hair/floppy",
        "top/short-sleeve-collared",
        "bottom/slacks",
        "footwear/sparkles",
    ] {
        assert!(original.parts.contains(&id(part)));
    }
    let equipped = edit(
        &catalog,
        &original,
        &Request::Equip(id("facewear/round-glasses")),
    )
    .unwrap();
    assert!(!matches_preset(&equipped, preset));
    assert_eq!(equipped.parts.len(), original.parts.len() + 1);
    let removed = edit(
        &catalog,
        &equipped,
        &Request::Remove(id("facewear/round-glasses")),
    )
    .unwrap();
    assert!(matches_preset(&removed, preset));
}

#[test]
fn accessories_coexist_and_conflicts_replace_only_the_affected_parts() {
    let mut catalog = catalog();
    let mut loadout = catalog.presets[16].loadout();
    for part in [
        "headwear/test-top-hat",
        "headwear/headphones",
        "facewear/round-glasses",
        "accessory/hearing-aids",
    ] {
        loadout = edit(&catalog, &loadout, &Request::Equip(id(part))).unwrap();
    }
    resolve_loadout(
        &catalog,
        &loadout,
        &crate::morph_application::capabilities(),
    )
    .unwrap();
    let legacy = cubacadabra_morphs::project_v2_to_v1(&catalog, &loadout).unwrap();
    for slot in ["hat", "ear-accessory", "glasses", "ear-device", "hair"] {
        assert!(legacy.equipment.contains_key(slot));
    }
    let replaced = edit(
        &catalog,
        &loadout,
        &Request::Equip(id("facewear/square-glasses")),
    )
    .unwrap();
    assert!(!replaced.parts.contains(&id("facewear/round-glasses")));
    assert_eq!(replaced.parts.len(), loadout.parts.len());
    catalog
        .assets
        .iter_mut()
        .find(|a| a.id == id("headwear/test-top-hat"))
        .unwrap()
        .conflicts
        .push("ear-accessory".into());
    let next = edit(
        &catalog,
        &loadout,
        &Request::Equip(id("headwear/test-top-hat")),
    )
    .unwrap();
    assert!(!next.parts.contains(&id("headwear/headphones")));
    assert!(next.parts.contains(&id("facewear/round-glasses")));
}

#[test]
fn starter_selection_replaces_all_previous_parts_face_and_parameters() {
    let catalog = catalog();
    let mut current = catalog.presets[0].loadout();
    current.parameters.insert(
        "primary".into(),
        MorphParameterValue::Text("#ff00ff".into()),
    );
    current.face = Some(id("face/sad"));
    let next = edit(
        &catalog,
        &current,
        &Request::Preset(catalog.presets[16].id.clone()),
    )
    .unwrap();
    assert!(matches_preset(&next, &catalog.presets[16]));
    assert!(!next.parts.contains(&id("facewear/round-glasses")));
    assert!(
        edit(
            &catalog,
            &next,
            &Request::Remove(id("top/short-sleeve-collared"))
        )
        .is_err()
    );
}

#[test]
fn published_catalog_validates_complete_recipes_and_immutable_delivery() {
    let catalog = catalog();
    let hash = "ab".repeat(32);
    let thumbnail = format!("/morphs/thumbnails/sha256/ab/{hash}.png");
    let mut presets = catalog.presets.clone();
    for preset in &mut presets {
        preset.thumbnail = Some(thumbnail.clone());
    }
    let document = serde_json::json!({
        "release": "test-release",
        "assets": catalog.assets.iter().map(|asset| {
            let artifact = asset.source.as_ref().map(|_| serde_json::json!({
                "url": format!("/morphs/packs/sha256/ab/{hash}.morphpack"),
                "sha256": hash, "bytes": 42,
            }));
            serde_json::json!({
                "definition": asset,
                "delivery": if artifact.is_some() { "morphpack" } else { "builtin" },
                "artifact": artifact,
            })
        }).collect::<Vec<_>>(),
        "presets": presets,
    });
    let parsed = PublishedCatalog::parse(&document.to_string()).unwrap();
    assert_eq!(parsed.catalog.presets.len(), 24);
    assert_eq!(parsed.artifacts.len(), 20);
    assert_eq!(
        parsed.catalog.presets[16].parameters,
        presets[16].parameters
    );

    let mut invalid = document.clone();
    invalid["presets"][0]["thumbnail"] = "https://example.com/arbitrary.png".into();
    assert!(PublishedCatalog::parse(&invalid.to_string()).is_err());
    invalid = document.clone();
    invalid["presets"][0]["face"] = "cuba:hair/floppy.v1".into();
    assert!(PublishedCatalog::parse(&invalid.to_string()).is_err());
    invalid = document.clone();
    invalid["presets"][0]["parts"][0] = "cuba:hair/missing.v1".into();
    assert!(PublishedCatalog::parse(&invalid.to_string()).is_err());
    invalid = document;
    let asset = invalid["assets"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|asset| asset["delivery"] == "morphpack")
        .unwrap();
    asset["artifact"]["url"] = "/mutable/latest.morphpack".into();
    assert!(PublishedCatalog::parse(&invalid.to_string()).is_err());
}

//! Studio-only source manifests for authored morph assets.
//!
//! This authoring slice validates the sidecar contract around a Blender export,
//! inspects its GLB container/JSON LOD metadata, decodes bounded mesh data for
//! the Studio preview, and compiles the shared runtime pack format.

pub use cubacadabra_morphs::{
    MAX_MORPH_PACK_BYTES, MAX_MORPH_PACK_SURFACES, MORPH_PACK_MAGIC,
    MORPH_PACK_MULTI_SURFACE_SCHEMA_VERSION, MORPH_PACK_SCHEMA_VERSION,
    MORPH_PACK_SKINNED_SCHEMA_VERSION, MorphPackVertexSkin,
};
use cubacadabra_morphs::{MorphAssetDefinition, MorphAssetKind, MorphDiagnostic};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MORPH_SOURCE_SCHEMA_VERSION: u16 = 1;
pub const MAX_SOURCE_MANIFEST_BYTES: usize = 256 * 1024;
pub const MAX_SOURCE_GLB_BYTES: usize = 64 * 1024 * 1024;
const MAX_NODE_NAME_BYTES: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MorphAttachmentMode {
    Rigid,
    Skinned,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MorphAttachment {
    pub mode: MorphAttachmentMode,
    pub joint: String,
    #[serde(default = "identity_translation")]
    pub translation: [f32; 3],
    #[serde(default = "identity_rotation")]
    pub rotation: [f32; 4],
    #[serde(default = "identity_scale")]
    pub scale: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MorphGeometrySource {
    pub file: String,
    pub lod_nodes: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub triangle_counts: BTreeMap<String, u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MorphSourceManifest {
    pub schema_version: u16,
    pub asset: MorphAssetDefinition,
    pub geometry: MorphGeometrySource,
    pub attachment: MorphAttachment,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<MorphSkinContract>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MorphSkinContract {
    pub skeleton: String,
    pub joint_order: Vec<String>,
    pub max_influences: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MorphSourceInspection {
    pub asset_id: String,
    pub asset_kind: MorphAssetKind,
    pub geometry_file: String,
    pub lod_count: usize,
    pub attachment_joint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MorphGlbInspection {
    pub version: u32,
    pub json_bytes: usize,
    pub bin_bytes: usize,
    pub lods: BTreeMap<String, MorphGlbLodInspection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MorphGlbLodInspection {
    pub node: String,
    pub mesh_index: usize,
    pub primitive_count: usize,
    pub triangle_count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MorphGlbPreviewMesh {
    pub name: String,
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub base_color: Option<[f32; 4]>,
    pub use_avatar_tint: bool,
    pub skinning: Option<Vec<MorphPackVertexSkin>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MorphGlbSourceSummary {
    pub version: u32,
    pub node_names: Vec<String>,
    pub mesh_names: Vec<String>,
    pub material_names: Vec<String>,
    pub lod_candidates: BTreeMap<String, Vec<String>>,
    pub node_triangle_counts: BTreeMap<String, u32>,
    pub triangle_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MorphPackSummary {
    pub asset_id: String,
    pub byte_len: usize,
    pub lod_triangle_counts: BTreeMap<String, u32>,
}

impl MorphSourceManifest {
    pub fn validate(&self) -> Vec<MorphDiagnostic> {
        let mut diagnostics = Vec::new();
        if !matches!(self.schema_version, MORPH_SOURCE_SCHEMA_VERSION | 2) {
            diagnostics.push(error(
                "MORPH_SOURCE_UNSUPPORTED_SCHEMA",
                "schemaVersion",
                format!("expected schema {MORPH_SOURCE_SCHEMA_VERSION}"),
            ));
        }
        diagnostics.extend(self.asset.validate());
        if self.asset.kind == MorphAssetKind::Base
            && self.attachment.mode != MorphAttachmentMode::Skinned
        {
            diagnostics.push(error(
                "MORPH_SOURCE_BASE_NOT_ACCESSORY",
                "asset.kind",
                "base assets must use a skinned attachment",
            ));
        }
        validate_geometry(&self.geometry, &mut diagnostics);
        validate_attachment(&self.attachment, &mut diagnostics);
        if self.attachment.mode == MorphAttachmentMode::Skinned {
            match &self.skin {
                Some(skin)
                    if skin.skeleton.len() <= MAX_NODE_NAME_BYTES
                        && !skin.skeleton.is_empty()
                        && skin.joint_order.len() == 15
                        && skin.max_influences == 4 => {}
                Some(_) => diagnostics.push(error(
                    "MORPH_SOURCE_INVALID_SKIN_CONTRACT",
                    "skin",
                    "skinned assets need a skeleton, exactly 15 joints, and maxInfluences 4",
                )),
                None => diagnostics.push(error(
                    "MORPH_SOURCE_MISSING_SKIN_CONTRACT",
                    "skin",
                    "skinned assets must declare their 15-joint skin contract",
                )),
            }
        }
        if let Some(source) = &self.asset.source {
            if source.geometry != self.geometry.file {
                diagnostics.push(error(
                    "MORPH_SOURCE_GEOMETRY_PATH_MISMATCH",
                    "asset.source.geometry",
                    "asset source geometry must match geometry.file",
                ));
            }
        } else {
            diagnostics.push(error(
                "MORPH_SOURCE_MISSING_GEOMETRY_REFERENCE",
                "asset.source",
                "source manifests must declare asset.source.geometry",
            ));
        }
        diagnostics
    }

    pub fn inspect(&self) -> Result<MorphSourceInspection, Vec<MorphDiagnostic>> {
        let diagnostics = self.validate();
        if diagnostics.is_empty() {
            Ok(MorphSourceInspection {
                asset_id: self.asset.id.to_string(),
                asset_kind: self.asset.kind,
                geometry_file: self.geometry.file.clone(),
                lod_count: self.geometry.lod_nodes.len(),
                attachment_joint: self.attachment.joint.clone(),
            })
        } else {
            Err(diagnostics)
        }
    }
}

pub fn parse_source_manifest(source: &str) -> Result<MorphSourceManifest, Vec<MorphDiagnostic>> {
    if source.len() > MAX_SOURCE_MANIFEST_BYTES {
        return Err(vec![error(
            "MORPH_SOURCE_TOO_LARGE",
            "$",
            format!("source manifest exceeds {MAX_SOURCE_MANIFEST_BYTES} bytes"),
        )]);
    }
    let manifest: MorphSourceManifest = serde_json::from_str(source).map_err(|parse_error| {
        vec![error(
            "MORPH_SOURCE_INVALID_JSON",
            "$",
            parse_error.to_string(),
        )]
    })?;
    let diagnostics = manifest.validate();
    if diagnostics.is_empty() {
        Ok(manifest)
    } else {
        Err(diagnostics)
    }
}

/// Inspect the JSON and container structure of a GLB referenced by a source
/// manifest. This deliberately stops before decoding vertex buffers; the
/// compiler can add that step once the contract is stable without changing
/// the manifest API.
pub fn inspect_glb_bytes(
    manifest: &MorphSourceManifest,
    bytes: &[u8],
) -> Result<MorphGlbInspection, Vec<MorphDiagnostic>> {
    let mut diagnostics = manifest.validate();
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    if bytes.len() > MAX_SOURCE_GLB_BYTES {
        diagnostics.push(error(
            "MORPH_GLB_TOO_LARGE",
            "geometry.file",
            format!("GLB exceeds {MAX_SOURCE_GLB_BYTES} bytes"),
        ));
        return Err(diagnostics);
    }
    if !manifest.geometry.file.ends_with(".glb") {
        diagnostics.push(error(
            "MORPH_GLB_UNSUPPORTED_SOURCE",
            "geometry.file",
            "binary inspection currently accepts only .glb sources",
        ));
        return Err(diagnostics);
    }
    let (version, json, bin) = match read_glb_chunks(bytes) {
        Ok(value) => value,
        Err(diagnostic) => return Err(vec![diagnostic]),
    };
    let document: serde_json::Value = match serde_json::from_slice(&json) {
        Ok(value) => value,
        Err(parse_error) => {
            return Err(vec![error(
                "MORPH_GLB_INVALID_JSON",
                "geometry.file",
                parse_error.to_string(),
            )]);
        }
    };
    let nodes = document
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_NODES",
                "geometry.file",
                "GLB JSON must contain a nodes array",
            )]
        })?;
    let meshes = document
        .get("meshes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_MESHES",
                "geometry.file",
                "GLB JSON must contain a meshes array",
            )]
        })?;
    let accessors = document
        .get("accessors")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_ACCESSORS",
                "geometry.file",
                "GLB JSON must contain an accessors array",
            )]
        })?;

    let mut lods = BTreeMap::new();
    for level in ["near", "mid", "far"] {
        let node_name = manifest.geometry.lod_nodes[level].as_str();
        let matching_nodes = nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                node.get("name").and_then(serde_json::Value::as_str) == Some(node_name)
            })
            .collect::<Vec<_>>();
        if matching_nodes.len() != 1 {
            diagnostics.push(error(
                "MORPH_GLB_LOD_NODE_NOT_UNIQUE",
                &format!("geometry.lodNodes.{level}"),
                format!("expected exactly one node named {node_name:?}"),
            ));
            continue;
        }
        let (node_index, node) = matching_nodes[0];
        if !node_hierarchy_transform_is_applied(nodes, node_index) {
            diagnostics.push(error(
                "MORPH_GLB_UNAPPLIED_NODE_TRANSFORM",
                &format!("geometry.lodNodes.{level}"),
                "LOD node and parent transforms must be identity; apply Location, Rotation, and Scale in Blender before export",
            ));
            continue;
        }
        let Some(mesh_index) = node.get("mesh").and_then(serde_json::Value::as_u64) else {
            diagnostics.push(error(
                "MORPH_GLB_LOD_NODE_MISSING_MESH",
                &format!("geometry.lodNodes.{level}"),
                "LOD nodes must reference a mesh",
            ));
            continue;
        };
        let Ok(mesh_index) = usize::try_from(mesh_index) else {
            diagnostics.push(error(
                "MORPH_GLB_INVALID_MESH_INDEX",
                &format!("geometry.lodNodes.{level}"),
                "mesh index is too large",
            ));
            continue;
        };
        let Some(mesh) = meshes.get(mesh_index) else {
            diagnostics.push(error(
                "MORPH_GLB_INVALID_MESH_INDEX",
                &format!("geometry.lodNodes.{level}"),
                "LOD node references a missing mesh",
            ));
            continue;
        };
        let Some(primitives) = mesh.get("primitives").and_then(serde_json::Value::as_array) else {
            diagnostics.push(error(
                "MORPH_GLB_MISSING_PRIMITIVES",
                &format!("geometry.lodNodes.{level}"),
                "LOD mesh must contain primitives",
            ));
            continue;
        };
        if primitives.is_empty() || primitives.len() > MAX_MORPH_PACK_SURFACES {
            diagnostics.push(error(
                "MORPH_GLB_LOD_PRIMITIVE_COUNT",
                &format!("geometry.lodNodes.{level}"),
                format!(
                    "LOD meshes must contain 1..={MAX_MORPH_PACK_SURFACES} triangle-list primitives"
                ),
            ));
            continue;
        }
        let mut triangle_count = 0u32;
        let mut valid = true;
        for (primitive_index, primitive) in primitives.iter().enumerate() {
            let mode = primitive
                .get("mode")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(4);
            if mode != 4 {
                diagnostics.push(error(
                    "MORPH_GLB_UNSUPPORTED_PRIMITIVE_MODE",
                    &format!("geometry.lodNodes.{level}.primitives[{primitive_index}].mode"),
                    "compiled rigid LODs must use a triangle list; triangulate the mesh before export",
                ));
                valid = false;
                continue;
            }
            let count = primitive
                .get("indices")
                .and_then(serde_json::Value::as_u64)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| accessors.get(index))
                .and_then(|accessor| accessor.get("count"))
                .and_then(serde_json::Value::as_u64)
                .or_else(|| {
                    primitive
                        .get("attributes")
                        .and_then(|attributes| attributes.get("POSITION"))
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|index| usize::try_from(index).ok())
                        .and_then(|index| accessors.get(index))
                        .and_then(|accessor| accessor.get("count"))
                        .and_then(serde_json::Value::as_u64)
                });
            let Some(count) = count else {
                diagnostics.push(error(
                    "MORPH_GLB_PRIMITIVE_MISSING_COUNT",
                    &format!("geometry.lodNodes.{level}.primitives[{primitive_index}]"),
                    "primitive needs an index or POSITION accessor with a count",
                ));
                valid = false;
                continue;
            };
            if count % 3 != 0 {
                diagnostics.push(error(
                    "MORPH_GLB_INVALID_TRIANGLE_LIST",
                    &format!("geometry.lodNodes.{level}.primitives[{primitive_index}]"),
                    "triangle-list index or vertex count must be divisible by three",
                ));
                valid = false;
                continue;
            }
            let triangles = count / 3;
            let Ok(triangles) = u32::try_from(triangles) else {
                diagnostics.push(error(
                    "MORPH_GLB_TRIANGLE_COUNT_OVERFLOW",
                    &format!("geometry.lodNodes.{level}"),
                    "triangle count exceeds the supported limit",
                ));
                valid = false;
                continue;
            };
            triangle_count = triangle_count.saturating_add(triangles);
        }
        if valid {
            let expected = match level {
                "near" => manifest.asset.lod.near,
                "mid" => manifest.asset.lod.mid,
                "far" => manifest.asset.lod.far,
                _ => unreachable!("the inspector only visits known LODs"),
            };
            if triangle_count > expected {
                diagnostics.push(error(
                    "MORPH_GLB_TRIANGLE_BUDGET_EXCEEDED",
                    &format!("asset.lod.{level}"),
                    format!("asset budget is {expected}, GLB reports {triangle_count}"),
                ));
            }
            if let Some(legacy_count) = manifest.geometry.triangle_counts.get(level)
                && *legacy_count != triangle_count
            {
                diagnostics.push(error(
                    "MORPH_GLB_LEGACY_TRIANGLE_COUNT_MISMATCH",
                    &format!("geometry.triangleCounts.{level}"),
                    format!("legacy sidecar count is {legacy_count}, GLB reports {triangle_count}"),
                ));
            }
            lods.insert(
                level.to_owned(),
                MorphGlbLodInspection {
                    node: node_name.to_owned(),
                    mesh_index,
                    primitive_count: primitives.len(),
                    triangle_count,
                },
            );
        }
    }
    if diagnostics.is_empty() {
        Ok(MorphGlbInspection {
            version,
            json_bytes: json.len(),
            bin_bytes: bin.len(),
            lods,
        })
    } else {
        Err(diagnostics)
    }
}

/// Inspect the source structure without requiring a `.morph.json` sidecar.
/// Studio uses this to turn a raw Blender export into an actionable draft
/// review before an artist has mapped LOD nodes or an attachment joint.
pub fn inspect_glb_source(bytes: &[u8]) -> Result<MorphGlbSourceSummary, Vec<MorphDiagnostic>> {
    if bytes.len() > MAX_SOURCE_GLB_BYTES {
        return Err(vec![error(
            "MORPH_GLB_TOO_LARGE",
            "geometry.file",
            format!("GLB exceeds {MAX_SOURCE_GLB_BYTES} bytes"),
        )]);
    }
    let (version, json, _bin) = read_glb_chunks(bytes).map_err(|diagnostic| vec![diagnostic])?;
    let document: serde_json::Value = serde_json::from_slice(&json).map_err(|parse_error| {
        vec![error(
            "MORPH_GLB_INVALID_JSON",
            "geometry.file",
            parse_error.to_string(),
        )]
    })?;
    let nodes = document
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_NODES",
                "nodes",
                "GLB JSON must contain a nodes array",
            )]
        })?;
    let meshes = document
        .get("meshes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_MESHES",
                "meshes",
                "GLB JSON must contain a meshes array",
            )]
        })?;
    let materials = document
        .get("materials")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let accessors = document
        .get("accessors")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let node_names = nodes
        .iter()
        .filter_map(|node| node.get("name").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mesh_names = meshes
        .iter()
        .filter_map(|mesh| mesh.get("name").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let material_names = materials
        .iter()
        .filter_map(|material| material.get("name").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut lod_candidates = BTreeMap::new();
    for level in ["near", "mid", "far"] {
        let candidates = node_names
            .iter()
            .filter(|name| {
                let normalized = name.to_ascii_lowercase();
                normalized.contains(level)
                    || (level == "near" && normalized.contains("lod0"))
                    || (level == "mid" && normalized.contains("lod1"))
                    || (level == "far" && normalized.contains("lod2"))
            })
            .cloned()
            .collect::<Vec<_>>();
        lod_candidates.insert(level.to_owned(), candidates);
    }
    let mut triangle_count = 0u32;
    let mut node_triangle_counts = BTreeMap::new();
    for node in nodes {
        let Some(name) = node.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(mesh_index) = node
            .get("mesh")
            .and_then(serde_json::Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
        else {
            continue;
        };
        let Some(primitives) = meshes
            .get(mesh_index)
            .and_then(|mesh| mesh.get("primitives"))
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        let node_triangles = primitives
            .iter()
            .filter_map(|primitive| {
                let count = primitive
                    .get("indices")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|index| usize::try_from(index).ok())
                    .and_then(|index| accessors.get(index))
                    .and_then(|accessor| accessor.get("count"))
                    .and_then(serde_json::Value::as_u64)
                    .or_else(|| {
                        primitive
                            .get("attributes")
                            .and_then(|attributes| attributes.get("POSITION"))
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|index| usize::try_from(index).ok())
                            .and_then(|index| accessors.get(index))
                            .and_then(|accessor| accessor.get("count"))
                            .and_then(serde_json::Value::as_u64)
                    })?;
                triangle_count_for_mode(
                    primitive
                        .get("mode")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(4),
                    count,
                )
            })
            .fold(0u64, u64::saturating_add)
            .min(u64::from(u32::MAX)) as u32;
        node_triangle_counts.insert(name.to_owned(), node_triangles);
    }
    for mesh in meshes {
        let Some(primitives) = mesh.get("primitives").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for primitive in primitives {
            let count = primitive
                .get("indices")
                .and_then(serde_json::Value::as_u64)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| accessors.get(index))
                .and_then(|accessor| accessor.get("count"))
                .and_then(serde_json::Value::as_u64)
                .or_else(|| {
                    primitive
                        .get("attributes")
                        .and_then(|attributes| attributes.get("POSITION"))
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|index| usize::try_from(index).ok())
                        .and_then(|index| accessors.get(index))
                        .and_then(|accessor| accessor.get("count"))
                        .and_then(serde_json::Value::as_u64)
                });
            let Some(count) = count else { continue };
            let mode = primitive
                .get("mode")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(4);
            if let Some(triangles) = triangle_count_for_mode(mode, count) {
                triangle_count =
                    triangle_count.saturating_add(triangles.min(u64::from(u32::MAX)) as u32);
            }
        }
    }
    Ok(MorphGlbSourceSummary {
        version,
        node_names,
        mesh_names,
        material_names,
        lod_candidates,
        node_triangle_counts,
        triangle_count,
    })
}

/// Decode the first mesh primitive's POSITION and index accessors for a small
/// Studio preview. This is intentionally not a runtime importer: limits keep
/// it bounded, and the result is CPU-owned data that can later be replaced by
/// the compiled `.morphpack` path.
pub fn decode_glb_preview(bytes: &[u8]) -> Result<MorphGlbPreviewMesh, Vec<MorphDiagnostic>> {
    decode_glb_preview_node(bytes, None)
}

/// Decode the first primitive from a named node's mesh for the Studio LOD
/// preview. Passing `None` preserves the original first-mesh behavior used by
/// raw imports without authored LOD mappings.
pub fn decode_glb_preview_node(
    bytes: &[u8],
    node_name: Option<&str>,
) -> Result<MorphGlbPreviewMesh, Vec<MorphDiagnostic>> {
    decode_glb_preview_node_primitive(bytes, node_name, 0)
}

fn decode_glb_preview_node_primitive(
    bytes: &[u8],
    node_name: Option<&str>,
    primitive_index: usize,
) -> Result<MorphGlbPreviewMesh, Vec<MorphDiagnostic>> {
    if bytes.len() > MAX_SOURCE_GLB_BYTES {
        return Err(vec![error(
            "MORPH_GLB_TOO_LARGE",
            "geometry.file",
            format!("GLB exceeds {MAX_SOURCE_GLB_BYTES} bytes"),
        )]);
    }
    let (_version, json, bin) = read_glb_chunks(bytes).map_err(|diagnostic| vec![diagnostic])?;
    let document: serde_json::Value = serde_json::from_slice(&json).map_err(|parse_error| {
        vec![error(
            "MORPH_GLB_INVALID_JSON",
            "geometry.file",
            parse_error.to_string(),
        )]
    })?;
    let meshes = document
        .get("meshes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_MESHES",
                "meshes",
                "GLB JSON must contain a meshes array",
            )]
        })?;
    let mesh_index = if let Some(node_name) = node_name {
        document
            .get("nodes")
            .and_then(serde_json::Value::as_array)
            .and_then(|nodes| {
                nodes.iter().find_map(|node| {
                    (node.get("name").and_then(serde_json::Value::as_str) == Some(node_name))
                        .then(|| node.get("mesh"))
                        .flatten()
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|index| usize::try_from(index).ok())
                })
            })
            .ok_or_else(|| {
                vec![error(
                    "MORPH_GLB_PREVIEW_NODE_NOT_FOUND",
                    "nodes",
                    format!("preview node {node_name:?} is missing or has no mesh"),
                )]
            })?
    } else {
        0
    };
    let mesh = meshes.get(mesh_index).ok_or_else(|| {
        vec![error(
            "MORPH_GLB_INVALID_MESH_INDEX",
            "nodes",
            format!("preview node references missing mesh {mesh_index}"),
        )]
    })?;
    let mesh_name = mesh
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("preview-mesh")
        .to_owned();
    let primitive = mesh
        .get("primitives")
        .and_then(serde_json::Value::as_array)
        .and_then(|primitives| primitives.get(primitive_index))
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_PRIMITIVES",
                &format!("meshes[{mesh_index}].primitives[{primitive_index}]"),
                "mesh does not contain the requested primitive",
            )]
        })?;
    let material = primitive
        .get("material")
        .and_then(serde_json::Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| document.get("materials")?.as_array()?.get(index));
    let base_color = material
        .and_then(|material| material.get("pbrMetallicRoughness"))
        .and_then(|pbr| pbr.get("baseColorFactor"))
        .and_then(serde_json::Value::as_array)
        .and_then(|values| {
            let values = values
                .iter()
                .map(serde_json::Value::as_f64)
                .collect::<Option<Vec<_>>>()?;
            (values.len() == 4
                && values.iter().all(|value| value.is_finite())
                && values.iter().all(|value| (0.0..=1.0).contains(value)))
            .then(|| {
                [
                    values[0] as f32,
                    values[1] as f32,
                    values[2] as f32,
                    values[3] as f32,
                ]
            })
        });
    let use_avatar_tint = material
        .and_then(|material| material.get("extras"))
        .and_then(|extras| extras.get("cubaUseAvatarTint"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let position_accessor = primitive
        .get("attributes")
        .and_then(|attributes| attributes.get("POSITION"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_POSITION",
                "meshes[0].primitives[0]",
                "preview mesh needs a POSITION accessor",
            )]
        })?;
    let accessors = document
        .get("accessors")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_ACCESSORS",
                "accessors",
                "GLB JSON must contain an accessors array",
            )]
        })?;
    let buffer_views = document
        .get("bufferViews")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_BUFFER_VIEWS",
                "bufferViews",
                "GLB JSON must contain bufferViews",
            )]
        })?;
    let vertices = decode_positions(
        accessors.get(position_accessor).ok_or_else(|| {
            vec![error(
                "MORPH_GLB_INVALID_ACCESSOR",
                "meshes[0].primitives[0].attributes.POSITION",
                "POSITION accessor is out of range",
            )]
        })?,
        buffer_views,
        &bin,
    )?;
    let indices = if let Some(index_accessor) = primitive
        .get("indices")
        .and_then(serde_json::Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
    {
        decode_indices(
            accessors.get(index_accessor).ok_or_else(|| {
                vec![error(
                    "MORPH_GLB_INVALID_ACCESSOR",
                    "meshes[0].primitives[0].indices",
                    "index accessor is out of range",
                )]
            })?,
            buffer_views,
            &bin,
        )?
    } else {
        (0..u32::try_from(vertices.len()).map_err(|_| {
            vec![error(
                "MORPH_GLB_PREVIEW_LIMIT",
                "meshes[0]",
                "preview vertex count exceeds the supported limit",
            )]
        })?)
            .collect()
    };
    let skinning = if let Some(attributes) = primitive.get("attributes") {
        let joints_accessor = attributes
            .get("JOINTS_0")
            .and_then(serde_json::Value::as_u64)
            .and_then(|index| usize::try_from(index).ok());
        let weights_accessor = attributes
            .get("WEIGHTS_0")
            .and_then(serde_json::Value::as_u64)
            .and_then(|index| usize::try_from(index).ok());
        match (joints_accessor, weights_accessor) {
            (Some(joints_accessor), Some(weights_accessor)) => Some(decode_skinning(
                accessors.get(joints_accessor).ok_or_else(|| {
                    vec![error(
                        "MORPH_GLB_INVALID_ACCESSOR",
                        "meshes[0].primitives[0].attributes.JOINTS_0",
                        "JOINTS_0 accessor is out of range",
                    )]
                })?,
                accessors.get(weights_accessor).ok_or_else(|| {
                    vec![error(
                        "MORPH_GLB_INVALID_ACCESSOR",
                        "meshes[0].primitives[0].attributes.WEIGHTS_0",
                        "WEIGHTS_0 accessor is out of range",
                    )]
                })?,
                buffer_views,
                &bin,
                vertices.len(),
            )?),
            (None, None) => None,
            _ => {
                return Err(vec![error(
                    "MORPH_GLB_INCOMPLETE_SKINNING",
                    "meshes[0].primitives[0].attributes",
                    "JOINTS_0 and WEIGHTS_0 must be provided together",
                )]);
            }
        }
    } else {
        None
    };
    if indices
        .iter()
        .any(|index| usize::try_from(*index).map_or(true, |index| index >= vertices.len()))
    {
        return Err(vec![error(
            "MORPH_GLB_INVALID_INDEX",
            "meshes[0].primitives[0].indices",
            "preview index references a missing vertex",
        )]);
    }
    Ok(MorphGlbPreviewMesh {
        name: mesh_name,
        vertices,
        indices,
        base_color,
        use_avatar_tint,
        skinning,
    })
}

/// Compile a validated rigid accessory into a deterministic CPU mesh pack.
/// The format is intentionally small and renderer-neutral so the shared
/// runtime can adopt it later without making Studio depend on GPU code.
pub fn compile_morph_pack(
    manifest: &MorphSourceManifest,
    glb: &[u8],
) -> Result<(Vec<u8>, MorphPackSummary), Vec<MorphDiagnostic>> {
    let inspection = inspect_glb_bytes(manifest, glb)?;
    let manifest_json = serde_json::to_vec(manifest).map_err(|serialize_error| {
        vec![error(
            "MORPH_PACK_SERIALIZE_FAILED",
            "$",
            serialize_error.to_string(),
        )]
    })?;
    let mut meshes = Vec::new();
    for level in ["near", "mid", "far"] {
        let node = manifest.geometry.lod_nodes[level].as_str();
        let primitive_count = inspection.lods[level].primitive_count;
        let mut primitives = Vec::with_capacity(primitive_count);
        for primitive_index in 0..primitive_count {
            primitives.push(decode_glb_preview_node_primitive(
                glb,
                Some(node),
                primitive_index,
            )?);
        }
        meshes.push((level, primitives));
    }
    let mut pack = Vec::with_capacity(manifest_json.len() + 64);
    pack.extend_from_slice(MORPH_PACK_MAGIC);
    let skinned = manifest.attachment.mode == MorphAttachmentMode::Skinned;
    let multi_surface = meshes.iter().any(|(_, primitives)| primitives.len() > 1);
    write_u16(
        &mut pack,
        if multi_surface {
            MORPH_PACK_MULTI_SURFACE_SCHEMA_VERSION
        } else if skinned {
            MORPH_PACK_SKINNED_SCHEMA_VERSION
        } else {
            MORPH_PACK_SCHEMA_VERSION
        },
    );
    write_u16(&mut pack, 0);
    write_u32(
        &mut pack,
        u32::try_from(manifest_json.len()).map_err(|_| {
            vec![error(
                "MORPH_PACK_LIMIT",
                "$",
                "manifest is too large for a morph pack",
            )]
        })?,
    );
    pack.extend_from_slice(&manifest_json);
    let mut lod_triangle_counts = BTreeMap::new();
    for (level, primitives) in meshes {
        let triangle_count = inspection.lods[level].triangle_count;
        lod_triangle_counts.insert(level.to_owned(), triangle_count);
        let vertex_count = primitives
            .iter()
            .map(|mesh| mesh.vertices.len())
            .sum::<usize>();
        let index_count = primitives
            .iter()
            .map(|mesh| mesh.indices.len())
            .sum::<usize>();
        write_u32(&mut pack, triangle_count);
        write_u32(
            &mut pack,
            u32::try_from(vertex_count).map_err(|_| {
                vec![error(
                    "MORPH_PACK_LIMIT",
                    "geometry",
                    "vertex count is too large for a morph pack",
                )]
            })?,
        );
        write_u32(
            &mut pack,
            u32::try_from(index_count).map_err(|_| {
                vec![error(
                    "MORPH_PACK_LIMIT",
                    "geometry",
                    "index count is too large for a morph pack",
                )]
            })?,
        );
        if multi_surface {
            write_u16(
                &mut pack,
                u16::try_from(primitives.len()).map_err(|_| {
                    vec![error(
                        "MORPH_PACK_LIMIT",
                        "geometry",
                        "surface count is too large for a morph pack",
                    )]
                })?,
            );
            let mut index_start = 0u32;
            for mesh in &primitives {
                let surface_index_count = u32::try_from(mesh.indices.len()).map_err(|_| {
                    vec![error(
                        "MORPH_PACK_LIMIT",
                        "geometry",
                        "surface index count is too large for a morph pack",
                    )]
                })?;
                write_u32(&mut pack, index_start);
                write_u32(&mut pack, surface_index_count);
                let mut flags = 0u8;
                if mesh.base_color.is_some() {
                    flags |= 1;
                }
                if mesh.use_avatar_tint {
                    flags |= 2;
                }
                pack.push(flags);
                if let Some(color) = mesh.base_color {
                    for value in color {
                        pack.extend_from_slice(&value.to_le_bytes());
                    }
                }
                index_start = index_start
                    .checked_add(surface_index_count)
                    .ok_or_else(|| {
                        vec![error(
                            "MORPH_PACK_LIMIT",
                            "geometry",
                            "surface index ranges overflow the morph pack",
                        )]
                    })?;
            }
        } else {
            match primitives[0].base_color {
                Some(color) => {
                    pack.push(1);
                    for value in color {
                        pack.extend_from_slice(&value.to_le_bytes());
                    }
                }
                None => pack.push(0),
            }
        }
        for mesh in &primitives {
            for vertex in &mesh.vertices {
                for value in vertex {
                    pack.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
        if skinned {
            for mesh in &primitives {
                let Some(skinning) = &mesh.skinning else {
                    return Err(vec![error(
                        "MORPH_GLB_MISSING_SKINNING",
                        &format!("geometry.lodNodes.{level}"),
                        "skinned assets need JOINTS_0 and WEIGHTS_0 attributes",
                    )]);
                };
                for skin in skinning {
                    for joint in skin.joints {
                        pack.extend_from_slice(&joint.to_le_bytes());
                    }
                    for weight in skin.weights {
                        pack.extend_from_slice(&weight.to_le_bytes());
                    }
                }
            }
        }
        let mut vertex_start = 0u32;
        for mesh in primitives {
            for index in mesh.indices {
                let index = index.checked_add(vertex_start).ok_or_else(|| {
                    vec![error(
                        "MORPH_PACK_LIMIT",
                        "geometry",
                        "flattened vertex index overflows the morph pack",
                    )]
                })?;
                pack.extend_from_slice(&index.to_le_bytes());
            }
            vertex_start = vertex_start
                .checked_add(u32::try_from(mesh.vertices.len()).map_err(|_| {
                    vec![error(
                        "MORPH_PACK_LIMIT",
                        "geometry",
                        "vertex count is too large for a morph pack",
                    )]
                })?)
                .ok_or_else(|| {
                    vec![error(
                        "MORPH_PACK_LIMIT",
                        "geometry",
                        "flattened vertex ranges overflow the morph pack",
                    )]
                })?;
        }
        if pack.len() > MAX_MORPH_PACK_BYTES {
            return Err(vec![error(
                "MORPH_PACK_TOO_LARGE",
                "$",
                format!("compiled pack exceeds {MAX_MORPH_PACK_BYTES} bytes"),
            )]);
        }
    }
    Ok((
        pack.clone(),
        MorphPackSummary {
            asset_id: manifest.asset.id.to_string(),
            byte_len: pack.len(),
            lod_triangle_counts,
        },
    ))
}

fn write_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

const MAX_PREVIEW_VERTICES: usize = 200_000;
const MAX_PREVIEW_INDICES: usize = 600_000;

fn decode_positions(
    accessor: &serde_json::Value,
    buffer_views: &[serde_json::Value],
    bin: &[u8],
) -> Result<Vec<[f32; 3]>, Vec<MorphDiagnostic>> {
    let count = bounded_count(accessor, "POSITION")?;
    if accessor.get("type").and_then(serde_json::Value::as_str) != Some("VEC3")
        || accessor
            .get("componentType")
            .and_then(serde_json::Value::as_u64)
            != Some(5126)
    {
        return Err(vec![error(
            "MORPH_GLB_UNSUPPORTED_POSITION",
            "meshes[0].primitives[0].attributes.POSITION",
            "POSITION must be a float VEC3 accessor",
        )]);
    }
    let layout = accessor_layout(accessor, buffer_views, bin, 12, "POSITION")?;
    let mut vertices = Vec::with_capacity(count);
    for index in 0..count {
        let start = layout.start + index * layout.stride;
        let values = read_vec3(bin, start);
        let Some(values) = values else {
            return Err(vec![error(
                "MORPH_GLB_TRUNCATED_POSITION",
                "meshes[0].primitives[0].attributes.POSITION",
                "POSITION data is truncated",
            )]);
        };
        if !values.iter().all(|value| value.is_finite()) {
            return Err(vec![error(
                "MORPH_GLB_NONFINITE_POSITION",
                "meshes[0].primitives[0].attributes.POSITION",
                "POSITION data must be finite",
            )]);
        }
        vertices.push(values);
    }
    Ok(vertices)
}

fn decode_indices(
    accessor: &serde_json::Value,
    buffer_views: &[serde_json::Value],
    bin: &[u8],
) -> Result<Vec<u32>, Vec<MorphDiagnostic>> {
    let count = bounded_count(accessor, "indices")?;
    let component_type = accessor
        .get("componentType")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_UNSUPPORTED_INDICES",
                "meshes[0].primitives[0].indices",
                "index accessor needs a component type",
            )]
        })?;
    let component_size = match component_type {
        5121 => 1,
        5123 => 2,
        5125 => 4,
        _ => {
            return Err(vec![error(
                "MORPH_GLB_UNSUPPORTED_INDICES",
                "meshes[0].primitives[0].indices",
                "indices must use unsigned byte, short, or int",
            )]);
        }
    };
    if accessor.get("type").and_then(serde_json::Value::as_str) != Some("SCALAR") {
        return Err(vec![error(
            "MORPH_GLB_UNSUPPORTED_INDICES",
            "meshes[0].primitives[0].indices",
            "indices must be a SCALAR accessor",
        )]);
    }
    let layout = accessor_layout(accessor, buffer_views, bin, component_size, "indices")?;
    let mut indices = Vec::with_capacity(count);
    for index in 0..count {
        let start = layout.start + index * layout.stride;
        let Some(value) = read_index(bin, start, component_type) else {
            return Err(vec![error(
                "MORPH_GLB_TRUNCATED_INDICES",
                "meshes[0].primitives[0].indices",
                "index data is truncated",
            )]);
        };
        indices.push(value);
    }
    Ok(indices)
}

fn decode_skinning(
    joints_accessor: &serde_json::Value,
    weights_accessor: &serde_json::Value,
    buffer_views: &[serde_json::Value],
    bin: &[u8],
    vertex_count: usize,
) -> Result<Vec<MorphPackVertexSkin>, Vec<MorphDiagnostic>> {
    let joints_count = bounded_count(joints_accessor, "JOINTS_0")?;
    let weights_count = bounded_count(weights_accessor, "WEIGHTS_0")?;
    if joints_count != vertex_count || weights_count != vertex_count {
        return Err(vec![error(
            "MORPH_GLB_SKINNING_COUNT_MISMATCH",
            "meshes[0].primitives[0].attributes",
            "JOINTS_0 and WEIGHTS_0 counts must match POSITION",
        )]);
    }
    if joints_accessor
        .get("type")
        .and_then(serde_json::Value::as_str)
        != Some("VEC4")
        || !matches!(
            joints_accessor
                .get("componentType")
                .and_then(serde_json::Value::as_u64),
            Some(5121) | Some(5123)
        )
    {
        return Err(vec![error(
            "MORPH_GLB_UNSUPPORTED_SKIN_JOINTS",
            "meshes[0].primitives[0].attributes.JOINTS_0",
            "JOINTS_0 must be an unsigned byte or short VEC4 accessor",
        )]);
    }
    if weights_accessor
        .get("type")
        .and_then(serde_json::Value::as_str)
        != Some("VEC4")
        || weights_accessor
            .get("componentType")
            .and_then(serde_json::Value::as_u64)
            != Some(5126)
    {
        return Err(vec![error(
            "MORPH_GLB_UNSUPPORTED_SKIN_WEIGHTS",
            "meshes[0].primitives[0].attributes.WEIGHTS_0",
            "WEIGHTS_0 must be a float VEC4 accessor",
        )]);
    }
    let joints_component_type = joints_accessor
        .get("componentType")
        .and_then(serde_json::Value::as_u64)
        .expect("validated JOINTS_0 component type");
    let joints_component_size = if joints_component_type == 5121 { 1 } else { 2 };
    let joints_layout = accessor_layout(
        joints_accessor,
        buffer_views,
        bin,
        joints_component_size * 4,
        "JOINTS_0",
    )?;
    let weights_layout = accessor_layout(weights_accessor, buffer_views, bin, 16, "WEIGHTS_0")?;
    let mut skinning = Vec::with_capacity(vertex_count);
    for index in 0..vertex_count {
        let joints_start = joints_layout.start + index * joints_layout.stride;
        let Some(joints) = read_joints(bin, joints_start, joints_component_type) else {
            return Err(vec![error(
                "MORPH_GLB_TRUNCATED_SKIN_JOINTS",
                "meshes[0].primitives[0].attributes.JOINTS_0",
                "JOINTS_0 data is truncated",
            )]);
        };
        let weights_start = weights_layout.start + index * weights_layout.stride;
        let Some(weights) = read_vec4(bin, weights_start) else {
            return Err(vec![error(
                "MORPH_GLB_TRUNCATED_SKIN_WEIGHTS",
                "meshes[0].primitives[0].attributes.WEIGHTS_0",
                "WEIGHTS_0 data is truncated",
            )]);
        };
        if joints.iter().any(|joint| *joint >= 15)
            || weights
                .iter()
                .any(|weight| !weight.is_finite() || !(0.0..=1.0).contains(weight))
            || (weights.iter().sum::<f32>() - 1.0).abs() > 0.01
        {
            return Err(vec![error(
                "MORPH_GLB_INVALID_SKIN",
                &format!("meshes[0].primitives[0].attributes[{index}]"),
                "skin joints must reference the 15-joint rig and weights must be finite, normalized, and within 0..=1",
            )]);
        }
        skinning.push(MorphPackVertexSkin { joints, weights });
    }
    Ok(skinning)
}

struct AccessorLayout {
    start: usize,
    stride: usize,
}

fn accessor_layout(
    accessor: &serde_json::Value,
    buffer_views: &[serde_json::Value],
    bin: &[u8],
    element_size: usize,
    label: &str,
) -> Result<AccessorLayout, Vec<MorphDiagnostic>> {
    let view_index = accessor
        .get("bufferView")
        .and_then(serde_json::Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_MISSING_BUFFER_VIEW",
                label,
                "accessor must reference a bufferView",
            )]
        })?;
    let view = buffer_views.get(view_index).ok_or_else(|| {
        vec![error(
            "MORPH_GLB_INVALID_BUFFER_VIEW",
            label,
            "bufferView is out of range",
        )]
    })?;
    let view_start = view
        .get("byteOffset")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let stride = view
        .get("byteStride")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(element_size);
    if stride < element_size {
        return Err(vec![error(
            "MORPH_GLB_INVALID_STRIDE",
            label,
            "bufferView stride is smaller than the element",
        )]);
    }
    let accessor_offset = accessor
        .get("byteOffset")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let Some(start) = view_start.checked_add(accessor_offset) else {
        return Err(vec![error(
            "MORPH_GLB_INVALID_OFFSET",
            label,
            "accessor offset overflows",
        )]);
    };
    if start >= bin.len() {
        return Err(vec![error(
            "MORPH_GLB_INVALID_OFFSET",
            label,
            "accessor starts outside the BIN chunk",
        )]);
    }
    Ok(AccessorLayout { start, stride })
}

fn bounded_count(accessor: &serde_json::Value, label: &str) -> Result<usize, Vec<MorphDiagnostic>> {
    let count = accessor
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            vec![error(
                "MORPH_GLB_INVALID_COUNT",
                label,
                "accessor count is missing or too large",
            )]
        })?;
    let limit = if label == "indices" {
        MAX_PREVIEW_INDICES
    } else {
        MAX_PREVIEW_VERTICES
    };
    if count == 0 || count > limit {
        return Err(vec![error(
            "MORPH_GLB_PREVIEW_LIMIT",
            label,
            format!("accessor count must be 1..={limit}"),
        )]);
    }
    Ok(count)
}

fn read_vec3(bytes: &[u8], start: usize) -> Option<[f32; 3]> {
    let x = f32::from_le_bytes(bytes.get(start..start + 4)?.try_into().ok()?);
    let y = f32::from_le_bytes(bytes.get(start + 4..start + 8)?.try_into().ok()?);
    let z = f32::from_le_bytes(bytes.get(start + 8..start + 12)?.try_into().ok()?);
    Some([x, y, z])
}

fn read_vec4(bytes: &[u8], start: usize) -> Option<[f32; 4]> {
    Some([
        f32::from_le_bytes(bytes.get(start..start + 4)?.try_into().ok()?),
        f32::from_le_bytes(bytes.get(start + 4..start + 8)?.try_into().ok()?),
        f32::from_le_bytes(bytes.get(start + 8..start + 12)?.try_into().ok()?),
        f32::from_le_bytes(bytes.get(start + 12..start + 16)?.try_into().ok()?),
    ])
}

fn read_joints(bytes: &[u8], start: usize, component_type: u64) -> Option<[u16; 4]> {
    match component_type {
        5121 => Some([
            u16::from(*bytes.get(start)?),
            u16::from(*bytes.get(start + 1)?),
            u16::from(*bytes.get(start + 2)?),
            u16::from(*bytes.get(start + 3)?),
        ]),
        5123 => Some([
            u16::from_le_bytes(bytes.get(start..start + 2)?.try_into().ok()?),
            u16::from_le_bytes(bytes.get(start + 2..start + 4)?.try_into().ok()?),
            u16::from_le_bytes(bytes.get(start + 4..start + 6)?.try_into().ok()?),
            u16::from_le_bytes(bytes.get(start + 6..start + 8)?.try_into().ok()?),
        ]),
        _ => None,
    }
}

fn read_index(bytes: &[u8], start: usize, component_type: u64) -> Option<u32> {
    match component_type {
        5121 => bytes.get(start).copied().map(u32::from),
        5123 => Some(u32::from(u16::from_le_bytes(
            bytes.get(start..start + 2)?.try_into().ok()?,
        ))),
        5125 => Some(u32::from_le_bytes(
            bytes.get(start..start + 4)?.try_into().ok()?,
        )),
        _ => None,
    }
}

fn node_transform_is_applied(node: &serde_json::Value) -> bool {
    const EPSILON: f64 = 0.0001;
    let values = |field: &str| {
        node.get(field).map(|value| {
            value
                .as_array()
                .and_then(|values| {
                    values
                        .iter()
                        .map(serde_json::Value::as_f64)
                        .collect::<Option<Vec<_>>>()
                })
                .filter(|values| values.iter().all(|value| value.is_finite()))
        })
    };
    let translation = values("translation").is_none_or(|values| {
        values.is_some_and(|values| {
            values.len() == 3 && values.iter().all(|value| value.abs() <= EPSILON)
        })
    });
    let scale = values("scale").is_none_or(|values| {
        values.is_some_and(|values| {
            values.len() == 3 && values.iter().all(|value| (value - 1.0).abs() <= EPSILON)
        })
    });
    let rotation = values("rotation").is_none_or(|values| {
        values.is_some_and(|values| {
            values.len() == 4
                && values[..3].iter().all(|value| value.abs() <= EPSILON)
                && (values[3].abs() - 1.0).abs() <= EPSILON
        })
    });
    let matrix = values("matrix").is_none_or(|values| {
        values.is_some_and(|values| {
            let identity = [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ];
            values.len() == identity.len()
                && values
                    .iter()
                    .zip(identity)
                    .all(|(value, expected)| (value - expected).abs() <= EPSILON)
        })
    });
    translation && rotation && scale && matrix
}

fn node_hierarchy_transform_is_applied(nodes: &[serde_json::Value], node_index: usize) -> bool {
    let mut current = node_index;
    let mut visited = BTreeSet::new();
    loop {
        if !visited.insert(current) {
            return false;
        }
        let Some(node) = nodes.get(current) else {
            return false;
        };
        if !node_transform_is_applied(node) {
            return false;
        }
        let parents = nodes
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                candidate
                    .get("children")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|children| {
                        children.iter().any(|child| {
                            child.as_u64().and_then(|child| usize::try_from(child).ok())
                                == Some(current)
                        })
                    })
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        match parents.as_slice() {
            [] => return true,
            [parent] => current = *parent,
            _ => return false,
        }
    }
}

fn triangle_count_for_mode(mode: u64, count: u64) -> Option<u64> {
    match mode {
        4 => Some(count / 3),
        5 | 6 if count >= 3 => Some(count - 2),
        5 | 6 => Some(0),
        _ => None,
    }
}

fn read_glb_chunks(bytes: &[u8]) -> Result<(u32, Vec<u8>, Vec<u8>), MorphDiagnostic> {
    if bytes.len() < 12 {
        return Err(error(
            "MORPH_GLB_TRUNCATED",
            "geometry.file",
            "GLB header is truncated",
        ));
    }
    if &bytes[0..4] != b"glTF" {
        return Err(error(
            "MORPH_GLB_INVALID_MAGIC",
            "geometry.file",
            "GLB magic must be glTF",
        ));
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().expect("checked header length"));
    if version != 2 {
        return Err(error(
            "MORPH_GLB_UNSUPPORTED_VERSION",
            "geometry.file",
            "only GLB version 2 is supported",
        ));
    }
    let declared_length =
        u32::from_le_bytes(bytes[8..12].try_into().expect("checked header length"));
    let Ok(declared_length) = usize::try_from(declared_length) else {
        return Err(error(
            "MORPH_GLB_INVALID_LENGTH",
            "geometry.file",
            "GLB length is too large",
        ));
    };
    if declared_length != bytes.len() {
        return Err(error(
            "MORPH_GLB_INVALID_LENGTH",
            "geometry.file",
            "declared GLB length does not match the file",
        ));
    }
    let mut offset = 12;
    let mut json = None;
    let mut bin = Vec::new();
    while offset < bytes.len() {
        if bytes.len() - offset < 8 {
            return Err(error(
                "MORPH_GLB_TRUNCATED_CHUNK",
                "geometry.file",
                "GLB chunk header is truncated",
            ));
        }
        let chunk_length = u32::from_le_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .expect("checked chunk header length"),
        );
        let Ok(chunk_length) = usize::try_from(chunk_length) else {
            return Err(error(
                "MORPH_GLB_INVALID_CHUNK_LENGTH",
                "geometry.file",
                "GLB chunk length is too large",
            ));
        };
        let chunk_type = &bytes[offset + 4..offset + 8];
        let chunk_start = offset + 8;
        let Some(chunk_end) = chunk_start.checked_add(chunk_length) else {
            return Err(error(
                "MORPH_GLB_INVALID_CHUNK_LENGTH",
                "geometry.file",
                "GLB chunk length overflows the file",
            ));
        };
        if chunk_end > bytes.len() {
            return Err(error(
                "MORPH_GLB_TRUNCATED_CHUNK",
                "geometry.file",
                "GLB chunk extends past the file",
            ));
        }
        match chunk_type {
            b"JSON" if json.is_none() => json = Some(bytes[chunk_start..chunk_end].to_vec()),
            b"JSON" => {
                return Err(error(
                    "MORPH_GLB_MULTIPLE_JSON_CHUNKS",
                    "geometry.file",
                    "GLB must contain exactly one JSON chunk",
                ));
            }
            b"BIN\0" if bin.is_empty() => bin.extend_from_slice(&bytes[chunk_start..chunk_end]),
            b"BIN\0" => {
                return Err(error(
                    "MORPH_GLB_MULTIPLE_BIN_CHUNKS",
                    "geometry.file",
                    "GLB must contain at most one BIN chunk",
                ));
            }
            _ => {}
        }
        offset = chunk_end;
    }
    let Some(mut json) = json else {
        return Err(error(
            "MORPH_GLB_MISSING_JSON",
            "geometry.file",
            "GLB must contain a JSON chunk",
        ));
    };
    while matches!(json.last(), Some(byte) if *byte == b'\0' || byte.is_ascii_whitespace()) {
        json.pop();
    }
    Ok((version, json, bin))
}

fn validate_geometry(source: &MorphGeometrySource, diagnostics: &mut Vec<MorphDiagnostic>) {
    let valid_path = !source.file.is_empty()
        && source.file.len() <= 240
        && source.file.is_ascii()
        && !source.file.starts_with('/')
        && !source.file.contains('\\')
        && source
            .file
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..");
    if !valid_path || !(source.file.ends_with(".glb") || source.file.ends_with(".gltf")) {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_GEOMETRY_PATH",
            "geometry.file",
            "geometry must be a safe relative .glb or .gltf path",
        ));
    }

    let mut lod_nodes = BTreeSet::new();
    for level in ["near", "mid", "far"] {
        let node_path = format!("geometry.lodNodes.{level}");
        match source.lod_nodes.get(level) {
            Some(node) if !node.is_empty() && node.len() <= MAX_NODE_NAME_BYTES => {
                if !lod_nodes.insert(node) {
                    diagnostics.push(error(
                        "MORPH_SOURCE_DUPLICATE_LOD_NODE",
                        &node_path,
                        "near, mid, and far must reference distinct GLB nodes",
                    ));
                }
            }
            Some(_) => diagnostics.push(error(
                "MORPH_SOURCE_INVALID_LOD_NODE",
                &node_path,
                "LOD node names must be 1..=96 bytes",
            )),
            None => diagnostics.push(error(
                "MORPH_SOURCE_MISSING_LOD_NODE",
                &node_path,
                "near, mid, and far LOD nodes are required",
            )),
        }
    }
    if source.lod_nodes.len() != 3 {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_LOD_COUNT",
            "geometry",
            "only near, mid, and far LOD node entries are allowed",
        ));
    }
    for level in source.triangle_counts.keys() {
        if !matches!(level.as_str(), "near" | "mid" | "far") {
            diagnostics.push(error(
                "MORPH_SOURCE_INVALID_LEGACY_TRIANGLE_COUNT",
                "geometry.triangleCounts",
                "legacy triangle counts may only contain near, mid, and far",
            ));
        }
    }
}

fn validate_attachment(attachment: &MorphAttachment, diagnostics: &mut Vec<MorphDiagnostic>) {
    if attachment.joint.is_empty()
        || attachment.joint.len() > MAX_NODE_NAME_BYTES
        || !attachment.joint.is_ascii()
        || !attachment.joint.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_ATTACHMENT_JOINT",
            "attachment.joint",
            "attachment joints must be lower-case semantic names",
        ));
    }
    if !attachment
        .translation
        .iter()
        .all(|value| value.is_finite() && value.abs() <= 10.0)
    {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_ATTACHMENT_TRANSLATION",
            "attachment.translation",
            "translation values must be finite and within +/-10 units",
        ));
    }
    if !attachment.rotation.iter().all(|value| value.is_finite()) {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_ATTACHMENT_ROTATION",
            "attachment.rotation",
            "rotation values must be finite",
        ));
    } else {
        let length = attachment
            .rotation
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        if !(0.99..=1.01).contains(&length) {
            diagnostics.push(error(
                "MORPH_SOURCE_INVALID_ATTACHMENT_ROTATION",
                "attachment.rotation",
                "rotation quaternion must be normalized",
            ));
        }
    }
    if !attachment
        .scale
        .iter()
        .all(|value| value.is_finite() && (0.01..=100.0).contains(value))
    {
        diagnostics.push(error(
            "MORPH_SOURCE_INVALID_ATTACHMENT_SCALE",
            "attachment.scale",
            "scale values must be finite and within 0.01..=100",
        ));
    }
}

fn error(code: &str, path: &str, message: impl Into<String>) -> MorphDiagnostic {
    MorphDiagnostic {
        code: code.to_owned(),
        path: path.to_owned(),
        message: message.into(),
    }
}

fn identity_translation() -> [f32; 3] {
    [0.0; 3]
}

fn identity_rotation() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn identity_scale() -> [f32; 3] {
    [1.0; 3]
}

#[cfg(test)]
mod tests {
    use super::*;
    use cubacadabra_morphs::{MorphAssetId, MorphLodBudget, MorphProvenance, MorphSourceReference};
    use serde_json::json;

    fn fixture() -> MorphSourceManifest {
        let id = MorphAssetId::parse("cuba:headwear/star-cap.v1").unwrap();
        MorphSourceManifest {
            schema_version: 1,
            asset: MorphAssetDefinition {
                id,
                kind: MorphAssetKind::Headwear,
                display_name: "Star Cap".to_owned(),
                rig_profile: Some(MorphAssetId::parse("cuba:rig/biped15.v1").unwrap()),
                fit_profiles: vec![MorphAssetId::parse("cuba:fit/person-standard.v1").unwrap()],
                supported_bases: vec![MorphAssetId::parse("cuba:base/person.v1").unwrap()],
                occupied_slots: vec!["headwear".to_owned()],
                coverage: vec!["scalp".to_owned()],
                conflicts: vec!["full-headwear".to_owned()],
                materials: vec!["cloth".to_owned()],
                lod: MorphLodBudget {
                    near: 1200,
                    mid: 500,
                    far: 100,
                },
                required_capabilities: vec![
                    cubacadabra_morphs::CapabilityId::parse("mesh.rigid.v1").unwrap(),
                ],
                source: Some(MorphSourceReference {
                    geometry: "star-cap.glb".to_owned(),
                }),
                provenance: MorphProvenance {
                    source: "Blender fixture".to_owned(),
                    license: "Cubacadabra official".to_owned(),
                },
            },
            geometry: MorphGeometrySource {
                file: "star-cap.glb".to_owned(),
                lod_nodes: BTreeMap::from([
                    ("near".to_owned(), "StarCap_LOD0".to_owned()),
                    ("mid".to_owned(), "StarCap_LOD1".to_owned()),
                    ("far".to_owned(), "StarCap_LOD2".to_owned()),
                ]),
                triangle_counts: BTreeMap::from([
                    ("near".to_owned(), 1200),
                    ("mid".to_owned(), 500),
                    ("far".to_owned(), 100),
                ]),
            },
            attachment: MorphAttachment {
                mode: MorphAttachmentMode::Rigid,
                joint: "head".to_owned(),
                translation: [0.0, 0.45, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            },
            skin: None,
        }
    }

    #[test]
    fn publish_requires_applied_blender_node_transforms() {
        assert!(node_transform_is_applied(
            &json!({"translation": [0.0, 0.0, 0.00001]})
        ));
        assert!(!node_transform_is_applied(
            &json!({"scale": [2.0, 2.0, 2.0]})
        ));
        assert!(!node_transform_is_applied(
            &json!({"rotation": [0.0, 0.2, 0.0, 0.98]})
        ));
        assert!(!node_transform_is_applied(&json!({"matrix": [1.0, 0.0]})));
        assert!(node_hierarchy_transform_is_applied(
            &[json!({"children": [1]}), json!({})],
            1
        ));
        assert!(!node_hierarchy_transform_is_applied(
            &[
                json!({"scale": [2.0, 2.0, 2.0], "children": [1]}),
                json!({})
            ],
            1
        ));
    }

    #[test]
    fn rigid_fixture_inspects_cleanly() {
        let inspection = fixture().inspect().unwrap();
        assert_eq!(inspection.asset_id, "cuba:headwear/star-cap.v1");
        assert_eq!(inspection.lod_count, 3);
        assert_eq!(inspection.attachment_joint, "head");
    }

    #[test]
    fn parser_round_trips_the_source_manifest() {
        let source = serde_json::to_string(&fixture()).unwrap();
        let parsed = parse_source_manifest(&source).unwrap();
        assert_eq!(parsed, fixture());
    }

    #[test]
    fn rejects_missing_lods_and_unsafe_attachment_data() {
        let mut manifest = fixture();
        manifest.geometry.lod_nodes.remove("far");
        manifest.attachment.joint = "../head".to_owned();
        manifest.attachment.rotation = [0.0, 0.0, 0.0, 0.0];
        let diagnostics = manifest.validate();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.code == "MORPH_SOURCE_MISSING_LOD_NODE")
        );
        assert!(
            diagnostics
                .iter()
                .any(|item| item.code == "MORPH_SOURCE_INVALID_ATTACHMENT_JOINT")
        );
        assert!(
            diagnostics
                .iter()
                .any(|item| item.code == "MORPH_SOURCE_INVALID_ATTACHMENT_ROTATION")
        );
    }

    #[test]
    fn rejects_reusing_one_node_for_multiple_lods() {
        let mut manifest = fixture();
        manifest
            .geometry
            .lod_nodes
            .insert("mid".to_owned(), "StarCap_LOD0".to_owned());
        let diagnostics = manifest.validate();
        assert!(diagnostics.iter().any(|item| {
            item.code == "MORPH_SOURCE_DUPLICATE_LOD_NODE" && item.path == "geometry.lodNodes.mid"
        }));
    }

    fn glb_fixture(manifest: &MorphSourceManifest, counts: [u64; 3]) -> Vec<u8> {
        let levels = ["near", "mid", "far"];
        let nodes = levels
            .iter()
            .enumerate()
            .map(|(index, level)| {
                json!({ "name": manifest.geometry.lod_nodes[*level], "mesh": index })
            })
            .collect::<Vec<_>>();
        let meshes = counts
            .iter()
            .enumerate()
            .map(|(index, _count)| {
                json!({
                    "name": format!("mesh-{index}"),
                    "primitives": [{
                        "attributes": { "POSITION": index },
                        "mode": 4
                    }]
                })
            })
            .collect::<Vec<_>>();
        let accessors = counts
            .iter()
            .map(|count| json!({ "count": count * 3, "type": "SCALAR" }))
            .collect::<Vec<_>>();
        let document = json!({
            "asset": { "version": "2.0" },
            "nodes": nodes,
            "meshes": meshes,
            "accessors": accessors
        });
        let mut json_bytes = serde_json::to_vec(&document).unwrap();
        while !json_bytes.len().is_multiple_of(4) {
            json_bytes.push(b' ');
        }
        let total_length = 12 + 8 + json_bytes.len();
        let mut bytes = Vec::with_capacity(total_length);
        bytes.extend_from_slice(b"glTF");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&(total_length as u32).to_le_bytes());
        bytes.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"JSON");
        bytes.extend_from_slice(&json_bytes);
        bytes
    }

    #[test]
    fn glb_fixture_matches_declared_lods_and_triangle_budgets() {
        let manifest = fixture();
        let glb = glb_fixture(&manifest, [1200, 500, 100]);
        let inspection = inspect_glb_bytes(&manifest, &glb).unwrap();
        assert_eq!(inspection.version, 2);
        assert_eq!(inspection.bin_bytes, 0);
        assert_eq!(inspection.lods["near"].triangle_count, 1200);
        assert_eq!(inspection.lods["far"].node, "StarCap_LOD2");
    }

    #[test]
    fn glb_fixture_rejects_triangle_budget_mismatch() {
        let manifest = fixture();
        let glb = glb_fixture(&manifest, [1201, 500, 100]);
        let diagnostics = inspect_glb_bytes(&manifest, &glb).unwrap_err();
        assert!(diagnostics.iter().any(|item| {
            item.code == "MORPH_GLB_TRIANGLE_BUDGET_EXCEEDED" && item.path == "asset.lod.near"
        }));
    }

    #[test]
    fn source_summary_reports_nodes_meshes_and_lod_candidates() {
        let manifest = fixture();
        let summary = inspect_glb_source(&glb_fixture(&manifest, [1200, 500, 100])).unwrap();
        assert_eq!(summary.version, 2);
        assert_eq!(
            summary.node_names,
            ["StarCap_LOD0", "StarCap_LOD1", "StarCap_LOD2"]
        );
        assert_eq!(summary.mesh_names.len(), 3);
        assert_eq!(summary.lod_candidates["near"], ["StarCap_LOD0"]);
        assert_eq!(summary.triangle_count, 1800);
    }

    fn preview_glb_fixture() -> Vec<u8> {
        let document = json!({
            "asset": { "version": "2.0" },
            "meshes": [{
                "name": "Triangle",
                "primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]
            }],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}
            ],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 6}
            ],
            "buffers": [{"byteLength": 44}]
        });
        let mut json_bytes = serde_json::to_vec(&document).unwrap();
        while !json_bytes.len().is_multiple_of(4) {
            json_bytes.push(b' ');
        }
        let mut bin = Vec::new();
        for vertex in [[-1.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for value in vertex {
                bin.extend_from_slice(&value.to_le_bytes());
            }
        }
        for index in [0_u16, 1, 2] {
            bin.extend_from_slice(&index.to_le_bytes());
        }
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let total_length = 12 + 8 + json_bytes.len() + 8 + bin.len();
        let mut bytes = Vec::with_capacity(total_length);
        bytes.extend_from_slice(b"glTF");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&(total_length as u32).to_le_bytes());
        bytes.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"JSON");
        bytes.extend_from_slice(&json_bytes);
        bytes.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"BIN\0");
        bytes.extend_from_slice(&bin);
        bytes
    }

    #[test]
    fn decodes_bounded_preview_positions_and_indices() {
        let preview = decode_glb_preview(&preview_glb_fixture()).unwrap();
        assert_eq!(preview.name, "Triangle");
        assert_eq!(preview.vertices.len(), 3);
        assert_eq!(preview.indices, [0, 1, 2]);
        assert_eq!(preview.vertices[2], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn compiles_the_checked_in_three_lod_fixture_deterministically() {
        let source = include_str!("../fixtures/top_hat_3lod.morph.json");
        let manifest = parse_source_manifest(source).unwrap();
        let glb = include_bytes!("../fixtures/top_hat_3lod.glb");
        let (first, summary) = compile_morph_pack(&manifest, glb).unwrap();
        let (second, repeated_summary) = compile_morph_pack(&manifest, glb).unwrap();
        assert_eq!(first, second);
        assert_eq!(summary, repeated_summary);
        assert_eq!(&first[..8], MORPH_PACK_MAGIC);
        assert_eq!(summary.asset_id, "cuba:headwear/top-hat-3lod.v1");
        assert_eq!(summary.lod_triangle_counts["near"], 24);
        assert_eq!(summary.lod_triangle_counts["mid"], 8);
        assert_eq!(summary.lod_triangle_counts["far"], 4);
    }
}

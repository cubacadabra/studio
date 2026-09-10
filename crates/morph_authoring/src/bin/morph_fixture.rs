use serde_json::json;
use std::{env, fs, path::Path};

fn main() {
    let output = env::args()
        .nth(1)
        .unwrap_or_else(|| "fixtures/top_hat_3lod.glb".to_owned());
    let bytes = build_fixture();
    if let Some(parent) = Path::new(&output).parent() {
        fs::create_dir_all(parent).expect("fixture directory must be writable");
    }
    fs::write(&output, bytes).expect("fixture GLB must be writable");
    println!("wrote {output}");
}

fn build_fixture() -> Vec<u8> {
    let positions: [[f32; 3]; 16] = [
        [-1.2, 0.0, -0.9],
        [1.2, 0.0, -0.9],
        [1.2, 0.0, 0.9],
        [-1.2, 0.0, 0.9],
        [-1.2, 0.2, -0.9],
        [1.2, 0.2, -0.9],
        [1.2, 0.2, 0.9],
        [-1.2, 0.2, 0.9],
        [-0.65, 0.2, -0.55],
        [0.65, 0.2, -0.55],
        [0.65, 0.2, 0.55],
        [-0.65, 0.2, 0.55],
        [-0.65, 1.7, -0.55],
        [0.65, 1.7, -0.55],
        [0.65, 1.7, 0.55],
        [-0.65, 1.7, 0.55],
    ];
    let brim = box_indices(0);
    let crown = box_indices(8);
    let near_indices = brim.iter().chain(crown.iter()).copied().collect::<Vec<_>>();
    let meshes = [
        ("TopHat_Near", near_indices.clone()),
        ("TopHat_Mid", near_indices[..24].to_vec()),
        ("TopHat_Far", near_indices[..12].to_vec()),
    ];
    let mut bin = Vec::new();
    let mut buffer_views = Vec::new();
    let mut accessors = Vec::new();
    let mut json_meshes = Vec::new();
    let mut json_nodes = Vec::new();
    for (mesh_index, (name, indices)) in meshes.iter().enumerate() {
        align4(&mut bin, 0);
        let position_offset = bin.len();
        for position in positions {
            for value in position {
                bin.extend_from_slice(&value.to_le_bytes());
            }
        }
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": position_offset,
            "byteLength": positions.len() * 12
        }));
        let position_accessor = accessors.len();
        accessors.push(json!({
            "bufferView": buffer_views.len() - 1,
            "componentType": 5126,
            "count": positions.len(),
            "type": "VEC3",
            "min": [-1.2, 0.0, -0.9],
            "max": [1.2, 1.7, 0.9]
        }));
        align4(&mut bin, 0);
        let index_offset = bin.len();
        for index in indices {
            bin.extend_from_slice(&index.to_le_bytes());
        }
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": index_offset,
            "byteLength": indices.len() * 2
        }));
        let index_accessor = accessors.len();
        accessors.push(json!({
            "bufferView": buffer_views.len() - 1,
            "componentType": 5123,
            "count": indices.len(),
            "type": "SCALAR"
        }));
        json_meshes.push(json!({
            "name": name,
            "primitives": [{
                "attributes": {"POSITION": position_accessor},
                "indices": index_accessor,
                "material": 0,
                "mode": 4
            }]
        }));
        json_nodes.push(json!({"name": name, "mesh": mesh_index}));
    }
    let document = json!({
        "asset": {"version": "2.0", "generator": "Cubacadabra Studio fixture"},
        "scene": 0,
        "scenes": [{"nodes": [0, 1, 2]}],
        "nodes": json_nodes,
        "meshes": json_meshes,
        "materials": [{
            "name": "hat-blue",
            "pbrMetallicRoughness": {"baseColorFactor": [0.08, 0.12, 0.85, 1.0]}
        }],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": buffer_views,
        "accessors": accessors
    });
    let mut json_bytes = serde_json::to_vec(&document).expect("fixture JSON must serialize");
    align4(&mut json_bytes, b' ');
    align4(&mut bin, 0);
    let total_length = 12 + 8 + json_bytes.len() + 8 + bin.len();
    let mut glb = Vec::with_capacity(total_length);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total_length as u32).to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x004E4942u32.to_le_bytes());
    glb.extend_from_slice(&bin);
    glb
}

fn box_indices(offset: u16) -> Vec<u16> {
    [
        0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 1, 5, 6, 1, 6, 2, 2, 6, 7, 2, 7, 3,
        4, 0, 3, 4, 3, 7,
    ]
    .into_iter()
    .map(|index| index + offset)
    .collect()
}

fn align4(bytes: &mut Vec<u8>, fill: u8) {
    while bytes.len() % 4 != 0 {
        bytes.push(fill);
    }
}

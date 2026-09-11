#!/usr/bin/env python3
"""Generate small Blender-compatible skinned clothing proof assets.

The meshes are deliberately simple migration fixtures. Each asset uses the
canonical 15-joint hierarchy and four-weight attributes, so Studio can compile
the files through the same schema 2 path as a Blender export.
"""

import json
import math
import struct
import sys
from pathlib import Path

import generate_person_asset as person


ASSETS = {
    "top": {
        "id": "cuba:top/person-top.v1",
        "kind": "top",
        "name": "Person Top",
        "slots": ["shirt"],
        "coverage": ["torso", "arms"],
        "color": [0.10, 0.52, 0.46, 1.0],
        "string_color": [0.92, 0.84, 0.68, 1.0],
        "pieces": [
            (1, (0.0, 0.0, 0.0), (1.10, 1.04, 0.73), "torso"),
            (3, (0.0, -0.40, 0.0), (0.46, 1.13, 0.49), "sleeve"),
            (6, (0.0, -0.40, 0.0), (0.46, 1.13, 0.49), "sleeve"),
        ],
    },
    "bottom": {
        "id": "cuba:bottom/person-bottom.v1",
        "kind": "bottom",
        "name": "Person Bottom",
        "slots": ["pants"],
        "coverage": ["legs"],
        "color": [0.20, 0.28, 0.66, 1.0],
        "pieces": [
            (9, (0.0, -0.15, 0.0), (0.46, 0.66, 0.47), "shorts"),
            (12, (0.0, -0.15, 0.0), (0.46, 0.66, 0.47), "shorts"),
        ],
    },
    "shoes": {
        "id": "cuba:footwear/person-shoes.v1",
        "kind": "footwear",
        "name": "Person Shoes",
        "slots": ["shoes"],
        "coverage": ["feet"],
        "color": [0.08, 0.08, 0.12, 1.0],
        "pieces": [
            (11, (0.0, 0.155, -0.09), (0.50, 0.29, 0.76), "shoe"),
            (14, (0.0, 0.155, -0.09), (0.50, 0.29, 0.76), "shoe"),
        ],
    },
}


def hood_shell(vertices, indices, joints, weights, detail):
    """Add an open, face-safe hood shell skinned to the head joint.

    The front opening leaves the analytic face unobstructed. The two angular
    patches meet around the back and sides of the head, giving the top a
    recognizable hood silhouette without adding a second bone hierarchy.
    """
    radial, rows = {3: (16, 7), 2: (12, 5), 1: (8, 3)}[detail]
    for start_angle, end_angle in [(0.0, math.pi * 1.27), (math.pi * 1.70, math.tau)]:
        start = len(vertices)
        for row in range(rows + 1):
            t = row / rows
            y = -0.30 + 0.86 * t
            crown = math.sin(math.pi * min(1.0, t * 0.94))
            radius_x = 0.53 * (0.82 + 0.18 * crown)
            radius_z = 0.56 * (0.82 + 0.18 * crown)
            for column in range(radial + 1):
                angle = start_angle + (end_angle - start_angle) * column / radial
                vertices.append((
                    radius_x * math.cos(angle),
                    y,
                    0.055 + radius_z * math.sin(angle),
                ))
                joints.append([2, 0, 0, 0])
                weights.append([1.0, 0.0, 0.0, 0.0])
                if row < rows and column < radial:
                    a = start + row * (radial + 1) + column
                    b = a + radial + 1
                    indices.extend([a, b, a + 1, a + 1, b, b + 1])


def clothing_lod_geometry(pieces, detail):
    vertices, indices, joints, weights = [], [], [], []
    for joint, center, size, shape in pieces:
        person.profile_piece(vertices, indices, joints, weights, center, size, joint, detail, shape)
    if pieces and pieces[0][3] == "torso":
        hood_shell(vertices, indices, joints, weights, detail)
        # Two cords hang over the front of the hoodie. Their small center
        # position and forward z offset keep them visible on the torso.
        for x in (-0.12, 0.12):
            person.profile_piece(
                vertices,
                indices,
                joints,
                weights,
                (x, 0.16, -0.39),
                (0.038, 0.34, 0.038),
                1,
                detail,
                "string",
            )
            person.profile_piece(
                vertices,
                indices,
                joints,
                weights,
                (x, -0.025, -0.39),
                (0.070, 0.085, 0.070),
                1,
                detail,
                "pebble",
            )
    return vertices, indices, joints, weights


def make_glb(output, asset):
    binary = bytearray()
    views, accessors, meshes = [], [], []

    def add_blob(blob, target=None):
        while len(binary) % 4:
            binary.append(0)
        offset = len(binary)
        binary.extend(blob)
        view = {"buffer": 0, "byteOffset": offset, "byteLength": len(blob)}
        if target is not None:
            view["target"] = target
        views.append(view)
        return len(views) - 1

    triangle_counts = {}
    for level, detail in [("Near", 3), ("Mid", 2), ("Far", 1)]:
        positions, indices, joints, weights = clothing_lod_geometry(asset["pieces"], detail)
        triangle_counts[level.lower()] = len(indices) // 3
        blobs = [
            b"".join(struct.pack("<3f", *value) for value in positions),
            b"".join(struct.pack("<I", value) for value in indices),
            b"".join(struct.pack("<4H", *value) for value in joints),
            b"".join(struct.pack("<4f", *value) for value in weights),
        ]
        pos_view, idx_view, joint_view, weight_view = [
            add_blob(blob, 34962 if index != 1 else 34963)
            for index, blob in enumerate(blobs)
        ]
        pos_accessor = len(accessors)
        accessors.append({"bufferView": pos_view, "componentType": 5126, "count": len(positions), "type": "VEC3"})
        idx_accessor = len(accessors)
        accessors.append({"bufferView": idx_view, "componentType": 5125, "count": len(indices), "type": "SCALAR"})
        joint_accessor = len(accessors)
        accessors.append({"bufferView": joint_view, "componentType": 5123, "count": len(joints), "type": "VEC4"})
        weight_accessor = len(accessors)
        accessors.append({"bufferView": weight_view, "componentType": 5126, "count": len(weights), "type": "VEC4"})
        meshes.append({
            "name": f"Person{asset['kind'].title()}_{level}",
            "primitives": [{
                "attributes": {"POSITION": pos_accessor, "JOINTS_0": joint_accessor, "WEIGHTS_0": weight_accessor},
                "indices": idx_accessor,
                "material": 0,
                "mode": 4,
            }],
        })

    nodes = []
    for name, parent_index, translation in person.JOINTS:
        node = {"name": name, "translation": list(translation)}
        if parent_index is not None:
            nodes[parent_index].setdefault("children", [])
        nodes.append(node)
    for index, (_, parent_index, _) in enumerate(person.JOINTS):
        if parent_index is not None:
            nodes[parent_index].setdefault("children", []).append(index)
    lod_node_indices = []
    for index, level in enumerate(["Near", "Mid", "Far"]):
        lod_node_indices.append(len(nodes))
        nodes.append({"name": f"Person{asset['kind'].title()}_{level}", "mesh": index, "skin": 0})
    nodes[0].setdefault("children", []).extend(lod_node_indices)
    inverse_bind = b"".join(
        struct.pack("<16f", *([1.0 if i % 5 == 0 else 0.0 for i in range(16)]))
        for _ in person.JOINTS
    )
    inverse_view = add_blob(inverse_bind)
    inverse_accessor = len(accessors)
    accessors.append({"bufferView": inverse_view, "componentType": 5126, "count": len(person.JOINTS), "type": "MAT4"})
    document = {
        "asset": {"version": "2.0", "generator": "Cubacadabra Phase 5 clothing fixture"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": nodes,
        "meshes": meshes,
        "skins": [{"name": "Person_Skin", "joints": list(range(len(person.JOINTS))), "inverseBindMatrices": inverse_accessor, "skeleton": 0}],
        "materials": [{"name": asset["name"], "pbrMetallicRoughness": {"baseColorFactor": asset["color"], "roughnessFactor": 0.82, "metallicFactor": 0.0}}],
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": len(binary)}],
    }
    json_blob = json.dumps(document, separators=(",", ":")).encode("utf-8")
    json_blob += b" " * ((4 - len(json_blob) % 4) % 4)
    binary += b"\0" * ((4 - len(binary) % 4) % 4)
    total = 12 + 8 + len(json_blob) + 8 + len(binary)
    output.write_bytes(struct.pack("<III", 0x46546C67, 2, total))
    with output.open("ab") as stream:
        stream.write(struct.pack("<II", len(json_blob), 0x4E4F534A))
        stream.write(json_blob)
        stream.write(struct.pack("<II", len(binary), 0x004E4942))
        stream.write(binary)
    return triangle_counts


def write_sidecar(glb_path, asset):
    glb_path.with_suffix(".morph.json").write_text(json.dumps({
        "schemaVersion": 2,
        "asset": {
            "id": asset["id"],
            "kind": asset["kind"],
            "displayName": asset["name"],
            "rigProfile": "cuba:rig/biped15.v1",
            "fitProfiles": ["cuba:fit/person-standard.v1"],
            "supportedBases": ["cuba:base/person.v1"],
            "occupiedSlots": asset["slots"],
            "coverage": asset["coverage"],
            "conflicts": [],
            "materials": [asset["kind"]],
            "lod": asset["lod"],
            "requiredCapabilities": ["skin.biped15-linear.v1", "material.cuba-pbr.v1"],
            "source": {"geometry": glb_path.name},
            "provenance": {"source": "Cubacadabra generated Blender-compatible Phase 5 clothing fixture", "license": "Cubacadabra official"},
        },
        "geometry": {"file": glb_path.name, "lodNodes": {
            "near": f"Person{asset['kind'].title()}_Near",
            "mid": f"Person{asset['kind'].title()}_Mid",
            "far": f"Person{asset['kind'].title()}_Far",
        }},
        "attachment": {"mode": "skinned", "joint": "root", "translation": [0, 0, 0], "rotation": [0, 0, 0, 1], "scale": [1, 1, 1]},
        "skin": {"skeleton": "Person_Skin", "jointOrder": [name for name, _, _ in person.JOINTS], "maxInfluences": 4},
    }, indent=2) + "\n")


def main():
    output_dir = Path(sys.argv[1] if len(sys.argv) > 1 else "/Users/aa/Downloads")
    output_dir.mkdir(parents=True, exist_ok=True)
    for key, asset in ASSETS.items():
        glb_path = output_dir / f"person_{key}.glb"
        asset["lod"] = make_glb(glb_path, asset)
        write_sidecar(glb_path, asset)
        print(glb_path)


if __name__ == "__main__":
    main()

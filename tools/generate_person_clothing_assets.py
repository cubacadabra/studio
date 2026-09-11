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
        "materials": [
            ("top", "Person Top", [0.10, 0.52, 0.46, 1.0], True, 0.82),
            ("drawstring", "White Drawstring", [0.97, 0.97, 0.95, 1.0], False, 0.68),
        ],
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
        "materials": [
            ("bottom", "Person Bottom", [0.20, 0.28, 0.66, 1.0], True, 0.82),
        ],
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
        "materials": [
            ("footwear", "Navy Upper", [0.08, 0.11, 0.16, 1.0], False, 0.72),
            ("sole", "Ivory Sole", [0.96, 0.93, 0.84, 1.0], False, 0.76),
            ("laces", "Ivory Laces", [0.98, 0.97, 0.94, 1.0], False, 0.66),
        ],
        "pieces": [
            (11, (0.0, 0.155, -0.09), (0.50, 0.29, 0.76), "shoe"),
            (14, (0.0, 0.155, -0.09), (0.50, 0.29, 0.76), "shoe"),
        ],
    },
}


def hood_ring(vertices, indices, joints, weights, detail):
    """Build the soft oval collar that gives a lowered hood its opening."""
    radial, rows = {3: (28, 11), 2: (24, 10), 1: (20, 9)}[detail]
    start = len(vertices)
    center = (0.0, 0.45, 0.12)
    size = (0.73, 0.21, 0.72)
    for row in range(rows + 1):
        t = row / rows
        v, u = math.sin(t * math.tau), math.cos(t * math.tau)
        for column in range(radial + 1):
            angle = column / radial * math.tau
            point = (
                (0.33 + 0.15 * u) * math.cos(angle),
                0.22 * v + 0.26 * math.sin(angle),
                (0.32 + 0.15 * u) * math.sin(angle),
            )
            vertices.append(tuple(center[axis] + point[axis] * size[axis] for axis in range(3)))
            joints.append([1, 0, 0, 0])
            weights.append([1.0, 0.0, 0.0, 0.0])
            if row < rows and column < radial:
                a = start + row * (radial + 1) + column
                b = a + radial + 1
                indices.extend([a, b, a + 1, a + 1, b, b + 1])


def folded_hood(vertices, indices, joints, weights, detail):
    """Add the lowered hood profile used by the established Person art."""
    hood_ring(vertices, indices, joints, weights, detail)
    person.profile_piece(
        vertices,
        indices,
        joints,
        weights,
        (0.0, 0.30, 0.30),
        (0.69, 0.60, 0.38),
        1,
        detail,
        "pebble",
    )


def hoodie_pocket(vertices, indices, joints, weights, detail):
    """Add a shallow kangaroo pocket that follows the front torso surface."""
    person.profile_piece(
        vertices,
        indices,
        joints,
        weights,
        (0.0, -0.21, -0.345),
        (0.66, 0.32, 0.10),
        1,
        detail,
        "pebble",
    )


def normalize(vector):
    length = math.sqrt(sum(value * value for value in vector))
    return tuple(value / length for value in vector)


def cross(a, b):
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


def curved_cord(vertices, indices, joints, weights, side, detail):
    """Sweep a slim capped tube from the neckline to a short aglet."""
    radial = {3: 10, 2: 8, 1: 6}[detail]
    points = [
        (side * 0.105, 0.505, -0.190),
        (side * 0.125, 0.435, -0.235),
        (side * 0.145, 0.315, -0.285),
        (side * 0.145, 0.175, -0.325),
        (side * 0.135, 0.055, -0.350),
        (side * 0.135, 0.010, -0.350),
    ]
    start = len(vertices)
    for row, point in enumerate(points):
        previous = points[max(0, row - 1)]
        following = points[min(len(points) - 1, row + 1)]
        tangent = normalize(tuple(following[axis] - previous[axis] for axis in range(3)))
        basis_x = normalize(cross(tangent, (0.0, 0.0, 1.0)))
        basis_z = cross(tangent, basis_x)
        radius = 0.022 if row == 0 else (0.015 if row < len(points) - 2 else 0.020)
        for column in range(radial + 1):
            angle = column / radial * math.tau
            vertices.append(tuple(
                point[axis]
                + basis_x[axis] * math.cos(angle) * radius
                + basis_z[axis] * math.sin(angle) * radius
                for axis in range(3)
            ))
            joints.append([1, 0, 0, 0])
            weights.append([1.0, 0.0, 0.0, 0.0])
            if row < len(points) - 1 and column < radial:
                a = start + row * (radial + 1) + column
                b = a + radial + 1
                indices.extend([a, b, a + 1, a + 1, b, b + 1])
    for row, top in [(0, False), (len(points) - 1, True)]:
        ring = start + row * (radial + 1)
        cap = len(vertices)
        vertices.append(points[row])
        joints.append([1, 0, 0, 0])
        weights.append([1.0, 0.0, 0.0, 0.0])
        for column in range(radial):
            a = ring + column
            indices.extend([cap, a + 1, a] if top else [cap, a, a + 1])


def shoe_detail_surfaces(detail):
    """Recreate the established Person shoe sole and three raised lace bands."""
    soles = ([], [], [], [])
    laces = ([], [], [], [])
    # The foot joint rests 0.05 units above the floor. The sole profile is
    # centered so its lowest normalized point (-0.5) lands exactly on it.
    sole_center_y = -0.05 + 0.5 * 0.095
    for joint in (11, 14):
        person.profile_piece(
            *soles,
            (0.0, sole_center_y, -0.09),
            (0.51, 0.095, 0.77),
            joint,
            detail,
            "shoe",
        )
        for offset in (-0.32, 0.0, 0.32):
            person.profile_piece(
                *laces,
                (0.0, 0.26 + offset * 0.7 * 0.065, -0.24 + offset * 0.24),
                (0.31, 0.065 * 0.28, 0.24 * 0.15),
                joint,
                max(1, detail - 1),
                "pebble",
            )
    return soles, laces


def clothing_lod_surfaces(asset, detail):
    pieces = asset["pieces"]
    cloth = ([], [], [], [])
    vertices, indices, joints, weights = cloth
    for joint, center, size, shape in pieces:
        person.profile_piece(vertices, indices, joints, weights, center, size, joint, detail, shape)
    if pieces and pieces[0][3] == "torso":
        folded_hood(vertices, indices, joints, weights, detail)
        hoodie_pocket(vertices, indices, joints, weights, detail)
        cords = ([], [], [], [])
        for side in (-1.0, 1.0):
            curved_cord(*cords, side, detail)
        return [cloth, cords]
    if pieces and pieces[0][3] == "shoe":
        return [cloth, *shoe_detail_surfaces(detail)]
    return [cloth]


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
        surfaces = clothing_lod_surfaces(asset, detail)
        if len(surfaces) != len(asset["materials"]):
            raise ValueError(f"surface/material mismatch for {asset['id']}")
        triangle_counts[level.lower()] = sum(len(surface[1]) // 3 for surface in surfaces)
        primitives = []
        for material, (positions, indices, joints, weights) in enumerate(surfaces):
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
            primitives.append({
                "attributes": {"POSITION": pos_accessor, "JOINTS_0": joint_accessor, "WEIGHTS_0": weight_accessor},
                "indices": idx_accessor,
                "material": material,
                "mode": 4,
            })
        meshes.append({
            "name": f"Person{asset['kind'].title()}_{level}",
            "primitives": primitives,
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
    materials = []
    for _, name, color, use_avatar_tint, roughness in asset["materials"]:
        materials.append({
            "name": name,
            "extras": {"cubaUseAvatarTint": use_avatar_tint},
            "pbrMetallicRoughness": {
                "baseColorFactor": color,
                "roughnessFactor": roughness,
                "metallicFactor": 0.0,
            },
        })
    document = {
        "asset": {"version": "2.0", "generator": "Cubacadabra Phase 5 clothing fixture"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": nodes,
        "meshes": meshes,
        "skins": [{"name": "Person_Skin", "joints": list(range(len(person.JOINTS))), "inverseBindMatrices": inverse_accessor, "skeleton": 0}],
        "materials": materials,
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
            "materials": [material[0] for material in asset["materials"]],
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

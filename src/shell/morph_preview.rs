use super::*;

#[derive(Clone, Copy)]
struct MorphPreviewProjection {
    rect: Rect,
    center: [f32; 3],
    pixels_per_unit: f32,
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}

impl MorphPreviewProjection {
    fn project_world(self, vertex: [f32; 3]) -> egui::Pos2 {
        egui::pos2(
            self.rect.center().x
                + (vertex[0] - self.center[0]) * self.pixels_per_unit
                + (vertex[2] - self.center[2]) * self.pixels_per_unit * 0.12,
            self.rect.center().y - (vertex[1] - self.center[1]) * self.pixels_per_unit
                + (vertex[2] - self.center[2]) * self.pixels_per_unit * 0.06,
        )
    }

    fn transform_mesh(self, vertex: [f32; 3]) -> [f32; 3] {
        let scaled = [
            vertex[0] * self.scale[0],
            vertex[1] * self.scale[1],
            vertex[2] * self.scale[2],
        ];
        let [qx, qy, qz, qw] = self.rotation;
        let q = [qx, qy, qz];
        let twice_cross = cross3(q, scaled).map(|value| value * 2.0);
        let rotated = add3(
            scaled,
            add3(twice_cross.map(|value| value * qw), cross3(q, twice_cross)),
        );
        add3(rotated, self.translation)
    }

    fn project_mesh(self, vertex: [f32; 3]) -> egui::Pos2 {
        self.project_world(self.transform_mesh(vertex))
    }
}

fn morph_preview_projection(
    rect: Rect,
    mesh: &MorphGlbPreviewMesh,
    attachment: &MorphAttachment,
) -> Option<MorphPreviewProjection> {
    if mesh.vertices.is_empty() {
        return None;
    }
    // Keep the standard person head in frame so attachment scale is visible
    // instead of being hidden by an isolated auto-fit preview.
    let mut min: [f32; 3] = [-0.55, -0.46, -0.39];
    let mut max: [f32; 3] = [0.55, 0.46, 0.39];
    let projection = MorphPreviewProjection {
        rect,
        center: [0.0; 3],
        pixels_per_unit: 1.0,
        translation: attachment.translation,
        rotation: attachment.rotation,
        scale: attachment.scale,
    };
    for vertex in &mesh.vertices {
        let vertex = projection.transform_mesh(*vertex);
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    let span = (max[0] - min[0])
        .max(max[1] - min[1])
        .max(max[2] - min[2])
        .max(0.0001);
    let pixels_per_unit = (rect.width().min(rect.height()) * 0.78) / span;
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    Some(MorphPreviewProjection {
        center,
        pixels_per_unit,
        ..projection
    })
}

fn paint_morph_head_reference(
    ui: &egui::Ui,
    rect: Rect,
    mesh: &MorphGlbPreviewMesh,
    attachment: &MorphAttachment,
    colors: Palette,
) {
    let Some(projection) = morph_preview_projection(rect, mesh, attachment) else {
        return;
    };
    let minimum = projection.project_world([-0.55, 0.46, 0.0]);
    let maximum = projection.project_world([0.55, -0.46, 0.0]);
    let head = Rect::from_min_max(minimum, maximum);
    ui.painter()
        .rect_filled(head, 18.0, colors.secondary_text.linear_multiply(0.16));
    ui.painter().rect_stroke(
        head,
        18.0,
        Stroke::new(1.0, colors.secondary_text.linear_multiply(0.45)),
        StrokeKind::Inside,
    );
}

fn paint_morph_surface(
    ui: &egui::Ui,
    rect: Rect,
    mesh: &MorphGlbPreviewMesh,
    attachment: &MorphAttachment,
    color: Color32,
) {
    if mesh.indices.len() < 3 {
        return;
    }
    let Some(projection) = morph_preview_projection(rect, mesh, attachment) else {
        return;
    };
    let light = normalize3([0.35, 0.75, 0.65]);
    let base_color = mesh.base_color.unwrap_or([
        f32::from(color.r()) / 255.0,
        f32::from(color.g()) / 255.0,
        f32::from(color.b()) / 255.0,
        1.0,
    ]);
    let mut triangles = Vec::new();
    for triangle in mesh
        .indices
        .chunks(3)
        .filter(|triangle| triangle.len() == 3)
    {
        let Some(a) = mesh.vertices.get(triangle[0] as usize).copied() else {
            continue;
        };
        let Some(b) = mesh.vertices.get(triangle[1] as usize).copied() else {
            continue;
        };
        let Some(c) = mesh.vertices.get(triangle[2] as usize).copied() else {
            continue;
        };
        let a = projection.transform_mesh(a);
        let b = projection.transform_mesh(b);
        let c = projection.transform_mesh(c);
        let points = [
            projection.project_world(a),
            projection.project_world(b),
            projection.project_world(c),
        ];
        // Low-detail exports can contain faces that are valid in 3D but
        // collapse to a sub-pixel sliver in this fixed front preview. egui's
        // polygon fill turns those into distracting bars, so leave them to
        // the wireframe inspection instead.
        if projected_triangle_area(points) < 0.5 {
            continue;
        }
        let normal = cross3(sub3(b, a), sub3(c, a));
        let brightness = (dot3(normalize3(normal), light).abs() * 0.55 + 0.45).clamp(0.0, 1.0);
        triangles.push(((a[2] + b[2] + c[2]) / 3.0, points, brightness));
    }
    triangles.sort_by(|first, second| first.0.total_cmp(&second.0));
    for (_, points, brightness) in triangles {
        let fill = Color32::from_rgba_unmultiplied(
            (base_color[0] * brightness * 255.0) as u8,
            (base_color[1] * brightness * 255.0) as u8,
            (base_color[2] * brightness * 255.0) as u8,
            (base_color[3] * 185.0) as u8,
        );
        ui.painter().add(egui::Shape::convex_polygon(
            points.to_vec(),
            fill,
            Stroke::NONE,
        ));
    }
}

pub(crate) fn projected_triangle_area(points: [egui::Pos2; 3]) -> f32 {
    ((points[1].x - points[0].x) * (points[2].y - points[0].y)
        - (points[1].y - points[0].y) * (points[2].x - points[0].x))
        .abs()
        * 0.5
}

fn paint_morph_wireframe(
    ui: &egui::Ui,
    rect: Rect,
    mesh: &MorphGlbPreviewMesh,
    attachment: &MorphAttachment,
    color: Color32,
) {
    if mesh.vertices.is_empty() || mesh.indices.len() < 3 {
        return;
    }
    let Some(projection) = morph_preview_projection(rect, mesh, attachment) else {
        return;
    };
    let stroke = Stroke::new(1.0, color);
    for triangle in mesh
        .indices
        .chunks(3)
        .filter(|triangle| triangle.len() == 3)
    {
        let Some(a) = mesh.vertices.get(triangle[0] as usize).copied() else {
            continue;
        };
        let Some(b) = mesh.vertices.get(triangle[1] as usize).copied() else {
            continue;
        };
        let Some(c) = mesh.vertices.get(triangle[2] as usize).copied() else {
            continue;
        };
        let points = [
            projection.project_mesh(a),
            projection.project_mesh(b),
            projection.project_mesh(c),
        ];
        ui.painter().line_segment([points[0], points[1]], stroke);
        ui.painter().line_segment([points[1], points[2]], stroke);
        ui.painter().line_segment([points[2], points[0]], stroke);
    }
}

fn sub3(first: [f32; 3], second: [f32; 3]) -> [f32; 3] {
    [
        first[0] - second[0],
        first[1] - second[1],
        first[2] - second[2],
    ]
}

fn add3(first: [f32; 3], second: [f32; 3]) -> [f32; 3] {
    [
        first[0] + second[0],
        first[1] + second[1],
        first[2] + second[2],
    ]
}

fn cross3(first: [f32; 3], second: [f32; 3]) -> [f32; 3] {
    [
        first[1] * second[2] - first[2] * second[1],
        first[2] * second[0] - first[0] * second[2],
        first[0] * second[1] - first[1] * second[0],
    ]
}

fn dot3(first: [f32; 3], second: [f32; 3]) -> f32 {
    first[0] * second[0] + first[1] * second[1] + first[2] * second[2]
}

fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let length = dot3(value, value).sqrt();
    if length > f32::EPSILON {
        [value[0] / length, value[1] / length, value[2] / length]
    } else {
        [0.0, 1.0, 0.0]
    }
}

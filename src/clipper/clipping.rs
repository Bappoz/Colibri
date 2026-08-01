use crate::math::utils::Vec4d;

// Posição no Clip Space antes da divisão por W
#[derive(Clone, Copy, Debug)]
pub struct ClipVertex {
    pub pos: Vec4d,
    pub uv: [f64; 2],
    pub intensity: f64,
}

/// Retorna o ponto interpoladona intereção
/// da aresta com plano Near
fn intersect_near(v1: &ClipVertex, v2: &ClipVertex) -> ClipVertex {
    // Fator de interpolacao t = (w1 + z1) / ((w1 + z1) - (w2 - z2))
    let d1 = v1.pos.z() + v1.pos.w();
    let d2 = v2.pos.z() + v2.pos.w();
    let den = d1 - d2;

    let t = if den.abs() > 1e-6 { d1 / den } else { 0.0 };

    let lerp = |a: f64, b: f64| a + t * (b - a);

    // Interpolação Linear da posição 4d
    ClipVertex {
        pos: Vec4d::new(
            lerp(v1.pos.x(), v2.pos.x()),
            lerp(v1.pos.y(), v2.pos.y()),
            lerp(v1.pos.z(), v2.pos.z()),
            lerp(v1.pos.w(), v2.pos.w()),
        ),
        uv: [lerp(v1.uv[0], v2.uv[0]), lerp(v1.uv[1], v2.uv[1])],
        intensity: lerp(v1.intensity, v2.intensity),
    }
}

// Clipa um triangulo contra o plano Near
// Pode retorna 0, 1 ou 2 traingulos
pub fn clip_triangle_near(v0: ClipVertex, v1: ClipVertex, v2: ClipVertex) -> Vec<[ClipVertex; 3]> {
    let mut inside = Vec::with_capacity(3);
    let mut outside = Vec::with_capacity(3);

    let vertices = [v0, v1, v2];

    // No Clip Space, um ponto está na frente do near se: z >= -w
    for v in &vertices {
        if v.pos.z() >= -(v.pos.w()) {
            inside.push(*v);
        } else {
            outside.push(*v);
        }
    }

    let mut clipped_triangles = Vec::new();
    match inside.len() {
        3 => {
            clipped_triangles.push([v0, v1, v2]);
        }
        2 => {
            let v_out = outside[0];
            let i0 = vertices.iter().position(|v| v.pos == v_out.pos).unwrap();

            let v_in1 = vertices[(i0 + 1) % 3];
            let v_in2 = vertices[(i0 + 2) % 3];

            let intercept1 = intersect_near(&v_in1, &v_out);
            let intercept2 = intersect_near(&v_in2, &v_out);

            clipped_triangles.push([v_in1, v_in2, intercept1]);
            clipped_triangles.push([v_in2, intercept2, intercept1]);
        }
        1 => {
            let v_in = inside[0];
            let i0 = vertices.iter().position(|v| v.pos == v_in.pos).unwrap();

            let v_out1 = vertices[(i0 + 1) % 3];
            let v_out2 = vertices[(i0 + 2) % 3];

            let intercept1 = intersect_near(&v_in, &v_out1);
            let intercept2 = intersect_near(&v_in, &v_out2);

            clipped_triangles.push([v_in, intercept1, intercept2]);
        }
        _ => {}
    }
    clipped_triangles
}

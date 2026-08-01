use crate::math::utils::Vec3d;

#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn load_from_obj(path: &str) -> Self {
        let (models, _materials) = tobj::load_obj(
            path,
            &tobj::LoadOptions {
                single_index: true,
                triangulate: true,
                ..Default::default()
            },
        )
        .expect("failed to load .obj");

        let mesh = &models[0].mesh;
        let vertex_count = mesh.positions.len() / 3;
        let mut vertices = Vec::with_capacity(vertex_count);

        for i in 0..vertex_count {
            let position = [
                mesh.positions[i * 3],
                mesh.positions[i * 3 + 1],
                mesh.positions[i * 3 + 2],
            ];

            let normal = if mesh.normals.is_empty() {
                [0.0, 0.0, 0.0] // Arquivo nao possui normai -> calcula por face
            } else {
                [
                    mesh.normals[i * 3],
                    mesh.normals[i * 3 + 1],
                    mesh.normals[i * 3 + 2],
                ]
            };

            let uv = if mesh.texcoords.is_empty() {
                [0.0, 0.0]
            } else {
                [mesh.texcoords[i * 2], mesh.texcoords[i * 2 + 1]]
            };

            vertices.push(Vertex {
                position,
                normal,
                uv,
            });
        }

        if mesh.normals.is_empty() {
            Self::compute_vertex_normals(&mut vertices, &mesh.indices);
        }

        Self {
            vertices,
            indices: mesh.indices.clone(),
        }
    }

    fn compute_vertex_normals(vertices: &mut [Vertex], indices: &[u32]) {
        let mut accum = vec![[0.0f64; 3]; vertices.len()];

        for tri in indices.chunks_exact(3) {
            let [i0, i1, i2] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let p0 = Vec3d::new(
                vertices[i0].position[0] as f64,
                vertices[i0].position[1] as f64,
                vertices[i0].position[2] as f64,
            );
            let p1 = Vec3d::new(
                vertices[i1].position[0] as f64,
                vertices[i1].position[1] as f64,
                vertices[i1].position[2] as f64,
            );
            let p2 = Vec3d::new(
                vertices[i2].position[0] as f64,
                vertices[i2].position[1] as f64,
                vertices[i2].position[2] as f64,
            );

            let face_normal = (p1 - p0).cross(&(p2 - p0));
            for &i in &[i0, i1, i2] {
                accum[i][0] += face_normal.x();
                accum[i][1] += face_normal.y();
                accum[i][2] += face_normal.z();
            }
        }

        for (v, n) in vertices.iter_mut().zip(accum) {
            let normalized = Vec3d::new(n[0], n[1], n[2]).normalize();
            v.normal = [
                normalized.x() as f32,
                normalized.y() as f32,
                normalized.z() as f32,
            ];
        }
    }

    /// Quad no plano XY, virado pra +Z (mesma convenção de winding da face
    /// frontal do cube.obj). Serve pra testar UV perspective-correct: nenhum
    /// dos .obj do projeto tem `vt`, então sem isso não tem UV variável pra ver.
    pub fn textured_quad() -> Self {
        Self {
            vertices: vec![
                Vertex { position: [-1.0, -1.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 1.0] },
                Vertex { position: [1.0, -1.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 1.0] },
                Vertex { position: [1.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 0.0] },
                Vertex { position: [-1.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0] },
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }
}

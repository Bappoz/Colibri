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

        Self {
            vertices,
            indices: mesh.indices.clone(),
        }
    }
}

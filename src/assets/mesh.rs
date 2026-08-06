//! Indexed triangle meshes and `.obj` loading.

use crate::error::{Error, Result};
use crate::math::Vec3d;

/// A single mesh vertex.
///
/// Stored as `f32` arrays rather than [`Vec3d`]: meshes are the bulkiest
/// thing the engine keeps in RAM and vertex data is only ever read, widened to
/// `f64` once per frame at transform time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    /// Position in model space.
    pub position: [f32; 3],
    /// Unit normal in model space, used for the diffuse lighting term.
    pub normal: [f32; 3],
    /// Texture coordinates, `(0, 0)` at the top-left texel.
    pub uv: [f32; 2],
}

/// An indexed triangle mesh in model space.
///
/// `indices` is always a multiple of three; every consecutive triple is one
/// triangle wound **counter-clockwise** when seen from the front, which is the
/// winding [`crate::render::raster`] relies on to cull back faces.
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    /// Unique vertices, addressed by `indices`.
    pub vertices: Vec<Vertex>,
    /// Triangle list: three indices into `vertices` per triangle.
    pub indices: Vec<u32>,
}

impl Mesh {
    /// Loads a Wavefront `.obj` file, merging every model it contains into a
    /// single mesh.
    ///
    /// Faces are triangulated and the vertex streams are de-interleaved into a
    /// single index buffer. Missing attributes are filled in:
    ///
    /// * no `vn` — normals are derived from the faces
    ///   ([`Mesh::compute_vertex_normals`]);
    /// * no `vt` — UVs default to `(0, 0)`, which samples one flat texel.
    pub fn load_from_obj(path: &str) -> Result<Self> {
        let (models, _materials) = tobj::load_obj(
            path,
            &tobj::LoadOptions {
                // One index per vertex: the rasterizer wants a single stream,
                // not the three parallel ones the format allows.
                single_index: true,
                triangulate: true,
                ..Default::default()
            },
        )
        .map_err(|e| Error::MeshLoad {
            path: path.to_string(),
            reason: e.to_string(),
        })?;

        let mut mesh = Mesh::default();
        let mut has_normals = true;

        for model in &models {
            let source = &model.mesh;
            let vertex_count = source.positions.len() / 3;
            // Indices are local to each model, so they shift by however many
            // vertices were already merged in.
            let base = mesh.vertices.len() as u32;

            has_normals &= !source.normals.is_empty();
            mesh.vertices.reserve(vertex_count);

            for i in 0..vertex_count {
                let position = [
                    source.positions[i * 3],
                    source.positions[i * 3 + 1],
                    source.positions[i * 3 + 2],
                ];

                let normal = if source.normals.is_empty() {
                    [0.0, 0.0, 0.0] // filled in below, from the faces
                } else {
                    [
                        source.normals[i * 3],
                        source.normals[i * 3 + 1],
                        source.normals[i * 3 + 2],
                    ]
                };

                let uv = if source.texcoords.is_empty() {
                    [0.0, 0.0]
                } else {
                    // `.obj` puts the UV origin at the bottom-left, images put
                    // it at the top-left — hence the flipped V.
                    [source.texcoords[i * 2], 1.0 - source.texcoords[i * 2 + 1]]
                };

                mesh.vertices.push(Vertex {
                    position,
                    normal,
                    uv,
                });
            }

            mesh.indices.extend(source.indices.iter().map(|i| i + base));
        }

        if mesh.indices.is_empty() {
            return Err(Error::EmptyMesh {
                path: path.to_string(),
            });
        }

        if !has_normals {
            mesh.compute_vertex_normals();
        }

        Ok(mesh)
    }

    /// Number of triangles in the mesh.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Distance from the model-space origin to the farthest vertex.
    ///
    /// Bundled models range from a half-unit cube to a teapot several units
    /// across, so anything that wants to frame a mesh — a camera, a default
    /// scale — has to ask how big it is rather than assume. Measured from the
    /// origin, not from the centroid, because that is the point the model
    /// matrix rotates around.
    ///
    /// Returns `0.0` for an empty mesh.
    pub fn bounding_radius(&self) -> f64 {
        self.vertices
            .iter()
            .map(|v| {
                let p = v.position;
                ((p[0] as f64).powi(2) + (p[1] as f64).powi(2) + (p[2] as f64).powi(2)).sqrt()
            })
            .fold(0.0, f64::max)
    }

    /// Whether the mesh carries usable texture coordinates.
    ///
    /// A mesh whose `.obj` had no `vt` lines ends up with every UV at the
    /// origin, which samples one flat texel — the texture is loaded and
    /// applied, it just cannot vary. Most of the bundled models are in that
    /// state, so the engine warns instead of leaving it looking like a bug.
    pub fn has_texture_coords(&self) -> bool {
        self.vertices.iter().any(|v| v.uv != [0.0, 0.0])
    }

    /// Derives smooth vertex normals from the faces.
    ///
    /// Each face normal is accumulated onto its three vertices and the sum is
    /// normalized at the end. The cross product is left unnormalized on
    /// purpose: its magnitude is twice the triangle area, so large faces
    /// weigh more than slivers — the usual area-weighted average.
    pub fn compute_vertex_normals(&mut self) {
        let mut accumulated = vec![Vec3d::ZERO; self.vertices.len()];

        for triangle in self.indices.chunks_exact(3) {
            let [i0, i1, i2] = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            let (p0, p1, p2) = (
                self.position_of(i0),
                self.position_of(i1),
                self.position_of(i2),
            );

            let face_normal = (p1 - p0).cross(&(p2 - p0));
            accumulated[i0] += face_normal;
            accumulated[i1] += face_normal;
            accumulated[i2] += face_normal;
        }

        for (vertex, normal) in self.vertices.iter_mut().zip(accumulated) {
            let n = normal.normalize();
            vertex.normal = [n.x() as f32, n.y() as f32, n.z() as f32];
        }
    }

    /// Widens vertex `i`'s position to the engine's `f64` math type.
    fn position_of(&self, i: usize) -> Vec3d {
        let p = self.vertices[i].position;
        Vec3d::new(p[0] as f64, p[1] as f64, p[2] as f64)
    }

    /// A unit quad on the XY plane facing `+Z`, wound like the front face of
    /// `assets/cube.obj`.
    ///
    /// None of the bundled `.obj` files carry `vt` data, so this is the only
    /// geometry with UVs that actually vary across the surface — which makes
    /// it the test case for perspective-correct texturing.
    pub fn textured_quad() -> Self {
        Self {
            vertices: vec![
                Vertex {
                    position: [-1.0, -1.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    uv: [0.0, 1.0],
                },
                Vertex {
                    position: [1.0, -1.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    uv: [1.0, 1.0],
                },
                Vertex {
                    position: [1.0, 1.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    uv: [1.0, 0.0],
                },
                Vertex {
                    position: [-1.0, 1.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    uv: [0.0, 0.0],
                },
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O quad de teste são dois triângulos que compartilham a diagonal.
    #[test]
    fn quad_is_two_triangles() {
        let quad = Mesh::textured_quad();
        assert_eq!(quad.triangle_count(), 2);
        assert_eq!(quad.vertices.len(), 4);
    }

    /// Normais derivadas das faces apontam para fora e têm comprimento 1.
    #[test]
    fn computed_normals_face_outwards() {
        let mut quad = Mesh::textured_quad();
        for v in &mut quad.vertices {
            v.normal = [0.0, 0.0, 0.0];
        }
        quad.compute_vertex_normals();

        for v in &quad.vertices {
            assert!((v.normal[2] - 1.0).abs() < 1e-6, "normal: {:?}", v.normal);
        }
    }

    /// Arquivo inexistente vira erro com o caminho, não panic.
    #[test]
    fn missing_file_reports_the_path() {
        let err = Mesh::load_from_obj("assets/does-not-exist.obj").unwrap_err();
        assert!(err.to_string().contains("does-not-exist.obj"));
    }

    /// O `.obj` do repositório carrega e vem triangulado.
    #[test]
    fn bundled_cube_loads() {
        let cube = Mesh::load_from_obj("assets/cube.obj").expect("assets/cube.obj deve carregar");
        assert!(cube.triangle_count() >= 12);
        assert_eq!(cube.indices.len() % 3, 0);
        assert!(cube.has_texture_coords());
    }

    /// O raio mede o vértice mais distante da origem, não a caixa alinhada.
    #[test]
    fn bounding_radius_reaches_the_farthest_vertex() {
        let quad = Mesh::textured_quad();
        assert!((quad.bounding_radius() - 2.0_f64.sqrt()).abs() < 1e-9);
        assert_eq!(Mesh::default().bounding_radius(), 0.0);
    }
}

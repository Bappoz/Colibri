//! Imports a glTF scene's node tree into a [`Scene`].

use crate::assets::{Assets, Mesh, MeshHandle, Texture, TextureHandle};
use crate::ecs::Entity;
use crate::error::{Error, Result};
use crate::math::Vec3d;
use crate::scene::{MeshRenderer, Parent, Scene, Transform};

/// Loads `path` and spawns one entity per node of its default scene,
/// preserving the `Parent` relationships from the file.
///
/// Every spawned node gets a [`Transform`]. Mesh primitives keep separate
/// [`MeshRenderer`] entities so the importer can apply each primitive's
/// base-color factor and texture. Primitives without an explicit material use
/// `fallback_texture`.
///
/// The renderer is opaque: base-color alpha, metallic/roughness, and sampler
/// settings are not applied. Only UV set 0 is supported. Non-uniform node
/// scales are preserved and shaded with an inverse-transpose normal matrix.
///
/// # Errors
///
/// Fails on a malformed file, an empty default scene, an invalid rotation,
/// or a material that references a UV set other than 0.
pub fn spawn_gltf_scene(
    scene: &mut Scene,
    assets: &mut Assets,
    path: &str,
    fallback_texture: TextureHandle,
) -> Result<Vec<Entity>> {
    let (document, buffers, images) = gltf::import(path).map_err(|e| Error::GltfLoad {
        path: path.to_string(),
        reason: e.to_string(),
    })?;

    let root_nodes: Vec<_> = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .ok_or_else(|| Error::EmptyGltfScene {
            path: path.to_string(),
        })?
        .nodes()
        .collect();

    if root_nodes.is_empty() {
        return Err(Error::EmptyGltfScene {
            path: path.to_string(),
        });
    }

    let mut spawned = Vec::new();
    let mut image_textures = vec![None; images.len()];
    let white_texture = assets.add_texture(Texture::white());
    for node in root_nodes {
        spawn_node(
            scene,
            assets,
            path,
            &buffers,
            &images,
            &mut image_textures,
            fallback_texture,
            white_texture,
            &node,
            None,
            &mut spawned,
        )?;
    }
    Ok(spawned)
}

/// Spawns one node and recurses into its children, wiring each child's
/// [`Parent`] to the entity just spawned for `node`.
#[allow(clippy::too_many_arguments)] // one caller, each argument threads a
// distinct piece of state through the recursion; a struct would just be
// destructured back into these same names at every call site.
fn spawn_node(
    scene: &mut Scene,
    assets: &mut Assets,
    path: &str,
    buffers: &[gltf::buffer::Data],
    images: &[gltf::image::Data],
    image_textures: &mut [Option<TextureHandle>],
    fallback_texture: TextureHandle,
    white_texture: TextureHandle,
    node: &gltf::Node,
    parent: Option<Entity>,
    spawned: &mut Vec<Entity>,
) -> Result<()> {
    let transform = local_transform(path, node)?;
    let entity = scene.world.spawn();
    scene.world.insert(entity, transform);
    if let Some(parent) = parent {
        scene.world.insert(entity, Parent(parent));
    }

    spawn_mesh_primitives(
        scene,
        assets,
        path,
        buffers,
        images,
        image_textures,
        node,
        entity,
        fallback_texture,
        white_texture,
    )?;

    spawned.push(entity);

    for child in node.children() {
        spawn_node(
            scene,
            assets,
            path,
            buffers,
            images,
            image_textures,
            fallback_texture,
            white_texture,
            &child,
            Some(entity),
            spawned,
        )?;
    }
    Ok(())
}

/// Registers each supported primitive as a separate drawable entity.
///
/// The first drawable shares the node entity and its transform. Additional
/// drawables are identity-transform children, so they inherit the node's
/// world transform while remaining separate for per-primitive materials.
#[allow(clippy::too_many_arguments)]
fn spawn_mesh_primitives(
    scene: &mut Scene,
    assets: &mut Assets,
    path: &str,
    buffers: &[gltf::buffer::Data],
    images: &[gltf::image::Data],
    image_textures: &mut [Option<TextureHandle>],
    node: &gltf::Node,
    node_entity: Entity,
    fallback_texture: TextureHandle,
    white_texture: TextureHandle,
) -> Result<()> {
    let Some(mesh) = node.mesh() else {
        return Ok(());
    };

    let mut node_has_drawable = false;
    let get_buffer_data =
        |buffer: gltf::Buffer| buffers.get(buffer.index()).map(|d| d.0.as_slice());

    let mesh_index = mesh.index();
    for primitive in mesh.primitives() {
        let primitive_index = primitive.index();
        let imported_mesh = Mesh::from_gltf_primitive(&primitive, get_buffer_data.clone())
            .map_err(|reason| Error::GltfPrimitive {
                path: path.to_string(),
                node: node.name().unwrap_or("<unnamed>").to_string(),
                mesh_index,
                primitive_index,
                reason,
            })?;
        let mesh_handle: MeshHandle = assets.add_mesh(imported_mesh);
        let renderer = renderer_for_primitive(
            assets,
            path,
            node,
            images,
            image_textures,
            &primitive,
            mesh_handle,
            fallback_texture,
            white_texture,
        )?;

        if !node_has_drawable {
            scene.world.insert(node_entity, renderer);
            node_has_drawable = true;
            continue;
        }

        let primitive_entity = scene.world.spawn();
        scene.world.insert(primitive_entity, Transform::IDENTITY);
        scene.world.insert(primitive_entity, Parent(node_entity));
        scene.world.insert(primitive_entity, renderer);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn renderer_for_primitive(
    assets: &mut Assets,
    path: &str,
    node: &gltf::Node,
    images: &[gltf::image::Data],
    image_textures: &mut [Option<TextureHandle>],
    primitive: &gltf::Primitive,
    mesh: MeshHandle,
    fallback_texture: TextureHandle,
    white_texture: TextureHandle,
) -> Result<MeshRenderer> {
    let material = primitive.material();
    let Some(_material_index) = material.index() else {
        return Ok(MeshRenderer::new(mesh, fallback_texture));
    };

    let pbr = material.pbr_metallic_roughness();
    let factor = pbr.base_color_factor();
    let tint = color_factor_to_tint(factor);
    let texture = if let Some(info) = pbr.base_color_texture() {
        if info.tex_coord() != 0 {
            return Err(Error::UnsupportedGltfTexCoord {
                path: path.to_string(),
                node: node.name().unwrap_or("<unnamed>").to_string(),
                tex_coord: info.tex_coord(),
            });
        }

        let image_index = info.texture().source().index();
        let cached_texture =
            image_textures
                .get_mut(image_index)
                .ok_or_else(|| Error::GltfLoad {
                    path: path.to_string(),
                    reason: format!("material references missing image {image_index}"),
                })?;
        if let Some(texture) = *cached_texture {
            texture
        } else {
            let image = images.get(image_index).ok_or_else(|| Error::GltfLoad {
                path: path.to_string(),
                reason: format!("material references missing image data {image_index}"),
            })?;
            let texture = assets.add_texture(Texture::from_gltf_image(path, image)?);
            *cached_texture = Some(texture);
            texture
        }
    } else {
        white_texture
    };

    Ok(MeshRenderer::new(mesh, texture).with_tint(tint))
}

/// Packs the RGB components of a glTF base color factor into `0x00RRGGBB`.
fn color_factor_to_tint(factor: [f32; 4]) -> u32 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    (channel(factor[0]) << 16) | (channel(factor[1]) << 8) | channel(factor[2])
}

/// Converts one node's glTF transform into a [`Transform`].
fn local_transform(path: &str, node: &gltf::Node) -> Result<Transform> {
    let (translation, rotation, scale) = node.transform().decomposed();

    let rotation = quaternion_to_euler(rotation).ok_or_else(|| Error::InvalidNodeRotation {
        path: path.to_string(),
        node: node.name().unwrap_or("<unnamed>").to_string(),
    })?;

    Ok(Transform::from_translation(Vec3d::new(
        translation[0] as f64,
        translation[1] as f64,
        translation[2] as f64,
    ))
    .with_rotation(rotation)
    .with_scale(Vec3d::new(
        scale[0] as f64,
        scale[1] as f64,
        scale[2] as f64,
    )))
}

/// Converts a glTF quaternion to the engine's yaw (`Y`), pitch (`X`), roll
/// (`Z`) Euler convention, matching `Ry * Rx * Rz` in [`Transform::matrix`].
fn quaternion_to_euler(rotation: [f32; 4]) -> Option<Vec3d> {
    let [x, y, z, w] = rotation.map(f64::from);
    let length = (x * x + y * y + z * z + w * w).sqrt();
    if !length.is_finite() || length <= f64::EPSILON {
        return None;
    }

    let (x, y, z, w) = (x / length, y / length, z / length, w / length);
    let r11 = 1.0 - 2.0 * (y * y + z * z);
    let r12 = 2.0 * (x * y - z * w);
    let r13 = 2.0 * (x * z + y * w);
    let r21 = 2.0 * (x * y + z * w);
    let r22 = 1.0 - 2.0 * (x * x + z * z);
    let r23 = 2.0 * (y * z - x * w);
    let r33 = 1.0 - 2.0 * (x * x + y * y);

    let pitch = (-r23).clamp(-1.0, 1.0).asin();
    let (yaw, roll) = if pitch.cos().abs() > 1e-8 {
        (r13.atan2(r33), r21.atan2(r22))
    } else {
        // At gimbal lock yaw and roll are not unique. Fix roll to zero and
        // preserve the same combined orientation in yaw.
        ((pitch.sin() * r12).atan2(r11), 0.0)
    };

    Some(Vec3d::new(pitch, yaw, roll))
}

#[test]
fn spawn_gltf_scene_wires_parent_and_mesh() {
    let mut assets = Assets::new();
    let mut scene = Scene::new();
    let texture = assets.add_texture(crate::assets::Texture::white());

    let spawned = spawn_gltf_scene(
        &mut scene,
        &mut assets,
        "src/assets/tests/two_node_triangle.gltf",
        texture,
    )
    .unwrap();

    assert_eq!(spawned.len(), 2);
    assert_eq!(assets.mesh_count(), 1);
    let (root, child) = (spawned[0], spawned[1]);

    assert!(scene.world.contains::<MeshRenderer>(root));
    assert!(!scene.world.contains::<MeshRenderer>(child));
    assert_eq!(scene.world.get::<Parent>(child), Some(&Parent(root)));

    let child_pos = crate::scene::world_matrix(&scene.world, child).transform_point(Vec3d::ZERO);
    assert!((child_pos.x() - 2.0).abs() < 1e-6);
}

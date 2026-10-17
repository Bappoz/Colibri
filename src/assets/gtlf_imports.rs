//! Imports a glTF scene's node tree into a [`Scene`], one entity per node.

use crate::assets::{Assets, Mesh, MeshHandle, TextureHandle};
use crate::ecs::Entity;
use crate::error::{Error, Result};
use crate::math::Vec3d;
use crate::scene::{MeshRenderer, Parent, Scene, Transform};

/// Loads `path` and spawns one entity per node of its default scene,
/// preserving the `Parent` relationships from the file.
///
/// Every spawned entity gets a [`Transform`]; a node with a [`Mesh`] also
/// gets a [`MeshRenderer`] sampling `fallback_texture` (glTF materials are not
/// imported yet — every node shares one texture until that lands).
///
/// # Errors
///
/// Fails on a malformed file, an empty default scene, or a node whose
/// rotation or non-uniform scale `Transform` cannot represent (see the
/// module docs on [`crate::scene`] for why).
pub fn spawn_gltf_scene(
    scene: &mut Scene,
    assets: &mut Assets,
    path: &str,
    fallback_texture: TextureHandle,
) -> Result<Vec<Entity>> {
    let (document, buffers, _images) = gltf::import(path).map_err(|e| Error::GltfLoad {
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
    for node in root_nodes {
        spawn_node(
            scene,
            assets,
            path,
            &buffers,
            fallback_texture,
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
    fallback_texture: TextureHandle,
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

    spawn_mesh_primitives(scene, assets, buffers, node, entity, fallback_texture);

    spawned.push(entity);

    for child in node.children() {
        spawn_node(
            scene,
            assets,
            path,
            buffers,
            fallback_texture,
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
fn spawn_mesh_primitives(
    scene: &mut Scene,
    assets: &mut Assets,
    buffers: &[gltf::buffer::Data],
    node: &gltf::Node,
    node_entity: Entity,
    fallback_texture: TextureHandle,
) {
    let Some(mesh) = node.mesh() else {
        return;
    };

    let mut node_has_drawable = false;
    let get_buffer_data =
        |buffer: gltf::Buffer| buffers.get(buffer.index()).map(|d| d.0.as_slice());

    for primitive in mesh.primitives() {
        let Some(mesh) = Mesh::from_gltf_primitive(&primitive, get_buffer_data.clone()) else {
            continue;
        };
        let mesh_handle: MeshHandle = assets.add_mesh(mesh);
        let renderer = MeshRenderer::new(mesh_handle, fallback_texture);

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
}

/// Converts one node's glTF transform into a [`Transform`], rejecting
/// anything the engine cannot represent yet.
fn local_transform(path: &str, node: &gltf::Node) -> Result<Transform> {
    let (translation, rotation, scale) = node.transform().decomposed();

    const IDENTITY_ROTATION: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    const TOL: f32 = 1e-5;
    let is_identity_rotation = rotation
        .iter()
        .zip(IDENTITY_ROTATION)
        .all(|(a, b)| (a - b).abs() < TOL);
    let is_uniform_scale = (scale[0] - scale[1]).abs() < TOL && (scale[1] - scale[2]).abs() < TOL;

    if !is_identity_rotation || !is_uniform_scale {
        return Err(Error::UnsupportedNodeTransform {
            path: path.to_string(),
            node: node.name().unwrap_or("<unnamed>").to_string(),
        });
    }

    Ok(Transform::from_translation(Vec3d::new(
        translation[0] as f64,
        translation[1] as f64,
        translation[2] as f64,
    ))
    .with_uniform_scale(scale[0] as f64))
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

//! Parent/child relationships between entities, and the world-space matrix
//! that folds a whole ancestor chain into one.
//!
//! There is no `GlobalTransform` component here on purpose: [`world_matrix`]
//! computes the world-space matrix on demand instead of caching it, so it is
//! never one frame stale and never depends on a system having run first —
//! [`crate::render::Renderer::render`] can call it directly, with or without
//! [`Scene::update`](crate::scene::Scene::update) ever having run.

use crate::ecs::{Entity, World};
use crate::math::Mat4x4;
use crate::scene::Transform;

/// Which entity, if any, this one's [`Transform`] is relative to.
///
/// No `Parent` means the transform is already in world space — the entity is
/// a root. Like every component, `Parent` needs no registration; the world
/// creates its column on first insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parent(pub Entity);

/// How many ancestors [`world_matrix`] climbs before giving up.
///
/// Protects against a `Parent` cycle (A parent of B parent of A), which would
/// otherwise walk forever. 64 is far beyond any hierarchy this engine expects.
const MAX_HIERARCHY_DEPTH: usize = 64;

/// The world-space matrix of `entity`: its own [`Transform`], folded into
/// every ancestor's, root first.
///
/// An entity with no [`Parent`] gets exactly `Transform::matrix()` back — the
/// hierarchy is invisible to a scene that never uses it. An entity with no
/// `Transform` at all gets the identity matrix.
pub fn world_matrix(world: &World, entity: Entity) -> Mat4x4 {
    // Walk up toward the root, collecting each ancestor's local matrix.
    let mut chain = Vec::new();
    let mut current = Some(entity);
    for _ in 0..MAX_HIERARCHY_DEPTH {
        let Some(node) = current else { break };
        let Some(transform) = world.get::<Transform>(node) else {
            break;
        };
        chain.push(transform.matrix());
        current = world.get::<Parent>(node).map(|parent| parent.0);
    }

    // Multiply root-to-leaf: `chain` was collected leaf-to-root, so reverse it
    // first. `fold` starting from the identity handles the "no ancestors"
    // case (a root) for free — the loop body just never runs twice.
    chain
        .into_iter()
        .rev()
        .fold(Mat4x4::identity(), |world_so_far, local| {
            world_so_far * local
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{Assets, Mesh, Texture};
    use crate::math::Vec3d;
    use crate::scene::{MeshRenderer, Scene};

    /// A `MeshRenderer` good enough for tests that only care about position —
    /// handles have no public constructor beyond `Assets::add_mesh`/
    /// `add_texture`.
    fn dummy_renderer(assets: &mut Assets) -> MeshRenderer {
        let mesh = assets.add_mesh(Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());
        MeshRenderer::new(mesh, texture)
    }

    /// With no `Parent`, the global matrix is exactly the local one.
    #[test]
    fn world_matrix_of_a_root_is_its_own_local_matrix() {
        let mut assets = Assets::new();
        let mut scene = Scene::new();
        let transform = Transform::from_translation(Vec3d::new(1.0, 2.0, 3.0));
        let entity = scene.spawn_object(transform, dummy_renderer(&mut assets));

        let world = world_matrix(&scene.world, entity);

        assert_eq!(world, transform.matrix());
    }

    /// A child folds the parent's matrix into its own — sun and planet.
    #[test]
    fn world_matrix_of_a_child_folds_in_the_parent() {
        let mut assets = Assets::new();
        let mut scene = Scene::new();
        let sun = scene.spawn_object(
            Transform::from_translation(Vec3d::new(10.0, 0.0, 0.0)),
            dummy_renderer(&mut assets),
        );
        let planet = scene.spawn_object(
            Transform::from_translation(Vec3d::new(5.0, 0.0, 0.0)),
            dummy_renderer(&mut assets),
        );
        scene.world.insert(planet, Parent(sun));

        let world = world_matrix(&scene.world, planet);

        // The planet orbits 5 units from the sun, which sits at 10 — 15 total.
        let position = world.transform_point(Vec3d::ZERO);
        assert!((position.x() - 15.0).abs() < 1e-9);
    }

    /// Three generations: sun, planet, moon — the scene graph's "aha" case.
    #[test]
    fn world_matrix_of_a_grandchild_folds_in_every_ancestor() {
        let mut assets = Assets::new();
        let mut scene = Scene::new();
        let sun = scene.spawn_object(
            Transform::from_translation(Vec3d::new(10.0, 0.0, 0.0)),
            dummy_renderer(&mut assets),
        );
        let planet = scene.spawn_object(
            Transform::from_translation(Vec3d::new(5.0, 0.0, 0.0)),
            dummy_renderer(&mut assets),
        );
        scene.world.insert(planet, Parent(sun));
        let moon = scene.spawn_object(
            Transform::from_translation(Vec3d::new(1.0, 0.0, 0.0)),
            dummy_renderer(&mut assets),
        );
        scene.world.insert(moon, Parent(planet));

        let world = world_matrix(&scene.world, moon);
        let position = world.transform_point(Vec3d::ZERO);

        // 10 (sun) + 5 (planet) + 1 (moon) = 16.
        assert!((position.x() - 16.0).abs() < 1e-9);
    }

    /// Moving the parent moves the child along — the whole point of the
    /// hierarchy.
    #[test]
    fn moving_the_parent_moves_the_child_along() {
        let mut assets = Assets::new();
        let mut scene = Scene::new();
        let sun = scene.spawn_object(Transform::IDENTITY, dummy_renderer(&mut assets));
        let moon = scene.spawn_object(
            Transform::from_translation(Vec3d::new(2.0, 0.0, 0.0)),
            dummy_renderer(&mut assets),
        );
        scene.world.insert(moon, Parent(sun));

        scene.world.get_mut::<Transform>(sun).unwrap().translation = Vec3d::new(100.0, 0.0, 0.0);

        let position = world_matrix(&scene.world, moon).transform_point(Vec3d::ZERO);
        assert!((position.x() - 102.0).abs() < 1e-9);
    }

    /// A `Parent` cycle (A parent of B, B parent of A) must not hang.
    #[test]
    fn a_parent_cycle_does_not_hang() {
        let mut assets = Assets::new();
        let mut scene = Scene::new();
        let a = scene.spawn_object(Transform::IDENTITY, dummy_renderer(&mut assets));
        let b = scene.spawn_object(Transform::IDENTITY, dummy_renderer(&mut assets));
        scene.world.insert(a, Parent(b));
        scene.world.insert(b, Parent(a));

        // Just has to return — if it hung, the test would never finish.
        let _ = world_matrix(&scene.world, a);
    }

    /// With no `Transform` at all, the matrix is the identity — the scene's
    /// own camera is a ready-made example, since it never carries one.
    #[test]
    fn an_entity_without_a_transform_gets_the_identity() {
        let scene = Scene::new();
        let camera_entity = scene.world.entities().next().unwrap();

        assert_eq!(
            world_matrix(&scene.world, camera_entity),
            Mat4x4::identity()
        );
    }
}

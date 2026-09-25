//! Angular velocity, and the system that integrates it.

use crate::ecs::World;
use crate::math::Vec3d;
use crate::scene::Transform;

/// Euler angles added to the entity's rotation every second, in radians.
///
/// The component that proves the split: an entity can spin without being
/// drawable, and be drawable without spinning. [`spin_system`] consumes it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Spin(pub Vec3d);

/// Integrates every [`Spin`] into its entity's [`Transform`].
///
/// Reads `Spin`, writes `Transform`: exactly the shape of
/// [`World::query2_mut`]. Entities with a `Spin` and no `Transform` simply
/// never match.
pub fn spin_system(world: &mut World, dt: f64) {
    for (_, spin, transform) in world.query2_mut::<Spin, Transform>() {
        transform.rotation += spin.0 * dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{Assets, Mesh, Texture};
    use crate::ecs::Entity;
    use crate::scene::{MeshRenderer, Scene};

    /// A minimal scene with one drawable entity, so tests don't repeat the
    /// asset setup.
    fn scene_with_one_object() -> (Scene, Assets, Entity) {
        let mut assets = Assets::new();
        let mesh = assets.add_mesh(Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());

        let mut scene = Scene::new();
        let entity = scene.spawn_object(Transform::IDENTITY, MeshRenderer::new(mesh, texture));
        (scene, assets, entity)
    }

    /// `update` integrates the angular velocity with the frame's `dt`.
    #[test]
    fn update_integrates_spin() {
        let (mut scene, _assets, entity) = scene_with_one_object();
        scene.world.insert(entity, Spin(Vec3d::new(0.0, 2.0, 0.0)));

        scene.update(0.5);

        let rotation = scene.world.get::<Transform>(entity).unwrap().rotation;
        assert!((rotation.y() - 1.0).abs() < 1e-12);
    }

    /// Without a `Spin`, the transform is left alone — the system does not
    /// run on entities that never asked for it.
    #[test]
    fn update_leaves_entities_without_spin_alone() {
        let (mut scene, _assets, entity) = scene_with_one_object();

        scene.update(1.0);

        let transform = scene.world.get::<Transform>(entity).unwrap();
        assert_eq!(*transform, Transform::IDENTITY);
    }

    /// A `Spin` without a `Transform` cannot bring the system down.
    #[test]
    fn spin_without_a_transform_is_ignored() {
        let mut scene = Scene::new();
        let entity = scene.world.spawn();
        scene.world.insert(entity, Spin(Vec3d::new(1.0, 0.0, 0.0)));

        scene.update(1.0);

        assert!(scene.world.get::<Transform>(entity).is_none());
    }
}

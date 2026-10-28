//! The [`Scene`]: a [`World`] plus the resources every frame needs — the
//! camera and the light — and the systems that advance it.

use crate::assets::Assets;
use crate::ecs::{Entity, Schedule, World};
use crate::math::Vec3d;

use super::camera::Camera;
use super::hierarchy::world_matrix;
use super::light::DirectionalLight;
use super::mesh_renderer::MeshRenderer;
use super::spin::spin_system;
use super::transform::Transform;

/// A camera, a light and a world of entities.
///
/// The camera lives inside `world`, as the sole entity carrying a [`Camera`]
/// component — not as a special field outside it. [`Scene::camera`]/
/// [`Scene::camera_mut`] fetch it through [`World::single`].
pub struct Scene {
    /// The entities and their components — including the camera.
    pub world: World,
    /// The single directional light lighting every object.
    pub light: DirectionalLight,
    /// Systems run, in order, by [`Scene::update`].
    schedule: Schedule,
}

impl Default for Scene {
    fn default() -> Self {
        let mut world = World::default();
        let camera_entity = world.spawn();
        world.insert(camera_entity, Camera::default());

        let mut schedule = Schedule::new();
        schedule.add(spin_system);

        Self {
            world,
            light: DirectionalLight::default(),
            schedule,
        }
    }
}

impl Scene {
    /// An empty scene with a default camera and light.
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawns a drawable entity: a [`Transform`] plus a [`MeshRenderer`].
    ///
    /// Sugar for the common case. An entity that needs anything else — a
    /// [`Spin`](crate::scene::Spin), or a transform with no mesh — is built
    /// component by component through [`Scene::world`].
    pub fn spawn_object(&mut self, transform: Transform, renderer: MeshRenderer) -> Entity {
        let entity = self.world.spawn();
        self.world.insert(entity, transform);
        self.world.insert(entity, renderer);
        entity
    }

    /// The scene's single camera.
    pub fn camera(&self) -> &Camera {
        self.world.single::<Camera>()
    }

    /// Mutable access to the scene's single camera.
    pub fn camera_mut(&mut self) -> &mut Camera {
        self.world.single_mut::<Camera>()
    }

    /// Iterates the drawable entities: those carrying both a [`Transform`] and
    /// a [`MeshRenderer`]. This is the renderer's input.
    ///
    /// The walk goes over the `MeshRenderer` column and looks the transform up
    /// per entity, and not the other way round: everything drawable has a
    /// renderer, while transforms also belong to entities that are never
    /// drawn (and, as of this scene shape, to the camera too).
    pub fn drawables(&self) -> impl Iterator<Item = (&Transform, &MeshRenderer)> + '_ {
        self.world
            .query2::<MeshRenderer, Transform>()
            .map(|(_, renderer, transform)| (transform, renderer))
    }

    /// Number of drawable entities.
    pub fn drawable_count(&self) -> usize {
        self.drawables().count()
    }

    /// Computes a conservative world-space bounding sphere for every drawable
    /// mesh, including all vertices and their parent transforms.
    ///
    /// The sphere is centered on the combined world-space AABB. Non-finite
    /// vertices are ignored; `None` means no finite drawable vertices exist.
    /// A zero-radius result is valid but cannot be used to frame a camera.
    pub fn bounding_sphere(&self, assets: &Assets) -> Option<(Vec3d, f64)> {
        let mut min = Vec3d::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max = Vec3d::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut found_vertex = false;

        for (entity, renderer, _) in self.world.query2::<MeshRenderer, Transform>() {
            let model = world_matrix(&self.world, entity);
            for vertex in &assets.mesh(renderer.mesh).vertices {
                let position = model
                    .transform_point(Vec3d::new(
                        vertex.position[0] as f64,
                        vertex.position[1] as f64,
                        vertex.position[2] as f64,
                    ))
                    .xyz();
                if !position.x().is_finite()
                    || !position.y().is_finite()
                    || !position.z().is_finite()
                {
                    continue;
                }

                min = Vec3d::new(
                    min.x().min(position.x()),
                    min.y().min(position.y()),
                    min.z().min(position.z()),
                );
                max = Vec3d::new(
                    max.x().max(position.x()),
                    max.y().max(position.y()),
                    max.z().max(position.z()),
                );
                found_vertex = true;
            }
        }

        if !found_vertex {
            return None;
        }

        let center = (min + max) * 0.5;
        let radius = ((max - min) * 0.5).length();
        (radius.is_finite()).then_some((center, radius))
    }

    /// Number of live entities, drawable or not.
    pub fn len(&self) -> usize {
        self.world.len()
    }

    /// Whether the scene holds no entity at all.
    pub fn is_empty(&self) -> bool {
        self.world.is_empty()
    }

    /// Advances the scene by `dt` seconds, running every scheduled system.
    pub fn update(&mut self, dt: f64) {
        self.schedule.run(&mut self.world, dt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{Assets, Mesh, Texture};
    use crate::math::Vec3d;
    use crate::scene::Spin;

    /// A minimal scene with one asset of each, so tests don't repeat setup.
    fn scene_with_one_object() -> (Scene, Assets, Entity) {
        let mut assets = Assets::new();
        let mesh = assets.add_mesh(Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());

        let mut scene = Scene::new();
        let entity = scene.spawn_object(Transform::IDENTITY, MeshRenderer::new(mesh, texture));
        (scene, assets, entity)
    }

    /// `spawn_object` attaches the two components that make the entity
    /// visible.
    #[test]
    fn spawn_object_attaches_transform_and_renderer() {
        let (scene, _assets, entity) = scene_with_one_object();

        // 2: the freshly spawned object, plus the camera the scene is born
        // with.
        assert_eq!(scene.len(), 2);
        assert!(scene.world.contains::<Transform>(entity));
        assert!(scene.world.contains::<MeshRenderer>(entity));
        assert_eq!(scene.drawable_count(), 1);
    }

    /// After despawn, the old handle returns nothing anymore.
    #[test]
    fn despawn_invalidates_the_handle() {
        let (mut scene, _assets, entity) = scene_with_one_object();

        assert!(scene.world.despawn(entity));

        assert!(scene.world.get::<Transform>(entity).is_none());
        assert!(scene.world.get::<MeshRenderer>(entity).is_none());
        // Doesn't go empty: the camera is still alive.
        assert_eq!(scene.len(), 1);
        assert_eq!(scene.drawable_count(), 0);
        assert!(!scene.world.despawn(entity), "double despawn is a no-op");
    }

    /// Recycling the slot must not let the old handle read the new object.
    #[test]
    fn a_recycled_slot_does_not_leak_through_the_old_handle() {
        let (mut scene, mut assets, old) = scene_with_one_object();
        scene.world.despawn(old);

        let mesh = assets.add_mesh(Mesh::default());
        let texture = assets.add_texture(Texture::white());
        let new = scene.spawn_object(Transform::IDENTITY, MeshRenderer::new(mesh, texture));

        assert_eq!(new.index(), old.index(), "the slot should be reused");
        assert!(scene.world.get::<MeshRenderer>(old).is_none());
        assert!(scene.world.get::<MeshRenderer>(new).is_some());
        assert_eq!(scene.drawable_count(), 1);
    }

    /// What a single bundle couldn't express: an entity with a transform and
    /// no mesh isn't drawable, but it exists and can still spin.
    #[test]
    fn an_entity_without_a_mesh_is_not_drawable() {
        let (mut scene, _assets, _drawable) = scene_with_one_object();

        let marker = scene.world.spawn();
        scene.world.insert(marker, Transform::IDENTITY);
        scene.world.insert(marker, Spin(Vec3d::new(0.0, 1.0, 0.0)));

        // 3: the object, the marker, and the camera the scene is born with.
        assert_eq!(scene.len(), 3, "all three entities are alive");
        assert_eq!(scene.drawable_count(), 1, "only one has a mesh");

        scene.update(1.0);
        let rotation = scene.world.get::<Transform>(marker).unwrap().rotation;
        assert!((rotation.y() - 1.0).abs() < 1e-12, "and it still spins");
    }

    /// And the inverse: a mesh without a transform has nowhere to be drawn.
    #[test]
    fn a_mesh_without_a_transform_is_skipped() {
        let (mut scene, mut assets, _drawable) = scene_with_one_object();
        let mesh = assets.add_mesh(Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());

        let orphan = scene.world.spawn();
        scene.world.insert(orphan, MeshRenderer::new(mesh, texture));

        assert_eq!(scene.world.count::<MeshRenderer>(), 2);
        assert_eq!(scene.drawable_count(), 1, "the orphan is left out");
    }

    /// The camera is a real entity: it exists from the moment the scene is
    /// born.
    #[test]
    fn the_scene_is_born_with_exactly_one_camera() {
        let scene = Scene::new();
        assert_eq!(scene.camera().position, Vec3d::ZERO);
    }
}

//! What there is to draw: entities with a transform, a mesh and a material,
//! plus the camera and light that observe them.
//!
//! [`Scene`] is a deliberately plain, single-archetype component storage: one
//! `Option<RenderObject>` slot per entity index. It is the stand-in for the
//! real ECS storage of the next roadmap stage, and it already buys the thing
//! that matters — the renderer iterates entities, and nothing in the engine
//! owns a hard-coded mesh any more.

pub mod camera;
pub mod light;
pub mod transform;

pub use camera::Camera;
pub use light::DirectionalLight;
pub use transform::Transform;

use crate::assets::{MeshHandle, TextureHandle};
use crate::ecs::{Entity, EntityAllocator};
use crate::math::Vec3d;

/// Tint that leaves the sampled texture untouched (white, fully modulated).
pub const NO_TINT: u32 = 0x00FF_FFFF;

/// Everything the renderer needs to know about one drawable entity.
///
/// This is the bundle that the ECS will later split into independent
/// components (`Transform`, `MeshRenderer`, `Spin`); keeping it as one struct
/// until the storage exists avoids inventing an abstraction twice.
#[derive(Debug, Clone, Copy)]
pub struct RenderObject {
    /// Where the object sits in world space.
    pub transform: Transform,
    /// Geometry to draw.
    pub mesh: MeshHandle,
    /// Texture sampled across that geometry.
    pub texture: TextureHandle,
    /// Per-object color multiplied into the sampled texel, `0x00RRGGBB`.
    /// [`NO_TINT`] leaves the texture as it is.
    pub tint: u32,
    /// Euler angles added to the rotation every second, in radians.
    ///
    /// A placeholder for a proper `Velocity` component and movement system:
    /// [`Scene::update`] is the one-line system that consumes it today.
    pub angular_velocity: Vec3d,
}

impl RenderObject {
    /// A static, untinted object at the given transform.
    pub fn new(transform: Transform, mesh: MeshHandle, texture: TextureHandle) -> Self {
        Self {
            transform,
            mesh,
            texture,
            tint: NO_TINT,
            angular_velocity: Vec3d::ZERO,
        }
    }

    /// Builder-style tint.
    pub fn with_tint(mut self, tint: u32) -> Self {
        self.tint = tint;
        self
    }

    /// Builder-style spin, in radians per second.
    pub fn with_angular_velocity(mut self, angular_velocity: Vec3d) -> Self {
        self.angular_velocity = angular_velocity;
        self
    }
}

/// A camera, a light and a set of drawable entities.
#[derive(Default)]
pub struct Scene {
    /// Mints and recycles the entity ids.
    entities: EntityAllocator,
    /// Components indexed by [`Entity::index`]; `None` marks a free slot.
    objects: Vec<Option<RenderObject>>,
    /// Point of view used to render the scene.
    pub camera: Camera,
    /// The single directional light lighting every object.
    pub light: DirectionalLight,
}

impl Scene {
    /// An empty scene with a default camera and light.
    pub fn new() -> Self {
        Self {
            entities: EntityAllocator::new(),
            objects: Vec::new(),
            camera: Camera::default(),
            light: DirectionalLight::default(),
        }
    }

    /// Adds an object to the scene and returns the entity that owns it.
    pub fn spawn(&mut self, object: RenderObject) -> Entity {
        let entity = self.entities.spawn();
        let slot = entity.index() as usize;
        if slot >= self.objects.len() {
            self.objects.resize(slot + 1, None);
        }
        self.objects[slot] = Some(object);
        entity
    }

    /// Removes an entity and its object.
    ///
    /// Returns `false` for a stale handle, leaving the scene untouched.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.entities.despawn(entity) {
            return false;
        }
        self.objects[entity.index() as usize] = None;
        true
    }

    /// Borrows an entity's object, or `None` if the handle is stale.
    pub fn get(&self, entity: Entity) -> Option<&RenderObject> {
        self.entities
            .is_alive(entity)
            .then(|| self.objects[entity.index() as usize].as_ref())
            .flatten()
    }

    /// Mutably borrows an entity's object, or `None` if the handle is stale.
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut RenderObject> {
        if !self.entities.is_alive(entity) {
            return None;
        }
        self.objects[entity.index() as usize].as_mut()
    }

    /// Iterates over the live objects in slot order — the renderer's input.
    pub fn objects(&self) -> impl Iterator<Item = &RenderObject> + '_ {
        self.objects.iter().flatten()
    }

    /// Iterates over the live objects along with their entity.
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &RenderObject)> + '_ {
        self.entities
            .iter()
            .filter_map(|e| self.objects[e.index() as usize].as_ref().map(|o| (e, o)))
    }

    /// Number of live objects.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Whether the scene has nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Advances the scene by `dt` seconds.
    ///
    /// The whole simulation, for now: integrate the per-object angular
    /// velocity. This is where the systems of the ECS stage will plug in.
    pub fn update(&mut self, dt: f64) {
        for object in self.objects.iter_mut().flatten() {
            object.transform.rotation += object.angular_velocity * dt;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{Assets, Mesh, Texture};

    /// Cena mínima com um asset de cada, para os testes não repetirem setup.
    fn scene_with_one_object() -> (Scene, Assets, Entity) {
        let mut assets = Assets::new();
        let mesh = assets.add_mesh(Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());

        let mut scene = Scene::new();
        let entity = scene.spawn(RenderObject::new(Transform::IDENTITY, mesh, texture));
        (scene, assets, entity)
    }

    /// Spawn devolve um handle que enxerga o objeto recém-criado.
    #[test]
    fn spawn_registers_the_object() {
        let (scene, _assets, entity) = scene_with_one_object();
        assert_eq!(scene.len(), 1);
        assert!(scene.get(entity).is_some());
    }

    /// Depois do despawn o handle antigo não devolve mais nada.
    #[test]
    fn despawn_invalidates_the_handle() {
        let (mut scene, _assets, entity) = scene_with_one_object();
        assert!(scene.despawn(entity));

        assert!(scene.get(entity).is_none());
        assert!(scene.is_empty());
        assert_eq!(scene.objects().count(), 0);
        assert!(!scene.despawn(entity), "despawn duplo é no-op");
    }

    /// Reciclar o slot não pode fazer o handle velho ler o objeto novo.
    #[test]
    fn a_recycled_slot_does_not_leak_through_the_old_handle() {
        let (mut scene, mut assets, old) = scene_with_one_object();
        scene.despawn(old);

        let mesh = assets.add_mesh(Mesh::default());
        let texture = assets.add_texture(Texture::white());
        let new = scene.spawn(RenderObject::new(Transform::IDENTITY, mesh, texture));

        assert_eq!(new.index(), old.index());
        assert!(scene.get(old).is_none());
        assert!(scene.get(new).is_some());
    }

    /// `update` integra a velocidade angular com o dt do frame.
    #[test]
    fn update_integrates_angular_velocity() {
        let (mut scene, _assets, entity) = scene_with_one_object();
        scene.get_mut(entity).unwrap().angular_velocity = Vec3d::new(0.0, 2.0, 0.0);

        scene.update(0.5);

        let rotation = scene.get(entity).unwrap().transform.rotation;
        assert!((rotation.y() - 1.0).abs() < 1e-12);
    }
}

//! What there is to draw: entities carrying a transform and a mesh, plus the
//! camera and light that observe them.
//!
//! A [`Scene`] is a [`World`] — the entities and their component columns —
//! next to the handful of things that are *not* per entity: the camera and the
//! light. Those are resources, and they stay plain fields until a scene needs
//! more than one of each.
//!
//! # Components
//!
//! | Component | Meaning |
//! |---|---|
//! | [`Transform`] | Where the entity is in world space |
//! | [`MeshRenderer`] | What geometry to draw there, and how to shade it |
//! | [`Spin`] | Radians per second added to the rotation |
//!
//! They are independent on purpose: the renderer draws whoever has both a
//! transform and a mesh, [`spin_system`] turns whoever has both a transform
//! and a spin, and an entity carrying only one of them is perfectly valid —
//! which is exactly what the single bundled `RenderObject` could not express.

pub mod camera;
pub mod light;
pub mod transform;

pub use camera::Camera;
pub use light::DirectionalLight;
pub use transform::Transform;

use crate::assets::{MeshHandle, TextureHandle};
use crate::ecs::{Entity, World};
use crate::math::Vec3d;

/// Tint that leaves the sampled texture untouched (white, fully modulated).
pub const NO_TINT: u32 = 0x00FF_FFFF;

/// Geometry and how to shade it.
///
/// Attach it next to a [`Transform`] to make an entity drawable; a
/// `MeshRenderer` with no transform is skipped, since there is nowhere to put
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshRenderer {
    /// Geometry to draw.
    pub mesh: MeshHandle,
    /// Texture sampled across that geometry.
    pub texture: TextureHandle,
    /// Color multiplied into the sampled texel, `0x00RRGGBB`. [`NO_TINT`]
    /// leaves the texture as it is.
    pub tint: u32,
}

impl MeshRenderer {
    /// An untinted renderer for `mesh`, sampling `texture`.
    pub const fn new(mesh: MeshHandle, texture: TextureHandle) -> Self {
        Self {
            mesh,
            texture,
            tint: NO_TINT,
        }
    }

    /// Builder-style tint.
    pub const fn with_tint(mut self, tint: u32) -> Self {
        self.tint = tint;
        self
    }
}

/// Euler angles added to the entity's rotation every second, in radians.
///
/// The component that proves the split: an entity can spin without being
/// drawable, and be drawable without spinning. [`spin_system`] consumes it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Spin(pub Vec3d);

/// A camera, a light and a world of entities.
#[derive(Default)]
pub struct Scene {
    /// The entities and their components.
    pub world: World,
    /// Point of view used to render the scene.
    pub camera: Camera,
    /// The single directional light lighting every object.
    pub light: DirectionalLight,
}

impl Scene {
    /// An empty scene with a default camera and light.
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawns a drawable entity: a [`Transform`] plus a [`MeshRenderer`].
    ///
    /// Sugar for the common case. An entity that needs anything else — a
    /// [`Spin`], or a transform with no mesh — is built component by component
    /// through [`Scene::world`].
    pub fn spawn_object(&mut self, transform: Transform, renderer: MeshRenderer) -> Entity {
        let entity = self.world.spawn();
        self.world.insert(entity, transform);
        self.world.insert(entity, renderer);
        entity
    }

    /// Iterates the drawable entities: those carrying both a [`Transform`] and
    /// a [`MeshRenderer`]. This is the renderer's input.
    ///
    /// The walk goes over the `MeshRenderer` column and looks the transform up
    /// per entity, and not the other way round: everything drawable has a
    /// renderer, while transforms also belong to entities that are never drawn.
    pub fn drawables(&self) -> impl Iterator<Item = (&Transform, &MeshRenderer)> + '_ {
        self.world
            .iter::<MeshRenderer>()
            .filter_map(|(entity, renderer)| Some((self.world.get::<Transform>(entity)?, renderer)))
    }

    /// Number of drawable entities.
    pub fn drawable_count(&self) -> usize {
        self.drawables().count()
    }

    /// Number of live entities, drawable or not.
    pub fn len(&self) -> usize {
        self.world.len()
    }

    /// Whether the scene holds no entity at all.
    pub fn is_empty(&self) -> bool {
        self.world.is_empty()
    }

    /// Advances the scene by `dt` seconds.
    ///
    /// The whole simulation, for now: one system. When there is a second one,
    /// this is where the scheduler of the next stage takes over.
    pub fn update(&mut self, dt: f64) {
        spin_system(&mut self.world, dt);
    }
}

/// Integrates every [`Spin`] into its entity's [`Transform`].
///
/// The two-column shape the borrow checker cannot see through yet: iterating
/// `Spin` borrows the world immutably while `get_mut::<Transform>` wants it
/// mutably, even though the two columns are provably disjoint. Collecting the
/// pairs first is the honest workaround until the typed queries of the next
/// stage; `Spin` is one `Vec3d`, so the copy is cheap.
pub fn spin_system(world: &mut World, dt: f64) {
    let spins: Vec<(Entity, Spin)> = world.iter::<Spin>().map(|(e, spin)| (e, *spin)).collect();

    for (entity, spin) in spins {
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            transform.rotation += spin.0 * dt;
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
        let entity = scene.spawn_object(Transform::IDENTITY, MeshRenderer::new(mesh, texture));
        (scene, assets, entity)
    }

    /// `spawn_object` anexa os dois componentes que tornam a entidade visível.
    #[test]
    fn spawn_object_attaches_transform_and_renderer() {
        let (scene, _assets, entity) = scene_with_one_object();

        assert_eq!(scene.len(), 1);
        assert!(scene.world.contains::<Transform>(entity));
        assert!(scene.world.contains::<MeshRenderer>(entity));
        assert_eq!(scene.drawable_count(), 1);
    }

    /// Depois do despawn o handle antigo não devolve mais nada.
    #[test]
    fn despawn_invalidates_the_handle() {
        let (mut scene, _assets, entity) = scene_with_one_object();

        assert!(scene.world.despawn(entity));

        assert!(scene.world.get::<Transform>(entity).is_none());
        assert!(scene.world.get::<MeshRenderer>(entity).is_none());
        assert!(scene.is_empty());
        assert_eq!(scene.drawable_count(), 0);
        assert!(!scene.world.despawn(entity), "despawn duplo é no-op");
    }

    /// Reciclar o slot não pode fazer o handle velho ler o objeto novo.
    #[test]
    fn a_recycled_slot_does_not_leak_through_the_old_handle() {
        let (mut scene, mut assets, old) = scene_with_one_object();
        scene.world.despawn(old);

        let mesh = assets.add_mesh(Mesh::default());
        let texture = assets.add_texture(Texture::white());
        let new = scene.spawn_object(Transform::IDENTITY, MeshRenderer::new(mesh, texture));

        assert_eq!(new.index(), old.index(), "o slot deve ser reaproveitado");
        assert!(scene.world.get::<MeshRenderer>(old).is_none());
        assert!(scene.world.get::<MeshRenderer>(new).is_some());
        assert_eq!(scene.drawable_count(), 1);
    }

    /// O que o bundle único não conseguia expressar: uma entidade com
    /// transform e sem malha não é desenhável, mas existe e pode girar.
    #[test]
    fn an_entity_without_a_mesh_is_not_drawable() {
        let (mut scene, _assets, _drawable) = scene_with_one_object();

        let marker = scene.world.spawn();
        scene.world.insert(marker, Transform::IDENTITY);
        scene.world.insert(marker, Spin(Vec3d::new(0.0, 1.0, 0.0)));

        assert_eq!(scene.len(), 2, "as duas entidades estão vivas");
        assert_eq!(scene.drawable_count(), 1, "só uma tem malha");

        scene.update(1.0);
        let rotation = scene.world.get::<Transform>(marker).unwrap().rotation;
        assert!((rotation.y() - 1.0).abs() < 1e-12, "e ainda assim gira");
    }

    /// E o inverso: malha sem transform não tem onde ser desenhada.
    #[test]
    fn a_mesh_without_a_transform_is_skipped() {
        let (mut scene, mut assets, _drawable) = scene_with_one_object();
        let mesh = assets.add_mesh(Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());

        let orphan = scene.world.spawn();
        scene.world.insert(orphan, MeshRenderer::new(mesh, texture));

        assert_eq!(scene.world.count::<MeshRenderer>(), 2);
        assert_eq!(scene.drawable_count(), 1, "o órfão fica de fora");
    }

    /// `update` integra a velocidade angular com o dt do frame.
    #[test]
    fn update_integrates_spin() {
        let (mut scene, _assets, entity) = scene_with_one_object();
        scene.world.insert(entity, Spin(Vec3d::new(0.0, 2.0, 0.0)));

        scene.update(0.5);

        let rotation = scene.world.get::<Transform>(entity).unwrap().rotation;
        assert!((rotation.y() - 1.0).abs() < 1e-12);
    }

    /// Sem `Spin` o transform não é tocado — o sistema não roda em cima de
    /// quem não pediu.
    #[test]
    fn update_leaves_entities_without_spin_alone() {
        let (mut scene, _assets, entity) = scene_with_one_object();

        scene.update(1.0);

        let transform = scene.world.get::<Transform>(entity).unwrap();
        assert_eq!(*transform, Transform::IDENTITY);
    }

    /// Um `Spin` sem `Transform` não pode derrubar o sistema.
    #[test]
    fn spin_without_a_transform_is_ignored() {
        let mut scene = Scene::new();
        let entity = scene.world.spawn();
        scene.world.insert(entity, Spin(Vec3d::new(1.0, 0.0, 0.0)));

        scene.update(1.0);

        assert!(scene.world.get::<Transform>(entity).is_none());
    }

    /// O tint continua sendo por entidade depois do split.
    #[test]
    fn the_tint_travels_with_the_renderer() {
        let (mut scene, mut assets, plain) = scene_with_one_object();
        let mesh = assets.add_mesh(Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());

        let tinted = scene.spawn_object(
            Transform::IDENTITY,
            MeshRenderer::new(mesh, texture).with_tint(0x00FF_0000),
        );

        assert_eq!(
            scene.world.get::<MeshRenderer>(plain).unwrap().tint,
            NO_TINT
        );
        assert_eq!(
            scene.world.get::<MeshRenderer>(tinted).unwrap().tint,
            0x00FF_0000
        );
    }
}

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
use crate::ecs::{Entity, Schedule, World};
use crate::math::{Mat4x4, Vec3d};

/// Tint that leaves the sampled texture untouched (white, fully modulated).
pub const NO_TINT: u32 = 0x00FF_FFFF;

/// How many ancestors `world_matrix` climbs before giving up.
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
    // Walk UP toward the root, collecting each ancestor's local matrix
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

/// Which entity, if any, this one's [`Transform`] is relative to.
///
/// No `Parent` means the transform is already in world space — the entity is
/// a root. Like every component, `Parent` needs no registration; the world
/// creates its column on first insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parent(pub Entity);

/// A camera, a light and a world of entities.
pub struct Scene {
    /// The entities and their components.
    pub world: World,
    /// The single directional light lighting every object.
    pub light: DirectionalLight,
    /// System run, in order, by [`Scene::update`]
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
    /// [`Spin`], or a transform with no mesh — is built component by component
    /// through [`Scene::world`].
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
    /// renderer, while transforms also belong to entities that are never drawn.
    pub fn drawables(&self) -> impl Iterator<Item = (&Transform, &MeshRenderer)> + '_ {
        self.world
            .query2::<MeshRenderer, Transform>()
            .map(|(_, renderer, transform)| (transform, renderer))
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
    /// Advances the scene by `dt` seconds, running every scheduled system.
    pub fn update(&mut self, dt: f64) {
        self.schedule.run(&mut self.world, dt)
    }
}

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

    /// Um `MeshRenderer` qualquer, para testes que só se importam com posição
    /// — precisa de um `Assets` de verdade, porque os handles não têm
    /// construtor público (só nascem de `Assets::add_mesh`/`add_texture`).
    fn dummy_renderer(assets: &mut Assets) -> MeshRenderer {
        let mesh = assets.add_mesh(Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());
        MeshRenderer::new(mesh, texture)
    }

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

        // 2: o objeto recém-criado, mais a câmera, que já nasce com a cena.
        assert_eq!(scene.len(), 2);
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
        // Não fica vazia: a câmera continua viva.
        assert_eq!(scene.len(), 1);
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

        // 3: o objeto, o marcador e a câmera, que já nasce com a cena.
        assert_eq!(scene.len(), 3, "as três entidades estão vivas");
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

    /// A câmera é uma entidade de verdade: existe desde que a cena nasce, e
    /// `despawn` nela devolveria a cena para um estado sem `single` válido —
    /// mas ninguém de fora tem o `Entity` dela para fazer isso.
    #[test]
    fn the_scene_is_born_with_exactly_one_camera() {
        let scene = Scene::new();
        assert_eq!(scene.camera().position, Vec3d::ZERO);
    }

    /// Sem `Parent`, a matriz global é exatamente a local.
    #[test]
    fn world_matrix_of_a_root_is_its_own_local_matrix() {
        let mut assets = Assets::new();
        let mut scene = Scene::new();
        let transform = Transform::from_translation(Vec3d::new(1.0, 2.0, 3.0));
        let entity = scene.spawn_object(transform, dummy_renderer(&mut assets));

        let world = crate::scene::world_matrix(&scene.world, entity);

        assert_eq!(world, transform.matrix());
    }

    /// Um filho soma a matriz do pai com a própria — sol e planeta.
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

        let world = crate::scene::world_matrix(&scene.world, planet);

        // O planeta orbita 5 unidades do sol, que está em 10 — no mundo, 15.
        let position = world.transform_point(Vec3d::ZERO);
        assert!((position.x() - 15.0).abs() < 1e-9);
    }

    /// Três gerações: sol, planeta, lua — o teste "aha" do scene graph.
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

        let world = crate::scene::world_matrix(&scene.world, moon);
        let position = world.transform_point(Vec3d::ZERO);

        // 10 (sol) + 5 (planeta) + 1 (lua) = 16.
        assert!((position.x() - 16.0).abs() < 1e-9);
    }

    /// Mover o pai move o filho junto — a promessa central da etapa.
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

        let position = crate::scene::world_matrix(&scene.world, moon).transform_point(Vec3d::ZERO);
        assert!((position.x() - 102.0).abs() < 1e-9);
    }

    /// Um ciclo (A pai de B, B pai de A) não pode travar o programa.
    #[test]
    fn a_parent_cycle_does_not_hang() {
        let mut assets = Assets::new();
        let mut scene = Scene::new();
        let a = scene.spawn_object(Transform::IDENTITY, dummy_renderer(&mut assets));
        let b = scene.spawn_object(Transform::IDENTITY, dummy_renderer(&mut assets));
        scene.world.insert(a, Parent(b));
        scene.world.insert(b, Parent(a));

        // Só precisa devolver — se travasse, o teste nunca terminaria.
        let _ = crate::scene::world_matrix(&scene.world, a);
    }

    /// Sem `Transform` nenhum, a matriz é a identidade.
    #[test]
    fn an_entity_without_a_transform_gets_the_identity() {
        let scene = Scene::new();
        let entity = scene.world.entities().next().unwrap(); // a câmera, sem Transform

        assert_eq!(
            crate::scene::world_matrix(&scene.world, entity),
            Mat4x4::identity()
        );
    }
}

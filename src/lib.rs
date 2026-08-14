//! # Colibri
//!
//! A game engine written from scratch in Rust, one testable stage at a time.
//! Today it is a **software renderer** — no GPU is involved anywhere — on its
//! way to a data-oriented ECS engine.
//!
//! ## Module map
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`math`] | Vectors and 4x4 matrices, and the space conventions |
//! | [`assets`] | Meshes, textures, and the handle registry that owns them |
//! | [`ecs`] | Entities, component storage, and the world that holds them |
//! | [`scene`] | Transforms, camera, light, and the drawable entities |
//! | [`render`] | Clipping, rasterization, and the frame pipeline |
//! | [`engine`] | Window, input, frame timing, and the loop |
//! | [`error`] | The shared error type |
//!
//! ## Conventions
//!
//! Right-handed world space, `+Y` up, `-Z` forward. Column vectors, so `M * v`
//! and chains read right to left. Counter-clockwise front faces. The details
//! live in [`math::matrix`] and [`render::raster`], next to the code that
//! depends on them.
//!
//! ## Example
//!
//! ```
//! use colibri::assets::{Assets, Mesh, Texture};
//! use colibri::math::Vec3d;
//! use colibri::render::{RenderOptions, Renderer};
//! use colibri::scene::{Camera, MeshRenderer, Scene, Spin, Transform};
//!
//! let mut assets = Assets::new();
//! let mesh = assets.add_mesh(Mesh::textured_quad());
//! let texture = assets.add_texture(Texture::checkerboard(64, 8));
//!
//! let mut scene = Scene::new();
//! scene.camera = Camera::new(Vec3d::ZERO);
//!
//! // A drawable entity is a Transform plus a MeshRenderer; anything else it
//! // needs — a Spin, here — is one more component on the same id.
//! let quad = scene.spawn_object(
//!     Transform::from_translation(Vec3d::new(0.0, 0.0, -3.0)),
//!     MeshRenderer::new(mesh, texture),
//! );
//! scene.world.insert(quad, Spin(Vec3d::new(0.0, 1.0, 0.0)));
//!
//! // Render into a plain pixel buffer — no window required.
//! let mut renderer = Renderer::new(RenderOptions::default());
//! renderer.resize(64, 64);
//! let mut pixels = vec![0u32; 64 * 64];
//! let stats = renderer.render(&mut pixels, &scene, &assets);
//!
//! assert!(stats.pixels_shaded > 0);
//! ```

#![warn(missing_docs)]

pub mod assets;
pub mod ecs;
pub mod engine;
pub mod error;
pub mod math;
pub mod render;
pub mod scene;

pub use error::{Error, Result};

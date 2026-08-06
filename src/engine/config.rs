//! Command-line configuration for the demo application.

use crate::render::RenderOptions;

/// Default model loaded when `--model` is not given.
pub const DEFAULT_MODEL: &str = "assets/cube.obj";

/// Everything the engine reads from the command line.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Path to the `.obj` file to display.
    pub model_path: String,
    /// Path to the texture, or `None` for the built-in procedural
    /// checkerboard.
    ///
    /// The checkerboard is the default because straight lines are the pattern
    /// that exposes UV and perspective-correction problems immediately — and
    /// because a photographic texture on an untextured `.obj` just looks like
    /// a bug.
    pub texture_path: Option<String>,
    /// Rasterizer switches, also toggleable at runtime.
    pub render: RenderOptions,
    /// Camera movement speed, in world units per second.
    pub move_speed: f64,
    /// Radians of camera rotation per unit of raw mouse movement.
    pub mouse_sensitivity: f64,
}

impl Default for EngineConfig {
    /// The bundled cube with the procedural checkerboard, culling on, no
    /// debug overlay.
    fn default() -> Self {
        Self {
            model_path: DEFAULT_MODEL.to_string(),
            texture_path: None,
            render: RenderOptions::default(),
            move_speed: 3.0,
            mouse_sensitivity: 0.0025,
        }
    }
}

impl EngineConfig {
    /// Parses the configuration from command-line arguments, excluding the
    /// program name.
    ///
    /// Hand-rolled rather than `clap`: the surface is five flags, and the
    /// project's rule is no dependency without a reason.
    ///
    /// Returns `Err` with a user-facing message on an unknown flag or a
    /// missing value, and `Ok(None)` when `--help` was requested.
    pub fn from_args<I>(args: I) -> Result<Option<Self>, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut config = Self::default();
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => return Ok(None),
                "-t" | "--triangles" => config.render.debug_triangle_tint = true,
                "-w" | "--wireframe" => config.render.wireframe = true,
                "--no-cull" => config.render.backface_culling = false,
                "-m" | "--model" => {
                    config.model_path = next_value(&mut args, &arg)?;
                }
                "--texture" => {
                    config.texture_path = Some(next_value(&mut args, &arg)?);
                }
                other => return Err(format!("unknown argument '{other}'\n\n{}", Self::usage())),
            }
        }

        Ok(Some(config))
    }

    /// The `--help` text, also shown when parsing fails.
    pub fn usage() -> &'static str {
        concat!(
            "colibri — software renderer\n\n",
            "USAGE:\n",
            "    cargo run --release -- [OPTIONS]\n\n",
            "OPTIONS:\n",
            "    -m, --model <PATH>     .obj file to display [default: assets/cube.obj]\n",
            "        --texture <PATH>   image file to sample [default: procedural checkerboard]\n",
            "    -t, --triangles        tint each triangle to expose the tessellation\n",
            "    -w, --wireframe        overlay triangle edges\n",
            "        --no-cull          disable back-face culling\n",
            "    -h, --help             print this message\n\n",
            "CONTROLS:\n",
            "    W A S D                move  ·  Space / Left Ctrl  up and down\n",
            "    Mouse                  look  ·  Left Shift          sprint\n",
            "    F                      toggle wireframe\n",
            "    C                      toggle back-face culling\n",
            "    T                      toggle per-triangle tint\n",
            "    R                      reset the camera\n",
            "    Esc                    quit\n",
        )
    }
}

/// Reads the value that follows a flag, or reports which flag is missing it.
fn next_value<I>(args: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .ok_or_else(|| format!("'{flag}' expects a value\n\n{}", EngineConfig::usage()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<EngineConfig>, String> {
        EngineConfig::from_args(args.iter().map(|s| s.to_string()))
    }

    /// Sem argumento nenhum, cai nos defaults do projeto.
    #[test]
    fn defaults_are_used_when_no_flags_are_given() {
        let config = parse(&[]).unwrap().unwrap();
        assert_eq!(config.model_path, DEFAULT_MODEL);
        assert!(config.render.backface_culling);
        assert!(!config.render.wireframe);
    }

    /// Flags curtas e longas acumulam.
    #[test]
    fn flags_combine() {
        let config = parse(&["-t", "-w", "--no-cull"]).unwrap().unwrap();
        assert!(config.render.debug_triangle_tint);
        assert!(config.render.wireframe);
        assert!(!config.render.backface_culling);
    }

    /// `--model` e `--texture` consomem o valor seguinte.
    #[test]
    fn value_flags_consume_the_next_argument() {
        let config = parse(&["--model", "a.obj", "--texture", "b.png"])
            .unwrap()
            .unwrap();
        assert_eq!(config.model_path, "a.obj");
        assert_eq!(config.texture_path.as_deref(), Some("b.png"));
    }

    /// Sem `--texture`, a textura é a procedural (nenhum arquivo).
    #[test]
    fn the_default_texture_is_procedural() {
        assert!(parse(&[]).unwrap().unwrap().texture_path.is_none());
    }

    /// Flag desconhecida vira erro com o texto de uso, não panic.
    #[test]
    fn unknown_flag_is_an_error() {
        let err = parse(&["--nope"]).unwrap_err();
        assert!(err.contains("--nope"));
        assert!(err.contains("USAGE"));
    }

    /// Valor faltando é erro, e não engole a próxima flag em silêncio.
    #[test]
    fn missing_value_is_an_error() {
        assert!(parse(&["--model"]).is_err());
    }

    /// `--help` não é erro: é "não abra a janela".
    #[test]
    fn help_short_circuits() {
        assert!(parse(&["--help"]).unwrap().is_none());
    }
}

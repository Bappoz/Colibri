<div align="center">
  <img width="300" alt="colibri" src="https://github.com/user-attachments/assets/c9f6836e-5623-42c4-b955-27c0015587bd" />
  <h1>Colibri</h1>
  <p><strong>Uma engine de jogos ECS — 3D e 2D — escrita do zero em Rust, construída em etapas pequenas e demonstráveis.</strong></p>
</div>

---

Colibri é uma biblioteca de engine e uma aplicação de demonstração. Ela começou
com um renderizador de software para tornar visíveis e compreensíveis as peças
de uma engine: matemática, rasterização, assets, entidades e sistemas. A
biblioteca pode renderizar cenas em um buffer de pixels sem abrir uma janela; a
aplicação acrescenta janela, input e apresentação.

## Estado atual

A fundação do **renderizador 3D por software** e o núcleo inicial do **ECS**
estão implementados. O caminho principal não usa GPU.

- Rasterizador de CPU com clipping, back-face culling, z-buffer, UV com correção
  de perspectiva, textura e iluminação difusa direcional com ambiente.
- Estágio de geometria separado do rasterizador paralelo por faixas de pixels;
  `RenderStats` expõe contadores por frame para avaliar o trabalho.
- Câmera FPS e input acumulado; movimento é escalado por `dt`, mouse é consumido
  uma vez por frame e teclas são liberadas ao perder foco.
- ECS com entidades geracionais, colunas `SparseSet<T>`, `World`, queries de um
  e dois componentes, `Schedule` sequencial e sistemas por frame.
- Cena hierárquica com `Parent`; a matriz de mundo combina os transforms da
  cadeia de ancestrais sob demanda.
- Assets `.obj`, imagens e cenas `.gltf`/`.glb`, referenciados por handles.
  Importação glTF mantém a hierarquia, primitivas triangulares separadas,
  rotações e fator/textura de cor base do material.
- Aplicação de demonstração com controles de câmera, wireframe, tint por
  triângulo e seleção de threads do rasterizador.

### Limitações conhecidas

- Matrizes singulares (por exemplo, escala zero em um eixo) não têm uma
  transformação de normais definida; esses objetos recebem iluminação ambiente.
- Materiais são renderizados como opacos; só cor base, textura e UV set 0 são
  usados. Transparência, metallic/roughness e configurações de sampler não são
  aplicadas. O importador aceita primitivas de triângulos.
- Modelos glTF preservam as dimensões e transforms de origem. A câmera enquadra
  a cena ao carregar e acompanha mudanças de viewport até o usuário movê-la;
  `R` restaura o enquadramento. A cena OBJ mantém seu enquadramento atual.
- O ECS e a execução dos sistemas são single-threaded. Só o rasterizador é
  paralelo por enquanto.

O enquadramento automático leva em conta todas as primitivas e transforms da
cena importada. A próxima prioridade é definir e registrar os próximos chunks
de runtime e assets conforme a biblioteca avança.

## Começar

```bash
cargo run --release                 # janela com a cena OBJ de demonstração
cargo run --release -- --help        # opções e controles
cargo run --release -- --model scene.glb
cargo run --release --example bench # benchmark headless
```

Formatos reconhecidos por `--model`: `.obj`, `.gltf` e `.glb`. Um caminho
relativo é interpretado a partir do diretório do projeto.

### Controles

| Tecla | Ação |
|---|---|
| `W` `A` `S` `D` | mover · `Space` / `Left Ctrl` sobe e desce |
| Mouse | olhar · `Left Shift` corre |
| `F` | liga/desliga wireframe |
| `C` | liga/desliga back-face culling |
| `T` | liga/desliga tint por triângulo |
| `R` | reseta a câmera |
| `H` | imprime ajuda e controles no terminal |
| `Esc` | sai |

### Opções

```text
-m, --model <PATH>    arquivo .obj, .gltf ou .glb [padrão: assets/cube.obj]
    --texture <PATH>  imagem para amostrar [padrão: xadrez procedural]
-t, --triangles       colore triângulos para expor a tesselação
-w, --wireframe       desenha arestas sobre as faces
    --no-cull         desliga back-face culling
    --threads <N>     threads do rasterizador [padrão: uma por core]
-h, --help            mostra ajuda
```

Nos OBJ sem coordenadas UV, a textura amostra um único texel. O programa avisa
no startup; isso é esperado para os modelos de demonstração sem `vt`.

## Usar como biblioteca

O crate exporta os módulos `assets`, `ecs`, `engine`, `math`, `render` e
`scene`. Por exemplo, é possível criar uma cena e renderizá-la para um buffer
sem criar janela:

```rust
use colibri::assets::{Assets, Mesh, Texture};
use colibri::math::Vec3d;
use colibri::render::{RenderOptions, Renderer};
use colibri::scene::{Camera, MeshRenderer, Scene, Transform};

let mut assets = Assets::new();
let mesh = assets.add_mesh(Mesh::textured_quad());
let texture = assets.add_texture(Texture::checkerboard(64, 8));

let mut scene = Scene::new();
*scene.camera_mut() = Camera::new(Vec3d::ZERO);
scene.spawn_object(
    Transform::from_translation(Vec3d::new(0.0, 0.0, -3.0)),
    MeshRenderer::new(mesh, texture),
);

let mut renderer = Renderer::new(RenderOptions::default());
renderer.resize(640, 360);
let mut pixels = vec![0u32; 640 * 360];
let stats = renderer.render(&mut pixels, &scene, &assets);
assert!(stats.pixels_shaded > 0);
```

## Organização

```text
src/
├── assets/   meshes, texturas, handles e importador glTF
├── ecs/      entidades, sparse sets, world, queries e schedule
├── engine/   configuração, janela, input e tempo de frame
├── math/     vetores e matrizes; convenções de espaço
├── render/   clipping, rasterização e pipeline de frame
└── scene/    câmera, luz, transforms, hierarquia e renderers
examples/    benchmark headless
assets/      modelos e imagens de demonstração
```

As convenções de espaço, ordem de multiplicação e winding ficam documentadas
junto ao código de matemática e rasterização que depende delas.

## Desenvolvimento e performance

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --release --example bench -- assets/cube.obj 200 1920 1080 1.1
```

O benchmark é headless e aceita `[modelo] [frames] [largura] [altura] [zoom]
[threads]`. Zoom é a distância da câmera em múltiplos do raio do modelo; zoom
baixo testa o caso em que a geometria ocupa grande parte da tela.

Meça em `--release` antes de afirmar que uma otimização melhorou o desempenho.
O estágio de geometria ainda roda em uma thread; malhas densas e pequenas na
tela podem ficar limitadas por esse estágio, mesmo com rasterização paralela.

Com [`just`](https://github.com/casey/just) instalado, `just --list` mostra
atalhos para executar, formatar, analisar e medir o projeto.

## Direção do projeto

| Etapa | Foco | Estado |
|---|---|---|
| Fundações | janela, loop, matemática e framebuffer | implementado |
| Renderização | clipping, culling, iluminação, texturas e rasterização paralela | implementado |
| Interação | input por frame e câmera livre | implementado |
| Núcleo ECS | entidades geracionais, sparse sets, queries e sistemas | implementado |
| Cena e assets | hierarquia, OBJ e importação glTF/GLB | implementado; importador em evolução |
| Transformações | normais corretas para escala não uniforme | implementado |
| Experiência de importação | enquadramento automático de cena | implementado |
| Próximas áreas | GPU, 2D, física, animação e ferramentas | planejamento futuro |

## Princípios

1. Corretude antes de performance; performance antes de estilo.
2. Cada etapa deve ser demonstrável e ter escopo compreensível.
3. Implementar para entender: matemática e ECS permanecem próprios enquanto
   forem parte do objetivo de aprendizado.
4. Medir antes de afirmar desempenho.

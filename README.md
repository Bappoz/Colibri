<div align="center">
  <img width="300" alt="colibri" src="https://github.com/user-attachments/assets/c9f6836e-5623-42c4-b955-27c0015587bd" />
  <h1>Colibri</h1>
  <p><strong>Uma engine de jogos ECS — 3D e 2D — escrita em Rust, construída do zero, uma etapa testável por vez.</strong></p>
</div>

---

Colibri é um projeto de aprendizado que evolui um renderizador de software puro
até uma biblioteca de engine data-oriented (ECS). A ideia não é ter tudo
pronto, e sim seguir um **roadmap claro** onde cada etapa entrega algo que você
consegue **ver ou testar na prática**.

## Status atual

Renderizador de **software** (sem GPU) sobre `winit` + `softbuffer`, já dirigido
por entidades.

**Concluído**

- Janela, loop de frame com `dt` clampado e framebuffer de CPU
- Matemática 3D própria (`Vec3d`, `Vec4d`, `Mat4x4`) — convenção de **vetor coluna**
- Carga de `.obj` (`tobj`) e de texturas (`image`), atrás de handles
- Rasterização baricêntrica com z-buffer, correção de perspectiva e textura
- Back-face culling, trivial reject de frustum e clipping do near plane
- Luz direcional difusa com termo ambiente
- Câmera livre (FPS) com input acumulado
- **Entidades geracionais** + cena com múltiplos objetos
- **Rasterização paralela** por faixas horizontais (`std::thread::scope`)

**Próximo** — storage de componentes de verdade
([etapa 06](docs/src/pages/stages/06-ecs-storage.mdx))

## Rodando

```bash
cargo run --release              # cena de demonstração
cargo run --release -- --help    # flags e controles
```

Com [`just`](https://github.com/casey/just) instalado, `just --list` mostra os
atalhos (`just run`, `just debug`, `just bench`, `just gate`).

### Controles

| Tecla | Ação |
|---|---|
| `W` `A` `S` `D` | mover · `Space` / `Ctrl` sobe e desce |
| Mouse | olhar · `Shift` corre |
| `F` | liga/desliga wireframe |
| `C` | liga/desliga back-face culling |
| `T` | liga/desliga tint por triângulo |
| `R` | reseta a câmera |
| `H` | imprime os controles no terminal |
| `Esc` | sai |

### Flags

```
-m, --model <PATH>     .obj a exibir       [default: assets/cube.obj]
    --texture <PATH>   imagem a amostrar   [default: xadrez procedural]
-t, --triangles        colore cada triângulo (expõe a tesselação)
-w, --wireframe        desenha as arestas por cima
    --no-cull          desliga o back-face culling
    --threads <N>      threads de rasterização  [default: uma por core]
```

> A textura padrão é o xadrez gerado em código: linhas retas são o que
> denuncia erro de UV ou de correção de perspectiva na hora. Dos modelos em
> `assets/`, só `cube.obj` tem `vt` — nos outros a textura amostra um texel só
> e a superfície mostra apenas a iluminação (a engine avisa no startup).

## Estrutura do projeto

```
src/
├── main.rs        # CLI + event loop
├── app.rs         # ApplicationHandler do winit (só roteia eventos)
├── error.rs       # Error/Result compartilhados
├── math/          # vec.rs (Vec3d, Vec4d) · matrix.rs (Mat4x4) — convenções aqui
├── assets/        # mesh.rs (.obj) · texture.rs · registry de handles
├── ecs/           # entity.rs — índices geracionais
├── scene/         # transform · camera · light · Scene (entidades desenháveis)
├── render/        # clip · raster · target · renderer (o pipeline de frame)
└── engine/        # core (janela + loop) · input · clock · config
examples/bench.rs  # benchmark headless, sem janela
assets/            # modelos .obj e imagens
docs/              # site do roadmap (Astro)
```

Regra de leitura: as **convenções de espaço** (mão, ordem de multiplicação,
winding) estão documentadas em `src/math/matrix.rs` e `src/render/raster.rs`,
ao lado do código que depende delas.

## Testes e performance

```bash
cargo test                        # 86 testes unitários, sem janela
cargo run --release --example bench
```

O benchmark é headless — renderiza num `Vec<u32>` — então dá para comparar
mudanças no rasterizador sem abrir janela. Argumentos:
`[modelo] [frames] [largura] [altura] [zoom] [threads]`. O `zoom` é a distância
da câmera em múltiplos do raio do modelo: `2.5` enquadra, `1.1` cola a câmera na
geometria. **É o caso de zoom baixo que importa** — é o que trava quando você
voa para dentro da cena.

Medido nesta máquina (i7-1255U, 12 threads, `--release`, 1920x1080, melhor de 3):

| Caso | Antes | 1 thread | Todas as threads | Ganho |
|---|---|---|---|---|
| cubo colado (`1.1x`) | 75,0 ms · 13 fps | 49,1 ms | **9,3 ms · 108 fps** | 8,1x |
| cubo perto (`1.5x`) | 44,5 ms · 22 fps | 29,9 ms | **5,9 ms · 170 fps** | 7,6x |
| cubo enquadrado (`2.5x`) | 14,0 ms | 9,5 ms | **2,5 ms** | 5,5x |
| teapot, 6320 tris | 8,3 ms | 7,0 ms | **3,9 ms** | 2,1x |
| mountains, 4860 tris | 7,7 ms | 7,4 ms | **5,1 ms** | 1,5x |

A coluna "1 thread" isola o ganho de algoritmo do ganho de paralelismo. Malha
densa e pequena na tela (mountains) ganha menos: aí o gargalo é o estágio de
geometria, que ainda roda numa thread só — é o próximo alvo.

Rode o bench **antes e depois** de mexer em `src/render/` — os números acima são
o ponto de comparação, não uma meta.

## Roadmap (documentação)

O plano completo, etapa a etapa — com objetivos, diagramas, exemplos de código,
ferramentas do Rust e fontes de pesquisa — vive num site Astro em [`docs/`](docs/):

```bash
cd docs
npm install
npm run dev      # abre em http://localhost:4321
```

As fases, em ordem:

| Fase | Foco | O que você ganha |
|---|---|---|
| **0** | Fundações | Janela, framebuffer, math ✅ |
| **A** | Rasterizador software | Triângulos sólidos, culling, clipping, luz, textura ✅ |
| **B** | Interação | Input acumulado + câmera livre (FPS) ✅ |
| **C** | Núcleo ECS | Entidades geracionais ✅ · storage, sistemas, queries |
| **D** | Cena & Assets | Transform hierárquico, handles, glTF/Blender |
| **E** | GPU | Migração para `wgpu`, shaders WGSL, PBR |
| **F** | 2D | Sprites, batching, texto — sobre o mesmo ECS |
| **G** | Sistemas de jogo | Input actions, áudio, física (`rapier`), animação |
| **H** | Ferramentas & Inovação | Editor `egui`, hot-reload, live-link com Blender |

A **Fase C** é a virada de chave: o `Engine` monolítico é dissolvido numa
arquitetura ECS — o primeiro passo já está feito, o `Engine` não é mais dono da
mesh nem da projeção. A **Fase H** é onde a Colibri tenta fazer algo que as
engines existentes não fazem bem — as apostas estão descritas na
[etapa 16](docs/src/pages/stages/16-inovacao.mdx).

## Stack

`winit` · `softbuffer` · `tobj` · `image` — hoje. `wgpu` · `gltf` · `egui` ·
`rapier` — pelo caminho. Matemática é implementada no projeto de propósito:
`glam`/`nalgebra` esconderiam exatamente o que se quer entender.

## Filosofia

1. **Corretude > performance > estilo.**
2. **Cada etapa é testável** — nada de "confie que funciona".
3. **Entender antes de abstrair** — por isso o software renderer vem antes da GPU,
   e o ECS é escrito à mão antes de considerar um crate pronto.
4. **Medir antes de afirmar** — daí o `examples/bench.rs`.

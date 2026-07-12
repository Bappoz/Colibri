<div align="center">
  <img width="300" alt="colibri" src="https://github.com/user-attachments/assets/c9f6836e-5623-42c4-b955-27c0015587bd" />
  <h1>Colibri</h1>
  <p><strong>Uma engine de jogos ECS — 3D e 2D — escrita em Rust, construída do zero, uma etapa testável por vez.</strong></p>
</div>

---

Colibri é um projeto de aprendizado que evolui um renderizador de software puro
até uma biblioteca de engine data-oriented (ECS) completa. A ideia não é ter tudo
pronto, e sim seguir um **roadmap claro** onde cada etapa entrega algo que você
consegue **ver ou testar na prática**.

## Status atual

Renderizador de **software** (sem GPU) sobre `winit` + `softbuffer`:

**Concluído**

- Janela, loop de frame com `dt` real e framebuffer de CPU
- Matemática 3D própria (`Vec3d`, `Vec4d`, `Mat4x4`) — convenção de **vetor coluna**
- Carga de `.obj` (`tobj`) e projeção em perspectiva
- Cubo em **wireframe** girando na tela

**Próximo** — triângulo preenchido + Z-buffer ([etapa 01](docs/src/pages/stages/01-rasterizacao.mdx))

> **Nota:** o crate ainda se chama `rustic-3dgraphic-engine` (nome de origem do
> repositório). A renomeação para `colibri` é uma tarefa pendente — envolve mexer
> nos imports em `src/`.

## Rodando a engine

```bash
cargo run
```

Abre uma janela fullscreen com o cubo girando. `Esc` ou fechar encerra.

## Estrutura do projeto

```
src/
├── main.rs          # ponte winit → App (só roteia eventos)
├── app.rs           # ApplicationHandler: cria a Engine, encaminha eventos
├── engine/core.rs   # Engine: dona da janela, framebuffer e loop de frame
├── math/utils.rs    # Vec3d, Vec4d, Mat4x4 (convenção de coluna)
├── texture/         # mesh (carga .obj) + texturing (amostragem)
└── clipper/         # clipping.rs (stub — entra na etapa 02)
assets/              # modelos .obj e imagens de teste
docs/                # site do roadmap (Astro) — veja abaixo
```

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
| **0** | Fundações | Janela, framebuffer, math (concluído) |
| **A** | Rasterizador software | Triângulos sólidos, culling, clipping, luz, textura |
| **B** | Interação | Input acumulado + câmera livre (FPS) |
| **C** | Núcleo ECS | Entidades geracionais, storage, sistemas, queries |
| **D** | Cena & Assets | Transform hierárquico, handles, glTF/Blender |
| **E** | GPU | Migração para `wgpu`, shaders WGSL, PBR |
| **F** | 2D | Sprites, batching, texto — sobre o mesmo ECS |
| **G** | Sistemas de jogo | Input actions, áudio, física (`rapier`), animação |
| **H** | Ferramentas & Inovação | Editor `egui`, hot-reload, live-link com Blender |

A **Fase C** é a virada de chave: o `Engine` monolítico é dissolvido numa
arquitetura ECS. A **Fase H** é onde a Colibri tenta fazer algo que as engines
existentes não fazem bem — as apostas estão descritas na
[etapa 16](docs/src/pages/stages/16-inovacao.mdx).

## Stack

`winit` · `softbuffer` · `tobj` — hoje. `wgpu` · `glam`/math próprio · `gltf` ·
`egui` · `rapier` — pelo caminho.

## Filosofia

1. **Corretude > performance > estilo.**
2. **Cada etapa é testável** — nada de "confie que funciona".
3. **Entender antes de abstrair** — por isso o software renderer vem antes da GPU,
   e o ECS é escrito à mão antes de considerar um crate pronto.

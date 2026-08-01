# Colibri — engine de jogos ECS (3D/2D) em Rust, do zero

Projeto de **aprendizado por etapas**: evolui de um renderizador de software puro até uma engine data-oriented (ECS). Cada etapa do roadmap entrega algo que dá para ver ou testar — o valor está em implementar o mecanismo, não em usar uma engine pronta.

## Stack
Rust edition 2024 · `winit` 0.30 (janela/eventos) · `softbuffer` 0.4 (framebuffer na CPU, **sem GPU**) · `tobj` (OBJ) · `image`.

## Comandos
```bash
cargo run --release          # janela + cena atual (release importa: rasterização na CPU)
cargo test
cargo clippy -- -D warnings && cargo fmt
```
Existe flag de debug para visualizar wireframe/triângulos — ver `src/` e o README antes de inventar outra.

## Estado e escopo
Roadmap e o que está pronto ficam no `README.md` (mantido atualizado) — leia antes de propor a "próxima feature". Já implementado inclui rasterização baricêntrica com z-buffer, back-face culling e clipping de frustum.

## Convenções
- Uma etapa por vez, testável e demonstrável; não pular para GPU/wgpu, ECS completo ou física antes de a etapa atual fechar.
- Matemática (vetor, matriz, transformação) implementada no projeto — não trocar por `glam`/`nalgebra` sem decisão explícita: a implementação é o objetivo.
- Sem `unwrap` em caminho de loop de render (por frame) sem invariante comentada; erro de carregamento de asset falha com mensagem clara.
- Convenções de espaço (mão, ordem de multiplicação, winding para culling) declaradas por comentário onde a matriz é construída — é onde bugs aparecem.
- Medir com `--release` antes de afirmar ganho de performance.

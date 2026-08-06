# Colibri — receitas de desenvolvimento.
# `just --list` mostra tudo; `just gate` é o que precisa passar antes de commitar.

default:
    @just --list

# Roda a engine (release importa: a rasterização é na CPU).
run *ARGS:
    cargo run --release -- {{ARGS}}

# Roda com os overlays de debug ligados (tint por triângulo + wireframe).
debug *ARGS:
    cargo run --release -- --triangles --wireframe {{ARGS}}

# Benchmark headless. Args: [modelo] [frames] [largura] [altura].
bench *ARGS:
    cargo run --release --example bench -- {{ARGS}}

test:
    cargo test

fmt:
    cargo fmt

lint:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings

# Documentação da lib, com os itens privados de fora.
doc:
    cargo doc --no-deps --open

# Gate completo, na ordem format -> lint -> test -> build.
gate: fmt lint test
    cargo build --release

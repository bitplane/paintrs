#!/usr/bin/env bash
# Shared by CI, just check, and release preflight. Run from the repository root.
set -euo pipefail

RUSTUP_TOOLCHAIN=$(python3 -c 'import tomllib; print(tomllib.load(open("rust-toolchain.toml", "rb"))["toolchain"]["channel"])')
export RUSTUP_TOOLCHAIN
if ! rustup run "$RUSTUP_TOOLCHAIN" cargo clippy --version >/dev/null 2>&1 ||
   ! rustup run "$RUSTUP_TOOLCHAIN" rustfmt --version >/dev/null 2>&1; then
    rustup toolchain install "$RUSTUP_TOOLCHAIN" --profile minimal --component clippy,rustfmt --no-self-update
fi

cargo check --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps

msrv=$(python3 -c 'import tomllib; v = tomllib.load(open("Cargo.toml", "rb"))["package"]["rust-version"]; print(v + ".0" if v.count(".") == 1 else v)')
if ! rustup run "$msrv" rustc --version >/dev/null 2>&1; then
    rustup toolchain install "$msrv" --profile minimal --no-self-update
fi
cargo +"$msrv" test --locked --all-targets
python3 tests/release_recipe.py

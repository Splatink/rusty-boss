#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check
cargo test --locked
cargo test --locked --no-default-features
cargo test --locked --no-default-features --features gui
cargo test --locked --no-default-features --features midi
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --no-default-features --all-targets -- -D warnings
cargo clippy --locked --no-default-features --features gui --all-targets -- -D warnings
cargo clippy --locked --no-default-features --features midi --all-targets -- -D warnings
cargo build --release --locked
if [[ "${RUSTY_BOSS_CHECK_DESKTOP:-0}" == 1 ]]; then
  RUSTY_BOSS_SMOKE_TEST=1 timeout 20s target/release/rusty-boss
fi

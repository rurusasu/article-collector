#!/usr/bin/env bash
set -euo pipefail

cargo run --quiet --locked --example plugin_registry -- \
  validate registry/plugins-v1.json registry/trusted-keys.json
cargo test --quiet --locked --example plugin_registry

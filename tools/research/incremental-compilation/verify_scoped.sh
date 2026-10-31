#!/usr/bin/env bash
# Limited changed-component checks; this is not the formal full suite.
# Caller supplies the pinned Rust/LLVM environment described in RESULTS.md.
set -euo pipefail
cargo test --offline --locked --lib \
  -p zil-db -p zil-expand -p zil-ir -p zil-lower -p zil-syntax -p zil-verify \
  -p zil-llvm-api -p zil-backend-llvm -p zil-driver \
  --features zil-llvm-api/native,zil-backend-llvm/llvm-api,zil-driver/llvm-api
cargo test --offline --locked \
  -p zil-driver -p zil-ir -p zil-syntax -p zil-llvm-api \
  --features zil-llvm-api/native,zil-driver/llvm-api \
  --test cp33_parse_budget --test cp34_fingerprint_sources --test cp35_shared_verify \
  --test impl083_bytes_native --test impl120_field_backing_native \
  --test impl228_consumer --test impl228_generation --test impl229_where_copy \
  --test object_cache212 --test cp34_fingerprint --test cp33_parse_receipts \
  --test builder210 --test narrow_constants
cargo clippy --offline --locked --lib \
  -p zil-db -p zil-expand -p zil-ir -p zil-lower -p zil-syntax -p zil-verify \
  -p zil-llvm-api -p zil-backend-llvm -p zil-driver \
  --features zil-llvm-api/native,zil-backend-llvm/llvm-api,zil-driver/llvm-api \
  -- -D warnings

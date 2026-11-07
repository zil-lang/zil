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
  --test parse_work_budget --test source_fingerprint_identity --test shared_borrow_verification \
  --test bounded_byte_native_bytes_native --test owned_field_backing_field_backing_native \
  --test generated_event_consumers --test generated_event_consumers_generation --test nominal_where_binders_where_copy \
  --test native_object_cache --test identity_fingerprint --test cp33_parse_receipts \
  --test structured_llvm_builder --test narrow_constants --test project_frontend_reuse
cargo clippy --offline --locked --all-targets \
  -p zil-db -p zil-expand -p zil-ir -p zil-lower -p zil-syntax -p zil-verify \
  -p zil-llvm-api -p zil-backend-llvm -p zil-driver \
  --features zil-llvm-api/native,zil-backend-llvm/llvm-api,zil-driver/llvm-api \
  -- -D warnings

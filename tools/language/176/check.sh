#!/usr/bin/env bash
set -euo pipefail
zil176_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd)
cd -- "$zil176_root"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export ZIL_TEST_REPORT_DIR="${ZIL_TEST_REPORT_DIR:-/tmp/zil-impl176-evidence}"
cargo test -p zil-driver \
  --test impl176_callable_fields \
  --test impl065_helpers --test impl121_callable_nominals \
  --test impl125_factory_helpers --test impl126_helper_sequences \
  --test impl167_owned_arrays --test impl167_array_capture_boundary \
  --test impl168_retained_composition --test impl170_package_fields \
  --offline -j2 -- --nocapture

#!/usr/bin/env bash
set -euo pipefail
zil176_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd)
cd -- "$zil176_root"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export ZIL_TEST_REPORT_DIR="${ZIL_TEST_REPORT_DIR:-/tmp/zil-impl176-evidence}"
cargo test -p zil-driver \
  --test callable_package_fields_callable_fields \
  --test concrete_callable_helpers --test callable_nominal_values_callable_nominals \
  --test returned_handler_helpers_factory_helpers --test finite_helper_sequences \
  --test owned_array_elements_owned_arrays --test owned_array_elements_array_capture_boundary \
  --test retained_state_composition_retained_composition --test owning_array_fields_package_fields \
  --offline -j2 -- --nocapture

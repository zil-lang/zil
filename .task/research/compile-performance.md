Doing
# ZIL 编译性能长期专项

```json
{
  "id": "compile-performance",
  "parent_task": "B0",
  "status": "Doing",
  "baseline": "11e05a70419a5f64191a06f72bc3f16744d01f7b",
  "spec_hash": "de431dc8632119e6bca306bd8c48b5497024f2c1330d37a600af9d52c6b16702",
  "normative_hash": "e4bd2d0f939a205b01618c0d5ce53a8725549be093db2658272633021131e44d",
  "branch": "research/compile-performance",
  "authority": "user-requested independent research branch",
  "experiments": {
    "CP01": {
      "status": "NoGo",
      "author": "/root/frontend_audit",
      "write_set": [
        "crates/zil-db/src/lib.rs",
        "crates/zil-db/src/tests.rs",
        "crates/zil-db/examples/compile_performance.rs"
      ],
      "result": "docs/research/compile-performance/CP01-RESULT.md"
    },
    "CP02": {
      "status": "ReviewPass",
      "author": "/root/backend_audit",
      "write_set": [
        "crates/zil-driver/src",
        "crates/zil-driver/tests",
        "crates/zil-backend-llvm/src",
        "crates/zil-backend-llvm/tests",
        "crates/zil-llvm-api/src",
        "crates/zil-llvm-api/tests",
        "crates/zil-cli/src",
        "crates/zil-cli/tests"
      ],
      "default_off_control": "Complete: 480 requests; intervals cross 1; build wall point estimate +4.70%, CPU +5.30%"
    },
    "CP03": {
      "status": "ReviewPass",
      "author": "/root/compiler_research",
      "write_set": [
        "tools/research/compile-performance",
        "docs/research/compile-performance/SOURCES.md"
      ]
    },
    "CP04": {
      "status": "NoGo",
      "author": "/root/frontend_audit",
      "reviewer": "/root/backend_audit",
      "plan": "docs/research/compile-performance/CP04-PLAN.md",
      "write_set": [
        "crates/zil-llvm-api/src/native.rs",
        "crates/zil-llvm-api/src/lib.rs",
        "crates/zil-backend-llvm/src/object_cache.rs",
        "crates/zil-driver/tests/o0_ssa.rs",
        "tools/research/compile-performance/runtime_bench.py",
        "tools/research/compile-performance/test_runtime_bench.py",
        "tools/research/compile-performance/README.md"
      ],
      "candidate": "527657a9256cfaae2babb88dc8a2e3f78f4d6092",
      "reason": "No established compile gain; weighted runtime regression in the frozen five-case workload",
      "result": "docs/research/compile-performance/CP04-RESULT.md"
    },
    "CP05": {
      "status": "Doing",
      "author": "/root",
      "baseline": "11e05a70419a5f64191a06f72bc3f16744d01f7b",
      "write_set": [
        "crates/zil-llvm-api/src/timing.rs",
        "crates/zil-llvm-api/Cargo.toml",
        "crates/zil-backend-llvm/Cargo.toml",
        "crates/zil-driver/Cargo.toml",
        "crates/zil-cli/Cargo.toml",
        "crates/zil-cli/src",
        "crates/zil-cli/tests",
        "crates/zil-driver/tests",
        "crates/zil-driver/src",
        "changes/compile-performance-cp05-timing-feature.md"
      ],
      "purpose": "Compile-time removal of disabled timing collection",
      "plan": "docs/research/compile-performance/CP05-08-PLAN.md"
    },
    "CP06": {
      "status": "Doing",
      "author": "/root/llvm_strategy",
      "baseline": "11e05a70419a5f64191a06f72bc3f16744d01f7b",
      "write_set": [
        "crates/zil-llvm-api/src/preflight.rs",
        "crates/zil-llvm-api/src/native.rs",
        "crates/zil-llvm-api/src/native/builder.rs",
        "crates/zil-llvm-api/tests",
        "evidence/research/compile-performance/CP06"
      ],
      "purpose": "LLVM local-name elimination and validated instruction-order fast path, separate commits",
      "plan": "docs/research/compile-performance/CP05-08-PLAN.md"
    },
    "CP07": {
      "status": "Doing",
      "author": "/root/toolchain_setup",
      "baseline": "11e05a70419a5f64191a06f72bc3f16744d01f7b",
      "write_set": [
        "crates/zil-backend-llvm/src/build.rs",
        "crates/zil-backend-llvm/tests",
        "evidence/research/compile-performance/CP07"
      ],
      "purpose": "Explicit paired LLVM LLD link selection, preserve failures and publication",
      "plan": "docs/research/compile-performance/CP05-08-PLAN.md"
    },
    "CP08": {
      "status": "Doing",
      "author": "/root/benchmark_audit",
      "baseline": "11e05a70419a5f64191a06f72bc3f16744d01f7b",
      "write_set": [
        "tools/research/compile-performance/rust_bench.py",
        "tools/research/compile-performance/test_rust_bench.py",
        "docs/research/compile-performance/CP08-BENCHMARK.md"
      ],
      "purpose": "Real LLVM23 ZIL/Rust complete-build and runtime comparison",
      "plan": "docs/research/compile-performance/CP05-08-PLAN.md"
    },
    "CP10": {
      "status": "Doing",
      "author": "/root/llvm_strategy",
      "baseline": "6c3b3adb2435137fac9f4a1c8257c44e47d909e8",
      "write_set": [
        "crates/zil-expand/src/lib.rs",
        "crates/zil-expand/tests",
        "crates/zil-types/src/aggregates.rs",
        "crates/zil-types/tests",
        "evidence/research/compile-performance/CP10"
      ],
      "purpose": "Build immutable project aggregate/callable-name lookup tables once per request; preserve all budget charges, input order, diagnostics and validation",
      "plan": "docs/research/compile-performance/CP10-PLAN.md"
    }
  },
  "mainline_acceptance": "Hold: CP02 disabled-path compile cost requires further validation",
  "next_priority": "LLVM23 implementation, independent review, quiet full-build A/B and real Rust comparison"
}
```

协调记录只在本研究分支维护；2026-10-30 用户明确授权继续研究开发与 GitHub 操作。既有历史实验与负结果保持。

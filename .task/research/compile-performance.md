Doing
# ZIL 编译性能长期专项

```json
{
  "id": "compile-performance",
  "parent_task": "B0",
  "status": "Doing",
  "baseline": "ad509b5003d9d2a303428f9d01732292988084d8",
  "spec_hash": "245cf29a95d721da6268c43191204777a1015639cba56e8e9025aeac012d0a2c",
  "normative_hash": "4d5480fd3f7de30f6a846489e624ce01bb64d7e835c7e01930bd3410a2206db1",
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
      ]
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
      "status": "Doing",
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
      ]
    }
  }
}
```

完整派工与验证合同见 `docs/research/compile-performance/PLAN.md`。协调记录只在本分支维护。

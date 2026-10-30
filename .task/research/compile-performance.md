Doing
# ZIL 编译性能长期专项

```json
{
  "id": "compile-performance",
  "parent_task": "B0",
  "status": "Doing",
  "baseline": "11e05a70419a5f64191a06f72bc3f16744d01f7b",
  "spec_hash": "58e6c031b059a1efbdbe49a7f0c2eecd61fed0518be1e9d04a0af2f75b07a9cf",
  "normative_hash": "5029830ff7d09378a69ee7cc310536b85f9fec3e7a438ddacc187b14aa6b421a",
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
      "status": "ReviewPass",
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
      "plan": "docs/research/compile-performance/CP05-08-PLAN.md",
      "candidate": "124158826aab043882f4ee29e45ff379465874e8",
      "performance": "Combined fixed 6e candidate: O0 cold ZIL/Rust 0.770692; main bridge wall 0.867481, CPU 0.857453. Individual contributions not isolated.",
      "result": "docs/research/compile-performance/LLVM23-RESULT.md"
    },
    "CP06": {
      "status": "ReviewPass",
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
      "plan": "docs/research/compile-performance/CP05-08-PLAN.md",
      "candidate": "d4c152ed15a7c0b05dd209cb8008e374cf2ec6c8",
      "performance": "Combined fixed 6e candidate: O0 cold ZIL/Rust 0.770692; main bridge wall 0.867481, CPU 0.857453. Individual contributions not isolated.",
      "result": "docs/research/compile-performance/LLVM23-RESULT.md"
    },
    "CP07": {
      "status": "ReviewPass",
      "author": "/root/toolchain_setup",
      "baseline": "11e05a70419a5f64191a06f72bc3f16744d01f7b",
      "write_set": [
        "crates/zil-backend-llvm/src/build.rs",
        "crates/zil-backend-llvm/tests",
        "evidence/research/compile-performance/CP07"
      ],
      "purpose": "Explicit paired LLVM LLD link selection, preserve failures and publication",
      "plan": "docs/research/compile-performance/CP05-08-PLAN.md",
      "candidate": "6339ed8b5f2b8250982a3511371e91ad9ba71bc2",
      "performance": "Combined fixed 6e candidate: O0 cold ZIL/Rust 0.770692; main bridge wall 0.867481, CPU 0.857453. Individual contributions not isolated.",
      "result": "docs/research/compile-performance/LLVM23-RESULT.md"
    },
    "CP08": {
      "status": "ReviewPass",
      "author": "/root/benchmark_audit",
      "baseline": "11e05a70419a5f64191a06f72bc3f16744d01f7b",
      "write_set": [
        "tools/research/compile-performance/rust_bench.py",
        "tools/research/compile-performance/test_rust_bench.py",
        "docs/research/compile-performance/CP08-BENCHMARK.md"
      ],
      "purpose": "Real LLVM23 ZIL/Rust complete-build and runtime comparison",
      "plan": "docs/research/compile-performance/CP05-08-PLAN.md",
      "candidate": "de4b3a65673d246b8e88789b1c4ad0675662334c",
      "performance": "Measurement tool: final 940 build +600 runtime rows independently audited; tool itself is not a speedup.",
      "runner_v2": "8bb908a8c7187d0206d75fb44a6d4aaed2abdd32",
      "runner_v2_review": "f0c9e42cb87ce3257f2fc8ae7ef9175f871ff84c"
    },
    "CP10": {
      "status": "ReviewPass",
      "author": "/root/llvm_strategy",
      "baseline": "0d28a51452f57c2cd27c1007058056c021bf3481",
      "write_set": [
        "crates/zil-expand/src/lib.rs",
        "crates/zil-expand/tests",
        "crates/zil-types/src/aggregates.rs",
        "crates/zil-types/tests",
        "evidence/research/compile-performance/CP10",
        "crates/zil-expand/src/receivers.rs"
      ],
      "purpose": "Build immutable project aggregate/callable-name lookup tables once per request; preserve all budget charges, input order, diagnostics and validation",
      "plan": "docs/research/compile-performance/CP10-PLAN.md",
      "candidate": "c317862f4f9b821645238b30ef03b4508a122038",
      "review": "cb6d016b0fb784f4f1a322f0326595abb2f1f2d6",
      "performance": "Combined fixed 6e candidate: O0 cold ZIL/Rust 0.770692; main bridge wall 0.867481, CPU 0.857453. Individual contributions not isolated.",
      "result": "docs/research/compile-performance/LLVM23-RESULT.md"
    },
    "CP11": {
      "status": "ReviewPass",
      "author": "/root/toolchain_setup + /root",
      "baseline": "40ce762767f22d661fb94f1d8fca4dab68fef279",
      "purpose": "Preserve all validation and logical costs while sharing SourceMap index and moving owned AST/machine arguments",
      "plan": "docs/research/compile-performance/CP11-13-PLAN.md",
      "write_set": [
        "crates/zil-base/src/source_map.rs",
        "crates/zil-base/tests",
        "crates/zil-expand/src/lib.rs",
        "crates/zil-expand/src/modules.rs",
        "crates/zil-expand/tests",
        "crates/zil-backend-llvm/src/emit.rs",
        "crates/zil-backend-llvm/src/emit/resource_closures.rs",
        "evidence/research/compile-performance/CP11"
      ],
      "performance": "Combined fixed 6e candidate: O0 cold ZIL/Rust 0.770692; main bridge wall 0.867481, CPU 0.857453. Individual contributions not isolated.",
      "measurement_plan": "docs/research/compile-performance/LLVM23-ROUND2-MEASUREMENT-PLAN.md",
      "candidates": [
        "0093420b178a75717c406c6fa54c0c9d0b99d243",
        "53f3c0ac64306a3730448123d13dbd9c88e7d7e1",
        "4b2beb13b146d29683b9088b56346d4f15e97409"
      ],
      "reviews": [
        "fb75e827413e32c55a539ade314ad93cc8a766ca",
        "c73248e4d9711f26e3dea8891ca7253e27ca3851",
        "be3d2dbe6fc90e8ea8661442a767b45babbf035c"
      ],
      "result": "docs/research/compile-performance/LLVM23-RESULT.md"
    },
    "CP12": {
      "status": "ReviewPass",
      "author": "/root/benchmark_audit",
      "baseline": "40ce762767f22d661fb94f1d8fca4dab68fef279",
      "purpose": "Explicit trusted local frontend checkpoint; complete unchanged input only, fresh independent Verify and budget accounting",
      "plan": "docs/research/compile-performance/CP11-13-PLAN.md",
      "write_set": [
        "crates/zil-driver/src",
        "crates/zil-driver/tests",
        "crates/zil-cli/src",
        "crates/zil-cli/tests",
        "crates/zil-ir/src/program.rs",
        "crates/zil-ir/src/wire.rs",
        "evidence/research/compile-performance/CP12",
        "docs/research/compile-performance/CP12-QUERY-CACHE.md",
        "crates/zil-backend-llvm/src/lib.rs",
        "crates/zil-backend-llvm/src/object_cache.rs"
      ],
      "performance": "Correctness ReviewPass. Combined unchanged/Rust 0.926736; full incremental mix 1.162195, target not reached; default None.",
      "measurement_plan": "docs/research/compile-performance/LLVM23-ROUND2-MEASUREMENT-PLAN.md",
      "candidate": "397bf24fef0d7f58aaefb444c17c733652b5e12f",
      "review": "8d8bf861df1b33883002398907ed82d05379a711",
      "result": "docs/research/compile-performance/LLVM23-RESULT.md"
    },
    "CP13": {
      "status": "NoGo",
      "author": "/root/llvm_strategy",
      "baseline": "40ce762767f22d661fb94f1d8fca4dab68fef279",
      "purpose": "Investigate O2 multi-CGU configuration and optimization scope with runtime quality controls",
      "plan": "docs/research/compile-performance/CP11-13-PLAN.md",
      "write_set": [
        "evidence/research/compile-performance/CP13",
        "docs/research/compile-performance/CP13-O2.md",
        "crates/zil-backend-llvm/src/build.rs",
        "crates/zil-backend-llvm/src/machine_cgu.rs",
        "crates/zil-backend-llvm/tests",
        "crates/zil-driver/src/lib.rs",
        "crates/zil-driver/src/session.rs",
        "crates/zil-cli/src/main.rs"
      ],
      "performance": "Pre-registered closed 8/4 compile ratio 0.941638 with interval crossing 1; tree4 runtime wall 1.272744 and CPU 1.282923, all five blocks regress. No adoption; default-off research code retained. Single-CGU compile 3.132322.",
      "measurement_plan": "docs/research/compile-performance/LLVM23-ROUND2-MEASUREMENT-PLAN.md",
      "candidates": [
        "6d3f01580c1ae921a1b26b410a8288acc8dff4ba",
        "ec089eab8088d1e616515fb573b66576323765e9"
      ],
      "reviews": [
        "ac10f2c5417b15a98c95f546f711dbf9a5d5a601",
        "4d181f2add1694862db6860e8156f052c86a169a"
      ],
      "result": "docs/research/compile-performance/LLVM23-RESULT.md",
      "runtime_review": "256b0f40d1f119b39f3fb2fbe9962af579ddaafe"
    },
    "CP14": {
      "status": "ReviewPass",
      "author": "/root/o2_strategy",
      "baseline": "f95e751d6782310d8d48c7b3eba3037e176e3eb3",
      "purpose": "Keep small closed O2 executables in one codegen unit using a frozen 2048 machine-instruction threshold; preserve default modes and reject invalid unit counts.",
      "plan": "docs/research/compile-performance/COMPILE4-PLAN.md",
      "write_set": [
        "crates/zil-backend-llvm/src/build.rs",
        "crates/zil-backend-llvm/src/machine_cgu.rs",
        "crates/zil-backend-llvm/tests/closed_executable_cp13.rs",
        "crates/zil-backend-llvm/tests/closed_executable_cp14.rs"
      ],
      "candidate": "0ed1043c98e6ffc2256b86b34cb8dd626c15c861",
      "measured_source": "0393660f7dc8414b0ab8c77cd3d576395f4fe095",
      "result": "docs/research/compile-performance/COMPILE4-RESULT.md",
      "acceptance_scope": "Research implementation and targeted verification only; mainline/product gates NotRun.",
      "published_candidate": "021d1411fe4297469a067b9cd2408f3318d918a0",
      "performance": "Direct closed O2 1/1 versus candidate requested8/4: small tree and retained ELF identical, no stable per-case regression observed in three blocks; not universal adoption. Whole candidate O2/Rust compile1.096820."
    },
    "CP15": {
      "status": "ReviewPass",
      "author": "/root",
      "baseline": "f95e751d6782310d8d48c7b3eba3037e176e3eb3",
      "purpose": "Avoid deep copying unchanged closure-inference output while executing all original validation and logical budget charges.",
      "plan": "docs/research/compile-performance/COMPILE4-PLAN.md",
      "write_set": [
        "crates/zil-types/src/inference.rs",
        "crates/zil-types/src/lib.rs",
        "crates/zil-types/src/program.rs",
        "crates/zil-driver/src/project.rs",
        "crates/zil-driver/tests/borrowed_inference_cp15.rs"
      ],
      "candidate": "6b543595e8d86cb03816f86cdcfb56941e5a3876",
      "measured_source": "0393660f7dc8414b0ab8c77cd3d576395f4fe095",
      "result": "docs/research/compile-performance/COMPILE4-RESULT.md",
      "acceptance_scope": "Research implementation and targeted verification only; mainline/product gates NotRun.",
      "published_candidate": "aa5cf96328097d6c8ec1d965c53f4511cec97474",
      "performance": "O0 source/object/no-query same-host bridge0.991120 wall,0.993421CPU; small0.89% wall gain, not universal across cases; all recorded O0 ELF unchanged. Test Option fix in integrated0393660f."
    },
    "CP16": {
      "status": "ReviewPass",
      "author": "/root/benchmark_review",
      "baseline": "f95e751d6782310d8d48c7b3eba3037e176e3eb3",
      "purpose": "Freeze explicit optional Rust LTO controls alongside the unchanged native primary comparison.",
      "plan": "docs/research/compile-performance/COMPILE4-PLAN.md",
      "write_set": [
        "tools/research/compile-performance/rust_bench.py",
        "tools/research/compile-performance/test_rust_bench.py",
        "docs/research/compile-performance/CP16-BENCHMARK-CONTROLS.md"
      ],
      "candidate": "e6314eaa82b42c1d8c0782a813c0f9ff010499dc",
      "measured_source": "0393660f7dc8414b0ab8c77cd3d576395f4fe095",
      "result": "docs/research/compile-performance/COMPILE4-RESULT.md",
      "acceptance_scope": "Research implementation and targeted verification only; mainline/product gates NotRun.",
      "published_candidate": "6fe00cda5a2de694bb6338dc32c5fe88af2e9571"
    },
    "CP17": {
      "status": "ReviewPass",
      "author": "/root/benchmark_review",
      "baseline": "e6314eaa82b42c1d8c0782a813c0f9ff010499dc",
      "candidate": "d7a8abc52bdf460b73de5f0f32d2683af456372f",
      "purpose": "Run frozen source builds, targeted correctness controls and the existing full five-case comparison serially on one CI host; retain failed and complete evidence without granting adoption.",
      "plan": "docs/research/compile-performance/COMPILE4-PLAN.md",
      "write_set": [
        "tools/research/compile-performance/compile4_ci.py",
        ".github/workflows/compile-performance.yml",
        "docs/research/compile-performance/CP17-CI.md"
      ],
      "measured_source": "0393660f7dc8414b0ab8c77cd3d576395f4fe095",
      "result": "docs/research/compile-performance/COMPILE4-RESULT.md",
      "acceptance_scope": "Research implementation and targeted verification only; mainline/product gates NotRun.",
      "published_candidate": "0284ddd1154b94ef5b3765166a854307b43d0661",
      "run": "https://github.com/zil-lang/zil/actions/runs/41068328117"
    },
    "CP18": {
      "status": "ReviewPass",
      "author": "/root/shape_opt",
      "baseline": "17d31322fdc12bb22ea17c7e1d3a64d7c378ce10",
      "purpose": "Preserve checks, diagnostic order and logical charges while sharing sparse project declaration views and borrowing unchanged shape resolution. Separate implementation commits and measurements.",
      "plan": "docs/research/compile-performance/COMPILE5-PLAN.md",
      "write_set": [
        "crates/zil-types/src/aggregates.rs",
        "crates/zil-types/src/enums.rs",
        "crates/zil-types/src/program.rs",
        "crates/zil-types/src/inference.rs",
        "crates/zil-types/src/lib.rs",
        "crates/zil-types/tests/shape_resolution_cp18.rs"
      ],
      "candidate": "8cd1a412e5ab260843469a3d9b25125a9bed5140",
      "validation": "78 passed default-feature affected targets",
      "performance": "Combined O0 bridge B/A 0.984244; separate local composite probes have mixed block directions and wall point ratios 1.090573/0.888787/1.101003. Individual contributions unisolated; adoption Hold.",
      "independent_source_review": "Pass by /root/industry_audit; CP18 Cow additionally checked by /root/ct_opt",
      "evidence": "evidence/research/compile-performance/COMPILE5/local/MANIFEST.json",
      "published_candidate": "b27f5feaadc51d4e593ad31b3380f67fed128fdf",
      "measured_source": "951471d5a6e26e15de96e0bfaf91cd3e537a3dd9",
      "result": "docs/research/compile-performance/COMPILE5-RESULT.md",
      "acceptance_scope": "Research correctness/targeted execution passed; performance adoption Hold; no mainline/product gate acceptance.",
      "combined_validation": "CI 41086451087: 231 targeted Rust tests, 64 Python controls, 600 compile and 240 runtime rows independently verified."
    },
    "CP19": {
      "status": "ReviewPass",
      "author": "/root/ct_opt",
      "baseline": "17d31322fdc12bb22ea17c7e1d3a64d7c378ce10",
      "purpose": "Investigate CT allocation/index reuse; do not skip walker checks or resolve/check stages. Implementation only after demonstrated concrete redundant work.",
      "plan": "docs/research/compile-performance/COMPILE5-PLAN.md",
      "write_set": [
        "crates/zil-ct/src/source.rs",
        "crates/zil-ct/src/source",
        "crates/zil-driver/src/ct_program.rs",
        "crates/zil-driver/src/project.rs",
        "crates/zil-ct/tests/borrowed_source_cp19.rs",
        "crates/zil-driver/tests/ct_reuse_cp19.rs"
      ],
      "candidate": "ce2d87e7ef93594e7461760fc184efc7f181d5f2",
      "validation": "26 CT lib plus 4 driver passed",
      "performance": "Combined O0 bridge B/A 0.984244; separate local composite probes have mixed block directions and wall point ratios 1.090573/0.888787/1.101003. Individual contributions unisolated; adoption Hold.",
      "independent_source_review": "Pass by /root/industry_audit",
      "evidence": "evidence/research/compile-performance/COMPILE5/local/MANIFEST.json",
      "published_candidate": "f571fa8a063d9b304bfad92a26d3b624ef48bb16",
      "measured_source": "951471d5a6e26e15de96e0bfaf91cd3e537a3dd9",
      "result": "docs/research/compile-performance/COMPILE5-RESULT.md",
      "acceptance_scope": "Research correctness/targeted execution passed; performance adoption Hold; no mainline/product gate acceptance.",
      "combined_validation": "CI 41086451087: 231 targeted Rust tests, 64 Python controls, 600 compile and 240 runtime rows independently verified."
    },
    "CP20": {
      "status": "ReviewPass",
      "author": "/root",
      "reviewer": "/root/industry_audit",
      "baseline": "c8bacf734",
      "purpose": "Require bridge summaries and preflight to bind complete receipts, exact frozen commands/sources, raw CLI/native log hashes and oracle bytes. Preserve all timing/weights.",
      "write_set": [
        "tools/research/compile-performance/bench.py",
        "tools/research/compile-performance/test_bench.py",
        "tools/research/compile-performance/runtime_bench.py",
        "tools/research/compile-performance/compile4_ci.py",
        "tools/research/compile-performance/test_runtime_bench.py"
      ],
      "plan": "docs/research/compile-performance/COMPILE5-PLAN.md",
      "candidate": "71d2c20c554ddfa36a0741be4aa1ed290de9ea5c",
      "validation": "64 Python controls passed",
      "performance": "Evidence validation tool, not a speedup; frozen estimators and all 8 measured summaries independently reproduced.",
      "independent_source_review": "Pass by /root/industry_audit",
      "evidence": "evidence/research/compile-performance/COMPILE5/local/MANIFEST.json",
      "published_candidate": "3e58541c37f6e563809679b55706cd4b6756ccb1",
      "measured_source": "951471d5a6e26e15de96e0bfaf91cd3e537a3dd9",
      "result": "docs/research/compile-performance/COMPILE5-RESULT.md",
      "acceptance_scope": "Research correctness/targeted execution passed; performance adoption Hold; no mainline/product gate acceptance.",
      "combined_validation": "CI 41086451087: 231 targeted Rust tests, 64 Python controls, 600 compile and 240 runtime rows independently verified."
    },
    "CP21": {
      "status": "Doing",
      "author": "/root/toolchain_recovery",
      "reviewer": "/root/incremental_review",
      "baseline": "485301e1f1dcdfb0b4e24f6249355104243c3fe3",
      "purpose": "Skip impossible invalidation scans for absent query producers without a reverse-index structure",
      "plan": "docs/research/compile-performance/COMPILE6-PLAN.md",
      "write_set": [
        "crates/zil-db/src/lib.rs",
        "crates/zil-db/src/tests.rs",
        "crates/zil-db/examples/compile_performance.rs",
        "evidence/research/compile-performance/CP21"
      ]
    },
    "CP22": {
      "status": "Doing",
      "author": "/root/zit_analysis",
      "reviewer": "/root/incremental_review",
      "baseline": "485301e1f1dcdfb0b4e24f6249355104243c3fe3",
      "purpose": "Per-module trusted-local parse reuse with exact budget receipts and current import discovery; fresh Verify remains",
      "plan": "docs/research/compile-performance/COMPILE6-PLAN.md",
      "write_set": [
        "crates/zil-syntax/src/lib.rs",
        "crates/zil-syntax/src/parser/mod.rs",
        "crates/zil-syntax/src/attributes/mod.rs",
        "crates/zil-syntax/src/cache.rs",
        "crates/zil-driver/src/lib.rs",
        "crates/zil-driver/src/session.rs",
        "crates/zil-driver/src/project.rs",
        "crates/zil-driver/src/parse_cache.rs",
        "crates/zil-driver/tests/parse_cache_cp22.rs",
        "crates/zil-syntax/tests/parse_cache_cp22.rs",
        "docs/research/compile-performance/CP22-PARSE-CACHE.md",
        "evidence/research/compile-performance/CP22"
      ]
    }
  },
  "mainline_acceptance": "Hold: main not changed by this research round; targeted correctness passed, composite-type performance risk signals unresolved; full workspace/product gates NotRun.",
  "next_priority": "Isolate type_cow clone/compare and CT memo costs before adoption; then fine-grained frontend invalidation for private edits and O2 improvements preserving per-case runtime quality.",
  "latest_result": "docs/research/compile-performance/COMPILE5-RESULT.md",
  "measured_source": "951471d5a6e26e15de96e0bfaf91cd3e537a3dd9",
  "scope": "Same-host combined O0 bridge reduces wall cost 1.576%; Rust O0cold0.818380, incremental1.312799,O2closed1.047511. Separate local composite probes mixed; no cross-host pooled score or universal Rust advantage.",
  "compile4_baseline": "64072793a0f41994f216a6207bf49ee1e2d224f8",
  "compile4_status": "Complete_Exploratory_NotAProductGate:149 Cargo tests and52 Python controls pass;600 build+240 runtime rows independently checked.",
  "compile5_baseline": "a715f828261ff4b6ef9c8125623064360745aae4",
  "compile5_status": "Complete_Exploratory_NotAProductGate: 231 Rust + 64 Python controls passed; 600 compile +240 runtime rows independently checked; 36 separate auxiliary compile samples retained; adoption Hold.",
  "compile5_auxiliary": {
    "author": "/root",
    "source_generator_author": "/root/ct_opt",
    "executor_reviewer": "/root/shape_opt",
    "raw_results_reviewer": "/root/industry_audit",
    "scope": "Separate local host, same A/B CPUs [0,1], fixed 3 cases and36 rows;6 unscored preflight; no pooled score.",
    "evidence": "evidence/research/compile-performance/COMPILE5/auxiliary",
    "source_scope": "No production compiler change after951471d5."
  },
  "compile6_baseline": "485301e1f1dcdfb0b4e24f6249355104243c3fe3",
  "compile6_status": "Doing: CP21 and CP22 independently dispatched; no new measured performance claim"
}
```

协调记录只在本研究分支维护；2026-10-30 用户明确授权继续研究开发与 GitHub 操作。既有历史实验与负结果保持。


2026-10-30 zil编译4：承接 f95e751d6，CP14/15/16 并行派工。已合入最新主线 a8e8831a，远端基线固定为 64072793；main 本身不受本轮未验收改动影响。

2026-10-30 CP17 由根协调者派工并审核上述写集。CP15 实际回归位于 driver，以真实解析/展开输入覆盖借用、改写及预算；合流保留 source #if 扣费，且独立复核发现并修正测试中 Option<CoreExpr> 解包遗漏。仍等待真实 Rust 执行。

2026-10-30 zil编译4：候选0393660f完成远端执行和独立复算。CP14/15/16/17按研究范围ReviewPass；149项定向Cargo测试、52项工具控制通过，840条正式计时记录完整；总体超过Rust未达成，main合流Hold。详见COMPILE4-RESULT.md，历史负结果不改。

2026-10-31 北京时间 zil编译5：候选951471d5完成独立同机CI和三份复核。O0旧新桥接降低1.576%；与Rust冷/增量/O2分别0.818380/1.312799/1.047511。辅助复合类型窗口正负混合，不以主矩阵收益覆盖风险；研究交付完成，主线采用Hold。完整证据见COMPILE5-RESULT.md。

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
      "result": "docs/research/compile-performance/LLVM23-RESULT.md",
      "compile6_addendum": "Previously untested publication-temp collision independently reproduced on original485; original Pass scope/data retained. Independently reviewed CP28 repairs the defect in research only, without parse cache adoption. Main unchanged by this round."
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
      "status": "NoGo",
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
      ],
      "candidate": "f31fa58c4b0cf3f5fdd264e938dc2cb6646202bd",
      "result": "docs/research/compile-performance/CP21-RESULT.md",
      "performance": "Full CLI 240 requests, 768 native checks; wall B/A 1.0189467169, CPU 1.0257634777, exploratory wall interval [0.9655310566,1.0271433996]; no established gain"
    },
    "CP22": {
      "status": "NoGo",
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
        "crates/zil-cli/src/main.rs",
        "crates/zil-driver/tests/parse_cache_cp22.rs",
        "crates/zil-syntax/tests/parse_cache_cp22.rs",
        "docs/research/compile-performance/CP22-PARSE-CACHE.md",
        "evidence/research/compile-performance/CP22"
      ],
      "cli_scope": "HELP description only: syntax memo and frontend checkpoint; no CLI behavior change",
      "candidate": "f660aeb68e4cf86eab0bc7c9e219d5bcf8bdb5ea",
      "delivery": "7ca57fb61aa6fe4f7cac963a531a6cf2851792e4",
      "review": "evidence/research/compile-performance/COMPILE6/CP22-review.json",
      "performance": "Initial targeted/engagement checks passed; later publication review ChangesRequested. Full CLI wall B/A 1.0205131985, CPU 0.9917486312, cold wall 1.3299064214, private-edit wall 0.9329474997; NoGo, original observations retained, successor CP26 separate.",
      "known_correctness_issue": "Independent late review reproduced existing-temp collision: cleanup guard constructed before create_new can unlink a previously published output on AlreadyExists. Initial tests/data retained; CP26 must fix ownership before adoption.",
      "review_addendum": "evidence/research/compile-performance/COMPILE6/CP22-review-addendum.json",
      "result": "docs/research/compile-performance/CP22-RESULT.md"
    },
    "CP23": {
      "status": "NoGo",
      "author": "/root/toolchain_recovery",
      "reviewer": "/root/incremental_review",
      "baseline": "485301e1f1dcdfb0b4e24f6249355104243c3fe3",
      "purpose": "Fast-path missing or leaf query reads while preserving active-cycle priority and all nonleaf closure checks",
      "plan": "docs/research/compile-performance/COMPILE6-PLAN.md",
      "write_set": [
        "crates/zil-db/src/lib.rs",
        "crates/zil-db/src/tests.rs",
        "crates/zil-db/examples/query_read_cp23.rs",
        "evidence/research/compile-performance/CP23"
      ],
      "candidate": "4496b1ab6c62ff1131dfbe4e09b33467f346099c",
      "performance": "Original full window retains two LLD failures and NoScore. Separate recovered window 240/240 plus768native, wall B/A1.0270763911589396, CPU1.035669598423132, exploratory95[0.9757440034850736,1.0528941508185157]. Independently recomputed; no established complete-request gain.",
      "result": "docs/research/compile-performance/CP23-RESULT.md",
      "review": "evidence/research/compile-performance/COMPILE6/CP23-recovered-full-cli-review.json"
    },
    "CP24": {
      "status": "ReviewPass",
      "author": "/root",
      "reviewer": "/root/incremental_review",
      "baseline": "8bc38b24a75e56cfb9ca07d454aa22bf60190f2b",
      "purpose": "Freeze a symmetric explicit frontend-cache A/B window and retain parse/checkpoint reports without changing default windows, weights or estimator",
      "write_set": [
        "tools/research/compile-performance/bench.py",
        "tools/research/compile-performance/test_bench.py",
        "tools/research/compile-performance/rust_bench.py (cache_scope/help text only)"
      ],
      "plan": "docs/research/compile-performance/COMPILE6-PLAN.md",
      "reviewed_commit": "c24cc08333dfb4ee710a7443414093f82841e0eb",
      "review": "evidence/research/compile-performance/COMPILE6/CP24-review.json"
    },
    "CP26": {
      "status": "ReviewPass",
      "author": "/root/zit_analysis",
      "reviewer": "/root/incremental_review",
      "baseline": "7ca57fb61aa6fe4f7cac963a531a6cf2851792e4",
      "purpose": "Isolate recoverable parse memo publication IO: buffered complete write/flush/close and atomic rename; no per-file durability promise, fresh verification and all failure gates preserved",
      "write_set": [
        "crates/zil-driver/src/parse_cache.rs",
        "crates/zil-driver/tests/parse_publication_cp26.rs",
        "docs/research/compile-performance/CP26-PARSE-PUBLICATION.md",
        "evidence/research/compile-performance/CP26"
      ],
      "plan": "docs/research/compile-performance/COMPILE6-PLAN.md",
      "candidate": "d8872b52f5a215c2efd33aaadba5083f471e0bc2",
      "author_validation": "22 author and independently rerun targeted tests, independent 5 CLI / 4 native controls passed. Release SHA256 c9f10a9840fd8bdbb884e4981c7dbb5895ebfe653ecb7b486b1979c16441278f.",
      "delivery": "e6bf523eee3c83510505d74838fdfd298cbff502",
      "review": "evidence/research/compile-performance/COMPILE6/CP26-review.json",
      "performance": "Complete independently reviewed 240/240 full CLI and 768 native checks; wall B/A 0.9938432463326667, CPU 1.0205148634147498, exploratory95 [0.9504020168445472,1.0264901370314046]. Cold wall 0.8389311500021226; no established overall gain.",
      "result": "docs/research/compile-performance/CP26-RESULT.md"
    },
    "CP27": {
      "status": "ReviewPass",
      "author": "/root/toolchain_recovery",
      "reviewer": "/root/incremental_review",
      "baseline": "d8872b52f5a215c2efd33aaadba5083f471e0bc2",
      "purpose": "Repair preexisting frontend checkpoint and LLVM object-cache temporary-file ownership; only delete a path after this request successfully creates it, and relinquish cleanup on rename success. Preserve formats, keys, budgets, verifier and publication policy.",
      "write_set": [
        "crates/zil-driver/src/query_cache.rs",
        "crates/zil-backend-llvm/src/object_cache.rs",
        "crates/zil-driver/tests/cache_publication_cp27.rs",
        "crates/zil-backend-llvm/src/object_cache.rs (scoped regression tests)",
        "docs/research/compile-performance/CP27-TEMP-OWNERSHIP.md",
        "evidence/research/compile-performance/CP27"
      ],
      "plan": "docs/research/compile-performance/COMPILE6-PLAN.md",
      "findings": [
        "evidence/research/compile-performance/COMPILE6/CP12-review-finding.json",
        "evidence/research/compile-performance/COMPILE6/OBJECT-review-finding.json"
      ],
      "candidate": "d9a71f603d73c1df4a367189cc5b41ca15bf7d70",
      "author_validation": "31 targeted tests passed; independent 31 rerun plus 9 real CLI / 9 native controls; final release SHA256 b82d690e8e26c6337f02d67cf515d816ed833db6b6d69ff6f66db49a889db7d3.",
      "delivery": "a1ff78eb8b3cee0a9ba2720ac4bbaa6aa8174dd0",
      "review": "evidence/research/compile-performance/COMPILE6/CP27-review.json",
      "delivery_review": "evidence/research/compile-performance/COMPILE6/CP27-delivery-review.json",
      "published_source": "abefce0c2c1731a085ce97f941e32b3286209b30",
      "published_delivery": "79bb5348fe426a62aecaae808321f70966cbc24c",
      "validation": "31 author and independently rerun targeted tests; independent query4CLI/4native and object5CLI/5native. Overlapping executions not added as unique tests.",
      "performance": "Full combined candidate versus original485: 240/240 full CLI and 768 native checks pass; wall B/A 1.0758383658, CPU1.0792596348, exploratory95 [0.9559321878,1.1226603064], independent formal data/math/parse/object review Pass. Full performance candidate NoGo; not an isolated estimate of the two ownership fixes. Native Rust complete360build/1152native plus180runtime independently reviewed; O0cold0.7941191017, incremental1.8629389121, O2closed1.0217591117; no general compiler advantage.",
      "result": "docs/research/compile-performance/CP27-RESULT.md",
      "performance_review": "evidence/research/compile-performance/COMPILE6/CP27-vs-baseline-performance-verdict.json"
    },
    "CP25": {
      "status": "Planned",
      "author": "/root/toolchain_recovery",
      "reviewer": "/root/incremental_review",
      "baseline": "474e0d84a0e2a941e318533f8558441731d738fe",
      "scope": "Read-only feasibility study completed; no new production stable-owner/transaction collector implemented or performance measured.",
      "purpose": "Separate stable owner from complete invocation identity, capture actual dependencies transactionally, and only then investigate result-surface point-to-point propagation and ensure-current.",
      "write_set": [
        "docs/research/compile-performance/CP25-FEASIBILITY.md",
        "docs/research/compile-performance/ZIT-QUERY-REVIEW.md",
        "docs/research/compile-performance/ZIT-FULL-COVERAGE-ADDENDUM.md"
      ],
      "result": "docs/research/compile-performance/CP25-FEASIBILITY.md",
      "adoption": "Hold: present project-level query granularity, globally assigned IDs, negative lookups, semantic summaries and shared budgets prevent a safe scheduling-only shortcut."
    },
    "CP28": {
      "status": "ReviewPass",
      "author": "/root/toolchain_recovery (implementation), /root/ownership_finish (validation)",
      "reviewer": "/root/cp28_independent",
      "baseline": "485301e1f1dcdfb0b4e24f6249355104243c3fe3",
      "purpose": "Independently backport the two proven preexisting temporary-file ownership fixes onto the original baseline, without adopting the failed parse-cache performance candidate. Preserve key/format/quota/Verify/cache opt-in/durability contracts.",
      "branch": "research/cp28-cache-ownership",
      "write_set": [
        "crates/zil-driver/src/query_cache.rs",
        "crates/zil-backend-llvm/src/object_cache.rs",
        "crates/zil-driver/tests/cache_publication_cp27.rs",
        "docs/research/compile-performance/CP28-CACHE-OWNERSHIP.md",
        "evidence/research/compile-performance/CP28"
      ],
      "validation_plan": "Exact baseline+scoped patch/source audit; same 31 relevant controls rerun on CP28 and independent real query/object collision-preservation, repair and warm-hit CLI/native controls. No compile performance claim. All build/CLI work waits until the three frozen Rust formal windows are complete.",
      "adoption": "Correctness-only local research integration approved by independent Pass; remote update separately confirmed; main Hold.",
      "candidate": "fc7fa3dcb53516cad1fca0d38362c8d6b2fccc16",
      "delivery": "6e5e81c59c5324a0f555ac736d8a03fb9c40e984",
      "crates_tree": "a7ae8da1862a69cf673b8db50431b4991cb0d1c5",
      "review": "evidence/research/compile-performance/CP28/CP28-review.json",
      "result": "docs/research/compile-performance/CP28-CACHE-OWNERSHIP.md",
      "validation": "31 author and independently rerun targeted tests, separately9CLI/9native each; exact208 source inputs/source and delivery crates/release metadata bound. Overlapping validations not summed.",
      "performance": "NotRun onCP28. Full CP27 combined candidate performance does not isolate these two guards."
    }
  },
  "mainline_acceptance": "Hold: main not changed by this research round; targeted correctness passed, composite-type performance risk signals unresolved; full workspace/product gates NotRun.",
  "next_priority": "Isolate cache preparation/validation and AST materialization full costs, then stable owner/transactional dependencies/result facets/ensure-current and affected SCC recomputation. CP25 feasibility only; do not add scheduling without actual work reduction.",
  "latest_result": "docs/research/compile-performance/COMPILE6-RESULT.md",
  "measured_source": "d9a71f603d73c1df4a367189cc5b41ca15bf7d70",
  "scope": "Compile6: full parse candidate NoGo versus original485 wall1.075838/CPU1.079260. NativeRustO0cold0.794119,incremental1.862939,O2closed1.021759. Adopt only independently reviewed baseline CP28 ownership fixes; no CP28 performance claim.",
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
  "compile6_status": "Complete_Exploratory_NotAProductGate: deepZIT review and original-source synchronous controls; five independently reviewed complete ZIL240-row windows (original CP23 failed window separately retained), threeRust360build/1152native+180runtime; full candidate NoGo. CP28correctness Pass and local research integration; remote publication must be confirmed separately.",
  "adopted_correctness_source": "fc7fa3dcb53516cad1fca0d38362c8d6b2fccc16",
  "compile6_remote_publication": {
    "confirmed_remote": "e7b492c6e04128e6106bbf29c0d6c7b381556d77",
    "confirmed_tree": "156f35b43b990608104bce607206671b16140ab2",
    "status": "Blocked_WriteAPITimeouts_ReadbackUnchanged",
    "reason": "create_blob/create_tree/create_branch20s bounded requests all timeout; readback works; no explicit permissions or auto-review rejection returned",
    "receipt": "evidence/research/compile-performance/COMPILE6/GIT-FINAL-PUBLICATION.json",
    "scope": "Final local integration/evidence not yet published remotely; isolated earlier CP21/22/23/26/27 branches remain published; main not written by this round"
  }
}
```

协调记录只在本研究分支维护；2026-10-30 用户明确授权继续研究开发与 GitHub 操作。既有历史实验与负结果保持。


2026-10-30 zil编译4：承接 f95e751d6，CP14/15/16 并行派工。已合入最新主线 a8e8831a，远端基线固定为 64072793；main 本身不受本轮未验收改动影响。

2026-10-30 CP17 由根协调者派工并审核上述写集。CP15 实际回归位于 driver，以真实解析/展开输入覆盖借用、改写及预算；合流保留 source #if 扣费，且独立复核发现并修正测试中 Option<CoreExpr> 解包遗漏。仍等待真实 Rust 执行。

2026-10-30 zil编译4：候选0393660f完成远端执行和独立复算。CP14/15/16/17按研究范围ReviewPass；149项定向Cargo测试、52项工具控制通过，840条正式计时记录完整；总体超过Rust未达成，main合流Hold。详见COMPILE4-RESULT.md，历史负结果不改。

2026-10-31 北京时间 zil编译5：候选951471d5完成独立同机CI和三份复核。O0旧新桥接降低1.576%；与Rust冷/增量/O2分别0.818380/1.312799/1.047511。辅助复合类型窗口正负混合，不以主矩阵收益覆盖风险；研究交付完成，主线采用Hold。完整证据见COMPILE5-RESULT.md。

## 2026-10-31 编译7续接

用户授权加强增量编译并继续专项；从编译6本地ee72fd6独立恢复。CP29有限标量Types原始结果复用、CP30已发布查询快读、CP31完整会话与源码候选开销窗口开始施工。精确写集和独审分工见 `docs/research/compile-performance/COMPILE7-PLAN.md`；不改变父任务、正式门、schema、语义和预算。CP25仍无生产计量。稀疏历史及handoff基线失败如实保留。

2026-10-31 编译7追加派工：CP29完整会话负结果保留；CP32从a0f38d2b隔离继续，研究有界部分保留、延迟快照复制和已验证源权重复用，64MiB与语义/预算/Verify合同不变。写集、作者和独审见COMPILE7-PLAN。

2026-10-31 编译7追加CP34：CP32完整窗口未建立净收益；从ee72fd6独立研究TypedModule指纹等价流式编码，消除完整编码Vec的临时物化。既有SHA256、V27字节序、预算、Verify权限与错误保持；独立作者/审核及完整成本口径见COMPILE7-PLAN。CP33正式窗口中不得构建或测试。

2026-10-31 编译7追加CP35：CP33完整计时停止；从ee72fd6独立研究fresh Verify共享不可变raw输入，保留原check_module及全部权限/预算/错误合同，仅消除输入深复制。CP34/35各自对基线，不混合归因，作者与非作者独审写集见COMPILE7-PLAN。

2026-10-31 编译7追加CP33：源码确认公开编辑改变逐文件剩余token/node额度从而扩大Parse失效；从ee72独立移植纯内存ParseUsage/预算拟合身份，不含旧磁盘memo。作者incremental_research、非作者cp25_evidence，见COMPILE7-PLAN。

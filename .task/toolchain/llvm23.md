Done
# chore(toolchain): upgrade LLVM to 23.1.4

```json
{
  "id": "toolchain-llvm23",
  "status": "Done",
  "authority": "User-authorized one-time repository-wide toolchain maintenance",
  "branch": "chore/llvm-23-upgrade",
  "baseline": "ad509b5003d9d2a303428f9d01732292988084d8",
  "implementation_author": "/root",
  "independent_reviewer": "/root/llvm_migration_audit",
  "spec_hash": "245cf29a95d721da6268c43191204777a1015639cba56e8e9025aeac012d0a2c",
  "baseline_normative_hash": "4d5480fd3f7de30f6a846489e624ce01bb64d7e835c7e01930bd3410a2206db1",
  "normative_hash": "8b4b3d219999c8f2904b276e051745bf65f9761e421630bdb4bbe0ec2781e02d",
  "normative_change": "Only policy/dependencies.json records LLVM 23.1.4 C API upstream; language spec and interfaces unchanged",
  "target": "LLVM/Clang/LLD 23.1.4; Rust/Cargo 1.99.1",
  "write_set": [
    "crates/zil-llvm-api",
    "tools/llvm",
    "tools/toolchain-profile.json",
    "policy/dependencies.json",
    "policy/llvm-api-ffi.md",
    ".github/workflows",
    "docs/LLVM-API.md",
    "docs/toolchain/llvm23",
    ".task/toolchain/llvm23.md",
    "evidence/toolchain/llvm23"
  ],
  "plan": "docs/toolchain/llvm23/PLAN.md",
  "implementation_commit": "48fa354559be0bc7f6330b388a367e30c7184312",
  "review": "Pass: independent static review, 58 actual limited native tests, and complete acceptance-evidence addendum",
  "mainline_acceptance": "Accepted and merged via PR #7 after exact-head CI and independent review passed",
  "performance": "NotMeasured; compatibility migration is not a speedup claim",
  "ci": "Pass: exact-head run40927910495, completed 2026-10-30 02:22:43 Asia/Shanghai; source tree identical to integration commit",
  "result": "docs/toolchain/llvm23/RESULT.md",
  "evidence": "evidence/toolchain/llvm23",
  "issue": "https://github.com/zil-lang/zil/issues/6",
  "pr": "https://github.com/zil-lang/zil/pull/7",
  "review_receipt": "evidence/toolchain/llvm23/acceptance-addendum.json",
  "integration_commit": "918de0208229010e54183690d61a0453216b7b2a",
  "accepted_head": "c1fcc745e859c19cef83256e8977dde895749a5e",
  "acceptance_record": "evidence/toolchain/llvm23/mainline-acceptance.json",
  "normative_migration": "evidence/toolchain/llvm23/normative-input-migration.json",
  "integration_write_set": [
    ".task/B0.md",
    ".task/T01.md",
    ".task/T02.md",
    ".task/T03.md",
    ".task/T04.md",
    ".task/T05.md",
    ".task/T06.md",
    ".task/T07.md",
    ".task/T08.md",
    ".task/T09.md",
    ".task/W1.md",
    ".task/W2.md",
    ".task/W3.md",
    ".task/W4.md",
    ".task/impl/181.md",
    ".task/impl/208.md",
    ".task/impl/209.md",
    ".task/impl/210.md",
    ".task/impl/211.md",
    ".task/impl/212.md",
    "data/normative-inputs.json",
    "data/task-index.json",
    "data/handoff.json",
    "evidence/toolchain/llvm23"
  ]
}
```

2026-10-30（Asia/Shanghai）：核对远端 main 与编译专项均仍为LLVM17。
用户明确本次属于一次性升级，使用chore/llvm-23-upgrade；验收合入后可删除，
再由长期research/compile-performance吸收新main。不得把CP02 Hold混入本次升级。
本单由集成方维护，不修改其他语言施工工单或回填历史工具链证据。

2026-10-30：升级实现已完成，完整CI默认2743/0/0、参考2711/0/0及其余既有门通过；
真实跨版本缓存与错配控制通过。两次本地参考中断、零选择误调用及首次夹具缺失均保留。
工单保持Doing直到主线集成；不把分支完成写成main已经升级，也不回填历史LLVM17证据。


2026-10-30 10:23:40（Asia/Shanghai）：用户明确授权验收后直接合并并删除升级分支。最终候选完整CI40927910495成功，非作者重新核对原始日志和精确tree。PR #7已以merge commit 918de020合入main，Issue #6自动关闭，本次升级工单由集成方置Done。合入后的当前规范登记迁移单独留证；历史交接缺口不宣称通过。分支删除作为收尾操作单独核实。

# CP17 远端可复现验证入口

本入口在 GitHub Actions 的独立 Linux 作业中构建两个精确源码版本，先完成限定正确性检查，再串行执行现有编译与运行性能工具。编译器构建、Cargo 测试、安装和预检不进入性能样本；被测 ZIL/Rust CLI 的完整进程、LLVM 与链接成本仍由原 runner 计量。

## 触发与源身份

`.github/workflows/compile-performance.yml` 只响应 `research/compile-performance` 上运行 manifest、此入口或该工作流本身的变更，也可手动 `workflow_dispatch`。单纯提交报告、证据或普通源码不会重新触发整套测量；新源码要测量时，应提交新的 manifest。作业只授予 `contents: read`，checkout 不保留凭证，没有写回仓库步骤，复用仓库已有的固定 action SHA。

运行 manifest 固定在 `tools/research/compile-performance/compile4-run.json`，由集成方提交。格式如下，SHA/tree 值必须替换为真实值：

```json
{
  "schema": "zil-compile4-ci/1",
  "run_id": "compile4-round1",
  "base_revision": "FULL_REMOTE_BASE_COMMIT_SHA",
  "candidate_revision": "HEAD",
  "candidate_crates_tree": "EXACT_GIT_CRATES_TREE_SHA",
  "llvm_version": "23.1.4",
  "rust_version": "1.99.1",
  "blocks": 3,
  "seed": 20261030,
  "tests": [
    {"package": "zil-types", "targets": ["project_lookup_cp10"], "lib": true, "features": []},
    {"package": "zil-backend-llvm", "targets": ["closed_executable_cp13"], "lib": true, "features": ["llvm-api"]},
    {"package": "zil-driver", "targets": ["borrowed_inference_cp15", "query_cache_cp12", "closures_frontend", "projects", "impl219_budget", "impl219_selection"], "lib": false, "features": ["llvm-api"]}
  ]
}
```

`candidate_revision: HEAD` 避免 manifest 自身包含自身提交 SHA 的循环；入口仍严格核对实际 HEAD、GitHub 事件 SHA、manifest 原始提交字节和 `git rev-parse HEAD:crates`。baseline 必须是完整 40 位提交 SHA、存在于本仓历史且是候选祖先。候选或 baseline 不符、工作树不干净、必需测试缺失，直接拒绝开始测量。

各测试组明确选择 package、integration target、是否包含 lib tests 和 features；LLVM 相关组必须开启 `llvm-api`，types lib tests 必须包含。运行 `cargo fmt --all -- --check`、workspace `cargo check`、上述限定 release tests，以及现有三套 Python runner 控制。每个选择的测试 target 都必须报告实际执行，零执行不能作为成功验证；格式检查的 diff、错误和退出码同样保留在独立日志及命令收据中。

## 固定测量矩阵

所有窗口保持原五语料和 5%/15%/30%/30%/20% 权重，各三个 ABBA/BAAB 配对区块；没有子集选择或重加权入口。

| 窗口 | 实际配置 | 正式计时行数 |
| --- | --- | ---: |
| 旧新 ZIL O0 桥接 | 双方 8 CGU/4 jobs，source/object 缓存，query 关闭；cold/unchanged/private/public 四请求 | 240 次构建 |
| 候选与 Rust O0 cold | 无编译缓存；Rust 原生 CGU/linker/LTO；候选 8/4 | 60 次构建、60 次运行 |
| 候选与 Rust O0 incremental | 候选 source/object/query，Rust incremental；原四请求权重 | 240 次构建、60 次运行 |
| 候选与 Rust O2 closed | 候选 closed 8/4；Rust 原生 CGU/linker/LTO；无编译缓存 | 60 次构建、60 次运行 |
| 直接 O2 运行质量 | 此次 baseline closed 1/1 对此次 candidate closed 8/4 | 60 次运行 |

合计 600 次完整构建和 240 次原生运行计时，预检及质量产物构建另列。桥接与 Rust incremental 的 query 设置不同，不能拼成同一分数。直接质量窗口重新编译并冻结双方产物，不用旧窗口的 Rust 比值相减推断退化。

编译器安装固定 LLVM/Clang/LLD 23.1.4 与 Rust 1.99.1；Rust 自带 LLVM 的具体版本由实际 `rustc -Vv` 留存，不声称与 ZIL 使用同一个 LLVM DSO。CPU affinity 从 runner 实际允许集合选至多四个，运行窗口选其中一个；不宣称 GitHub 物理宿主完全独占。

## 执行与留证

在完整、已提交的候选 checkout 中先核对 manifest：

```sh
python3 tools/research/compile-performance/compile4_ci.py --validate-only
```

手动复现需要先完成指定工具链安装；输出和工作目录必须是两个不存在且互不包含的新目录：

```sh
python3 tools/research/compile-performance/compile4_ci.py \
  --out /tmp/compile4-evidence \
  --work /tmp/compile4-work \
  --llvm-prefix /path/to/llvm-23.1.4 \
  --budget-seconds 3600
```

两个编译器分别使用独立 target，保存 CLI 二进制与源提交、完整构建命令/日志和 SHA-256 后才删除本次创建的 target；不删除源码、预检程序、原始计时或日志。入口保存每条外部命令的 argv、cwd、退出码、耗时、日志哈希，以及各冻结计划、输入/oracle、原 runner 摘要和逐文件清单。Actions 将证据打成保留二进制权限的 tar.gz，再连同安装收据及 setup 日志上传，避免上传 Cargo 缓存和大量零散文件的开销。

脚本时间上限 60 分钟、作业上限 75 分钟是失败保护，不是编译性能承诺。任何错误、超时或未知状态都保留原始材料，不重试覆盖原目录；整体标记 `IncompleteOrFailed_NoScore`，已完成的独立窗口仍按其原始身份留存。全部窗口完成只标记 `Complete_Exploratory_NotAProductGate`，不自动接受 CP14/CP15，不将稳定运行退化用加权收益抵消。独立复算、逐语料运行质量裁决和正式产品门仍由集成方执行。

本入口只改善实测与复现能力：未改安全核、语言表达、运行实现或既有量化门，也不构成新的 Rust 编译优势。真实远端是否通过必须引用具体 Actions run、源提交及该次 artifact；本地语法检查或调度控制不能替代远端实测。

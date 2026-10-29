# research/compile-performance：ZIL 编译性能专项

## 研究身份与范围

用户于2026-10-29明确要求独立环境、跟随 GitHub 最新版本、长期研究分支并阶段合流。
远端主线基线：`ad509b5003d9d2a303428f9d01732292988084d8`，tree `fec01a1f44bd710a2f131122041708f977683206`。
标题与远端分支均为 `research/compile-performance`。本轮独立检出，不续写旧会话工作树；本机工具链仅只读复用。
历史212的量化结果不移作本轮结果，当前主线已经具备直接 LLVMBuilder 与 opt-in 本地对象缓存。

这是用户明确新开的长期专项，其协调权威记录为本分支 `.task/research/compile-performance.md`；实验使用独立任务分支/worktree/target，不争用语言线的数字切片编号，不修改远端 main 或语言施工状态。正式父任务 B0 仍 Doing。既有全局 handoff 在基线即失败（1138项历史记录问题），不伪称本专项修复或通过；其定位结果及摘要作为恢复证据保存。

## 本轮派工

| 实验 | 作者 | 写集 | 目的 |
|---|---|---|---|
| CP01 | /root/frontend_audit | crates/zil-db/src/lib.rs、src/tests.rs、crates/zil-db/examples/compile_performance.rs | 精确反向依赖索引替换失效时反复全表扫描；不改变read/预算/每请求Verify |
| CP02 | /root/backend_audit | crates/zil-driver/{src,tests}、crates/zil-backend-llvm/{src,tests}、crates/zil-llvm-api/{src,tests}、crates/zil-cli/{src,tests} | 显式可选阶段墙钟诊断，定义串行父子范围与并发unit范围；默认关闭 |
| CP03 | /root/compiler_research | tools/research/compile-performance/**、docs/research/compile-performance/SOURCES.md | 冻结语料/二进制/次序、端到端ABBA/BAAB与归因；不修改旧性能门/212脚本 |

CP01与CP02写集不相交。先审CP01候选，独立测其真实收益；CP02单列测量，不把探针成本藏入候选、不混同两项收益。所有实现提交须经非作者审核，报告和结果绑定精确提交/源码树、编译器二进制和输入摘要。禁止额外并发构建污染正式计时。

## 不变条件

规范依据 `_zil_design.md` §19.1—19.5；源语言、IR/schema、ABI、优化质量、默认CGU/工人数、逻辑预算、Verify独立性与失败清理保持。不得使用旧Verified资格、改变cache信任模型、吞掉错误或取消。根Cargo/锁与工具链由协调者单写；本轮LLVM升级是另行待测项，不把LLVM17冒充最新。

## 冻结与验证

先保存基线和候选独立二进制；CP01先独立旧扫描oracle、依赖替换/失败/多指纹/菱形/clear控制，再运行受影响driver测试。主测采用完整CLI进程墙钟和进程树CPU；check/build/O0/O2、冷/未变/私有/公开、单CGU/并行CGU分开。原生正向结果仅证明所测消费者行为，不代表Rust同等完整语言安全。

n/2n/4n的数据库探针只作算法归因，不能替代完整请求成本。基准计划在测量前提交并冻结；任何失败、退化和计划修订均保留，不选掉不利单元。共享宿主无法保证物理独占，结果按探索性报告。Rust比较只在相同任务边界和明确版本/选项下另开窗口；不得引用历史比率乘算。

## 长期合流策略

2026-10-29阶段裁决：CP01/CP04均NoGo并回退；CP02研究可用，但默认关闭对main的build点估计有正成本、探索区间跨1，生产合流Hold。下述合流流程只在后续阶段明确通过后执行，不把研究分支存在视为已经接受。

保留 `research/compile-performance` 作为经过审核的可合流研究线；每个实验在临时task分支，未通过实验不保留在当前可合流的生产树。为保存可复现的负结果，允许候选与回退成对进入研究历史，发布点必须完成回退并给出裁决。阶段通过后，用户可用 Merge commit 将长期分支合入 main，随后将最新main merge回长期分支；双方保留祖先关系，长期分支继续存在。临时任务分支合完可删。

对这条持续分支不用 squash/rebase 合流，不force-push已分享历史。阶段报告按提交保存在 `docs/research/compile-performance/` 和 `evidence/research/compile-performance/`；研究仍可按路径/提交追溯，分支名只是持续工作入口。若整阶段有未通过实验，只单独提交结果及保留候选引用，不把未验代码混进用户下一次合流。

## 四目标边界

本轮只提交编译成本相关的实现和证据。安全保持现有独立核验但全语言安全尚未证明；运行性能必须控制相同机器行为/优化配置；语法与维护未更改；总体超过Rust及至少一项产品级突破均未建立。任何局部收益不升级产品门。

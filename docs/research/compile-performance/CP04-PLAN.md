# CP04：O0 标准 SSA 提升实验合同

状态：Doing，尚无性能裁决。协调权威为本分支 `.task/research/compile-performance.md`；作者 `/root/frontend_audit`，非作者审核 `/root/backend_audit`，运行质量测量工具 `/root/compiler_research`，正式计时由根串行协调。

## 假设和边界

CP02 的单次大语料诊断中，完整请求约 3.043 秒，LLVM object emit 约 0.986 秒，机器 IR 约 0.291 秒，Verify（包含输入复制）约 0.095 秒。这是定位信息，不是稳定性能估计。LLVM 17.0.6 的 O0 已自动启用 FastISel，因此不实施“补开启 FastISel”。

当前 lowering 将普通局部变量写入入口块 alloca；既有 tree1024x8 优化前 IR 的 10,246 个 alloca 全部符合保守的直接标量 load/store 形状。只据此建立假设：LLVM 标准 mem2reg 可能减少后端对象发射工作量，其自身支配树/PHI 成本也必须计入总编译时间。静态计数不是实际提升量，生产 `.ll` sidecar 是优化前表示。

本实验明确修订原“优化配置相同”的实验前提：只改变 O0 的准备 pass，比较的是旧 O0 配方与新 O0 配方。TargetMachine codegen optlevel 仍为 0；O2 配方与级别不变，不同时改 SROA、InstCombine、SimplifyCFG、CGU、workers、LLVM 版本或语言预算。不能以降低运行质量交换编译速度。

## 写集与实现

独立任务分支 `task/cperf-o0-ssa`，源码基线 CP02 `9614aac90ee58359fef05da4dcfe92011b7c91c2`，不包含 CP01 已否决索引。

- `crates/zil-llvm-api/src/native.rs`：O0 `default<O0>` 改为 `default<O0>,function(mem2reg)`；O2 `default<O2>` 不变。
- `crates/zil-llvm-api/src/lib.rs`：仅允许共享实际 pass 配方 accessor，避免 backend 缓存配方与执行字符串分叉。
- `crates/zil-backend-llvm/src/object_cache.rs`：按现有精确协议更新 namespace 版本，显式包含完整 pass 配方；保留编译器及 LLVM 实际身份和全部原配置。
- 以上模块的必要单元测试及新增 `crates/zil-driver/tests/o0_ssa.rs`：循环/合流、实际地址用途、清理和陷阱差分；可复用既有 fixture，避免复制全量门。
- `tools/research/compile-performance/runtime_bench.py`、对应测试与 README：独立运行质量协议；不覆写 CP01/CP03 原始结果或聚合语义。

保留独立 ZIL Verify、LLVM pass 前后 Verify、每请求现行核验、取消点和错误类别、逻辑预算、缓存信任边界及产物发布顺序。不得给地址用途增加不真实的 noalias 信息，不得自制删除 alloca。`receipt.pipeline` 必须记录实际执行字符串；`llvm_optimize` 应包含提升的真实成本。真实可执行 identity 已隔离不同构建，显式配方进一步使缓存决策可审计。

## 限定验证与裁决

先用同版本 opt 对既有优化前 IR 做独立诊断，记录实际 pass 后指令数；此文本路径只用于归因，不替代生产 Builder 验证。运行作者定向测试、非作者源码审核，根独立执行 backend/driver 影响闭包。保留所有失败和修订。

完整编译比较用 CP02 对 CP04，两端 `--timings` 关闭，O0、1 CGU/1 worker、对象缓存均关闭。沿用五语料与权重、四请求序列，三 ABBA/BAAB 配对区块，只测受影响的 build（240 请求）；另用 O2 做未改配方行为控制。冻结实际二进制、源码树、输入、脚本和命令。不得与 CP01 或旧 212 窗口混合。

运行质量用五个 base 语料和同一独立输出 oracle，编译在计时外，双方输入逐字相同，记录二进制大小及哈希，固定 affinity，配对多区块。为减少进程启动占比，各语料预先固定较长输入；不得看到候选结果后选择有利输入或丢弃不利单元。语义失败直接 NoGo；未证明编译收益则 NoGo；出现无法解释的稳定运行退化也不晋升。共享宿主、有限语料只支持阶段研究裁决，不授予产品通过状态。

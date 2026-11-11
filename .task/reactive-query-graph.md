# 响应式编译查询图专项：真实生产接入

父范围：goal G10.5.1.15。分支：`research/reactive-query-graph`。本轮实读 main `99035931d358add0eaed6d4535035c38413701dd`，接续源 `c75c58bf7f564a1ce8049702f9c6ec41c82d1da2`，无需变更基线。其他编译分支保持原样；本轮 goal 认领读回 `a67f6778e1696991cf4b45c8eb12ba2efa86ff43`。

## 当前切片与真实消费者

本轮生产增量：zil-db 的稳定 owner 图被 zil-types FunctionSession 真实调用，driver 的 `reactive_queries` / CLI `--reactive-queries` 显式选择。按包/源模块路径/函数名区分节点，compact ModuleId/FunctionId 仍在完整绑定有效性中；不删除旧 QueryKey.input。实际 inputs 是完整上下文、声明签名和经当前不可变 CoreFunction 全量 Eq 证实的正文版本。批量安装 inputs 后按高度求函数 BodyKey 投影，读取实际签名依赖，接现有 CFG/cost/span-rebase 缓存。旧内容键/磁盘 work product 保留。调度图不能携带 Verified 权限。

该切片减少可复用函数的正文编码与调用查表；Eq 比较、完整 header/context 构造、Expand、形状检查和后续分析仍存在，不能宣称所有整图工作消失。原 scalar admission 之外仍回到既有完整路径，mixed/trait/CT/resource消费者用于组合控制而非宣称全部细粒度覆盖。源码路径只标识 owner，不单独证明有效性；raw source_fingerprint 不授权复用。普通递归通过签名输入，不误报查询环；尚未把所有真实递归固定点拆入外层图。

失败可抛弃加速图，保留原内容缓存；driver checkpoint仅共享raw entries Arc、不深克隆整图。预算/首诊断/当前Verify保持；没有缓存接受权限。新增phase timing与每请求真实Types/初始化工作计数，whole-project命中输出null而非旧统计。

## 写集与协调

认领driver session/project/lib及真实差分测试、Types incremental、zil-db/reactive、CLI flag、tools/research/reactive-query-graph测量器。root单写新增Types→zil-db一条内部依赖：crates/zil-types/Cargo.toml、Cargo.lock、policy/dependencies.json对应边。根Cargo.toml、公共IR/schema、Verify内核及后端未改；policy其他边及LLVM unsafe字段不改。goal独立提交、不合入main。

## 已实际核验

- 当前环境可复用：`source /workspace/scratch/abf690ce880c/toolchain/env.sh`；Rust1.99.1(58cf8f9c35b355199727627d07ae34a4e2846d67)、LLVM/Clang/LLD23.1.4，仓库checker Pass。不是沿用前轮缺工具链结论。
- 独立baseline worktree `/workspace/scratch/e28839ace920/baseline`，target `/workspace/scratch/e28839ace920/targets/baseline`；release成功，binary SHA256 `64e1ff43bbaa59f786f1cc899b69e707072219a9c0abf0122ce31cd7b77f981d`。
- baseline真实ZUI语言StoreFields check18.823ms，O2单CGU build115.237ms，输入ASCII3输出DEFHZMNOPK/exit0，两次Verify1。属于环境烟测。
- 原始zit.ts已取得，SHA256 `8ac65b2ce90c32d1bee9aebe480e2f51bfa560b6f7e9d2633a91fa2732ab6bfb`；原ZIP SHA256 `1ca89da7c6535d6026697866fedae7403e8de2195f040f81f5026e22f29e6ce5`。22个manifest文件及三份冻结结果hash全匹配。私人原始源码没有放入仓库。
- 最小mixed生成Source全13变体：同baseline双臂26请求（24success+2正确SafeReject）、144native oracle通过。仅测量器正控制，不是性能证据。
- default预算规模筛选：mixed14×14×16 check888.50ms；scalar20×20×16 check936.16ms，均Verify1。mixed16×16×16、scalar32×32两规模原CT/词法预算失败保留，不进入成功速度分母、不放宽限额。

## 本轮验收状态

当前是WIP生产接入，候选尚待精确编译、真实差分、配对性能、非作者独审。旧JS模型与旧模型数字保留于README/receipt，不是当前编译成绩。尚未证明大项目用户等待下降，不合入main、不删除其他分支。

真实原始日志暂位于baseline的 `out/runs/reactive-baseline-{build,smoke}-20261111` 及本树 `out/runs/native-workload-{probe,scale}-20261111-r*`；结束前必须补持久化索引，目前不冒称这些临时路径已备份。

## 下一条恢复命令

读取远端本分支最新SHA与goal G10.5.1.15；创建固定候选SHA的独立validation worktree/target，然后：

```sh
source /workspace/scratch/abf690ce880c/toolchain/env.sh
export CARGO_TARGET_DIR=/workspace/scratch/e28839ace920/targets/reactive-validation
cargo test --locked --offline -p zil-db -p zil-types -p zil-driver --lib -j 4
```

先解决真实失败，再执行native_measure.py同一候选binary默认legacy/--reactive-queries、相同O2/cache/CGU配对。完整退出条件仍见前轮合同 `e31e7ece:.task/reactive-query-graph.md` 与本轮用户请求；不能以此有限切片关闭父门。停止回复后不承诺后台施工。

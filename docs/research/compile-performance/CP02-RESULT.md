# CP02：可选完整阶段诊断

状态：实现及限定测试、非作者静态审核通过；不声称性能提速。生产源码提交 `9614aac90ee58359fef05da4dcfe92011b7c91c2`，crates tree `2d3bdfe9336f666508e2ba88218480c8e3aa36ea`。集成的 crates 逐字相同，映射见 evidence 下 source-map。

## 使用及记账

CLI 增加 `--timings`，`CompileOptions.timings` 默认 false。关闭时没有新增 trace 状态分配、取时或锁；默认 JSON 增加 `timings:null`，并非字节兼容不变。打开后返回共享单调墙钟起点上的 span，包含 id、parent、name、kind、unit_index、start_ns、wall_ns、outcome。

范围是编译 API 请求，包含源码发现和 Session 容量清理，不含进程启动和报告序列化；完整 CLI 基准仍另测。父 span 包含子 span，并行 unit 相互重叠，不能相加当成总墙钟或 CPU。所有阶段包围真实调用取时，不用总量相减捏造阶段。旧 `report.seconds` 维持原起点，新根 span 才覆盖以前未计入的工作。探针开销在外层 span 中，阶段数字不当作关闭探针后的稳定成本。

覆盖 source read/parse/discovery、query identity/execute、expand/types/ct/analyze/独立 Verify/lower、machine IR、LLVM 文本渲染、unit 切分、缓存准备/键/查找/命中预检/写入、Builder、前后 LLVM Verify、pass、object emit、link、sidecar read、发布校验与缓存提交。单 CGU Clang 路径仍为真实组合的 `clang_compile_and_link`，不伪拆。

## 保持的约束

timings 不进入 query/action key，不授予核验资格。每请求独立 Verify、缓存 machine preflight、错误类别、取消点、确定性 worker 错误选择、产物发布与缓存提交顺序不变。RAII 在失败或展开栈时收口 span，不把未完成工作标成成功。

## 有限验证

作者在精确最终源码上执行 driver 阶段测试 5/5、CLI 阶段测试 2/2、LLVM API timing 单元测试 2/2，以及 release 构建；覆盖真实 O0 原生执行、缓存命中、机器 IR/.ll 一致、返回值、链接失败旧产物保护、query 身份、每次 Verify、实际超容量 Session 清理及取消/错误无 running span。

`/root/frontend_audit` 非作者审核完整 diff 与上述日志，结论 Pass，但没有冒充独立重跑。早期 `pub(super)` 在 crate root 导致 E0433 的失败日志保留，最终一行改为 `pub(crate)`；不删失败过程。未运行全 workspace 或产品级门。

## 单次阶段定位

同一 release 编译器、LLVM 17.0.6、O0、1 CGU/worker，现有五语料中抽取大样本和真实消费者做诊断：

| tree1024x8 build span | 墙钟毫秒 | 说明 |
|---|---:|---|
| compile_path | 3043.192 | 请求根范围 |
| source_discovery | 127.319 | 根的前端子范围 |
| verify | 94.587 | 包含现有输入复制，不能当成纯复制成本 |
| machine_lower | 291.121 | backend 子范围 |
| llvm_builder | 192.824 | llvm 子范围 |
| llvm_optimize | 17.884 | 原 O0 pass |
| llvm_object_emit | 985.865 | 当前最大的明确后端子范围 |
| link | 101.814 | 含链接进程 |

小型 retained build 总计 44.903ms，其中 link 32.439ms。不同规模瓶颈不同。以上为各一条诊断记录，共享宿主和缓存状态未构成配对性能窗口；大 check 的另一条记录不能拿来与 build 内前端阶段直接比较。

下一实验 CP04 只测试 O0 标准 mem2reg，先减少后端不必要栈槽。直接共享 Verify 输入的设计暂缓：现有整个 Verify 范围只占该大 build 约 3%，尚未隔离复制成本。原始诊断 JSON、工具/输入/output 摘要、作者日志及独立审核见 `evidence/research/compile-performance/cp02/`。

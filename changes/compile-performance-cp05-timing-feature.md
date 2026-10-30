# CP05：按构建能力选择阶段诊断

LLVM23研究版默认移除阶段计时采集器；`timings` feature沿CLI→driver→backend→API显式启用。普通构建保留JSON `timings:null`，不改变源语言、Verify、预算、对象缓存或机器产物。

变化仅影响显式诊断请求：未编入时CLI `--timings` 返回已有 `E-CLI-ARGUMENT`（64），库 `CompileOptions.timings=true` 返回driver阶段Unsupported；不能返回成功的空测量。启用方式：`cargo build --release -p zil-cli --features timings`。

原阶段时钟父子范围和失败记录保留。原测试在feature开启时全部执行，默认路径新增所有driver入口及CLI的明确拒绝测试。零尺寸、无Drop只是工程性质，性能结论仍由本轮冻结完整CLI对照决定，不据此宣称零成本。

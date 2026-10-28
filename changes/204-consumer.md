# 204 真实消费者与原始失败记录

五模块源码位于 `examples/mainline/store_captures204/`；独立 runner 和文本证据收集器位于 `tools/language/204/`。这是一份新消费者，203 原源码、oracle 和结果均不改写。

限定范围包括 Fn 的实际拥有子 Closure、FnMut 捕获完整含 Store Package、FnOnce 实际 TakeCapture、两同型内层 Store 不同拥有值、外层 State/Store、普通完整 T helper、元素窗口、0/1/continue/break/满 push/pop、give 前后申请失败清理。另有短源 `impl204_analyze_capture` 用 fallible 参数检查 callee 已准备但 body 未进时的 Store 环境责任，与已 give 后局部清理作可观察对照。

当前仅提交源码和执行前 oracle。编译、Linux/Wine、申请失败矩阵尚未执行，未将计划数记为通过。成功或失败都按实际冻结 source/CLI/命令独立追加；任何新生产变更必须换匹配 CLI，不能倒标旧运行。

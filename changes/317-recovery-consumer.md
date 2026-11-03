# 317 普通组件 API 恢复组合

以现存 299 主线 `f27a9330d` 重新施工，不继承无法恢复的 301–312
源码或机器成绩。只增加 `examples/mainline/recovery317` 和已有
`generic_regressions` 测试目标中的消费者，不修改编译器、schema、预算或产品门。

组件以普通库 API 连接三个前置：313 的有限 Copy 元组普通绑定、314 的
Type/Syntax 与纯 Copy 泛型、315 的普通局部完整 Product 可变 helper。
资源 Product 暂按 315 明确支持的 Mark 形状，不把 Store 资格当作已开放。
运行时使用已有 `c_with_callback` 登记—同步运行—关闭窗口，回调拥有资源，
调用者在窗口关闭后再释放回调环境。成功、业务错误和登记失败分别核对清理。
近邻拒绝覆盖普通 Product 活跃共享窗口与可变 helper 冲突、外部窗口中的
回调别名以及错误实例身份；非法输入不运行。

按固定依赖提交在本分树组合后，限测现有目标；实际命令、机器 stdout/退出码
与未完成项在交付时补记。仅声明当前源组合与 O0/O2 行为，完整语言、ZUI、B0、
原生 Windows 及 Rust 产品四目标不因本单晋升。

启动 `node tools/handoff.mjs --check` 实际 Fail：历史记录及实施工单摘要过期，
工具把当前分树识别为 main。施工权威按父派工指定 `/workspace/zil` 主树，
不修改交接元数据；Fail 不作为语言正确性成绩。

基线限定命令（`source /workspace/.zil-env/env.sh` 后显式
`CARGO_TARGET_DIR=/workspace/.zil-env/target317 CARGO_BUILD_JOBS=1`）：
`cargo test -p zil-driver --test generic_regressions recovery317_component_source -- --exact recovery317_component_source_and_nearest_rejections --nocapture`。
实际 0 passed / 1 failed / 14 filtered；消费者在 CT 阶段 `E-CT-SOURCE`：
`CT function must be a concrete ordinary function instance`，尚未到 Verify/native。
这保留 314 前置的实际缺口，不改成常量绕过。

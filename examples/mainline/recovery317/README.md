# 317 组件语言前置组合

这是一组普通库 API 消费者：`config` 提供编译期泛型类型与纯记录配置，
`component` 提供拥有资源的 Panel、事件解码和可变更新 helper，`bridge`
封装已有结构化外部回调窗口，`main` 完成一次组件更新和关闭。没有窗口框架。

同一 `value_type<T>` 分别实例化 i32/i64，结果别名实际约束后续值类型；同一
`Config<T,P>` 保留两个实际类型参数，P 为 phantom，错误 phantom 实参另有近邻。
事件返回嵌套 Copy 元组，拆绑定只求值一次（宿主看到恰一个 `E`），且分别
检查 first=2、second=3，避免交换投影仍得到相同累计值。Panel
由 body 局部拥有；两个 `apply(&mut panel, event)` 更新同一完整 Product。
body 环境和 handler 环境分别拥有 `B`、`H` 两个资源。关闭期间宿主再调用
trampoline，要求返回关闭哨兵且不执行 handler；返回后 handler 仍可再次调用。

`host.c` 是显式外部事件库及清理观察者，不承载编译生成的 ZIL 行为。
它断言每个资源只析构一次、Panel/body 先于关闭、handler 晚于关闭；null
登记时 body 不执行但其环境仍析构。业务错误在关闭返回后传播为普通错误联合。

| stdin | 退出码 | stdout | 行为 |
| --- | ---: | --- | --- |
| `0` | 0 | `REVSPBCAH` | 登记、事件、派发、成功、Panel/body 析构、关闭、返回、handler 析构 |
| `1` | 1 | `REVFPBCAH` | 同一路径业务错误，完整清理后传播 |
| `2` | 2 | `RNBAH` | null 登记，不运行 body，释放 body 环境和 handler |

`negative` 保留四个最近反例：活动共享 Panel 窗口与可变更新冲突、
调用者保有同一 handler 别名、同布局不同名义记录误传给 CT 配置函数，以及同一
声明的错误 phantom 实参。它们只编译拒绝。`controls` 中两个对应窗口末用
后恢复的正控必须到达 Verify；各反例须锁实际阶段及诊断，不能接受任意前置误拒。
`observed/unsupported_callback_address_capture.zil` 保存初次写法：地址进入 body
捕获先在 Types 返回 Unsupported，不能充当活动调用者别名冲突的证据。

复用已有目标，先加载环境并选择独立 target、单 job：

```sh
source /workspace/.zil-env/env.sh
export CARGO_TARGET_DIR=/workspace/.zil-env/target317
export CARGO_BUILD_JOBS=1
cargo test -p zil-driver --features llvm-api --test generic_regressions recovery317_ -- --nocapture
```

上面的显式 feature 同时核 Clang 参考后端与 LLVM API，均执行 O0/O2 的全部三种输入；
不启用该 feature 时只运行 Clang。机器输出需与上表逐字一致，编译必须实际经过独立 Verify，
不允许后端 fallback。`ZIL_TEST_REPORT_DIR` 可保留编译报告、阶段 IR 与机器结果，
默认临时目录在成功后删除。恢复基线和依赖组合后的实测见
`changes/317-recovery-consumer.md`，不能从此处预期表推定已经通过。

此组合只覆盖有限同步生命周期；完整 ZUI、长期返回登记、异步/并发、原生
Windows、Rust 性能/内存和维护成本比较不在本单验收范围。

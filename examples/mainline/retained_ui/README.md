# 返回并保存真实环境的同步事件内核

五个 ZIL 模块将工厂、调用 helper、事件校验和 I/O 分开。`factories.counter` 返回 `impl FnMut(i32) -> i32`；工厂的临时资源 F 先销毁，拥有资源 A 随环境返回，调用者在多轮输入之间保存同一个环境。`reader`、`once`、`fallible` 和 `snapshot` 分别覆盖拥有 Fn、消费 FnOnce、错误回调和 Copy 快照。

运行 `zil build examples/mainline/retained_ui/main.zil -o /tmp/retained-ui` 后，可以输入 `l++-q`。预期输出为 `F0\n1\n2\n1\nA`：F 表示工厂已退出，A 直到调用者退出才释放。输入 `p` 则在较晚实参失败时输出 `BCDZ` 并退出 1，已准备资源与拥有环境仍按责任次序清理。

`crates/zil-driver/tests/impl070_retained.rs` 编译真实 ZIL/LLVM O0、同一 LLVM O2，以及独立编写的 Rust O0/O2，对 20 个输入逐一核对输出、退出码和空 stderr，共 80 次机器执行。源码 test 同时检查五个公开工厂合同均经过 Verify 发布；两字段清理、空清理和外部依赖为空均有检查。私有 `retain(give guard)` 还核对拥有输入转入返回环境的机器责任。没有生成 Rust 来实现 ZIL。

| 输入 | 含义 | 输出 | 退出码 |
|---|---|---|---:|
| `s` | 拥有 Fn 返回、借入调用、移动、再调用 | `2121B` | 0 |
| `z` | 返回后从不调用 | `ZB` | 0 |
| `m` | 同一个 FnMut 环境跨 helper 和调用方更新 | `F23A` | 0 |
| `0` / `1` | FnOnce 零次 / 一次调用 | `CD0` / `CD1` | 0 |
| `p` / `P` | 较晚实参错误 / 成功 | `BCDZ` / `HCDB3Z` | 1 / 0 |
| `e` / `f` / `E` | 第一次 / 第二次回调错误 / 全成功 | `1E` / `12E` / `12:3:33E` | 1 / 1 / 0 |
| `c` | 不借用工厂局部的 Copy 快照 | `42` | 0 |
| `g` | 拥有实参进入工厂并随环境返回 | `77G` | 0 |
| `l...` | 最多 32 次同步事件的保存和调用 | 按输入变化，最后释放 A | 0 或 1 |

环境沿现有内联 struct 按值返回，不引入隐式堆、引用计数或锁。此次行为对照没有测相对 Rust 的运行时间或内存峰值，也不是平台窗口、GPU、异构 callback 容器、异步、注销排空或完整 ZUI。

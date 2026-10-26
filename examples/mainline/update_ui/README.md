# 组件字段更新与资源生命周期

这里推进的是 ZUI 所需的同步组件资源更新内核：真实 ZIL 源码经过 Types、Analyze、独立 Verify、LLVM 和机器执行。尚无平台窗口、GPU、字体、异步调度或通用容器；不是完整 ZUI。

`main.zil` 经 `window/events/io` 四模块运行有界事件环。`+/-` 修改计数并重绘，`r` 替换 Surface，`t` 取走 Surface 交给函数制造新 Surface 后补入，`b` 交替执行先取走/仍保留旧 Surface 的分支，再在汇合处补入。更新期间 Model 的兄弟字段 Desc 保持可用。`f` 在新 Surface 准备时失败，旧 Surface 仍由应用负责清理。`=` 不修改，空白跳过，`q`/EOF 正常关闭；最多读取128字节。

每次绘制打印计数，退出打印 `#帧数`。旧 Surface 释放打印 `S`，应用退出的 `MSU` 对应 Model、Surface、Subscription 声明顺序。完整包仍可跨函数参数和返回；部分包只留在局部，补齐后才能整体转交。

## 精确清理观察

`lifecycle.zil` 的三字段包初始拥有 A/B/C，外层资源为 Z。

| 输入 | 退出码 | stdout | 覆盖行为 |
|---|---:|---|---|
| `0` | 1 | `ABCZ` | 新资源尚未准备，旧字段保留到退出 |
| `1` | 1 | `DABCZ` | 新资源 D 已准备，晚实参失败先清 D |
| `2` | 1 | `ABCZ` | A 已取走进入实参临时；失败清 A，部分包只清 B/C |
| `3` | 0 | `ADBCZ` | 先清旧 A，再提交 D，退出清 D/B/C |
| `4` | 0 | `BACZ` | 取走 A/B，交换补入，再完整跨函数返回 |
| `5` | 0 | `CBAZ` | 取走全部拥有字段，Copy 字段仍可写；局部资源逆序清理 |
| `6` | 0 | `ADBCADBC` | 分别执行 LIVE/MOVED 两种汇合路径并补入 |
| `7` | 0 | `ABCDBCZ` | 三次循环取走/制造/补入，旧实例逐轮释放 |
| `8` | 0 | `ABCZ` | 同字段取走再回填，不重复释放 |
| `9` | 1 | `ADBCZ` | D、取走的 A 已成为前两个实参；晚实参失败逆序清理 |

`fd.zil` 使用真实 Linux `dup/close`，观察替换后的旧句柄、取走后离开作用域的句柄、部分包退出的剩余句柄均已关闭；全部符合才打印 `F`。它不是只靠字符日志推断资源关闭。

三个入口各有独立手写 Rust 对照，使用普通值、显式 `Option::take` 和 Drop 表达行为，不读取 ZIL IR。`impl074_packages.rs` 固定20组输入，比较 ZIL/Rust 各 O0/O2 的退出码、stdout、stderr；另检查 raw/wire CT 拒绝和修改过的 LIR 不得复用旧验证凭证。运行结果是当前有限语义的行为证据，不是运行速度比较。

## 表示与边界

完整 SSA/ABI 仍是原来的内联 struct。只有出现字段取走、清理或写入的局部包才在函数入口分配直接 Managed 字段的责任 flag；每轮循环重用固定栈槽。Whole Store 建立全部责任，Whole Take 清除全部；字段 Take 只清目标，字段 Drop 条件清目标且先清标志再调用用户 teardown。包退出按声明顺序清仍存活字段。字段 flag 不进入函数 ABI，也不需要堆盒、RC、锁或全局世代表。

当前范围是直接 Managed 字段和有限 Copy 字段，包无整体自定义 Drop。一般嵌套拥有包、包含拥有闭包的包、动态成员集合、拥有错误载荷和自定义整体析构后的拆分仍待后续合同。旧 Desc 的未来使用必须在修改该字段前结束；兄弟字段不受牵连。准备失败不撤销 RHS 已发生的副作用；当前 teardown panic 沿用 abort 行为。

```sh
cargo test --locked --offline -p zil-driver --test impl074_packages
cargo run --locked --offline -p zil-cli -- build examples/mainline/update_ui/main.zil -o out/update-ui
printf '+r-tbbq' | out/update-ui
```

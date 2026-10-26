# ZUI 前置：完整拥有包与有界事件环

这里是 ZIL 语言主线的真实多模块应用，覆盖已有编译路径与本轮完整 `package` 的组合。它是同步、单线程、有固定输入上限的文本渲染内核；尚未接入平台窗口、GPU、字体、异步执行或跨帧保存的捕获回调。

## 程序与观察

| 入口 | 运行工作 | 独立观察 |
|---|---|---|
| `main.zil` | 四模块事件分发、状态更新、dirty 重绘、字段 Desc 本地回调、Window 整体移交/参数/返回 | stdin 驱动；每帧打印计数，正常结束打印帧数；最终 `MSU` 分别是 Model、Surface、Subscription 的声明顺序清理 |
| `lifecycle.zil` | 三资源包乱序准备、四个准备失败点、较晚实参 `?`、正常消费、continue/break | `0..5` 与 `l` 分别锁定准备与完整清理，详见下表 |
| `fd.zil` | 两个真实 Linux `dup(0)` 描述符放进包，跨函数往返，离开作用域 | 分别再次 `dup` 原句柄，均须失败；全部符合才打印 `F` |

事件入口接受 `+`、`-`、`r`、`=` 和 `q`；空格、CR、LF 跳过。`+/-` 在 -50..50 内修改状态，`r` 强制重绘，`=` 保持当前状态，`q` 或 EOF 正常关闭。最多读取128个输入字节；越界或未知事件通过 `E!i32/?` 返回错误并清理完整应用。stdin 是现有显式 C ABI 边界，输出错误和真正平台事件尚未建模。

包本身按普通非 Copy Value 移动：`frame(app, decode(code)?)` 先把包存进拥有实参临时，后面的 decode 失败也能准确清理。`frame` 读取字段描述，并把 Model 的描述捕获到局部 `view`；最后一次使用结束后才允许返回整包。跨模块 API 保留包的名义形状及私有 manages 字段。

| lifecycle 输入 | 预期退出 | stdout | 含义 |
|---|---:|---|---|
| `0` | 1 | `Z` | 第一个资源尚未成功，只有外层资源 |
| `1` | 1 | `CZ` | 只准备了 C |
| `2` | 1 | `ACZ` | C 后 A 成功，按逆准备顺序清理 |
| `3` | 1 | `BACZ` | C、A、B 成功，但最后 Copy 字段失败；仍未提交完整包 |
| `4` | 1 | `ABCZ` | 包已完整并经过 relay；后续实参失败，按包声明顺序清理 |
| `5` | 0 | `XABCZ` | finish 真正执行后清理 |
| `l` | 0 | `ABCABC` | continue 和 break 各清理当轮完整包一次 |

每个入口都有独立手写 Rust 对照，直接描述相同资源与行为，不读取 ZIL IR。`crates/zil-driver/tests/impl060.rs` 使用实际源码→Types→Analyze→独立 Verify→LLVM→native，在 ZIL/Rust 各 O0/O2 上比较退出码、stdout 和 stderr。该对照验证当前有限语义，不是运行速度或内存基准。

## 运行

从项目根目录使用已配置的 Rust / LLVM 工具链：

```sh
cargo build --offline -p zil-cli
target/debug/zil build examples/mainline/package_ui/main.zil -o out/package-ui
printf '++r-q' | out/package-ui
cargo test --offline -p zil-driver --test impl060
```

`Type::Package` 使用内联字段加单个完整责任标志；字段 Desc 指向本地包中真实资源头。这里没有部分完成的公开包对象：先用现有真实 owning locals 守护已求值字段，全部成功后无失败提交。机器清理先清除整体责任标志，再按声明顺序调用各字段 teardown。一般部分移出、replace、嵌套包、拥有错误载荷、包内闭包保存和包整体自定义 Drop 尚未开放。

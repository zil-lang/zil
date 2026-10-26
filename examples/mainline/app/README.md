# 多模块批数据应用

应用在编译期准备 8 条数据，验证每条数据，按行号选择变换函数，用可变闭包积累统计，然后复制累积器并分别追加不同输入。最后借用统计字段和模块数据，输出可核对的完整 i64 摘要。

| 阶段 | 行为 |
|---|---|
| `seed/data/mod.zil` | 目录模块；编译期生成记录、8 元素数组和权重数组 |
| `ops.zil` | 数值变换、数组元素共享投影、跨函数返回共享引用 |
| `main.zil` | 用户泛型 `Option<T>`、`Result<E,T>`、`Row<T>`；模式条件和循环；函数指针数组与未注解的 `apply`；可变闭包与独立副本 |
| `main.rs`、`ops.rs`、`seed/…/*.rs` | 相同操作顺序的 Rust 行为对照，使用标准 `Option`/`Result`、`const fn` 和 Copy 闭包 |

Rust 标准 `Result<T,E>` 的参数顺序与此例自行声明的 ZIL `Result<E,T>` 不同，成功值与错误值的含义一致。Rust 对照在 `apply` 的可调用参数上显式标注函数指针类型。

## 运行

在仓库根目录、项目指定的 Rust 与 clang 工具链环境中执行：

```sh
cargo run -p zil-cli --offline --locked -- run examples/mainline/app/main.zil --entry result
cargo run -p zil-cli --offline --locked -- run examples/mainline/app/main.zil
cargo test -p zil-driver --test mainline_app --offline --locked
```

`result` 应打印 `475000738663`。普通 `main` 自检后退出 0，不打印内容。集成测试实际编译运行 ZIL 和 Rust 的 O0/O2 版本，同时核对完整输出和自检退出，并清理临时二进制。

## 可人工复核的中间结果

| 项目 | 结果 |
|---|---:|
| 数据校验和 | 30000000050 |
| 有效行 / 错误行 | 6 / 2 |
| 有效行转换后合计 | 40000000116 |
| 原累积器追加 4 后总数 | 40000000120 |
| 副本追加 14 后总数 | 40000000130 |
| 最终原累积器成功 / 错误计数 | 7 / 3 |
| 副本成功 / 错误计数 | 7 / 2 |
| 错误行号合计 | 8 |
| 捕获前原 `Stats.total` | 0 |

最终摘要使用源码中的加权算式，计入两份独立状态、数据校验和、共享投影结果及错误路径。配对运行属于行为核对；没有计时或峰值内存对比，也不据此宣称语言整体优于 Rust。

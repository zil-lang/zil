# 同步错误与资源清理

这是四个真实 ZIL 模块组成的原生程序；`main.rs` 是独立手写的 Rust 行为对照。
`Fault!i32` 表示错误载荷为 `Fault`、成功载荷为 `i32`。后缀 `?` 在成功时取值，
在错误时按原错误类型提前返回，并清理当前函数仍持有的已初始化资源和拥有闭包。

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/error_cleanup/main.zil --output out/error-cleanup
printf '2' | ./out/error-cleanup
```

这个输入输出 `CBA`，退出码为 1。成功的 `i32` 结果作为退出码，成功的 `void`
结果为 0，错误结果为 1；入口不自动打印错误。普通 checked trap 仍终止进程，
不承诺执行错误传播的清理。

| 输入 | 退出码 | stdout | 覆盖路径 |
|---|---:|---|---|
| `0` | 1 | `A` | 首次检查失败 |
| `1` | 1 | `BA` | 拥有闭包创建后失败 |
| `2` | 1 | `CBA` | 全部对象创建后失败 |
| `3` | 7 | `CBA` | 成功返回后的正常清理 |
| `A` / `a` | 1 / 0 | `vBA` / `vXBA` | 较早的 give 实参已求值，较晚实参传播错误或成功 |
| `F` / `f` | 1 / 0 | `vBA` / `vXBA` | 已求值的 FnOnce callee 等待实参 |
| `P` / `p` | 1 / 0 | `vA` / `vBA` | Copy 字段尚未构成完整资源时失败或成功 |
| `C` / `c` | 1 / 0 | `BA` / `XBA` | FnOnce 正文提前失败或移出捕获 |
| `L` / `l` | 1 / 0 | `BCA` / `BCDA` | continue、break 与错误退出组合 |
| 其他（测试用 `?`） | 1 | 空 | 无资源的错误入口 |

`cargo test --offline --locked -p zil-driver --test impl056` 在 ZIL/Rust 的 O0/O2
四个二进制上逐一比较这 15 个输入的 stdout、退出码和空 stderr，共 60 次原生执行。
同一测试目标另检验 void 入口、Copy 聚合载荷、函数指针、错误注入拒绝与 CT 边界。
这是行为和清理顺序对照，未测运行时间、峰值内存或相对 Rust 的编译速度。

当前错误联合两侧仅支持有限 Copy 类型，机器表示为内联 `{tag, E, T}`；非活动载荷置零。
两侧同型时隐式注入有歧义，类型阶段拒绝；错误传播不隐式转换错误类型。
资源字段仍为 Copy，失败的字段表达式不会建立完整对象的 Drop 责任；这不代表一般拥有聚合
的逐字段部分初始化已经实现。拥有载荷、逃逸闭包、panic unwind、取消和异步仍不在本例合同内。

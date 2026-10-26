# C1：实际局部闭包与具体环境

`main.zil` 通过 `import scoring.weights` 调用另一个模块；闭包参数与结果由正文和调用约束推导，创建时复制 `base = 120`，外部随后修改为 `999`，复制到 `saved` 的环境保持创建时的值。四次调用计算 `120 + 122 + 124 + 126 = 492`。

```sh
cargo build --locked -p zil-cli
./target/debug/zil build examples/closurelab/main.zil --output out/closurelab
./out/closurelab
./target/debug/zil run examples/closurelab/main.zil --entry answer
```

直接运行原生程序用退出码自检；`--entry answer` 的原生 wrapper 输出完整 `i64` 结果。预期分别为退出码 `0`、文本 `492`；本轮实际运行身份与结果记录在验收报告中。

其他最小例子：

| 文件 | 入口 | 预期结果 | 主要行为 |
|---|---|---:|---|
| capture.zil | main | 42 | 创建时捕获，外部修改不影响环境 |
| copy_local.zil | main | 42 | bool 捕获、局部复制和赋值 |
| inference.zil | main | 42 | 无捕获参数与结果推导 |
| inference.zil | wide | 2147483650 | 从 i64 捕获和调用约束推导 |

例如 `./target/debug/zil run examples/closurelab/inference.zil --entry wide`。不要把普通直接运行的进程退出码当作完整整数结果。

当前范围为无捕获普通闭包与显式 `move` 的 Copy 标量捕获，局部具体环境、只读 `Fn` 普通调用。普通借入捕获、FnMut、资源/地址/Desc、捕获另一个环境、返回/参数逃逸、函数指针转换和 dyn 尚未贯通。`move` 是本例明确要求的创建时快照，不改变完整语言里普通捕获的用途推导规则。

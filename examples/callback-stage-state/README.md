# 高阶函数与闭包组合

`main.zil` 把函数指针、闭包参数推导、返回函数、记录中的回调更新及数组回调组合成完整的原生程序。

```sh
cargo run --offline --locked -p zil-cli -- run examples/callback-stage-state/main.zil --entry result
cargo run --offline --locked -p zil-cli -- build examples/callback-stage-state/main.zil --output out/higher_order_closure_calls
./out/higher_order_closure_calls
```

`result` 应打印 `5000000043`；普通 `main` 的进程退出码为 `0`。
测试 `crates/zil-driver/tests/higher_order_closure_calls.rs` 还覆盖模块与静态方法函数值、Unit ABI、共享捕获冲突、enum 回调载荷、工作/深度预算及原始捕获签名伪造。O0/O2 使用同一实际 LLVM IR，没有 Rust 中转。

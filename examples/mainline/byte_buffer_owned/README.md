# 拥有动态字节的保存回调

四个普通 ZIL 模块组成一个流处理器：`state.make()` 创建含动态字节和累加值的 Package，显式移动到返回的 FnMut 环境；主循环保存环境并逐字节调用。非零输入通过普通库函数增长存储，输出 `(累计字节值 + 当前批长度) % 251`；零输入替换拥有字段、释放旧批次并输出零。退出时释放最后一批。

本例覆盖跨模块工厂返回、拥有 Package 捕获、反复调用、Copy 字段更新、动态增长和拥有字段替换。`main.rs` 是独立行为参照；只核对 stdout/退出状态，不以这两份程序的运行成本推导 Rust 性能关系。分配失败和准确释放次数由 `impl099_owned_bytes_native` 的单独故障分配器用例观察。

```sh
cargo run -p zil-cli -- build examples/mainline/byte_buffer_owned/main.zil -o batcher
```

自动验收通过同一生产 driver 的 `compile_path` 接入 LLVM 和原生链接器，完整命令与阶段产物保存在测试指定的 `ZIL_TEST_REPORT_DIR`。

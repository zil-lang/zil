# ZIL 通用借入序列示例

`sequence.zil` 提供普通切片参数、长度、索引、修改和参数来源返回；同一函数处理3字节数组及16字节工作区的不同运行时区间。`main.zil` 是流式 ASCII 大写转换消费者，保留其它字节，包括 NUL 和 UTF-8 编码。它使用普通 libc 字节 I/O；没有将切片胖指针交给 C。

```sh
cargo run --offline --locked -p zil-cli -- build examples/mainline/slices/main.zil -o out/slices
```

`main.rs` 为独立 Rust 行为对照。两者输出等价的验证不构成编译时间、运行时间、内存或完整安全的对比结论。实际执行身份和结果只读092收据及最终统一门。

切片不拥有后备存储，不隐式分配，不支持本片之外的动态扩容、借用字段保存或长期外部回调。共享/独占、来源、活性和边界经过独立 Verify；两个看似不同的范围仍以整个后备对象保守检查冲突。当前元素是有限 Copy 值；闭包捕获、FFI 和 CT 的切片执行保持关闭。

# Product 拥有环境消费者

`main.zil` 将三层拥有 Product 交给保存的只读回调，再将两个同型回调放入 `State<F>` 和外层 `Store<State<F>>`。普通代码复用已有构造、调用、容量与错误路径；只有环境准入和完整 child 的逐项责任/清理是本片增量。

两个完整值通过已接受的 `T:StoreElement` 模板进入 Store，实际 callback 在 main 的具体类型上下文调用；没有扩大旧 State 模板的一输入限制。测试另将 factory 和投影切换为 FnMut / `store_at_mut`，每次取出再完整补回 child，验证增长移动后保存的环境标志。

源码预期：正常输出 `ABCDEFGH`、退出 0；第一次 Store 分配失败输出 `EFGHABCD`、退出 1；扩容失败输出 `ABCDEFGH`、退出 2。成功时 Store 的初始化前缀按元素顺序清理，第一次 reserve 失败时两个尚未转交的参数按逆初始化顺序清理。

Linux x64 的 Root 是 28 字节，环境追加 shell/child 两个 i1 并按 4 字节对齐后为 32 字节；State 的 ByteBuffer 头为 24 字节，故每个元素步幅 56 字节。reserve 2/4 的真实请求应分别为 112/224 字节。

`crates/zil-driver/tests/impl216_machine.rs` 固定这些预期，并覆盖 Fn、FnMut、FnOnce、提前/错误退出、未调用销毁及不同实例。allocator observer 复用 `tools/language/173/allocator_probe.c`，观察真实 realloc/free 和逐次失败；不替代语言权限检查。

执行与接受状态以精确提交的证据为准。本消费者不宣称完整 ZUI、完整 1.4 或原生 Windows 已验收。

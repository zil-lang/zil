# 字节、固定缓冲区和同步 C IO

本组程序直接从 ZIL 源码生成 LLVM 原生程序。Linux x86_64 的实际 `read/write/pipe/close/dup` 调用贯通初始化、地址借用、词法 unsafe、资源关闭和机器 ABI。它是语言基础库的一个可用增量，尚无动态容器、UTF-8 字符串类型或平台窗口。

```sh
cargo run --offline --locked -p zil-cli -- build examples/mainline/byte_io/main.zil -o out/byte-stream
printf 'hello, Zil!\n' | out/byte-stream
cargo test --offline --locked -p zil-driver --test impl083_bytes_native
```

## 程序与真实观测

| 文件 | 行为 | 核对范围 |
|---|---|---|
| `main.zil` / `main.rs` | 以一个 64 字节数组流式转换 ASCII 小写，其余字节原样传递 | 空输入、NUL、高位字节、UTF-8、63/64/65 与 127/128/129 等跨块边界 |
| `pipe.zil` / `pipe.rs` | 两个拥有句柄组成 Pipe；写入 8 字节，提前关闭写端，读出并确认 EOF，最后关闭读端 | 真实 OS 文件描述符，重复 `dup` 探测关闭状态；读取 `[0,127,128,255,228,184,173,10]` |
| `errors.zil` / `errors.rs` | 非法 fd、保留初始化字节、长度越界、空写 | 成功打印 `E`，失败处以不同退出码定位 |
| `arithmetic.zil` / `arithmetic.rs` | 所有 65,536 对 u8 输入的算术、位运算、比较、合法移位、合法 checked 运算 | 独立宽整数模型 checksum `73740310`，ZIL/Rust 各 O0/O2 |
| `partial.zil` / `partial.rs` + `short_write.c` | 强制短写及写过前缀后的零进展/错误、非法返回计数 | 显式 C 测试夹具，经链接器包装 `write`；成功数据仍交真实 OS；不当作 libc 自发短写证据 |

正常 IO 与算术共 15 输入场景，ZIL/Rust 各 O0/O2 为 60 次行为对照；C 夹具另 4 场景×4 为 16 次。另有运行时失败与 u8 返回 harness 共 27 次 ZIL 原生执行。它们都比较实际输出和退出状态，未测运行时间、峰值内存或编译速度；不能由行为相同推出性能同级。

独立审查另发现并修复了 move Copy 捕获经可变地址修改后没有回写环境的旧遗漏。新增 8 个源程序×O0/O2 共16次原生回归，覆盖普通调用、地址别名、数组投影、循环中提前返回、Unit fallthrough、`?` 同步错误、直接赋值与地址更新交错及负状态提前返回；本 suite 因而共7组、119次原生执行。新捕获扫描与回写分别核对 Types 分析预算213/212、指令预算47/46，少一单位均ResourceLimit。C 自增与失败返回的独立复核另记于083审核收据，不混计到119。嵌套闭包推断仍是既有明确Unsupported范围。

## 合同和边界

`u8` 是完整 0..255 字节，LLVM 为 i8、大小/对齐 1。普通算术 checked，显式 wrapping 模 256。比较、除余及右移按无符号规则执行，移位计数须小于 8。`u8(x)`、`i32(x)`、`i64(x)` 保留数学值，越界执行时 trap，CT 时拒绝，不隐式截断。独立 Verify 同时核对输入与目标类型，转换不能制造地址权限。

`io.zil` 的 `noescape(buffer)` 是作者对 C 实现作出的 unsafe 承诺：调用同步、不保存地址、不回调或重入、不跨边界展开。长度、可变权限和 C ABI 真实性由 wrapper 负责。每个地址实参仍经过活性、来源、重借、同时冲突和独占消费检查；声明不会制造权限，也不自动推出 noalias/nocapture/dereferenceable。全部数组在调用前已初始化。

这里的 Linux x86_64 `size_t`/`ssize_t` 分别按 64 位参数/返回传递，wrapper 的计数限制为 0..64。它不是任意平台通用 C header 绑定。共享/独占地址目标仅为 u8/i32/i64 与递归非空固定数组；C 值参数/结果仍采用已有 i32/i64/unit profile，无窄值 C ABI、地址返回、raw 指针转换或保留回调。

`write_all` 会改变输入数组内容：短写后把未写前缀移到开头继续写，因而无需新增指针算术。它遇到零进展、负返回和超出请求的返回计数立即报错；不承诺 errno 解释、EINTR 重试、非阻塞调度、SIGPIPE 管理或持久化落盘。示例里的数值打印只用于非负 checksum。

实际 `read` 可以短读，0 表示 EOF；实际 `write` 可以短写，成功也不表示数据已经落盘。这些平台合同参见 Linux man-pages 项目的 [read(2)](https://man7.org/linux/man-pages/man2/read.2.html)、[write(2)](https://man7.org/linux/man-pages/man2/write.2.html) 和 [pipe(2)](https://man7.org/linux/man-pages/man2/pipe.2.html)，本次核对于 2026-10-25。

源码/CT 正反例、完整阶段 IR、LLVM、构建命令、输入输出与真实失败留在独立运行目录。正式通过身份以 evidence/impl/083 与最终验收收据为准；这里列出的范围不是整个语言或 ZUI 的完成声明。

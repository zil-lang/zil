# 动态整数流应用

从真实 stdin 读入以 ASCII 空白分隔的有符号十进制整数，输出 `accepted rejected total`。每个有效数的绝对值至多 1000000；无效 token 计入 rejected。有效数依次交替经过 `x * 2` 与 `x + 11`，计入 total。最多处理 1000 个 token，超过时退出 2 且不输出部分结果。正常退出 0，putchar 当场报告失败时退出 3。

```sh
printf '10 20 -3\n' | cargo run --offline --locked -p zil-cli -- run examples/mainline/stream/main.zil
# 3 0 45
printf '1 broken 2 -4 +5\n' | cargo run --offline --locked -p zil-cli -- run examples/mainline/stream/main.zil --json
# JSON 的 run.stdout 为 "4 1 23\n"
cargo test --offline --locked -p zil-driver --test mainline_stream
cargo test --offline --locked -p zil-cli --test stream_io
```

`model.zil` 提供公共泛型 Option/Result、公共 Stats 和泛型 identity；`tokens.zil` 的私有解析状态通过 `&mut` 更新；`main.zil` 的默认闭包捕获直接更新外部 Stats，闭包最后使用后重新读取 Stats；`io.zil` 只在显式 unsafe 中调用私有 C 声明，其余解析、控制流、格式化和计算都是普通 ZIL。

Rust 对照采用标准字节切分与整数解析，与 ZIL 的逐字节状态机独立；测试把相同输入送入两种语言的 O0/O2 可执行程序，并检查明确的期望输出。此例没有使用 `--entry` 打印包装器。

目前 getchar 不能区分 EOF 和底层输入错误，退出时 C stdout 的最终刷新错误也不在本例协议中；完整 IO 错误模型和资源生命周期仍需后续语言/标准库工作。源码中的具体限额是应用协议，不是语言的通用输入限制。

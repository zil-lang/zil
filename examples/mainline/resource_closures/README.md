# 三模块资源闭包程序

本程序用真实stdin选择一条资源/闭包路径，普通ZIL main执行实际调用和Drop。`token.zil`定义资源与Drop，`io.zil`把getchar/putchar限制在私有C声明和显式unsafe内。`main.rs`是独立Rust行为参考；它不参与ZIL编译、安全核验或生成机器码。

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/resource_closures/main.zil -o out/resource-closures --emit-dir out/resource-closure-stages
printf O | ./out/resource-closures
printf 2 | ./out/resource-closures
cargo test --offline --locked -p zil-driver --test impl053
```

输出字符来自实际析构，均没有末尾换行。输入读取一个字节；以下是受测协议。

| 输入 | stdout | 退出码 | 观察的行为 |
|---|---|---:|---|
| `D` | `C` | 0 | Desc闭包读取更新后的目标，并通过Desc闭包写目标 |
| `S` | `A` | 0 | 拥有资源的Fn重复只读，最终只清理一次 |
| `U` | `AB` | 0 | 环境创建后从未调用，仍清理拥有字段 |
| `O` | `CB` | 0 | FnOnce中give一个字段，返回后清理剩余字段 |
| `M` | `C` | 0 | 拥有FnMut重复修改自己的资源 |
| `E` | `A` | 7 | 提前返回仍清理已创建环境 |
| `T` | `A` | 0 | 环境转交，原位置不重复清理 |
| `L` | `AB` | 0 | 循环continue/break清理每次构造的环境 |
| `0` | `A` | 1 | 首个资源后返回失败值，后续环境尚未构造 |
| `1` | `BA` | 2 | 环境完成后返回失败值 |
| `2` | `CBA` | 3 | 尾资源完成后返回失败值 |
| `3` | `CBA` | 0 | 完整正常返回 |

`Outcome`是普通用户定义枚举，四个后续分支通过正常Return携带成功/失败值。测试验证这些具体构造边界的清理，不代表语言级`E!T`、后缀`?`、部分初始化聚合、unwind或取消已经实现。当前checked trap终止进程，不执行unwind清理。

`impl053`分别构建ZIL O0/O2和Rust O0/O2，12输入共48次原生执行，比较stdout、退出码与空stderr；另检查已核编译链、三模块输入、环境Drop/live flag和固定FnOnce临时存储，无隐式malloc/noalias。此测试是行为比较，没有比较编译速度、运行时间或峰值内存。

Rust参考使用Cell表达串行Desc更新，显式保持对整个Token的借入，避免精确字段捕获把观察对象改成Copy字段快照。Drop外部C语义、最终缓冲刷新等仍属于样例外部合同；此样例不是完整IO库。真实fd成功关闭另见`../resources/fd.zil`。

当前本地具体环境可捕获Desc及拥有Managed；拥有捕获本身不决定FnOnce，移出捕获资源才需要消费调用。闭包显式参数和结果仍限有限Copy，环境逃逸/嵌套保存等边界见`docs/RESOURCE-HANDOFF.md`。

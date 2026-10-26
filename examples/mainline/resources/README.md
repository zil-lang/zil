# 资源主线原生程序

`main.zil`、`token.zil`、`io.zil` 是三个真实源码模块。工厂返回 Managed，普通传参和 `let alias = owner` 取得 Desc，`give` 转交责任；`impl Drop` 在实际清理时输出 payload 中的字符。描述可以顺序读取和修改字段，不能在所有者转交后继续使用。

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/resources/main.zil -o out/resources
printf x | ./out/resources
printf '!' | ./out/resources
```

普通输入（包括 EOF）输出 `EICFG|HA`，退出0；`!` 输出 `EICA`，退出2。输出没有末尾换行。

| 字符 | 实际行为 |
|---|---|
| E | D 工厂结果作为 give 参数交入 consume，修改后在函数返回时清理 |
| I | 工厂临时借入 read，在完整表达式结束时清理 |
| C | B 经共享描述修改，再转交给 y；原位置不重复清理 |
| F、G | continue 和 break 均清理该轮成功构造的局部 |
| `|` | main 正文输出，标识普通退出清理前的时点 |
| H、A | main 的两个仍存活资源按逆成功初始化顺序清理 |

`main.rs` 是独立 Rust 行为对照，使用普通所有权和 Drop。测试在 ZIL/Rust、O0/O2 下分别核对四组正常输入与一组提前返回，共20次原生执行。

## 实际操作系统资源

`fd.zil` 使用私有 C `dup`/`close`，把成功复制的 stdin 描述符放入资源，转交后自动关闭。离开作用域后再次 dup 该编号必须失败；成功输出 `F`、退出0。`fd.rs` 使用 Rust 标准库 `OwnedFd` 作独立对照，两种语言的 O0/O2 共4次执行。

```sh
./target/debug/zil build examples/mainline/resources/fd.zil -o out/resource-fd
./out/resource-fd < /dev/null
cargo test --offline --locked -p zil-driver --test impl049
```

该例的测试 profile 是 Linux x86_64、stdin 已打开、无并发重用被关闭编号。只有非负 dup 结果进入资源。C ABI 和外部操作合同仍由显式 unsafe 封装承担；这个成功路径不是完整 POSIX 错误/重入协议，也没有证明任意裸整数都是合法句柄。

## 实现边界

payload 限有限 Copy 类型；正常作用域、return、break、continue 与本片完整表达式临时有实际清理。inline payload 配合真实 live flag，Desc 是普通 pointer，不附 noalias，不引入隐式堆盒或 RC。最终 emitted LLVM 与独立已核模块逐字段绑定。

资源嵌套、替换、拥有资源的闭包、Desc 返回/保存、一般地址到资源、错误展开和取消仍未完成。清理顺序核验尚无一般清理作用域身份，使用保守的局部顺序；checked trap 是进程 abort，不能称为清理展开。完整语言和产品目标仍未完成。

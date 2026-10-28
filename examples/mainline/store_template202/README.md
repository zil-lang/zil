# 普通 StoreElement 模板的五模块消费者

程序把两个独立创建的 FnMut 环境从 Pair 移入两个完整 Item，再在 Store 元素的真实位置调用。`state.zil` 通过公开的普通 `T: StoreElement` 函数建立 Store、reserve、push、pop 以及读取 len/capacity；这些函数在 `store_helpers.zil`，保持真实函数边界。窗口仍由现有 `store_at_mut` 建立，字段调用仍经过既有同步 `invoke`。

| 模块 | 职责 |
|---|---|
| `main.zil` | 输入选择、结果/Tag 校验、真实切片写入校验 |
| `factory.zil` | 分别创建 A/B 环境，各自拥有 Ticket 和 ByteBuffer |
| `state.zil` | Pair→Item→Store 转交、元素窗口、ABBA 调用、扩容、拥有 pop 和清理 |
| `store_helpers.zil` | 六个普通完整 T 模板：empty/reserve/push/pop/len/capacity |
| `portable.zil` | Copy Tag 和同步字符输入/输出 |

原 201 四模块源码保留在原目录；本目录是 202 的独立消费者。当前运行状态必须查看与实际编译器提交绑定的收据，不能从此程序或 201 的成绩推定通过。

## 输入与独立预期

| 输入 | 主要路径 | 正常 stdout |
|---|---|---|
| `0` | 零次回调、扩容、pop 后不调用 | `2b1aZ` |
| `1` | 一轮 ABBA，pop 后再调用 B | `ABBAB2b1aZ` |
| `2` | `continue` 跳过第二轮 | `ABBAB2b1aZ` |
| `3` | 两轮 ABBA，pop 后再调用 B | `ABBAABBAB2b1aZ` |
| `7` | 三轮 ABBA，在下一轮 `break` | `ABBAABBAABBAB2b1aZ` |
| `6` | 容量为 1，第二次 push 返回完整 B 错误载荷并清理 | `2b1aZ` |
| `9` / EOF | 无效输入，无资源构造 | 空输出、exit 9 |

正常有效输入 exit 0。输入 6 不调用回调；main 额外检查两字节均未改变。其输出虽然与输入 0 相同，但实际只分配一个 64 字节元素槽；输入 0 最终扩为四槽、峰值 256 字节。独立分配 oracle 必须同时核计数、峰值和清理，不能只比较 stdout。

满容量 push 不会重新分配。B 的完整拥有错误载荷跨普通 `push<T>` 返回后销毁，先打印 `2b`；Store 中 A 随后打印 `1a`，最后 Pair 的剩余 marker 打印 `Z`。首次 reserve 的真实分配失败时 Pair 尚持有两个环境，预期 `Zab`、exit 1。

其他有效输入覆盖初次 reserve、每个环境自己的 ByteBuffer reserve、Store 扩容、pop 后 B 的最后一次调用；每个真实分配点分别注入失败，并有无失败及最后一次之后的控制。未变的源布局独立计算为 Item stride 64：Mark、含 Ticket/ByteBuffer/live flags 的具体环境、Tag；运行中不从观察值或 LLVM 反推 oracle。

## 运行与证据身份

`tools/language/202/consumer.py` 为新 runner。它要求干净的源码提交、成功的干净 CLI 构建收据及冻结 CLI SHA256，并核对编译器提交和消费者提交的 Rust 生产输入没有差异。消费者源码/测试/说明可以晚于 CLI 构建，但不能把别的编译器二进制归到当前生产。

Linux 示例；输出目录必须尚不存在：

```bash
python3 tools/language/178/run_command.py \
  --out "$STORE202_RUN/compiler-build" -- cargo build --locked -p zil-cli

python3 tools/language/202/consumer.py \
  --repo "$PWD" --cli "$STORE202_CLI" \
  --compiler-build-receipt "$STORE202_RUN/compiler-build/command.json" \
  --compiler-sha256 "$STORE202_CLI_SHA256" \
  --platform linux --clang "$STORE202_CLANG" \
  --out "$STORE202_RUN/linux"
```

其中 `STORE202_CLI` 是上述构建的实际 CLI 路径，`STORE202_CLI_SHA256` 取该文件的实际 SHA256。runner 在 O0/O2 分别构建并核 `verification_runs=1` 和五个实际源模块，然后运行普通与分配探针矩阵。计划每平台 126 次执行、86 个真实 allocator 失败用例；实际成功数以新收据为准。

Win64/Wine 使用同一 runner 的 `--platform win64`，另外提供 `--wine`、`--wine-library-path`、`--linker` 和三个 `--windows-lib`。使用明确的 WINEPREFIX/WINESERVER/WINELOADER/WINEDEBUG，运行前清除 TMPDIR；收据同时记录这些实际继承环境。Wine 与原生 Linux 分列，原生 Windows 保持 NotRun，除非另有真实原生执行证据。

探针只观察/注入分配器调用：Linux 复用 173 的 realloc/free wrapper；Win64 复用 181 的 Heap API 探针并对独立 LLVM 副本做精确外部符号重定向。普通构建和探针执行分列，ZIL 原始 LLVM 不覆盖。最终交付排除 exe/object/编译缓存；命令、原始 stdout/stderr、文本 IR 与清理证据独立打包。

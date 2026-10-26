# schema17：显式字节存储、泛型窗口与拥有组合

本片以ZIL语言为主体。它提供可以处理运行时增长输入的普通语言能力，未实现ZUI框架。

## 已贯通的接口

| 源操作 | 输入/结果 | 语义 |
|---|---|---|
| `bytes_new()` | 新的Managed ByteBuffer | 空头，不分配，不失败 |
| `bytes_reserve(b,n)` | Desc、i64 → bool | 绝对容量；负数false；已有容量够用true；失败不改变旧头、内容或责任 |
| `bytes_push(b,x)` | Desc、u8 → bool | 不分配；满则false；先初始化字节再增加len |
| `bytes_len(b)` / `bytes_capacity(b)` | Desc → i64 | 长度是已初始化前缀，容量不能当作可读长度 |
| `bytes_view(b)` | Desc → `&[u8]` | 共享窗口继承真实来源 |
| `bytes_view_mut(b)` | Desc → `&mut[u8]` | 独占窗口继承真实来源 |

ByteBuffer为封闭内建资源。内部拥有头是`{ptr,i64 len,i64 cap}`，x64尺寸24字节，借片为`{ptr,i64 len}`。普通函数参数默认Desc；显式give参数和create/give结果沿用既有唯一责任合同。源代码不能伪造字段、复制拥有头或指定自定义teardown。

`0 ≤ len ≤ cap ≤ i64::MAX`与初始化区间`[0,len)`由封闭原语维持。实际增长策略放在普通ZIL的`buffer.append`里；内置push不会偷偷扩容。Linux使用realloc/free，Win64使用GetProcessHeap/HeapAlloc/HeapReAlloc/HeapFree；无新隐式RC、共享堆盒或全局ZIL对象表。外部分配器遵守ABI及不重入仍属于明确可信边界。

## 能组合到什么程度

- 普通工厂创建后返回、give参数消费、give结果转交、Desc参数返回共享/可变窗口已经从源码运行到机器。
- 泛型`put<T>`、`tail<T>`与`tail_mut<T>`可同时处理固定数组的窗口与动态backing窗口。符号检查不会从某个具体实例推断未声明的Copy或数值能力。
- 两个不同形参名字不构成分离证明。`view(a); use(view); reserve(b)`可以接受同一实参；`view(a); reserve(b); use(view)`要求两个实际来源分离。义务在真实活性点产生并沿调用传播。
- 本地拥有者或给入拥有参数不能返回无依托的裸切片；reserve/push/give与尚活跃的窗口冲突。普通错误出口和准备后续实参失败都必须结清已准备的临时拥有者。

## 099：拥有字段和保存环境

ByteBuffer已允许直接进入`package P { manages data: ByteBuffer; ... }`，可完整构造、整体参数/返回、字段take/refill/替换；已有准备失败和错误出口遵守唯一清理。实际Managed ByteBuffer及完整Package可拥有捕获，工厂能返回现有`impl Fn/FnMut/FnOnce`并跨调用保存状态。

描述与窗口追溯实际Package shell或capture位置，当前以整个shell为保守根。不同字段名称不会自动制造apart证明；活窗期间相交的取走、替换、增长及整个载体移动/销毁均拒绝。最后使用后的资格恢复有独立正例。两层普通helper写效果必须影响FnMut判定。

LLVM同时处理直接capture的内联类型与对齐；释放Package字段或capture前先清外部责任flag，再调用内建`zil.bytes.drop`。099作者和非作者分别保留真实释放观察，Linux原生与Win64/Wine兼容执行按其实际范围分列。见[四模块保存消费者](../examples/mainline/byte_buffer_owned/README.md)、[作者收据](../evidence/impl/099/author-result.json)和[独审](../evidence/reviews/099/review.json)。

## 094–096 历史机器与故障证据

096作者最终运行24次真实源码编译，每次独立Verify恰好1次；38个原生进程中包括8个Rust对照、8个明确SIGILL(4)越界trap和22个其他正常ZIL执行。4个输入长度为0、9、256、32769字节，包含NUL及全部字节值，O0/O2输出与Rust一致。

故障注入仅用于测试：在指定正分配尺寸失败，核对旧内容、len、cap和准确释放计数；C文件不是产品存储实现。额外4次Linux/Win64-Wine便携执行覆盖内部拥有ABI、增长、泛型修改、give参数与结果及实际校验和。Win64兼容执行和原生Windows验证分别列示，不把内部24字节头称作公开C结构体FFI。

最初源面错误、泛型推断缺口和夹具语法错误均在原始归档中保留；最终通过没有覆盖早期失败。精确作者、非作者及正式运行分别读取evidence/impl/094–096、evidence/reviews/094–096和evidence/final-summary.json，不能累加为不同覆盖。

## 仍关闭的能力

ByteBuffer Desc借入捕获、借自环境的裸slice返回、普通拥有聚合、拥有错误载荷、CT、公开拥有头FFI及自定义分配器仍关闭。直接拥有Package字段和实际拥有捕获已由099完成，不能沿用其早期临时封门。本片不是一般`Vec<T>`，未涉及任意对齐、零尺寸元素、元素析构或通用部分初始化；不是UTF-8字符串库，也没有异步取消或并发语义。

后继先解决一般元素容器的析构、对齐、部分初始化和明确借入/返回合同；所有跨阶段消费者和最近反例同批更新。完整语言NotSatisfied、产品NotRun、B0 Doing保持。

## 直接运行

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/byte_buffer/main.zil -o out/dynamic-bytes
./out/dynamic-bytes < input.bin > output.bin
cargo test --offline --locked -p zil-driver --test impl096_byte_buffer_native
```

消费者保留全部非ASCII字节，把ASCII小写转大写；输入长度由实际存储增长决定。Win64重放另需显式Heap*导入库：

```sh
llvm-dlltool -m i386:x86-64 -d tools/byte-buffer/096/kernel32-heap.def -l out/kernel32-heap.lib
python3 tools/byte-buffer/096/verify.py --help
```

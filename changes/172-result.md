# 172：拥有 Package 的动态前缀存储结果

作者候选为 `69882483d6b29bee8d831386325da739ac915fd4`，`crates` tree 为 `118e246b47beb9e027d656a09ebbd411191a0fff`。本记录对应作者限定验证；独立审核和主树集成由 173 负责。设计正文未改，schema/Wire 20；当前规范摘要 `c358b8c5e0abaf11d81b1d55e23f6896cfb485eb4291f34937fe43e9e7ea319c`。

## 已形成的语言能力

`Store<T>` 是真实的密封拥有资源，保存运行时数量的完整 Package。有效元素恰为初始化前缀 `[0,len)`，尾部容量没有元素责任。普通局部、Desc 参数、create 工厂、give 输入/返回和自动清理已贯通 Types、Analyze、独立 Verify、Lower、LLVM 与 Linux x64 原生执行。没有静态枚举容量，也没有将 ByteBuffer 解释成一般元素容器。

第一片允许元素含原有直接 Managed 与 Copy 字段，且至少一个 manages 字段、布局非零。用户有限 Copy resource 和 ByteBuffer 字段均可用。170 的拥有数组字段、Store 字段/捕获、递归 Store、Closure 元素和元素地址/借片继续明确拒绝；Types、Verify、Lower 和 LLVM 对这些边界分别闭合。

实际源码入口如下，`store_new()` 通过具体注解或 owning 返回上下文取得元素类型：

```zil
fn empty() -> Store<Item> contract { result(create) } {
    return store_new()
}
fn relay(give value: Store<Item>) -> Store<Item> contract { result(give) } {
    return give value
}
```

| 操作 | 行为 |
|---|---|
| `store_new()` | null/0/0 空头，不分配；显式 `<T>` 调用语法不是本片入口 |
| `store_reserve(s, capacity)` | 指定绝对容量；负数、字节尺寸溢出或分配失败返回 false，原头和元素不变；不自动增长/缩小 |
| `store_push(s, item)` | 返回 `Item!void`；成功发布完整元素；满时错误载荷归还完整 Item，调用者不能继续使用已交出的输入 |
| `store_pop(s)` | 返回 `void!Item`；非空先减少 len 再转交尾项；空时返回 Unit 错误 |
| `store_len(s)` / `store_capacity(s)` | 读取运行时长度/容量 |
| 整体给出与销毁 | 头及其整个前缀一起转交；销毁按索引升序、Package 字段声明序清理，最后释放块 |

`examples/mainline/impl172` 的四模块程序将元素准备、存储接口、循环增长/排空和外部输入分开。`main.zil` 从真正的 `getchar` 输入读取数量，显式每次扩容 2 项，跨工厂/普通函数/give 转交后尾取。每个取出元素还在声明模块中核对 ByteBuffer 内容，再通过普通拥有 Package 参数清理。源码模块属于一个实际 ZIL 编译项目，不是手填 IR 或 C 容器。

## 实际验证

完整作者受测候选为上述提交，`CARGO_INCREMENTAL=0`，每次 Cargo 构建 `-j2`。独立 fresh target 中完成 172 的 **15/15** 项及相交 170 的 **16/16** 项，均无跳过；定向 Clippy `-D warnings` 与 `cargo fmt --check` 通过。源码测试包含 20 次 O0/O2 native 构建执行、19 个明确 Phase/Code/消息的源码反例，以及 22 组 raw/Wire 成对篡改拒绝。作者没有执行正式工作区全量门。

独立 CLI SHA-256 为 `b315b878f9f6889084a2505be3ec3fc6b988cfdeb247e19e40a7a8b6397b5fd2`。`tools/language/172/consumer.py` 在该候选上另外运行 **12 次普通执行、22 次 libc 分配探针执行**；O0/O2 分开构建和链接。探针只是记录/失败注入实际 `realloc/free`，容器实现仍由 ZIL 编译器生成。所有执行均符合预期退出码、析构输出与真实分配账目，`allocated == frees` 且 `live == 0`。

| 四项输入的失败位置 | 实际已完成分配数 | 析构输出 | 退出码 |
|---|---:|---|---:|
| 无失败 | 5 | DCBA | 0 |
| 1：初始 Store reserve | 0 | 空 | 1 |
| 2：第一项 payload | 1 | A | 1 |
| 3：第二项 payload | 2 | BA | 1 |
| 4：Store 扩容 | 3 | AB | 1 |
| 5：第三项 payload | 3 | CAB | 1 |
| 6：第四项 payload | 4 | DABC | 1 |

这里一次成功扩容会搬移既有块，不能把它计成新的独立存活分配。另有已存两项后 reserve 到 6 的失败恢复程序：失败后 len/cap 不变，仍能逐个核对原元素内容并排空，输出 BA、退出 0。满 Store 的 push 错误结果包含 Ticket 和 ByteBuffer，返回到原生错误入口后恰好清理一次；该 payload 自身的准备失败也单独注入。

Raw/Wire 覆盖 New 的显式 element/result 错配、伪造密封定义/用户 teardown、MakeManaged 组装头、ByteBuffer 原语冒用、错误容量/元素、重复元素转移、遗漏 Store/Pop 结果清理、复制头、头字段读取、无效 ValueId、普通 Package/170 数组叶/错误载荷/闭包绕入 Store。完整类型和来源由 Verify 自行核验；变体只刷新候选身份，不在变体上重新调用 Analyze 生成证明。

预算测试实际测得 `N=7122`：raw 和 Wire 在 N 均通过，在 N−1 均 ResourceLimit。极宽 New 元数据、258 层深类型、四层长度 1,000,000 的 Copy 数组所形成的字节尺寸乘积也各有独立拒绝路径，不抬高默认预算。

## 本轮发现并修复的公共边界

深度压力样本先暴露公共 Wire Writer 的递归栈溢出。把 raw Verify 前置后，它正确在 fingerprint 前返回 ResourceLimit；绕过递归 Writer、独立构造相同格式字节，又确认 Reader 同类溢出。173 在 `bf2280d8ef53ed234f0605e10a55d7741b914b3e` 将类型编码和解码改为显式遍历。

原样恶意候选在修复前后 SHA-256 均为 `dcd22cf30d2f1239d09b0babd22c5079ed12a04e8cf2c76e75760c8474f7e531`。最终测试同时检查 Writer 输出与手工字节逐字一致、raw 默认限制拒绝、Wire 默认限制拒绝；没有提高线程栈或验证深度。日志中的栈溢出、Cargo rlib/rmeta 基础设施失败和早期夹具失败均保留，未计成语言反例成功。

旧 170 CLI `f128bd2f1d95649b73bbe3fdcca9e742e39125b913d87209ed1ab5874d33aabf` 对同一最终四模块源码仍在 Types 返回 `E-RESOURCE-TYPE`，verification_runs=0；相应输入与身份收据与新候选分开保存。

## Rust 四目标与剩余缺口

| 目标 | 本片证据与界限 |
|---|---|
| 同类安全 | 完整责任载体、独立 raw/Wire 拒绝和真实每准备点失败清理已执行。范围限明确的顺序、密封前缀合同；不代表一般拥有图、逃逸、取消和全语言安全证明。 |
| 运行/内存成本 | 真实 24 字节头、内联 Package、显式 allocator ABI、分配与释放平衡有机器证据。没有与 Rust 同合同计时/峰值内存比较，不能称同级或胜出。 |
| 表达/维护 | 原先不能表达的运行时完整拥有元素集合可由普通模块函数使用；失败值也是普通拥有错误载荷。没有维护任务工作量比较，Store 仍不能作为 State 字段或捕获。 |
| 编译成本 | 没有编译性能研究或 Rust 对照；构建日志时长仅是命令证据。预算按新增真实遍历收费，未降门槛。 |
| 可重复净突破 | 已有实质语言能力增量；不据此宣称产品级 Rust 净胜出或原创一般容器/图理论。 |

下一项 ZUI 前置应围绕明确需求选择 Store 的 State 字段/拥有捕获、完整元素的有限访问/替换，以及有作用域依据的元素借入合同。当前没有承诺地址在 reserve 后稳定；也没有外部回调保持/关闭、异步取消、Windows 原生或一般递归对象的完成结论。完整语言 `NotSatisfied`、产品 `NotRun`、父 B0 `Doing` 保持。

精确收据见 `evidence/impl/172/SUMMARY.json` 与 `FILES.json`。精简证据约 2.1 MB，包含必要文本、阶段表示和候选字节，不含本机执行文件或 Cargo 产物；源码交付包可按当前交付规则排除整个 evidence 目录。

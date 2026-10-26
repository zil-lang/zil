# 0124 — 有符号移位与普通文本消费者

本片补齐一个由真实文本代码直接触发的语言缺口：普通 UTF-8 decoder 的 `scalar=(scalar<<6)|(byte&63)` 在 Types、Analyze 后被 Verify 拒绝。把移位换成乘64的基线控制可通过；源 `#(...)` 同样受旧 Verify 准入限制，尽管 CT 执行器已实现对应整数操作。接受性提案 `725ebd1d420c86f4fec90fae4f25aa53296c0de5` 先于生产修改，精确受测源码提交为 `a389613cdc28af30db710ee4e833fd97ee02b912`。

## 实现与实际消费者

生产只改 Verify 的现有 binary_result 及 LLVM 的现有移位分支。同型 i32/i64 现在可贯通普通表达式与 CT；结果仍是原类型。移位量须满足 `0 <= n < 位宽`。LLVM 先作无符号 `n >= width` 检查，负量的位表示也被此 guard 拒绝；合法分支发射没有 `nsw/nuw/exact` 的 shl 或 ashr。u8/u16 右移仍为 lshr。左移丢弃高位，右移符号扩展，负左操作数合法。CT 非法量为既有 Ct/E-CT-ARITHMETIC，Linux 原生非法量观察为 SIGILL；这是 checked trap，并不声称发生清理展开。

`examples/mainline/text_decode` 的四个普通 ZIL 模块实现 ByteBuffer 追加、UTF-8 逐标量解码、标量/字节索引和 UTF-16 写入。运行输入由 stdin 进入动态 ByteBuffer；`getchar` 只负责字节 I/O，文本算法不委托 C/Rust。非法编码和输出不足时，write_utf16 在写入前返回，原输出不变。测试覆盖补充平面代理对、NUL、编码边界、overlong、surrogate、截断、非法 continuation 和容量边界。

此消费者上限为1024输入字节及32个UTF-16输出槽；库函数接收普通借片。它没有建立拥有 String 类型、不变量构造器、grapheme、规范化、排版或长期外部字符串留存能力。bytes_truncate 留作独立后继任务。

## 源码绑定的限定验证

干净提交 a389613 上顺序执行限定 Driver、Verify、LLVM lib、fmt 及 clippy -D warnings，执行前后 HEAD 相同且 tracked tree 干净。91个测试通过，未失败、忽略或过滤：Driver35、Verify38、LLVM18。本片新增源码测试5组、raw/wire测试3组；其余为直接受影响回归，不是正式全量验收。

本片实际完成14次原生构建和12次CT精确拒绝编译。358次原生执行分为：i32/i64动态移位合法224次，非法计数精确SIGILL40次；包含32个CT结果断言的程序O0/O2各一次；四模块文本45个输入各O0/O2共90次；文本失败保持和标量位置接口O0/O2共2次。位模式参照使用独立i128/u128数学模型；文本参照使用Rust标准UTF-8检查和encode_utf16，均仅作测试oracle。

raw/wire独立构造并重绑内容身份，保留同型整数结构准入、异型及非整数 E-UNSUPPORTED、伪结果类型 E-VERIFY-TYPE、未定义/未来SSA E-VERIFY-DOMINANCE，以及CT已有step上限和禁止外部执行边界。结构准入不是对移位量值域的静态证明；非法量的实际CT和原生失败由源码测试另核。现有impl082仅迁移旧signed暂不支持期待，u8及u16回归继续执行。

`evidence/impl/124/acceptance.json` 和 raw archive 保留执行命令、最终源码字节、阶段IR、机器产物、输入、预期、实际输出与退出状态。首次错误的main签名、非法i64字面量后缀，以及消费者中未显式提升i32字面量导致的Types错误均原样保留，不计作编译器bug。初始探针记录了执行路径和返回，但当时没有逐次记录HEAD/二进制hash；最终运行才是完整的干净源码绑定证据，不能倒填基线探针身份。

## Rust四目标与剩余差距

| 目标 | 本片实证与限制 |
|---|---|
| Rust同类安全 | 核验仍独立重建类型/SSA，CT能力和预算边界保留，移位前guard排除LLVM非法计数；属于当前有限操作与消费者证据，不是全语言安全证明。 |
| 运行与内存同级 | O0/O2真实程序的数值、UTF编码和失败保持与独立参照相符；没有运行时间、峰值内存或Rust成本比较。 |
| 表达与维护更简单 | 原本被语言缺口阻挡的普通四模块文本程序现可编译执行；没有测量维护成本，也没有声称文本接口全面成熟。 |
| 编译成本更低 | 此片是能力补齐，没有计时结论；图编译研究由独立工单处理。 |
| 至少一项净突破 | 本轮取得可执行语言增量；本片未证明产品级Rust净突破。 |

schema17、规范、默认预算、产品门及正式父任务均未修改。作者不合入、不跑正式全量，不将此限定结果晋升为独审或产品验收；完整语言NotSatisfied、产品NotRun、B0 Doing仍由集成方按既有门处理。

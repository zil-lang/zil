# 338：大型普通 Copy Enum/ErrorUnion 内存 ABI

## 实施前真实基线

权威工单 `.task/impl/338.md`，生产基线 `e5a4d51d844e29c4f4f5184eaff5704b1b27dd24`，
root 冻结规范 `c4da43441` 的 §19.4。沿用333的4096字节门槛、`.lc1`版本化
copy-in/out和普通Value语义，不提高默认额度，不新增拥有/闭包/foreign/helper权限。

B release CLI `/workspace/.zil-env/target-mainline-release/release/zil`，LLVM23.1.4，
Linux x64，CPU45s/AS2GiB/wall55s/core0、原生5s。实际源在实施前 `/tmp/zil338-probe`：

| 实际输入 | SHA256 | 结果 |
|---|---|---|
| enum-payload-a12 | a2a8f54308afb6181f7e667b0003ecbe6599eba827736b7983acedf3fe1ff4cb | check Success/Verify1；API O0 Codegen Unsupported，无机器产物 |
| error-payload-a12 | 74365008b3fba73675fabf079d0f885b65c569ed18ec72f146b082f90347c12a | check Success/Verify1；API O0 Codegen Unsupported，无机器产物 |
| enum-empty-unused-a12 | 79a165db0d7de38d13726ca6eac7a76d0b8c9f6545c4bb4443ce1749a9127679 | API O0 Success/native0，定义/调用仍为未版本化 `{i32,[4096 x i8]}` 整值SSA |
| enum-empty-unused-a18 | ab697a908f49aa63fbaee3c2236dfd115fb2467fa557320ebcad0c84bc9c890c | API O0 48.475s 后SIGKILL(-9)，无JSON/stderr；资源受限未完成，不称无约束LLVM逻辑崩溃 |
| error-unused-only-a12 | 35ab6f68cce20ba5c4e0ba1cc4916a5bd3c7ea00d2b080b04712deebb225e1fe | API O0 Success/native0，说明并无一般大Sum提前拒绝门 |

前两个实际拒绝为 Codegen `E-UNSUPPORTED`: `large Copy value requires an unsupported callable/helper ABI`。
原a18规模保留，未缩小为a12冒称原规模通过。现有大move capture的2MiB Verify API栈溢出
继续KnownIncomplete；本单不增加compiler/test线程栈，不以栈变更结清该问题。

## 有限实施合同与写集

共享snapshot/classify扩大到普通Copy Enum/ErrorUnion。大构造完整归零目标槽，
显式写tag及实际payload；tag与投影从内存读取，保留变体/错误分支检查和trap。
Enum沿用i32 tag与自然对齐的max-payload words；ErrorUnion沿用i1/error/result结构。
非活动payload/padding不是语言值；大字段只复制独立快照，小enum字段保留既有canonical
payload写入。普通直接/间接调用、返回及callee入口统一走333计划；完整fnptr载体依赖
继续决定`.lc1`，纯数据地址和小签名保持原约定，特殊未贯通消费者提前Unsupported。

生产限 `emit/copy_aggregates.rs`、`emit/enums.rs`、`emit/errors.rs`，
`emit/large_copy_abi.rs`只在必要时补模型/边界；测试用既有aggregates target、固定
`codegen-pending/large-copy-enum-error-*.zil`及 `docs/validation/large-copy-338.json`。
root管理规范、主树工单和剩余语言文档。不改schema/Wire、Types/Analyze/Verify和默认额度。

验证限定真实源两后端O0/O2、原a18、tag/活动payload/大小对齐、Copy求值与替换、两侧错误
传播/清理、fnptr载体符号和CGU/cache及N/N−1/fresh；实际命令与原失败在交付追加。

## 首轮实际失败与必要消费者补齐

首轮三个原a18输入各Clang/API×O0/O2已实际12次native0；普通错误载荷包含4096字节
数组的`main()->Fault!i32`则在Link E-LLVM-COMPILE失败：native entry wrapper仍调用
未版本化`@zil.fn.7()`并期待整值SSA，而真实定义已为`.lc1`/outptr。root据实准许
把`emit.rs`的entry_wrapper与单一调用点加入写集，消费同一333计划、仅读tag和成功
标量；既有拥有error清理路径保持。修复前失败保留，不冒称已闭合所有消费者。

另一新record-variant程序在Expand命中既有fnptr结果无法推断enum模式问题；使用
明确`Message`类型注解后再核验，本单不改frontend或放宽未核来源。

338新增借入大Enum的closure正源在普通2MiB Rust API test worker明确stack overflow/SIGABRT，
未到Codegen；当前不改变测试/compiler栈。原实际输入固定为
`large-copy-enum-error-borrow-capture.zil`，单列KnownIncomplete，以普通CLI另核阶段与机器结果；
不把小栈失败计作Unsupported、正常机器结果或本单栈修复。默认target不保留会中止整进程的
该API测试函数；固定源和原命令/实际失败仍保留。

独立预审指出新Enum内存路径仍clone完整源码Enum，原16/node不足以覆盖长nominal/
variant和嵌套record字段名。按真实新增工作给大型Enum snapshot单独增加借入递归
metadata费用（节点与实际名字字节），折入Slot.work，使重复构造/投影在事件增长前
同一余额中预付；普通旧Array/Tuple/Record的333费用不增加。ErrorUnion新内存操作
借入类型，不无因扩大旧路径费用。补长名N/N−1/fresh近邻；此前450910只记修补前
具名a18费用，最终N另实测，不提高默认额度。

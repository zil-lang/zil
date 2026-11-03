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
| enum-empty-unused-a12 | 稍后绑定固定源 | API O0 Success/native0，定义/调用仍为未版本化 `{i32,[4096 x i8]}` 整值SSA |
| enum-empty-unused-a18 | ab697a908f49aa63fbaee3c2236dfd115fb2467fa557320ebcad0c84bc9c890c | API O0 48.475s 后SIGKILL(-9)，无JSON/stderr；资源受限未完成，不称无约束LLVM逻辑崩溃 |
| error-unused-only-a12 | 稍后绑定固定源 | API O0 Success/native0，说明并无一般大Sum提前拒绝门 |

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
